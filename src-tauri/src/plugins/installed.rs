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

fn entry_items(names: Vec<String>, prefix: &str) -> Vec<CatalogEntryItem> {
    names
        .into_iter()
        .map(|n| CatalogEntryItem {
            id: format!("{prefix}-{n}"),
            name: n,
            description: String::new(),
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

fn knowledge_entry_items(names: Vec<String>) -> Vec<CatalogEntryItem> {
    names
        .into_iter()
        .map(|n| CatalogEntryItem {
            id: format!("knowledge-{}", n.replace('/', "-")),
            name: n,
            description: String::new(),
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

        let agents = entry_items(list_md_file_stems(&install_dir.join("agents")), "agent");
        let skills = entry_items(list_dir_names(&install_dir.join("skills")), "skill");
        let knowledge = knowledge_entry_items(list_knowledge_md_relative_stems(
            &install_dir.join("knowledge"),
        ));

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
    }
}
