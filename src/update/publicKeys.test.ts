import { describe, expect, it } from 'vitest';
import { UPDATE_PUBLIC_KEYS } from './publicKeys.js';
import { verifyEd25519, NoUpdatePublicKeysConfiguredError } from './ed25519Verify.js';

describe('UPDATE_PUBLIC_KEYS — K-3 미발급 상태의 정직 표기', () => {
  it('지금은 비어 있다(release-gates.md §7.6 — K-3 발급 전)', () => {
    expect(UPDATE_PUBLIC_KEYS).toEqual([]);
  });

  it('이 빈 배열을 그대로 검증에 쓰면 무엇이 들어와도 전면 거부된다(검증 생략이 아니다)', () => {
    expect(() => verifyEd25519(Buffer.from('anything'), 'c2ln', UPDATE_PUBLIC_KEYS)).toThrow(
      NoUpdatePublicKeysConfiguredError
    );
  });
});
