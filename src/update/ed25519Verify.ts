// U-1 (docs/architecture.md §3.6.2) — 서명 검증 원시 계층. Node 내장 `crypto`의
// ed25519만 쓴다(의존성 0 — U-1 원문: "공개키는 번들 상수"). 전송·파일시스템은 여기
// 없다(주입 대상은 이 파일 위 계층인 `manifestVerifier.ts`/`updateFlow.ts`가 받는다).

import { createPublicKey, verify as verifyEd25519Signature } from 'node:crypto';
import type { UpdatePublicKey } from './publicKeys.js';

export class NoUpdatePublicKeysConfiguredError extends Error {
  constructor() {
    super(
      '업데이트 서명 검증용 공개키가 0개입니다(release-gates.md §7.6 — K-3 미발급). ' +
        '키가 없으면 검증을 생략하지 않고 모든 업데이트를 거부합니다(fail-closed).'
    );
    this.name = 'NoUpdatePublicKeysConfiguredError';
  }
}

/**
 * `data`에 대한 `signatureBase64`가 `keys` 중 하나로 검증되는지 확인한다.
 *
 * - `keys`가 비어 있으면 **검증을 생략하지 않고 던진다** — "키 부재 = 무조건 수락"이
 *   되는 순간 이 모듈 전체가 무의미해진다(작업 지시의 핵심 경고).
 * - `keys`가 여럿이면(K-3 + K-3′) **하나라도 통과하면 수락**한다 — 예비키 취지
 *   (release-gates.md §7.6 K-3′). 전부 실패해야 최종 거부다.
 */
export function verifyEd25519(data: Buffer, signatureBase64: string, keys: readonly UpdatePublicKey[]): boolean {
  if (keys.length === 0) {
    throw new NoUpdatePublicKeysConfiguredError();
  }

  let signature: Buffer;
  try {
    signature = Buffer.from(signatureBase64, 'base64');
  } catch {
    return false;
  }
  if (signature.length === 0) return false;

  for (const key of keys) {
    try {
      const publicKey = createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: key.x }, format: 'jwk' });
      if (verifyEd25519Signature(null, data, publicKey, signature)) return true;
    } catch {
      // 이 키 항목 자체가 형식 오류여도 다음 키로 계속 시도한다 — 하나라도 통과하면
      // 수락, 전부 실패해야 거부다(위 K-3′ 취지).
      continue;
    }
  }
  return false;
}
