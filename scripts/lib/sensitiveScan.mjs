// 검사 ⑨(민감값 스캔)의 정본 로직 v2 — docs/policy-contract.md §8.3 ⑨ / §8.6,
// docs/security-plan.md §12.2(C-0~C-7)·§12.4(B1~B5).
// `src/compat-check/checks/check9-sensitiveValueScan.ts`(CI, vitest)와 `.githooks/pre-push`
// (로컬 pre-push 훅)가 **이 한 모듈**을 함께 import한다 — "같은 패턴 파일을 사용(규칙
// 이중 정의 금지)"을 파일 하나 공유로 문자 그대로 만족시킨다.
//
// 일반 JS(TS 아님)로 쓴 이유: pre-push 훅은 git이 그대로 `node <hook>`으로 실행하므로
// TypeScript 컴파일 단계가 없다 — 이 모듈과 그 호출자 전부가 타입 스트리핑/번들링 없이
// 바로 실행 가능해야 한다.
//
// C-0 스코프 규율: `sensitive-classes.json`의 `scanScopes`가 파일 확장자별로 "인용부호
// 안만 볼지(quotedOnly) 전문을 볼지(wholeText)"를 정한다. 코드 확장자(.ts/.js/.json 등)만
// quotedOnly — 식별자 체인(`result.policy.otel`)이 도메인처럼 오탐되는 것을 막는 1차
// 방어선이 여전히 필요하기 때문이다. 그 밖(.md/.yml/.sh 등 문서·설정 형식)은 전문 스캔—
// 마크다운 산문·표 셀·링크·무인용 YAML 스칼라가 이전에는 전면 통과했다(X-3). 새 확장자는
// `scanScopes.default`(wholeText)로 떨어진다 — fail-closed.

const IPV4_RE = /\b(?:(?:25[0-5]|2[0-4]\d|1?\d{1,2})\.){3}(?:25[0-5]|2[0-4]\d|1?\d{1,2})\b/g;

// 널리 쓰이는 포괄적 IPv6 매칭 패턴(완전형·압축형·IPv4-embedded 전부 커버) — RFC 4291 §2.2.
const IPV6_RE = new RegExp(
  [
    '(?:[0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}',
    '(?:[0-9A-Fa-f]{1,4}:){1,7}:',
    '(?:[0-9A-Fa-f]{1,4}:){1,6}:[0-9A-Fa-f]{1,4}',
    '(?:[0-9A-Fa-f]{1,4}:){1,5}(?::[0-9A-Fa-f]{1,4}){1,2}',
    '(?:[0-9A-Fa-f]{1,4}:){1,4}(?::[0-9A-Fa-f]{1,4}){1,3}',
    '(?:[0-9A-Fa-f]{1,4}:){1,3}(?::[0-9A-Fa-f]{1,4}){1,4}',
    '(?:[0-9A-Fa-f]{1,4}:){1,2}(?::[0-9A-Fa-f]{1,4}){1,5}',
    '[0-9A-Fa-f]{1,4}:(?:(?::[0-9A-Fa-f]{1,4}){1,6})',
    ':(?:(?::[0-9A-Fa-f]{1,4}){1,7}|:)',
    '::(?:ffff(?::0{1,4})?:)?(?:(?:25[0-5]|2[0-4]\\d|1?\\d{1,2})\\.){3}(?:25[0-5]|2[0-4]\\d|1?\\d{1,2})',
    '(?:[0-9A-Fa-f]{1,4}:){1,4}:(?:(?:25[0-5]|2[0-4]\\d|1?\\d{1,2})\\.){3}(?:25[0-5]|2[0-4]\\d|1?\\d{1,2})',
  ].join('|'),
  'g'
);

