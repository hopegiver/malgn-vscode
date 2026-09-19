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
/// 주입할 수 있다. 실제 호출부는 `cfg!(windows)`(런타임 상수 — `#[cfg(windows)]`와
/// 달리 모든 플랫폼에서 컴파일된다)를 넘긴다.
pub(crate) fn expand_tilde(entry: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    expand_tilde_for_platform(entry, home, cfg!(windows))
}

/// 홈-상대 구분자를 문자열 수준에서만 판정하는 순수 함수(`Path`/`PathBuf`를
/// 쓰지 않는다) — `is_windows`를 인자로 받아 macOS CI에서도 Windows 분기를
/// 그대로 검증할 수 있다. `PathBuf::join`을 여기서 썼다면 결과가 **컴파일
/// 호스트**의 구분자 규칙을 따라 macOS에서 도는 테스트가 Windows 경로 조립을
/// 검증하지 못했을 것이다(`dev_tools/platform.rs`의 `win_join` 주석과 동일한
/// 함정 — 그 문서화된 교훈을 그대로 재사용한다).
///
/// 실사용자 버그(hub 이슈 01m2wm499xmh3046rnvx4cyn8n, Windows): 기존 구현은
/// `~/`(슬래시)만 인정해 `~\workspace`(Windows 사용자가 탐색기 주소창에서
/// 복사하거나 손으로 입력할 때 자연스러운 백슬래시 표기)를 `~사용자명` 형태로
/// 오판하고 거부했다 — workspace 항목이 통째로 빠지면서 "C드라이브에 폴더가
/// 분명히 있는데 자율업무가 프로젝트를 못 찾는다"는 증상으로 나타난다.
/// Windows에서만 `~\`도 `~/`와 동일하게 인정한다(비-Windows 플랫폼은 기존
/// 동작 그대로 — 이미 `~/`가 아닌 다른 구분자는 전부 거부되고 있었으므로
/// 새로 받아들이는 입력이 없으면 기존에 거부되던 입력도 그대로 거부된다).
fn strip_home_relative<'a>(rest: &'a str, is_windows: bool) -> Option<&'a str> {
    if let Some(sub) = rest.strip_prefix('/') {
        return Some(sub);
    }
    if is_windows {
        if let Some(sub) = rest.strip_prefix('\\') {
            return Some(sub);
        }
    }
    None
}

fn expand_tilde_for_platform(
    entry: &str,
    home: Option<&Path>,
    is_windows: bool,
) -> Result<PathBuf, String> {
    let Some(rest) = entry.strip_prefix('~') else {
        return Ok(PathBuf::from(entry));
    };
    if rest.is_empty() {
        return home
            .map(|h| h.to_path_buf())
            .ok_or_else(|| format!("홈 디렉터리를 확인할 수 없어 '{entry}' 항목을 건너뜁니다."));
    }
    if let Some(sub) = strip_home_relative(rest, is_windows) {
        return home
            .map(|h| h.join(sub))
            .ok_or_else(|| format!("홈 디렉터리를 확인할 수 없어 '{entry}' 항목을 건너뜁니다."));
    }
    Err(format!(
        "'{entry}': '~사용자명' 형태의 경로는 지원하지 않습니다."
    ))
}

// 원자적 쓰기 + 백업 경로 유틸은 `crate::fs_atomic`로 추출됨(3번째 소비자인
// `app_links`가 생기면서 공용 모듈화 — `docs/design-app-links.md` §2-4).
use crate::fs_atomic::{backup_path_for, write_atomically};

