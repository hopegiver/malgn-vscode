// gate.assertValid 회귀 테스트 — architecture.md §1.2 검사 축 ①~⑤ + "실패의 두 등급".
// PM 위임 작업(W3) 완료판정 #3·#4가 요구하는 거부 경로 5종 + 심각도 분리를 여기서 증명한다.

import { beforeEach, describe, expect, it } from 'vitest';
import { assertValid, ConsentGateError, resetConsentNonceStoreForTests } from './gate.js';
import { MV_CONSENT_EXPIRED, MV_CONSENT_INVALID } from './errors.js';
import { computeDiffHash } from '../reconciler/diffHash.js';
import type { Change, Plan } from '../../providers/types.js';
import type { ConsentToken } from './types.js';

const EXTENSION_VERSION = '0.1.0'; // compat/compatibility.json.extensionVersion과 동일(PR-7)

const baseChange: Change = {
  id: 'c1',
  target: '~/.claude/settings.json#otel.exporter',
  kind: 'update',
  level: 'L1',
  before: 'unset',
  after: 'otlp',
  reversible: true,
  rationale: 'OTel exporter 설정',
};

function makePlan(changes: readonly Change[] = [baseChange]): Plan {
  return {
    providerId: 'otel',
    changes,
    diffHash: computeDiffHash('otel', changes),
  };
}

function makeConsent(overrides: Partial<ConsentToken> = {}, plan: Plan = makePlan()): ConsentToken {
  const grantedAt = new Date('2026-09-04T00:00:00.000Z');
  const expiresAt = new Date(grantedAt.getTime() + 15 * 60 * 1000);
  return {
    providerId: plan.providerId,
    diffHash: computeDiffHash(plan.providerId, plan.changes),
    changeIds: plan.changes.map((c) => c.id),
    extensionVersion: EXTENSION_VERSION,
    grantedAt: grantedAt.toISOString(),
    expiresAt: expiresAt.toISOString(),
    nonce: 'nonce-fixed-for-test',
    ...overrides,
  };
}

/** 대부분의 테스트에서 "만료 전 어느 시점"으로 쓸 고정 시각 */
const WITHIN_TTL = new Date('2026-09-04T00:05:00.000Z');

