import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { HEARTBEAT_FILE_MODE, writeHeartbeat } from './heartbeat.js';

let dir: string;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), 'malgn-heartbeat-'));
});

afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe('writeHeartbeat', () => {
  it('{ts,pid,version}을 JSON으로 쓴다', async () => {
    const path = join(dir, 'heartbeat.json');
    await writeHeartbeat(path, { ts: '2026-09-07T00:00:00.000Z', pid: 1234, version: '0.1.0' });
    const parsed = JSON.parse(await readFile(path, 'utf8'));
    expect(parsed).toEqual({ ts: '2026-09-07T00:00:00.000Z', pid: 1234, version: '0.1.0' });
  });

  it('파일 모드는 0600이다', async () => {
    const path = join(dir, 'heartbeat.json');
    await writeHeartbeat(path, { ts: '2026-09-07T00:00:00.000Z', pid: 1, version: '0.1.0' });
    const st = await stat(path);
    expect(st.mode & 0o777).toBe(HEARTBEAT_FILE_MODE);
  });

  it('반복 호출 시 매번 최신 내용으로 덮어쓴다', async () => {
    const path = join(dir, 'heartbeat.json');
    await writeHeartbeat(path, { ts: '2026-09-07T00:00:00.000Z', pid: 1, version: '0.1.0' });
    await writeHeartbeat(path, { ts: '2026-09-07T00:01:00.000Z', pid: 1, version: '0.1.0' });
    const parsed = JSON.parse(await readFile(path, 'utf8'));
    expect(parsed.ts).toBe('2026-09-07T00:01:00.000Z');
  });
});
