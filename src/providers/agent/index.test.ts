import { describe, expect, it } from 'vitest';
import { createAgentProvider } from './index.js';
import type { ExecFileFn } from '../../platform/exec.js';

function fakeExec(stdout: string): ExecFileFn {
  return (_file, _args, _options, callback) => callback(null, stdout, '');
}

describe('createAgentProvider', () => {
  const provider = createAgentProvider({
    execFileFn: fakeExec('[]'),
    env: {},
    claudeHomeDir: '/home/runner/.claude',
    readTextFile: async () => {
      throw new Error('ENOENT');
    },
  });

  it('id는 agent이고 dependsOn은 비어 있다(§1.2 "mcp는 agent 이후")', () => {
    expect(provider.id).toBe('agent');
    expect(provider.dependsOn).toEqual([]);
  });

  it('detect()는 던지지 않고 Observed를 반환한다', async () => {
    const observed = await provider.detect({ targetFolderTrusted: false });
    expect(observed.providerId).toBe('agent');
  });

  it('plan()이 잘못된 desired(providerId 불일치)를 받으면 빈 plan(추측하지 않는다)', async () => {
    const observed = await provider.detect({ targetFolderTrusted: false });
    const plan = provider.plan(observed, { providerId: 'mcp' });
    expect(plan.changes).toEqual([]);
  });
});
