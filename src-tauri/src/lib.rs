use chrono::{DateTime, Local, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

mod autonomy;
mod cli_launcher;
mod cloudflare_integration;
mod dev_tools;
mod github_integration;
mod jira_integration;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 세션 JSON의 `cwd`를 `~/.claude/projects/<이 값>/` 디렉터리명으로 바꾼다 — 이
/// 프로젝트가 실제로 쓰는 규칙(`/`를 전부 `-`로 치환)을 그대로 따른다. 슬래시를
/// 전부 지우기 때문에 결과에는 경로 구분자가 하나도 남지 않는다 — `cwd`에 `..`가
/// 있어도 항상 하나의 평평한(flat) 디렉터리명 문자열이 될 뿐 상위 디렉터리로
/// 빠져나갈 수 없다(경로 트래버설이 구조적으로 불가능하다).
fn sanitize_cwd_for_project_dir(cwd: &str) -> String {
    cwd.replace('/', "-")
}

fn truncate_title(text: &str, max_chars: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.chars().count() <= max_chars {
        cleaned.to_string()
    } else {
        let truncated: String = cleaned.chars().take(max_chars).collect();
        format!("{}…", truncated.trim_end())
    }
}

/// `message.content`에서 제목으로 쓸 텍스트를 뽑는다. 보통 문자열이지만 배열(툴
/// 결과 등 콘텐츠 블록)일 수도 있어 첫 텍스트 블록을 찾는다.
fn extract_text_from_content(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.iter().find_map(|item| {
            let text = item.get("text").and_then(|t| t.as_str())?.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }),
        _ => None,
    }
}

/// 세션의 `sessionId`/`cwd`로 대응하는 대화 로그(jsonl)를 찾아 첫 `"type":"user"`
/// 줄의 메시지 텍스트를 제목으로 뽑는다. 파일이 없거나, user 줄을 못 찾거나,
/// 텍스트를 못 뽑으면 `None` — 호출자가 세션 JSON의 `name` 필드로 폴백한다.
///
/// 파일 전체를 읽지 않는다 — `BufReader`로 줄 단위로 읽다가 첫 `"type":"user"` 줄을
/// 찾는 즉시 멈춘다(대화 로그는 최대 수 MB라 전체 로드를 피한다).
fn find_session_title(session: &Value) -> Option<String> {
    let session_id = session.get("sessionId").and_then(|v| v.as_str())?;
    let cwd = session.get("cwd").and_then(|v| v.as_str())?;
    let home = dirs::home_dir()?;

    let sanitized = sanitize_cwd_for_project_dir(cwd);
    let jsonl_path = home
        .join(".claude")
        .join("projects")
        .join(&sanitized)
        .join(format!("{session_id}.jsonl"));

    let file = std::fs::File::open(&jsonl_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }

        // 첫 "user" 타입 줄을 찾았다 — 텍스트 추출 성공 여부와 무관하게 여기서 멈춘다.
        return value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(extract_text_from_content)
            .map(|t| truncate_title(&t, 46));
    }
    None
}

/// `~/.claude/sessions/*.json` 메타데이터를 읽고, 가능하면 대화 로그에서 뽑은
/// 제목(`title` 필드)을 얹어 반환한다. 경로가 이 함수 안에 고정되어 있어 프론트엔드가
/// 다른 경로를 지정할 방법이 없다(사용자 입력을 받지 않는 커맨드). 대화 전문
/// (`~/.claude/projects/**/*.jsonl`)은 제목 한 줄만 훑고 그 이상은 읽지 않는다 — 다른
/// 여러 프로젝트의 민감한 대화 전체를 프론트엔드로 넘기지 않는다.
///
/// 파일 하나가 없거나 깨져 있어도(JSON 파싱 실패) 그 항목만 건너뛰고 전체 목록은
/// 계속 만든다 — 세션 메타데이터는 외부 프로세스가 계속 쓰고 있을 수 있는 값이라
/// 언제든 깨진 상태로 읽힐 수 있다고 가정한다.
fn read_claude_sessions() -> Vec<Value> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let dir = home.join(".claude").join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if let Some(title) = find_session_title(&value) {
            if let Value::Object(ref mut map) = value {
                map.insert("title".to_string(), Value::String(title));
            }
        }
        sessions.push(value);
    }
    sessions
}

#[tauri::command]
fn list_claude_sessions() -> Vec<Value> {
    read_claude_sessions()
}

// ---------------- 워크스페이스 프로젝트 스캔 ----------------
// electron 브랜치의 src/host/window/{projectScanner,statusParser}.ts 알고리즘을
// 그대로 포팅한다(판별 기준·이모지 섹션 파싱·보관 키워드까지 동일). IPC 대신 Tauri
// 커맨드로 옮긴 것만 다르다. 렌더러가 임의 경로를 넘기는 두 번째 커맨드(원본의
// getProjectDetail(path))는 두지 않는다 — 이 커맨드가 STATUS.md 내용까지 한 번에
// 전부 반환하므로 프론트엔드는 이미 받은 목록에서 path로 찾기만 하면 되고, 그래서
// 원본의 pathGuard.ts(렌더러발 경로 검증)에 해당하는 별도 가드가 애초에 필요 없다
// — 사용자 입력을 받는 fs 커맨드 자체가 없다.

#[derive(Serialize, Clone)]
struct ProjectStatusSections {
    parsed: bool,
    current: Option<String>,
    #[serde(rename = "recentDone")]
    recent_done: Option<String>,
    #[serde(rename = "inProgress")]
    in_progress: Option<String>,
    blocked: Option<String>,
    raw: String,
}

#[derive(Serialize)]
struct WorkspaceProject {
    name: String,
    path: String,
    #[serde(rename = "hasStatus")]
    has_status: bool,
    #[serde(rename = "archiveStatus")]
    archive_status: String,
    sections: Option<ProjectStatusSections>,
    #[serde(rename = "updatedAt")]
    updated_at: i64,
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
fn parse_status_markdown(content: &str) -> ProjectStatusSections {
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

fn classify_archive_status(
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

/// 후보 workspace 루트 목록 — 실제로 존재하는 디렉터리만 반환한다. macOS는
/// `~/workspace` 하나뿐이지만, Windows는 사람마다 `%USERPROFILE%\workspace`와
/// `C:\workspace` 둘 중 하나를 쓰는 걸로 확인돼 둘 다 후보에 넣는다(둘 다 있으면
/// 둘 다 스캔해서 합친다 — 어느 쪽이 "맞는" 것인지 앱이 임의로 고르지 않는다).
pub(crate) fn workspace_roots() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("workspace"));
    }
    #[cfg(windows)]
    {
        candidates.push(PathBuf::from("C:\\workspace"));
    }
    candidates.into_iter().filter(|p| p.is_dir()).collect()
}

