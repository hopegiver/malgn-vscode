// install 에러 코드 어휘 — architecture.md §4.8.4·policy-contract.md §4 원문에 그대로
// 등장하는 "doc-exact" 상수(§0.5 공통 규약 `MV_<PROVIDER|AREA>_<REASON>`). W10(install
// provider 본체)이 아직 없어(이 슬라이스가 아님) 이 코드들을 실제로 `ApplyResult.code`에
// 싣는 provider 구현은 없지만, 저널·진단 리포트(W5)가 `lastInstallAttempt` 블록을 만들
// 때 "무엇을 구분해야 하는가"의 정본으로 이 상수를 참조해야 하고, W10이 apply()를
// 구현할 때 새로 이름을 짓지 않고 여기를 import하는 것이 계약이다.
//
// `MV_INSTALL_TARGET_UNVERIFIED`는 여기 없다 — plan() p0(§4.8.4)이 이미
// `src/core/policy/errors.ts`에 정의해 두었고(W2/W6), 그 파일이 정본이다. 이 파일은
// **나머지 9개**(apply()의 p1~p3·exec·verify 단계 + step 0.5 정책 재조회)만 채운다.
//
// docs 리터럴 대조(반환문에 명시할 근거): `grep -on "MV_INSTALL_[A-Z_]*"
// docs/architecture.md docs/policy-contract.md`로 확인한 전체 집합과 이 파일의 상수
// 이름이 1:1이다(`MV_INSTALL_TARGET_UNVERIFIED` 제외).

/** §4.8.4 p1 — 매니저 절대경로가 `allowedManagerPaths` 밖(blocked) */
export const MV_INSTALL_MANAGER_PATH_DENIED = 'MV_INSTALL_MANAGER_PATH_DENIED';

/** §4.8.4 p2 — artifactKind !== 'Binary'(승격이 필요해 실행하지 않음, blocked) */
export const MV_INSTALL_NEEDS_ELEVATION = 'MV_INSTALL_NEEDS_ELEVATION';

/** §4.8.4 p3·§3.5.3 — targetVersion이 compat 하한 미만(동의 화면 진입 전 안내 격하) */
export const MV_INSTALL_TARGET_BELOW_COMPAT = 'MV_INSTALL_TARGET_BELOW_COMPAT';

/** §4.8.4 apply step 0.5 — install.mode/killSwitch가 동의~실행 사이에 바뀌어
 * exec 이전에 즉시 중단(devops-review.md 2라운드 질문5 권고 반영, N-7과 동형) */
export const MV_INSTALL_POLICY_CHANGED = 'MV_INSTALL_POLICY_CHANGED';

/** §4.8.4 apply step 5 — 전체 10분 또는 무출력 60초 정체 타임아웃 초과("매니저가 멈춤").
 * devops-review.md 2라운드 Q4가 지적한 공백 — `MV_INSTALL_VERIFY_FAILED`("매니저가
 * 거부")와 반드시 분리해야 지원 인력이 사내망 프록시 차단 같은 "멈춤" 상황과 매니저의
 * "명시적 거부"를 구분할 수 있다(policy-contract.md §4 "에러 코드 구분"). */
export const MV_INSTALL_TIMEOUT = 'MV_INSTALL_TIMEOUT';

/** §4.8.4 apply step 6① — `<tool> --version` verify 실패("매니저가 거부·설치 실패") */
export const MV_INSTALL_VERIFY_FAILED = 'MV_INSTALL_VERIFY_FAILED';

/** §4.8.4 apply step 6② — macOS cask `codesign --verify` 불일치·미서명(blocked, 자동 제거 금지) */
export const MV_INSTALL_SIGNATURE_INVALID = 'MV_INSTALL_SIGNATURE_INVALID';

/** §4.8.4 apply step 6② — `codesign` 자체 부재(차단 아님, severity high) */
export const MV_INSTALL_SIGNATURE_UNCHECKED = 'MV_INSTALL_SIGNATURE_UNCHECKED';

/** §4.8.4·§4.8.7 — 패키지 매니저 자체가 PC에 없음(blocked + 안내 격하,
 * "자동설치가 100%가 아닌 유일한 구조적 구멍") */
export const MV_INSTALL_NO_PACKAGE_MANAGER = 'MV_INSTALL_NO_PACKAGE_MANAGER';

export type InstallErrorCode =
  | typeof MV_INSTALL_MANAGER_PATH_DENIED
  | typeof MV_INSTALL_NEEDS_ELEVATION
  | typeof MV_INSTALL_TARGET_BELOW_COMPAT
  | typeof MV_INSTALL_POLICY_CHANGED
  | typeof MV_INSTALL_TIMEOUT
  | typeof MV_INSTALL_VERIFY_FAILED
  | typeof MV_INSTALL_SIGNATURE_INVALID
  | typeof MV_INSTALL_SIGNATURE_UNCHECKED
  | typeof MV_INSTALL_NO_PACKAGE_MANAGER;

/**
 * exec 결과(§4.8.4 step 5~6)에서 "매니저가 멈췄다"와 "매니저가 거부했다"를 구분하는
 * 유일한 판정 함수(devops-review.md 2라운드 Q4 권고의 실행 지점) — W10의 apply()가
 * `ApplyResult.code`를 채울 때 이 함수를 호출하는 것이 계약이다. verify 단계(서명·
 * compat 재대조)는 이 함수의 책임 밖이다(exitCode/timedOut만으로 판단 가능한 구간만
 * 다룬다) — 그 구간은 §4.8.4 step 6이 별도로 처리한다.
 *
 * 순수 함수: 값이 없다(`exitCode: null`이고 `timedOut: false`)면 아직 종료되지 않았다는
 * 뜻이라 `null`(미분류)을 반환한다 — 호출자가 이미 종료된 프로세스만 넘긴다고 가정하지
 * 않는다.
 */
export function classifyInstallExecFailure(outcome: {
  readonly exitCode: number | null;
  readonly timedOut: boolean;
}): InstallErrorCode | null {
  if (outcome.timedOut) return MV_INSTALL_TIMEOUT;
  if (outcome.exitCode === 0) return null; // 성공 — 분류할 실패가 없다
  if (outcome.exitCode === null) return null; // 아직 종료되지 않음 — 호출자 책임 밖
  return MV_INSTALL_VERIFY_FAILED;
}
