import { describe, expect, it } from 'vitest';
import { recordVersionTransition } from './versionTransitionRecord.js';
import type { VersionTransitionRecord, VersionTransitionSink } from './versionTransitionRecord.js';

describe('recordVersionTransition (U-4 저널 축)', () => {
  it('주입된 sink의 record를 정확히 한 번, 받은 그대로 호출한다', async () => {
    const received: VersionTransitionRecord[] = [];
    const sink: VersionTransitionSink = {
      record: async (entry) => {
        received.push(entry);
      },
    };
    const entry: VersionTransitionRecord = {
      ts: '2026-09-07T00:00:00.000Z',
      fromVersion: '1.2.0',
      toVersion: '1.3.0',
      trigger: 'verified-manifest',
    };
    await recordVersionTransition(sink, entry);
    expect(received).toEqual([entry]);
  });

  it('sink가 실패하면 그 실패를 그대로 전파한다(조용히 삼키지 않는다)', async () => {
    const sink: VersionTransitionSink = {
      record: async () => {
        throw new Error('디스크 쓰기 실패');
      },
    };
    await expect(
      recordVersionTransition(sink, { ts: '2026-09-07T00:00:00.000Z', fromVersion: '1.2.0', toVersion: '1.3.0', trigger: 'timer' })
    ).rejects.toThrow('디스크 쓰기 실패');
  });
});
