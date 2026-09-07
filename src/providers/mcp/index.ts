// mcp provider 조립 — architecture.md §5(REQ-5). `dependsOn: ['agent']`(§1.2 "mcp는
// agent 이후")가 실행 순서를 강제한다(`providers/registry.ts`의 `topologicalOrder`).

import type { ApplyContext, ApplyResult, DesiredSlice, DetectContext, Observed, Plan, Provider, VerifyResult } from '../types.js';
import type { ExecFileFn } from '../../platform/exec.js';
import { detectMcp } from './detect.js';
import { planMcp } from './plan.js';
import { applyMcp } from './apply.js';
import { verifyMcp } from './verify.js';

export interface McpProviderDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
}

export function createMcpProvider(deps: McpProviderDeps): Provider {
  return {
    id: 'mcp',
    dependsOn: ['agent'],
    detect: (ctx: DetectContext): Promise<Observed> => detectMcp(deps, ctx),
    plan: (observed: Observed, _desired: DesiredSlice): Plan => planMcp(observed),
    apply: (plan: Plan, _consent, ctx: ApplyContext): Promise<ApplyResult> => applyMcp(deps, plan, ctx),
    verify: (ctx: DetectContext): Promise<VerifyResult> => verifyMcp(deps, ctx),
  };
}
