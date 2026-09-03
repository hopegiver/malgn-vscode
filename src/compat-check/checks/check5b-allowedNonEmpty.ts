// 검사 ⑤B — 모든 `allowed*` 키가 비어 있지 않은 값 노드를 갖고 플레이스홀더가 없는지
// (문서 언급 여부와 무관 — docs/policy-contract.md §8.3 "⑤B", 오늘보다 강화)
//
// ⑤C(v0.6 M-14 원본 로직)는 "문서에 backtick으로 등장하는 식별자"만 검사했다. ⑤B는 그
// 제약을 없앤다 — `loadCodeConstants()`가 실제로 노출하는 모든 `allowed*` 키를 문서 언급
// 여부와 무관하게 전수 검사한다. CI에서 도는 부분이다(docs/ 불필요 — 입력이 상수 자체다).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { containsPlaceholder, isEmptyValue } from '../lib/allowedValueChecks.js';

export function checkAllowedNonEmpty(): CheckResult {
  const id = '⑤B';
  const label = '모든 allowed* 키가 비어 있지 않고 플레이스홀더가 없는지 (문서 언급 여부 무관)';
  const violations: { ref: string; message: string }[] = [];

  const constants = loadCodeConstants() as unknown as Record<string, unknown>;

  for (const [name, value] of Object.entries(constants)) {
    if (!name.startsWith('allowed')) continue;
    if (isEmptyValue(value)) {
      violations.push({ ref: 'PR-11② (M-14)', message: `\`${name}\`의 값이 비어 있습니다` });
      continue;
    }
    if (containsPlaceholder(value)) {
      violations.push({ ref: 'PR-11② (M-14)', message: `\`${name}\`의 값에 플레이스홀더(<...>) 문자열이 남아 있습니다` });
    }
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
