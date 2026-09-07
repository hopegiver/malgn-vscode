// SIGN-R2 구조적 배선 검사 테스트 — docs/release-gates.md §7.6.2.
// 두 계층: ① 실제 저장소 대조(오늘은 서명 파이프라인이 없어 skippedNoTarget) ②
// fixture 위반 주입(서명 파이프라인이 생겼다고 가정했을 때 배선 누락을 실제로 잡는지).
//
// [W7 MVP — 판단 근거 기록] 로컬 dev `.app` 빌드 도구로 `electron-builder`가 아니라
// `@electron/packager`를 선택한 이유 중 하나가 이 검사다: `electron-builder`는
// `CODESIGN_PACKAGER_DEPENDENCIES`에 있어 devDependency로만 추가해도(실서명을 전혀
// 하지 않아도) "서명 패키저 등장" 신호를 켜 SIGN-R2 배선을 요구한다. `@electron/packager`는
// 그 목록에 없어(서명이 아니라 순수 앱 번들링 도구) 신호를 켜지 않는다 — 오늘 여전히
// 서명 파이프라인이 없다는 사실과 일치한다(다른 이유: 전이 의존이 훨씬 적고
// deprecated 하위 의존이 없어 `pnpm-lock.yaml` 민감값 스캔 오탐도 만들지 않았다).

import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { checkCodesignSigningWiring } from './codesignSigningWiring.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_REPO_ROOT = join(HERE, '..', '..');

describe('실제 저장소 대조 — 서명 파이프라인이 아직 없는 이 슬라이스 상태', () => {
  it('신호가 0개라 "대상 없음"으로 통과한다(오늘의 정상 상태 — @electron/packager는 서명 패키저 목록 밖)', () => {
    const result = checkCodesignSigningWiring(REAL_REPO_ROOT);
    expect(result.skippedNoTarget).toBe(true);
    expect(result.violations).toEqual([]);
  });
});

describe('fixture 위반 주입 — 서명 파이프라인이 생겼다고 가정', () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-sign-wiring-'));
    await mkdir(join(root, 'scripts'), { recursive: true });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('package.json scripts에 codesign 호출이 등장하는데 배선이 없으면 위반을 잡는다', async () => {
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({ scripts: { 'post-sign': 'codesign --sign "Developer ID" dist/app.app' } }),
      'utf8'
    );
    const result = checkCodesignSigningWiring(root);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations.some((v) => v.includes('SIGN-R2'))).toBe(true);
  });

  it('package.json scripts에 codesign 호출 + 배선 마커가 함께 있으면 위반이 없다', async () => {
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({
        scripts: {
          'post-sign': 'codesign --sign "Developer ID" dist/app.app && node scripts/verify-codesign-certificate-hash.mjs --actual-hash=$HASH',
        },
      }),
      'utf8'
    );
    const result = checkCodesignSigningWiring(root);
    expect(result.violations).toEqual([]);
    expect(result.skippedNoTarget).toBe(false);
  });

  it('electron-builder 의존성만 있어도(codesign 리터럴 없이) 신호로 잡아 배선을 요구한다', async () => {
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({ devDependencies: { 'electron-builder': '^25.0.0' }, scripts: {} }),
      'utf8'
    );
    const result = checkCodesignSigningWiring(root);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations.some((v) => v.includes('electron-builder'))).toBe(true);
  });

  it('scripts/ 하위 파일에서 codesign을 직접 호출하는데 배선이 없으면 위반을 잡는다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: {} }), 'utf8');
    await writeFile(
      join(root, 'scripts', 'sign-mac.mjs'),
      `import { execFileSync } from 'node:child_process';\nexecFileSync('codesign', ['--sign', 'Developer ID', 'dist/app.app']);\n`,
      'utf8'
    );
    const result = checkCodesignSigningWiring(root);
    expect(result.violations.some((v) => v.includes('sign-mac.mjs'))).toBe(true);
  });

  it('scripts/ 하위 파일에서 codesign 호출 + 배선 마커가 함께 있으면 위반이 없다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: {} }), 'utf8');
    await writeFile(
      join(root, 'scripts', 'sign-mac.mjs'),
      `import { execFileSync } from 'node:child_process';\n` +
        `execFileSync('codesign', ['--sign', 'Developer ID', 'dist/app.app']);\n` +
        `execFileSync('node', ['scripts/verify-codesign-certificate-hash.mjs', '--actual-hash=' + hash]);\n`,
      'utf8'
    );
    const result = checkCodesignSigningWiring(root);
    expect(result.violations).toEqual([]);
  });

  it('codesign 관련 신호가 전혀 없으면 "대상 없음"으로 통과한다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: { build: 'esbuild.mjs' } }), 'utf8');
    await writeFile(join(root, 'scripts', 'unrelated.mjs'), `console.log('no signing here');\n`, 'utf8');
    const result = checkCodesignSigningWiring(root);
    expect(result.skippedNoTarget).toBe(true);
    expect(result.violations).toEqual([]);
  });
});
