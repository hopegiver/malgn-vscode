// ---------------- 프로세스 실행 ----------------

use crate::process_util::SilentCommand;

/// `dev_tools.rs`/`autonomy.rs`가 이미 정한 관례(결정 5.2 — 상수를 공유하지
/// 않고 각자 별도로 둔다) 그대로 이 모듈에도 독립적으로 둔다.
pub(super) const CLAUDE_PATH_CANDIDATES: [&str; 4] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
    // P2-3b: 앱이 설치한 Node의 npm 전역 prefix에 설치된 Claude(dev_tools DEV_TOOLS와 같은 값).
    crate::dev_tools::NPM_GLOBAL_CLAUDE_CANDIDATE,
];

/// `claude mcp *` 서브커맨드가 멈춰도(네트워크 불량, 인증 프롬프트 대기 등)
/// 앱이 영구히 막히지 않도록 두는 상한. 헬스체크성 호출이라 15초면 충분히
/// 넉넉하다.
const MCP_COMMAND_TIMEOUT_SECS: u64 = 15;

/// `claude <args>`를 `resolve_binary_expand_home` + `build_child_path_env` +
/// `current_dir(home)`로 실행한다. 반환값은 (성공 시 stdout, 실패 시 stderr —
/// 둘 다 비어있으면 나머지 쪽, success 여부). 바이너리를 찾지 못하거나 홈
/// 디렉터리를 확인할 수 없으면 `Err`.
pub(super) fn run_mcp_command(args: &[&str]) -> Result<(String, bool), String> {
    run_mcp_command_with_env(args, None)
}

/// `run_mcp_command`와 동일하되, `extra_env`가 `Some((key, value))`이면 그
/// 자식 프로세스에만 환경변수를 하나 추가로 주입한다(이 호출에만 적용되고
/// 다른 `run_mcp_command` 호출자에는 영향이 없다 — OAuth client secret을
/// `MCP_CLIENT_SECRET`으로 넘기는 `mcp_add` 전용 경로).
///
/// 예전엔 `Command::output()`으로 무기한 블로킹했다(P0 버그와 동일 계열 —
/// `claude` CLI가 멈추면 이 호출도 영원히 멈춘다). `dev_tools.rs`의
/// `run_process_with_timeout_cancellable`과 동일한 관용구(stdin 차단 + 파이프
/// 리더 스레드 + `try_wait()` 폴링 타임아웃)를 이 모듈 안에서 가볍게 재현한다
/// — 그쪽 헬퍼는 프로세스 그룹 kill·`on_spawn`/`should_abort` 훅 등 도구
/// 설치 전용 기능까지 딸려 있고 `current_dir` 지정도 지원하지 않아 그대로
/// 재사용하기보다 이 파일의 필요(고정 `home` cwd, 단일 자식 프로세스)에 맞는
/// 최소 형태로 옮긴다.
pub(super) fn run_mcp_command_with_env(
    args: &[&str],
    extra_env: Option<(&str, &str)>,
) -> Result<(String, bool), String> {
    let resolved =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
            .ok_or_else(|| {
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                    .to_string()
            })?;
    let home = dirs::home_dir().ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    let mut command = std::process::Command::new(&resolved);
    command
        .args(args)
        .current_dir(&home)
        .env("PATH", &path_env)
        .envs(crate::dev_tools::claude_child_env(&resolved))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .silent();
    if let Some((key, value)) = extra_env {
        command.env(key, value);
    }

    run_bounded(
        &mut command,
        std::time::Duration::from_secs(MCP_COMMAND_TIMEOUT_SECS),
        crate::process_util::DRAIN_GRACE,
    )
}

