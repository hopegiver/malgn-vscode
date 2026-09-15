// ==================== 5. 버전 문자열 정규화(결정 4) ====================

use super::DevTool;
use crate::process_util::SilentCommand;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
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

/// Windows에는 POSIX 프로세스 그룹이 없다 — 직속 자식만 종료한다(`Child::kill`
/// → `TerminateProcess`). Npm/Pnpm/SelfBinary 러너는 손자 프로세스를 남기는
/// 경우가 드물어 이것으로 충분하다.
///
/// Winget 러너는 다르다(N4, 2라운드): winget이 UAC로 승격해서 띄우는 msiexec
/// 등 손자 프로세스는 우리 프로세스보다 높은 무결성 수준(elevated token)으로
/// 실행된다 — 그 PID를 알아내 `kill`을 시도해도 권한 부족으로 ACCESS_DENIED가
/// 난다. "드물게 남는다"가 아니라 "구조적으로 죽일 수 없다"이다. 타임아웃
/// 시 winget.exe 자신(직속 자식)은 종료되지만, 이미 승격되어 독립한 설치
/// 프로세스는 백그라운드에서 계속 진행될 수 있다(`actions.rs`의
/// `timed_out_message`가 이 사실을 사용자에게 알린다). Job Object 도입은
/// 범위 밖(설계 §9 미해결쟁점 1). pid 인자는 unix 버전과 시그니처를 맞추기
/// 위해서만 존재하며 사용하지 않는다.
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
    // N1(2라운드 비차단): Windows에서만 현재 디렉터리를 `%SystemRoot%`로
    // 고정한다(`super::platform::child_current_dir` 참고 — `.cmd` shim은
    // `cmd.exe`가 해석하는데 그 명령 해석이 현재 디렉터리를 먼저 본다). Mac은
    // `child_current_dir`가 항상 `None`이라 이 블록이 아무 것도 하지 않는다
    // (기존 동작 무변경 — 이 머신은 항상 Mac이라 실행 시 검증됨).
    if let Some(dir) = super::platform::child_current_dir(
        super::platform::platform_now(),
        &super::platform::EnvRoots::from_env(),
    ) {
        command.current_dir(dir);
    }
    // HOME은 상속(brew/npm 캐시·설정이 필요) — PATH만 명시적으로 덮어쓴다.
    // 새 프로세스 그룹으로 스폰(force_kill_process_group의 그룹 kill이 작동하려면
    // 필요) — Windows에는 이 개념 자체가 없어 이 호출도 없다(CommandExt는 unix 전용).
    #[cfg(unix)]
    command.process_group(0);
    // 버전 조회·도구 설치 등 전부 백그라운드 실행이라 Windows에서 콘솔 창이
    // 뜨면 안 된다(non-Windows에서는 no-op).
    command.silent();

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

