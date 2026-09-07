// 신선도 판정 — architecture.md §2.7 "임계(예: 3× 주기) 초과면 OS 알림 1회". 순수 계산만
// 담는다: 실제 하트비트 파일을 읽는 것(OS 스케줄러가 하루 1회 실행하는 별도 프로세스의
// 몫)과 OS 알림을 띄우는 것(호스트 어댑터의 몫)은 이 파일 밖이다 — `core/reconciler/
// stopGate.ts`가 "판정 계산"과 "값을 어디서 읽어오는지"를 분리한 것과 같은 원칙.
//
// [재수렴 주기와의 관계] §2.1 표 "재수렴: 기동 시 1회 + 주기(기본 1h)" — 이 값이
// 하트비트 갱신 주기다. §2.7의 "3× 주기"를 그 값에 고정해 안전 임계값이 두 곳에서
// 따로 계산되지 않게 한다(안전 임계값은 정본 하나만 — 이 파일이 그 정본이다. 이 값을
// 바꾸면 architecture.md §2.1·§2.7도 함께 확인할 것).

export const RECONCILE_INTERVAL_MS = 60 * 60 * 1000; // §2.1 "주기(기본 1h)"
export const HEARTBEAT_STALE_MULTIPLIER = 3; // §2.7 "임계(예: 3× 주기)"
export const HEARTBEAT_STALE_THRESHOLD_MS = RECONCILE_INTERVAL_MS * HEARTBEAT_STALE_MULTIPLIER;

/**
 * 하트비트가 신선도 임계(기본 3시간)를 넘겼는지 판정한다. `lastHeartbeatAt`이
 * `null`이면(파일 자체가 없음 — 설치 직후 첫 tick 이전, 또는 파일이 지워진 상태) 이미
 * 임계를 넘긴 것으로 간주한다 — "판정 불가는 안전한 방향으로"(PR-6과 같은 정신:
 * 하트비트가 없다는 것 자체가 "죽었을 수 있다"는 신호이지 "아직 정상"이 아니다).
 */
export function isHeartbeatStale(lastHeartbeatAt: Date | null, now: Date, thresholdMs: number = HEARTBEAT_STALE_THRESHOLD_MS): boolean {
  if (lastHeartbeatAt === null) return true;
  return now.getTime() - lastHeartbeatAt.getTime() >= thresholdMs;
}
