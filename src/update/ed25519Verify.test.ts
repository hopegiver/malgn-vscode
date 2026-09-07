import { generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { NoUpdatePublicKeysConfiguredError, verifyEd25519 } from './ed25519Verify.js';
import type { UpdatePublicKey } from './publicKeys.js';

function makeTestKeyPair(label: string): { publicKey: UpdatePublicKey; sign: (data: Buffer) => string } {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' }) as { x: string };
  return {
    publicKey: { label, x: jwk.x },
    sign: (data: Buffer) => cryptoSign(null, data, privateKey).toString('base64'),
  };
}

describe('verifyEd25519', () => {
  it('키가 0개면 검증을 생략하지 않고 던진다(fail-closed — 키 부재 시 전면 거부)', () => {
    const data = Buffer.from('update-manifest-bytes');
    expect(() => verifyEd25519(data, 'anything', [])).toThrow(NoUpdatePublicKeysConfiguredError);
  });

  it('양성 대조군 — 등록된 키로 만든 유효한 서명은 수락한다', () => {
    const data = Buffer.from('update-manifest-bytes');
    const { publicKey, sign } = makeTestKeyPair('primary');
    const signature = sign(data);
    expect(verifyEd25519(data, signature, [publicKey])).toBe(true);
  });

  it('변조된 데이터에 대해서는 같은 서명이라도 거부한다', () => {
    const original = Buffer.from('update-manifest-bytes');
    const tampered = Buffer.from('update-manifest-BYTES');
    const { publicKey, sign } = makeTestKeyPair('primary');
    const signature = sign(original);
    expect(verifyEd25519(tampered, signature, [publicKey])).toBe(false);
  });

  it('신뢰되지 않은 키로 만든 서명은 거부한다', () => {
    const data = Buffer.from('update-manifest-bytes');
    const attacker = makeTestKeyPair('attacker');
    const legit = makeTestKeyPair('legit');
    const signature = attacker.sign(data);
    expect(verifyEd25519(data, signature, [legit.publicKey])).toBe(false);
  });

  it('K-3′ 예비키 취지 — 등록된 키가 여럿이고 그중 하나로만 서명해도 수락한다', () => {
    const data = Buffer.from('update-manifest-bytes');
    const primary = makeTestKeyPair('primary(K-3)');
    const backup = makeTestKeyPair('backup(K-3-prime)');
    const signature = backup.sign(data);
    expect(verifyEd25519(data, signature, [primary.publicKey, backup.publicKey])).toBe(true);
  });

  it('base64로 디코드조차 안 되는 서명 문자열은 거부한다(예외를 던지지 않고 false)', () => {
    const data = Buffer.from('update-manifest-bytes');
    const { publicKey } = makeTestKeyPair('primary');
    expect(verifyEd25519(data, '', [publicKey])).toBe(false);
  });
});
