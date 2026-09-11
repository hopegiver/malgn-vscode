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

mod verify;

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

fn generate_random_urlsafe(byte_len: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// RFC 7636 PKCE S256: code_challenge = BASE64URL(SHA256(code_verifier)).
fn code_challenge_from_verifier(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

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

/// 루프백 콜백 요청의 경로+쿼리(`request.url()`이 주는 형태, 예:
/// "/callback?code=...&state=...")를 파싱해 code를 뽑는다. state 불일치·error
/// 파라미터·code 없음은 전부 로그인 거부 사유다.
fn parse_oauth_callback_url(
    raw_path_and_query: &str,
    expected_state: &str,
) -> Result<String, String> {
    let full = format!("http://127.0.0.1{raw_path_and_query}");
    let parsed =
        url::Url::parse(&full).map_err(|e| format!("콜백 URL을 해석하지 못했습니다: {e}"))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(format!("Google 로그인이 취소되었거나 실패했습니다: {err}"));
    }
    let state = state.ok_or_else(|| "콜백에 state 값이 없습니다.".to_string())?;
    if state != expected_state {
        return Err("state 값이 일치하지 않습니다(CSRF 의심) — 로그인을 거부합니다.".to_string());
    }
    code.ok_or_else(|| "콜백에 code 값이 없습니다.".to_string())
}

/// 루프백 서버 하나로 딱 한 번의 콜백 요청만 받고 즉시 닫는다. 최대 5분 대기.
async fn wait_for_oauth_callback(
    server: tiny_http::Server,
    expected_state: String,
) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();

    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
        loop {
            if std::time::Instant::now() > deadline {
                let _ = tx.send(Err(
                    "로그인 대기 시간이 초과되었습니다(5분). 다시 시도해주세요.".to_string(),
                ));
                return;
            }
            match server.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(Some(request)) => {
                    let raw = request.url().to_string();
                    let result = parse_oauth_callback_url(&raw, &expected_state);
                    let body = if result.is_ok() {
                        "<html><body><h3>로그인 완료 — 이 창을 닫고 앱으로 돌아가세요.</h3></body></html>"
                    } else {
                        "<html><body><h3>로그인 실패 — 앱으로 돌아가 오류 메시지를 확인하세요.</h3></body></html>"
                    };
                    if let Ok(header) = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    ) {
                        let response = tiny_http::Response::from_string(body).with_header(header);
                        let _ = request.respond(response);
                    }
                    let _ = tx.send(result);
                    return;
                }
                Ok(None) => continue, // recv_timeout — 아직 요청 없음, 데드라인까지 계속 대기
                Err(e) => {
                    let _ = tx.send(Err(format!("루프백 서버 오류: {e}")));
                    return;
                }
            }
        }
    });

    match rx.await {
        Ok(result) => result,
        Err(_) => Err("로그인 대기 채널이 예기치 않게 닫혔습니다.".to_string()),
    }
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
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("루프백 서버를 열지 못했습니다: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| "루프백 포트를 확인하지 못했습니다.".to_string())?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let auth_url = build_google_auth_url(&redirect_uri, &code_challenge, &state);

    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(&auth_url, None::<&str>)
            .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))?;
    }

    let code = wait_for_oauth_callback(server, state).await?;
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
    fn generates_pkce_challenge_matching_rfc7636_test_vector() {
        // RFC 7636 Appendix B 공식 테스트 벡터로 검증한다.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = code_challenge_from_verifier(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generates_random_urlsafe_strings_that_differ_and_are_urlsafe() {
        let a = generate_random_urlsafe(32);
        let b = generate_random_urlsafe(32);
        assert_ne!(
            a, b,
            "매번 다른 무작위 값이어야 합니다(state/verifier 재사용 방지)"
        );
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "URL-safe base64 문자만 포함해야 합니다: {a}"
        );
    }

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

    #[test]
    fn parses_valid_oauth_callback() {
        let code = parse_oauth_callback_url("/callback?code=abc123&state=xyz", "xyz")
            .expect("정상 콜백은 성공해야 합니다");
        assert_eq!(code, "abc123");
    }

    #[test]
    fn rejects_oauth_callback_with_mismatched_state() {
        let result = parse_oauth_callback_url("/callback?code=abc123&state=WRONG", "xyz");
        assert!(
            result.is_err(),
            "state 불일치는 CSRF 의심으로 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_with_error_param() {
        let result = parse_oauth_callback_url("/callback?error=access_denied&state=xyz", "xyz");
        assert!(
            result.is_err(),
            "Google이 보낸 error 파라미터는 거부 사유여야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_missing_code() {
        let result = parse_oauth_callback_url("/callback?state=xyz", "xyz");
        assert!(result.is_err(), "code가 없으면 거부되어야 합니다");
    }
}
