// `trust.grant` 표면의 L2 동의 UI 실제 구현 — architecture.md §3.6.1 ③ "L2 동의 1회,
// 절대경로 전문 표시". `dialog.showMessageBox`를 쓴다(§2.8이 강제하는 렌더러 프로세스가
// 아니라 메인 프로세스 네이티브 다이얼로그라 CSP·nonce 요건이 적용되지 않는다 — 이
// 다이얼로그는 HTML을 렌더링하지 않는다).
//
// 이 파일은 `trustGrantSurface.ts`가 요구하는 `showConsentDialog` 시그니처를 채우는
// 어댑터일 뿐이고, 로직(승인 시에만 원장에 기록)은 이미 그 파일에 있다 — 이 파일에
// 판단을 두지 않는다.

import { dialog } from 'electron';

export async function showTrustConsentDialogElectron(absoluteTargetFolderPath: string): Promise<boolean> {
  const result = await dialog.showMessageBox({
    type: 'question',
    // 외부 통제 문자열이 아니라 이 함수 자신이 조립하는 고정 문구 + 절대경로 값이라
    // §2.8의 "외부 통제 문자열은 텍스트로만 렌더" 요건과 별개다(HTML 삽입 자체가 없다).
    message: '이 폴더를 신뢰하시겠습니까?',
    detail: absoluteTargetFolderPath,
    buttons: ['신뢰함', '취소'],
    defaultId: 1,
    cancelId: 1,
  });
  return result.response === 0;
}
