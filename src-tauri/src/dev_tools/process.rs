// ==================== 5. 버전 문자열 정규화(결정 4) ====================

use super::DevTool;
use crate::process_util::{spawn_pipe_drain, SilentCommand, DRAIN_GRACE};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
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

/// Windows에는 POSIX 프로세스 그룹이 없다 — 대신 `taskkill /PID <pid> /T /F`로
/// 직속 자식과 그 자손 트리를 함께 강제 종료한다(`process_util::
/// kill_process_group_with_grace`의 Windows 분기 — 셸을 거치지 않고
/// `%SystemRoot%\System32\taskkill.exe` 절대경로로 직접 실행하므로 S1
/// 불변식과 bare-name spawn 금지 원칙을 그대로 지킨다). `claude.cmd` →
/// `cmd.exe` → `node` 구조에서 `Child::kill`(`TerminateProcess`)만 쓰면 직속
/// 자식인 `cmd.exe`만 죽고 손자 `node`가 살아남아 파이프를 계속 물고 있었다.
///
/// 이어서 `Child::kill`을 한 번 더 호출한다 — taskkill이 없거나 실패한 경우의
/// 최소 폴백이며(이미 죽었으면 무해한 에러), 그 뒤 `wait`로 직속 자식을
/// 수거한다. `taskkill /T`는 kill 시점에 부모 PID 관계가 살아 있는 트리만
/// 따라간다: 중간 프로세스가 이미 종료돼 고아가 된 손자는 트리에서 끊겨
/// 못 찾는다(Job Object 도입은 범위 밖, 설계 §9 미해결쟁점 1). 그 경우에도
/// 호출자는 `drain` 상한(`process_util::DRAIN_GRACE`) 덕에 막히지 않는다.
///
/// Winget 러너는 구조적 한계가 따로 있다(N4, 2라운드): winget이 UAC로
/// 승격해서 띄우는 msiexec 등 손자 프로세스는 우리 프로세스보다 높은 무결성
/// 수준(elevated token)으로 실행된다 — taskkill도 권한 부족으로 ACCESS_DENIED가
/// 난다. "드물게 남는다"가 아니라 "구조적으로 죽일 수 없다"이다. 타임아웃
/// 시 winget.exe 자신(직속 자식)은 종료되지만, 이미 승격되어 독립한 설치
/// 프로세스는 백그라운드에서 계속 진행될 수 있다(`actions.rs`의
/// `timed_out_message`가 이 사실을 사용자에게 알린다).
#[cfg(windows)]
fn force_kill_process_group(pid: i32, child: &mut std::process::Child) -> Option<i32> {
    crate::process_util::kill_process_group_with_grace(pid as u32);
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
    run_process_with_timeout_cancellable(
        binary_path,
        args,
        path_env,
        extra_env,
        timeout,
        None,
        None,
        None,
    )
}

/// `on_spawn`: spawn 직후 자식 pid를 공표한다(자율업무 런타임 레지스트리
/// 등록용). `should_abort`: `wait_up_to_cancellable`의 100ms 폴링마다 확인해
/// `true`면 타임아웃과 동일한 경로(`force_kill_process_group`)로 kill한다.
/// 자율업무는 이 함수를 통해 앱 종료 시 진행 중인 `claude -p`를 정리한다.
/// `working_dir`: `Some`이면 그 디렉터리로 `current_dir`를 고정한다(자율업무의
/// project_path). `None`이면 기존 동작대로 Windows 전용 `child_current_dir`
/// 보정만 적용한다(도구 설치·버전조회 호출부는 전부 `None`).
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
    working_dir: Option<&std::path::Path>,
) -> ProcessRunOutput {
    run_process_inner(
        binary_path,
        args,
        path_env,
        extra_env,
        timeout,
        on_spawn,
        should_abort,
        working_dir,
        DRAIN_GRACE,
    )
}

