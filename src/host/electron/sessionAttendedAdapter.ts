// `computeSessionAttendedFromIdleState()`(순수, vitest 검증)의 실제 Electron 입력 어댑터.
// `powerMonitor.getSystemIdleState(threshold)`의 반환 타입은 Electron 쪽에서 이미
// `'active' | 'idle' | 'locked' | 'unknown'`으로 좁혀져 있다 — 이 파일이 하는 일은 그
// 호출 자체와 임계값(초) 선택뿐이다.

import { powerMonitor } from 'electron';
import { computeSessionAttendedFromIdleState } from '../session/sessionAttended.js';

/** idle 판정 임계 — 60초 이상 입력이 없으면 idle로 본다. 이 값 자체는 `sessionAttended`
 * 판정에 영향이 없다('idle'도 attended=true로 매핑된다, `sessionAttended.ts` 참고) —
 * `powerMonitor` API가 요구하는 인자라서 값을 하나 골라야 할 뿐이다. */
const IDLE_THRESHOLD_SECONDS = 60;

export function resolveSessionAttended(): boolean {
  const idleState = powerMonitor.getSystemIdleState(IDLE_THRESHOLD_SECONDS);
  return computeSessionAttendedFromIdleState(idleState);
}
