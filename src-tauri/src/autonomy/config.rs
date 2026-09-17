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
/// `Deserialize`는 derive에서 제외하고 아래 수동 구현을 쓴다 — 미지 문자열
/// (미래 버전이 쓴 값)을 흡수해 파일 전체 파싱 실패로 인한 task 전멸을
/// 막기 위함(§8.3).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleMode {
    /// 이전 실행 "완료" 후 `interval`분 뒤 재실행. 레거시 파일의 유일한 동작이자 기본값.
    #[default]
    Interval,
    /// 로컬 벽시계 기준 고정 시각(+요일)에 실행. 매일/매주 두 UI 탭이 이 모드
    /// 하나를 공유한다(`days` 빈 배열=매일 / 선택=매주).
    FixedTime,
    /// 매시 `hourly_minute`분(로컬 벽시계)에 실행. 신규 의존성 없이
    /// `chrono`만으로 계산한다(`schedule::next_hourly`).
    Hourly,
    /// 표준 5필드 cron 표현식(`분 시 일 월 요일`, 로컬 벽시계) 기준 실행
    /// (`schedule::next_cron`, `cron-parser` 크레이트).
    Cron,
}

impl ScheduleMode {
    /// 다음 실행 시각이 "벽시계 약속"에서 나오는 모드인가(= 직전 완료
    /// 시각으로부터의 카운트다운이 아닌가). 저장 직후 즉시 재계산과
    /// 중지→재개 재계산이 필요한 모드가 정확히 이 집합이다 — `mod.rs`의
    /// `should_reschedule_on_save`/`autonomy_set_enabled`가 공용으로 쓴다
    /// (두 곳에 같은 질문이 각각 다른 형태로 박혀 있다가 어긋났던 것이
    /// 과거 결함의 뿌리였다).
    ///
    /// `_ =>` 와일드카드를 쓰지 않는다 — 모드를 추가하면 **여기서 컴파일
    /// 에러가 나서** 추가자가 "이 모드는 저장·재개 때 재계산해야 하는가"를
    /// 반드시 한 번 판단하게 만드는 것이 이 match의 목적이다.
    pub(crate) fn is_wall_clock(self) -> bool {
        match self {
            ScheduleMode::Interval => false,
            ScheduleMode::FixedTime | ScheduleMode::Hourly | ScheduleMode::Cron => true,
        }
    }
}

