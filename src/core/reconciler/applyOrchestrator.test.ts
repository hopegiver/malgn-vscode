import { describe, expect, it, vi } from 'vitest';
import { runApplyWithConsent, type ChangeRecordEntry } from './applyOrchestrator.js';
import type { StopSignals } from './stopGate.js';
import type { ApplyContext, Plan, Provider } from '../../providers/types.js';

const PLAN: Plan = {
  providerId: 'agent',
  changes: [{ id: 'agent-plugin', target: 'malgn-agent@malgnsoft-plugins', kind: 'install', level: 'L2', after: '{}', reversible: true, rationale: 'r' }],
  diffHash: 'ignored',
};

const EMPTY_PLAN: Plan = { providerId: 'agent', changes: [], diffHash: 'x' };

const CTX: ApplyContext = { targetFolderTrusted: true };

const ALLOW_ALL_SIGNALS: StopSignals = {
  killSwitch: { minAppVersion: null, maxAppVersion: null, disableProviders: [], message: null, upgradeHint: null },
  currentExtensionVersion: '0.1.0',
  compatGateBelowMinimum: false,
  targetFolderTrusted: true,
  policyCheckoutStale: false,
  hrs4ReconsentRequired: false,
  sessionAttended: true,
};

function fakeProvider(applyImpl?: Provider['apply']): Provider {
  return {
    id: 'agent',
    dependsOn: [],
    detect: async () => ({ providerId: 'agent', status: 'drift', code: 'X', message: 'x', observedAt: '2026-01-01T00:00:00Z' }),
    plan: () => PLAN,
    apply: applyImpl ?? (async () => ({ providerId: 'agent', status: 'ok', code: 'MV_AGENT_OK', message: 'applied', appliedChangeIds: ['agent-plugin'] })),
    verify: async () => ({ providerId: 'agent', status: 'ok', code: 'MV_AGENT_OK', message: 'verified', verifiedAt: '2026-01-01T00:00:01Z' }),
  };
}

describe('runApplyWithConsent', () => {
  it('plan.changes가 비어 있으면 승인 화면을 띄우지도 않고 즉시 no-changes', async () => {
    const requestApproval = vi.fn();
    const recordChange = vi.fn();
    const outcome = await runApplyWithConsent({
      provider: fakeProvider(),
      plan: EMPTY_PLAN,
      extensionVersion: '0.1.0',
      stopSignals: ALLOW_ALL_SIGNALS,
      applyContext: CTX,
      requestApproval,
      recordChange,
    });
    expect(outcome).toEqual({ kind: 'no-changes' });
    expect(requestApproval).not.toHaveBeenCalled();
    expect(recordChange).not.toHaveBeenCalled();
  });

  it('무인 세션이면 consent.issue 표면에서 정지되고 승인 화면을 띄우지 않는다', async () => {
    const requestApproval = vi.fn();
    const outcome = await runApplyWithConsent({
      provider: fakeProvider(),
      plan: PLAN,
      extensionVersion: '0.1.0',
      stopSignals: { ...ALLOW_ALL_SIGNALS, sessionAttended: false },
      applyContext: CTX,
      requestApproval,
      recordChange: vi.fn(),
    });
    expect(outcome.kind).toBe('stopped');
    if (outcome.kind === 'stopped') expect(outcome.surface).toBe('consent.issue');
    expect(requestApproval).not.toHaveBeenCalled();
  });

  it('사용자가 승인 화면에서 거부하면 rejected-by-user', async () => {
    const outcome = await runApplyWithConsent({
      provider: fakeProvider(),
      plan: PLAN,
      extensionVersion: '0.1.0',
      stopSignals: ALLOW_ALL_SIGNALS,
      applyContext: CTX,
      requestApproval: async () => false,
      recordChange: vi.fn(),
    });
    expect(outcome).toEqual({ kind: 'rejected-by-user' });
  });

  it('HRS4 재동의 필요 신호는 consent.issue는 통과시키고 provider.apply만 정지한다', async () => {
    const requestApproval = vi.fn(async () => true);
    const outcome = await runApplyWithConsent({
      provider: fakeProvider(),
      plan: PLAN,
      extensionVersion: '0.1.0',
      stopSignals: { ...ALLOW_ALL_SIGNALS, hrs4ReconsentRequired: true },
      applyContext: CTX,
      requestApproval,
      recordChange: vi.fn(),
    });
    expect(requestApproval).toHaveBeenCalledOnce(); // consent.issue는 통과해 화면까지는 뜬다
    expect(outcome.kind).toBe('stopped');
    if (outcome.kind === 'stopped') expect(outcome.surface).toBe('provider.apply');
  });

  it('정상 승인 경로 — apply → 저널 기록 → verify까지 전부 수행한다', async () => {
    const recordChange = vi.fn(async (_entry: ChangeRecordEntry) => {});
    const applyImpl: Provider['apply'] = vi.fn(async (plan, consent) => {
      // consent가 실제로 이 plan에 바인딩된 유효한 토큰인지 provider 관점에서도 확인
      expect(consent.providerId).toBe(plan.providerId);
      return { providerId: 'agent' as const, status: 'ok' as const, code: 'MV_AGENT_OK', message: 'applied', appliedChangeIds: ['agent-plugin'] };
    });
    const outcome = await runApplyWithConsent({
      provider: fakeProvider(applyImpl),
      plan: PLAN,
      extensionVersion: '0.1.0',
      stopSignals: ALLOW_ALL_SIGNALS,
      applyContext: CTX,
      requestApproval: async () => true,
      recordChange,
    });
    expect(outcome.kind).toBe('applied');
    expect(applyImpl).toHaveBeenCalledOnce();
    expect(recordChange).toHaveBeenCalledOnce();
    const recordedEntry = recordChange.mock.calls[0]?.[0];
    expect(recordedEntry?.provider).toBe('agent');
    expect(recordedEntry?.changes).toEqual(PLAN.changes);
    if (outcome.kind === 'applied') {
      expect(outcome.result.status).toBe('ok');
      expect(outcome.verify.status).toBe('ok');
    }
  });
});
