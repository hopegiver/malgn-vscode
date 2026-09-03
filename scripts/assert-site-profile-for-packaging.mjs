#!/usr/bin/env node
// 패키징(.vsix) 직전 게이트 — docs/policy-contract.md §8.4 fail-closed 3종의 셋째.
// `src/generated/siteConstants.ts`(gen:site 산출물)의 siteProfile을 읽어 'site'가 아니면
// 0이 아닌 종료 코드로 실패한다. `src/core/policy/siteProfileGuard.ts`의
// `assertSiteProfileForPackaging`과 같은 판정이며(정본은 그 파일), 이 스크립트는 그 판정을
// 패키징 파이프라인(TypeScript를 실행하지 않는 자리)에서 CLI로 재현한다.
//
// `MALGN_GEN_SITE_ROOT` 환경변수로 저장소 루트를 오버라이드할 수 있다(gen-site.mjs와 동일
// 관례 — 테스트가 임시 디렉터리를 저장소처럼 꾸며 spawn하기 위해서다).

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export function extractSiteProfile(generatedModuleText) {
  const m = /siteProfile: 'site' \| 'example' = "(site|example)"/.exec(generatedModuleText);
  if (!m) throw new Error('siteConstants.ts에서 siteProfile을 찾을 수 없습니다 — gen:site를 먼저 실행하십시오');
  return m[1];
}

export function main(rootOverride) {
  const root = rootOverride ?? process.env.MALGN_GEN_SITE_ROOT ?? DEFAULT_ROOT;
  const generatedPath = join(root, 'src', 'generated', 'siteConstants.ts');
  const text = readFileSync(generatedPath, 'utf8');
  const siteProfile = extractSiteProfile(text);
  if (siteProfile !== 'site') {
    throw new Error(`siteProfile이 'site'가 아닙니다(${siteProfile}) — 패키징을 실패시킵니다 (docs/policy-contract.md §8.4)`);
  }
  return siteProfile;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  try {
    const profile = main();
    console.log(`[assert-site-profile-for-packaging] OK — siteProfile=${profile}`);
  } catch (error) {
    console.error(`[assert-site-profile-for-packaging] 패키징 거부: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
