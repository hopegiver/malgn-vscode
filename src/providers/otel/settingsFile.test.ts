import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { ReadTextFile } from '../agent/marketplaceReader.js';
import { claudeSettingsPath, readClaudeSettings, writeClaudeSettingsEnvAtomic } from './settingsFile.js';

function fakeReadTextFile(files: Record<string, string>): ReadTextFile {
  return async (path: string) => {
    if (!(path in files)) {
      const err = new Error('ENOENT') as NodeJS.ErrnoException;
      err.code = 'ENOENT';
      throw err;
    }
    return files[path]!;
  };
}

describe('claudeSettingsPath', () => {
  it('claudeHomeDir/settings.json을 만든다', () => {
    expect(claudeSettingsPath('/home/runner/.claude')).toBe(join('/home/runner/.claude', 'settings.json'));
  });
});

describe('readClaudeSettings', () => {
  it('파일이 없으면 missing(최초 실행 정상 상태)', async () => {
    const outcome = await readClaudeSettings('/x/settings.json', fakeReadTextFile({}));
    expect(outcome.kind).toBe('missing');
  });

  it('JSON 파싱 실패면 unreadable', async () => {
    const outcome = await readClaudeSettings('/x/settings.json', fakeReadTextFile({ '/x/settings.json': '{not json' }));
    expect(outcome.kind).toBe('unreadable');
  });

  it('env의 문자열 값만 남기고 otelHeadersHelper를 읽는다', async () => {
    const raw = JSON.stringify({ env: { A: '1', B: 2, C: null }, otelHeadersHelper: '/x/otel-headers.sh', other: 'ignored' });
    const outcome = await readClaudeSettings('/x/settings.json', fakeReadTextFile({ '/x/settings.json': raw }));
    expect(outcome.kind).toBe('ok');
    if (outcome.kind !== 'ok') return;
    expect(outcome.snapshot.env).toEqual({ A: '1' });
    expect(outcome.snapshot.otelHeadersHelper).toBe('/x/otel-headers.sh');
  });

  it('otelHeadersHelper가 없으면 null', async () => {
    const raw = JSON.stringify({ env: {} });
    const outcome = await readClaudeSettings('/x/settings.json', fakeReadTextFile({ '/x/settings.json': raw }));
    expect(outcome.kind).toBe('ok');
    if (outcome.kind !== 'ok') return;
    expect(outcome.snapshot.otelHeadersHelper).toBeNull();
  });
});

describe('writeClaudeSettingsEnvAtomic', () => {
  let dir: string;
  let settingsPath: string;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'malgn-otel-settings-'));
    settingsPath = join(dir, 'settings.json');
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('파일이 없으면 새로 만들고 백업은 만들지 않는다(최초 실행)', async () => {
    const { backupPath } = await writeClaudeSettingsEnvAtomic({
      path: settingsPath,
      readTextFile: (p) => readFile(p, 'utf8'),
      updates: { CLAUDE_CODE_ENABLE_TELEMETRY: '1' },
      backupsDir: join(dir, 'backups'),
      now: () => new Date('2026-09-07T00:00:00.000Z'),
    });
    expect(backupPath).toBeNull();
    const written = JSON.parse(await readFile(settingsPath, 'utf8'));
    expect(written.env.CLAUDE_CODE_ENABLE_TELEMETRY).toBe('1');
  });

  it('기존 파일이 있으면 백업을 만들고 다른 최상위 키·다른 env 키를 보존한다', async () => {
    const { writeFile } = await import('node:fs/promises');
    await writeFile(settingsPath, JSON.stringify({ theme: 'dark', env: { EXISTING: 'keep' } }), 'utf8');

    const { backupPath } = await writeClaudeSettingsEnvAtomic({
      path: settingsPath,
      readTextFile: (p) => readFile(p, 'utf8'),
      updates: { NEW_KEY: 'new' },
      backupsDir: join(dir, 'backups'),
      now: () => new Date('2026-09-07T00:00:00.000Z'),
    });

    expect(backupPath).not.toBeNull();
    const backupContent = JSON.parse(await readFile(backupPath!, 'utf8'));
    expect(backupContent.env.EXISTING).toBe('keep');

    const written = JSON.parse(await readFile(settingsPath, 'utf8'));
    expect(written.theme).toBe('dark');
    expect(written.env.EXISTING).toBe('keep');
    expect(written.env.NEW_KEY).toBe('new');
  });

  it('updates가 기존 값을 덮어쓴다', async () => {
    const { writeFile } = await import('node:fs/promises');
    await writeFile(settingsPath, JSON.stringify({ env: { KEY: 'old' } }), 'utf8');

    await writeClaudeSettingsEnvAtomic({
      path: settingsPath,
      readTextFile: (p) => readFile(p, 'utf8'),
      updates: { KEY: 'new' },
      backupsDir: join(dir, 'backups'),
      now: () => new Date(),
    });

    const written = JSON.parse(await readFile(settingsPath, 'utf8'));
    expect(written.env.KEY).toBe('new');
  });
});
