// apply 오케스트레이션 — architecture.md §1.2 "apply() 오케스트레이션은 engine이 provider.apply()
// 직전에 assertValid를 한 번 더 돌리는 이중 검사" · §2.2 "apply는 3가지 경로로만 시작된다"의
// 그 시작 경로 하나(사람이 동의 화면에서 승인)가 실제로 수렴하는 자리. `core/reconciler/
// engine.ts`(§1.1) 원문 주석이 "apply() 오케스트레이션은 W7이 신설 지점"이라 명시한 그
// 슬라이스가 이 파일이다.
//
// [단일 호출 지점] `gate.assertValid`는 이 함수 안에서 **정확히 한 번**만 호출된다
// (`provider.apply()` 직전) — `gate.ts` 원문 주석 "같은 토큰으로 두 번 연속 호출하면
// 두 번째는 실패한다"를 피하려면 호출자가 단일 지점이어야 한다는 요구를 그대로 지킨다.
// "이중 검사"라는 표현은 assertValid를 두 번 부른다는 뜻이 아니라, **정지 판정
// (evaluateStop)을 두 표면(`consent.issue`·`provider.apply`)에 대해 각각** 돈다는
// 뜻이다(`stopGate.ts` 원문 주석과 일치) — 그래서 이 함수는 `evaluateStop`을 두 번,
// `assertValid`를 한 번 부른다.
//
// [순서] ① plan이 비어 있으면 즉시 종료(적용할 것이 없다) ② 정지 판정(consent.issue) —
// 동의 화면 자체를 띄우지 않는다(킬 스위치 등으로 이미 멈췄으면 동의를 구하는 것도
// 무의미하다) ③ 사람의 승인(UI) ④ 정지 판정(provider.apply) — ②와 ③ 사이에 상태가
// 바뀌었을 수 있다 ⑤ 동의 토큰 발급 ⑥ `assertValid`(유일한 호출 지점) ⑦ `provider.apply()`
// ⑧ 저널 기록 ⑨ `provider.verify()`.

import { evaluateStop, type StopReason, type StopSignals, type StoppableSurface } from './stopGate.js';
import { assertValid, ConsentGateError } from '../consent/gate.js';
import { issueConsentToken } from '../consent/issueToken.js';
import type { ApplyContext, ApplyResult, Plan, Provider, VerifyResult } from '../../providers/types.js';

export interface ChangeRecordEntry {
  readonly ts: string;
  readonly provider: Plan['providerId'];
  readonly changes: Plan['changes'];
  readonly backupPath: string | null;
  readonly diffHash: string;
}

export interface ApplyOrchestratorDeps {
  readonly provider: Provider;
  readonly plan: Plan;
  readonly extensionVersion: string;
  readonly stopSignals: StopSignals;
  readonly applyContext: ApplyContext;
  /** 동의 화면 UI — plan을 보여주고 승인/거부를 받는다(실제 구현은 Electron dialog,
   * 호스트 어댑터가 DI). */
  readonly requestApproval: (plan: Plan) => Promise<boolean>;
  /** §4.5 step⑥ — 저널 기록. 실제 구현은 `JournalStore.appendChange`. */
  readonly recordChange: (entry: ChangeRecordEntry) => Promise<void>;
  /** §1.2 "①②③⑤는 MV_CONSENT_INVALID(high)... 로그·진단 리포트에 남긴다" — 사고 신호만
   * 기록한다(MV_CONSENT_EXPIRED는 넘기지 않는다, 호출자가 이미 그 구분을 한다). */
  readonly recordConsentFailure?: (error: ConsentGateError) => Promise<void>;
  /** 테스트 전용 — 운영 코드는 항상 현재 시각을 쓴다 */
  readonly now?: () => Date;
}

export type ApplyOrchestratorOutcome =
  | { readonly kind: 'no-changes' }
  | { readonly kind: 'stopped'; readonly surface: StoppableSurface; readonly reasons: readonly StopReason[] }
  | { readonly kind: 'rejected-by-user' }
  | { readonly kind: 'consent-invalid'; readonly error: ConsentGateError }
  | { readonly kind: 'applied'; readonly result: ApplyResult; readonly verify: VerifyResult };

export async function runApplyWithConsent(deps: ApplyOrchestratorDeps): Promise<ApplyOrchestratorOutcome> {
  if (deps.plan.changes.length === 0) {
    return { kind: 'no-changes' };
  }

  const providerId = deps.plan.providerId;

  const consentIssueStop = evaluateStop('consent.issue', providerId, deps.stopSignals);
  if (consentIssueStop.stopped) {
    return { kind: 'stopped', surface: 'consent.issue', reasons: consentIssueStop.reasons };
  }

  const approved = await deps.requestApproval(deps.plan);
  if (!approved) {
    return { kind: 'rejected-by-user' };
  }

  const providerApplyStop = evaluateStop('provider.apply', providerId, deps.stopSignals);
  if (providerApplyStop.stopped) {
    return { kind: 'stopped', surface: 'provider.apply', reasons: providerApplyStop.reasons };
  }

  const now = deps.now ? deps.now() : new Date();
  const token = issueConsentToken(deps.plan, deps.extensionVersion, { now });

  try {
    // 단일 호출 지점(위 모듈 주석 참고) — 이 줄 밖에서는 이 plan/token 쌍에 대해
    // assertValid를 다시 부르지 않는다.
    assertValid(deps.plan, token, { now, currentExtensionVersion: deps.extensionVersion });
  } catch (error) {
    if (error instanceof ConsentGateError) {
      if (deps.recordConsentFailure && error.severity === 'high') {
        await deps.recordConsentFailure(error);
      }
      return { kind: 'consent-invalid', error };
    }
    throw error;
  }

  const result = await deps.provider.apply(deps.plan, token, deps.applyContext);

  await deps.recordChange({
    ts: now.toISOString(),
    provider: providerId,
    changes: deps.plan.changes,
    backupPath: null,
    diffHash: token.diffHash,
  });

  const verify = await deps.provider.verify(deps.applyContext);

  return { kind: 'applied', result, verify };
}
