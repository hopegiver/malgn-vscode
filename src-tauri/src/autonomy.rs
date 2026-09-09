// ---------------- 자율업무(무인 실행) ----------------
// 사용자가 등록한 프롬프트를 정해진 주기마다 특정 프로젝트 디렉터리에서
// `claude -p`로 무인 실행한다. 설정은 프로젝트별 `<project_root>/.claude/autonomy.json`
// 하나에만 저장한다 — 중앙 DB 없음. 실행은 이 앱이 켜져 있는 동안(`run()`의
// `setup()`에서 띄운 백그라운드 스레드)만 돈다. OS 스케줄러/launchd 연동은
// 의도적으로 범위 밖이다(MVP 경계 — 추가하지 않는다).
//
// 프로세스 실행 불변식(`lib.rs`의 `run_claude_command`와 동일한 계약):
// `std::process::Command`로 셸을 거치지 않고 직접 실행한다(바이너리 경로
// 해석은 `cli_launcher::resolve_binary_expand_home`을 쓰지만, 그 함수도
// 내부적으로 `Command::new`만 쓰지 셸을 스폰하지 않는다). 이 기능은 사용자가
// 자유 텍스트로 입력한 프롬프트를 인자로 그대로 넘기는 첫 사례이지만,
// `Command::new`는 셸 파싱을 거치지 않는다 — 각 인자가 셸 메타문자 해석
// 없이 그대로 하나의 argv 원소로 자식 프로세스에 전달되므로 셸 인젝션
// 경로가 없다. `--dangerously-skip-permissions`나 그 어떤 권한 우회
// 플래그도 추가하지 않는다 — 무인 실행도 해당 프로젝트의 기존
// `.claude/settings.json` 권한 설정 범위 안에서만 동작해야 한다는 안전
// 경계를 이 기능이 넓혀서는 안 된다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MIN_INTERVAL_MINUTES: u32 = 5;
const MAX_HISTORY_ENTRIES: usize = 10;
const SCHEDULER_TICK_SECS: u64 = 60;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AutonomyHistoryEntry {
    pub at: String,
    /// "success" | "failed"
    pub result: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AutonomyTaskConfig {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub subagent: Option<String>,
    #[serde(rename = "intervalMinutes")]
    pub interval_minutes: u32,
    pub enabled: bool,
    #[serde(rename = "preventOverlap")]
    pub prevent_overlap: bool,
    #[serde(rename = "lastRunAt", default)]
    pub last_run_at: Option<String>,
    /// "success" | "failed" | "running" | null
    #[serde(rename = "lastStatus", default)]
    pub last_status: Option<String>,
    #[serde(rename = "lastSummary", default)]
    pub last_summary: Option<String>,
    #[serde(default)]
    pub history: Vec<AutonomyHistoryEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct AutonomyFile {
    version: u32,
    tasks: Vec<AutonomyTaskConfig>,
}

impl Default for AutonomyFile {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct AutonomyProjectTasks {
    #[serde(rename = "projectPath")]
    pub project_path: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
    pub tasks: Vec<AutonomyTaskConfig>,
}

/// 동시 파일 쓰기 보호 — `lib.rs`의 `HISTORICAL_USAGE_CACHE`와 같은 전역
/// `Mutex` 패턴. 프론트엔드 커맨드 호출과 백그라운드 스케줄러 스레드가 같은
/// `autonomy.json`을 동시에 읽고 쓸 수 있어, 파일 하나 단위가 아니라 이
/// 모듈이 하는 모든 read-modify-write 구간 전체를 하나의 락으로 직렬화한다
/// (프로젝트 수·쓰기 빈도가 낮아 세밀한 락 대신 이 굵은 락으로 충분하다).
static AUTONOMY_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn autonomy_file_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude").join("autonomy.json")
}

fn read_autonomy_file(project_root: &Path) -> AutonomyFile {
    let path = autonomy_file_path(project_root);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return AutonomyFile::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_autonomy_file(project_root: &Path, file: &AutonomyFile) -> Result<(), String> {
    let path = autonomy_file_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    }
    let content = serde_json::to_string_pretty(file)
        .map_err(|e| format!("설정을 직렬화하지 못했습니다: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("설정을 저장하지 못했습니다: {e}"))
}

/// id가 이미 있으면 업데이트, 없으면 새로 추가한다. interval은 최소 5분으로
/// clamp하고, history는 최대 10개로 자른다 — 저장 경로(커맨드)와 스케줄러
/// 경로 양쪽에서 이 규칙이 항상 지켜지도록 여기 한 곳에 모아둔다.
fn upsert_task(file: &mut AutonomyFile, mut task: AutonomyTaskConfig) {
    if task.interval_minutes < MIN_INTERVAL_MINUTES {
        task.interval_minutes = MIN_INTERVAL_MINUTES;
    }
    if task.history.len() > MAX_HISTORY_ENTRIES {
        task.history.truncate(MAX_HISTORY_ENTRIES);
    }
    match file.tasks.iter_mut().find(|t| t.id == task.id) {
        Some(existing) => *existing = task,
        None => file.tasks.push(task),
    }
}

/// `lastRunAt`(RFC3339)부터 지금까지 경과한 분(分). 타임스탬프가 손상돼
/// 파싱할 수 없으면 `None` — 호출자(`is_due`)는 이 경우 "실행 대상"으로
/// 보수적으로 취급한다(손상된 데이터 때문에 작업이 영원히 멈추는 것을
/// 막는다).
fn minutes_elapsed_since(last_run_at: &str) -> Option<i64> {
    let dt = DateTime::parse_from_rfc3339(last_run_at)
        .ok()?
        .with_timezone(&Utc);
    Some((Utc::now() - dt).num_minutes())
}

/// 지금 이 task를 실행해야 하는가 — WBS에 명시된 판정식 그대로:
/// `enabled == true` && (`lastRunAt == null` || 경과분 >= `intervalMinutes`)
/// && (`preventOverlap == false` || `lastStatus != 'running'`).
fn is_due(task: &AutonomyTaskConfig) -> bool {
    if !task.enabled {
        return false;
    }
    if task.prevent_overlap && task.last_status.as_deref() == Some("running") {
        return false;
    }
    match &task.last_run_at {
        None => true,
        Some(last) => minutes_elapsed_since(last)
            .map(|elapsed| elapsed >= task.interval_minutes as i64)
            .unwrap_or(true),
    }
}

/// UTF-8 문자 경계를 존중하며 뒤에서부터 최대 `max_chars`자만 남긴다(바이트
/// 슬라이싱은 멀티바이트 문자 중간을 잘라 panic할 수 있어 쓰지 않는다).
fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        text.to_string()
    } else {
        text.chars().skip(total - max_chars).collect()
    }
}

fn now_utc_iso() -> String {
    Utc::now().to_rfc3339()
}

// ---------------- Tauri 커맨드 ----------------

/// `~/workspace/*` 를 훑어 `.claude/autonomy.json`이 있는 프로젝트만 모아
/// 반환한다. 파일이 없는 프로젝트는 목록에서 제외한다(빈 배열을 넣지 않는다).
#[tauri::command]
pub fn autonomy_list() -> Vec<AutonomyProjectTasks> {
    let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut results = Vec::new();
    for workspace_root in crate::workspace_roots() {
        let Ok(entries) = std::fs::read_dir(&workspace_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let project_path = entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let Some(name) = project_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if !autonomy_file_path(&project_path).is_file() {
                continue;
            }

            let file = read_autonomy_file(&project_path);
            results.push(AutonomyProjectTasks {
                project_path: project_path.to_string_lossy().to_string(),
                project_name: name.to_string(),
                tasks: file.tasks,
            });
        }
    }
    results.sort_by(|a, b| a.project_name.cmp(&b.project_name));
    results
}

/// upsert. `project_path`는 반드시 `resolve_validated_project_root`로 검증한다
/// (`~/workspace` 바로 아래 프로젝트인지 — 경로 탈출 방지). `.claude/` 디렉터리
/// ·파일이 없으면 새로 만든다.
#[tauri::command]
pub fn autonomy_save_task(project_path: String, task: AutonomyTaskConfig) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = read_autonomy_file(&root);
    upsert_task(&mut file, task);
    write_autonomy_file(&root, &file)
}

#[tauri::command]
pub fn autonomy_delete_task(project_path: String, task_id: String) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = read_autonomy_file(&root);
    file.tasks.retain(|t| t.id != task_id);
    write_autonomy_file(&root, &file)
}