/// 주어진 경로의 수정시각(mtime)을 UNIX epoch 밀리초로 반환한다. 메타데이터
/// 조회 실패, `modified()` 미지원, 또는 `SystemTime`이 UNIX_EPOCH보다 이전인
/// 클럭 이상 등 어떤 이유로든 값을 구할 수 없으면 패닉 대신 `None`을 반환한다
/// (호출자가 조용히 폴백한다).
fn mtime_millis(path: &std::path::Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

/// 각 workspace 루트 바로 아래 1단계 디렉터리만 스캔한다. `CLAUDE.md`가 있으면
/// malgn-agent 프로젝트로 인식한다(STATUS.md 유무는 판별 기준이 아니다 — 없으면
/// `archiveStatus:"unknown"` + `hasStatus:false`로 접는다, 추측 분류 금지).
fn scan_workspace_projects() -> Vec<WorkspaceProject> {
    let mut results: Vec<WorkspaceProject> = Vec::new();
    for workspace_root in workspace_roots() {
        let Ok(entries) = std::fs::read_dir(&workspace_root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }

            if !path.join("CLAUDE.md").is_file() {
                continue;
            }

            let status_path = path.join("STATUS.md");
            let has_status = status_path.is_file();

            let (archive_status, sections) = if has_status {
                match std::fs::read_to_string(&status_path) {
                    Ok(content) => {
                        let parsed_sections = parse_status_markdown(&content);
                        let status =
                            classify_archive_status(true, &parsed_sections.current).to_string();
                        (status, Some(parsed_sections))
                    }
                    Err(_) => ("unknown".to_string(), None),
                }
            } else {
                ("unknown".to_string(), None)
            };

            let claude_md_mtime = mtime_millis(&path.join("CLAUDE.md"));
            let status_mtime = if has_status {
                mtime_millis(&status_path)
            } else {
                None
            };
            let updated_at = claude_md_mtime
                .into_iter()
                .chain(status_mtime)
                .max()
                .unwrap_or(0);

            results.push(WorkspaceProject {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                has_status,
                archive_status,
                sections,
                updated_at,
            });
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

#[tauri::command]
fn list_workspace_projects() -> Vec<WorkspaceProject> {
    scan_workspace_projects()
}

// ---------------- 개발 환경 실설치/업데이트 ----------------
// `check_dev_tools`(읽기 전용 버전 조회) + 실제 install/update 실행 엔진은
// `dev_tools` 모듈로 분리했다 — 정본은 프로젝트 루트
// `scratch-design-devtools-update.md`(결정 1~6 + 부록 A/B/C). 이 파일에는
// 커맨드 재노출(아래 invoke_handler)만 남긴다.

// ---------------- 카탈로그: 설치된 플러그인 (실제 로컬 데이터) ----------------
// ~/.claude/plugins/installed_plugins.json에서 scope가 "user"인 항목만 "설치된
// 버전"으로 취급한다(project scope는 프로젝트별로 다른 버전을 쓸 수 있어 무시).
// 각 플러그인의 installPath 아래 .claude-plugin/plugin.json + agents/*.md +
// skills/*/ + knowledge/*/ 실물을 그대로 읽는다. "최신 버전"은 네트워크 호출이
// 필요해 이 커맨드의 범위 밖이다 — 버전 비교 없이 설치된 버전만 보여준다.

#[derive(Serialize, Clone, Debug)]
struct CatalogEntryItem {
    id: String,
    name: String,
    description: String,
}

#[derive(Serialize, Debug)]
struct InstalledPlugin {
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

fn read_installed_plugins() -> Vec<InstalledPlugin> {
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
        let knowledge = entry_items(list_dir_names(&install_dir.join("knowledge")), "knowledge");

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

#[tauri::command]
fn list_installed_plugins() -> Vec<InstalledPlugin> {
    read_installed_plugins()
}

// ---------------- 마켓플레이스 (실제 로컬 데이터) ----------------

#[derive(Serialize, Debug)]
struct MarketplaceInfo {
    id: String,
    repo: Option<String>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<String>,
}

fn read_known_marketplaces() -> Vec<MarketplaceInfo> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path = home
        .join(".claude")
        .join("plugins")
        .join("known_marketplaces.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    let Some(obj) = root.as_object() else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for (id, value) in obj {
        let repo = value
            .get("source")
            .and_then(|s| s.get("repo"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        let last_updated = value
            .get("lastUpdated")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        results.push(MarketplaceInfo {
            id: id.clone(),
            repo,
            last_updated,
        });
    }
    results.sort_by(|a, b| a.id.cmp(&b.id));
    results
}

#[tauri::command]
fn list_known_marketplaces() -> Vec<MarketplaceInfo> {
    read_known_marketplaces()
}

// ---------------- OTel 설정 (읽기 전용 실데이터) ----------------
// ~/.claude/settings.json은 권한·훅 등 OTel과 무관한 설정도 담고 있어 파일 전체를
// 읽어 보여주지 않는다 — env 객체에서 키 이름이 "OTEL_"로 시작하는 것만 골라
// 반환한다. 읽기 전용이고, 프론트엔드의 "저장" 버튼은 이 값을 이 파일에 다시 쓰지
// 않는다(그 화면은 여전히 목업 — 전역 설정 파일을 잘못 건드리면 Claude Code 자체
// 동작에 영향을 준다).
fn collect_otel_env() -> std::collections::BTreeMap<String, String> {
    let Some(home) = dirs::home_dir() else {
        return Default::default();
    };
    let path = home.join(".claude").join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Default::default();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Default::default();
    };
    let Some(env) = root.get("env").and_then(|v| v.as_object()) else {
        return Default::default();
    };

    let mut result = std::collections::BTreeMap::new();
    for (key, value) in env {
        if !key.starts_with("OTEL_") {
            continue;
        }
        if let Some(s) = value.as_str() {
            result.insert(key.clone(), s.to_string());
        }
    }
    result
}

#[tauri::command]
fn read_otel_env() -> std::collections::BTreeMap<String, String> {
    collect_otel_env()
}

// ---------------- 플러그인 실제 업데이트 (사용자 명시 승인) ----------------
// `claude` CLI를 셸을 거치지 않고 프로그램명+인자 배열로 직접 실행한다(인젝션
// 경로 없음). 인자는 항상 이 파일이 이미 읽어둔 신뢰할 수 있는 값(설치된 플러그인
// id·마켓플레이스 id)만 들어온다 — 프론트엔드에 자유 텍스트 입력 필드가 없어
// 임의 문자열이 인자로 들어갈 경로 자체가 없다.
#[derive(Serialize, Debug)]
struct CommandResult {
    success: bool,
    message: String,
}

fn run_claude_command(args: &[&str]) -> CommandResult {
    match std::process::Command::new("claude").args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                CommandResult {
                    success: true,
                    message: if stdout.is_empty() {
                        "완료되었습니다.".to_string()
                    } else {
                        stdout
                    },
                }
            } else {
                let msg = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    "알 수 없는 오류로 실패했습니다.".to_string()
                };
                CommandResult {
                    success: false,
                    message: msg,
                }
            }
        }
        Err(e) => CommandResult {
            success: false,
            message: format!("claude 명령을 실행할 수 없습니다: {e}"),
        },
    }
}

/// `plugin_id`는 `installed_plugins.json`의 키(예: "malgn-agent@malgnsoft-plugins")
/// 형식 그대로 `claude plugin update`에 넘긴다. 성공해도 재시작해야 적용된다 —
/// 프론트엔드가 안내 문구를 붙인다.
#[tauri::command]
fn update_plugin(plugin_id: String) -> CommandResult {
    run_claude_command(&["plugin", "update", &plugin_id])
}

#[tauri::command]
fn refresh_marketplaces() -> CommandResult {
    run_claude_command(&["plugin", "marketplace", "update"])
}

// ---------------- 프로젝트 파일 트리 + 미리보기 (읽기 전용) ----------------
// electron 브랜치 src/host/window/pathGuard.ts의 "워크스페이스 루트 바로 아래
// 1단계 자식만 허용" 검증을 포팅한다 — project_path가 ~/workspace 바깥이나 다단계
// 하위 경로를 가리키지 못하게 막는다. relative_path는 canonicalize() 기반
// starts_with 검사로 추가 방어한다(심볼릭 링크·`..`까지 실제로 해석해서 막는다 —
// 문자열 비교보다 강하다).

fn is_direct_child_of_workspace_root(workspace_root: &Path, candidate: &Path) -> bool {
    let Ok(resolved_root) = workspace_root.canonicalize() else {
        return false;
    };
    let Ok(resolved_candidate) = candidate.canonicalize() else {
        return false;
    };
    let Ok(rel) = resolved_candidate.strip_prefix(&resolved_root) else {
        return false;
    };
    !rel.as_os_str().is_empty() && rel.components().count() == 1
}

pub(crate) fn resolve_validated_project_root(project_path: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(project_path);
    let is_child_of_any_root = workspace_roots()
        .iter()
        .any(|root| is_direct_child_of_workspace_root(root, &candidate));
    if !is_child_of_any_root {
        return None;
    }
    candidate.canonicalize().ok()
}

#[derive(Serialize, Debug)]
struct TreeNode {
    name: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "isDirectory")]
    is_directory: bool,
    children: Option<Vec<TreeNode>>,
    truncated: bool,
}

const EXCLUDED_DIR_NAMES: [&str; 9] = [
    "node_modules",
    ".git",
    "dist",
    "target",
    ".next",
    "build",
    ".venv",
    "__pycache__",
    ".cache",
];
const MAX_TREE_DEPTH: usize = 4;

fn build_tree(dir: &Path, rel_prefix: &str, depth: usize) -> Vec<TreeNode> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<TreeNode> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') && name != ".gitignore" {
            continue;
        }
        let relative_path = if rel_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let is_dir = path.is_dir();

        if is_dir {
            if EXCLUDED_DIR_NAMES.contains(&name) {
                nodes.push(TreeNode {
                    name: name.to_string(),
                    relative_path,
                    is_directory: true,
                    children: Some(Vec::new()),
                    truncated: true,
                });
                continue;
            }
            if depth >= MAX_TREE_DEPTH {
                nodes.push(TreeNode {
                    name: name.to_string(),
                    relative_path,
                    is_directory: true,
                    children: None,
                    truncated: true,
                });
                continue;
            }
            let children = build_tree(&path, &relative_path, depth + 1);
            nodes.push(TreeNode {
                name: name.to_string(),
                relative_path,
                is_directory: true,
                children: Some(children),
                truncated: false,
            });
        } else {
            nodes.push(TreeNode {
                name: name.to_string(),
                relative_path,
                is_directory: false,
                children: None,
                truncated: false,
            });
        }
    }
    nodes.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    nodes
}

#[tauri::command]
fn list_project_tree(project_path: String) -> Vec<TreeNode> {
    let Some(root) = resolve_validated_project_root(&project_path) else {
        return Vec::new();
    };
    build_tree(&root, "", 0)
}

#[derive(Serialize, Debug)]
#[serde(tag = "kind")]
enum FilePreview {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "tooLarge")]
    TooLarge { size: u64 },
    #[serde(rename = "binary")]
    Binary,
    #[serde(rename = "notFound")]
    NotFound,
    #[serde(rename = "denied")]
    Denied,
}

const MAX_PREVIEW_BYTES: u64 = 512 * 1024;
const BINARY_EXTENSIONS: [&str; 20] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "pdf", "zip", "gz", "tar", "woff",
    "woff2", "ttf", "eot", "exe", "dll", "so", "dylib",
];

#[tauri::command]
fn read_project_file(project_path: String, relative_path: String) -> FilePreview {
    let Some(root) = resolve_validated_project_root(&project_path) else {
        return FilePreview::Denied;
    };

    let candidate = root.join(&relative_path);
    let Ok(canon_candidate) = candidate.canonicalize() else {
        return FilePreview::NotFound;
    };
    if !canon_candidate.starts_with(&root) {
        return FilePreview::Denied;
    }
    if !canon_candidate.is_file() {
        return FilePreview::NotFound;
    }

    let Ok(metadata) = std::fs::metadata(&canon_candidate) else {
        return FilePreview::NotFound;
    };
    if metadata.len() > MAX_PREVIEW_BYTES {
        return FilePreview::TooLarge {
            size: metadata.len(),
        };
    }

    let looks_binary_ext = canon_candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false);
    if looks_binary_ext {
        return FilePreview::Binary;
    }

    let Ok(bytes) = std::fs::read(&canon_candidate) else {
        return FilePreview::NotFound;
    };
    let sample_len = bytes.len().min(8000);
    if bytes[..sample_len].contains(&0) {
        return FilePreview::Binary;
    }

    match String::from_utf8(bytes) {
        Ok(content) => FilePreview::Text { content },
        Err(_) => FilePreview::Binary,
    }
}

// ---------------- 사용량 통계: 일별 사용량 (실제 로컬 데이터) ----------------
// ~/.claude/projects/**/*.jsonl 전체(모든 프로젝트)를 대상으로 "type":"assistant"
// 줄의 message.usage(입력/출력/캐시 토큰)와 최상위 timestamp만 읽는다. 대화 내용
// (content)은 절대 읽지 않는다 — 순수 숫자 집계다. 대화 로그는 append-only라
// 파일 수정시각이 그 파일의 가장 최신 줄 시각과 같다는 성질을 이용해, 30일보다
// 오래 전에 마지막으로 수정된 파일은 통째로 건너뛴다(열지도 않는다) — 그 다음
// 줄 단위 필터(타임스탬프 30일 이전이면 스킵)로 범위를 다시 좁힌다. `BufReader`로
// 줄 단위 스트리밍한다 — 파일 전체를 메모리에 올리지 않는다.
//
// 어제까지는 다시 바뀔 수 없는 확정값이라 HISTORICAL_USAGE_CACHE에 담아 날짜가
// 바뀌기 전까지 재사용하고(앱 시작 시 백그라운드로 미리 채워둠), 오늘만 호출마다
// 새로 스캔한다(대상 파일이 훨씬 적어 가볍다). 두 경우 다 파일 단위로 rayon
// 병렬 스캔한다 — 파일마다 독립이라 합치기 쉽다.

const USAGE_LOOKBACK_DAYS: i64 = 30;

#[derive(Serialize, Debug, Default, Clone)]
struct DailyUsage {
    date: String,
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
    #[serde(rename = "cacheCreationTokens")]
    cache_creation_tokens: u64,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: u64,
}

