// STOPPABLE_SURFACES + 정지 판정 — architecture.md §3.6.1(정본)·§3.5.3·§2.1·§3.7.3(N-2)
// 구현. W4 작업 지시 정본.
//
// [핵심 성질] "정지 방향만 가능하고 주입은 불가"다. §3.6.1 원문:
//   "정지 판정을 내리는 모든 경로가 이 상수를 참조한다. 이 표가 유일한 정본이다."
// 이 모듈이 그 "모든 경로"의 유일한 구현이다 — 킬 스위치(G-2) / 호환 게이트 하한 미달
// (§3.5.3) / 미신뢰 워크스페이스(§2.1) / 정책 체크아웃 14일 노후(§3.7.3·N-2) /
// 고위험 표면 변경 재동의(HRS 4, §7.3.1) 다섯 경로가 전부 `evaluateStop()` 하나를
// 통해서만 정지를 표현한다. HRS 4의 "표면 파일 변경 감지" 자체(무엇이 바뀌었는지 판단)는
// W7 소관이라 여기 없다 — 이 모듈은 그 판정 결과(불리언 신호)가 이 상수를 통해서만
// 정지로 이어지도록 배선만 제공한다(작업 지시 "W4는 STOPPABLE_SURFACES가 그 경로를
// 포함하도록만 하십시오").
//
// [주입 불가 보장] `evaluateStop()`의 반환 타입(`StopDecision`)은 `{stopped, reasons}`만
// 갖는다 — 어떤 입력을 넣어도 이 함수가 새 Change·새 target·새 provider 대상을 만들어낼
// 수 없다(타입 자체가 생성 능력을 갖지 않는다). 이 함수는 순수하고(네트워크·fs 없음)
// 상태를 갖지 않는다 — 매 호출이 입력 신호만으로 결정된다.
//
// [정지 불가 범위 — install 예외] 작업 지시 원문: "정지되는 것은 install 외 provider의
// apply()와 신규 동의 발급뿐이다." install provider의 apply는 하한 미달을 해소할
// **유일한 경로**이므로(§3.5.3) 5개 정지 경로 **전부**에서 예외다. §2.1 원문 표는
// "금지: 모든 apply()"라고만 적어 install 예외를 명시하지 않지만, §3.6.1이 "이 표가
// 유일한 정본"이라고 선언하고 이 작업 지시가 install 예외를 5개 경로 전체에 걸어
// 명시했으므로 그 판단을 따른다 — §2.1 문면과의 긴장은 반환문에 별도 보고한다(문서는
// 고치지 않는다).
//
// [HRS 4는 provider.apply만 정지한다] STOPPABLE_SURFACES 두 값 중 `consent.issue`까지
// HRS 4가 막으면 재동의(그 자체가 HRS 4의 복구 경로)를 발급할 수단이 없어진다 —
// "정지만 가능하고 주입은 불가"의 취지를 HRS 4에 적용하면 "재동의를 받을 통로까지
// 막지 않는다"가 나온다(§3.7.4 "PR-9의 비대칭을 전파 속도 축에 적용"과 같은 논리를
// 여기서는 "복구 경로 축"에 적용한 것). 다른 네 경로(킬 스위치·호환 게이트·미신뢰
// 워크스페이스·정책 노후)는 두 표면을 함께 정지시킨다 — N-2 표(§2.5)가 "apply()와
// 신규 동의 발급만 멈추고"라고 명시한 것과 §3.6.1이 이전 세 표에 같은 어휘가
// "복붙"돼 있었다고 서술한 것으로 보아 네 경로가 원래 동일한 두 표면 벡터를 썼다고
// 판단한다. 이 판단 근거를 반환문에 명시한다.

import type { ProviderId } from '../../providers/types.js';
import type { EffectiveKillSwitch } from '../policy/types.js';
import { compareVersions, parseRange, parseVersion } from '../policy/semver.js';

