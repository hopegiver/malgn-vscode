import { describe, expect, it } from 'vitest';
import { compareVersions, formatRange, narrowRange, parseRange, parseVersion } from './semver.js';

describe('parseVersion', () => {
  it('parses major.minor.patch', () => {
    expect(parseVersion('2.1.237')).toEqual({ major: 2, minor: 1, patch: 237 });
  });
  it('defaults missing minor/patch to 0', () => {
    expect(parseVersion('2')).toEqual({ major: 2, minor: 0, patch: 0 });
  });
  it('rejects non-numeric input', () => {
    expect(parseVersion('not-a-version')).toBeNull();
  });
});

describe('compareVersions', () => {
  it('orders by major, then minor, then patch', () => {
    expect(compareVersions(parseVersion('1.9.0')!, parseVersion('2.0.0')!)).toBe(-1);
    expect(compareVersions(parseVersion('2.1.237')!, parseVersion('2.1.236')!)).toBe(1);
    expect(compareVersions(parseVersion('1.0.0')!, parseVersion('1.0.0')!)).toBe(0);
  });
});

describe('parseRange', () => {
  it('parses lower-bound-only range', () => {
    expect(parseRange('>=2.1.237')).toEqual({ lower: { op: '>=', version: { major: 2, minor: 1, patch: 237 } } });
  });
  it('parses lower+upper range', () => {
    expect(parseRange('>=1.8.24 <2.0.0')).toEqual({
      lower: { op: '>=', version: { major: 1, minor: 8, patch: 24 } },
      upper: { op: '<', version: { major: 2, minor: 0, patch: 0 } },
    });
  });
  it('rejects a range without a lower bound', () => {
    expect(parseRange('<2.0.0')).toBeNull();
  });
  it('rejects garbage', () => {
    expect(parseRange('not a range')).toBeNull();
  });
});

describe('formatRange round-trip', () => {
  it('round-trips lower+upper', () => {
    const range = parseRange('>=1.8.24 <2.0.0')!;
    expect(formatRange(range)).toBe('>=1.8.24 <2.0.0');
  });
});

// --- PR-9(정책은 좁힐 수만 있다) 정본 테스트 — policy-contract.md §2.3 표와 1:1 대응 ---
describe('narrowRange — PR-9 좁히기 전용 규칙', () => {
  it('하한을 올리는 정책 값은 채택된다', () => {
    const result = narrowRange('>=1.8.24 <2.0.0', '>=1.8.30 <2.0.0');
    expect(result).not.toBeNull();
    expect(result!.effective).toBe('>=1.8.30 <2.0.0');
    expect(result!.widened).toBe(false);
  });

  it('상한을 내리는 정책 값은 채택된다', () => {
    const result = narrowRange('>=1.8.24 <2.0.0', '>=1.8.24 <1.9.0');
    expect(result).not.toBeNull();
    expect(result!.effective).toBe('>=1.8.24 <1.9.0');
    expect(result!.widened).toBe(false);
  });

  it('하한을 내리려는 시도는 폐기되고 widened=true다 (번들 값 유지)', () => {
    const result = narrowRange('>=2.1.237', '>=1.0.0');
    expect(result).not.toBeNull();
    expect(result!.effective).toBe('>=2.1.237'); // 번들 값 그대로
    expect(result!.widened).toBe(true);
  });

  it('상한을 올리려는(없애려는) 시도는 폐기되고 widened=true다 (번들 값 유지)', () => {
    const result = narrowRange('>=1.8.24 <2.0.0', '>=1.8.24 <3.0.0');
    expect(result).not.toBeNull();
    expect(result!.effective).toBe('>=1.8.24 <2.0.0'); // 번들 상한 유지
    expect(result!.widened).toBe(true);
  });

  it('번들에 상한이 없을 때 정책이 상한을 추가하는 것은 항상 좁히기다', () => {
    const result = narrowRange('>=2.1.237', '>=2.1.237 <3.0.0');
    expect(result).not.toBeNull();
    expect(result!.effective).toBe('>=2.1.237 <3.0.0');
    expect(result!.widened).toBe(false);
  });

  it('정책 range가 파싱 불가하면 null(형식 오류로 처리, widened 아님)', () => {
    expect(narrowRange('>=2.1.237', 'garbage')).toBeNull();
  });
});
