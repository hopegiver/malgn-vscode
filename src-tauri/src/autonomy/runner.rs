// 프롬프트 조립·claude 경로 해석·프로세스 실행(설계 §1·§4·§9). `scheduler.rs`가
// due로 판정한 task 하나를 스냅숏으로 받아 끝까지(성공/실패/타임아웃/앱 종료로
// 중단) 처리하고, 메모리 런타임 상태 갱신 + 로그 기록 + 이벤트 emit까지 담당한다.

use super::log;
use super::runtime::{self, RunStatus, TaskKey};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// 실행 대상으로 확정된 task 하나의 스냅숏 — "running으로 마킹" 시점의
/// prompt/subagent/timeout을 그대로 들고 다닌다. 실제 `claude -p` 실행(수초~
/// 수분) 도중 사용자가 같은 task를 편집해도 이번 실행은 마킹 시점 값으로
/// 끝까지 진행한다(현행 `DueTaskSnapshot` 규율 유지).
pub(crate) struct TaskSnapshot {
    pub key: TaskKey,
    pub project_path: PathBuf,
    pub task_id: String,
    pub task_name: String,
    pub prompt: String,
    pub subagent: Option<String>,
    pub interval_minutes: u32,
    pub timeout_minutes: u32,
}

/// 모든 자율업무 실행 프롬프트 끝에 붙는 안내 footer. malgnai-hub 연동 여부는
/// 프로젝트마다 다르므로 Rust 쪽에서 project_id를 알아내거나 연동 여부를
/// 판단하려 하지 않는다 — 그 판단은 해당 프로젝트의 CLAUDE.md/STATUS.md를
/// 이미 읽고 있는 `claude` 세션에게 조건부로 위임한다.
const HUB_RECORD_FOOTER: &str = "\n\n작업을 마치면 이 프로젝트에 malgnai-hub MCP 연동(CLAUDE.md에 명시된 규율)이 설정되어 있는 경우 그 규율에 따라 work_record 등으로 결과를 기록하라. 연동이 없는 프로젝트라면 이 지시는 무시하라.";

/// `dev_tools.rs`의 Claude 도구 정의(`DEV_TOOLS`)와 값은 같지만, 그 파일이
/// 이미 정한 관례(상수를 공유하지 않고 각자 별도로 둔다, 회귀 위험 0)를 그대로
/// 따라 이 모듈에도 독립적으로 둔다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

pub(crate) fn build_final_prompt(prompt: &str, subagent: &Option<String>) -> String {
    let base = match subagent {
        Some(agent) if !agent.trim().is_empty() => {
            format!("다음 작업을 {agent} 에이전트에게 위임해 처리하라: {prompt}")
        }
        _ => prompt.to_string(),
    };
    format!("{base}{HUB_RECORD_FOOTER}")
}

/// UTF-8 문자 경계를 존중하며 뒤에서부터 최대 `max_chars`자만 남긴다(바이트
/// 슬라이싱은 멀티바이트 문자 중간을 잘라 panic할 수 있어 쓰지 않는다).
pub(crate) fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        text.to_string()
    } else {
        text.chars().skip(total - max_chars).collect()
    }
}

struct ExecOutcome {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    aborted: bool,
    /// spawn 자체가 실패했거나 바이너리를 못 찾았을 때의 메시지.
    hard_error: Option<String>,
}

fn execute(snapshot: &TaskSnapshot, final_prompt: &str, key: &TaskKey) -> ExecOutcome {
    let resolved_claude =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude");

    let Some(claude_path) = resolved_claude else {
        return ExecOutcome {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            aborted: false,
            hard_error: Some(
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                    .to_string(),
            ),
        };
    };

    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));
    let args = vec!["-p".to_string(), final_prompt.to_string()];
    let key_for_spawn = key.clone();
    let on_spawn = |pid: u32| {
        runtime::set_child_pid(&key_for_spawn, Some(pid));
    };
    let should_abort = || runtime::SHUTTING_DOWN.load(Ordering::Relaxed);

    let output = crate::dev_tools::run_process_with_timeout_cancellable(
        &claude_path,
        &args,
        &path_env,
        &[],
        Duration::from_secs(snapshot.timeout_minutes as u64 * 60),
        Some(&on_spawn),
        Some(&should_abort),
    );

    if let Some(spawn_error) = output.spawn_error {
        return ExecOutcome {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            aborted: false,
            hard_error: Some(spawn_error),
        };
    }

    let success = output.exit_code == Some(0) && !output.timed_out && !output.aborted;
    ExecOutcome {
        success,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        timed_out: output.timed_out,
        aborted: output.aborted,
        hard_error: None,
    }
}

