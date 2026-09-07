// agent provider detect() — architecture.md §3.2 표 detect 행: "`claude plugin list
// --json` / `claude plugin marketplace list --json` → 설치 여부·버전·scope·gitCommitSha /
// 마켓플레이스 등록 여부". 부작용 0(읽기 전용 exec + 읽기 전용 fs), 절대 throw하지
// 않는다(PR-8) — 모든 실패 경로가 `Observed`로 접힌다.
//
// [대상 선정] 어떤 플러그인/마켓플레이스를 볼지는 **정책이 아니라 코드 상수**에서
// 가져온다(`compat/compatibility.json`의 `allowedPlugins[0]`/`allowedMarketplaces[0]`) —
// `plan()`이 받는 `DesiredSlice`(정책 파생)는 이 시점에 아직 없다(activationSequence.ts:
// detect 전체 완료 후에야 desired를 만든다). 이것은 install provider의 `detect()`가
// `allowedInstallTargets`(코드 상수)로 대상을 고르는 것과 같은 패턴이다 — 정책은 좁힐
// 뿐 대상 자체를 추가하지 못한다(PR-9)는 성질이 detect 단계부터 성립한다.

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { DetectContext, Observed } from '../../providers/types.js';
import type { ExecFileFn } from '../../platform/exec.js';
import { runExec } from '../../platform/exec.js';
import {
  AGENT_EXEC_TIMEOUT_MS,
  buildMarketplaceListArgv,
  buildPluginListArgv,
  marketplaceNameFromPluginId,
  parseMarketplaceListJson,
  parsePluginListJson,
  pluginNameOnly,
} from './cli.js';
import { readKnownMarketplaceEntry, readMarketplaceEntryForPlugin, type ReadTextFile } from './marketplaceReader.js';
import { isKnownBlockedVersion } from './knownVersionBlock.js';
import {
  MV_AGENT_CLAUDE_UNAVAILABLE,
  MV_AGENT_DISABLED,
  MV_AGENT_KNOWN_VERSION_BLOCKED,
  MV_AGENT_MARKETPLACE_MISSING,
  MV_AGENT_MARKETPLACE_REPO_MISMATCH,
  MV_AGENT_NOT_INSTALLED,
  MV_AGENT_OK,
} from './errors.js';

export interface AgentDetectDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
}

export interface AgentObservedDetail {
  readonly pluginId: string;
  readonly marketplaceRepo: string;
  readonly claudeAvailable: boolean;
  readonly installed: boolean;
  readonly scope: string | null;
  readonly version: string | null;
  readonly enabled: boolean | null;
  readonly marketplaceRegistered: boolean;
  readonly marketplaceName: string | null;
  readonly marketplaceRepoMatches: boolean | null;
  /** `marketplace.json`의 원소 원문(있으면) — plan()의 entryJson 바인딩(§3.2.2 ③)이
   * 이 값을 그대로 재사용한다(같은 재실행에서 다시 읽지 않는다). */
  readonly entry: Record<string, unknown> | null;
}

function nowIso(): string {
  return new Date().toISOString();
}

function detailOf(overrides: Partial<AgentObservedDetail> & Pick<AgentObservedDetail, 'pluginId' | 'marketplaceRepo'>): AgentObservedDetail {
  return {
    claudeAvailable: false,
    installed: false,
    scope: null,
    version: null,
    enabled: null,
    marketplaceRegistered: false,
    marketplaceName: null,
    marketplaceRepoMatches: null,
    entry: null,
    ...overrides,
  };
}

