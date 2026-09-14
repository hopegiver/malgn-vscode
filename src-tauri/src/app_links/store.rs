// 앱링크 스키마·파일 IO·순수 검증 함수. 설계 정본:
// `docs/design-app-links.md` §1(데이터 모델) §2(저장 방식) §4(입력 검증).
//
// 읽기 규칙(§2-2)과 쓰기 규칙(§2-3)은 `config/user_config.rs` /
// `otel_settings.rs`와 동일한 3분기(파일 없음=정상 / 정상 JSON=사용 / 손상=Err)
// 및 백업+원자적 쓰기 패턴을 따른다. 차이는 딱 하나 — "파일 없음"일 때
// 빈 목록이 아니라 시드 기본값(`super::seed::default_links()`)을 돌려준다는
// 것이다(PM이 이번 작업에서 추가한 신규 요구사항). 파일이 한 번이라도
// 생성되면(빈 배열로 저장해도) 그 뒤로는 시드가 다시 적용되지 않는다 —
// `load_from_path`가 "파일 읽기 성공"과 "파일 없음"을 구분하는 지점이
// 유일한 분기다.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs_atomic::{backup_path_for, write_atomically};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppLink {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

fn default_version() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppLinksFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub links: Vec<AppLink>,
}

// ---- 검증 임계값 정본. 이 4개 상수는 이 파일에만 존재한다(설계 §3-2). ----
pub(crate) const MAX_LINKS: usize = 50;
pub(crate) const MAX_NAME_LENGTH: usize = 40;
pub(crate) const MAX_URL_LENGTH: usize = 2048;
pub(crate) const ALLOWED_SCHEMES: [&str; 2] = ["https://", "http://"];

pub(crate) fn app_links_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("malgn-agent-apps.json"))
}

/// 정규화 — trim만(§4-2). 에러가 아니라 조용히 수행한다.
pub(crate) fn normalize_link(link: AppLink) -> AppLink {
    AppLink {
        id: link.id,
        name: link.name.trim().to_string(),
        url: link.url.trim().to_string(),
        enabled: link.enabled,
    }
}

/// V7~V10 — URL 형식 검증. `url` crate를 새 의존성으로 들이지 않고 접두
/// 검사 + 호스트부 비어있지 않음 + 공백/제어문자 금지로 충분한 안전성을
/// 확보한다(설계 §4-3 근거, `otel_settings.rs::validate_value`와 동일 방식).
pub(crate) fn validate_url(url: &str) -> Result<(), String> {
    // V7: 길이(바이트 기준)
    if url.len() > MAX_URL_LENGTH {
        return Err(format!("주소가 너무 깁니다(최대 {MAX_URL_LENGTH}자)."));
    }

    // V8: 스킴 화이트리스트(소문자 비교)
    let lower = url.to_ascii_lowercase();
    let Some(scheme) = ALLOWED_SCHEMES.iter().find(|s| lower.starts_with(*s)) else {
        return Err("주소는 https:// 또는 http://로 시작해야 합니다.".to_string());
    };

    // V9: 스킴 뒤 호스트부가 비어 있음
    let after_scheme = &url[scheme.len()..];
    let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..host_end];
    if host.is_empty() {
        return Err("주소에 호스트가 없습니다.".to_string());
    }

    // V10: 공백·제어문자
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("주소에 공백이나 제어문자가 들어 있습니다.".to_string());
    }

    Ok(())
}

/// V2,V4,V5,V6 + `validate_url` — 링크 1건의 검증. 이미 `normalize_link`를
/// 거친 값을 넘겨받는 것이 정상 경로지만, 방어적으로 내부에서도 trim한
/// 값으로 검사한다(호출부가 정규화를 빠뜨려도 규칙이 깨지지 않도록).
pub(crate) fn validate_link(link: &AppLink) -> Result<(), String> {
    let name = link.name.trim();
    let url = link.url.trim();

    // V2: id 형식
    let id_is_valid = !link.id.is_empty()
        && link.id.chars().count() <= 64
        && link
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !id_is_valid {
        return Err("앱링크 식별자 형식이 올바르지 않습니다.".to_string());
    }

    // V4: 빈 이름
    if name.is_empty() {
        return Err("이름은 비워 둘 수 없습니다.".to_string());
    }

    // V5: 이름 길이(문자 수 기준)
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(format!("'{name}': 이름은 {MAX_NAME_LENGTH}자 이내여야 합니다."));
    }

    // V6: 빈 주소
    if url.is_empty() {
        return Err(format!("'{name}': 주소는 비워 둘 수 없습니다."));
    }

    validate_url(url).map_err(|e| format!("'{name}': {e}"))?;

    Ok(())
}

