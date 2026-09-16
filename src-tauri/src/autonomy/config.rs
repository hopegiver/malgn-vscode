// `<project>/.claude/autonomy.json` 스키마·읽기/쓰기·정규화 + 안전 임계값 정본.
//
// 상태를 이 파일에 저장하지 않는다(설계 §1~3) — `lastRunAt/lastStatus/
// lastSummary/history/preventOverlap`은 구조체에서 아예 뺐다. 옛 파일에 이
// 필드가 남아 있어도 `serde`의 기본 동작("모르는 필드 무시")대로 조용히
// 버려지고, 다음 저장(추가·수정·삭제·토글 중 아무거나) 시점에 파일에서
// 사라진다. `#[serde(deny_unknown_fields)]`를 붙이면 레거시 파일이 통째로
// 파싱 실패하므로 절대 붙이지 않는다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------- 안전 임계값 정본(설계 §1) ----------------
// 아래 상수는 이 파일 한 곳에만 존재한다. UI는 이 값을 복제하지 않고
// `malgn_agent_config_get()`의 `limits`로 받아 표시한다 — 값을 바꿀 때 고칠
// 파일이 1개가 되도록.
pub(crate) const TICK_SECONDS: u64 = 10;
pub(crate) const STARTUP_GRACE_MINUTES: u32 = 3;
pub(crate) const MIN_INTERVAL_MINUTES: u32 = 5;
pub(crate) const MAX_INTERVAL_MINUTES: u32 = 10_080; // 7일
pub(crate) const DEFAULT_TIMEOUT_MINUTES: u32 = 60;
pub(crate) const MIN_TIMEOUT_MINUTES: u32 = 1;
pub(crate) const MAX_TIMEOUT_MINUTES: u32 = 480; // 8시간
pub(crate) const DEFAULT_CONCURRENCY: u32 = 3;
pub(crate) const MAX_CONCURRENCY: u32 = 8;
pub(crate) const DEFAULT_LOG_RETENTION_DAYS: u32 = 30;
pub(crate) const MAX_LOG_RETENTION_DAYS: u32 = 365;
/// 앱이 꺼져 있는 동안 지나간 고정시각 회차를, 앱을 켠 뒤 따라잡아 실행해
/// 줄 최대 창(분). 이 값은 반드시 "회차 간 최소 간격"(현재 24시간)보다
/// 작아야 한다 — 그래야 따라잡기가 최대 1회로 자연히 제한된다
/// (`schedule.rs` 상단 불변식 주석 참조).
pub(crate) const MISSED_RUN_GRACE_MINUTES: u32 = 30;

