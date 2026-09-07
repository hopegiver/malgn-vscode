import { describe, expect, it } from 'vitest';
import { applyMcp } from './apply.js';
import type { ExecFileFn } from '../../platform/exec.js';
import type { ApplyContext, Plan } from '../../providers/types.js';
import { MV_MCP_ENABLE_FAILED, MV_MCP_OK } from './errors.js';

const CTX: ApplyContext = { targetFolderTrusted: true };
const PLAN: Plan = {
  providerId: 'mcp',
  changes: [{ id: 'mcp-plugin-enable', target: 'malgn-agent@malgnsoft-plugins', kind: 'exec', level: 'L2', after: 'enabled', reversible: true, rationale: 'x' }],
  diffHash: 'x',
};

function fakeExec(ok: boolean): ExecFileFn {
  return (_file, _args, _options, callback) => callback(ok ? null : ({ code: 1 } as never), '', '');
}

describe('applyMcp', () => {
  it('변경이 없으면 즉시 ok', async () => {
    const result = await applyMcp({ execFileFn: fakeExec(true), env: {} }, { providerId: 'mcp', changes: [], diffHash: 'x' }, CTX);
    expect(result.status).toBe('ok');
    expect(result.appliedChangeIds).toEqual([]);
  });

  it('enable 성공이면 ok + appliedChangeIds', async () => {
    const result = await applyMcp({ execFileFn: fakeExec(true), env: {} }, PLAN, CTX);
    expect(result.status).toBe('ok');
    expect(result.code).toBe(MV_MCP_OK);
    expect(result.appliedChangeIds).toEqual(['mcp-plugin-enable']);
  });

  it('enable 실패면 blocked', async () => {
    const result = await applyMcp({ execFileFn: fakeExec(false), env: {} }, PLAN, CTX);
    expect(result.status).toBe('blocked');
    expect(result.code).toBe(MV_MCP_ENABLE_FAILED);
  });
});
