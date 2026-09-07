import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { Observed } from '../../providers/types.js';
import { buildDefaultOtelEnv, buildOtelResourceAttributesValue, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';
import type { OtelObservedDetail } from './detect.js';
import { planOtel, type OtelDesiredSlice } from './plan.js';

const constants = loadCodeConstants();

function observedWith(detail: Partial<OtelObservedDetail> & Pick<OtelObservedDetail, 'observedEnv'>): Observed {
  return {
    providerId: 'otel',
    status: 'drift',
    code: 'MV_OTEL_DRIFT',
    message: 'x',
    observedAt: new Date().toISOString(),
    detail: {
      platformSupported: true,
      settingsFileFound: true,
      headersHelperConfigured: true,
      headersHelperPath: '/x/otel-headers.sh',
      headersHelperExecutable: true,
      missingKeys: [],
      mismatchedKeys: [],
      resourceAttributesConfigured: false,
      ...detail,
    } satisfies OtelObservedDetail,
  };
}

const desiredNoPolicy: OtelDesiredSlice = { providerId: 'otel', policyEnv: null };

describe('planOtel', () => {
  it('detail이 없으면(blocked/unknown) 빈 plan(PR-6)', () => {
    const observed: Observed = { providerId: 'otel', status: 'unknown', code: 'x', message: 'x', observedAt: new Date().toISOString() };
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    expect(plan.changes).toEqual([]);
  });

  it('헤더 헬퍼가 없거나 실행 불가면 빈 plan', () => {
    const observed = observedWith({ observedEnv: {}, headersHelperConfigured: false });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    expect(plan.changes).toEqual([]);
  });

  it('로컬 env가 완전히 비어 있으면 desired 키 전부에 대해 add(L1) Change를 만든다', () => {
    const observed = observedWith({ observedEnv: {} });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    const desired = buildDefaultOtelEnv(constants);
    // desired.env 키 개수 + resource attributes 1개
    expect(plan.changes.length).toBe(Object.keys(desired.env).length + 1);
    for (const change of plan.changes) {
      expect(change.kind).toBe('add');
      expect(change.level).toBe('L1');
      expect(change.before).toBeUndefined();
    }
  });

  it('로컬에 다른 값이 있으면 update(L2) Change를 만든다', () => {
    const desired = buildDefaultOtelEnv(constants);
    const key = Object.keys(desired.env)[0]!;
    const observed = observedWith({ observedEnv: { [key]: 'wrong-value' } });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    const change = plan.changes.find((c) => c.target === `env.${key}`);
    expect(change).toBeDefined();
    expect(change!.kind).toBe('update');
    expect(change!.level).toBe('L2');
    expect(change!.before).toBe('wrong-value');
    expect(change!.after).toBe(desired.env[key]);
  });

  it('로컬 값이 이미 desired와 같으면 그 키는 Change를 만들지 않는다(no-op)', () => {
    const desired = buildDefaultOtelEnv(constants);
    const observed = observedWith({ observedEnv: { ...desired.env, [OTEL_RESOURCE_ATTRIBUTES_KEY]: buildOtelResourceAttributesValue('tester') } });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    expect(plan.changes).toEqual([]);
  });

  it('OTEL_RESOURCE_ATTRIBUTES는 osUsername으로만 만들고 employee.name을 담지 않는다', () => {
    const observed = observedWith({ observedEnv: buildDefaultOtelEnv(constants).env });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    const change = plan.changes.find((c) => c.target === `env.${OTEL_RESOURCE_ATTRIBUTES_KEY}`);
    expect(change).toBeDefined();
    expect(change!.after).toBe('employee.id=tester');
    expect(change!.after).not.toContain('employee.name');
  });

  it('로컬에만 있는 미지 OTEL_* 키는 Change를 만들지 않는다(L0 — 보고만, 삭제 안 함)', () => {
    const desired = buildDefaultOtelEnv(constants);
    const observed = observedWith({
      observedEnv: { ...desired.env, [OTEL_RESOURCE_ATTRIBUTES_KEY]: buildOtelResourceAttributesValue('tester'), OTEL_LOCAL_ONLY_EXPERIMENT: 'x' },
    });
    const plan = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    expect(plan.changes.find((c) => c.target.includes('OTEL_LOCAL_ONLY_EXPERIMENT'))).toBeUndefined();
    expect(plan.changes).toEqual([]);
  });

  it('policyEnv가 있으면 그 값을 desired로 채택한다(사이트면 기본값을 덮어쓴다)', () => {
    const observed = observedWith({ observedEnv: {} });
    const policyDesired: OtelDesiredSlice = { providerId: 'otel', policyEnv: { CLAUDE_CODE_ENABLE_TELEMETRY: '1' } };
    const plan = planOtel({ osUsername: 'tester' }, observed, policyDesired);
    const teleChange = plan.changes.find((c) => c.target === 'env.CLAUDE_CODE_ENABLE_TELEMETRY');
    expect(teleChange).toBeDefined();
    expect(teleChange!.after).toBe('1');
    // policyEnv에 없는 사이트면 기본 키(예: OTEL_METRICS_EXPORTER)는 계획되지 않는다
    expect(plan.changes.find((c) => c.target === 'env.OTEL_METRICS_EXPORTER')).toBeUndefined();
  });

  it('diffHash는 같은 변경 집합에 대해 결정적이다', () => {
    const observed = observedWith({ observedEnv: {} });
    const plan1 = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    const plan2 = planOtel({ osUsername: 'tester' }, observed, desiredNoPolicy);
    expect(plan1.diffHash).toBe(plan2.diffHash);
    expect(plan1.diffHash.length).toBeGreaterThan(0);
  });
});
