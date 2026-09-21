// ---------------- claude CLI 로그인을 앱 안에서 시작 ----------------
// 지금까지는 인증 실패 시 터미널을 열어 사용자가 직접 `claude login`을 쳐야
// 했다(`session_chat::open_claude_login_terminal`, 이 파일이 대체하지 않고
// 나란히 둔다 — 이 모듈의 흐름이 실패하면 사용자가 막다른 길에 몰리지 않게
// 하는 폴백이다). 이 모듈은 같은 로그인 절차를 앱 화면 안에서 시작할 수
// 있게 한다 — `claude auth login`을 이 앱이 자식 프로세스로 spawn하고,
// 표준출력에서 로그인 URL을 뽑아 프론트에 실어 보낸다.
//
// ⚠️ 절대 지키는 경계 3가지(작업 지시):
// 1) 자격증명은 claude CLI가 자기 저장소에 직접 쓴다 — 이 모듈은 그 파일을
//    읽거나 쓰지 않는다. 완료 판정도 CLI 자신의 `claude auth status --json`
//    구조화 출력을 다시 물어봐서만 한다(아래 `check_claude_auth_status`) —
//    자식 프로세스의 종료코드를 완료 신호로 추측하지 않는다.
// 2) 과금 주체 고정 — `--console`(Anthropic Console = API 종량과금)은
//    **절대** 쓰지 않는다. `--claudeai`(Claude 구독)를 인자에 항상 명시
//    고정한다(CLI 기본값에 기대지 않는다 — 향후 기본값이 바뀌면 이 앱을
//    쓰는 조직이 조용히 API 종량 요금을 떠안게 된다).
// 3) 이 앱 자체 OAuth 구현이 아니다 — `claude auth login`을 그대로 위임
//    실행할 뿐이다.
//
// 비TTY 실측(격리 환경, `CLAUDE_CONFIG_DIR`를 스크래치 경로로 돌려 이 머신의
// 실제 로그인 세션은 건드리지 않았다): `stdin=/dev/null`로
// `claude auth login --claudeai`를 spawn해도 "Raw mode is not supported"로
// 죽지 않는다 — 대신 "Opening browser to sign in…" / "If the browser didn't
// open, visit: https://…" / "Paste code here if prompted >"를 출력한 채
// 정상 대기했다(실제 로그인은 완료하지 않고 프로세스만 SIGTERM으로 정리).
// 바이너리 문자열 조사(`strings`)로 "Sign-in timed out while waiting for you
// to continue"/"Sign-in timed out before the browser flow completed"를
// 확인했다 — `--claudeai` 경로는 수동 코드 붙여넣기가 아니라 서버 폴링
// 기반으로 자동 완료되고 자체 타임아웃도 갖는다는 근거다. "추천 로그인
// 방식을 이 머신에서 쓸 수 없음"(수동 입력 폴백이 필요한 경우)만 코드
// 붙여넣기를 요구하는데, 이 앱은 그 입력 창을 제공하지 않으므로 그 문구가
// 보이면 기다리지 않고 즉시 실패로 처리해 "터미널 열기" 폴백으로 안내한다
// (아래 `run_login`의 `fallback_unavailable` 분기).
//
// 미검증으로 남긴 것: 이미 로그인된 상태에서 이 커맨드를 또 실행하면 CLI가
// "계정을 바꾸시겠습니까" 류의 추가 확인을 요구할 가능성 — 실제 호출부(세션
// 채팅의 인증 실패 배너)는 애초에 인증이 깨졌을 때만 노출되므로 이 상태가
// 흔하지는 않지만, 완전히 배제하지는 않는다. 이 경로가 걸리면 앱 자체
// 타임아웃(`LOGIN_MAX_WAIT`)이 5분 뒤 프로세스를 정리하고 실패로 보고한다
// (영구 대기는 아니다).

use std::io::{BufRead, BufReader, Read};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::process_util::{kill_process_group_with_grace, SilentCommand};

