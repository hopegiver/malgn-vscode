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
mod schedule;
mod scheduler;

use serde::Serialize;
use std::time::Duration;

pub use log::RunHistoryEntry;
pub use runtime::AutonomyRuntimeStatus;

/// `Mutex::lock()`이 poison(락을 쥔 다른 스레드가 패닉)되어도 값을 그대로
/// 복구해 계속 쓴다(리뷰 D1 권고 ②). 이 크레이트의 자율업무 상태
/// (`runtime::RUNTIME`·`config::AUTONOMY_FILE_LOCK`·설정 오류 중복 로그 억제용
/// static 등)는 전부 재구성 가능한 상태다 — poisoning 이후에도 마지막으로 쓰인
/// 값을 신뢰하고 계속 도는 편이, `unwrap()`으로 이 락을 쓰는 모든 후속 호출을
/// 연쇄 패닉시켜 스케줄러 스레드를 영구 정지시키는 것보다 안전하다. 이
/// 크레이트의 모든 `Mutex::lock()`은 `.unwrap()` 대신 이 헬퍼를 쓴다(테스트
/// 코드의 셋업/단언용 락은 예외 — 그쪽은 poisoning 자체가 테스트 격리 실패의
/// 신호라 그대로 패닉하는 편이 낫다).
pub(crate) fn lock_recovering<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// TaskKey의 `project_path` 컴포넌트를 만드는 정본(C1) — `resolve_validated_project_root`
/// (`workspace/tree.rs:39`)가 돌려주는 **std** `canonicalize()` 결과를,
/// 스케줄러 스캐너(`scheduler::scan_all_tasks`/`autonomy_list_blocking`)가
/// 쓰는 **`dunce::canonicalize`** 기반 경로 표현으로 다시 맞춘다.
///
/// Windows에서 std `std::fs::canonicalize()`는 `\\?\`(확장 길이) 접두를
/// 붙이는데, 이 저장소가 `dev_tools/classify.rs:456-464`에 이미 문서화해 둔
/// 바로 그 함정이다 — 스케줄러 쪽 워크스페이스 루트는
/// `config/user_config.rs:221`의 `dunce::canonicalize`에서 오므로, 접두
/// 유무가 갈리면 이 커맨드들이 만드는 TaskKey가 스케줄러 키와 **항상**
/// 달라진다(유령 키 → 중복 실행·상태 미표시). `dunce::canonicalize`를 한 번
/// 더 통과시키면 접두가 제거돼 두 문자열이 같아진다(이미 절대경로이므로
/// 재정규화 비용만 들고, unix에서는 no-op).
///
/// 근본 수정(`resolve_validated_project_root` 자체를 `dunce`로 바꾸는 것)은
/// `session_chat`까지 영향이 번지므로 이번 브랜치 스코프 밖이다 — 리뷰
/// 권고대로 국소 수정(이 헬퍼 + 호출부 2곳)만 적용한다.
fn task_key_root(root: &std::path::Path) -> String {
    dunce::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// 저장(신규 등록/편집) 직후 즉시 재스케줄해야 하는지 판정하는 순수 함수 —
/// M3(벽시계 모드로 저장되면 항상 재계산)와 m1(모드 자체가 바뀌면 어느
/// 방향이든 재계산, "옛 고정 시각이 그대로 남는" 문제 해소)을 하나의
/// 조건으로 합친다. Interval 모드 그대로 값만 바뀐 저장(모드 불변)은
/// `false` — 기존 "편집은 다음 회차부터 반영" 동작을 그대로 보존한다.
///
/// `new_mode.is_wall_clock()`을 쓴다(`config::ScheduleMode::is_wall_clock`,
/// exhaustive match 1곳) — 예전에는 `new_mode == FixedTime`으로만 비교해서
/// `Hourly→Hourly`(분만 변경)·`Cron→Cron`(표현식만 변경) 편집이 재스케줄되지
/// 않는 결함이 있었다. 이 술어로 바꾸면 다음 모드가 추가될 때도 컴파일
/// 에러로 판단을 강제한다.
fn should_reschedule_on_save(
    previous_mode: Option<config::ScheduleMode>,
    new_mode: config::ScheduleMode,
) -> bool {
    new_mode.is_wall_clock() || previous_mode.map(|m| m != new_mode).unwrap_or(false)
}

/// L1 검증(설계 §7.2) — 저장 전 즉시 피드백. 이 검증이 없어도 L2
/// (`normalize_task`)+L3(계산 시점 `None`)이 이미 안전하지만(실행되지 않을
/// 뿐), 이게 없으면 사용자가 "저장은 됐는데 영영 안 돈다"를 겪는다. 파일을
/// 쓰기 전에 거부해 그 UX 실패를 막는 것이 이 함수의 유일한 존재 이유다.
fn validate_schedule_fields(task: &config::AutonomyTaskConfig) -> Result<(), String> {
    match task.schedule_mode {
        config::ScheduleMode::Hourly => match task.hourly_minute {
            Some(m) if m <= 59 => Ok(()),
            _ => Err("매시간 모드는 0~59 사이의 분을 지정해야 합니다.".to_string()),
        },
        config::ScheduleMode::Cron => {
            let expr = task.cron.as_deref().unwrap_or("");
            schedule::validate_cron(expr)
        }
        config::ScheduleMode::Interval | config::ScheduleMode::FixedTime => Ok(()),
    }
}

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
fn autonomy_list_blocking() -> Result<Vec<AutonomyProjectTasks>, String> {
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

            let _guard = lock_recovering(&config::AUTONOMY_FILE_LOCK);
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

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn autonomy_list() -> Result<Vec<AutonomyProjectTasks>, String> {
    tauri::async_runtime::spawn_blocking(autonomy_list_blocking)
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
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
    validate_schedule_fields(&task)?;
    let task_id = task.id.clone();

    let _guard = lock_recovering(&config::AUTONOMY_FILE_LOCK);
    let mut file = config::read_autonomy_file(&root);
    let previous_mode = file
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .map(|t| t.schedule_mode);
    config::upsert_task(&mut file, task);
    config::write_autonomy_file(&root, &file)?;
    drop(_guard);

    // 재스케줄 조건은 `should_reschedule_on_save`(M3·m1) 참조. `initial_next_run_at`
    // (앱 시작 전용 30분 따라잡기 앵커)은 편집 경로에 쓰지 않는다 — 대신
    // 앵커 없는 `schedule::reschedule_next_run_at`을 쓴다(M3: "오늘 이미
    // 돈 회차"가 편집 저장마다 다시 잡혀 같은 날 두 번째 실행이 되는 것을
    // 막는다). 미등록 키도 즉시 삽입하는 upsert형 `reschedule_or_register`를
    // 쓴다(M4: 신규 등록 직후 다음 tick까지 이어지던 "실행 시각이 올바르지
    // 않습니다" 거짓 오류 창을 없앤다).
    // TaskKey의 경로 컴포넌트는 `task_key_root`로 스캐너와 같은 표현으로
    // 맞춘다(C1).
    if let Some(saved) = file.tasks.iter().find(|t| t.id == task_id) {
        if should_reschedule_on_save(previous_mode, saved.schedule_mode) {
            let key: runtime::TaskKey = (task_key_root(&root), task_id);
            runtime::reschedule_or_register(&key, schedule::reschedule_next_run_at(saved, chrono::Utc::now()));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn autonomy_delete_task(project_path: String, task_id: String) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = lock_recovering(&config::AUTONOMY_FILE_LOCK);
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

    let _guard = lock_recovering(&config::AUTONOMY_FILE_LOCK);
    let mut file = config::read_autonomy_file(&root);
    let (should_reschedule, saved) = {
        let Some(task) = file.tasks.iter_mut().find(|t| t.id == task_id) else {
            return Err("해당 자율업무를 찾을 수 없습니다.".to_string());
        };
        task.enabled = enabled;
        let should_reschedule = enabled && task.schedule_mode.is_wall_clock();
        (should_reschedule, task.clone())
    };
    config::write_autonomy_file(&root, &file)?;
    drop(_guard);

    // PM 결정(M2, 이번 라운드에서 Hourly/Cron까지 `is_wall_clock()`으로
    // 확장): 중지했던 벽시계 모드(FixedTime/Hourly/Cron) task를 재개하면
    // 밀린 회차를 즉시 돌리지 않고 다음 예정 시각까지 기다린다 — 설계 §5가
    // 세운 "앱이 꺼져 있던 동안 지난 회차는 건너뛴다" 원칙을 재개(중지→
    // enabled) 전이에도 동일 적용한다. 이 가드가 없으면 매시 30분 task를
    // 3시간 중지 후 재개하는 순간 `next_run_at`에 남은 과거값이 그대로
    // due가 되어 즉시 1회 실행된다. Interval 모드 재개는 손대지 않는다
    // (완료 후 interval 경과분을 즉시 도는 것은 기존에도 있던 동작이고
    // 이번 리뷰의 지적 대상이 아니다 — 리뷰 M2 사유).
    if should_reschedule {
        let key: runtime::TaskKey = (task_key_root(&root), task_id);
        runtime::reschedule_or_register(&key, schedule::reschedule_next_run_at(&saved, chrono::Utc::now()));
    }

    Ok(())
}

/// "지금 실행" 수동 트리거 — `next_run_at` 도래를 기다리지 않고 해당 task를
/// 즉시 실행시킨다. 프론트 계약: `Ok(())` 성공 / `Err(한국어 메시지)` 실패
/// (프론트가 그대로 토스트로 띄운다). 트리거 패턴은 `scheduler::tick`의
/// due 실행 경로(mark_started → emit_status → 워커 스레드 spawn)를 그대로
/// 재사용하고 due 판정(`select_due`)만 건너뛴다.
///
/// 결정 1(이미 실행 중이면 거부)·2(concurrency 한도 존중)는
/// `runtime::try_start_now` 한 곳에서 원자적으로 처리한다. 결정 3(수동 실행
/// 후 `next_run_at` 특별 처리 없음)은 이 함수가 `next_run_at`을 전혀 건드리지
/// 않는 것으로 구현된다 — `runner::run_task`가 완료 시점에 기존
/// `schedule::next_run_after_finish`로 정상 재계산한다.
#[tauri::command]
pub fn autonomy_run_now(
    app_handle: tauri::AppHandle,
    project_path: String,
    task_id: String,
) -> Result<(), String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;

    let _guard = lock_recovering(&config::AUTONOMY_FILE_LOCK);
    let file = config::read_autonomy_file(&root);
    drop(_guard);

    let Some(task) = file.tasks.iter().find(|t| t.id == task_id) else {
        return Err("해당 자율업무를 찾을 수 없습니다.".to_string());
    };

    let cfg = crate::config::load().map_err(|e| format!("전역 설정을 불러오지 못했습니다: {e}"))?;
    let concurrency = cfg
        .autonomy
        .concurrency
        .unwrap_or(config::DEFAULT_CONCURRENCY)
        .clamp(1, config::MAX_CONCURRENCY) as usize;

    let key: runtime::TaskKey = (task_key_root(&root), task_id.clone());
    runtime::try_start_now(&key, concurrency)?;

    let timeout_minutes = config::effective_timeout_minutes(task, cfg.autonomy.default_timeout);
    let snapshot = runner::TaskSnapshot {
        key: key.clone(),
        project_path: root,
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        prompt: task.prompt.clone(),
        subagent: task.subagent.clone(),
        schedule: schedule::ScheduleSnapshot::from(task),
        timeout_minutes,
    };

    runner::emit_status(&app_handle, &key);

    std::thread::spawn(move || {
        runner::run_task(snapshot, app_handle);
    });

    Ok(())
}

/// 메모리 런타임 상태 조회 — 설정(파일)과 별개 커맨드로 분리해, "설정의 값"과
/// "메모리 상태"가 한 객체에 섞여 다시 상태를 파일에 저장하고 싶은 유혹이
/// 생기지 않게 한다(설계 §2). 항상 성공한다(빈 배열도 정상 상태).
#[tauri::command]
pub fn autonomy_runtime_status() -> Vec<AutonomyRuntimeStatus> {
    runtime::snapshot_all()
}

/// 스케줄러 heartbeat 조회(리뷰 D1 권고 ③) — `lastTickAt`과 `now`(같은 커맨드
/// 호출 시점의 서버 시각) 둘 다 내려준다. `nextRunAt`(각 task의 다음 예정
/// 시각)과 달리 이건 "스케줄러 루프 자체가 살아 있는가"를 나타낸다:
/// `now - lastTickAt`이 `tickSeconds`를 한참(수 배) 넘기면 스케줄러 스레드가
/// 멈춘 것이다(패닉으로 스레드가 죽었거나, 앱이 아직 `spawn_scheduler`를 호출하기
/// 전이면 `lastTickAt`이 `null`이다). 프런트가 이 값을 "N분째 응답 없음" 같은
/// 배너로 쓰려면 `src/`쪽에서 이 커맨드를 주기적으로 invoke하고 임계값(예:
/// `tickSeconds`의 3~6배)을 정해 비교하는 코드가 별도로 필요하다 — 이 커맨드는
/// 그 판단에 쓸 원값만 내보낸다(판단 로직을 백엔드에 넣지 않는다 — 임계값은
/// UI 정책이라 프런트 소관).
#[derive(Serialize, Clone, Debug)]
pub struct SchedulerHealth {
    #[serde(rename = "lastTickAt")]
    pub last_tick_at: Option<chrono::DateTime<chrono::Utc>>,
    pub now: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "tickSeconds")]
    pub tick_seconds: u64,
}

#[tauri::command]
pub fn autonomy_scheduler_health() -> SchedulerHealth {
    SchedulerHealth {
        last_tick_at: runtime::last_tick_at(),
        now: chrono::Utc::now(),
        tick_seconds: config::TICK_SECONDS,
    }
}

/// 과거 실행 이력 조회 — "설정=파일 / 상태=메모리 / 이력=로그" 3분리(설계 §2)의
/// 세 번째 축을 읽기 전용으로 노출한다. `autonomy.json`에는 `history`를
/// 되살리지 않기로 한 PM 결정에 따른 대안 경로다. 다른 3개 커맨드와 동일하게
/// `resolve_validated_project_root`로 경로를 검증한다(워크스페이스 밖 경로·
/// 존재하지 않는 경로는 여기서 걸러진다). 실제 스캔·파싱은 `log::read_task_history`
/// 에 위임 — 손상 파일 스킵, limit 적용, 정렬은 전부 그쪽 책임이다.
fn autonomy_task_history_blocking(
    project_path: String,
    task_id: String,
    limit: Option<u32>,
) -> Result<Vec<RunHistoryEntry>, String> {
    let root = crate::resolve_validated_project_root(&project_path)
        .ok_or_else(|| "프로젝트 경로가 올바르지 않습니다.".to_string())?;
    Ok(log::read_task_history(&root, &task_id, limit))
}

/// `autonomy_list`(m4 지적, 리뷰 2026-09-17)와 동일한 이유·관용구 — 이 커맨드는
/// 날짜 디렉터리 전수 `read_dir` + 정렬 + 최대 `limit`개 파일 `open`+`BufRead`를
/// 수행하는 동기 파일 I/O라, `pub fn`으로 두면 Tauri 메인(=UI) 스레드가 그
/// 시간만큼 멈춘다. `spawn_blocking`으로 별도 스레드에 위임한다. 본문 로직은
/// `autonomy_task_history_blocking`으로 그대로 옮겼을 뿐 변경 없음.
#[tauri::command]
pub async fn autonomy_task_history(
    project_path: String,
    task_id: String,
    limit: Option<u32>,
) -> Result<Vec<RunHistoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        autonomy_task_history_blocking(project_path, task_id, limit)
    })
    .await
    .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// `catch_unwind`에 넘긴 클로저가 패닉했을 때 `Box<dyn Any + Send>` 페이로드에서
