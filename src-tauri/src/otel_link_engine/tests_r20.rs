// R20 — 웹 /keys 폐기(일반·유출 의심)와 앱 엔진 상태의 정렬.
//
// 허브 웹에서 앱이 발급한 기기 키("맑은에이전트 관리")를 폐기하면 앱의 다음 점검은 세 갈래 중 하나로 수렴한다.
//   재발급 가능   일반 폐기(manual): 그 키 1개만 폐기. GET `confirm_required` → 앱에서 "다시 발급"(재로그인 불필요).
//   재연결 필요   유출 의심 폐기(suspected_leak): 맑은에이전트 연결(mhd_)과 이 기기의 다른 수집키까지 폐기.
//                3경로가 401 → `GET /api/devices/self`도 401(또는 409 ALREADY_RENEWED) → 앱에서 다시 로그인해 연결.
//                (구 허브 동작인 GET `blocked`/`suspected_leak`도 같은 재연결 안내다.)
//   기능 없음     self 200: 서버에 이 기능이 없다(D-7a). 재로그인도 재발급도 아니다.
//
// 요청 쪽은 가짜 객체가 아니라 실제 `ReqwestIngestApi` ↔ 127.0.0.1 목 서버(계약 C표의 상태코드·본문)다. `GET /api/devices/self`
// 확인 호출만 `FakeProbe`로 대신한다(그 경로는 허브 인증 모듈의 일이다). 설정 파일은 임시 디렉터리뿐이다.
// 테스트 이름은 모두 `r20_`로 시작한다.

use super::testkit::*;
use super::*;
use crate::otel_hub_client::testing::{json, spawn, Reply, Seen};
use crate::otel_hub_client::Created;
use crate::otel_hub_client::{ReqwestIngestApi, CREDENTIAL_PATH, ROTATE_PATH};
use crate::otel_link::{compose_status, IngestLinkState, IngestLinkStatus, LinkFailureCause};
use std::sync::{Arc, Mutex};

const T0: u64 = 1_000_000;
const HOUR: u64 = 3_600_000;

// ---------------- 목 서버 응답 빌더 ----------------

