// 무인 세션 판정의 실제 OS 신호 계산 — architecture.md §3.6.1 ⑥ "신호원: 세션 attach
// 여부 · 화면 잠금 · 콘솔 vs 원격 세션 · 앱 창 포그라운드(macOS CGSessionCopyCurrentDictionary
// / Windows WTSQuerySessionInformation + 세션 잠금 이벤트)". `core/reconciler/stopGate.ts`가
// 명시적으로 "이 신호의 실제 OS별 계산은 이 모듈의 책임이 아니다"라고 밝힌 자리가
// 여기다(호스트 어댑터, W-N1).
//
// [범위를 좁힌 이유 — 정직 표기] 원문이 언급하는 4개 신호원 중 macOS
// `CGSessionCopyCurrentDictionary`·Windows `WTSQuerySessionInformation`은 네이티브 바인딩
// (추가 npm 의존 또는 커스텀 네이티브 모듈)이 필요하다 — `tech-stack.md` §3 "런타임
// 의존성 2개 상한"과 §0.6 "플랫폼 런타임 추가는 1개까지"를 지키기 위해 이번 슬라이스는
// **Electron이 이미 내장한 신호**(`powerMonitor.getSystemIdleState()` — 화면 잠금 상태를
// `'locked'`로 직접 보고하고, macOS·Windows 양쪽에서 동작하는 크로스플랫폼 API)만
// 쓴다. "콘솔 vs 원격 세션" 구분은 이 신호에 없어 다루지 못한다 — **판정 불가 항목이므로
// 그 축은 반영하지 않고, 대신 아래 매핑에서 'unknown'을 fail-closed(false)로 접어
// 과대 신뢰를 만들지 않는다.** 더 정밀한 신호가 필요해지면(예: 원격 세션 구분) 네이티브
// 애드온 도입은 §0.6의 "추가는 1개까지" 게이트를 다시 통과해야 한다.

export type SystemIdleState = 'active' | 'idle' | 'locked' | 'unknown';

/**
 * `powerMonitor.getSystemIdleState(threshold)`의 결과를 `sessionAttended` 불리언으로
 * 옮기는 단일 변환 지점(`core/trust/ledger.ts`의 `computeTargetFolderTrusted`와 같은
 * 패턴). **판정 불가(`'unknown'`)와 화면 잠금(`'locked'`)은 둘 다 `false`다** — §3.6.1 ⑥
 * "판정 불가 시 sessionAttended = false(fail-closed)"를 그대로 따른다. `'idle'`은
 * 마우스·키보드 입력이 없을 뿐 잠금 상태가 아니므로 `true`로 취급한다 — 이 함수가
 * 구분하는 것은 "사람이 지금 이 화면 앞에 있는가"가 아니라 "이 세션이 잠겨 있지 않고
 * 판정 가능한가"다(원문이 실제로 막으려는 사고 — 잠긴 화면 뒤에서 자동으로 도는
 * apply — 를 정확히 겨눈다).
 */
export function computeSessionAttendedFromIdleState(idleState: SystemIdleState): boolean {
  if (idleState === 'locked') return false;
  if (idleState === 'unknown') return false;
  return true;
}
