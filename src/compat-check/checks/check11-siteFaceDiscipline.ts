// 검사 ⑪ — 사이트면 규율 (docs/policy-contract.md §8.3 신설 3종 / §8.6 재발 방지 4)
//
// (a) 예시면(`compat/site.example.json`)의 모든 authority 값이 예약 네임스페이스
//     (RFC 2606/5737/3849)에만 속함
// (b) 예시면의 **형태**(키 개수·배열 길이)가 스냅샷 `siteShape`와 일치
// (c) 예시면으로 `compatibility.json`의 구멍을 채웠을 때 미해결 구멍이 남지 않음
//     (남으면 fail-closed — gen-site.mjs와 같은 판정을 재사용해 이중 정의를 피한다)

import { readFileSync } from 'node:fs';
import { hasRemainingHole, resolveSiteHoles } from '../../../scripts/gen-site.mjs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

interface AllowedAuthoritiesShape {
  readonly otel?: readonly string[];
  readonly extension?: readonly string[];
  readonly identity?: readonly string[];
  readonly hub?: readonly string[];
  readonly mcp?: readonly string[];
  readonly [key: string]: readonly string[] | undefined;
}

interface SiteExample {
  readonly authorities?: AllowedAuthoritiesShape;
  readonly keychainItems?: readonly string[];
}

interface Snapshot {
  readonly siteShape?: {
    readonly authorities?: { readonly keys?: readonly string[]; readonly counts?: Readonly<Record<string, number>> };
    readonly keychainItems?: { readonly count?: number };
  };
}

interface ReservedNamespaceAllowlist {
  readonly domainSuffixes?: readonly string[];
  readonly ipv4Cidrs?: readonly string[];
}

function ipv4ToInt(ip: string): number | null {
  const parts = ip.split('.').map(Number);
  if (parts.length !== 4 || parts.some((p) => Number.isNaN(p) || p < 0 || p > 255)) return null;
  return ((parts[0]! << 24) >>> 0) + (parts[1]! << 16) + (parts[2]! << 8) + parts[3]!;
}

function ipv4InCidr(ip: string, cidr: string): boolean {
  const [base, bitsStr] = cidr.split('/');
  const bits = Number(bitsStr);
  const ipInt = ipv4ToInt(ip);
  const baseInt = base ? ipv4ToInt(base) : null;
  if (ipInt === null || baseInt === null || Number.isNaN(bits)) return false;
  if (bits === 0) return true;
  const mask = bits >= 32 ? 0xffffffff : (0xffffffff << (32 - bits)) >>> 0;
  return (ipInt & mask) === (baseInt & mask);
}

function isReservedAuthority(authority: string, allowlist: ReservedNamespaceAllowlist): boolean {
  // host[:port] 또는 *.suffix 형태 — 먼저 host 부분과 port를 분리한다.
  const withoutWildcard = authority.startsWith('*.') ? authority.slice(2) : authority;
  const host = withoutWildcard.includes(':') ? withoutWildcard.slice(0, withoutWildcard.lastIndexOf(':')) : withoutWildcard;

  const ipv4Like = /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host);
  if (ipv4Like) {
    return (allowlist.ipv4Cidrs ?? []).some((cidr) => ipv4InCidr(host, cidr));
  }

  const lower = host.toLowerCase();
  return (allowlist.domainSuffixes ?? []).some((entry) => {
    const e = entry.toLowerCase();
    if (e.startsWith('.')) return lower.endsWith(e);
    return lower === e || lower.endsWith(`.${e}`);
  });
}

export interface Check11Input {
  readonly siteExampleJsonPath: string;
  readonly compatibilityJsonPath: string;
  readonly contractSnapshotJsonPath: string;
  readonly sensitiveClassesJsonPath: string;
}

export function checkSiteFaceDiscipline(input: Check11Input): CheckResult {
  const id = '⑪';
  const label = '사이트면 규율 — 예시면은 예약 네임스페이스만 + 형태 일치 + 구멍 무잔존';
  const violations: { ref: string; message: string }[] = [];

  const siteExample = JSON.parse(readFileSync(input.siteExampleJsonPath, 'utf8')) as SiteExample;
  const sensitiveClasses = JSON.parse(readFileSync(input.sensitiveClassesJsonPath, 'utf8')) as {
    reservedNamespaceAllowlist?: ReservedNamespaceAllowlist;
  };
  const allowlist = sensitiveClasses.reservedNamespaceAllowlist ?? {};

  // (a) 예약 네임스페이스만
  const authorities = siteExample.authorities ?? {};
  for (const [category, values] of Object.entries(authorities)) {
    for (const authority of values ?? []) {
      if (!isReservedAuthority(authority, allowlist)) {
        violations.push({ ref: '⑪(a)', message: `site.example.json의 authorities.${category}에 예약 네임스페이스 밖 값이 있습니다: ${authority}` });
      }
    }
  }

  // (b) 형태 일치 — 스냅샷 siteShape와 대조
  let snapshot: Snapshot;
  try {
    snapshot = JSON.parse(readFileSync(input.contractSnapshotJsonPath, 'utf8')) as Snapshot;
  } catch (error) {
    return fail(id, label, [
      { ref: '⑪(b)', message: `${input.contractSnapshotJsonPath}를 읽거나 파싱할 수 없습니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }
  const snapshotKeys = [...(snapshot.siteShape?.authorities?.keys ?? [])].sort();
  const exampleKeys = Object.keys(authorities).sort();
  if (JSON.stringify(snapshotKeys) !== JSON.stringify(exampleKeys)) {
    violations.push({ ref: '⑪(b)', message: `authorities 키 집합이 스냅샷과 다릅니다: 스냅샷=${JSON.stringify(snapshotKeys)}, 예시면=${JSON.stringify(exampleKeys)}` });
  } else {
    for (const key of exampleKeys) {
      const expectedCount = snapshot.siteShape?.authorities?.counts?.[key];
      const actualCount = authorities[key]?.length ?? 0;
      if (expectedCount !== undefined && expectedCount !== actualCount) {
        violations.push({ ref: '⑪(b)', message: `authorities.${key}의 개수가 스냅샷과 다릅니다: 스냅샷=${expectedCount}, 예시면=${actualCount}` });
      }
    }
  }
  const expectedKeychainCount = snapshot.siteShape?.keychainItems?.count;
  const actualKeychainCount = siteExample.keychainItems?.length ?? 0;
  if (expectedKeychainCount !== undefined && expectedKeychainCount !== actualKeychainCount) {
    violations.push({ ref: '⑪(b)', message: `keychainItems 개수가 스냅샷과 다릅니다: 스냅샷=${expectedKeychainCount}, 예시면=${actualKeychainCount}` });
  }

  // (c) 구멍 무잔존 — compatibility.json의 $site 구멍을 site.example.json으로 채워 본다.
  const compatibilityJson = JSON.parse(readFileSync(input.compatibilityJsonPath, 'utf8')) as Record<string, unknown>;
  const errors: string[] = [];
  const resolved = resolveSiteHoles(compatibilityJson, siteExample as unknown as Record<string, unknown>, errors);
  if (errors.length > 0 || hasRemainingHole(resolved)) {
    violations.push({ ref: '⑪(c)', message: `예시면으로 compatibility.json 구멍을 채우지 못했습니다: ${errors.join('; ')}` });
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
