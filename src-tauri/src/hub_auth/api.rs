// 허브 서버 HTTP 클라이언트 (설계 §7.1 api.rs, §5).
//
// 토큰 원문을 다루는 곳이다: Authorization 헤더로 내보내고(`bearer_auth`는 헤더를
// sensitive로 표시한다), 발급·갱신 응답에서 받는다. 여기서 나가는 오류는 전부
// `HubAuthError`뿐이며 reqwest 오류는 `classify_reqwest`(URL 제거 + 분류 라벨)를 거친다.
// 요청 헤더·본문을 Debug로 찍는 로그는 없다.
//
// 타임아웃은 연결 5초·전체 15초(§7.1). 리다이렉트는 따라가지 않는다 — 토큰을 실은
// 요청이 다른 호스트로 재전송되는 경로를 구조적으로 없앤다.
//
// 동기(blocking) 클라이언트다. 이 모듈은 tokio 컨텍스트 밖의 전용 스레드에서만 호출한다
// (mod.rs의 run_blocking, 갱신 스케줄러 스레드).

use super::error::{classify_reqwest, sanitize_server_code, HubAuthError};
use super::store::is_valid_token;
use serde::Deserialize;
use std::time::Duration;

/// 발급·갱신 응답에서 받은 새 토큰. 원문을 담으므로 Serialize 없음, Debug는 마스킹.
pub(crate) struct IssuedToken {
    token: String,
    pub(crate) device_token_id: String,
    pub(crate) expires_at: String,
    pub(crate) renew_after: String,
}

impl IssuedToken {
    pub(crate) fn expose_token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedToken")
            .field("token", &"mhd_***")
            .field("device_token_id", &self.device_token_id)
            .field("expires_at", &self.expires_at)
            .field("renew_after", &self.renew_after)
            .finish()
    }
}

/// `GET /api/devices/self` 응답(토큰 없음).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfInfo {
    pub(crate) device_token_id: String,
    pub(crate) device_name: String,
    pub(crate) expires_at: String,
    pub(crate) renew_after: String,
    pub(crate) renewable: bool,
}

pub(crate) struct CodeExchange<'a> {
    pub(crate) client_id: &'a str,
    pub(crate) code: &'a str,
    pub(crate) code_verifier: &'a str,
    pub(crate) redirect_uri: &'a str,
}

