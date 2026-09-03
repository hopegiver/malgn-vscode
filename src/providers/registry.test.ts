import { describe, expect, it } from 'vitest';
import {
  DependencyCycleError,
  DuplicateProviderError,
  ProviderRegistry,
  UnknownDependencyError,
  topologicalOrder,
} from './registry.js';
import type { DetectContext, Observed, Provider } from './types.js';

function stubObserved(id: Provider['id']): Observed {
  return { providerId: id, status: 'ok', code: `MV_${id.toUpperCase()}_OK`, message: 'ok', observedAt: new Date().toISOString() };
}

function makeProvider(id: Provider['id'], dependsOn: readonly Provider['id'][] = []): Provider {
  return {
    id,
    dependsOn,
    async detect(_ctx: DetectContext) {
      return stubObserved(id);
    },
    plan(observed) {
      return { providerId: observed.providerId, changes: [], diffHash: 'stub' };
    },
    async apply(plan, _consent, _ctx) {
      return { providerId: plan.providerId, status: 'ok', code: 'MV_STUB_OK', message: 'ok', appliedChangeIds: [] };
    },
    async verify(_ctx) {
      return { providerId: id, status: 'ok', code: `MV_${id.toUpperCase()}_OK`, message: 'ok', verifiedAt: new Date().toISOString() };
    },
  };
}

describe('ProviderRegistry', () => {
  it('registers and retrieves providers by id', () => {
    const registry = new ProviderRegistry();
    const install = makeProvider('install');
    registry.register(install);

    expect(registry.get('install')).toBe(install);
    expect(registry.has('install')).toBe(true);
    expect(registry.has('agent')).toBe(false);
    expect(registry.size()).toBe(1);
  });

  it('rejects duplicate ids', () => {
    const registry = new ProviderRegistry();
    registry.register(makeProvider('install'));
    expect(() => registry.register(makeProvider('install'))).toThrow(DuplicateProviderError);
  });
});

describe('topologicalOrder', () => {
  it('orders agent after install and mcp after agent (§1.2 원문 예시)', () => {
    const install = makeProvider('install');
    const agent = makeProvider('agent', ['install']);
    const mcp = makeProvider('mcp', ['agent']);

    // 등록 순서를 일부러 뒤섞어도 결과는 의존 순서를 지켜야 한다
    const ordered = topologicalOrder([mcp, install, agent]);
    const ids = ordered.map((p) => p.id);

    expect(ids.indexOf('install')).toBeLessThan(ids.indexOf('agent'));
    expect(ids.indexOf('agent')).toBeLessThan(ids.indexOf('mcp'));
  });

  it('throws on unknown dependency (fail-closed, PR-6)', () => {
    const agent = makeProvider('agent', ['install']); // install 미등록
    expect(() => topologicalOrder([agent])).toThrow(UnknownDependencyError);
  });

  it('throws on dependency cycles', () => {
    const a = makeProvider('agent', ['mcp']);
    const b = makeProvider('mcp', ['agent']);
    expect(() => topologicalOrder([a, b])).toThrow(DependencyCycleError);
  });

  it('providers with no dependencies keep a stable, valid order', () => {
    const otel = makeProvider('otel');
    const github = makeProvider('github');
    const ordered = topologicalOrder([otel, github]);
    expect(ordered.map((p) => p.id).sort()).toEqual(['github', 'otel']);
  });
});
