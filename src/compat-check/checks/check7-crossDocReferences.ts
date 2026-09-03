// 검사 ⑦ — 위성 문서의 `architecture.md` 절 참조가 실재 헤딩을 가리키는지 (§7, M-18)
// (policy-contract.md §6 "검사 3중" (a)⑦, 상세 규칙은 §7 본문)
//
// 규칙(§7 원문 그대로 구현):
//  - 대상 문서: policy-contract.md·tech-stack.md·malgn-auth-requirements.md·
//    malgnai-hub-requirements.md·README.md(= docs/README.md — "현재 상태를 서술하는
//    문서"라는 대상 정의에 맞는 이 저장소의 문서 지도).
//  - 해석 규칙: ① 같은 줄에서 다른 문서명이 §참조보다 앞서 언급되면 그 문서를 대상으로
//    ② 아니면 자기 문서에 그 번호의 절이 있으면 자기 문서 ③ 아니면 architecture.md.
//    `§7.4-<n>`은 architecture.md §7.4/§7.4.1 표의 항목 번호로 취급한다.
//  - 예외: `구 §<번호>`·`v0.<n> §<번호>`처럼 시점 한정자가 바로 앞에 붙은 참조는 제외.
//  - architecture-changelog.md는 대상에서 뺀다(이력 문서의 절 번호는 기록 당시 좌표).

import { readFileSync } from 'node:fs';
import { basename } from 'node:path';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { extractHeadingNumbers, extractTableRowItems } from '../lib/markdownHeadings.js';

export interface Check7Input {
  /** 대상 문서(스캔 대상) 경로 목록 */
  readonly targetDocPaths: readonly string[];
  /** 절 번호 해석에 쓰이는 문서 레지스트리: 파일명(basename) → 절대경로 */
  readonly docRegistryPaths: Readonly<Record<string, string>>;
  readonly architectureMdFileName: string;
}

const REF_RE = /§(\d+(?:\.\d+)*)(-[0-9A-Za-z]+)?/g;
const KNOWN_DOC_NAMES = [
  'architecture.md',
  'policy-contract.md',
  'tech-stack.md',
  'malgn-auth-requirements.md',
  'malgnai-hub-requirements.md',
  'README.md',
  'security-plan.md',
  'devops-review.md',
];

function isExempt(before: string): boolean {
  const cleaned = before.replace(/[`*]/g, '').trimEnd();
  // "구 §…"(예: "…e.md` 구 §3.7.1") / "(구 §…"(예: "§7.3.1(구 §7.4-1")처럼 "구" 앞에
  // 괄호·공백·문자열 시작이 올 수 있다 — 실측 두 형태 모두 있어 둘 다 허용한다.
  return /(?:^|[\s(])구$/.test(cleaned) || /v0\.\d+$/.test(cleaned);
}

/** 같은 줄에서 §참조 앞에 가장 가까이 등장한 문서명을 찾는다(해석 규칙 ①). */
function findPrecedingDocName(line: string, refIndex: number): string | null {
  let best: { name: string; index: number } | null = null;
  for (const name of KNOWN_DOC_NAMES) {
    let searchFrom = 0;
    for (;;) {
      const idx = line.indexOf(name, searchFrom);
      if (idx === -1 || idx >= refIndex) break;
      if (best === null || idx > best.index) best = { name, index: idx };
      searchFrom = idx + 1;
    }
  }
  return best?.name ?? null;
}

interface DocHeadingInfo {
  readonly headings: ReadonlySet<string>;
  readonly tableItems: ReadonlySet<string>;
}

export function checkCrossDocReferences(input: Check7Input): CheckResult {
  const id = '⑦';
  const label = '위성 문서의 architecture.md 절 참조가 실재 헤딩을 가리키는지 (§7, M-18)';
  const violations: { ref: string; message: string }[] = [];

  const headingCache = new Map<string, DocHeadingInfo>();
  function headingsFor(fileName: string): DocHeadingInfo | null {
    const cached = headingCache.get(fileName);
    if (cached) return cached;
    const path = input.docRegistryPaths[fileName];
    if (path === undefined) return null;
    const text = readFileSync(path, 'utf8');
    const info: DocHeadingInfo = { headings: extractHeadingNumbers(text), tableItems: extractTableRowItems(text) };
    headingCache.set(fileName, info);
    return info;
  }

  for (const docPath of input.targetDocPaths) {
    const selfName = basename(docPath);
    const text = readFileSync(docPath, 'utf8');
    const lines = text.split('\n');

    lines.forEach((line, lineIdx) => {
      REF_RE.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = REF_RE.exec(line)) !== null) {
        const base = m[1];
        const suffix = m[2]; // e.g. "-6a"
        if (base === undefined) continue;
        const matchStart = m.index;
        if (isExempt(line.slice(0, matchStart))) continue;

        const precedingDoc = findPrecedingDocName(line, matchStart);
        let targetDoc: string;
        if (precedingDoc !== null) {
          targetDoc = precedingDoc;
        } else {
          const selfHeadings = headingsFor(selfName);
          if (selfHeadings !== null && selfHeadings.headings.has(base)) {
            targetDoc = selfName;
          } else {
            targetDoc = input.architectureMdFileName;
          }
        }

        const info = headingsFor(targetDoc);
        if (info === null) {
          violations.push({
            ref: '§7',
            message: `${docPath}:${lineIdx + 1} — 참조 대상 문서 "${targetDoc}"를 레지스트리에서 찾을 수 없습니다 (§${base}${suffix ?? ''})`,
          });
          continue;
        }

        // 접미사(`-6a` 등)가 있으면 "§7.4-<n>은 §7.4 표의 항목 번호로 취급한다"는 해석
        // 규칙이 전적으로 적용된다 — base("7.4")가 헤딩으로도 실재한다는 사실(§7.4
        // 섹션 자체의 헤딩)이 항목 번호 검증을 무력화하면 안 된다. 접미사가 없을
        // 때만 순수 헤딩 번호 대조로 판정한다.
        const valid = suffix !== undefined ? info.tableItems.has(suffix.slice(1)) : info.headings.has(base);

        if (!valid) {
          violations.push({
            ref: '§7 (M-18)',
            message: `${docPath}:${lineIdx + 1} — §${base}${suffix ?? ''} 참조가 "${targetDoc}"에 실재하지 않습니다`,
          });
        }
      }
    });
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
