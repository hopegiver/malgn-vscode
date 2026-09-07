import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { InvalidBackupFileNameError, writeBackupFile } from './backupWriter.js';

let backupsDir: string;

beforeEach(async () => {
  backupsDir = await mkdtemp(join(tmpdir(), 'malgn-backups-'));
});

afterEach(async () => {
  await rm(backupsDir, { recursive: true, force: true });
});

function modeOf(fullMode: number): number {
  return fullMode & 0o777;
}

describe('writeBackupFile — NT-R20 백업 0600(생성 시점)', () => {
  it('내용을 그대로 쓰고 실제 경로를 반환한다', async () => {
    const path = await writeBackupFile({
      backupsDir,
      timestamp: '2026-09-07T00-00-00',
      fileName: 'settings.json',
      content: '{"a":1}',
    });
    expect(await readFile(path, 'utf8')).toBe('{"a":1}');
    expect(path).toBe(join(backupsDir, '2026-09-07T00-00-00', 'settings.json'));
  });

  it('파일 모드가 생성 시점부터 0600이다', async () => {
    const path = await writeBackupFile({
      backupsDir,
      timestamp: '2026-09-07T00-00-00',
      fileName: 'settings.json',
      content: 'x',
    });
    const st = await stat(path);
    expect(modeOf(st.mode)).toBe(0o600);
  });

  it('timestamp 하위 디렉터리도 0700으로 생성된다', async () => {
    const path = await writeBackupFile({
      backupsDir,
      timestamp: '2026-09-07T00-00-00',
      fileName: 'settings.json',
      content: 'x',
    });
    const st = await stat(join(backupsDir, '2026-09-07T00-00-00'));
    expect(modeOf(st.mode)).toBe(0o700);
    void path;
  });

  it('파일 이름에 경로 구분자가 있으면 거부한다(디렉터리 이스케이프 방지)', async () => {
    await expect(
      writeBackupFile({ backupsDir, timestamp: '2026-09-07T00-00-00', fileName: '../evil', content: 'x' })
    ).rejects.toBeInstanceOf(InvalidBackupFileNameError);
    await expect(
      writeBackupFile({ backupsDir, timestamp: '2026-09-07T00-00-00', fileName: 'a/b', content: 'x' })
    ).rejects.toBeInstanceOf(InvalidBackupFileNameError);
  });
});
