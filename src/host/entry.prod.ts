// 운영 채널 진입점 — architecture.md §3.6.2 U-3 "개발 빌드는 별도 키 + 별도 상수 +
// 별도 번들 식별자로 가른다(한 바이너리에 분기 금지)"의 실행 지점. **이 파일과
// `entry.dev.ts` 중 어느 쪽이 실제로 빌드·실행되는가가 분기의 유일한 위치다** — 이
// 파일 자신의 코드 안에는 채널을 고르는 조건문이 없다(있을 수 없다 — 이 파일은
// 이미 운영 채널 하나만 안다).
//
// [완료 판정 #2 — AT-U 재확인] 이 파일은 `src/update/**` 밖(`src/host/`)에 있어
// AT-U1~U3의 스캔 대상이 아니지만, 소비하는 세 모듈(`update/updateFlow.js`·
// `update/publicKeys.js`·`update/updateServerAuthority.js`)은 정책 모듈을 참조하지
// 않는다 — 이 파일도 정책 관련 식별자를 쓰지 않는다(코드에 그런 값이 필요한 지점
// 자체가 없다).

import { app } from 'electron';
import { bootstrapAndRunOnce, installTopLevelExceptionHandler } from './app.js';
import { createTray } from './electron/trayAdapter.js';
import { assembleProdUpdateChannelConfig } from './update/prodChannel.js';
import { wireApplyMenu } from './apply/wireApplyMenu.js';
import type { ActivationStatusReport } from './activation/activationSequence.js';

// 패키징(W14)이 실제 트레이 아이콘 자산을 리소스 디렉터리에 배치한다 — 이 경로는
// 그 계약점을 문서화하는 자리다(자산이 없으면 Electron이 빈 아이콘으로 계속 진행할
// 뿐 앱을 막지 않는다 — PR-8과 같은 정신).
const TRAY_ICON_PATH = `${app.getAppPath()}/resources/tray-icon.png`;

// U-3 — 채널 조립을 여기서 1회 수행한다(모듈 선택으로만 분기, 런타임 if/switch 없음).
// 값이 아직 없으면(패키징 이전) 명시적으로 던진다(BundleIdentifierNotConfiguredError) —
// 조용히 빈 문자열로 기동하지 않는다.
void assembleProdUpdateChannelConfig(process.env);

app.whenReady().then(async () => {
  const { tray, setTrayState } = createTray(TRAY_ICON_PATH);
  installTopLevelExceptionHandler(setTrayState);
  // entry.dev.ts와 동일한 이유(주석 참고) — reportStatus가 handles보다 먼저 온다.
  let latestReport: ActivationStatusReport | undefined;
  const handles = await bootstrapAndRunOnce(setTrayState, (report) => {
    latestReport = report;
  });
  if (latestReport) {
    wireApplyMenu({ tray, handles, homeDir: app.getPath('home') }, latestReport);
  }
});