fn parse_iso_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn local_date_key(dt: &DateTime<Utc>) -> String {
    let local: DateTime<Local> = dt.with_timezone(&Local);
    local.format("%Y-%m-%d").to_string()
}

fn find_recent_jsonl_files(
    dir: &Path,
    cutoff_mtime: std::time::SystemTime,
    out: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_recent_jsonl_files(&path, cutoff_mtime, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        // 파일 전체가 30일보다 오래 전에 마지막으로 수정됐으면(=append-only 로그의
        // 마지막 줄조차 30일 이전) 열어보지도 않고 건너뛴다.
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff_mtime {
                    continue;
                }
            }
        }
        out.push(path);
    }
}

// 파일 하나를 스캔해 그 파일 안에서 나온 날짜별 부분합을 반환한다(다른 파일과
// 독립이라 rayon으로 파일 단위 병렬화하기 좋다). `skip_date`가 있으면 그 날짜
// 줄은 제외한다 — "오늘"은 매번 별도로 실시간 스캔하므로 과거분 캐시 계산에서는
// 오늘 줄을 빼서 이중 집계를 막는다.
fn scan_file_daily_usage(
    path: &Path,
    cutoff_dt: DateTime<Utc>,
    skip_date: Option<&str>,
) -> std::collections::BTreeMap<String, DailyUsage> {
    let mut buckets: std::collections::BTreeMap<String, DailyUsage> =
        std::collections::BTreeMap::new();
    let Ok(f) = std::fs::File::open(path) else {
        return buckets;
    };
    // 스트리밍 응답이 여러 JSONL 줄로 쪼개져 기록될 때 같은 message.id가 반복
    // 등장하며 매번 그 턴의 usage를 그대로 다시 실어 나른다(실측: 한 세션에서
    // 고유 메시지 124개인데 usage가 실린 줄은 238개 — 거의 2배 중복). id별로
    // 파일 안에서 한 번만 센다 — 안 세면 토큰이 최대 ~2배 부풀려진다.
    let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(ts) = value
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_timestamp)
        else {
            continue;
        };
        if ts < cutoff_dt {
            continue;
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            if !seen_message_ids.insert(id.to_string()) {
                continue;
            }
        }
        let Some(usage) = msg.get("usage") else {
            continue;
        };

        let key = local_date_key(&ts);
        if skip_date.is_some_and(|d| d == key) {
            continue;
        }
        let entry = buckets.entry(key.clone()).or_insert_with(|| DailyUsage {
            date: key,
            ..Default::default()
        });
        entry.input_tokens += usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.output_tokens += usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.cache_creation_tokens += usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.cache_read_tokens += usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    }
    buckets
}

fn merge_daily_usage_maps(
    mut a: std::collections::BTreeMap<String, DailyUsage>,
    b: std::collections::BTreeMap<String, DailyUsage>,
) -> std::collections::BTreeMap<String, DailyUsage> {
    for (key, v) in b {
        let entry = a.entry(key.clone()).or_insert_with(|| DailyUsage {
            date: key,
            ..Default::default()
        });
        entry.input_tokens += v.input_tokens;
        entry.output_tokens += v.output_tokens;
        entry.cache_creation_tokens += v.cache_creation_tokens;
        entry.cache_read_tokens += v.cache_read_tokens;
    }
    a
}

fn today_local_date_key() -> String {
    local_date_key(&Utc::now())
}

// 어제까지의 집계는 성격상 다시는 안 바뀐다(과거 날짜로 새 줄이 추가될 리 없다)
// — 그래서 무효화 로직 없이 "오늘 날짜가 바뀌기 전까지" 그냥 재사용해도 된다.
// 앱을 오래 켜둔 채 자정을 넘기면 다음 호출에서 cached_as_of가 어긋난 걸 감지해
// 하루 한 번만 다시 계산한다.
struct HistoricalUsageCache {
    cached_as_of: String,
    days: Vec<DailyUsage>,
}

static HISTORICAL_USAGE_CACHE: std::sync::Mutex<Option<HistoricalUsageCache>> =
    std::sync::Mutex::new(None);

// 30일 룩백 범위에서 "오늘"을 뺀 나머지(어제까지)를 파일 단위로 병렬 스캔한다.
// 파일이 서로 독립이라 rayon의 파일별 par_iter + reduce로 코어 수만큼 나눠
// 읽는다 — 이 부분이 전체 스캔 시간의 대부분을 차지했다.
fn compute_historical_daily_usage(today: &str) -> Vec<DailyUsage> {
    use rayon::prelude::*;

    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return Vec::new();
    }

    let lookback = std::time::Duration::from_secs(USAGE_LOOKBACK_DAYS as u64 * 24 * 60 * 60);
    let cutoff_mtime = std::time::SystemTime::now()
        .checked_sub(lookback)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

    let mut files = Vec::new();
    find_recent_jsonl_files(&projects_dir, cutoff_mtime, &mut files);

    let buckets = files
        .par_iter()
        .map(|file| scan_file_daily_usage(file, cutoff_dt, Some(today)))
        .reduce(std::collections::BTreeMap::new, merge_daily_usage_maps);

    buckets.into_values().collect()
}

fn get_or_refresh_historical_daily_usage() -> Vec<DailyUsage> {
    let today = today_local_date_key();
    {
        let guard = HISTORICAL_USAGE_CACHE.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if cache.cached_as_of == today {
                return cache.days.clone();
            }
        }
    }
    let days = compute_historical_daily_usage(&today);
    *HISTORICAL_USAGE_CACHE.lock().unwrap() = Some(HistoricalUsageCache {
        cached_as_of: today,
        days: days.clone(),
    });
    days
}

// "오늘"만 매번 새로 스캔한다 — mtime 컷오프를 오늘 자정으로 좁혀서 대상 파일
// 자체가 훨씬 적다(과거분처럼 캐싱하면 방금 쓴 토큰이 안 보이니 여기만 캐시하지
// 않는다).
fn compute_today_daily_usage(today: &str) -> Option<DailyUsage> {
    use rayon::prelude::*;

    let Some(home) = dirs::home_dir() else {
        return None;
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return None;
    }
    let cutoff_mtime =
        local_midnight_as_system_time(today).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

    let mut files = Vec::new();
    find_recent_jsonl_files(&projects_dir, cutoff_mtime, &mut files);

    let buckets = files
        .par_iter()
        .map(|file| scan_file_daily_usage(file, cutoff_dt, None))
        .reduce(std::collections::BTreeMap::new, merge_daily_usage_maps);

    buckets
        .into_iter()
        .find(|(date, _)| date == today)
        .map(|(_, v)| v)
}

fn aggregate_daily_usage() -> Vec<DailyUsage> {
    let today = today_local_date_key();
    let mut buckets: std::collections::BTreeMap<String, DailyUsage> =
        get_or_refresh_historical_daily_usage()
            .into_iter()
            .map(|d| (d.date.clone(), d))
            .collect();
    if let Some(today_bucket) = compute_today_daily_usage(&today) {
        buckets.insert(today.clone(), today_bucket);
    }
    buckets.into_values().collect()
}

#[tauri::command]
fn get_daily_usage() -> Vec<DailyUsage> {
    aggregate_daily_usage()
}

// ---------------- 사용량 통계: 일별 상세 (특정 날짜 하루치 세션/에이전트/툴 랭킹) ----------------
// 단가표는 ~/workspace/malgnai/server/lib/pricing.js를 그대로 옮긴 것이다(가족별
// USD/1M 토큰 — opus/sonnet/haiku, 캐시쓰기·캐시읽기 별도 단가, 미매칭 모델은
// sonnet 폴백). 세션·서브에이전트 조인 개념은 ~/workspace/malgnai/bin/sync-claude.js
// readSessionUsage()(308~437줄)를 따르되, agentId→agent_type 매핑 방식만 이
// 머신의 실제 온디스크 포맷에 맞게 바꿨다: 원본은 "부모의 Agent/Task tool_use
// input.prompt"와 "서브에이전트 첫 메시지 텍스트"를 문자열로 매칭해서 타입을
// 추론하지만(그 환경은 서브에이전트가 부모와 같은 파일에 isSidechain:true로 인라인
// 기록됨), 이 Claude Code 버전은 서브에이전트를 별도 파일
// `<sessionId>/subagents/agent-<agentId>.jsonl` + 같은 이름의 `.meta.json`으로
// 저장하고 그 meta.json에 `agentType` 필드를 이미 직접 담고 있다 — 그래서 프롬프트
// 텍스트 매칭보다 훨씬 신뢰할 수 있는 직접 필드 읽기로 대체했다(개념은 "무엇이
// 서브에이전트였고 어떤 타입이었는지 조인한다"로 동일, 구현 방법만 실제 데이터
// 구조에 맞춰 적응).
//
// "일별 사용량"(최근 30일 요약, 가벼움)에서 특정 날짜를 클릭했을 때만 호출되는
// 상세 API다 — 60일 전체를 매번 스캔하던 이전 "토큰 도둑" 방식과 달리 딱 하루만
// 본다: 파일 mtime이 요청받은 날짜의 로컬 자정보다 이전이면 열어보지도 않고
// 건너뛰고, 연 파일 안에서도 각 줄의 timestamp를 로컬 날짜로 바꿔 요청 날짜와
// 일치하는 줄만 집계한다.

struct ModelPrice {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

fn model_family(model: &str) -> &'static str {
    let m = model.to_lowercase();
    if m.contains("opus") {
        "opus"
    } else if m.contains("haiku") {
        "haiku"
    } else {
        // sonnet이거나 미매칭 — pricing.js의 modelFamily()와 동일하게 sonnet 폴백.
        "sonnet"
    }
}

fn price_for_family(family: &str) -> ModelPrice {
    match family {
        "opus" => ModelPrice {
            input: 15.0,
            output: 75.0,
            cache_write: 18.75,
            cache_read: 1.5,
        },
        "haiku" => ModelPrice {
            input: 1.0,
            output: 5.0,
            cache_write: 1.25,
            cache_read: 0.1,
        },
        _ => ModelPrice {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.3,
        },
    }
}

fn cost_for_usage(
    model: &str,
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
) -> f64 {
    let p = price_for_family(model_family(model));
    const M: f64 = 1_000_000.0;
    (input as f64 / M) * p.input
        + (output as f64 / M) * p.output
        + (cache_creation as f64 / M) * p.cache_write
        + (cache_read as f64 / M) * p.cache_read
}

#[derive(Default)]
struct UsageTally {
    turns: u32,
    tokens: u64,
    cost: f64,
}

