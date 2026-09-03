// 검사 ② — 정책 fixture의 스키마 통과
// (policy-contract.md §6 "검사 3중" (a)②, S3: docs/policy-contract.md §8.3)
//
// v1.2-split(S3) 이후 입력은 문서 JSONC 블록이 아니라 `compat/policy.sample.json`(실물
// 파일)이다 — 정본 반전(§8.2 ①)으로 "정책 fixture"의 정본이 문서에서 compat/로 옮겨졌다.
// 이 fixture는 원래 policy-contract.md §1이 스스로 "이 스키마의 모든 리프 필드에 전수
// 검증표의 행이 대응한다"고 선언하던 JSONC 예시를 값 그대로(민감 슬롯 없음 — 정책
// 스키마 예시에는 애초에 사내 authority가 없었다) 옮긴 것이다.
//
// "스키마 통과"의 기준: `loadPolicyFromText`가 **파일 전체를 거부(status:'rejected')하지
// 않는다.** 개별 필드가 플레이스홀더(`<수집기>` 등)라 issue가 나는 것은 검사②의 대상이
// 아니다 — 그건 loader.ts의 필드별 검증기 몫이고 fieldCoverage.test.ts가 이미 덮는다.
// 검사②가 잡는 것은 "fixture 자체가 깨져서 파싱조차 안 되거나 스키마 형태가 바뀌어
// 로더가 통째로 거부하는" 회귀다.

import { readFileSync } from 'node:fs';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { loadPolicyFromText } from '../../core/policy/loader.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check2Input {
  readonly policySampleJsonPath: string;
  readonly currentExtensionVersion: string;
}

export function checkPolicyFixtureSchema(input: Check2Input): CheckResult {
  const id = '②';
  const label = '정책 fixture의 스키마 통과';

  let rawText: string;
  try {
    rawText = readFileSync(input.policySampleJsonPath, 'utf8');
    JSON.parse(rawText); // 형태만 사전 확인 — 순수 JSON이라 주석 제거가 필요 없다
  } catch (error) {
    return fail(id, label, [
      { ref: 'compat/policy.sample.json', message: `유효한 JSON이 아닙니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }

  const constants = loadCodeConstants();
  const result = loadPolicyFromText(rawText, constants, { currentExtensionVersion: input.currentExtensionVersion });
  if (result.status === 'rejected') {
    return fail(id, label, [
      { ref: 'compat/policy.sample.json', message: `정책 fixture가 파일 전체 거부를 유발합니다: ${result.code} — ${result.message}` },
    ]);
  }

  return ok(id, label);
}
