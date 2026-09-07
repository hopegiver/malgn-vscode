#!/usr/bin/env node
// `dist/dev-app/host`를 `@electron/packager`가 앱 번들로 감쌀 수 있는 최소 디렉터리로
// 만든다 — `package.json`(name/version/main)만 있으면 되고 `node_modules`는 필요
// 없다(esbuild가 이미 전량 번들했고, `electron` 모듈은 Electron 런타임이 특수 처리해
// 일반 node_modules 해석 없이도 `require('electron')`이 해결된다).
//
// [배포 불가 표기] `productName`은 `scripts/package-dev-app.mjs`의
// `DEV_APP_PRODUCT_NAME` 상수가 정본이다("Dev - Local Only" 표기) — 여기 다시 적지
// 않는다(안전 임계값/정본 중복 방지 원칙과 같은 이유: 두 곳에 같은 문자열을 박으면
// 한쪽만 고치는 불일치가 생긴다).

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveHostBuildPaths } from './build-host.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export function buildStagedPackageJson(rootPackageJson, mainFile) {
  return {
    name: 'malgn-dev-app',
    version: rootPackageJson.version,
    private: true,
    main: mainFile,
    // productName은 electron-builder.dev.yml이 정본이다 — 여기 다시 적지 않는다
    // (안전 임계값/정본 중복 방지 원칙과 같은 이유: 두 곳에 같은 문자열을 박으면
    // 한쪽만 고치는 불일치가 생긴다).
  };
}

export function stageDevApp(rootOverride, entryFile = 'entry.dev.cjs') {
  const root = rootOverride ?? DEFAULT_ROOT;
  const rootPackageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  const paths = resolveHostBuildPaths(root);
  const staged = buildStagedPackageJson(rootPackageJson, entryFile);
  writeFileSync(join(paths.outDir, 'package.json'), `${JSON.stringify(staged, null, 2)}\n`, 'utf8');
  return { outDir: paths.outDir, staged };
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  const { outDir, staged } = stageDevApp();
  console.log(`[stage-dev-app] ${join(outDir, 'package.json')} — main=${staged.main}`);
}
