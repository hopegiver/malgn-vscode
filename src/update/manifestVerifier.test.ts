// C-10 종료조건(작업 지시) — "변조 매니페스트 1회 거부"를 이 파일이 실측한다.
// 실제 ed25519 키페어를 만들어 ① 정상 서명은 수락(양성 대조군) ② 서명 후 필드를
// 바꾼 변조 매니페스트는 거부, 를 같은 테스트 스위트 안에서 함께 증명한다 — 전부
// 거부하는 코드도 ①이 없으면 통과해 버리는 것을 막기 위해서다.

import { generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { canonicalizeUpdateManifest } from './manifest.js';
import type { SignedUpdateManifest, UnsignedUpdateManifest } from './manifest.js';
import { NoUpdatePublicKeysConfiguredError } from './ed25519Verify.js';
import { UpdateManifestSignatureInvalidError, verifyUpdateManifest } from './manifestVerifier.js';
import type { UpdatePublicKey } from './publicKeys.js';
import { UpdateManifestShapeInvalidError } from './manifest.js';

const UNSIGNED: UnsignedUpdateManifest = {
  schemaVersion: 1,
  version: '1.3.0',
  artifactUrl: 'https://updates.example.com/v1.3.0/artifact.bin',
  artifactSha256: `sha256:${'b'.repeat(64)}`,
  publishedAt: '2026-09-07T00:00:00.000Z',
};

function issueTestKeyPair(label: string): { publicKey: UpdatePublicKey; sign: (data: Buffer) => string } {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' }) as { x: string };
  return {
    publicKey: { label, x: jwk.x },
    sign: (data: Buffer) => cryptoSign(null, data, privateKey).toString('base64'),
  };
}

function signManifest(unsigned: UnsignedUpdateManifest, sign: (data: Buffer) => string): SignedUpdateManifest {
  const signature = sign(canonicalizeUpdateManifest(unsigned));
  return { ...unsigned, signature };
}

describe('verifyUpdateManifest (U-1 매니페스트 축)', () => {
  it('양성 대조군 — 등록된 키로 정상 서명된 매니페스트는 수락하고 VerifiedUpdateManifest로 승격한다', () => {
    const key = issueTestKeyPair('K-3');
    const signed = signManifest(UNSIGNED, key.sign);
    const verified = verifyUpdateManifest(signed, [key.publicKey]);
    expect(verified.version).toBe('1.3.0');
  });

  it('변조 매니페스트 거부 — 서명 후 필드(version)를 바꾸면 서명 검증이 실패한다', () => {
    const key = issueTestKeyPair('K-3');
    const signed = signManifest(UNSIGNED, key.sign);
    const tampered: SignedUpdateManifest = { ...signed, version: '9.9.9' };
    expect(() => verifyUpdateManifest(tampered, [key.publicKey])).toThrow(UpdateManifestSignatureInvalidError);
  });

  it('변조 매니페스트 거부 — artifactSha256을 바꿔치기해도(해시 교체 시도) 서명 검증이 실패한다', () => {
    const key = issueTestKeyPair('K-3');
    const signed = signManifest(UNSIGNED, key.sign);
    const tampered: SignedUpdateManifest = { ...signed, artifactSha256: `sha256:${'e'.repeat(64)}` };
    expect(() => verifyUpdateManifest(tampered, [key.publicKey])).toThrow(UpdateManifestSignatureInvalidError);
  });

  it('신뢰되지 않은 키로 서명된 매니페스트는 거부한다', () => {
    const legit = issueTestKeyPair('legit');
    const attacker = issueTestKeyPair('attacker');
    const signed = signManifest(UNSIGNED, attacker.sign);
    expect(() => verifyUpdateManifest(signed, [legit.publicKey])).toThrow(UpdateManifestSignatureInvalidError);
  });

  it('키 부재 시 전면 거부 — 공개키가 0개면 서명이 형식상 아무리 그럴듯해도 예외를 던진다(검증 생략이 아니다)', () => {
    const key = issueTestKeyPair('K-3');
    const signed = signManifest(UNSIGNED, key.sign);
    expect(() => verifyUpdateManifest(signed, [])).toThrow(NoUpdatePublicKeysConfiguredError);
  });

  it('형태가 애초에 매니페스트가 아니면 서명 검증까지 가지 않고 형태 오류로 거부한다', () => {
    expect(() => verifyUpdateManifest({ not: 'a manifest' }, [])).toThrow(UpdateManifestShapeInvalidError);
  });
});
