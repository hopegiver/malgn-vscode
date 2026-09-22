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
// ---------------- v0.2.11 사고와 근본 수정(이 리비전) ----------------
// v0.2.11은 자식 stdin을 `Stdio::null()`로 spawn했다. 실사용 환경에서 CLI는
// 브라우저에 인증 코드 페이지를 띄우고 코드 입력을 기다리는데, stdin이 막혀
// 있어 그 코드가 영영 들어갈 수 없었다 — 사용자는 멈춘 화면만 봤다. 곁들인
// 폴백 감지("recommended sign-in isn't available on this machine" 문구
// 매칭)도 다른 상황의 문구라 못 잡았다. 45명에게 배포된 뒤 `03b8c24`로
// 프론트 진입점만 끄고 롤백했다(hub 이슈 01m33qe0zhn55mczhcdgec2b61).
//
// 바이너리 문자열 재조사(claude 2.1.276)로 실제 코드를 확인했다:
// `process.stdout.write("Paste code here if prompted > ")`. 여기서 세
// 가지가 따라온다.
// 1) Ink TUI가 아니라 `process.stdout.write`다 → TTY가 없어도(파이프로도)
//    이 프롬프트는 우리 쪽 stdout에 반드시 도달한다.
// 2) 문구가 "if prompted"다 → 이 프롬프트는 정상 폴링 경로와 배타적인
//    "수동 케이스 전용" 분기가 아니라, "브라우저가 코드를 줬다면 여기
//    붙여넣어라"는 상시 통로로 보인다. 그래서 이 리비전은 "이 문구가
//    보이면 폴백"류의 감지 분기를 아예 없앴다 — 그 분기가 문구를 잘못
//    매칭해서 사고가 났었다. 대신 로그인이 진행 중인 동안 코드 입력창을
//    처음부터 상시 띄워 둔다(감지 실패로 사용자가 갇히는 경로 자체가
//    존재하지 않는다).
// 3) `"> "`로 끝나고 줄바꿈이 없다 → **stdout을 줄 단위(`BufReader::lines()`)로
//    읽으면 이 프롬프트는 다음 개행이 올 때까지 버퍼에 갇혀 영원히 안
//    나온다.** 지난 사고를 그대로 재현하는 지점이라, 이 리비전은 stdout을
//    바이트 단위로 읽는다(아래 `pump_stdout`).
//
// 구현: 자식 stdin을 `Stdio::piped()`로 바꾸고(`run_login`), 그 쓰기 끝을
// 로그인 전역 슬롯(`LOGIN_STATE`)에 보관한다. 새 커맨드
// `submit_claude_auth_login_code`가 그 핸들에 사용자가 제출한 문자열 + LF를
// 써넣는다 — claude가 실제로 코드를 요구하든 안 하든 이 경로는 항상 존재하고,
// 요구하지 않으면 그냥 아무도 안 쓸 뿐이다. 취소·타임아웃 시에는 그 핸들을
// 명시적으로 drop해 쓰기 끝을 닫는다(피워진 stdin을 아무도 안 닫으면 자식이
// EOF를 못 받아 또 다른 얼굴의 무한 대기에 빠질 수 있다 — `finish_login()`의
// 정상 종료 경로도 슬롯 전체를 take()하며 같은 효과를 낸다).
//
// 정상 폴링 경로(자동 완료·완료 판정·앱 자체 타임아웃)는 이 리비전에서
// 손대지 않았다 — 실측대로 `--claudeai`는 서버 폴링 기반으로 자동 완료되고
// 자체 타임아웃도 갖는다(바이너리 문자열 "Sign-in timed out..." 확인됨).
//
// 미검증으로 남긴 것: 실제 claude로 로그인 완료까지 가는 end-to-end(이
// 머신은 이미 로그인돼 있어 깨뜨리면 안 된다 — `docs`/작업 지시 참고).
// 검증은 ①격리 `CLAUDE_CONFIG_DIR`에서의 정상 폴링 경로(기존에 이미 확인)
// ②가짜 자식 프로세스로 이 파일 하단 단위 테스트가 확인하는 "개행 없는
// 프롬프트 감지 + stdin 파이프로 실제 전달"로 나뉜다.

