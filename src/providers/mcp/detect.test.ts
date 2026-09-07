import { describe, expect, it } from 'vitest';
import { detectMcp } from './detect.js';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { ExecFileFn } from '../../platform/exec.js';
import {
  MV_MCP_AGENT_NOT_INSTALLED,
  MV_MCP_HUB_URL_MISMATCH,
  MV_MCP_NOT_REGISTERED,
  MV_MCP_OAUTH_REQUIRED,
  MV_MCP_OK,
  MV_MCP_PLUGIN_DISABLED,
} from './errors.js';

// [site 프로필 독립] 이 파일은 로컬에 `site/site.json`이 있는지(siteProfile==='site')
// 없는지(siteProfile==='example')에 관계없이 항상 같은 결과를 내야 한다 — 예전에는
// `hub.example.com`을 하드코딩해 example 프로필에서만 통과했다(W8이 실제 site.json을
// 만들면서 이 결합이 드러났다). "허용목록 안" 픽스처는 `loadCodeConstants()`가 실제로
// 내놓는 `allowedAuthorities.mcp[0]`을 그대로 써서 어느 프로필에서도 일치하게 하고,
// "허용목록 밖" 픽스처만 예약 네임스페이스(RFC 2606) 고정 도메인을 쓴다 — 실 사이트면의
// `mcp` 권한이 `*.example.net`을 가리킬 리 없으므로 항상 밖이다.
const MCP_ALLOWED_AUTHORITY = loadCodeConstants().allowedAuthorities.mcp[0]!;

const PLUGIN_LIST_ENABLED = JSON.stringify([
  { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: true },
]);
const PLUGIN_LIST_DISABLED = JSON.stringify([
  { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: false },
]);

const MCP_GET_CONNECTED = `plugin:malgn-agent:malgnai-hub:
  Status: ✔ Connected
  URL: https://${MCP_ALLOWED_AUTHORITY}/mcp`;

const MCP_GET_CONNECTED_OUTSIDE_ALLOWLIST = `plugin:malgn-agent:malgnai-hub:
  Status: ✔ Connected
  URL: https://rogue-hub.example.net/mcp`;

const MCP_GET_NOT_FOUND = `No MCP server named "plugin:malgn-agent:malgnai-hub". Configured servers: x`;

function fakeExec(handler: (args: readonly string[]) => { stdout?: string; error?: { code?: number | string } }): ExecFileFn {
  return (_file, args, _options, callback) => {
    const { stdout = '', error } = handler(args);
    callback((error as never) ?? null, stdout, error ? stdout : '');
  };
}

describe('detectMcp', () => {
  it('플러그인 미설치면 blocked + MV_MCP_AGENT_NOT_INSTALLED', async () => {
    const execFileFn = fakeExec(() => ({ stdout: '[]' }));
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('blocked');
    expect(observed.code).toBe(MV_MCP_AGENT_NOT_INSTALLED);
  });

  it('설치됐으나 비활성이면 drift + MV_MCP_PLUGIN_DISABLED', async () => {
    const execFileFn = fakeExec(() => ({ stdout: PLUGIN_LIST_DISABLED }));
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_MCP_PLUGIN_DISABLED);
  });

  it('활성 + mcp get 정상 연결이면 ok', async () => {
    const execFileFn = fakeExec((args) => (args[0] === 'mcp' ? { stdout: MCP_GET_CONNECTED } : { stdout: PLUGIN_LIST_ENABLED }));
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('ok');
    expect(observed.code).toBe(MV_MCP_OK);
  });

  it('활성이나 mcp get이 못 찾으면 drift + MV_MCP_NOT_REGISTERED(자동 등록하지 않는다)', async () => {
    const execFileFn: ExecFileFn = (_file, args, _options, callback) => {
      if (args[0] === 'mcp') {
        callback({ name: 'Error', message: 'x', code: 1 }, '', MCP_GET_NOT_FOUND);
        return;
      }
      callback(null, PLUGIN_LIST_ENABLED, '');
    };
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_MCP_NOT_REGISTERED);
  });

  it('OAuth 인증 필요 상태 텍스트면 MV_MCP_OAUTH_REQUIRED', async () => {
    const mcpGetAuth = `x:\n  Status: ✗ Needs authentication\n  URL: https://${MCP_ALLOWED_AUTHORITY}/mcp\n`;
    const execFileFn: ExecFileFn = (_file, args, _options, callback) => {
      if (args[0] === 'mcp') {
        callback(null, mcpGetAuth, '');
        return;
      }
      callback(null, PLUGIN_LIST_ENABLED, '');
    };
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_MCP_OAUTH_REQUIRED);
  });

  it('등록된 URL이 allowedAuthorities.mcp 밖이면 drift + MV_MCP_HUB_URL_MISMATCH — 자동 정정하지 않는다', async () => {
    const execFileFn: ExecFileFn = (_file, args, _options, callback) => {
      if (args[0] === 'mcp') {
        callback(null, MCP_GET_CONNECTED_OUTSIDE_ALLOWLIST, '');
        return;
      }
      callback(null, PLUGIN_LIST_ENABLED, '');
    };
    const observed = await detectMcp({ execFileFn, env: {} }, {} as never);
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_MCP_HUB_URL_MISMATCH);
  });
});
