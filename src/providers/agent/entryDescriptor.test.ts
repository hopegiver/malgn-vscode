import { describe, expect, it } from 'vitest';
import { buildAgentEntryDescriptor } from './entryDescriptor.js';

describe('buildAgentEntryDescriptor', () => {
  const desiredFallback = { plugin: 'malgn-agent', scope: 'user', channel: 'stable' };

  it('version만 다른 entry는 같은 descriptor를 낸다(재동의 스킵의 근거)', () => {
    const a = buildAgentEntryDescriptor(
      'malgnsoft/claude-plugins',
      { name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' },
      desiredFallback
    );
    const b = buildAgentEntryDescriptor(
      'malgnsoft/claude-plugins',
      { name: 'malgn-agent', source: './malgn-agent', version: '1.8.34' },
      desiredFallback
    );
    expect(a).toBe(b);
  });

  it('source가 바뀌면 다른 descriptor를 낸다(재동의 필요)', () => {
    const a = buildAgentEntryDescriptor(
      'malgnsoft/claude-plugins',
      { name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' },
      desiredFallback
    );
    const b = buildAgentEntryDescriptor(
      'malgnsoft/claude-plugins',
      { name: 'malgn-agent', source: './changed-path', version: '1.8.33' },
      desiredFallback
    );
    expect(a).not.toBe(b);
  });

  it('entry가 null(마켓플레이스 미등록)이면 desiredFallback으로 인코딩한다', () => {
    const descriptor = buildAgentEntryDescriptor('malgnsoft/claude-plugins', null, desiredFallback);
    expect(descriptor).toContain('desired');
    expect(descriptor).toContain('malgn-agent');
  });

  it('키 순서가 달라도 같은 descriptor(정규화)', () => {
    const a = buildAgentEntryDescriptor('repo', { b: 1, a: 2, version: '1' }, desiredFallback);
    const b = buildAgentEntryDescriptor('repo', { a: 2, b: 1, version: '2' }, desiredFallback);
    expect(a).toBe(b);
  });
});
