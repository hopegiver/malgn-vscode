// 검사 ① — `compatibility.json` ↔ `package.json`·문서 일치
// (policy-contract.md §6 "검사 3중" (a)①. tech-stack.md §5.4 원문: "compat/compatibility.json이
// 생성물(package.json engines 등)과 일치")
//
// 이 슬라이스 시점에는 §3.5.3이 요구하는 "PR-7 생성" 파이프라인(임계값을 빌드 시
// package.json engines 등에 생성)이 아직 없다(그 파이프라인 자체는 W6 범위 밖). 그래서
// "일치"를 두 가지 **실재하는** 대조로 구현한다 — 새 스키마를 발명하지 않고 이미 있는
// 값끼리만 비교한다:
//   (a) compatibility.json.extensionVersion === package.json.version
//       (확장 버전의 정본은 하나뿐이어야 한다 — PR-7)
//   (b) compatibility.json.requires.{claudeCode,malgnAgent}가 policy-contract.md §2의
//       JSONC 예시에 박제된 리터럴 값과 같다 — 그 JSONC가 "이 값들의 유일한 정본"이라고
//       스스로 선언하는 블록이므로(§2 원문), 실제 fixture와 어긋나면 문서가 거짓말을
//       하고 있는 것이다.

import { readFileSync } from 'node:fs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { parseFencedJsonc } from '../lib/textUtils.js';

export interface Check1Input {
  readonly packageJsonPath: string;
  readonly compatibilityJsonPath: string;
  readonly policyContractMdPath: string;
  readonly architectureMdPath: string;
}

export function checkCompatibilityDocSync(input: Check1Input): CheckResult {
  const id = '①';
  const label = 'compatibility.json ↔ package.json·문서 일치';
  const violations: { ref: string; message: string }[] = [];

  const pkg = JSON.parse(readFileSync(input.packageJsonPath, 'utf8')) as { version?: unknown };
  const compat = JSON.parse(readFileSync(input.compatibilityJsonPath, 'utf8')) as {
    extensionVersion?: unknown;
    requires?: { claudeCode?: unknown; malgnAgent?: unknown };
  };
  const doc = readFileSync(input.policyContractMdPath, 'utf8');
  const architectureDoc = readFileSync(input.architectureMdPath, 'utf8');

  if (typeof pkg.version !== 'string') {
    violations.push({ ref: 'PR-7', message: 'package.json.version이 문자열이 아닙니다' });
  } else if (compat.extensionVersion !== pkg.version) {
    violations.push({
      ref: 'PR-7',
      message: `compatibility.json.extensionVersion(${String(compat.extensionVersion)}) !== package.json.version(${pkg.version})`,
    });
  }

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
