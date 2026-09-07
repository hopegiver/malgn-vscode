// architecture.md §5.3 "확장이 `claude mcp add`를 실행하는 경로는 v1에 존재하지 않는다.
// `providers/mcp/**`가 `mcp add`·`.mcp.json` 쓰기를 호출하지 않음을 아키텍처 테스트로
// 강제한다." 이 파일은 그 검사가 재사용하는 순수 스캔 함수를 제공한다(테스트 파일
// 자체와 스캔 로직을 분리해 자기 검증 테스트를 쓰기 쉽게 하기 위해 — 다른
// architecture-tests/*.ts 짝 파일들과 같은 패턴).

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

/** `'mcp', 'add'`(argv 형태) 또는 리터럴 `'mcp add'`, 그리고 `.mcp.json` 문자열 리터럴 —
 * 어느 형태로 쓰든 이 패턴들이 잡는다. */
const MCP_ADD_PATTERNS = [/['"]mcp['"]\s*,\s*['"]add['"]/, /mcp\s+add\b/, /\.mcp\.json/];

export function findMcpInjectionOffenders(files: readonly string[]): string[] {
  return files.filter((file) => {
    const text = readFileSync(file, 'utf8');
    return MCP_ADD_PATTERNS.some((pattern) => pattern.test(text));
  });
}
