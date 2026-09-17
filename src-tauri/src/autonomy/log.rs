// 로그 — 과거 실행의 유일한 정본(설계 §9). 상태를 더 이상 `autonomy.json`에
// 저장하지 않으므로, "지난번에 뭐가 있었는지"는 오직 이 로그 파일로만 알 수 있다.

use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use std::io::BufRead;
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

// ---------------- 과거 실행 이력 조회(읽기 전용 스캔) ----------------
// `autonomy.json`(설정)에는 `history`를 되살리지 않는다는 PM 결정에 따라,
// 과거 실행 이력은 오직 여기 로그 디렉터리를 스캔해서만 얻는다. 이 함수는
// **아무것도 쓰지 않는다** — write_run_log가 이미 만들어 둔 파일을 읽기만
// 한다.

/// 프론트에 camelCase로 내려가는 이력 항목 하나. `AutonomyRuntimeStatus`와
/// 동일한 관용구(`#[serde(rename = ...)]`)를 쓴다.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RunHistoryEntry {
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "finishedAt")]
    pub finished_at: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    /// "success" | "failed" | "timeout" | "aborted"
    pub result: String,
    #[serde(rename = "logPath")]
    pub log_path: String,
}

/// `limit` 미지정 시 기본값. 이 커맨드 하나에서만 쓰는 지역 기본값이라
/// `config.rs`의 "안전 임계값 정본" 목록에는 넣지 않는다(그 목록은 실행
/// 스케줄링 자체를 좌우하는 값들이고, 이건 단순 조회 페이지 크기다).
const DEFAULT_HISTORY_LIMIT: u32 = 20;

/// `<project>/.claude/logs/autonomy/<YYYY-MM-DD>/<safeTaskId>-<HHMMSS>.log`를
/// 스캔해 해당 task의 실행 이력을 최신순으로 반환한다.
///
/// 2단계로 나눈다:
/// 1) 파일을 열지 않고 디렉터리명(날짜)·파일명(시각)만으로 정렬 키를 만들어
///    최신순 정렬 후 `limit`개만 남긴다 — 30일치가 쌓여 있어도 나머지는
///    아예 열지 않는다.
/// 2) 그렇게 골라진 `limit`개만 헤더를 파싱한다. 개별 파일이 손상됐으면
///    그 파일만 건너뛰고 나머지는 정상 반환한다(전체 실패 금지) — 이 설계상
///    손상 파일이 상위 `limit`개 안에 여러 개 섞이면 결과가 `limit`보다
///    적게 나올 수 있다(사용자가 다시 조회하면 스크롤/새로고침으로 자연히
///    드러나는 수준의 트레이드오프로 판단 — 손상 파일을 우회해 그 뒤 파일을
///    추가로 여는 backfill은 하지 않는다. 하지 않으면 "상위 limit개만 연다"는
///    성능 보장이 깨진다).
pub(crate) fn read_task_history(
    project_root: &Path,
    task_id: &str,
    limit: Option<u32>,
) -> Vec<RunHistoryEntry> {
    let limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT).max(1) as usize;
    let safe_id = sanitize_task_id(task_id);
    let file_prefix = format!("{safe_id}-");
    let autonomy_logs_dir = project_root.join(".claude").join("logs").join("autonomy");

    // 디렉터리 자체가 없다 = 한 번도 실행된 적 없는 task(정상 상태) → 빈 배열.
    // `sweep_old_logs`와 동일한 관용구: read_dir 실패는 전부 "이력 없음"으로
    // 취급한다(권한 문제 등도 포함되지만, 화면은 "이력 없음"으로 안전하게
    // 수렴한다 — 에러 배너보다 나은 기본값).
    let Ok(date_entries) = std::fs::read_dir(&autonomy_logs_dir) else {
        return Vec::new();
    };

    let mut candidates: Vec<(chrono::NaiveDate, chrono::NaiveTime, PathBuf)> = Vec::new();
    for date_entry in date_entries.flatten() {
        let date_path = date_entry.path();
        if !date_path.is_dir() {
            continue;
        }
        let Some(dir_name) = date_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `sweep_old_logs`와 동일 — 날짜 형식이 아닌 쓰레기 디렉터리는 조용히 건너뛴다.
        let Ok(dir_date) = chrono::NaiveDate::parse_from_str(dir_name, "%Y-%m-%d") else {
            continue;
        };

        let Ok(file_entries) = std::fs::read_dir(&date_path) else {
            continue;
        };
        for file_entry in file_entries.flatten() {
            let file_path = file_entry.path();
            let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // 이 task의 파일만: `sanitize_task_id`로 얻은 정확한 접두사로
            // 매칭한다(safe_id 안에 '-'가 더 있어도 `strip_prefix`는 정확히
            // 그 문자열을 찾으므로 흔들리지 않는다).
            let Some(rest) = file_name.strip_prefix(file_prefix.as_str()) else {
                continue;
            };
            let Some(time_part) = rest.strip_suffix(".log") else {
                continue;
            };
            let Ok(file_time) = chrono::NaiveTime::parse_from_str(time_part, "%H%M%S") else {
                continue;
            };
            candidates.push((dir_date, file_time, file_path));
        }
    }

    // 최신순(날짜 내림차순 → 같은 날이면 시각 내림차순).
    candidates.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter_map(|(_, _, path)| parse_history_header(&path))
        .collect()
}

