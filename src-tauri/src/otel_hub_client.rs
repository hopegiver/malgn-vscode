// 허브 수집키 자동 발급 API 클라이언트 — `GET|POST /api/devices/self/ingest-credential`,
// `POST …/rotate` (허브 계약: malgnai-public `docs/design/device-ingest-key-provisioning.md` 요약 A~C).
//
// 이 모듈은 HTTP 왕복과 응답 해석만 한다. 언제 부르고 결과를 어떻게 다루는지(D-1~D-15)는
// `otel_link_engine.rs`가 정한다.
//
// 요청 규칙(계약 A):
//   - 인증은 `Authorization: Bearer mhd_…` 하나뿐이다(쿼리 파라미터·다른 인증 헤더로 토큰을 보내지 않는다).
//   - 경로는 계약 문자열과 정확히 일치한다(끝 슬래시·하위 경로 없음).
//   - 본문이 있는 요청(발급)만 `Content-Type: application/json`.
//   - 요청 타임아웃은 30초 이상(`REQUEST_TIMEOUT`). 너무 짧으면 서버가 처리를 끝낸 요청을 "응답 유실"로
//     오판해 rotate 재시도가 반복되고 회전 예산이 악의 없이 소진된다.
//   - 리다이렉트는 따라가지 않는다 — 토큰을 실은 요청이 다른 호스트로 재전송되는 경로를 없앤다.
//
// 비노출 불변식: 기기 토큰(`mhd_`)과 수집키(`mhk_`)는 이 모듈의 오류·Debug·로그에 실리지 않는다.
//   - 오류는 enum(`ApiError`)이고 서버 본문은 코드(모양 검사 통과분)·`blocked_reason`·`retry_at`·
//     `current`의 메타(원문 없는 필드만)만 꺼낸다. 서버가 보낸 `message`는 버린다.
//   - 발급 응답의 원문(`token`)은 `SecretString`으로만 들고 다닌다(Debug 가림, Serialize 없음).
//   - 이 모듈에는 로그 출력이 없다.

use crate::hub_auth::error::sanitize_server_code;
use crate::hub_auth::store::is_valid_token;
use crate::otel_ingest_key::SecretString;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub(crate) const CREDENTIAL_PATH: &str = "/api/devices/self/ingest-credential";
pub(crate) const ROTATE_PATH: &str = "/api/devices/self/ingest-credential/rotate";

/// 요청 전체 타임아웃. 계약은 "30초 이상"이다 — 낮추지 않는다.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(35);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// `blocked_reason`(= `issue_policy == blocked`이거나 409 `DEVICE_REISSUE_BLOCKED`). 서버 값은 snake_case
/// (`suspected_leak`·`quota`)이고 프론트로는 camelCase로 나간다.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BlockedReason {
    SuspectedLeak,
    Quota,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IssuePolicy {
    HasCurrent,
    Auto,
    ConfirmRequired,
    Blocked,
}

/// 수집키 메타(원문 없음). 앱이 쓰는 필드만 둔다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CredentialMeta {
    /// `mhk_` + 4자(8자). settings.json 키의 앞 8자와 비교한다(D-4).
    pub token_prefix: String,
    pub last_signal_at_metrics: Option<String>,
    pub last_signal_at_logs: Option<String>,
}

impl CredentialMeta {
    /// 첫 데이터가 도착했는가(D-12): metrics 또는 logs 신호 시각이 null이 아니다.
    pub(crate) fn has_signal(&self) -> bool {
        self.last_signal_at_metrics.is_some() || self.last_signal_at_logs.is_some()
    }
}

#[derive(Deserialize)]
struct MetaWire {
    token_prefix: String,
    #[serde(default)]
    last_signal_at_metrics: Option<String>,
    #[serde(default)]
    last_signal_at_logs: Option<String>,
}

