#!/usr/bin/env node
// `.githooks/pre-push`가 호출하는 실제 스캐너 — docs/policy-contract.md §8.6 2차선.
// `src/compat-check/checks/check9-sensitiveValueScan.ts`(CI, 검사 ⑨)와 **같은 모듈**
// (scripts/lib/sensitiveScan.mjs)과 **같은 패턴 파일**(compat/sensitive-classes.json)을
// 재사용한다 — 로컬 훅과 CI 검사가 서로 다른 기준으로 갈라지지 않게 하기 위해서다.
//
// git이 실행하는 pre-push 훅답게 인자 없이 바로 실행 가능해야 한다(TS 컴파일 단계 없음).

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadClassesConfig, scanText } from './lib/sensitiveScan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');

export function scanRepo(repoRoot) {
  const config = loadClassesConfig(readFileSync(join(repoRoot, 'compat', 'sensitive-classes.json'), 'utf8'));

  const lsFilesOut = execFileSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf8' });
  const files = lsFilesOut.split('\0').filter((p) => p.length > 0);

  const violations = [];
  for (const relPath of files) {
    let text;
    try {
      text = readFileSync(join(repoRoot, relPath), 'utf8');
    } catch {
      continue; // 바이너리·읽기 불가 — 건너뛴다
    }
    violations.push(...scanText(relPath, text, config));
  }
  return violations;
}

export function main(repoRoot = ROOT) {
  const violations = scanRepo(repoRoot);

  if (violations.length === 0) {
    console.log('[pre-push] 민감값 스캔 통과 — 검출 0건');
    return 0;
  }

  console.error(`[pre-push] 민감값 스캔 실패 — ${violations.length}건 검출. push를 차단합니다.`);
  for (const v of violations) {
    console.error(`  - ${v.file}: ${v.classId} 부류에 걸리는 값: ${v.match}`);
  }
  console.error('docs/policy-contract.md §8 — 값을 site/(비추적)로 옮기거나 예약 네임스페이스로 바꾸십시오.');
  return 1;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  process.exitCode = main();
}
