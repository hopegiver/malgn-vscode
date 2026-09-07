// Electron 메인 프로세스 부트스트랩 — `extension.ts`(VS Code 확장, 23줄)의 네이티브
// 대응물이 조립되는 자리(architecture.md §11 W-N1). **로직을 여기로 끌어올리지
// 않는다** — 이 파일이 하는 일은 이미 검증된 조각(activationSequence·TrustLedger·
// JournalStore·heartbeat)을 실제 Electron API·실제 경로로 연결하는 것뿐이다.
//
// [조립만 하고 아직 실행하지 않는 것 — 정직 표기] 로그인 항목(자동 시작) L2 등록과
// 그때의 자기 서명 자가검증(§2.1 표), Electron 번들로의 실제 패키징·아이콘 자산은
// 이 슬라이스 범위 밖이다(§7.5.1/W14, 그리고 이번 작업 지시의 A~E 항목 밖) — 이
// 함수는 그 등록이 나중에 걸릴 자리(`app.whenReady().then(...)` 안)를 이미 갖고
// 있으므로 다음 슬라이스가 이 함수 본문에 단계를 추가하기만 하면 된다.
//
// [apply()를 호출하지 않는다] 이 파일도 `Provider.apply`·`gate.assertValid`를
// import하지 않는다 — `runActivationSequence`만 호출한다.

import { app } from 'electron';
import { TrustLedger } from '../core/trust/ledger.js';
import { JournalStore } from '../core/journal/store.js';
import { runActivationSequence } from './activation/activationSequence.js';
import type { TrayState } from './activation/activationSequence.js';
import { ensureAppDataDirStructure } from './appData/ensure.js';
import { resolveRealAppDataDir } from './electron/appDataDirAdapter.js';
import { showTrustConsentDialogElectron } from './electron/trustDialogAdapter.js';
import { requestTargetFolderTrust } from './trust/trustGrantSurface.js';
import { HEARTBEAT_FILE_NAME, writeHeartbeat } from './heartbeat/heartbeat.js';
import { detectRemoteEnvSignals, buildRemoteEnvWarningMessage } from './remote/remoteEnvDetection.js';
import { loadEffectivePolicyWithFallback } from './policySource/loadWithFallback.js';
import type { ReadTextFile } from './policySource/checkoutAndInstalledPaths.js';
import compatibilityRaw from '../../compat/compatibility.json';
import { join } from 'node:path';
import { readFile } from 'node:fs/promises';

export interface AppRuntimeHandles {
  readonly setTrayState: (state: TrayState) => void;
  readonly trustLedger: TrustLedger;
  readonly journal: JournalStore;
  readonly appDataDir: string;
}

const readTextFile: ReadTextFile = (path) => readFile(path, 'utf8');

/**
 * §2.2 활성화 시퀀스를 실제 경로·실제 저장소로 조립해 1회 실행한다. 호출자
 * (`entry.prod.ts`/`entry.dev.ts`)가 `app.whenReady()` 이후 이 함수를 부른다. 재수렴
 * 주기(`RECONCILE_INTERVAL_MS`, §2.1 "기본 1h")마다 다시 호출하는 반복은 그 호출자의
 * 몫이다(이 함수 자신은 1회 실행만 책임진다 — 반복 스케줄링과 활성화 로직을 한 함수에
 * 묶지 않는다).
 */
export async function bootstrapAndRunOnce(setTrayState: (state: TrayState) => void): Promise<AppRuntimeHandles> {
  const appDataDir = resolveRealAppDataDir();
  await ensureAppDataDirStructure({ appDataDir, platform: process.platform });

  const trustLedger = new TrustLedger({ baseDir: appDataDir });
  const journal = new JournalStore({ baseDir: appDataDir });

  // §2.6 "헤더 상시 표시" — 원격 신호가 있으면 경고를 1회 로그에 남긴다(실제 트레이
  // 알림 UI는 이 슬라이스 범위 밖이지만, 신호 계산과 문구 조립은 이미 실행된다).
  const remoteSignals = detectRemoteEnvSignals(process.env);
  const remoteWarning = buildRemoteEnvWarningMessage(remoteSignals);
  if (remoteWarning) {
    // eslint 없음 — LogOutputChannel 배선(W-N1 후속)이 아직 없어 console을 쓴다.
    // eslint-disable-next-line no-console
    console.warn(`[malgn] ${remoteWarning}`);
  }

  const claudeHomeDir = join(app.getPath('home'), '.claude');

  await runActivationSequence({
    setTrayState,
    loadPolicy: () =>
      loadEffectivePolicyWithFallback({
        claudeHomeDir,
        readTextFile,
        currentExtensionVersion: compatibilityRaw.extensionVersion,
      }),
    providers: [], // W7~W10 이전 — 항상 빈 배열
    buildDesiredSlice: (providerId) => ({ providerId }),
    detectContext: { targetFolderTrusted: false }, // I-B 대상 provider가 없어 이 슬라이스에서는 미사용
    reportStatus: () => {
      // 대시보드 UI(W11)가 아직 없다 — 상태 리포트를 받을 자리만 갖춘다.
    },
  });

  await writeHeartbeat(join(appDataDir, HEARTBEAT_FILE_NAME), {
    ts: new Date().toISOString(),
    pid: process.pid,
    version: compatibilityRaw.extensionVersion,
  });

  return { setTrayState, trustLedger, journal, appDataDir };
}

/** `trust.grant` 표면 진입점 — 대시보드/온보딩 UI(이 슬라이스 범위 밖)가 나중에
 * 이 함수를 호출한다. 실제 다이얼로그는 Electron `dialog`를 쓴다. */
export async function grantTargetFolderTrust(trustLedger: TrustLedger, targetFolderPath: string): Promise<boolean> {
  const outcome = await requestTargetFolderTrust(targetFolderPath, { ledger: trustLedger, showConsentDialog: showTrustConsentDialogElectron });
  return outcome.userApproved;
}

/** N-9 "앱 자체 예외 — 최상위 핸들러가 잡아 로그 + 트레이 error 배지. 다른 창을
 * 가로채는 전역 모달을 띄우지 않는다." 자동 재시작(§2.7 크래시 루프 방지)은 이
 * 슬라이스 범위 밖이다 — `heartbeat/crashLoopGuard.ts`가 그 다음 단계가 쓸 순수
 * 판정 로직을 이미 제공한다(반환문에 명시). */
export function installTopLevelExceptionHandler(setTrayState: (state: TrayState) => void): void {
  process.on('uncaughtException', (error) => {
    // eslint-disable-next-line no-console
    console.error('[malgn] uncaughtException', error);
    setTrayState('error');
  });
}
