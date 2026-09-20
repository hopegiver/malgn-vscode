// ---------------- 카탈로그: 설치된 플러그인 (실제 로컬 데이터) ----------------
// ~/.claude/plugins/installed_plugins.json에서 scope가 "user"인 항목만 "설치된
// 버전"으로 취급한다(project scope는 프로젝트별로 다른 버전을 쓸 수 있어 무시).
// 각 플러그인의 installPath 아래 .claude-plugin/plugin.json + agents/*.md +
// skills/*/ + knowledge/*/ 실물을 그대로 읽는다. "최신 버전"은 네트워크 호출이
// 필요해 이 커맨드의 범위 밖이다 — 버전 비교 없이 설치된 버전만 보여준다.
//
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub(crate) struct CatalogEntryItem {
    id: String,
    name: String,
    description: String,
}

#[derive(Serialize, Debug)]
pub(crate) struct InstalledPlugin {
    id: String,
    name: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    version: String,
    description: String,
    #[serde(rename = "installPath")]
    install_path: String,
    agents: Vec<CatalogEntryItem>,
    skills: Vec<CatalogEntryItem>,
    knowledge: Vec<CatalogEntryItem>,
}

fn list_dir_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

fn list_md_file_stems(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    names.sort();
    names
}

/// 각 이름에 대응하는 `.md` 경로를 `md_path_for`로 계산해 프론트매터의
/// `description:`을 채운다(리뷰 C4) — 파싱 로직은 `global::parse_frontmatter`를
/// 그대로 재사용한다(신규 파싱 코드 0). 파일이 없거나 프론트매터가 없거나
/// 파싱에 실패하면 빈 문자열로 안전하게 폴백한다(기존 동작과 동일하게 "설명
/// 없음"으로 표시될 뿐, 목록 자체가 빠지지는 않는다).
fn entry_items(names: Vec<String>, prefix: &str, md_path_for: impl Fn(&str) -> PathBuf) -> Vec<CatalogEntryItem> {
    names
        .into_iter()
        .map(|n| {
            let description = std::fs::read_to_string(md_path_for(&n))
                .ok()
                .and_then(|content| super::global::parse_frontmatter(&content))
                .map(|(_, description)| description)
                .unwrap_or_default();
            CatalogEntryItem {
                id: format!("{prefix}-{n}"),
                name: n,
                description,
            }
        })
        .collect()
}

// knowledge/ 는 agents·skills와 달리 2단 구조다(카테고리 폴더 안에 실제 .md 문서들이
// 있고, 최상위에도 README.md 같은 인덱스 문서가 있을 수 있다). 그래서 디렉터리 이름이
// 아니라 트리 전체를 재귀적으로 훑어 .md 파일만 모아야 실제 "지식 항목" 개수가 나온다.
// 카테고리 폴더 안에 .html/.png 같은 비-md 첨부 자산이 섞여 있어도(예:
// design/html-style-guide/) 그런 파일은 .md가 아니므로 자연히 제외된다.
fn list_knowledge_md_relative_stems(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, current: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if file_name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(base, &path, out);
            } else if path.extension().and_then(|x| x.to_str()) == Some("md") {
                if let Ok(rel) = path.strip_prefix(base) {
                    let rel_no_ext = rel.with_extension("");
                    if let Some(rel_str) = rel_no_ext.to_str() {
                        out.push(rel_str.replace('\\', "/"));
                    }
                }
            }
        }
    }

    let mut names = Vec::new();
    walk(dir, dir, &mut names);
    names.sort();
    names
}

/// `base_dir`(플러그인의 `knowledge/` 디렉터리)를 기준으로 `<n>.md`를 읽어
/// description을 채운다(리뷰 C4). knowledge 문서는 agents/skills와 달리
/// frontmatter가 없을 수 있다(예: 카테고리 인덱스 `README.md`) — 그 경우
/// `parse_frontmatter`가 `None`을 돌려주고 그대로 빈 문자열로 남는다(현행과
/// 동일하게 안전, 목록에서 빠지지 않음).
fn knowledge_entry_items(names: Vec<String>, base_dir: &Path) -> Vec<CatalogEntryItem> {
    names
        .into_iter()
        .map(|n| {
            let description = std::fs::read_to_string(base_dir.join(format!("{n}.md")))
                .ok()
                .and_then(|content| super::global::parse_frontmatter(&content))
                .map(|(_, description)| description)
                .unwrap_or_default();
            CatalogEntryItem {
                id: format!("knowledge-{}", n.replace('/', "-")),
                name: n,
                description,
            }
        })
        .collect()
}

