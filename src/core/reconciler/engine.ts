// Reconciler engine — architecture.md §1.1(core/reconciler) · §1.2 · §2.2 활성화 시퀀스 ⑤.
//
// 이 슬라이스(W1)의 engine은 "detect() 오케스트레이션(격리 + 타임아웃)"만 구현한다.
// apply() 오케스트레이션(§1.2 "apply()는 engine.ts만 호출" 이중 검사, N-5 reconcile.lock)은
// 동의 게이트 본체(W3)·저널(W5)이 있어야 의미가 있어 이번 슬라이스에는 없다 — apply를
// 실제로 트리거하는 진입점(온보딩·대시보드·커맨드 팔레트, §2.2)도 아직 없다.

import type { DetectContext, Observed, Provider, ProviderId } from '../../providers/types.js';

/** §2.2 ⑤ "개별 타임아웃 5초" — 이 값이 정본이다 (PR-7 단일 정본 값) */
export const DETECT_TIMEOUT_MS = 5000;

export interface DetectAllOptions {
  /** 테스트에서만 오버라이드한다. 운영 코드는 기본값(DETECT_TIMEOUT_MS)을 쓴다 */
  readonly timeoutMs?: number;
}

function nowIso(): string {
  return new Date().toISOString();
}

function reasonCode(providerId: ProviderId, reason: string): string {
  return `MV_${providerId.toUpperCase()}_${reason}`;
}

function timeoutObserved(providerId: ProviderId, timeoutMs: number): Observed {
  return {
    providerId,
    status: 'unknown',
    code: reasonCode(providerId, 'DETECT_TIMEOUT'),
    message: `detect() timed out after ${timeoutMs}ms`,
    observedAt: nowIso(),
  };
}

function failedObserved(providerId: ProviderId, error: unknown): Observed {
  const message = error instanceof Error ? error.message : String(error);
  return {
    providerId,
    status: 'unknown',
    code: reasonCode(providerId, 'DETECT_FAILED'),
    message,
    detail: error instanceof Error ? undefined : error,
    observedAt: nowIso(),
  };
}

/**
 * provider 하나의 detect()를 격리 실행한다 (PR-8: 어떤 provider의 실패도 확장 전체를
 * 중단시키지 않는다). 두 가지 방어를 동시에 건다:
 *  ① try/catch — provider가 계약(§1.2 "던지지 않고 'unknown' 반환")을 어기고 throw해도 삼킨다
 *  ② 타임아웃 — provider가 영원히 resolve하지 않아도(N-3: `gh auth status`가 keyring
 *     프롬프트로 멈추는 사례) 5초 후 'unknown'으로 대체한다
 *
 * 타임아웃 시 `ctx.signal`을 abort한다 — 실제 하위 프로세스를 죽이는 것은 그 신호를
 * 구독하는 provider 구현체(W7+, `platform/exec.ts`)의 책임이며 engine은 신호만 보장한다.
 */
async function detectOneIsolated(
  provider: Provider,
  baseCtx: DetectContext,
  timeoutMs: number
): Promise<Observed> {
  const controller = new AbortController();
  // 호출자가 이미 abort된/구독 대상 신호를 넘겼다면 함께 전파한다
  baseCtx.signal?.addEventListener('abort', () => controller.abort(), { once: true });
  const ctx: DetectContext = { ...baseCtx, signal: controller.signal };

  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<Observed>((resolve) => {
    timer = setTimeout(() => {
      controller.abort();
      resolve(timeoutObserved(provider.id, timeoutMs));
    }, timeoutMs);
  });

  try {
    return await Promise.race([provider.detect(ctx), timeout]);
  } catch (error) {
    return failedObserved(provider.id, error);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

/**
 * 등록된 provider 전체에 대해 detect()를 동시에·격리해서 돌린다 (§2.2 ⑤ `Promise.allSettled`).
 * 반환 순서는 입력 `providers` 순서와 같다 — 호출자가 결과를 provider와 대응시키기 쉽게 하기
 * 위해서다(각 Observed에도 providerId가 있어 순서에 의존하지 않아도 무방하다).
 */
export async function detectAll(
  providers: readonly Provider[],
  ctx: DetectContext,
  options: DetectAllOptions = {}
): Promise<readonly Observed[]> {
  const timeoutMs = options.timeoutMs ?? DETECT_TIMEOUT_MS;
  const settled = await Promise.allSettled(
    providers.map((provider) => detectOneIsolated(provider, ctx, timeoutMs))
  );
  return settled.map((outcome, index) => {
    if (outcome.status === 'fulfilled') {
      return outcome.value;
    }
    // detectOneIsolated는 이미 내부에서 catch하므로 이 분기는 방어적 이중 보장(PR-8)일
    // 뿐이며 정상 경로에서는 도달하지 않는다.
    const provider = providers[index];
    return failedObserved(provider ? provider.id : ('unknown' as ProviderId), outcome.reason);
  });
}
