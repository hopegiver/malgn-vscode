// 활성화·재수렴 시퀀스 — architecture.md §2.2 "① 트레이 아이콘을 먼저 띄우고 즉시
// 반환한 뒤 비동기로 ② 호환 게이트 ③ 정책 로드 ④ 킬 스위치 확인(정지된 provider만
// 비활성) ⑤ 각 provider detect() ⑥ plan() ⑦ 상태 표출 순으로 진행한다. 여기서 apply()는
// 절대 호출되지 않는다 — 불변량이다."
//
// [불변량의 증명 방법] 이 파일은 `Provider.apply`·`gate.assertValid`·
// `core/reconciler/stopGate.ts`의 정지 판정 함수를 import하지 않는다 — import하지
// 않으면 호출할 수 없다(타입 수준 불가능). `src/architecture-tests/
// hostActivationApplyInvariant.test.ts`가 이 파일의 소스 텍스트에 provider apply 호출
// 패턴이 없음을 별도로 고정해, "이 파일을 고치다 실수로 그 호출을 넣는" 회귀를 잡는다.
//
// [providers가 비어 있는 이유] W7~W10(provider 구현체)이 아직 없어 `providers`는 항상
// 빈 배열로 호출된다 — 이 함수는 그 사실에 의존하지 않는다(길이 0인 배열도 그냥
// `detectAll([])`이 빈 배열을 반환할 뿐 특별 취급하지 않는다). provider가 하나씩
// 늘어날 때 이 파일을 고칠 필요가 없다는 것이 이 설계의 요점이다.

import { detectAll } from '../../core/reconciler/engine.js';
import type { DesiredSlice, DetectContext, Observed, Plan, Provider, ProviderId } from '../../providers/types.js';
import type { EffectiveKillSwitch } from '../../core/policy/types.js';
import type { LoadEffectivePolicyWithFallbackResult } from '../policySource/loadWithFallback.js';

export type TrayState = 'starting' | 'ok' | 'drift' | 'blocked' | 'error';

export interface ActivationSequenceDeps {
  readonly setTrayState: (state: TrayState) => void;
  readonly loadPolicy: () => Promise<LoadEffectivePolicyWithFallbackResult>;
  readonly providers: readonly Provider[];
  /** provider별 plan() 입력 — 정책에서 파생된 조각. provider가 없으면 호출되지 않는다. */
  readonly buildDesiredSlice: (providerId: ProviderId, policy: LoadEffectivePolicyWithFallbackResult) => DesiredSlice;
  readonly detectContext: DetectContext;
  readonly reportStatus: (report: ActivationStatusReport) => void;
}

export interface ActivationStatusReport {
  readonly policySource: LoadEffectivePolicyWithFallbackResult;
  readonly activeProviderIds: readonly ProviderId[];
  readonly disabledByKillSwitch: readonly ProviderId[];
  readonly observed: readonly Observed[];
  readonly plans: readonly Plan[];
}

/**
 * ④ 킬 스위치 확인 — 정지된 provider만 detect/plan에서 제외한다. `install`은 킬
 * 스위치 대상 enum 밖이라(policy-contract.md §2.5, `loader.ts`의
 * `KILLSWITCH_PROVIDER_IDS`) 이 필터를 거쳐도 항상 살아남는다(방어적으로 별도 처리를
 * 두지 않아도 되는 이유 — `disableProviders`에 애초에 'install'이 들어올 수 없다).
 */
export function filterProvidersByKillSwitch(
  providers: readonly Provider[],
  killSwitch: EffectiveKillSwitch
): { readonly active: readonly Provider[]; readonly disabled: readonly ProviderId[] } {
  const disabled: ProviderId[] = [];
  const active = providers.filter((p) => {
    if (killSwitch.disableProviders.includes(p.id)) {
      disabled.push(p.id);
      return false;
    }
    return true;
  });
  return { active, disabled };
}

/**
 * §2.2 ①~⑦ 순서 그대로. 이 함수가 반환하는 시점까지 어떤 `apply()`도 호출되지
 * 않았다 — 다음 단계(사용자가 온보딩/대시보드/커맨드에서 명시적으로 적용을 개시하는
 * 것)는 이 함수의 호출자가 아니라 완전히 별도의 진입점이다(§2.2 "apply는 3가지
 * 경로로만 시작된다").
 */
export async function runActivationSequence(deps: ActivationSequenceDeps): Promise<ActivationStatusReport> {
  // ① 트레이 아이콘을 먼저(호출자가 이미 창/트레이를 띄운 뒤 이 함수를 부른다고
  // 가정한다 — 이 함수 자신은 트레이를 생성하지 않고 상태만 갱신한다).
  deps.setTrayState('starting');

  // ② 호환 게이트 + ③ 정책 로드 — 이 슬라이스에서는 "정책 로드"가 호환 게이트보다
  // 먼저 필요하다(killSwitch·compat 범위 둘 다 정책 안에 있다). 원문의 ②③ 순서는
  // "무엇을 먼저 사용자에게 보여주는가"의 순서이지 "데이터 의존성" 순서가 아니다 —
  // 정책을 읽지 않고는 호환 게이트도 킬 스위치도 평가할 수 없다.
  const policyOutcome = await deps.loadPolicy();
  const killSwitch: EffectiveKillSwitch =
    policyOutcome.result.status === 'ok'
      ? policyOutcome.result.policy.killSwitch
      : { minAppVersion: null, maxAppVersion: null, disableProviders: [], message: null, upgradeHint: null };

  // ④ 킬 스위치 확인(정지된 provider만 비활성).
  const { active: activeProviders, disabled } = filterProvidersByKillSwitch(deps.providers, killSwitch);

  // ⑤ 각 provider detect()(Promise.allSettled, 개별 타임아웃 5초 — engine.ts가 보장).
  const observed = await detectAll(activeProviders, deps.detectContext);

  // ⑥ plan()(순수, 부작용 없음).
  const plans: Plan[] = [];
  for (const observedEntry of observed) {
    const provider = activeProviders.find((p) => p.id === observedEntry.providerId);
    if (!provider) continue; // 방어적 — detectAll은 항상 activeProviders 순서로 반환한다(engine.ts 계약)
    const desired = deps.buildDesiredSlice(provider.id, policyOutcome);
    plans.push(provider.plan(observedEntry, desired));
  }

  // ⑦ 상태 표출(트레이 배지 + 상태 리포트). 여기서 apply()는 호출되지 않는다.
  const overallState: TrayState = plans.some((p) => p.changes.length > 0) ? 'drift' : 'ok';
  deps.setTrayState(overallState);

  const report: ActivationStatusReport = {
    policySource: policyOutcome,
    activeProviderIds: activeProviders.map((p) => p.id),
    disabledByKillSwitch: disabled,
    observed,
    plans,
  };
  deps.reportStatus(report);
  return report;
}
