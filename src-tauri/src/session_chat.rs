// ---------------- 세션 상세 = 실제 대화 + 이어쓰기 ----------------
// 정본: docs/design/session-chat.md §3(메시지 모델·접기)·§4(IPC 계약)·
// §5(보안 경계 S1~S13)·§7(비정상 케이스). 커맨드 3개(read_session_transcript /
// send_session_message / cancel_session_turn) + 이벤트 2개(session-chat-delta /
// session-chat-done)를 그 설계 그대로 구현한다.
//
// S1/S2 불변식: 셸을 거치지 않는다(std::process::Command + 인자 배열만, `sh -c`
// 없음). 사용자가 입력한 대화 텍스트는 **stdin으로만** 자식 프로세스에 전달하고
// argv에는 절대 넣지 않는다 — argv에 넣으면 사용자가 친 `--...`가 CLI 플래그로
// 해석된다(설계 §1-B7 실측).

use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

// ==================== 상수 ====================

const MAX_MESSAGES: usize = 400;
const MAX_MESSAGE_CHARS: usize = 8000;
const MAX_INPUT_CHARS: usize = 32_000;
const MAX_CONCURRENT_TOTAL: usize = 3;

/// `dev_tools.rs`/`autonomy.rs`/`mcp_manager.rs`가 이미 정한 관례(상수를 공유
/// 하지 않고 각자 별도로 둔다, 회귀 위험 0)를 그대로 따른다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

/// 도구 실행 권한 모드(설계 §6-② 확정: A안). `--permission-prompts none`만
/// 준다 — 터미널에서 `claude`를 직접 칠 때와 동일한 권한이며, 사용자의
/// permission-mode 설정을 그대로 따른다. 프롬프트가 필요한 행동은 자동
/// 거부되어 GUI가 멈추지 않는다.
const TOOL_PERMISSION_ARGS: [&str; 2] = ["--permission-prompts", "none"];

