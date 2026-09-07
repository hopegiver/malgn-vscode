// entryJson 정규화 — architecture.md §3.2.2 ③ "동의는 인벤토리가 아니라 항목 선언
// 자체에 바인딩한다: `diffHash = hash({marketplaceRepo, entryJson(정규화, version 제외)})`."
//
// 이 프로젝트의 일반 `Plan.diffHash`는 `computeDiffHash(providerId, changes)`
// (`core/reconciler/diffHash.ts`, W1 정본)로 이미 통일돼 있다 — agent provider가 별도
// 해시 필드를 새로 만들지 않는다. 대신 §3.2.2가 요구하는 **바인딩 대상**(마켓플레이스
// 저장소 + version 제외 entryJson)을 `Change.after`의 **내용**으로 인코딩해서, 그
// 일반 해시 계산이 자연히 §3.2.2가 원하는 성질("entryJson 동일 + compat 범위 내 버전
// 업데이트는 최초 1회 동의가 커버")을 만족하게 한다 — `after`에 구체 버전 문자열이
// 없으므로 버전만 오른 재탐지는 바이트가 같은 `Change`를 만들고, 그 결과 같은
// `diffHash`를 낸다.

import { stableStringify } from '../../core/reconciler/diffHash.js';

/**
 * `marketplace.json`의 원소(`entry`)에서 `version` 키를 뺀 나머지를 정규화(키 정렬)
 * 직렬화한다. `entry`가 아직 없으면(마켓플레이스 미등록 최초 상태) `null`을 받아
 * `desiredFallback`만으로 인코딩한다.
 */
export function buildAgentEntryDescriptor(
  marketplaceRepo: string,
  entry: Record<string, unknown> | null,
  desiredFallback: { readonly plugin: string; readonly scope: string; readonly channel: string }
): string {
  if (entry === null) {
    return stableStringify({ marketplaceRepo, entryJson: null, desired: desiredFallback });
  }
  const { version: _version, ...entryWithoutVersion } = entry;
  return stableStringify({ marketplaceRepo, entryJson: entryWithoutVersion });
}
