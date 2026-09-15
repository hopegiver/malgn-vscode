// ---------------- 세션 목록/제목 ----------------
// `~/.claude/sessions/*.json` 메타데이터를 읽어 목록 커맨드로 반환한다. 원래
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// `extract_text_from_content`/`truncate_title`는 `usage_stats::detail`의
// `first_user_title_from_file`도 재사용한다(그래서 `pub(crate)`).

// 진입점(`list_claude_sessions`)·registry 원본 읽기·제목 추출은 이 파일이 맡고,
// 워크스페이스 경로 필터링은 `path_filter`, jsonl 스캔/파싱은 `jsonl_scan`,
// registry 정리(dedup/live 오버레이)는 `registry` 서브모듈로 옮겼다.
mod jsonl_scan;
mod path_filter;
mod registry;

use jsonl_scan::build_jsonl_rows;
use path_filter::{allowed_workspace_roots, is_cwd_within_allowed_workspace};
use registry::{build_live_registry, filter_and_dedup_sessions, overlay_running_state};
use serde_json::Value;
use std::io::{BufRead, BufReader};

/// 세션 JSON의 `cwd`를 `~/.claude/projects/<이 값>/` 디렉터리명으로 바꾼다 — 이
/// 프로젝트가 실제로 쓰는 규칙(`/`를 전부 `-`로 치환)을 그대로 따른다. 슬래시를
/// 전부 지우기 때문에 결과에는 경로 구분자가 하나도 남지 않는다 — `cwd`에 `..`가
/// 있어도 항상 하나의 평평한(flat) 디렉터리명 문자열이 될 뿐 상위 디렉터리로
/// 빠져나갈 수 없다(경로 트래버설이 구조적으로 불가능하다).
fn sanitize_cwd_for_project_dir(cwd: &str) -> String {
    cwd.replace('/', "-")
}