/// 테스트에서 가짜 서버로 바꿀 수 있게 추상화한 허브 API.
pub(crate) trait HubApi: Send + Sync {
    fn exchange_code(&self, req: &CodeExchange<'_>) -> Result<IssuedToken, HubAuthError>;
    fn renew(&self, token: &str) -> Result<IssuedToken, HubAuthError>;
    fn self_info(&self, token: &str) -> Result<SelfInfo, HubAuthError>;
    fn delete_self(&self, token: &str) -> Result<(), HubAuthError>;
}

// ---------------- 응답 해석 (순수 함수) ----------------

#[derive(Deserialize)]
struct TokenGrantBody {
    access_token: Option<String>,
    device_token: Option<String>,
    device_token_id: Option<String>,
    expires_at: Option<String>,
    renew_after: Option<String>,
}

pub(crate) fn parse_issued(
    body: &str,
    field_is_access_token: bool,
) -> Result<IssuedToken, HubAuthError> {
    let b: TokenGrantBody = serde_json::from_str(body).map_err(|_| HubAuthError::BadResponse)?;
    let token = if field_is_access_token {
        b.access_token
    } else {
        b.device_token
    };
    let token = token.ok_or(HubAuthError::BadResponse)?;
    if !is_valid_token(&token) {
        return Err(HubAuthError::BadResponse);
    }
    Ok(IssuedToken {
        token,
        device_token_id: b
            .device_token_id
            .filter(|s| !s.is_empty())
            .ok_or(HubAuthError::BadResponse)?,
        expires_at: b.expires_at.ok_or(HubAuthError::BadResponse)?,
        renew_after: b.renew_after.ok_or(HubAuthError::BadResponse)?,
    })
}

#[derive(Deserialize)]
struct SelfBody {
    device_token_id: Option<String>,
    device_name: Option<String>,
    expires_at: Option<String>,
    renew_after: Option<String>,
    renewable: Option<bool>,
}

pub(crate) fn parse_self(body: &str) -> Result<SelfInfo, HubAuthError> {
    let b: SelfBody = serde_json::from_str(body).map_err(|_| HubAuthError::BadResponse)?;
    Ok(SelfInfo {
        device_token_id: b.device_token_id.ok_or(HubAuthError::BadResponse)?,
        device_name: b.device_name.unwrap_or_default(),
        expires_at: b.expires_at.ok_or(HubAuthError::BadResponse)?,
        renew_after: b.renew_after.ok_or(HubAuthError::BadResponse)?,
        renewable: b.renewable.unwrap_or(false),
    })
}

/// 비정상 응답을 오류로 바꾼다. 서버 본문은 신뢰하지 않는다: 코드는 모양 검사를 통과한
/// 것만 쓰고, 메시지 본문(`message`/`error_description`)은 버린다.
/// 지원 형식: 봉투 `{error:{code,message}}`, RFC `{error:"invalid_grant"}`.
pub(crate) fn error_from_response(status: u16, body: &str) -> HubAuthError {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let code = match v.get("error") {
        Some(serde_json::Value::String(s)) => sanitize_server_code(s),
        Some(obj) => obj
            .get("code")
            .and_then(|c| c.as_str())
            .map(sanitize_server_code)
            .unwrap_or_else(|| "UNKNOWN".to_string()),
        None => "UNKNOWN".to_string(),
    };
    if status == 409 && code == "RENEW_TOO_EARLY" {
        // renew_after 위치가 명세에 고정돼 있지 않아 최상위와 error 안쪽을 모두 본다.
        let ra = v
            .get("renew_after")
            .or_else(|| v.get("error").and_then(|e| e.get("renew_after")))
            .and_then(|x| x.as_str())
            .filter(|s| s.len() <= 40 && chrono::DateTime::parse_from_rfc3339(s).is_ok())
            .map(|s| s.to_string());
        return HubAuthError::RenewTooEarly { renew_after: ra };
    }
    HubAuthError::Http { status, code }
}

// ---------------- reqwest 구현 ----------------

pub(crate) struct ReqwestApi {
    origin: String,
    client: reqwest::blocking::Client,
}

impl ReqwestApi {
    pub(crate) fn new(origin: &str) -> Result<Self, HubAuthError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("malgn-agent/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(classify_reqwest)?;
        Ok(ReqwestApi {
            origin: origin.trim_end_matches('/').to_string(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.origin, path)
    }

    /// 2xx면 본문을, 아니면 해석한 오류를 돌려준다.
    fn finish(&self, req: reqwest::blocking::RequestBuilder) -> Result<String, HubAuthError> {
        let resp = req.send().map_err(classify_reqwest)?;
        let status = resp.status();
        let body = resp.text().map_err(classify_reqwest)?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(error_from_response(status.as_u16(), &body))
        }
    }
}

impl HubApi for ReqwestApi {
    fn exchange_code(&self, req: &CodeExchange<'_>) -> Result<IssuedToken, HubAuthError> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", req.code),
            ("code_verifier", req.code_verifier),
            ("redirect_uri", req.redirect_uri),
            ("client_id", req.client_id),
        ];
        let body = self.finish(self.client.post(self.url("/api/oauth/token")).form(&params))?;
        parse_issued(&body, true)
    }

    fn renew(&self, token: &str) -> Result<IssuedToken, HubAuthError> {
        let body = self.finish(
            self.client
                .post(self.url("/api/devices/renew"))
                .bearer_auth(token),
        )?;
        parse_issued(&body, false)
    }

    fn self_info(&self, token: &str) -> Result<SelfInfo, HubAuthError> {
        let body = self.finish(
            self.client
                .get(self.url("/api/devices/self"))
                .bearer_auth(token),
        )?;
        parse_self(&body)
    }

