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

/// 미등록 task를 레지스트리에 등록한다 — `next_run_at`은 호출자(scheduler)가
/// `schedule::initial_next_run_at()`으로 이미 계산해 넘긴 값을 그대로 쓴다.
/// 이 모듈은 스케줄 해석 방법을 모르는 순수 상태 모듈 성격을 유지한다
/// (`config`/`schedule`를 몰라도 된다). 이미 등록돼 있으면 아무것도 하지
/// 않는다.
pub(crate) fn ensure_registered(key: &TaskKey, initial_next_run_at: Option<DateTime<Utc>>) {
    let mut map = RUNTIME.lock().unwrap();
    map.entry(key.clone()).or_insert_with(|| TaskRuntime {
        next_run_at: initial_next_run_at,
        ..Default::default()
    });
}

/// FixedTime 편집·재개를 즉시 반영하기 위한 재스케줄(M2·M3·M4) — `running ==
/// false`일 때만 `next_run_at`을 덮어쓰고, 나머지 필드(`status`/`summary`/
/// `log_path` 등 마지막 실행 정보)는 그대로 보존한다. 실행 중이면 no-op(현재
/// 회차를 흔들지 않는다). 아직 레지스트리에 없는 키는(M4 — 신규 등록 직후
/// 다음 tick의 reconcile을 기다리지 않고) **즉시 삽입한다** — `reschedule_if_idle`
/// (미등록 키에 no-op)이었던 이전 버전을 이 upsert 동작으로 대체했다. 다음
/// tick의 `ensure_registered`는 이미 등록된 키엔 손대지 않으므로(`or_insert_with`)
/// 이 함수가 미리 넣어 둔 값을 덮어쓰지 않는다.
pub(crate) fn reschedule_or_register(key: &TaskKey, next: Option<DateTime<Utc>>) {
    let mut map = RUNTIME.lock().unwrap();
    let rt = map.entry(key.clone()).or_default();
    if !rt.running {
        rt.next_run_at = next;
    }
}

/// 설정에서 사라진 키는 `running==false`일 때만 제거한다 — 실행 중이면 끝난
/// 뒤 다음 tick에 제거한다(설계 §1 — 실행 중 삭제가 현재 실행을 끊지 않는다).
pub(crate) fn prune_missing(keep: &std::collections::HashSet<TaskKey>) {
    let mut map = RUNTIME.lock().unwrap();
    map.retain(|key, rt| keep.contains(key) || rt.running);
}

/// `scheduler::tick`이 `select_due`로 뽑은 due 키를 실제로 시작 마킹한다.
/// M1: `select_due`(조회 락)와 이 함수(마킹 락) 사이의 창에서 `try_start_now`
/// ("지금 실행")가 같은 키를 먼저 시작시켰을 수 있다 — 이미 `running`이면
/// 아무 필드도 덮지 않고 `false`를 반환해, 호출자가 두 번째 `run_task`
/// 워커 스레드 spawn을 건너뛰게 한다(중복 `claude -p`·`child_pid` 유실 방지).
/// 반환값이 `true`일 때만 실제로 시작 마킹이 일어난 것이다.
pub(crate) fn mark_started(key: &TaskKey) -> bool {
    let mut map = RUNTIME.lock().unwrap();
    let rt = map.entry(key.clone()).or_default();
    if rt.running {
        return false;
    }
    rt.running = true;
    rt.child_pid = None;
    rt.last_started_at = Some(Utc::now());
    true
}