const DOMAIN_CANDIDATE_RE = /\b(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\b/g;
const QUOTED_STRING_RE = /"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|`((?:[^`\\]|\\.)*)`/g;

// C-5 — 포트는 호스트가 면제 대상이어도 별도로 판정한다. 트리거: IPv4/IPv6(대괄호)/도메인
// 후보/`}`(템플릿 보간 `${HOST}:PORT` 잔여 형태) 직후의 `:포트`.
const PORT_TRIGGER_RE = new RegExp(
  '(?:' +
    `(?<ipv4>(?:(?:25[0-5]|2[0-4]\\d|1?\\d{1,2})\\.){3}(?:25[0-5]|2[0-4]\\d|1?\\d{1,2}))` +
    '|' +
    `(?<ipv6b>\\[[0-9A-Fa-f:]+\\])` +
    '|' +
    `(?<domain>(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\\.)+[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)` +
    '|' +
    `(?<brace>\\})` +
    ')\\s*:(?<port>\\d{2,5})\\b',
  'g'
);

const HIGH_ENTROPY_TOKEN_RE = /[A-Za-z0-9+/=_-]{32,}/g;

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

// ---------------------------------------------------------------------------
// C-0 — 파일 확장자별 스캔 스코프(quotedOnly vs wholeText)
// ---------------------------------------------------------------------------

/** 이 파일의 glob 지원 범위는 의도적으로 좁다 — `sensitive-classes.json`이 실제로 쓰는
 * `**\/<simple-glob>` 형태(디렉터리 무관, 파일명만 매칭)만 지원한다. 새 패턴 형태가
 * 필요해지면 여기를 넓힌다(그 전까지는 default(wholeText)로 fail-closed 떨어진다). */
function matchesGlob(filePath, glob) {
  const simple = glob.startsWith('**/') ? glob.slice(3) : glob;
  const basename = filePath.slice(filePath.lastIndexOf('/') + 1);
  const escaped = simple.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '[^/]*');
  return new RegExp(`^${escaped}$`).test(basename);
}

function getFileScope(filePath, scanScopes) {
  for (const g of scanScopes.quotedOnly ?? []) {
    if (matchesGlob(filePath, g)) return 'quotedOnly';
  }
  for (const g of scanScopes.wholeText ?? []) {
    if (matchesGlob(filePath, g)) return 'wholeText';
  }
  return scanScopes.default ?? 'wholeText';
}

// ---------------------------------------------------------------------------
// IPv4/IPv6 CIDR 매칭
// ---------------------------------------------------------------------------

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

/** IPv6 주소 문자열(대괄호·zone id 제거 후) → 128비트 BigInt. 파싱 불가면 null. */
function ipv6ToBigInt(addrRaw) {
  let a = addrRaw.replace(/^\[|\]$/g, '').split('%')[0];

  // embedded IPv4(마지막 세그먼트가 점 표기) — 16비트 두 그룹으로 치환.
  const ipv4Embed = /:((?:\d{1,3}\.){3}\d{1,3})$/.exec(a);
  if (ipv4Embed) {
    const parts = ipv4Embed[1].split('.').map(Number);
    if (parts.some((p) => Number.isNaN(p) || p < 0 || p > 255)) return null;
    const hex1 = (((parts[0] << 8) | parts[1]) >>> 0).toString(16).padStart(4, '0');
    const hex2 = (((parts[2] << 8) | parts[3]) >>> 0).toString(16).padStart(4, '0');
    a = `${a.slice(0, a.length - ipv4Embed[1].length)}${hex1}:${hex2}`;
  }

  let groups;
  const dblIdx = a.indexOf('::');
  if (dblIdx !== -1) {
    if (a.indexOf('::', dblIdx + 1) !== -1) return null; // '::'가 두 번 이상 — 잘못된 주소
    const head = a.slice(0, dblIdx);
    const tail = a.slice(dblIdx + 2);
    const headGroups = head.length ? head.split(':') : [];
    const tailGroups = tail.length ? tail.split(':') : [];
    const missing = 8 - headGroups.length - tailGroups.length;
    if (missing < 0) return null;
    groups = [...headGroups, ...Array(missing).fill('0'), ...tailGroups];
  } else {
    groups = a.split(':');
  }
  if (groups.length !== 8) return null;

  let result = 0n;
  for (const g of groups) {
    if (!/^[0-9A-Fa-f]{1,4}$/.test(g)) return null;
    result = (result << 16n) | BigInt(parseInt(g, 16));
  }
  return result;
}