    fn delete_self(&self, token: &str) -> Result<(), HubAuthError> {
        self.finish(
            self.client
                .delete(self.url("/api/devices/self"))
                .bearer_auth(token),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_auth::store::testing::{test_token, TEST_TOKEN};
    use std::sync::{Arc, Mutex};

    // ---- 순수 파서 ----

    fn grant_body(token: &str, field: &str) -> String {
        serde_json::json!({
            field: token, "device_token_id": "01JNEW",
            "expires_at": "2099-01-01T00:00:00.000Z", "renew_after": "2098-12-01T00:00:00.000Z"
        })
        .to_string()
    }

    #[test]
    fn parses_issue_and_renew_responses() {
        let a = parse_issued(&grant_body(TEST_TOKEN, "access_token"), true).unwrap();
        assert_eq!(a.expose_token(), TEST_TOKEN);
        assert_eq!(a.device_token_id, "01JNEW");
        let r = parse_issued(&grant_body(TEST_TOKEN, "device_token"), false).unwrap();
        assert_eq!(r.expose_token(), TEST_TOKEN);
        // 필드 이름을 서로 바꿔 쓰면 거부
        assert_eq!(
            parse_issued(&grant_body(TEST_TOKEN, "device_token"), true).unwrap_err(),
            HubAuthError::BadResponse
        );
    }

    #[test]
    fn rejects_malformed_issue_responses() {
        for body in [
            "".to_string(),
            "[]".to_string(),
            grant_body("mhd_short", "access_token"),
            grant_body(
                "not_a_device_token_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                "access_token",
            ),
            r#"{"access_token":"x"}"#.to_string(),
        ] {
            assert_eq!(
                parse_issued(&body, true).unwrap_err(),
                HubAuthError::BadResponse,
                "{body}"
            );
        }
    }

    #[test]
    fn issued_token_debug_is_masked() {
        let t = parse_issued(&grant_body(TEST_TOKEN, "access_token"), true).unwrap();
        let dbg = format!("{t:?}");
        assert!(dbg.contains("mhd_***") && !dbg.contains(&TEST_TOKEN[4..]));
    }

    #[test]
    fn parses_self_response() {
        let body = r#"{"device_token_id":"01J","device_name":"mbp","expires_at":"2099-01-01T00:00:00.000Z","renew_after":"2098-12-01T00:00:00.000Z","renewable":true,"user":{"email":"a@malgnsoft.com"}}"#;
        let s = parse_self(body).unwrap();
        assert_eq!(s.device_name, "mbp");
        assert!(s.renewable);
        assert_eq!(parse_self("{}").unwrap_err(), HubAuthError::BadResponse);
    }

    #[test]
    fn error_mapping_envelope_and_rfc_forms() {
        assert_eq!(
            error_from_response(409, r#"{"error":{"code":"ALREADY_RENEWED","message":"x"}}"#),
            HubAuthError::Http {
                status: 409,
                code: "ALREADY_RENEWED".into()
            }
        );
        assert_eq!(
            error_from_response(
                400,
                r#"{"error":"invalid_grant","error_description":"bad"}"#
            ),
            HubAuthError::Http {
                status: 400,
                code: "invalid_grant".into()
            }
        );
        assert_eq!(
            error_from_response(502, "<html>Bad gateway</html>"),
            HubAuthError::Http {
                status: 502,
                code: "UNKNOWN".into()
            }
        );
    }

    #[test]
    fn too_early_carries_renew_after_from_either_location() {
        let top =
            r#"{"error":{"code":"RENEW_TOO_EARLY"},"renew_after":"2098-12-01T00:00:00.000Z"}"#;
        let inner =
            r#"{"error":{"code":"RENEW_TOO_EARLY","renew_after":"2098-12-01T00:00:00.000Z"}}"#;
        for b in [top, inner] {
            assert_eq!(
                error_from_response(409, b),
                HubAuthError::RenewTooEarly {
                    renew_after: Some("2098-12-01T00:00:00.000Z".into())
                }
            );
        }
        assert_eq!(
            error_from_response(
                409,
                r#"{"error":{"code":"RENEW_TOO_EARLY"},"renew_after":"junk"}"#
            ),
            HubAuthError::RenewTooEarly { renew_after: None }
        );
    }

    #[test]
    fn server_echoing_a_token_in_the_error_body_never_reaches_the_error() {
        let evil = format!(
            r#"{{"error":{{"code":"{}","message":"bad token {}"}}}}"#,
            test_token('Z'),
            test_token('Y')
        );
        let e = error_from_response(401, &evil);
        let all = format!("{e} {e:?}");
        assert!(!all.contains("ZZZZ") && !all.contains("YYYY"), "{all}");
    }

    // ---- 실제 reqwest ↔ 로컬 tiny_http 왕복 ----

    struct Seen {
        method: String,
        path: String,
        auth: Option<String>,
        body: String,
    }

    /// 요청 하나당 스크립트된 (status, body, extra headers)를 순서대로 응답하는 서버.
    fn spawn_server(
        script: Vec<(u16, String, Vec<(&'static str, String)>)>,
    ) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for (status, body, headers) in script {
                let mut req = match server.recv_timeout(Duration::from_secs(10)) {
                    Ok(Some(r)) => r,
                    _ => return,
                };
                let mut b = String::new();
                let _ = req.as_reader().read_to_string(&mut b);
                let auth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string());
                seen2.lock().unwrap().push(Seen {
                    method: req.method().to_string(),
                    path: req.url().to_string(),
                    auth,
                    body: b,
                });
                let mut resp = tiny_http::Response::from_string(body).with_status_code(status);
                for (k, v) in headers {
                    resp = resp.with_header(
                        tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap(),
                    );
                }
                let _ = req.respond(resp);
            }
        });
        (format!("http://127.0.0.1:{port}"), seen)
    }

    #[test]
    fn http_roundtrip_exchange_renew_self_delete() {
        let (origin, seen) = spawn_server(vec![
            (200, grant_body(TEST_TOKEN, "access_token"), vec![]),
            (200, grant_body(&test_token('R'), "device_token"), vec![]),
            (
                200,
                r#"{"device_token_id":"01J","device_name":"mbp","expires_at":"2099-01-01T00:00:00.000Z","renew_after":"2098-12-01T00:00:00.000Z","renewable":true}"#.into(),
                vec![],
            ),
            (200, r#"{"status":"revoked","revoked_count":2}"#.into(), vec![]),
        ]);
        let api = ReqwestApi::new(&origin).unwrap();
        let issued = api
            .exchange_code(&CodeExchange {
                client_id: "cid",
                code: "the-code",
                code_verifier: "the-verifier",
                redirect_uri: "http://127.0.0.1:1/callback",
            })
            .unwrap();
        assert_eq!(issued.expose_token(), TEST_TOKEN);
        let renewed = api.renew(TEST_TOKEN).unwrap();
        assert_eq!(renewed.expose_token(), test_token('R'));
        assert_eq!(api.self_info(TEST_TOKEN).unwrap().device_name, "mbp");
        api.delete_self(TEST_TOKEN).unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].path, "/api/oauth/token");
        assert!(seen[0].auth.is_none());
        for needle in [
            "grant_type=authorization_code",
            "code=the-code",
            "code_verifier=the-verifier",
            "client_id=cid",
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcallback",
        ] {
            assert!(
                seen[0].body.contains(needle),
                "{needle} in {}",
                seen[0].body
            );
        }
        assert_eq!(
            (seen[1].method.as_str(), seen[1].path.as_str()),
            ("POST", "/api/devices/renew")
        );
        assert_eq!(
            seen[1].auth.as_deref(),
            Some(format!("Bearer {TEST_TOKEN}").as_str())
        );
        assert_eq!(
            (seen[2].method.as_str(), seen[2].path.as_str()),
            ("GET", "/api/devices/self")
        );
        assert_eq!(
            (seen[3].method.as_str(), seen[3].path.as_str()),
            ("DELETE", "/api/devices/self")
        );
    }

