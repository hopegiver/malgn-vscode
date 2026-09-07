// mcp provider apply() — architecture.md §5.2 "②는 `plugin enable`로 고쳐야 한다" ·
// §5.3 "설치됐으나 enabledPlugins에 없음/false → `claude plugin enable ...` 제안".
// 이 파일에 신규 MCP 서버 등록 서브커맨드나 프로젝트 MCP 설정 파일 쓰기가 없다는 것
// 자체가 §5.2 판정의 구조적 증거다(`mcpProviderNoInjection.test.ts`가 grep으로 고정).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { runExec, type ExecFileFn } from '../../platform/exec.js';
import type { ApplyContext, ApplyResult, Plan } from '../../providers/types.js';
import { AGENT_EXEC_TIMEOUT_MS } from '../agent/cli.js';
import { buildPluginEnableArgv } from './cli.js';
import { MV_MCP_ENABLE_FAILED, MV_MCP_OK } from './errors.js';

export interface McpApplyDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
}

export async function applyMcp(deps: McpApplyDeps, plan: Plan, ctx: ApplyContext): Promise<ApplyResult> {
  if (plan.changes.length === 0) {
    return { providerId: 'mcp', status: 'ok', code: MV_MCP_OK, message: '적용할 변경이 없습니다', appliedChangeIds: [] };
  }
  const pluginId = loadCodeConstants().allowedPlugins[0];
  if (!pluginId) {
    return { providerId: 'mcp', status: 'blocked', code: MV_MCP_ENABLE_FAILED, message: 'allowedPlugins가 비어 있습니다', appliedChangeIds: [] };
  }

  const enableArgv = buildPluginEnableArgv(pluginId);
  const outcome = await runExec({
    execFileFn: deps.execFileFn,
    file: enableArgv.file,
    args: enableArgv.args,
    env: deps.env,
    timeoutMs: AGENT_EXEC_TIMEOUT_MS,
    signal: ctx.signal,
  });
  if (outcome.kind !== 'ok') {
    return { providerId: 'mcp', status: 'blocked', code: MV_MCP_ENABLE_FAILED, message: `plugin enable 실패: ${outcome.kind}`, appliedChangeIds: [] };
  }

  return {
    providerId: 'mcp',
    status: 'ok',
    code: MV_MCP_OK,
    message: `${pluginId} 활성화 완료`,
    appliedChangeIds: plan.changes.map((c) => c.id),
  };
}