function ipv6InCidr(addr, cidr) {
  const [base, bitsStr] = cidr.split('/');
  const bits = Number(bitsStr);
  const addrInt = ipv6ToBigInt(addr);
  const baseInt = ipv6ToBigInt(base);
  if (addrInt === null || baseInt === null || Number.isNaN(bits)) return false;
  if (bits <= 0) return true;
  const shift = 128n - BigInt(Math.min(bits, 128));
  const mask = shift >= 128n ? 0n : (((1n << 128n) - 1n) >> shift) << shift;
  return (addrInt & mask) === (baseInt & mask);
}

function isExemptIpv6(ip, exemptCidrs) {
  return exemptCidrs.some((cidr) => ipv6InCidr(ip, cidr));
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

// ---------------------------------------------------------------------------
// 클래스 설정 로드/컴파일
// ---------------------------------------------------------------------------

/** `(?i)접두` PCRE 스타일 표기를 JS RegExp(source, flags)로 변환한다. */
function compileMaybeCiPattern(pattern) {
  if (pattern.startsWith('(?i)')) return { source: pattern.slice(4), flags: 'i' };
  return { source: pattern, flags: '' };
}

function compileClass(raw) {
  const covers = raw.covers;
  // PUB-X·PUB-A는 redactMatch:true가 구조로 강제된다 — JSON에서 false로 적어도 무시한다
  // (security-plan.md §12.2 C-1: "PUB-X·PUB-A는 true 고정").
  const redact = raw.redactMatch === true || covers === 'PUB-X' || covers === 'PUB-A';
  const base = { id: raw.id, covers, kind: raw.kind, enabled: raw.enabled !== false, redact };

  switch (raw.kind) {
    case 'regex-any': {
      const patterns = (raw.patterns ?? []).map((p) => new RegExp(p, 'g'));
      const exemptPatterns = (raw.exemptPatterns ?? []).map((p) => new RegExp(p, 'i'));
      return { ...base, scope: raw.scope, patterns, exemptPatterns };
    }
    case 'key-with-literal-value': {
      const { source, flags } = compileMaybeCiPattern(raw.keyNameRe);
      const keyNameRe = new RegExp(source, flags);
      const placeholderValueRe = raw.placeholderValueRe ? new RegExp(raw.placeholderValueRe) : null;
      return {
        ...base,
        keyNameRe,
        placeholderValues: new Set((raw.placeholderValues ?? []).filter((v) => typeof v === 'string')),
        placeholderValueRe,
      };
    }
    case 'ipv4':
      return { ...base, exemptCidrs: [...(raw.exemptCidrs ?? [])] };
    case 'ipv6':
      return { ...base, exemptCidrs: [...(raw.exemptCidrs ?? [])] };
    case 'domain-default-deny':
      return {
        ...base,
        recognizedTlds: new Set((raw.recognizedTlds ?? []).map((t) => t.toLowerCase())),
        publicAllowlist: [...(raw.publicAllowlist ?? [])],
      };
    case 'port-after-authority':
      return { ...base, allowedPorts: new Set(raw.allowedPorts ?? []) };
    case 'structural-key-path':
      return {
        ...base,
        filesGlob: raw.filesGlob,
        excludeFiles: new Set(raw.excludeFiles ?? []),
        keyPaths: raw.keyPaths ?? [],
      };
    case 'high-entropy':
      return { ...base, minLength: raw.minLength ?? 32, minEntropy: raw.minEntropy ?? 4.0 };
    default:
      return base;
  }
}

/**
 * `sensitive-classes.json`을 로드해 스캐너 설정으로 정규화한다(v2 — C-0~C-7).
 */
export function loadClassesConfig(sensitiveClassesJsonText) {
  const parsed = JSON.parse(sensitiveClassesJsonText);
  const reservedNamespaceAllowlist = parsed.reservedNamespaceAllowlist ?? {};
  const ipv4Reserved = reservedNamespaceAllowlist.ipv4Cidrs ?? [];
  const ipv6Reserved = reservedNamespaceAllowlist.ipv6Cidrs ?? [];
  const reservedDomainSuffixes = reservedNamespaceAllowlist.domainSuffixes ?? [];
  const publicAllowlist = parsed.publicAllowlist ?? [];
  const publicAllowlistValues = publicAllowlist.map((e) => e.value);

  const classes = (parsed.classes ?? []).map(compileClass).map((c) => {
    if (c.kind === 'ipv4') return { ...c, exemptCidrs: [...c.exemptCidrs, ...ipv4Reserved] };
    if (c.kind === 'ipv6') return { ...c, exemptCidrs: [...c.exemptCidrs, ...ipv6Reserved] };
    if (c.kind === 'domain-default-deny') return { ...c, publicAllowlist: [...c.publicAllowlist, ...publicAllowlistValues] };
    return c;
  });

  return {
    version: parsed.version ?? 1,
    classTaxonomy: parsed.classTaxonomy ?? [],
    enforcedClasses: parsed.enforcedClasses ?? [],
    taxonomyIdsSha256: parsed.taxonomyIdsSha256 ?? null,
    scanScopes: parsed.scanScopes ?? { quotedOnly: [], wholeText: [], default: 'wholeText' },
    classes,
    reservedDomainSuffixes,
    publicAllowlist,
    pathExemptions: parsed.pathExemptions ?? [],
    // 하위호환: v1 소비자가 참조하던 필드 이름도 함께 노출한다(현재 이 필드를 직접 읽는
    // 외부 코드는 없다 — check9/check11은 loadClassesConfig 결과를 scanText에만 넘긴다).
    ipv4ExemptCidrs: [...(classes.find((c) => c.kind === 'ipv4')?.exemptCidrs ?? [])],
    recognizedTlds: classes.find((c) => c.kind === 'domain-default-deny')?.recognizedTlds ?? new Set(),
  };
}

/** B4(§12.4) — `enforcedClasses`의 각 부류를 커버하는 **활성** class가 1개 이상 있는지
 * 확인한다. 없는 부류 id 목록을 돌려준다(빈 배열 = 전부 커버됨). 오늘 상태(PUB-X·PUB-A
 * 패턴 0개)는 이 함수 하나로 즉시 비어있지 않은 배열을 반환해야 한다. */
export function getEnforcedCoverageGaps(config) {
  const covered = new Set((config.classes ?? []).filter((c) => c.enabled !== false).map((c) => c.covers));
  return (config.enforcedClasses ?? []).filter((cls) => !covered.has(cls));
}

// ---------------------------------------------------------------------------
// 클래스별 스캔 로직
// ---------------------------------------------------------------------------

function pushViolation(violations, filePath, cls, rawMatch) {
  violations.push({
    file: filePath,
    classId: cls.id,
    covers: cls.covers,
    match: cls.redact ? '[REDACTED]' : rawMatch,
    redacted: cls.redact,
  });
}

function scanRegexAny(filePath, text, cls, violations) {
  for (const re of cls.patterns) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(text)) !== null) {
      const match = m[0];
      if (cls.exemptPatterns.some((ex) => ex.test(match))) continue;
      pushViolation(violations, filePath, cls, match);
      if (match.length === 0) re.lastIndex += 1; // 0폭 매치 무한루프 방지
    }
  }
}