/// 이미 구성된 `command`(stdout/stderr piped)를 `timeout` 안에 끝내고 출력을 수거한다.
/// `run_mcp_command_with_env`에서 claude 해석과 분리해 둔 본체 — 상한 동작을 실제 프로세스로
/// 테스트하려는 용도다. 출력 수거는 `process_util::PipeDrain`으로 하며, 자식이 끝나거나
/// kill된 뒤에도 자손이 파이프를 물고 있으면 `drain_grace`에서 끊고 그때까지 읽은 만큼만
/// 쓴다(예전에는 리더 스레드를 상한 없이 `join()`해 이 함수가 반환하지 않았다).
fn run_bounded(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
    drain_grace: std::time::Duration,
) -> Result<(String, bool), String> {
    let spawned = {
        // 테스트 전용: 파이프 상속 경합을 피하려고 spawn을 직렬화한다(`SPAWN_LOCK` 주석).
        #[cfg(test)]
        let _spawn_guard = crate::process_util::test_support::SPAWN_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        command.spawn()
    };
    let mut child = spawned.map_err(|e| format!("claude 명령을 실행하지 못했습니다: {e}"))?;

    let stdout_drain = child.stdout.take().map(crate::process_util::spawn_pipe_drain);
    let stderr_drain = child.stderr.take().map(crate::process_util::spawn_pipe_drain);

    let started = std::time::Instant::now();
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => break None,
        }
    };

    // 두 파이프가 하나의 마감시각을 공유한다.
    let drain_deadline = std::time::Instant::now() + drain_grace;
    let stdout_bytes = crate::process_util::PipeDrain::collect_logged(
        stdout_drain,
        drain_deadline,
        "mcp_manager claude 명령 stdout",
    );
    let stderr_bytes = crate::process_util::PipeDrain::collect_logged(
        stderr_drain,
        drain_deadline,
        "mcp_manager claude 명령 stderr",
    );
    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();

    let Some(status) = exit_status else {
        return Err(format!(
            "claude 명령이 {}초 내에 끝나지 않아 중단했습니다.",
            timeout.as_secs()
        ));
    };

    let success = status.success();
    let text = if success || stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    Ok((text, success))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GUI(.app) 실행 시 PATH가 제한될 수 있다는 `cli_launcher.rs`의 실측
    /// 문제를 이 모듈도 겪지 않는지 회귀로 고정한다 — `autonomy.rs`의 동일
    /// 테스트와 같은 취지다.
    // 머신 의존(이 개발 머신에 claude가 실제로 설치돼 있어야 함) — CI에는 그
    // 전제가 없어 #[ignore]. 로컬 실행: cargo test -- --ignored
    // mcp_manager::process::tests::claude_path_candidates_resolve_to_something_on_a_machine_with_claude_installed
    #[test]
    #[ignore]
    fn claude_path_candidates_resolve_to_something_on_a_machine_with_claude_installed() {
        let resolved =
            crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude");
        assert!(
            resolved.is_some(),
            "이 개발 머신에는 claude가 설치돼 있어야 하는데 절대경로 후보와 PATH 폴백 모두 실패했습니다"
        );
    }

    /// 자식이 정상 종료했지만 자손(`sleep 60 &`)이 stdout/stderr 파이프를 상속한 채 살아
    /// 있다. 예전 구현은 EOF를 영영 못 받아 리더 `join()`에서 멈췄다. 지금은 주입한 상한
    /// 안에 반환하고, 자식이 종료 전에 쓴 출력과 성공 판정은 그대로여야 한다.
    #[cfg(unix)]
    #[test]
    fn descendant_holding_pipe_does_not_block_and_keeps_output_and_success() {
        use crate::process_util::test_support::{unique_pidfile, KillOnDrop};
        use std::time::{Duration, Instant};

        let pidfile = unique_pidfile("mcp-normal");
        let guard = KillOnDrop(pidfile.clone());
        let script = format!(
            "echo before-exit; sleep 60 & echo $! > '{}'; exit 0",
            pidfile.display()
        );
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let started = Instant::now();
        let result = run_bounded(&mut command, Duration::from_secs(10), Duration::from_millis(500));
        let elapsed = started.elapsed();

        let (text, success) = result.expect("자식은 정상 종료했으므로 Ok여야 한다");
        assert!(success);
        assert_eq!(text.trim(), "before-exit");
        assert!(
            elapsed >= Duration::from_millis(450),
            "자손이 파이프를 물고 있으면 EOF가 안 오므로 주입한 상한까지 기다렸어야 한다(전제 검증): {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "자손이 파이프를 물고 있어도 상한 안에 반환해야 한다: {elapsed:?}"
        );
        assert!(guard.pid().is_some(), "자손 pid가 기록되어야 한다(전제)");
    }

    /// 타임아웃 경로: `child.kill()`은 직속 자식만 죽이므로 자손이 파이프를 계속 문다.
    /// 그래도 타임아웃 에러를 상한 안에 돌려줘야 한다.
    #[cfg(unix)]
    #[test]
    fn timeout_kill_with_descendant_holding_pipe_still_returns_timeout_error() {
        use crate::process_util::test_support::{unique_pidfile, KillOnDrop};
        use std::time::{Duration, Instant};

        let pidfile = unique_pidfile("mcp-timeout");
        let guard = KillOnDrop(pidfile.clone());
        // exec로 sleep이 셸을 대체하면 pid가 같아져 손자가 생기지 않는다 — exec 없이 셸이
        // 손자를 낳고 기다리게 한다(셸 kill 뒤에도 손자가 남는다).
        let script = format!("sleep 60 & echo $! > '{}'; wait", pidfile.display());
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let started = Instant::now();
        let result = run_bounded(&mut command, Duration::from_millis(400), Duration::from_millis(500));
        let elapsed = started.elapsed();

        let err = result.expect_err("타임아웃이면 Err여야 한다");
        assert!(err.contains("내에 끝나지 않아 중단"), "{err}");
        assert!(
            elapsed < Duration::from_secs(5),
            "kill 뒤 손자가 파이프를 물고 있어도 상한 안에 반환해야 한다: {elapsed:?}"
        );
        assert!(guard.pid().is_some(), "자손 pid가 기록되어야 한다(전제)");
    }
}
