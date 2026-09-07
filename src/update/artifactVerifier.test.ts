// C-10 종료조건(작업 지시) — "변조 아티팩트 1회 거부"를 이 파일이 실측한다. 이 검사는
// manifestVerifier.test.ts의 서명 검증과는 **다른 코드 경로**(해시 비교)다 — 완료
// 정의가 요구하는 "②③은 다른 코드 경로다"를 그대로 반영한다.

import { createHash, generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { assertArtifactMatchesVerifiedManifest, UpdateArtifactHashMismatchError } from './artifactVerifier.js';
import { canonicalizeUpdateManifest } from './manifest.js';
import type { UnsignedUpdateManifest, VerifiedUpdateManifest } from './manifest.js';

function sha256Of(buffer: Buffer): string {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

/** manifestVerifier.ts를 거치지 않고 "이미 검증을 통과했다고 가정한" 값을 직접
 * 만든다 — 이 파일의 관심사는 서명이 아니라 해시 대조 한 가지뿐이다. */
function fakeVerified(unsigned: UnsignedUpdateManifest): VerifiedUpdateManifest {
  const { privateKey } = generateKeyPairSync('ed25519');
  const signature = cryptoSign(null, canonicalizeUpdateManifest(unsigned), privateKey).toString('base64');
  return { ...unsigned, signature } as VerifiedUpdateManifest;
}

describe('assertArtifactMatchesVerifiedManifest (U-1 아티팩트 축)', () => {
  it('양성 대조군 — 실제 바이트의 해시가 매니페스트 선언과 같으면 통과한다', () => {
    const artifact = Buffer.from('the real artifact bytes');
    const manifest = fakeVerified({
      schemaVersion: 1,
      version: '2.0.0',
      artifactUrl: 'https://updates.example.com/v2.0.0/artifact.bin',
      artifactSha256: sha256Of(artifact),
      publishedAt: '2026-09-07T00:00:00.000Z',
    });
    expect(() => assertArtifactMatchesVerifiedManifest(artifact, manifest)).not.toThrow();
  });

  it('변조 아티팩트 거부 — 다운로드된 바이트가 매니페스트 선언 해시와 다르면 거부한다(서명과 무관한 해시 비교)', () => {
    const originalArtifact = Buffer.from('the real artifact bytes');
    const swappedArtifact = Buffer.from('a different, malicious payload');
    const manifest = fakeVerified({
      schemaVersion: 1,
      version: '2.0.0',
      artifactUrl: 'https://updates.example.com/v2.0.0/artifact.bin',
      artifactSha256: sha256Of(originalArtifact),
      publishedAt: '2026-09-07T00:00:00.000Z',
    });
    expect(() => assertArtifactMatchesVerifiedManifest(swappedArtifact, manifest)).toThrow(UpdateArtifactHashMismatchError);
  });

  it('대소문자 표기 차이는 결과에 영향을 주지 않는다(해시 정규화)', () => {
    const artifact = Buffer.from('case-insensitive-check');
    const manifest = fakeVerified({
      schemaVersion: 1,
      version: '2.0.1',
      artifactUrl: 'https://updates.example.com/v2.0.1/artifact.bin',
      artifactSha256: sha256Of(artifact).toUpperCase(),
      publishedAt: '2026-09-07T00:00:00.000Z',
    });
    expect(() => assertArtifactMatchesVerifiedManifest(artifact, manifest)).not.toThrow();
  });
});
