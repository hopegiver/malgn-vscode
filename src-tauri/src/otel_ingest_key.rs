// 허브 OTLP 수집키(`mhk_…`) 처리 — `OTEL_EXPORTER_OTLP_HEADERS` 한 키를 다룬다.
//
// 허브 `/v1/*`는 `Authorization: Bearer mhk_…` 없이는 401을 낸다. 클라이언트(Claude
// Code)에는 허브 설계(`docs/design/otlp-ingest.md` §5.3)대로 `~/.claude/settings.json`
// 의 `env.OTEL_EXPORTER_OTLP_HEADERS="Authorization=Bearer mhk_…"`로 전달한다.
//
// 이 모듈의 불변식(리뷰 대상): **수집키 원문은 이 모듈 밖으로, 특히 프론트로 가는 응답·
// 에러 문자열·로그·Debug 출력으로 나가지 않는다.**
//   - 입력은 `SecretString`(Debug가 가려진 래퍼)으로만 받는다. `Serialize`는 구현하지 않는다.
//   - 응답에는 `IngestKeyStatus`(상태 + 허브와 같은 `mhk_`+4자 마스킹)만 싣는다.
//   - 모든 `Err` 문자열은 입력값을 인용하지 않는 고정 문구다.
//   - 기존 헤더가 `mhk_` 형식이 아닌 `Authorization`이면(`Unrecognized`) 그 원문은 어디에도
//     내보내지 않는다.
//
// 형식 검증은 허브 `server/lib/otlp-limits.js`의 `CREDENTIAL_TOKEN_BODY_RE =
// /^mhk_[A-Za-z0-9_-]{43}$/`(총 47자)와 같다. 허브 정규식을 바꾸면 여기도 같이 바꾼다.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 수집키를 담는 `settings.json` env 키. `otel_settings::MANAGED_OTEL_KEYS`에도 있지만
/// `values`로는 읽지도 쓰지도 않고 이 모듈만 다룬다.
pub(crate) const HEADERS_ENV_KEY: &str = "OTEL_EXPORTER_OTLP_HEADERS";

const KEY_PREFIX: &str = "mhk_";
const KEY_BODY_LEN: usize = 43;
const KEY_TOTAL_LEN: usize = KEY_PREFIX.len() + KEY_BODY_LEN; // 47
/// 허브 표시용 prefix와 같은 길이 — `mhk_` + 4자.
const MASK_VISIBLE_LEN: usize = KEY_PREFIX.len() + 4;

const AUTH_HEADER_NAME: &str = "Authorization";
const BEARER_SCHEME: &str = "Bearer";

/// Debug 출력이 값을 드러내지 않는 문자열 래퍼. 프론트에서 온 수집키 원문은 반드시 이
/// 타입으로만 들고 다닌다(`Serialize` 미구현 — 응답 구조체에 실수로 넣어도 컴파일되지 않는다).
#[derive(Deserialize, Clone, Default)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl SecretString {
    pub(crate) fn new(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}

/// 저장 요청에서 수집키를 어떻게 할지. 필드를 생략하면 `Keep`(기존 키 유지)이다 — 마스킹된
/// 값이 요청에 실려 실제 키를 덮어쓰는 경로 자체가 없다(`Set`은 사용자가 새로 입력한 원문만).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum IngestKeyChange {
    #[default]
    Keep,
    Set {
        key: SecretString,
    },
    Clear,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IngestKeyState {
    /// `Authorization` 헤더 없음.
    Unset,
    /// `Authorization=Bearer mhk_…`(허브 형식 일치).
    Set,
    /// `Authorization`이 있지만 허브 형식이 아님 — 원문은 노출하지 않는다.
    Unrecognized,
}

/// 프론트로 나가는 수집키 상태. 원문 필드가 없다.
#[derive(Serialize, Clone, Debug)]
pub struct IngestKeyStatus {
    pub state: IngestKeyState,
    /// `state == Set`일 때만 `mhk_AbCd…` 형태. 그 외에는 `None`.
    pub masked: Option<String>,
    /// 허브 인증키 관리 화면 URL(프론트가 기존 외부 링크 열기로 연다).
    #[serde(rename = "issueUrl")]
    pub issue_url: String,
}

impl IngestKeyStatus {
    pub(crate) fn from_headers(raw_headers: Option<&str>, issue_url: String) -> Self {
        let (state, masked) = classify(raw_headers);
        Self {
            state,
            masked,
            issue_url,
        }
    }
}