function isPlaceholderValue(value, cls) {
  if (typeof value !== 'string') return true;
  if (value.trim().length === 0) return true;
  if (cls.placeholderValues.has(value)) return true;
  if (cls.placeholderValueRe && cls.placeholderValueRe.test(value)) return true;
  return false;
}

function walkKeyLiteralValues(node, path, cls, visit) {
  if (node === null || typeof node !== 'object') return;
  if (Array.isArray(node)) {
    node.forEach((item, i) => walkKeyLiteralValues(item, [...path, String(i)], cls, visit));
    return;
  }
  for (const [key, value] of Object.entries(node)) {
    const nextPath = [...path, key];
    if (cls.keyNameRe.test(key) && typeof value === 'string') {
      visit(nextPath.join('.'), value);
    }
    walkKeyLiteralValues(value, nextPath, cls, visit);
  }
}

/** C-2 — json/jsonc는 파싱해서 키-값으로, 그 밖은 `keyNameRe\s*[:=]\s*['"\`]값['"\`]`
 * 정규식으로 스캔한다. 값(원문)은 절대 violation.match에 싣지 않는다 — 키 경로만 싣는다
 * (redactMatch와 별개로, 이 클래스는 애초에 값을 노출하지 않는다). */
function scanKeyWithLiteralValue(filePath, text, cls, violations) {
  const isJsonLike = /\.jsonc?$/i.test(filePath);
  if (isJsonLike) {
    try {
      const parsed = JSON.parse(text);
      walkKeyLiteralValues(parsed, [], cls, (path, value) => {
        if (!isPlaceholderValue(value, cls)) pushViolation(violations, filePath, cls, path);
      });
      return;
    } catch {
      // 파싱 불가 — 정규식 폴백으로 내려간다(fail-closed 유지, 조용히 건너뛰지 않는다).
    }
  }
  // 이름 있는 그룹을 쓴다 — keyNameRe.source 자체가 내부 캡처 그룹을 가질 수 있어
  // (예: `pass(word|wd|phrase)?`) 번호 그룹으로 인덱스를 세면 어긋난다.
  const flags = `g${cls.keyNameRe.flags.includes('i') ? 'i' : ''}`;
  const re = new RegExp(`(?<matchedKey>${cls.keyNameRe.source})["'\`]?\\s*[:=]\\s*["'\`](?<matchedValue>[^"'\`]*)["'\`]`, flags);
  let m;
  while ((m = re.exec(text)) !== null) {
    if (!isPlaceholderValue(m.groups.matchedValue, cls)) pushViolation(violations, filePath, cls, m.groups.matchedKey);
  }
}

