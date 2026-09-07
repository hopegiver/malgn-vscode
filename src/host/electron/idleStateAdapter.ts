// §3.6.1 ⑥ 무인 세션 신호의 실제 Electron 어댑터 — `host/session/sessionAttended.ts`
// (순수 변환)가 요구하는 `SystemIdleState` 입력을 `powerMonitor.getSystemIdleState()`로
// 채운다. 이 파일 자신은 판정하지 않는다(변환은 `computeSessionAttendedFromIdleState`
// 하나가 전담 — 반환문에 명시: 여기서 다시 판정 로직을 쓰면 두 곳에서 같은 판정이
// 갈릴 위험이 생긴다).

import { powerMonitor } from 'electron';
import { computeSessionAttendedFromIdleState, type SystemIdleState } from '../session/sessionAttended.js';

/** 이 값을 바꾸면 함께 확인할 것: `architecture.md` §3.6.1 ⑥ 표. */
export const IDLE_THRESHOLD_SECONDS = 60;

export function resolveSessionAttended(): boolean {
  const idleState = powerMonitor.getSystemIdleState(IDLE_THRESHOLD_SECONDS) as SystemIdleState;
  return computeSessionAttendedFromIdleState(idleState);
}
