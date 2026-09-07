// agent provider verify() — architecture.md §3.2 표 verify 행: "`claude plugin list
// --json` 재확인 + `enabledPlugins` 확인. 실패 시 저널 근거로 안내." 이 슬라이스는
// `enabledPlugins`(settings.json) 직접 파일 대조까지는 하지 않고 `claude plugin list
// --json`의 `enabled` 필드(같은 사실의 CLI 노출)로 재확인한다 — 별도 설정 파일 파서를
// 새로 추가하지 않기 위한 선택(반환문에 명시할 설계 범위 축소).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { DetectContext, VerifyResult } from '../../providers/types.js';
import { runExec, type ExecFileFn } from '../../platform/exec.js';
import { AGENT_EXEC_TIMEOUT_MS, buildPluginListArgv, parsePluginListJson } from './cli.js';
import { MV_AGENT_OK, MV_AGENT_VERIFY_FAILED } from './errors.js';

export interface AgentVerifyDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
}

function nowIso(): string {
  return new Date().toISOString();
}

export async function verifyAgent(deps: AgentVerifyDeps, ctx: DetectContext): Promise<VerifyResult> {
  const constants = loadCodeConstants();
  const pluginId = constants.allowedPlugins[0];
  const scope = constants.allowedInstallScopes[0] ?? 'user';
  if (!pluginId) {
    return { providerId: 'agent', status: 'unknown', code: 'MV_AGENT_NO_TARGET_CONFIGURED', message: 'allowedPlugins가 비어 있습니다', verifiedAt: nowIso() };
  }

  const listArgv = buildPluginListArgv();
  const outcome = await runExec({
    execFileFn: deps.execFileFn,
    file: listArgv.file,
    args: listArgv.args,
    env: deps.env,
    timeoutMs: AGENT_EXEC_TIMEOUT_MS,
    signal: ctx.signal,
  });
  if (outcome.kind !== 'ok') {
    return { providerId: 'agent', status: 'unknown', code: MV_AGENT_VERIFY_FAILED, message: `재확인 실패: ${outcome.kind}`, verifiedAt: nowIso() };
  }

  const entries = parsePluginListJson(outcome.stdout);
  const match = entries.find((e) => e.id === pluginId && e.scope === scope);
  if (!match || match.enabled !== true) {
    return {
      providerId: 'agent',
      status: 'blocked',
      code: MV_AGENT_VERIFY_FAILED,
      message: `${pluginId}가 설치·활성 상태로 재확인되지 않았습니다`,
      verifiedAt: nowIso(),
    };
  }

  return { providerId: 'agent', status: 'ok', code: MV_AGENT_OK, message: `${pluginId} ${match.version} 재확인 완료`, verifiedAt: nowIso() };
}
