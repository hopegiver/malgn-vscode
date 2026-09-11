// ---------------- STATUS.md 파싱 ----------------
// `lib.rs`의 워크스페이스 프로젝트 스캔 절 중 STATUS.md 마크다운 파싱 부분만 그대로
// 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
use serde::Serialize;

#[derive(Serialize, Clone)]
pub(crate) struct ProjectStatusSections {
    pub(crate) parsed: bool,
    pub(crate) current: Option<String>,
    #[serde(rename = "recentDone")]
    pub(crate) recent_done: Option<String>,
    #[serde(rename = "inProgress")]
    pub(crate) in_progress: Option<String>,
    pub(crate) blocked: Option<String>,
    pub(crate) raw: String,
}

struct Heading<'a> {
    line_index: usize,
    text: &'a str,
}

/// `^#{1,6}\s+` 정규식과 동등한 판정 — 1~6개의 `#` 뒤에 공백류가 와야 헤딩으로 본다.
fn is_heading_line(line: &str) -> bool {
    let hash_count = line.chars().take_while(|&c| c == '#').count();
    if hash_count == 0 || hash_count > 6 {
        return false;
    }
    line.chars()
        .nth(hash_count)
        .is_some_and(|c| c.is_whitespace())
}

/// STATUS.md의 "🟢 현재 상태 / ✅ 최근 완료 / 🚧 진행 중 / ⛔ 막힌 것" 섹션을
/// 이모지만으로 식별한다(제목 텍스트는 보지 않는다). 같은 이모지가 여러 번 나오면
/// 첫 번째만 취한다. 어떤 이모지도 없으면 `parsed:false` — 호출자(프론트엔드)가
/// `raw`를 그대로 보여주는 폴백을 쓴다.
pub(crate) fn parse_status_markdown(content: &str) -> ProjectStatusSections {
    let lines: Vec<&str> = content.split('\n').collect();
    let headings: Vec<Heading> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_heading_line(line))
        .map(|(i, line)| Heading {
            line_index: i,
            text: line,
        })
        .collect();

    let mut current: Option<String> = None;
    let mut recent_done: Option<String> = None;
    let mut in_progress: Option<String> = None;
    let mut blocked: Option<String> = None;

    for (i, heading) in headings.iter().enumerate() {
        let body_end = headings
            .get(i + 1)
            .map(|h| h.line_index)
            .unwrap_or(lines.len());
        let body = lines[(heading.line_index + 1)..body_end]
            .join("\n")
            .trim()
            .to_string();

        if current.is_none() && heading.text.contains('🟢') {
            current = Some(body.clone());
        }
        if recent_done.is_none() && heading.text.contains('✅') {
            recent_done = Some(body.clone());
        }
        if in_progress.is_none() && heading.text.contains('🚧') {
            in_progress = Some(body.clone());
        }
        if blocked.is_none() && heading.text.contains('⛔') {
            blocked = Some(body);
        }
    }

    let parsed =
        current.is_some() || recent_done.is_some() || in_progress.is_some() || blocked.is_some();

    ProjectStatusSections {
        parsed,
        current,
        recent_done,
        in_progress,
        blocked,
        raw: content.to_string(),
    }
}

// 구조화된 상태 필드가 없는 자유 텍스트에서 뽑아내는 휴리스틱 — "이 프로젝트가
// 보관/중단됐다"는 의도가 분명한 구(phrase)만 본다(원본 electron 브랜치와 동일한
// 오탐 회피 근거: 한 단어 키워드는 "종료 안 됨" 같은 복합어에서 오탐한다).
const ARCHIVE_KEYWORDS: [&str; 8] = [
    "프로젝트 보관",
    "보관 처리",
    "보관 상태",
    "개발 중단",
    "서비스 종료",
    "운영 종료",
    "archived",
    "deprecated",
];

pub(crate) fn classify_archive_status(
    has_status: bool,
    current_section_body: &Option<String>,
) -> &'static str {
    if !has_status {
        return "unknown";
    }
    match current_section_body {
        None => "unknown",
        Some(body) => {
            let normalized = body.to_lowercase();
            if ARCHIVE_KEYWORDS
                .iter()
                .any(|k| normalized.contains(&k.to_lowercase()))
            {
                "archived"
            } else {
                "active"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_markdown_by_emoji_only() {
        let content = "# 🟢 현재 상태\n진행 중\n\n## ✅ 최근 완료\n완료 항목\n";
        let parsed = parse_status_markdown(content);
        assert!(parsed.parsed);
        assert_eq!(parsed.current.as_deref(), Some("진행 중"));
        assert_eq!(parsed.recent_done.as_deref(), Some("완료 항목"));
        assert_eq!(parsed.in_progress, None);
        assert_eq!(parsed.blocked, None);
    }

    #[test]
    fn falls_back_when_no_known_emoji_heading() {
        let content = "# 아무 형식\n형식이 다른 내용\n";
        let parsed = parse_status_markdown(content);
        assert!(!parsed.parsed);
        assert_eq!(parsed.raw, content);
    }
}
