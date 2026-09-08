#!/usr/bin/env node
// 네이티브 호스트(Electron 메인 프로세스) 개발용 번들 — W-N1 MVP 슬라이스.
//
// [범위] 이 스크립트는 `pnpm package`(VS Code `.vsix` 릴리스 파이프라인, `prepackage`가
// `assert-site-profile-for-packaging.mjs`로 `site/` 게이트를 강제하는 그 경로)와
// **완전히 분리된 별도 명령**이다 — `package.json`의 어떤 기존 스크립트도 이 파일을
// 호출하지 않는다. 산출물은 `dist/dev-app/`(이미 `.gitignore`의 `dist/` 아래라 새
// gitignore 규칙이 필요 없다) 안에만 쓰고, `esbuild.mjs`(VS Code 확장 번들, 산출물
// `dist/extension.cjs`)의 계약은 건드리지 않는다 — 두 산출물이 같은 esbuild 설정을
// 공유하지 않는 이유는 대상(Node vs Electron main)과 external(`vscode` vs `electron`)이
// 다르기 때문이다.
//
// [배포 불가 표기] 여기서 만드는 산출물은 서명되지 않고 `site/` 게이트를 거치지 않는다
// — 로컬 실행 전용이다. 실 릴리스 패키징(서명·공증·업데이트 피드)은 `release-gates.md`
// §7.6·tech-stack.md §5의 몫이며 이 스크립트는 그 절차를 대체하지 않는다.
//
// [패키징 도구 선택 — `@electron/packager`, `electron-builder` 아님] 순수 앱 번들링만
// 하고 서명·공증·인스톨러를 만들지 않는다 — 그래서 `architecture-tests/
// codesignSigningWiring.ts`의 "서명 패키저 등장" 신호(`electron-builder`·
// `@electron/osx-sign`·`electron-osx-sign`·`@electron/notarize`)에 걸리지 않는다(오늘도
// 서명 파이프라인이 없다는 사실과 일치). 전이 의존도 훨씬 적어(`electron-builder`는
// deprecated 하위 의존을 끌고 와 `pnpm-lock.yaml` 민감값 스캔 오탐을 만들었다) 공급망
// 표면도 더 얕다.

import { build } from 'esbuild';
import { copyFileSync, cpSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export function resolveHostBuildPaths(root) {
  const outDir = join(root, 'dist', 'dev-app', 'host');
  return {
    entryDev: join(root, 'src', 'host', 'entry.dev.ts'),
    entryProd: join(root, 'src', 'host', 'entry.prod.ts'),
    outDir,
    resourcesSrc: join(root, 'resources'),
    resourcesDest: join(outDir, 'resources'),
    // 메인 창(W11 대시보드) 렌더러 — createMainWindow.ts가 `${app.getAppPath()}/renderer/*`로
    // 읽는다(§app.getAppPath()는 stage-dev-app.mjs가 package.json을 두는 outDir과 같다,
    // 기존 tray-icon 리소스 경로 계약과 동일 관용구).
    preloadEntry: join(root, 'src', 'host', 'window', 'preload.ts'),
    rendererEntry: join(root, 'src', 'host', 'window', 'renderer', 'main.ts'),
    rendererStaticSrc: join(root, 'src', 'host', 'window', 'renderer'),
    rendererDest: join(outDir, 'renderer'),
  };
}

export async function buildHost(rootOverride) {
  const root = rootOverride ?? DEFAULT_ROOT;
  const paths = resolveHostBuildPaths(root);

  if (!existsSync(paths.outDir)) {
    mkdirSync(paths.outDir, { recursive: true });
  }

  /** @type {import('esbuild').BuildOptions} */
  const shared = {
    bundle: true,
    format: 'cjs',
    platform: 'node',
    target: 'node18',
    external: ['electron'],
    sourcemap: true,
    minify: false,
    logLevel: 'info',
  };

  await build({ ...shared, entryPoints: [paths.entryDev], outfile: join(paths.outDir, 'entry.dev.cjs') });
  await build({ ...shared, entryPoints: [paths.entryProd], outfile: join(paths.outDir, 'entry.prod.cjs') });
  // 프리로드는 메인 프로세스와 같은 실행 계약(CJS·Node 대상·`electron` external)이다 —
  // `contextIsolation:true`에서도 Electron이 프리로드 스크립트에는 `require('electron')`을
  // 계속 제공한다(렌더러 자신과는 다르다).
  await build({ ...shared, entryPoints: [paths.preloadEntry], outfile: join(paths.rendererDest, 'preload.cjs') });

  // 렌더러(main.js)는 브라우저 대상이다 — `electron`을 external로 두지 않는다(애초에
  // import하지 않는다, `preload.ts`가 노출한 `window.malgn`만 쓴다).
  await build({
    entryPoints: [paths.rendererEntry],
    outfile: join(paths.rendererDest, 'main.js'),
    bundle: true,
    format: 'iife',
    platform: 'browser',
    target: 'chrome120',
    sourcemap: true,
    minify: false,
    logLevel: 'info',
  });

  if (!existsSync(paths.rendererDest)) mkdirSync(paths.rendererDest, { recursive: true });
  copyFileSync(join(paths.rendererStaticSrc, 'index.html'), join(paths.rendererDest, 'index.html'));
  copyFileSync(join(paths.rendererStaticSrc, 'styles.css'), join(paths.rendererDest, 'styles.css'));

  if (existsSync(paths.resourcesSrc)) {
    cpSync(paths.resourcesSrc, paths.resourcesDest, { recursive: true });
  }

  return paths;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  buildHost().then((paths) => {
    console.log(`[build-host] entry.dev.cjs / entry.prod.cjs → ${paths.outDir}`);
  });
}
