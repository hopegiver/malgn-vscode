// mcp provider verify() — apply(plugin enable) 직후 재확인. `claude plugin list --json`의
// `enabled` 필드로 재확인한다(agent provider verify.ts와 같은 패턴).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { DetectContext, VerifyResult } from '../../providers/types.js';
import { runExec, type ExecFileFn } from '../../platform/exec.js';
import { AGENT_EXEC_TIMEOUT_MS, buildPluginListArgv, parsePluginListJson } from '../agent/cli.js';
import { MV_MCP_OK, MV_MCP_VERIFY_FAILED } from './errors.js';

export interface McpVerifyDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
}

function nowIso(): string {
  return new Date().toISOString();
}

export async function verifyMcp(deps: McpVerifyDeps, ctx: DetectContext): Promise<VerifyResult> {
  const constants = loadCodeConstants();
  const pluginId = constants.allowedPlugins[0];
  const scope = constants.allowedInstallScopes[0] ?? 'user';
  if (!pluginId) {
    return { providerId: 'mcp', status: 'unknown', code: 'MV_MCP_NO_TARGET_CONFIGURED', message: 'allowedPlugins가 비어 있습니다', verifiedAt: nowIso() };
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
    return { providerId: 'mcp', status: 'unknown', code: MV_MCP_VERIFY_FAILED, message: `재확인 실패: ${outcome.kind}`, verifiedAt: nowIso() };
  }
  const entries = parsePluginListJson(outcome.stdout);
  const match = entries.find((e) => e.id === pluginId && e.scope === scope);
  if (!match || match.enabled !== true) {
    return { providerId: 'mcp', status: 'blocked', code: MV_MCP_VERIFY_FAILED, message: '활성 상태로 재확인되지 않았습니다', verifiedAt: nowIso() };
  }
  return { providerId: 'mcp', status: 'ok', code: MV_MCP_OK, message: '활성 상태 재확인 완료', verifiedAt: nowIso() };
}
