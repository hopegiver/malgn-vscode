// 트레이 아이콘 — architecture.md §2.2 "① 트레이 아이콘을 먼저 띄우고 즉시 반환". 이
// 파일은 그 ①단계와 `activationSequence.ts`가 요구하는 `setTrayState` 콜백을 실제
// Electron `Tray`로 연결하는 얇은 어댑터다. 상태별 아이콘 자산(PNG)은 패키징(W14)이
// 결정할 실제 리소스 번들링 방식에 따라 달라지므로, 이 슬라이스는 배지 텍스트
// (title/tooltip)로 상태를 표현한다 — 트레이 아이콘 자체가 없어도(자산 부재) 앱이
// 죽지 않게 하기 위해서다(PR-8과 같은 정신: 부수 자산 부재가 핵심 기능을 막지
// 않는다).

import { Tray } from 'electron';
import type { TrayState } from '../activation/activationSequence.js';

const TRAY_STATE_LABEL: Readonly<Record<TrayState, string>> = {
  starting: 'Malgn — 시작 중',
  ok: 'Malgn — 정상',
  drift: 'Malgn — 변경 감지됨',
  blocked: 'Malgn — 정지됨',
  error: 'Malgn — 오류',
};

/**
 * 트레이 아이콘을 만들고 `setTrayState` 콜백을 반환한다. 아이콘 이미지는 호출자가
 * 넘긴다(패키징이 실제 자산 경로를 결정하므로 이 파일이 경로를 하드코딩하지 않는다).
 */
export function createTray(iconPath: string): { readonly tray: Tray; readonly setTrayState: (state: TrayState) => void } {
  const tray = new Tray(iconPath);
  tray.setToolTip(TRAY_STATE_LABEL.starting);
  const setTrayState = (state: TrayState): void => {
    tray.setToolTip(TRAY_STATE_LABEL[state]);
  };
  return { tray, setTrayState };
}
