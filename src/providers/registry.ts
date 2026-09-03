// Provider 레지스트리 — architecture.md §1.1(providers/) · §1.2(`dependsOn`).
//
// 등록·조회 + dependsOn 위상 정렬만 담당한다. 실제 provider 구현체(install/agent/otel/
// github/cloudflare/mcp)는 W7~W10에서 이 레지스트리에 register()된다 — 이번 슬라이스에는
// 아무것도 등록하지 않은 빈 레지스트리와, 그것을 채울 계약(타입)만 존재한다.

import type { Provider, ProviderId } from './types.js';

export class DuplicateProviderError extends Error {
  constructor(id: ProviderId) {
    super(`Provider '${id}' is already registered`);
    this.name = 'DuplicateProviderError';
  }
}

export class UnknownDependencyError extends Error {
  constructor(id: ProviderId, missing: ProviderId) {
    super(`Provider '${id}' depends on unregistered provider '${missing}'`);
    this.name = 'UnknownDependencyError';
  }
}

export class DependencyCycleError extends Error {
  constructor(cycle: readonly ProviderId[]) {
    super(`Provider dependency cycle detected: ${cycle.join(' -> ')}`);
    this.name = 'DependencyCycleError';
  }
}

/**
 * 등록·조회 전용 레지스트리. 실행 순서 판단(위상 정렬)은 `topologicalOrder()`가 순수
 * 함수로 별도 제공한다 — 레지스트리 자체는 상태 보관 이상의 로직을 갖지 않는다(PR-6:
 * 상태를 모르면 추측해 적용하지 않는다 — 여기서는 "아직 없다"를 그대로 빈 배열로 둔다).
 */
export class ProviderRegistry {
  private readonly providers = new Map<ProviderId, Provider>();

  register(provider: Provider): void {
    if (this.providers.has(provider.id)) {
      throw new DuplicateProviderError(provider.id);
    }
    this.providers.set(provider.id, provider);
  }

  get(id: ProviderId): Provider | undefined {
    return this.providers.get(id);
  }

  has(id: ProviderId): boolean {
    return this.providers.has(id);
  }

  /** 등록 순서를 보장하지 않는다 — 실행 순서가 필요하면 `topologicalOrder()`를 쓴다 */
  list(): readonly Provider[] {
    return Array.from(this.providers.values());
  }

  size(): number {
    return this.providers.size;
  }
}

/**
 * `dependsOn`을 만족하는 실행 순서로 정렬한다 (§1.2 주석: "mcp는 agent 이후, agent는
 * install 이후"). fail-closed(PR-6)로 동작한다 — 알 수 없는 의존이나 순환은 조용히
 * 무시하지 않고 던진다.
 */
export function topologicalOrder(providers: readonly Provider[]): readonly Provider[] {
  const byId = new Map<ProviderId, Provider>(providers.map((p) => [p.id, p]));
  for (const provider of providers) {
    for (const dep of provider.dependsOn) {
      if (!byId.has(dep)) {
        throw new UnknownDependencyError(provider.id, dep);
      }
    }
  }

  const VISITING = 1;
  const VISITED = 2;
  const state = new Map<ProviderId, typeof VISITING | typeof VISITED>();
  const order: Provider[] = [];
  const path: ProviderId[] = [];

  function visit(provider: Provider): void {
    const mark = state.get(provider.id);
    if (mark === VISITED) return;
    if (mark === VISITING) {
      throw new DependencyCycleError([...path, provider.id]);
    }
    state.set(provider.id, VISITING);
    path.push(provider.id);
    for (const depId of provider.dependsOn) {
      const dep = byId.get(depId);
      if (dep) visit(dep);
    }
    path.pop();
    state.set(provider.id, VISITED);
    order.push(provider);
  }

  for (const provider of providers) {
    visit(provider);
  }

  return order;
}
