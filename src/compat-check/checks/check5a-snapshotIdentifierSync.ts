// 검사 ⑤A — 스냅샷의 `allowed*` 식별자 집합 == 코드 상수의 `allowed*` 키 집합 (양방향)
// (docs/policy-contract.md §8.3 "⑤A" / §8.5 D3)
//
// CI에서 도는 부분이다(docs/ 없이도 실행 가능 — 입력이 `compat/contract-snapshot.json`과
// `loadCodeConstants()`뿐이다). 양방향성이 핵심이다: 상수에 `allowed*` 키를 새로 추가하면
// 스냅샷이 낡아 이 검사가 실패하고, 스냅샷 갱신(`pnpm contract:snapshot`)은 docs/가 있어야만
// 가능하다(D1) — 그래서 "계약을 바꾸는 커밋은 문서면 검사를 통과해야만 랜딩된다"(§8.3 ⑤ 행).

import { readFileSync } from 'node:fs';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check5aInput {
  readonly contractSnapshotJsonPath: string;
  /** 스냅샷에는 나타나지만 상수 키가 아닌 것으로 알려진 식별자(문서 산문에만 등장 등).
   * §8.3 ⑤A 원문 "면제목록 명시"의 구현 지점 — 기본은 빈 배열이다. */
  readonly exempt?: readonly string[];
}

interface ContractSnapshot {
  readonly docIdentifiers?: readonly string[];
}

export function checkSnapshotIdentifierSync(input: Check5aInput): CheckResult {
  const id = '⑤A';
  const label = '스냅샷 docIdentifiers == 코드 상수 allowed* 키 집합 (양방향, §8.5 D3)';
  const exempt = new Set(input.exempt ?? []);

  let snapshot: ContractSnapshot;
  try {
    snapshot = JSON.parse(readFileSync(input.contractSnapshotJsonPath, 'utf8')) as ContractSnapshot;
  } catch (error) {
    return fail(id, label, [
      { ref: '§8.5', message: `${input.contractSnapshotJsonPath}를 읽거나 파싱할 수 없습니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }

  const fromSnapshot = new Set((snapshot.docIdentifiers ?? []).filter((name) => !exempt.has(name)));

  const constants = loadCodeConstants() as unknown as Record<string, unknown>;
  const fromConstants = new Set(Object.keys(constants).filter((key) => key.startsWith('allowed')));

  const violations: { ref: string; message: string }[] = [];
  for (const name of fromSnapshot) {
    if (!fromConstants.has(name)) {
      violations.push({ ref: '§8.5 D3', message: `스냅샷에는 있지만 코드 상수 키 집합에는 없습니다: ${name} (contract:snapshot이 낡았을 수 있습니다)` });
    }
  }
  for (const name of fromConstants) {
    if (!fromSnapshot.has(name)) {
      violations.push({ ref: '§8.5 D3', message: `코드 상수에는 있지만 스냅샷 docIdentifiers에는 없습니다: ${name} (\`pnpm contract:snapshot\`을 다시 실행하십시오)` });
    }
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
