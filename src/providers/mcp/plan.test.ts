import { describe, expect, it } from 'vitest';
import { planMcp } from './plan.js';
import type { Observed } from '../../providers/types.js';
import { MV_MCP_OK, MV_MCP_PLUGIN_DISABLED } from './errors.js';

function observed(code: string, detail: Record<string, unknown>): Observed {
  return { providerId: 'mcp', status: 'drift', code, message: 'x', detail, observedAt: '2026-01-01T00:00:00Z' };
}

describe('planMcp', () => {
  it('MV_MCP_PLUGIN_DISABLED면 enable Change 하나를 만든다', () => {
    const plan = planMcp(observed(MV_MCP_PLUGIN_DISABLED, { pluginInstalled: true, pluginEnabled: false }));
    expect(plan.changes).toHaveLength(1);
    expect(plan.changes[0]?.kind).toBe('exec');
    expect(plan.changes[0]?.level).toBe('L2');
  });

  it('다른 모든 코드는 빈 plan(보고만, §5.3)', () => {
    expect(planMcp(observed(MV_MCP_OK, {})).changes).toEqual([]);
    expect(planMcp(observed('MV_MCP_HUB_URL_MISMATCH', {})).changes).toEqual([]);
    expect(planMcp(observed('MV_MCP_OAUTH_REQUIRED', {})).changes).toEqual([]);
    expect(planMcp(observed('MV_MCP_NOT_REGISTERED', {})).changes).toEqual([]);
  });
});
