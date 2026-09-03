// ⑤B/⑤C가 공유하는 "값 실재 + 플레이스홀더 부재" 판정(PR-11②, M-14) — 한 곳에서만
// 정의해 두 검사가 서로 다른 기준으로 갈라지지 않게 한다.

const PLACEHOLDER_RE = /<[^>]+>/;

export function containsPlaceholder(value: unknown): boolean {
  if (typeof value === 'string') return PLACEHOLDER_RE.test(value);
  if (Array.isArray(value)) return value.some(containsPlaceholder);
  if (value !== null && typeof value === 'object') {
    return Object.values(value as Record<string, unknown>).some(containsPlaceholder);
  }
  return false;
}

export function isEmptyValue(value: unknown): boolean {
  if (Array.isArray(value)) return value.length === 0;
  if (value !== null && typeof value === 'object') return Object.keys(value as object).length === 0;
  return value === undefined;
}
