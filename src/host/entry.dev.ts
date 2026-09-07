// 개발 채널 진입점 — `entry.prod.ts`와 대칭이다. `assembleDevUpdateChannelConfig`만
// 다르고 나머지 조립 로직은 완전히 같다(공유 로직은 `app.js`에 있다 — 두 파일이
// 각자 중복 구현하지 않는다).

import { app } from 'electron';
import { bootstrapAndRunOnce, installTopLevelExceptionHandler } from './app.js';
import { createTray } from './electron/trayAdapter.js';
import { assembleDevUpdateChannelConfig } from './update/devChannel.js';

const TRAY_ICON_PATH = `${app.getAppPath()}/resources/tray-icon-dev.png`;

void assembleDevUpdateChannelConfig(process.env);

app.whenReady().then(async () => {
  const { setTrayState } = createTray(TRAY_ICON_PATH);
  installTopLevelExceptionHandler(setTrayState);
  await bootstrapAndRunOnce(setTrayState);
});
