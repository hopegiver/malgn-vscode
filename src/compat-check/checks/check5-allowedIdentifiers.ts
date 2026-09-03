// 검사 ⑤ — 문서에 backtick으로 등장하는 모든 `allowed*` 식별자 집합 ⊆ 코드 상수
// 블록·policy-contract.md의 키 집합 (PR-11 ②, M-21 / v0.6 M-14 강화)
// (policy-contract.md §6 "검사 3중" (a)⑤)
//
// v0.6 강화 내용(§0 PR-11② 주석 원문): "각 `allowed*`가 비어 있지 않은 값 노드를
// 갖는가 + 값 어디에도 플레이스홀더(`<...>`) 문자열이 없는가"까지 대조한다. 대조
// 상대는 `loadCodeConstants()`가 조립한 병합 뷰(compatibility.json + 3개 $ref
// fixture)다 — 그것이 "코드 상수 블록"의 실행 시점 형태이기 때문이다.
//
// 알려진 설계 갭(작업 지시 원문): "`otel.env` '알려진 키 화이트리스트'에 대응하는
// 코드 상수 정본이 §2에 없다" — 그러나 이 갭은 문서 어디에도
// backtick으로 `allowedOtelEnvKeys` 형태로 등장하지 않는다(grep 확인, 2026-09-03).
// PR-11②는 "문서에 backtick으로 등장하는 식별자"만 대상으로 하므로 이 검사의 스캔
// 대상에 애초에 걸리지 않는다 — 갭이 사라진 것이 아니라 이 검사의 탐지 범위 밖에
// 있을 뿐이다(별도 보고 대상).

import { readFileSync } from 'node:fs';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check5Input {
  readonly docPaths: readonly string[];
}

const BACKTICK_ALLOWED_RE = /`(allowed[A-Za-z]+)(?:\.[A-Za-z]+)?`/g;
const PLACEHOLDER_RE = /<[^>]+>/;

function containsPlaceholder(value: unknown): boolean {
  if (typeof value === 'string') return PLACEHOLDER_RE.test(value);
  if (Array.isArray(value)) return value.some(containsPlaceholder);
  if (value !== null && typeof value === 'object') {
    return Object.values(value as Record<string, unknown>).some(containsPlaceholder);
  }
  return false;
}

function isEmptyValue(value: unknown): boolean {
  if (Array.isArray(value)) return value.length === 0;
  if (value !== null && typeof value === 'object') return Object.keys(value as object).length === 0;
  return value === undefined;
}

export function checkAllowedIdentifiers(input: Check5Input): CheckResult {
  const id = '⑤';
  const label = '문서의 allowed* 식별자 ⊆ 코드 상수 키 집합 + 값 실재 + 플레이스홀더 부재 (PR-11②)';
  const violations: { ref: string; message: string }[] = [];

  const identifiers = new Set<string>();
  for (const docPath of input.docPaths) {
    const text = readFileSync(docPath, 'utf8');
    BACKTICK_ALLOWED_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = BACKTICK_ALLOWED_RE.exec(text)) !== null) {
      const name = m[1];
      if (name !== undefined) identifiers.add(name);
    }
  }

  const constants = loadCodeConstants() as unknown as Record<string, unknown>;

  for (const name of identifiers) {
    if (!(name in constants)) {
      violations.push({ ref: 'PR-11②', message: `문서 식별자 \`${name}\`가 코드 상수 블록에 값 정의가 없습니다` });
      continue;
    }
    const value = constants[name];
    if (isEmptyValue(value)) {
      violations.push({ ref: 'PR-11② (M-14)', message: `\`${name}\`의 값이 비어 있습니다` });
      continue;
    }
    if (containsPlaceholder(value)) {
      violations.push({ ref: 'PR-11② (M-14)', message: `\`${name}\`의 값에 플레이스홀더(<...>) 문자열이 남아 있습니다` });
    }
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