/// 토글 전용 경량 커맨드 — 리스트 전체를 다시 읽지 않고 즉시 반영한다.
#[tauri::command]
pub fn autonomy_set_enabled(
    project_path: String,
    task_id: String,
    enabled: bool,
) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = read_autonomy_file(&root);
    let Some(task) = file.tasks.iter_mut().find(|t| t.id == task_id) else {
        return Err("해당 자율업무를 찾을 수 없습니다.".to_string());
    };
    task.enabled = enabled;
    write_autonomy_file(&root, &file)
}

// ---------------- 백그라운드 스케줄러 ----------------

/// 실행 대상으로 확정된 task 하나의 스냅숏 — "running으로 마킹" 시점의
/// prompt/subagent를 그대로 들고 다닌다. 실제 `claude -p` 실행(수초~수분)
/// 도중 사용자가 같은 task를 편집해도 이번 실행은 마킹 시점 값으로 끝까지
/// 진행한다(도중에 파일을 다시 읽어 뒤섞인 상태로 실행하지 않는다).
struct DueTaskSnapshot {
    id: String,
    prompt: String,
    subagent: Option<String>,
}

/// 60초 tick마다 전체 워크스페이스를 훑어 due한 task를 찾아 실행한다.
/// 이 함수 자체는 파일 스캔 + "running 마킹"만 하고, 실제 `claude -p` 실행은
/// `run_task_and_record`에 위임한다 — 마킹(락 보유)과 실행(락 미보유, 수초
/// ~수분 블로킹)을 분리해 락을 오래 쥐지 않는다.
fn scheduler_tick(app_handle: &tauri::AppHandle) {
    for workspace_root in crate::workspace_roots() {
        let Ok(entries) = std::fs::read_dir(&workspace_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let project_path = entry.path();
            if !project_path.is_dir() {
                continue;
            }
            process_project_due_tasks(&project_path, app_handle);
        }
    }
}

