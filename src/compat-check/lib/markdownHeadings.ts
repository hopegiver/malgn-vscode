// docs/policy-contract.md §7(문서 상호참조 검사)이 요구하는 "실제 헤딩 번호 집합"
// 추출 유틸. architecture.md의 헤딩은 `## 3.5 …` 형태로 번호가 붙어 있고, §7.4/§7.4.1은
// 헤딩이 아니라 표 행이라 별도 규칙("§7.4-<n>은 §7.4 표의 항목 번호로 취급", §7 본문)이
// 적용된다 — 이 모듈이 그 특례를 구현한다.

const HEADING_LINE_RE = /^#{1,6}\s+.+$/gm;
const LEADING_NUMBER_RE = /^#{1,6}\s+(\d+(?:\.\d+)*)\b/;
/** 결합 헤딩("### 5.1 불변량과 관측 · 5.2 [확정] 주입이 아니라 …") 특례 — 이 문서들의
 * 관례는 `·` 뒤에 두 번째 절 번호를 잇는 것이다. 임의 프로즈의 숫자를 오탐하지 않도록
 * `·` 바로 뒤인 경우로 한정한다(전체 라인에서 숫자를 다 긁으면 "1차" 같은 표현의
 * "1"도 헤딩 번호로 오인한다). */
const COMBINED_NUMBER_RE = /·\s*(\d+(?:\.\d+)*)\b/g;

/** `## 3.5 …` 같은 마크다운 헤딩에서 절 번호 집합을 뽑는다. 결합 헤딩(`· 5.2`)도 포함한다. */
export function extractHeadingNumbers(markdown: string): ReadonlySet<string> {
  const result = new Set<string>();
  let lineMatch: RegExpExecArray | null;
  HEADING_LINE_RE.lastIndex = 0;
  while ((lineMatch = HEADING_LINE_RE.exec(markdown)) !== null) {
    const line = lineMatch[0];
    const leading = LEADING_NUMBER_RE.exec(line);
    if (leading?.[1] !== undefined) result.add(leading[1]);

    COMBINED_NUMBER_RE.lastIndex = 0;
    let combined: RegExpExecArray | null;
    while ((combined = COMBINED_NUMBER_RE.exec(line)) !== null) {
      if (combined[1] !== undefined) result.add(combined[1]);
    }
  }
  return result;
}

/**
 * §7.4·§7.4.1 표의 첫 열 항목 번호(`| **6a** |` 형태)를 뽑는다. §7 본문의 해석 규칙
 * "§7.4-<n>은 §7.4 표의 항목 번호로 취급한다"의 정본 구현 — 이 특례가 없으면
 * `§7.4-6a` 같은 흔한 참조가 전부 "존재하지 않는 절"로 오탐된다.
 */
const TABLE_ROW_ITEM_RE = /^\| \*\*([0-9]+[a-zA-Z]?)\*\* \|/gm;

export function extractTableRowItems(markdown: string): ReadonlySet<string> {
  const result = new Set<string>();
  let m: RegExpExecArray | null;
  TABLE_ROW_ITEM_RE.lastIndex = 0;
  while ((m = TABLE_ROW_ITEM_RE.exec(markdown)) !== null) {
    const num = m[1];
    if (num !== undefined) result.add(num);
  }
  return result;
}
