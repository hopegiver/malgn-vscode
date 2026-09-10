// 전역 설정 파일(`~/.claude/malgn-agent.json`) 진입점. 실제 파싱·검증·캐시는
// `user_config.rs`에 있고, 이 파일은 ① 크레이트 전역에서 쓰는 얇은 진입점
// (`load`, `workspace_roots_checked`) ② 설정 화면이 상시 표시하는 상태 커맨드
// (`malgn_agent_config_get`)만 담는다.

mod user_config;

use serde::Serialize;
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