fn process_project_due_tasks(project_path: &Path, app_handle: &tauri::AppHandle) {
    if !autonomy_file_path(project_path).is_file() {
        return;
    }

    let due_snapshots: Vec<DueTaskSnapshot> = {
        let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
        let mut file = read_autonomy_file(project_path);
        let mut snapshots = Vec::new();
        for task in file.tasks.iter_mut() {
            if is_due(task) {
                // 중복 트리거 방지 — 즉시 running으로 바꿔 저장한다(같은 tick
                // 안에서 이 project를 다시 스캔할 일은 없지만, 다음 tick이나
                // 동시 커맨드 호출이 같은 task를 또 due로 보지 않게 한다).
                task.last_status = Some("running".to_string());
                snapshots.push(DueTaskSnapshot {
                    id: task.id.clone(),
                    prompt: task.prompt.clone(),
                    subagent: task.subagent.clone(),
                });
            }
        }
        if !snapshots.is_empty() {
            let _ = write_autonomy_file(project_path, &file);
        }
        snapshots
    };

    for snapshot in due_snapshots {
        run_task_and_record(project_path, snapshot, app_handle);
    }
}

/// 모든 자율업무 실행 프롬프트 끝에 붙는 안내 footer. malgnai-hub 연동 여부는
/// 프로젝트마다 다르므로(구 `project_autonomy_*` 계열은 폐기돼 이 규율과
/// 무관하다) Rust 쪽에서 project_id를 알아내거나 연동 여부를 판단하려 하지
/// 않는다 — 그 판단은 해당 프로젝트의 CLAUDE.md/STATUS.md를 이미 읽고 있는
/// `claude` 세션에게 조건부로 위임한다(연동 없는 프로젝트에서는 이 문구를
/// 그냥 무시하면 된다).
const HUB_RECORD_FOOTER: &str = "\n\n작업을 마치면 이 프로젝트에 malgnai-hub MCP 연동(CLAUDE.md에 명시된 규율)이 설정되어 있는 경우 그 규율에 따라 work_record 등으로 결과를 기록하라. 연동이 없는 프로젝트라면 이 지시는 무시하라.";