/// `claude -p <최종프롬프트>`를 `current_dir(project_path)`로 동기 실행한다
/// (일회성 워커 스레드 안이므로 블로킹 무방). 종료 후 메모리 런타임 상태를
/// 갱신하고, 로그를 기록하고, `autonomy-task-updated` 이벤트를 emit한다.
///
/// 바이너리 해석은 bare name `Command::new("claude")`에 의존하지 않는다 —
/// `cli_launcher.rs` 상단에 문서화된 실측 제약대로, Finder로 띄운 `.app`은
/// launchctl 기본 PATH만 상속해 `/opt/homebrew/bin` 등이 비어 있을 수 있다.
/// 이미 검증된 `resolve_binary_expand_home`으로 실행 파일을 찾고,
/// `dev_tools::build_child_path_env`로 자식 프로세스의 PATH도 명시적으로
/// 넓힌다. HOME 등 나머지 환경변수는 그대로 상속한다(PATH만 덮어쓴다).
pub(crate) fn run_task(snapshot: TaskSnapshot, app_handle: tauri::AppHandle) {
    let started_at = chrono::Utc::now();
    let started_instant = Instant::now();
    let final_prompt = build_final_prompt(&snapshot.prompt, &snapshot.subagent);

    let exec = execute(&snapshot, &final_prompt, &snapshot.key);

    let finished_at = chrono::Utc::now();
    let duration_ms = started_instant.elapsed().as_millis() as u64;

    let result_label: &'static str = if exec.timed_out {
        "timeout"
    } else if exec.aborted {
        "aborted"
    } else if exec.success {
        "success"
    } else {
        "failed"
    };

    let status = if exec.timed_out {
        RunStatus::Timeout
    } else if exec.success {
        RunStatus::Success
    } else {
        RunStatus::Failed
    };

    let summary_text = if let Some(err) = &exec.hard_error {
        err.clone()
    } else if exec.success || exec.stderr.trim().is_empty() {
        exec.stdout.trim().to_string()
    } else {
        exec.stderr.trim().to_string()
    };
    let summary = tail_chars(&summary_text, 500);

    let log_entry = log::RunLogEntry {
        task_id: snapshot.task_id.clone(),
        task_name: snapshot.task_name.clone(),
        project_path: snapshot.project_path.to_string_lossy().to_string(),
        started_at,
        finished_at,
        duration_ms,
        result: result_label,
        exit_code: exec.exit_code,
        timed_out: exec.timed_out,
        timeout_minutes: snapshot.timeout_minutes,
        aborted: exec.aborted,
        prompt: final_prompt,
        stdout: exec.stdout,
        stderr: exec.stderr,
    };

    // 쓰기 실패(권한·디스크)는 실행 자체를 실패로 만들지 않는다 — summary에
    // 덧붙이고 진행한다(설계 §9).
    let (log_path, final_summary) = match log::write_run_log(&snapshot.project_path, &log_entry) {
        Ok(path) => (Some(path), summary),
        Err(e) => (None, format!("{summary} (로그 기록 실패: {e})")),
    };

    // 앱이 종료 중이면 다음 회차를 스케줄하지 않는다(어차피 스케줄러 루프도
    // 곧 멈춘다) — next_run_at을 계속 미래로 미루는 의미 없는 계산을 피한다.
    let next_run_at = if runtime::SHUTTING_DOWN.load(Ordering::Relaxed) {
        None
    } else {
        Some(finished_at + chrono::Duration::minutes(snapshot.interval_minutes as i64))
    };

    runtime::mark_finished(
        &snapshot.key,
        status,
        Some(final_summary),
        duration_ms,
        log_path,
        next_run_at,
    );

    emit_status(&app_handle, &snapshot.key);
}

pub(crate) fn emit_status(app_handle: &tauri::AppHandle, key: &TaskKey) {
    use tauri::Emitter;
    let payload = {
        let map = runtime::RUNTIME.lock().unwrap();
        map.get(key).map(|rt| runtime::to_status(key, rt))
    };
    if let Some(payload) = payload {
        let _ = app_handle.emit("autonomy-task-updated", payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_final_prompt_wraps_with_subagent_delegation_phrase_and_hub_footer() {
        let expected = format!(
            "다음 작업을 malgn-agent:qa-engineer 에이전트에게 위임해 처리하라: 테스트를 실행하라{HUB_RECORD_FOOTER}"
        );
        assert_eq!(
            build_final_prompt(
                "테스트를 실행하라",
                &Some("malgn-agent:qa-engineer".to_string())
            ),
            expected
        );
    }

    #[test]
    fn build_final_prompt_uses_raw_prompt_with_hub_footer_when_no_subagent() {
        let expected = format!("테스트를 실행하라{HUB_RECORD_FOOTER}");
        assert_eq!(build_final_prompt("테스트를 실행하라", &None), expected);
    }

    /// footer는 malgnai-hub 연동 여부와 무관하게 항상 조건부 안내문으로만
    /// 붙는다 — "설정되어 있는 경우"/"연동이 없는 프로젝트라면 무시하라"
    /// 문구가 실제로 포함돼 강제가 아님을 회귀로 고정한다.
    #[test]
    fn hub_record_footer_is_conditional_not_mandatory() {
        assert!(HUB_RECORD_FOOTER.contains("설정되어 있는 경우"));
        assert!(HUB_RECORD_FOOTER.contains("연동이 없는 프로젝트라면"));
        assert!(HUB_RECORD_FOOTER.contains("무시"));
    }

    #[test]
    fn tail_chars_keeps_string_shorter_than_limit_untouched() {
        assert_eq!(tail_chars("짧은 문자열", 500), "짧은 문자열");
    }

    #[test]
    fn tail_chars_keeps_last_n_chars_without_panicking_on_multibyte_boundary() {
        let text: String = std::iter::repeat('가').take(1000).collect();
        let tail = tail_chars(&text, 500);
        assert_eq!(tail.chars().count(), 500);
    }

    /// GUI(.app) 실행 시 PATH가 제한될 수 있다는 `cli_launcher.rs`의 실측
    /// 문제를 이 모듈도 겪지 않는지 회귀로 고정한다 — 이 CI/개발 머신에는
    /// 실제로 claude가 설치돼 있으므로 절대경로 후보든 PATH 폴백이든 반드시
    /// 무언가를 찾아야 한다(둘 다 실패해 None이 나오면 회귀).
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
