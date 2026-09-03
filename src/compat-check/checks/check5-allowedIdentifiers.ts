// 검사 ⑤C — 문서에 backtick으로 등장하는 모든 `allowed*` 식별자 집합 ⊆ 코드 상수
// 블록·policy-contract.md의 키 집합 (PR-11 ②, M-21 / v0.6 M-14 강화)
// (policy-contract.md §6 "검사 3중" (a)⑤, S4 3분할: docs/policy-contract.md §8.3)
//
// v1.2-split(S4) 이후 이 검사는 **로컬 full 모드 전용**이다(문서 실지 스캔이라 docs/가
// 있어야 실행 가능) — CI에서 강제하는 부분은 ⑤A(스냅샷 식별자 집합 == 상수 키 집합,
// 양방향)와 ⑤B(모든 allowed* 키가 비어 있지 않고 플레이스홀더가 없는지, 문서 언급
// 여부와 무관)로 옮겨졌다. ⑤A가 계약 변경 커밋마다 스냅샷 재생성(§8.5 D1)을 강제하므로
// "계약을 바꾸는 커밋"에 한해 이 검사도 기계적으로 강제된다(문서면 위생, §8.3 ⑤ 행 참고).
//
// v0.6 강화 내용(§0 PR-11② 주석 원문): "각 `allowed*`가 비어 있지 않은 값 노드를
// 갖는가 + 값 어디에도 플레이스홀더(`<...>`) 문자열이 없는가"까지 대조한다. 대조
// 상대는 `loadCodeConstants()`가 조립한 병합 뷰(compatibility.json + 사이트면 + $ref
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
import { containsPlaceholder, isEmptyValue } from '../lib/allowedValueChecks.js';

export interface Check5Input {
  readonly docPaths: readonly string[];
}

const BACKTICK_ALLOWED_RE = /`(allowed[A-Za-z]+)(?:\.[A-Za-z]+)?`/g;

export function checkAllowedIdentifiers(input: Check5Input): CheckResult {
  const id = '⑤C';
  const label = '문서의 allowed* 식별자 ⊆ 코드 상수 키 집합 + 값 실재 + 플레이스홀더 부재 (PR-11②, 로컬 full 전용)';
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