/// `token_prefix`는 `mhk_` + 4자(영숫자·`_`·`-`)다. 모양이 다르면 이 응답을 신뢰하지 않는다.
fn valid_prefix(p: &str) -> bool {
    p.len() == 8
        && p.starts_with("mhk_")
        && p[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl MetaWire {
    fn into_meta(self) -> Option<CredentialMeta> {
        if !valid_prefix(&self.token_prefix) {
            return None;
        }
        // 시각 문자열은 null/비어 있음만 구분하면 되므로 길이를 제한해 둔다(임의 큰 문자열을 들고 다니지 않는다).
        let clip = |v: Option<String>| v.filter(|s| !s.is_empty() && s.len() <= 64);
        Some(CredentialMeta {
            token_prefix: self.token_prefix,
            last_signal_at_metrics: clip(self.last_signal_at_metrics),
            last_signal_at_logs: clip(self.last_signal_at_logs),
        })
    }
}

/// `GET` 200 응답.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CredentialState {
    pub current: Option<CredentialMeta>,
    pub issue_policy: IssuePolicy,
    pub blocked_reason: Option<BlockedReason>,
}

#[derive(Deserialize)]
struct StateWire {
    current: Option<MetaWire>,
    issue_policy: IssuePolicy,
    #[serde(default)]
    blocked_reason: Option<String>,
}

/// 201 응답(발급·rotate). 원문(`token`)은 `SecretString`이라 Debug에 찍히지 않는다.
#[derive(Clone, Debug)]
pub(crate) struct Created {
    pub token: SecretString,
}

#[derive(Deserialize)]
struct CreatedWire {
    token: String,
}

/// 비정상 상태 응답. 계약 C표의 `code`와 부가 필드만 담는다(서버 `message`는 버린다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpError {
    pub status: u16,
    /// 모양 검사를 통과한 서버 코드(`[A-Za-z0-9_]{1,40}`), 아니면 "UNKNOWN".
    pub code: String,
    /// 409 `DEVICE_REISSUE_BLOCKED`의 `error.blocked_reason`.
    pub blocked_reason: Option<BlockedReason>,
    /// 429의 `Retry-After`(초, 1~86400으로 제한). 헤더가 없으면 None.
    pub retry_after_secs: Option<u64>,
    /// 429 `ROTATE_BUDGET_EXCEEDED`의 `error.retry_at`(RFC 3339로 검증된 것만).
    pub retry_at: Option<String>,
    /// 409 `DEVICE_CREDENTIAL_EXISTS`의 본문 `current`.
    pub current: Option<CredentialMeta>,
}

/// API 호출 실패. "요청이 서버에 닿았는가"를 구분하는 것이 핵심이다 — rotate의 응답 유실 재시도(D-5)가
/// 이 구분에 달려 있다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ApiError {
    /// 요청을 서버에 보내지 못했다(연결 실패·TLS 실패·요청 조립 실패). 서버 쪽에서 아무 일도 없었다.
    NotSent,
    /// 요청을 보냈지만 쓸 수 있는 응답을 받지 못했다(타임아웃·연결 끊김·응답 본문 읽기 실패·해석 불가한
    /// 2xx 본문). 서버는 처리를 끝냈을 수 있다 — rotate라면 새 키가 만들어졌을 수 있다.
    Lost,
    Http(HttpError),
}

pub(crate) trait IngestApi: Send + Sync {
    /// `GET /api/devices/self/ingest-credential` → 200.
    fn get(&self, device_token: &str) -> Result<CredentialState, ApiError>;
    /// `POST /api/devices/self/ingest-credential` → 201. `confirm_reissue`는 true일 때만 본문에 실린다.
    fn create(
        &self,
        device_token: &str,
        label: &str,
        confirm_reissue: bool,
    ) -> Result<Created, ApiError>;
    /// `POST /api/devices/self/ingest-credential/rotate` → 201. 본문 없음.
    fn rotate(&self, device_token: &str) -> Result<Created, ApiError>;
}

// ---------------- 응답 해석 (순수 함수) ----------------

