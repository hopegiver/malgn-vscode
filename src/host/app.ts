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
import type { ActivationStatusReport, TrayState } from './activation/activationSequence.js';
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
import { electronExecFile } from './electron/execFileAdapter.js';
import { userInfo } from 'node:os';
import { createAgentProvider } from '../providers/agent/index.js';
import { createMcpProvider } from '../providers/mcp/index.js';
import { createOtelProvider } from '../providers/otel/index.js';
import { nodePathExecutable } from '../providers/otel/settingsFile.js';
import type { OtelDesiredSlice } from '../providers/otel/plan.js';
import type { DesiredSlice, Provider, ProviderId } from '../providers/types.js';
import type { LoadEffectivePolicyWithFallbackResult } from './policySource/loadWithFallback.js';

export interface AppRuntimeHandles {
  readonly setTrayState: (state: TrayState) => void;
  readonly trustLedger: TrustLedger;
  readonly journal: JournalStore;
  readonly appDataDir: string;
  /** W7 — apply 오케스트레이션(`host/apply/**`, 이 파일 밖)이 `runApplyWithConsent`를
   * 부를 때 필요한 provider 핸들. 이 파일 자신은 여전히 이 값으로 apply를 호출하지
   * 않는다(위 "[apply()를 호출하지 않는다]" 불변량 — 반환만 하고 쓰지 않는다). */
  readonly providers: readonly Provider[];
  readonly claudeHomeDir: string;
}

const readTextFile: ReadTextFile = (path) => readFile(path, 'utf8');

/**
 * W7 — `runActivationSequence`가 요구하는 `buildDesiredSlice`. agent는 정책의
 * `agent`/`compat` 조각을 그대로 옮기고(`providers/agent/plan.ts`의 `AgentDesiredSlice`),
 * mcp는 정책 필드가 없으므로(REQ-5, §0.1.1 A-1) providerId만 담는다. 정책 로드가
 * `rejected` 상태면 agent도 "차단"으로 접는다(PR-6 — 정책을 못 읽었는데 추측해 설치
 * 대상을 고르지 않는다).
 */
function buildDesiredSlice(providerId: ProviderId, policy: LoadEffectivePolicyWithFallbackResult): DesiredSlice {
  if (providerId === 'agent') {
    if (policy.result.status !== 'ok') {
      return { providerId: 'agent', agent: { blocked: true, reason: '정책을 로드하지 못했습니다' }, compat: { malgnAgent: '>=0.0.0', claudeCode: '>=0.0.0' } };
    }
    return { providerId: 'agent', agent: policy.result.policy.agent, compat: policy.result.policy.compat };
  }
  if (providerId === 'otel') {
    // W8 — 정책이 otel.env를 제안하면(이미 loader.ts의 validateOtel을 통과한 값) 그대로
    // 넘기고, 정책을 못 읽었거나 otel이 blocked·빈 env면 null을 넘긴다 — `planOtel`이
    // null일 때 사이트면 코드 상수(otelDefaultEnv)로 스스로 기본값을 만든다
    // (providers/otel/desiredEnv.ts). 정책은 여기서도 "제안"만 한다(PR-4).
    const policyEnv =
      policy.result.status === 'ok' && !policy.result.policy.otel.blocked && Object.keys(policy.result.policy.otel.env).length > 0
        ? policy.result.policy.otel.env
        : null;
    return { providerId: 'otel', policyEnv } satisfies OtelDesiredSlice;
  }
  return { providerId };
}

/**
 * §2.2 활성화 시퀀스를 실제 경로·실제 저장소로 조립해 1회 실행한다. 호출자
 * (`entry.prod.ts`/`entry.dev.ts`)가 `app.whenReady()` 이후 이 함수를 부른다. 재수렴
 * 주기(`RECONCILE_INTERVAL_MS`, §2.1 "기본 1h")마다 다시 호출하는 반복은 그 호출자의
 * 몫이다(이 함수 자신은 1회 실행만 책임진다 — 반복 스케줄링과 활성화 로직을 한 함수에
 * 묶지 않는다).
 */
export async function bootstrapAndRunOnce(
  setTrayState: (state: TrayState) => void,
  onReport?: (report: ActivationStatusReport) => void
): Promise<AppRuntimeHandles> {
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

  // W7 — agent(§3.2)·mcp(§5) provider 배선. `dependsOn:['agent']`(mcp)가
  // `runActivationSequence`의 `detectAll`/`plan` 순회 자체를 바꾸지는 않지만(engine은
  // 입력 배열 순서로 병렬 detect한다, §2.2 ⑤), 배열 순서를 dependsOn과 맞춰 둔다 —
  // `providers/registry.ts`의 `topologicalOrder`가 실행 순서를 강제해야 하는 지점(향후
  // apply 오케스트레이션의 순차 실행)에서 이 순서가 그대로 쓰인다.
  const agentProvider = createAgentProvider({ execFileFn: electronExecFile, env: process.env, claudeHomeDir, readTextFile });
  const mcpProvider = createMcpProvider({ execFileFn: electronExecFile, env: process.env });
  // W8 — otel(macOS만, §4.2). `dependsOn: []`이라 agent/mcp와 실행 순서 의존은 없지만
  // (§1.2 "mcp는 agent 이후"만 명시돼 있다), 배열에서는 이 셋 뒤에 둔다 — 나중에 실제
  // dependsOn이 생기면 이 순서를 그대로 쓸 수 있게 하기 위함이다.
  const otelProvider = createOtelProvider({
    claudeHomeDir,
    readTextFile,
    pathExecutable: nodePathExecutable,
    platform: process.platform,
    backupsDir: join(appDataDir, 'backups'),
    osUsername: userInfo().username,
  });
  const providers: readonly Provider[] = [agentProvider, mcpProvider, otelProvider];

  await runActivationSequence({
    setTrayState,
    loadPolicy: () =>
      loadEffectivePolicyWithFallback({
        claudeHomeDir,
        readTextFile,
        currentExtensionVersion: compatibilityRaw.extensionVersion,
      }),
    providers,
    buildDesiredSlice: (providerId, policy) => buildDesiredSlice(providerId, policy),
    detectContext: { targetFolderTrusted: false }, // I-B 대상 provider가 없어 이 슬라이스에서는 미사용
    reportStatus: (report) => {
      // 대시보드 UI(W11)는 아직 없다 — 그러나 W7 동의 화면(`host/apply/**`)이 "지금
      // 적용" 메뉴를 채우려면 최신 plan이 필요하므로, 그 배선이 구독할 수 있게
      // 콜백으로 넘긴다(이 함수 자신은 report의 내용을 해석하지 않는다).
      onReport?.(report);
    },
  });

  await writeHeartbeat(join(appDataDir, HEARTBEAT_FILE_NAME), {
    ts: new Date().toISOString(),
    pid: process.pid,
    version: compatibilityRaw.extensionVersion,
  });

  return { setTrayState, trustLedger, journal, appDataDir, providers, claudeHomeDir };
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
