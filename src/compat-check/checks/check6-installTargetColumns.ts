// 검사 ⑥ — `allowedInstallTargets` 격자의 모든 행이 열 스펙 전량을 갖고 `verified`가
// 불리언 리터럴인지 (PR-11 ③, M-17)
// (policy-contract.md §6 "검사 3중" (a)⑥)
//
// 정본 구현은 이미 W2의 `validateInstallTargetsGrid`(codeConstants.ts)에 있다 —
// PR-11③이 요구하는 "고정된 열 스펙과의 대조"(표↔fixture 동치가 아니라)가 정확히
// 그 함수의 동작이다(파일 상단 주석 참고). 이 검사는 그 함수를 **바꾸지 않고** 실제
// fixture(`compat/install-targets.json`)에 적용해 거부된 행이 0건인지만 확인한다 —
// W1·W2 계약을 이 슬라이스에서 재구현하지 않는다(작업 지시 "W1·W2가 만든 계약을
// 바꾸지 마십시오").

import { readFileSync } from 'node:fs';
import { validateInstallTargetsGrid } from '../../core/policy/codeConstants.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check6Input {
  readonly installTargetsJsonPath: string;
}

export function checkInstallTargetColumns(input: Check6Input): CheckResult {
  const id = '⑥';
  const label = 'allowedInstallTargets 격자 — 열 스펙 전량 + verified 불리언 리터럴 (PR-11③)';

  const raw = JSON.parse(readFileSync(input.installTargetsJsonPath, 'utf8')) as unknown[];
  const { rejected } = validateInstallTargetsGrid(raw);

  if (rejected.length === 0) return ok(id, label);
  return fail(
    id,
    label,
    rejected.map((r) => ({ ref: 'PR-11③', message: `install-targets.json 행 거부: ${r.reason} — ${JSON.stringify(r.row)}` }))
  );
}