pub(crate) fn truncate_title(text: &str, max_chars: usize) -> String {
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
pub(crate) fn extract_text_from_content(content: &Value) -> Option<String> {
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

/// `~/.claude/sessions/*.json` 메타데이터를 원본 그대로 읽는다(정리 전). 파일
/// 하나가 없거나 깨져 있어도(JSON 파싱 실패) 그 항목만 건너뛰고 전체 목록은
/// 계속 만든다 — 세션 메타데이터는 외부 프로세스가 계속 쓰고 있을 수 있는 값이라
/// 언제든 깨진 상태로 읽힐 수 있다고 가정한다.
fn read_raw_registry_sessions() -> Vec<Value> {
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
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        sessions.push(value);
    }
    sessions
}

/// 목록 행을 만드는 진입점. 소스는 jsonl(`build_jsonl_rows`)이고, registry
/// (`~/.claude/sessions/*.json`)는 `filter_and_dedup_sessions()`(순서 고정:
/// 자식 제외 → 죽은 pid 필터 → sessionId dedup, 본체·시그니처 변경 금지)를
/// 통과한 뒤 `build_live_registry()`로 신규 세션(우리 자식만 등록된 세션,
/// M2)의 표시를 되살리고, m3: 그 결과를 `is_cwd_within_allowed_workspace`로도
/// 걸러(registry 오버레이 경로도 안전장치 ②를 통과해야 한다) `overlay_running_state`로
/// "지금 실행 중" 오버레이를 얹는다.
fn read_claude_sessions() -> Vec<Value> {
    let jsonl_rows = build_jsonl_rows();

    let raw_registry = read_raw_registry_sessions();
    let exclude_pids = crate::session_chat::active_turn_pids();
    let filtered_registry = filter_and_dedup_sessions(raw_registry.clone(), &exclude_pids);
    let active_session_ids = crate::session_chat::active_turn_session_ids();
    let mut live_registry = build_live_registry(filtered_registry, &raw_registry, &active_session_ids);

    // m3: registry 오버레이 경로(특히 위에서 되살린 신규 세션 항목과, jsonl이
    // 아직 없어 폴백 행이 될 항목)는 이전에 안전장치 ①②를 전혀 거치지 않았다
    // — 워크스페이스 밖 cwd(예: 스크래치패드)가 목록에 새어나갈 수 있었다.
    // `is_cwd_within_allowed_workspace`(M1의 권위 검사, 실제 워크스페이스
    // 루트만 통과)로 게이트를 추가해 닫는다. `cwd` 필드가 없는 항목은 판단
    // 근거가 없으므로 보수적으로 제외한다(다른 안전장치들과 반대로 여기는
    // fail-closed다 — 이 경로는 "새로 추가하는 게이트"라 기존 관용구를
    // 그대로 따를 이유가 없다).
    let allowed_roots = allowed_workspace_roots();
    live_registry.retain(|value| {
        value
            .get("cwd")
            .and_then(|c| c.as_str())
            .is_some_and(|cwd| is_cwd_within_allowed_workspace(cwd, &allowed_roots))
    });

    for value in live_registry.iter_mut() {
        if let Some(title) = find_session_title(value) {
            if let Value::Object(map) = value {
                map.insert("title".to_string(), Value::String(title));
            }
        }
    }

    overlay_running_state(jsonl_rows, live_registry)
}

/// 예전엔 sync였다(P0 버그와 동일 계열 — non-async `#[tauri::command]`는
/// 메인 스레드에서 돈다). `dev_tools.rs`의 `check_dev_tools`가 정한 관용구
/// (async + `spawn_blocking`)를 그대로 따른다 — 프론트 `invoke()` 계약은
/// 항상 Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn list_claude_sessions() -> Vec<Value> {
    tauri::async_runtime::spawn_blocking(read_claude_sessions)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신의 실제 ~/.claude/sessions/*.json을 읽어 최소 1건 이상 파싱되는지
    // 확인한다(이 프로젝트 세션 자체의 메타데이터 파일이 그 디렉토리에 있어야 한다).
    // GUI 없는 환경에서 네이티브 창을 스크린샷할 수 없을 때 이 fs 읽기 로직 자체가
    // 실제로 동작함을 증명하는 자동화된 근거로 쓴다.
    // 머신 의존 — CI 러너에는 ~/.claude/sessions가 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored session_list::tests::finds_at_least_one_real_session_file
    #[test]
    #[ignore]
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

    // 실제 세션 중 최소 하나는 대화 로그(jsonl)에서 제목을 뽑아낼 수 있어야 한다 —
    // 이 세션 자체가 malgn-vscode 프로젝트에서 지금 실행 중이라 그 jsonl이 실제로
    // 존재하고 자라고 있다.
    // 머신 의존 — CI 러너에는 그 jsonl이 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored session_list::tests::extracts_title_for_at_least_one_real_session
    #[test]
    #[ignore]
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

    // ==================== QA 실데이터 검증(#[ignore] — 이 머신 의존) ====================
    // 실행: cargo test -- --ignored --nocapture session_list::tests::real_scan
    // 목적: 서브에이전트 유령 세션(실측 1,000건 이상)이 섞이지 않는지, 그리고
    // registry에서 사라진 뒤로 목록에 영영 안 보이던 끝난 세션이 이제 보이는지
    // 실제 데이터로 확인한다.
    #[test]
    #[ignore]
    fn real_scan_excludes_subagent_ghosts_and_surfaces_previously_hidden_finished_session() {
        let sessions = read_claude_sessions();
        assert!(
            sessions.len() < 1_000,
            "서브에이전트 유령 세션이 섞였을 가능성이 있습니다: {}건",
            sessions.len()
        );
        assert!(
            sessions.iter().any(|s| s
                .get("sessionId")
                .and_then(|v| v.as_str())
                == Some("7b57eff2-8ea4-450d-b3b7-45510a499bf3")),
            "이전에 registry에서 사라져 목록에 안 보이던 끝난 세션이 여전히 보이지 않습니다"
        );
    }
}
