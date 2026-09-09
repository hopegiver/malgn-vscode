// ---------------- Jira 연동 (개인별 자격증명, macOS 키체인) ----------------
// 이 저장소 규칙은 나중에 "자율업무 실행" 기능이 그대로 읽어 쓸 계약이므로 임의로
// 바꾸지 않는다:
//   - 키체인 service = "malgn-agent:jira:{host 소문자}", account = "{이메일 소문자}"
//   - 키체인에는 토큰 원문만 저장한다(base64(email:token)을 미리 조립해 넣지 않는다
//     — Authorization 헤더는 사용 시점에 만든다)
//   - 비밀이 아닌 메타(site/host/email/accountId/displayName)는 app_config_dir 아래
//     평문 JSON 파일로 둔다. 실제로 획득 가능한 값만 저장한다(토큰 만료일 등 추측값
//     금지 — /rest/api/3/myself 응답에 없다)
//
// 저장 전 검증: GET {site}/rest/api/3/myself를 Basic 인증으로 1회 호출해 200일
// 때만 키체인에 쓴다. 실패하면 아무것도 저장하지 않는다.
//
// 토큰 유출 차단: 토큰을 프론트엔드로 돌려주는 커맨드는 없다(저장 커맨드는 상태만
// 반환, 조회 커맨드는 연결 여부와 비밀 아닌 메타만 반환). 외부 응답 본문은 에러
// 메시지에 붙이지 않고 상태코드 기반 고정 메시지로만 매핑한다.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

/// 토큰 문자열을 감싸는 뉴타입 — `Debug`/`Display`/`Serialize`를 derive하지 않아
/// `{:?}` 로그·panic 메시지·직렬화 응답 어디서도 원문이 그대로 나올 수 없다.
/// 컴파일러가 이 타입을 실수로 Serialize 구조체 필드에 넣으면 즉시 에러를 내
/// 코드리뷰가 아니라 타입체커가 유출을 막는다.
pub struct SecretToken(String);

impl SecretToken {
    fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretToken(***)")
    }
}

fn service_name(host: &str) -> String {
    format!("malgn-agent:jira:{}", host.to_lowercase())
}

/// 비밀이 아닌 메타 정보. 실제로 `/rest/api/3/myself` 응답에서 얻을 수 있는 값만
/// 담는다 — 토큰 만료일 같은 추측값은 필드 자체를 두지 않는다.
///
/// `email`은 **조회 키**다 — 키체인 account로 쓴 값과 항상 동일해야 하며(
/// `jira_status`/`jira_disconnect`가 이 값으로 키체인을 조회한다), Jira가
/// `/rest/api/3/myself`에서 돌려준 이메일로 덮어써서는 안 된다. Jira가 돌려준
/// 표시용 이메일은 `display_email`에 별도로만 담는다.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct JiraMeta {
    pub site: String,
    pub host: String,
    /// 키체인 account와 항상 동일한 조회 키(사용자가 입력한 이메일의 소문자).
    pub email: String,
    /// Jira가 `/rest/api/3/myself`에서 돌려준 표시용 이메일. 조회에는 쓰지
    /// 않는다 — `email`과 다를 수 있다(대소문자 정규화, 계정 기본 주소 차이 등).
    #[serde(rename = "displayEmail", skip_serializing_if = "Option::is_none")]
    pub display_email: Option<String>,
    #[serde(rename = "accountId")]
    pub account_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct JiraConnectionStatus {
    pub connected: bool,
    #[serde(flatten)]
    pub meta: Option<JiraMeta>,
}

fn meta_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("설정 디렉터리를 확인하지 못했습니다: {e}"))?;
    Ok(dir.join("integrations").join("jira.json"))
}

fn read_meta(app: &tauri::AppHandle) -> Option<JiraMeta> {
    let path = meta_file_path(app).ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<JiraMeta>(&content).ok()
}

fn write_meta(app: &tauri::AppHandle, meta: &JiraMeta) -> Result<(), String> {
    let path = meta_file_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    }
    let content = serde_json::to_string_pretty(meta)
        .map_err(|e| format!("메타 정보를 직렬화하지 못했습니다: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("메타 정보를 저장하지 못했습니다: {e}"))
}

fn remove_meta(app: &tauri::AppHandle) -> Result<(), String> {
    let path = meta_file_path(app)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("메타 정보를 제거하지 못했습니다: {e}")),
    }
}