function scanIpv4(filePath, text, cls, violations) {
  IPV4_RE.lastIndex = 0;
  let m;
  while ((m = IPV4_RE.exec(text)) !== null) {
    if (!isExemptIpv4(m[0], cls.exemptCidrs)) pushViolation(violations, filePath, cls, m[0]);
  }
}

function scanIpv6(filePath, text, cls, violations) {
  IPV6_RE.lastIndex = 0;
  let m;
  while ((m = IPV6_RE.exec(text)) !== null) {
    if (!isExemptIpv6(m[0], cls.exemptCidrs)) pushViolation(violations, filePath, cls, m[0]);
  }
}

function scanDomain(filePath, text, cls, reservedDomainSuffixes, violations) {
  DOMAIN_CANDIDATE_RE.lastIndex = 0;
  let dm;
  while ((dm = DOMAIN_CANDIDATE_RE.exec(text)) !== null) {
    const candidate = dm[0];
    const lastLabel = candidate.slice(candidate.lastIndexOf('.') + 1).toLowerCase();
    if (!cls.recognizedTlds.has(lastLabel)) continue;
    if (isReservedOrAllowedDomain(candidate, reservedDomainSuffixes, cls.publicAllowlist)) continue;
    pushViolation(violations, filePath, cls, candidate);
  }
}

