// agent provider apply() — architecture.md §3.2 표 apply 행 + §3.2.2(대체 통제) 전체 +
// §3.2.2 ④(TOCTOU) 실행 지점. `ConsentToken`이 타입 수준에서 요구되지만(providers/types.ts)
// 이 함수 자신은 `gate.assertValid`를 호출하지 않는다 — 그 재검증은 호출자(오케스트레이션,
// `core/reconciler/applyOrchestrator.ts`)가 apply 직전 **단일 호출 지점**에서 한다
// (`consent/gate.ts` 원문 주석 "engine이 단일 호출 지점이 되도록 설계").
//
// 순서(§3.2.2 1~4 그대로):
//   1. `marketplace add`(이미 있으면 실패해도 무시) → `marketplace update` → installLocation 확인
//   2. `marketplace.json`을 읽기 전용 파싱 → 명령 선언 fail-closed 판정(§3.2.2 ②)
//   3. 판정 통과 시에만 `-y`를 붙여 `install`/`update` 실행
//   4. TOCTOU: 읽기 전 체크아웃 HEAD와 install 후 HEAD 대조(marketplaceSha.ts)
//   5. `enable`(활성화 — MCP 복구 경로와 동일 명령, §5.1)

import type { ApplyContext, ApplyResult, Plan } from '../../providers/types.js';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { runExec, type ExecFileFn } from '../../platform/exec.js';
import {
  AGENT_EXEC_TIMEOUT_MS,
  buildMarketplaceAddArgv,
  buildMarketplaceUpdateArgv,
  buildPluginEnableArgv,
  buildPluginInstallArgv,
  buildPluginUpdateArgv,
  marketplaceNameFromPluginId,
  pluginNameOnly,
} from './cli.js';
import { readKnownMarketplaceEntry, readMarketplaceEntryForPlugin, type ReadTextFile } from './marketplaceReader.js';
import { evaluateMarketplaceEntryDeclaration } from './marketplaceDeclaration.js';
import { readMarketplaceCheckoutHead, shaMismatch } from './marketplaceSha.js';
import {
  MV_AGENT_APPLY_FAILED,
  MV_AGENT_MARKETPLACE_DECLARES_COMMAND,
  MV_AGENT_OK,
  MV_AGENT_SHA_MISMATCH,
} from './errors.js';

export interface AgentApplyDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
}

function failure(code: string, message: string, detail?: unknown): ApplyResult {
  return { providerId: 'agent', status: 'blocked', code, message, detail, appliedChangeIds: [] };
}

