// 로그 — 과거 실행의 유일한 정본(설계 §9). 상태를 더 이상 `autonomy.json`에
// 저장하지 않으므로, "지난번에 뭐가 있었는지"는 오직 이 로그 파일로만 알 수 있다.

use chrono::{DateTime, Local, Utc};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 각 스트림(stdout/stderr) 최대 보관 크기 — 초과분은 꼬리만 남기고 절단한다.
const MAX_STREAM_BYTES: usize = 512 * 1024;

/// `[A-Za-z0-9._-]` 외 문자를 `_`로 치환하고 40자로 자른다. **필수 방어** —
/// task id는 손으로 편집 가능한 JSON에서 오고 그대로 로그 경로에 들어가므로,
/// `../../..` 같은 값이 들어오면 프로젝트 밖에 파일을 쓰게 된다.
pub(crate) fn sanitize_task_id(task_id: &str) -> String {
    let sanitized: String = task_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let truncated: String = sanitized.chars().take(40).collect();
    if truncated.is_empty() {
        "task".to_string()
    } else {
        truncated
    }
}

/// `<project>/.claude/logs/autonomy/YYYY-MM-DD/<safeTaskId>-HHMMSS.log` —
/// 날짜/시각은 로컬 시간(사용자가 자기 날짜로 찾는다).
pub(crate) fn log_path_for(
    project_root: &Path,
    task_id: &str,
    started_at_local: DateTime<Local>,
) -> PathBuf {
    let date_dir = started_at_local.format("%Y-%m-%d").to_string();
    let time_part = started_at_local.format("%H%M%S").to_string();
    let safe_id = sanitize_task_id(task_id);
    project_root
        .join(".claude")
        .join("logs")
        .join("autonomy")
        .join(date_dir)
        .join(format!("{safe_id}-{time_part}.log"))
}

/// UTF-8 문자 경계를 존중하며 뒤에서부터 최대 `max_chars`자만 남긴다.
/// `runner::tail_chars`와 동일한 알고리즘을 여기서도 재사용한다(스트림 512KB
/// 절단에 재사용 — 설계 지시).
pub(crate) fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        text.to_string()
    } else {
        text.chars().skip(total - max_chars).collect()
    }
}

/// 512KB(바이트) 초과 시 꼬리를 남기고 절단 표시를 삽입한다. 바이트 상한을
/// "문자 수"로 그대로 쓸 수는 없으므로(멀티바이트), 보수적으로 같은 문자
/// 수만큼만 `tail_chars`로 잘라 UTF-8 경계를 절대 깨지 않는다.
fn truncate_stream(label: &str, text: &str) -> String {
    if text.len() <= MAX_STREAM_BYTES {
        return text.to_string();
    }
    let tail = tail_chars(text, MAX_STREAM_BYTES);
    format!(
        "...[{label} 절단됨: 원본 {}바이트 중 마지막 일부만 표시]...\n{}",
        text.len(),
        tail
    )
}

pub(crate) struct RunLogEntry {
    pub task_id: String,
    pub task_name: String,
    pub project_path: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u64,
    /// "success" | "failed" | "timeout" | "aborted"
    pub result: &'static str,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub timeout_minutes: u32,
    /// 앱 종료로 인해 중단된 경우(§4 — `should_abort` 경로).
    pub aborted: bool,
    pub prompt: String,
    pub stdout: String,
    pub stderr: String,
}

/// 로그 디렉터리(`.claude/logs`) 자체를 git에서 격리한다 — 로그에는 프롬프트
/// 전문과 `claude` 출력이 평문으로 남는데, 지정된 위치가 커밋 대상인
/// `.claude/` 안이기 때문이다. 이미 있으면(사용자가 지웠어도) 절대 덮어쓰지
/// 않는다. 사용자 프로젝트의 루트 `.gitignore`는 절대 수정하지 않는다.
pub(crate) fn ensure_logs_gitignore(logs_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(logs_dir)?;
    let gitignore_path = logs_dir.join(".gitignore");
    if gitignore_path.exists() {
        return Ok(());
    }
    std::fs::write(&gitignore_path, "*\n")
}

