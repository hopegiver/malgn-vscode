// `~/.claude/malgn-agent.json` — 이 앱 전용 전역 설정 파일. Claude Code의
// `~/.claude/settings.json`과 소유자가 다르므로 절대 섞지 않는다(otel_settings.rs가
// settings.json을 따로 다룬다).
//
// 로드 규칙(설계 §5): 파일 없음 → 코드 기본값(정상) / 정상 JSON → 사용(섹션
// 누락은 `#[serde(default)]`로 개별 기본값, 모르는 키는 무시) / JSON 손상 →
// 오류(조용한 기본값 폴백 금지). "조용한 기본값 폴백 금지"를 지키기 위해
// 손상된 파일은 `Err`를 그대로 올린다 — 호출자(`config::workspace_roots()`)가
// fail-closed(빈 목록)로 흡수한다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// workspace 항목 상한 — 오타·과다 등록 방어(설계 §5 ⑤).
const MAX_WORKSPACE_ENTRIES: usize = 8;

fn default_version() -> u32 {
    1
}

/// 파일이 아예 없을 때 쓰는 코드 기본값 — 기존 `lib.rs::workspace_roots()`가
/// 하드코딩했던 후보와 동일하다(마이그레이션으로 동작이 바뀌지 않는다).
fn default_workspace_list() -> Vec<String> {
    #[allow(unused_mut)]
    let mut list = vec!["~/workspace".to_string()];
    #[cfg(windows)]
    list.push("C:\\workspace".to_string());
    list
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct AutonomyGlobalConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
    #[serde(
        default,
        rename = "defaultTimeout",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_timeout: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct LogsGlobalConfig {
    #[serde(
        default,
        rename = "retentionDays",
        skip_serializing_if = "Option::is_none"
    )]
    pub retention_days: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct UserConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_workspace_list")]
    pub workspaces: Vec<String>,
    #[serde(default)]
    pub autonomy: AutonomyGlobalConfig,
    #[serde(default)]
    pub logs: LogsGlobalConfig,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            workspaces: default_workspace_list(),
            autonomy: AutonomyGlobalConfig::default(),
            logs: LogsGlobalConfig::default(),
        }
    }
}

pub(crate) fn config_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("malgn-agent.json"))
}

/// 순수 함수 — 주어진 경로를 그대로 읽는다. 파일이 없으면(또는 읽기 자체가
/// 실패하면) 기본값, 파싱에 실패하면 `Err`. 테스트가 이 함수에 `std::env::
/// temp_dir()` 아래 임시 경로를 넘겨 실제 홈 디렉터리를 건드리지 않고 검증한다.
pub(crate) fn load_from_path(path: &Path) -> Result<UserConfig, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Ok(UserConfig::default()),
    };
    serde_json::from_str::<UserConfig>(&content)
        .map_err(|e| format!("전역 설정 파일이 손상되었습니다({}): {e}", path.display()))
}

fn file_fingerprint(path: &Path) -> Option<(i64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let mtime_ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some((i64::try_from(mtime_ms).ok()?, metadata.len()))
}

/// mtime+길이 캐시(설계 §5) — tick마다 불려도 안전하도록 매번 `fs::metadata`만
/// 확인하고(수 µs), 값이 바뀌지 않았으면 마지막으로 파싱한 결과를 그대로
/// 재사용한다. 워처 없이도 사용자가 파일을 고치면 다음 호출에 반영된다.
static CACHE: Mutex<Option<(Option<(i64, u64)>, Result<UserConfig, String>)>> = Mutex::new(None);

pub(crate) fn load() -> Result<UserConfig, String> {
    let Some(path) = config_file_path() else {
        return Ok(UserConfig::default());
    };
    let fingerprint = file_fingerprint(&path);

    {
        let cache = CACHE.lock().unwrap();
        if let Some((cached_fp, cached_result)) = cache.as_ref() {
            if *cached_fp == fingerprint {
                return cached_result.clone();
            }
        }
    }

    let result = load_from_path(&path);
    let mut cache = CACHE.lock().unwrap();
    *cache = Some((fingerprint, result.clone()));
    result
}

/// `~`로 시작하는 항목만 홈 기준으로 확장한다. `~user` 형태(다른 사용자 홈)는
/// 거부하고, 그 외 범용 환경변수 확장은 하지 않는다(설계 §5 보안 핵심).
/// `home`을 인자로 받는 순수 함수라 테스트에서 임시 디렉터리를 "가짜 홈"으로
/// 주입할 수 있다.
pub(crate) fn expand_tilde(entry: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let Some(rest) = entry.strip_prefix('~') else {
        return Ok(PathBuf::from(entry));
    };
    if rest.is_empty() {
        return home
            .map(|h| h.to_path_buf())
            .ok_or_else(|| format!("홈 디렉터리를 확인할 수 없어 '{entry}' 항목을 건너뜁니다."));
    }
    if let Some(sub) = rest.strip_prefix('/') {
        return home
            .map(|h| h.join(sub))
            .ok_or_else(|| format!("홈 디렉터리를 확인할 수 없어 '{entry}' 항목을 건너뜁니다."));
    }
    Err(format!(
        "'{entry}': '~사용자명' 형태의 경로는 지원하지 않습니다."
    ))
}

fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

/// workspace 항목 검증(설계 §5) — 존재하지 않거나 디렉터리가 아님 / 파일시스템
/// 루트 / 홈 디렉터리 자체 / 정규화 후 중복 / 8개 초과분을 거부하고 warning
/// 목록에 담는다. 유효 항목이 0개면 `Ok(vec![])`(fail-closed)다.
pub(crate) fn validate_workspace_entries(entries: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    validate_workspace_entries_with_home(entries, dirs::home_dir().as_deref())
}