/// 사람이 읽을 수 있는 메시지를 최대한 뽑아낸다(`&str`/`String` 두 가지 흔한
/// 페닉 페이로드 형태만 다룬다 — 그 외 타입이면 고정 문자열로 폴백, 과설계
/// 금지).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(알 수 없는 패닉 페이로드)".to_string()
    }
}

/// `run()`의 `setup()`에서 한 번 호출한다. `TICK_SECONDS`(10초)마다
/// `scheduler::tick`을 돈다 — 그 tick 자체가 설정 리로드+reconcile+실행
/// 트리거를 전부 포함한다.
///
/// `catch_unwind`로 `tick()` 호출 자체를 감싼다(리뷰 D1 권고 ① — 이 스케줄러는
/// 스레드가 1개뿐이라 `tick()` 안에서 패닉하면 그 스레드가 영구 종료되고,
/// 이후 모든 자율업무가 조용히(화면에 아무 오류도 없이) 멈춘다. 세션 채팅
/// 모듈(`session_chat`)에 이미 있는 것과 같은 기법을 여기 적용 누락분에
/// 적용한다 — `AssertUnwindSafe`가 필요한 이유는 `app_handle`을 참조로 넘기는
/// 클로저가 기본적으로 `UnwindSafe`로 추론되지 않기 때문인데, 패닉 이후에도
/// 이 핸들 자체는 계속 정상 재사용 가능한 값이라(내부적으로 이미 Arc 기반
/// 공유 핸들) 안전하다.
pub fn spawn_scheduler(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let handle = app_handle.clone();
        if let Err(payload) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scheduler::tick(&handle)))
        {
            eprintln!(
                "[autonomy] 스케줄러 tick이 패닉했습니다 — 다음 tick({}s 후)으로 계속 진행합니다: {}",
                config::TICK_SECONDS,
                panic_message(&*payload)
            );
        }
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
    use super::*;

    // 경로 트래버설 차단 — 보안 관련이라 회귀 방지용으로 고정해둔다. 이
    // 커맨드가 직접 정의한 로직은 아니지만(`crate::resolve_validated_project_root`
    // 위임), 자율업무 3개 커맨드 전부가 이 게이트를 통과시킨다는 전제를 이
    // 모듈에서도 회귀로 고정한다.
    #[test]
    fn resolve_validated_project_root_rejects_path_outside_workspace() {
        assert!(crate::resolve_validated_project_root("/etc").is_none());
        assert!(crate::resolve_validated_project_root("/etc/passwd").is_none());
    }

    // ---------------- D1: 스케줄러 패닉 방지 ----------------

    // D1② — `lock_recovering`은 poisoning된 뮤텍스에서도 패닉하지 않고 마지막
    // 값을 그대로 복구한다. `.lock().unwrap()`이었다면 이 테스트의 두 번째
    // `lock_recovering` 호출이 그대로 패닉했을 것이다.
    #[test]
    fn lock_recovering_recovers_value_after_poisoning() {
        let mutex = std::sync::Mutex::new(41);

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = mutex.lock().unwrap();
            *guard += 1;
            panic!("의도적으로 락을 쥔 채 패닉시켜 poisoning을 유발한다");
        }));
        assert!(poisoned.is_err(), "패닉이 실제로 일어나야 poisoning 시나리오가 성립한다");
        assert!(mutex.is_poisoned(), "이 시점에 뮤텍스가 poisoned 상태여야 한다");

        // 여기서 일반 `.lock().unwrap()`을 썼다면 아래 줄이 그대로 패닉한다.
        let recovered = *lock_recovering(&mutex);
        assert_eq!(recovered, 42, "패닉 직전에 쓴 값(42)이 poisoning 이후에도 살아남아야 한다");
    }

    // D1① — `panic_message`가 흔한 두 패닉 페이로드 형태(&str/String)에서 메시지를
    // 뽑아내는지 확인한다. `spawn_scheduler`의 `catch_unwind` 로그 메시지가 이
    // 함수에 의존한다.
    #[test]
    fn panic_message_extracts_str_and_string_payloads() {
        let str_payload = std::panic::catch_unwind(|| panic!("정적 문자열 패닉"))
            .unwrap_err();
        assert_eq!(panic_message(&*str_payload), "정적 문자열 패닉");

        let owned = "동적 String 패닉".to_string();
        let string_payload = std::panic::catch_unwind(move || panic!("{owned}")).unwrap_err();
        assert_eq!(panic_message(&*string_payload), "동적 String 패닉");
    }

    // D1① — `spawn_scheduler`가 쓰는 것과 동일한 `catch_unwind(AssertUnwindSafe(..))`
    // 패턴을 재현해, 한 번의 패닉이 다음 호출을 막지 않음을 고정한다(스케줄러
    // 스레드가 패닉 1회로 영구 종료되지 않는다는 계약의 핵심).
    #[test]
    fn catch_unwind_pattern_survives_panic_and_next_call_still_runs() {
        let panicking_tick = || panic!("이번 tick만 패닉(테스트 유발)");
        let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(panicking_tick));
        assert!(first.is_err(), "패닉이 catch_unwind 밖으로 전파되면 안 된다");

        let ran = std::sync::atomic::AtomicBool::new(false);
        let ok_tick = || ran.store(true, std::sync::atomic::Ordering::SeqCst);
        let second = std::panic::catch_unwind(std::panic::AssertUnwindSafe(ok_tick));
        assert!(second.is_ok(), "패닉 이후에도 다음 호출은 정상 진행돼야 한다");
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst), "다음 tick이 실제로 실행돼야 한다");
    }

    // D1③ — heartbeat: `record_tick_completed`가 `last_tick_at`을 갱신하고,
    // 이 값이 `autonomy_scheduler_health` 커맨드로 그대로 노출된다.
    #[test]
    fn record_tick_completed_updates_last_tick_at_and_scheduler_health_reflects_it() {
        let before = chrono::Utc::now();
        runtime::record_tick_completed();
        let after = chrono::Utc::now();

        let health = autonomy_scheduler_health();
        let last = health.last_tick_at.expect("record_tick_completed 직후에는 값이 있어야 한다");
        assert!(last >= before && last <= after, "heartbeat 시각이 호출 구간 안에 있어야 한다");
        assert_eq!(health.tick_seconds, config::TICK_SECONDS);
    }

    // 비정상 케이스(project_path가 워크스페이스 밖/존재하지 않음) — 신규
    // 커맨드도 기존 3개 커맨드와 동일한 게이트를 통과시키는지 회귀 고정.
    // `autonomy_task_history`는 m4(리뷰 2026-09-17) 이후 `async`라 동기
    // 테스트에서 직접 호출할 수 없다 — `autonomy_list`/`autonomy_list_blocking`과
    // 동일한 관례대로, 실제 검증 로직이 들어 있는 블로킹 버전을 테스트한다.
    #[test]
    fn autonomy_task_history_rejects_path_outside_workspace() {
        let result = autonomy_task_history_blocking("/etc".to_string(), "t-1".to_string(), None);
        assert!(result.is_err());
    }

    // C1 회귀(핵심) — 리뷰 §9-4가 요구한 "구현 단계에서 두 경로 문자열이
    // 일치하는지 실측으로 확인할 것"의 이행. 스캐너(`scan_all_tasks`/
    // `autonomy_list_blocking`)가 만드는 경로 문자열
    // (`dunce::canonicalize(workspace_root)` + `read_dir` 엔트리)과, 커맨드가
    // `task_key_root`로 만드는 TaskKey 경로 문자열이 같은 프로젝트에 대해
    // 바이트 단위로 일치해야 한다. 이게 어긋나면 "지금 실행"이 유령 키를
    // 만들어 스케줄러의 중복 방어를 우회한다(mac/linux에서는 애초에
    // `dunce::canonicalize == std::fs::canonicalize`라 이 테스트가 항상
    // 통과하지만, Windows의 `\\?\` 접두 불일치를 코드로 못 박아 두는 것이
    // 목적이다).
    #[test]
    fn task_key_root_matches_scanner_path_string_for_same_project() {
        let base = std::env::temp_dir().join(format!(
            "malgn-c1-taskkey-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace_root = base.join("workspace");
        let project_dir = workspace_root.join("demo-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        // 스캐너 쪽 문자열 — `config/user_config.rs:221`(workspace_roots_checked
        // 경유)이 워크스페이스 루트에 쓰는 것과 동일한 관용구
        // (`dunce::canonicalize`) 뒤에, `scan_all_tasks`/`autonomy_list_blocking`이
        // 그대로 쓰는 `read_dir` 엔트리 경로를 이어붙인다.
        let scanner_root = dunce::canonicalize(&workspace_root).unwrap();
        let mut scanner_path_str = None;
        for entry in std::fs::read_dir(&scanner_root).unwrap().flatten() {
            if entry.file_name() == "demo-project" {
                scanner_path_str = Some(entry.path().to_string_lossy().to_string());
            }
        }
        let scanner_path_str = scanner_path_str.expect("read_dir에서 project 엔트리를 찾아야 한다");

        // 커맨드 쪽 — `resolve_validated_project_root`(`workspace/tree.rs:39`)가
        // 실제로 반환하는 std `canonicalize()` 결과를 그대로 재현한 뒤(워크스페이스
        // 등록 자체는 이 테스트 범위 밖 — 위 `resolve_validated_project_root_rejects_*`
        // 테스트가 그 가드를 별도로 고정한다) `task_key_root`를 적용한다.
        let std_canonical = project_dir.canonicalize().unwrap();
        let command_key = task_key_root(&std_canonical);

        assert_eq!(
            command_key, scanner_path_str,
            "커맨드가 만드는 TaskKey 경로 문자열이 스캐너 경로 문자열과 일치해야 한다(C1)"
        );

        std::fs::remove_dir_all(&base).ok();
    }

    // M3 회귀 — 신규 등록·편집(FixedTime으로 저장)은 항상 재계산한다.
    #[test]
    fn should_reschedule_on_save_is_true_for_new_or_edited_fixed_time_task() {
        assert!(should_reschedule_on_save(None, config::ScheduleMode::FixedTime));
        assert!(should_reschedule_on_save(
            Some(config::ScheduleMode::FixedTime),
            config::ScheduleMode::FixedTime
        ));
    }

    // 기존 동작 보존 — Interval 모드 그대로 값만 바뀐 저장은 "다음 회차부터
    // 반영"을 유지한다(재계산하지 않는다).
    #[test]
    fn should_reschedule_on_save_is_false_when_interval_mode_unchanged() {
        assert!(!should_reschedule_on_save(
            Some(config::ScheduleMode::Interval),
            config::ScheduleMode::Interval
        ));
    }

    // m1 회귀 — 모드 자체가 바뀌면 방향과 무관하게 재계산한다(FixedTime→
    // Interval 전환 시 옛 고정 시각이 최대 24시간 남는 문제 해소).
    #[test]
    fn should_reschedule_on_save_is_true_when_mode_switches_either_direction() {
        assert!(should_reschedule_on_save(
            Some(config::ScheduleMode::FixedTime),
            config::ScheduleMode::Interval
        ));
        assert!(should_reschedule_on_save(
            Some(config::ScheduleMode::Interval),
            config::ScheduleMode::FixedTime
        ));
    }

    // T11 — `is_wall_clock()` 경유 S1/S2. Hourly→Hourly(분만 변경)·
    // Cron→Cron(표현식만 변경) 편집이 재스케줄되지 않던 결함(§6.2 S1)의
    // 회귀 방지 — PM이 코드로 직접 확인한 실측 버그.
    #[test]
    fn should_reschedule_on_save_is_true_for_hourly_or_cron_edit_even_when_mode_unchanged() {
        assert!(
            should_reschedule_on_save(Some(config::ScheduleMode::Hourly), config::ScheduleMode::Hourly),
            "Hourly→Hourly(분만 변경)도 재스케줄돼야 한다"
        );
        assert!(
            should_reschedule_on_save(Some(config::ScheduleMode::Cron), config::ScheduleMode::Cron),
            "Cron→Cron(표현식만 변경)도 재스케줄돼야 한다"
        );
    }

    #[test]
    fn should_reschedule_on_save_is_true_for_new_hourly_or_cron_task() {
        assert!(should_reschedule_on_save(None, config::ScheduleMode::Hourly));
        assert!(should_reschedule_on_save(None, config::ScheduleMode::Cron));
    }

    // ---------------- validate_schedule_fields(L1, 설계 §7.2) ----------------

    fn task_with_mode(mode: config::ScheduleMode) -> config::AutonomyTaskConfig {
        config::AutonomyTaskConfig {
            id: "t".to_string(),
            name: "n".to_string(),
            prompt: "p".to_string(),
            subagent: None,
            interval: 30,
            schedule_mode: mode,
            at_time: None,
            days: Vec::new(),
            hourly_minute: None,
            cron: None,
            enabled: true,
            timeout: None,
        }
    }

    #[test]
    fn validate_schedule_fields_rejects_hourly_without_minute() {
        let task = task_with_mode(config::ScheduleMode::Hourly);
        let err = validate_schedule_fields(&task).unwrap_err();
        assert_eq!(err, "매시간 모드는 0~59 사이의 분을 지정해야 합니다.");
    }

    #[test]
    fn validate_schedule_fields_rejects_hourly_minute_out_of_range() {
        let mut task = task_with_mode(config::ScheduleMode::Hourly);
        task.hourly_minute = Some(75);
        assert!(validate_schedule_fields(&task).is_err());
    }

    #[test]
    fn validate_schedule_fields_accepts_hourly_minute_zero() {
        let mut task = task_with_mode(config::ScheduleMode::Hourly);
        task.hourly_minute = Some(0);
        assert!(validate_schedule_fields(&task).is_ok(), "0은 유효한 분이다(매시 정각)");
    }

    #[test]
    fn validate_schedule_fields_rejects_cron_without_expression() {
        let task = task_with_mode(config::ScheduleMode::Cron);
        assert!(validate_schedule_fields(&task).is_err());
    }

    #[test]
    fn validate_schedule_fields_accepts_valid_cron_expression() {
        let mut task = task_with_mode(config::ScheduleMode::Cron);
        task.cron = Some("0 9 * * 1-5".to_string());
        assert!(validate_schedule_fields(&task).is_ok());
    }

    #[test]
    fn validate_schedule_fields_is_noop_for_interval_and_fixed_time() {
        assert!(validate_schedule_fields(&task_with_mode(config::ScheduleMode::Interval)).is_ok());
        assert!(validate_schedule_fields(&task_with_mode(config::ScheduleMode::FixedTime)).is_ok());
    }
}
