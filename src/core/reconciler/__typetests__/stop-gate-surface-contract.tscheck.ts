// 타입 계약 증거 — architecture.md §3.6.1 / W4 완료 판정 #3(아키텍처 테스트).
//
// "STOPPABLE_SURFACES 밖은 정지시킬 수 없다"는 성질을, STOPPABLE_SURFACES 밖의 문자열을
// `evaluateStop()`의 `surface` 인자로 넣는 코드가 실제로 타입 검사에서 실패함을 보여
// 증명한다. `pnpm run check-types`(tsc --noEmit)에서만 검사되고 vitest는 실행하지 않는다
// (vitest.config.ts의 `**/__typetests__/**` 제외 — `provider-apply-consent-contract.tscheck.ts`와
// 같은 패턴).

import type { ProviderId } from '../../../providers/types.js';
import { evaluateStop, type StopSignals } from '../stopGate.js';

declare const providerId: ProviderId;
declare const signals: StopSignals;

// 정상 호출 — STOPPABLE_SURFACES 원소는 통과한다 (양성 대조군)
void evaluateStop('provider.apply', providerId, signals);
void evaluateStop('consent.issue', providerId, signals);

// @ts-expect-error 'provider.detect'는 STOPPABLE_SURFACES 밖이다 — detect()는 어떤 정지
// 경로에서도 정지될 수 없다(§3.6.1). 이 표면을 넣을 수 있다면 그 자체가 설계 위반이다.
void evaluateStop('provider.detect', providerId, signals);

// @ts-expect-error 'dashboard'는 STOPPABLE_SURFACES 밖이다 — 대시보드는 정지 불가 범위다.
void evaluateStop('dashboard', providerId, signals);

// @ts-expect-error 'diagnostics.report'는 STOPPABLE_SURFACES 밖이다 — 진단 리포트는 정지
// 불가 범위다.
void evaluateStop('diagnostics.report', providerId, signals);

// @ts-expect-error 'revert'(Malgn: 최근 변경 되돌리기, G-4)는 STOPPABLE_SURFACES 밖이다 —
// 되돌리기가 막히면 킬스위치를 해제할 다음 정책도 못 읽는 사태(자기잠금)로 이어진다.
void evaluateStop('revert', providerId, signals);

// @ts-expect-error 임의 문자열은 통과하지 않는다 — surface는 정확히 두 리터럴만 허용한다.
void evaluateStop('anything-else', providerId, signals);