fn build_final_prompt(snapshot: &DueTaskSnapshot) -> String {
    let base = match &snapshot.subagent {
        Some(agent) if !agent.trim().is_empty() => {
            format!(
                "다음 작업을 {agent} 에이전트에게 위임해 처리하라: {}",
                snapshot.prompt
            )
        }
        _ => snapshot.prompt.clone(),
    };
    format!("{base}{HUB_RECORD_FOOTER}")
}

/// `dev_tools.rs`의 Claude 도구 정의(`DEV_TOOLS`)와 값은 같지만, 그 파일이
/// 이미 정한 관례(결정 5.2 — 상수를 공유하지 않고 각자 별도로 둔다, 회귀
/// 위험 0)를 그대로 따라 이 모듈에도 독립적으로 둔다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

/// `claude -p <최종프롬프트>`를 `current_dir(project_path)`로 동기 실행한다
/// (백그라운드 스레드 안이므로 블로킹 무방). 종료 후 lastRunAt/lastStatus/
/// lastSummary/history를 갱신해 저장하고, 있으면 `autonomy-task-updated`
/// 이벤트를 emit한다.
///
/// 바이너리 해석은 bare name `Command::new("claude")`에 의존하지 않는다 —
/// `cli_launcher.rs` 상단에 문서화된 실측 제약대로, Finder로 띄운 `.app`은
/// launchctl 기본 PATH만 상속해 `/opt/homebrew/bin` 등이 비어 있을 수 있다
/// (`gh`/`wrangler` 연동에서 이미 확인된 문제 — 이 기능이 처음 겪는 문제가
/// 아니다). 그래서 이미 검증된 `resolve_binary_expand_home`(절대경로 후보
/// 우선 → 실패 시 PATH의 bare name 1회 시도)으로 실행 파일을 찾고,
/// `dev_tools::build_child_path_env`로 자식 프로세스의 PATH도 명시적으로
/// 넓힌다(claude 내부에서 다시 git/node 등을 셔뱅으로 부를 수 있어서다).
/// HOME 등 나머지 환경변수는 그대로 상속한다(PATH만 덮어쓴다).
fn run_task_and_record(project_path: &Path, snapshot: DueTaskSnapshot, app_handle: &tauri::AppHandle) {
    let final_prompt = build_final_prompt(&snapshot);

    let resolved_claude =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude");

    let (success, summary) = match &resolved_claude {
        None => (
            false,
            "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                .to_string(),
        ),
        Some(claude_path) => {
            let path_env = crate::dev_tools::build_child_path_env(Some(claude_path));
            let output = std::process::Command::new(claude_path)
                .args(["-p", &final_prompt])
                .current_dir(project_path)
                .env("PATH", &path_env)
                .output();

            match &output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let success = out.status.success();
                    let text = if success || stderr.trim().is_empty() {
                        stdout.trim()
                    } else {
                        stderr.trim()
                    };
                    (success, tail_chars(text, 500))
                }
                Err(e) => (false, format!("claude 명령을 실행하지 못했습니다: {e}")),
            }
        }
    };

    {
        let _guard = AUTONOMY_FILE_LOCK.lock().unwrap();
        let mut file = read_autonomy_file(project_path);
        if let Some(task) = file.tasks.iter_mut().find(|t| t.id == snapshot.id) {
            let now = now_utc_iso();
            let result = if success { "success" } else { "failed" }.to_string();
            task.last_run_at = Some(now.clone());
            task.last_status = Some(result.clone());
            task.last_summary = Some(summary);
            task.history.insert(0, AutonomyHistoryEntry { at: now, result });
            if task.history.len() > MAX_HISTORY_ENTRIES {
                task.history.truncate(MAX_HISTORY_ENTRIES);
            }
        }
        let _ = write_autonomy_file(project_path, &file);
    }

    use tauri::Emitter;
    let _ = app_handle.emit("autonomy-task-updated", ());
}

