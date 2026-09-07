// 크래시 루프 방지 — architecture.md §2.7 "자동 재시작은 지수 백오프 + 상한(예: 5회/시).
// 상한 도달 시 재시작을 멈추고 알림 + 진단 리포트 유도. 무한 재시작은 로그인 경험을
// 해치고 원인을 감춘다."
//
// 순수 계산만 담는다 — 재시작 타임스탬프 기록을 어디에 영속화할지(메모리·파일)는
// 호출자(호스트 어댑터의 프로세스 감독 코드)의 몫이다.

export const CRASH_LOOP_MAX_RESTARTS_PER_WINDOW = 5; // §2.7 "예: 5회/시"
export const CRASH_LOOP_WINDOW_MS = 60 * 60 * 1000; // "/시"
/** 지수 백오프 밑변 — n번째(0-indexed) 재시작 전 대기 시간 = BASE_DELAY_MS * 2^n,
 * 상한 윈도우(1시간) 안에서 5회까지만 허용되므로 마지막 지연이 윈도우를 넘지 않도록
 * 작은 밑변을 쓴다(1s, 2s, 4s, 8s, 16s — 5회 합산 31초, 윈도우 1시간에 비해 충분히 작다). */
export const CRASH_LOOP_BASE_DELAY_MS = 1000;

export interface CrashLoopDecision {
  /** true면 재시작을 허용한다. false면 §2.7 "상한 도달" — 재시작을 멈추고 알림 +
   * 진단 리포트로 격하해야 한다(그 UI 배선은 호출자 몫). */
  readonly allowRestart: boolean;
  /** allowRestart가 true일 때만 의미가 있다 — 재시작 전 대기할 시간(지수 백오프). */
  readonly delayMs: number;
}

/**
 * `recentRestartTimestamps`(밀리초 epoch, 오름차순일 필요 없음)에서 `windowMs` 안에 든
 * 것만 세어 상한을 판정한다. 상한 미만이면 재시작을 허용하고 그 순번(윈도우 안 재시작
 * 횟수)에 따른 지수 백오프 지연을 함께 반환한다.
 */
export function decideCrashLoopRestart(
  recentRestartTimestamps: readonly number[],
  now: number,
  options: { readonly maxRestartsPerWindow?: number; readonly windowMs?: number; readonly baseDelayMs?: number } = {}
): CrashLoopDecision {
  const maxRestarts = options.maxRestartsPerWindow ?? CRASH_LOOP_MAX_RESTARTS_PER_WINDOW;
  const windowMs = options.windowMs ?? CRASH_LOOP_WINDOW_MS;
  const baseDelayMs = options.baseDelayMs ?? CRASH_LOOP_BASE_DELAY_MS;

  const withinWindow = recentRestartTimestamps.filter((ts) => now - ts < windowMs);
  const countWithinWindow = withinWindow.length;

  if (countWithinWindow >= maxRestarts) {
    return { allowRestart: false, delayMs: 0 };
  }
  const delayMs = baseDelayMs * 2 ** countWithinWindow;
  return { allowRestart: true, delayMs };
}
