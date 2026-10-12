// ---------------- 로그인: Google OAuth (PKCE, RFC 8252 데스크톱 앱 패턴) ----------------
// malgnsoft.com Google Workspace 계정만 허용한다. 흐름:
//   1. PKCE code_verifier/code_challenge + state(CSRF 방지) 생성
//   2. 127.0.0.1의 임시 포트에 짧게 뜨는 루프백 HTTP 서버(tiny_http)로 리다이렉트를 받는다
//   3. 시스템 브라우저로 Google 인증 URL을 연다(Tauri opener 플러그인의 Rust API — JS
//      쪽 invoke를 한 번 더 거치지 않고 Rust에서 직접 연다. capabilities는 이미
//      "opener:default"가 있어 추가 변경이 필요 없다 — 그 권한은 opener *플러그인*
//      자체를 앱에 허용하는 것이지, Rust 코드에서 그 플러그인의 Rust API를 호출하는
//      경로에는 별도 capabilities 항목이 필요 없다)
//   4. 콜백에서 code+state를 받아 state를 검증한 뒤 서버를 닫는다
//   5. code를 id_token으로 교환한다(공개 클라이언트 PKCE라 client_secret 불필요)
//   6. id_token 서명을 Google JWKS로 검증하고 iss/aud/exp/hd/email_verified를 확인한다
//      — hd 쿼리 파라미터는 힌트일 뿐이라 여기서 반드시 재검증해야 실제 제한이 된다.
// reqwest(HTTPS 직접 호출)도 Tauri의 http 플러그인이 아니라 Rust 표준 크레이트라
// capabilities와 무관하다.
//
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// ⚠️ 절대 손대지 말 것(작업 지시): Google OAuth 검증 로직 전체(이 파일 +
// `verify.rs`) — 최근 사람 승인·검증을 거친 로직이다.
// (범용 루프백 부분 — PKCE 값 생성·콜백 서버·결과 페이지 — 만 `loopback_oauth.rs`로
// 추출해 허브 연결과 공유한다. 검증 로직과 토큰 교환은 이 파일에 그대로 있다.)

mod verify;

use crate::loopback_oauth::{
    bind_loopback, code_challenge_from_verifier, generate_random_urlsafe, open_in_browser,
    wait_for_oauth_callback,
};

/// Google Cloud Console에서 발급받은 OAuth 클라이언트(유형: 데스크톱 앱) ID.
const GOOGLE_OAUTH_CLIENT_ID: &str =
    "618490200291-di1v35vf0gg1pg0up2q55jh9kk9jf4b5.apps.googleusercontent.com";
/// 데스크톱 앱 타입도 Google은 토큰 교환 시 client_secret을 요구한다(RFC 8252 순수
/// 공개클라이언트와 다른 Google 고유 동작). 값 자체는 Google 문서 기준 진짜 기밀로
/// 취급되지는 않지만, 이 저장소는 public이라 소스에 리터럴로 남기지 않는다 —
/// build.rs가 로컬 `src-tauri/.env`(gitignored, CI에서는 repo secret을 환경변수로
/// 대신 주입)에서 읽어 컴파일 타임에 `cargo:rustc-env`로 넘기고, 여기서는
/// `option_env!`로만 받는다. 빌드 시점에 값이 전혀 없었으면 None — google_oauth_login()이
/// 그 경우 패닉하지 않고 명확한 에러를 반환한다(GOOGLE_OAUTH_CLIENT_ID의 TODO 처리와
/// 동일한 패턴).
const GOOGLE_OAUTH_CLIENT_SECRET: Option<&str> = option_env!("GOOGLE_OAUTH_CLIENT_SECRET");
const GOOGLE_OAUTH_ALLOWED_DOMAIN: &str = "malgnsoft.com";
const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_JWKS_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/certs";


/// 매번 대화형(prompt 파라미터 없음) 로그인 화면을 띄운다 — 구글이 계정 선택/동의
/// 화면을 표준 방식대로 보여준다.
fn build_google_auth_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let mut url =
        url::Url::parse(GOOGLE_AUTH_ENDPOINT).expect("고정 URL 파싱은 항상 성공해야 한다");
    url.query_pairs_mut()
        .append_pair("client_id", GOOGLE_OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        // 힌트일 뿐이다 — 실제 강제는 id_token의 hd 클레임을 검증하는 쪽에서 한다.
        .append_pair("hd", GOOGLE_OAUTH_ALLOWED_DOMAIN);
    url.to_string()
}




