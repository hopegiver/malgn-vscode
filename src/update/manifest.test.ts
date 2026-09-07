import { describe, expect, it } from 'vitest';
import { UpdateManifestShapeInvalidError, assertPlausibleSignedUpdateManifest, canonicalizeUpdateManifest } from './manifest.js';
import type { SignedUpdateManifest, UnsignedUpdateManifest } from './manifest.js';

const BASE: UnsignedUpdateManifest = {
  schemaVersion: 1,
  version: '1.2.3',
  artifactUrl: 'https://updates.example.com/v1.2.3/artifact.bin',
  artifactSha256: `sha256:${'a'.repeat(64)}`,
  publishedAt: '2026-09-07T00:00:00.000Z',
};

describe('canonicalizeUpdateManifest', () => {
  it('같은 내용이면 항상 같은 바이트를 만든다(필드 순서 고정)', () => {
    const bytesA = canonicalizeUpdateManifest(BASE);
    const bytesB = canonicalizeUpdateManifest({ ...BASE });
    expect(bytesA.equals(bytesB)).toBe(true);
  });

  it('필드 값이 하나라도 다르면 바이트가 달라진다', () => {
    const bytesA = canonicalizeUpdateManifest(BASE);
    const bytesB = canonicalizeUpdateManifest({ ...BASE, version: '1.2.4' });
    expect(bytesA.equals(bytesB)).toBe(false);
  });

  it('signature 필드는 서명 대상에 포함되지 않는다(순환 방지) — canonicalize 입력 타입 자체에 그 필드가 없다', () => {
    const signed: SignedUpdateManifest = { ...BASE, signature: 'irrelevant-for-canonicalization' };
    const bytesFromSigned = canonicalizeUpdateManifest(signed);
    const bytesFromUnsigned = canonicalizeUpdateManifest(BASE);
    expect(bytesFromSigned.equals(bytesFromUnsigned)).toBe(true);
  });
});

describe('assertPlausibleSignedUpdateManifest', () => {
  it('형태를 갖춘 값은 통과한다', () => {
    const value: SignedUpdateManifest = { ...BASE, signature: 'c2lnbmF0dXJl' };
    expect(() => assertPlausibleSignedUpdateManifest(value)).not.toThrow();
  });

  it('객체가 아니면 거부한다', () => {
    expect(() => assertPlausibleSignedUpdateManifest('not-an-object')).toThrow(UpdateManifestShapeInvalidError);
    expect(() => assertPlausibleSignedUpdateManifest(null)).toThrow(UpdateManifestShapeInvalidError);
  });

  it('artifactSha256 형식이 어긋나면 거부한다', () => {
    const value = { ...BASE, artifactSha256: 'not-a-hash', signature: 'x' };
    expect(() => assertPlausibleSignedUpdateManifest(value)).toThrow(UpdateManifestShapeInvalidError);
  });

  it('signature가 없으면 거부한다', () => {
    expect(() => assertPlausibleSignedUpdateManifest(BASE)).toThrow(UpdateManifestShapeInvalidError);
  });

  it('publishedAt이 유효한 날짜 문자열이 아니면 거부한다', () => {
    const value = { ...BASE, publishedAt: 'not-a-date', signature: 'x' };
    expect(() => assertPlausibleSignedUpdateManifest(value)).toThrow(UpdateManifestShapeInvalidError);
  });
});