// ---------------------------------------------------------------------------
// STOPPABLE_SURFACES — §3.6.1 정본. 이 배열이 "정지될 수 있는 대상"의 전체 집합이다.
// ---------------------------------------------------------------------------

/**
 * 정지 판정을 내리는 모든 경로가 참조하는 유일한 정본(§3.6.1 원문 그대로).
 * 이 배열 밖은 어떤 정지 경로에서도 정지되지 않는다:
 *   detect() · 대시보드 · 진단 리포트 · Malgn:최근 변경 되돌리기(G-4) · 정책 재로드 · 확장 활성화
 * `evaluateStop()`의 `surface` 매개변수 타입이 이 배열의 원소로 고정되므로, 이 상수 밖의
 * 문자열은 애초에 컴파일되지 않는다 — "코드 상수 밖을 정지시키는 코드"가 타입 수준에서
 * 성립하지 않는다.
 */
export const STOPPABLE_SURFACES = ['provider.apply', 'consent.issue'] as const;

export type StoppableSurface = (typeof STOPPABLE_SURFACES)[number];

// ---------------------------------------------------------------------------
// 정지 사유 — 5개 정지 경로와 1:1 대응
// ---------------------------------------------------------------------------

export const MV_STOP_KILL_SWITCH = 'MV_STOP_KILL_SWITCH';
export const MV_STOP_COMPAT_GATE_BELOW_MINIMUM = 'MV_STOP_COMPAT_GATE_BELOW_MINIMUM';
export const MV_STOP_UNTRUSTED_WORKSPACE = 'MV_STOP_UNTRUSTED_WORKSPACE';
export const MV_STOP_POLICY_CHECKOUT_STALE = 'MV_STOP_POLICY_CHECKOUT_STALE';
export const MV_STOP_HRS4_RECONSENT_REQUIRED = 'MV_STOP_HRS4_RECONSENT_REQUIRED';

export type StopReasonCode =
  | typeof MV_STOP_KILL_SWITCH
  | typeof MV_STOP_COMPAT_GATE_BELOW_MINIMUM
  | typeof MV_STOP_UNTRUSTED_WORKSPACE
  | typeof MV_STOP_POLICY_CHECKOUT_STALE
  | typeof MV_STOP_HRS4_RECONSENT_REQUIRED;

export interface StopReason {
  readonly code: StopReasonCode;
  readonly message: string;
}

// ---------------------------------------------------------------------------
// 입력 신호 — 5개 정지 경로 각각의 "이미 계산된" 판정 결과.
// 각 신호의 *계산*(실제 claude --version 실행, 워크스페이스 신뢰 조회, 체크아웃 mtime
// 조회, HRS4 표면 diff 판정)은 이 모듈의 책임이 아니다 — 이 모듈은 신호를 받아
// STOPPABLE_SURFACES를 통해서만 정지로 번역하는 유일한 지점이라는 것이 W4의 범위다.
// ---------------------------------------------------------------------------

export interface StopSignals {
  /** G-2 킬 스위치. `killSwitch.disableProviders[]`는 정책 로더 단계에서 이미
   * `install`을 걸러낸 값이지만(W2, KILLSWITCH_PROVIDER_IDS enum), 이 함수는 그 계층이
   * 뚫리는 경우까지 대비해 `providerId === 'install'`을 별도로 재확인한다(아래
   * `evaluateStop` 진입부 — 방어심층). */
  readonly killSwitch: EffectiveKillSwitch;
  /** compat/compatibility.json.extensionVersion(PR-7 정본)과 같은 값이어야 한다 —
   * 호출자가 gate.assertValid와 동일한 정본을 재사용한다. */
  readonly currentExtensionVersion: string;
  /** §3.5.3 — claudeCode 하한 미달로 호환 게이트가 blocked인가. */
  readonly compatGateBelowMinimum: boolean;
  /** §2.1 — untrustedWorkspaces.supported:"limited" 조회 결과 */
  readonly workspaceTrusted: boolean;
  /** §3.7.3·N-2 — 정책 체크아웃이 POLICY_CHECKOUT_STALE_DAYS 이상 미갱신 */
  readonly policyCheckoutStale: boolean;
  /** §7.3.1 HRS 4 — 고위험 표면 변경이 감지되어 재동의가 필요한가. 판정 로직은 W7. */
  readonly hrs4ReconsentRequired: boolean;
}