// ==================== IPC 타입 (설계 §4-1 그대로) ====================

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub kind: String,
    pub text: String,
    pub tool_count: u32,
    pub at: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionTranscript {
    pub session_id: String,
    pub cwd: String,
    pub transcript_path: String,
    pub messages: Vec<ChatMessage>,
    pub truncated: bool,
    pub live: bool,
    /// M3: 이 세션에 지금 진행 중인 턴이 있으면 그 `turn_id`(없으면 `null`).
    /// 화면 재진입 시 프론트가 이 값으로 델타/완료 이벤트 필터에 다시 붙는다.
    pub active_turn_id: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SendStarted {
    pub turn_id: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct SessionChatDeltaPayload {
    session_id: String,
    turn_id: String,
    kind: String,
    text: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct SessionChatDonePayload {
    session_id: String,
    turn_id: String,
    ok: bool,
    canceled: bool,
    error: Option<String>,
}

// ==================== S5: session_id 검증 ====================

/// `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`와
/// 동등한 판정. 파일명·`--resume` 인자 양쪽에 쓰이므로 `..`·`/`로 시작하는 값이
/// 원천 차단된다(S5).
fn validate_session_id(session_id: &str) -> Result<(), String> {
    let b = session_id.as_bytes();
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    let ok = b.len() == 36
        && b[0..8].iter().all(|&c| is_hex(c))
        && b[8] == b'-'
        && b[9..13].iter().all(|&c| is_hex(c))
        && b[13] == b'-'
        && b[14..18].iter().all(|&c| is_hex(c))
        && b[18] == b'-'
        && b[19..23].iter().all(|&c| is_hex(c))
        && b[23] == b'-'
        && b[24..36].iter().all(|&c| is_hex(c));
    if ok {
        Ok(())
    } else {
        Err("세션 ID 형식이 올바르지 않습니다.".to_string())
    }
}

// ==================== S6: 트랜스크립트 탐색 ====================

/// `~/.claude/projects/*/` 1단계 자식 디렉터리에서 `<sid>.jsonl`만 확인한다
/// (재귀 없음, 서브에이전트 디렉터리를 건드리지 않는다 — S6).
fn resolve_transcript_path(session_id: &str) -> Result<PathBuf, String> {
    validate_session_id(session_id)?;
    let home =
        dirs::home_dir().ok_or_else(|| "이 세션의 대화 기록 파일을 찾을 수 없습니다.".to_string())?;
    let projects_dir = home.join(".claude").join("projects");
    let entries = std::fs::read_dir(&projects_dir)
        .map_err(|_| "이 세션의 대화 기록 파일을 찾을 수 없습니다.".to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let candidate = path.join(format!("{session_id}.jsonl"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("이 세션의 대화 기록 파일을 찾을 수 없습니다.".to_string())
}

// ==================== §3: jsonl → 메시지 모델 ====================

/// 접기·병합 이전의 원시 표시 단위. `tool_use_id`는 R3(tool_result) 매칭에만
/// 쓰고 최종 `ChatMessage`로 변환할 때 버린다.
#[derive(Debug)]
struct RawMsg {
    kind: &'static str, // "user" | "assistant" | "tool"
    text: String,
    tool_count: u32,
    at: Option<String>,
    /// 병합 추적은 `parse_lines`의 로컬 `text_index` 맵으로 이뤄진다 — 이
    /// 필드 자체는 기록만 되고 다시 읽히지 않지만, `RawMsg`가 assistant
    /// text 병합 단위를 나타낸다는 것을 구조적으로 보여주기 위해 남겨둔다.
    #[allow(dead_code)]
    message_id: Option<String>,
    tool_use_id: Option<String>,
    failed: bool,
}

/// R2 메타 태그 필터(QA 실데이터 발견 대응): 텍스트가 `<`로 시작하고 **알려진
/// 메타 태그 계열**이면 버린다. 태그 이름 하나를 추출해 화이트리스트에
/// 들어있는지만 확인하는 방식이라("계열 단위") 접두사를 하나씩 늘리지 않아도
/// 되고, 사용자가 실제로 `<div>`처럼 알려지지 않은 태그를 쳤을 때는 살아남는다.
/// `<local-command-*>`는 접미사가 다양해(`caveat`/`stdout`/`stderr` 등) 접두사
/// 계열로 남겨둔다.
fn is_known_meta_tag(text: &str) -> bool {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix('<') else {
        return false;
    };
    let end = rest
        .find(|c: char| c == '>' || c == '/' || c.is_whitespace())
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() {
        return false;
    }
    const KNOWN_EXACT_TAGS: [&str; 5] = [
        "task-notification",
        "system-reminder",
        "command-name",
        "ide_opened_file",
        "ide_selection",
    ];
    if KNOWN_EXACT_TAGS.contains(&name) {
        return true;
    }
    name.starts_with("local-command-")
}

/// `content`(문자열 또는 블록 배열)에서 모든 text 블록을 이어붙인다. 문자열이면
/// 그대로, 배열이면 `type=="text"`인 블록만 순서대로 모은다. **블록 단위**로
/// 메타 태그를 걸러낸다(`is_known_meta_tag`) — `ide_opened_file` 같은 IDE 자동
/// 주석은 실제 사용자 텍스트와 **같은 content 배열의 별도 블록**으로 섞여 오기
/// 때문에, 전체 텍스트를 합친 뒤 접두사만 보면 뒤따르는 진짜 사용자 메시지까지
/// 통째로 버려진다(실데이터로 확인).
fn extract_all_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            if s.trim().is_empty() || is_known_meta_tag(s) {
                None
            } else {
                Some(s.to_string())
            }
        }
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() && !is_known_meta_tag(text) {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

/// §3-2 대표 인자 추출. `input`이 객체가 아니거나(Object) 값을 못 찾으면 빈
/// 문자열. "그 외 → input의 첫 문자열 값" 폴백은 `serde_json`이 기본으로 쓰는
/// `BTreeMap`(키 알파벳 순) 기준이다 — 원본 JSON 등장 순서와 다를 수 있는
/// 근사치이며, 이 폴백은 표에 명시된 6개 도구 외의 드문 케이스에서만 쓰인다.
fn representative_arg(name: &str, input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    let direct = match name {
        "Bash" => input.get("command").and_then(|v| v.as_str()),
        "Read" | "Edit" | "Write" | "NotebookEdit" => input.get("file_path").and_then(|v| v.as_str()),
        "Grep" | "Glob" => input.get("pattern").and_then(|v| v.as_str()),
        "Task" => input.get("description").and_then(|v| v.as_str()),
        "WebFetch" => input.get("url").and_then(|v| v.as_str()),
        _ => None,
    };
    let value = direct.map(|s| s.to_string()).or_else(|| first_string_value(input));
    truncate_chars(&value.unwrap_or_default(), 60)
}

fn first_string_value(input: &Value) -> Option<String> {
    if let Value::Object(map) = input {
        for v in map.values() {
            if let Value::String(s) = v {
                return Some(s.clone());
            }
        }
    }
    None
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

/// R1~R3: `type=="user"` 줄 하나를 처리한다.
fn handle_user_line(value: &Value, at: Option<String>, raw: &mut Vec<RawMsg>) {
    let Some(content) = value.pointer("/message/content") else {
        return;
    };

    // R3: tool_result 블록이 있으면 말풍선을 만들지 않고, 직전 tool 줄의 상태만
    // 갱신한다(is_error==true인 것만 "실패" 표시).
    if let Value::Array(items) = content {
        let tool_results: Vec<&Value> = items
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            .collect();
        if !tool_results.is_empty() {
            for tr in tool_results {
                let is_error = tr.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                if !is_error {
                    continue;
                }
                let Some(tool_use_id) = tr.get("tool_use_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                if let Some(m) = raw
                    .iter_mut()
                    .rev()
                    .find(|m| m.tool_use_id.as_deref() == Some(tool_use_id))
                {
                    if !m.failed {
                        m.text.push_str(" · 실패");
                        m.failed = true;
                    }
                }
            }
            return;
        }
    }

    // R2: isSidechain / isMeta / origin.kind!="human"(origin 있을 때만) 이면 버림.
    if value.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }
    if value.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }
    if let Some(origin) = value.get("origin") {
        if origin.get("kind").and_then(|v| v.as_str()) != Some("human") {
            return;
        }
    }

    // 메타 태그 필터(§3-1 R2 텍스트 접두사 규칙)는 `extract_all_text`가 블록
    // 단위로 이미 처리한다 — `ide_opened_file` 등 IDE 자동 주석이 실제 사용자
    // 텍스트와 같은 배열의 별도 블록으로 섞여 오므로, 여기서 합쳐진 전체
    // 텍스트만 보면 실제 메시지까지 통째로 버려진다(QA 실데이터로 확인).
    let Some(text) = extract_all_text(content) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }

    raw.push(RawMsg {
        kind: "user",
        text,
        tool_count: 0,
        at,
        message_id: None,
        tool_use_id: None,
        failed: false,
    });
}

/// R4~R6: `type=="assistant"` 줄 하나를 처리한다. text 블록은 같은
/// `message.id`끼리 이어붙이고(R4), tool_use 블록은 항상 별도의 tool 줄을
/// 만든다(R5). thinking 등 그 외 블록은 버린다(R6).
fn handle_assistant_line(
    value: &Value,
    at: Option<String>,
    raw: &mut Vec<RawMsg>,
    text_index: &mut HashMap<String, usize>,
) {
    let message_id = value
        .pointer("/message/id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let Some(Value::Array(items)) = value.pointer("/message/content") else {
        return;
    };

    for item in items {
        match item.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                let text = item.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                if let Some(mid) = &message_id {
                    if let Some(&idx) = text_index.get(mid) {
                        raw[idx].text.push_str(text);
                        continue;
                    }
                }
                raw.push(RawMsg {
                    kind: "assistant",
                    text: text.to_string(),
                    tool_count: 0,
                    at: at.clone(),
                    message_id: message_id.clone(),
                    tool_use_id: None,
                    failed: false,
                });
                if let Some(mid) = &message_id {
                    text_index.insert(mid.clone(), raw.len() - 1);
                }
            }
            Some("tool_use") => {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("도구");
                let id = item.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let arg = representative_arg(name, item.get("input"));
                raw.push(RawMsg {
                    kind: "tool",
                    text: format!("⚙ {name} · {arg}"),
                    tool_count: 1,
                    at: at.clone(),
                    message_id: None,
                    tool_use_id: id,
                    failed: false,
                });
            }
            _ => {} // thinking(R6) 등은 버린다
        }
    }
}

/// 한 줄씩(이미 파싱된 `&str` 이터레이터) 처리해 원시 메시지 목록 + cwd를 만든다.
/// 깨진 JSON 줄은 건너뛴다(§7). 파일 I/O와 분리해 두어 단위 테스트에서 실제
/// 파일 없이 바로 호출할 수 있다.
fn parse_lines<'a, I: Iterator<Item = &'a str>>(lines: I) -> (Vec<RawMsg>, Option<String>) {
    let mut raw: Vec<RawMsg> = Vec::new();
    let mut cwd: Option<String> = None;
    let mut assistant_text_index: HashMap<String, usize> = HashMap::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue; // 깨진 줄만 건너뛴다(§7)
        };

        if cwd.is_none() {
            if let Some(c) = value.get("cwd").and_then(|v| v.as_str()) {
                cwd = Some(c.to_string());
            }
        }

        let at = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match value.get("type").and_then(|v| v.as_str()) {
            Some("user") => handle_user_line(&value, at, &mut raw),
            Some("assistant") => handle_assistant_line(&value, at, &mut raw, &mut assistant_text_index),
            _ => {} // R7: 그 외 모든 type은 버림
        }
    }

    (raw, cwd)
}

/// §3-3: tool 줄이 연속 3개 이상이면 `⚙ 도구 N회 실행` 하나로 접는다. 2개
/// 이하는 그대로 둔다.
fn fold_consecutive_tools(raw: Vec<RawMsg>) -> Vec<RawMsg> {
    let mut out: Vec<RawMsg> = Vec::with_capacity(raw.len());
    let mut iter = raw.into_iter().peekable();
    while let Some(first) = iter.next() {
        if first.kind != "tool" {
            out.push(first);
            continue;
        }
        let mut group = vec![first];
        while iter.peek().map(|m| m.kind == "tool").unwrap_or(false) {
            group.push(iter.next().unwrap());
        }
        if group.len() >= 3 {
            let n = group.len() as u32;
            let at = group.last().and_then(|m| m.at.clone());
            out.push(RawMsg {
                kind: "tool",
                text: format!("⚙ 도구 {n}회 실행"),
                tool_count: n,
                at,
                message_id: None,
                tool_use_id: None,
                failed: false,
            });
        } else {
            out.extend(group);
        }
    }
    out
}

/// §3-4: 메시지 400개 상한(오래된 것부터 버림) + 메시지당 8000자 상한.
fn finalize_messages(raw: Vec<RawMsg>) -> (Vec<ChatMessage>, bool) {
    let total = raw.len();
    let truncated = total > MAX_MESSAGES;
    let start = if truncated { total - MAX_MESSAGES } else { 0 };
    let messages = raw
        .into_iter()
        .skip(start)
        .map(|m| ChatMessage {
            kind: m.kind.to_string(),
            text: truncate_chars(&m.text, MAX_MESSAGE_CHARS),
            tool_count: m.tool_count,
            at: m.at,
        })
        .collect();
    (messages, truncated)
}

/// 단위 테스트 전용 파이프라인: 줄 목록 → 원시 메시지 → 접기 → 상한 적용.
/// 프로덕션 경로(`read_session_transcript`)는 파일에서 읽은 줄을 이 함수
/// 대신 직접 조립한다(cwd 등 부가 정보를 함께 반환해야 해서) — 로직 자체는
/// `parse_lines`/`fold_consecutive_tools`/`finalize_messages`로 동일하게
/// 공유된다.
#[cfg(test)]
fn build_transcript_messages<'a, I: Iterator<Item = &'a str>>(lines: I) -> (Vec<ChatMessage>, bool) {
    let (raw, _cwd) = parse_lines(lines);
    let folded = fold_consecutive_tools(raw);
    finalize_messages(folded)
}

fn parse_transcript_file(path: &Path) -> Result<(Vec<RawMsg>, Option<String>), String> {
    let file = std::fs::File::open(path)
        .map_err(|_| "이 세션의 대화 기록 파일을 찾을 수 없습니다.".to_string())?;
    let reader = BufReader::new(file);
    let raw_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    Ok(parse_lines(raw_lines.iter().map(|s| s.as_str())))
}

/// `send_session_message`가 필요로 하는 cwd만 뽑는 가벼운 스캔(S4) — 첫
/// `cwd` 필드를 찾는 즉시 멈춘다. 전체 메시지 파싱과 별개로 둬서 전송 경로가
/// 불필요하게 무거워지지 않는다.
fn read_cwd_from_transcript(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|_| "이 세션의 대화 기록 파일을 찾을 수 없습니다.".to_string())?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(cwd) = value.get("cwd").and_then(|v| v.as_str()) {
            return Ok(cwd.to_string());
        }
    }
    Err("세션의 작업 폴더를 찾을 수 없습니다: (트랜스크립트에서 cwd를 찾지 못했습니다)".to_string())
}

