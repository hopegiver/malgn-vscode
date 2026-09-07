// SIGN-R2 대조 로직 단위 테스트 — docs/release-gates.md §7.6.2.
import { describe, expect, it } from 'vitest';
import {
  CertificateHashBaselineMissingError,
  CertificateHashMismatchError,
  assertCertificateHashMatches,
} from './codesignCertificateGuard.js';

describe('assertCertificateHashMatches (SIGN-R2)', () => {
  it('기대값이 null이면 베이스라인 미확립 에러로 던진다(조용한 통과 금지)', () => {
    expect(() => assertCertificateHashMatches(null, 'sha256:aaaa')).toThrow(CertificateHashBaselineMissingError);
  });

  it('기대값이 빈 문자열이어도 베이스라인 미확립으로 취급한다', () => {
    expect(() => assertCertificateHashMatches('', 'sha256:aaaa')).toThrow(CertificateHashBaselineMissingError);
  });

  it('기대값과 실제값이 다르면 불일치 에러로 던진다(fail-closed)', () => {
    expect(() => assertCertificateHashMatches('sha256:expected', 'sha256:actual')).toThrow(CertificateHashMismatchError);
  });

  it('기대값과 실제값이 같으면 통과한다(예외 없음)', () => {
    expect(() => assertCertificateHashMatches('sha256:same', 'sha256:same')).not.toThrow();
  });
});