// 이 시점부터는 프로덕션 경로에서 직접 호출되지 않는다(`get_daily_detail`은
// 날짜 필터·tool_use 집계까지 겸하는 `scan_usage_lines_for_date`를 쓴다) —
// 다만 message.id 중복 제거 계약 자체를 고정하는 회귀 테스트가 이 함수를
// 직접 검증하므로 테스트 빌드에서만 컴파일한다.
#[cfg(test)]
fn scan_usage_lines(path: &Path, tally: &mut UsageTally) {
    let Ok(f) = std::fs::File::open(path) else {
        return;
    };
    // aggregate_daily_usage()와 같은 이유로 message.id 기준 중복 제거가 필요하다 —
    // 스트리밍 응답이 여러 줄로 쪼개져 기록되며 같은 턴의 usage가 반복된다.
    let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            if !seen_message_ids.insert(id.to_string()) {
                continue;
            }
        }
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        let Some(usage) = msg.get("usage") else {
            continue;
        };
        let input = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_creation = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        tally.turns += 1;
        tally.tokens += input + output + cache_creation + cache_read;
        tally.cost += cost_for_usage(model, input, output, cache_creation, cache_read);
    }
}

fn first_user_title_from_file(path: &Path) -> String {
    let Ok(f) = std::fs::File::open(path) else {
        return String::new();
    };
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = value.get("message").and_then(|m| m.get("content")) {
            if let Some(text) = extract_text_from_content(content) {
                return truncate_title(&text, 80);
            }
        }
        return String::new();
    }
    String::new()
}

#[derive(Serialize, Debug, Clone)]
struct ToolUsage {
    #[serde(rename = "toolName")]
    tool_name: String,
    count: u32,
}

#[derive(Serialize, Debug, Clone)]
struct AgentUsage {
    // 세션 본인의 턴은 "main", 서브에이전트는 meta.json의 agentType.
    #[serde(rename = "agentType")]
    agent_type: String,
    turns: u32,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
}

#[derive(Serialize, Debug, Clone)]
struct SessionDetail {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "projectKey")]
    project_key: String,
    title: String,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
    agents: Vec<AgentUsage>,
    tools: Vec<ToolUsage>,
}

#[derive(Serialize, Debug, Clone, Default)]
struct DailyDetailReport {
    date: String,
    sessions: Vec<SessionDetail>,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
}

/// 상위 개수 제한 없이 툴 사용 랭킹에 올릴 최대 항목 수.
const DAILY_DETAIL_TOP_TOOLS: usize = 10;

/// `scan_usage_lines`와 같은 message.id 중복 제거를 적용하되, 요청받은 로컬
/// 날짜와 일치하는 줄만 집계하고 tool_use 블록 사용 횟수도 함께 센다(같은
/// 파일 안에서 세션의 메인 트랜스크립트와 서브에이전트 트랜스크립트를 모두
/// 이 함수로 훑어 `tool_counts`에 합산한다).
fn scan_usage_lines_for_date(
    path: &Path,
    date: &str,
    tally: &mut UsageTally,
    tool_counts: &mut std::collections::HashMap<String, u32>,
) {
    let Ok(f) = std::fs::File::open(path) else {
        return;
    };
    let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(ts) = value
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_timestamp)
        else {
            continue;
        };
        if local_date_key(&ts) != date {
            continue;
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        // 스트리밍 응답이 여러 줄로 쪼개져 usage/tool_use가 반복 기록되는 것을
        // 막는다 — id당 한 번만 usage와 tool_use를 센다.
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            if !seen_message_ids.insert(id.to_string()) {
                continue;
            }
        }

        if let Some(usage) = msg.get("usage") {
            let model = msg
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            tally.turns += 1;
            tally.tokens += input + output + cache_creation + cache_read;
            tally.cost += cost_for_usage(model, input, output, cache_creation, cache_read);
        }

        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                    continue;
                }
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    *tool_counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}

/// 요청받은 로컬 날짜("YYYY-MM-DD")의 로컬 자정을 `SystemTime`으로 바꾼다 —
/// 파일 mtime이 이보다 이전이면 그 날짜의 활동을 담고 있을 수 없으므로 열어보지
/// 않고 건너뛰는 컷오프로 쓴다. 날짜 파싱에 실패하면 `None`.
fn local_midnight_as_system_time(date: &str) -> Option<std::time::SystemTime> {
    let naive_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let naive_midnight = naive_date.and_hms_opt(0, 0, 0)?;
    let local_midnight = match Local.from_local_datetime(&naive_midnight) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(dt, _) => dt,
        chrono::LocalResult::None => return None,
    };
    Some(local_midnight.with_timezone(&Utc).into())
}

fn aggregate_daily_detail(date: &str) -> DailyDetailReport {
    let mut report = DailyDetailReport {
        date: date.to_string(),
        ..Default::default()
    };

    let Some(home) = dirs::home_dir() else {
        return report;
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return report;
    }
    let Some(cutoff_mtime) = local_midnight_as_system_time(date) else {
        return report;
    };

    let is_recent_enough = |path: &Path| -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|modified| modified >= cutoff_mtime)
            .unwrap_or(true)
    };

    let mut sessions: Vec<SessionDetail> = Vec::new();

    let Ok(project_entries) = std::fs::read_dir(&projects_dir) else {
        return report;
    };
    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let Some(project_key) = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            continue;
        };

        let Ok(session_entries) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for session_entry in session_entries.flatten() {
            let session_path = session_entry.path();
            // 메인 세션 트랜스크립트만 여기서 다룬다("<sessionId>.jsonl" 직속 파일).
            // 서브에이전트 트랜스크립트는 "<sessionId>/subagents/*.jsonl"에 따로 있다.
            if session_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = session_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };

            let mut tool_counts: std::collections::HashMap<String, u32> =
                std::collections::HashMap::new();
            let mut agent_accum: std::collections::BTreeMap<String, (u32, u64, f64)> =
                std::collections::BTreeMap::new();

            if is_recent_enough(&session_path) {
                let mut main_tally = UsageTally::default();
                scan_usage_lines_for_date(&session_path, date, &mut main_tally, &mut tool_counts);
                if main_tally.turns > 0 {
                    agent_accum.insert(
                        "main".to_string(),
                        (main_tally.turns, main_tally.tokens, main_tally.cost),
                    );
                }
            }

            let subagents_dir = project_path.join(&session_id).join("subagents");
            if subagents_dir.is_dir() {
                if let Ok(agent_files) = std::fs::read_dir(&subagents_dir) {
                    for agent_entry in agent_files.flatten() {
                        let agent_path = agent_entry.path();
                        if agent_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                            continue;
                        }
                        if !is_recent_enough(&agent_path) {
                            continue;
                        }
                        let Some(agent_stem) = agent_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string())
                        else {
                            continue;
                        };

                        let meta_path = subagents_dir.join(format!("{agent_stem}.meta.json"));
                        let agent_type = std::fs::read_to_string(&meta_path)
                            .ok()
                            .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                            .and_then(|v| {
                                v.get("agentType")
                                    .and_then(|t| t.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| "unknown".to_string());

                        let mut agent_tally = UsageTally::default();
                        scan_usage_lines_for_date(
                            &agent_path,
                            date,
                            &mut agent_tally,
                            &mut tool_counts,
                        );
                        if agent_tally.turns == 0 {
                            continue;
                        }

                        let entry = agent_accum.entry(agent_type).or_insert((0, 0, 0.0));
                        entry.0 += agent_tally.turns;
                        entry.1 += agent_tally.tokens;
                        entry.2 += agent_tally.cost;
                    }
                }
            }

            // 그날 활동(usage 라인)이 전혀 없는 세션은 결과에서 제외한다.
            if agent_accum.is_empty() {
                continue;
            }

            let mut agents: Vec<AgentUsage> = agent_accum
                .into_iter()
                .map(|(agent_type, (turns, total_tokens, cost_usd))| AgentUsage {
                    agent_type,
                    turns,
                    total_tokens,
                    cost_usd,
                })
                .collect();
            agents.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

            let mut tools: Vec<ToolUsage> = tool_counts
                .into_iter()
                .map(|(tool_name, count)| ToolUsage { tool_name, count })
                .collect();
            tools.sort_by(|a, b| b.count.cmp(&a.count));
            tools.truncate(DAILY_DETAIL_TOP_TOOLS);

            let total_tokens: u64 = agents.iter().map(|a| a.total_tokens).sum();
            let cost_usd: f64 = agents.iter().map(|a| a.cost_usd).sum();
            let title = first_user_title_from_file(&session_path);

            sessions.push(SessionDetail {
                session_id,
                project_key: project_key.clone(),
                title: if title.is_empty() {
                    "(제목 없음)".to_string()
                } else {
                    title
                },
                total_tokens,
                cost_usd,
                agents,
                tools,
            });
        }
    }

    sessions.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    report.total_tokens = sessions.iter().map(|s| s.total_tokens).sum();
    report.cost_usd = sessions.iter().map(|s| s.cost_usd).sum();
    report.sessions = sessions;
    report
}

#[tauri::command]
fn get_daily_detail(date: String) -> DailyDetailReport {
    aggregate_daily_detail(&date)
}

// ---------------- 로그인: Google OAuth (PKCE, RFC 8252 데스크톱 앱 패턴) ----------------
// malgnsoft.com Google Workspace 계정만 허용한다. 흐름:
//   1. PKCE code_verifier/code_challenge + state(CSRF 방지) 생성
//   2. 127.0.0.1의 임시 포트에 짧게 뜨는 루프백 HTTP 서버(tiny_http)로 리다이렉트를 받는다
//   3. 시스템 브라우저로 Google 인증 URL을 연다(Tauri opener 플러그인의 Rust API — JS
//      쪽 invoke를 한 번 더 거치지 않고 Rust에서 직접 연다. capabilities는 이미
//      "opener:default"가 있어 추가 변경이 필요 없다 — 그 권한은 opener *플러그인*
//      자체를 앱에 허용하는 것이지, Rust 코드에서 그 플러그인의 Rust API를 호출하는
//      경로에는 별도 capabilities 항목이 필요 없다)
//   4. 콜백에서 code+state를 받아 state를 검증한 뒤 서버를 닫는다
//   5. code를 id_token으로 교환한다(공개 클라이언트 PKCE라 client_secret 불필요)
//   6. id_token 서명을 Google JWKS로 검증하고 iss/aud/exp/hd/email_verified를 확인한다
//      — hd 쿼리 파라미터는 힌트일 뿐이라 여기서 반드시 재검증해야 실제 제한이 된다.
// reqwest(HTTPS 직접 호출)도 Tauri의 http 플러그인이 아니라 Rust 표준 크레이트라
// capabilities와 무관하다.

