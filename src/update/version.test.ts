import { describe, expect, it } from 'vitest';
import { compareUpdateVersions, parseUpdateVersion } from './version.js';

describe('parseUpdateVersion', () => {
  it('X.Y.Z 형태를 파싱한다', () => {
    expect(parseUpdateVersion('1.2.3')).toEqual({ major: 1, minor: 2, patch: 3 });
  });

  it('부분 생략·prerelease·빈 문자열은 거부한다', () => {
    expect(parseUpdateVersion('1.2')).toBeNull();
    expect(parseUpdateVersion('1.2.3-beta')).toBeNull();
    expect(parseUpdateVersion('')).toBeNull();
    expect(parseUpdateVersion('v1.2.3')).toBeNull();
  });
});

describe('compareUpdateVersions', () => {
  it('major/minor/patch 순서로 비교한다', () => {
    const a = parseUpdateVersion('1.2.3')!;
    const b = parseUpdateVersion('1.2.4')!;
    const c = parseUpdateVersion('2.0.0')!;
    expect(compareUpdateVersions(a, b)).toBe(-1);
    expect(compareUpdateVersions(b, a)).toBe(1);
    expect(compareUpdateVersions(a, a)).toBe(0);
    expect(compareUpdateVersions(c, b)).toBe(1);
  });
});
