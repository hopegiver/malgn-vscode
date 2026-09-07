import { describe, expect, it, vi } from 'vitest';
import type { LoadEffectivePolicyWithFallbackResult } from '../policySource/loadWithFallback.js';
import { filterProvidersByKillSwitch, runActivationSequence } from './activationSequence.js';
import type { DesiredSlice, Observed, Plan, Provider } from '../../providers/types.js';
import type { EffectiveKillSwitch } from '../../core/policy/types.js';

function makeKillSwitch(disableProviders: readonly Provider['id'][] = []): EffectiveKillSwitch {
  return { minAppVersion: null, maxAppVersion: null, disableProviders, message: null, upgradeHint: null };
}

function makeProvider(id: Provider['id'], changes: Plan['changes'] = []): Provider {
  return {
    id,
    dependsOn: [],
    detect: vi.fn(async (): Promise<Observed> => ({ providerId: id, status: 'ok', code: `MV_${id.toUpperCase()}_OK`, message: 'ok', observedAt: new Date().toISOString() })),
    plan: vi.fn((_observed, _desired: DesiredSlice): Plan => ({ providerId: id, changes, diffHash: 'deadbeef' })),
    apply: vi.fn(),
    verify: vi.fn(),
  };
}

const OK_POLICY_OUTCOME: LoadEffectivePolicyWithFallbackResult = {
  source: 'bundled',
  sourcePath: null,
  result: {
    status: 'ok',
    policy: {
      schemaVersion: 1,
      generatedAt: null,
      extension: { latestVersion: null, downloadHint: null },
      killSwitch: makeKillSwitch(),
      rollout: [],
      compat: { malgnAgent: '>=1.8.24 <2.0.0', claudeCode: '>=2.1.237' },
      agent: { blocked: true, reason: 'no policy' },
      otel: { blocked: false, env: {}, headersHelper: null },
      install: { mode: 'assisted' },
      github: { blocked: true, requiredScopes: [] },
      cloudflare: { loginMode: 'wrangler-oauth' },
    },
    issues: [],
  },
  skippedSources: [],
};

describe('filterProvidersByKillSwitch', () => {
  it('킬 스위치가 비활성화한 provider를 제외하고 disabled 목록에 담는다', () => {
    const providers = [makeProvider('agent'), makeProvider('otel')];
    const { active, disabled } = filterProvidersByKillSwitch(providers, makeKillSwitch(['otel']));
    expect(active.map((p) => p.id)).toEqual(['agent']);
    expect(disabled).toEqual(['otel']);
  });

  it('킬 스위치가 없으면 전부 active다', () => {
    const providers = [makeProvider('agent'), makeProvider('otel')];
    const { active, disabled } = filterProvidersByKillSwitch(providers, makeKillSwitch());
    expect(active).toHaveLength(2);
    expect(disabled).toEqual([]);
  });
});

describe('runActivationSequence — §2.2 ①~⑦, apply()는 절대 호출되지 않는다', () => {
  it('provider가 0개(현재 상태)여도 정상적으로 완료되고 상태를 보고한다', async () => {
    const setTrayState = vi.fn();
    const reportStatus = vi.fn();
    const report = await runActivationSequence({
      setTrayState,
      loadPolicy: async () => OK_POLICY_OUTCOME,
      providers: [],
      buildDesiredSlice: () => ({ providerId: 'agent' }),
      detectContext: { targetFolderTrusted: false },
      reportStatus,
    });
    expect(setTrayState).toHaveBeenCalledWith('starting');
    expect(setTrayState).toHaveBeenCalledWith('ok');
    expect(report.activeProviderIds).toEqual([]);
    expect(report.observed).toEqual([]);
    expect(reportStatus).toHaveBeenCalledWith(report);
  });

  it('킬 스위치로 비활성화된 provider는 detect/plan을 호출하지 않는다', async () => {
    const otel = makeProvider('otel');
    const agent = makeProvider('agent');
    const basePolicy = OK_POLICY_OUTCOME.result.status === 'ok' ? OK_POLICY_OUTCOME.result.policy : null;
    if (basePolicy === null) throw new Error('test fixture setup error');
    const killSwitchPolicy: LoadEffectivePolicyWithFallbackResult = {
      ...OK_POLICY_OUTCOME,
      result: { status: 'ok', policy: { ...basePolicy, killSwitch: makeKillSwitch(['otel']) }, issues: [] },
    };
    const report = await runActivationSequence({
      setTrayState: vi.fn(),
      loadPolicy: async () => killSwitchPolicy,
      providers: [otel, agent],
      buildDesiredSlice: () => ({ providerId: 'agent' }),
      detectContext: { targetFolderTrusted: false },
      reportStatus: vi.fn(),
    });
    expect(otel.detect).not.toHaveBeenCalled();
    expect(agent.detect).toHaveBeenCalled();
    expect(report.disabledByKillSwitch).toEqual(['otel']);
    expect(report.activeProviderIds).toEqual(['agent']);
  });

  it('plan에 변경사항이 있으면 트레이 상태를 drift로 표시한다', async () => {
    const agent = makeProvider('agent', [
      { id: 'agent:add:x', target: 'x', kind: 'add', level: 'L1', after: 'y', reversible: true, rationale: 'r' },
    ]);
    const setTrayState = vi.fn();
    await runActivationSequence({
      setTrayState,
      loadPolicy: async () => OK_POLICY_OUTCOME,
      providers: [agent],
      buildDesiredSlice: () => ({ providerId: 'agent' }),
      detectContext: { targetFolderTrusted: false },
      reportStatus: vi.fn(),
    });
    expect(setTrayState).toHaveBeenCalledWith('drift');
  });

  it('provider.apply를 호출하지 않는다', async () => {
    const agent = makeProvider('agent');
    await runActivationSequence({
      setTrayState: vi.fn(),
      loadPolicy: async () => OK_POLICY_OUTCOME,
      providers: [agent],
      buildDesiredSlice: () => ({ providerId: 'agent' }),
      detectContext: { targetFolderTrusted: false },
      reportStatus: vi.fn(),
    });
    expect(agent.apply).not.toHaveBeenCalled();
  });
});
