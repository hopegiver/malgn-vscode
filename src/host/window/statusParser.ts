// STATUS.md 파서 — 이 저장소 STATUS.md가 실물 예시(§ "🟢 현재 상태" / "✅ ..." /
// "🚧 ..." / "⛔ ...")다. 프로젝트마다 헤딩 뒤 제목 문구는 제각각일 수 있으므로
// **이모지만으로** 섹션을 식별한다(제목 텍스트는 매칭에 쓰지 않는다). 어떤 이모지도
// 발견되지 않으면 `parsed:false`를 반환하고, 호출자(렌더러)는 `raw`를 그대로 보여주는
// 폴백을 쓴다 — "형식이 제각각이어도 파싱 실패로 죽지 않는다"는 요구사항을 이 함수
// 하나가 책임진다(순수 함수라 Electron 없이도 단위 테스트로 전 케이스를 고정할 수 있다).

const SECTION_EMOJI = {
  current: '🟢',
  recentDone: '✅',
  inProgress: '🚧',
  blocked: '⛔',
} as const;

type SectionKey = keyof typeof SECTION_EMOJI;

export interface ParsedStatusSections {
  readonly current: string | null;
  readonly recentDone: string | null;
  readonly inProgress: string | null;
  readonly blocked: string | null;
  /** 원문 전체 — 파싱 성공 여부와 무관하게 항상 채워진다(폴백 렌더용). */
  readonly raw: string;
  /** 네 섹션 중 하나라도 인식됐으면 true. false면 렌더러는 raw만 보여준다. */
  readonly parsed: boolean;
}

const HEADING_LINE_RE = /^#{1,6}\s+/;

/**
 * 마크다운 헤딩(`#`~`######`) 줄을 이모지로 식별해 각 섹션의 본문(다음 헤딩 전까지)을
 * 추출한다. 같은 이모지가 여러 번 나오면 **첫 번째**만 취한다(STATUS.md는 섹션당 한 번
 * 등장하는 것이 정본 구조 — architecture.md 문서 규율과 동일 관용구).
 */
export function parseStatusMarkdown(content: string): ParsedStatusSections {
  try {
    const lines = content.split('\n');
    const headings: { readonly lineIndex: number; readonly text: string }[] = [];
    lines.forEach((line, lineIndex) => {
      if (HEADING_LINE_RE.test(line)) headings.push({ lineIndex, text: line });
    });

    const sections: Partial<Record<SectionKey, string>> = {};
    for (let i = 0; i < headings.length; i++) {
      const heading = headings[i];
      if (!heading) continue;
      const bodyEnd = headings[i + 1]?.lineIndex ?? lines.length;
      const body = lines.slice(heading.lineIndex + 1, bodyEnd).join('\n').trim();
      for (const key of Object.keys(SECTION_EMOJI) as SectionKey[]) {
        if (key in sections) continue; // 첫 매칭만
        if (heading.text.includes(SECTION_EMOJI[key])) {
          sections[key] = body;
        }
      }
    }

    const parsed = Object.keys(sections).length > 0;
    return {
      current: sections.current ?? null,
      recentDone: sections.recentDone ?? null,
      inProgress: sections.inProgress ?? null,
      blocked: sections.blocked ?? null,
      raw: content,
      parsed,
    };
  } catch {
    // 파싱 실패해도 죽지 않는다 — 원문 폴백만 채워 반환한다.
    return { current: null, recentDone: null, inProgress: null, blocked: null, raw: content, parsed: false };
  }
}
