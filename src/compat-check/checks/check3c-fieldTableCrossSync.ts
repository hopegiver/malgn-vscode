// 검사 ③C — check3의 27개 리프 목록과 fieldCoverage.test.ts의 독립 사본이 서로 같은지
// 코드 대 코드로 대조 (docs/policy-contract.md §8.3 "③C 신설")
//
// 두 목록은 같은 표("필드별 전수 검증표")에서 나왔지만 파일을 공유하지 않는다 —
// check3-fixtureLeafCoverage.ts는 "fixture에 표에 없는 필드가 있는가"(PR-11①)를,
// fieldCoverage.test.ts는 "표에 있는 필드를 로더가 실제로 검증하는가"(동작 증거)를
// 서로 다른 각도로 증명한다. 표가 바뀌었는데 한쪽 사본만 갱신되면 두 검사는 각자
// 자기 사본 기준으로는 계속 통과할 수 있다 — 그 사고를 잡는 것이 이 검사의 유일한 목적.
//
// 구현은 **소스 텍스트를 정적으로 파싱**한다(ESM import로 fieldCoverage.test.ts를 직접
// 불러오지 않는다) — 그 파일을 import하면 vitest가 그 안의 `describe/it`을 실행해
// compat:check와 `pnpm test`가 "항상 분리 실행된다"는 vitest.compat.config.ts의 불변량이
// 깨진다. 대신 두 파일의 `DOC_TABLE_LEAF_FIELDS = [...] as const;` 배열 리터럴을 텍스트로
// 뽑아 집합 대조한다 — 이것이 오히려 "코드 대 코드"라는 문구에 더 충실한 방식이다.

import { readFileSync } from 'node:fs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

const ARRAY_LITERAL_RE = /DOC_TABLE_LEAF_FIELDS(?:\s*:[^=]+)?\s*=\s*\[([\s\S]*?)\]\s*as const/;
const STRING_ITEM_RE = /'((?:[^'\\]|\\.)*)'/g;

function extractLeafFields(sourcePath: string): readonly string[] | null {
  const text = readFileSync(sourcePath, 'utf8');
  const m = ARRAY_LITERAL_RE.exec(text);
  if (!m?.[1]) return null;
  const body = m[1];
  const items: string[] = [];
  STRING_ITEM_RE.lastIndex = 0;
  let sm: RegExpExecArray | null;
  while ((sm = STRING_ITEM_RE.exec(body)) !== null) {
    if (sm[1] !== undefined) items.push(sm[1]);
  }
  return items;
}

export interface Check3cInput {
  readonly check3SourcePath: string;
  readonly fieldCoverageTestSourcePath: string;
}

export function checkFieldTableCrossSync(input: Check3cInput): CheckResult {
  const id = '③C';
  const label = 'check3의 리프 목록과 fieldCoverage.test.ts의 독립 사본이 코드 대 코드로 일치하는지';

  const fromCheck3 = extractLeafFields(input.check3SourcePath);
  const fromFieldCoverage = extractLeafFields(input.fieldCoverageTestSourcePath);

  if (fromCheck3 === null) {
    return fail(id, label, [{ ref: '③C', message: `${input.check3SourcePath}에서 DOC_TABLE_LEAF_FIELDS 배열을 찾을 수 없습니다` }]);
  }
  if (fromFieldCoverage === null) {
    return fail(id, label, [{ ref: '③C', message: `${input.fieldCoverageTestSourcePath}에서 DOC_TABLE_LEAF_FIELDS 배열을 찾을 수 없습니다` }]);
  }

  const a = new Set(fromCheck3);
  const b = new Set(fromFieldCoverage);
  const violations: { ref: string; message: string }[] = [];

  for (const field of a) {
    if (!b.has(field)) {
      violations.push({ ref: '③C', message: `check3에는 있지만 fieldCoverage.test.ts에는 없는 리프: ${field}` });
    }
  }
  for (const field of b) {
    if (!a.has(field)) {
      violations.push({ ref: '③C', message: `fieldCoverage.test.ts에는 있지만 check3에는 없는 리프: ${field}` });
    }
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
