// U-1 (docs/architecture.md §3.6.2) — 매니페스트 축. "매니페스트만 검증하면 해시
// 교체가 남는다"의 반대편, 즉 매니페스트 자체가 변조되지 않았음을 보장하는 계층이다.
// 아티팩트 축(실제 바이트가 매니페스트가 선언한 해시와 같은지)은 `artifactVerifier.ts`
// — "다른 코드 경로"(완료 정의 ②③)를 명시적으로 분리했다.

import { verifyEd25519 } from './ed25519Verify.js';
import { assertPlausibleSignedUpdateManifest, canonicalizeUpdateManifest } from './manifest.js';
import type { SignedUpdateManifest, VerifiedUpdateManifest } from './manifest.js';
import type { UpdatePublicKey } from './publicKeys.js';

export class UpdateManifestSignatureInvalidError extends Error {
  constructor() {
    super('업데이트 매니페스트 서명 검증 실패 — 변조되었거나 신뢰되지 않은 키로 서명되었습니다(U-1).');
    this.name = 'UpdateManifestSignatureInvalidError';
  }
}

/**
 * U-1(매니페스트 축)의 유일한 집행 지점이자 `VerifiedUpdateManifest`의 유일한 생성자.
 *
 * 순서: ① 형태 검사(`assertPlausibleSignedUpdateManifest` — 실패하면 그 예외가 그대로
 * 전파된다) ② 서명 대상 바이트 재조립 ③ ed25519 검증. `keys`가 비어 있으면
 * `verifyEd25519`가 여기서 던진다(fail-closed — 이 함수는 그 예외를 삼키지 않는다).
 */
export function verifyUpdateManifest(candidate: unknown, keys: readonly UpdatePublicKey[]): VerifiedUpdateManifest {
  assertPlausibleSignedUpdateManifest(candidate);
  const manifest: SignedUpdateManifest = candidate;
  const data = canonicalizeUpdateManifest(manifest);
  const isValid = verifyEd25519(data, manifest.signature, keys);
  if (!isValid) {
    throw new UpdateManifestSignatureInvalidError();
  }
  return manifest as VerifiedUpdateManifest;
}
