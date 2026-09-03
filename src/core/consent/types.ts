// ConsentToken 타입 계약 — architecture.md §1.2 정본.
//
// 이 파일은 데이터 형태(타입)만 정의한다. 토큰 발급·검증 로직(`gate.assertValid`,
// L0~L3 판정, globalState 저장소)은 W3(동의 게이트 본체)의 책임이며 이 슬라이스(W1)에는
// 없다 — 다만 Provider.apply()가 이 타입을 요구하는 시그니처는 W1이 확정한다. 이후
// W3(동의 게이트)·W10(install, Sensitive)이 이 계약 위에 선다.

import type { ProviderId } from '../../providers/types.js';

/**
 * PR-3(동의는 diff에 바인딩)의 데이터 형태.
 * `approved: true` 같은 플래그가 아니라 "무엇에 대한 동의인가"를 통째로 담는다.
 */
export interface ConsentToken {
  readonly providerId: ProviderId;
  /** plan 재계산 결과와 반드시 일치해야 한다 — gate.assertValid의 핵심 재검증 대상 (W3) */
  readonly diffHash: string;
  readonly changeIds: readonly string[];
  readonly extensionVersion: string;
  /** ISO8601 */
  readonly grantedAt: string;
  /** grantedAt + 15분, 창 비활성 시 즉시 만료 (만료 규칙 자체의 판정은 W3) */
  readonly expiresAt: string;
  /** 1회용 — 소비되면 재사용 불가 (소비 처리는 W3) */
  readonly nonce: string;
}
