import { describe, expect, it } from 'vitest';
import type { DesiredSlice } from '../types.js';
import { createOtelProvider, type OtelProviderDeps } from './index.js';

function baseDeps(overrides: Partial<OtelProviderDeps> = {}): OtelProviderDeps {
  return {
    claudeHomeDir: '/home/runner/.claude',
    readTextFile: async () => {
      const err = new Error('ENOENT') as NodeJS.ErrnoException;
      err.code = 'ENOENT';
      throw err;
    },
    pathExecutable: async () => true,
    platform: 'darwin',
    backupsDir: '/home/runner/.malgn/backups',
    osUsername: 'tester',
    ...overrides,
  };
}

describe('createOtelProvider', () => {
  it('id는 otel이고 dependsOn=[]', () => {
    const provider = createOtelProvider(baseDeps());
    expect(provider.id).toBe('otel');
    expect(provider.dependsOn).toEqual([]);
  });

  it('detect()는 던지지 않는다', async () => {
    const provider = createOtelProvider(baseDeps());
    const observed = await provider.detect({ targetFolderTrusted: false });
    expect(observed.providerId).toBe('otel');
  });

  it('plan()에 계약과 다른 desired(providerId 다름)를 넘기면 추측하지 않고 빈 plan을 반환한다(PR-6)', async () => {
    const provider = createOtelProvider(baseDeps());
    const observed = await provider.detect({ targetFolderTrusted: false });
    const wrongDesired = { providerId: 'agent' } as unknown as DesiredSlice;
    const plan = provider.plan(observed, wrongDesired);
    expect(plan.changes).toEqual([]);
  });

  it('verify()는 던지지 않는다', async () => {
    const provider = createOtelProvider(baseDeps());
    const result = await provider.verify({ targetFolderTrusted: false });
    expect(result.providerId).toBe('otel');
  });
});
