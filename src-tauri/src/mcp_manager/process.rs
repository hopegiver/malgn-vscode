// ---------------- 프로세스 실행 ----------------

/// `dev_tools.rs`/`autonomy.rs`가 이미 정한 관례(결정 5.2 — 상수를 공유하지
/// 않고 각자 별도로 둔다) 그대로 이 모듈에도 독립적으로 둔다.
pub(super) const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
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
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some((key, value)) = extra_env {
        command.env(key, value);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("claude 명령을 실행하지 못했습니다: {e}"))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_reader = stdout_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut pipe, &mut buf);
            buf
        })
    });
    let stderr_reader = stderr_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut pipe, &mut buf);
            buf
        })
    });

    let timeout = std::time::Duration::from_secs(MCP_COMMAND_TIMEOUT_SECS);
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

    let stdout_bytes = stdout_reader.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr_bytes = stderr_reader.and_then(|h| h.join().ok()).unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();

    let Some(status) = exit_status else {
        return Err(format!(
            "claude 명령이 {MCP_COMMAND_TIMEOUT_SECS}초 내에 끝나지 않아 중단했습니다."
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
    #[test]
    fn claude_path_candidates_resolve_to_something_on_a_machine_with_claude_installed() {
        let resolved =
            crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude");
        assert!(
            resolved.is_some(),
            "이 개발 머신에는 claude가 설치돼 있어야 하는데 절대경로 후보와 PATH 폴백 모두 실패했습니다"
        );
    }
}
