// tech-stack.md §4/§7 정본 — "child_process 직접 사용 금지(`platform/exec.ts` 경유만)".
// `providers/agent/**`·`providers/mcp/**`가 `node:child_process`를 직접 import하지 않고
// `platform/exec.ts`의 `runExec()`만 쓴다는 것을 고정한다.

import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { findChildProcessImportOffenders, listTsFilesExcludingTests } from './providerExecOnlyViaPlatform.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC_ROOT = join(HERE, '..');

describe('providers/agent·mcp는 node:child_process를 직접 import하지 않는다(tech-stack.md §4/§7)', () => {
  const files = [...listTsFilesExcludingTests(join(SRC_ROOT, 'providers', 'agent')), ...listTsFilesExcludingTests(join(SRC_ROOT, 'providers', 'mcp'))];

  it('검사 대상 파일 목록이 비어 있지 않다(자기 점검)', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('어떤 파일도 node:child_process를 직접 import하지 않는다', () => {
    const offenders = findChildProcessImportOffenders(files);
    expect(offenders.map((f) => relative(SRC_ROOT, f))).toEqual([]);
  });

  it('[자기 검증] 패턴 자체는 실제로 탐지 가능하다', () => {
    expect(findChildProcessImportOffenders.length).toBeGreaterThanOrEqual(1);
    const fakeOffendingSource = "import { execFile } from 'node:child_process';";
    expect(/from\s+['"]node:child_process['"]/.test(fakeOffendingSource)).toBe(true);
  });
});
