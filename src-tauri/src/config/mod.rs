// 전역 설정 파일(`~/.claude/malgn-agent.json`) 진입점. 실제 파싱·검증·캐시는
// `user_config.rs`에 있고, 이 파일은 ① 크레이트 전역에서 쓰는 얇은 진입점
// (`load`, `workspace_roots_checked`) ② 설정 화면이 상시 표시하는 상태 커맨드
// (`malgn_agent_config_get`)만 담는다.

mod user_config;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) use user_config::UserConfig;

pub(crate) fn load() -> Result<UserConfig, String> {
    user_config::load()
}

/// `lib.rs::workspace_roots()`와 `autonomy::autonomy_list()`가 함께 쓰는 정본.
/// 손상된 설정은 `Err`로 그대로 올린다 — 호출자별로 흡수 방식이 다르다
/// (`lib.rs`는 `unwrap_or_default()`로 fail-closed, `autonomy_list()`는 `Err`를
/// 프론트에 그대로 전달).
pub(crate) fn workspace_roots_checked() -> Result<Vec<PathBuf>, String> {
    let cfg = load()?;
    let (valid, _warnings) = user_config::validate_workspace_entries(&cfg.workspaces);
    Ok(valid)
}

#[derive(Serialize, Clone, Debug)]
pub struct AutonomyDefaultsSection {
    pub concurrency: u32,
    #[serde(rename = "defaultTimeout")]
    pub default_timeout: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct LogsSection {
    #[serde(rename = "retentionDays")]
    pub retention_days: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct LimitsSection {
    #[serde(rename = "minInterval")]
    pub min_interval: u32,
    #[serde(rename = "maxInterval")]
    pub max_interval: u32,
    #[serde(rename = "minTimeout")]
    pub min_timeout: u32,
    #[serde(rename = "maxTimeout")]
    pub max_timeout: u32,
    #[serde(rename = "maxConcurrency")]
    pub max_concurrency: u32,
    #[serde(rename = "startupGraceMinutes")]
    pub startup_grace_minutes: u32,
    #[serde(rename = "missedRunGraceMinutes")]
    pub missed_run_grace_minutes: u32,
}

/// 안전 임계값 정본은 `autonomy::config`(설계 §1) 한 곳뿐이다 — 여기서는
/// 그 값을 그대로 옮겨 담기만 한다(값을 바꿀 때 고칠 파일이 1개가 되도록).
fn limits() -> LimitsSection {
    use crate::autonomy::config as autonomy_config;
    LimitsSection {
        min_interval: autonomy_config::MIN_INTERVAL_MINUTES,
        max_interval: autonomy_config::MAX_INTERVAL_MINUTES,
        min_timeout: autonomy_config::MIN_TIMEOUT_MINUTES,
        max_timeout: autonomy_config::MAX_TIMEOUT_MINUTES,
        max_concurrency: autonomy_config::MAX_CONCURRENCY,
        startup_grace_minutes: autonomy_config::STARTUP_GRACE_MINUTES,
        missed_run_grace_minutes: autonomy_config::MISSED_RUN_GRACE_MINUTES,
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct MalgnAgentConfigStatus {
    pub ok: bool,
    pub error: Option<String>,
    #[serde(rename = "configPath")]
    pub config_path: String,
    #[serde(rename = "fileExists")]
    pub file_exists: bool,
    pub workspaces: Vec<String>,
    pub warnings: Vec<String>,
    pub autonomy: AutonomyDefaultsSection,
    pub logs: LogsSection,
    pub limits: LimitsSection,
}

/// 설정 화면이 상시 표시하는 상태. 설정이 손상됐을 때 `ok:false`+`error`로
/// 사용자에게 보이는 오류 표면 3곳 중 하나(나머지는 `autonomy_list()`의 `Err`,
/// 스케줄러 tick의 1회성 `eprintln!` — 설계 §5-3).
#[tauri::command]
pub fn malgn_agent_config_get() -> MalgnAgentConfigStatus {
    use crate::autonomy::config as autonomy_config;

    let path = user_config::config_file_path();
    let config_path = path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let file_exists = path.as_ref().map(|p| p.is_file()).unwrap_or(false);

    match load() {
        Ok(cfg) => {
            let (valid, warnings) = user_config::validate_workspace_entries(&cfg.workspaces);
            MalgnAgentConfigStatus {
                ok: true,
                error: None,
                config_path,
                file_exists,
                workspaces: valid
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                warnings,
                autonomy: AutonomyDefaultsSection {
                    concurrency: cfg
                        .autonomy
                        .concurrency
                        .unwrap_or(autonomy_config::DEFAULT_CONCURRENCY),
                    default_timeout: cfg
                        .autonomy
                        .default_timeout
                        .unwrap_or(autonomy_config::DEFAULT_TIMEOUT_MINUTES),
                },
                logs: LogsSection {
                    retention_days: autonomy_config::effective_log_retention_days(
                        cfg.logs.retention_days,
                    ),
                },
                limits: limits(),
            }
        }
        Err(e) => MalgnAgentConfigStatus {
            ok: false,
            error: Some(e),
            config_path,
            file_exists,
            workspaces: Vec::new(),
            warnings: Vec::new(),
            autonomy: AutonomyDefaultsSection {
                concurrency: autonomy_config::DEFAULT_CONCURRENCY,
                default_timeout: autonomy_config::DEFAULT_TIMEOUT_MINUTES,
            },
            logs: LogsSection {
                retention_days: autonomy_config::DEFAULT_LOG_RETENTION_DAYS,
            },
            limits: limits(),
        },
    }
}

// ---------------- 쓰기 커맨드(설계 §13 확장) ----------------
// 프론트에서 오는 원시 입력 — 필드명은 camelCase로 (역)직렬화된다.

#[derive(Deserialize, Clone, Debug)]
pub struct MalgnAgentConfigAutonomyInput {
    pub concurrency: u32,
    #[serde(rename = "defaultTimeout")]
    pub default_timeout: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MalgnAgentConfigLogsInput {
    #[serde(rename = "retentionDays")]
    pub retention_days: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MalgnAgentConfigInput {
    pub workspaces: Vec<String>,
    pub autonomy: MalgnAgentConfigAutonomyInput,
    pub logs: MalgnAgentConfigLogsInput,
}

/// concurrency ∈ [1, MAX_CONCURRENCY], default_timeout ∈ [MIN_TIMEOUT_MINUTES,
/// MAX_TIMEOUT_MINUTES]로 clamp하는 순수 함수 — 단위 테스트 대상.
fn clamp_autonomy_defaults(concurrency: u32, default_timeout: u32) -> (u32, u32) {
    use crate::autonomy::config as autonomy_config;
    let clamped_concurrency = concurrency.clamp(1, autonomy_config::MAX_CONCURRENCY);
    let clamped_timeout = default_timeout.clamp(
        autonomy_config::MIN_TIMEOUT_MINUTES,
        autonomy_config::MAX_TIMEOUT_MINUTES,
    );
    (clamped_concurrency, clamped_timeout)
}

/// retention_days ∈ [1, MAX_LOG_RETENTION_DAYS]로 clamp하는 순수 함수 —
/// 단위 테스트 대상.
fn clamp_log_retention_days(retention_days: u32) -> u32 {
    use crate::autonomy::config as autonomy_config;
    retention_days.clamp(1, autonomy_config::MAX_LOG_RETENTION_DAYS)
}

/// 원본 workspace 배열을 저장 전에 다듬는 순수 함수 — trim, 빈 문자열 제거,
/// 32개 cap(말도 안 되게 큰 원본 배열 방어. 8개 상한 자체는
/// `validate_workspace_entries`가 매 load 시점에 적용한다). 유효성 필터링은
/// 하지 않는다 — 파일에는 원본 문자열 그대로 저장한다(정책: `malgn_agent_
/// config_get()`이 매 load 시점에 검증하는 것과 대칭).
const MAX_RAW_WORKSPACE_ENTRIES: usize = 32;

fn sanitize_raw_workspace_list(entries: Vec<String>) -> Vec<String> {
    entries
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .take(MAX_RAW_WORKSPACE_ENTRIES)
        .collect()
}

/// `malgn_agent_config_get()`이 읽는 파일을 프론트가 직접 편집·저장할 수
/// 있게 한다(설계 §5/§13에서 "범위 밖"으로 뒀던 부분의 확장). 저장 직후
/// 최신 상태를 그대로 재사용해 응답한다 — 프론트가 별도 재조회를 하지 않아도
/// 최신 값을 받는다.
#[tauri::command]
pub fn malgn_agent_config_save(
    payload: MalgnAgentConfigInput,
) -> Result<MalgnAgentConfigStatus, String> {
    let (concurrency, default_timeout) = clamp_autonomy_defaults(
        payload.autonomy.concurrency,
        payload.autonomy.default_timeout,
    );
    let retention_days = clamp_log_retention_days(payload.logs.retention_days);
    let workspaces = sanitize_raw_workspace_list(payload.workspaces);

    let cfg = UserConfig {
        version: 1,
        workspaces,
        autonomy: user_config::AutonomyGlobalConfig {
            concurrency: Some(concurrency),
            default_timeout: Some(default_timeout),
        },
        logs: user_config::LogsGlobalConfig {
            retention_days: Some(retention_days),
        },
    };

    let path = user_config::config_file_path()
        .ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다".to_string())?;
    user_config::save_to_path(&path, &cfg)?;

    Ok(malgn_agent_config_get())
}

#[cfg(test)]
mod save_tests {
    use super::*;

    #[test]
    fn clamp_autonomy_defaults_clamps_below_minimum() {
        let (concurrency, timeout) = clamp_autonomy_defaults(0, 0);
        assert_eq!(concurrency, 1);
        assert_eq!(timeout, crate::autonomy::config::MIN_TIMEOUT_MINUTES);
    }

    #[test]
    fn clamp_autonomy_defaults_clamps_above_maximum() {
        let (concurrency, timeout) = clamp_autonomy_defaults(999, 999_999);
        assert_eq!(concurrency, crate::autonomy::config::MAX_CONCURRENCY);
        assert_eq!(timeout, crate::autonomy::config::MAX_TIMEOUT_MINUTES);
    }

    #[test]
    fn clamp_autonomy_defaults_keeps_in_range_values_unchanged() {
        let (concurrency, timeout) = clamp_autonomy_defaults(4, 90);
        assert_eq!(concurrency, 4);
        assert_eq!(timeout, 90);
    }

    #[test]
    fn clamp_log_retention_days_clamps_below_minimum() {
        assert_eq!(clamp_log_retention_days(0), 1);
    }

    #[test]
    fn clamp_log_retention_days_clamps_above_maximum() {
        assert_eq!(
            clamp_log_retention_days(9_999),
            crate::autonomy::config::MAX_LOG_RETENTION_DAYS
        );
    }

    #[test]
    fn sanitize_raw_workspace_list_trims_and_drops_blank_entries() {
        let entries = vec![
            "  /tmp/a  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "/tmp/b".to_string(),
        ];
        let cleaned = sanitize_raw_workspace_list(entries);
        assert_eq!(cleaned, vec!["/tmp/a".to_string(), "/tmp/b".to_string()]);
    }

    #[test]
    fn sanitize_raw_workspace_list_caps_at_thirty_two() {
        let entries: Vec<String> = (0..50).map(|i| format!("/tmp/proj-{i}")).collect();
        let cleaned = sanitize_raw_workspace_list(entries);
        assert_eq!(cleaned.len(), MAX_RAW_WORKSPACE_ENTRIES);
        assert_eq!(cleaned[0], "/tmp/proj-0");
    }
}