fn validate_workspace_entries_with_home(
    entries: &[String],
    home: Option<&Path>,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut valid: Vec<PathBuf> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let canonical_home = home.and_then(|h| h.canonicalize().ok());

    for entry in entries {
        if valid.len() >= MAX_WORKSPACE_ENTRIES {
            warnings.push(format!(
                "'{entry}': workspace 항목은 최대 {MAX_WORKSPACE_ENTRIES}개까지만 허용됩니다."
            ));
            continue;
        }

        let expanded = match expand_tilde(entry, home) {
            Ok(p) => p,
            Err(msg) => {
                warnings.push(msg);
                continue;
            }
        };

        if !expanded.is_dir() {
            warnings.push(format!("'{entry}': 존재하지 않거나 디렉터리가 아닙니다."));
            continue;
        }

        let canonical = expanded.canonicalize().unwrap_or(expanded);

        if is_filesystem_root(&canonical) {
            warnings.push(format!(
                "'{entry}': 파일시스템 루트는 workspace로 쓸 수 없습니다."
            ));
            continue;
        }

        if let Some(h) = &canonical_home {
            if &canonical == h {
                warnings.push(format!(
                    "'{entry}': 홈 디렉터리 자체는 workspace로 쓸 수 없습니다."
                ));
                continue;
            }
        }

        if !seen.insert(canonical.clone()) {
            warnings.push(format!("'{entry}': 중복된 workspace 항목입니다."));
            continue;
        }

        valid.push(canonical);
    }

    (valid, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-user-config-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir
    }

    #[test]
    fn load_from_path_returns_default_when_file_missing() {
        let dir = temp_subdir("missing");
        let path = dir.join("does-not-exist.json");
        let cfg = load_from_path(&path).expect("파일이 없으면 기본값이어야 합니다");
        assert_eq!(cfg.workspaces, default_workspace_list());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 신규(최소) — 손상 JSON이 Err인지.
    #[test]
    fn load_from_path_returns_err_for_corrupted_json() {
        let dir = temp_subdir("corrupted");
        let path = dir.join("malgn-agent.json");
        std::fs::write(&path, "{ this is not valid json").unwrap();
        let result = load_from_path(&path);
        assert!(result.is_err(), "손상된 JSON은 Err여야 합니다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_path_applies_field_defaults_for_missing_sections() {
        let dir = temp_subdir("partial");
        let path = dir.join("malgn-agent.json");
        std::fs::write(&path, r#"{ "version": 1, "workspaces": ["/tmp"] }"#).unwrap();
        let cfg = load_from_path(&path).expect("파싱에 성공해야 합니다");
        assert_eq!(cfg.workspaces, vec!["/tmp".to_string()]);
        assert_eq!(cfg.autonomy.concurrency, None);
        assert_eq!(cfg.logs.retention_days, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 신규(최소) — expand_tilde + workspace 검증(홈 거부).
    #[test]
    fn expand_tilde_rejects_tilde_user_form() {
        let result = expand_tilde("~otheruser/workspace", Some(Path::new("/Users/me")));
        assert!(result.is_err(), "~user 형태는 거부되어야 합니다");
    }

    #[test]
    fn expand_tilde_expands_using_provided_home() {
        let expanded = expand_tilde("~/workspace", Some(Path::new("/Users/me"))).unwrap();
        assert_eq!(expanded, PathBuf::from("/Users/me/workspace"));
    }

    // 신규(최소) — workspace 검증(루트 거부).
    #[test]
    fn validate_workspace_entries_rejects_filesystem_root() {
        let (valid, warnings) =
            validate_workspace_entries_with_home(&["/".to_string()], Some(Path::new("/nonexistent-home")));
        assert!(valid.is_empty());
        assert!(!warnings.is_empty());
    }

    // 신규(최소) — workspace 검증(홈 디렉터리 자체 거부).
    #[test]
    fn validate_workspace_entries_rejects_home_directory_itself() {
        let home = temp_subdir("home-itself");
        let entries = vec![home.to_string_lossy().to_string()];
        let (valid, warnings) = validate_workspace_entries_with_home(&entries, Some(&home));
        assert!(valid.is_empty(), "홈 디렉터리 자체는 거부되어야 합니다");
        assert!(!warnings.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn validate_workspace_entries_rejects_nonexistent_directory() {
        let home = temp_subdir("nonexistent-parent");
        let missing = home.join("no-such-dir");
        let entries = vec![missing.to_string_lossy().to_string()];
        let (valid, warnings) = validate_workspace_entries_with_home(&entries, Some(&home));
        assert!(valid.is_empty());
        assert!(!warnings.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn validate_workspace_entries_deduplicates_and_caps_at_eight() {
        let home = temp_subdir("many-entries");
        let mut entries = Vec::new();
        for i in 0..10 {
            let sub = home.join(format!("proj-{i}"));
            std::fs::create_dir_all(&sub).unwrap();
            entries.push(sub.to_string_lossy().to_string());
        }
        // 중복 추가
        entries.push(entries[0].clone());

        let (valid, warnings) = validate_workspace_entries_with_home(&entries, Some(&home));
        assert_eq!(valid.len(), 8, "8개 초과분은 거부되어야 합니다");
        assert!(!warnings.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn validate_workspace_entries_accepts_valid_directory() {
        let home = temp_subdir("valid-entry");
        let project = home.join("workspace");
        std::fs::create_dir_all(&project).unwrap();
        let entries = vec![project.to_string_lossy().to_string()];
        let (valid, warnings) = validate_workspace_entries_with_home(&entries, Some(&home));
        assert_eq!(valid.len(), 1);
        assert!(warnings.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
