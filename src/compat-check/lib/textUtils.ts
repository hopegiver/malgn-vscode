// 문서(마크다운/JSONC) 파싱 공통 유틸 — compat:check의 여러 검사가 공유한다.
// 순수 함수만 둔다(fs 접근은 각 check 진입점이 담당).

/**
 * JSONC(주석 있는 JSON) 주석 제거. 문자열 리터럴 내부의 `//`(예: `https://...` URL)는
 * 보존해야 한다 — 그렇지 않으면 policy-contract.md §1/§2의 JSONC 예시 블록에 있는
 * `"https://..."` 값이 주석으로 오인돼 잘려나간다. 문자열 상태를 추적하는 단순
 * 상태기계로 처리한다(블록 주석 형태는 이 문서들에 등장하지 않아 지원하지 않는다).
 */
export function stripJsonComments(text: string): string {
  let out = '';
  let inString = false;
  let escapeNext = false;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i] ?? '';
    const next = text[i + 1];
    if (inString) {
      out += ch;
      if (escapeNext) {
        escapeNext = false;
      } else if (ch === '\\') {
        escapeNext = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }
    if (ch === '"') {
      inString = true;
      out += ch;
      continue;
    }
    if (ch === '/' && next === '/') {
      while (i < text.length && text[i] !== '\n') i += 1;
      out += '\n';
      continue;
    }
    out += ch;
  }
  return out;
}

/**
 * 마크다운 코드펜스(```jsonc ... ``` 등) 중 `marker` 문자열을 포함하는 첫 블록의
 * 본문(펜스 자체는 제외)을 반환한다. 없으면 null.
 */
export function extractFencedBlockContaining(markdown: string, marker: string): string | null {
  const fenceRe = /```[a-zA-Z]*\n([\s\S]*?)\n```/g;
  let m: RegExpExecArray | null;
  while ((m = fenceRe.exec(markdown)) !== null) {
    const body = m[1] ?? '';
    if (body.includes(marker)) return body;
  }
  return null;
}

/** JSONC 코드펜스 블록을 찾아 주석을 제거하고 JSON.parse한다. 실패 시 null. */
export function parseFencedJsonc(markdown: string, marker: string): unknown | null {
  const block = extractFencedBlockContaining(markdown, marker);
  if (block === null) return null;
  try {
    return JSON.parse(stripJsonComments(block));
  } catch {
    return null;
  }
}