/// `dev_tools.rs`/`autonomy.rs`/`mcp_manager.rs`/`session_chat/mod.rs`가 이미
/// 정한 관례(후보 상수를 공유하지 않고 각자 별도로 둔다, 회귀 위험 0)를
/// 그대로 따른다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

/// 앱 쪽 자체 상한. `google_oauth::wait_for_oauth_callback`의 5분과 동일한
/// 값을 그대로 따른다(이 저장소가 이미 브라우저 왕복 대기에 쓰는 관례) — CLI
/// 자신도 더 짧은 내부 타임아웃을 갖지만(위 모듈 주석의 실측 근거) 그 값은
/// CLI 버전에 따라 달라질 수 있으므로 앱도 독립적인 상한을 걸어 영구 대기를
/// 막는다.
const LOGIN_MAX_WAIT: Duration = Duration::from_secs(300);

/// 로그인 시도는 앱 전체에서 한 번에 하나만 — 세션 채팅 턴의
/// `ACTIVE_TURNS`(세션당 1개, 전역 최대 3개)와 달리 이 흐름은 특정 세션에
/// 묶이지 않는 전역 단일 자원(브라우저 로그인 창 하나)이라 슬롯도 하나면
/// 충분하다.
struct LoginState {
    pid: Option<u32>,
    canceled: bool,
}

static LOGIN_STATE: OnceLock<Mutex<Option<LoginState>>> = OnceLock::new();

fn login_state() -> &'static Mutex<Option<LoginState>> {
    LOGIN_STATE.get_or_init(|| Mutex::new(None))
}

fn register_login() -> Result<(), String> {
    let mut guard = login_state().lock().unwrap();
    if guard.is_some() {
        return Err("이미 로그인 절차가 진행 중입니다.".to_string());
    }
    *guard = Some(LoginState {
        pid: None,
        canceled: false,
    });
    Ok(())
}

/// pid를 등록하고, 그 사이 이미 취소 요청이 들어왔었는지 반환한다
/// (`session_chat::turn::set_turn_pid`와 동일한 패턴).
fn set_login_pid(pid: u32) -> bool {
    let mut guard = login_state().lock().unwrap();
    match guard.as_mut() {
        Some(state) => {
            state.pid = Some(pid);
            state.canceled
        }
        None => false,
    }
}

/// 슬롯을 비우고, 취소된 상태였는지 반환한다.
fn finish_login() -> bool {
    let mut guard = login_state().lock().unwrap();
    guard.take().map(|s| s.canceled).unwrap_or(false)
}

/// 앱 종료 시(`lib.rs`의 `ExitRequested`) 진행 중인 로그인 프로세스를
/// 정리한다 — `session_chat::request_shutdown`과 동일한 목적(고아 프로세스
/// 방지).
pub fn request_shutdown() {
    let pid = {
        let guard = login_state().lock().unwrap();
        guard.as_ref().and_then(|s| s.pid)
    };
    if let Some(pid) = pid {
        kill_process_group_with_grace(pid);
    }
}

/// `open_claude_login_terminal`(터미널 폴백)이 이미 쓰는 후보 경로 탐색을
/// 그대로 재사용한다.
fn resolve_claude() -> Result<String, String> {
    crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude").ok_or_else(|| {
        "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).".to_string()
    })
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(count - max_chars).collect()
}

// ==================== claude auth status --json ====================

#[derive(serde::Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAuthStatus {
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub email: Option<String>,
    pub org_name: Option<String>,
    pub subscription_type: Option<String>,
}