function scanPort(filePath, text, cls, config, violations) {
  const domainClass = config.classes.find((c) => c.kind === 'domain-default-deny');
  PORT_TRIGGER_RE.lastIndex = 0;
  let m;
  while ((m = PORT_TRIGGER_RE.exec(text)) !== null) {
    if (m.groups.domain) {
      const lastLabel = m.groups.domain.slice(m.groups.domain.lastIndexOf('.') + 1).toLowerCase();
      if (!domainClass || !domainClass.recognizedTlds.has(lastLabel)) continue; // file.ts:29 류 오탐 방지
    }
    const port = Number(m.groups.port);
    if (cls.allowedPorts.has(port)) continue;
    pushViolation(violations, filePath, cls, `:${m.groups.port}`);
  }
}

function shannonEntropy(str) {
  const freq = new Map();
  for (const ch of str) freq.set(ch, (freq.get(ch) ?? 0) + 1);
  let entropy = 0;
  for (const count of freq.values()) {
    const p = count / str.length;
    entropy -= p * Math.log2(p);
  }
  return entropy;
}

function scanHighEntropy(filePath, text, cls, violations) {
  HIGH_ENTROPY_TOKEN_RE.lastIndex = 0;
  let m;
  while ((m = HIGH_ENTROPY_TOKEN_RE.exec(text)) !== null) {
    const token = m[0];
    if (token.length < cls.minLength) continue;
    if (shannonEntropy(token) < cls.minEntropy) continue;
    pushViolation(violations, filePath, cls, token);
  }
}

// --- C-6 site-hole-discipline (structural-key-path) ---

function matchesFilesGlob(filePath, glob) {
  if (glob === 'compat/*.json') {
    const rest = filePath.startsWith('compat/') ? filePath.slice('compat/'.length) : null;
    return rest !== null && !rest.includes('/') && rest.endsWith('.json');
  }
  return false;
}

function isSiteHole(value) {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Object.keys(value).length === 1 &&
    typeof value.$site === 'string'
  );
}

/** 값이 아니라 이름·개수만 담는 스키마 노드(예: contract-snapshot.json의 siteShape)를
 * 걸러낸다 — C-6은 "사이트 값"을 검사하는 것이지 "값의 형태를 기록한 스키마"를 검사하는
 * 게 아니다(그건 검사 ⑩(d)의 몫). filesGlob이 이미 compat/contract-snapshot.json을
 * excludeFiles로 뺐지만, 다른 파일이 같은 이름을 재사용할 경우를 대비한 이중 방어. */
function isSchemaOnlyShape(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value);
  return keys.length > 0 && keys.every((k) => k === 'keys' || k === 'counts' || k === 'count');
}

function collectStringLeaves(node, out) {
  if (typeof node === 'string') {
    out.push(node);
    return;
  }
  if (Array.isArray(node)) {
    node.forEach((item) => collectStringLeaves(item, out));
    return;
  }
  if (node !== null && typeof node === 'object') {
    for (const v of Object.values(node)) collectStringLeaves(v, out);
  }
}

function pathMatchesKeyPathSpec(path, specSegments) {
  if (path.length < specSegments.length) return false;
  const tail = path.slice(path.length - specSegments.length);
  return tail.every((seg, i) => seg === specSegments[i]);
}

function collectSiteHoleTargets(node, path, keyPaths, out) {
  if (node === null || typeof node !== 'object' || Array.isArray(node)) return;
  const specs = keyPaths.map((k) => k.split('.'));
  for (const [key, value] of Object.entries(node)) {
    const nextPath = [...path, key];
    const matched = specs.some((spec) => pathMatchesKeyPathSpec(nextPath, spec));
    if (matched) {
      out.push({ path: nextPath, value });
      continue; // 대상 노드 내부는 leaf 추출로만 다룬다 — 추가로 재귀 진입하지 않는다
    }
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      collectSiteHoleTargets(value, nextPath, keyPaths, out);
    } else if (Array.isArray(value)) {
      value.forEach((item, i) => {
        if (item !== null && typeof item === 'object') collectSiteHoleTargets(item, [...nextPath, String(i)], keyPaths, out);
      });
    }
  }
}

