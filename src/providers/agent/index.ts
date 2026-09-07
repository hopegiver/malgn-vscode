// agent provider 조립 — architecture.md §1.2 균일 Provider 인터페이스를 §3.2의 malgn-agent
// 연동으로 채운다. 이 파일은 DI(실행 함수·env·경로)를 받아 `Provider` 객체 하나를
// 만드는 팩토리일 뿐이고, 각 단계의 실제 로직은 `detect.ts`/`plan.ts`/`apply.ts`/
// `verify.ts`에 있다(파일당 책임 하나 원칙 — 이 파일에서 새 판단을 추가하지 않는다).

import type { ApplyContext, ApplyResult, DesiredSlice, DetectContext, Observed, Plan, Provider, VerifyResult } from '../types.js';
import type { ExecFileFn } from '../../platform/exec.js';
import { computeDiffHash } from '../../core/reconciler/diffHash.js';
import { detectAgent } from './detect.js';
import { planAgent, type AgentDesiredSlice } from './plan.js';
import { applyAgent } from './apply.js';
import { verifyAgent } from './verify.js';
import type { ReadTextFile } from './marketplaceReader.js';

export interface AgentProviderDeps {
  readonly execFileFn: ExecFileFn;
  readonly env: Readonly<Record<string, string | undefined>>;
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
}

function isAgentDesiredSlice(desired: DesiredSlice): desired is AgentDesiredSlice {
  return desired.providerId === 'agent' && 'agent' in desired && 'compat' in desired;
}

/**
 * `Provider` 계약을 만족하는 agent provider를 만든다. `apply()`는 타입 수준에서
 * `ConsentToken`을 요구하지만(providers/types.ts) 실제 런타임 재검증(`gate.assertValid`)은
 * 이 함수 밖(오케스트레이션 단일 호출 지점)에서 이미 끝났다고 가정한다 — 이 provider
 * 자신은 토큰 내용을 다시 읽지 않는다(재검증 로직 중복 방지).
 */
export function createAgentProvider(deps: AgentProviderDeps): Provider {
  return {
    id: 'agent',
    dependsOn: [],
    detect: (ctx: DetectContext): Promise<Observed> => detectAgent(deps, ctx),
    plan: (observed: Observed, desired: DesiredSlice): Plan => {
      if (!isAgentDesiredSlice(desired)) {
        // 계약 위반(호출자가 잘못된 desired를 넘김) — 추측하지 않고 빈 plan(PR-6).
        return { providerId: 'agent', changes: [], diffHash: computeDiffHash('agent', []) };
      }
      return planAgent(observed, desired);
    },
    apply: (plan: Plan, _consent, ctx: ApplyContext): Promise<ApplyResult> => applyAgent(deps, plan, ctx),
    verify: (ctx: DetectContext): Promise<VerifyResult> => verifyAgent(deps, ctx),
  };
}
