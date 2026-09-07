// 백업 파일 쓰기 — architecture.md §6.1 배치표("백업 파일 → 앱 데이터 디렉터리
// +backups/<ts>/") · G-4("모든 Change 전 백업 + 최근 변경 되돌리기") · NT-R20("백업
// 0600·디렉터리 0700, 생성 시점 권한").
//
// 이 슬라이스(W-N1)는 Change 적용 로직(G-4의 "되돌리기" 자체) 없이 provider apply가
// 아직 없다 — 그래서 이 파일은 "백업 한 장을 안전한 권한으로 쓰는" 원자 단위만 제공한다.
// 실제 Change별 백업 호출(적용 직전 대상 파일 스냅샷)은 provider apply 오케스트레이션
// (W7+)이 이 함수를 호출해 배선한다 — 로직을 어댑터로 끌어올리지 않기 위해 "백업 한
// 건을 쓴다"는 원자 동작 이상은 이 파일에 두지 않는다.

import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

export const BACKUP_DIR_MODE = 0o700;
export const BACKUP_FILE_MODE = 0o600;

export interface WriteBackupFileOptions {
  readonly backupsDir: string;
  /** ISO8601 등 정렬 가능한 타임스탬프 문자열 — `backups/<ts>/` 하위 디렉터리 이름 */
  readonly timestamp: string;
  /** 백업 파일 이름(예: 원본 파일의 basename). 경로 구분자를 포함하면 안 된다 —
   * 하위 디렉터리 이스케이프(`../`)를 막기 위해 검증한다. */
  readonly fileName: string;
  readonly content: string;
}

export class InvalidBackupFileNameError extends Error {
  constructor(fileName: string) {
    super(`백업 파일 이름에 경로 구분자를 쓸 수 없습니다: ${fileName}`);
    this.name = 'InvalidBackupFileNameError';
  }
}

/**
 * `backups/<timestamp>/<fileName>`에 내용을 0600 모드로 **생성 시점부터** 쓴다(mkdir 후
 * chmod가 아니라 `writeFile`의 `mode` 옵션으로 생성 시점 권한을 고정한다 — NT-R20과
 * 동일한 원칙). 반환값은 실제로 쓴 절대경로 — 저널의 `backupPath` 필드(§4.5 step⑥)에
 * 그대로 쓸 수 있는 형태다.
 */
export async function writeBackupFile(options: WriteBackupFileOptions): Promise<string> {
  if (options.fileName.includes('/') || options.fileName.includes('\\') || options.fileName.includes('..')) {
    throw new InvalidBackupFileNameError(options.fileName);
  }
  const dir = join(options.backupsDir, options.timestamp);
  await mkdir(dir, { recursive: true, mode: BACKUP_DIR_MODE });
  const filePath = join(dir, options.fileName);
  await writeFile(filePath, options.content, { encoding: 'utf8', mode: BACKUP_FILE_MODE });
  return filePath;
}
