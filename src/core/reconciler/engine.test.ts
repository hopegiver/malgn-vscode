import { describe, expect, it } from 'vitest';
import { DETECT_TIMEOUT_MS, detectAll } from './engine.js';
import type { DetectContext, Observed, Provider, VerifyResult } from '../../providers/types.js';

const baseCtx: DetectContext = { workspaceTrusted: true };

function okObserved(id: Provider['id']): Observed {
  return { providerId: id, status: 'ok', code: `MV_${id.toUpperCase()}_OK`, message: 'ok', observedAt: new Date().toISOString() };
}

function verifyStub(id: Provider['id']): Promise<VerifyResult> {
  return Promise.resolve({ providerId: id, status: 'ok', code: `MV_${id.toUpperCase()}_OK`, message: 'ok', verifiedAt: new Date().toISOString() });
}

function fastProvider(id: Provider['id']): Provider {
  return {
    id,
    dependsOn: [],
    async detect() {
      return okObserved(id);
    },
    plan(observed) {
      return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
    },
    async apply(plan) {
      return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
    },
    verify: () => verifyStub(id),
  };
}

/** detect()가 절대 resolve/reject하지 않는(하청 프로세스가 멈춘) provider — N-3 재현 */
function hangingProvider(id: Provider['id']): Provider {
  return {
    id,
    dependsOn: [],
    detect() {
      return new Promise<Observed>(() => {
        /* 의도적으로 영원히 resolve하지 않는다 */
      });
    },
    plan(observed) {
      return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
    },
    async apply(plan) {
      return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
    },
    verify: () => verifyStub(id),
  };
}

/** detect()가 계약(PR-8: 던지지 않는다)을 어기고 동기적으로 throw하는 provider */
function throwingProvider(id: Provider['id']): Provider {
  return {
    id,
    dependsOn: [],
    detect(): Promise<Observed> {
      throw new Error(`${id} detect() exploded`);
    },
    plan(observed) {
      return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
    },
    async apply(plan) {
      return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
    },
    verify: () => verifyStub(id),
  };
}

/** detect()가 Promise를 reject하는 provider */
function rejectingProvider(id: Provider['id']): Provider {
  return {
    id,
    dependsOn: [],
    async detect(): Promise<Observed> {
      return Promise.reject(new Error(`${id} detect() rejected`));
    },
    plan(observed) {
      return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
    },
    async apply(plan) {
      return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
    },
    verify: () => verifyStub(id),
  };
}

describe('detectAll — timeout (§2.2 ⑤, N-3)', () => {
  it('resolves a hanging provider as unknown/DETECT_TIMEOUT within the configured timeout, not the default 5s', async () => {
    const timeoutMs = 50;
    const started = Date.now();
    const [result] = await detectAll([hangingProvider('otel')], baseCtx, { timeoutMs });
    const elapsed = Date.now() - started;

    expect(result).toBeDefined();
    expect(result!.status).toBe('unknown');
    expect(result!.code).toBe('MV_OTEL_DETECT_TIMEOUT');
    // 넉넉한 여유를 둬도 default(5000ms)보다 한참 짧아야 한다 — 실제로 타임아웃이 동작했다는 증거
    expect(elapsed).toBeLessThan(1000);
  });

  it('aborts the per-call signal on timeout so a well-behaved provider can react', async () => {
    let observedAborted = false;
    const provider: Provider = {
      id: 'github',
      dependsOn: [],
      detect(ctx) {
        return new Promise<Observed>((resolve) => {
          ctx.signal?.addEventListener('abort', () => {
            observedAborted = true;
            resolve({ providerId: 'github', status: 'unknown', code: 'MV_GITHUB_DETECT_TIMEOUT', message: 'aborted', observedAt: new Date().toISOString() });
          });
        });
      },
      plan(observed) {
        return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
      },
      async apply(plan) {
        return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
      },
      verify: () => verifyStub('github'),
    };

    await detectAll([provider], baseCtx, { timeoutMs: 30 });
    expect(observedAborted).toBe(true);
  });

  it('exports the §2.2 정본 5초 default when no override is given', () => {
    expect(DETECT_TIMEOUT_MS).toBe(5000);
  });
});

describe('detectAll — isolation (PR-8)', () => {
  it('a provider that throws synchronously does not affect other providers, and is reported as unknown', async () => {
    const results = await detectAll(
      [fastProvider('install'), throwingProvider('agent'), fastProvider('mcp')],
      baseCtx,
      { timeoutMs: 200 }
    );

    const byId = new Map(results.map((r) => [r.providerId, r]));
    expect(byId.get('install')?.status).toBe('ok');
    expect(byId.get('mcp')?.status).toBe('ok');
    expect(byId.get('agent')?.status).toBe('unknown');
    expect(byId.get('agent')?.code).toBe('MV_AGENT_DETECT_FAILED');
  });

  it('a provider whose detect() rejects does not affect other providers', async () => {
    const results = await detectAll([fastProvider('install'), rejectingProvider('cloudflare')], baseCtx, {
      timeoutMs: 200,
    });
    const byId = new Map(results.map((r) => [r.providerId, r]));
    expect(byId.get('install')?.status).toBe('ok');
    expect(byId.get('cloudflare')?.status).toBe('unknown');
    expect(byId.get('cloudflare')?.code).toBe('MV_CLOUDFLARE_DETECT_FAILED');
  });

  it('a hanging provider does not block or slow down other providers (concurrent, not sequential)', async () => {
    const started = Date.now();
    const results = await detectAll(
      [hangingProvider('otel'), fastProvider('install'), fastProvider('agent')],
      baseCtx,
      { timeoutMs: 60 }
    );
    const elapsed = Date.now() - started;

    const byId = new Map(results.map((r) => [r.providerId, r]));
    expect(byId.get('install')?.status).toBe('ok');
    expect(byId.get('agent')?.status).toBe('ok');
    expect(byId.get('otel')?.status).toBe('unknown');
    // 순차 실행이었다면 fast provider들이 hanging provider의 timeout을 기다렸어야 하므로,
    // 전체 소요시간이 timeout 한 번 분량에 가깝다는 것으로 동시 실행을 방증한다
    expect(elapsed).toBeLessThan(500);
  });

  it('returns results for every provider, preserving input order', async () => {
    const results = await detectAll([fastProvider('install'), fastProvider('mcp')], baseCtx, { timeoutMs: 200 });
    expect(results.map((r) => r.providerId)).toEqual(['install', 'mcp']);
  });
});