use std::io::{Read, Write};
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
    /// 자식 stdin의 쓰기 끝 — `submit_claude_auth_login_code`가 여기 쓴다.
    /// `Option`을 `None`으로 바꾸면(`.take()`) 그 즉시 drop되어 파이프가
    /// 닫힌다(취소·타임아웃·정상 종료 공통 경로).
    stdin: Option<std::process::ChildStdin>,
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
        stdin: None,
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

/// 자식 stdin의 쓰기 끝을 슬롯에 보관한다 — 슬롯이 이미 비어 있으면(등록 전에
/// 취소·종료가 끝난 레이스) 인자로 받은 값이 즉시 스코프를 벗어나며
/// drop되어 그 자체로 닫힌다. 별도 분기가 필요 없다.
fn set_login_stdin(stdin: std::process::ChildStdin) {
    let mut guard = login_state().lock().unwrap();
    if let Some(state) = guard.as_mut() {
        state.stdin = Some(stdin);
    }
}

/// 취소·타임아웃 시 명시적으로 호출해 stdin 쓰기 끝을 닫는다(작업 지시).
/// 이미 `kill_process_group_with_grace`로 시그널을 보내지만, 그것과 별개로
/// "아무도 stdin을 닫지 않아 자식이 EOF를 못 받는" 경로 자체를 없앤다.
fn close_login_stdin() {
    let mut guard = login_state().lock().unwrap();
    if let Some(state) = guard.as_mut() {
        drop(state.stdin.take());
    }
}

/// 슬롯을 비우고, 취소된 상태였는지 반환한다. `guard.take()`가 통째로
/// `LoginState`를 반환·drop하므로 남아있던 stdin도 이 시점에 자동으로
/// 닫힌다(정상 종료 경로의 stdin 정리).
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
    // 특정 프로젝트와 무관한 전역 호출이라 `global_cwd::default_global_cwd()`
    // (workspace 루트 → 홈 디렉터리 순, 존재 확인 후 첫 값)로 CWD를 명시적으로
    // 고정한다(`plugins::run_claude_command`와 동일한 근거 — 작업 지시서
    // "인증 상태 확인"이 바로 이 호출). `None`이면(후보가 모두 없음)
    // `run_process_with_timeout_cancellable`이 대신 Windows 전용
    // `child_current_dir` 보정만 적용한다(Mac은 그마저도 없이 기존 동작 유지).
    let cwd = crate::global_cwd::default_global_cwd();
    let output = crate::dev_tools::run_process_with_timeout_cancellable(
        &claude_path,
        &args,
        &path_env,
        &[],
        Duration::from_secs(15),
        None,
        None,
        cwd.as_deref(),
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

/// 자식 stdout 원문 청크 그대로 프론트에 스트리밍한다(줄 경계 없음 —
/// `pump_stdout`이 읽는 그대로). 사용자가 코드 입력창 옆에서 "claude가 지금
/// 뭘 묻고 있는지"를 원문으로 볼 수 있게 하는 용도이지, 이 값을 보고 입력창
/// 노출 여부를 분기하지 않는다(입력창은 로그인이 진행 중인 동안 항상 있다).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuthLoginOutputPayload {
    text: String,
}

fn emit_login_url(app: &tauri::AppHandle, url: &str) {
    use tauri::Emitter;
    let _ = app.emit("claude-auth-login-url", ClaudeAuthLoginUrlPayload { url: url.to_string() });
}

