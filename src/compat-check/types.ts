// compat:check 공통 타입 — docs/policy-contract.md §6 "검사 3중" (a) 차단성
// `pnpm compat:check`가 강제하는 검사 ①~⑧의 결과 형태.
//
// 각 검사는 순수 함수로 구현한다(파일 읽기는 runner.ts/각 check가 직접 하되, 로직
// 자체는 입력을 받아 위반 목록을 돌려주는 형태를 유지해 테스트가 실제 파일 시스템
// 상태에 얽매이지 않게 한다 — 특히 검사 ⑧은 회귀 테스트 4종이 완전히 인메모리로
// 동작해야 한다는 요구(작업 지시 "증거 파일을 실제로 만들지 마십시오")를 만족해야 한다).

/** 위반 1건. `ref`는 이 위반이 강제하는 문서 근거(PR 번호·검사 번호 등)를 담아
 * CI 로그만 보고도 "정책-contract.md의 어느 항목을 어겼는지" 추적 가능하게 한다. */
export interface CheckViolation {
  readonly ref: string;
  readonly message: string;
}

export interface CheckResult {
  /** 원문자 번호(①~⑧) — policy-contract.md §6 "검사 3중" (a)의 항목 번호와 정확히 일치 */
  readonly id: string;
  /** 사람이 읽는 한 줄 요약(어떤 규칙을 강제하는지) */
  readonly label: string;
  readonly violations: readonly CheckViolation[];
}

export function ok(id: string, label: string): CheckResult {
  return { id, label, violations: [] };
}

export function fail(id: string, label: string, violations: readonly CheckViolation[]): CheckResult {
  return { id, label, violations };
}
