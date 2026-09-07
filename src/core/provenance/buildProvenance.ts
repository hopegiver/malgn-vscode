// NT-R22 (docs/release-gates.md §7.6.6) — "빌드 프로베넌스 레코드(siteProfile·커밋
// SHA·아티팩트 SHA-256·빌드 시각)를 릴리스마다 남기고 K-3로 서명한다."
//
// [지금 실제로 되는 것과 조건부 대기인 것 — 정직 표기]
// - **실제로 동작**: `siteProfile`·커밋 SHA·아티팩트(`dist/extension.cjs`) SHA-256·
//   빌드 시각을 담는 레코드 생성·검증. `pnpm run compile`이 실제로 그 아티팩트를
//   만드니(§7.6.6 원문이 지목한 대상 그대로) 이 레코드는 "언젠가"가 아니라 매 빌드
//   실제로 산출된다(`scripts/generate-build-provenance.mjs`가 `postcompile` 훅으로
//   배선되어 있다).
// - **조건부 대기(K-3 서명)**: `signature` 필드는 지금 항상 `null`이다 — 업데이트
//   서명 키(K-3, release-gates.md §7.6 표)가 아직 존재하지 않는다(키 발급 자체가
//   이 작업의 범위 밖이다). `signature: null`은 "서명 안 함"을 조용히 정상 상태로
//   두는 게 아니라 **명시적 미서명 표시**다 — K-3가 실제로 생겼는데 이 필드가 계속
//   null로 남으면 `src/architecture-tests/provenanceSigningWiring.test.ts`가 그
//   상태를 구조적으로 잡는다(신호가 있는데 배선이 없으면 실패).

import { createHash } from 'node:crypto';

export interface BuildProvenanceRecord {
  readonly recordVersion: 1;
  readonly siteProfile: 'site' | 'example';
  readonly commitSha: string;
  /** 저장소 루트 기준 상대경로(예: 'dist/extension.cjs') — 절대경로는 기기마다
   * 달라 레코드 자체가 이식 불가능해진다. */
  readonly artifactPath: string;
  readonly artifactSha256: string;
  readonly buildTimestamp: string;
  /** K-3(업데이트 서명 키) 서명 — 아직 없다. null이 아닌 값이 채워지는 순간부터
   * "서명된 프로베넌스"가 된다(§7.6.6 요구의 완성). */
  readonly signature: string | null;
}

export class InvalidProvenanceInputError extends Error {}

const SHA256_HEX_RE = /^[0-9a-f]{64}$/i;
const COMMIT_SHA_RE = /^[0-9a-f]{7,40}$/i;

export interface BuildProvenanceInput {
  readonly siteProfile: 'site' | 'example';
  readonly commitSha: string;
  readonly artifactPath: string;
  readonly artifactSha256: string;
  readonly buildTimestamp: string;
}

/** SHA-256 hex 다이제스트를 계산한다(콘텐츠 → `sha256:<hex>` 표기, 나머지 모듈과
 * 표기를 통일한다 — `compat/codesign-cert.json`·`SensitiveScanViolation` 등이 이미
 * 이 접두 표기를 쓴다). */
export function sha256Of(buffer: Buffer): string {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

/** 입력을 검증하고 레코드를 조립한다(fail-closed) — 형식이 어긋난 값으로 "그럴듯한"
 * 레코드를 만들지 않는다. `signature`는 호출자가 못 정한다(항상 null로 시작 — K-3
 * 서명은 별도 함수 `withSignature`로만 붙인다, 아래). */
export function buildProvenanceRecord(input: BuildProvenanceInput): BuildProvenanceRecord {
  if (input.siteProfile !== 'site' && input.siteProfile !== 'example') {
    throw new InvalidProvenanceInputError(`siteProfile이 'site'|'example'이 아닙니다: ${String(input.siteProfile)}`);
  }
  if (!COMMIT_SHA_RE.test(input.commitSha)) {
    throw new InvalidProvenanceInputError(`commitSha가 유효한 git SHA 형태가 아닙니다: ${input.commitSha}`);
  }
  if (input.artifactPath.trim().length === 0) {
    throw new InvalidProvenanceInputError('artifactPath가 비어 있습니다');
  }
  const normalizedSha = input.artifactSha256.startsWith('sha256:') ? input.artifactSha256.slice('sha256:'.length) : input.artifactSha256;
  if (!SHA256_HEX_RE.test(normalizedSha)) {
    throw new InvalidProvenanceInputError(`artifactSha256이 유효한 SHA-256 hex가 아닙니다: ${input.artifactSha256}`);
  }
  if (Number.isNaN(Date.parse(input.buildTimestamp))) {
    throw new InvalidProvenanceInputError(`buildTimestamp가 유효한 ISO 8601 문자열이 아닙니다: ${input.buildTimestamp}`);
  }

  return {
    recordVersion: 1,
    siteProfile: input.siteProfile,
    commitSha: input.commitSha,
    artifactPath: input.artifactPath,
    artifactSha256: `sha256:${normalizedSha}`,
    buildTimestamp: input.buildTimestamp,
    signature: null,
  };
}

/**
 * SIGN-R5(release-gates.md §7.6.3)가 안내 문서에 동봉할 재료 — 레코드가 주장하는
 * 아티팩트 해시가 실제로 지금 손에 든 아티팩트와 같은지 사후 대조한다. 다르면
 * "이 레코드는 이 아티팩트를 증명하지 않는다"는 뜻이다(재빌드로 내용이 바뀌었거나
 * 레코드가 낡았다).
 */
export function verifyArtifactMatchesRecord(record: BuildProvenanceRecord, actualArtifact: Buffer): boolean {
  return record.artifactSha256 === sha256Of(actualArtifact);
}
