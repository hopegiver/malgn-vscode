// 단일 tick 스케줄러(설계 §1) — 스레드는 "스케줄러 스레드 1개 + 실행 중
// task마다 일회성 워커 스레드"만 존재한다. 설정 리로드 시점 = 매 tick(tick마다
// workspace를 훑어 `autonomy.json`을 다시 읽고 런타임 레지스트리를 reconcile
// 한다) — 그래서 추가/삭제/토글에 최대 `TICK_SECONDS` 안에 반응하고, 이를
// 위한 코드가 따로 없다(리로드가 곧 tick 본체다).

use super::config::{self, AutonomyTaskConfig};
use super::log;
use super::runner::{self, TaskSnapshot};
use super::runtime::{self, TaskKey, TaskRuntime};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

struct ScannedTask {
    project_path: PathBuf,
    project_path_str: String,
    task: AutonomyTaskConfig,
}

fn scan_all_tasks(roots: &[PathBuf]) -> Vec<ScannedTask> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let project_path = entry.path();
            if !project_path.is_dir() {
                continue;
            }
            if !config::autonomy_file_path(&project_path).is_file() {
                continue;
            }
            let _guard = config::AUTONOMY_FILE_LOCK.lock().unwrap();
            let file = config::read_autonomy_file(&project_path);
            drop(_guard);
            let project_path_str = project_path.to_string_lossy().to_string();
            for task in file.tasks {
                out.push(ScannedTask {
                    project_path: project_path.clone(),
                    project_path_str: project_path_str.clone(),
                    task,
                });
            }
        }
    }
    out
}

/// 판정식(설계 §1, 매 tick 상태 Mutex 보유 상태에서):
/// `due(task) = task.enabled && !rt.running && now >= rt.next_run_at`.
/// due 후보를 `next_run_at` 오름차순(가장 오래 밀린 것 우선)으로 정렬한 뒤,
/// 앞에서부터 `running_count < concurrency`인 동안만 선택한다.
///
/// 런타임 맵을 인자로 받는 순수 함수라 전역 `RUNTIME`을 건드리지 않고도
/// 테스트할 수 있다(§12 — `select_due_*`).
pub(crate) fn select_due(
    map: &BTreeMap<TaskKey, TaskRuntime>,
    candidates: &[(TaskKey, bool)],
    now: DateTime<Utc>,
    concurrency: usize,
) -> Vec<TaskKey> {
    let already_running = map.values().filter(|rt| rt.running).count();
    if already_running >= concurrency {
        return Vec::new();
    }
    let slots = concurrency - already_running;

    let mut due: Vec<(TaskKey, DateTime<Utc>)> = candidates
        .iter()
        .filter(|(_, enabled)| *enabled)
        .filter_map(|(key, _)| {
            let rt = map.get(key)?;
            if rt.running {
                return None;
            }
            let next = rt.next_run_at?;
            if next <= now {
                Some((key.clone(), next))
            } else {
                None
            }
        })
        .collect();

    due.sort_by_key(|(_, next)| *next);
    due.into_iter().take(slots).map(|(k, _)| k).collect()
}

/// 설정 손상 시 "아무 일도 하지 않고, 오류 문자열이 직전과 달라졌을 때만
/// 1회 `eprintln!`"(설계 §5-3, 로그 폭주 방지).
static LAST_CONFIG_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn report_config_error(err: &str) {
    let mut last = LAST_CONFIG_ERROR.lock().unwrap();
    if last.as_deref() != Some(err) {
        eprintln!("[autonomy] 전역 설정 오류로 스케줄러가 대기합니다: {err}");
        *last = Some(err.to_string());
    }
}

fn clear_config_error() {
    let mut last = LAST_CONFIG_ERROR.lock().unwrap();
    *last = None;
}

