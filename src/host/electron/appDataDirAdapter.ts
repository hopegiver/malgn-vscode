// Electron `app.getPath` → `computeAppDataDir()` 입력 어댑터. 로직은 `../appData/paths.js`
// (순수, vitest 검증)에 있다 — 이 파일은 실제 Electron 값을 그 입력 형태로 옮기기만
// 한다. `app.getPath('home')`은 Electron이 OS별로 이미 정규화해 주는 값이라 별도
// `os.homedir()` 호출보다 이쪽을 우선한다(Electron 런타임 안에서는 이 값이 정본이다).

import { app } from 'electron';
import { computeAppDataDir } from '../appData/paths.js';

export function resolveRealAppDataDir(): string {
  return computeAppDataDir({ platform: process.platform, homedir: app.getPath('home'), env: process.env });
}
