// ---------------- 프로세스 spawn과 턴 수명주기 ----------------
// `claude -p`를 실제로 띄우고(S1/S2 불변식: 셸을 거치지 않고, 사용자 텍스트는
// stdin으로만 전달), 진행 중인 턴을 세션당 1개/전역 3개로 제한하며(S8),
// 취소·앱 종료 시 프로세스 그룹을 정리한다(S9, M1).

use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::transcript::representative_arg;

const MAX_CONCURRENT_TOTAL: usize = 3;

/// 도구 실행 권한 모드(설계 §6-② 확정: A안). `--permission-prompts none`만
/// 준다 — 터미널에서 `claude`를 직접 칠 때와 동일한 권한이며, 사용자의
/// permission-mode 설정을 그대로 따른다. 프롬프트가 필요한 행동은 자동
/// 거부되어 GUI가 멈추지 않는다.
const TOOL_PERMISSION_ARGS: [&str; 2] = ["--permission-prompts", "none"];

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

// pid 생존 확인은 `crate::process_util::pid_alive`(공유 모듈)를 쓴다 —
// `kill_process_group_with_grace`(SIGTERM 유예 종료 확인)가 계속 사용한다.
use crate::process_util::pid_alive;

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
pub(crate) fn active_turn_id_for_session(session_id: &str) -> Option<String> {
    let map = active_turns().lock().unwrap();
    map.iter()
        .find(|(_, t)| t.session_id == session_id)
        .map(|(turn_id, _)| turn_id.clone())
}

/// `lib.rs`의 `read_claude_sessions()`가 registry에서 우리 자식 프로세스(앱이
/// `send_session_message`로 스스로 띄운 `claude -p`) 항목을 먼저 제외할 수
/// 있도록 현재 진행 중인 턴들의 pid 집합을 노출한다. `ACTIVE_TURNS`를 복붙해
/// lib.rs에 별도로 두지 않고 이 하나의 조회 헬퍼를 공유한다.
pub(crate) fn active_turn_pids() -> std::collections::HashSet<u32> {
    let map = active_turns().lock().unwrap();
    map.values().filter_map(|t| t.pid).collect()
}

pub(crate) fn register_turn(turn_id: &str, session_id: &str) -> Result<(), String> {
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
pub(crate) fn kill_process_group_with_grace(pid: u32) {
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
pub(crate) fn kill_process_group_with_grace(pid: u32) {
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
/// 만든다(MVP: 새 의존성 없이 충분). `turn_id`뿐 아니라 신규 세션의
/// `session_id`(앱이 사전 지정해 `claude --session-id`에 넘기는 값, §
/// `start_new_session_message`)도 이 하나의 생성기를 공유한다 — 두 값 모두
/// 표준 UUID v4 형태만 요구하므로 용도별로 복붙하지 않는다.
pub(crate) fn generate_uuid_v4() -> String {
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

/// `resume=true`면 기존 재개 경로(`--resume <session_id>`, 기존 동작 그대로
/// 회귀 없음). `resume=false`면 신규 세션 경로 — `--session-id <session_id>`로
/// 앱이 사전 생성한 UUID를 CLI에 지정해, `run_turn()`이 stream-json의
/// `system`/`init` 이벤트에서 session_id를 사후 캡처하지 않아도 되게 한다
/// (그 사후 캡처 경로 자체가 없다는 것이 이 설계의 핵심 — §설계 결정 참조).
fn build_claude_args(session_id: &str, resume: bool) -> Vec<String> {
    let mut args = vec!["-p".to_string()];
    if resume {
        args.push("--resume".to_string());
        args.push(session_id.to_string());
    } else {
        args.push("--session-id".to_string());
        args.push(session_id.to_string());
    }
    args.push("--output-format".to_string());
    args.push("stream-json".to_string());
    args.push("--verbose".to_string());
    args.push("--include-partial-messages".to_string());
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
pub(crate) fn run_turn(
    app: tauri::AppHandle,
    claude_path: String,
    path_env: String,
    cwd: String,
    session_id: String,
    turn_id: String,
    text: String,
    resume: bool,
) {
    let args = build_claude_args(&session_id, resume);

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

/// 진행 중인 턴을 취소한다: 레지스트리에 `canceled` 표시를 남기고, pid가
/// 이미 확보돼 있으면 즉시 프로세스 그룹을 정리한다. `turn_id`가 이미 끝난
/// 턴이면(레지스트리에 없음) 아무 것도 하지 않는다 — 정상 케이스.
pub(crate) fn cancel_turn(turn_id: &str) {
    let pid = {
        let mut map = active_turns().lock().unwrap();
        let Some(entry) = map.get_mut(turn_id) else {
            return;
        };
        entry.canceled = true;
        entry.pid
    };
    if let Some(pid) = pid {
        kill_process_group_with_grace(pid);
    }
}

// ==================== 단위 테스트 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::transcript::validate_session_id;

    // turn_id/session_id는 UUID v4 형식이어야 한다
    #[test]
    fn generate_uuid_v4_produces_uuid_v4_shape() {
        let id = generate_uuid_v4();
        assert!(validate_session_id(&id).is_ok(), "생성된 id는 표준 UUID 형태여야 합니다: {id}");
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

    // build_claude_args: 기존 재개 경로(resume=true)의 인자 구성은 절대
    // 바뀌면 안 된다(회귀 금지 — 설계 명시).
    #[test]
    fn build_claude_args_resume_true_matches_existing_resume_shape() {
        let args = build_claude_args("11111111-2222-4333-8444-555555555501", true);
        assert_eq!(
            args,
            vec![
                "-p",
                "--resume",
                "11111111-2222-4333-8444-555555555501",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--permission-prompts",
                "none",
            ]
        );
    }

    // build_claude_args: 신규 세션 경로(resume=false)는 --resume 대신
    // --session-id를 쓴다.
    #[test]
    fn build_claude_args_resume_false_uses_session_id_flag() {
        let args = build_claude_args("11111111-2222-4333-8444-555555555501", false);
        assert_eq!(
            args,
            vec![
                "-p",
                "--session-id",
                "11111111-2222-4333-8444-555555555501",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--permission-prompts",
                "none",
            ]
        );
        assert!(!args.contains(&"--resume".to_string()), "신규 세션 경로에는 --resume이 없어야 합니다");
    }
}