pub(crate) fn parse_state(body: &str) -> Result<CredentialState, ApiError> {
    let w: StateWire = serde_json::from_str(body).map_err(|_| ApiError::Lost)?;
    let current = match w.current {
        None => None,
        Some(m) => Some(m.into_meta().ok_or(ApiError::Lost)?),
    };
    Ok(CredentialState {
        current,
        issue_policy: w.issue_policy,
        blocked_reason: w.blocked_reason.as_deref().and_then(parse_blocked_reason),
    })
}

fn parse_blocked_reason(raw: &str) -> Option<BlockedReason> {
    match raw {
        "suspected_leak" => Some(BlockedReason::SuspectedLeak),
        "quota" => Some(BlockedReason::Quota),
        _ => None,
    }
}

pub(crate) fn parse_created(body: &str) -> Result<Created, ApiError> {
    let w: CreatedWire = serde_json::from_str(body).map_err(|_| ApiError::Lost)?;
    if w.token.is_empty() {
        return Err(ApiError::Lost);
    }
    Ok(Created {
        token: SecretString::new(&w.token),
    })
}

/// 비정상 응답을 오류로 바꾼다. 서버 본문은 신뢰하지 않는다 — 코드는 모양 검사를 통과한 것만 쓰고
/// `message`는 버린다.
pub(crate) fn parse_error(status: u16, retry_after_header: Option<&str>, body: &str) -> HttpError {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let err = v.get("error");
    let code = err
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .map(sanitize_server_code)
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let blocked_reason = err
        .and_then(|e| e.get("blocked_reason"))
        .and_then(|b| b.as_str())
        .and_then(parse_blocked_reason);
    let retry_at = err
        .and_then(|e| e.get("retry_at"))
        .and_then(|r| r.as_str())
        .filter(|s| s.len() <= 40 && chrono::DateTime::parse_from_rfc3339(s).is_ok())
        .map(str::to_string);
    let retry_after_secs = retry_after_header
        .and_then(|h| h.trim().parse::<u64>().ok())
        .map(|s| s.clamp(1, 86_400));
    let current = v
        .get("current")
        .and_then(|c| serde_json::from_value::<MetaWire>(c.clone()).ok())
        .and_then(MetaWire::into_meta);
    HttpError {
        status,
        code,
        blocked_reason,
        retry_after_secs,
        retry_at,
        current,
    }
}

// ---------------- reqwest 구현 ----------------

pub(crate) struct ReqwestIngestApi {
    origin: String,
    client: reqwest::blocking::Client,
}

impl ReqwestIngestApi {
    pub(crate) fn new(origin: &str) -> Result<Self, ApiError> {
        Self::build(origin, REQUEST_TIMEOUT)
    }

    /// 요청 타임아웃을 줄여 "보낸 뒤 응답이 없다"를 시험하는 용도(제품 코드는 `new`만 쓴다 — 30초 이상 하한).
    #[cfg(test)]
    pub(crate) fn with_timeout_for_test(origin: &str, timeout: Duration) -> Result<Self, ApiError> {
        Self::build(origin, timeout)
    }