// ==================== live 배지(§6-①) ====================

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// 이 값은 전송 허용 여부를 결정하는 하드 게이트다(§6-①: live면 전송 금지).
/// Windows에서 추가 의존성 없이 PID 생존 여부를 확인할 안전한 표준 API가
/// 없어 registry 항목 존재만으로 항상 `true`(live)를 반환한다 — 그 결과
/// Windows에서는 모든 세션이 fail-closed로 항상 읽기 전용이 된다.
#[cfg(windows)]
fn pid_alive(_pid: u32) -> bool {
    true
}

/// `~/.claude/sessions/*.json`(현재 다른 창에서 실행 중인 세션의 registry)에
/// 이 `session_id`가 살아있는 pid로 등록되어 있는가.
fn is_session_live(session_id: &str) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let dir = home.join(".claude").join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if value.get("sessionId").and_then(|v| v.as_str()) != Some(session_id) {
            continue;
        }
        if let Some(pid) = value.get("pid").and_then(|v| v.as_u64()) {
            if pid_alive(pid as u32) {
                return true;
            }
        }
    }
    false
}

// ==================== (a) read_session_transcript ====================

#[tauri::command]
pub fn read_session_transcript(session_id: String) -> Result<SessionTranscript, String> {
    let path = resolve_transcript_path(&session_id)?;
    let (raw, cwd) = parse_transcript_file(&path)?;
    let folded = fold_consecutive_tools(raw);
    let (messages, truncated) = finalize_messages(folded);
    let live = is_session_live(&session_id);
    let active_turn_id = active_turn_id_for_session(&session_id);

    Ok(SessionTranscript {
        session_id,
        cwd: cwd.unwrap_or_default(),
        transcript_path: path.to_string_lossy().to_string(),
        messages,
        truncated,
        live,
        active_turn_id,
    })
}