fn emit_login_output(app: &tauri::AppHandle, text: &str) {
    use tauri::Emitter;
    let _ = app.emit("claude-auth-login-output", ClaudeAuthLoginOutputPayload { text: text.to_string() });
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

/// 자식 stdout을 바이트 단위로 읽으며 청크가 도착할 때마다 `on_chunk`를
/// 부른다. EOF(자식이 stdout을 닫음, 보통 프로세스 종료)까지 블로킹으로
/// 읽고 반환한다.
///
/// `BufReader::lines()`(줄 단위)를 쓰지 않는 이유(이 파일 상단 주석 ③):
/// claude CLI가 찍는 `"Paste code here if prompted > "` 프롬프트는 개행이
/// 없다 — 줄 단위 리더는 다음 개행이 올 때까지 그 내용을 버퍼에 가둔다.
/// 멀티바이트 UTF-8 문자가 청크 경계에서 잘리면 `from_utf8_lossy`가 그
/// 부분을 교체 문자로 바꾸는데, 이 프로세스가 찍는 텍스트(URL·영어 안내문)는
/// 전부 ASCII라 실사용에서 발생하지 않는다.
fn pump_stdout(mut stdout: impl Read, mut on_chunk: impl FnMut(&str)) {
    let mut buf = [0u8; 256];
    loop {
        match stdout.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => on_chunk(&String::from_utf8_lossy(&buf[..n])),
        }
    }
}

fn run_login(app: tauri::AppHandle, claude_path: String, path_env: String) {
    let mut command = Command::new(&claude_path);
    // 과금 주체 고정(절대 경계 ②, 이 파일 상단 주석): `--console`(Anthropic
    // Console = API 종량과금)은 쓰지 않는다. `--claudeai`(Claude 구독)를
    // 여기서 명시 고정한다 — CLI 기본값과 지금은 같아도, 그 기본값에
    // 기대지 않는다.
    command.args(["auth", "login", "--claudeai"]);
    command.env("PATH", &path_env);
    // check_claude_auth_status와 동일한 근거(전역 호출) — CWD를 명시적으로
    // 고정한다. 후보가 모두 없으면 `.current_dir()`을 호출하지 않고 기존 동작
    // (앱 프로세스의 CWD 상속)을 유지한다.
    if let Some(dir) = crate::global_cwd::default_global_cwd() {
        command.current_dir(dir);
    }
    // stdin=piped(근본 수정, 이 파일 상단 주석): claude가 브라우저에서 받은
    // 코드를 요구할 수 있는 통로를 항상 열어둔다. 정상 폴링 경로는 이
    // 입력을 안 읽을 뿐이라 있어도 해가 없다.
    command.stdin(Stdio::piped());
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

    // stdin 쓰기 끝을 전역 슬롯에 보관해 `submit_claude_auth_login_code`가
    // 쓸 수 있게 한다. 이미 취소된 레이스라면(위 already_canceled) 굳이
    // 보관하지 않고 바로 drop해 닫는다 — 어차피 프로세스를 죽이는 중이다.
    match child.stdin.take() {
        Some(stdin) if !already_canceled => set_login_stdin(stdin),
        Some(stdin) => drop(stdin),
        None => {}
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
            // 취소와 동일한 이유로 kill 전에 stdin을 먼저 닫는다(작업 지시) —
            // 시그널과 무관하게 "쓰기 끝이 열린 채 아무도 안 닫는" 경로를
            // 없앤다.
            close_login_stdin();
            kill_process_group_with_grace(watchdog_pid);
        }
    });

    // 분기 없이 바이트 단위로 읽는다(이 파일 상단 주석 ③) — URL은 누적
    // 버퍼에서 찾고, 원문 청크는 그대로 프론트에 스트리밍해 사용자가 claude가
    // 지금 무엇을 묻는지 볼 수 있게 한다. "특정 문구가 보이면 폴백" 같은
    // 분기는 여기 없다 — 코드 입력창은 로그인이 진행 중인 동안 항상 있다
    // (views/sessions.ts).
    let mut url_emitted = false;
    let mut accumulated = String::new();

    if let Some(stdout) = child.stdout.take() {
        pump_stdout(stdout, |chunk| {
            emit_login_output(&app, chunk);
            if url_emitted {
                return;
            }
            accumulated.push_str(chunk);
            let Some(idx) = accumulated.find("https://") else {
                return;
            };
            let rest = &accumulated[idx..];
            // URL이 아직 다 도착하지 않았을 수 있다(청크 경계) — 공백/개행으로
            // 끝맺는 걸 찾을 때까지 다음 청크를 기다린다.
            if let Some(end) = rest.find(char::is_whitespace) {
                emit_login_url(&app, &rest[..end]);
                url_emitted = true;
            }
        });
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
        // 취소 시 stdin을 반드시 닫는다(작업 지시) — piped stdin을 아무도
        // 안 닫으면 자식이 EOF를 못 받아 또 다른 얼굴의 무한 대기에 빠질 수
        // 있다. `.take()`가 반환한 값을 곧장 drop해 쓰기 끝을 닫는다.
        drop(state.stdin.take());
        state.pid
    };
    if let Some(pid) = pid {
        kill_process_group_with_grace(pid);
    }
}