/// 알려진 값 4개는 그대로 매핑하고, 그 외 미지 문자열(미래 버전이 쓴 값 —
/// 예: `"monthly"`)은 `Cron`(표현식 없음, `cron` 필드는 별도로 `None`이 됨)
/// 으로 떨어뜨린다. `Interval`로 떨어뜨리면 정반대(최대 5분마다 무인 실행)가
/// 되므로 절대 그렇게 하지 않는다 — fail-closed(§8.3-3, 흡수 대상을 `Cron`
/// 으로 고른 이유: 다른 후보는 사용자가 편집 모달을 열고 저장만 눌러도
/// 유효한 스케줄로 조용히 굳어지지만, `Cron`은 빈 표현식을 저장 시점(L1)에
/// 거부하므로 쓰기 게이트에서도 fail-closed다).
///
/// 이 구현은 "키는 있는데 값이 미지"일 때만 관여한다 — **키 자체가 없는
/// 레거시 파일은 필드의 `#[serde(default)]`가 그대로 `Default`(=`Interval`)
/// 를 채운다.** 두 경로를 헷갈리면 레거시 파일이 전부 `Cron`으로 바뀐다.
impl<'de> Deserialize<'de> for ScheduleMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "interval" => ScheduleMode::Interval,
            "fixedTime" => ScheduleMode::FixedTime,
            "hourly" => ScheduleMode::Hourly,
            "cron" => ScheduleMode::Cron,
            _ => ScheduleMode::Cron,
        })
    }
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
    /// Hourly 모드의 실행 "분"(0..=59). **매시 이 분에** 실행한다(기기 로컬
    /// 벽시계 기준 — 상대간격 60분과 달리 정각 정렬). 범위를 벗어난 값은
    /// 정규화에서 버린다(clamp하지 않는다 — 사용자가 지정하지 않은 분에
    /// 실행하지 않기 위함, `at_time`과 같은 fail-closed 규칙).
    #[serde(default, rename = "hourlyMinute", skip_serializing_if = "Option::is_none")]
    pub hourly_minute: Option<u8>,
    /// Cron 모드의 표준 5필드 표현식(`분 시 일 월 요일`). **기기 로컬
    /// 벽시계** 기준으로 해석한다(`at_time`과 동일 원칙 — UTC로 변환해
    /// 저장하지 않는다). 정규화를 통과하지 못한 값은 보존하지 않는다
    /// (fail-closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    pub enabled: bool,
    /// 분(分). 미지정이면 전역 `autonomy.defaultTimeout` → `DEFAULT_TIMEOUT_MINUTES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl AutonomyTaskConfig {
    /// FixedTime 모드이고 `at_time`이 유효하게 파싱될 때만 spec을 만든다.
    /// `normalize_task()`를 거친 값이면 항상 정규형이지만, 손으로 만든 값이
    /// 들어올 수도 있으므로 여기서도 다시 한번 파싱을 확인한다(방어적).
    ///
    /// 이 부등호(`!=`)는 화이트리스트가 아니라 **모드 게이트**다 — 신규
    /// 모드(`Hourly`/`Cron`)가 추가돼도 자동으로 `None`이 되어 옳다(`Hourly`
    /// task가 `at_time` 잔여값을 갖고 있어도 FixedTime 사양으로 오독되지
    /// 않는다). 다음에 모드를 또 추가해도 이 함수는 고칠 필요가 없다 —
    /// 짝이 되는 `hourly_minute_spec()`/`cron_expr()`가 같은 모양의 게이트를
    /// 아래에 둔다.
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

    /// Hourly 모드이고 분이 0..=59일 때만 값을 준다(`fixed_time_spec`과 같은
    /// 부등호 게이트).
    pub(crate) fn hourly_minute_spec(&self) -> Option<u8> {
        if self.schedule_mode != ScheduleMode::Hourly {
            return None;
        }
        self.hourly_minute.filter(|m| *m <= 59)
    }

    /// Cron 모드이고 표현식이 비어 있지 않을 때만 값을 준다(`fixed_time_spec`
    /// 과 같은 부등호 게이트). 형태 검증(보안 가드)은 `schedule::next_cron_in`
    /// 이 파싱 직전에 별도로 한 번 더 한다 — 이 접근자는 "값이 있는가"만 본다.
    pub(crate) fn cron_expr(&self) -> Option<&str> {
        if self.schedule_mode != ScheduleMode::Cron {
            return None;
        }
        match self.cron.as_deref() {
            Some(s) if !s.trim().is_empty() => Some(s),
            _ => None,
        }
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

    // ── 신규 A: 모드가 소유하지 않는 "옵셔널 스케줄 필드"는 버린다 ──
    //   모드별로 필드가 정확히 하나만 남는다 → 손으로 열어 본 파일이
    //   자명해지고, 미지 모드 흡수(Deserialize impl)가 "스케줄 없음"임을
    //   데이터로 보장한다. `interval`은 이 규칙에서 제외한다 — 레거시 호환
    //   때문에 항상 존재·항상 clamp라는 기존 계약(위 두 줄)이 따로 있다.
    if task.schedule_mode != ScheduleMode::FixedTime {
        task.at_time = None;
        task.days.clear();
    }
    if task.schedule_mode != ScheduleMode::Hourly {
        task.hourly_minute = None;
    }
    if task.schedule_mode != ScheduleMode::Cron {
        task.cron = None;
    }

    // ── 신규 B: 소유한 필드만 정규화(fail-closed, 모드 다운그레이드 금지) ──
    //   범위 밖 분·파싱 불가 cron은 버려질 뿐 모드를 Interval로 내리지
    //   않는다 — 그러면 하루 한두 번 의도가 최소 5분 간격 실행으로 바뀌어
    //   과잉 실행이 된다(위 at_time과 같은 원칙).
    if task.schedule_mode == ScheduleMode::Hourly {
        task.hourly_minute = task.hourly_minute.filter(|m| *m <= 59);
    }
    if task.schedule_mode == ScheduleMode::Cron {
        task.cron = task
            .cron
            .as_deref()
            .and_then(super::schedule::normalize_cron_expr);
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
            schedule_mode: ScheduleMode::Interval,
            at_time: None,
            days: Vec::new(),
            hourly_minute: None,
            cron: None,
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

    // 픽스처 수정(설계 §8.2) — 이 테스트의 진짜 의도는 "요일 살균"이고
    // `days`는 FixedTime 모드에서만 의미를 갖는다. 신규 A(모드가 소유하지
    // 않는 필드는 버림) 아래서 Interval 모드로 두면 `days`가 무조건 `[]`가
    // 되어 정렬/중복제거/범위제거를 더 이상 검증하지 못하므로 FixedTime으로
    // 바꾼다 — 동작 회귀가 아니라 테스트가 검증하려던 상황을 정확히 하는
    // 수정이다.
    #[test]
    fn normalize_sorts_dedups_and_drops_out_of_range_days() {
        let mut task = sample_task("days-1");
        task.schedule_mode = ScheduleMode::FixedTime;
        task.at_time = Some("09:00".to_string());
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

    // ---------------- Hourly/Cron 모드(설계 §6~§8, 이번 라운드) ----------------

    #[test]
    fn is_wall_clock_is_true_for_all_wall_clock_modes_and_false_for_interval() {
        assert!(!ScheduleMode::Interval.is_wall_clock());
        assert!(ScheduleMode::FixedTime.is_wall_clock());
        assert!(ScheduleMode::Hourly.is_wall_clock());
        assert!(ScheduleMode::Cron.is_wall_clock());
    }

    #[test]
    fn hourly_minute_spec_gates_on_mode_and_range() {
        let mut task = sample_task("hourly-spec-1");
        task.schedule_mode = ScheduleMode::Hourly;
        task.hourly_minute = Some(30);
        assert_eq!(task.hourly_minute_spec(), Some(30));

        task.hourly_minute = Some(75); // u8이라 값 자체는 담기지만 범위 밖
        assert_eq!(task.hourly_minute_spec(), None, "0..=59 밖은 게이트에서 걸러야 한다");

        task.hourly_minute = Some(30);
        task.schedule_mode = ScheduleMode::Interval;
        assert_eq!(task.hourly_minute_spec(), None, "모드가 Hourly가 아니면 값이 있어도 None");
    }

    #[test]
    fn cron_expr_gates_on_mode_and_emptiness() {
        let mut task = sample_task("cron-spec-1");
        task.schedule_mode = ScheduleMode::Cron;
        task.cron = Some("0 9 * * 1-5".to_string());
        assert_eq!(task.cron_expr(), Some("0 9 * * 1-5"));

        task.cron = Some("   ".to_string());
        assert_eq!(task.cron_expr(), None, "공백만 있는 표현식은 없는 것과 같다");

        task.cron = None;
        assert_eq!(task.cron_expr(), None);

        task.cron = Some("0 9 * * 1-5".to_string());
        task.schedule_mode = ScheduleMode::FixedTime;
        assert_eq!(task.cron_expr(), None, "모드가 Cron이 아니면 값이 있어도 None");
    }

    #[test]
    fn schedule_mode_serializes_as_camel_case_hourly_and_cron() {
        let mut hourly = sample_task("hourly-json-1");
        hourly.schedule_mode = ScheduleMode::Hourly;
        hourly.hourly_minute = Some(30);
        let json = serde_json::to_string(&hourly).unwrap();
        assert!(json.contains("\"scheduleMode\":\"hourly\""));
        assert!(json.contains("\"hourlyMinute\":30"));

        let mut cron = sample_task("cron-json-1");
        cron.schedule_mode = ScheduleMode::Cron;
        cron.cron = Some("0 9 * * 1-5".to_string());
        let json = serde_json::to_string(&cron).unwrap();
        assert!(json.contains("\"scheduleMode\":\"cron\""));
        assert!(json.contains("\"cron\":\"0 9 * * 1-5\""));
    }

    // 권장 테스트 — `hourlyMinute: 0`이 `skip_serializing_if`에 걸려 사라지면
    // "매시 정각"을 저장할 수 없게 된다. `Some(0)`은 반드시 직렬화돼야 한다.
    #[test]
    fn hourly_minute_zero_is_not_skipped_by_serde() {
        let mut task = sample_task("hourly-zero-1");
        task.schedule_mode = ScheduleMode::Hourly;
        task.hourly_minute = Some(0);
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"hourlyMinute\":0"), "Some(0)은 skip되면 안 된다: {json}");
    }

    // T13 — normalize_task 신규 A: Hourly task의 atTime/days 잔여값은
    // 제거되고, FixedTime task의 days는 보존된다(모드가 소유한 필드만
    // 남긴다는 규칙이 서로 다른 모드에서 정확히 반대로 동작하는지 확인).
    #[test]
    fn normalize_drops_fields_not_owned_by_current_mode_but_keeps_owned_ones() {
        let mut hourly_task = sample_task("hourly-residual-1");
        hourly_task.schedule_mode = ScheduleMode::Hourly;
        hourly_task.hourly_minute = Some(15);
        hourly_task.at_time = Some("09:00".to_string()); // FE 잔여값 가정
        hourly_task.days = vec![2, 4]; // FE 잔여값 가정
        let normalized = normalize_task(hourly_task);
        assert_eq!(normalized.at_time, None, "Hourly가 소유하지 않는 at_time은 버려져야 한다");
        assert!(normalized.days.is_empty(), "Hourly가 소유하지 않는 days는 버려져야 한다");
        assert_eq!(normalized.hourly_minute, Some(15), "Hourly가 소유한 필드는 보존돼야 한다");

        let mut fixed_task = sample_task("fixed-residual-1");
        fixed_task.schedule_mode = ScheduleMode::FixedTime;
        fixed_task.at_time = Some("09:00".to_string());
        fixed_task.days = vec![2, 4];
        fixed_task.cron = Some("0 9 * * 1-5".to_string()); // FE 잔여값 가정
        let normalized = normalize_task(fixed_task);
        assert_eq!(normalized.days, vec![2, 4], "FixedTime이 소유한 days는 보존돼야 한다");
        assert_eq!(normalized.cron, None, "FixedTime이 소유하지 않는 cron은 버려져야 한다");
    }

    // T12 — normalize_task fail-closed: 범위 밖 분 / 깨진 cron은 각각 None이
    // 되면서도 모드는 유지된다(Interval로 다운그레이드하지 않는다).
    #[test]
    fn normalize_clears_out_of_range_hourly_minute_without_downgrading_mode() {
        let mut task = sample_task("bad-hourly-1");
        task.schedule_mode = ScheduleMode::Hourly;
        task.hourly_minute = Some(75);
        let normalized = normalize_task(task);
        assert_eq!(normalized.hourly_minute, None, "범위 밖 분은 버려져야 한다(fail-closed)");
        assert_eq!(normalized.schedule_mode, ScheduleMode::Hourly, "Interval로 다운그레이드하면 안 된다");
    }

    #[test]
    fn normalize_clears_unparseable_cron_without_downgrading_mode() {
        let mut task = sample_task("bad-cron-1");
        task.schedule_mode = ScheduleMode::Cron;
        task.cron = Some("not a cron".to_string());
        let normalized = normalize_task(task);
        assert_eq!(normalized.cron, None, "파싱 불가 cron은 버려져야 한다(fail-closed)");
        assert_eq!(normalized.schedule_mode, ScheduleMode::Cron, "Interval로 다운그레이드하면 안 된다");
    }

    #[test]
    fn normalize_collapses_internal_whitespace_in_valid_cron() {
        let mut task = sample_task("cron-whitespace-1");
        task.schedule_mode = ScheduleMode::Cron;
        task.cron = Some("0   9  * * 1-5".to_string());
        let normalized = normalize_task(task);
        assert_eq!(normalized.cron, Some("0 9 * * 1-5".to_string()));
    }

    // T14(핵심) — 미지 scheduleMode 흡수: 파일 전체가 파싱 실패로 tasks=0이
    // 되던 실측 버그(§1)가 이번 수동 Deserialize로 재발하지 않는지 확인한다.
    // 미지 값 1건이 섞여 있어도 다른 task는 전부 살아남아야 하고, 그 1건은
    // Cron(표현식 없음)으로 흡수되며, 키가 아예 없는 레거시 task는 여전히
    // Interval이어야 한다(두 경로를 헷갈리면 레거시 파일이 전부 Cron으로
    // 바뀐다).
    #[test]
    fn unknown_schedule_mode_is_absorbed_as_cron_without_losing_other_tasks() {
        let json = r#"{
            "version": 1,
            "tasks": [
                { "id": "legacy", "name": "레거시", "prompt": "p", "interval": 10, "enabled": true },
                { "id": "future", "name": "미래", "prompt": "p", "interval": 10, "scheduleMode": "monthly", "enabled": true },
                { "id": "known", "name": "알려짐", "prompt": "p", "interval": 10, "scheduleMode": "hourly", "hourlyMinute": 15, "enabled": true }
            ]
        }"#;
        let file: AutonomyFile = serde_json::from_str(json).expect("미지 값이 있어도 파일 전체 파싱은 성공해야 한다");
        assert_eq!(file.tasks.len(), 3, "다른 task가 함께 소실되면 안 된다");

        let legacy = file.tasks.iter().find(|t| t.id == "legacy").unwrap();
        assert_eq!(legacy.schedule_mode, ScheduleMode::Interval, "키가 없는 레거시 task는 여전히 Interval이어야 한다");

        let future = file.tasks.iter().find(|t| t.id == "future").unwrap();
        assert_eq!(future.schedule_mode, ScheduleMode::Cron, "미지 값은 Cron(표현식 없음)으로 흡수돼야 한다");
        assert_eq!(future.cron, None);

        let known = file.tasks.iter().find(|t| t.id == "known").unwrap();
        assert_eq!(known.schedule_mode, ScheduleMode::Hourly);
        assert_eq!(known.hourly_minute, Some(15));
    }

    // §8.3 마이그레이션 표 — `manual`(초판 설계가 유출됐을 경우)도 같은
    // 흡수 경로를 탄다.
    #[test]
    fn unknown_manual_value_is_absorbed_as_cron() {
        let json = r#"{"id":"x","name":"n","prompt":"p","interval":10,"scheduleMode":"manual","enabled":true}"#;
        let task: AutonomyTaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(task.schedule_mode, ScheduleMode::Cron);
    }
}
