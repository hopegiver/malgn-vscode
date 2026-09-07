// ConsentToken 발급 — architecture.md §1.2 PR-3("동의는 diff에 바인딩") · §2.1
// "grantedAt + 15분, 창 비활성 시 즉시 만료"의 발급 측 구현. `gate.ts`는 **검증**만
// 하고(`assertValid`) 발급 로직이 없었다 — 동의 화면(UI)이 "승인" 버튼을 누른 순간
// 호출할 발급 지점이 필요해 이 슬라이스(W7 MVP)가 채운다(설계 갭 채움, 반환문에 명시:
// 원문 §1.2는 `ConsentToken`의 형태만 정의하고 발급 함수 시그니처는 명시하지 않았다).
//
// [만료 15분] `expiresAt`을 발급 시점 기준 15분 뒤로 고정한다 — 이 값의 정본은 여기
// 하나다(안전 임계값 단일 정본 원칙). 이 값을 바꾸면 architecture.md §2.1 표·이 파일의
// 상수·테스트를 함께 확인한다.

import { randomUUID } from 'node:crypto';
import type { Plan } from '../../providers/types.js';
import type { ConsentToken } from './types.js';
import { computeDiffHash } from '../reconciler/diffHash.js';

/** §2.1 "grantedAt + 15분" — 단일 정본. 바꾸면 architecture.md §2.1과 함께 확인. */
export const CONSENT_TTL_MS = 15 * 60 * 1000;

export interface IssueConsentTokenOptions {
  /** 테스트 전용 — 운영 코드는 항상 실제 현재 시각을 쓴다 */
  readonly now?: Date;
  /** 테스트 전용 — 운영 코드는 항상 crypto.randomUUID()를 쓴다 */
  readonly nonce?: string;
}

/**
 * 사람이 동의 화면에서 "승인"을 누른 직후 호출한다. `diffHash`는 `plan.diffHash`
 * 필드를 그대로 신뢰하지 않고 `plan.changes`로부터 재계산한다 — `gate.assertValid`가
 * 검증 시 같은 재계산을 하므로(§1.2 ②), 발급 시점에 이미 같은 계산을 쓰는 것이
 * "발급된 토큰이 검증을 통과 못 하는" 불일치를 구조적으로 없앤다.
 */
export function issueConsentToken(plan: Plan, extensionVersion: string, options: IssueConsentTokenOptions = {}): ConsentToken {
  const now = options.now ?? new Date();
  const grantedAt = now.toISOString();
  const expiresAt = new Date(now.getTime() + CONSENT_TTL_MS).toISOString();
  return {
    providerId: plan.providerId,
    diffHash: computeDiffHash(plan.providerId, plan.changes),
    changeIds: plan.changes.map((c) => c.id),
    extensionVersion,
    grantedAt,
    expiresAt,
    nonce: options.nonce ?? randomUUID(),
  };
}
