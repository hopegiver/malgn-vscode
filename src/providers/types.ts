// Provider 균일 인터페이스 — architecture.md §1.2 정본.
// 이 파일은 타입만 정의한다. 구체 provider 구현(install/agent/otel/github/cloudflare/mcp)은
// W7~W10에서 추가된다.

import type { ConsentToken } from '../core/consent/types.js';

/** provider id — kebab-case (architecture.md §0.5 공통 규약) */
export type ProviderId = 'install' | 'agent' | 'otel' | 'github' | 'cloudflare' | 'mcp';

/** provider 결과 공통 포장 (architecture.md §0.5 하단 "공통 규약") */
export type ProviderStatus = 'ok' | 'drift' | 'blocked' | 'unknown';

export interface ProviderResult {
  readonly status: ProviderStatus;
  /** `MV_<PROVIDER>_<REASON>` 형식. 표시 문자열(한국어)은 별도 매핑 테이블이 담당한다 */
  readonly code: string;
  readonly message: string;
  readonly detail?: unknown;
}

/** detect()의 결과. 부작용 0, 절대 throw하지 않고 실패도 'unknown'으로 표현한다 (PR-8) */
export interface Observed extends ProviderResult {
  readonly providerId: ProviderId;
  readonly observedAt: string; // ISO8601
}

/** apply() 직후 재검증 결과 */
export interface VerifyResult extends ProviderResult {
  readonly providerId: ProviderId;
  readonly verifiedAt: string; // ISO8601
}

/** apply()의 실행 결과 */
export interface ApplyResult extends ProviderResult {
  readonly providerId: ProviderId;
  readonly appliedChangeIds: readonly string[];
}

/**
 * 정책이 이 provider에 대해 원하는 상태의 조각.
 * 정책 스키마(리프 필드별 싱크 분류·검증 규칙)의 구체 형태는 W2(§3.7.2)가 확정한다.
 * PR-4(정책 권능 상한)에 따라 정책은 "제안"만 하므로 여기서는 provider별 임의 값을 담는
 * 자리만 잡아 둔다 — 실행 능력은 코드의 화이트리스트(각 provider의 plan())가 고정한다.
 */
export interface DesiredSlice {
  readonly providerId: ProviderId;
  readonly [key: string]: unknown;
}

export type ChangeKind = 'add' | 'update' | 'exec' | 'register' | 'install';
export type ChangeLevel = 'L0' | 'L1' | 'L2' | 'L2G';

/**
 * 단일 변경 단위 (architecture.md §1.2). 원문 코드블록에는 없지만 `id` 필드를 추가했다 —
 * ConsentToken.changeIds[]가 참조할 대상이 원문에 명시돼 있지 않아 채워 넣은 설계 갭이다.
 * `target`(파일 경로·키 등 사람이 읽는 라벨)은 provider 구현에 따라 plan 내에서
 * 유일성이 보장되지 않을 수 있어 별도 `id`를 두었다 — 보고 대상 (W1 반환문 참고).
 */
export interface Change {
  readonly id: string;
  readonly target: string;
  readonly kind: ChangeKind;
  /** L3는 plan() 단계에서 생성 불가 (PR-4) — 이 유니온에 L3를 넣지 않는 것 자체가 그 강제다 */
  readonly level: ChangeLevel;
  readonly before?: string;
  readonly after: string;
  readonly reversible: boolean;
  readonly rationale: string;
}

export interface Plan {
  readonly providerId: ProviderId;
  readonly changes: readonly Change[];
  readonly diffHash: string;
}

/** detect()/verify() 호출 시 공통 컨텍스트 */
export interface DetectContext {
  readonly workspaceTrusted: boolean;
  readonly remoteName?: string;
  /**
   * engine이 타임아웃 시 abort()를 호출한다 (§2.2 ⑤ 5초 타임아웃).
   * provider가 이 신호를 구독해 실제 프로세스를 kill하는 것은 provider 구현체(W7+)의 책임이다 —
   * engine은 신호만 보장하고 실제 kill 연결은 강제할 수 없다.
   */
  readonly signal?: AbortSignal;
}

/** apply() 호출 시 컨텍스트. 구체 필드(로거·exec 핸들 등)는 provider가 늘어나며 확정된다 */
export interface ApplyContext {
  readonly workspaceTrusted: boolean;
  readonly remoteName?: string;
  readonly signal?: AbortSignal;
}

/**
 * 균일 Provider 인터페이스 (architecture.md §1.2 — 이 구조의 핵심).
 *
 * `apply()`가 `ConsentToken`을 타입 수준에서 요구하는 것이 요점이다. 단, 타입은
 * "토큰을 받았는가"만 강제하고 "그 동의가 지금 실행할 plan에 대한 것인가"는 강제하지
 * 못한다 — 런타임 재검증(`gate.assertValid`)은 W3(동의 게이트 본체)의 책임이다.
 */
export interface Provider {
  readonly id: ProviderId;
  /** mcp는 agent 이후, agent는 install 이후 (§1.2 원문 주석) */
  readonly dependsOn: readonly ProviderId[];
  /** 부작용 0. 던지지 않고 'unknown'을 반환한다 (PR-8) — 다만 engine은 방어적으로 catch도 한다 */
  detect(ctx: DetectContext): Promise<Observed>;
  /** 순수 함수. 네트워크·fs 금지 (PR-2) */
  plan(observed: Observed, desired: DesiredSlice): Plan;
  /** ConsentToken 없이는 타입 검사에서 호출할 수 없다 — 이 시그니처 자체가 안전 성질이다 */
  apply(plan: Plan, consent: ConsentToken, ctx: ApplyContext): Promise<ApplyResult>;
  /** apply 직후 재검증 */
  verify(ctx: DetectContext): Promise<VerifyResult>;
}
