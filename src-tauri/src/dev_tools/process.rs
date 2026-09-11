// ==================== 5. 버전 문자열 정규화(결정 4) ====================

use super::DevTool;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// "숫자+(.숫자+)+" 뒤에 선택적 `[-+.][영숫자.]+` 접미부를 붙여 첫 번째로 나타나는
/// 버전 토큰만 뽑는다. regex 크레이트 없이 손으로 구현(문자 클래스가 단순해
/// 의존성 추가보다 싸다는 판단은 argv 검증과 동일).
fn extract_version_token(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            let mut group_count = 1;
            let mut k = j;
            loop {
                if k < n && chars[k] == '.' {
                    let mut m = k + 1;
                    let digit_start = m;
                    while m < n && chars[m].is_ascii_digit() {
                        m += 1;
                    }
                    if m == digit_start {
                        break;
                    }
                    k = m;
                    group_count += 1;
                } else {
                    break;
                }
            }
            if group_count >= 2 {
                let mut end = k;
                if end < n && matches!(chars[end], '-' | '+' | '.') {
                    let mut m = end + 1;
                    let suffix_start = m;
                    while m < n && (chars[m].is_ascii_alphanumeric() || chars[m] == '.') {
                        m += 1;
                    }
                    if m > suffix_start {
                        end = m;
                    }
                }
                return Some(chars[start..end].iter().collect());
            }
        }
        i += 1;
    }
    None
}

/// 첫 줄만 취하고, 그 안에서 첫 버전 토큰을 뽑는다. 매치가 없으면 첫 줄 원문으로
/// 폴백(그래도 비어 있으면 None → 호출부가 UnknownAfter로 처리).
pub(crate) fn normalize_version(raw: &str) -> Option<String> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    extract_version_token(first_line).or_else(|| Some(first_line.to_string()))
}

// ==================== 6. 실행 엔진(결정 6) ====================

pub(crate) static EXECUTION_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct ProcessRunOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
    /// 앱 종료(`should_abort`)로 인해 중단됐는지 — `timed_out`과 별개 플래그라
    /// 로그에서 "타임아웃"과 "앱 종료로 중단"을 구분할 수 있다(자율업무
    /// 설계 §4). 기존 `run_process_with_timeout` 경로(`should_abort: None`)는
    /// 항상 `false`다.
    pub(crate) aborted: bool,
    pub(crate) duration: Duration,
    pub(crate) spawn_error: Option<String>,
}

enum WaitOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
    Aborted,
}

/// `should_abort`가 `Some`이고 매 100ms 폴링마다 `true`를 반환하면 즉시
/// `Aborted`로 끝낸다 — 자율업무 워커가 앱 종료 시 진행 중인 `claude -p`를
/// 타임아웃과 동일한 kill 경로로 정리하기 위한 훅이다. `should_abort`가
/// `None`이면(기존 `run_process_with_timeout` 경로) 이 확인을 건너뛰어
/// 기존 동작을 한 글자도 바꾸지 않는다.
fn wait_up_to_cancellable(
    child: &mut std::process::Child,
    timeout: Duration,
    started: Instant,
    should_abort: Option<&dyn Fn() -> bool>,
) -> WaitOutcome {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status),
            Ok(None) => {
                if let Some(check) = should_abort {
                    if check() {
                        return WaitOutcome::Aborted;
                    }
                }
                if started.elapsed() >= timeout {
                    return WaitOutcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // 기존 `wait_up_to`와 동일하게 취급: try_wait 자체가 에러면 즉시
            // kill 경로로 넘긴다(재시도하지 않는다).
            Err(_) => return WaitOutcome::TimedOut,
        }
    }
}

