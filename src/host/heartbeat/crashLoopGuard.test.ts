import { describe, expect, it } from 'vitest';
import { CRASH_LOOP_MAX_RESTARTS_PER_WINDOW, decideCrashLoopRestart } from './crashLoopGuard.js';

describe('decideCrashLoopRestart — §2.7 지수 백오프 + 상한(5회/시)', () => {
  it('재시작 이력이 없으면 즉시 허용하고 최소 지연을 준다', () => {
    const decision = decideCrashLoopRestart([], 0);
    expect(decision.allowRestart).toBe(true);
    expect(decision.delayMs).toBe(1000);
  });

  it('윈도우 안 재시작 횟수가 늘수록 지연이 지수적으로 커진다', () => {
    const now = 1_000_000;
    const timestamps = [now - 1000, now - 2000]; // 윈도우 안에 2회
    const decision = decideCrashLoopRestart(timestamps, now);
    expect(decision.allowRestart).toBe(true);
    expect(decision.delayMs).toBe(1000 * 2 ** 2); // 세 번째 시도
  });

  it(`윈도우 안 재시작이 ${CRASH_LOOP_MAX_RESTARTS_PER_WINDOW}회에 도달하면 재시작을 멈춘다`, () => {
    const now = 1_000_000;
    const timestamps = Array.from({ length: CRASH_LOOP_MAX_RESTARTS_PER_WINDOW }, (_, i) => now - i * 100);
    const decision = decideCrashLoopRestart(timestamps, now);
    expect(decision.allowRestart).toBe(false);
    expect(decision.delayMs).toBe(0);
  });

  it('윈도우 밖(1시간 이전)의 재시작은 카운트에서 제외된다', () => {
    const now = 10_000_000;
    const oldTimestamps = Array.from({ length: 20 }, () => now - 2 * 60 * 60 * 1000); // 2시간 전, 20회
    const decision = decideCrashLoopRestart(oldTimestamps, now);
    expect(decision.allowRestart).toBe(true);
    expect(decision.delayMs).toBe(1000); // 윈도우 안 이력 0건 취급
  });

  it('커스텀 옵션(상한·윈도우·밑변)을 주입할 수 있다', () => {
    const decision = decideCrashLoopRestart([0, 1, 2], 100, { maxRestartsPerWindow: 3, windowMs: 1000, baseDelayMs: 10 });
    expect(decision.allowRestart).toBe(false);
  });
});
