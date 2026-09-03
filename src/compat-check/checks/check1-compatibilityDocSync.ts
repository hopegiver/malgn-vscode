// 검사 ①B — 문서 사본(policy-contract.md §2 JSONC 예시 · architecture.md 산문) ↔
// compat/compatibility.json 실값 일치 (policy-contract.md §6 "검사 3중" (a)①의 나머지 절반)
// (S4 분할: docs/policy-contract.md §8.3)
//
// v1.2-split 이후 이 검사는 **로컬 full 모드 전용**이다 — docs/policy-contract.md·
// architecture.md를 직접 읽는다. §8.2의 정본 반전(계약 값의 정본이 문서 JSONC에서
// compat/ 실물 파일로 옮겨짐) 덕분에, 문서가 낡아도 코드가 계약을 벗어나는 일은 생기지
// 않는다 — 이 검사가 잡는 것은 "사람이 낡은 문서를 보고 fixture를 잘못 고치는" 2차
// 경로뿐이며, 그래서 CI에서 강제할 필요가 없다(§8.3 ① 행: "①B는 CI 아니오").
//
// (a) extensionVersion == package.json.version 비교는 ①A(check1a-extensionVersionSync.ts)로
// 옮겨져 docs/ 없이도 CI에서 항상 강제된다 — 이 파일은 (b)(c)만 남는다.

import { readFileSync } from 'node:fs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { parseFencedJsonc } from '../lib/textUtils.js';

export interface Check1bInput {
  readonly compatibilityJsonPath: string;
  readonly policyContractMdPath: string;
  readonly architectureMdPath: string;
}

export function checkCompatibilityDocSync(input: Check1bInput): CheckResult {
  const id = '①B';
  const label = '문서 사본(§2 JSONC·architecture.md 산문) ↔ compatibility.json 실값 일치 (로컬 full 전용)';
  const violations: { ref: string; message: string }[] = [];

  const compat = JSON.parse(readFileSync(input.compatibilityJsonPath, 'utf8')) as {
    requires?: { claudeCode?: unknown; malgnAgent?: unknown };
  };
  const doc = readFileSync(input.policyContractMdPath, 'utf8');
  const architectureDoc = readFileSync(input.architectureMdPath, 'utf8');

  const docConstants = parseFencedJsonc(doc, 'compat/compatibility.json') as {
    requires?: { claudeCode?: unknown; malgnAgent?: unknown };
  } | null;

  if (docConstants === null) {
    violations.push({
      ref: '§2',
      message: 'policy-contract.md §2의 compat/compatibility.json JSONC 예시 블록을 찾거나 파싱할 수 없습니다',
    });
  } else {
    const docClaudeCode = docConstants.requires?.claudeCode;
    const docMalgnAgent = docConstants.requires?.malgnAgent;
    if (docClaudeCode !== compat.requires?.claudeCode) {
      violations.push({
        ref: '§2 requires.claudeCode',
        message: `문서 예시(${String(docClaudeCode)}) !== compatibility.json 실값(${String(compat.requires?.claudeCode)})`,
      });
    }
    if (docMalgnAgent !== compat.requires?.malgnAgent) {
      violations.push({
        ref: '§2 requires.malgnAgent',
        message: `문서 예시(${String(docMalgnAgent)}) !== compatibility.json 실값(${String(compat.requires?.malgnAgent)})`,
      });
    }
  }

  // architecture.md §3.5의 산문 하한 서술("`claudeCode` 하한 = `>=2.1.237`")도 같은 값을
  // 가리켜야 한다 — 이 값이 실측(O-21)으로 갱신될 때 fixture만 바뀌고 산문이 남으면
  // "일치"가 조용히 깨진다.
  const proseMatch = /`claudeCode`\s*하한\s*=\s*`([^`]+)`/.exec(architectureDoc);
  if (proseMatch?.[1] !== undefined && proseMatch[1] !== compat.requires?.claudeCode) {
    violations.push({
      ref: 'architecture.md §3.5 산문',
      message: `문서 산문 하한(${proseMatch[1]}) !== compatibility.json 실값(${String(compat.requires?.claudeCode)})`,
    });
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