// ==================== S8: 세션당 1개 / 전역 3개 동시 실행 ====================

struct ActiveTurnState {
    session_id: String,
    pid: Option<u32>,
    canceled: bool,
}

static ACTIVE_TURNS: OnceLock<Mutex<HashMap<String, ActiveTurnState>>> = OnceLock::new();

fn active_turns() -> &'static Mutex<HashMap<String, ActiveTurnState>> {
    ACTIVE_TURNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// M3: `session_id`로 `ACTIVE_TURNS`를 역조회한다 — 화면 재진입 시 프론트가
/// 진행 중인 턴에 다시 붙을 수 있도록 `read_session_transcript`가 이 값을
/// `active_turn_id`로 실어 보낸다.
fn active_turn_id_for_session(session_id: &str) -> Option<String> {
    let map = active_turns().lock().unwrap();
    map.iter()
        .find(|(_, t)| t.session_id == session_id)
        .map(|(turn_id, _)| turn_id.clone())
}

fn register_turn(turn_id: &str, session_id: &str) -> Result<(), String> {
    let mut map = active_turns().lock().unwrap();
    if map.values().any(|t| t.session_id == session_id) {
        return Err("이 세션에 이미 진행 중인 요청이 있습니다.".to_string());
    }
    if map.len() >= MAX_CONCURRENT_TOTAL {
        return Err("동시에 실행할 수 있는 요청은 최대 3개입니다.".to_string());
    }
    map.insert(
        turn_id.to_string(),
        ActiveTurnState {
            session_id: session_id.to_string(),
            pid: None,
            canceled: false,
        },
    );
    Ok(())
}

/// pid를 등록하고, 그 사이 이미 취소 요청이 들어왔었는지 반환한다.
fn set_turn_pid(turn_id: &str, pid: u32) -> bool {
    let mut map = active_turns().lock().unwrap();
    if let Some(entry) = map.get_mut(turn_id) {
        entry.pid = Some(pid);
        entry.canceled
    } else {
        false
    }
}

/// 레지스트리에서 제거하고, 취소된 상태였는지 반환한다.
fn finish_turn(turn_id: &str) -> bool {
    let mut map = active_turns().lock().unwrap();
    map.remove(turn_id).map(|t| t.canceled).unwrap_or(false)
}

/// 앱 종료 시 호출한다(M1). `process_group(0)`(unix)으로 새 프로세스 그룹
/// 리더로 분리된 자식은 앱 프로세스가 죽어도 함께 죽지 않으므로, `ACTIVE_TURNS`에
/// 남아있는 모든 턴을 순회해 명시적으로 정리한다 — `autonomy::request_shutdown_and_wait`
/// 와 같은 목적(고아 프로세스 방지)이며, 여기서는 이미 검증된 `cancel_session_turn`과
/// 동일한 `kill_process_group_with_grace`(SIGTERM → 3초 유예 → SIGKILL)를 그대로
/// 재사용한다.
pub fn request_shutdown() {
    let pids: Vec<u32> = {
        let map = active_turns().lock().unwrap();
        map.values().filter_map(|t| t.pid).collect()
    };
    for pid in pids {
        kill_process_group_with_grace(pid);
    }
}

// ==================== 프로세스 그룹 kill(S9, 패턴만 dev_tools.rs 승계) ====================