export async function detectAgent(deps: AgentDetectDeps, ctx: DetectContext): Promise<Observed> {
  const constants = loadCodeConstants();
  const pluginId = constants.allowedPlugins[0];
  const marketplaceRepo = constants.allowedMarketplaces[0];

  if (!pluginId || !marketplaceRepo) {
    // 코드 상수 자체가 비어 있으면(구성 오류) 이 provider는 아무것도 판단할 수 없다 —
    // 던지지 않고 unknown으로 접는다(PR-8).
    return {
      providerId: 'agent',
      status: 'unknown',
      code: 'MV_AGENT_NO_TARGET_CONFIGURED',
      message: 'compat/compatibility.json에 allowedPlugins/allowedMarketplaces가 비어 있습니다',
      observedAt: nowIso(),
    };
  }

  const listArgv = buildPluginListArgv();
  const listOutcome = await runExec({
    execFileFn: deps.execFileFn,
    file: listArgv.file,
    args: listArgv.args,
    env: deps.env,
    timeoutMs: AGENT_EXEC_TIMEOUT_MS,
    signal: ctx.signal,
  });

  if (listOutcome.kind === 'spawn-error') {
    return {
      providerId: 'agent',
      status: 'blocked',
      code: MV_AGENT_CLAUDE_UNAVAILABLE,
      message: 'claude CLI를 실행할 수 없습니다 — PATH에 claude가 있는지 확인하세요',
      detail: detailOf({ pluginId, marketplaceRepo }),
      observedAt: nowIso(),
    };
  }
  if (listOutcome.kind !== 'ok') {
    return {
      providerId: 'agent',
      status: 'unknown',
      code: 'MV_AGENT_DETECT_LIST_FAILED',
      message: `claude plugin list --json 실패: ${listOutcome.kind}`,
      detail: detailOf({ pluginId, marketplaceRepo, claudeAvailable: true }),
      observedAt: nowIso(),
    };
  }

  const entries = parsePluginListJson(listOutcome.stdout);
  const desiredScope = constants.allowedInstallScopes[0] ?? 'user';
  const match = entries.find((e) => e.id === pluginId && e.scope === desiredScope) ?? null;

  const marketplaceListArgv = buildMarketplaceListArgv();
  const marketplaceOutcome = await runExec({
    execFileFn: deps.execFileFn,
    file: marketplaceListArgv.file,
    args: marketplaceListArgv.args,
    env: deps.env,
    timeoutMs: AGENT_EXEC_TIMEOUT_MS,
    signal: ctx.signal,
  });
  const marketplaceEntries = marketplaceOutcome.kind === 'ok' ? parseMarketplaceListJson(marketplaceOutcome.stdout) : [];
  const marketplaceName = marketplaceNameFromPluginId(pluginId);
  const marketplaceListing = marketplaceName ? marketplaceEntries.find((m) => m.name === marketplaceName) ?? null : null;
  const marketplaceRegistered = marketplaceListing !== null;
  const marketplaceRepoMatches = marketplaceListing ? marketplaceListing.repo === marketplaceRepo : null;

  let entry: Record<string, unknown> | null = null;
  if (marketplaceRegistered && marketplaceListing) {
    const known = await readKnownMarketplaceEntry(deps.claudeHomeDir, marketplaceListing.name, deps.readTextFile);
    if (known) {
      entry = await readMarketplaceEntryForPlugin(known.installLocation, pluginNameOnly(pluginId), deps.readTextFile);
    }
  }

  const detail = detailOf({
    pluginId,
    marketplaceRepo,
    claudeAvailable: true,
    installed: match !== null,
    scope: match?.scope ?? null,
    version: match?.version ?? null,
    enabled: match?.enabled ?? null,
    marketplaceRegistered,
    marketplaceName: marketplaceListing?.name ?? null,
    marketplaceRepoMatches,
    entry,
  });

  if (marketplaceRegistered && marketplaceRepoMatches === false) {
    return {
      providerId: 'agent',
      status: 'drift',
      code: MV_AGENT_MARKETPLACE_REPO_MISMATCH,
      message: `같은 이름(${marketplaceListing?.name})으로 다른 저장소가 등록되어 있습니다 — 자동 정정하지 않습니다`,
      detail,
      observedAt: nowIso(),
    };
  }

  if (!marketplaceRegistered) {
    return {
      providerId: 'agent',
      status: 'drift',
      code: MV_AGENT_MARKETPLACE_MISSING,
      message: '마켓플레이스가 아직 등록되지 않았습니다',
      detail,
      observedAt: nowIso(),
    };
  }

  if (!match) {
    return {
      providerId: 'agent',
      status: 'drift',
      code: MV_AGENT_NOT_INSTALLED,
      message: `${pluginId}가 설치되어 있지 않습니다(scope=${desiredScope})`,
      detail,
      observedAt: nowIso(),
    };
  }

  if (match.enabled === false) {
    return {
      providerId: 'agent',
      status: 'drift',
      code: MV_AGENT_DISABLED,
      message: `${pluginId}가 설치되어 있으나 비활성화되어 있습니다`,
      detail,
      observedAt: nowIso(),
    };
  }

  if (match.version && isKnownBlockedVersion(match.version, constants.known)) {
    // §3.5 "이 버전은 known 결함으로 apply가 차단됩니다" — plan()도 방어심층으로 같은
    // 판정을 다시 하지만(knownVersionBlock.ts 공유), 그 사실을 UI가 볼 수 있으려면
    // 여기서 이미 status/code로 드러나야 한다(Plan에는 code 필드가 없다).
    return {
      providerId: 'agent',
      status: 'blocked',
      code: MV_AGENT_KNOWN_VERSION_BLOCKED,
      message: `${match.version}은 known 결함으로 apply가 차단됩니다 — 업데이트 후 다시 시도하세요`,
      detail,
      observedAt: nowIso(),
    };
  }

  return {
    providerId: 'agent',
    status: 'ok',
    code: MV_AGENT_OK,
    message: `${pluginId} ${match.version} 설치·활성 상태`,
    detail,
    observedAt: nowIso(),
  };
}
