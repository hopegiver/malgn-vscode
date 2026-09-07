// 앱 데이터 디렉터리 경로 계산 — architecture.md §6.1 배치표("동의 기록·저널·firstRunAt·
// 코호트·알림 억제 / 대상 폴더 신뢰 원장 / 백업 파일 / heartbeat.json → 앱 데이터
// 디렉터리") · §1.1(host adapter, W-N1)의 "실제 앱 데이터 경로를 주입하는" 지점.
//
// [순수 함수 원칙] 이 파일은 `os.homedir()`·`process.platform`·`process.env`를 직접 읽지
// 않는다 — 호출자(host/electron 계층)가 그 값을 넘긴다. Electron의 `app.getPath('appData')`도
// 결국 같은 OS 표준 경로를 반환하므로(darwin: `~/Library/Application Support`, win32:
// `%APPDATA%`), 이 함수는 Electron 없이도 같은 결과를 낼 수 있고 그래서 vitest에서 직접
// 검증한다 — "실제 경로 계산은 이 클래스의 책임이 아니다"(`core/trust/ledger.ts` 주석)라던
// 빈 자리가 여기다.

import { posix, win32 } from 'node:path';

export type SupportedHostPlatform = 'darwin' | 'win32' | 'linux';

export interface AppDataDirInputs {
  readonly platform: NodeJS.Platform;
  readonly homedir: string;
  /** win32에서 `%APPDATA%`가 이미 계산돼 있으면 그 값을 쓴다(레지스트리/셸 변수 우회
   * 재계산을 피한다) — 없으면 `homedir/AppData/Roaming`으로 폴백한다. */
  readonly env: Readonly<Record<string, string | undefined>>;
}

export const APP_DATA_DIR_NAME = 'Malgn';

export class UnsupportedHostPlatformError extends Error {
  constructor(platform: string) {
    super(`지원하지 않는 플랫폼입니다: ${platform} (tech-stack.md §1 — macOS/Windows만 v1 지원 대상)`);
    this.name = 'UnsupportedHostPlatformError';
  }
}

/**
 * OS별 앱 데이터 디렉터리 절대경로를 계산한다. `tech-stack.md` §1 "대상 OS"에 따라
 * macOS·Windows만 지원한다 — Linux는 v1 범위 밖(안내만)이라 값을 만들어내지 않고
 * 명시적으로 거부한다(추측해 진행하지 않는다, PR-6과 같은 정신).
 */
export function computeAppDataDir(inputs: AppDataDirInputs): string {
  if (inputs.platform === 'darwin') {
    return posix.join(inputs.homedir, 'Library', 'Application Support', APP_DATA_DIR_NAME);
  }
  if (inputs.platform === 'win32') {
    const appData = inputs.env.APPDATA && inputs.env.APPDATA.length > 0 ? inputs.env.APPDATA : win32.join(inputs.homedir, 'AppData', 'Roaming');
    return win32.join(appData, APP_DATA_DIR_NAME);
  }
  throw new UnsupportedHostPlatformError(inputs.platform);
}

export function computeBackupsDir(appDataDir: string, inputs: Pick<AppDataDirInputs, 'platform'>): string {
  const pathImpl = inputs.platform === 'win32' ? win32 : posix;
  return pathImpl.join(appDataDir, 'backups');
}