/// 실측 출력(이 머신, `claude auth status --json`): `{"loggedIn": true,
/// "authMethod": "claude.ai", "apiProvider": "firstParty", "email": "...",
/// "orgName": "...", "subscriptionType": "max", ...}`. 이 함수는 값이
/// 있으면 뽑아 쓰고, JSON이 아니거나 필드가 없으면(CLI 버전 차이·미로그인
/// 등) 조용히 기본값(`logged_in: false`)으로 떨어진다 — 종료코드를 신뢰
/// 신호로 쓰지 않는다는 원칙과 같은 이유로, 파싱 실패도 예외로 만들지
/// 않는다.
fn parse_claude_auth_status(stdout: &str) -> ClaudeAuthStatus {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return ClaudeAuthStatus::default();
    };
    ClaudeAuthStatus {
        logged_in: value.get("loggedIn").and_then(|v| v.as_bool()).unwrap_or(false),
        auth_method: value.get("authMethod").and_then(|v| v.as_str()).map(String::from),
        email: value.get("email").and_then(|v| v.as_str()).map(String::from),
        org_name: value.get("orgName").and_then(|v| v.as_str()).map(String::from),
        subscription_type: value
            .get("subscriptionType")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// 보너스 항목(작업 지시): 이 프로젝트가 지금까지 갖지 못했던 신뢰할 만한
/// 인증 상태 신호. 기존에는 `state.sessionChat.authError`(마지막 채팅 턴의
/// 결과)뿐이라 "아직 대화를 안 함"과 "인증됨"을 구분할 수 없었다 — 이
/// 커맨드는 그 갭을 메우는 조회 전용 진입점이다(대시보드 UI 반영은 이
/// 작업 범위 밖, 별도 작업으로 남긴다). `run_login`의 완료 판정도 이
/// 함수를 그대로 재사용한다.
#[tauri::command]
pub fn check_claude_auth_status() -> Result<ClaudeAuthStatus, String> {
    let claude_path = resolve_claude()?;
    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));
    let args = ["auth".to_string(), "status".to_string(), "--json".to_string()];
    let output = crate::dev_tools::run_process_with_timeout_cancellable(
        &claude_path,
        &args,
        &path_env,
        &[],
        Duration::from_secs(15),
        None,
        None,
        None,
    );
    if let Some(err) = output.spawn_error {
        return Err(err);
    }
    if output.timed_out {
        return Err("claude auth status 조회가 시간 초과되었습니다.".to_string());
    }
    Ok(parse_claude_auth_status(&output.stdout))
}

// ==================== 앱 안 로그인 시작 ====================

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuthLoginUrlPayload {
    url: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuthLoginFinishedPayload {
    ok: bool,
    canceled: bool,
    error: Option<String>,
    status: Option<ClaudeAuthStatus>,
}

fn emit_login_url(app: &tauri::AppHandle, url: &str) {
    use tauri::Emitter;
    let _ = app.emit("claude-auth-login-url", ClaudeAuthLoginUrlPayload { url: url.to_string() });
}

fn emit_login_finished(
    app: &tauri::AppHandle,
    ok: bool,
    canceled: bool,
    error: Option<String>,
    status: Option<ClaudeAuthStatus>,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "claude-auth-login-finished",
        ClaudeAuthLoginFinishedPayload {
            ok,
            canceled,
            error,
            status,
        },
    );
}

/// 세션 채팅의 인증 실패 배너 "앱에서 로그인" 버튼용(프론트: `sessions.ts`).
/// 즉시 반환하고(spawn만 확인), 실제 진행 상황은 `claude-auth-login-url`/
/// `claude-auth-login-finished` 이벤트로 스트리밍한다 — `session_chat`의
/// `send_session_message`/`run_turn` 관계와 동일한 구조다.
#[tauri::command]
pub fn start_claude_auth_login(app: tauri::AppHandle) -> Result<(), String> {
    register_login()?;

    let claude_path = match resolve_claude() {
        Ok(p) => p,
        Err(e) => {
            finish_login();
            return Err(e);
        }
    };
    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));

    std::thread::spawn(move || run_login(app, claude_path, path_env));
    Ok(())
}