/// 프로세스 그룹 kill: SIGTERM → 3초 유예(try_wait 폴링) → SIGKILL. brew가 낳는
/// curl/git/ruby 손자 프로세스까지 함께 죽인다(`child.kill()`은 직속 자식만 죽여
/// 손자가 다운로드를 계속하는 문제가 있다 — 그래서 group kill이 필수).
#[cfg(unix)]
fn force_kill_process_group(pid: i32, child: &mut std::process::Child) -> Option<i32> {
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let grace_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => {
                if Instant::now() >= grace_deadline {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
}

/// Windows에는 POSIX 프로세스 그룹이 없다. 이 함수가 다루는 러너(Npm/Pnpm/
/// SelfBinary)는 현재 macOS 전용 게이트(install_resolver 모듈 참고)로 Windows에서
/// 실행에 도달하지 않지만, 그 게이트를 걷어내 Windows 지원을 열 때를 대비해 정의는
/// 유지한다 — 그 시점에도 손자 프로세스를 남기는 경우가 드물어, 직속
/// 자식만 종료(`Child::kill` → `TerminateProcess`)하는 것으로 MVP 범위에서는
/// 충분하다고 판단했다(50인 미만 내부 도구, Job Object 등 고급 처리는 과설계).
/// 손자 프로세스는 정리되지 않을 수 있다는 제약을 감수한다. pid 인자는 unix
/// 버전과 시그니처를 맞추기 위해서만 존재하며 사용하지 않는다.
#[cfg(windows)]
fn force_kill_process_group(_pid: i32, child: &mut std::process::Child) -> Option<i32> {
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// stdin=null(부록 B.1 — 프롬프트가 즉시 EOF를 받아 정지 대신 실패한다) +
/// stdout/stderr 리더 스레드(파이프 64KB 버퍼가 차서 자식이 write에서 멈추는
/// 교착을 막는다) + try_wait() 100ms 폴링 타임아웃 + 프로세스 그룹 kill.
///
/// `run_process_with_timeout_cancellable`의 얇은 래퍼다(`on_spawn`/
/// `should_abort` 없이 호출) — 기존 4개 호출부와 3개 테스트는 이 시그니처가
/// 그대로 유지되므로 한 글자도 바뀌지 않는다(자율업무 설계 §4).
pub(crate) fn run_process_with_timeout(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
) -> ProcessRunOutput {
    run_process_with_timeout_cancellable(binary_path, args, path_env, extra_env, timeout, None, None)
}

/// `on_spawn`: spawn 직후 자식 pid를 공표한다(자율업무 런타임 레지스트리
/// 등록용). `should_abort`: `wait_up_to_cancellable`의 100ms 폴링마다 확인해
/// `true`면 타임아웃과 동일한 경로(`force_kill_process_group`)로 kill한다.
/// 자율업무는 이 함수를 통해 앱 종료 시 진행 중인 `claude -p`를 정리한다.
///
/// 주의: 이 함수는 `EXECUTION_LOCK`을 취하지 않는다(그 락은 도구 설치를
/// 직렬화하는 락이라 자율업무가 취하면 전역 동시성이 1이 되어 concurrency
/// 설계가 무너진다) — 호출자가 직접 그 락을 잡지 않도록 주의한다.
pub(crate) fn run_process_with_timeout_cancellable(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
    on_spawn: Option<&dyn Fn(u32)>,
    should_abort: Option<&dyn Fn() -> bool>,
) -> ProcessRunOutput {
    let started = Instant::now();
    let mut command = Command::new(binary_path);
    command.args(args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("PATH", path_env);
    for (k, v) in extra_env {
        command.env(k, v);
    }
    // HOME은 상속(brew/npm 캐시·설정이 필요) — PATH만 명시적으로 덮어쓴다.
    // 새 프로세스 그룹으로 스폰(force_kill_process_group의 그룹 kill이 작동하려면
    // 필요) — Windows에는 이 개념 자체가 없어 이 호출도 없다(CommandExt는 unix 전용).
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ProcessRunOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                aborted: false,
                duration: started.elapsed(),
                spawn_error: Some(format!("실행할 수 없습니다: {e}")),
            };
        }
    };

    let pid = child.id() as i32;
    if let Some(cb) = on_spawn {
        cb(pid as u32);
    }
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_reader = stdout_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_reader = stderr_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let (exit_code, timed_out, aborted) =
        match wait_up_to_cancellable(&mut child, timeout, started, should_abort) {
            WaitOutcome::Exited(status) => (status.code(), false, false),
            WaitOutcome::TimedOut => (force_kill_process_group(pid, &mut child), true, false),
            WaitOutcome::Aborted => (force_kill_process_group(pid, &mut child), false, true),
        };

    let stdout_bytes = stdout_reader
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr_bytes = stderr_reader
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    ProcessRunOutput {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
        timed_out,
        aborted,
        duration: started.elapsed(),
        spawn_error: None,
    }
}

/// `<brew_prefix>/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` — 절대경로
/// 실행만으로는 부족하다(brew/npm 내부에서 git/curl/ruby/node를 셔뱅으로 부른다).
/// runner_path의 bin 디렉터리를 최우선으로 넣고 표준 경로를 뒤에 덧붙인다.
pub(crate) fn build_child_path_env(runner_path: Option<&str>) -> String {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(rp) = runner_path {
        if let Some(bin_dir) = Path::new(rp).parent() {
            let s = bin_dir.to_string_lossy().to_string();
            // 빈 문자열은 POSIX PATH에서 CWD를 의미한다(예: bare name "brew"의
            // parent()는 Some("")) — 자식 프로세스가 CWD에서 git/curl 등을
            // 먼저 찾게 되므로 반드시 배제한다.
            if !s.is_empty() {
                dirs.push(s);
            }
        }
    }
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let s = d.to_string();
        if !dirs.contains(&s) {
            dirs.push(s);
        }
    }
    dirs.join(":")
}

