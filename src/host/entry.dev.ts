// 개발 채널 진입점 — `entry.prod.ts`와 대칭이다. `assembleDevUpdateChannelConfig`만
// 다르고 나머지 조립 로직은 완전히 같다(공유 로직은 `app.js`에 있다 — 두 파일이
// 각자 중복 구현하지 않는다).

import { app } from 'electron';
import { bootstrapAndRunOnce, installTopLevelExceptionHandler } from './app.js';
import { createTray } from './electron/trayAdapter.js';
import { assembleDevUpdateChannelConfig } from './update/devChannel.js';
import { wireApplyMenu } from './apply/wireApplyMenu.js';
import type { ActivationStatusReport } from './activation/activationSequence.js';

const TRAY_ICON_PATH = `${app.getAppPath()}/resources/tray-icon-dev.png`;

void assembleDevUpdateChannelConfig(process.env);

app.whenReady().then(async () => {
  const { tray, setTrayState } = createTray(TRAY_ICON_PATH);
  installTopLevelExceptionHandler(setTrayState);
  // `reportStatus`(§2.2 ⑦)는 `bootstrapAndRunOnce`가 resolve되기 **전**, `handles`(journal·
  // trustLedger)가 아직 없는 시점에 호출된다 — 그래서 report만 여기 잠깐 담아 두고,
  // `handles`가 준비된 뒤 한 번만 `wireApplyMenu`를 부른다(이 슬라이스는 재수렴 루프가
  // 없어 활성화당 report가 정확히 1개다).
  let latestReport: ActivationStatusReport | undefined;
  const handles = await bootstrapAndRunOnce(setTrayState, (report) => {
    latestReport = report;
  });
  if (latestReport) {
    wireApplyMenu({ tray, handles, homeDir: app.getPath('home') }, latestReport);
  }
});
