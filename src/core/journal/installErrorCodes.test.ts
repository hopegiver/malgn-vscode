import { describe, expect, it } from 'vitest';
import {
  MV_INSTALL_MANAGER_PATH_DENIED,
  MV_INSTALL_NEEDS_ELEVATION,
  MV_INSTALL_NO_PACKAGE_MANAGER,
  MV_INSTALL_POLICY_CHANGED,
  MV_INSTALL_SIGNATURE_INVALID,
  MV_INSTALL_SIGNATURE_UNCHECKED,
  MV_INSTALL_TARGET_BELOW_COMPAT,
  MV_INSTALL_TIMEOUT,
  MV_INSTALL_VERIFY_FAILED,
  classifyInstallExecFailure,
} from './installErrorCodes.js';

// docs-exact 대조 — architecture.md·policy-contract.md에 리터럴로 등장하는 문자열과
// 정확히 같은지 고정한다(`grep -on "MV_INSTALL_[A-Z_]*" docs/architecture.md
// docs/policy-contract.md`로 확인한 집합, 반환문에 근거 명시).
describe('MV_INSTALL_* 코드 — docs 원문 리터럴과 정확히 일치(doc-exact)', () => {
  it.each([
    [MV_INSTALL_MANAGER_PATH_DENIED, 'MV_INSTALL_MANAGER_PATH_DENIED'],
    [MV_INSTALL_NEEDS_ELEVATION, 'MV_INSTALL_NEEDS_ELEVATION'],
    [MV_INSTALL_TARGET_BELOW_COMPAT, 'MV_INSTALL_TARGET_BELOW_COMPAT'],
    [MV_INSTALL_POLICY_CHANGED, 'MV_INSTALL_POLICY_CHANGED'],
    [MV_INSTALL_TIMEOUT, 'MV_INSTALL_TIMEOUT'],
    [MV_INSTALL_VERIFY_FAILED, 'MV_INSTALL_VERIFY_FAILED'],
    [MV_INSTALL_SIGNATURE_INVALID, 'MV_INSTALL_SIGNATURE_INVALID'],
    [MV_INSTALL_SIGNATURE_UNCHECKED, 'MV_INSTALL_SIGNATURE_UNCHECKED'],
    [MV_INSTALL_NO_PACKAGE_MANAGER, 'MV_INSTALL_NO_PACKAGE_MANAGER'],
  ])('%s === %s', (actual, expected) => {
    expect(actual).toBe(expected);
  });
});

// devops-review.md 2라운드 Q4 — "타임아웃 전용 에러코드가 없다... 매니저가 거부한 것과
// 매니저가 멈춘 것이 지원 인력 눈에는 구분되지 않는다"의 실행 지점.
describe('classifyInstallExecFailure — MV_INSTALL_TIMEOUT vs MV_INSTALL_VERIFY_FAILED 구분', () => {
  it('timedOut이면 exitCode와 무관하게 항상 MV_INSTALL_TIMEOUT — "매니저가 멈춤"', () => {
    expect(classifyInstallExecFailure({ exitCode: null, timedOut: true })).toBe(MV_INSTALL_TIMEOUT);
    expect(classifyInstallExecFailure({ exitCode: 1, timedOut: true })).toBe(MV_INSTALL_TIMEOUT);
  });

  it('timedOut이 아니고 exitCode !== 0이면 MV_INSTALL_VERIFY_FAILED — "매니저가 거부"', () => {
    expect(classifyInstallExecFailure({ exitCode: 1, timedOut: false })).toBe(MV_INSTALL_VERIFY_FAILED);
    expect(classifyInstallExecFailure({ exitCode: 127, timedOut: false })).toBe(MV_INSTALL_VERIFY_FAILED);
  });

  it('exitCode === 0이고 timedOut이 아니면 성공 — 분류할 실패가 없다(null)', () => {
    expect(classifyInstallExecFailure({ exitCode: 0, timedOut: false })).toBeNull();
  });

  it('아직 종료되지 않은 프로세스(exitCode null, timedOut false)는 미분류(null)', () => {
    expect(classifyInstallExecFailure({ exitCode: null, timedOut: false })).toBeNull();
  });
});
