import { describe, expect, it } from 'vitest';
import { InvalidUpdateVersionError, UpdateDowngradeRejectedError, assertNotDowngrade } from './downgradeGuard.js';

describe('assertNotDowngrade (U-2)', () => {
  it('후보 버전이 더 크면 통과한다', () => {
    expect(() => assertNotDowngrade('1.2.0', '1.3.0')).not.toThrow();
  });

  it('후보 버전이 더 작으면 다운그레이드로 거부한다', () => {
    expect(() => assertNotDowngrade('1.3.0', '1.2.0')).toThrow(UpdateDowngradeRejectedError);
  });

  it('같은 버전도 진행 대상이 아니므로 거부한다(호출자는 이 경우 애초에 이 함수를 부르지 않는 것이 정상 경로)', () => {
    expect(() => assertNotDowngrade('1.2.0', '1.2.0')).toThrow(UpdateDowngradeRejectedError);
  });

  it('형식이 깨진 버전 문자열은 fail-closed로 던진다', () => {
    expect(() => assertNotDowngrade('not-a-version', '1.2.0')).toThrow(InvalidUpdateVersionError);
    expect(() => assertNotDowngrade('1.2.0', 'not-a-version')).toThrow(InvalidUpdateVersionError);
  });
});
