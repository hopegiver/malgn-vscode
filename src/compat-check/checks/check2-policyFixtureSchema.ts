// 검사 ② — 정책 fixture의 스키마 통과
// (policy-contract.md §6 "검사 3중" (a)②)
//
// "정책 fixture"는 실제로 배치된 `workstation-profile.json` 파일이 아직 없다(로더가 원문
// 텍스트를 받는 순수 함수라 파일 획득은 이 슬라이스 밖 — W1/W2 계약, loader.ts 주석
// 참고). 대신 policy-contract.md §1이 **"위 스키마의 모든 리프 필드에 이 문서 상단
// 전수 검증표의 행이 대응한다"**고 스스로 선언하는 JSONC 예시 블록을 "정책 fixture"로
// 삼는다 — 문서가 이미 그 블록을 스키마의 정본 표현으로 취급하고 있으므로 새로
// 발명하는 것이 아니라 문서의 자기 서술을 그대로 실행한다.
//
// "스키마 통과"의 기준: `loadPolicyFromText`가 **파일 전체를 거부(status:'rejected')하지
// 않는다.** 개별 필드가 플레이스홀더(`<수집기>` 등)라 issue가 나는 것은 검사②의 대상이
// 아니다 — 그건 loader.ts의 필드별 검증기 몫이고 fieldCoverage.test.ts가 이미 덮는다.
// 검사②가 잡는 것은 "예시 블록 자체가 깨져서 파싱조차 안 되거나 스키마 형태가 바뀌어
// 로더가 통째로 거부하는" 회귀다.

import { readFileSync } from 'node:fs';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { loadPolicyFromText } from '../../core/policy/loader.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { extractFencedBlockContaining, stripJsonComments } from '../lib/textUtils.js';

export interface Check2Input {
  readonly policyContractMdPath: string;
  readonly currentExtensionVersion: string;
}

export function checkPolicyFixtureSchema(input: Check2Input): CheckResult {
  const id = '②';
  const label = '정책 fixture의 스키마 통과';
  const violations: { ref: string; message: string }[] = [];

  const doc = readFileSync(input.policyContractMdPath, 'utf8');
  const block = extractFencedBlockContaining(doc, '"schemaVersion": 1, "generatedAt"');
  if (block === null) {
    return fail(id, label, [{ ref: '§1', message: 'policy-contract.md §1의 정책 스키마 JSONC 예시 블록을 찾을 수 없습니다' }]);
  }

  let rawText: string;
  try {
    // 예시 블록을 다시 JSON 문자열로 직렬화해 loadPolicyFromText(원문 텍스트를 받는
    // 계약)에 그대로 태운다 — JSON.parse→stringify 왕복으로 주석만 제거하고 값은
    // 손대지 않는다.
    rawText = JSON.stringify(JSON.parse(stripJsonComments(block)));
  } catch (error) {
    return fail(id, label, [
      { ref: '§1', message: `정책 스키마 예시 블록이 유효한 JSONC가 아닙니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }

  const constants = loadCodeConstants();
  const result = loadPolicyFromText(rawText, constants, { currentExtensionVersion: input.currentExtensionVersion });
  if (result.status === 'rejected') {
    violations.push({
      ref: '§1',
      message: `정책 스키마 예시가 파일 전체 거부를 유발합니다: ${result.code} — ${result.message}`,
    });
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
