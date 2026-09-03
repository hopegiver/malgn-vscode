import { describe, expect, it } from 'vitest';
import { computeDiffHash } from './diffHash.js';
import type { Change } from '../../providers/types.js';

const changeA: Change = {
  id: 'c1',
  target: '~/.claude/settings.json#otel.exporter',
  kind: 'update',
  level: 'L1',
  before: 'unset',
  after: 'otlp',
  reversible: true,
  rationale: 'OTel exporter 설정',
};

const changeB: Change = {
  id: 'c2',
  target: '~/.claude/settings.json#otel.endpoint',
  kind: 'add',
  level: 'L1',
  after: 'https://collector.internal',
  reversible: true,
  rationale: 'OTel endpoint 추가',
};

describe('computeDiffHash', () => {
  it('is deterministic for the same input', () => {
    const h1 = computeDiffHash('otel', [changeA, changeB]);
    const h2 = computeDiffHash('otel', [changeA, changeB]);
    expect(h1).toBe(h2);
  });

  it('is insensitive to key insertion order within a Change object', () => {
    const reordered: Change = {
      rationale: changeA.rationale,
      after: changeA.after,
      before: changeA.before,
      reversible: changeA.reversible,
      level: changeA.level,
      kind: changeA.kind,
      target: changeA.target,
      id: changeA.id,
    };
    const h1 = computeDiffHash('otel', [changeA]);
    const h2 = computeDiffHash('otel', [reordered]);
    expect(h1).toBe(h2);
  });

  it('changes when the change content changes', () => {
    const h1 = computeDiffHash('otel', [changeA]);
    const h2 = computeDiffHash('otel', [{ ...changeA, after: 'different-value' }]);
    expect(h1).not.toBe(h2);
  });

  it('changes when providerId changes, even with identical changes', () => {
    const h1 = computeDiffHash('otel', [changeA]);
    const h2 = computeDiffHash('github', [changeA]);
    expect(h1).not.toBe(h2);
  });

  it('is sensitive to change array order (application order is meaningful, §4.5)', () => {
    const h1 = computeDiffHash('otel', [changeA, changeB]);
    const h2 = computeDiffHash('otel', [changeB, changeA]);
    expect(h1).not.toBe(h2);
  });

  it('produces a 64-character hex sha256 digest', () => {
    const h = computeDiffHash('otel', [changeA]);
    expect(h).toMatch(/^[0-9a-f]{64}$/);
  });
});
