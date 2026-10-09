// ---------------- 루프백 OAuth 공용 부분 (RFC 8252 데스크톱 앱 패턴) ----------------
// `google_oauth/mod.rs`에서 범용 부분(PKCE 값 생성, 루프백 콜백 서버, 결과 페이지)만
// 그대로 추출한 것이다 — Google ID 토큰 검증(`google_oauth/verify.rs`)과 Google 전용
// 로직(인증 URL, 코드→id_token 교환)은 건드리지 않고 google_oauth에 남겼다. 허브 기기
// 토큰 발급(`hub_auth/connect.rs`)이 같은 코드를 쓴다(설계 docs/design/device-token-auth.md
// §4 D1, §7.1). 로직은 추출 전과 같고, 유일한 차이는 콜백 오류 문구의 제공자 이름
// ("Google"/"허브")이 인자로 들어온 것뿐이다.

/// 포트 0으로 루프백 서버를 연다(OS 배정 임시 포트 — RFC 8252 권고). redirect_uri의
/// 호스트는 `127.0.0.1` 리터럴이다(서버가 desktop client에 정확일치를 요구한다, 설계 §5.1 m3).
pub(crate) fn bind_loopback() -> Result<(tiny_http::Server, String), String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("루프백 서버를 열지 못했습니다: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| "루프백 포트를 확인하지 못했습니다.".to_string())?;
    Ok((server, format!("http://127.0.0.1:{port}/callback")))
}

/// 시스템 브라우저로 URL을 연다(Tauri opener 플러그인의 Rust API).
pub(crate) fn open_in_browser(app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))
}

