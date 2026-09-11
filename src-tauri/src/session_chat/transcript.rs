// ---------------- 트랜스크립트 파싱/렌더 변환 ----------------
// jsonl(`~/.claude/projects/*/*.jsonl`) 한 줄씩을 §3 규칙(R1~R7)에 따라
// 화면에 보일 `ChatMessage` 목록으로 바꾼다: 파싱(`parse_lines`) → 도구 접기
// (`fold_consecutive_tools`, §3-3) → 개수/길이 상한(`finalize_messages`, §3-4).
// S5(session_id 검증)·S6(트랜스크립트 탐색)도 이 파일에 함께 둔다 — 둘 다
// "jsonl 파일을 찾아 읽는다"는 같은 관심사의 앞단이다.

use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

// ==================== §3-4 상한 상수 ====================

pub(crate) const MAX_MESSAGES: usize = 400;
pub(crate) const MAX_MESSAGE_CHARS: usize = 8000;

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
    /// M3: 이 세션에 지금 진행 중인 턴이 있으면 그 `turn_id`(없으면 `null`).
    /// 화면 재진입 시 프론트가 이 값으로 델타/완료 이벤트 필터에 다시 붙는다.
    pub active_turn_id: Option<String>,
}

// ==================== S5: session_id 검증 ====================

/// `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`와
/// 동등한 판정. 파일명·`--resume` 인자 양쪽에 쓰이므로 `..`·`/`로 시작하는 값이
/// 원천 차단된다(S5).
pub(crate) fn validate_session_id(session_id: &str) -> Result<(), String> {
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
pub(crate) fn resolve_transcript_path(session_id: &str) -> Result<PathBuf, String> {
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
pub(crate) fn representative_arg(name: &str, input: Option<&Value>) -> String {
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

/// `read_session_transcript` 커맨드가 필요로 하는 파일 읽기 → 파싱 → 도구
/// 접기 → 상한 적용 파이프라인을 하나로 묶어 노출한다(원래
/// `read_session_transcript` 본문에 있던 4줄을 그대로 옮긴 것 — 로직 변경
/// 없음). `RawMsg`는 이 모듈 내부 표현이라 커맨드 계층에 노출하지 않는다.
pub(crate) fn read_transcript_messages(
    path: &Path,
) -> Result<(Vec<ChatMessage>, bool, Option<String>), String> {
    let (raw, cwd) = parse_transcript_file(path)?;
    let folded = fold_consecutive_tools(raw);
    let (messages, truncated) = finalize_messages(folded);
    Ok((messages, truncated, cwd))
}

/// `send_session_message`가 필요로 하는 cwd만 뽑는 가벼운 스캔(S4) — 첫
/// `cwd` 필드를 찾는 즉시 멈춘다. 전체 메시지 파싱과 별개로 둬서 전송 경로가
/// 불필요하게 무거워지지 않는다.
pub(crate) fn read_cwd_from_transcript(path: &Path) -> Result<String, String> {
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
}
