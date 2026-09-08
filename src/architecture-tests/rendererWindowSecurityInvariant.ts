// architecture.md §2.8 "렌더러 표면 강제 설정" — "창 생성 지점이 하나임을 강제하고 그
// 지점의 옵션 객체를 검사한다." 이 파일이 그 검사가 재사용하는 순수 스캔 함수를
// 제공한다(다른 architecture-tests/*.ts 짝 파일과 같은 패턴).

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export function listTsFilesExcludingTests(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFilesExcludingTests(full));
    } else if (entry.isFile() && entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) {
      out.push(full);
    }
  }
  return out;
}

const NEW_BROWSER_WINDOW_RE = /new\s+BrowserWindow\s*\(/g;

/** 파일별로 `BrowserWindow` 생성자 호출 횟수를 센다(이 파일 자신이 그 패턴의
 * 리터럴 텍스트를 포함하면 자기 자신을 오탐하므로, 이 주석에서도 그 리터럴을
 * 피한다). */
export function countBrowserWindowConstructorCalls(files: readonly string[]): ReadonlyMap<string, number> {
  const counts = new Map<string, number>();
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    const matches = text.match(NEW_BROWSER_WINDOW_RE);
    if (matches && matches.length > 0) counts.set(file, matches.length);
  }
  return counts;
}

/** §2.8 고정값 5종 + 내비게이션 차단 2종이 창 생성 파일 소스에 텍스트로 등장하는지
 * 확인한다(순수 텍스트 검사 — 런타임에 옵션 객체를 실제로 구성하는지는 이 검사의
 * 책임 밖이고, `createMainWindow.ts`의 단위 테스트가 있다면 그쪽 몫이다). */
export const REQUIRED_SECURITY_MARKERS: readonly RegExp[] = [
  /nodeIntegration:\s*false/,
  /contextIsolation:\s*true/,
  /sandbox:\s*true/,
  /webSecurity:\s*true/,
  /will-navigate/,
  /setWindowOpenHandler/,
];

export function findMissingSecurityMarkers(fileContent: string): RegExp[] {
  return REQUIRED_SECURITY_MARKERS.filter((marker) => !marker.test(fileContent));
}