#[cfg(unix)]
fn kill_process_group_with_grace(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        if pid_alive(pid) {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    });
}

/// Windows에는 POSIX 프로세스 그룹이 없다. `taskkill /T /F`는 셸을 거치지
/// 않는 직접 프로세스 실행(S1 불변식 유지)이며 대상 트리를 강제 종료하므로
/// 별도의 유예 폴링이 필요 없다.
#[cfg(windows)]
fn kill_process_group_with_grace(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

fn tail_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.trim().to_string();
    }
    let start = text.len() - max_bytes;
    let mut boundary = start;
    while boundary < text.len() && !text.is_char_boundary(boundary) {
        boundary += 1;
    }
    text[boundary..].trim().to_string()
}

/// UUID v4 — `uuid` 크레이트를 새로 추가하지 않고 기존 의존성 `rand`로 직접
/// 만든다(MVP: 새 의존성 없이 충분).
fn generate_turn_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn build_claude_args(session_id: &str) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--resume".to_string(),
        session_id.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--include-partial-messages".to_string(),
    ];
    args.extend(TOOL_PERMISSION_ARGS.iter().map(|s| s.to_string()));
    args
}

fn emit_delta(app: &tauri::AppHandle, session_id: &str, turn_id: &str, kind: &str, text: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        "session-chat-delta",
        SessionChatDeltaPayload {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: kind.to_string(),
            text: text.to_string(),
        },
    );
}

fn emit_done(
    app: &tauri::AppHandle,
    session_id: &str,
    turn_id: &str,
    ok: bool,
    canceled: bool,
    error: Option<String>,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "session-chat-done",
        SessionChatDonePayload {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            ok,
            canceled,
            error,
        },
    );
}

/// 스폰부터 종료까지: stdin에 프롬프트를 쓰고(S2) 닫은 뒤, stdout을 줄 단위로
/// 읽어 델타를 emit하고, stderr는 별도 스레드로 소비(파이프 교착 방지)한 뒤
/// 정확히 한 번 `session-chat-done`을 emit한다(§4-2).
#[allow(clippy::too_many_arguments)]
fn run_turn(
    app: tauri::AppHandle,
    claude_path: String,
    path_env: String,
    cwd: String,
    session_id: String,
    turn_id: String,
    text: String,
) {
    let args = build_claude_args(&session_id);

    let mut command = Command::new(&claude_path);
    command.args(&args);
    command.current_dir(&cwd);
    command.env("PATH", &path_env);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            finish_turn(&turn_id);
            emit_done(
                &app,
                &session_id,
                &turn_id,
                false,
                false,
                Some(format!("claude 명령을 실행하지 못했습니다: {e}")),
            );
            return;
        }
    };

    let pid = child.id();
    let already_canceled = set_turn_pid(&turn_id, pid);
    if already_canceled {
        kill_process_group_with_grace(pid);
    }

    // 리더를 stdin 쓰기보다 먼저 시작한다(m1): 최대 32,000자(~96KB)는 파이프
    // 버퍼(수십 KB)를 넘을 수 있어, 자식이 먼저 내보내는 출력을 아무도 읽지
    // 않는 상태로 큰 stdin을 동기 write하면 양쪽이 서로 파이프가 비/차길
    // 기다리며 교착할 수 있다. stdout은 메인 스레드가 아래에서 바로 읽고,
    // stderr는 이미 별도 스레드다. stdin 쓰기도 별도 스레드로 돌려 모든 파이프가
    // 동시에 흐르게 한다.
    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    // S2: 사용자 텍스트는 stdin으로만 전달한다. argv에는 절대 넣지 않는다.
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
            // stdin이 여기서 drop되어 자식에 EOF가 전달된다.
        });
    }

    let mut result_error: Option<String> = None;
    let mut saw_result = false;

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match value.get("type").and_then(|v| v.as_str()) {
                Some("stream_event") => {
                    let event_type = value.pointer("/event/type").and_then(|v| v.as_str());
                    if event_type == Some("content_block_delta") {
                        let delta_type = value.pointer("/event/delta/type").and_then(|v| v.as_str());
                        if delta_type == Some("text_delta") {
                            if let Some(t) = value.pointer("/event/delta/text").and_then(|v| v.as_str()) {
                                emit_delta(&app, &session_id, &turn_id, "text", t);
                            }
                        }
                    }
                }
                Some("assistant") => {
                    if let Some(Value::Array(items)) = value.pointer("/message/content") {
                        for item in items {
                            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("도구");
                                let arg = representative_arg(name, item.get("input"));
                                let line = format!("⚙ {name} · {arg}");
                                emit_delta(&app, &session_id, &turn_id, "tool", &line);
                            }
                        }
                    }
                }
                Some("result") => {
                    saw_result = true;
                    let is_error = value.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                    if is_error {
                        let subtype = value.get("subtype").and_then(|v| v.as_str()).unwrap_or("error");
                        let result_text = value.get("result").and_then(|v| v.as_str()).unwrap_or("");
                        result_error = Some(if result_text.is_empty() {
                            subtype.to_string()
                        } else {
                            format!("{subtype}: {result_text}")
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let stderr_bytes = stderr_reader.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes).to_string();

    let status = child.wait();
    let exit_code = status.as_ref().ok().and_then(|s| s.code());

    let canceled = finish_turn(&turn_id);

    let error = if canceled {
        None
    } else if let Some(e) = result_error {
        Some(e)
    } else if exit_code != Some(0) && !saw_result {
        let tail = tail_bytes(&stderr_text, 4096);
        Some(if tail.is_empty() {
            format!("claude 프로세스가 비정상 종료됐습니다(exit={exit_code:?}).")
        } else {
            tail
        })
    } else {
        None
    };

    let ok = !canceled && error.is_none();
    emit_done(&app, &session_id, &turn_id, ok, canceled, error);
}

// ==================== (b) send_session_message ====================

#[tauri::command]
pub fn send_session_message(
    app: tauri::AppHandle,
    session_id: String,
    text: String,
) -> Result<SendStarted, String> {
    if text.trim().is_empty() {
        return Err("보낼 내용을 입력하세요.".to_string());
    }
    if text.chars().count() > MAX_INPUT_CHARS {
        return Err("한 번에 보낼 수 있는 길이를 초과했습니다(최대 32,000자).".to_string());
    }

    let transcript_path = resolve_transcript_path(&session_id)?;
    let cwd = read_cwd_from_transcript(&transcript_path)?;
    if !Path::new(&cwd).is_dir() {
        return Err(format!("세션의 작업 폴더를 찾을 수 없습니다: {cwd}"));
    }

    // §6-① 갱신: 세션목록에 뜨는 세션은 §1-E 실측대로 예외 없이 전부 "지금
    // 다른 창에서 실행 중"이라 원래의 하드 게이트(live면 무조건 거부)를 두면
    // 입력창이 항상 비활성化되어 재개 기능 자체가 성립하지 않았다(문서 §6-①
    // 트레이드오프 표의 B안 단점 그대로 재현됨). 그래서 차단을 제거하고
    // read_session_transcript가 돌려주는 `live` 플래그로 화면에 경고만
    // 띄운다(docs/design/session-chat.md §6-① 결정 갱신 참조) — 다른 창이
    // 모르는 채 같은 jsonl에 이어붙는 대화 분기(§1-E, 손상은 아님)는 감수한다.
    let claude_path = crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
        .ok_or_else(|| {
            "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).".to_string()
        })?;

    let turn_id = generate_turn_id();
    register_turn(&turn_id, &session_id)?;

    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));
    let session_id_for_thread = session_id.clone();
    let turn_id_for_thread = turn_id.clone();

    std::thread::spawn(move || {
        run_turn(
            app,
            claude_path,
            path_env,
            cwd,
            session_id_for_thread,
            turn_id_for_thread,
            text,
        );
    });

    Ok(SendStarted { turn_id })
}