/// 사용자가 입력한 사이트 URL에서 호스트만 소문자로 뽑는다(키체인 service 이름에
/// 쓰기 위함). 스킴이 없으면 https를 붙여서 파싱한다.
fn parse_host(site: &str) -> Result<String, String> {
    let trimmed = site.trim();
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = url::Url::parse(&with_scheme)
        .map_err(|_| "Jira 사이트 URL 형식이 올바르지 않습니다.".to_string())?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "Jira 사이트 URL에서 호스트를 찾을 수 없습니다.".to_string())?;
    Ok(host.to_lowercase())
}

/// `/rest/api/3/myself` 호출 URL. 트레일링 슬래시는 정리하되 사용자가 입력한
/// 스킴·경로는 그대로 존중한다(우리가 임의로 host만으로 재조립하지 않는다).
fn build_myself_url(site: &str) -> String {
    let trimmed = site.trim().trim_end_matches('/');
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    format!("{with_scheme}/rest/api/3/myself")
}

#[derive(Deserialize)]
struct JiraMyselfResponse {
    #[serde(rename = "accountId")]
    account_id: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
}

/// 외부 응답 본문을 절대 그대로 붙이지 않는다 — 상태코드만으로 고정 메시지에
/// 매핑한다(`lib.rs`의 Google 토큰 교환 에러가 응답 전문을 그대로 올리는 반례이니
/// 그 패턴을 따르지 않는다).
fn map_jira_status_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "인증에 실패했습니다. 이메일 또는 API 토큰을 확인해주세요.".to_string(),
        403 => "접근이 거부되었습니다. 계정 권한을 확인해주세요.".to_string(),
        404 => "Jira 사이트 주소를 찾을 수 없습니다. URL을 확인해주세요.".to_string(),
        code => format!("Jira 연결을 확인하지 못했습니다 (상태 코드 {code})."),
    }
}

/// ① 현재 연동 상태 조회. 메타 파일이 있어도 키체인 항목이 실제로 없으면(사용자가
/// 키체인 접근 앱에서 직접 지웠을 수 있다) "연결 안 됨"으로 되돌리고 고아가 된
/// 메타 파일을 정리한다 — 메타만 남아 "연결됨"으로 잘못 표시되는 상태를 막는다.
#[tauri::command]
pub fn jira_status(app: tauri::AppHandle) -> JiraConnectionStatus {
    let Some(meta) = read_meta(&app) else {
        return JiraConnectionStatus::default();
    };

    match keyring::Entry::new(&service_name(&meta.host), &meta.email) {
        Ok(entry) => match entry.get_password() {
            Ok(_) => JiraConnectionStatus {
                connected: true,
                meta: Some(meta),
            },
            Err(keyring::Error::NoEntry) => {
                let _ = remove_meta(&app);
                JiraConnectionStatus::default()
            }
            // 키체인 접근 자체가 불안정한 경우(잠김 등)는 메타는 보존하되 연결
            // 안 됨으로 보수적으로 보고한다 — 섣불리 메타를 지우지 않는다.
            Err(_) => JiraConnectionStatus {
                connected: false,
                meta: Some(meta),
            },
        },
        Err(_) => JiraConnectionStatus {
            connected: false,
            meta: Some(meta),
        },
    }
}

/// 키체인 account로 쓸 조회 키(`lookup_email`)와 메타에 저장할 조회 키를
/// **항상 동일하게** 만드는 유일한 지점. Jira가 돌려준 `emailAddress`는
/// `display_email`에만 담고 `email`(조회 키)을 덮어쓰지 않는다 — 덮어쓰면
/// `jira_status`/`jira_disconnect`가 키체인 account와 다른 값으로 조회하게
/// 되어 방금 저장한 자격증명을 못 찾는 결함(고아 메타 → "연결 안 됨"으로
/// 되돌아감)이 재발한다.
fn build_meta(
    site: &str,
    host: String,
    lookup_email: String,
    body: JiraMyselfResponse,
) -> JiraMeta {
    JiraMeta {
        site: site.trim().trim_end_matches('/').to_string(),
        host,
        email: lookup_email,
        display_email: body.email_address,
        account_id: body.account_id,
        display_name: body.display_name,
    }
}

