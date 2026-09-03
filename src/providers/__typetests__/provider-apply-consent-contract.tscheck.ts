// 타입 계약 증거 — architecture.md §1.2 / W1 완료 판정 #3.
//
// "apply()가 동의 토큰을 타입으로 요구한다"는 안전 성질을, 동의 토큰 없이 apply()를
// 호출하는 코드가 실제로 타입 검사에서 실패함을 보여 증명한다.
//
// 이 파일은 `pnpm run check-types`(tsc --noEmit)에서만 검사된다. **실행되지 않는다** —
// vitest는 파일명에 test/spec을 포함하지 않는 이 파일을 기본 include 패턴으로 줍지 않고,
// vitest.config.ts가 `__typetests__` 디렉터리를 명시적으로도 제외한다. `declare const`로
// 선언된 값은 런타임 실체가 없어(타입 전용) 이 파일을 실제로 실행하면 즉시 깨진다 — 그것이
// 의도다.
//
// 검증 방법: 아래 각 `@ts-expect-error` 줄에서 실제로 타입 오류가 나지 않으면(즉 동의 없이도
// apply()가 통과하면) TypeScript가 "Unused '@ts-expect-error' directive"로 빌드를 실패시킨다.
// 즉 이 파일이 tsc를 통과하는 것 자체가 계약이 지켜지고 있다는 증거다.

import type { ApplyContext, Plan, Provider } from '../types.js';
import type { ConsentToken } from '../../core/consent/types.js';

declare const provider: Provider;
declare const plan: Plan;
declare const ctx: ApplyContext;

const validConsent: ConsentToken = {
  providerId: provider.id,
  diffHash: plan.diffHash,
  changeIds: [],
  extensionVersion: '0.1.0',
  grantedAt: new Date(0).toISOString(),
  expiresAt: new Date(15 * 60 * 1000).toISOString(),
  nonce: 'nonce-value',
};

// 정상 호출 — 유효한 ConsentToken을 주면 컴파일된다 (양성 대조군)
void provider.apply(plan, validConsent, ctx);

// @ts-expect-error apply()는 consent 인자를 생략할 수 없다 — 필수 파라미터임을 증명한다
void provider.apply(plan, ctx);

// @ts-expect-error apply()는 undefined를 ConsentToken으로 받아들이지 않는다
void provider.apply(plan, undefined, ctx);

// @ts-expect-error ConsentToken의 필수 필드(diffHash·changeIds·expiresAt·nonce 등)가 없는
// 임의 객체는 consent로 통과할 수 없다 — "그럴듯한 아무 값"도 막는다
void provider.apply(plan, { providerId: provider.id }, ctx);