function isReservedOrAllowedAuthorityValue(value, config) {
  const withoutWildcard = value.startsWith('*.') ? value.slice(2) : value;
  const host =
    withoutWildcard.includes(':') && !withoutWildcard.startsWith('[')
      ? withoutWildcard.slice(0, withoutWildcard.lastIndexOf(':'))
      : withoutWildcard;

  if (/^\d{1,3}(?:\.\d{1,3}){3}$/.test(host)) {
    const ipv4Class = config.classes.find((c) => c.kind === 'ipv4');
    if (ipv4Class && isExemptIpv4(host, ipv4Class.exemptCidrs)) return true;
  }

  const lower = host.toLowerCase();
  const reservedMatch = config.reservedDomainSuffixes.some((entry) => {
    const e = entry.toLowerCase();
    if (e.startsWith('.')) return lower.endsWith(e);
    return lower === e || lower.endsWith(`.${e}`);
  });
  if (reservedMatch) return true;

  return config.publicAllowlist.some((e) => e.value === value || e.value === host);
}

function scanSiteHoleDiscipline(filePath, text, cls, config, violations) {
  if (!matchesFilesGlob(filePath, cls.filesGlob) || cls.excludeFiles.has(filePath)) return;
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    return; // 스키마 검증은 검사②의 몫 — 이 구조 검사는 파싱 가능한 JSON에만 적용된다
  }
  const targets = [];
  collectSiteHoleTargets(parsed, [], cls.keyPaths, targets);
  for (const { path, value } of targets) {
    if (isSiteHole(value)) continue;
    if (isSchemaOnlyShape(value)) continue;
    const leaves = [];
    collectStringLeaves(value, leaves);
    for (const leaf of leaves) {
      if (isReservedOrAllowedAuthorityValue(leaf, config)) continue;
      pushViolation(violations, filePath, cls, `${path.join('.')}=${leaf}`);
    }
  }
}

// ---------------------------------------------------------------------------
// 진입점
// ---------------------------------------------------------------------------

/**
 * 텍스트 하나(파일 내용)를 스캔해 위반 목록을 돌려준다. `filePath`는 스코프 판정(C-0)과
 * 보고 라벨을 겸한다(저장소 루트 기준 상대경로를 넘겨야 한다 — pathExemptions/filesGlob이
 * 상대경로 문자열을 그대로 비교한다).
 */
export function scanText(filePath, text, config) {
  const violations = [];
  const exemptClassIds = new Set(
    (config.pathExemptions ?? []).filter((e) => e.path === filePath).flatMap((e) => e.classes ?? [])
  );

  const fileScope = getFileScope(filePath, config.scanScopes ?? {});
  const quotedJoined = extractQuotedStrings(text).join('\n');
  const scopedSpace = fileScope === 'wholeText' ? text : quotedJoined;

  for (const cls of config.classes ?? []) {
    if (cls.enabled === false) continue;
    if (exemptClassIds.has(cls.id)) continue;

    const searchSpace = cls.scope === 'wholeText' ? text : scopedSpace;

    switch (cls.kind) {
      case 'regex-any':
        scanRegexAny(filePath, searchSpace, cls, violations);
        break;
      case 'key-with-literal-value':
        scanKeyWithLiteralValue(filePath, text, cls, violations);
        break;
      case 'ipv4':
        scanIpv4(filePath, searchSpace, cls, violations);
        break;
      case 'ipv6':
        scanIpv6(filePath, searchSpace, cls, violations);
        break;
      case 'domain-default-deny':
        scanDomain(filePath, searchSpace, cls, config.reservedDomainSuffixes, violations);
        break;
      case 'port-after-authority':
        scanPort(filePath, searchSpace, cls, config, violations);
        break;
      case 'structural-key-path':
        scanSiteHoleDiscipline(filePath, text, cls, config, violations);
        break;
      case 'high-entropy':
        scanHighEntropy(filePath, searchSpace, cls, violations);
        break;
      default:
        break;
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