/// V1,V3,V11 + `validate_link` 전체 — 저장 경로(전량 거부). 정규화 후의
/// 값을 돌려준다(저장 시점에 trim된 값이 파일에 남도록).
pub(crate) fn validate_links(links: &[AppLink]) -> Result<Vec<AppLink>, String> {
    if links.len() > MAX_LINKS {
        return Err(format!("앱링크는 최대 {MAX_LINKS}개까지 등록할 수 있습니다."));
    }

    let normalized: Vec<AppLink> = links.iter().cloned().map(normalize_link).collect();

    let mut seen_ids: HashSet<&str> = HashSet::new();
    let mut seen_urls: HashSet<&str> = HashSet::new();
    for link in &normalized {
        validate_link(link)?;
        if !seen_ids.insert(link.id.as_str()) {
            return Err(format!("'{}': 식별자가 중복되었습니다.", link.name));
        }
        if !seen_urls.insert(link.url.as_str()) {
            return Err(format!("'{}': 이미 등록된 주소입니다.", link.name));
        }
    }

    Ok(normalized)
}

/// 읽기 경로 — 유효 항목과 사유(warnings)를 분리한다(전량 거부가 아니라
/// 걸러내기). 검증 규칙 자체는 `validate_link` 한 곳을 저장 경로와 공유한다.
pub(crate) fn sanitize_loaded_links(links: Vec<AppLink>) -> (Vec<AppLink>, Vec<String>) {
    let mut valid = Vec::new();
    let mut warnings = Vec::new();
    for link in links {
        let normalized = normalize_link(link);
        match validate_link(&normalized) {
            Ok(()) => valid.push(normalized),
            Err(e) => warnings.push(e),
        }
    }
    (valid, warnings)
}

/// 순수 함수 — 주어진 경로를 그대로 읽는다. 파일이 없으면(또는 읽기 자체가
/// 실패하면) **시드 기본값**을 담은 정상값, 파싱에 실패하면 `Err`. 테스트가
/// 이 함수에 `std::env::temp_dir()` 아래 임시 경로를 넘겨 실제 홈 디렉터리를
/// 건드리지 않고 검증한다.
///
/// 시드는 "파일이 아직 한 번도 생성되지 않았다"는 조건에서만 적용된다 — 그
/// 뒤로 사용자가 전부 지우고 빈 배열로 저장하면 파일이 존재하게 되고, 이
/// 분기는 다시는 타지 않는다(T-seed-empty-file-returns-no-seed 테스트로
/// 고정).
pub(crate) fn load_from_path(path: &Path) -> Result<AppLinksFile, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return Ok(AppLinksFile {
                version: default_version(),
                links: super::seed::default_links(),
            });
        }
    };
    serde_json::from_str::<AppLinksFile>(&content)
        .map_err(|e| format!("앱링크 설정 파일이 손상되었습니다({}): {e}", path.display()))
}

/// 순수 함수 — 주어진 경로에 저장한다(`user_config.rs::save_to_path`와 동일
/// 절차 + otel 원칙 1개 추가). 절차: 기존 파일이 있는데 파싱 실패 시 즉시
/// `Err`(아무것도 쓰지 않는다) → 파싱 성공한 기존 파일이 있으면 롤링 1세대
/// 백업 → 직렬화 → 원자적 쓰기.
pub(crate) fn save_to_path(path: &Path, file: &AppLinksFile) -> Result<(), String> {
    let existing_content = std::fs::read_to_string(path).ok();

    if let Some(content) = &existing_content {
        if serde_json::from_str::<AppLinksFile>(content).is_err() {
            return Err(format!(
                "기존 앱링크 설정 파일을 해석하지 못해 저장을 중단합니다({}). 파일을 직접 고치거나 지운 뒤 다시 시도해주세요.",
                path.display()
            ));
        }
    }

    if let Some(content) = &existing_content {
        let backup_path = backup_path_for(path);
        std::fs::write(&backup_path, content)
            .map_err(|e| format!("백업 파일을 쓰지 못했습니다: {e}"))?;
    }

    let pretty = serde_json::to_string_pretty(file)
        .map_err(|e| format!("앱링크 설정을 직렬화하지 못했습니다: {e}"))?;
    write_atomically(path, &pretty)
}

