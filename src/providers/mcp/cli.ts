// `claude mcp get`/`claude plugin enable` 얇은 래퍼 — architecture.md §5.2·§5.3.
// `claude mcp get`/`list`에는 `--json`이 없다(2.1.252 실측 `--help`에 옵션이 없다) —
// 이 파일의 파서는 실측 텍스트 출력 형태를 그대로 다룬다:
//
//   $ claude mcp get "plugin:malgn-agent:malgnai-hub"
//   plugin:malgn-agent:malgnai-hub:
//     Scope: Dynamic config (from command line)
//     Status: ✔ Connected
//     Type: http
//     URL: https://malgnai-hub.apiserver.kr/mcp
//     OAuth: client_id configured
//
//   $ claude mcp get "no-such-server" (exit 1, stderr)
//   No MCP server named "no-such-server". Configured servers: ...

export const MCP_EXEC_TIMEOUT_MS = 10_000;

/** §5.2 O-3 실측 — 플러그인이 등록한 MCP 서버는 `plugin:<pluginName>:<mcpServerName>`
 * 이름으로 노출된다(마켓플레이스 접미사 없는 플러그인 이름). */
export function pluginMcpServerName(pluginNameOnly: string, mcpServerName: string): string {
  return `plugin:${pluginNameOnly}:${mcpServerName}`;
}

export function buildMcpGetArgv(serverName: string): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['mcp', 'get', serverName] };
}

export function buildPluginEnableArgv(pluginId: string): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'enable', pluginId] };
}

export interface McpGetParsed {
  readonly found: boolean;
  readonly connected: boolean | null;
  readonly statusText: string | null;
  readonly url: string | null;
}

const STATUS_LINE_RE = /Status:\s*(.+)/;
const URL_LINE_RE = /URL:\s*(\S+)/;
const NOT_FOUND_RE = /No MCP server named/;

/** exit 0 + stdout(찾음) 또는 exit!=0 + stderr(못 찾음/오류) 둘 다 받아 판정한다 —
 * 어느 스트림에 실려 왔는지는 호출자가 신경 쓸 필요가 없도록 두 텍스트를 합쳐 본다. */
export function parseMcpGetOutput(stdout: string, stderr: string): McpGetParsed {
  const combined = `${stdout}\n${stderr}`;
  if (NOT_FOUND_RE.test(combined)) {
    return { found: false, connected: null, statusText: null, url: null };
  }
  const statusMatch = STATUS_LINE_RE.exec(combined);
  const urlMatch = URL_LINE_RE.exec(combined);
  if (!statusMatch && !urlMatch) {
    // stdout이 비어 있거나 예상 밖 형태 — 추측하지 않고 "찾지 못함"과 구분되는
    // "판정 불가"로 접는다(connected: null).
    return { found: stdout.trim().length > 0, connected: null, statusText: null, url: null };
  }
  const statusText = statusMatch?.[1]?.trim() ?? null;
  return {
    found: true,
    connected: statusText ? statusText.includes('✔') || /connected/i.test(statusText) : null,
    statusText,
    url: urlMatch?.[1] ?? null,
  };
}
