// 메모리 Runtime 상태(설계 §2) — "설정=파일 / 상태=메모리 / 이력=로그"의
// 가운데 축. 앱을 재시작하면 이 상태는 전부 사라진다(그래서 STARTUP_GRACE가
// 필요하다 — `scheduler.rs` 참조).

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// `(projectPath, taskId)` — 자율업무 하나를 유일하게 식별하는 키.
pub(crate) type TaskKey = (String, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Success,
    Failed,
    Timeout,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TaskRuntime {
    pub running: bool,
    /// 종료 시 SIGKILL 보루(§1-4)용. 프론트에 노출하지 않는다.
    pub child_pid: Option<u32>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub status: Option<RunStatus>,
    pub summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub log_path: Option<String>,
}

/// `BTreeMap::new()`는 const라 `static ... = Mutex::new(BTreeMap::new())` 한
/// 줄로 끝난다(`HashMap::new()`는 const fn이 아니라 `OnceLock`이 더 필요했을
/// 것). task 수는 수십 개라 성능 차이는 없고, 덤으로 상태 목록 정렬 순서가
/// 결정적이다.
pub(crate) static RUNTIME: Mutex<BTreeMap<TaskKey, TaskRuntime>> = Mutex::new(BTreeMap::new());

/// 앱 종료 훅이 세우는 플래그. 워커의 `should_abort` 콜백이 이 값을 폴링한다.
pub(crate) static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug)]
pub struct AutonomyRuntimeStatus {
    #[serde(rename = "projectPath")]
    pub project_path: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub running: bool,
    #[serde(rename = "lastStartedAt")]
    pub last_started_at: Option<DateTime<Utc>>,
    #[serde(rename = "lastFinishedAt")]
    pub last_finished_at: Option<DateTime<Utc>>,
    #[serde(rename = "nextRunAt")]
    pub next_run_at: Option<DateTime<Utc>>,
    pub status: Option<RunStatus>,
    pub summary: Option<String>,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<u64>,
    #[serde(rename = "logPath")]
    pub log_path: Option<String>,
}

pub(crate) fn to_status(key: &TaskKey, rt: &TaskRuntime) -> AutonomyRuntimeStatus {
    AutonomyRuntimeStatus {
        project_path: key.0.clone(),
        task_id: key.1.clone(),
        running: rt.running,
        last_started_at: rt.last_started_at,
        last_finished_at: rt.last_finished_at,
        next_run_at: rt.next_run_at,
        status: rt.status,
        summary: rt.summary.clone(),
        duration_ms: rt.duration_ms,
        log_path: rt.log_path.clone(),
    }
}

pub(crate) fn snapshot_all() -> Vec<AutonomyRuntimeStatus> {
    let map = RUNTIME.lock().unwrap();
    map.iter().map(|(k, rt)| to_status(k, rt)).collect()
}

pub(crate) fn running_count() -> usize {
    RUNTIME.lock().unwrap().values().filter(|rt| rt.running).count()
}

/// 미등록 task를 레지스트리에 등록한다 — `next_run_at = now + STARTUP_GRACE`
/// (신규 등록 task도 동일 규칙, 설계 §1). 이미 등록돼 있으면 아무것도 하지 않는다.
pub(crate) fn ensure_registered(key: &TaskKey, startup_grace: chrono::Duration) {
    let mut map = RUNTIME.lock().unwrap();
    map.entry(key.clone()).or_insert_with(|| TaskRuntime {
        next_run_at: Some(Utc::now() + startup_grace),
        ..Default::default()
    });
}

/// 설정에서 사라진 키는 `running==false`일 때만 제거한다 — 실행 중이면 끝난
/// 뒤 다음 tick에 제거한다(설계 §1 — 실행 중 삭제가 현재 실행을 끊지 않는다).
pub(crate) fn prune_missing(keep: &std::collections::HashSet<TaskKey>) {
    let mut map = RUNTIME.lock().unwrap();
    map.retain(|key, rt| keep.contains(key) || rt.running);
}

pub(crate) fn mark_started(key: &TaskKey) {
    let mut map = RUNTIME.lock().unwrap();
    let rt = map.entry(key.clone()).or_default();
    rt.running = true;
    rt.child_pid = None;
    rt.last_started_at = Some(Utc::now());
}

pub(crate) fn set_child_pid(key: &TaskKey, pid: Option<u32>) {
    let mut map = RUNTIME.lock().unwrap();
    if let Some(rt) = map.get_mut(key) {
        rt.child_pid = pid;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mark_finished(
    key: &TaskKey,
    status: RunStatus,
    summary: Option<String>,
    duration_ms: u64,
    log_path: Option<String>,
    next_run_at: Option<DateTime<Utc>>,
) {
    let mut map = RUNTIME.lock().unwrap();
    let rt = map.entry(key.clone()).or_default();
    rt.running = false;
    rt.child_pid = None;
    rt.last_finished_at = Some(Utc::now());
    rt.status = Some(status);
    rt.summary = summary;
    rt.duration_ms = Some(duration_ms);
    rt.log_path = log_path;
    rt.next_run_at = next_run_at;
}

/// 앱 종료 2초 유예 후에도 남아 있는 task들을 위한 unix 한정 보루 — 레지스트리에
/// 보관 중인 `child_pid`들에 직접 `SIGKILL`을 쏜다(설계 §1-4-4).
#[cfg(unix)]
pub(crate) fn force_kill_all_remaining() {
    let map = RUNTIME.lock().unwrap();
    for rt in map.values() {
        if rt.running {
            if let Some(pid) = rt.child_pid {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
        }
    }
}

#[cfg(windows)]
pub(crate) fn force_kill_all_remaining() {
    // Windows는 잔여 손자 프로세스가 남을 수 있다는 제약을 감수한다
    // (`dev_tools.rs`가 이미 문서화한 동일 제약과 일관된다).
}