/// ② 연결 시작(검증 후 저장). `GET {site}/rest/api/3/myself`를 Basic 인증으로
/// 1회 호출해 200일 때만 키체인에 쓴다. 토큰은 Authorization 헤더로만 싣고
/// `set_sensitive(true)`로 표시한다 — 쿼리스트링·URL 임베딩은 쓰지 않는다.
#[tauri::command]
pub async fn jira_connect(
    app: tauri::AppHandle,
    site: String,
    email: String,
    token: String,
) -> Result<JiraConnectionStatus, String> {
    let token = SecretToken::new(token);
    let host = parse_host(&site)?;
    let email_lower = email.trim().to_lowercase();
    if email_lower.is_empty() {
        return Err("계정 이메일을 입력해주세요.".to_string());
    }

    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD
        .encode(format!("{email_lower}:{}", token.expose_secret()));

    let mut auth_value = reqwest::header::HeaderValue::from_str(&format!("Basic {basic}"))
        .map_err(|e| format!("인증 헤더를 구성하지 못했습니다: {e}"))?;
    auth_value.set_sensitive(true);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 클라이언트를 구성하지 못했습니다: {e}"))?;

    let url = build_myself_url(&site);
    let response = client
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, auth_value)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("Jira 서버에 연결하지 못했습니다: {e}"))?;

    if !response.status().is_success() {
        return Err(map_jira_status_error(response.status()));
    }

    let body = response
        .json::<JiraMyselfResponse>()
        .await
        .map_err(|e| format!("Jira 응답을 해석하지 못했습니다: {e}"))?;

    // 키체인에는 토큰 원문만 넣는다 — base64(email:token)을 미리 조립해 넣지 않는다.
    // account는 email_lower(사용자 입력값)로 쓴다 — 메타의 조회 키(email)와
    // 반드시 같은 값이어야 하므로, 아래 build_meta에도 같은 email_lower를 넘긴다.
    let entry = keyring::Entry::new(&service_name(&host), &email_lower)
        .map_err(|e| format!("키체인 항목을 열지 못했습니다: {e}"))?;
    entry
        .set_password(token.expose_secret())
        .map_err(|e| format!("키체인에 토큰을 저장하지 못했습니다: {e}"))?;

    let meta = build_meta(&site, host, email_lower, body);
    write_meta(&app, &meta)?;

    Ok(JiraConnectionStatus {
        connected: true,
        meta: Some(meta),
    })
}

/// ③ 연결 해제. 키체인 삭제 → 메타 제거 순서로 한다(반대로 하면 조회 키인
/// host/email을 먼저 잃어 미아 항목이 남는다). 키체인에 항목이 이미 없는 경우
/// (`NoEntry`)는 사용자가 키체인 접근 앱에서 직접 지웠을 수 있으므로 성공으로
/// 취급한다 — 그렇지 않으면 UI가 "연결됨"에 영원히 갇힌다. 메타가 애초에 없으면
/// 이미 연결 해제된 것으로 보고 그대로 성공 처리한다(멱등).
#[tauri::command]
pub fn jira_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    let Some(meta) = read_meta(&app) else {
        return Ok(());
    };

    match keyring::Entry::new(&service_name(&meta.host), &meta.email) {
        Ok(entry) => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(format!("키체인에서 토큰을 지우지 못했습니다: {e}")),
        },
        Err(e) => return Err(format!("키체인 항목을 열지 못했습니다: {e}")),
    }

    remove_meta(&app)
}

