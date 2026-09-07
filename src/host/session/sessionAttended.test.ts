import { describe, expect, it } from 'vitest';
import { computeSessionAttendedFromIdleState } from './sessionAttended.js';

describe('computeSessionAttendedFromIdleState — §3.6.1 ⑥ fail-closed 매핑', () => {
  it("'active'는 true다", () => {
    expect(computeSessionAttendedFromIdleState('active')).toBe(true);
  });

  it("'idle'은 true다(잠기지 않았다면 무인 아님)", () => {
    expect(computeSessionAttendedFromIdleState('idle')).toBe(true);
  });

  it("'locked'는 false다", () => {
    expect(computeSessionAttendedFromIdleState('locked')).toBe(false);
  });

  it("'unknown'(판정 불가)은 false다 — fail-closed", () => {
    expect(computeSessionAttendedFromIdleState('unknown')).toBe(false);
  });
});
