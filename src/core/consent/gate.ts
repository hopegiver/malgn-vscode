// 동의 게이트 런타임 검증 — architecture.md §1.2 `gate.assertValid(plan, consent)` 정본.
//
// W1은 `Provider.apply()`가 `ConsentToken`을 **타입 수준**에서 요구하도록 시그니처를
// 확정했다(`src/providers/types.ts`). 그러나 타입은 "토큰을 받았는가"만 강제하고
// **"그 동의가 지금 실행하려는 plan에 대한 것인가"는 강제하지 못한다** — 그 런타임
// 재검증이 이 모듈이다. 검사 축 다섯은 §1.2 원문 순서를 그대로 따른다:
//   ① providerId 일치
//   ② diffHash === hash(plan) ← plan.diffHash 필드를 신뢰하지 않고 changes로부터
//      재계산해 비교한다(핵심 — plan.diffHash는 호출자가 잘못 채워 넣었을 수 있다)
//   ③ extensionVersion 일치
//   ④ now < expiresAt
//   ⑤ nonce 미소비 → 소비 처리(다른 넷을 모두 통과한 뒤에만 소비한다 — 실패한 시도가
//     nonce를 태워 정당한 재시도를 막지 않도록)
//
// **실패의 두 등급**(§1.2): ④(만료)는 정상 상황이라 `MV_CONSENT_EXPIRED`(`info`)로 분리하고,
// ①②③⑤(사고 신호)는 `MV_CONSENT_INVALID`(`high`)로 묶는다.
//
// apply() 오케스트레이션(engine이 provider.apply() 직전에 assertValid를 한 번 더 돌리는
// "이중 검사" 배선, §1.2)은 실제 apply 경로가 아직 없어(§11 W7이 신설 지점) 이번
// 슬라이스에는 없다 — 이 모듈은 그 배선이 호출할 단일 검증 함수만 제공한다. **주의**:
// 아래 구현은 성공 시 nonce를 소비하므로, 같은 토큰으로 `assertValid`를 두 번 연속 호출하면
// 두 번째 호출은 (합법적인 재검사라도) `MV_CONSENT_INVALID`로 실패한다 — W7이 "이중 검사"를
// 배선할 때는 engine이 **단일 호출 지점**이 되도록 설계해야 한다(반환문에 명시).

import type { Plan } from '../../providers/types.js';
import type { ConsentToken } from './types.js';
import { computeDiffHash } from '../reconciler/diffHash.js';
import { MV_CONSENT_EXPIRED, MV_CONSENT_INVALID } from './errors.js';
// PR-7 정본: compat/compatibility.json.extensionVersion == package.json.version
// (check1a-extensionVersionSync.ts가 CI에서 강제). 이 값이 "지금 실행 중인 확장의 버전"의
// 유일한 정본이라 여기서 그대로 재사용한다 — gen:site가 만드는 site 프로필 상수
// (`src/generated/siteConstants.ts`)까지 끌어올 필요가 없다(consent 검증은 사이트면과
// 무관하다).
import compatibilityRaw from '../../../compat/compatibility.json';

const CURRENT_EXTENSION_VERSION: string = compatibilityRaw.extensionVersion;

export type ConsentFailureSeverity = 'info' | 'high';

/**
 * assertValid가 검사 실패 시 던지는 에러. `code`는 §1.2가 정의한 두 문자열 중 하나이고
 * `severity`가 "정상 상황(만료)"과 "사고 신호(그 외 넷)"를 구분한다.
 */
export class ConsentGateError extends Error {
  readonly code: string;
  readonly severity: ConsentFailureSeverity;

  constructor(code: string, severity: ConsentFailureSeverity, message: string) {
    super(message);
    this.name = 'ConsentGateError';
    this.code = code;
    this.severity = severity;
    // TS 5.x + ES2022 target에서 Error 상속 시 프로토타입 체인 보정 (instanceof 안전성)
    Object.setPrototypeOf(this, ConsentGateError.prototype);
  }
}