/// 열기 경로의 순수 부분 — id로 조회 + 저장 시와 동일한 `validate_url`로
/// 재검증까지(§3-3 2·3단계). `open_url` 자체(4단계)는 `AppHandle`이 필요해
/// 단위테스트 대상이 아니므로, 그 앞부분만 떼어내 이 함수로 덮는다.
pub(crate) fn find_link_for_open<'a>(
    links: &'a [AppLink],
    id: &str,
) -> Result<&'a AppLink, String> {
    let link = links.iter().find(|l| l.id == id).ok_or_else(|| {
        "해당 앱링크를 찾을 수 없습니다. 목록을 새로고침해 주세요.".to_string()
    })?;
    validate_url(&link.url).map_err(|e| format!("'{}': {e}", link.name))?;
    Ok(link)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-app-links-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir
    }

    fn link(id: &str, name: &str, url: &str) -> AppLink {
        AppLink {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            enabled: true,
        }
    }

    // ---- T1~T8: validate_url ----

    #[test]
    fn t1_validate_url_accepts_https() {
        assert!(validate_url("https://a.example.com").is_ok());
    }

    #[test]
    fn t2_validate_url_accepts_http() {
        assert!(validate_url("http://a.example.com").is_ok());
    }

    #[test]
    fn t3_validate_url_rejects_mailto_and_tel() {
        assert!(validate_url("mailto:x@y.com").is_err());
        assert!(validate_url("tel:123").is_err());
    }

    #[test]
    fn t4_validate_url_rejects_file_and_javascript_schemes() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn t5_validate_url_rejects_missing_scheme() {
        assert!(validate_url("wiki.example.com").is_err());
    }

    #[test]
    fn t6_validate_url_rejects_empty_host() {
        assert!(validate_url("https://").is_err());
    }

    #[test]
    fn t7_validate_url_rejects_whitespace_and_control_chars() {
        assert!(validate_url("https://a.com/ b").is_err());
        assert!(validate_url("https://a.com/\nX").is_err());
    }

    #[test]
    fn t8_validate_url_rejects_too_long() {
        let long = format!("https://a.com/{}", "x".repeat(3000));
        assert!(validate_url(&long).is_err());
    }

    // ---- T9: validate_link ----

    #[test]
    fn t9_validate_link_rejects_empty_name() {
        let l = link("abc", "", "https://a.com");
        assert!(validate_link(&l).is_err());
    }

    #[test]
    fn t9_validate_link_rejects_name_over_forty_chars() {
        let l = link("abc", &"가".repeat(41), "https://a.com");
        assert!(validate_link(&l).is_err());
    }

    #[test]
    fn t9_validate_link_rejects_empty_id() {
        let l = link("", "이름", "https://a.com");
        assert!(validate_link(&l).is_err());
    }

    #[test]
    fn t9_validate_link_rejects_id_with_space() {
        let l = link("a b", "이름", "https://a.com");
        assert!(validate_link(&l).is_err());
    }

    // ---- T10~T11: validate_links ----

    #[test]
    fn t10_validate_links_rejects_duplicate_url() {
        let links = vec![
            link("a", "링크1", "https://dup.example.com"),
            link("b", "링크2", "https://dup.example.com"),
        ];
        assert!(validate_links(&links).is_err());
    }

    #[test]
    fn t10_validate_links_rejects_duplicate_id() {
        let links = vec![
            link("same-id", "링크1", "https://one.example.com"),
            link("same-id", "링크2", "https://two.example.com"),
        ];
        assert!(validate_links(&links).is_err());
    }

    #[test]
    fn t11_validate_links_rejects_over_fifty_entries() {
        let links: Vec<AppLink> = (0..51)
            .map(|i| link(&format!("id-{i}"), &format!("이름{i}"), &format!("https://a{i}.com")))
            .collect();
        assert!(validate_links(&links).is_err());
    }

    // ---- T12~T13: 정규화 ----

    #[test]
    fn t12_normalize_link_trims_name_and_url() {
        let l = link("a", "  위키  ", " https://a.com ");
        let normalized = normalize_link(l);
        assert_eq!(normalized.name, "위키");
        assert_eq!(normalized.url, "https://a.com");
    }

    #[test]
    fn t13_validate_links_accepts_input_valid_only_after_trim() {
        let links = vec![link("a", "  이름  ", "  https://a.com  ")];
        let result = validate_links(&links).expect("trim 후 유효해지므로 통과해야 합니다");
        assert_eq!(result[0].name, "이름");
        assert_eq!(result[0].url, "https://a.com");
    }

    // ---- T14~T17: 읽기 경로 ----

    #[test]
    fn t14_load_from_path_returns_seed_when_file_missing() {
        let dir = temp_subdir("missing");
        let path = dir.join("does-not-exist.json");
        let file = load_from_path(&path).expect("파일이 없으면 시드가 반환되어야 합니다");
        assert_eq!(file.links.len(), 5, "파일이 없으면 시드 5건이 반환되어야 합니다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_not_revived_when_file_exists_with_empty_links() {
        let dir = temp_subdir("empty-links-file");
        let path = dir.join("malgn-agent-apps.json");
        std::fs::write(&path, r#"{ "version": 1, "links": [] }"#).unwrap();
        let file = load_from_path(&path).expect("정상 JSON은 파싱에 성공해야 합니다");
        assert_eq!(
            file.links.len(),
            0,
            "사용자가 전부 지우고 저장한 빈 배열 파일에는 시드가 되살아나면 안 됩니다"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t15_load_from_path_returns_err_for_corrupted_json() {
        let dir = temp_subdir("corrupted");
        let path = dir.join("malgn-agent-apps.json");
        std::fs::write(&path, "{ this is not valid json").unwrap();
        assert!(load_from_path(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t16_load_from_path_ignores_unknown_keys() {
        let dir = temp_subdir("unknown-keys");
        let path = dir.join("malgn-agent-apps.json");
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "theme": "dark",
                "links": [
                    { "id": "a", "name": "위키", "url": "https://a.com", "enabled": true, "icon": "x" }
                ]
            }"#,
        )
        .unwrap();
        let file = load_from_path(&path).expect("모르는 키가 있어도 파싱에 성공해야 합니다");
        assert_eq!(file.links.len(), 1);
        assert_eq!(file.links[0].name, "위키");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t17_sanitize_loaded_links_filters_invalid_entries() {
        let links = vec![
            link("a", "유효1", "https://a.com"),
            link("b", "유효2", "http://b.com"),
            link("c", "잘못된스킴", "file:///etc/passwd"),
            link("d", "", "https://d.com"),
        ];
        let (valid, warnings) = sanitize_loaded_links(links);
        assert_eq!(valid.len(), 2);
        assert_eq!(warnings.len(), 2);
        let _ = &valid; // 값 사용(정적 분석 경고 방지)
    }

    // ---- T18~T20: 쓰기 경로 ----

    #[test]
    fn t18_save_then_load_round_trips() {
        let dir = temp_subdir("save-roundtrip");
        let path = dir.join("malgn-agent-apps.json");
        let file = AppLinksFile {
            version: 1,
            links: vec![
                link("a", "위키", "https://wiki.example.com"),
                link("b", "지라", "https://jira.example.com"),
            ],
        };

        save_to_path(&path, &file).expect("저장에 성공해야 합니다");
        let loaded = load_from_path(&path).expect("저장 직후 로드는 성공해야 합니다");

        assert_eq!(loaded.version, file.version);
        assert_eq!(loaded.links, file.links);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t19_second_save_creates_rolling_backup_of_first_save() {
        let dir = temp_subdir("save-backup");
        let path = dir.join("malgn-agent-apps.json");

        let first = AppLinksFile {
            version: 1,
            links: vec![link("a", "첫번째", "https://first.example.com")],
        };
        save_to_path(&path, &first).expect("첫 번째 저장에 성공해야 합니다");

        let second = AppLinksFile {
            version: 1,
            links: vec![link("a", "두번째", "https://second.example.com")],
        };
        save_to_path(&path, &second).expect("두 번째 저장에 성공해야 합니다");

        let backup_path = backup_path_for(&path);
        assert!(backup_path.is_file(), "백업 파일이 생성되어야 한다");
        let backup: AppLinksFile =
            serde_json::from_str(&std::fs::read_to_string(&backup_path).unwrap()).unwrap();
        assert_eq!(backup.links, first.links, "백업은 첫 번째 저장 값이어야 한다");

        let current: AppLinksFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(current.links, second.links);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t20_save_does_not_overwrite_unparseable_existing_file() {
        let dir = temp_subdir("unparseable");
        let path = dir.join("malgn-agent-apps.json");
        let original = "{ this is not valid json";
        std::fs::write(&path, original).unwrap();

        let file = AppLinksFile {
            version: 1,
            links: vec![link("a", "새항목", "https://new.example.com")],
        };
        let result = save_to_path(&path, &file);

        assert!(result.is_err(), "파싱 실패한 파일에 저장하면 Err여야 한다");
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, original, "파싱 못 한 파일은 절대 덮어쓰면 안 된다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- T21: 열기 경로 ----

    #[test]
    fn t21_find_link_for_open_rejects_missing_id() {
        let links = vec![link("a", "위키", "https://a.com")];
        assert!(find_link_for_open(&links, "없는-id").is_err());
    }

    #[test]
    fn find_link_for_open_returns_link_when_found_and_valid() {
        let links = vec![link("a", "위키", "https://a.com")];
        let found = find_link_for_open(&links, "a").expect("존재하는 id는 찾아야 합니다");
        assert_eq!(found.id, "a");
    }

    #[test]
    fn find_link_for_open_revalidates_hand_edited_url() {
        let links = vec![link("a", "위키", "file:///etc/passwd")];
        assert!(
            find_link_for_open(&links, "a").is_err(),
            "손으로 편집된 파일의 url은 열기 시점에도 재검증되어야 합니다"
        );
    }
}