/// 자율업무 실행기가 나중에 쓸 내부 전용 읽기 함수 — `#[tauri::command]`로
/// 노출하지 않는다(토큰이 프론트엔드로 나갈 경로를 만들지 않기 위해서다).
#[allow(dead_code)]
pub(crate) fn read_jira_token(host: &str, email: &str) -> Result<SecretToken, String> {
    let entry = keyring::Entry::new(&service_name(host), &email.to_lowercase())
        .map_err(|e| format!("키체인 항목을 열지 못했습니다: {e}"))?;
    let secret = entry
        .get_password()
        .map_err(|e| format!("키체인에서 토큰을 가져오지 못했습니다: {e}"))?;
    Ok(SecretToken::new(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_token_debug_never_leaks_value() {
        let token = SecretToken::new("super-secret-value".to_string());
        assert_eq!(format!("{token:?}"), "SecretToken(***)");
    }

    #[test]
    fn parse_host_lowercases_and_handles_missing_scheme() {
        assert_eq!(
            parse_host("MalgnSoft.atlassian.net").unwrap(),
            "malgnsoft.atlassian.net"
        );
        assert_eq!(
            parse_host("https://MalgnSoft.atlassian.net/").unwrap(),
            "malgnsoft.atlassian.net"
        );
    }

    #[test]
    fn parse_host_rejects_unparseable_input() {
        assert!(parse_host("").is_err());
    }

    #[test]
    fn build_myself_url_appends_path_and_trims_trailing_slash() {
        assert_eq!(
            build_myself_url("https://malgnsoft.atlassian.net/"),
            "https://malgnsoft.atlassian.net/rest/api/3/myself"
        );
        assert_eq!(
            build_myself_url("malgnsoft.atlassian.net"),
            "https://malgnsoft.atlassian.net/rest/api/3/myself"
        );
    }

    #[test]
    fn map_jira_status_error_uses_fixed_messages_not_body() {
        let msg = map_jira_status_error(reqwest::StatusCode::UNAUTHORIZED);
        assert!(msg.contains("인증에 실패"));
        let msg = map_jira_status_error(reqwest::StatusCode::NOT_FOUND);
        assert!(msg.contains("찾을 수 없습니다"));
    }

    #[test]
    fn service_name_uses_documented_naming_contract() {
        assert_eq!(
            service_name("MalgnSoft.atlassian.net"),
            "malgn-agent:jira:malgnsoft.atlassian.net"
        );
    }

    /// 결함 1 회귀 방지: 키체인 account로 쓴 값(lookup_email)과 메타에 저장돼
    /// 이후 `jira_status`/`jira_disconnect`가 조회에 쓰는 `meta.email`이
    /// 항상 같아야 한다 — Jira가 `emailAddress`로 다른 값(대소문자, 계정 기본
    /// 주소 등)을 돌려줘도 조회 키를 덮어써서는 안 된다.
    #[test]
    fn meta_email_always_matches_keychain_lookup_key_even_when_jira_returns_different_email() {
        let lookup_email = "user@example.com".to_string();
        let body = JiraMyselfResponse {
            account_id: Some("acc-1".to_string()),
            display_name: Some("Test User".to_string()),
            // Jira가 대소문자 정규화 등으로 입력값과 다른 이메일을 돌려주는 경우.
            email_address: Some("User@Example.com".to_string()),
        };

        let meta = build_meta(
            "https://example.atlassian.net",
            "example.atlassian.net".to_string(),
            lookup_email.clone(),
            body,
        );

        // 조회 키는 반드시 키체인 account로 쓴 값과 동일해야 한다.
        assert_eq!(meta.email, lookup_email);
        // 이 조회 키로 구성한 키체인 항목이 실제 쓰기에 쓰인 것과 같은 항목을 가리켜야 한다.
        assert_eq!(
            service_name(&meta.host),
            service_name("example.atlassian.net")
        );
        // Jira가 돌려준 표시용 이메일은 별도 필드에만 남고 조회 키를 오염시키지 않는다.
        assert_eq!(meta.display_email.as_deref(), Some("User@Example.com"));
    }

    /// Jira가 emailAddress를 아예 돌려주지 않아도(권한 등의 이유로 null인 경우)
    /// 조회 키는 여전히 lookup_email 그대로다.
    #[test]
    fn meta_email_matches_lookup_key_when_jira_omits_email_address() {
        let lookup_email = "user@example.com".to_string();
        let body = JiraMyselfResponse {
            account_id: None,
            display_name: None,
            email_address: None,
        };

        let meta = build_meta(
            "https://example.atlassian.net",
            "example.atlassian.net".to_string(),
            lookup_email.clone(),
            body,
        );

        assert_eq!(meta.email, lookup_email);
        assert_eq!(meta.display_email, None);
    }
}