/// Google Cloud Console에서 발급받은 OAuth 클라이언트(유형: 데스크톱 앱) ID.
const GOOGLE_OAUTH_CLIENT_ID: &str =
    "618490200291-di1v35vf0gg1pg0up2q55jh9kk9jf4b5.apps.googleusercontent.com";
/// 데스크톱 앱 타입도 Google은 토큰 교환 시 client_secret을 요구한다(RFC 8252 순수
/// 공개클라이언트와 다른 Google 고유 동작). 값 자체는 Google 문서 기준 진짜 기밀로
/// 취급되지는 않지만, 이 저장소는 public이라 소스에 리터럴로 남기지 않는다 —
/// build.rs가 로컬 `src-tauri/.env`(gitignored, CI에서는 repo secret을 환경변수로
/// 대신 주입)에서 읽어 컴파일 타임에 `cargo:rustc-env`로 넘기고, 여기서는
/// `option_env!`로만 받는다. 빌드 시점에 값이 전혀 없었으면 None — google_oauth_login()이
/// 그 경우 패닉하지 않고 명확한 에러를 반환한다(GOOGLE_OAUTH_CLIENT_ID의 TODO 처리와
/// 동일한 패턴).
const GOOGLE_OAUTH_CLIENT_SECRET: Option<&str> = option_env!("GOOGLE_OAUTH_CLIENT_SECRET");
const GOOGLE_OAUTH_ALLOWED_DOMAIN: &str = "malgnsoft.com";
const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_JWKS_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/certs";

fn generate_random_urlsafe(byte_len: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// RFC 7636 PKCE S256: code_challenge = BASE64URL(SHA256(code_verifier)).
fn code_challenge_from_verifier(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// 매번 대화형(prompt 파라미터 없음) 로그인 화면을 띄운다 — 구글이 계정 선택/동의
/// 화면을 표준 방식대로 보여준다.
fn build_google_auth_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let mut url =
        url::Url::parse(GOOGLE_AUTH_ENDPOINT).expect("고정 URL 파싱은 항상 성공해야 한다");
    url.query_pairs_mut()
        .append_pair("client_id", GOOGLE_OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        // 힌트일 뿐이다 — 실제 강제는 id_token의 hd 클레임을 검증하는 쪽에서 한다.
        .append_pair("hd", GOOGLE_OAUTH_ALLOWED_DOMAIN);
    url.to_string()
}

/// 루프백 콜백 요청의 경로+쿼리(`request.url()`이 주는 형태, 예:
/// "/callback?code=...&state=...")를 파싱해 code를 뽑는다. state 불일치·error
/// 파라미터·code 없음은 전부 로그인 거부 사유다.
fn parse_oauth_callback_url(
    raw_path_and_query: &str,
    expected_state: &str,
) -> Result<String, String> {
    let full = format!("http://127.0.0.1{raw_path_and_query}");
    let parsed =
        url::Url::parse(&full).map_err(|e| format!("콜백 URL을 해석하지 못했습니다: {e}"))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(format!("Google 로그인이 취소되었거나 실패했습니다: {err}"));
    }
    let state = state.ok_or_else(|| "콜백에 state 값이 없습니다.".to_string())?;
    if state != expected_state {
        return Err("state 값이 일치하지 않습니다(CSRF 의심) — 로그인을 거부합니다.".to_string());
    }
    code.ok_or_else(|| "콜백에 code 값이 없습니다.".to_string())
}

/// 루프백 서버 하나로 딱 한 번의 콜백 요청만 받고 즉시 닫는다. 최대 5분 대기.
async fn wait_for_oauth_callback(
    server: tiny_http::Server,
    expected_state: String,
) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();

    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
        loop {
            if std::time::Instant::now() > deadline {
                let _ = tx.send(Err(
                    "로그인 대기 시간이 초과되었습니다(5분). 다시 시도해주세요.".to_string(),
                ));
                return;
            }
            match server.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(Some(request)) => {
                    let raw = request.url().to_string();
                    let result = parse_oauth_callback_url(&raw, &expected_state);
                    let body = if result.is_ok() {
                        "<html><body><h3>로그인 완료 — 이 창을 닫고 앱으로 돌아가세요.</h3></body></html>"
                    } else {
                        "<html><body><h3>로그인 실패 — 앱으로 돌아가 오류 메시지를 확인하세요.</h3></body></html>"
                    };
                    if let Ok(header) = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    ) {
                        let response = tiny_http::Response::from_string(body).with_header(header);
                        let _ = request.respond(response);
                    }
                    let _ = tx.send(result);
                    return;
                }
                Ok(None) => continue, // recv_timeout — 아직 요청 없음, 데드라인까지 계속 대기
                Err(e) => {
                    let _ = tx.send(Err(format!("루프백 서버 오류: {e}")));
                    return;
                }
            }
        }
    });

    match rx.await {
        Ok(result) => result,
        Err(_) => Err("로그인 대기 채널이 예기치 않게 닫혔습니다.".to_string()),
    }
}

#[derive(serde::Deserialize)]
struct GoogleTokenResponse {
    id_token: String,
}

async fn exchange_code_for_id_token(
    client: &reqwest::Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_secret: &str,
) -> Result<String, String> {
    let params = [
        ("code", code),
        ("client_id", GOOGLE_OAUTH_CLIENT_ID),
        ("client_secret", client_secret),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let resp = client
        .post(GOOGLE_TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("토큰 교환 요청을 보내지 못했습니다: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("토큰 교환이 거부되었습니다: {text}"));
    }
    let token_response = resp
        .json::<GoogleTokenResponse>()
        .await
        .map_err(|e| format!("토큰 응답을 해석하지 못했습니다: {e}"))?;
    Ok(token_response.id_token)
}

#[derive(serde::Deserialize, Debug)]
struct GoogleJwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(serde::Deserialize, Debug)]
struct GoogleJwks {
    keys: Vec<GoogleJwk>,
}

async fn fetch_google_jwks(client: &reqwest::Client) -> Result<GoogleJwks, String> {
    let resp = client
        .get(GOOGLE_JWKS_ENDPOINT)
        .send()
        .await
        .map_err(|e| format!("Google JWKS를 가져오지 못했습니다: {e}"))?;
    resp.json::<GoogleJwks>()
        .await
        .map_err(|e| format!("JWKS 응답을 해석하지 못했습니다: {e}"))
}

#[derive(serde::Deserialize, Debug, Clone)]
struct GoogleIdTokenClaims {
    // iss/aud는 jsonwebtoken::Validation의 set_issuer/set_audience가 디코딩 과정에서
    // 이미 검증한다 — 구조체 필드는 디버그 가시성(로그 등)을 위해서만 남겨둔다.
    #[allow(dead_code)]
    iss: String,
    #[allow(dead_code)]
    aud: String,
    email: Option<String>,
    email_verified: Option<bool>,
    hd: Option<String>,
    name: Option<String>,
}

/// 서명·`iss`·`aud`·`exp`를 `jsonwebtoken`으로 검증한다(`exp`는 라이브러리가 기본
/// 켜져 있는 검증이라 별도 수동 체크가 필요 없다). JWKS에서 토큰 헤더의 `kid`와
/// 일치하는 키를 못 찾으면 실패시킨다 — 서명 검증 없이 클레임만 믿지 않는다.
fn verify_google_id_token_signature(
    id_token: &str,
    jwks: &GoogleJwks,
    client_id: &str,
) -> Result<GoogleIdTokenClaims, String> {
    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|e| format!("토큰 헤더를 해석하지 못했습니다: {e}"))?;
    let kid = header
        .kid
        .ok_or_else(|| "토큰 헤더에 kid가 없습니다.".to_string())?;
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == kid)
        .ok_or_else(|| "JWKS에서 일치하는 키를 찾지 못했습니다.".to_string())?;

    let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
        .map_err(|e| format!("공개키를 구성하지 못했습니다: {e}"))?;

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);

    jsonwebtoken::decode::<GoogleIdTokenClaims>(id_token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("토큰 서명 또는 클레임 검증에 실패했습니다: {e}"))
}

/// 서명 검증과 분리된 순수 로직 — hd 도메인 제한 + email_verified 확인. hd 클레임은
/// URL 파라미터가 아니라 여기, 서명 검증을 통과한 토큰의 클레임에서만 신뢰한다.
fn check_domain_restriction(claims: &GoogleIdTokenClaims, required_hd: &str) -> Result<(), String> {
    if claims.hd.as_deref() != Some(required_hd) {
        return Err(format!(
            "허용되지 않은 조직 도메인입니다(hd={:?}, 필요값={required_hd}).",
            claims.hd
        ));
    }
    if claims.email_verified != Some(true) {
        return Err("이메일이 인증되지 않았습니다(email_verified가 true가 아닙니다).".to_string());
    }
    Ok(())
}

#[derive(Serialize, Debug, Clone)]
struct GoogleLoginResult {
    email: String,
    name: String,
    hd: String,
}

struct OauthAttemptResult {
    code: String,
    code_verifier: String,
    redirect_uri: String,
}

/// 인가 시도 한 번 — 매번 새 code_verifier/state와 새 루프백 포트를 쓴다(이전 시도의
/// 서버는 콜백 하나 받으면 바로 닫히므로 재사용할 수 없다).
async fn attempt_google_oauth_authorization(
    app: &tauri::AppHandle,
) -> Result<OauthAttemptResult, String> {
    let code_verifier = generate_random_urlsafe(64);
    let code_challenge = code_challenge_from_verifier(&code_verifier);
    let state = generate_random_urlsafe(32);

    // 포트를 먼저 확보해야 redirect_uri를 만들 수 있다 — 고정 포트 대신 OS가 배정한
    // 임시 포트를 쓴다(RFC 8252 권고: 다른 앱과의 포트 충돌을 피한다).
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("루프백 서버를 열지 못했습니다: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| "루프백 포트를 확인하지 못했습니다.".to_string())?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let auth_url = build_google_auth_url(&redirect_uri, &code_challenge, &state);

    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(&auth_url, None::<&str>)
            .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))?;
    }

    let code = wait_for_oauth_callback(server, state).await?;
    Ok(OauthAttemptResult {
        code,
        code_verifier,
        redirect_uri,
    })
}