/// "지금 실행" 전용 원자적 시작 시도(`autonomy_run_now`) — 결정 1·2를 하나의
/// 락 구간 안에서 함께 처리한다. `select_due`(scheduler.rs)처럼 "조회 락"과
/// "마킹 락"을 분리하면, 그 사이 창에서 스케줄러 tick이 같은 task를 먼저
/// 시작시키는 TOCTOU가 생길 수 있다 — 이 커맨드는 사용자가 버튼을 누른
/// 그 순간의 판정이 그대로 유효해야 하므로 분리하지 않는다.
/// 실패해도 상태를 바꾸지 않는다(부분 마킹 없음). 성공하면 `mark_started`와
/// 동일한 필드를 채운다.
pub(crate) fn try_start_now(key: &TaskKey, concurrency: usize) -> Result<(), String> {
    let mut map = RUNTIME.lock().unwrap();
    if let Some(rt) = map.get(key) {
        if rt.running {
            return Err("이미 실행 중인 작업입니다.".to_string());
        }
    }
    let already_running = map.values().filter(|rt| rt.running).count();
    if already_running >= concurrency {
        return Err("동시 실행 한도를 초과했습니다.".to_string());
    }
    let rt = map.entry(key.clone()).or_default();
    rt.running = true;
    rt.child_pid = None;
    rt.last_started_at = Some(Utc::now());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: &str) -> TaskKey {
        ("/tmp/project".to_string(), n.to_string())
    }

    /// 테스트 간 전역 `RUNTIME` 상태가 새지 않게 매 테스트가 고유 키를 쓰지만,
    /// 그래도 격리를 위해 시작 시점에 해당 키를 지운다.
    fn cleanup(key: &TaskKey) {
        RUNTIME.lock().unwrap().remove(key);
    }

    #[test]
    fn reschedule_or_register_overwrites_next_run_at_and_preserves_last_result_for_registered_task() {
        let k = key("reschedule-idle-1");
        cleanup(&k);

        let original_next = Utc::now() + chrono::Duration::minutes(5);
        {
            let mut map = RUNTIME.lock().unwrap();
            map.insert(
                k.clone(),
                TaskRuntime {
                    running: false,
                    next_run_at: Some(original_next),
                    status: Some(RunStatus::Success),
                    summary: Some("이전 실행 요약".to_string()),
                    log_path: Some("/tmp/log".to_string()),
                    ..Default::default()
                },
            );
        }

        let new_next = Utc::now() + chrono::Duration::hours(1);
        reschedule_or_register(&k, Some(new_next));

        let map = RUNTIME.lock().unwrap();
        let rt = map.get(&k).unwrap();
        assert_eq!(rt.next_run_at, Some(new_next));
        assert_eq!(rt.status, Some(RunStatus::Success), "마지막 실행 상태는 보존돼야 한다");
        assert_eq!(rt.summary.as_deref(), Some("이전 실행 요약"));
        assert_eq!(rt.log_path.as_deref(), Some("/tmp/log"));
        drop(map);
        cleanup(&k);
    }

    // try_start_now — 결정 1: 이미 실행 중이면 상태를 건드리지 않고 Err.
    #[test]
    fn try_start_now_rejects_already_running_task() {
        let k = key("run-now-already-running");
        cleanup(&k);
        {
            let mut map = RUNTIME.lock().unwrap();
            map.insert(k.clone(), TaskRuntime { running: true, ..Default::default() });
        }

        let result = try_start_now(&k, 8);

        assert!(result.is_err());
        let map = RUNTIME.lock().unwrap();
        assert!(map.get(&k).unwrap().running, "실행 중 상태가 그대로 유지돼야 한다");
        drop(map);
        cleanup(&k);
    }

    // try_start_now — 결정 2: concurrency 한도에 도달했으면 아직 등록되지
    // 않은/실행 중이 아닌 task라도 거부한다.
    #[test]
    fn try_start_now_rejects_when_concurrency_limit_reached() {
        let running_key = key("run-now-limit-running");
        let target_key = key("run-now-limit-target");
        cleanup(&running_key);
        cleanup(&target_key);
        {
            let mut map = RUNTIME.lock().unwrap();
            map.insert(running_key.clone(), TaskRuntime { running: true, ..Default::default() });
        }

        let result = try_start_now(&target_key, 1);

        assert!(result.is_err());
        let map = RUNTIME.lock().unwrap();
        assert!(
            map.get(&target_key).map(|rt| !rt.running).unwrap_or(true),
            "한도 초과로 거부된 task는 running으로 마킹되면 안 된다"
        );
        drop(map);
        cleanup(&running_key);
        cleanup(&target_key);
    }

    // try_start_now — 미등록 task도 성공 시 즉시 running=true로 마킹되고
    // last_started_at이 채워진다(scheduler의 mark_started와 동일 관용구).
    #[test]
    fn try_start_now_marks_unregistered_task_running_on_success() {
        let k = key("run-now-success");
        cleanup(&k);

        let result = try_start_now(&k, 8);

        assert!(result.is_ok());
        let map = RUNTIME.lock().unwrap();
        let rt = map.get(&k).unwrap();
        assert!(rt.running);
        assert!(rt.last_started_at.is_some());
        drop(map);
        cleanup(&k);
    }

    // M4 회귀 — reschedule_or_register는 미등록 키도 즉시 삽입한다(등록
    // 직후 next tick까지 next_run_at이 비어 화면이 거짓 오류를 보이는 창을
    // 없앤다).
    #[test]
    fn reschedule_or_register_inserts_entry_for_unregistered_key() {
        let k = key("reschedule-upsert-new");
        cleanup(&k);

        let next = Utc::now() + chrono::Duration::minutes(10);
        reschedule_or_register(&k, Some(next));

        let map = RUNTIME.lock().unwrap();
        let rt = map.get(&k).expect("미등록 키도 즉시 삽입돼야 한다");
        assert_eq!(rt.next_run_at, Some(next));
        assert!(!rt.running);
        drop(map);
        cleanup(&k);
    }

    #[test]
    fn reschedule_or_register_is_noop_while_task_is_running() {
        let k = key("reschedule-upsert-running");
        cleanup(&k);

        let original_next = Utc::now() + chrono::Duration::minutes(5);
        {
            let mut map = RUNTIME.lock().unwrap();
            map.insert(
                k.clone(),
                TaskRuntime {
                    running: true,
                    next_run_at: Some(original_next),
                    ..Default::default()
                },
            );
        }

        reschedule_or_register(&k, Some(Utc::now() + chrono::Duration::hours(1)));

        let map = RUNTIME.lock().unwrap();
        assert_eq!(map.get(&k).unwrap().next_run_at, Some(original_next));
        drop(map);
        cleanup(&k);
    }

    // M1 회귀(핵심) — 이미 running인 키에 mark_started를 호출하면 false를
    // 반환하고 시작 시각·child_pid 등 기존 필드를 전혀 덮지 않는다. 이
    // 가드가 없으면 scheduler::tick의 mark_started가 try_start_now("지금
    // 실행")보다 늦게 도착했을 때 두 번째 run_task 워커를 spawn시킨다.
    #[test]
    fn mark_started_returns_false_and_does_not_overwrite_when_already_running() {
        let k = key("mark-started-guard");
        cleanup(&k);

        let original_started_at = Utc::now() - chrono::Duration::minutes(1);
        {
            let mut map = RUNTIME.lock().unwrap();
            map.insert(
                k.clone(),
                TaskRuntime {
                    running: true,
                    last_started_at: Some(original_started_at),
                    child_pid: Some(4242),
                    ..Default::default()
                },
            );
        }

        let result = mark_started(&k);

        assert!(!result, "이미 running이면 false를 반환해야 한다");
        let map = RUNTIME.lock().unwrap();
        let rt = map.get(&k).unwrap();
        assert_eq!(
            rt.last_started_at,
            Some(original_started_at),
            "이미 시작된 회차의 시작 시각을 덮으면 안 된다"
        );
        assert_eq!(rt.child_pid, Some(4242), "먼저 시작된 자식의 pid가 유실되면 안 된다");
        drop(map);
        cleanup(&k);
    }

    #[test]
    fn mark_started_marks_unregistered_task_running_and_returns_true() {
        let k = key("mark-started-fresh");
        cleanup(&k);

        let result = mark_started(&k);

        assert!(result);
        let map = RUNTIME.lock().unwrap();
        let rt = map.get(&k).unwrap();
        assert!(rt.running);
        assert!(rt.last_started_at.is_some());
        drop(map);
        cleanup(&k);
    }
}