export interface StopDecision {
  readonly stopped: boolean;
  readonly reasons: readonly StopReason[];
}

/** 정지 불가 범위 ① — 하한 미달을 해소할 유일한 경로(§3.5.3). 5개 정지 경로 전부에서
 * 예외다(작업 지시 "정지되는 것은 install 외 provider의 apply()와 신규 동의 발급뿐"). */
const NEVER_STOPPABLE_PROVIDER: ProviderId = 'install';

function killSwitchStops(
  providerId: ProviderId,
  killSwitch: EffectiveKillSwitch,
  currentExtensionVersion: string
): boolean {
  if (killSwitch.disableProviders.includes(providerId)) return true;

  const current = parseVersion(currentExtensionVersion);
  if (!current) return false; // 파싱 불가한 현재 버전은 이 함수 책임 밖 — fail-open하지 않되 여기서는 판단 보류

  if (killSwitch.minExtensionVersion) {
    const min = parseVersion(killSwitch.minExtensionVersion);
    if (min && compareVersions(current, min) < 0) return true;
  }
  if (killSwitch.maxExtensionVersion) {
    const max = parseVersion(killSwitch.maxExtensionVersion);
    if (max && compareVersions(current, max) > 0) return true;
  }
  return false;
}

/**
 * 정지 판정 — 유일한 정본 함수. `surface`가 `STOPPABLE_SURFACES` 밖의 값이면 TS가
 * 컴파일 타임에 거부한다(`StoppableSurface` 유니온 밖의 문자열을 넣을 수 없다).
 *
 * install provider는 어떤 신호가 들어와도 정지되지 않는다(정지 불가 범위 ①) — 이 예외를
 * 신호마다 개별 처리하지 않고 함수 진입부 한 곳에서 처리한다. "5개 신호 중 하나가 install
 * 예외를 빠뜨린다"는 실수가 구조적으로 발생할 수 없다(M-13이 지적한 결함의 재발 방지).
 */
export function evaluateStop(
  surface: StoppableSurface,
  providerId: ProviderId,
  signals: StopSignals
): StopDecision {
  if (providerId === NEVER_STOPPABLE_PROVIDER) {
    return { stopped: false, reasons: [] };
  }

  const reasons: StopReason[] = [];

  // ① 킬 스위치(G-2) — provider.apply·consent.issue 둘 다 정지
  if (killSwitchStops(providerId, signals.killSwitch, signals.currentExtensionVersion)) {
    reasons.push({
      code: MV_STOP_KILL_SWITCH,
      message: signals.killSwitch.message ?? '킬 스위치로 정지되었습니다',
    });
  }

  // ② 호환 게이트 하한 미달(§3.5.3) — provider.apply·consent.issue 둘 다 정지
  //    (install은 위에서 이미 반환됐으므로 여기 도달한 시점에 install일 수 없다 — §3.5.3의
  //    "install provider는 예외적으로 계속 동작"이 이 함수 진입부의 이른 반환으로 이미 충족됨)
  if (signals.compatGateBelowMinimum) {
    reasons.push({
      code: MV_STOP_COMPAT_GATE_BELOW_MINIMUM,
      message: '호환 하한 미달 — Claude Code 업데이트가 필요합니다',
    });
  }

  // ③ 미신뢰 워크스페이스(§2.1) — provider.apply·consent.issue 둘 다 정지
  if (!signals.workspaceTrusted) {
    reasons.push({
      code: MV_STOP_UNTRUSTED_WORKSPACE,
      message: '미신뢰 워크스페이스에서는 적용할 수 없습니다',
    });
  }

  // ④ 정책 체크아웃 14일 노후(§3.7.3·N-2) — provider.apply·consent.issue 둘 다 정지.
  //    detect·대시보드·진단·되돌리기·정책 재로드는 이 함수가 애초에 다루지 않는 표면이라
  //    (STOPPABLE_SURFACES 밖) 여기서 멈출 방법이 없다 — "구조적으로 막지 못한다".
  if (signals.policyCheckoutStale) {
    reasons.push({
      code: MV_STOP_POLICY_CHECKOUT_STALE,
      message: '오래된 정책으로 PC를 바꾸지 않습니다 — 진단과 되돌리기는 계속 쓸 수 있습니다',
    });
  }

  // ⑤ HRS 4 재동의(§7.3.1) — provider.apply만 정지한다(위 모듈 주석 "HRS 4는
  //    provider.apply만 정지한다" 참고). consent.issue까지 막으면 재동의 자체가 불가능해진다.
  if (signals.hrs4ReconsentRequired && surface === 'provider.apply') {
    reasons.push({
      code: MV_STOP_HRS4_RECONSENT_REQUIRED,
      message: '고위험 표면 변경이 감지되어 재동의가 필요합니다',
    });
  }

  return { stopped: reasons.length > 0, reasons };
}