// ==================== (c) cancel_session_turn ====================

#[tauri::command]
pub fn cancel_session_turn(turn_id: String) -> Result<(), String> {
    let pid = {
        let mut map = active_turns().lock().unwrap();
        let Some(entry) = map.get_mut(&turn_id) else {
            // 이미 끝난 턴 — 정상 케이스(§4-1)
            return Ok(());
        };
        entry.canceled = true;
        entry.pid
    };
    if let Some(pid) = pid {
        kill_process_group_with_grace(pid);
    }
    Ok(())
}

// ==================== 단위 테스트(설계 §10 (a)~(f)) ====================

#[cfg(test)]
mod tests {
    use super::*;

    // (a) tool_result가 말풍선을 만들지 않는다
    #[test]
    fn tool_result_does_not_create_a_bubble() {
        let lines = [
            r#"{"type":"assistant","timestamp":"t1","message":{"id":"m1","content":[{"type":"tool_use","id":"tu1","name":"Bash","input":{"command":"ls -1"}}]}}"#,
            r#"{"type":"user","timestamp":"t2","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"out1.txt","is_error":false}]}}"#,
        ];
        let (messages, _truncated) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1, "tool_result 줄이 별도 말풍선을 만들면 안 됩니다");
        assert_eq!(messages[0].kind, "tool");
    }

    // tool_result의 is_error==true면 해당 tool 줄에 실패 표시가 붙는다(R3 부가 검증)
    #[test]
    fn failed_tool_result_marks_the_preceding_tool_line() {
        let lines = [
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"tu1","name":"Bash","input":{"command":"exit 1"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"err","is_error":true}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1);
        assert!(messages[0].text.contains("실패"), "실패 표시가 없습니다: {}", messages[0].text);
    }

    // (b) 같은 message.id의 assistant 여러 줄이 하나로 합쳐진다
    #[test]
    fn same_message_id_assistant_lines_are_merged_into_one_bubble() {
        let lines = [
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"안녕"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"하세요"}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1, "같은 message.id는 하나의 말풍선으로 합쳐져야 합니다");
        assert_eq!(messages[0].text, "안녕하세요");
        assert_eq!(messages[0].kind, "assistant");
    }

    // (c) origin.kind="task-notification" user 줄이 걸러진다
    #[test]
    fn task_notification_origin_user_line_is_filtered_out() {
        let lines = [
            r#"{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"작업 알림입니다"}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert!(messages.is_empty(), "task-notification origin 줄은 버려져야 합니다");
    }

    // (d) origin 없는 user 줄은 살아남는다
    #[test]
    fn user_line_without_origin_field_survives() {
        let lines = [r#"{"type":"user","message":{"content":"안녕하세요"}}"#];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, "user");
        assert_eq!(messages[0].text, "안녕하세요");
    }

    // QA 실데이터 고정(Medium): `/clear` 등 슬래시 커맨드의 `<command-name>` 마커가
    // 그대로 노출되지 않는다. 실측 원문 형태(단일 text 블록에 3개 태그가 개행으로
    // 이어붙어 있음)를 그대로 고정한다.
    #[test]
    fn slash_command_name_marker_line_is_filtered_out() {
        let lines = [
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<command-name>/clear</command-name>\n            <command-message>clear</command-message>\n            <command-args></command-args>"}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert!(messages.is_empty(), "<command-name> 마커 줄은 버려져야 합니다");
    }

    // QA 실데이터 고정(Medium): `<ide_opened_file>`은 실제 사용자 텍스트와 같은
    // content 배열의 **별도 블록**으로 섞여 온다(실측). 전체 텍스트를 합친 뒤
    // 접두사만 보는 필터는 뒤따르는 진짜 메시지까지 통째로 버리므로, 메타 블록만
    // 걸러지고 실제 메시지는 살아남아야 한다.
    #[test]
    fn ide_opened_file_block_is_filtered_but_real_text_in_same_message_survives() {
        let lines = [
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<ide_opened_file>The user opened the file /a/b.ts in the IDE.</ide_opened_file>"},{"type":"text","text":"이 파일 고쳐줘"}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1, "메타 블록만 걸러지고 실제 메시지는 남아야 합니다");
        assert_eq!(messages[0].text, "이 파일 고쳐줘");
    }

    // QA 실데이터 고정(Medium): `<ide_selection>`도 같은 계열로 필터된다.
    #[test]
    fn ide_selection_marker_line_is_filtered_out() {
        let lines = [
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<ide_selection>The user selected lines 1-3.</ide_selection>"}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert!(messages.is_empty(), "<ide_selection> 마커 줄은 버려져야 합니다");
    }

    // 화이트리스트 방식 회귀 방지: 사용자가 실제로 `<div>` 같은 알려지지 않은
    // 태그를 쳤을 때는 살아남아야 한다(접두사를 하나씩 늘리는 방식이면 깨지기
    // 쉬운 지점).
    #[test]
    fn unknown_angle_bracket_text_is_not_treated_as_meta_tag() {
        let lines = [r#"{"type":"user","message":{"content":"<div>테스트</div>"}}"#];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1, "알려지지 않은 태그는 필터되면 안 됩니다");
        assert_eq!(messages[0].text, "<div>테스트</div>");
    }

    // (e) 연속 3개 이상 tool이 하나로 접힌다
    #[test]
    fn three_or_more_consecutive_tool_lines_fold_into_one() {
        let lines = [
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"pwd"}}]}}"#,
            r#"{"type":"assistant","message":{"id":"m3","content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"echo hi"}}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 1, "3개 이상의 연속 tool 줄은 하나로 접혀야 합니다");
        assert_eq!(messages[0].kind, "tool");
        assert_eq!(messages[0].tool_count, 3);
        assert_eq!(messages[0].text, "⚙ 도구 3회 실행");
    }

    // 2개 이하는 접히지 않는다(§3-3 경계값)
    #[test]
    fn two_consecutive_tool_lines_are_not_folded() {
        let lines = [
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"pwd"}}]}}"#,
        ];
        let (messages, _) = build_transcript_messages(lines.iter().copied());
        assert_eq!(messages.len(), 2, "2개 이하는 접히지 않아야 합니다");
    }

    // (f) 400개 상한에서 최신이 남는다
    #[test]
    fn message_cap_keeps_the_most_recent_400() {
        let owned_lines: Vec<String> = (0..450)
            .map(|i| format!(r#"{{"type":"user","message":{{"content":"msg-{i}"}}}}"#))
            .collect();
        let (messages, truncated) =
            build_transcript_messages(owned_lines.iter().map(|s| s.as_str()));
        assert!(truncated, "400개를 초과하면 truncated=true여야 합니다");
        assert_eq!(messages.len(), 400);
        assert_eq!(messages[0].text, "msg-50", "가장 오래된 50개가 버려져야 합니다");
        assert_eq!(messages[399].text, "msg-449", "최신 메시지가 남아야 합니다");
    }

    // session_id 형식 검증
    #[test]
    fn validate_session_id_rejects_path_traversal_like_input() {
        assert!(validate_session_id("11111111-2222-4333-8444-555555555501").is_ok());
        assert!(validate_session_id("../../etc/passwd").is_err());
        assert!(validate_session_id("not-a-uuid").is_err());
    }

    // representative_arg §3-2 규칙
    #[test]
    fn representative_arg_uses_tool_specific_field() {
        let input = serde_json::json!({"command": "ls -1"});
        assert_eq!(representative_arg("Bash", Some(&input)), "ls -1");
        let input2 = serde_json::json!({"file_path": "/tmp/a.txt"});
        assert_eq!(representative_arg("Read", Some(&input2)), "/tmp/a.txt");
    }

    // turn_id는 UUID v4 형식이어야 한다
    #[test]
    fn generate_turn_id_produces_uuid_v4_shape() {
        let id = generate_turn_id();
        assert!(validate_session_id(&id).is_ok(), "turn_id는 표준 UUID 형태여야 합니다: {id}");
        assert_eq!(id.chars().nth(14), Some('4'), "버전 니블이 4여야 합니다");
    }

    // S8: 세션당 1개 동시 실행 제한
    #[test]
    fn register_turn_rejects_second_concurrent_turn_for_same_session() {
        // 다른 테스트와 정적 레지스트리를 공유하므로 이 테스트 전용 세션 id를 쓴다.
        let session = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
        let turn1 = "11111111-1111-4111-8111-111111111111";
        let turn2 = "22222222-2222-4222-8222-222222222222";
        assert!(register_turn(turn1, session).is_ok());
        let second = register_turn(turn2, session);
        assert!(second.is_err());
        assert_eq!(second.unwrap_err(), "이 세션에 이미 진행 중인 요청이 있습니다.");
        // 정리
        finish_turn(turn1);
    }

    // ==================== QA 실데이터 검증(#[ignore] — 이 머신 의존, CI에서 실행 안 함) ====================
    // 실행: cargo test -- --ignored --nocapture session_chat::tests::real_world_transcripts
    // 목적: 합성 fixture가 아니라 ~/.claude/projects/**/*.jsonl 실파일에 대해
    // read_session_transcript()를 프로덕션 코드 그대로(수정 없이) 호출해
    // 패닉/빈 결과/상한 위반 여부를 눈으로 확인한다. 읽기 전용이라 부작용 없음.
    #[test]
    #[ignore]
    fn real_world_transcripts_parse_without_panicking_or_garbage() {
        let home = dirs::home_dir().expect("home dir");
        let projects_dir = home.join(".claude").join("projects");
        let mut candidates: Vec<(PathBuf, u64)> = Vec::new();
        for entry in std::fs::read_dir(&projects_dir).expect("read projects dir").flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    candidates.push((p, size));
                }
            }
        }
        candidates.sort_by_key(|(_, size)| *size);
        let n = candidates.len();
        assert!(n >= 10, "실 jsonl 파일이 10개 미만입니다: {n}");

        // 크기 다양성: 가장 작은 4개 + 중간 4개 + 가장 큰 4개 (총 12개, 중복 제거)
        let mut picked: Vec<&(PathBuf, u64)> = Vec::new();
        picked.extend(candidates.iter().take(4));
        picked.extend(candidates.iter().skip(n / 2).take(4));
        picked.extend(candidates.iter().rev().take(4));
        picked.sort_by_key(|(p, _)| p.clone());
        picked.dedup_by_key(|(p, _)| p.clone());

        eprintln!("=== 실데이터 검증 대상 {}개 (전체 후보 {n}개 중) ===", picked.len());

        let mut empty_sessions: Vec<String> = Vec::new();
        for (path, size) in &picked {
            let sid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let sid_for_panic = sid.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(move || read_session_transcript(sid_for_panic));
            let elapsed = start.elapsed();
            match result {
                Ok(Ok(t)) => {
                    eprintln!(
                        "OK  sid={sid} size={size}B elapsed={elapsed:?} messages={} truncated={} live={} cwd={}",
                        t.messages.len(),
                        t.truncated,
                        t.live,
                        t.cwd
                    );
                    if t.messages.is_empty() {
                        empty_sessions.push(format!("{sid} ({path:?}, {size}B)"));
                    }
                    for m in t.messages.iter().take(3) {
                        let preview: String = m.text.chars().take(100).collect();
                        eprintln!(
                            "    [{}] toolCount={} {}",
                            m.kind,
                            m.tool_count,
                            preview.replace('\n', " ")
                        );
                    }
                    assert!(
                        t.messages.len() <= MAX_MESSAGES,
                        "400개 상한을 넘었습니다: sid={sid} len={}",
                        t.messages.len()
                    );
                    for m in &t.messages {
                        assert!(
                            m.text.chars().count() <= MAX_MESSAGE_CHARS + 1,
                            "8000자 컷이 걸리지 않았습니다: sid={sid} len={}",
                            m.text.chars().count()
                        );
                    }
                    // 접기 규칙(§3-3) 위반 검증: tool_count>=3인 줄이 연속 3개 tool
                    // 원본 없이 하나로 접혔는지는 여기서 직접 재현하기 어려우므로,
                    // 대신 "tool" 종류인데 tool_count==0인 비정상 상태만 방어적으로 체크.
                    for m in &t.messages {
                        if m.kind == "tool" {
                            assert!(m.tool_count >= 1, "tool 메시지의 tool_count가 0입니다: sid={sid}");
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("ERR sid={sid} error={e}");
                }
                Err(_) => {
                    panic!("PANIC while parsing sid={sid} path={path:?}");
                }
            }
        }

        eprintln!(
            "=== 빈 세션 {}개/{} ===\n{}",
            empty_sessions.len(),
            picked.len(),
            empty_sessions.join("\n")
        );
    }

    // 실행: cargo test -- --ignored --nocapture session_chat::tests::live_flag_matches
    // 목적: ~/.claude/sessions/*.json registry에 살아있는 pid로 등록된 세션에
    // 대해 read_session_transcript(...).live == true 인지 실제로 확인한다.
    #[test]
    #[ignore]
    fn live_flag_matches_sessions_registry() {
        let home = dirs::home_dir().expect("home dir");
        let sessions_dir = home.join(".claude").join("sessions");
        let entries = std::fs::read_dir(&sessions_dir).expect("read ~/.claude/sessions");

        let mut checked = 0;
        let mut live_true = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            let Some(sid) = value.get("sessionId").and_then(|v| v.as_str()) else {
                eprintln!("SKIP {path:?}: sessionId 필드 없음");
                continue;
            };
            if validate_session_id(sid).is_err() {
                continue;
            }
            if resolve_transcript_path(sid).is_err() {
                eprintln!("SKIP sid={sid}: 대응하는 jsonl 트랜스크립트를 찾지 못함(registry엔 있음)");
                continue;
            }
            let t = read_session_transcript(sid.to_string()).expect("read transcript");
            checked += 1;
            if t.live {
                live_true += 1;
            }
            eprintln!(
                "sid={sid} live={} pid={:?} entrypoint={:?} kind={:?}",
                t.live,
                value.get("pid"),
                value.get("entrypoint"),
                value.get("kind")
            );
            assert!(t.live, "registry에 살아있는 pid로 등록된 세션인데 live=false로 나왔습니다: sid={sid}");
        }

        eprintln!("=== checked={checked} live_true={live_true} ===");
        assert!(checked > 0, "registry에서 jsonl까지 매칭되는 세션을 하나도 찾지 못했습니다");
    }
}