    fn build(origin: &str, timeout: Duration) -> Result<Self, ApiError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("malgn-agent/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ApiError::NotSent)?;
        Ok(Self {
            origin: origin.trim_end_matches('/').to_string(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.origin, path)
    }

    /// 요청을 보내고 `expect` 상태의 본문을 돌려준다.
    fn send(
        &self,
        device_token: &str,
        req: reqwest::blocking::RequestBuilder,
        expect: u16,
    ) -> Result<String, ApiError> {
        // 형식이 어긋난 값은 헤더에 싣지 않는다(헤더 주입 방지). 이 경우 서버에는 아무것도 가지 않는다.
        if !is_valid_token(device_token) {
            return Err(ApiError::NotSent);
        }
        let resp = req.bearer_auth(device_token).send().map_err(|e| {
            if e.is_connect() || e.is_builder() {
                ApiError::NotSent
            } else {
                ApiError::Lost
            }
        })?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text().map_err(|_| ApiError::Lost)?;
        if status == expect {
            Ok(body)
        } else if (200..300).contains(&status) {
            // 기대와 다른 2xx — 서버가 처리했는지 알 수 없다.
            Err(ApiError::Lost)
        } else {
            Err(ApiError::Http(parse_error(
                status,
                retry_after.as_deref(),
                &body,
            )))
        }
    }
}

impl IngestApi for ReqwestIngestApi {
    fn get(&self, device_token: &str) -> Result<CredentialState, ApiError> {
        let body = self.send(
            device_token,
            self.client.get(self.url(CREDENTIAL_PATH)),
            200,
        )?;
        parse_state(&body)
    }

    fn create(
        &self,
        device_token: &str,
        label: &str,
        confirm_reissue: bool,
    ) -> Result<Created, ApiError> {
        let mut payload = serde_json::json!({ "label": label });
        if confirm_reissue {
            payload["confirm_reissue"] = serde_json::Value::Bool(true);
        }
        let body = self.send(
            device_token,
            self.client.post(self.url(CREDENTIAL_PATH)).json(&payload),
            201,
        )?;
        parse_created(&body)
    }

    fn rotate(&self, device_token: &str) -> Result<Created, ApiError> {
        let body = self.send(
            device_token,
            self.client.post(self.url(ROTATE_PATH)),
            201,
        )?;
        parse_created(&body)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! 요청 측까지 검증하는 목 HTTP 서버(tiny_http, 127.0.0.1 임의 포트). 실제 허브에는 아무것도 보내지 않는다.
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// 서버가 받은 요청 한 건.
    #[derive(Clone, Debug)]
    pub(crate) struct Seen {
        pub method: String,
        /// 쿼리를 포함한 요청 URL 그대로.
        pub url: String,
        /// 소문자 헤더 이름 → 값 목록.
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Seen {
        pub(crate) fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        }
    }

    pub(crate) enum Reply {
        /// (상태, 본문, 추가 헤더)
        Respond(u16, String, Vec<(&'static str, String)>),
        /// 요청을 읽은 뒤 이 시간 동안 응답하지 않는다(클라이언트 타임아웃 시험용).
        Stall(Duration),
    }

    pub(crate) fn json(status: u16, body: &str) -> Reply {
        Reply::Respond(status, body.to_string(), vec![])
    }

    /// 요청 헤더를 끝까지 읽은 뒤 응답 없이 소켓을 닫는 서버(응답 유실). tiny_http는 응답 없이 요청을 버리면
    /// 500을 보내 버리므로 원시 소켓으로 만든다. 받은 요청 수를 돌려준다.
    pub(crate) fn spawn_dropping() -> (String, Arc<Mutex<usize>>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = Arc::new(Mutex::new(0usize));
        let count2 = count.clone();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let mut seen = Vec::new();
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..n]);
                    if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                        *count2.lock().unwrap() += 1;
                        break;
                    }
                }
                drop(stream); // 응답 없이 닫는다
            }
        });
        (format!("http://127.0.0.1:{port}"), count)
    }

    pub(crate) fn spawn(script: Vec<Reply>) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for reply in script {
                let mut req = match server.recv_timeout(Duration::from_secs(20)) {
                    Ok(Some(r)) => r,
                    _ => return,
                };
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                seen2.lock().unwrap().push(Seen {
                    method: req.method().to_string(),
                    url: req.url().to_string(),
                    headers: req
                        .headers()
                        .iter()
                        .map(|h| (h.field.as_str().as_str().to_ascii_lowercase(), h.value.to_string()))
                        .collect(),
                    body,
                });
                match reply {
                    Reply::Respond(status, text, headers) => {
                        let mut resp = tiny_http::Response::from_string(text).with_status_code(status);
                        for (k, v) in headers {
                            resp = resp.with_header(
                                tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap(),
                            );
                        }
                        let _ = req.respond(resp);
                    }
                    Reply::Stall(d) => {
                        std::thread::sleep(d);
                        drop(req);
                    }
                }
            }
        });
        (format!("http://127.0.0.1:{port}"), seen)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::hub_auth::store::testing::{test_token, TEST_TOKEN};

    const GET_OK: &str = r#"{"current":{"id":"01k7abc","label":"맑은에이전트 mbp","token_prefix":"mhk_Ab3x","status":"active","last_signal_at_metrics":null,"last_signal_at_logs":null,"scope":{"id":"otlp.ingest.write"}},"last_revoked":null,"issue_policy":"has_current","blocked_reason":null}"#;

    fn created_body(fill: char) -> String {
        format!(
            r#"{{"id":"01k7abc","token_prefix":"mhk_{f}{f}{f}{f}","label":"x","created_at":"2026-10-15T01:02:03.456Z","token":"mhk_{}","env":{{"OTEL_METRICS_EXPORTER":"otlp"}},"notice":"n","scope":{{}}}}"#,
            fill.to_string().repeat(43),
            f = fill
        )
    }

    fn err_body(code: &str) -> String {
        format!(r#"{{"error":{{"code":"{code}","message":"서버 문구는 버려진다 {TEST_TOKEN}"}}}}"#)
    }

    // ---- 요청 측: 경로 정확일치·헤더 하나뿐·쿼리 없음·본문 형태 ----

    #[test]
    fn requests_use_exact_paths_one_bearer_header_no_query_and_expected_bodies() {
        let (origin, seen) = spawn(vec![
            json(200, GET_OK),
            json(201, &created_body('N')),
            json(201, &created_body('C')),
            json(201, &created_body('R')),
        ]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let tok = test_token('D');

        assert_eq!(api.get(&tok).unwrap().issue_policy, IssuePolicy::HasCurrent);
        api.create(&tok, "맑은에이전트 mbp", false).unwrap();
        api.create(&tok, "맑은에이전트 mbp", true).unwrap();
        api.rotate(&tok).unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        let expected = [
            ("GET", CREDENTIAL_PATH),
            ("POST", CREDENTIAL_PATH),
            ("POST", CREDENTIAL_PATH),
            ("POST", ROTATE_PATH),
        ];
        for (s, (method, path)) in seen.iter().zip(expected) {
            assert_eq!(s.method, method);
            assert_eq!(s.url, path, "경로는 정확히 일치해야 하고 쿼리가 없어야 한다");
            assert!(!s.url.contains('?') && !s.url.ends_with('/'));
            assert_eq!(s.header("authorization"), Some(format!("Bearer {tok}").as_str()));
            // 토큰은 Authorization 하나로만 간다 — 다른 인증/쿠키 헤더도 토큰을 싣지 않는다.
            for (name, value) in &s.headers {
                if name != "authorization" {
                    assert!(!value.contains(&tok[4..]), "{name} 헤더에 토큰이 실렸다");
                }
                assert!(
                    !matches!(name.as_str(), "cookie" | "proxy-authorization" | "x-api-key"),
                    "예상 밖 인증 헤더 {name}"
                );
            }
            assert!(!s.body.contains(&tok[4..]));
        }
        // 본문: GET/rotate는 없음, 발급은 JSON. confirm_reissue는 true일 때만 실린다.
        assert_eq!(seen[0].body, "");
        assert_eq!(seen[0].header("content-type"), None);
        let b1: serde_json::Value = serde_json::from_str(&seen[1].body).unwrap();
        assert_eq!(b1, serde_json::json!({"label":"맑은에이전트 mbp"}));
        assert_eq!(seen[1].header("content-type"), Some("application/json"));
        let b2: serde_json::Value = serde_json::from_str(&seen[2].body).unwrap();
        assert_eq!(
            b2,
            serde_json::json!({"label":"맑은에이전트 mbp","confirm_reissue":true})
        );
        assert_eq!(seen[3].body, "", "rotate는 본문을 받지 않는다");
    }

    #[test]
    fn request_timeout_is_at_least_thirty_seconds() {
        // 계약 D: 30초 미만이면 서버가 끝낸 요청을 "응답 유실"로 오판해 회전 예산이 소진된다.
        assert!(REQUEST_TIMEOUT >= Duration::from_secs(30));
    }

    // ---- 상태코드 → 응답 해석 (계약 C표) ----

    #[test]
    fn get_200_parses_every_issue_policy_and_blocked_reason() {
        let body = |policy: &str, reason: &str, current: &str| {
            format!(
                r#"{{"current":{current},"last_revoked":null,"issue_policy":"{policy}","blocked_reason":{reason}}}"#
            )
        };
        let cases = [
            (body("auto", "null", "null"), IssuePolicy::Auto, None, false),
            (body("confirm_required", "null", "null"), IssuePolicy::ConfirmRequired, None, false),
            (body("blocked", r#""quota""#, "null"), IssuePolicy::Blocked, Some(BlockedReason::Quota), false),
            (body("blocked", r#""suspected_leak""#, "null"), IssuePolicy::Blocked, Some(BlockedReason::SuspectedLeak), false),
            (GET_OK.to_string(), IssuePolicy::HasCurrent, None, true),
        ];
        let replies: Vec<Reply> = cases.iter().map(|(b, ..)| json(200, b)).collect();
        let (origin, _seen) = spawn(replies);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        for (_, policy, reason, has_current) in cases {
            let s = api.get(&test_token('D')).unwrap();
            assert_eq!(s.issue_policy, policy);
            assert_eq!(s.blocked_reason, reason);
            assert_eq!(s.current.is_some(), has_current);
        }
    }

    #[test]
    fn get_current_keeps_signal_times_only_as_presence() {
        let body = r#"{"current":{"token_prefix":"mhk_Ab3x","last_signal_at_metrics":"2026-10-15T01:00:00.000Z","last_signal_at_logs":null},"issue_policy":"has_current","blocked_reason":null}"#;
        let s = parse_state(body).unwrap();
        let cur = s.current.unwrap();
        assert!(cur.has_signal());
        assert_eq!(cur.token_prefix, "mhk_Ab3x");
        let none = parse_state(GET_OK).unwrap().current.unwrap();
        assert!(!none.has_signal());
    }

    #[test]
    fn malformed_or_unexpected_2xx_bodies_count_as_lost_not_as_success() {
        for body in [
            "".to_string(),
            "[]".to_string(),
            r#"{"issue_policy":"nope","current":null}"#.to_string(),
            r#"{"issue_policy":"has_current","current":{"token_prefix":"mhd_Ab3x"}}"#.to_string(), // 접두어 오류
            r#"{"issue_policy":"has_current","current":{"token_prefix":"mhk_Ab3"}}"#.to_string(), // 길이 오류
        ] {
            assert_eq!(parse_state(&body).unwrap_err(), ApiError::Lost, "{body}");
        }
        assert_eq!(parse_created("{}").unwrap_err(), ApiError::Lost);
        assert_eq!(parse_created(r#"{"token":""}"#).unwrap_err(), ApiError::Lost);
        let created = parse_created(&created_body('K')).unwrap();
        assert_eq!(created.token.expose().len(), 47);
    }

    #[test]
    fn create_and_rotate_with_wrong_success_status_are_lost() {
        // 201이 아닌 2xx는 서버가 처리했는지 알 수 없다 → 응답 유실로 다룬다.
        let (origin, _seen) = spawn(vec![json(200, &created_body('K')), json(204, "")]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        assert_eq!(api.create(&test_token('D'), "x", false).unwrap_err(), ApiError::Lost);
        assert_eq!(api.rotate(&test_token('D')).unwrap_err(), ApiError::Lost);
    }

    fn http_err(r: Result<impl std::fmt::Debug, ApiError>) -> HttpError {
        match r {
            Err(ApiError::Http(h)) => h,
            other => panic!("HTTP 오류여야 한다: {other:?}"),
        }
    }

    #[test]
    fn every_documented_error_status_and_code_is_mapped() {
        // (상태, code, 어느 경로에서 받는가) — 계약 C표의 행을 그대로 옮겼다.
        let table: Vec<(u16, &str)> = vec![
            (400, "VALIDATION_ERROR"),
            (401, "UNAUTHORIZED"),
            (403, "NOT_DESKTOP_TOKEN"),
            (404, "NO_DEVICE_CREDENTIAL"),
            (404, "NOT_FOUND"),
            (409, "ALREADY_RENEWED"),
            (409, "TOKEN_SUPERSEDED"),
            (409, "DEVICE_CREDENTIAL_EXISTS"),
            (409, "DEVICE_REISSUE_BLOCKED"),
            (409, "REISSUE_CONFIRMATION_REQUIRED"),
            (409, "CREDENTIAL_LIMIT"),
            (409, "ALREADY_ROTATED"),
            (429, "RATE_LIMITED"),
            (429, "ROTATE_BUDGET_EXCEEDED"),
            (500, "INTERNAL_ERROR"),
        ];
        let replies: Vec<Reply> = table
            .iter()
            .map(|(s, c)| json(*s, &err_body(c)))
            .collect();
        let (origin, _seen) = spawn(replies);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        for (status, code) in table {
            let h = http_err(api.rotate(&test_token('D')));
            assert_eq!((h.status, h.code.as_str()), (status, code));
        }
    }

    #[test]
    fn error_extras_blocked_reason_retry_after_retry_at_and_current_are_parsed() {
        let blocked = r#"{"error":{"code":"DEVICE_REISSUE_BLOCKED","message":"m","blocked_reason":"quota"}}"#;
        let budget = r#"{"error":{"code":"ROTATE_BUDGET_EXCEEDED","message":"m","retry_at":"2026-10-16T01:02:03.456Z"}}"#;
        let exists = r#"{"error":{"code":"DEVICE_CREDENTIAL_EXISTS","message":"m"},"current":{"token_prefix":"mhk_Zz9q","last_signal_at_logs":"2026-10-15T00:00:00.000Z"}}"#;
        let (origin, _seen) = spawn(vec![
            json(409, blocked),
            Reply::Respond(429, budget.into(), vec![("Retry-After", "3600".into())]),
            Reply::Respond(429, err_body("RATE_LIMITED"), vec![("Retry-After", "60".into())]),
            Reply::Respond(429, err_body("RATE_LIMITED"), vec![("Retry-After", "0".into())]),
            Reply::Respond(429, err_body("RATE_LIMITED"), vec![("Retry-After", "999999".into())]),
            Reply::Respond(429, err_body("RATE_LIMITED"), vec![("Retry-After", "Wed, 21 Oct 2026".into())]),
            json(409, exists),
        ]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let t = test_token('D');
        assert_eq!(http_err(api.create(&t, "x", false)).blocked_reason, Some(BlockedReason::Quota));
        let b = http_err(api.rotate(&t));
        assert_eq!(b.retry_after_secs, Some(3600));
        assert_eq!(b.retry_at.as_deref(), Some("2026-10-16T01:02:03.456Z"));
        assert_eq!(http_err(api.rotate(&t)).retry_after_secs, Some(60));
        assert_eq!(http_err(api.rotate(&t)).retry_after_secs, Some(1), "0초는 1초로 올린다");
        assert_eq!(http_err(api.rotate(&t)).retry_after_secs, Some(86_400), "상한 24시간");
        assert_eq!(http_err(api.rotate(&t)).retry_after_secs, None, "정수가 아닌 값은 무시");
        let e = http_err(api.create(&t, "x", false));
        let cur = e.current.unwrap();
        assert_eq!(cur.token_prefix, "mhk_Zz9q");
        assert!(cur.has_signal());
    }

    #[test]
    fn server_echoed_tokens_in_error_bodies_never_reach_the_error() {
        let evil = format!(
            r#"{{"error":{{"code":"{}","message":"bad {}","retry_at":"{}"}},"current":{{"token_prefix":"{}"}}}}"#,
            test_token('Z'),
            test_token('Y'),
            test_token('X'),
            test_token('W')
        );
        let h = parse_error(401, None, &evil);
        let all = format!("{h:?}");
        for seed in ['Z', 'Y', 'X', 'W'] {
            assert!(!all.contains(&test_token(seed)[4..]), "{all}");
        }
        assert_eq!(h.code, "UNKNOWN");
        assert!(h.retry_at.is_none() && h.current.is_none());
    }

    #[test]
    fn redirects_are_not_followed_and_surface_as_http_status() {
        let (origin, seen) = spawn(vec![Reply::Respond(
            302,
            String::new(),
            vec![("Location", "http://127.0.0.1:9/steal".into())],
        )]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let h = http_err(api.get(&test_token('D')));
        assert_eq!(h.status, 302);
        assert_eq!(seen.lock().unwrap().len(), 1, "리다이렉트를 따라가면 토큰이 다른 곳으로 간다");
    }

    // ---- 전송 계층 ----

    #[test]
    fn connection_refused_is_not_sent_and_dropped_connection_is_lost() {
        let port = {
            let s = tiny_http::Server::http("127.0.0.1:0").unwrap();
            s.server_addr().to_ip().unwrap().port()
        };
        let api = ReqwestIngestApi::new(&format!("http://127.0.0.1:{port}")).unwrap();
        assert_eq!(api.rotate(&test_token('D')).unwrap_err(), ApiError::NotSent);

        let (origin, received) = spawn_dropping();
        let api = ReqwestIngestApi::new(&origin).unwrap();
        assert_eq!(
            api.rotate(&test_token('D')).unwrap_err(),
            ApiError::Lost,
            "요청이 서버에 닿은 뒤 연결이 끊기면 응답 유실이다"
        );
        assert_eq!(*received.lock().unwrap(), 1);
    }

    #[test]
    fn a_request_that_times_out_after_being_sent_is_lost_not_unsent() {
        let (origin, seen) = spawn(vec![Reply::Stall(Duration::from_millis(1500))]);
        let api = ReqwestIngestApi::with_timeout_for_test(&origin, Duration::from_millis(300)).unwrap();
        let started = std::time::Instant::now();
        assert_eq!(api.rotate(&test_token('D')).unwrap_err(), ApiError::Lost);
        assert!(started.elapsed() < Duration::from_millis(1400), "클라이언트 타임아웃이 먼저 난다");
        assert_eq!(seen.lock().unwrap().len(), 1, "요청은 서버에 닿았다");
    }

    #[test]
    fn malformed_device_token_is_never_sent() {
        let (origin, seen) = spawn(vec![json(200, GET_OK)]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        // 수집키(mhk_) 모양은 기기 토큰이 아니다 — 가짜 값은 조립해 만든다(시크릿 스캔 오탐 방지).
        let ingest_shaped = format!("mhk_{}", "A".repeat(43));
        for bad in ["", ingest_shaped.as_str(), "mhd_short", "mhd_a\r\nX: 1"] {
            assert_eq!(api.get(bad).unwrap_err(), ApiError::NotSent, "{bad:?}");
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn errors_and_debug_output_never_contain_tokens() {
        let (origin, _seen) = spawn(vec![json(401, &err_body("UNAUTHORIZED")), json(201, &created_body('K'))]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let e = api.get(TEST_TOKEN).unwrap_err();
        let created = api.rotate(TEST_TOKEN).unwrap();
        for s in [format!("{e:?}"), format!("{created:?}")] {
            assert!(!s.contains(&TEST_TOKEN[4..]) && !s.contains("KKKK"), "{s}");
        }
    }
}
