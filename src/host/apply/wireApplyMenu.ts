// 동의 화면 1개 + apply 오케스트레이션 실제 배선 — architecture.md §2.2 "apply는 3가지
// 경로로만 시작된다"의 그중 하나(트레이 메뉴에서 사람이 명시적으로 개시)를 여기서
// 채운다. **이 파일은 `host/activation/**` 밖이다** — `hostActivationApplyInvariant.test.ts`가
// 강제하는 "활성화 시퀀스는 apply를 부르지 않는다"와 이 파일이 apply를 부르는 것은
// 모순이 아니다(그 불변량은 정확히 "활성화 시퀀스 자신"에만 건다 — 사람이 명시적으로
// 개시하는 이 경로는 §2.2가 애초에 별도로 허용한 통로다). `host/app.ts`도 여전히
// `Provider.apply`를 import하지 않는다(이 파일이 대신 부른다) — 그 파일의 "apply()를
// 호출하지 않는다" 불변량은 그대로 유지된다.
//
// [정직 표기 — 이번 슬라이스가 채우지 못한 신호] `hrs4ReconsentRequired`는 항상
// `false`다 — 고위험 표면 diff 계산기(§7.3.1 HRS 4)가 이 코드베이스에 아직 없다(W7
// 범위 밖, 별도 슬라이스 필요). 이 필드가 실제로 계산되기 시작하면 이 상수를 그
// 계산 결과로 바꿔야 한다.

import { Menu, type Tray } from 'electron';
import { stat } from 'node:fs/promises';
import type { ActivationStatusReport } from '../activation/activationSequence.js';
import type { AppRuntimeHandles } from '../app.js';
import { runApplyWithConsent } from '../../core/reconciler/applyOrchestrator.js';
import type { StopSignals } from '../../core/reconciler/stopGate.js';
import { isPolicyCheckoutStale, isBelowCompatMinimum } from '../../core/reconciler/stopGate.js';
import { ConsentGateError } from '../../core/consent/gate.js';
import { showApplyConsentDialogElectron } from '../electron/consentDialogAdapter.js';
import { resolveSessionAttended } from '../electron/idleStateAdapter.js';
import { detectClaudeCodeVersion } from '../claudeCli/detectClaudeCodeVersion.js';
import { electronExecFile } from '../electron/execFileAdapter.js';
import compatibilityRaw from '../../../compat/compatibility.json';
import type { Provider } from '../../providers/types.js';
import { resolveWorkstationTrustKey } from './workstationTrustKey.js';

export { resolveWorkstationTrustKey };

async function resolvePolicyCheckoutStale(report: ActivationStatusReport): Promise<boolean> {
  if (report.policySource.source !== 'checkout' || report.policySource.sourcePath === null) return false;
  try {
    const stats = await stat(report.policySource.sourcePath);
    return isPolicyCheckoutStale(stats.mtime);
  } catch {
    return false; // 조회 실패는 이 판정의 책임 밖 — false로 접어 다른 정지 경로에 맡긴다
  }
}

async function resolveCompatGateBelowMinimum(claudeCodeRange: string, homeDir: string): Promise<boolean> {
  const version = await detectClaudeCodeVersion({ execFileFn: electronExecFile, homeDir, pathEnv: process.env.PATH ?? '' });
  if (!version) return false; // 미확인은 이 판정의 책임 밖(§3.5.3 원문과 동일 관용구)
  return isBelowCompatMinimum(version, claudeCodeRange);
}

export interface WireApplyMenuDeps {
  readonly tray: Tray;
  readonly handles: AppRuntimeHandles;
  readonly homeDir: string;
}

/**
 * 최신 `ActivationStatusReport`를 받을 때마다 호출한다 — 변경이 있는 provider마다
 * "지금 적용" 메뉴 항목을 만들고, 클릭 시 `runApplyWithConsent`가 실제 동의→적용→
 * 저널→재확인을 수행한다.
 */
export function wireApplyMenu(deps: WireApplyMenuDeps, report: ActivationStatusReport): void {
  const pendingPlans = report.plans.filter((plan) => plan.changes.length > 0);

  if (pendingPlans.length === 0) {
    deps.tray.setContextMenu(Menu.buildFromTemplate([{ label: 'Malgn — 변경 없음', enabled: false }]));
    return;
  }

  const items = pendingPlans.map((plan) => ({
    label: `지금 적용 — ${plan.providerId}`,
    click: () => {
      void applyOneProvider(deps, report, plan.providerId);
    },
  }));
  deps.tray.setContextMenu(Menu.buildFromTemplate(items));
}

async function applyOneProvider(deps: WireApplyMenuDeps, report: ActivationStatusReport, providerId: string): Promise<void> {
  const provider = deps.handles.providers.find((p: Provider) => p.id === providerId);
  const plan = report.plans.find((p) => p.providerId === providerId);
  if (!provider || !plan) return;

  const killSwitch =
    report.policySource.result.status === 'ok'
      ? report.policySource.result.policy.killSwitch
      : { minAppVersion: null, maxAppVersion: null, disableProviders: [], message: null, upgradeHint: null };
  const claudeCodeRange = report.policySource.result.status === 'ok' ? report.policySource.result.policy.compat.claudeCode : compatibilityRaw.requires.claudeCode;

  const [policyCheckoutStale, compatGateBelowMinimum, targetFolderTrustedState] = await Promise.all([
    resolvePolicyCheckoutStale(report),
    resolveCompatGateBelowMinimum(claudeCodeRange, deps.homeDir),
    deps.handles.trustLedger.get(resolveWorkstationTrustKey(deps.homeDir)),
  ]);

  const stopSignals: StopSignals = {
    killSwitch,
    currentExtensionVersion: compatibilityRaw.extensionVersion,
    compatGateBelowMinimum,
    targetFolderTrusted: targetFolderTrustedState === 'trusted',
    policyCheckoutStale,
    hrs4ReconsentRequired: false, // 정직 표기 — 위 모듈 주석 참고
    sessionAttended: resolveSessionAttended(),
  };

  await runApplyWithConsent({
    provider,
    plan,
    extensionVersion: compatibilityRaw.extensionVersion,
    stopSignals,
    applyContext: { targetFolderTrusted: stopSignals.targetFolderTrusted },
    requestApproval: showApplyConsentDialogElectron,
    recordChange: (entry) => deps.handles.journal.appendChange(entry),
    recordConsentFailure: (error: ConsentGateError) =>
      deps.handles.journal.appendConsentFailure({
        ts: new Date().toISOString(),
        code: error.code,
        severity: 'high',
        message: error.message,
        providerId: plan.providerId,
      }),
  });
}