#[tauri::command]
async fn google_oauth_login(app: tauri::AppHandle) -> Result<GoogleLoginResult, String> {
    if GOOGLE_OAUTH_CLIENT_ID.starts_with("TODO") {
        return Err(
            "Google OAuth Client ID가 아직 설정되지 않았습니다. src-tauri/src/lib.rs의 GOOGLE_OAUTH_CLIENT_ID를 Google Cloud Console에서 발급받은 값으로 채워주세요.".to_string(),
        );
    }
    let client_secret = GOOGLE_OAUTH_CLIENT_SECRET.ok_or_else(|| {
        "Google OAuth Client Secret이 설정되지 않았습니다. src-tauri/.env에 GOOGLE_OAUTH_CLIENT_SECRET을 설정한 뒤 다시 빌드해주세요(build.rs가 빌드 시점에 주입합니다).".to_string()
    })?;

    let attempt = attempt_google_oauth_authorization(&app).await?;

    let client = reqwest::Client::new();
    let id_token = exchange_code_for_id_token(
        &client,
        &attempt.code,
        &attempt.code_verifier,
        &attempt.redirect_uri,
        client_secret,
    )
    .await?;
    let jwks = fetch_google_jwks(&client).await?;
    let claims = verify_google_id_token_signature(&id_token, &jwks, GOOGLE_OAUTH_CLIENT_ID)?;
    check_domain_restriction(&claims, GOOGLE_OAUTH_ALLOWED_DOMAIN)?;

    Ok(GoogleLoginResult {
        email: claims.email.unwrap_or_default(),
        name: claims.name.unwrap_or_default(),
        hd: claims.hd.unwrap_or_default(),
    })
}