pub(crate) fn generate_random_urlsafe(byte_len: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// RFC 7636 PKCE S256: code_challenge = BASE64URL(SHA256(code_verifier)).
pub(crate) fn code_challenge_from_verifier(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// 루프백 콜백 요청의 경로+쿼리(`request.url()`이 주는 형태, 예:
/// "/callback?code=...&state=...")를 파싱해 code를 뽑는다. state 불일치·error
/// 파라미터·code 없음은 전부 로그인 거부 사유다.
pub(crate) fn parse_oauth_callback_url(
    provider_label: &str,
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
        return Err(format!(
            "{provider_label} 로그인이 취소되었거나 실패했습니다: {err}"
        ));
    }
    let state = state.ok_or_else(|| "콜백에 state 값이 없습니다.".to_string())?;
    if state != expected_state {
        return Err("state 값이 일치하지 않습니다(CSRF 의심) — 로그인을 거부합니다.".to_string());
    }
    code.ok_or_else(|| "콜백에 code 값이 없습니다.".to_string())
}

/// 완료 페이지 공통 뼈대 — 외부 리소스(폰트·이미지·CDN) 없이 인라인 CSS만
/// 쓴다(오프라인·사내망에서도 깨지지 않아야 한다는 작업 지시). 응답 헤더에
/// 이미 `charset=utf-8`이 붙어 있으므로 한글은 그대로 안전하다.
///
/// 문구는 "이 창을 닫고 앱으로 돌아가세요"가 아니라 "앱이 자동으로 앞으로
/// 나옵니다"로 쓴다 — `wait_for_oauth_callback`이 이 응답을 보낸 직후
/// `window_focus::focus_main_window`를 호출해 실제로 그렇게 동작하기
/// 때문이다(코드와 문구가 어긋나지 않게 이 함수 바로 아래에서 그 호출이
/// 일어난다).
fn oauth_result_page(accent: &str, icon: &str, title: &str, detail: &str) -> String {
    format!(
        r#"<!doctype html><html lang="ko"><head><meta charset="utf-8">
<title>맑은에이전트 로그인</title>
<style>
  html,body{{height:100%;margin:0;}}
  body{{
    display:flex;align-items:center;justify-content:center;
    font-family:-apple-system,"Malgun Gothic",sans-serif;
    background:#0f1115;color:#e6e6e6;
  }}
  .card{{
    text-align:center;padding:40px 48px;border-radius:16px;
    background:#1a1d24;box-shadow:0 8px 24px rgba(0,0,0,0.35);
    max-width:360px;
  }}
  .icon{{
    width:56px;height:56px;line-height:56px;margin:0 auto 20px;
    border-radius:50%;font-size:28px;color:#0f1115;background:{accent};
  }}
  h1{{font-size:18px;margin:0 0 8px;}}
  p{{font-size:14px;line-height:1.6;color:#9ba1ac;margin:0;}}
</style></head>
<body>
  <div class="card">
    <div class="icon">{icon}</div>
    <h1>{title}</h1>
    <p>{detail}</p>
  </div>
</body></html>"#
    )
}

fn oauth_success_page() -> String {
    oauth_result_page(
        "#3ddc84",
        "&#10003;",
        "로그인 완료",
        "맑은에이전트 창이 자동으로 앞에 나타납니다.<br>혹시 나타나지 않으면 이 창을 닫고 직접 전환해주세요.",
    )
}

fn oauth_failure_page() -> String {
    oauth_result_page(
        "#f2545b",
        "&#10005;",
        "로그인 실패",
        "맑은에이전트 창이 자동으로 앞에 나타납니다.<br>거기서 오류 메시지를 확인해주세요.",
    )
}

/// 루프백 서버 하나로 딱 한 번의 콜백 요청만 받고 즉시 닫는다. 최대 5분 대기.
///
/// 창 포커스 정책(작업 지시로 판단): 브라우저가 실제로 콜백을 보내온
/// 경우에만(`Ok(Some(request))`) 포커스한다 — 성공/실패(Google이 보낸
/// error 파라미터, state 불일치 등) 모두 포함한다. 둘 다 "사용자가 방금
/// 브라우저에서 어떤 식으로든 응답을 끝냈다"는 동일한 신호이기 때문이다
/// (이 앱에는 이 대기를 사용자가 직접 취소하는 버튼이 없다 — `authApi.ts`가
/// 그냥 invoke를 기다릴 뿐이다 — 그래서 "이미 앱 화면을 보고 있어서
/// 포커스가 불필요한" 경로가 애초에 없다). 반대로 5분 타임아웃(`Ok(None)`
/// 반복 끝 데드라인 도달)이나 서버 자체 오류는 포커스하지 않는다 — 콜백이
/// 한 번도 오지 않았다는 것은 사용자가 이미 브라우저 탭을 닫았거나 다른
/// 일로 떠났을 가능성이 높고, 5분이나 지난 시점에 갑자기 창을 앞으로
/// 당기면 그 사이 다른 작업을 하던 사용자를 방해한다(작업 지시의 "방해"
/// 사례에 해당).
pub(crate) async fn wait_for_oauth_callback(
    app: tauri::AppHandle,
    provider_label: &'static str,
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
                    let result = parse_oauth_callback_url(provider_label, &raw, &expected_state);
                    let body = if result.is_ok() {
                        oauth_success_page()
                    } else {
                        oauth_failure_page()
                    };
                    if let Ok(header) = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    ) {
                        let response = tiny_http::Response::from_string(body).with_header(header);
                        let _ = request.respond(response);
                    }
                    crate::window_focus::focus_main_window(&app);
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
    fn parses_valid_oauth_callback() {
        let code = parse_oauth_callback_url("Google", "/callback?code=abc123&state=xyz", "xyz")
            .expect("정상 콜백은 성공해야 합니다");
        assert_eq!(code, "abc123");
    }

    #[test]
    fn rejects_oauth_callback_with_mismatched_state() {
        let result = parse_oauth_callback_url("Google", "/callback?code=abc123&state=WRONG", "xyz");
        assert!(
            result.is_err(),
            "state 불일치는 CSRF 의심으로 거부되어야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_with_error_param() {
        let result =
            parse_oauth_callback_url("Google", "/callback?error=access_denied&state=xyz", "xyz");
        assert!(
            result.is_err(),
            "Google이 보낸 error 파라미터는 거부 사유여야 합니다"
        );
    }

    #[test]
    fn rejects_oauth_callback_missing_code() {
        let result = parse_oauth_callback_url("Google", "/callback?state=xyz", "xyz");
        assert!(result.is_err(), "code가 없으면 거부되어야 합니다");
    }

    // ==================== 완료 페이지(오프라인 안전 + 문구) ====================
    // 작업 지시: "HTML 본문을 테스트로 고정해라(문자열이 실수로 깨지면 잡히게)".
    // 두 페이지 공통으로 ①외부 리소스를 참조하지 않는지 ②charset=utf-8과
    // 짝이 맞는 meta 태그가 있는지 ③실제 동작(자동 포커스)과 문구가 맞는지를
    // 고정한다.
    fn assert_no_external_resources(html: &str) {
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "완료 페이지는 외부 리소스(폰트·이미지·CDN)를 참조하면 안 됩니다: {html}"
        );
        assert!(
            !html.to_lowercase().contains("<link") && !html.to_lowercase().contains("<script"),
            "완료 페이지는 <link>/<script> 태그를 쓰면 안 됩니다(인라인 CSS만): {html}"
        );
    }

    #[test]
    fn oauth_success_page_has_no_external_resources_and_utf8_meta() {
        let html = oauth_success_page();
        assert_no_external_resources(&html);
        assert!(html.contains(r#"<meta charset="utf-8">"#));
    }

    #[test]
    fn oauth_failure_page_has_no_external_resources_and_utf8_meta() {
        let html = oauth_failure_page();
        assert_no_external_resources(&html);
        assert!(html.contains(r#"<meta charset="utf-8">"#));
    }

    // 문구가 실제 동작(자동 포커스)과 어긋나지 않아야 한다 — "닫고 앱으로
    // 돌아가세요"류의, 사용자가 직접 전환해야 한다고 잘못 안내하는 문구가
    // 주 문장으로 돌아오면 이 테스트가 깨진다.
    #[test]
    fn oauth_success_page_mentions_automatic_focus_not_manual_switch() {
        let html = oauth_success_page();
        assert!(html.contains("로그인 완료"));
        assert!(
            html.contains("자동으로"),
            "자동 포커스 동작을 안내하는 문구가 있어야 합니다: {html}"
        );
    }

    #[test]
    fn oauth_failure_page_mentions_automatic_focus_not_manual_switch() {
        let html = oauth_failure_page();
        assert!(html.contains("로그인 실패"));
        assert!(
            html.contains("자동으로"),
            "자동 포커스 동작을 안내하는 문구가 있어야 합니다: {html}"
        );
    }

    #[test]
    fn oauth_success_and_failure_pages_are_visually_distinct() {
        let success = oauth_success_page();
        let failure = oauth_failure_page();
        assert_ne!(success, failure);
        assert!(
            success.contains("#3ddc84"),
            "성공 페이지는 성공 색상을 써야 합니다"
        );
        assert!(
            failure.contains("#f2545b"),
            "실패 페이지는 실패 색상을 써야 합니다"
        );
    }
}