export interface AssertValidOptions {
  /** 테스트 전용 — 운영 코드는 항상 실제 현재 시각을 쓴다 */
  readonly now?: Date;
  /** 테스트 전용 — 운영 코드는 항상 compat/compatibility.json(PR-7 정본)의 값을 쓴다 */
  readonly currentExtensionVersion?: string;
}

/** nonce는 프로세스 생애 동안 1회만 소비될 수 있다 (§1.2 ⑤). 확장 호스트 재시작마다 비워진다 —
 * 재시작을 넘어서는 nonce 재사용 방지가 필요하면 영속 저장이 필요하고 그것은 W5(저널)의
 * 책임 범위다(이번 슬라이스가 다루는 "그 세션 안에서 같은 토큰 두 번 apply 금지"에는
 * 이 정도로 충분하다 — 반환문에 명시). */
const consumedNonces = new Set<string>();

/**
 * architecture.md §1.2 `gate.assertValid(plan, consent)` — 유효하면 아무 것도 반환하지
 * 않고 조용히 통과한다(성공 시 nonce는 소비 처리됨). 유효하지 않으면 `ConsentGateError`를
 * 던진다. `plan`이 동의 시점과 한 비트라도 다르면(②) 반드시 거부된다.
 */
export function assertValid(plan: Plan, consent: ConsentToken, options: AssertValidOptions = {}): void {
  const now = options.now ?? new Date();
  const currentExtensionVersion = options.currentExtensionVersion ?? CURRENT_EXTENSION_VERSION;

  // ① providerId 일치
  if (consent.providerId !== plan.providerId) {
    throw new ConsentGateError(
      MV_CONSENT_INVALID,
      'high',
      `providerId 불일치: consent=${consent.providerId} plan=${plan.providerId}`
    );
  }

  // ② diffHash === hash(plan) — plan.diffHash 필드가 아니라 changes로부터 재계산해 비교한다.
  // (plan.diffHash 필드 자체를 신뢰하면 그 필드를 잘못/악의적으로 채운 호출자를 놓친다.)
  const recomputedDiffHash = computeDiffHash(plan.providerId, plan.changes);
  if (consent.diffHash !== recomputedDiffHash) {
    throw new ConsentGateError(
      MV_CONSENT_INVALID,
      'high',
      `diffHash 불일치 — plan이 동의 이후 변경됨: consent.diffHash=${consent.diffHash} hash(plan)=${recomputedDiffHash}`
    );
  }

  // ③ extensionVersion 일치
  if (consent.extensionVersion !== currentExtensionVersion) {
    throw new ConsentGateError(
      MV_CONSENT_INVALID,
      'high',
      `extensionVersion 불일치: consent=${consent.extensionVersion} current=${currentExtensionVersion}`
    );
  }

  // ④ now < expiresAt — 만료는 사고가 아니라 정상 상황(§1.2)
  const expiresAtMs = Date.parse(consent.expiresAt);
  if (!Number.isFinite(expiresAtMs) || now.getTime() >= expiresAtMs) {
    throw new ConsentGateError(
      MV_CONSENT_EXPIRED,
      'info',
      `동의 만료: expiresAt=${consent.expiresAt} now=${now.toISOString()}`
    );
  }

  // ⑤ nonce 미소비 확인 → 소비 처리. ①~④를 모두 통과한 뒤에만 소비한다.
  if (consumedNonces.has(consent.nonce)) {
    throw new ConsentGateError(MV_CONSENT_INVALID, 'high', `nonce 재사용 시도: ${consent.nonce}`);
  }
  consumedNonces.add(consent.nonce);
}

/** 테스트 전용 — 소비된 nonce 집합을 비운다. 운영 코드는 호출하지 않는다. */
export function resetConsentNonceStoreForTests(): void {
  consumedNonces.clear();
}