/// 세션 채팅 화면의 코드 입력창 "코드 제출" 버튼용. 로그인이 진행 중인 동안
/// 항상 호출 가능하다(claude가 실제로 코드를 요구하는지 이 앱은 판단하지
/// 않는다 — 이 파일 상단 주석의 "분기를 없앤다" 설계). 제출값 끝에 LF 한
/// 글자만 붙인다 — 파이프(비TTY) stdin을 읽는 Node 계열 입력은 LF만으로 한
/// 줄 입력이 완결되고, CRLF를 붙이면 오히려 CR이 코드 문자열에 섞여 들어갈
/// 위험이 있다.
#[tauri::command]
pub fn submit_claude_auth_login_code(code: String) -> Result<(), String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Err("코드를 입력해주세요.".to_string());
    }
    let mut guard = login_state().lock().unwrap();
    let Some(state) = guard.as_mut() else {
        return Err("진행 중인 로그인이 없습니다.".to_string());
    };
    let Some(stdin) = state.stdin.as_mut() else {
        return Err("로그인 프로세스가 아직 입력을 받을 준비가 되지 않았습니다. 잠시 후 다시 시도해주세요.".to_string());
    };
    let mut payload = trimmed.as_bytes().to_vec();
    payload.push(b'\n');
    stdin
        .write_all(&payload)
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("코드를 전달하지 못했습니다: {e}"))
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

    // `pump_stdout` 자체를 LOGIN_STATE 없이 순수하게 검증한다(병렬 실행
    // 안전 — 전역 상태를 건드리지 않는다). `Cursor`는 개행 유무와 무관하게
    // 읽은 바이트를 그대로 반환하므로, 마지막 청크가 개행으로 끝나지 않아도
    // (v0.2.11이 놓쳤던 "Paste code here if prompted > " 조건) on_chunk가
    // 그 내용을 받는다는 것을 이 테스트가 직접 고정한다.
    #[test]
    fn pump_stdout_forwards_chunk_even_without_trailing_newline() {
        let source = std::io::Cursor::new(b"Opening browser...\nPaste code here if prompted > ".to_vec());
        let mut received = String::new();
        pump_stdout(source, |chunk| received.push_str(chunk));
        assert_eq!(received, "Opening browser...\nPaste code here if prompted > ");
        assert!(
            received.ends_with("if prompted > "),
            "개행 없이 끝나는 마지막 텍스트가 on_chunk에 도달해야 합니다(실제: {received:?})"
        );
    }

    // ==================== 전역 단일 슬롯(register/set_pid/finish/cancel/stdin) ====================
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

        // 4) 로그인이 없을 때 코드를 제출하면 명확한 에러를 반환한다(패닉 아님).
        let no_login_err = submit_claude_auth_login_code("ABC".to_string());
        assert!(no_login_err.is_err());

        // 5) 로그인은 등록됐지만 아직 stdin이 준비되지 않았을 때(spawn 직후
        //    ~ set_login_stdin 호출 사이의 창)도 마찬가지로 명확한 에러여야
        //    한다 — run_login이 아직 자식을 spawn하지 못한 상태를 흉내낸다.
        assert!(register_login().is_ok());
        let not_ready_err = submit_claude_auth_login_code("ABC".to_string());
        assert!(not_ready_err.is_err());
        // 슬롯을 비워 다음 단계(6)가 이어서 register_login()할 수 있게 한다.
        let _ = finish_login();

        // 6) 가짜 자식 프로세스 — stdin 배관 end-to-end. v0.2.11 사고 재현
        //    조건 그대로: 개행 없는 프롬프트("Paste code here if prompted >
        //    ")를 찍고 자기 stdin을 한 줄 읽어 파일에 기록하는 가짜 "claude"를
        //    실제로 spawn한다. `register_login`/`set_login_pid`/
        //    `set_login_stdin`/`submit_claude_auth_login_code` — run_login이
        //    쓰는 것과 완전히 같은 함수들을 그대로 거친다. "감지"는
        //    `pump_stdout`이 개행 없이도 청크를 넘겨주는 것으로, "전달"은
        //    파일에 그 문자열이 실제로 나타나는 것으로 각각 증명한다(둘 다
        //    "에러가 안 난다"가 아니라 "있다"로 판정한다 — 이 작업의 완료
        //    기준). 위 1~5단계와 같은 LOGIN_STATE 슬롯을 계속 쓰므로 같은
        //    테스트 함수 안에 순차적으로 둔다(병렬 테스트 간 공유 상태 없음
        //    원칙 — 이 파일을 처음 분리해 별도 테스트 함수로 뒀더니
        //    `login_slot_lifecycle_*`와 경합해 register_login()이 실제로
        //    실패했다).
        #[cfg(unix)]
        {
            assert!(register_login().is_ok());

            let out_file = std::env::temp_dir().join(format!("claude_auth_fake_child_{}.txt", std::process::id()));
            let _ = std::fs::remove_file(&out_file);
            // printf(개행 없음)로 실측 프롬프트를 그대로 찍고, 자기 stdin을
            // 한 줄 읽어 OUT_FILE에 그대로 기록한다. 개행을 붙이지 않아
            // (마지막 printf도 %s) v0.2.11 사고 조건(줄바꿈 없는 stdout)을
            // 정확히 재현한다.
            let script = format!(
                "printf 'Paste code here if prompted > '; IFS= read -r line; printf '%s' \"$line\" > '{}'",
                out_file.display()
            );

            let mut child = Command::new("/bin/sh")
                .arg("-c")
                .arg(&script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("가짜 자식 프로세스(/bin/sh) spawn 실패");

            let pid = child.id();
            set_login_pid(pid);
            set_login_stdin(child.stdin.take().expect("가짜 자식의 stdin 파이프가 없음"));

            let stdout = child.stdout.take().expect("가짜 자식의 stdout 파이프가 없음");
            let chunks: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let chunks_for_reader = chunks.clone();
            let reader = std::thread::spawn(move || {
                pump_stdout(stdout, |chunk| chunks_for_reader.lock().unwrap().push(chunk.to_string()));
            });

            // 개행 없는 프롬프트가 도착할 때까지 폴링(최대 2초) — 여기서
            // 실패하면 줄 단위 리더(BufReader::lines())로 되돌아간 회귀다.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut prompt_seen = false;
            while std::time::Instant::now() < deadline {
                if chunks.lock().unwrap().join("").contains("Paste code here if prompted > ") {
                    prompt_seen = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(prompt_seen, "개행 없는 프롬프트를 감지하지 못했습니다(바이트 단위 읽기 회귀)");

            // 감지 후 실제 프론트 "코드 제출" 버튼이 부르는 것과 동일한
            // 커맨드로 코드를 전달한다.
            submit_claude_auth_login_code("HARNESS-CODE-123".to_string()).expect("코드 제출 실패");

            reader.join().expect("stdout 리더 스레드 조인 실패");
            child.wait().expect("가짜 자식 프로세스 회수 실패");
            finish_login();

            let written = std::fs::read_to_string(&out_file).expect("가짜 자식이 출력 파일을 쓰지 않음");
            let _ = std::fs::remove_file(&out_file);
            assert_eq!(
                written, "HARNESS-CODE-123",
                "submit_claude_auth_login_code로 보낸 코드가 가짜 자식이 기록한 파일에 그대로 나타나야 합니다"
            );
        }
    }
}