pub(crate) fn write_run_log(project_root: &Path, entry: &RunLogEntry) -> Result<String, String> {
    let logs_root = project_root.join(".claude").join("logs");
    ensure_logs_gitignore(&logs_root).map_err(|e| format!("로그 디렉터리 격리 실패: {e}"))?;

    let started_local = entry.started_at.with_timezone(&Local);
    let path = log_path_for(project_root, &entry.task_id, started_local);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("로그 디렉터리를 만들지 못했습니다: {e}"))?;
    }

    let content = format!(
        "task.id / task.name / project / started / finished / durationMs\n\
         {} / {} / {} / {} / {} / {}\n\
         result: {}     exitCode: {}\n\
         timedOut: {}    timeoutMinutes: {}    aborted(앱 종료): {}\n\
         --- prompt ---\n{}\n\
         --- stdout ---\n{}\n\
         --- stderr ---\n{}\n",
        entry.task_id,
        entry.task_name,
        entry.project_path,
        entry.started_at.to_rfc3339(),
        entry.finished_at.to_rfc3339(),
        entry.duration_ms,
        entry.result,
        entry
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "none".to_string()),
        entry.timed_out,
        entry.timeout_minutes,
        entry.aborted,
        entry.prompt,
        truncate_stream("stdout", &entry.stdout),
        truncate_stream("stderr", &entry.stderr),
    );

    std::fs::write(&path, content).map_err(|e| format!("로그 파일을 쓰지 못했습니다: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

/// 보존 스윕 — 대상은 이름이 `YYYY-MM-DD`로 파싱되는 디렉터리만(파싱되지 않는
/// 이름은 절대 지우지 않는다).
pub(crate) fn sweep_old_logs(project_root: &Path, retention_days: u32) {
    let autonomy_logs_dir = project_root.join(".claude").join("logs").join("autonomy");
    let Ok(entries) = std::fs::read_dir(&autonomy_logs_dir) else {
        return;
    };
    let cutoff = Local::now().date_naive() - chrono::Duration::days(retention_days as i64);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(dir_date) = chrono::NaiveDate::parse_from_str(name, "%Y-%m-%d") else {
            continue;
        };
        if dir_date < cutoff {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// 스윕 시점 = 앱 시작 후 첫 tick(스케줄러 루프는 시작하자마자 첫 `tick()`을
/// 돈다) + 이후 로컬 날짜가 바뀌는 첫 tick. 별도 백그라운드 스레드를 두지
/// 않고 이 함수 하나로 두 요구를 만족시킨다 — 최초 호출(`last=None`)이 곧
/// "앱 시작 후 1회"다.
static LAST_SWEEP_DATE: Mutex<Option<chrono::NaiveDate>> = Mutex::new(None);

pub(crate) fn maybe_sweep_logs(roots: &[PathBuf], retention_days: u32) {
    let today = Local::now().date_naive();
    {
        let mut last = LAST_SWEEP_DATE.lock().unwrap();
        if *last == Some(today) {
            return;
        }
        *last = Some(today);
    }
    for root in roots {
        sweep_projects_under(root, retention_days);
    }
}

fn sweep_projects_under(workspace_root: &Path, retention_days: u32) {
    let Ok(entries) = std::fs::read_dir(workspace_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sweep_old_logs(&path, retention_days);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-autonomy-log-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir
    }

    // 신규(최소) — sanitize_task_id가 경로 구분자(`/`)를 제거해 `../` 시퀀스가
    // 더 이상 디렉터리를 가로지를 수 없게 만드는지. `.`은 허용 문자라 남지만,
    // `/`가 전부 `_`로 바뀌므로 결과는 하나의 평평한 파일명일 뿐이다.
    #[test]
    fn sanitize_task_id_strips_path_traversal_sequences() {
        let sanitized = sanitize_task_id("../../../etc/passwd");
        assert!(
            !sanitized.contains('/'),
            "경로 구분자가 남아 있으면 상위 디렉터리로 빠져나갈 수 있다"
        );
    }

    #[test]
    fn sanitize_task_id_truncates_to_forty_chars() {
        let long_id: String = std::iter::repeat('a').take(100).collect();
        let sanitized = sanitize_task_id(&long_id);
        assert_eq!(sanitized.chars().count(), 40);
    }

    #[test]
    fn sanitize_task_id_falls_back_to_task_when_input_is_empty() {
        let sanitized = sanitize_task_id("");
        assert_eq!(sanitized, "task", "빈 입력은 'task'로 폴백해야 한다");
    }

    #[test]
    fn sanitize_task_id_replaces_slash_with_underscore() {
        let sanitized = sanitize_task_id("../../../etc/passwd");
        assert_eq!(sanitized, ".._.._.._etc_passwd");
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

    // 신규(최소) — ensure_logs_gitignore가 기존 파일을 덮어쓰지 않는지.
    #[test]
    fn ensure_logs_gitignore_does_not_overwrite_existing_file() {
        let dir = temp_subdir("gitignore");
        ensure_logs_gitignore(&dir).unwrap();
        let gitignore_path = dir.join(".gitignore");
        std::fs::write(&gitignore_path, "custom content\n").unwrap();

        ensure_logs_gitignore(&dir).unwrap();

        let content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert_eq!(content, "custom content\n", "기존 파일을 덮어쓰면 안 된다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_run_log_creates_file_and_gitignore() {
        let dir = temp_subdir("write-log");
        let entry = RunLogEntry {
            task_id: "t-1".to_string(),
            task_name: "테스트".to_string(),
            project_path: dir.to_string_lossy().to_string(),
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_ms: 100,
            result: "success",
            exit_code: Some(0),
            timed_out: false,
            timeout_minutes: 60,
            aborted: false,
            prompt: "테스트 프롬프트".to_string(),
            stdout: "출력".to_string(),
            stderr: String::new(),
        };
        let log_path = write_run_log(&dir, &entry).expect("로그 쓰기에 성공해야 합니다");
        assert!(Path::new(&log_path).is_file());
        assert!(dir.join(".claude").join("logs").join(".gitignore").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
