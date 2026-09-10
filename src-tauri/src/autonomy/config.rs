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
    #[serde(alias = "intervalMinutes")]
    pub interval: u32,
    pub enabled: bool,
    /// 분(分). 미지정이면 전역 `autonomy.defaultTimeout` → `DEFAULT_TIMEOUT_MINUTES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
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
    task.interval = task.interval.clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
    if let Some(t) = task.timeout {
        task.timeout = Some(t.clamp(MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES));
    }
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
}