/// `run()`의 `setup()`에서 한 번 호출한다 — `watch_claude_sessions_dir`와 같은
/// "스레드 하나 띄우고 앱 수명 내내 산다" 패턴.
pub fn spawn_scheduler(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        scheduler_tick(&app_handle);
        std::thread::sleep(std::time::Duration::from_secs(SCHEDULER_TICK_SECS));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(id: &str) -> AutonomyTaskConfig {
        AutonomyTaskConfig {
            id: id.to_string(),
            name: "야간 빌드 점검".to_string(),
            prompt: "빌드 로그를 확인하고 실패 원인을 요약하라".to_string(),
            subagent: Some("malgn-agent:qa-engineer".to_string()),
            interval_minutes: 30,
            enabled: true,
            prevent_overlap: true,
            last_run_at: None,
            last_status: None,
            last_summary: None,
            history: Vec::new(),
        }
    }

    #[test]
    fn task_json_roundtrip_uses_documented_camel_case_field_names() {
        let task = sample_task("task-1");
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"intervalMinutes\""));
        assert!(json.contains("\"preventOverlap\""));
        assert!(json.contains("\"lastRunAt\""));
        assert!(json.contains("\"lastStatus\""));
        assert!(json.contains("\"lastSummary\""));

        let roundtripped: AutonomyTaskConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped, task);
    }

    #[test]
    fn task_deserializes_with_only_required_fields_missing_optionals_default() {
        let json = r#"{
            "id": "task-2",
            "name": "이름",
            "prompt": "프롬프트",
            "intervalMinutes": 10,
            "enabled": true,
            "preventOverlap": false
        }"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(task.subagent, None);
        assert_eq!(task.last_run_at, None);
        assert_eq!(task.last_status, None);
        assert_eq!(task.last_summary, None);
        assert!(task.history.is_empty());
    }

    #[test]
    fn autonomy_file_roundtrip_preserves_tasks() {
        let file = AutonomyFile {
            version: 1,
            tasks: vec![sample_task("a"), sample_task("b")],
        };
        let json = serde_json::to_string_pretty(&file).unwrap();
        let parsed: AutonomyFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].id, "a");
        assert_eq!(parsed.tasks[1].id, "b");
    }

    #[test]
    fn upsert_adds_new_task_when_id_not_found() {
        let mut file = AutonomyFile::default();
        upsert_task(&mut file, sample_task("new-1"));
        assert_eq!(file.tasks.len(), 1);
        assert_eq!(file.tasks[0].id, "new-1");
    }

    #[test]
    fn upsert_updates_existing_task_in_place_without_duplicating() {
        let mut file = AutonomyFile::default();
        upsert_task(&mut file, sample_task("dup"));

        let mut updated = sample_task("dup");
        updated.name = "이름이 바뀐 작업".to_string();
        updated.enabled = false;
        upsert_task(&mut file, updated);

        assert_eq!(file.tasks.len(), 1, "같은 id는 새로 추가되지 않고 갱신되어야 한다");
        assert_eq!(file.tasks[0].name, "이름이 바뀐 작업");
        assert!(!file.tasks[0].enabled);
    }

    #[test]
    fn upsert_clamps_interval_below_minimum_to_five() {
        let mut file = AutonomyFile::default();
        let mut task = sample_task("short-interval");
        task.interval_minutes = 1;
        upsert_task(&mut file, task);
        assert_eq!(file.tasks[0].interval_minutes, MIN_INTERVAL_MINUTES);
    }

    #[test]
    fn upsert_truncates_history_to_max_entries() {
        let mut file = AutonomyFile::default();
        let mut task = sample_task("long-history");
        task.history = (0..15)
            .map(|i| AutonomyHistoryEntry {
                at: format!("2026-01-{:02}T00:00:00Z", i + 1),
                result: "success".to_string(),
            })
            .collect();
        upsert_task(&mut file, task);
        assert_eq!(file.tasks[0].history.len(), MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn is_due_true_when_never_run_and_enabled() {
        let task = sample_task("never-run");
        assert!(is_due(&task));
    }

    #[test]
    fn is_due_false_when_disabled_even_if_interval_elapsed() {
        let mut task = sample_task("disabled");
        task.enabled = false;
        task.last_run_at = Some((Utc::now() - chrono::Duration::hours(1)).to_rfc3339());
        assert!(!is_due(&task));
    }

    #[test]
    fn is_due_false_when_interval_not_yet_elapsed() {
        let mut task = sample_task("recent");
        task.interval_minutes = 30;
        task.last_run_at = Some((Utc::now() - chrono::Duration::minutes(5)).to_rfc3339());
        assert!(!is_due(&task));
    }

    #[test]
    fn is_due_true_when_interval_elapsed() {
        let mut task = sample_task("elapsed");
        task.interval_minutes = 30;
        task.last_run_at = Some((Utc::now() - chrono::Duration::minutes(45)).to_rfc3339());
        assert!(is_due(&task));
    }

    #[test]
    fn is_due_false_when_prevent_overlap_and_currently_running() {
        let mut task = sample_task("overlapping");
        task.prevent_overlap = true;
        task.last_status = Some("running".to_string());
        task.last_run_at = Some((Utc::now() - chrono::Duration::hours(2)).to_rfc3339());
        assert!(!is_due(&task));
    }

    #[test]
    fn is_due_true_when_overlap_allowed_even_while_running() {
        let mut task = sample_task("overlap-allowed");
        task.prevent_overlap = false;
        task.last_status = Some("running".to_string());
        task.last_run_at = Some((Utc::now() - chrono::Duration::hours(2)).to_rfc3339());
        assert!(is_due(&task));
    }

    #[test]
    fn build_final_prompt_wraps_with_subagent_delegation_phrase_and_hub_footer() {
        let snapshot = DueTaskSnapshot {
            id: "x".to_string(),
            prompt: "테스트를 실행하라".to_string(),
            subagent: Some("malgn-agent:qa-engineer".to_string()),
        };
        let expected = format!(
            "다음 작업을 malgn-agent:qa-engineer 에이전트에게 위임해 처리하라: 테스트를 실행하라{HUB_RECORD_FOOTER}"
        );
        assert_eq!(build_final_prompt(&snapshot), expected);
    }

    #[test]
    fn build_final_prompt_uses_raw_prompt_with_hub_footer_when_no_subagent() {
        let snapshot = DueTaskSnapshot {
            id: "x".to_string(),
            prompt: "테스트를 실행하라".to_string(),
            subagent: None,
        };
        let expected = format!("테스트를 실행하라{HUB_RECORD_FOOTER}");
        assert_eq!(build_final_prompt(&snapshot), expected);
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

    #[test]
    fn resolve_validated_project_root_rejects_path_outside_workspace() {
        assert!(crate::resolve_validated_project_root("/etc").is_none());
        assert!(crate::resolve_validated_project_root("/etc/passwd").is_none());
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