fn run_login(app: tauri::AppHandle, claude_path: String, path_env: String) {
    let mut command = Command::new(&claude_path);
    // 과금 주체 고정(절대 경계 ②, 이 파일 상단 주석): `--console`(Anthropic
    // Console = API 종량과금)은 쓰지 않는다. `--claudeai`(Claude 구독)를
    // 여기서 명시 고정한다 — CLI 기본값과 지금은 같아도, 그 기본값에
    // 기대지 않는다.
    command.args(["auth", "login", "--claudeai"]);
    command.env("PATH", &path_env);
    // stdin=null: 위 모듈 주석의 비TTY 실측과 동일한 조건(`< /dev/null`).
    // 정상 경로(폴링 기반 자동완료)는 입력이 필요 없고, 수동 코드 붙여넣기
    // 폴백은 이 앱이 입력 창을 제공하지 않으므로 애초에 지원 범위 밖이다.
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.silent();
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            finish_login();
            emit_login_finished(
                &app,
                false,
                false,
                Some(format!("claude 명령을 실행하지 못했습니다: {e}")),
                None,
            );
            return;
        }
    };

    let pid = child.id();
    let already_canceled = set_login_pid(pid);
    if already_canceled {
        kill_process_group_with_grace(pid);
    }

    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    // 앱 쪽 상한(LOGIN_MAX_WAIT) 워치독 — 1초 틱으로 완료 플래그를 확인해
    // 정상 종료 시 즉시 스스로 빠진다. 5분을 한 번에 sleep하고 나서 pid를
    // 죽이면, 그 사이 프로세스가 이미 정상 종료·회수되고 OS가 같은 pid를
    // 재사용한 경우 엉뚱한 프로세스를 죽일 창이 넓어진다 — 대신
    // `google_oauth::wait_for_oauth_callback`과 같은 1초 tick 폴링 스타일로
    // 그 창을 좁힌다.
    let done = Arc::new(AtomicBool::new(false));
    let done_for_watchdog = done.clone();
    let watchdog_pid = pid;
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + LOGIN_MAX_WAIT;
        while std::time::Instant::now() < deadline {
            if done_for_watchdog.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        if !done_for_watchdog.load(Ordering::Relaxed) {
            kill_process_group_with_grace(watchdog_pid);
        }
    });

    let mut url_emitted = false;
    let mut fallback_unavailable = false;

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !url_emitted {
                if let Some(idx) = trimmed.find("https://") {
                    emit_login_url(&app, &trimmed[idx..]);
                    url_emitted = true;
                }
            }
            // 이 문구가 보이면 정상 폴링 경로가 아니라 수동 코드 붙여넣기
            // 폴백이 필요하다는 뜻이다 — 이 앱은 그 입력 창을 제공하지
            // 않으므로 타임아웃까지 기다리지 않고 즉시 실패로 처리한다.
            if trimmed.contains("recommended sign-in isn't available on this machine") {
                fallback_unavailable = true;
                kill_process_group_with_grace(pid);
                break;
            }
        }
    }

    let stderr_bytes = stderr_reader.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes).to_string();

    let _ = child.wait();
    done.store(true, Ordering::Relaxed);

    let canceled = finish_login();

    if canceled {
        emit_login_finished(&app, false, true, None, None);
        return;
    }

    if fallback_unavailable {
        emit_login_finished(
            &app,
            false,
            false,
            Some(
                "이 환경에서는 앱 안 로그인을 쓸 수 없습니다(추천 로그인 방식 사용 불가). 아래 \"터미널 열기\"로 진행해주세요."
                    .to_string(),
            ),
            None,
        );
        return;
    }

    // 종료코드를 완료 신호로 추측하지 않는다(이 파일 상단 경계 ①) — 대신
    // `claude auth status --json`을 다시 물어 실제 로그인 여부를 확인한다.
    match check_claude_auth_status() {
        Ok(status) if status.logged_in => {
            emit_login_finished(&app, true, false, None, Some(status));
        }
        Ok(status) => {
            let tail = tail_chars(&stderr_text, 2000);
            let message = if tail.is_empty() {
                "로그인이 완료되지 않았습니다. 시간이 초과되었거나 브라우저에서 로그인을 취소했을 수 있습니다.".to_string()
            } else {
                tail
            };
            emit_login_finished(&app, false, false, Some(message), Some(status));
        }
        Err(e) => {
            emit_login_finished(
                &app,
                false,
                false,
                Some(format!("로그인 결과를 확인하지 못했습니다: {e}")),
                None,
            );
        }
    }
}

