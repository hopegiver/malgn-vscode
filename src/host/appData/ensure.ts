// NT-R20(architecture.md §6.1) — "앱 데이터 디렉터리를 이제 우리가 만든다 ... 생성
// 시점의 권한을 테스트로 고정한다 — mkdir 후 chmod는 그 사이에 레이스가 있으므로
// 생성 모드로 지정하고 umask 영향을 테스트로 확인한다."
//
// `TrustLedger`·`JournalStore`(core/*)는 이미 이 패턴(`mkdir(dir, {recursive:true,
// mode:0o700})` 뒤 `writeFile(..., {mode:0o600})`)을 각자 지연 생성 시점에 쓴다 — 이
// 파일은 그 패턴을 앱 기동 시점에 **먼저** 한 번 실행해 두는 부트스트랩 지점이다(각
// 저장소 클래스가 첫 쓰기 전에는 디렉터리가 없다고 가정해도 깨지지 않도록, 기동
// 시퀀스가 이 함수를 한 번 부른 뒤 각 저장소를 생성한다).

import { mkdir } from 'node:fs/promises';
import { computeBackupsDir } from './paths.js';

export const APP_DATA_DIR_MODE = 0o700;
export const BACKUPS_DIR_MODE = 0o700;
export const BACKUP_FILE_MODE = 0o600;

export interface EnsureAppDataDirStructureOptions {
  readonly appDataDir: string;
  readonly platform: NodeJS.Platform;
}

/**
 * 앱 데이터 디렉터리 + `backups/` 하위 디렉터리를 생성 모드 0700으로 만든다. 이미
 * 있으면(재기동) `recursive:true`가 조용히 통과시킨다 — 기존 권한을 되돌리지 않는다
 * (이미 있는 디렉터리의 mode를 이 함수가 사후에 chmod하지 않는다는 뜻이다: 사후
 * chmod는 이 함수가 방지하려는 바로 그 레이스를 다시 열기 때문이다).
 */
export async function ensureAppDataDirStructure(options: EnsureAppDataDirStructureOptions): Promise<void> {
  await mkdir(options.appDataDir, { recursive: true, mode: APP_DATA_DIR_MODE });
  const backupsDir = computeBackupsDir(options.appDataDir, { platform: options.platform });
  await mkdir(backupsDir, { recursive: true, mode: BACKUPS_DIR_MODE });
}