/// `TICK_SECONDS`마다 전체 워크스페이스를 훑어 due한 task를 찾아 실행한다.
pub(crate) fn tick(app_handle: &tauri::AppHandle) {
    if runtime::SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }

    let cfg = match crate::config::load() {
        Ok(c) => c,
        Err(e) => {
            report_config_error(&e);
            return;
        }
    };

    let roots = match crate::config::workspace_roots_checked() {
        Ok(r) => r,
        Err(e) => {
            report_config_error(&e);
            return;
        }
    };
    clear_config_error();

    let scanned = scan_all_tasks(&roots);
    let now = Utc::now();
    let startup_grace = chrono::Duration::minutes(config::STARTUP_GRACE_MINUTES as i64);

    // reconcile: 새 키 등록(`next_run_at = now + STARTUP_GRACE`), 사라진 키는
    // 실행 중이 아닐 때만 제거.
    let mut seen_keys: HashSet<TaskKey> = HashSet::new();
    for item in &scanned {
        let key: TaskKey = (item.project_path_str.clone(), item.task.id.clone());
        runtime::ensure_registered(&key, startup_grace);
        seen_keys.insert(key);
    }
    runtime::prune_missing(&seen_keys);

    let concurrency = cfg
        .autonomy
        .concurrency
        .unwrap_or(config::DEFAULT_CONCURRENCY)
        .clamp(1, config::MAX_CONCURRENCY) as usize;

    let candidates: Vec<(TaskKey, bool)> = scanned
        .iter()
        .map(|item| {
            (
                (item.project_path_str.clone(), item.task.id.clone()),
                item.task.enabled,
            )
        })
        .collect();

    let due_keys = {
        let map = runtime::RUNTIME.lock().unwrap();
        select_due(&map, &candidates, now, concurrency)
    };

    for key in due_keys {
        let Some(item) = scanned
            .iter()
            .find(|s| s.project_path_str == key.0 && s.task.id == key.1)
        else {
            continue;
        };

        let timeout_minutes = config::effective_timeout_minutes(&item.task, cfg.autonomy.default_timeout);
        let snapshot = TaskSnapshot {
            key: key.clone(),
            project_path: item.project_path.clone(),
            task_id: item.task.id.clone(),
            task_name: item.task.name.clone(),
            prompt: item.task.prompt.clone(),
            subagent: item.task.subagent.clone(),
            interval_minutes: item.task.interval,
            timeout_minutes,
        };

        runtime::mark_started(&key);
        runner::emit_status(app_handle, &key);

        let handle = app_handle.clone();
        std::thread::spawn(move || {
            runner::run_task(snapshot, handle);
        });
    }

    // 로그 보존 스윕 트리거(설계 §9) — 날짜가 바뀐 첫 tick에 1회. 앱 시작 후
    // 첫 tick도 이 함수가 그대로 커버한다(별도 부팅 스레드 불필요).
    log::maybe_sweep_logs(
        &roots,
        config::effective_log_retention_days(cfg.logs.retention_days),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: &str) -> TaskKey {
        ("/tmp/project".to_string(), n.to_string())
    }

    fn runtime_with(next_run_at: Option<DateTime<Utc>>, running: bool) -> TaskRuntime {
        TaskRuntime {
            running,
            next_run_at,
            ..Default::default()
        }
    }

    // select_due — 미등록(맵에 키가 없음)은 due가 아니다.
    #[test]
    fn select_due_excludes_unregistered_task() {
        let map: BTreeMap<TaskKey, TaskRuntime> = BTreeMap::new();
        let candidates = vec![(key("a"), true)];
        let due = select_due(&map, &candidates, Utc::now(), 3);
        assert!(due.is_empty());
    }

    // select_due — 유예 중(next_run_at이 미래)은 due가 아니다.
    #[test]
    fn select_due_excludes_task_within_startup_grace() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(key("a"), runtime_with(Some(now + chrono::Duration::minutes(3)), false));
        let candidates = vec![(key("a"), true)];
        let due = select_due(&map, &candidates, now, 3);
        assert!(due.is_empty());
    }

    // select_due — 이미 실행 중인 task는 제외한다(직렬 보장의 핵심).
    #[test]
    fn select_due_excludes_task_currently_running() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(
            key("a"),
            runtime_with(Some(now - chrono::Duration::minutes(1)), true),
        );
        let candidates = vec![(key("a"), true)];
        let due = select_due(&map, &candidates, now, 3);
        assert!(due.is_empty());
    }

    // select_due — 완료 후 interval이 경과하면(next_run_at <= now) due다.
    #[test]
    fn select_due_includes_task_after_interval_elapsed() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(
            key("a"),
            runtime_with(Some(now - chrono::Duration::minutes(1)), false),
        );
        let candidates = vec![(key("a"), true)];
        let due = select_due(&map, &candidates, now, 3);
        assert_eq!(due, vec![key("a")]);
    }

    #[test]
    fn select_due_excludes_disabled_task_even_if_due() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(
            key("a"),
            runtime_with(Some(now - chrono::Duration::minutes(1)), false),
        );
        let candidates = vec![(key("a"), false)];
        let due = select_due(&map, &candidates, now, 3);
        assert!(due.is_empty());
    }

    // select_due — concurrency 초과분은 next_run_at 오름차순으로 앞에서부터만
    // 선택한다.
    #[test]
    fn select_due_respects_concurrency_limit_and_orders_by_next_run_at() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(
            key("older"),
            runtime_with(Some(now - chrono::Duration::minutes(10)), false),
        );
        map.insert(
            key("newer"),
            runtime_with(Some(now - chrono::Duration::minutes(1)), false),
        );
        let candidates = vec![(key("older"), true), (key("newer"), true)];
        let due = select_due(&map, &candidates, now, 1);
        assert_eq!(due, vec![key("older")], "가장 오래 밀린 것이 우선이어야 한다");
    }

    #[test]
    fn select_due_returns_empty_when_already_at_concurrency_limit() {
        let now = Utc::now();
        let mut map = BTreeMap::new();
        map.insert(key("running-1"), runtime_with(Some(now), true));
        map.insert(
            key("waiting"),
            runtime_with(Some(now - chrono::Duration::minutes(1)), false),
        );
        let candidates = vec![(key("running-1"), true), (key("waiting"), true)];
        let due = select_due(&map, &candidates, now, 1);
        assert!(due.is_empty(), "이미 concurrency 한도만큼 실행 중이면 새로 선택하지 않는다");
    }
}
