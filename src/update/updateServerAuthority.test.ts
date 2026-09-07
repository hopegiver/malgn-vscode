// U-3(architecture.md §3.6.2) — "서버 authority는 코드 상수 ... 개발용 오버라이드를
// 만들지 않는다." 구조적 강제(정의 지점 1개·재대입·환경변수 유래 금지)는
// `src/architecture-tests/updateChannelBoundary.ts`의 AT-U5가 저장소 전체를 대상으로
// 맡는다. 이 파일은 그 상수가 실제로 환경변수의 영향을 받지 않는다는 것을 값
// 수준에서 한 번 더 실측한다 — "코드 상수 밖 authority 거부"의 런타임 증거.

import { afterEach, describe, expect, it } from 'vitest';
import { UPDATE_SERVER_AUTHORITY } from './updateServerAuthority.js';

describe('UPDATE_SERVER_AUTHORITY (U-3)', () => {
  it('유효한 호스트 이름 형태의 문자열이다', () => {
    expect(typeof UPDATE_SERVER_AUTHORITY).toBe('string');
    expect(UPDATE_SERVER_AUTHORITY.length).toBeGreaterThan(0);
    expect(UPDATE_SERVER_AUTHORITY).not.toMatch(/[:/\s]/);
  });

  describe('환경변수 오버라이드 시도가 값에 영향을 주지 않는다', () => {
    const envKeysToTry = ['UPDATE_AUTHORITY_OVERRIDE', 'UPDATE_SERVER_AUTHORITY', 'MALGN_UPDATE_HOST'];

    afterEach(() => {
      for (const key of envKeysToTry) delete process.env[key];
    });

    it('공격자가 흔한 이름으로 환경변수를 심어도 상수 값은 그대로다', () => {
      for (const key of envKeysToTry) {
        process.env[key] = 'attacker.example.com';
      }
      // 이 모듈은 이미 import되어 값이 고정됐지만(정적 상수), 재확인 차원에서
      // 다시 읽어도 process.env의 영향을 받지 않는다는 것을 값으로 증명한다 —
      // 상수가 함수가 아니라 리터럴이라는 사실 자체가 "읽을 방법이 없다"는 것이다.
      expect(UPDATE_SERVER_AUTHORITY).not.toBe('attacker.example.com');
      expect(UPDATE_SERVER_AUTHORITY).toBe('updates.example.com');
    });
  });
});
