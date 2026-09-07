// known[] 결함 버전 판정 — architecture.md §3.5 "known 결함으로 apply가 차단됩니다".
// `detect.ts`(Observed.status를 blocked로 올리기 위해)와 `plan.ts`(방어심층 — Change를
// 만들지 않기 위해) 둘 다 이 함수를 재사용한다. 순수 함수(코드 상수만 읽는다, I/O 없음).

import { compareVersions, parseVersion } from '../../core/policy/semver.js';
import type { KnownVersionEntry } from '../../core/policy/types.js';

export function isKnownBlockedVersion(version: string, known: readonly KnownVersionEntry[]): boolean {
  const parsed = parseVersion(version);
  if (!parsed) return false;
  return known.some((entry) => {
    if (entry.component !== 'malgnAgent') return false;
    const rangeVersion = parseVersion(entry.range);
    return rangeVersion !== null && compareVersions(parsed, rangeVersion) === 0;
  });
}
