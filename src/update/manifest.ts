// U-1 (docs/architecture.md §3.6.2) — 업데이트 매니페스트의 형태와 정규화(서명 대상
// 바이트 조립). 낱말은 "정책"이 아니라 "매니페스트"다 — architecture.md §3.7이 그
// 구분을 요구한 것과 같은 이유로(§3.7 도입부: 다른 매니페스트들과 혼동 방지), 이
// 채널의 문서는 서명 없는 파일이 아니라 **서명된** 배포 선언이다.

const SHA256_RE = /^sha256:[0-9a-f]{64}$/i;

export interface UnsignedUpdateManifest {
  readonly schemaVersion: 1;
  /** X.Y.Z — `version.ts`의 `parseUpdateVersion`이 인정하는 형태만 유효하다. */
  readonly version: string;
  readonly artifactUrl: string;
  /** `sha256:<64-hex>` 표기 — `src/core/provenance/buildProvenance.ts`의 `sha256Of`와
   * 표기를 통일한다(다른 모듈이지만 사람이 눈으로 대조할 때 형태가 같아야 한다). */
  readonly artifactSha256: string;
  /** ISO 8601. */
  readonly publishedAt: string;
}

export interface SignedUpdateManifest extends UnsignedUpdateManifest {
  /** base64 — ed25519 서명, 대상 바이트는 `canonicalizeUpdateManifest`가 만든다. */
  readonly signature: string;
}

/**
 * [브랜드 타입] `verifyUpdateManifest`(manifestVerifier.ts)를 통과한 값만 이 타입을
 * 가질 수 있다 — 유일한 생성자는 그 함수 하나다. 정책 모듈 디렉터리의
 * `siteProfileGuard.ts`가 쓰는 것과 같은 계약 형태(값이 아니라 "어떤 함수를
 * 거쳤는가"로 타입을 좁힌다)이되, 이 파일은 그 모듈을 import하지 않는다(AT-U1) —
 * 형태만 같은 독립 구현이다.
 */
export type VerifiedUpdateManifest = SignedUpdateManifest & { readonly __verifiedUpdateManifestBrand: unique symbol };

export class UpdateManifestShapeInvalidError extends Error {}

/**
 * 서명 대상 바이트를 만든다. 필드 순서를 이 함수가 고정한다(같은 내용은 항상 같은
 * 바이트를 만든다) — `signature` 필드 자신은 여기 포함되지 않는다(서명 대상에 서명을
 * 포함시키면 순환이 된다).
 */
export function canonicalizeUpdateManifest(m: UnsignedUpdateManifest): Buffer {
  const ordered = {
    schemaVersion: m.schemaVersion,
    version: m.version,
    artifactUrl: m.artifactUrl,
    artifactSha256: m.artifactSha256,
    publishedAt: m.publishedAt,
  };
  return Buffer.from(JSON.stringify(ordered), 'utf8');
}

/** 런타임에 들어온 `unknown` 값이 서명된 매니페스트의 형태를 갖췄는지만 본다(서명이
 * 유효한지는 별도 — `manifestVerifier.ts`). 형태부터 어긋나면 서명 검증을 시도할
 * 이유가 없으므로 먼저 걸러낸다. */
export function assertPlausibleSignedUpdateManifest(value: unknown): asserts value is SignedUpdateManifest {
  if (typeof value !== 'object' || value === null) {
    throw new UpdateManifestShapeInvalidError('업데이트 매니페스트가 객체가 아닙니다');
  }
  const v = value as Record<string, unknown>;
  if (v.schemaVersion !== 1) {
    throw new UpdateManifestShapeInvalidError(`schemaVersion이 1이 아닙니다: ${String(v.schemaVersion)}`);
  }
  if (typeof v.version !== 'string' || v.version.trim().length === 0) {
    throw new UpdateManifestShapeInvalidError('version이 비어 있거나 문자열이 아닙니다');
  }
  if (typeof v.artifactUrl !== 'string' || v.artifactUrl.trim().length === 0) {
    throw new UpdateManifestShapeInvalidError('artifactUrl이 비어 있거나 문자열이 아닙니다');
  }
  if (typeof v.artifactSha256 !== 'string' || !SHA256_RE.test(v.artifactSha256)) {
    throw new UpdateManifestShapeInvalidError(`artifactSha256이 유효한 sha256:<hex> 형태가 아닙니다: ${String(v.artifactSha256)}`);
  }
  if (typeof v.publishedAt !== 'string' || Number.isNaN(Date.parse(v.publishedAt))) {
    throw new UpdateManifestShapeInvalidError(`publishedAt이 유효한 ISO 8601 문자열이 아닙니다: ${String(v.publishedAt)}`);
  }
  if (typeof v.signature !== 'string' || v.signature.trim().length === 0) {
    throw new UpdateManifestShapeInvalidError('signature가 비어 있거나 문자열이 아닙니다');
  }
}
