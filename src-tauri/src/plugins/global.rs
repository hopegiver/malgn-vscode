// ---------------- 카탈로그: 전역(user-level) 에이전트/스킬 (읽기 전용 조회) ----------------
// 플러그인에 묶이지 않은, 개인이 직접 만든 전역 에이전트(`~/.claude/agents/*.md`)와
// 스킬(`~/.claude/skills/*/SKILL.md`)을 조회해 상태표시만 한다. enable/disable, 삭제,
// 편집 기능은 이 기능의 범위 밖이다 — installed.rs(플러그인 카탈로그)와는 별개의
// 조회 대상이다.

use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug)]
pub(crate) struct GlobalEntry {
    name: String,
    description: String,
    path: String,
    status: String,
}

#[derive(Serialize, Debug)]
pub(crate) struct GlobalCatalog {
    agents: Vec<GlobalEntry>,
    skills: Vec<GlobalEntry>,
}

/// YAML 프론트매터에서 최상위 `name:`/`description:` 값만 뽑아낸다. 들여쓰기된
/// 중첩 라인은 무시한다 — 과설계 금지.
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    let first = lines.next()?;
    if first.trim() != "---" {
        return None;
    }

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    for line in lines {
        if line.trim() == "---" {
            break;
        }
        // 최상위 key: value만 취급 — 들여쓰기된 라인은 건너뜀.
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = strip_quotes(value.trim());
        match key {
            "name" => name = Some(value.to_string()),
            "description" => description = Some(value.to_string()),
            _ => {}
        }
    }

    let name = name?;
    if name.is_empty() {
        return None;
    }
    Some((name, description.unwrap_or_default()))
}

fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn read_global_agents(home: &Path) -> Vec<GlobalEntry> {
    let dir = home.join(".claude").join("agents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut results: Vec<GlobalEntry> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            let path_str = path.to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path).ok();

            let (name, description, status) = match content.as_deref().and_then(parse_frontmatter)
            {
                Some((name, description)) => (name, description, "ok".to_string()),
                None => (stem, String::new(), "invalid".to_string()),
            };

            Some(GlobalEntry {
                name,
                description,
                path: path_str,
                status,
            })
        })
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

fn read_global_skills(home: &Path) -> Vec<GlobalEntry> {
    let dir = home.join(".claude").join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut results: Vec<GlobalEntry> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let skill_dir = e.path();
            let dir_name = skill_dir.file_name()?.to_str()?.to_string();
            let dir_path_str = skill_dir.to_string_lossy().to_string();
            let skill_md_path = skill_dir.join("SKILL.md");

            let content = std::fs::read_to_string(&skill_md_path).ok();
            let (name, description, path, status) = match content
                .as_deref()
                .and_then(parse_frontmatter)
            {
                Some((name, description)) => (
                    name,
                    description,
                    skill_md_path.to_string_lossy().to_string(),
                    "ok".to_string(),
                ),
                None => (dir_name, String::new(), dir_path_str, "invalid".to_string()),
            };

            Some(GlobalEntry {
                name,
                description,
                path,
                status,
            })
        })
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

pub(crate) fn read_global_catalog() -> GlobalCatalog {
    let Some(home) = dirs::home_dir() else {
        return GlobalCatalog {
            agents: Vec::new(),
            skills: Vec::new(),
        };
    };

    GlobalCatalog {
        agents: read_global_agents(&home),
        skills: read_global_skills(&home),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_frontmatter() {
        let content = "---\nname: foo\ndescription: bar\n---\n본문";
        assert_eq!(
            parse_frontmatter(content),
            Some(("foo".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn returns_none_when_no_frontmatter() {
        let content = "# 그냥 마크다운\n\n프론트매터 없음";
        assert_eq!(parse_frontmatter(content), None);
    }

    #[test]
    fn returns_none_when_name_missing() {
        let content = "---\ndescription: bar\n---\n본문";
        assert_eq!(parse_frontmatter(content), None);
    }
}
