import { describe, expect, it } from 'vitest';
import { UnsupportedHostPlatformError, computeAppDataDir, computeBackupsDir } from './paths.js';

// [민감값 스캔 회피 — 의도적] `/Users/<이름>`·`/home/<이름>`·`C:\Users\<이름>`은
// `compat/sensitive-classes.json`의 `home-directory-path` 부류가 "개발자 로컬 절대경로"
// 형태로 잡는다(회전 불가·비가역의 실례). 실제 홈 디렉터리 값이 필요 없는 순수 로직
// 테스트라 그 형태를 아예 쓰지 않는다 — CI 러너 홈으로 이미 예외 처리된 `/home/runner`
// 계열이나 `C:\Users\Public`도 쓰지 않고, 형태 자체가 다른 고정 fixture 경로를 쓴다.
const FIXTURE_POSIX_HOME = '/opt/fixture-home';
const FIXTURE_WIN_HOME = 'C:\\fixture-home';

describe('computeAppDataDir — tech-stack.md §1 대상 OS(macOS·Windows)만 지원', () => {
  it('macOS는 ~/Library/Application Support/Malgn이다', () => {
    expect(computeAppDataDir({ platform: 'darwin', homedir: FIXTURE_POSIX_HOME, env: {} })).toBe(
      `${FIXTURE_POSIX_HOME}/Library/Application Support/Malgn`
    );
  });

  it('Windows는 %APPDATA%\\Malgn이다(APPDATA 존재 시 그 값을 쓴다)', () => {
    expect(
      computeAppDataDir({ platform: 'win32', homedir: FIXTURE_WIN_HOME, env: { APPDATA: `${FIXTURE_WIN_HOME}\\AppData\\Roaming` } })
    ).toBe(`${FIXTURE_WIN_HOME}\\AppData\\Roaming\\Malgn`);
  });

  it('Windows에서 APPDATA가 없으면 homedir\\AppData\\Roaming으로 폴백한다', () => {
    expect(computeAppDataDir({ platform: 'win32', homedir: FIXTURE_WIN_HOME, env: {} })).toBe(
      `${FIXTURE_WIN_HOME}\\AppData\\Roaming\\Malgn`
    );
  });

  it('Linux는 v1 범위 밖이라 추측하지 않고 명시적으로 거부한다', () => {
    expect(() => computeAppDataDir({ platform: 'linux', homedir: FIXTURE_POSIX_HOME, env: {} })).toThrow(UnsupportedHostPlatformError);
  });
});

describe('computeBackupsDir — §6.1 "backups/<ts>/"의 부모 디렉터리', () => {
  it('macOS 경로 구분자로 appDataDir/backups를 만든다', () => {
    const appDataDir = `${FIXTURE_POSIX_HOME}/Library/Application Support/Malgn`;
    expect(computeBackupsDir(appDataDir, { platform: 'darwin' })).toBe(`${appDataDir}/backups`);
  });

  it('Windows 경로 구분자로 appDataDir\\backups를 만든다', () => {
    const appDataDir = `${FIXTURE_WIN_HOME}\\AppData\\Roaming\\Malgn`;
    expect(computeBackupsDir(appDataDir, { platform: 'win32' })).toBe(`${appDataDir}\\backups`);
  });
});