/// 세션 채팅 화면의 "취소" 조작용. `turn_id`가 없어도(로그인 절차는 전역
/// 단일 슬롯이라) 실행 중인 로그인이 있으면 그것을 취소한다. 이미 끝난
/// 절차를 취소해도(슬롯이 비어 있음) 아무 일도 하지 않는다 — 정상 케이스.
#[tauri::command]
pub fn cancel_claude_auth_login() {
    let pid = {
        let mut guard = login_state().lock().unwrap();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.canceled = true;
        state.pid
    };
    if let Some(pid) = pid {
        kill_process_group_with_grace(pid);
    }
}

// ==================== 단위 테스트 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_auth_status_reads_real_shape() {
        // 실측 출력 그대로(이 머신, `claude auth status --json`) — 필드
        // 이름·타입이 실제 CLI와 어긋나면 이 테스트가 먼저 깨진다.
        let json = r#"{"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "email": "dev@malgnsoft.com", "orgName": "malgnsoft", "subscriptionType": "max"}"#;
        let status = parse_claude_auth_status(json);
        assert!(status.logged_in);
        assert_eq!(status.auth_method.as_deref(), Some("claude.ai"));
        assert_eq!(status.email.as_deref(), Some("dev@malgnsoft.com"));
        assert_eq!(status.org_name.as_deref(), Some("malgnsoft"));
        assert_eq!(status.subscription_type.as_deref(), Some("max"));
    }

    #[test]
    fn parse_claude_auth_status_not_logged_in() {
        let json = r#"{"loggedIn": false}"#;
        let status = parse_claude_auth_status(json);
        assert!(!status.logged_in);
        assert!(status.auth_method.is_none());
    }

    #[test]
    fn parse_claude_auth_status_falls_back_to_default_on_invalid_json() {
        let status = parse_claude_auth_status("not json at all");
        assert!(!status.logged_in);
    }

    #[test]
    fn tail_chars_returns_whole_text_when_under_limit() {
        assert_eq!(tail_chars("  hello  ", 100), "hello");
    }

    #[test]
    fn tail_chars_truncates_to_last_n_chars() {
        let text = "abcdefghij";
        assert_eq!(tail_chars(text, 3), "hij");
    }

    // ==================== 전역 단일 슬롯(register/set_pid/finish/cancel) ====================
    // `LOGIN_STATE`는 세션별 키가 없는 프로세스 전역 단일 슬롯이다(session_chat
    // ::turn의 `ACTIVE_TURNS`처럼 세션 id로 나눌 수 없다) — `cargo test`
    // 기본값(스레드 병렬 실행)에서 여러 테스트가 이 슬롯을 동시에 건드리면
    // 서로의 상태를 관찰해 거짓 실패/성공이 날 수 있다. 그래서 하나의 테스트
    // 함수 안에서 순차적으로만 검증한다(테스트 간 병렬 공유 상태 없음).
    #[test]
    fn login_slot_lifecycle_register_cancel_finish() {
        // 1) 첫 등록은 성공하고, 진행 중일 때 두 번째 등록은 거부된다.
        assert!(register_login().is_ok());
        let second = register_login();
        assert!(second.is_err());
        assert_eq!(second.unwrap_err(), "이미 로그인 절차가 진행 중입니다.");

        // 2) pid가 아직 없는 상태에서 취소하면, 이후 set_login_pid가 그
        //    취소를 그대로 보고해야 한다(run_login이 즉시 kill하도록).
        cancel_claude_auth_login();
        let already_canceled = set_login_pid(12345);
        assert!(already_canceled);
        let canceled = finish_login();
        assert!(canceled);

        // 3) 슬롯이 비었으니 다시 등록할 수 있고, 취소 없이 끝내면 false.
        assert!(register_login().is_ok());
        let canceled = finish_login();
        assert!(!canceled);
    }
}