// ---------------- 세션목록 실시간 감시 (파일시스템 이벤트, 폴링 아님) ----------------
// ~/.claude/sessions/ 를 앱이 켜져 있는 동안 계속 감시한다(세션목록 화면을 보고
// 있을 때만이 아니라 setup()에서 한 번 등록). 변경이 감지되면 프론트엔드에
// "claude-sessions-changed" 이벤트만 쏘고, 실제로 무엇이 바뀌었는지는 프론트가
// 이미 있는 list_claude_sessions()를 다시 호출해서 알아낸다 — 이 함수는 "다시
// 불러올 시점"만 알려준다. notify-debouncer-mini가 짧은 시간에 몰리는 이벤트를
// 500ms로 묶어준다(디바운스) — setInterval 폴링이 아니라 OS 파일시스템 이벤트
// 기반이다.
fn watch_claude_sessions_dir(app_handle: tauri::AppHandle) {
    use notify_debouncer_mini::new_debouncer;
    use notify_debouncer_mini::notify::RecursiveMode;
    use std::sync::mpsc;
    use std::time::Duration;

    let Some(home) = dirs::home_dir() else {
        return;
    };
    let sessions_dir = home.join(".claude").join("sessions");

    // 세션 디렉터리가 아직 없으면(첫 세션 전) 조용히 포기한다 — 앱이 죽으면 안 된다.
    if !sessions_dir.is_dir() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    let Ok(mut debouncer) = new_debouncer(Duration::from_millis(500), tx) else {
        return;
    };
    if debouncer
        .watcher()
        .watch(&sessions_dir, RecursiveMode::NonRecursive)
        .is_err()
    {
        return;
    }

    // debouncer를 이 블록 안에 살려둔 채 rx를 계속 받는다 — 이 for 루프가 이
    // 백그라운드 스레드의 나머지 수명이다(앱이 종료되면 스레드도 함께 끝난다).
    for result in rx {
        if let Ok(events) = result {
            if !events.is_empty() {
                use tauri::Emitter;
                let _ = app_handle.emit("claude-sessions-changed", ());
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let sessions_handle = app.handle().clone();
            std::thread::spawn(move || {
                watch_claude_sessions_dir(sessions_handle);
            });
            // 어제까지의 사용량 캐시를 앱 시작과 동시에 백그라운드에서 미리 채워둔다
            // — 사용자가 "사용량 통계"를 처음 열었을 때부터 이미 준비돼 있게.
            std::thread::spawn(|| {
                get_or_refresh_historical_daily_usage();
            });
            // 자율업무 스케줄러 — 60초 tick으로 워크스페이스 전체를 훑어 due한
            // task를 `claude -p`로 무인 실행한다. 앱이 켜져 있는 동안만 돈다.
            autonomy::spawn_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            list_claude_sessions,
            list_workspace_projects,
            dev_tools::check_dev_tools,
            dev_tools::preview_dev_tool_update,
            dev_tools::update_dev_tool,
            dev_tools::install_dev_tool,
            dev_tools::open_manual_instruction,
            list_installed_plugins,
            list_known_marketplaces,
            read_otel_env,
            update_plugin,
            refresh_marketplaces,
            list_project_tree,
            read_project_file,
            get_daily_usage,
            get_daily_detail,
            google_oauth_login,
            github_integration::github_status,
            github_integration::github_connect,
            github_integration::github_disconnect,
            cloudflare_integration::cloudflare_status,
            cloudflare_integration::cloudflare_connect,
            cloudflare_integration::cloudflare_disconnect,
            jira_integration::jira_status,
            jira_integration::jira_connect,
            jira_integration::jira_disconnect,
            autonomy::autonomy_list,
            autonomy::autonomy_save_task,
            autonomy::autonomy_delete_task,
            autonomy::autonomy_set_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신의 실제 ~/.claude/sessions/*.json을 읽어 최소 1건 이상 파싱되는지
    // 확인한다(이 프로젝트 세션 자체의 메타데이터 파일이 그 디렉토리에 있어야 한다).
    // GUI 없는 환경에서 네이티브 창을 스크린샷할 수 없을 때 이 fs 읽기 로직 자체가
    // 실제로 동작함을 증명하는 자동화된 근거로 쓴다.
    #[test]
    fn finds_at_least_one_real_session_file() {
        let sessions = read_claude_sessions();
        assert!(
            !sessions.is_empty(),
            "~/.claude/sessions/*.json에서 세션을 하나도 찾지 못했습니다"
        );
        assert!(
            sessions.iter().any(|s| s.get("sessionId").is_some()),
            "sessionId 필드를 가진 세션이 하나도 없습니다"
        );
    }

    // dev_tools 모듈로 이전됨(check_dev_tools_blocking, docker 삭제로 7→6) —
    // 여기서는 크레이트가 async 커맨드를 정상적으로 노출하는지만 남겨둔다.
    // 실제 판정 테스트는 src/dev_tools.rs의
    // devtool_table_has_exactly_six_entries_without_docker 등을 참조.

    // 세션목록 실시간 감시의 핵심 메커니즘(디바운서로 감싼 notify 워처가 디렉터리
    // 변경을 실제로 잡아내는지)만 격리해서 검증한다 — app_handle.emit()까지 엮인
    // 전체 흐름(Tauri 앱 컨텍스트 필요)은 유닛 테스트로 무리하게 재현하지 않고
    // `cargo check`/`tsc`로 컴파일 정확성만 확인했다(프론트 연동은 수동 확인 필요).
    #[test]
    fn detects_file_change_in_watched_directory() {
        use notify_debouncer_mini::new_debouncer;
        use notify_debouncer_mini::notify::RecursiveMode;
        use std::sync::mpsc;
        use std::time::Duration;

        let tmp_dir =
            std::env::temp_dir().join(format!("malgn-vscode-watch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).expect("임시 디렉터리를 만들지 못했습니다");

        let (tx, rx) = mpsc::channel();
        let mut debouncer =
            new_debouncer(Duration::from_millis(200), tx).expect("디바운서 생성 실패");
        debouncer
            .watcher()
            .watch(&tmp_dir, RecursiveMode::NonRecursive)
            .expect("워처 등록 실패");

        std::fs::write(tmp_dir.join("test.json"), "{}").expect("테스트 파일 쓰기 실패");

        let event = rx.recv_timeout(Duration::from_secs(5));
        assert!(
            event.is_ok(),
            "디렉터리 변경 이벤트를 감지하지 못했습니다(타임아웃)"
        );
        let events = event.unwrap();
        assert!(
            events.is_ok(),
            "워처가 에러를 반환했습니다: {:?}",
            events.err()
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn parses_rfc3339_timestamp() {
        assert!(parse_iso_timestamp("2026-09-02T09:01:16.983Z").is_some());
        assert!(parse_iso_timestamp("완전히 이상한 문자열").is_none());
    }

    // 이 세션 자체가 지금 malgn-vscode 프로젝트에서 대량의 assistant 메시지를
    // 만들어내고 있으니, 오늘 날짜의 집계가 0보다 커야 한다 — 실제 로컬 대화
    // 로그(usage 필드)를 정말로 읽어서 합산한다는 실증 근거로 쓴다.
    #[test]
    fn aggregates_daily_usage_and_includes_today_with_nonzero_tokens() {
        let daily = aggregate_daily_usage();
        assert!(!daily.is_empty(), "최근 30일 사용량 집계가 비어 있습니다");

        let today = Local::now().format("%Y-%m-%d").to_string();
        let today_entry = daily.iter().find(|d| d.date == today);
        assert!(
            today_entry.is_some(),
            "오늘({today}) 날짜의 사용량 항목이 없습니다"
        );

        let today_entry = today_entry.unwrap();
        let total = today_entry.input_tokens
            + today_entry.output_tokens
            + today_entry.cache_creation_tokens
            + today_entry.cache_read_tokens;
        assert!(total > 0, "오늘 사용량 합계가 0입니다");
    }

    // 어제까지의 집계는 캐시에서 그대로 재사용되고, 다시 계산해도 같은 결과가
    // 나와야 한다(캐시가 틀린 값을 굳혀버리면 안 된다) — 캐시 히트/미스 두 경로
    // 모두 검증한다.
    #[test]
    fn historical_daily_usage_cache_is_consistent_across_calls() {
        let today = today_local_date_key();
        let first = get_or_refresh_historical_daily_usage();
        let cached = HISTORICAL_USAGE_CACHE
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.cached_as_of.clone());
        assert_eq!(
            cached,
            Some(today.clone()),
            "캐시가 오늘 날짜로 채워지지 않았습니다"
        );

        let second = get_or_refresh_historical_daily_usage();
        assert_eq!(
            first.len(),
            second.len(),
            "캐시 히트 결과의 항목 수가 달라졌습니다"
        );
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.date, b.date);
            assert_eq!(a.input_tokens, b.input_tokens);
            assert_eq!(a.output_tokens, b.output_tokens);
        }
        assert!(
            !first.iter().any(|d| d.date == today),
            "과거분 캐시에 오늘 날짜가 섞여 있습니다"
        );
    }

    // "토큰 도둑" 단가 계산 — pricing.js PRICING 표를 정확히 옮겼는지 결정론적으로
    // 고정한다(비용 계산이라 숫자가 틀리면 안 되는 부분). 1M 토큰씩 넣으면 family별
    // 단가 합과 정확히 같아야 한다.
    #[test]
    fn calculates_cost_using_ported_pricing_table() {
        let opus_cost = cost_for_usage(
            "claude-opus-4-8",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (opus_cost - (15.0 + 75.0 + 18.75 + 1.5)).abs() < 1e-9,
            "opus 단가 계산이 pricing.js와 다릅니다: {opus_cost}"
        );

        let sonnet_cost = cost_for_usage(
            "claude-sonnet-4-6",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (sonnet_cost - (3.0 + 15.0 + 3.75 + 0.3)).abs() < 1e-9,
            "sonnet 단가 계산이 pricing.js와 다릅니다: {sonnet_cost}"
        );

        let haiku_cost = cost_for_usage(
            "claude-haiku-4-5",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (haiku_cost - (1.0 + 5.0 + 1.25 + 0.1)).abs() < 1e-9,
            "haiku 단가 계산이 pricing.js와 다릅니다: {haiku_cost}"
        );

        // 미매칭 모델명은 pricing.js의 modelFamily()처럼 sonnet으로 폴백해야 한다.
        let unknown_cost = cost_for_usage("some-unreleased-model", 1_000_000, 0, 0, 0);
        assert!(
            (unknown_cost - 3.0).abs() < 1e-9,
            "미매칭 모델 폴백이 sonnet 단가가 아닙니다: {unknown_cost}"
        );
    }

    // 회귀 방지 — 실제로 겪은 버그: 스트리밍 응답이 여러 JSONL 줄로 쪼개져 기록되며
    // 같은 message.id가 반복 등장하고 매번 같은 usage를 다시 싣는다(실측: 한
    // 세션에서 고유 메시지 124개인데 usage가 실린 줄은 238개 — 거의 2배). 같은
    // id를 두 번 세면 토큰·비용이 부풀려진다. 여기서는 그 스트리밍 중복 패턴을
    // 합성 fixture로 재현해 scan_usage_lines()가 id당 한 번만 세는지 고정한다.
    #[test]
    fn deduplicates_repeated_message_id_when_scanning_usage() {
        let tmp = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-dedup-test-{}.jsonl",
            std::process::id()
        ));
        let content = concat!(
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:00.000Z","message":{"id":"msg_dup1","model":"claude-sonnet-4-6","usage":{"input_tokens":2,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":10000}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:01.000Z","message":{"id":"msg_dup1","model":"claude-sonnet-4-6","usage":{"input_tokens":2,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":10000}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:02.000Z","message":{"id":"msg_unique2","model":"claude-sonnet-4-6","usage":{"input_tokens":1,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":5000}}}"#,
            "\n",
        );
        std::fs::write(&tmp, content).expect("fixture 파일 쓰기 실패");

        let mut tally = UsageTally::default();
        scan_usage_lines(&tmp, &mut tally);
        let _ = std::fs::remove_file(&tmp);

        // msg_dup1은 한 번만, msg_unique2는 한 번 — 총 2턴이어야 한다(3이면 중복 제거 실패).
        assert_eq!(tally.turns, 2, "중복 message.id가 두 번 세어졌습니다");
        assert_eq!(
            tally.tokens,
            (2 + 100 + 10000) + (1 + 50 + 5000),
            "중복 제거 후 토큰 합계가 예상과 다릅니다"
        );
    }

    // 이 세션 자체가 지금 malgn-vscode 프로젝트에서 오늘 날짜의 활동을 만들어내고
    // 있으니, 오늘 날짜로 상세 집계를 요청하면 이 프로젝트 세션이 0보다 큰 토큰으로
    // 잡혀야 한다 — "하루만 스캔"하는 실제 파일 I/O 경로가 정말 동작함을 증명한다.
    #[test]
    fn aggregates_daily_detail_for_today_from_real_data() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let report = aggregate_daily_detail(&today);
        assert_eq!(report.date, today);
        assert!(
            !report.sessions.is_empty(),
            "오늘 날짜 상세 집계가 비어 있습니다"
        );
        assert!(report.total_tokens > 0, "오늘 날짜 총 토큰이 0입니다");
        assert!(
            report
                .sessions
                .iter()
                .any(|s| s.project_key == "-Users-hopegiver-workspace-malgn-vscode"),
            "이 프로젝트(malgn-vscode) 세션이 오늘 상세 집계에 없습니다"
        );
        // 하루치만 봤으니 각 세션의 tools는 상위 10개를 넘지 않아야 한다.
        assert!(
            report.sessions.iter().all(|s| s.tools.len() <= 10),
            "tools 상위 개수 제한이 지켜지지 않았습니다"
        );
    }

    // 실제 세션 중 최소 하나는 대화 로그(jsonl)에서 제목을 뽑아낼 수 있어야 한다 —
    // 이 세션 자체가 malgn-vscode 프로젝트에서 지금 실행 중이라 그 jsonl이 실제로
    // 존재하고 자라고 있다.
    #[test]
    fn extracts_title_for_at_least_one_real_session() {
        let sessions = read_claude_sessions();
        assert!(
            sessions.iter().any(|s| s
                .get("title")
                .and_then(|t| t.as_str())
                .is_some_and(|t| !t.is_empty())),
            "실제 세션 중 제목을 추출한 것이 하나도 없습니다"
        );
    }

    // 이 머신의 실제 ~/workspace를 스캔해 이 프로젝트(malgn-vscode) 자신이 목록에
    // 있는지 확인한다 — CLAUDE.md가 있으니 반드시 인식되어야 한다.
    #[test]
    fn finds_this_project_in_workspace() {
        let projects = scan_workspace_projects();
        assert!(
            !projects.is_empty(),
            "~/workspace 아래에서 프로젝트를 하나도 찾지 못했습니다"
        );
        assert!(
            projects.iter().any(|p| p.name == "malgn-vscode"),
            "malgn-vscode 자기 자신이 목록에 없습니다"
        );
    }

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

    // 이 머신에 실제로 설치된 malgn-agent 플러그인(user scope)을 찾아 agents/skills/
    // knowledge 실물 목록까지 비어있지 않은지 확인한다.
    #[test]
    fn finds_this_machines_malgn_agent_plugin() {
        let plugins = read_installed_plugins();
        let found = plugins.iter().find(|p| p.name == "malgn-agent");
        assert!(found.is_some(), "malgn-agent 플러그인을 찾지 못했습니다");
        let p = found.unwrap();
        assert!(!p.agents.is_empty(), "agents 목록이 비어 있습니다");
        assert!(!p.skills.is_empty(), "skills 목록이 비어 있습니다");
        assert!(!p.knowledge.is_empty(), "knowledge 목록이 비어 있습니다");
    }

    #[test]
    fn finds_known_marketplaces() {
        let marketplaces = read_known_marketplaces();
        assert!(
            !marketplaces.is_empty(),
            "마켓플레이스를 하나도 찾지 못했습니다"
        );
        assert!(marketplaces.iter().any(|m| m.id == "malgnsoft-plugins"));
    }

    // env에서 OTEL_ 접두사가 아닌 키가 섞여 나오면 안 된다(범위 밖 설정 노출 방지).
    #[test]
    fn reads_only_otel_prefixed_keys() {
        let env = collect_otel_env();
        assert!(
            env.keys().all(|k| k.starts_with("OTEL_")),
            "OTEL_ 접두사가 아닌 키가 섞여 있습니다"
        );
    }

    #[test]
    fn rejects_project_path_outside_workspace_root() {
        let tree = list_project_tree("/etc".to_string());
        assert!(
            tree.is_empty(),
            "워크스페이스 바깥 경로가 거부되지 않았습니다"
        );
    }

    // 경로 트래버설 차단 — 보안 관련이라 회귀 방지용으로 고정해둔다.
    #[test]
    fn blocks_path_traversal_in_file_preview() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let result = read_project_file(
            project.path.clone(),
            "../../../../../../../etc/passwd".to_string(),
        );
        assert!(
            matches!(result, FilePreview::Denied | FilePreview::NotFound),
            "경로 트래버설이 차단되지 않았습니다: {:?}",
            result
        );
    }

    #[test]
    fn reads_a_real_text_file_from_this_project() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let result = read_project_file(project.path.clone(), "package.json".to_string());
        match result {
            FilePreview::Text { content } => assert!(content.contains("\"name\"")),
            other => panic!("package.json을 텍스트로 읽지 못했습니다: {:?}", other),
        }
    }

    #[test]
    fn builds_tree_excluding_node_modules() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let tree = list_project_tree(project.path.clone());
        assert!(!tree.is_empty(), "파일 트리가 비어 있습니다");
        if let Some(nm) = tree.iter().find(|n| n.name == "node_modules") {
            assert!(nm.truncated, "node_modules가 제외 표시되지 않았습니다");
        }
    }

    // ⚠️ update_plugin()/refresh_marketplaces()를 실제로 호출하는 테스트는 의도적으로
    // 두지 않는다 — 이 머신에 실제 설치된 malgn-agent 플러그인(이 에이전트 자신이
    // 로드되어 있는 바로 그 플러그인)을 `cargo test`를 돌릴 때마다 매번 실제로
    // 업데이트해버리는 부작용은 누구도 원하지 않는다. 컴파일 통과(`cargo check`)로만
    // 검증했고, 실제 클릭 검증은 사용자가 직접 GUI에서 해야 한다.

    // ---------------- Google OAuth 로그인 ----------------
    // 전체 흐름(브라우저 상호작용 필요)은 통합 테스트로 만들지 않는다 — PKCE 생성,
    // 콜백 파싱, hd 도메인 제한, 그리고 실제 서명된 JWT로 서명·iss·aud·exp 검증까지
    // 순수 로직 단위로 전부 고정한다.

    #[test]
    fn generates_pkce_challenge_matching_rfc7636_test_vector() {
        // RFC 7636 Appendix B 공식 테스트 벡터로 검증한다.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = code_challenge_from_verifier(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generates_random_urlsafe_strings_that_differ_and_are_urlsafe() {
        let a = generate_random_urlsafe(32);
        let b = generate_random_urlsafe(32);
        assert_ne!(
            a, b,
            "매번 다른 무작위 값이어야 합니다(state/verifier 재사용 방지)"
        );
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "URL-safe base64 문자만 포함해야 합니다: {a}"
        );
    }

    #[test]
    fn builds_google_auth_url_with_required_params() {
        let url = build_google_auth_url(
            "http://127.0.0.1:54321/callback",
            "challenge123",
            "state123",
        );
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(
            url.contains("hd=malgnsoft.com"),
            "hd 힌트가 빠졌습니다: {url}"
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=challenge123"));
        assert!(url.contains("state=state123"));
        assert!(
            !url.contains("prompt="),
            "prompt는 붙지 않아야 합니다: {url}"
        );
    }

    #[test]
    fn parses_valid_oauth_callback() {
        let code = parse_oauth_callback_url("/callback?code=abc123&state=xyz", "xyz")
            .expect("정상 콜백은 성공해야 합니다");
        assert_eq!(code, "abc123");
    }

    #[test]
    fn rejects_oauth_callback_with_mismatched_state() {
        let result = parse_oauth_callback_url("/callback?code=abc123&state=WRONG", "xyz");
        assert!(
            result.is_err(),
            "state 불일치는 CSRF 의심으로 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_with_error_param() {
        let result = parse_oauth_callback_url("/callback?error=access_denied&state=xyz", "xyz");
        assert!(
            result.is_err(),
            "Google이 보낸 error 파라미터는 거부 사유여야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_missing_code() {
        let result = parse_oauth_callback_url("/callback?state=xyz", "xyz");
        assert!(result.is_err(), "code가 없으면 거부되어야 합니다");
    }

    // ---- hd 도메인 제한 + email_verified 확인 (서명 검증과 분리된 순수 로직) ----

    fn sample_claims(hd: Option<&str>, email_verified: Option<bool>) -> GoogleIdTokenClaims {
        GoogleIdTokenClaims {
            iss: "https://accounts.google.com".to_string(),
            aud: "test-client-id".to_string(),
            email: Some("dev@malgnsoft.com".to_string()),
            email_verified,
            hd: hd.map(|s| s.to_string()),
            name: Some("Dev".to_string()),
        }
    }

    #[test]
    fn accepts_correct_domain_and_verified_email() {
        let claims = sample_claims(Some("malgnsoft.com"), Some(true));
        assert!(check_domain_restriction(&claims, "malgnsoft.com").is_ok());
    }

    #[test]
    fn rejects_wrong_hd_domain() {
        let claims = sample_claims(Some("gmail.com"), Some(true));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "다른 조직 도메인은 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_missing_hd_claim() {
        // 개인 Gmail 계정 등 Workspace가 아닌 계정은 hd 클레임 자체가 없다.
        let claims = sample_claims(None, Some(true));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "hd 클레임이 없으면 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_unverified_email() {
        let claims = sample_claims(Some("malgnsoft.com"), Some(false));
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "email_verified가 false면 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_missing_email_verified() {
        let claims = sample_claims(Some("malgnsoft.com"), None);
        assert!(
            check_domain_restriction(&claims, "malgnsoft.com").is_err(),
            "email_verified 클레임이 없으면 거부되어야 합니다"
        );
    }

    // ---- 서명 검증 (합성 RSA 키쌍으로 실제 서명된 JWT를 만들어 검증) ----
    // 테스트 전용으로 새로 생성한 2048비트 RSA 키쌍이다(운영 키와 무관 — Google의
    // 실제 개인키는 당연히 우리에게 없다). 이 키로 직접 서명한 토큰을 우리
    // verify_google_id_token_signature()에 통과시켜 서명·iss·aud·exp 검증이 실제로
    // 동작하는지 확인한다.
    const TEST_RSA_PRIVATE_KEY_PEM: &str =
        // pragma: allowlist-secret (테스트 전용 합성 키, 실서비스와 무관)
        "-----BEGIN RSA PRIVATE KEY-----
MIIEogIBAAKCAQEAnrvlIKSb3xV5R+JTXVvNsj5cPZWRh9NsV3qfFTeT4IewGBH8
MNDFlG21tb8PShKamFVgReoSp25X2+WlalGeePP7F8dV9Q7UGKQKpubKIbErYjiO
lSlB+g5nle5sIlHVZkg6aAkAC1g5hrCgYLun2MpCdaQx2qc846I1RooBFrsMiubj
jASrLibqcYwqbY2i3gXWh/FnCUtjqL33KG3aEoDEcuKCnarhTqnQ8XBjCye5PNIg
R1TwfcXmCpfuYyx2eWp4ouJ5QI4YHiGH2YberYnCvSQdxtUOi5zP8fTE0EBc081h
olKqb89JD+WobwH2uM1GQ1GE+1yMWW2I2UzUIQIDAQABAoIBAGZpXdQov/Q3W49Y
Y2bJczX76/FDzagvbSgnkfnTaNIlWSS+fdJU8BTqj6EaCthEln+QHdQdyDlEBOV4
DbhBvpfU+fyGfFvmXEslk0XJg0Inl5EAYmW0P8AAiS5/rD6cQ62BDkXPALtRCZRv
4plmmU1SeXyDGjMzUSKgpfTD1x39PkwPurHAMHoVgipwanXUQCuNzyD/Sr2DW9bg
OJm+LeBg2Y0ZuUGVYukswUTk1kZpWuTg+SiPwxdIaSCpEVD9I8J/EpaHu5HohiNI
2wMPHLFJQ9LEy9XqRUFH3retMKEatoQVDTxAj8gkRX+36HAQIWFbshAgWeo239zc
AXRlWN0CgYEAzzDTSvLLao3D4CXxajcF7XbWmOsw0MgRbXfRgXvHUPdb5D799tl+
tICymp69DeX4nkjUFtZsKBe1+wa+5MHupYAUYKWewf2DyOJl1z/Vjh9Bb2K1Q/nU
jwjDaX3our/z8+uZbtyrQoSQTIEUBfvEAHxG8yEGQfLlzZcS+nTZNNMCgYEAxCDC
TGsrDzh5fRNlx/eewgQBv9/0PS1M9uXnF7EHe0rfdLsIRWTGXdeWlKKeV0AtTJkd
rufeExkeiXo/TzUIoSoLmW39gHvDhtg0R8rcKdjqeA4Z1KTnGUKnOJNTKLQTO+Mv
uGxv2BdlDAVf4G+LnphstLJClFvlNT8KK2NBCrsCgYBV26/TgSWWdETVYCPYlhCY
xQRMvjmuaxn9uQdSlw6TmM21mfz4DE0bU7GvrVQ+rCwIu7lX9WdAfgLlkXgNp+fT
IW5QVpGhZgL0fg0h08wVZxJgrBDdqGvTEhiYYJrOuLjJPbqJXFyD5hc9/Mdla11f
riBgpDDJp3Rfa9lrfHx+DQKBgHXu/ObWyl2sp+D9+QX1cBFaN3MZR9RBmTYdqIgm
e0k4DIY0sRSJNH7ZVEKsRmpQvOyCZcb2xiLVx/cC+261hSrkDXWFHhpUUY6UE1vY
L+s59EOctwuW3R/jZIowjKC9J5OrWNac3eQirTA9Sxm5+Uq0fSlqx35Og9Uwwvy0
AjhhAoGAPqEamBCuJ9PAXEPxgCl3SWYVfJFUf8oA2LxLhiqv+tZL9soFIydKEQnJ
G6IzlUwCnzUWtpvSsli3j+KV0DFKug6srMh5peP/4zvfP0dg5UbW5Ts1KSK0yGlA
PZjgfX70Iyke1LFgmwoh3Mq4Yicc57EBfPpH5WAGMtPlrpf1NDM=
-----END RSA PRIVATE KEY-----";
    const TEST_RSA_N: &str = "nrvlIKSb3xV5R-JTXVvNsj5cPZWRh9NsV3qfFTeT4IewGBH8MNDFlG21tb8PShKamFVgReoSp25X2-WlalGeePP7F8dV9Q7UGKQKpubKIbErYjiOlSlB-g5nle5sIlHVZkg6aAkAC1g5hrCgYLun2MpCdaQx2qc846I1RooBFrsMiubjjASrLibqcYwqbY2i3gXWh_FnCUtjqL33KG3aEoDEcuKCnarhTqnQ8XBjCye5PNIgR1TwfcXmCpfuYyx2eWp4ouJ5QI4YHiGH2YberYnCvSQdxtUOi5zP8fTE0EBc081holKqb89JD-WobwH2uM1GQ1GE-1yMWW2I2UzUIQ";
    const TEST_RSA_E: &str = "AQAB";

    fn build_test_jwks() -> GoogleJwks {
        GoogleJwks {
            keys: vec![GoogleJwk {
                kid: "test-kid-1".to_string(),
                n: TEST_RSA_N.to_string(),
                e: TEST_RSA_E.to_string(),
            }],
        }
    }

    #[derive(serde::Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        aud: &'a str,
        email: &'a str,
        email_verified: bool,
        hd: &'a str,
        name: &'a str,
        exp: usize,
    }

    fn sign_test_token(claims: &TestClaims, kid: &str) -> String {
        let encoding_key =
            jsonwebtoken::EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes())
                .expect("테스트 RSA 개인키 파싱 실패");
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(kid.to_string());
        jsonwebtoken::encode(&header, claims, &encoding_key).expect("테스트 토큰 서명 실패")
    }

    fn future_exp() -> usize {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize
    }

    fn past_exp() -> usize {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(3600) as usize
    }

    #[test]
    fn verifies_correctly_signed_token_with_matching_claims() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");

        let verified = verify_google_id_token_signature(&token, &jwks, "test-client-id")
            .expect("정상 서명 토큰은 검증을 통과해야 합니다");
        assert_eq!(verified.email.as_deref(), Some("dev@malgnsoft.com"));
        assert_eq!(verified.hd.as_deref(), Some("malgnsoft.com"));
        assert!(check_domain_restriction(&verified, "malgnsoft.com").is_ok());
    }

    #[test]
    fn rejects_token_with_wrong_audience() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "someone-elses-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(
            result.is_err(),
            "우리 client_id가 아닌 aud는 거부되어야 합니다(다른 앱 발급 토큰 재사용 방지)"
        );
    }

    #[test]
    fn rejects_token_with_wrong_issuer() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://evil.example.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(result.is_err(), "Google이 아닌 iss는 거부되어야 합니다");
    }

    #[test]
    fn rejects_expired_token() {
        let jwks = build_test_jwks();
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: past_exp(),
        };
        let token = sign_test_token(&claims, "test-kid-1");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(result.is_err(), "만료된 토큰은 거부되어야 합니다");
    }

    #[test]
    fn rejects_token_signed_with_unknown_kid() {
        let jwks = build_test_jwks(); // kid: "test-kid-1"만 알고 있다
        let claims = TestClaims {
            iss: "https://accounts.google.com",
            aud: "test-client-id",
            email: "dev@malgnsoft.com",
            email_verified: true,
            hd: "malgnsoft.com",
            name: "Dev",
            exp: future_exp(),
        };
        let token = sign_test_token(&claims, "unknown-kid-999");
        let result = verify_google_id_token_signature(&token, &jwks, "test-client-id");
        assert!(
            result.is_err(),
            "JWKS에 없는 kid로 서명된 토큰은 검증할 방법이 없어 거부되어야 합니다"
        );
    }
}