/// 로그 파일의 앞부분(헤더 최대 4줄)만 읽어 파싱한다. `read_to_string`으로
/// 전체를 읽지 않는다 — stdout/stderr는 각각 최대 512KB까지 남고 파일 전체가
/// 1MB를 넘을 수 있는데, 목록에 필요한 값은 전부 앞 3~4줄에 있다.
/// `BufReader::lines()`는 줄바꿈을 찾을 때까지만 읽으므로 `.take(4)`로 나머지
/// (prompt/stdout/stderr 본문)는 디스크에서 아예 읽지 않는다.
fn parse_history_header(path: &Path) -> Option<RunHistoryEntry> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let lines: Vec<String> = reader.lines().take(4).filter_map(|l| l.ok()).collect();
    // 최소한 헤더 라벨행 + 데이터행 + result행 3줄은 있어야 한다. 파일이
    // 잘렸거나(쓰던 도중 죽음) 비어 있으면 이 파일만 조용히 건너뛴다.
    if lines.len() < 3 {
        return None;
    }

    // 2행: "task.id / task.name / project / started / finished / durationMs".
    // task.name이나 project 경로 자체가 '/'를 포함할 수 있다(경로는 항상
    // 포함한다) — 앞에서부터 순진하게 split하면 몇 번째 필드가 started인지
    // 어긋난다. 반면 뒤 3개 필드(RFC3339 두 개 + 정수 하나)는 절대 '/'를
    // 포함하지 않는 값이다. 그래서 구분자 " / "(공백-슬래시-공백) 기준으로
    // **오른쪽에서부터** `rsplitn(4, ...)`으로 3개만 떼어낸다 — 앞쪽에 실제
    // 구분자와 같은 패턴(" / ")이 몇 번 더 나오더라도(예: task 이름에 " / "가
    // 들어간 경우) 전부 4번째(마지막) 조각으로 뭉쳐지고, 이 커맨드는 그
    // 앞쪽 뭉치를 애초에 쓰지 않으므로 안전하다.
    let mut rev_fields = lines[1].rsplitn(4, " / ");
    let duration_str = rev_fields.next()?;
    let finished_at = rev_fields.next()?.trim().to_string();
    let started_at = rev_fields.next()?.trim().to_string();
    // rev_fields의 나머지(4번째, id/name/project 뭉치)는 쓰지 않는다.

    let duration_ms: u64 = duration_str.trim().parse().ok()?;

    // 손상 감지: 두 시각 다 유효한 RFC3339여야 정상 로그로 인정한다.
    chrono::DateTime::parse_from_rfc3339(&started_at).ok()?;
    chrono::DateTime::parse_from_rfc3339(&finished_at).ok()?;

    // 3행: "result: <값>     exitCode: <n|none>".
    let result = lines[2]
        .trim_start()
        .strip_prefix("result:")?
        .split_whitespace()
        .next()?
        .to_string();
    if !matches!(result.as_str(), "success" | "failed" | "timeout" | "aborted") {
        return None; // 알 수 없는 값 — 손상으로 간주하고 이 파일만 건너뛴다.
    }

    Some(RunHistoryEntry {
        started_at,
        finished_at,
        duration_ms,
        result,
        log_path: path.to_string_lossy().to_string(),
    })
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

    fn history_entry(task_id: &str, minutes_ago: i64, duration_ms: u64, result: &'static str) -> RunLogEntry {
        let started_at = Utc::now() - chrono::Duration::minutes(minutes_ago);
        RunLogEntry {
            task_id: task_id.to_string(),
            task_name: "이력 테스트 / 하위".to_string(), // task 이름 자체에 " / "가 섞여도
            // rsplitn(4, " / ") 파싱이 흔들리지 않는지 회귀 고정.
            project_path: "/Users/tester/workspace/demo".to_string(),
            started_at,
            finished_at: started_at + chrono::Duration::milliseconds(duration_ms as i64),
            duration_ms,
            result,
            exit_code: Some(0),
            timed_out: false,
            timeout_minutes: 60,
            aborted: false,
            prompt: "프롬프트".to_string(),
            stdout: "출력".to_string(),
            stderr: String::new(),
        }
    }

    // 왕복 검증(관례 필수) — write_run_log로 실제 로그 파일을 여러 개 써 놓고,
    // read_task_history가 그것을 최신순으로 다시 읽어내는지 확인한다. 포맷이
    // 나중에 바뀌면(예: 구분자 변경) 이 테스트가 깨져서 알려준다.
    #[test]
    fn read_task_history_roundtrip_returns_newest_first() {
        let dir = temp_subdir("history-roundtrip");
        // 가장 오래된 것부터 써도 결과는 최신순으로 정렬돼야 한다.
        write_run_log(&dir, &history_entry("t-hist", 4, 100, "timeout")).unwrap();
        write_run_log(&dir, &history_entry("t-hist", 2, 200, "failed")).unwrap();
        write_run_log(&dir, &history_entry("t-hist", 0, 300, "success")).unwrap();

        let history = read_task_history(&dir, "t-hist", None);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].result, "success"); // 가장 최근(0분 전)
        assert_eq!(history[1].result, "failed");
        assert_eq!(history[2].result, "timeout"); // 가장 오래됨(4분 전)
        assert_eq!(history[0].duration_ms, 300);
        assert!(Path::new(&history[0].log_path).is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // limit 동작 — 기본값(20)이 아닌 명시적 limit이 상위 N개만 돌려주는지.
    #[test]
    fn read_task_history_respects_limit() {
        let dir = temp_subdir("history-limit");
        write_run_log(&dir, &history_entry("t-hist", 4, 100, "timeout")).unwrap();
        write_run_log(&dir, &history_entry("t-hist", 2, 200, "failed")).unwrap();
        write_run_log(&dir, &history_entry("t-hist", 0, 300, "success")).unwrap();

        let history = read_task_history(&dir, "t-hist", Some(2));
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].result, "success");
        assert_eq!(history[1].result, "failed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 비정상 케이스 1 — 로그 디렉터리 자체가 없다(한 번도 실행된 적 없는
    // task). 화면이 "이력 없음"을 정상 상태로 보여줘야 하므로 에러가 아니라
    // 빈 배열이어야 한다.
    #[test]
    fn read_task_history_returns_empty_vec_when_logs_dir_missing() {
        let dir = temp_subdir("history-missing-dir");
        let history = read_task_history(&dir, "no-such-task", None);
        assert!(history.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 비정상 케이스 2 — 날짜 디렉터리 이름이 YYYY-MM-DD로 파싱되지 않는
    // 쓰레기 디렉터리가 섞여 있어도 무시하고 나머지는 정상 반환한다.
    #[test]
    fn read_task_history_ignores_non_date_directories() {
        let dir = temp_subdir("history-garbage-dir");
        write_run_log(&dir, &history_entry("t-hist", 0, 300, "success")).unwrap();

        let garbage_dir = dir
            .join(".claude")
            .join("logs")
            .join("autonomy")
            .join("not-a-date");
        std::fs::create_dir_all(&garbage_dir).unwrap();
        std::fs::write(garbage_dir.join("t-hist-999999.log"), "쓰레기").unwrap();

        let history = read_task_history(&dir, "t-hist", None);
        assert_eq!(history.len(), 1, "쓰레기 디렉터리는 무시하고 정상 로그만 반환해야 한다");
        assert_eq!(history[0].result, "success");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 비정상 케이스 3 — 로그 파일 1건이 손상됐다(헤더가 깨짐). 그 1건만
    // 건너뛰고 전체 조회는 실패하지 않아야 한다.
    #[test]
    fn read_task_history_skips_single_corrupted_file_without_failing_whole_query() {
        let dir = temp_subdir("history-corrupted");
        write_run_log(&dir, &history_entry("t-hist", 2, 200, "failed")).unwrap();
        let good_path = write_run_log(&dir, &history_entry("t-hist", 0, 300, "success")).unwrap();

        // 정상 파일 중 하나를 손상시킨다(헤더 3줄 미만으로 잘림).
        std::fs::write(&good_path, "task.id / task.name / project / started / finished / durationMs\n").unwrap();

        let history = read_task_history(&dir, "t-hist", None);
        assert_eq!(history.len(), 1, "손상된 1건은 빠지고 나머지 1건만 반환해야 한다");
        assert_eq!(history[0].result, "failed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 비정상 케이스 4 — task_id에 경로 순회 문자가 들어와도 sanitize_task_id를
    // 거치므로 다른 task의 로그를 넘보지 않고, 매칭되는 파일이 없으면 빈
    // 배열을 반환한다(에러가 아니다).
    #[test]
    fn read_task_history_with_path_traversal_task_id_returns_empty_and_does_not_panic() {
        let dir = temp_subdir("history-traversal");
        write_run_log(&dir, &history_entry("t-hist", 0, 300, "success")).unwrap();

        let history = read_task_history(&dir, "../../../etc/passwd", None);
        assert!(
            history.is_empty(),
            "sanitize된 접두사와 일치하는 파일이 없으므로 빈 배열이어야 한다"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
