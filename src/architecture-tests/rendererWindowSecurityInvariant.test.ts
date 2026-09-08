// architecture.md §2.8 정본 — "창 생성 지점이 하나임을 강제하고 그 지점의 옵션 객체를
// 검사한다." `src/host/window/createMainWindow.ts`가 그 유일한 지점이어야 한다.

import { readFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  countBrowserWindowConstructorCalls,
  findMissingSecurityMarkers,
  listTsFilesExcludingTests,
} from './rendererWindowSecurityInvariant.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC_ROOT = join(HERE, '..');
const EXPECTED_WINDOW_FILE = join(SRC_ROOT, 'host', 'window', 'createMainWindow.ts');

describe('창 생성 지점은 src/host/window/createMainWindow.ts 하나뿐이다(architecture.md §2.8)', () => {
  const files = listTsFilesExcludingTests(SRC_ROOT);

  it('검사 대상 파일 목록이 비어 있지 않다(자기 점검)', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('`new BrowserWindow(`이 등장하는 파일은 createMainWindow.ts 단 하나다', () => {
    const counts = countBrowserWindowConstructorCalls(files);
    const filesWithCalls = [...counts.keys()];
    expect(filesWithCalls.map((f) => relative(SRC_ROOT, f))).toEqual([relative(SRC_ROOT, EXPECTED_WINDOW_FILE)]);
  });

  it('그 지점에서 정확히 1번만 생성한다(중복 생성 경로가 숨어 있지 않다)', () => {
    const counts = countBrowserWindowConstructorCalls(files);
    expect(counts.get(EXPECTED_WINDOW_FILE)).toBe(1);
  });

  it('그 지점의 옵션 객체에 §2.8 고정값 5종 + 내비게이션 차단 2종이 모두 등장한다', () => {
    const text = readFileSync(EXPECTED_WINDOW_FILE, 'utf8');
    expect(findMissingSecurityMarkers(text)).toEqual([]);
  });

  it('[자기 검증] 마커 누락은 실제로 탐지 가능하다', () => {
    const fakeSource = 'new BrowserWindow({ webPreferences: { nodeIntegration: true } })';
    expect(findMissingSecurityMarkers(fakeSource).length).toBeGreaterThan(0);
  });

  it('[자기 검증] `new BrowserWindow(` 패턴 자체는 실제로 탐지 가능하다', () => {
    expect(/new\s+BrowserWindow\s*\(/.test('const w = new BrowserWindow({});')).toBe(true);
  });
});