/// 스케줄 모드. 레거시 파일에는 이 키가 없으므로 `default` = `Interval`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleMode {
    /// 이전 실행 "완료" 후 `interval`분 뒤 재실행. 레거시 파일의 유일한 동작이자 기본값.
    #[default]
    Interval,
    /// 로컬 벽시계 기준 고정 시각(+요일)에 실행.
    FixedTime,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AutonomyTaskConfig {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<String>,
    /// 분(分). 이전 실행 "완료" 후 대기 시간(설계 §1 — 완료 기준 interval).
    /// 레거시 파일의 `intervalMinutes`도 읽을 수 있지만, 쓰기는 `interval`
    /// 하나로만 한다(`skip_serializing`이 없어도 alias는 직렬화에 나가지 않는다).
    /// **모드와 무관하게 항상 존재하고 항상 clamp된다** — FixedTime 모드에서는
    /// 사용되지 않을 뿐 제거하지 않는다(레거시 파일 호환·기존 clamp 테스트 보존).
    #[serde(alias = "intervalMinutes")]
    pub interval: u32,
    /// 스케줄 모드. `skip_serializing_if`를 붙이지 않는다 — 다음 저장 때
    /// 파일에 명시적으로 적혀 사람이 읽었을 때 모드가 자명해진다.
    #[serde(default, rename = "scheduleMode")]
    pub schedule_mode: ScheduleMode,
    /// FixedTime 모드의 실행 시각. **기기 로컬 벽시계** "HH:MM"(24시간,
    /// 0패딩). UTC로 변환해 저장하지 않는다. Interval 모드면 `None`.
    #[serde(default, rename = "atTime", skip_serializing_if = "Option::is_none")]
    pub at_time: Option<String>,
    /// FixedTime 모드의 실행 요일. 0=일 … 6=토. **빈 배열 = 매일.**
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub days: Vec<u8>,
    pub enabled: bool,
    /// 분(分). 미지정이면 전역 `autonomy.defaultTimeout` → `DEFAULT_TIMEOUT_MINUTES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl AutonomyTaskConfig {
    /// FixedTime 모드이고 `at_time`이 유효하게 파싱될 때만 spec을 만든다.
    /// `normalize_task()`를 거친 값이면 항상 정규형이지만, 손으로 만든 값이
    /// 들어올 수도 있으므로 여기서도 다시 한번 파싱을 확인한다(방어적).
    pub(crate) fn fixed_time_spec(&self) -> Option<super::schedule::FixedTimeSpec> {
        if self.schedule_mode != ScheduleMode::FixedTime {
            return None;
        }
        let (hour, minute) = super::schedule::parse_hhmm(self.at_time.as_deref()?)?;
        Some(super::schedule::FixedTimeSpec {
            hour,
            minute,
            days: self.days.clone(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AutonomyFile {
    pub version: u32,
    pub tasks: Vec<AutonomyTaskConfig>,
}

impl Default for AutonomyFile {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
        }
    }
}

/// 동시 파일 쓰기 보호 — 프론트엔드 커맨드 호출과 백그라운드 스케줄러 스레드가
/// 같은 `autonomy.json`을 동시에 읽고 쓸 수 있어, 이 모듈이 하는 모든
/// read-modify-write 구간 전체를 하나의 락으로 직렬화한다.
pub(crate) static AUTONOMY_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn autonomy_file_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude").join("autonomy.json")
}

/// 정규화(clamp)는 이 함수 한 곳에 모으고, 읽기 직후와 upsert 양쪽에서 호출한다
/// (사람이 손으로 편집한 파일도 방어). `interval`이 완료 기준으로 바뀌어
/// `MIN_INTERVAL_MINUTES` 하한이 더 중요해졌다 — 5분은 "직전 실행이 끝난 뒤
/// 5분"이라 사실상 연속 실행에 가깝다. `MAX_INTERVAL_MINUTES`(7일)는 오타 방어.
pub(crate) fn normalize_task(mut task: AutonomyTaskConfig) -> AutonomyTaskConfig {
    // ── 기존 2줄: 손대지 않는다 ──────────────────────────────
    task.interval = task.interval.clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
    if let Some(t) = task.timeout {
        task.timeout = Some(t.clamp(MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES));
    }

    // ── 신규: 고정시각 필드 정규화 ───────────────────────────
    task.days.retain(|d| *d <= 6);
    task.days.sort_unstable();
    task.days.dedup();
    // 파싱 불가한 시각은 보존하지 않고 버린다 → FixedTime 모드가 스케줄
    // 불가 상태가 되고, select_due가 `next_run_at == None`으로 이미
    // 제외한다(fail-closed, 설계 §4.5 #14). 여기서 모드를 Interval로
    // "다운그레이드"하지 않는다 — 그러면 하루 1회 의도가 interval(최소
    // 5분) 실행으로 바뀌어 최대 288배 과잉 실행이 된다.
    task.at_time = task
        .at_time
        .as_deref()
        .and_then(super::schedule::parse_hhmm)
        .map(|(h, m)| super::schedule::format_hhmm(h, m));

    task
}

/// `effective_timeout_minutes(task) = clamp(task.timeout ?? cfg.autonomy.defaultTimeout
/// ?? DEFAULT_TIMEOUT_MINUTES, MIN, MAX)` — 함수 하나에만 존재(설계 §1).
/// 로그 보존 일수 정본 — 전역 설정의 `logs.retentionDays`가 없으면
/// `DEFAULT_LOG_RETENTION_DAYS`, 있으면 `MAX_LOG_RETENTION_DAYS`(1년, 오타
/// 방어)로 clamp한다.
pub(crate) fn effective_log_retention_days(configured: Option<u32>) -> u32 {
    configured
        .unwrap_or(DEFAULT_LOG_RETENTION_DAYS)
        .clamp(1, MAX_LOG_RETENTION_DAYS)
}

pub(crate) fn effective_timeout_minutes(
    task: &AutonomyTaskConfig,
    global_default_timeout: Option<u32>,
) -> u32 {
    let raw = task
        .timeout
        .or(global_default_timeout)
        .unwrap_or(DEFAULT_TIMEOUT_MINUTES);
    raw.clamp(MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES)
}

pub(crate) fn read_autonomy_file(project_root: &Path) -> AutonomyFile {
    let path = autonomy_file_path(project_root);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return AutonomyFile::default();
    };
    let mut file: AutonomyFile = serde_json::from_str(&content).unwrap_or_default();
    for task in file.tasks.iter_mut() {
        *task = normalize_task(task.clone());
    }
    file
}

pub(crate) fn write_autonomy_file(project_root: &Path, file: &AutonomyFile) -> Result<(), String> {
    let path = autonomy_file_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    }
    let content = serde_json::to_string_pretty(file)
        .map_err(|e| format!("설정을 직렬화하지 못했습니다: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("설정을 저장하지 못했습니다: {e}"))
}

/// id가 이미 있으면 업데이트, 없으면 새로 추가한다. 저장 경로(커맨드)와
/// 스케줄러 경로 양쪽에서 이 규칙이 항상 지켜지도록 정규화를 여기 한 곳에서 한다.
pub(crate) fn upsert_task(file: &mut AutonomyFile, task: AutonomyTaskConfig) {
    let normalized = normalize_task(task);
    match file.tasks.iter_mut().find(|t| t.id == normalized.id) {
        Some(existing) => *existing = normalized,
        None => file.tasks.push(normalized),
    }
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
            interval: 30,
            schedule_mode: ScheduleMode::Interval,
            at_time: None,
            days: Vec::new(),
            enabled: true,
            timeout: None,
        }
    }

    // 이관 — 문서화된 camelCase 대신 이제는 `interval` 하나만 나가는지 +
    // 레거시 필드가 출력에 없는지(설계 §3 자연 마이그레이션).
    #[test]
    fn task_json_roundtrip_uses_new_interval_field_without_legacy_names() {
        let task = sample_task("task-1");
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"interval\":30"));
        assert!(!json.contains("intervalMinutes"));
        assert!(!json.contains("preventOverlap"));
        assert!(!json.contains("lastRunAt"));
        assert!(!json.contains("lastStatus"));
        assert!(!json.contains("lastSummary"));
        assert!(!json.contains("history"));

        let roundtripped: AutonomyTaskConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped, task);
    }

    #[test]
    fn task_deserializes_with_only_required_fields_missing_optionals_default() {
        let json = r#"{
            "id": "task-2",
            "name": "이름",
            "prompt": "프롬프트",
            "interval": 10,
            "enabled": true
        }"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(task.subagent, None);
        assert_eq!(task.timeout, None);
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

        assert_eq!(
            file.tasks.len(),
            1,
            "같은 id는 새로 추가되지 않고 갱신되어야 한다"
        );
        assert_eq!(file.tasks[0].name, "이름이 바뀐 작업");
        assert!(!file.tasks[0].enabled);
    }

    #[test]
    fn upsert_clamps_interval_below_minimum_to_five() {
        let mut file = AutonomyFile::default();
        let mut task = sample_task("short-interval");
        task.interval = 1;
        upsert_task(&mut file, task);
        assert_eq!(file.tasks[0].interval, MIN_INTERVAL_MINUTES);
    }

    #[test]
    fn upsert_clamps_interval_above_maximum_to_seven_days() {
        let mut file = AutonomyFile::default();
        let mut task = sample_task("long-interval");
        task.interval = 999_999;
        upsert_task(&mut file, task);
        assert_eq!(file.tasks[0].interval, MAX_INTERVAL_MINUTES);
    }

    // 신규(최소) — `intervalMinutes` alias 수용.
    #[test]
    fn task_deserializes_legacy_interval_minutes_alias() {
        let json = r#"{
            "id": "legacy-1",
            "name": "레거시",
            "prompt": "프롬프트",
            "intervalMinutes": 15,
            "enabled": true
        }"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(task.interval, 15);
    }

    // 신규(최소) — 레거시 필드가 있는 JSON을 읽고 쓰면 사라지는지.
    #[test]
    fn task_with_legacy_fields_loses_them_after_write() {
        let json = r#"{
            "id": "legacy-2",
            "name": "레거시",
            "prompt": "프롬프트",
            "intervalMinutes": 20,
            "enabled": true,
            "preventOverlap": true,
            "lastRunAt": "2026-01-01T00:00:00Z",
            "lastStatus": "success",
            "lastSummary": "이전 결과",
            "history": [{"at": "2026-01-01T00:00:00Z", "result": "success"}]
        }"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        let written = serde_json::to_string(&task).unwrap();
        assert!(!written.contains("preventOverlap"));
        assert!(!written.contains("lastRunAt"));
        assert!(!written.contains("lastStatus"));
        assert!(!written.contains("lastSummary"));
        assert!(!written.contains("history"));
        assert_eq!(task.interval, 20);
    }

    // 신규(최소) — `effective_timeout_minutes` 우선순위 3단(task > 전역 > 상수)
    // + 상하한 clamp.
    #[test]
    fn effective_timeout_prefers_task_value_over_global_default() {
        let mut task = sample_task("t");
        task.timeout = Some(15);
        assert_eq!(effective_timeout_minutes(&task, Some(90)), 15);
    }

    #[test]
    fn effective_timeout_falls_back_to_global_default_when_task_unset() {
        let task = sample_task("t");
        assert_eq!(effective_timeout_minutes(&task, Some(90)), 90);
    }

    #[test]
    fn effective_timeout_falls_back_to_constant_default_when_nothing_set() {
        let task = sample_task("t");
        assert_eq!(effective_timeout_minutes(&task, None), DEFAULT_TIMEOUT_MINUTES);
    }

    #[test]
    fn effective_timeout_clamps_out_of_range_values() {
        let mut task = sample_task("t");
        task.timeout = Some(999_999);
        assert_eq!(effective_timeout_minutes(&task, None), MAX_TIMEOUT_MINUTES);
    }

    // "삭제가 안 되는 것 같다" 조사용 회귀 테스트 — 실제 임시 디렉터리에 진짜
    // 파일 I/O로 저장→삭제를 왕복시켜본다(`mod.rs`의 `autonomy_save_task`/
    // `autonomy_delete_task`가 하는 일과 완전히 동일한 순서: 읽기 → upsert/retain
    // → 쓰기). `resolve_validated_project_root`의 workspace 경로 검증은 별도
    // 관심사라 여기서는 다루지 않는다(그쪽은 mod.rs의 기존 테스트가 담당).
    #[test]
    fn save_then_delete_roundtrip_actually_removes_task_from_disk() {
        let dir = std::env::temp_dir().join(format!(
            "malgn-autonomy-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // 1) 저장(autonomy_save_task와 동일한 순서: read -> upsert -> write)
        let mut file = read_autonomy_file(&dir);
        upsert_task(&mut file, sample_task("roundtrip-1"));
        write_autonomy_file(&dir, &file).expect("첫 저장은 성공해야 한다");

        // 2) 실제로 디스크에 반영됐는지 새로 읽어 확인
        let reloaded = read_autonomy_file(&dir);
        assert_eq!(reloaded.tasks.len(), 1, "저장 직후 파일에서 다시 읽으면 task가 있어야 한다");
        assert_eq!(reloaded.tasks[0].id, "roundtrip-1");

        // 3) 삭제(autonomy_delete_task와 동일한 순서: read -> retain -> write)
        let mut file = read_autonomy_file(&dir);
        file.tasks.retain(|t| t.id != "roundtrip-1");
        write_autonomy_file(&dir, &file).expect("삭제 후 저장은 성공해야 한다");

        // 4) 다시 읽어서 실제로 사라졌는지 확인 — 여기서 남아 있으면 "삭제가 안
        // 된다"는 사용자 보고가 파일 계층 버그임이 확정된다.
        let after_delete = read_autonomy_file(&dir);
        assert_eq!(after_delete.tasks.len(), 0, "삭제 후 다시 읽으면 task가 사라져 있어야 한다");

        std::fs::remove_dir_all(&dir).ok();
    }

    // ---------------- 고정시각 모드(설계 §6~§8) ----------------

    #[test]
    fn legacy_json_without_schedule_fields_defaults_to_interval_mode() {
        let json = r#"{
            "id": "legacy-3",
            "name": "레거시",
            "prompt": "프롬프트",
            "interval": 10,
            "enabled": true
        }"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(task.schedule_mode, ScheduleMode::Interval);
        assert_eq!(task.at_time, None);
        assert!(task.days.is_empty());
    }

    #[test]
    fn schedule_mode_serializes_as_camel_case_fixed_time() {
        let mut task = sample_task("fixed-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("09:00".to_string());
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"scheduleMode\":\"fixedTime\""));
    }

    #[test]
    fn interval_mode_task_omits_at_time_and_days_from_json() {
        let task = sample_task("interval-1"); // at_time: None, days: []
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("atTime"));
        assert!(!json.contains("\"days\""));
    }

    #[test]
    fn normalize_sorts_dedups_and_drops_out_of_range_days() {
        let mut task = sample_task("days-1");
        task.days = vec![4, 2, 2, 9];
        let normalized = normalize_task(task);
        assert_eq!(normalized.days, vec![2, 4]);
    }

    #[test]
    fn normalize_canonicalizes_at_time_to_zero_padded() {
        let mut task = sample_task("time-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("9:5".to_string());
        let normalized = normalize_task(task);
        assert_eq!(normalized.at_time, None, "\"9:5\"는 분이 2자리가 아니라 거부돼야 한다");

        let mut task2 = sample_task("time-2");
        task2.schedule_mode = ScheduleMode::FixedTime;
        task2.at_time = Some("9:05".to_string());
        let normalized2 = normalize_task(task2);
        assert_eq!(normalized2.at_time, Some("09:05".to_string()));
    }

    #[test]
    fn normalize_clears_unparseable_at_time_without_downgrading_mode() {
        let mut task = sample_task("bad-time-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("25:00".to_string());
        let normalized = normalize_task(task);
        assert_eq!(normalized.at_time, None, "파싱 불가 시각은 버려져야 한다(fail-closed)");
        assert_eq!(
            normalized.schedule_mode,
            ScheduleMode::FixedTime,
            "Interval로 다운그레이드하면 안 된다 — 과잉 실행 위험"
        );
    }

    #[test]
    fn normalize_still_clamps_interval_in_fixed_time_mode() {
        let mut task = sample_task("fixed-clamp-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("09:00".to_string());
        task.interval = 1;
        let normalized = normalize_task(task);
        assert_eq!(normalized.interval, MIN_INTERVAL_MINUTES);
    }

    #[test]
    fn fixed_time_task_roundtrip_preserves_mode_time_and_days() {
        let mut task = sample_task("fixed-roundtrip-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("09:00".to_string());
        task.days = vec![2, 4];
        let json = serde_json::to_string(&task).unwrap();
        let roundtripped: AutonomyTaskConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped, task);
    }
}
