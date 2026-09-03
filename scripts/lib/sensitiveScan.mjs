// 검사 ⑨(민감값 스캔)의 정본 로직 — docs/policy-contract.md §8.3 ⑨ / §8.6.
// `src/compat-check/checks/check9-sensitiveValueScan.ts`(CI, vitest)와 `.githooks/pre-push`
// (로컬 pre-push 훅)가 **이 한 모듈**을 함께 import한다 — "같은 패턴 파일을 사용(규칙
// 이중 정의 금지)"을 파일 하나 공유로 문자 그대로 만족시킨다.
//
// 일반 JS(TS 아님)로 쓴 이유: pre-push 훅은 git이 그대로 `node <hook>`으로 실행하므로
// TypeScript 컴파일 단계가 없다 — 이 모듈과 그 호출자 전부가 타입 스트리핑/번들링 없이
// 바로 실행 가능해야 한다.

const IPV4_RE = /\b(?:(?:25[0-5]|2[0-4]\d|1?\d{1,2})\.){3}(?:25[0-5]|2[0-4]\d|1?\d{1,2})\b/g;
const DOMAIN_CANDIDATE_RE = /\b(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\b/g;
const QUOTED_STRING_RE = /"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|`((?:[^`\\]|\\.)*)`/g;

/** 텍스트에서 인용부호(따옴표) 안의 내용만 뽑는다 — 코드 식별자 체인
 * (`result.policy.otel` 등)이 도메인처럼 보여 오탐되는 것을 막는 1차 방어선이다. */
export function extractQuotedStrings(text) {
  const out = [];
  QUOTED_STRING_RE.lastIndex = 0;
  let m;
  while ((m = QUOTED_STRING_RE.exec(text)) !== null) {
    const content = m[1] ?? m[2] ?? m[3] ?? '';
    if (content.length > 0) out.push(content);
  }
  return out;
}

function ipv4ToInt(ip) {
  const parts = ip.split('.').map(Number);
  if (parts.length !== 4 || parts.some((p) => Number.isNaN(p) || p < 0 || p > 255)) return null;
  return ((parts[0] << 24) >>> 0) + (parts[1] << 16) + (parts[2] << 8) + parts[3];
}

function ipv4InCidr(ip, cidr) {
  const [base, bitsStr] = cidr.split('/');
  const bits = Number(bitsStr);
  const ipInt = ipv4ToInt(ip);
  const baseInt = ipv4ToInt(base);
  if (ipInt === null || baseInt === null || Number.isNaN(bits)) return false;
  if (bits === 0) return true;
  const mask = bits >= 32 ? 0xffffffff : (0xffffffff << (32 - bits)) >>> 0;
  return (ipInt & mask) === (baseInt & mask);
}

function isExemptIpv4(ip, exemptCidrs) {
  return exemptCidrs.some((cidr) => ipv4InCidr(ip, cidr));
}

function isReservedOrAllowedDomain(domain, reservedSuffixes, publicAllowlist) {
  const lower = domain.toLowerCase();
  const matchesAny = (list) =>
    list.some((entry) => {
      const e = entry.toLowerCase();
      if (e.startsWith('.')) return lower.endsWith(e);
      return lower === e || lower.endsWith(`.${e}`);
    });
  return matchesAny(reservedSuffixes) || matchesAny(publicAllowlist);
}

/**
 * `sensitive-classes.json`을 로드해 스캐너 설정으로 정규화한다.
 */
export function loadClassesConfig(sensitiveClassesJsonText) {
  const parsed = JSON.parse(sensitiveClassesJsonText);
  const ipv4Class = (parsed.classes ?? []).find((c) => c.kind === 'ipv4');
  const domainClass = (parsed.classes ?? []).find((c) => c.kind === 'domain-default-deny');
  return {
    ipv4ExemptCidrs: [
      ...(ipv4Class?.exemptCidrs ?? []),
      ...(parsed.reservedNamespaceAllowlist?.ipv4Cidrs ?? []),
    ],
    recognizedTlds: new Set((domainClass?.recognizedTlds ?? []).map((t) => t.toLowerCase())),
    publicAllowlist: domainClass?.publicAllowlist ?? [],
    reservedDomainSuffixes: parsed.reservedNamespaceAllowlist?.domainSuffixes ?? [],
  };
}

/**
 * 텍스트 하나(파일 내용)를 스캔해 위반 목록을 돌려준다. `filePath`는 보고용 라벨일 뿐이다.
 */
export function scanText(filePath, text, config) {
  const violations = [];
  const quoted = extractQuotedStrings(text);

  for (const s of quoted) {
    IPV4_RE.lastIndex = 0;
    let m;
    while ((m = IPV4_RE.exec(s)) !== null) {
      const ip = m[0];
      if (!isExemptIpv4(ip, config.ipv4ExemptCidrs)) {
        violations.push({ file: filePath, classId: 'ipv4-literal', match: ip });
      }
    }

    DOMAIN_CANDIDATE_RE.lastIndex = 0;
    let dm;
    while ((dm = DOMAIN_CANDIDATE_RE.exec(s)) !== null) {
      const candidate = dm[0];
      const lastLabel = candidate.slice(candidate.lastIndexOf('.') + 1).toLowerCase();
      if (!config.recognizedTlds.has(lastLabel)) continue; // TLD로 안 끝나면 도메인 후보가 아니다(코드 식별자/버전 문자열 배제)
      if (isReservedOrAllowedDomain(candidate, config.reservedDomainSuffixes, config.publicAllowlist)) continue;
      violations.push({ file: filePath, classId: 'network-authority-domain', match: candidate });
    }
  }

  return violations;
}

/**
 * 여러 파일을 스캔한다. `files`는 `{ path, text }` 목록(호출자가 읽기를 담당 —
 * 이 함수는 부작용 0의 순수 스캐너다).
 */
export function scanFiles(files, config) {
  const violations = [];
  for (const { path, text } of files) {
    violations.push(...scanText(path, text, config));
  }
  return violations;
}