/// `<brew_prefix>/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`(mac) /
/// `<runner bin>;%SystemRoot%\system32;...`(win) — 절대경로 실행만으로는
/// 부족하다(brew/npm 내부에서 git/curl/ruby/node를 셔뱅으로 부른다). 정본은
/// `platform::compose_path_env`(설계 §D.2) — 이 함수는 시그니처를 유지한 채
/// 플랫폼을 다시 읽어 위임하는 얇은 래퍼다. mac 분기는 기존 로직과 완전히
/// 동일한 값을 내야 한다(기존 3개 테스트가 무수정으로 이를 증명한다).
pub(crate) fn build_child_path_env(runner_path: Option<&str>) -> String {
    super::platform::compose_path_env(
        super::platform::platform_now(),
        runner_path,
        &super::platform::EnvRoots::from_env(),
    )
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
    // (파이프 리더 스레드 + try_wait 폴링 경로 자체의 배관 검증). `/bin/echo`는
    // Windows에 존재하지 않는 Unix 바이너리라(spawn_error가 나 CI에서 거짓
    // 실패했다 — review-devtools-windows-parity CI 8건 조사) `#[cfg(unix)]`로
    // 게이팅한다. Windows 등가물은 바로 아래 함수(같은 배관을 `cmd.exe /C
    // echo`로 검증).
    #[cfg(unix)]
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

    /// Windows 등가물(§ 위 유닉스 버전과 동일 취지). `cmd.exe`를 bare-name이
    /// 아니라 `%SystemRoot%\System32\cmd.exe` 절대경로로 고정한다 —
    /// bare-name 스폰이 하이재킹 표면이 된다는 이 코드베이스의 기존 원칙
    /// (`cli_launcher.rs`/`platform.rs::windows_system_tool` 주석)을 테스트
    /// 코드에도 그대로 적용한다. GitHub Actions `windows-latest`는 표준
    /// 설치라 `%SystemRoot%`가 항상 `C:\Windows`다. 미검증 — 이 개발 머신은
    /// Mac이라 `#[cfg(windows)]`가 컴파일조차 되지 않는다(Windows CI에서만
    /// 실행·확인 가능).
    #[cfg(windows)]
    #[test]
    fn run_process_with_timeout_captures_output_of_fast_process() {
        let output = run_process_with_timeout(
            r"C:\Windows\System32\cmd.exe",
            &["/C".to_string(), "echo".to_string(), "hello".to_string()],
            r"C:\Windows\System32;C:\Windows",
            &[],
            Duration::from_secs(5),
        );
        assert!(output.spawn_error.is_none());
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.timed_out);
    }

    // 결정 6: 타임아웃이 실제로 프로세스를 강제 종료하는지(짧은 타임아웃으로
    // `sleep`을 강제 종료 — 프로세스 그룹 kill 경로까지 실행됨). `/bin/sleep`도
    // Windows에 없어 위와 같은 이유로 `#[cfg(unix)]` 게이팅 + Windows 등가물
    // 분리.
    #[cfg(unix)]
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

    /// Windows 등가물. Windows에는 인자 없는 `/bin/sleep` 상당의 표준 유틸이
    /// 없어 `PING.EXE -n 31 127.0.0.1`(약 30초 이상 응답 없이 대기)로 대신
    /// 버티는 프로세스를 만든다. `force_kill_process_group`의 Windows 분기는
    /// 유예 없이 즉시 `TerminateProcess`이므로(process.rs 상단 주석) 500ms
    /// 타임아웃 뒤 바로 죽어야 한다 — 5초 상한은 유닉스 버전(3초 유예 포함)과
    /// 동일한 여유를 그대로 둔 것이다. 미검증 — Windows CI 전용.
    #[cfg(windows)]
    #[test]
    fn run_process_with_timeout_kills_hanging_process() {
        let started = Instant::now();
        let output = run_process_with_timeout(
            r"C:\Windows\System32\PING.EXE",
            &["-n".to_string(), "31".to_string(), "127.0.0.1".to_string()],
            r"C:\Windows\System32;C:\Windows",
            &[],
            Duration::from_millis(500),
        );
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "타임아웃 뒤 강제종료가 동작하지 않은 것"
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

    // `build_child_path_env`는 `platform::platform_now()`로 실행 중인 OS를 직접
    // 읽는 얇은 래퍼라(주입 불가 — 공개 진입점 자체가 "이 프로세스가 실제로 돌고
    // 있는 플랫폼"을 답해야 하는 지점이다) mac 전용 기대값은 Windows CI에서
    // 거짓 실패했다(review-devtools-windows-parity CI 8건 조사) — 순수 로직
    // 자체(`platform::compose_path_env`)는 이미 `platform_tests.rs`가
    // `Platform::Mac`/`Platform::Win`을 모두 주입해 골든 테스트로 고정하므로,
    // 여기 남는 두 테스트는 "이 래퍼가 이 머신의 실제 플랫폼에 맞게 제대로
    // 배선됐는가"만 각자의 OS에서 검증한다.
    #[cfg(unix)]
    #[test]
    fn build_child_path_env_prioritizes_runner_bin_dir() {
        let path = build_child_path_env(Some("/opt/homebrew/bin/brew"));
        assert!(path.starts_with("/opt/homebrew/bin:"));
        assert!(path.contains("/usr/bin"));
        assert!(path.contains("/bin"));
    }

    /// Windows 등가물 — `system_root`가 없으면 `C:\Windows`로 폴백하므로(§
    /// `platform::system_root_or_default`), 실제 CI 러너의 사용자명/홈 경로에
    /// 좌우되지 않는 `C:\Windows\system32` 포함 여부만 검증한다(완전 문자열
    /// 일치는 `compose_path_env_win_prioritizes_runner_bin_dir`가 주입된
    /// `EnvRoots`로 이미 커버한다). 미검증 — Windows CI 전용.
    #[cfg(windows)]
    #[test]
    fn build_child_path_env_prioritizes_runner_bin_dir() {
        let path = build_child_path_env(Some(r"C:\Program Files\nodejs\npm.cmd"));
        assert!(path.starts_with(r"C:\Program Files\nodejs;"));
        assert!(path.contains(r"C:\Windows\system32"));
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

    // N1(2라운드 비차단): Mac(및 그 외 Unix 일반)에서는 `child_current_dir`가
    // 항상 `None`이라 `run_process_with_timeout`이 `.current_dir()`를 호출하지
    // 않는다 — 자식이 이 프로세스의 실제 CWD를 그대로 상속해야 한다(무변경
    // 보장, 실측). `/bin/pwd`가 Windows에 없어 CI에서 거짓 실패했다
    // (review-devtools-windows-parity CI 8건 조사, 이번 라운드가 추가한 테스트라
    // 게이트 누락 자체가 이번 라운드 책임) — `#[cfg(unix)]`로 게이팅하고,
    // 바로 아래에 "Windows는 반대로 고정된다"는 불변식을 검증하는 등가물을 둔다.
    #[cfg(unix)]
    #[test]
    fn run_process_with_timeout_does_not_pin_current_dir_on_mac() {
        let expected = std::env::current_dir().expect("current_dir must be readable");
        let output = run_process_with_timeout(
            "/bin/pwd",
            &[],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(5),
        );
        assert_eq!(output.stdout.trim(), expected.to_string_lossy());
    }

    /// Windows 대칭 짝. `child_current_dir`(§platform.rs)는 Windows에서 항상
    /// `Some(%SystemRoot%)`를 돌려줘 자식 CWD를 고정한다(`.cmd` shim이
    /// `cmd.exe` 해석 시 CWD를 먼저 보는 하이재킹 표면을 막기 위해서 — N1).
    /// `cmd.exe /C cd`는 인자 없이 실행하면 현재 디렉터리를 stdout에 출력한다.
    /// 이 테스트가 없으면 Mac 쪽 "고정 안 함" 분기만 검증되고 Windows 쪽
    /// "고정함" 분기는 아무 테스트도 없는 채로 남는다(완결성 공백) — 미검증은
    /// "PowerShell/cmd.exe가 실제로 SystemRoot 환경변수를 그대로 노출하는가"
    /// 뿐이며, 이 개발 머신(Mac)에서는 컴파일조차 되지 않아 Windows CI에서만
    /// 최초로 실행·확인된다.
    #[cfg(windows)]
    #[test]
    fn run_process_with_timeout_pins_current_dir_to_system_root_on_windows() {
        let expected = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        let output = run_process_with_timeout(
            r"C:\Windows\System32\cmd.exe",
            &["/C".to_string(), "cd".to_string()],
            r"C:\Windows\System32;C:\Windows",
            &[],
            Duration::from_secs(5),
        );
        assert_eq!(output.stdout.trim(), expected);
    }
}
