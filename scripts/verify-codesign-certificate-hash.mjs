#!/usr/bin/env node
// SIGN-R2 CLI 재현 — docs/release-gates.md §7.6.2. `src/core/policy/codesignCertificateGuard.ts`의
// `assertCertificateHashMatches`와 같은 판정을 TypeScript를 실행하지 않는 서명 파이프라인
// 자리(devops의 codesign 후처리 스크립트 등)에서 CLI로 재현한다.
//
// [지금의 위치 — 조건부 대기] 이 스크립트를 호출하는 실제 codesign 파이프라인은 아직
// 없다(인증서 미발급). 그래서 지금은 어떤 npm/pnpm 라이프사이클도 이 스크립트를 부르지
// 않는다 — 그러나 그 부재가 "검사가 없다"를 뜻하지 않는다: `src/architecture-tests/
// codesignSigningWiring.test.ts`가 "codesign 호출이 패키징 파이프라인에 등장하는데 이
// 스크립트로의 배선이 없으면" CI를 실패시킨다(구조적 fail-closed, docs/release-gates.md
// §7.6.2 정직 표기 "SIGN-R2는 신설 대상"에 대한 응답).
//
// 사용법: node scripts/verify-codesign-certificate-hash.mjs --actual-hash=<sha256:...>
// (실제 파이프라인에서는 devops가 `codesign -dvvv --display <bundle>`로 뽑은 인증서
// 해시를 이 인자로 넘긴다 — 이 스크립트 자신은 `codesign`을 호출하지 않는다. 시스템
// 상태 변경 금지 원칙 아래 이 프로젝트는 실서명을 로컬에서 재현하지 않는다.)

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export class CertificateHashBaselineMissingError extends Error {}
export class CertificateHashMismatchError extends Error {}

/** src/core/policy/codesignCertificateGuard.ts와 판정이 동일해야 한다 — 정본은 그
 * TypeScript 파일이고, 여기는 TS 실행이 불가능한 자리를 위한 순수 JS 재구현이다.
 * 두 판정이 어긋나지 않는지는 codesignSigningWiring.test.ts가 병행 실행으로 대조한다. */
export function verifyCertificateHash(expectedHashSha256, actualHashSha256) {
  if (!expectedHashSha256 || expectedHashSha256.trim().length === 0) {
    throw new CertificateHashBaselineMissingError(
      'SIGN-R2: compat/codesign-cert.json의 expectedCertificateHashSha256이 설정되지 않았습니다 — ' +
        '최초 서명 인증서 발급 시 베이스라인을 확립하십시오 (docs/release-gates.md §7.6.2).'
    );
  }
  if (expectedHashSha256 !== actualHashSha256) {
    throw new CertificateHashMismatchError(
      `SIGN-R2: 서명 인증서 해시 불일치 — 기대 ${expectedHashSha256} / 실제 ${actualHashSha256}.`
    );
  }
}

export function parseActualHashArg(argv) {
  const arg = argv.find((a) => a.startsWith('--actual-hash='));
  return arg ? arg.slice('--actual-hash='.length) : null;
}

export function main(rootOverride, argv = process.argv.slice(2)) {
  const root = rootOverride ?? process.env.MALGN_GEN_SITE_ROOT ?? DEFAULT_ROOT;
  const certJsonPath = join(root, 'compat', 'codesign-cert.json');
  const cert = JSON.parse(readFileSync(certJsonPath, 'utf8'));

  const actualHash = parseActualHashArg(argv);
  if (!actualHash) {
    throw new Error('사용법: verify-codesign-certificate-hash.mjs --actual-hash=<sha256:...>');
  }

  verifyCertificateHash(cert.expectedCertificateHashSha256, actualHash);
  return { expected: cert.expectedCertificateHashSha256, actual: actualHash };
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  try {
    const result = main();
    console.log(`[verify-codesign-certificate-hash] OK — ${result.actual}`);
  } catch (error) {
    console.error(`[verify-codesign-certificate-hash] 서명 검증 실패(fail-closed): ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
