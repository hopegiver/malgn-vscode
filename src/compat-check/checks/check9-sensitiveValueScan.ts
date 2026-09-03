// 검사 ⑨ — 민감값 스캔 (docs/policy-contract.md §8.3 신설 3종 / §8.6 3차선)
//
// **추적 트리 전체**(`git ls-files`, 폴더 무관)를 오프라인 정규식으로 스캔한다. 패턴·
// allowlist는 `compat/sensitive-classes.json`(공개면 — 패턴만, 값 없음)에서 온다.
//
// 정본 스캔 로직은 `scripts/lib/sensitiveScan.mjs`에 있다 — 이 검사(CI)와 `.githooks/
// pre-push`(로컬)가 **같은 모듈**을 import해 "규칙 이중 정의"를 구조적으로 막는다(작업
// 지시 §8.6 2차선 원문 그대로).

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
// scripts/lib/sensitiveScan.mjs는 일반 JS(.mjs) 모듈이다 — TS 프로젝트 밖(scripts/)에
// 있다. 타입은 sensitiveScan.d.mts가 제공한다.
import { loadClassesConfig, scanText } from '../../../scripts/lib/sensitiveScan.mjs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check9Input {
  readonly repoRoot: string;
  readonly sensitiveClassesJsonPath: string;
}

export function checkSensitiveValueScan(input: Check9Input): CheckResult {
  const id = '⑨';
  const label = '민감값 스캔 — 추적 트리 전체, 오프라인 패턴 + allowlist';

  const config = loadClassesConfig(readFileSync(input.sensitiveClassesJsonPath, 'utf8'));

  const lsFilesOut = execFileSync('git', ['ls-files', '-z'], { cwd: input.repoRoot, encoding: 'utf8' });
  const files = lsFilesOut.split('\0').filter((p) => p.length > 0);

  const violations: { file: string; classId: string; match: string }[] = [];
  for (const relPath of files) {
    let text: string;
    try {
      text = readFileSync(`${input.repoRoot}/${relPath}`, 'utf8');
    } catch {
      continue; // 바이너리·읽기 불가 파일은 건너뛴다(텍스트 스캔 대상이 아니다)
    }
    violations.push(...scanText(relPath, text, config));
  }

  if (violations.length === 0) return ok(id, label);
  return fail(
    id,
    label,
    violations.map((v) => ({ ref: '§8.3 ⑨', message: `${v.file} — ${v.classId} 부류에 걸리는 값: ${v.match}` }))
  );
}