// ---------------------------------------------------------------------------
// 개별 신호 계산 헬퍼 — 순수 함수. 값을 어디서 읽어오는지(fs mtime·claude --version 실행 등)는
// 호출자(향후 §3.7.3 정책 소스 폴백 로직 · §2.2 호환 게이트 배선)의 책임이고, 여기서는
// "판정 계산"만 제공해 그 값이 여러 곳에서 각각 재계산되며 임계값이 흩어지는 것을 막는다
// (안전 임계값은 정본 하나만 — 이 값을 바꾸면 architecture.md §3.7.3·§3.5.3도 함께 확인).
// ---------------------------------------------------------------------------

/** §3.7.3·N-2 — "14일 이상 미갱신"의 단일 정본. 이 값을 바꾸면 반드시 함께 확인할 것:
 * architecture.md §3.7.3 표 · §11 W4 설명. */
export const POLICY_CHECKOUT_STALE_DAYS = 14;

/**
 * N-2 판정의 순수 계산부. 실제 "체크아웃이 마지막으로 갱신된 시각"을 얻는 것(파일시스템
 * mtime 조회 등)은 이 함수 밖이다 — 정책 소스 폴백 로직(§3.7.3)이 아직 없어(이 슬라이스가
 * 아님) 그 배선은 후속 슬라이스가 맡는다.
 */
export function isPolicyCheckoutStale(lastCheckoutUpdatedAt: Date, now: Date = new Date()): boolean {
  const ageMs = now.getTime() - lastCheckoutUpdatedAt.getTime();
  return ageMs >= POLICY_CHECKOUT_STALE_DAYS * 24 * 60 * 60 * 1000;
}

/**
 * §3.5.3 호환 게이트 판정의 순수 계산부 — "실측 claudeCode 버전이 EffectivePolicy.compat.claudeCode
 * (이미 PR-9로 좁혀진 범위)의 하한보다 낮은가"만 계산한다. 실제 `claude --version` 실행은
 * 이 함수 밖(provider detect() 몫)이다. 파싱 불가한 입력은 이 함수의 책임 밖으로 보고
 * `false`를 반환한다 — 상위 호출자가 이미 다른 경로(§3.5 compat 검증)에서 유효성을
 * 보장한 값을 넘긴다고 가정한다.
 */
export function isBelowCompatMinimum(actualClaudeCodeVersion: string, effectiveClaudeCodeRange: string): boolean {
  const actual = parseVersion(actualClaudeCodeVersion);
  const range = parseRange(effectiveClaudeCodeRange);
  if (!actual || !range) return false;
  return compareVersions(actual, range.lower.version) < 0;
}