pub(crate) fn check_tool_version(def: &DevTool, resolved_path: &str) -> Option<String> {
    let path_env = build_child_path_env(Some(resolved_path));
    let args: Vec<String> = def.version_args.iter().map(|s| s.to_string()).collect();
    let output = run_process_with_timeout(
        resolved_path,
        &args,
        &path_env,
        &[],
        Duration::from_secs(10),
    );
    if output.spawn_error.is_some() {
        return None;
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return Some(stdout.to_string());
    }
    let stderr = output.stderr.trim();
    if output.exit_code == Some(0) && !stderr.is_empty() {
        Some(stderr.to_string())
    } else {
        None
    }
}

pub(crate) fn truncate_log(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stdout}\n{stderr}");
    const MAX_CHARS: usize = 4000;
    let count = combined.chars().count();
    if count <= MAX_CHARS {
        combined
    } else {
        combined.chars().skip(count - MAX_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 완료판정 필수 테스트 ④: 버전 정규화 ──
    #[test]
    fn normalizes_real_measured_version_strings() {
        assert_eq!(
            normalize_version(
                "gh version 2.95.0 (2026-06-17)\nhttps://github.com/cli/cli/releases/tag/v2.95.0"
            ),
            Some("2.95.0".to_string())
        );
        assert_eq!(
            normalize_version("git version 2.50.1 (Apple Git-155)"),
            Some("2.50.1".to_string())
        );
        assert_eq!(
            normalize_version("2.1.252 (Claude Code)"),
            Some("2.1.252".to_string())
        );
        assert_eq!(normalize_version("v22.23.1"), Some("22.23.1".to_string()));
        assert_eq!(normalize_version("11.9.0"), Some("11.9.0".to_string()));
        assert_eq!(normalize_version(""), None);
        // 매치가 없으면 첫 줄 원문 폴백
        assert_eq!(
            normalize_version("no-version-here\nsecond line"),
            Some("no-version-here".to_string())
        );
    }

    // 결정 6: 짧게 끝나는 프로세스가 정상적으로 exit code/stdout을 돌려주는지
    // (파이프 리더 스레드 + try_wait 폴링 경로 자체의 배관 검증).
    #[test]
    fn run_process_with_timeout_captures_output_of_fast_process() {
        let output = run_process_with_timeout(
            "/bin/echo",
            &["hello".to_string()],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(5),
        );
        assert!(output.spawn_error.is_none());
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.timed_out);
    }

    // 결정 6: 타임아웃이 실제로 프로세스를 강제 종료하는지(짧은 타임아웃으로
    // `sleep`을 강제 종료 — 프로세스 그룹 kill 경로까지 실행됨).
    #[test]
    fn run_process_with_timeout_kills_hanging_process() {
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/sleep",
            &["30".to_string()],
            "/usr/bin:/bin",
            &[],
            Duration::from_millis(500),
        );
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "타임아웃+3초 유예를 크게 넘기면 강제종료가 동작하지 않은 것"
        );
    }

    #[test]
    fn stdin_is_null_so_a_reading_process_fails_fast_instead_of_hanging() {
        // `cat`은 stdin을 읽으려 하지만 stdin이 null이라 즉시 EOF를 받아 빠르게
        // 종료해야 한다(부록 B.1 — 정지 대신 실패).
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/cat",
            &[],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(5),
        );
        assert!(
            !output.timed_out,
            "stdin=null이 적용되지 않았다면 cat이 멈춰 타임아웃까지 갔을 것"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn build_child_path_env_prioritizes_runner_bin_dir() {
        let path = build_child_path_env(Some("/opt/homebrew/bin/brew"));
        assert!(path.starts_with("/opt/homebrew/bin:"));
        assert!(path.contains("/usr/bin"));
        assert!(path.contains("/bin"));
    }

    // M1 회귀 테스트: resolve_binary()가 절대경로 후보를 못 찾으면 bare name을
    // 그대로 반환하고, 그 값이 runner_path로 들어온다. Path::new("brew").parent()는
    // Some("")를 반환하므로 빈 항목이 PATH 앞머리에 들어가면 안 된다 —
    // POSIX에서 PATH의 길이 0 항목은 CWD를 의미한다.
    #[test]
    fn build_child_path_env_excludes_empty_entry_for_bare_name() {
        let path = build_child_path_env(Some("brew"));
        assert!(!path.split(':').any(|p| p.is_empty()));
    }

    #[test]
    fn build_child_path_env_excludes_empty_entry_for_none() {
        let path = build_child_path_env(None);
        assert!(!path.split(':').any(|p| p.is_empty()));
    }
}