/// `drain_grace`만 주입 가능하게 분리한 본체(테스트가 짧은 상한으로 출력 보존을
/// 확인하려는 용도 — 공개 진입점은 항상 `DRAIN_GRACE`를 넘긴다).
// 인자 9개는 공개 진입점(`run_process_with_timeout_cancellable`, 8개)의 시그니처에
// `drain_grace` 하나를 얹은 것이다. 옵션 구조체로 묶으면 호출부 9곳과 테스트의
// 시그니처 불변 약속(자율업무 설계 §4)을 건드리는 범위 밖 리팩터가 된다.
#[allow(clippy::too_many_arguments)]
fn run_process_inner(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
    on_spawn: Option<&dyn Fn(u32)>,
    should_abort: Option<&dyn Fn() -> bool>,
    working_dir: Option<&std::path::Path>,
    drain_grace: Duration,
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
    if let Some(dir) = working_dir {
        command.current_dir(dir);
    } else if let Some(dir) = super::platform::child_current_dir(
        // N1(2라운드 비차단): Windows에서만 현재 디렉터리를 `%SystemRoot%`로
        // 고정한다(`super::platform::child_current_dir` 참고 — `.cmd` shim은
        // `cmd.exe`가 해석하는데 그 명령 해석이 현재 디렉터리를 먼저 본다). Mac은
        // `child_current_dir`가 항상 `None`이라 이 블록이 아무 것도 하지 않는다
        // (기존 동작 무변경 — 이 머신은 항상 Mac이라 실행 시 검증됨).
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

    let spawned = {
        // 테스트 전용: 이 모듈 테스트끼리의 spawn을 직렬화한다(`process_util::test_support::SPAWN_LOCK` 주석).
        #[cfg(test)]
        let _spawn_guard = crate::process_util::test_support::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        command.spawn()
    };
    let mut child = match spawned {
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
    let stdout_drain = child.stdout.take().map(spawn_pipe_drain);
    let stderr_drain = child.stderr.take().map(spawn_pipe_drain);

    let (exit_code, timed_out, aborted) =
        match wait_up_to_cancellable(&mut child, timeout, started, should_abort) {
            WaitOutcome::Exited(status) => (status.code(), false, false),
            WaitOutcome::TimedOut => (force_kill_process_group(pid, &mut child), true, false),
            WaitOutcome::Aborted => (force_kill_process_group(pid, &mut child), false, true),
        };

    // 자식은 이미 종료/수거됐다. 리더는 EOF까지만 기다리되 상한을 둔다 —
    // 그룹/트리를 벗어난 자손이 파이프를 물고 있어도 반환하도록(`DRAIN_GRACE`
    // 주석 참고). 두 파이프가 하나의 마감시각을 공유한다.
    let drain_deadline = Instant::now() + drain_grace;
    let (stdout_bytes, stdout_capped) = stdout_drain
        .map(|d| d.collect_until(drain_deadline))
        .unwrap_or_default();
    let (stderr_bytes, stderr_capped) = stderr_drain
        .map(|d| d.collect_until(drain_deadline))
        .unwrap_or_default();
    if stdout_capped || stderr_capped {
        // 상한 초과 = 그룹·트리를 벗어난 자손이 파이프를 물고 있다는 뜻이다(고아 가능).
        // 반환값은 바꾸지 않으므로 이 한 줄이 현장 진단의 유일한 신호다. 인자는
        // 비밀이 들어갈 수 있어 프로그램 파일명만 남긴다.
        eprintln!(
            "[process] drain 상한 초과: pid={pid}, program={}, stdout={}, stderr={}, 상한={}ms, 종료사유={} — 자손이 파이프를 물고 있을 수 있음(고아 가능)",
            std::path::Path::new(binary_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            if stdout_capped { "미종료" } else { "EOF" },
            if stderr_capped { "미종료" } else { "EOF" },
            drain_grace.as_millis(),
            if timed_out {
                "타임아웃"
            } else if aborted {
                "중단"
            } else {
                "정상종료"
            },
        );
    }

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
    /// 유예 없이 즉시 `taskkill /T /F`로 트리를 종료하므로(위 함수 주석) 500ms
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
        // `exit_code.is_none()`은 여기서 단언하지 않는다 — Windows 분기의
        // `force_kill_process_group`은 무조건 `None`을 반환하므로(위 함수
        // 정의 참고) 항상 참인 항진명제라 회귀를 잡지 못한다(4차 리뷰 V6②).
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "타임아웃 뒤 강제종료가 동작하지 않은 것"
        );
    }

    // `/bin/cat`도 Windows에 없어 위와 같은 이유로 거짓 초록이 됐다
    // (review-devtools-windows-parity 4차 리뷰 V1 — 이전 라운드의 8건 조사가
    // "로그에 뜬 실패"만 훑어 이 자리를 놓쳤다: spawn 자체가 실패하면
    // `spawn_error: Some(..)` / `timed_out: false` / `elapsed ≈ 0`이 되어
    // 아래 두 단언이 **둘 다 통과**해 windows-latest에서 아무것도 검증하지
    // 않은 채 초록으로 찍혔다). `#[cfg(unix)]` 게이팅 + `spawn_error.is_none()`
    // 단언을 추가해, 앞으로 같은 유형의 게이팅 누락이 있으면 공허한 초록이
    // 아니라 즉시 빨강으로 드러나게 한다. Windows 등가물은 바로 아래.
    #[cfg(unix)]
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
            output.spawn_error.is_none(),
            "spawn 자체가 실패하면 아래 두 단언이 공허하게 통과한다 — 먼저 spawn 성공을 확인한다"
        );
        assert!(
            !output.timed_out,
            "stdin=null이 적용되지 않았다면 cat이 멈춰 타임아웃까지 갔을 것"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// Windows 등가물. `sort.exe`는 인자 없이 실행하면 stdin을 읽어 정렬한
    /// 결과를 출력하는 표준 유틸이다 — stdin이 null이면 즉시 EOF를 받아 빈
    /// 출력·exit code 0으로 빠르게 종료해야 한다(부록 B.1과 동일 취지, `cat`과
    /// 달리 EOF에서 항상 성공 종료해 exit_code까지 단언할 수 있다). 미검증 —
    /// 이 개발 머신(Mac)에서는 컴파일조차 되지 않아 Windows CI에서만
    /// 최초로 실행·확인된다.
    #[cfg(windows)]
    #[test]
    fn stdin_is_null_so_a_reading_process_fails_fast_instead_of_hanging() {
        let started = Instant::now();
        let output = run_process_with_timeout(
            r"C:\Windows\System32\sort.exe",
            &[],
            r"C:\Windows\System32;C:\Windows",
            &[],
            Duration::from_secs(5),
        );
        assert!(
            output.spawn_error.is_none(),
            "spawn 자체가 실패하면 아래 단언들이 공허하게 통과한다 — 먼저 spawn 성공을 확인한다"
        );
        assert!(
            !output.timed_out,
            "stdin=null이 적용되지 않았다면 sort가 멈춰 타임아웃까지 갔을 것"
        );
        assert_eq!(output.exit_code, Some(0));
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

    // ── 자손이 파이프를 물고 있어도 상한 안에 반환한다(리더 join 상한 없음 회귀) ──

    #[cfg(unix)]
    use crate::process_util::test_support::{unique_pidfile, KillOnDrop};

    /// (시간 단언은 `DRAIN_GRACE`=2초 기준 리터럴이다 — 상수를 바꾸면 같이 본다.)
    /// 자식이 정상 종료했지만 같은 그룹의 자손(`sleep 60 &`)이 stdout/stderr
    /// 파이프를 상속한 채 살아 있다. 예전 구현은 EOF를 영영 못 받아 `join()`에서
    /// 멈췄다. 지금은 `DRAIN_GRACE`(2초) 안에 반환하고, 자식이 종료 전에 쓴
    /// 출력은 보존해야 한다. 자손 pid는 `$!`로 파일에 남겨 테스트가 정리한다.
    #[cfg(unix)]
    #[test]
    fn descendant_holding_pipe_after_normal_exit_does_not_block_and_keeps_output() {
        let pidfile = unique_pidfile("normal");
        let guard = KillOnDrop(pidfile.clone());
        let script = format!(
            "echo before-exit; echo err-before-exit >&2; sleep 60 & echo $! > '{}'; exit 0",
            pidfile.display()
        );
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/sh",
            &["-c".to_string(), script],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(10),
        );
        let elapsed = started.elapsed();

        assert!(output.spawn_error.is_none());
        assert_eq!(output.exit_code, Some(0));
        assert!(!output.timed_out && !output.aborted);
        assert!(
            elapsed < Duration::from_secs(8),
            "자손이 파이프를 물고 있어도 상한 안에 반환해야 한다: {elapsed:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(1500),
            "자손이 파이프를 물고 있으면 EOF가 안 오므로 상한까지 기다렸어야 한다(테스트 전제 검증): {elapsed:?}"
        );
        assert_eq!(output.stdout.trim(), "before-exit");
        assert_eq!(output.stderr.trim(), "err-before-exit");
        assert!(guard.pid().is_some(), "자손 pid가 기록되어야 한다(전제)");
    }

    /// 타임아웃 kill 경로: 자손이 프로세스 그룹을 벗어나(`setsid`) 파이프를 물고
    /// 있으면 그룹 kill로도 못 죽인다. perl이 fork→setsid→exec로 그런 자손을
    /// 만든다(macOS/Linux 모두 `/usr/bin/perl` 기본 설치, `setsid` 명령은 macOS에
    /// 없다). kill 뒤에도 상한 안에 반환하고, kill 전에 쓴 출력은 보존해야 한다.
    #[cfg(unix)]
    #[test]
    fn setsid_escaped_descendant_holding_pipe_after_timeout_kill_does_not_block() {
        let pidfile = unique_pidfile("escaped");
        let guard = KillOnDrop(pidfile.clone());
        let script = format!(
            "echo started-ok; /usr/bin/perl -MPOSIX -e 'if (fork() == 0) {{ setsid(); exec \"/bin/sh\", \"-c\", \"echo \\$\\$ > {pf}; exec sleep 60\"; }}'; sleep 30",
            pf = pidfile.display()
        );
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/sh",
            &["-c".to_string(), script],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(1),
        );
        let elapsed = started.elapsed();

        assert!(output.spawn_error.is_none());
        assert!(output.timed_out, "자식(sleep 30)은 타임아웃으로 kill되어야 한다");
        // 타임아웃 1s + 그룹 kill(SIGTERM 즉시 사망) + DRAIN_GRACE 2s + 여유. 상한은
        // `DRAIN_GRACE` 상수와 무관한 리터럴이어야 상수를 키워도 회귀를 잡는다.
        assert!(
            elapsed < Duration::from_secs(8),
            "그룹을 벗어난 자손이 파이프를 물고 있어도 상한 안에 반환해야 한다: {elapsed:?}"
        );
        assert_eq!(output.stdout.trim(), "started-ok");
        // 자손이 실제로 살아서 그룹 밖에 있었는지(전제) — pid 파일이 늦게 생길 수 있어 잠깐 폴링한다.
        let poll_until = Instant::now() + Duration::from_secs(3);
        while guard.pid().is_none() && Instant::now() < poll_until {
            std::thread::sleep(Duration::from_millis(50));
        }
        let pid = guard.pid().expect("그룹 밖 자손 pid가 기록되어야 한다(전제)");
        assert!(
            crate::process_util::pid_alive(pid as u32),
            "그룹 kill 뒤에도 setsid 자손은 살아 있어야 이 테스트가 의미가 있다"
        );
    }

    /// 짧은 상한을 주입한 본체 호출: 주입한 상한이 실제로 적용되어(기본 2초가 아니라)
    /// 상한까지는 기다리고, 상한 이전에 도착한 출력(`mid`)은 보존하며, 상한 이후에
    /// 쓰는 출력(`late`)은 결과에 끼지 않는다. 자손(`sleep 5`가 낀 서브셸)이 파이프를
    /// 물고 있으므로 EOF가 오지 않아 상한까지 기다리는 것이 이 테스트의 전제다 —
    /// 하한 단언이 그 전제를 확인한다(자손이 파이프를 안 물었다면 즉시 반환해 실패).
    /// 상한 이후 자손이 쓰는 `late`는 반환 뒤에 도착하므로 결과에서 관측할 수 없고,
    /// 대신 반환 시각이 상한 부근이라는 점으로 리더 포기가 실제로 일어났음을 가른다.
    #[cfg(unix)]
    #[test]
    fn drain_grace_is_injectable_and_keeps_output_read_before_deadline() {
        let pidfile = unique_pidfile("inject");
        let guard = KillOnDrop(pidfile.clone());
        let script = format!(
            "echo early; (sleep 0.1; echo mid; sleep 5; echo late) & echo $! > '{}'; exit 0",
            pidfile.display()
        );
        let started = Instant::now();
        let output = run_process_inner(
            "/bin/sh",
            &["-c".to_string(), script],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(10),
            None,
            None,
            None,
            Duration::from_millis(600),
        );
        let elapsed = started.elapsed();
        assert!(output.spawn_error.is_none());
        assert_eq!(output.exit_code, Some(0));
        assert!(!output.timed_out && !output.aborted);
        assert!(
            elapsed >= Duration::from_millis(550),
            "자손이 파이프를 물고 있으면 EOF가 안 오므로 주입한 상한(600ms)까지 기다렸어야 한다(전제 검증): {elapsed:?}"
        );
        // 기본 상한(2초)이 아니라 주입한 상한이 적용됐다는 것만 가르면 되므로 넉넉히 잡는다.
        assert!(
            elapsed < Duration::from_millis(1500),
            "주입한 상한(600ms)을 지켜야 한다: {elapsed:?}"
        );
        assert_eq!(
            output.stdout.lines().collect::<Vec<_>>(),
            vec!["early", "mid"],
            "상한 이전에 읽은 출력만 보존해야 한다"
        );
        assert!(guard.pid().is_some(), "자손 pid가 기록되어야 한다(전제)");
    }

    /// Windows: 직속 자식 `cmd.exe`가 손자 `PING.EXE`(30초 이상)를 띄우고, 손자가
    /// stdout 파이프를 상속한다. 타임아웃 kill이 직속 자식만 죽이면 손자가
    /// 파이프를 계속 물어 리더 EOF가 안 온다. 트리째 종료(`taskkill /T /F`)하면
    /// 파이프가 바로 닫히므로, 일부러 큰 drain 상한(10초)을 주입해도 함수는
    /// 훨씬 일찍(8초 미만 — 타임아웃 3초 + taskkill 상한) 반환해야 한다 — 상한 덕분이 아니라 트리 종료 덕분임을
    /// 가르는 설계다. `cmd.exe`/`PING.EXE`는 절대경로. 미검증 — 이 개발
    /// 머신(Mac)에서는 컴파일 체크만 가능하고 실행은 Windows CI 전용이다.
    #[cfg(windows)]
    #[test]
    fn timeout_kill_terminates_grandchild_tree_so_pipe_closes_before_drain_grace() {
        // 타임아웃을 3초로 둔다: `cmd.exe /C`는 자신의 명령(PING)이 도는 동안만
        // 살아 있으므로 `timed_out`이 참이면 cmd가 PING을 기다리는 중이었다는 뜻이다.
        // 700ms처럼 짧으면 느린 러너에서 PING이 뜨기 전에 cmd가 죽어 손자 없이 공허하게
        // 통과할 수 있다. `ready`는 cmd가 명령줄을 해석해 PING 직전까지 갔다는 확인이다.
        // (PING의 첫 출력 줄이 파이프로 즉시 flush되는지는 Windows 실기로 확인하지
        // 못해 단언하지 않는다.)
        let started = Instant::now();
        let output = run_process_inner(
            r"C:\Windows\System32\cmd.exe",
            &[
                "/C".to_string(),
                r"echo ready & C:\Windows\System32\PING.EXE -n 31 127.0.0.1".to_string(),
            ],
            r"C:\Windows\System32;C:\Windows",
            &[],
            Duration::from_secs(3),
            None,
            None,
            None,
            Duration::from_secs(10),
        );
        assert!(output.spawn_error.is_none());
        assert!(
            output.stdout.contains("ready"),
            "cmd.exe가 명령줄을 해석해 PING 직전까지 가야 이 테스트가 의미가 있다: {:?}",
            output.stdout
        );
        assert!(
            output.timed_out,
            "cmd.exe가 PING을 기다리는 중이어야 타임아웃으로 간다(손자 존재 전제)"
        );
        // 3초 타임아웃 + taskkill(상한 5초) 안에 반환해야 한다. 트리 종료가 회귀하면
        // 손자가 파이프를 물어 3초 + 주입한 상한 10초 = 13초 부근이 된다.
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "손자(PING)가 트리 종료로 죽지 않아 파이프를 계속 물고 있었다: {:?}",
            started.elapsed()
        );
    }
}
