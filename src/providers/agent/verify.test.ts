import { describe, expect, it } from 'vitest';
import { verifyAgent } from './verify.js';
import type { ExecFileFn } from '../../platform/exec.js';
import { MV_AGENT_OK, MV_AGENT_VERIFY_FAILED } from './errors.js';

function fakeExec(stdout: string): ExecFileFn {
  return (_file, _args, _options, callback) => callback(null, stdout, '');
}

describe('verifyAgent', () => {
  it('설치·활성 상태면 ok', async () => {
    const stdout = JSON.stringify([{ id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: true }]);
    const result = await verifyAgent({ execFileFn: fakeExec(stdout), env: {} }, {} as never);
    expect(result.status).toBe('ok');
    expect(result.code).toBe(MV_AGENT_OK);
  });

  it('재확인에서 안 보이면 blocked', async () => {
    const result = await verifyAgent({ execFileFn: fakeExec('[]'), env: {} }, {} as never);
    expect(result.status).toBe('blocked');
    expect(result.code).toBe(MV_AGENT_VERIFY_FAILED);
  });

  it('enabled:false로 재확인되면 blocked', async () => {
    const stdout = JSON.stringify([{ id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: false }]);
    const result = await verifyAgent({ execFileFn: fakeExec(stdout), env: {} }, {} as never);
    expect(result.status).toBe('blocked');
  });
});
