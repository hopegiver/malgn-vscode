import { describe, expect, it } from 'vitest';
import { parseMcpGetOutput, pluginMcpServerName } from './cli.js';

const FOUND_STDOUT = `plugin:malgn-agent:malgnai-hub:
  Scope: Dynamic config (from command line)
  Status: ✔ Connected
  Type: http
  URL: https://hub.example.com/mcp
  OAuth: client_id configured`;

const NOT_FOUND_STDERR = `No MCP server named "no-such-server". Configured servers: example.com Gmail, malgnai-mcp, plugin:malgn-agent:malgnai-hub`;

describe('pluginMcpServerName', () => {
  it('실측 형태로 조립한다', () => {
    expect(pluginMcpServerName('malgn-agent', 'malgnai-hub')).toBe('plugin:malgn-agent:malgnai-hub');
  });
});

describe('parseMcpGetOutput', () => {
  it('실측 정상 출력을 파싱한다', () => {
    const parsed = parseMcpGetOutput(FOUND_STDOUT, '');
    expect(parsed).toEqual({ found: true, connected: true, statusText: '✔ Connected', url: 'https://hub.example.com/mcp' });
  });

  it('실측 "찾지 못함" 출력을 판정한다', () => {
    const parsed = parseMcpGetOutput('', NOT_FOUND_STDERR);
    expect(parsed).toEqual({ found: false, connected: null, statusText: null, url: null });
  });

  it('연결 안 됨(✔ 없음) 상태 텍스트도 statusText로 보존한다', () => {
    const stdout = 'x:\n  Status: ✗ Needs authentication\n  URL: https://hub.example.com/mcp\n';
    const parsed = parseMcpGetOutput(stdout, '');
    expect(parsed.connected).toBe(false);
    expect(parsed.statusText).toContain('Needs authentication');
  });

  it('완전히 낯선 형태는 판정 불가(connected null)로 접는다', () => {
    const parsed = parseMcpGetOutput('알 수 없는 형태', '');
    expect(parsed.connected).toBeNull();
  });
});
