// 개발 채널 진입점 — `entry.prod.ts`와 대칭이다. `assembleDevUpdateChannelConfig`만
// 다르고 나머지 조립 로직은 완전히 같다(공유 로직은 `app.js`에 있다 — 두 파일이
// 각자 중복 구현하지 않는다).

import { app } from 'electron';
import { join } from 'node:path';
import { bootstrapAndRunOnce, installTopLevelExceptionHandler } from './app.js';
import { createTray } from './electron/trayAdapter.js';
import { DEV_BUNDLE_IDENTIFIER_ENV_VAR, assembleDevUpdateChannelConfig } from './update/devChannel.js';
import { wireApplyMenu } from './apply/wireApplyMenu.js';
import { openMainWindow } from './window/createMainWindow.js';
import { registerProjectIpc } from './window/registerProjectIpc.js';
import type { ActivationStatusReport } from './activation/activationSequence.js';

const TRAY_ICON_PATH = `${app.getAppPath()}/resources/tray-icon-dev.png`;

// [MVP 범위 밖 — 자동업데이트 미실행] 로컬 dev `.app`은 Finder로 실행되면 셸
// 환경변수를 물려받지 못해 `MALGN_DEV_BUNDLE_IDENTIFIER`가 항상 비어 있다. MVP는
// 자동업데이트 실동작이 범위 밖이므로, 값이 없으면 채널 조립 자체를 건너뛴다 — 단,
// "아무 일도 안 일어난 것"과 "업데이트 기능이 의도적으로 꺼져 있는 것"을 구분할 수
// 있도록 콘솔에 그 사실을 남긴다(나중에 W14가 이 값을 주입하기 시작하면 이 분기가
// 조용히 사라지는 게 아니라 정상 조립 경로를 그대로 탄다 — 아래 참 분기).
// prod 쪽(entry.prod.ts → assembleProdUpdateChannelConfig)의 fail-closed(값 없으면
// BundleIdentifierNotConfiguredError throw)는 건드리지 않는다 — 이 분기는 dev
// 엔트리에만 있다.
if (process.env[DEV_BUNDLE_IDENTIFIER_ENV_VAR]) {
  void assembleDevUpdateChannelConfig(process.env);
} else {
  console.warn(
    `[entry.dev.ts] ${DEV_BUNDLE_IDENTIFIER_ENV_VAR}이 설정되지 않아 업데이트 채널 조립을 건너뜁니다 — MVP 범위에서 자동업데이트는 실행되지 않습니다(W14에서 활성화 예정).`,
  );
}

// W11 — 메인 창(대시보드) 렌더러 정적 자산 경로. `resources/`와 같은 관용구
// (`app.getAppPath()` 기준 상대경로 — dev 실행(`electron dist/dev-app/host/entry.dev.cjs`)과
// 패키징된 `.app` 양쪽에서 `build-host.mjs`가 같은 자리(`<outDir>/renderer/`)에
// 둔다).
const RENDERER_DIR = `${app.getAppPath()}/renderer`;
const WORKSPACE_ROOT = join(app.getPath('home'), 'workspace');

app.whenReady().then(async () => {
  const { tray, setTrayState } = createTray(TRAY_ICON_PATH);
  installTopLevelExceptionHandler(setTrayState);
  // 렌더러가 파일을 직접 읽지 못하게(§2.8 신뢰 경계) 스캔은 항상 메인 프로세스에서
  // 하고 IPC로만 넘긴다 — 창이 열리기 전에 미리 등록해 둔다.
  registerProjectIpc(WORKSPACE_ROOT);
  const openDashboard = (): void => {
    openMainWindow({ preloadPath: `${RENDERER_DIR}/preload.cjs`, indexHtmlPath: `${RENDERER_DIR}/index.html` });
  };
  // `reportStatus`(§2.2 ⑦)는 `bootstrapAndRunOnce`가 resolve되기 **전**, `handles`(journal·
  // trustLedger)가 아직 없는 시점에 호출된다 — 그래서 report만 여기 잠깐 담아 두고,
  // `handles`가 준비된 뒤 한 번만 `wireApplyMenu`를 부른다(이 슬라이스는 재수렴 루프가
  // 없어 활성화당 report가 정확히 1개다).
  let latestReport: ActivationStatusReport | undefined;
  const handles = await bootstrapAndRunOnce(setTrayState, (report) => {
    latestReport = report;
  });
  if (latestReport) {
    wireApplyMenu({ tray, handles, homeDir: app.getPath('home'), openMainWindow: openDashboard }, latestReport);
  }
});
