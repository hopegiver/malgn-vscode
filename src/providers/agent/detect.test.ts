import { describe, expect, it } from 'vitest';
import { detectAgent } from './detect.js';
import type { ExecFileFn } from '../../platform/exec.js';
import { MV_AGENT_CLAUDE_UNAVAILABLE, MV_AGENT_DISABLED, MV_AGENT_MARKETPLACE_MISSING, MV_AGENT_NOT_INSTALLED, MV_AGENT_OK } from './errors.js';

const PLUGIN_LIST_OK = JSON.stringify([
  { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: true, installPath: '/x' },
]);
const MARKETPLACE_LIST_OK = JSON.stringify([
  { name: 'malgnsoft-plugins', source: 'github', repo: 'malgnsoft/claude-plugins', installLocation: '/loc/malgnsoft-plugins' },
]);

function fakeExec(handler: (file: string, args: readonly string[]) => { stdout: string; error?: { code?: number | string } }): ExecFileFn {
  return (file, args, _options, callback) => {
    const { stdout, error } = handler(file, args);
    callback((error as never) ?? null, stdout, '');
  };
}

function fakeReadTextFile(files: Record<string, string>) {
  return async (path: string): Promise<string> => {
    const content = files[path];
    if (content === undefined) throw new Error(`ENOENT ${path}`);
    return content;
  };
}

describe('detectAgent — 실측 형태 기반', () => {
  it('claude가 PATH에 없으면 blocked + MV_AGENT_CLAUDE_UNAVAILABLE', async () => {
    const execFileFn = fakeExec(() => ({ stdout: '', error: { code: 'ENOENT' } }));
    const observed = await detectAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      {} as never
    );
    expect(observed.status).toBe('blocked');
    expect(observed.code).toBe(MV_AGENT_CLAUDE_UNAVAILABLE);
  });

  it('설치·활성·마켓플레이스 정상이면 ok', async () => {
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('marketplace')) return { stdout: MARKETPLACE_LIST_OK };
      return { stdout: PLUGIN_LIST_OK };
    });
    const readTextFile = fakeReadTextFile({
      '/home/runner/.claude/plugins/known_marketplaces.json': JSON.stringify({
        'malgnsoft-plugins': { source: { source: 'github', repo: 'malgnsoft/claude-plugins' }, installLocation: '/loc/malgnsoft-plugins' },
      }),
      '/loc/malgnsoft-plugins/.claude-plugin/marketplace.json': JSON.stringify({
        plugins: [{ name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' }],
      }),
    });
    const observed = await detectAgent({ execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile }, {} as never);
    expect(observed.status).toBe('ok');
    expect(observed.code).toBe(MV_AGENT_OK);
    const detail = observed.detail as { entry: unknown };
    expect(detail.entry).toEqual({ name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' });
  });

  it('마켓플레이스 미등록이면 drift + MV_AGENT_MARKETPLACE_MISSING', async () => {
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('marketplace')) return { stdout: '[]' };
      return { stdout: '[]' };
    });
    const observed = await detectAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      {} as never
    );
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_AGENT_MARKETPLACE_MISSING);
  });

  it('마켓플레이스는 등록됐지만 플러그인 미설치면 MV_AGENT_NOT_INSTALLED', async () => {
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('marketplace')) return { stdout: MARKETPLACE_LIST_OK };
      return { stdout: '[]' };
    });
    const observed = await detectAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      {} as never
    );
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_AGENT_NOT_INSTALLED);
  });

  it('설치돼 있으나 enabled:false면 MV_AGENT_DISABLED', async () => {
    const disabledList = JSON.stringify([
      { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: false },
    ]);
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('marketplace')) return { stdout: MARKETPLACE_LIST_OK };
      return { stdout: disabledList };
    });
    const observed = await detectAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      {} as never
    );
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_AGENT_DISABLED);
  });

  it('던지지 않는다 — exec가 예외적 콜백을 줘도 unknown으로 접는다', async () => {
    const execFileFn = fakeExec(() => ({ stdout: 'not json' }));
    const observed = await detectAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      {} as never
    );
    // parsePluginListJson('not json') → [] → 마켓플레이스도 파싱 실패 → MISSING이 먼저 걸린다
    expect(['drift', 'unknown']).toContain(observed.status);
  });
});
