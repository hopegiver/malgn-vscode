// U-1 (docs/architecture.md §3.6.2) — 아티팩트 축. "아티팩트만 검증하면 다운그레이드가
// 남는다"는 U-2(downgradeGuard.ts)가 별도로 막고, 이 파일은 그 반대편 문제, 즉
// "서명된 매니페스트는 진짜인데 실제로 받은 파일이 그 매니페스트가 말한 것과 다른"
// 경우를 막는다. **서명 검증이 아니라 해시 대조다** — 완료 정의가 요구하는 "다른
// 코드 경로"가 정확히 이것이다.

import { createHash } from 'node:crypto';
import type { VerifiedUpdateManifest } from './manifest.js';

export class UpdateArtifactHashMismatchError extends Error {
  constructor(public readonly expected: string, public readonly actual: string) {
    super(
      `업데이트 아티팩트 해시 불일치(U-1 아티팩트 축) — 매니페스트 선언 ${expected} / 실제 ${actual}. ` +
        '전송 중 변조되었거나 매니페스트가 가리킨 것과 다른 파일이 제공되었습니다.'
    );
    this.name = 'UpdateArtifactHashMismatchError';
  }
}

function sha256Of(buffer: Buffer): string {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

/**
 * `manifest`는 이미 서명 검증을 통과한 값(`VerifiedUpdateManifest`)이어야 한다 —
 * 타입 자체가 그것을 강제한다(브랜드 타입, 유일 생성자는 `verifyUpdateManifest`).
 * 그 신뢰된 선언과 실제로 받은 바이트의 SHA-256을 대조한다. 다르면 던진다
 * (fail-closed) — 통과하면 아무것도 반환하지 않는다.
 */
export function assertArtifactMatchesVerifiedManifest(artifact: Buffer, manifest: VerifiedUpdateManifest): void {
  const expected = manifest.artifactSha256.toLowerCase();
  const actual = sha256Of(artifact);
  if (actual !== expected) {
    throw new UpdateArtifactHashMismatchError(expected, actual);
  }
}
