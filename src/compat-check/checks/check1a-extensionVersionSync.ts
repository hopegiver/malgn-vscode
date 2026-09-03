// 검사 ①A — `compatibility.json.extensionVersion` == `package.json.version` (PR-7)
// (policy-contract.md §6 "검사 3중" (a)① / S4 분할: docs/policy-contract.md §8.3)
//
// v1.2-split(S4)에서 기존 검사①을 ①A(이 파일 — docs/ 불필요, CI)와 ①B(문서 사본 ↔
// fixture 비교, docs/ 필요, 로컬 full 전용 — check1-compatibilityDocSync.ts)로 나눴다.
// "확장 버전의 정본은 하나뿐이어야 한다"(PR-7)는 docs/ 유무와 무관하게 항상 강제되어야
// 하므로 CI에 남긴다.

import { readFileSync } from 'node:fs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check1aInput {
  readonly packageJsonPath: string;
  readonly compatibilityJsonPath: string;
}

export function checkExtensionVersionSync(input: Check1aInput): CheckResult {
  const id = '①A';
  const label = 'compatibility.json.extensionVersion == package.json.version (PR-7)';

  const pkg = JSON.parse(readFileSync(input.packageJsonPath, 'utf8')) as { version?: unknown };
  const compat = JSON.parse(readFileSync(input.compatibilityJsonPath, 'utf8')) as { extensionVersion?: unknown };

  if (typeof pkg.version !== 'string') {
    return fail(id, label, [{ ref: 'PR-7', message: 'package.json.version이 문자열이 아닙니다' }]);
  }
  if (compat.extensionVersion !== pkg.version) {
    return fail(id, label, [
      {
        ref: 'PR-7',
        message: `compatibility.json.extensionVersion(${String(compat.extensionVersion)}) !== package.json.version(${pkg.version})`,
      },
    ]);
  }

  return ok(id, label);
}