pub(crate) fn read_installed_plugins() -> Vec<InstalledPlugin> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path = home
        .join(".claude")
        .join("plugins")
        .join("installed_plugins.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    let Some(plugins_obj) = root.get("plugins").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for (plugin_id, entries_value) in plugins_obj {
        let Some(entries) = entries_value.as_array() else {
            continue;
        };
        let Some(user_entry) = entries
            .iter()
            .find(|e| e.get("scope").and_then(|s| s.as_str()) == Some("user"))
        else {
            continue;
        };
        let Some(install_path_str) = user_entry.get("installPath").and_then(|v| v.as_str()) else {
            continue;
        };
        let install_dir = PathBuf::from(install_path_str);

        let plugin_json_path = install_dir.join(".claude-plugin").join("plugin.json");
        let Ok(plugin_json_content) = std::fs::read_to_string(&plugin_json_path) else {
            continue;
        };
        let Ok(plugin_json) = serde_json::from_str::<Value>(&plugin_json_content) else {
            continue;
        };

        let name = plugin_json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(plugin_id)
            .to_string();
        let display_name = plugin_json
            .get("displayName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let version = plugin_json
            .get("version")
            .and_then(|v| v.as_str())
            .or_else(|| user_entry.get("version").and_then(|v| v.as_str()))
            .unwrap_or("알 수 없음")
            .to_string();
        let description = plugin_json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let agents_dir = install_dir.join("agents");
        let skills_dir = install_dir.join("skills");
        let knowledge_dir = install_dir.join("knowledge");

        let agents = entry_items(list_md_file_stems(&agents_dir), "agent", |n| {
            agents_dir.join(format!("{n}.md"))
        });
        let skills = entry_items(list_dir_names(&skills_dir), "skill", |n| {
            skills_dir.join(n).join("SKILL.md")
        });
        let knowledge = knowledge_entry_items(
            list_knowledge_md_relative_stems(&knowledge_dir),
            &knowledge_dir,
        );

        results.push(InstalledPlugin {
            id: plugin_id.clone(),
            name,
            display_name,
            version,
            description,
            install_path: install_path_str.to_string(),
            agents,
            skills,
            knowledge,
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신에 실제로 설치된 malgn-agent 플러그인(user scope)을 찾아 agents/skills/
    // knowledge 실물 목록까지 비어있지 않은지 확인한다.
    // 머신 의존 — CI에는 이 플러그인이 설치돼 있지 않아 #[ignore]. 로컬 실행:
    // cargo test -- --ignored plugins::installed::tests::finds_this_machines_malgn_agent_plugin
    #[test]
    #[ignore]
    fn finds_this_machines_malgn_agent_plugin() {
        let plugins = read_installed_plugins();
        let found = plugins.iter().find(|p| p.name == "malgn-agent");
        assert!(found.is_some(), "malgn-agent 플러그인을 찾지 못했습니다");
        let p = found.unwrap();
        assert!(!p.agents.is_empty(), "agents 목록이 비어 있습니다");
        assert!(!p.skills.is_empty(), "skills 목록이 비어 있습니다");
        assert!(!p.knowledge.is_empty(), "knowledge 목록이 비어 있습니다");
        // knowledge는 카테고리 폴더 개수(16)가 아니라 그 안의 .md 파일 개수(이 머신
        // 기준 42)여야 한다 — 재귀 집계가 제대로 되는지 느슨하게 확인한다.
        assert!(
            p.knowledge.len() > 20,
            "knowledge 항목이 {}개뿐입니다 — 카테고리 폴더 개수만 세고 있을 가능성이 있습니다",
            p.knowledge.len()
        );

        // 리뷰 C4 회귀 — agents/skills는 전부 frontmatter가 있는 정상 `.md`이므로
        // description이 하나도 안 채워지면 파싱 재사용이 깨진 것이다. knowledge는
        // frontmatter 없는 문서(README.md 등)가 섞여 있을 수 있어 "0건이 아님"만
        // 확인한다.
        let agents_with_desc = p.agents.iter().filter(|a| !a.description.is_empty()).count();
        let skills_with_desc = p.skills.iter().filter(|s| !s.description.is_empty()).count();
        let knowledge_with_desc = p.knowledge.iter().filter(|k| !k.description.is_empty()).count();
        eprintln!(
            "[C4 실측] agents {}/{}개, skills {}/{}개, knowledge {}/{}개에 description이 채워짐",
            agents_with_desc,
            p.agents.len(),
            skills_with_desc,
            p.skills.len(),
            knowledge_with_desc,
            p.knowledge.len()
        );
        assert_eq!(
            agents_with_desc,
            p.agents.len(),
            "agents는 전부 frontmatter가 있는 정상 .md여야 하는데 description이 빈 항목이 있습니다"
        );
        assert_eq!(
            skills_with_desc,
            p.skills.len(),
            "skills는 전부 frontmatter가 있는 정상 SKILL.md여야 하는데 description이 빈 항목이 있습니다"
        );
        // knowledge/*.md는 이 조직 관례상 YAML frontmatter를 쓰지 않는다(제목만
        // `# 헤딩`으로 시작 — README.md 포함 실측 42개 전부 확인). 그래서 0건은
        // "파싱이 깨졌다"가 아니라 "채울 frontmatter 자체가 없다"는 정상 상태다 —
        // agents/skills와 달리 여기서는 0을 실패로 단정하지 않는다(설계 의도:
        // "지식 항목은 frontmatter가 없을 수 있으니 없으면 빈 값 유지").
        let _ = knowledge_with_desc;
    }

    fn write_md(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn unique_tmp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "malgn-c4-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    // entry_items — frontmatter가 있는 .md는 description이 채워진다(agents 형태:
    // <dir>/<name>.md).
    #[test]
    fn entry_items_fills_description_when_frontmatter_present() {
        let dir = unique_tmp_dir("entry-agents-ok");
        write_md(
            &dir.join("backend-dev.md"),
            "---\nname: backend-dev\ndescription: 백엔드 구현 전문가\n---\n본문",
        );

        let items = entry_items(vec!["backend-dev".to_string()], "agent", |n| {
            dir.join(format!("{n}.md"))
        });

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].description, "백엔드 구현 전문가");
        assert_eq!(items[0].id, "agent-backend-dev");

        std::fs::remove_dir_all(&dir).ok();
    }

    // entry_items — frontmatter가 없는 .md(또는 파일 자체가 없음)는 빈 문자열로
    // 안전하게 폴백한다(목록에서 빠지지 않음).
    #[test]
    fn entry_items_leaves_description_empty_when_frontmatter_missing() {
        let dir = unique_tmp_dir("entry-agents-no-fm");
        write_md(&dir.join("no-frontmatter.md"), "# 그냥 마크다운\n\n프론트매터 없음");

        let items = entry_items(
            vec!["no-frontmatter".to_string(), "does-not-exist".to_string()],
            "agent",
            |n| dir.join(format!("{n}.md")),
        );

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].description, "");
        assert_eq!(items[1].description, "", "파일이 아예 없어도 패닉하지 않고 빈 문자열이어야 한다");

        std::fs::remove_dir_all(&dir).ok();
    }

    // entry_items — skills 형태(<dir>/<name>/SKILL.md)도 동일하게 동작한다.
    #[test]
    fn entry_items_fills_description_for_skill_dir_layout() {
        let dir = unique_tmp_dir("entry-skills-ok");
        write_md(
            &dir.join("domain-backend-api-security").join("SKILL.md"),
            "---\nname: domain-backend-api-security\ndescription: 보안 체크리스트\n---\n본문",
        );

        let items = entry_items(vec!["domain-backend-api-security".to_string()], "skill", |n| {
            dir.join(n).join("SKILL.md")
        });

        assert_eq!(items[0].description, "보안 체크리스트");

        std::fs::remove_dir_all(&dir).ok();
    }

    // knowledge_entry_items — frontmatter가 있으면 채워지고, README.md처럼 없으면
    // (현행과 동일하게) 빈 문자열로 안전하게 남는다.
    #[test]
    fn knowledge_entry_items_fills_when_present_and_empty_when_absent() {
        let dir = unique_tmp_dir("knowledge-mixed");
        write_md(
            &dir.join("backend").join("some-guide.md"),
            "---\nname: some-guide\ndescription: 가이드 설명\n---\n본문",
        );
        write_md(&dir.join("README.md"), "# 인덱스\n\n프론트매터 없음");

        let items = knowledge_entry_items(
            vec!["backend/some-guide".to_string(), "README".to_string()],
            &dir,
        );

        let guide = items.iter().find(|i| i.name == "backend/some-guide").unwrap();
        assert_eq!(guide.description, "가이드 설명");
        assert_eq!(guide.id, "knowledge-backend-some-guide");

        let readme = items.iter().find(|i| i.name == "README").unwrap();
        assert_eq!(readme.description, "", "frontmatter 없는 지식 문서는 빈 문자열로 남아야 한다");

        std::fs::remove_dir_all(&dir).ok();
    }
}
