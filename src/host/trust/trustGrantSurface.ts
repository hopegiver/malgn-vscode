// `trust.grant` 표면 배선 — architecture.md §3.6.1 ③("새 폴더가 처음 I-B 대상이 될 때
// L2 동의 1회, 절대경로 전문 표시. 이 표면은 trust.grant이며 STOPPABLE_SURFACES에
// 넣지 않는다 — 넣으면 ③이 자기 해소 경로를 막아 스스로 풀 수 없는 정지가 된다").
//
// [이 파일이 하지 않는 것 — 의도적] `core/reconciler/stopGate.ts`를 import하지 않는다.
// 정지 게이트 판정 함수를 호출하거나 정지 가능 표면 상수를 참조하는 코드가 이 파일에
// 없다는 것 자체가 "trust.grant는 정지 판정을 거치지 않는다"는 성질의 직접적인
// 증거다 — `trustGrantSurface.test.ts`의 소스 검사 describe가 이 파일의 소스 텍스트에
// 그 배선(import·호출)이 등장하지 않음을 고정한다.
//
// [L2 동의] 실제 동의 다이얼로그(Electron `dialog.showMessageBox`)는 이 파일 밖
// (`src/host/electron/trustDialogAdapter.ts`)에 있다 — 이 파일은 "다이얼로그를 어떻게
// 띄우는가"를 모르는 순수 오케스트레이션이고, `showConsentDialog`를 주입받는다(DI).

import type { TrustLedger, TrustState } from '../../core/trust/ledger.js';

export interface TrustGrantSurfaceDeps {
  readonly ledger: TrustLedger;
  /** L2 동의 UI — 절대경로 전문을 사람에게 보여주고 승인/거부를 받는다. 실제 구현은
   * Electron dialog(호스트 어댑터, 이 파일 밖)이고 테스트는 가짜 함수를 주입한다. */
  readonly showConsentDialog: (absoluteTargetFolderPath: string) => Promise<boolean>;
}

export interface TrustGrantOutcome {
  readonly state: TrustState;
  /** 사용자가 동의 다이얼로그에서 실제로 무엇을 선택했는지 — 이미 trusted였던 경로도
   * 다시 물어볼지는 호출자(대시보드 UI)가 결정한다. 이 함수는 항상 다이얼로그를 띄운다. */
  readonly userApproved: boolean;
}

/**
 * `trust.grant` 표면의 유일한 진입점. 절대경로 전문을 담아 L2 동의를 요청하고,
 * 승인하면 `ledger.grant()`로 즉시 반영한다. 거부하면 원장을 건드리지 않는다(기본값
 * `untrusted` 유지 — C-11이 요구하는 fail-closed와 일치).
 *
 * **정지 판정을 거치지 않는다** — 이 함수는 정지 게이트 판정 함수를 호출하지 않고, 킬
 * 스위치·호환 게이트·무인 세션 등 어떤 정지 신호도 조회하지 않는다. `trust.grant`가
 * `STOPPABLE_SURFACES` 밖인 것은 "이 함수가 정지 검사를 생략해도 안전하다"는 뜻이
 * 아니라 "애초에 정지 판정 대상이 아니다"라는 뜻이다(§3.6.1 ③ 원문 — 자기 해소 경로).
 */
export async function requestTargetFolderTrust(
  targetFolderPath: string,
  deps: TrustGrantSurfaceDeps
): Promise<TrustGrantOutcome> {
  const userApproved = await deps.showConsentDialog(targetFolderPath);
  if (!userApproved) {
    return { state: await deps.ledger.get(targetFolderPath), userApproved: false };
  }
  await deps.ledger.grant(targetFolderPath);
  return { state: 'trusted', userApproved: true };
}
