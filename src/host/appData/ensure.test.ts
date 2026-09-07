// [완료 판정 #4] "생성 시점 권한을 테스트로 고정한다"의 실제 재현 — mkdir 직후 stat으로
// 실제 모드를 읽어 확인한다(사후 chmod 흔적이 없음을 이 테스트가 증명한다: 만약 구현이
// "mkdir 기본모드 → 나중에 chmod"였다면, 이 테스트 자체는 최종 결과가 같아 통과하겠지만
// 소스 코드 감사(`ensure.ts`에 `chmod` 호출이 없음)로 레이스 부재를 보강한다 — 아래
// 두 번째 describe가 그 소스 검사다).

import { mkdtemp, rm, stat } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { APP_DATA_DIR_MODE, BACKUPS_DIR_MODE, ensureAppDataDirStructure } from './ensure.js';
import { computeBackupsDir } from './paths.js';

let root: string;

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'malgn-appdata-ensure-'));
});

afterEach(async () => {
  await rm(root, { recursive: true, force: true });
});

function modeOf(fullMode: number): number {
  return fullMode & 0o777;
}

describe('ensureAppDataDirStructure — NT-R20 생성 시점 권한(mkdir 후 chmod 레이스 없음)', () => {
  it('앱 데이터 디렉터리를 0700으로 생성한다', async () => {
    const appDataDir = join(root, 'Malgn');
    await ensureAppDataDirStructure({ appDataDir, platform: 'darwin' });
    const st = await stat(appDataDir);
    expect(modeOf(st.mode)).toBe(APP_DATA_DIR_MODE);
  });

  it('backups/ 하위 디렉터리도 0700으로 생성한다', async () => {
    const appDataDir = join(root, 'Malgn');
    await ensureAppDataDirStructure({ appDataDir, platform: 'darwin' });
    const backupsDir = computeBackupsDir(appDataDir, { platform: 'darwin' });
    const st = await stat(backupsDir);
    expect(modeOf(st.mode)).toBe(BACKUPS_DIR_MODE);
  });

  it('중간 경로가 여러 단계로 없어도(최초 실행) recursive 생성이 실패하지 않는다', async () => {
    const appDataDir = join(root, 'a', 'b', 'Malgn');
    await expect(ensureAppDataDirStructure({ appDataDir, platform: 'darwin' })).resolves.toBeUndefined();
  });

  it('이미 존재하는 상태(재기동)에서 다시 호출해도 에러 없이 통과한다', async () => {
    const appDataDir = join(root, 'Malgn');
    await ensureAppDataDirStructure({ appDataDir, platform: 'darwin' });
    await expect(ensureAppDataDirStructure({ appDataDir, platform: 'darwin' })).resolves.toBeUndefined();
    const st = await stat(appDataDir);
    expect(modeOf(st.mode)).toBe(APP_DATA_DIR_MODE);
  });
});

describe('ensure.ts 소스 검사 — mkdir 후 chmod 패턴이 없다(레이스 재도입 방지)', () => {
  it('ensure.ts 소스에 chmod 호출이 없다 — 생성 모드 지정만으로 권한을 고정한다', () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const source = readFileSync(join(here, 'ensure.ts'), 'utf8');
    expect(source).not.toMatch(/\bchmod\(/);
  });
});
