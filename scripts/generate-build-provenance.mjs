#!/usr/bin/env node
// NT-R22 (docs/release-gates.md §7.6.6) — 빌드 프로베넌스 레코드 생성.
// `postcompile` 라이프사이클로 배선되어 있다 — `pnpm run compile`이 `dist/extension.cjs`를
// 만들 때마다(즉 `pnpm run build`·`pnpm run package` 어느 경로로 불려도) 자동으로 뒤따라
// 실행되어 그 산출물에 대한 레코드를 남긴다. §7.5.1 M-40("이름에 의존하면 조용히
// 안 돈다")과 같은 이유로, npm 스크립트 *이름*이 아니라 **아티팩트를 실제로 만드는
// 단계 자체**에 매달았다 — 미래에 패키저가 바뀌어도 `compile`이 `dist/extension.cjs`를
// 만드는 한 이 훅은 따라간다.
//
// [fail-closed] 아래 중 하나라도 없으면 0이 아닌 종료 코드로 실패한다(조용한 스킵 금지):
//  - dist/extension.cjs(방금 compile이 만들었어야 할 아티팩트)
//  - src/generated/siteConstants.ts(gen:site가 만들었어야 할 산출물)
//  - .git(커밋 SHA 없이는 "무엇을 빌드했는지"를 증명할 수 없다)

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractSiteProfile } from './assert-site-profile-for-packaging.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export function computeArtifactSha256(buffer) {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

export function getCommitSha(root, execFileSyncFn = execFileSync) {
  try {
    return execFileSyncFn('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  } catch (error) {
    throw new Error(
      `NT-R22: git 커밋 SHA를 얻을 수 없습니다(${error instanceof Error ? error.message : String(error)}) — ` +
        '.git 없이는 이 아티팩트가 무엇으로부터 빌드됐는지 증명할 수 없습니다(fail-closed).'
    );
  }
}

/**
 * 순수 로직 진입점 — 파일 경로만 받고 실제 I/O는 얇게 감싼다(gen-site.mjs와 같은 관례,
 * 테스트가 임시 디렉터리로 실제 파일시스템을 통해 검증할 수 있게 한다).
 */
export function generateProvenance({
  root,
  artifactRelPath = join('dist', 'extension.cjs'),
  readFileFn = readFileSync,
  existsFn = existsSync,
  execFileSyncFn = execFileSync,
  now = () => new Date(),
}) {
  const artifactPath = join(root, artifactRelPath);
  if (!existsFn(artifactPath)) {
    throw new Error(`NT-R22: 프로베넌스 대상 아티팩트가 없습니다 — ${artifactPath} (compile을 먼저 실행하십시오)`);
  }
  const siteConstantsPath = join(root, 'src', 'generated', 'siteConstants.ts');
  if (!existsFn(siteConstantsPath)) {
    throw new Error(`NT-R22: ${siteConstantsPath}가 없습니다 — gen:site를 먼저 실행하십시오`);
  }

  const artifactBuffer = readFileFn(artifactPath);
  const artifactSha256 = computeArtifactSha256(artifactBuffer);
  const siteProfile = extractSiteProfile(readFileFn(siteConstantsPath, 'utf8'));
  const commitSha = getCommitSha(root, execFileSyncFn);
  const buildTimestamp = now().toISOString();

  const record = {
    recordVersion: 1,
    siteProfile,
    commitSha,
    artifactPath: relative(root, artifactPath).split('\\').join('/'),
    artifactSha256,
    buildTimestamp,
    // K-3(업데이트 서명 키) 없음 — release-gates.md §7.6.6이 요구하는 서명은 K-3가
    // 발급될 때까지 조건부 대기다. 이 필드가 null인 채로 있어도 산출물 자체는
    // 만들어진다(§7.5.1 M-40의 역방향 확인 수단이라는 목적은 미서명 상태로도
    // 부분 충족된다 — "무엇을 만들었는가"는 증명하되 "우리가 서명했다"는 아직
    // 증명하지 못한다). K-3가 생겼는데 이 필드가 계속 null이면
    // provenanceSigningWiring.test.ts가 그 상태를 구조적으로 잡는다.
    signature: null,
  };

  return record;
}

export function main(rootOverride) {
  const root = rootOverride ?? process.env.MALGN_GEN_SITE_ROOT ?? DEFAULT_ROOT;
  const record = generateProvenance({ root });
  const outPath = join(root, 'dist', 'build-provenance.json');
  writeFileSync(outPath, `${JSON.stringify(record, null, 2)}\n`, 'utf8');
  console.log(`[generate-build-provenance] siteProfile=${record.siteProfile} artifact=${record.artifactSha256} → ${outPath}`);
  return record;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  try {
    main();
  } catch (error) {
    console.error(`[generate-build-provenance] 실패(fail-closed): ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
