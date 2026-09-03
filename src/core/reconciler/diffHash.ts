// Plan.diffHash 계산 — architecture.md §1.2 (`diffHash === hash(plan)` 재계산 비교).
// PR-3(동의는 diff에 바인딩)이 성립하려면 같은 변경 집합은 항상 같은 해시를,
// 다른 변경 집합은 (실질적으로) 다른 해시를 내야 한다. 키 순서에 좌우되면 안 되므로
// 직렬화 전에 키를 재귀적으로 정렬한다.

import { createHash } from 'node:crypto';
import type { Change, ProviderId } from '../../providers/types.js';

/** 객체 키를 재귀적으로 정렬해 직렬화가 프로퍼티 삽입 순서에 좌우되지 않게 한다 */
function stableStringify(value: unknown): string {
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableStringify(item)).join(',')}]`;
  }
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  const entries = keys
    .filter((key) => record[key] !== undefined)
    .map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`);
  return `{${entries.join(',')}}`;
}

/**
 * providerId + changes로부터 결정적 sha256 해시를 계산한다.
 * `changes`의 배열 순서는 의미가 있는 데이터(적용 순서 §4.5)이므로 정렬하지 않는다 —
 * 오직 각 Change 객체 내부의 키 순서만 정규화한다.
 */
export function computeDiffHash(providerId: ProviderId, changes: readonly Change[]): string {
  const normalized = stableStringify({ providerId, changes });
  return createHash('sha256').update(normalized, 'utf8').digest('hex');
}
