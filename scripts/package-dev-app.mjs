#!/usr/bin/env node
// 로컬 개발용 `.app` 빌드 — `pnpm run package:dev-app`. `@electron/packager`(순수 앱
// 번들링, 서명 없음)로 `dist/dev-app/host`(esbuild 산출물 + 스테이징된 package.json)를
// macOS `.app` 번들로 감싼다.
//
// [배포 불가 표기] `identity`/`osxSign` 옵션을 주지 않는다(= 서명하지 않는다) —
// `productName`에 "(Dev - Local Only)"를 박아 다른 PC로 보내면 Gatekeeper가 그대로
// 막는다는 사실을 이름 자체가 드러낸다. `pnpm run package`(VS Code `.vsix`, `site/`
// 게이트 적용)와는 완전히 다른 산출물·다른 파이프라인이다 — 이 스크립트가 그 게이트를
// 호출하지도, 우회하지도 않는다(존재를 모른다).

import { packager } from '@electron/packager';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildHost, resolveHostBuildPaths } from './build-host.mjs';
import { stageDevApp } from './stage-dev-app.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export const DEV_APP_PRODUCT_NAME = 'Malgn (Dev - Local Only)';
export const DEV_APP_BUNDLE_ID = 'kr.malgnsoft.malgn.devlocal';

export async function packageDevApp(rootOverride) {
  const root = rootOverride ?? DEFAULT_ROOT;
  const paths = resolveHostBuildPaths(root);

  await buildHost(root);
  stageDevApp(root, 'entry.prod.cjs');

  const appPaths = await packager({
    dir: paths.outDir,
    out: join(root, 'dist', 'dev-app', 'out'),
    overwrite: true,
    name: DEV_APP_PRODUCT_NAME,
    appBundleId: DEV_APP_BUNDLE_ID,
    // identity/osxSign을 지정하지 않는다 — 의도적으로 무서명(macOS 로컬 실행 시
    // Gatekeeper가 막으면 사용자가 우클릭 열기로 넘긴다, 배포 대상이 아니므로 그걸로
    // 충분하다).
    platform: 'darwin',
    arch: process.arch === 'arm64' ? 'arm64' : 'x64',
    prune: false, // node_modules가 없다(esbuild가 전량 번들) — pruning할 대상 자체가 없다
  });

  return appPaths;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  packageDevApp().then((appPaths) => {
    console.log(`[package-dev-app] 생성됨(무서명·로컬 전용): ${appPaths.join(', ')}`);
  });
}
