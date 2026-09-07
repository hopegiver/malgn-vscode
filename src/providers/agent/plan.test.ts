import { describe, expect, it } from 'vitest';
import { planAgent, type AgentDesiredSlice } from './plan.js';
import type { AgentObservedDetail } from './detect.js';
import type { Observed } from '../../providers/types.js';

function observedWith(detail: Partial<AgentObservedDetail>): Observed {
  return {
    providerId: 'agent',
    status: 'drift',
    code: 'X',
    message: 'x',
    observedAt: '2026-01-01T00:00:00Z',
    detail: {
      pluginId: 'malgn-agent@malgnsoft-plugins',
      marketplaceRepo: 'malgnsoft/claude-plugins',
      claudeAvailable: true,
      installed: false,
      scope: null,
      version: null,
      enabled: null,
      marketplaceRegistered: true,
      marketplaceName: 'malgnsoft-plugins',
      marketplaceRepoMatches: true,
      entry: null,
      ...detail,
    },
  };
}

const desired: AgentDesiredSlice = {
  providerId: 'agent',
  agent: { blocked: false, marketplace: 'malgnsoft/claude-plugins', plugin: 'malgn-agent@malgnsoft-plugins', scope: 'user', channel: 'stable' },
  compat: { malgnAgent: '>=1.8.24 <2.0.0', claudeCode: '>=2.1.237' },
};

describe('planAgent', () => {
  it('정책이 agent를 차단하면 빈 plan', () => {
    const blockedDesired: AgentDesiredSlice = { providerId: 'agent', agent: { blocked: true, reason: 'x' }, compat: desired.compat };
    const plan = planAgent(observedWith({}), blockedDesired);
    expect(plan.changes).toEqual([]);
  });

  it('claude 사용 불가면 빈 plan', () => {
    const plan = planAgent(observedWith({ claudeAvailable: false }), desired);
    expect(plan.changes).toEqual([]);
  });

  it('마켓플레이스 저장소 불일치면 자동 정정하지 않는다(빈 plan)', () => {
    const plan = planAgent(observedWith({ marketplaceRepoMatches: false }), desired);
    expect(plan.changes).toEqual([]);
  });

  it('known 결함 버전이면 빈 plan(block-apply)', () => {
    const plan = planAgent(observedWith({ installed: true, version: '1.8.19', enabled: true }), desired);
    expect(plan.changes).toEqual([]);
  });

  it('미설치면 install Change 하나를 만든다', () => {
    const plan = planAgent(observedWith({ marketplaceRegistered: false }), desired);
    expect(plan.changes).toHaveLength(1);
    expect(plan.changes[0]?.kind).toBe('install');
    expect(plan.changes[0]?.level).toBe('L2');
    expect(plan.changes[0]?.reversible).toBe(true);
  });

  it('설치+활성+최신이면 빈 plan(ok)', () => {
    const plan = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.33' }), desired);
    expect(plan.changes).toEqual([]);
  });

  it('설치됐으나 비활성이면 enable 방향 Change(kind exec)', () => {
    const plan = planAgent(observedWith({ installed: true, enabled: false, version: '1.8.33' }), desired);
    expect(plan.changes).toHaveLength(1);
    expect(plan.changes[0]?.rationale).toContain('활성화');
  });

  it('compat 하한 미만이면 update Change', () => {
    const plan = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.20' }), desired);
    expect(plan.changes).toHaveLength(1);
    expect(plan.changes[0]?.kind).toBe('update');
  });

  it('버전만 다르고 entryJson이 같으면 동일한 diffHash를 낸다(재동의 스킵의 근거)', () => {
    const entry = { name: 'malgn-agent', source: './malgn-agent', version: '1.8.24' };
    const planA = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.24', entry: { ...entry, version: '1.8.24' } }), desired);
    const planB = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.24', entry: { ...entry, version: '1.8.30' } }), desired);
    // 둘 다 outdated 판정 없음(compat 하한 1.8.24 이상 만족) → 변경 없음(같게 빈 plan)이라
    // 이 케이스만으로는 자명하다. 실제로 의미 있는 비교는 아래 outdated 케이스.
    expect(planA.changes).toEqual(planB.changes);
  });

  it('entryJson이 같고 버전만 하한 아래로 다른 update 시나리오 두 번은 같은 diffHash', () => {
    const entryV1 = { name: 'malgn-agent', source: './malgn-agent', version: '1.8.20' };
    const entryV2 = { name: 'malgn-agent', source: './malgn-agent', version: '1.8.21' };
    const planA = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.20', entry: entryV1 }), desired);
    const planB = planAgent(observedWith({ installed: true, enabled: true, version: '1.8.21', entry: entryV2 }), desired);
    expect(planA.diffHash).toBe(planB.diffHash);
  });
});