export async function applyAgent(deps: AgentApplyDeps, plan: Plan, ctx: ApplyContext): Promise<ApplyResult> {
  if (plan.changes.length === 0) {
    return { providerId: 'agent', status: 'ok', code: MV_AGENT_OK, message: '적용할 변경이 없습니다', appliedChangeIds: [] };
  }

  const constants = loadCodeConstants();
  const pluginId = constants.allowedPlugins[0];
  const marketplaceRepo = constants.allowedMarketplaces[0];
  const scope = constants.allowedInstallScopes[0];
  if (!pluginId || !marketplaceRepo || !scope) {
    return failure(MV_AGENT_APPLY_FAILED, 'compat/compatibility.json에 필요한 코드 상수가 없습니다');
  }
  const marketplaceName = marketplaceNameFromPluginId(pluginId);
  if (!marketplaceName) {
    return failure(MV_AGENT_APPLY_FAILED, `pluginId 형식이 올바르지 않습니다: ${pluginId}`);
  }

  const changeIds = plan.changes.map((c) => c.id);
  const exec = (file: string, args: readonly string[]) =>
    runExec({ execFileFn: deps.execFileFn, file, args, env: deps.env, timeoutMs: AGENT_EXEC_TIMEOUT_MS, signal: ctx.signal });

  // 1. marketplace add(멱등 아님 — 이미 있으면 매니저가 알아서 실패시킨다, 그 실패는
  //    무시하고 update로 넘어간다) → marketplace update(체크아웃 최신화)
  const addArgv = buildMarketplaceAddArgv(marketplaceRepo);
  await exec(addArgv.file, addArgv.args);

  const updateArgv = buildMarketplaceUpdateArgv(marketplaceName);
  const updateOutcome = await exec(updateArgv.file, updateArgv.args);
  if (updateOutcome.kind !== 'ok') {
    return failure(MV_AGENT_APPLY_FAILED, `marketplace update 실패: ${updateOutcome.kind}`, updateOutcome);
  }

  // installLocation 확인
  const known = await readKnownMarketplaceEntry(deps.claudeHomeDir, marketplaceName, deps.readTextFile);
  if (!known) {
    return failure(MV_AGENT_APPLY_FAILED, `known_marketplaces.json에서 ${marketplaceName}의 체크아웃 위치를 찾을 수 없습니다`);
  }

  const beforeSha = await readMarketplaceCheckoutHead(known.installLocation, deps.execFileFn, deps.env);

  // 2. marketplace.json 읽기 전용 파싱 → 명령 선언 fail-closed 판정
  const pluginName = pluginNameOnly(pluginId);
  const entry = await readMarketplaceEntryForPlugin(known.installLocation, pluginName, deps.readTextFile);
  if (!entry) {
    return failure(MV_AGENT_APPLY_FAILED, `${known.installLocation}에서 ${pluginName} 항목을 찾을 수 없습니다`);
  }
  const declaration = evaluateMarketplaceEntryDeclaration(entry);
  if (!declaration.allowed) {
    return failure(
      MV_AGENT_MARKETPLACE_DECLARES_COMMAND,
      '마켓플레이스 항목이 명령 실행형 선언을 담고 있어 자동 적용을 중단합니다 — 터미널에서 직접 `claude plugin install ...`을 실행하세요',
      { entry, forbiddenKeyPaths: declaration.forbiddenKeyPaths, unknownSourceKind: declaration.unknownSourceKind }
    );
  }

  // 3. install/update — 판정을 통과했을 때만 -y(신뢰 이양점, §3.2)
  const isInstall = plan.changes.some((c) => c.kind === 'install');
  const cmdArgv = isInstall ? buildPluginInstallArgv(pluginId, scope, true) : buildPluginUpdateArgv(pluginId, scope, true);
  const cmdOutcome = await exec(cmdArgv.file, cmdArgv.args);
  if (cmdOutcome.kind !== 'ok') {
    return failure(MV_AGENT_APPLY_FAILED, `${isInstall ? 'install' : 'update'} 실패: ${cmdOutcome.kind}`, cmdOutcome);
  }

  // 4. TOCTOU 대조 — 불일치는 탐지지 예방이 아니다: 이미 실행된 install/update는 되돌릴
  //    수단이 없으므로(§9 "안 쓰는 도구가 남는다"와 같은 비대칭), 여기서는 다음 단계
  //    (enable)로 진행하지 않고 사람의 재검토를 요구한다.
  const afterSha = await readMarketplaceCheckoutHead(known.installLocation, deps.execFileFn, deps.env);
  if (shaMismatch(beforeSha, afterSha)) {
    return {
      providerId: 'agent',
      status: 'blocked',
      code: MV_AGENT_SHA_MISMATCH,
      message: `읽기 시점 체크아웃(${beforeSha.kind === 'sha' ? beforeSha.sha : '?'})과 설치 후 체크아웃(${
        afterSha.kind === 'sha' ? afterSha.sha : '?'
      })이 다릅니다 — 활성화를 보류합니다`,
      appliedChangeIds: changeIds,
    };
  }

  // 5. enable — §5.1의 MCP 복구 경로와 동일 명령. 실패해도 install/update 자체는 이미
  //    성공했으므로 apply 전체를 blocked로 되돌리지 않고 drift로 보고한다(다음 재수렴
  //    주기가 다시 시도한다).
  const enableArgv = buildPluginEnableArgv(pluginId);
  const enableOutcome = await exec(enableArgv.file, enableArgv.args);
  if (enableOutcome.kind !== 'ok') {
    return {
      providerId: 'agent',
      status: 'drift',
      code: MV_AGENT_APPLY_FAILED,
      message: `설치/업데이트는 성공했으나 활성화(plugin enable)에 실패했습니다: ${enableOutcome.kind}`,
      appliedChangeIds: changeIds,
    };
  }

  return {
    providerId: 'agent',
    status: 'ok',
    code: MV_AGENT_OK,
    message: `${pluginId} 적용 완료`,
    appliedChangeIds: changeIds,
  };
}