/// `settings.json`의 `OTEL_EXPORTER_OTLP_HEADERS` 값에 가할 변경.
#[derive(PartialEq, Eq)]
pub(crate) enum HeadersEdit {
    Unchanged,
    /// 남는 헤더가 없어 env 키 자체를 지운다.
    Remove,
    /// 이 문자열로 교체한다(수집키 원문을 포함한다 — Debug는 값을 가린다).
    Replace(String),
}

impl fmt::Debug for HeadersEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unchanged => f.write_str("Unchanged"),
            Self::Remove => f.write_str("Remove"),
            Self::Replace(_) => f.write_str("Replace([REDACTED])"),
        }
    }
}

const INVALID_KEY_MESSAGE: &str =
    "수집키 형식이 올바르지 않습니다. 허브에서 발급받은 mhk_로 시작하는 키 전체(47자)를 붙여넣으세요.";

fn is_valid_key(candidate: &str) -> bool {
    candidate.len() == KEY_TOTAL_LEN
        && candidate.starts_with(KEY_PREFIX)
        && candidate[KEY_PREFIX.len()..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn strip_prefix_ignore_ascii_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// 사용자가 붙여넣은 문자열에서 순수 수집키(`mhk_…` 47자)를 뽑아 검증한다.
///
/// 관대하게 받는 것(복사 실수 3종): 앞뒤 공백, `Bearer mhk_…`, `Authorization=Bearer mhk_…`
/// (발급 응답의 `env.OTEL_EXPORTER_OTLP_HEADERS` 값이나 헤더 전체를 복사해 오는 경우).
/// 접두어를 벗긴 뒤의 남은 문자열은 허브와 같은 정규식으로 엄격 검증한다 — 그 밖의 불일치
/// (길이, 금지 문자, 내부 공백, 따옴표 등)는 모두 거부한다. 에러 문자열은 입력을 인용하지 않는다.
pub(crate) fn normalize_ingest_key(input: &str) -> Result<String, String> {
    let mut rest = input.trim();
    if let Some(after_name) = strip_prefix_ignore_ascii_case(rest, AUTH_HEADER_NAME) {
        let after_name = after_name.trim_start();
        match after_name.strip_prefix('=') {
            Some(after_eq) => rest = after_eq.trim_start(),
            // `Authorization`으로 시작하지만 `=`가 없으면 키가 아니다.
            None => return Err(INVALID_KEY_MESSAGE.to_string()),
        }
    }
    if let Some(after_scheme) = strip_prefix_ignore_ascii_case(rest, BEARER_SCHEME) {
        // `Bearer` 뒤에는 공백이 있어야 한다("Bearermhk_…"는 거부).
        if after_scheme.starts_with(char::is_whitespace) {
            rest = after_scheme.trim_start();
        }
    }
    if is_valid_key(rest) {
        Ok(rest.to_string())
    } else {
        Err(INVALID_KEY_MESSAGE.to_string())
    }
}

/// 허브 표시용과 같은 마스킹 — `mhk_`+4자 뒤 `…`. 검증된 키에만 호출한다.
fn mask_key(key: &str) -> String {
    format!("{}…", &key[..MASK_VISIBLE_LEN])
}

/// `a=b,c=d` 헤더 문자열을 항목 단위로 쪼갠다(앞뒤 공백 제거, 빈 항목 제외).
fn split_entries(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// 항목이 `Authorization=<값>`이면 값(앞뒤 공백 제거)을 돌려준다. 헤더 이름은 대소문자 무시.
fn authorization_value(entry: &str) -> Option<&str> {
    let (name, value) = entry.split_once('=')?;
    if name.trim().eq_ignore_ascii_case(AUTH_HEADER_NAME) {
        Some(value.trim())
    } else {
        None
    }
}

/// `Bearer mhk_…`에서 키만 뽑는다. 스킴은 허브(`/^Bearer mhk_…$/`)와 같게 대소문자 구분.
fn bearer_key(value: &str) -> Option<&str> {
    let key = value.strip_prefix("Bearer ")?;
    is_valid_key(key).then_some(key)
}

fn classify(raw_headers: Option<&str>) -> (IngestKeyState, Option<String>) {
    let Some(raw) = raw_headers else {
        return (IngestKeyState::Unset, None);
    };
    let mut auth_values = split_entries(raw)
        .into_iter()
        .filter_map(|entry| authorization_value(&entry).map(str::to_string));
    match auth_values.next() {
        None => (IngestKeyState::Unset, None),
        Some(value) => match bearer_key(&value) {
            Some(key) => (IngestKeyState::Set, Some(mask_key(key))),
            None => (IngestKeyState::Unrecognized, None),
        },
    }
}

/// 기존 `OTEL_EXPORTER_OTLP_HEADERS` 값에 변경을 적용해 새 값을 만든다.
/// `Authorization` 외 `k=v` 항목은 순서·내용 그대로 보존한다. 중복된 `Authorization`
/// 항목은 하나로 합친다(첫 위치에 둔다). 입력 검증 실패 시 `Err`(원문 미인용).
pub(crate) fn apply_change(
    existing: Option<&str>,
    change: &IngestKeyChange,
) -> Result<HeadersEdit, String> {
    match change {
        IngestKeyChange::Keep => Ok(HeadersEdit::Unchanged),
        IngestKeyChange::Clear => {
            let Some(raw) = existing else {
                return Ok(HeadersEdit::Unchanged);
            };
            let kept: Vec<String> = split_entries(raw)
                .into_iter()
                .filter(|entry| authorization_value(entry).is_none())
                .collect();
            Ok(join_or_remove(kept))
        }
        IngestKeyChange::Set { key } => {
            let key = normalize_ingest_key(key.expose())?;
            let new_entry = format!("{AUTH_HEADER_NAME}={BEARER_SCHEME} {key}");
            let mut entries: Vec<String> = Vec::new();
            let mut placed = false;
            for entry in split_entries(existing.unwrap_or("")) {
                if authorization_value(&entry).is_some() {
                    if !placed {
                        entries.push(new_entry.clone());
                        placed = true;
                    }
                } else {
                    entries.push(entry);
                }
            }
            if !placed {
                entries.push(new_entry);
            }
            Ok(join_or_remove(entries))
        }
    }
}

fn join_or_remove(entries: Vec<String>) -> HeadersEdit {
    if entries.is_empty() {
        HeadersEdit::Remove
    } else {
        HeadersEdit::Replace(entries.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테스트 전용 가짜 키(`mhk_` + 43자). 실제 키가 아니다.
    fn fake_key(fill: char) -> String {
        format!("mhk_{}", fill.to_string().repeat(KEY_BODY_LEN))
    }

    #[test]
    fn fake_key_has_hub_length() {
        assert_eq!(fake_key('A').len(), 47);
    }

    // ---- 형식 검증(허브 정규식 /^mhk_[A-Za-z0-9_-]{43}$/ 와 동일) ----

    #[test]
    fn accepts_exactly_47_chars() {
        let key = fake_key('A');
        assert_eq!(normalize_ingest_key(&key).unwrap(), key);
    }

    #[test]
    fn rejects_46_and_48_chars() {
        let k47 = fake_key('A');
        assert!(normalize_ingest_key(&k47[..46]).is_err(), "46자");
        assert!(normalize_ingest_key(&format!("{k47}A")).is_err(), "48자");
    }

    #[test]
    fn accepts_underscore_and_hyphen_and_digits_in_body() {
        let body: String = "a_-Z09".repeat(7) + "a"; // 43자
        let key = format!("mhk_{body}");
        assert_eq!(key.len(), 47);
        assert_eq!(normalize_ingest_key(&key).unwrap(), key);
    }

    #[test]
    fn rejects_forbidden_characters_in_body() {
        for bad in ['+', '/', '=', '.', ' ', '*', '한', '\n', '"', '\''] {
            let mut body: Vec<char> = "A".repeat(KEY_BODY_LEN).chars().collect();
            body[10] = bad;
            let key = format!("mhk_{}", body.into_iter().collect::<String>());
            assert!(normalize_ingest_key(&key).is_err(), "금지 문자 {bad:?}");
        }
    }

    #[test]
    fn rejects_wrong_prefix() {
        let body = "A".repeat(KEY_BODY_LEN);
        for prefix in ["mhk-", "MHK_", "mhx_", "mh_k", "xmhk", "_mhk"] {
            assert!(
                normalize_ingest_key(&format!("{prefix}{body}")).is_err(),
                "{prefix}"
            );
        }
        // 접두사 없이 47자
        assert!(normalize_ingest_key(&"A".repeat(47)).is_err());
        assert!(normalize_ingest_key("").is_err());
        assert!(normalize_ingest_key("   ").is_err());
    }

    #[test]
    fn rejects_non_ascii_without_panicking_on_char_boundary() {
        // 바이트 길이는 47이지만 비ASCII가 섞인 경우 / 경계 슬라이스 패닉이 없어야 한다.
        let tricky = format!("mhk_{}한", "A".repeat(39));
        assert!(normalize_ingest_key(&tricky).is_err());
        assert!(normalize_ingest_key("Authorization=한글한글한글한글").is_err());
        assert!(normalize_ingest_key("한").is_err());
    }

    // ---- 붙여넣기 관대 처리 ----

    #[test]
    fn trims_surrounding_whitespace_and_newline() {
        let key = fake_key('B');
        assert_eq!(normalize_ingest_key(&format!("  {key}\n")).unwrap(), key);
    }

    #[test]
    fn accepts_bearer_and_full_header_paste() {
        let key = fake_key('C');
        assert_eq!(normalize_ingest_key(&format!("Bearer {key}")).unwrap(), key);
        assert_eq!(
            normalize_ingest_key(&format!("Authorization=Bearer {key}")).unwrap(),
            key
        );
        assert_eq!(
            normalize_ingest_key(&format!(" authorization = bearer   {key} ")).unwrap(),
            key
        );
    }

    #[test]
    fn rejects_malformed_wrappers() {
        let key = fake_key('D');
        assert!(normalize_ingest_key(&format!("Bearer{key}")).is_err());
        assert!(normalize_ingest_key(&format!("Authorization {key}")).is_err());
        assert!(normalize_ingest_key(&format!("Authorization=Basic {key}")).is_err());
        assert!(normalize_ingest_key(&format!("Bearer Bearer {key}")).is_err());
        assert!(normalize_ingest_key(&format!("\"{key}\"")).is_err());
        assert!(normalize_ingest_key(&format!("{key},x=y")).is_err());
        assert!(normalize_ingest_key(&format!("{key} {key}")).is_err());
    }

    // ---- 에러 문자열·Debug 출력에 원문이 없다 ----

    #[test]
    fn error_message_never_contains_input() {
        let secret_like = "mhk_SECRETSECRETSECRETSECRETSECRETSECRETSEC+"; // 금지 문자 포함
        let err = normalize_ingest_key(secret_like).unwrap_err();
        assert!(!err.contains("SECRET"));
        assert!(!err.contains("mhk_S"));
        let err = apply_change(
            None,
            &IngestKeyChange::Set {
                key: SecretString::new(secret_like),
            },
        )
        .unwrap_err();
        assert!(!err.contains("SECRET"));
    }

    #[test]
    fn debug_output_redacts_secret() {
        let key = fake_key('Z');
        let secret = SecretString::new(&key);
        assert!(!format!("{secret:?}").contains("ZZZZ"));
        let change = IngestKeyChange::Set { key: secret };
        assert!(!format!("{change:?}").contains("ZZZZ"));
        assert!(!format!("{:?}", Some(change.clone())).contains("ZZZZ"));
    }

    #[test]
    fn change_deserializes_from_frontend_shapes() {
        let key = fake_key('E');
        let set: IngestKeyChange =
            serde_json::from_str(&format!(r#"{{"action":"set","key":"{key}"}}"#)).unwrap();
        assert!(matches!(set, IngestKeyChange::Set { .. }));
        let clear: IngestKeyChange = serde_json::from_str(r#"{"action":"clear"}"#).unwrap();
        assert!(matches!(clear, IngestKeyChange::Clear));
        let keep: IngestKeyChange = serde_json::from_str(r#"{"action":"keep"}"#).unwrap();
        assert!(matches!(keep, IngestKeyChange::Keep));
        assert!(matches!(IngestKeyChange::default(), IngestKeyChange::Keep));
    }

    // ---- 상태 분류·마스킹 ----

    #[test]
    fn classify_unset_set_unrecognized() {
        let key = fake_key('F');
        assert_eq!(classify(None), (IngestKeyState::Unset, None));
        assert_eq!(classify(Some("")), (IngestKeyState::Unset, None));
        assert_eq!(
            classify(Some("x-team=a,x-env=b")),
            (IngestKeyState::Unset, None)
        );
        let (state, masked) = classify(Some(&format!("Authorization=Bearer {key}")));
        assert_eq!(state, IngestKeyState::Set);
        assert_eq!(masked.as_deref(), Some("mhk_FFFF…"));
        // 헤더 이름 대소문자 무시, 다른 항목과 섞여 있어도 인식
        let (state, _) = classify(Some(&format!("x-a=b, authorization=Bearer {key} ,x-c=d")));
        assert_eq!(state, IngestKeyState::Set);
    }

    #[test]
    fn classify_unrecognized_hides_foreign_value() {
        for raw in [
            "Authorization=Bearer not-a-hub-key-SECRETVALUE",
            "Authorization=Basic dXNlcjpwYXNz",
            "Authorization=Bearer%20mhk_SECRETVALUE",
            "Authorization=bearer mhk_SECRETVALUE",
            "Authorization=",
        ] {
            let (state, masked) = classify(Some(raw));
            assert_eq!(state, IngestKeyState::Unrecognized, "{raw}");
            assert!(masked.is_none(), "{raw}");
        }
    }

    #[test]
    fn status_serialization_has_no_raw_key() {
        let key = fake_key('Q');
        let status = IngestKeyStatus::from_headers(
            Some(&format!("Authorization=Bearer {key}")),
            "https://hub.example/keys".to_string(),
        );
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("mhk_QQQQ…"));
        assert!(!json.contains(&key));
        assert!(!json.contains("QQQQQ"), "마스킹 이상으로 노출됨: {json}");
        assert!(!format!("{status:?}").contains("QQQQQ"));
        assert!(json.contains(r#""state":"set""#));
        assert!(json.contains("issueUrl"));
    }

    // ---- 변경 적용: 다른 헤더 보존 ----

    #[test]
    fn set_appends_when_no_authorization_and_keeps_others() {
        let key = fake_key('G');
        let edit = apply_change(
            Some("x-team=platform, x-env=prod"),
            &IngestKeyChange::Set {
                key: SecretString::new(&key),
            },
        )
        .unwrap();
        assert_eq!(
            edit,
            HeadersEdit::Replace(format!(
                "x-team=platform,x-env=prod,Authorization=Bearer {key}"
            ))
        );
    }

    #[test]
    fn set_replaces_existing_authorization_in_place_including_foreign() {
        let key = fake_key('H');
        let edit = apply_change(
            Some("x-a=1,Authorization=Basic Zm9vOmJhcg==,x-b=2,authorization=Bearer old"),
            &IngestKeyChange::Set {
                key: SecretString::new(&format!("Bearer {key}")),
            },
        )
        .unwrap();
        assert_eq!(
            edit,
            HeadersEdit::Replace(format!("x-a=1,Authorization=Bearer {key},x-b=2"))
        );
    }

    #[test]
    fn set_on_missing_header_creates_value() {
        let key = fake_key('I');
        let edit = apply_change(
            None,
            &IngestKeyChange::Set {
                key: SecretString::new(&key),
            },
        )
        .unwrap();
        assert_eq!(
            edit,
            HeadersEdit::Replace(format!("Authorization=Bearer {key}"))
        );
    }

    #[test]
    fn clear_removes_only_authorization() {
        let key = fake_key('J');
        let edit = apply_change(
            Some(&format!("x-a=1,Authorization=Bearer {key},x-b=2")),
            &IngestKeyChange::Clear,
        )
        .unwrap();
        assert_eq!(edit, HeadersEdit::Replace("x-a=1,x-b=2".to_string()));
        // Authorization만 있었다면 env 키 자체를 지운다.
        let edit = apply_change(
            Some(&format!("Authorization=Bearer {key}")),
            &IngestKeyChange::Clear,
        )
        .unwrap();
        assert_eq!(edit, HeadersEdit::Remove);
        // 원래 없으면 변경 없음.
        assert_eq!(
            apply_change(None, &IngestKeyChange::Clear).unwrap(),
            HeadersEdit::Unchanged
        );
    }

    #[test]
    fn keep_never_touches_existing_value() {
        let key = fake_key('K');
        assert_eq!(
            apply_change(
                Some(&format!("Authorization=Bearer {key}")),
                &IngestKeyChange::Keep
            )
            .unwrap(),
            HeadersEdit::Unchanged
        );
    }

    #[test]
    fn invalid_set_is_err_and_does_not_edit() {
        assert!(apply_change(
            Some("x-a=1"),
            &IngestKeyChange::Set {
                key: SecretString::new("mhk_short")
            }
        )
        .is_err());
        // 마스킹 문자열("mhk_AbCd…")이 그대로 돌아와도 거부된다.
        assert!(apply_change(
            Some("x-a=1"),
            &IngestKeyChange::Set {
                key: SecretString::new("mhk_AAAA…")
            }
        )
        .is_err());
    }
}
