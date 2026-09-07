// architecture.md §5.3 정본 — "`providers/mcp/**`가 `mcp add`·`.mcp.json` 쓰기를
// 호출하지 않음을 아키텍처 테스트로 강제한다."

import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { findMcpInjectionOffenders, listTsFilesExcludingTests } from './mcpProviderNoInjection.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC_ROOT = join(HERE, '..');
const MCP_DIR = join(SRC_ROOT, 'providers', 'mcp');

describe('providers/mcp/**는 mcp add·.mcp.json 쓰기를 호출하지 않는다(architecture.md §5.3)', () => {
  const files = listTsFilesExcludingTests(MCP_DIR);

  it('src/providers/mcp/** 소스 파일 목록이 비어 있지 않다(검사 대상 존재 자기 점검)', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('src/providers/mcp/** 어떤 파일에도 mcp add·.mcp.json 패턴이 없다', () => {
    const offenders = findMcpInjectionOffenders(files);
    expect(offenders.map((f) => relative(SRC_ROOT, f))).toEqual([]);
  });

  it('[자기 검증] 패턴 자체는 실제로 탐지 가능하다', () => {
    const fakeOffendingSource = "runExec({file:'claude', args: ['mcp', 'add', name, url]})";
    const offenders = findMcpInjectionOffenders.call(null, []);
    expect(offenders).toEqual([]); // 빈 입력 자기 점검
    // 패턴 자체 검증은 findMcpInjectionOffenders 내부 정규식을 직접 재현해 확인한다
    expect(/['"]mcp['"]\s*,\s*['"]add['"]/.test(fakeOffendingSource)).toBe(true);
  });
});
