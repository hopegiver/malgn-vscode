// ---------------- 세션 목록/제목 ----------------
// `~/.claude/sessions/*.json` 메타데이터를 읽어 목록 커맨드로 반환한다. 원래
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// `extract_text_from_content`/`truncate_title`는 `usage_stats::detail`의
// `first_user_title_from_file`도 재사용한다(그래서 `pub(crate)`).

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

/// registry에서 읽은 원본 세션 목록을 sessionId 단위로 정리하는 순수 함수
/// (fs 접근 없음 — 유닛 테스트 대상). **아래 3단계 적용 순서를 반드시 지켜야
/// 한다**(실측으로 증명됨 — 반대 순서면 회귀가 난다):
///
/// 1. `exclude_pids`(앱이 스스로 띄운 자식 `claude -p`의 pid, 즉
///    `session_chat::active_turn_pids()`) 제외.
/// 2. 죽은 pid 필터(`process_util::pid_alive`).
/// 3. 같은 `sessionId`는 `startedAt` 최신 1건만 남긴다(dedup).
///
/// 순서가 중요한 이유: 앱이 `send_session_message`로 띄운 자식은 registry에
/// 자기 pid로 항목을 하나 더 만들고, 그 항목의 `startedAt`은 (턴을 시작한
/// 시점이라) 원본 IDE 세션 항목보다 항상 더 최신이다. 1번(자식 제외)보다 3번
/// (dedup)을 먼저 하면 "최신 1건만 남긴다"는 규칙이 원본이 아니라 우리 자식을
/// 선택해버리는 역전이 생긴다 — 1번을 먼저 해서 자식을 아예 후보에서 빼야 이
/// 역전이 원천 차단된다.
fn filter_and_dedup_sessions(
    sessions: Vec<Value>,
    exclude_pids: &std::collections::HashSet<u32>,
) -> Vec<Value> {
    // 1) ACTIVE_TURNS(우리 자식) 제외. pid 필드가 없는 항목은 판단 불가이므로
    //    보수적으로 통과시킨다(원래도 없는 형태였으면 걸러낼 근거가 없다).
    let step1: Vec<Value> = sessions
        .into_iter()
        .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
            Some(pid) => !exclude_pids.contains(&(pid as u32)),
            None => true,
        })
        .collect();

    // 2) 죽은 pid 필터.
    let step2: Vec<Value> = step1
        .into_iter()
        .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
            Some(pid) => crate::process_util::pid_alive(pid as u32),
            None => true,
        })
        .collect();

    // 3) sessionId dedup: startedAt 최신 1건만. sessionId가 없는 항목은
    //    dedup 키가 없으므로 그대로 통과시킨다(고유 취급).
    let mut by_session_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    let mut no_session_id: Vec<Value> = Vec::new();
    for value in step2 {
        let Some(session_id) = value.get("sessionId").and_then(|v| v.as_str()) else {
            no_session_id.push(value);
            continue;
        };
        let started_at = value.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
        match by_session_id.get(session_id) {
            Some(existing) => {
                let existing_started_at =
                    existing.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                if started_at > existing_started_at {
                    by_session_id.insert(session_id.to_string(), value);
                }
            }
            None => {
                by_session_id.insert(session_id.to_string(), value);
            }
        }
    }
    let mut result: Vec<Value> = by_session_id.into_values().collect();
    result.extend(no_session_id);
    result
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
///
/// registry를 읽은 직후 `filter_and_dedup_sessions()`(순서 고정: 자식 제외 →
/// 죽은 pid 필터 → sessionId dedup)로 정리한 다음, 살아남은 항목에 대해서만
/// 제목을 뽑는다 — 어차피 걸러질 항목의 대화 로그까지 읽지 않는다.
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
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        sessions.push(value);
    }

    let exclude_pids = crate::session_chat::active_turn_pids();
    let mut sessions = filter_and_dedup_sessions(sessions, &exclude_pids);

    for value in sessions.iter_mut() {
        if let Some(title) = find_session_title(value) {
            if let Value::Object(map) = value {
                map.insert("title".to_string(), Value::String(title));
            }
        }
    }
    sessions
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

    /// 테스트용 세션 registry 항목을 만든다. `pid_alive()`가 실제 OS 시그널을
    /// 쓰므로 "살아있는" pid로는 현재 테스트 프로세스 자신의 pid(`std::process::id()`)를,
    /// "죽은" pid로는 OS가 배정할 가능성이 사실상 없는 `u32::MAX - 1`을 쓴다
    /// (process_util.rs의 자체 테스트와 동일한 접근).
    fn fixture_session(pid: u32, session_id: &str, started_at: i64) -> Value {
        serde_json::json!({
            "pid": pid,
            "sessionId": session_id,
            "startedAt": started_at,
            "cwd": "/tmp/fixture",
        })
    }

    // 역전 방지 회귀 테스트: 같은 sessionId로 원본(IDE) 항목과 우리 자식
    // (`claude -p`) 항목이 둘 다 registry에 있을 때, 자식이 startedAt이 더
    // 최신이라도 1단계(exclude_pids)에서 먼저 빠지므로 3단계 dedup이 원본을
    // 밀어내지 않아야 한다. dedup을 exclude보다 먼저 적용하면 이 테스트가
    // 실패한다(자식의 최신 startedAt이 선택되어버림).
    #[cfg(unix)]
    #[test]
    fn active_turn_child_excluded_before_dedup_keeps_original() {
        // "원본"은 현재 테스트 프로세스 자신의 pid(항상 살아있고 자기 자신에게는
        // 신호 권한이 있다). "자식"은 실제로 띄운 보조 프로세스의 pid를 써서
        // 반드시 살아있게 만든다 — 이 테스트가 검증하려는 것은 "죽은 프로세스라
        // 걸러졌다"가 아니라 "exclude_pids 제외가 dedup보다 먼저 적용돼야
        // 한다"이므로, 자식도 살아있는 채로 exclude에만 넣는다(pid 1을 쓰지
        // 않는 이유: process_util 테스트 주석 참조 — 일반 사용자는 pid 1에
        // 신호를 보낼 권한이 없어 kill(1,0)이 EPERM으로 "죽음"처럼 보인다).
        let mut helper = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("보조 프로세스(sleep)를 띄우지 못했습니다");
        let original_pid = std::process::id();
        let child_pid = helper.id();
        let session_id = "shared-session-id";

        // 자식 항목은 같은 sessionId, 더 최신 startedAt(실제로 턴 시작 시점이
        // 원본 세션 시작 시점보다 항상 나중이라 그렇다), 그리고 exclude_pids에
        // 포함된 pid.
        let original = fixture_session(original_pid, session_id, 1_000);
        let child = fixture_session(child_pid, session_id, 9_999);

        let mut exclude = std::collections::HashSet::new();
        exclude.insert(child_pid);

        let sessions = vec![original.clone(), child];
        let result = filter_and_dedup_sessions(sessions, &exclude);

        assert_eq!(result.len(), 1, "정리 후 세션이 정확히 1건 남아야 합니다");
        assert_eq!(
            result[0].get("startedAt").and_then(|v| v.as_i64()),
            Some(1_000),
            "원본(startedAt=1000)이 남아야 하는데 자식(startedAt=9999)이 남았습니다 — \
             적용 순서가 뒤집혔을 가능성이 있습니다"
        );

        // 순서가 실제로 중요함을 직접 대조 검증한다: dedup을 exclude보다
        // 먼저 적용하면(반대 순서) 더 최신인 자식이 dedup에서 살아남고,
        // 그 다음에야 exclude로 제거되어 원본까지 함께 사라진다 — 즉 결과가
        // 0건이 되어 원본이 통째로 유실된다. 이 프로젝트의 실제 구현은 이
        // 순서를 쓰지 않지만, 반대 순서가 실제로 다른(더 나쁜) 결과를 낳는다는
        // 것을 명시적으로 남겨 "순서가 중요하다"는 요구사항 자체를 고정한다.
        let reversed_order_result: Vec<Value> = {
            // dedup 먼저
            let mut by_session_id: std::collections::HashMap<String, Value> =
                std::collections::HashMap::new();
            for value in [
                fixture_session(original_pid, session_id, 1_000),
                fixture_session(child_pid, session_id, 9_999),
            ] {
                let sid = value.get("sessionId").and_then(|v| v.as_str()).unwrap().to_string();
                let started = value.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                match by_session_id.get(&sid) {
                    Some(existing) => {
                        let existing_started =
                            existing.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                        if started > existing_started {
                            by_session_id.insert(sid, value);
                        }
                    }
                    None => {
                        by_session_id.insert(sid, value);
                    }
                }
            }
            // 그 다음 exclude 적용
            by_session_id
                .into_values()
                .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
                    Some(pid) => !exclude.contains(&(pid as u32)),
                    None => true,
                })
                .collect()
        };
        assert!(
            reversed_order_result.is_empty(),
            "반대 순서(dedup 먼저)였다면 원본까지 유실되어 0건이어야 하는데 \
             {reversed_order_result:?}가 남았습니다 — 순서가 중요하다는 전제 자체가 \
             깨졌으니 이 테스트를 다시 검토해야 합니다"
        );

        let _ = helper.kill();
        let _ = helper.wait();
    }

    #[test]
    fn dead_pid_session_is_filtered_out() {
        let dead_pid = u32::MAX - 1;
        let sessions = vec![fixture_session(dead_pid, "dead-session", 1_000)];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());
        assert!(
            result.is_empty(),
            "죽은 pid의 세션 항목은 걸러져야 하는데 남아있습니다: {result:?}"
        );
    }

    #[test]
    fn duplicate_alive_non_child_sessions_keep_latest_started_at() {
        let my_pid = std::process::id();
        let session_id = "duplicate-session-id";
        let older = fixture_session(my_pid, session_id, 1_000);
        let newer = fixture_session(my_pid, session_id, 2_000);

        let sessions = vec![older, newer];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(result.len(), 1, "같은 sessionId는 1건으로 합쳐져야 합니다");
        assert_eq!(
            result[0].get("startedAt").and_then(|v| v.as_i64()),
            Some(2_000),
            "startedAt이 더 최신인 항목이 남아야 합니다"
        );
    }

    #[test]
    fn single_normal_session_passes_through_unchanged() {
        let my_pid = std::process::id();
        let session = fixture_session(my_pid, "solo-session", 1_000);

        let sessions = vec![session.clone()];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], session);
    }

    // 회귀 방지: `pid` 필드가 없는 registry 항목은 1단계(exclude_pids)와 2단계
    // (죽은 pid 필터) 모두 "판단 불가 → 보수적으로 통과"를 명시적으로 선택한
    // 결과다(위 함수 주석의 "판단 불가이므로 보수적으로 통과시킨다" 참조).
    // 이 항목이 dead-pid 취급으로 걸러지지 않아야 한다는 것이 의도된 규칙이며,
    // 나중에 누가 "안전하게" fail-closed로 뒤집으면 이 테스트가 실패해야 한다.
    #[test]
    fn session_missing_pid_field_is_intentionally_kept_not_excluded() {
        let session = serde_json::json!({
            "sessionId": "no-pid-session",
            "startedAt": 1_000,
            "cwd": "/tmp/fixture",
        });

        let sessions = vec![session.clone()];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(
            result.len(),
            1,
            "pid 필드가 없는 항목은 판단 불가로 보수적으로 통과해야 하는데 걸러졌습니다: {result:?}"
        );
        assert_eq!(result[0], session);
    }

    // 회귀 방지: `sessionId` 필드가 없는 registry 항목은 3단계 dedup의 그룹핑
    // 키 자체가 없으므로 "고유 취급"해 서로 dedup되지 않고 둘 다 통과해야
    // 한다(위 함수 주석의 "dedup 키가 없으므로 그대로 통과시킨다(고유 취급)"
    // 참조). 이 규칙은 의도된 것이며, 나중에 누가 sessionId 부재 항목끼리도
    // 병합하도록 "정리"하면 이 테스트가 실패해야 한다.
    #[test]
    fn sessions_missing_session_id_field_are_intentionally_treated_as_unique_not_deduped() {
        let my_pid = std::process::id();
        let first = serde_json::json!({
            "pid": my_pid,
            "startedAt": 1_000,
            "cwd": "/tmp/fixture-a",
        });
        let second = serde_json::json!({
            "pid": my_pid,
            "startedAt": 2_000,
            "cwd": "/tmp/fixture-b",
        });

        let sessions = vec![first, second];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(
            result.len(),
            2,
            "sessionId가 없는 항목끼리는 dedup 키가 없어 병합되지 않고 둘 다 남아야 \
             하는데 결과가 다릅니다: {result:?}"
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
}
