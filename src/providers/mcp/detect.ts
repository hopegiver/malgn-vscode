// mcp provider detect() — architecture.md §5.3 detect/복구 표 정본 구현. `agent`
// provider와 마찬가지로 `claude plugin list --json`을 스스로 다시 호출한다 — provider는
// 서로 독립적으로 detect하고(`dependsOn`은 실행 **순서**만 정한다, `registry.ts`
// `topologicalOrder`) sibling의 Observed를 데이터로 주고받는 통로가 Provider 인터페이스에
// 없기 때문이다(설계 갭이 아니라 인터페이스 자체의 성질 — 반환문에 명시).
//
// [주입하지 않는다] 이 파일은 신규 MCP 서버 등록 서브커맨드나 프로젝트 MCP 설정 파일
// 쓰기를 호출하지 않는다는 것 자체가 §5.2 "주입은 셋 중 어느 것도 개선하지 못한다"의
// 구조적 증거다(`mcpProviderNoInjection.test.ts`가 grep으로 고정한다).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { authorityAllowed, extractHttpsAuthority } from '../../core/policy/sinkGuards.js';
import type { DetectContext, Observed } from '../../providers/types.js';
import { runExec, type ExecFileFn } from '../../platform/exec.js';
import { AGENT_EXEC_TIMEOUT_MS, buildPluginListArgv, parsePluginListJson, pluginNameOnly } from '../agent/cli.js';
import { buildMcpGetArgv, MCP_EXEC_TIMEOUT_MS, parseMcpGetOutput, pluginMcpServerName } from './cli.js';
import {
  MV_MCP_AGENT_NOT_INSTALLED,
  MV_MCP_HEALTH_CHECK_FAILED,
  MV_MCP_HUB_URL_MISMATCH,
  MV_MCP_NOT_REGISTERED,
  MV_MCP_OAUTH_REQUIRED,
  MV_MCP_OK,
  MV_MCP_PLUGIN_DISABLED,
} from './errors.js';

/** §5.1 O-3 — malgn-agent의 `.claude-plugin/plugin.json`이 선언하는 MCP 서버 키. 정책이
 * 바꿀 수 없는 값이라(REQ-5는 "malgnai-hub 하나"로 범위가 고정돼 있다, §0.1.1 A-1)
 * 코드 상수로 둔다. */
const MCP_SERVER_KEY = 'malgnai-hub';

export interface McpDetectDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
}

export interface McpObservedDetail {
  readonly pluginInstalled: boolean;
  readonly pluginEnabled: boolean | null;
  readonly mcpFound: boolean;
  readonly connected: boolean | null;
  readonly statusText: string | null;
  readonly url: string | null;
  readonly urlAuthorityAllowed: boolean | null;
}

function nowIso(): string {
  return new Date().toISOString();
}

export async function detectMcp(deps: McpDetectDeps, ctx: DetectContext): Promise<Observed> {
  const constants = loadCodeConstants();
  const pluginId = constants.allowedPlugins[0];
  const desiredScope = constants.allowedInstallScopes[0] ?? 'user';
  if (!pluginId) {
    return { providerId: 'mcp', status: 'unknown', code: 'MV_MCP_NO_TARGET_CONFIGURED', message: 'allowedPlugins가 비어 있습니다', observedAt: nowIso() };
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
  if (listOutcome.kind !== 'ok') {
    return { providerId: 'mcp', status: 'unknown', code: 'MV_MCP_DETECT_LIST_FAILED', message: `claude plugin list --json 실패: ${listOutcome.kind}`, observedAt: nowIso() };
  }
  const entries = parsePluginListJson(listOutcome.stdout);
  const match = entries.find((e) => e.id === pluginId && e.scope === desiredScope) ?? null;

  const baseDetail: McpObservedDetail = {
    pluginInstalled: match !== null,
    pluginEnabled: match?.enabled ?? null,
    mcpFound: false,
    connected: null,
    statusText: null,
    url: null,
    urlAuthorityAllowed: null,
  };

  if (!match) {
    return {
      providerId: 'mcp',
      status: 'blocked',
      code: MV_MCP_AGENT_NOT_INSTALLED,
      message: '플러그인이 설치되어 있지 않습니다 — agent provider(REQ-3)로 위임합니다',
      detail: baseDetail,
      observedAt: nowIso(),
    };
  }

  if (match.enabled === false) {
    return {
      providerId: 'mcp',
      status: 'drift',
      code: MV_MCP_PLUGIN_DISABLED,
      message: '플러그인이 비활성 상태입니다 — plugin enable로 복구 가능합니다',
      detail: baseDetail,
      observedAt: nowIso(),
    };
  }

  const serverName = pluginMcpServerName(pluginNameOnly(pluginId), MCP_SERVER_KEY);
  const getArgv = buildMcpGetArgv(serverName);
  const getOutcome = await runExec({
    execFileFn: deps.execFileFn,
    file: getArgv.file,
    args: getArgv.args,
    env: deps.env,
    timeoutMs: MCP_EXEC_TIMEOUT_MS,
    signal: ctx.signal,
  });
  const stdout = getOutcome.kind === 'ok' ? getOutcome.stdout : '';
  const stderr = getOutcome.kind === 'nonzero-exit' ? getOutcome.stderr : '';
  const parsed = parseMcpGetOutput(stdout, stderr);

  if (!parsed.found) {
    return {
      providerId: 'mcp',
      status: 'drift',
      code: MV_MCP_NOT_REGISTERED,
      message: `${serverName}이 아직 등록되어 있지 않습니다(전파 지연일 수 있습니다) — 자동 등록하지 않습니다`,
      detail: { ...baseDetail, mcpFound: false },
      observedAt: nowIso(),
    };
  }

  const urlAuthority = parsed.url ? extractHttpsAuthority(parsed.url) : null;
  const urlAuthorityAllowed = urlAuthority ? authorityAllowed(urlAuthority, constants.allowedAuthorities.mcp) : null;

  const detail: McpObservedDetail = {
    ...baseDetail,
    mcpFound: true,
    connected: parsed.connected,
    statusText: parsed.statusText,
    url: parsed.url,
    urlAuthorityAllowed,
  };

  if (urlAuthorityAllowed === false) {
    return {
      providerId: 'mcp',
      status: 'drift',
      code: MV_MCP_HUB_URL_MISMATCH,
      message: `등록된 URL(${parsed.url})이 허용 목록 밖입니다 — 자동 정정·삭제하지 않습니다`,
      detail,
      observedAt: nowIso(),
    };
  }

  if (parsed.connected === false) {
    const isAuthIssue = parsed.statusText ? /auth/i.test(parsed.statusText) : false;
    return {
      providerId: 'mcp',
      status: 'drift',
      code: isAuthIssue ? MV_MCP_OAUTH_REQUIRED : MV_MCP_HEALTH_CHECK_FAILED,
      message: isAuthIssue
        ? `${serverName} OAuth 인증이 필요합니다 — 브라우저에서 \`claude mcp login ${serverName}\`을 완료하세요`
        : `${serverName} 상태 이상: ${parsed.statusText ?? '알 수 없음'}`,
      detail,
      observedAt: nowIso(),
    };
  }

  return {
    providerId: 'mcp',
    status: 'ok',
    code: MV_MCP_OK,
    message: `${serverName} 정상 연결됨`,
    detail,
    observedAt: nowIso(),
  };
}