    #[test]
    fn http_errors_are_mapped_and_never_contain_the_token() {
        let (origin, _seen) = spawn_server(vec![
            (
                401,
                r#"{"error":{"code":"UNAUTHORIZED","message":"nope"}}"#.into(),
                vec![],
            ),
            (
                409,
                r#"{"error":{"code":"ALREADY_RENEWED"}}"#.into(),
                vec![],
            ),
            (500, "boom".into(), vec![]),
        ]);
        let api = ReqwestApi::new(&origin).unwrap();
        let e1 = api.self_info(TEST_TOKEN).unwrap_err();
        let e2 = api.renew(TEST_TOKEN).unwrap_err();
        let e3 = api.self_info(TEST_TOKEN).unwrap_err();
        assert_eq!(
            e1,
            HubAuthError::Http {
                status: 401,
                code: "UNAUTHORIZED".into()
            }
        );
        assert_eq!(
            e2,
            HubAuthError::Http {
                status: 409,
                code: "ALREADY_RENEWED".into()
            }
        );
        assert_eq!(
            e3,
            HubAuthError::Http {
                status: 500,
                code: "UNKNOWN".into()
            }
        );
        for e in [e1, e2, e3] {
            let s = format!("{e} {e:?}");
            assert!(!s.contains(&TEST_TOKEN[4..]), "{s}");
        }
    }

    #[test]
    fn connection_failure_is_a_network_error_without_url_or_token() {
        // 열려 있지 않은 포트: 서버를 열었다 바로 닫아 포트를 얻는다.
        let port = {
            let s = tiny_http::Server::http("127.0.0.1:0").unwrap();
            s.server_addr().to_ip().unwrap().port()
        };
        let api = ReqwestApi::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let e = api.renew(TEST_TOKEN).unwrap_err();
        assert!(matches!(e, HubAuthError::Network(_)), "{e:?}");
        let s = format!("{e} {e:?}");
        assert!(
            !s.contains("127.0.0.1") && !s.contains(&TEST_TOKEN[4..]),
            "{s}"
        );
    }

    #[test]
    fn redirects_are_not_followed_so_the_token_never_leaves_the_host() {
        let (origin, seen) = spawn_server(vec![(
            302,
            String::new(),
            vec![("Location", "http://127.0.0.1:9/steal".to_string())],
        )]);
        let api = ReqwestApi::new(&origin).unwrap();
        let e = api.self_info(TEST_TOKEN).unwrap_err();
        assert_eq!(
            e,
            HubAuthError::Http {
                status: 302,
                code: "UNKNOWN".into()
            }
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
