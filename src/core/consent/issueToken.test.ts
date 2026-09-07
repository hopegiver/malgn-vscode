import { describe, expect, it } from 'vitest';
import { issueConsentToken, CONSENT_TTL_MS } from './issueToken.js';
import { assertValid, resetConsentNonceStoreForTests } from './gate.js';
import { computeDiffHash } from '../reconciler/diffHash.js';
import type { Plan } from '../../providers/types.js';

const PLAN: Plan = {
  providerId: 'agent',
  changes: [{ id: 'x', target: 'malgn-agent@malgnsoft-plugins', kind: 'install', level: 'L2', after: '{}', reversible: true, rationale: 'r' }],
  diffHash: 'ignored-should-be-recomputed',
};

describe('issueConsentToken', () => {
  it('diffHash를 plan.changes로부터 재계산한다(plan.diffHash 필드를 신뢰하지 않는다)', () => {
    const token = issueConsentToken(PLAN, '0.1.0', { now: new Date('2026-01-01T00:00:00Z'), nonce: 'n1' });
    expect(token.diffHash).toBe(computeDiffHash('agent', PLAN.changes));
    expect(token.diffHash).not.toBe(PLAN.diffHash);
  });

  it('expiresAt = grantedAt + 15분', () => {
    const now = new Date('2026-01-01T00:00:00Z');
    const token = issueConsentToken(PLAN, '0.1.0', { now, nonce: 'n2' });
    expect(Date.parse(token.expiresAt) - Date.parse(token.grantedAt)).toBe(CONSENT_TTL_MS);
  });

  it('발급된 토큰은 즉시 gate.assertValid를 통과한다(발급↔검증 재계산 일치)', () => {
    resetConsentNonceStoreForTests();
    const now = new Date('2026-01-01T00:00:00Z');
    const token = issueConsentToken(PLAN, '0.1.0', { now, nonce: 'n3' });
    expect(() => assertValid(PLAN, token, { now, currentExtensionVersion: '0.1.0' })).not.toThrow();
  });
});
