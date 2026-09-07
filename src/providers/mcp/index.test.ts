import { describe, expect, it } from 'vitest';
import { createMcpProvider } from './index.js';
import type { ExecFileFn } from '../../platform/exec.js';

function fakeExec(stdout: string): ExecFileFn {
  return (_file, _args, _options, callback) => callback(null, stdout, '');
}

describe('createMcpProvider', () => {
  it('id는 mcp이고 dependsOn=["agent"](§1.2 "mcp는 agent 이후")', () => {
    const provider = createMcpProvider({ execFileFn: fakeExec('[]'), env: {} });
    expect(provider.id).toBe('mcp');
    expect(provider.dependsOn).toEqual(['agent']);
  });

  it('detect()는 던지지 않는다', async () => {
    const provider = createMcpProvider({ execFileFn: fakeExec('[]'), env: {} });
    const observed = await provider.detect({ targetFolderTrusted: false });
    expect(observed.providerId).toBe('mcp');
  });
});