fn get_body(policy: &str, current_prefix: Option<&str>, blocked: Option<&str>) -> String {
    let current = match current_prefix {
        Some(p) => format!(
            r#"{{"id":"01k7abc","label":"맑은에이전트 mbp","token_prefix":"{p}","status":"active","managed_by_device":true,"last_signal_at_metrics":null,"last_signal_at_logs":null,"scope":{{"id":"otlp.ingest.write"}}}}"#
        ),
        None => "null".to_string(),
    };
    let blocked = blocked.map(|b| format!(r#""{b}""#)).unwrap_or_else(|| "null".to_string());
    // 일반 폐기 뒤의 GET: 현행 키는 없고 last_revoked.revoke_reason이 manual이다.
    format!(
        r#"{{"current":{current},"last_revoked":{{"id":"01k6","token_prefix":"mhk_SSSS","revoke_reason":"manual","revoked_at":"2026-10-14T09:00:00.000Z"}},"issue_policy":"{policy}","blocked_reason":{blocked}}}"#
    )
}

fn created_body(fill: char) -> String {
    format!(
        r#"{{"id":"01k7abc","token_prefix":"{}","label":"x","created_at":"2026-10-15T01:02:03.456Z","token":"{}","env":{{}},"notice":"n","scope":{{}}}}"#,
        prefix_of(fill),
        fake_key(fill)
    )
}

fn err(status: u16, code: &str) -> Reply {
    json(status, &format!(r#"{{"error":{{"code":"{code}","message":"서버 문구"}}}}"#))
}

// ---------------- 한 벌의 장치 ----------------

struct Hub {
    path: std::path::PathBuf,
    tokens: FakeTokens,
    probe: FakeProbe,
    api: ReqwestIngestApi,
    seen: Arc<Mutex<Vec<Seen>>>,
    engine: LinkEngine,
}

impl Hub {
    /// `file_key`: 웹에서 폐기됐거나 폐기될 키가 이 PC의 settings.json에 남아 있는 상태(없으면 None).
    fn new(name: &str, file_key: Option<char>, replies: Vec<Reply>, probe: &[ProbeResult]) -> Self {
        let path = temp_settings_path(name);
        write_ready(&path, file_key.map(fake_key).as_deref());
        let (origin, seen) = spawn(replies);
        Self {
            path,
            tokens: FakeTokens::new(&device_token('D')),
            probe: FakeProbe::new(probe),
            api: ReqwestIngestApi::new(&origin).unwrap(),
            seen,
            engine: LinkEngine::new(),
        }
    }

    fn env(&self) -> Env<'_> {
        Env { path: &self.path, tokens: &self.tokens, api: &self.api, probe: &self.probe, label: "맑은에이전트 test-mac" }
    }

    fn check(&self, trigger: Trigger, now: u64) -> Phase {
        self.engine.run_check(&self.env(), trigger, now);
        self.engine.view().phase
    }

    fn status(&self) -> IngestLinkStatus {
        compose_status(&crate::otel_settings::read_ingest_inputs(&self.path), &self.engine.view(), true, true)
    }

    fn file(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
    }

    /// 지금까지 서버가 받은 요청의 (메서드, 경로).
    fn requests(&self) -> Vec<(String, String)> {
        self.seen.lock().unwrap().iter().map(|s| (s.method.clone(), s.url.clone())).collect()
    }

    /// 서버가 받은 발급(POST) 본문들.
    fn post_bodies(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.method == "POST" && s.url == CREDENTIAL_PATH)
            .map(|s| s.body.clone())
            .collect()
    }

    fn creates(&self) -> usize {
        self.post_bodies().len()
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        cleanup(&self.path);
    }
}

fn req(method: &str, path: &str) -> (String, String) {
    (method.to_string(), path.to_string())
}

// ---------------- 분기 판정: 엔진 상태가 세 갈래 중 어디인가 ----------------

#[derive(Debug, PartialEq, Eq)]
enum Stance {
    /// 사람의 확인을 받아 앱에서 바로 다시 발급할 수 있다. 재로그인 불필요.
    Reissuable,
    /// 맑은에이전트 연결이 끊겼다. 다시 발급이 아니라 다시 로그인해 연결해야 한다.
    ReconnectNeeded,
    /// 서버에 이 기능이 없다(self 200). 재로그인도 재발급도 아니다.
    FeatureMissing,
}

/// 엔진 단계와 화면 상태(플래그·실패 원인)가 한 갈래로 일치하는지 확인하고 그 갈래를 돌려준다.
fn stance(hub: &Hub) -> Stance {
    let phase = hub.engine.view().phase;
    let s = hub.status();
    let cause = s.failure.as_ref().map(|f| f.cause).or_else(|| s.notice.as_ref().map(|n| n.cause));
    match phase {
        Phase::ConfirmRequired => {
            assert!(s.confirm_required && s.revoked_by_hub, "재발급 가능: 확인 대기 + 폐기 표시: {s:?}");
            assert!(s.failure.is_none() && s.notice.is_none(), "재발급 가능한 상태는 실패가 아니다: {s:?}");
            assert!(s.hub_connected, "재로그인이 필요 없다 — 허브 연결은 살아 있다");
            Stance::Reissuable
        }
        Phase::Reconnect | Phase::Blocked(Some(BlockedReason::SuspectedLeak)) => {
            let want = if phase == Phase::Reconnect { LinkFailureCause::ReconnectRequired } else { LinkFailureCause::Blocked };
            assert_eq!(cause, Some(want), "{s:?}");
            assert_eq!(s.state, IngestLinkState::Failed, "{s:?}");
            assert!(!s.confirm_required && !s.confirm_declined, "재연결 상태에서 재발급 확인을 묻지 않는다: {s:?}");
            assert!(!s.revoked_by_hub, "재연결 상태는 '폐기된 키를 다시 발급' 표시를 달지 않는다: {s:?}");
            Stance::ReconnectNeeded
        }
        Phase::FeatureUnavailable => {
            assert_eq!(cause, Some(LinkFailureCause::FeatureUnavailable), "{s:?}");
            assert!(!s.confirm_required && !s.confirm_declined && !s.revoked_by_hub, "{s:?}");
            assert!(s.hub_connected, "{s:?}");
            Stance::FeatureMissing
        }
        other => panic!("세 갈래 중 어디도 아니다: {other:?} / {s:?}"),
    }
}

// ======================= 1. 일반 폐기(manual) — 재발급 가능, 재로그인 불필요 =======================

#[test]
fn r20_manual_revoke_get_confirm_required_is_reissuable_without_relogin() {
    // 웹에서 이 기기의 키(S) 1개를 폐기했다. 파일에는 아직 S가 남아 있다.
    let hub = Hub::new(
        "r20-manual-get",
        Some('S'),
        vec![json(200, &get_body("confirm_required", None, None)), json(201, &created_body('N'))],
        &[],
    );

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(stance(&hub), Stance::Reissuable);
    assert_eq!(hub.requests(), [req("GET", CREDENTIAL_PATH)], "사람이 답하기 전에는 발급하지 않는다");
    assert_eq!(hub.probe.count(), 0, "401이 아니므로 연결 확인(self) 호출이 없다 — 재로그인 경로로 가지 않는다");
    assert!(hub.file().contains(&fake_key('S')), "확인 전에는 파일을 바꾸지 않는다");

    // 앱에서 "다시 발급"에 동의한다 — 같은 기기 토큰으로 confirm_reissue:true를 보낸다(다시 로그인하지 않는다).
    let result = hub.engine.accept_confirm(&hub.env(), T0 + 5_000);

    assert_eq!(result, ReissueResult::Minted { prefix: prefix_of('N') });
    assert_eq!(hub.requests(), [req("GET", CREDENTIAL_PATH), req("POST", CREDENTIAL_PATH)]);
    assert!(hub.post_bodies()[0].contains(r#""confirm_reissue":true"#), "{:?}", hub.post_bodies());
    assert!(hub.file().contains(&fake_key('N')) && !hub.file().contains(&fake_key('S')));
    assert_eq!(hub.engine.view().phase, Phase::Idle);
    let s = hub.status();
    assert!(!s.confirm_required && !s.revoked_by_hub && s.failure.is_none(), "{s:?}");
    assert_eq!(hub.probe.count(), 0, "끝까지 연결 확인·재로그인이 없었다");
}

#[test]
fn r20_manual_revoke_post_409_reissue_confirmation_required_is_reissuable_without_relogin() {
    // GET은 auto였는데 POST 직전에 웹에서 폐기가 일어난 경우 — POST가 409 REISSUE_CONFIRMATION_REQUIRED를 준다.
    let hub = Hub::new(
        "r20-manual-post409",
        Some('S'),
        vec![
            json(200, &get_body("auto", None, None)),
            err(409, "REISSUE_CONFIRMATION_REQUIRED"),
            json(201, &created_body('N')),
        ],
        &[],
    );

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(stance(&hub), Stance::Reissuable);
    assert!(hub.post_bodies()[0].contains(r#""label""#) && !hub.post_bodies()[0].contains("confirm_reissue"), "첫 POST는 확인 없이 보낸다");

    let result = hub.engine.accept_confirm(&hub.env(), T0 + 5_000);

    assert_eq!(result, ReissueResult::Minted { prefix: prefix_of('N') });
    assert_eq!(hub.creates(), 2);
    assert!(hub.post_bodies()[1].contains(r#""confirm_reissue":true"#));
    assert_eq!(hub.probe.count(), 0);
    assert!(hub.file().contains(&fake_key('N')));
}

#[test]
fn r20_manual_revoke_rotate_404_then_get_confirm_required_is_reissuable_without_relogin() {
    // 회전하려는 사이에 웹에서 현행 키가 폐기됐다 — rotate가 404 NO_DEVICE_CREDENTIAL, 재조회가 confirm_required로 수렴한다.
    let hub = Hub::new(
        "r20-manual-rotate404",
        Some('S'),
        vec![
            json(200, &get_body("has_current", Some(&prefix_of('Z')), None)),
            err(404, "NO_DEVICE_CREDENTIAL"),
            json(200, &get_body("confirm_required", None, None)),
            json(201, &created_body('N')),
        ],
        &[],
    );

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(stance(&hub), Stance::Reissuable);
    assert_eq!(
        hub.requests(),
        [req("GET", CREDENTIAL_PATH), req("POST", ROTATE_PATH), req("GET", CREDENTIAL_PATH)],
        "404를 '키 없음 → 바로 POST'로 처리하지 않고 GET으로 돌아가 확인을 요구한다"
    );
    assert_eq!(hub.creates(), 0);

    let result = hub.engine.accept_confirm(&hub.env(), T0 + 5_000);

    assert_eq!(result, ReissueResult::Minted { prefix: prefix_of('N') });
    assert!(hub.post_bodies()[0].contains(r#""confirm_reissue":true"#));
    assert_eq!(hub.probe.count(), 0);
}

#[test]
fn r20_manual_revoke_declined_stays_reissuable_by_the_user_and_never_asks_for_relogin() {
    let hub = Hub::new(
        "r20-manual-declined",
        Some('S'),
        vec![json(200, &get_body("confirm_required", None, None)), json(200, &get_body("confirm_required", None, None))],
        &[],
    );
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);
    hub.engine.decline_confirm();

    let s = hub.status();
    assert!(s.confirm_declined && s.revoked_by_hub && s.failure.is_none(), "{s:?}");
    assert!(s.hub_connected);
    // 사용자가 [다시 시도]를 누르면 같은 기기 토큰으로 다시 묻는다 — 재로그인 없이.
    assert_eq!(hub.check(Trigger::User, T0 + 10_000), Phase::ConfirmRequired);
    assert_eq!(stance(&hub), Stance::Reissuable);
    assert_eq!(hub.probe.count(), 0);
    assert_eq!(hub.creates(), 0);
}

// ======================= 2. 연결이 끊김 — 재로그인·재연결 필요, 다시 발급 아님 =======================

/// 재연결 필요 상태의 공통 단언: 갈래, 문구(맑은에이전트 연결 해제 + 다시 로그인), 재발급 길 없음.
fn assert_reconnect_needed(hub: &Hub, expect_creates: usize) {
    assert_eq!(stance(hub), Stance::ReconnectNeeded);
    let s = hub.status();
    let msg = s.failure.as_ref().unwrap().message.clone();
    assert!(msg.contains("다시 로그인해 연결"), "{msg}");
    assert!(!msg.contains("다시 발급"), "연결이 끊겼는데 '다시 발급'을 행동으로 권하면 안 된다: {msg}");
    // 확인창(재발급) 길이 없다 — 동의해도 아무것도 하지 않는다.
    let before = hub.requests().len();
    let result = hub.engine.accept_confirm(&hub.env(), T0 + 99_000);
    assert_eq!(result, ReissueResult::NotStarted(NotStarted::NotPending), "재연결 필요 상태에서 재발급을 시작하면 안 된다");
    assert_eq!(hub.requests().len(), before, "추가 요청이 없다");
    assert_eq!(hub.creates(), expect_creates);
}

#[test]
fn r20_connection_lost_get_401_with_self_401_needs_relogin_not_reissue() {
    // 유출 의심 폐기: 연결(mhd_ 계열)이 폐기됐다. GET이 401이고 self도 401.
    let hub = Hub::new("r20-lost-get401", Some('S'), vec![err(401, "UNAUTHORIZED")], &[ProbeResult::Dead]);

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::Reconnect);
    assert_eq!(hub.probe.count(), 1, "401의 원인은 self 확인 호출로 가른다");
    assert_eq!(hub.status().state, IngestLinkState::Failed);
    assert_reconnect_needed(&hub, 0);
}

#[test]
fn r20_connection_lost_post_and_rotate_401_with_self_401_need_relogin_not_reissue() {
    // POST 401
    let hub = Hub::new(
        "r20-lost-post401",
        None,
        vec![json(200, &get_body("auto", None, None)), err(401, "UNAUTHORIZED")],
        &[ProbeResult::Dead],
    );
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::Reconnect);
    assert_reconnect_needed(&hub, 1);

    // rotate 401
    let hub = Hub::new(
        "r20-lost-rotate401",
        Some('S'),
        vec![json(200, &get_body("has_current", Some(&prefix_of('Z')), None)), err(401, "UNAUTHORIZED")],
        &[ProbeResult::Dead],
    );
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::Reconnect);
    assert_reconnect_needed(&hub, 0);
}

#[test]
fn r20_connection_lost_already_renewed_on_all_three_paths_needs_relogin_without_a_probe() {
    for path in ["get", "create", "rotate"] {
        let replies = match path {
            "get" => vec![err(409, "ALREADY_RENEWED")],
            "create" => vec![json(200, &get_body("auto", None, None)), err(409, "ALREADY_RENEWED")],
            _ => vec![json(200, &get_body("has_current", Some(&prefix_of('Z')), None)), err(409, "ALREADY_RENEWED")],
        };
        let hub = Hub::new("r20-lost-renewed", Some('S'), replies, &[]);
        assert_eq!(hub.check(Trigger::Startup, T0), Phase::Reconnect, "{path}");
        assert_eq!(hub.probe.count(), 0, "{path}: ALREADY_RENEWED는 확인 호출 없이 바로 재연결 흐름");
        assert_reconnect_needed(&hub, usize::from(path == "create"));
    }
}

#[test]
fn r20_suspected_leak_blocked_get_needs_relogin_and_never_posts() {
    // 구 허브 동작: GET이 200 blocked/suspected_leak. 결국 같은 재연결 안내다.
    let hub = Hub::new("r20-leak-blocked", Some('S'), vec![json(200, &get_body("blocked", None, Some("suspected_leak")))], &[]);

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::Blocked(Some(BlockedReason::SuspectedLeak)));
    assert_eq!(hub.requests(), [req("GET", CREDENTIAL_PATH)]);
    assert_eq!(hub.status().blocked_reason, Some(BlockedReason::SuspectedLeak));
    assert_reconnect_needed(&hub, 0);
    let msg = hub.status().failure.unwrap().message;
    assert!(msg.contains("다시 로그인해 연결"), "{msg}");
}

#[test]
fn r20_blocked_quota_keeps_its_guidance_and_is_not_presented_as_a_leak() {
    // 기존 안내 유지(계약 D-1: blocked → 사유를 보여 주고 연결 해제 후 다시 연결). 쿼터는 유출 의심이 아니다.
    let hub = Hub::new("r20-quota", Some('S'), vec![json(200, &get_body("blocked", None, Some("quota")))], &[]);

    assert_eq!(hub.check(Trigger::Startup, T0), Phase::Blocked(Some(BlockedReason::Quota)));
    let s = hub.status();
    assert_eq!(s.failure.as_ref().map(|f| f.cause), Some(LinkFailureCause::Blocked));
    assert_eq!(s.blocked_reason, Some(BlockedReason::Quota));
    assert!(!s.confirm_required && !s.revoked_by_hub);
    let msg = s.failure.unwrap().message;
    assert!(msg.contains("연결을 해제한 뒤 다시 로그인해 연결하세요"), "{msg}");
    assert!(!msg.contains("유출"), "{msg}");
    assert_eq!(hub.creates(), 0);
}

#[test]
fn r20_a_pending_revoke_confirmation_is_dropped_when_the_connection_turns_out_to_be_gone() {
    // 먼저 일반 폐기(확인 대기, 파일에 폐기된 키 S가 남음) → 사용자가 답하기 전에 웹에서 유출 의심 폐기까지 일어난다.
    // 다음 점검의 GET이 401이고 self도 401 — 이제 답은 "다시 발급"이 아니라 "다시 로그인해 연결"이다.
    let hub = Hub::new(
        "r20-manual-then-leak",
        Some('S'),
        vec![json(200, &get_body("confirm_required", None, None)), err(401, "UNAUTHORIZED")],
        &[ProbeResult::Dead],
    );
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(stance(&hub), Stance::Reissuable);
    assert_eq!(hub.engine.view().revoked_prefix.as_deref(), Some("mhk_SSSS"));

    assert_eq!(hub.check(Trigger::Periodic, T0 + HOUR), Phase::Reconnect);

    assert_eq!(hub.engine.view().revoked_prefix, None, "연결이 끊겼으면 이전 폐기 판정을 남기지 않는다");
    let s = hub.status();
    assert_eq!(s.masked.as_deref(), Some("mhk_SSSS…"), "파일 사실은 그대로 보인다");
    assert_reconnect_needed(&hub, 0);
}

#[test]
fn r20_a_confirmed_reissue_that_meets_a_dead_connection_reports_relogin_not_a_retryable_failure() {
    // 확인 대기 → 동의 → POST가 401, self도 401(그 사이 연결이 폐기됐다).
    let hub = Hub::new(
        "r20-accept-then-dead",
        Some('S'),
        vec![json(200, &get_body("confirm_required", None, None)), err(401, "UNAUTHORIZED")],
        &[ProbeResult::Dead],
    );
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::ConfirmRequired);

    let result = hub.engine.accept_confirm(&hub.env(), T0 + 5_000);

    assert_eq!(result, ReissueResult::Ended(Phase::Reconnect));
    assert_eq!(hub.engine.view().phase, Phase::Reconnect);
    assert!(hub.file().contains(&fake_key('S')), "키는 바뀌지 않았다");
    assert_eq!(hub.engine.view().revoked_prefix, None);
    assert_eq!(stance(&hub), Stance::ReconnectNeeded);
}

// ======================= 3. self 200 — 기능 없음 =======================

#[test]
fn r20_self_200_means_no_feature_neither_relogin_nor_reissue() {
    // 키가 없는 PC.
    let hub = Hub::new("r20-self200-nokey", None, vec![err(401, "UNAUTHORIZED")], &[ProbeResult::Alive]);
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::FeatureUnavailable);
    assert_eq!(stance(&hub), Stance::FeatureMissing);
    let s = hub.status();
    assert_eq!(s.state, IngestLinkState::Failed);
    let msg = s.failure.unwrap().message;
    assert!(msg.contains("허브 연결은 정상"), "{msg}");
    assert!(!msg.contains("로그인") && !msg.contains("다시 발급"), "재로그인도 재발급도 아니다: {msg}");
    // 재발급 길도 없다.
    assert_eq!(
        hub.engine.accept_confirm(&hub.env(), T0 + 1),
        ReissueResult::NotStarted(NotStarted::NotPending)
    );
    assert_eq!(hub.creates(), 0);
    // 1시간 뒤(또는 다음 앱 기동 시) 1번부터 다시 점검한다.
    assert_eq!(hub.engine.due(T0 + HOUR - 1), None);
    assert_eq!(hub.engine.due(T0 + HOUR), Some(Trigger::Periodic));

    // 키가 적용돼 있는 PC: 적용 사실을 지우지 않고 일시 안내(notice)로만 붙는다.
    let hub = Hub::new("r20-self200-key", Some('K'), vec![err(401, "UNAUTHORIZED")], &[ProbeResult::Alive]);
    assert_eq!(hub.check(Trigger::Startup, T0), Phase::FeatureUnavailable);
    assert_eq!(stance(&hub), Stance::FeatureMissing);
    let s = hub.status();
    assert_eq!(s.state, IngestLinkState::Applied, "{s:?}");
    assert!(s.failure.is_none() && s.notice.is_some());
}

// ======================= 4. 재발급 결과를 엔진이 직접 돌려준다(R18 RT-4) =======================

#[test]
fn r20_accept_confirm_returns_what_happened_for_every_outcome() {
    // (가) 새 키를 받아 적용했다 → Minted(앞 8자). 위 일반 폐기 테스트가 목 서버로 이미 고정한다. 여기서는 가짜 API로 나머지 결과.
    let cases: Vec<(&str, CreateReply, ReissueResult)> = vec![
        ("5xx", Err(http(503, "UNAVAILABLE")), ReissueResult::Ended(Phase::Transient)),
        ("네트워크", Err(ApiError::NotSent), ReissueResult::Ended(Phase::Transient)),
        ("429", Err(http(429, "RATE_LIMITED")), ReissueResult::Ended(Phase::RateLimited)),
        ("상한", Err(http(409, "CREDENTIAL_LIMIT")), ReissueResult::Ended(Phase::CredentialLimit)),
        (
            "차단",
            Err(http_with(409, "DEVICE_REISSUE_BLOCKED", |h| h.blocked_reason = Some(BlockedReason::Quota))),
            ReissueResult::Ended(Phase::Blocked(Some(BlockedReason::Quota))),
        ),
        ("서버 거부", Err(http(400, "VALIDATION_ERROR")), ReissueResult::Ended(Phase::HubRejected)),
        ("다시 확인 요구", Err(http(409, "REISSUE_CONFIRMATION_REQUIRED")), ReissueResult::Ended(Phase::HubRejected)),
        ("형식 불량 키", Ok(Created { token: SecretString::new("mhk_short"), env: Default::default() }), ReissueResult::Ended(Phase::InvalidKey)),
    ];
    for (label, reply, want) in cases {
        let rig = Rig::new("r20-outcome");
        write_ready(&rig.path, Some(&fake_key('S')));
        rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
        let engine = LinkEngine::new();
        engine.run_check(&rig.env(), Trigger::Startup, T0);
        rig.api.push_create(reply);

        let got = engine.accept_confirm(&rig.env(), T0 + 5_000);

        assert_eq!(got, want, "{label}");
        assert!(std::fs::read_to_string(&rig.path).unwrap().contains(&fake_key('S')), "{label}: 새 키가 적용되지 않았다");
    }

    // 다른 경로가 이미 키를 만들어 둔 경우(409 EXISTS, 앞 8자가 파일과 같다) — 새로 발급한 것이 아니다.
    let rig = Rig::new("r20-outcome-exists");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    let engine = LinkEngine::new();
    engine.run_check(&rig.env(), Trigger::Startup, T0);
    rig.api.push_create(Err(http_with(409, "DEVICE_CREDENTIAL_EXISTS", |h| {
        h.current = Some(meta(&prefix_of('S'), false));
    })));
    assert_eq!(engine.accept_confirm(&rig.env(), T0 + 5_000), ReissueResult::Ended(Phase::Idle));

    // 확인을 기다리는 상태가 아니다.
    let rig = Rig::new("r20-outcome-notpending");
    write_ready(&rig.path, None);
    assert_eq!(
        LinkEngine::new().accept_confirm(&rig.env(), T0),
        ReissueResult::NotStarted(NotStarted::NotPending)
    );
    assert!(rig.api.calls().is_empty());
}

#[test]
fn r20_accept_confirm_while_another_check_runs_reports_busy_and_sends_nothing() {
    let rig = Rig::new("r20-outcome-busy");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    let engine = LinkEngine::new();
    engine.run_check(&rig.env(), Trigger::Startup, T0);

    let held = engine.run.lock().unwrap(); // 다른 점검이 돌고 있다
    let got = engine.accept_confirm(&rig.env(), T0 + 1);
    drop(held);

    assert_eq!(got, ReissueResult::NotStarted(NotStarted::Busy));
    assert_eq!(rig.api.kinds(), ["get"], "아무것도 보내지 않았다");
    assert_eq!(engine.view().phase, Phase::ConfirmRequired, "대기 상태가 그대로라 나중에 다시 답할 수 있다");
}

#[test]
fn r20_accept_confirm_without_a_device_token_reports_no_connection_and_connection_change_discards_the_key() {
    // 기기 토큰이 없다.
    let mut rig = Rig::new("r20-outcome-notoken");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    let engine = LinkEngine::new();
    engine.run_check(&rig.env(), Trigger::Startup, T0);
    rig.tokens = FakeTokens::none();
    assert_eq!(engine.accept_confirm(&rig.env(), T0 + 1), ReissueResult::Ended(Phase::NoDeviceToken));
    assert!(rig.api.calls().iter().all(|c| !c.starts_with("create")), "토큰이 없으면 아무것도 보내지 않는다");

    // 요청 도중 연결이 바뀌었다 — 201을 받았어도 쓰지 않고 '새 키 적용'이라고 말하지 않는다.
    struct ReconnectsDuringCreate<'a> {
        engine: &'a LinkEngine,
        inner: &'a FakeApi,
    }
    impl IngestApi for ReconnectsDuringCreate<'_> {
        fn get(&self, t: &str) -> GetReply {
            self.inner.get(t)
        }
        fn create(&self, t: &str, l: &str, c: bool) -> CreateReply {
            let r = self.inner.create(t, l, c);
            self.engine.on_connected();
            r
        }
        fn rotate(&self, t: &str) -> CreateReply {
            self.inner.rotate(t)
        }
    }
    let rig = Rig::new("r20-outcome-midcall");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    rig.api.push_create(created('N'));
    let engine = LinkEngine::new();
    engine.run_check(&rig.env(), Trigger::Startup, T0);
    let api = ReconnectsDuringCreate { engine: &engine, inner: &rig.api };
    let env = Env { path: &rig.path, tokens: &rig.tokens, api: &api, probe: &rig.probe, label: &rig.label };

    let got = engine.accept_confirm(&env, T0 + 1);

    assert_eq!(got, ReissueResult::Ended(Phase::NoDeviceToken));
    let saved = std::fs::read_to_string(&rig.path).unwrap();
    assert!(saved.contains(&fake_key('S')) && !saved.contains(&fake_key('N')), "연결이 바뀐 뒤 도착한 키는 쓰지 않는다");
}

// ======================= 5. R18 N-14: after_applied의 폐기 표시 해제를 단언으로 고정 =======================

#[test]
fn r20_n14_a_newly_applied_key_releases_the_revoked_mark_itself_not_only_by_prefix_mismatch() {
    let hub = Hub::new(
        "r20-n14",
        Some('S'),
        vec![json(200, &get_body("confirm_required", None, None)), json(201, &created_body('N'))],
        &[],
    );
    hub.check(Trigger::Startup, T0);
    assert_eq!(hub.engine.view().revoked_prefix.as_deref(), Some("mhk_SSSS"), "전제: 폐기 판정이 파일 키를 가리킨다");

    let result = hub.engine.accept_confirm(&hub.env(), T0 + 5_000);

    assert_eq!(result, ReissueResult::Minted { prefix: prefix_of('N') });
    // 표시(prefix 대조)가 같은 결과를 내더라도, 엔진의 폐기 기록 자체가 비워져야 한다 — 나중에 이 줄을 지우면 이 시험이 알린다.
    assert_eq!(hub.engine.view().revoked_prefix, None);
    assert!(!hub.status().revoked_by_hub);
}
