// ---------------- 자율업무(무인 실행) ----------------
// 사용자가 등록한 프롬프트를 정해진 주기마다 특정 프로젝트 디렉터리에서
// `claude -p`로 무인 실행한다. "설정=파일 / 상태=메모리 / 이력=로그"로 역할을
// 나눈다(설계 `docs/design/autonomy-runtime-and-config.md`) — 실행 상태
// (running/lastStatus/summary 등)는 더 이상 `<project>/.claude/autonomy.json`에
// 저장하지 않는다. 실행은 이 앱이 켜져 있는 동안(`run()`의 `setup()`에서 띄운
// 백그라운드 스레드)만 돈다. OS 스케줄러/launchd 연동은 의도적으로 범위 밖이다.
//
// 프로세스 실행 불변식(`lib.rs`의 `run_claude_command`와 동일한 계약):
// `std::process::Command`로 셸을 거치지 않고 직접 실행한다. 이 기능은 사용자가
// 자유 텍스트로 입력한 프롬프트를 인자로 그대로 넘기는 첫 사례이지만,
// `Command::new`는 셸 파싱을 거치지 않는다 — 각 인자가 셸 메타문자 해석 없이
// 그대로 하나의 argv 원소로 자식 프로세스에 전달되므로 셸 인젝션 경로가 없다.
// 권한 확인을 건너뛰는 종류의 실행 플래그는 그 어떤 것도 추가하지 않는다 —
// 무인 실행도 해당 프로젝트의 기존 `.claude/settings.json` 권한 설정 범위
// 안에서만 동작해야 한다는 안전 경계를 이 기능이 넓혀서는 안 된다.

pub(crate) mod config;
mod log;
mod runner;
mod runtime;
mod scheduler;

use serde::Serialize;
use std::time::Duration;

pub use runtime::AutonomyRuntimeStatus;

#[derive(Serialize, Clone, Debug)]
pub struct AutonomyProjectTasks {
    #[serde(rename = "projectPath")]
    pub project_path: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
    pub tasks: Vec<config::AutonomyTaskConfig>,
}

/// workspace 아래 `.claude/autonomy.json`이 있는 프로젝트만 모아 반환한다.
/// 전역 설정이 손상되면 빈 배열이 아니라 `Err`를 그대로 던진다(설계 §5-3 —
/// 자율업무 화면에 이미 있는 error 상태로 바로 배너가 뜨게 하는, 사용자에게
/// 보이는 오류 표면 3곳 중 하나).
#[tauri::command]
pub fn autonomy_list() -> Result<Vec<AutonomyProjectTasks>, String> {
    let roots = crate::config::workspace_roots_checked()?;
    let mut results = Vec::new();
    for workspace_root in roots {
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
            if !config::autonomy_file_path(&project_path).is_file() {
                continue;
            }

            let _guard = config::AUTONOMY_FILE_LOCK.lock().unwrap();
            let file = config::read_autonomy_file(&project_path);
            drop(_guard);

            results.push(AutonomyProjectTasks {
                project_path: project_path.to_string_lossy().to_string(),
                project_name: name.to_string(),
                tasks: file.tasks,
            });
        }
    }
    results.sort_by(|a, b| a.project_name.cmp(&b.project_name));
    Ok(results)
}

/// upsert. `project_path`는 반드시 `resolve_validated_project_root`로 검증한다
/// (경로 탈출 방지). `.claude/` 디렉터리·파일이 없으면 새로 만든다.
#[tauri::command]
pub fn autonomy_save_task(
    project_path: String,
    task: config::AutonomyTaskConfig,
) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = config::AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = config::read_autonomy_file(&root);
    config::upsert_task(&mut file, task);
    config::write_autonomy_file(&root, &file)
}

#[tauri::command]
pub fn autonomy_delete_task(project_path: String, task_id: String) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = config::AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = config::read_autonomy_file(&root);
    file.tasks.retain(|t| t.id != task_id);
    config::write_autonomy_file(&root, &file)
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

    let _guard = config::AUTONOMY_FILE_LOCK.lock().unwrap();
    let mut file = config::read_autonomy_file(&root);
    let Some(task) = file.tasks.iter_mut().find(|t| t.id == task_id) else {
        return Err("해당 자율업무를 찾을 수 없습니다.".to_string());
    };
    task.enabled = enabled;
    config::write_autonomy_file(&root, &file)
}

/// 메모리 런타임 상태 조회 — 설정(파일)과 별개 커맨드로 분리해, "설정의 값"과
/// "메모리 상태"가 한 객체에 섞여 다시 상태를 파일에 저장하고 싶은 유혹이
/// 생기지 않게 한다(설계 §2). 항상 성공한다(빈 배열도 정상 상태).
#[tauri::command]
pub fn autonomy_runtime_status() -> Vec<AutonomyRuntimeStatus> {
    runtime::snapshot_all()
}

/// `run()`의 `setup()`에서 한 번 호출한다. `TICK_SECONDS`(10초)마다
/// `scheduler::tick`을 돈다 — 그 tick 자체가 설정 리로드+reconcile+실행
/// 트리거를 전부 포함한다.
pub fn spawn_scheduler(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        scheduler::tick(&app_handle);
        std::thread::sleep(Duration::from_secs(config::TICK_SECONDS));
    });
}

/// 앱 종료 시 자식 프로세스 정리(설계 §1-4). `SHUTTING_DOWN` 플래그를 세워
/// 워커들의 `should_abort` 콜백이 진행 중인 `claude -p`를 타임아웃과 동일한
/// 경로(force_kill_process_group)로 정리하게 만든 뒤, `running_count`가 0이
/// 될 때까지 `max_wait` 동안만 폴링한다(그 이상 앱 종료를 붙잡지 않는다).
/// 그래도 남아 있으면 unix 한정 보루로 `child_pid`에 직접 `SIGKILL`을 쏜다.
pub(crate) fn request_shutdown_and_wait(max_wait: Duration) {
    runtime::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    let deadline = std::time::Instant::now() + max_wait;
    while std::time::Instant::now() < deadline {
        if runtime::running_count() == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    runtime::force_kill_all_remaining();
}

#[cfg(test)]
mod tests {
    // 경로 트래버설 차단 — 보안 관련이라 회귀 방지용으로 고정해둔다. 이
    // 커맨드가 직접 정의한 로직은 아니지만(`crate::resolve_validated_project_root`
    // 위임), 자율업무 3개 커맨드 전부가 이 게이트를 통과시킨다는 전제를 이
    // 모듈에서도 회귀로 고정한다.
    #[test]
    fn resolve_validated_project_root_rejects_path_outside_workspace() {
        assert!(crate::resolve_validated_project_root("/etc").is_none());
        assert!(crate::resolve_validated_project_root("/etc/passwd").is_none());
    }
}
