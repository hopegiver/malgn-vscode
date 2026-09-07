import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { JournalStore } from '../../core/journal/store.js';
import { JournalVersionTransitionSink } from './journalVersionTransitionSink.js';

let baseDir: string;

beforeEach(async () => {
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-version-sink-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
});

describe('JournalVersionTransitionSink', () => {
  it('record()가 JournalStore.appendVersionTransition으로 위임한다', async () => {
    const journal = new JournalStore({ baseDir });
    const sink = new JournalVersionTransitionSink(journal);
    await sink.record({ ts: '2026-09-07T00:00:00.000Z', fromVersion: '0.1.0', toVersion: '0.2.0', trigger: 'user-command' });

    const [entry] = await journal.readAll();
    expect(entry).toMatchObject({ kind: 'versionTransition', fromVersion: '0.1.0', toVersion: '0.2.0', trigger: 'user-command' });
  });
});