#[derive(serde::Deserialize)]
struct GoogleTokenResponse {
    id_token: String,
}

async fn exchange_code_for_id_token(
    client: &reqwest::Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_secret: &str,
) -> Result<String, String> {
    let params = [
        ("code", code),
        ("client_id", GOOGLE_OAUTH_CLIENT_ID),
        ("client_secret", client_secret),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let resp = client
        .post(GOOGLE_TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("토큰 교환 요청을 보내지 못했습니다: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("토큰 교환이 거부되었습니다: {text}"));
    }
    let token_response = resp
        .json::<GoogleTokenResponse>()
        .await
        .map_err(|e| format!("토큰 응답을 해석하지 못했습니다: {e}"))?;
    Ok(token_response.id_token)
}

#[derive(serde::Serialize, Debug, Clone)]
pub(crate) struct GoogleLoginResult {
    email: String,
    name: String,
    hd: String,
}

struct OauthAttemptResult {
    code: String,
    code_verifier: String,
    redirect_uri: String,
}

/// 인가 시도 한 번 — 매번 새 code_verifier/state와 새 루프백 포트를 쓴다(이전 시도의
/// 서버는 콜백 하나 받으면 바로 닫히므로 재사용할 수 없다).
async fn attempt_google_oauth_authorization(
    app: &tauri::AppHandle,
) -> Result<OauthAttemptResult, String> {
    let code_verifier = generate_random_urlsafe(64);
    let code_challenge = code_challenge_from_verifier(&code_verifier);
    let state = generate_random_urlsafe(32);

    // 포트를 먼저 확보해야 redirect_uri를 만들 수 있다 — 고정 포트 대신 OS가 배정한
    // 임시 포트를 쓴다(RFC 8252 권고: 다른 앱과의 포트 충돌을 피한다).
    let (server, redirect_uri) = bind_loopback()?;

    let auth_url = build_google_auth_url(&redirect_uri, &code_challenge, &state);

    // Tauri opener 플러그인의 Rust API로 직접 연다(위 머리말 3번 참조).
    open_in_browser(app, &auth_url)?;

    let code = wait_for_oauth_callback(app.clone(), "Google", server, state).await?;
    Ok(OauthAttemptResult {
        code,
        code_verifier,
        redirect_uri,
    })
}

#[tauri::command]
pub async fn google_oauth_login(app: tauri::AppHandle) -> Result<GoogleLoginResult, String> {
    if GOOGLE_OAUTH_CLIENT_ID.starts_with("TODO") {
        return Err(
            "Google OAuth Client ID가 아직 설정되지 않았습니다. src-tauri/src/lib.rs의 GOOGLE_OAUTH_CLIENT_ID를 Google Cloud Console에서 발급받은 값으로 채워주세요.".to_string(),
        );
    }
    let client_secret = GOOGLE_OAUTH_CLIENT_SECRET.ok_or_else(|| {
        "Google OAuth Client Secret이 설정되지 않았습니다. src-tauri/.env에 GOOGLE_OAUTH_CLIENT_SECRET을 설정한 뒤 다시 빌드해주세요(build.rs가 빌드 시점에 주입합니다).".to_string()
    })?;

    let attempt = attempt_google_oauth_authorization(&app).await?;

    let client = reqwest::Client::new();
    let id_token = exchange_code_for_id_token(
        &client,
        &attempt.code,
        &attempt.code_verifier,
        &attempt.redirect_uri,
        client_secret,
    )
    .await?;
    let jwks = verify::fetch_google_jwks(&client).await?;
    let claims = verify::verify_google_id_token_signature(&id_token, &jwks, GOOGLE_OAUTH_CLIENT_ID)?;
    verify::check_domain_restriction(&claims, GOOGLE_OAUTH_ALLOWED_DOMAIN)?;

    Ok(GoogleLoginResult {
        email: claims.email.unwrap_or_default(),
        name: claims.name.unwrap_or_default(),
        hd: claims.hd.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_google_auth_url_with_required_params() {
        let url = build_google_auth_url(
            "http://127.0.0.1:54321/callback",
            "challenge123",
            "state123",
        );
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(
            url.contains("hd=malgnsoft.com"),
            "hd 힌트가 빠졌습니다: {url}"
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=challenge123"));
        assert!(url.contains("state=state123"));
        assert!(
            !url.contains("prompt="),
            "prompt는 붙지 않아야 합니다: {url}"
        );
    }
}