/// 순수 함수 — 주어진 경로에 저장한다(`otel_settings.rs::save_to_settings_file`과
/// 동일한 백업+원자적 쓰기 패턴). 상위 디렉터리 생성 → 기존 파일 있으면
/// `<path>.malgn-bak`로 롤링 1세대 백업 → 직렬화 → 원자적 쓰기 → 캐시 리셋
/// (다음 `load()` 호출이 새로 읽도록).
pub(crate) fn save_to_path(path: &Path, cfg: &UserConfig) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    }

    if let Ok(existing) = std::fs::read_to_string(path) {
        let backup_path = backup_path_for(path);
        std::fs::write(&backup_path, existing)
            .map_err(|e| format!("백업 파일을 쓰지 못했습니다: {e}"))?;
    }

    let pretty = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("설정을 직렬화하지 못했습니다: {e}"))?;
    write_atomically(path, &pretty)?;

    let mut cache = CACHE.lock().unwrap();
    *cache = None;
    Ok(())
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
    let canonical_home = home.and_then(|h| dunce::canonicalize(h).ok());

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

        let canonical = dunce::canonicalize(&expanded).unwrap_or(expanded);

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

    // 신규 — 실사용자 버그 회귀 고정(hub 이슈 01m2wm499xmh3046rnvx4cyn8n, Windows).
    // `Path`/`PathBuf`를 전혀 쓰지 않는 순수 문자열 함수라 macOS에서도 Windows
    // 분기(`is_windows: true`)를 그대로 검증할 수 있다.
    #[test]
    fn strip_home_relative_accepts_backslash_only_when_windows() {
        assert_eq!(strip_home_relative("\\workspace", true), Some("workspace"));
        assert_eq!(strip_home_relative("\\workspace", false), None);
    }

    #[test]
    fn strip_home_relative_accepts_forward_slash_on_any_platform() {
        assert_eq!(strip_home_relative("/workspace", true), Some("workspace"));
        assert_eq!(strip_home_relative("/workspace", false), Some("workspace"));
    }

    // `expand_tilde_for_platform`은 `home.join(sub)`을 쓰므로 결과 문자열의
    // 구분자 표기는 컴파일 호스트를 따른다(`dev_tools/platform.rs`의 `win_join`
    // 주석과 동일한 함정) — 그래서 Windows 리터럴 문자열과 비교하지 않고,
    // 슬래시 버전과 동일한 조합(`home.join("workspace")`)으로 계산한 기대값과
    // 비교한다. 이 비교는 호스트가 무엇이든 항상 성립한다.
    #[test]
    fn expand_tilde_for_platform_accepts_windows_backslash_separator() {
        let home = Path::new("/Users/me");
        let expanded = expand_tilde_for_platform("~\\workspace", Some(home), true)
            .expect("Windows에서는 백슬래시도 허용되어야 합니다");
        assert_eq!(expanded, home.join("workspace"));
    }

    #[test]
    fn expand_tilde_for_platform_rejects_backslash_separator_when_not_windows() {
        let result = expand_tilde_for_platform("~\\workspace", Some(Path::new("/Users/me")), false);
        assert!(
            result.is_err(),
            "비-Windows 플랫폼에서는 기존 동작대로 백슬래시가 거부되어야 합니다"
        );
    }

    // 공개 API(`expand_tilde`)가 현재 컴파일 타깃에 맞는 `is_windows`를 스스로
    // 정하는지 확인한다 — `cfg!(windows)`가 곧 컴파일 호스트를 반영하므로, 이
    // 단언은 macOS/windows-latest 양쪽 CI에서 각각 반대 분기를 실제로 실행해
    // 검증한다(둘 다 의미 있게 통과해야 하는 진짜 크로스플랫폼 테스트).
    #[test]
    fn expand_tilde_uses_current_platform_for_backslash_acceptance() {
        let result = expand_tilde("~\\workspace", Some(Path::new("/Users/me")));
        if cfg!(windows) {
            assert!(
                result.is_ok(),
                "Windows 호스트에서는 '~\\workspace'가 허용되어야 합니다"
            );
        } else {
            assert!(
                result.is_err(),
                "비-Windows 호스트에서는 '~\\workspace'가 거부되어야 합니다"
            );
        }
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

    #[cfg(windows)]
    #[test]
    fn validate_workspace_entries_strips_extended_length_prefix() {
        // Windows std::fs::canonicalize()는 `\\?\`(extended-length) 접두 경로를
        // 반환한다 — 이 값이 그대로 조회(get) 응답을 거쳐 편집 폼에 재채워지면
        // 사용자가 재저장할 때 `\\?\`가 raw 값으로 영구 저장돼 이후 스캔이
        // 실패한다. dunce::canonicalize()로 접두어를 제거해 방지한다.
        let home = temp_subdir("unc-prefix");
        let project = home.join("workspace");
        std::fs::create_dir_all(&project).unwrap();
        let entries = vec![project.to_string_lossy().to_string()];
        let (valid, warnings) = validate_workspace_entries_with_home(&entries, Some(&home));
        assert_eq!(valid.len(), 1);
        assert!(warnings.is_empty());
        assert!(!valid[0].to_string_lossy().starts_with(r"\\?\"));
        let _ = std::fs::remove_dir_all(&home);
    }

    fn sample_config(concurrency: u32) -> UserConfig {
        UserConfig {
            version: 1,
            workspaces: vec!["/tmp/proj-a".to_string(), "/tmp/proj-b".to_string()],
            autonomy: AutonomyGlobalConfig {
                concurrency: Some(concurrency),
                default_timeout: Some(45),
            },
            logs: LogsGlobalConfig {
                retention_days: Some(14),
            },
        }
    }

    // 신규 — save_to_path로 저장 후 load_from_path로 다시 읽으면 동일(round-trip).
    #[test]
    fn save_to_path_then_load_from_path_round_trips() {
        let dir = temp_subdir("save-roundtrip");
        let path = dir.join("malgn-agent.json");
        let cfg = sample_config(4);

        save_to_path(&path, &cfg).expect("저장에 성공해야 합니다");
        let loaded = load_from_path(&path).expect("저장 직후 로드는 성공해야 합니다");

        assert_eq!(loaded.version, cfg.version);
        assert_eq!(loaded.workspaces, cfg.workspaces);
        assert_eq!(loaded.autonomy.concurrency, cfg.autonomy.concurrency);
        assert_eq!(
            loaded.autonomy.default_timeout,
            cfg.autonomy.default_timeout
        );
        assert_eq!(loaded.logs.retention_days, cfg.logs.retention_days);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 신규 — 기존 파일이 있는 상태에서 두 번째 저장 시 롤링 1세대 백업
    // (`.malgn-bak`)이 생기고, 그 내용이 첫 번째 저장 값인지.
    #[test]
    fn save_to_path_creates_rolling_backup_of_previous_save() {
        let dir = temp_subdir("save-backup");
        let path = dir.join("malgn-agent.json");

        let first = sample_config(2);
        save_to_path(&path, &first).expect("첫 번째 저장에 성공해야 합니다");

        let second = sample_config(6);
        save_to_path(&path, &second).expect("두 번째 저장에 성공해야 합니다");

        let backup_path = backup_path_for(&path);
        assert!(backup_path.is_file(), "백업 파일이 생성되어야 한다");

        let backup_cfg: UserConfig =
            serde_json::from_str(&std::fs::read_to_string(&backup_path).unwrap()).unwrap();
        assert_eq!(
            backup_cfg.autonomy.concurrency,
            first.autonomy.concurrency,
            "백업은 첫 번째 저장 값이어야 한다"
        );

        let current: UserConfig =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(current.autonomy.concurrency, second.autonomy.concurrency);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
