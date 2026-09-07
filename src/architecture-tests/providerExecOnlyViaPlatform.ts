// tech-stack.md §4/§7 "child_process 직접 사용 금지(`platform/exec.ts` 경유만)" — 이
// 파일이 그 검사가 재사용하는 순수 스캔 함수를 제공한다.

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export function listTsFilesExcludingTests(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFilesExcludingTests(full));
    } else if (entry.isFile() && entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) {
      out.push(full);
    }
  }
  return out;
}

const CHILD_PROCESS_IMPORT_RE = /from\s+['"]node:child_process['"]|require\(\s*['"]node:child_process['"]\s*\)/;

export function findChildProcessImportOffenders(files: readonly string[]): string[] {
  return files.filter((file) => CHILD_PROCESS_IMPORT_RE.test(readFileSync(file, 'utf8')));
}