describe('gate.assertValid', () => {
  beforeEach(() => {
    resetConsentNonceStoreForTests();
  });

  it('providerId·diffHash·extensionVersion이 모두 일치하고 만료 전·nonce 미사용이면 통과한다(양성 대조군)', () => {
    const plan = makePlan();
    const consent = makeConsent({}, plan);
    expect(() => assertValid(plan, consent, { now: WITHIN_TTL })).not.toThrow();
  });

  describe('거부 경로 1 — plan 변조 (② diffHash 재계산 불일치)', () => {
    it('plan.changes가 동의 이후 한 비트라도 달라지면 MV_CONSENT_INVALID(high)로 거부된다', () => {
      const grantedPlan = makePlan();
      const consent = makeConsent({}, grantedPlan);
      // 동의 이후 plan이 바뀐 상황을 재현 — after 값 하나만 다르다
      const tamperedPlan = makePlan([{ ...baseChange, after: 'otlp-tampered' }]);

      expect(() => assertValid(tamperedPlan, consent, { now: WITHIN_TTL })).toThrow(ConsentGateError);
      try {
        assertValid(tamperedPlan, consent, { now: WITHIN_TTL });
        throw new Error('unreachable');
      } catch (e) {
        expect(e).toBeInstanceOf(ConsentGateError);
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_INVALID);
        expect(err.severity).toBe('high');
      }
    });

    it('plan.diffHash 필드만 일치시키고 changes를 바꿔도 재계산 비교가 잡아낸다(핵심 방어)', () => {
      const grantedPlan = makePlan();
      const consent = makeConsent({}, grantedPlan);
      // plan.diffHash 필드를 동의값과 억지로 맞춰도(호출자 실수/조작 가정) changes가 다르면
      // recomputed hash가 달라야 한다 — plan.diffHash 필드를 신뢰하지 않는 것이 요점(§1.2 ②).
      const forgedPlan: Plan = {
        providerId: 'otel',
        changes: [{ ...baseChange, after: 'otlp-tampered' }],
        diffHash: consent.diffHash, // 필드만 위조
      };

      expect(() => assertValid(forgedPlan, consent, { now: WITHIN_TTL })).toThrow(ConsentGateError);
    });
  });

  describe('거부 경로 2 — 다른 providerId (①)', () => {
    it('consent.providerId !== plan.providerId면 MV_CONSENT_INVALID(high)로 거부된다', () => {
      const plan = makePlan(); // providerId: 'otel'
      const consent = makeConsent({ providerId: 'github' }, plan);

      try {
        assertValid(plan, consent, { now: WITHIN_TTL });
        throw new Error('unreachable');
      } catch (e) {
        expect(e).toBeInstanceOf(ConsentGateError);
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_INVALID);
        expect(err.severity).toBe('high');
      }
    });
  });

  describe('거부 경로 3 — 만료 (④)', () => {
    it('now >= expiresAt이면 MV_CONSENT_EXPIRED(info)로 거부된다 — nonce 재사용과 다른 심각도', () => {
      const plan = makePlan();
      const consent = makeConsent({}, plan);
      const afterExpiry = new Date(Date.parse(consent.expiresAt) + 1);

      try {
        assertValid(plan, consent, { now: afterExpiry });
        throw new Error('unreachable');
      } catch (e) {
        expect(e).toBeInstanceOf(ConsentGateError);
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_EXPIRED);
        expect(err.severity).toBe('info'); // MV_CONSENT_INVALID의 'high'와 명시적으로 다르다
      }
    });

    it('now === expiresAt 경계는 만료로 취급한다(now < expiresAt만 유효)', () => {
      const plan = makePlan();
      const consent = makeConsent({}, plan);
      const exactlyAtExpiry = new Date(Date.parse(consent.expiresAt));

      try {
        assertValid(plan, consent, { now: exactlyAtExpiry });
        throw new Error('unreachable');
      } catch (e) {
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_EXPIRED);
      }
    });
  });

  describe('거부 경로 4 — nonce 재사용 (⑤)', () => {
    it('같은 토큰으로 두 번째 assertValid를 호출하면 MV_CONSENT_INVALID(high)로 거부된다', () => {
      const plan = makePlan();
      const consent = makeConsent({}, plan);

      expect(() => assertValid(plan, consent, { now: WITHIN_TTL })).not.toThrow(); // 1회차: 정상 소비

      try {
        assertValid(plan, consent, { now: WITHIN_TTL }); // 2회차: 같은 nonce 재사용
        throw new Error('unreachable');
      } catch (e) {
        expect(e).toBeInstanceOf(ConsentGateError);
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_INVALID);
        expect(err.severity).toBe('high');
        expect(err.message).toMatch(/nonce/);
      }
    });

    it('다른 nonce를 가진 별개 동의는 앞선 재사용 거부의 영향을 받지 않는다', () => {
      const plan = makePlan();
      const consentA = makeConsent({ nonce: 'nonce-A' }, plan);
      const consentB = makeConsent({ nonce: 'nonce-B' }, plan);

      expect(() => assertValid(plan, consentA, { now: WITHIN_TTL })).not.toThrow();
      expect(() => assertValid(plan, consentB, { now: WITHIN_TTL })).not.toThrow();
    });

    it('실패한 검증 시도(예: diffHash 불일치)는 nonce를 소비하지 않는다 — 이후 정당한 시도는 통과한다', () => {
      const grantedPlan = makePlan();
      const consent = makeConsent({}, grantedPlan);
      const tamperedPlan = makePlan([{ ...baseChange, after: 'otlp-tampered' }]);

      expect(() => assertValid(tamperedPlan, consent, { now: WITHIN_TTL })).toThrow(); // 실패 — nonce 미소비
      expect(() => assertValid(grantedPlan, consent, { now: WITHIN_TTL })).not.toThrow(); // 올바른 plan으로 재시도 — 통과
    });
  });

  describe('거부 경로 5 — extensionVersion 불일치 (③)', () => {
    it('consent.extensionVersion이 현재 확장 버전과 다르면 MV_CONSENT_INVALID(high)로 거부된다', () => {
      const plan = makePlan();
      const consent = makeConsent({ extensionVersion: '0.0.1' }, plan);

      try {
        assertValid(plan, consent, { now: WITHIN_TTL });
        throw new Error('unreachable');
      } catch (e) {
        expect(e).toBeInstanceOf(ConsentGateError);
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_INVALID);
        expect(err.severity).toBe('high');
      }
    });

    it('currentExtensionVersion 오버라이드(테스트 전용)로 임의 "현재 버전" 대비 불일치도 검증할 수 있다', () => {
      const plan = makePlan();
      const consent = makeConsent({ extensionVersion: '1.2.3' }, plan);

      expect(() =>
        assertValid(plan, consent, { now: WITHIN_TTL, currentExtensionVersion: '1.2.3' })
      ).not.toThrow();

      try {
        assertValid(plan, consent, { now: WITHIN_TTL, currentExtensionVersion: '9.9.9' });
        throw new Error('unreachable');
      } catch (e) {
        const err = e as ConsentGateError;
        expect(err.code).toBe(MV_CONSENT_INVALID);
      }
    });
  });

  it('검사 순서 — providerId 불일치가 diffHash 불일치보다 먼저 잡힌다(둘 다 위반이어도 ①이 먼저)', () => {
    const plan = makePlan();
    // providerId도 다르고 diffHash도 안 맞는 완전히 다른 동의
    const consent = makeConsent({ providerId: 'github', diffHash: 'not-a-real-hash' }, plan);

    try {
      assertValid(plan, consent, { now: WITHIN_TTL });
      throw new Error('unreachable');
    } catch (e) {
      const err = e as ConsentGateError;
      expect(err.message).toMatch(/providerId/);
    }
  });
});
