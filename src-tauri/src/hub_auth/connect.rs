// 허브 연결·해제 (설계 §3.1, §3.4, §10).
//
// 연결은 기존 hub OAuth 2.1(PKCE, 루프백)을 재사용한다. 앱 Google 로그인과 공유하는 것은
// 토큰이 아니라 코드(`loopback_oauth.rs`)뿐이다. 토큰 교환·검증·저장은 전부 Rust에서
// 일어나고 프론트로는 `HubAuthStatus`(메타데이터)만 돌아간다(§7.1 M4).

use super::api::CodeExchange;
use super::consts::desktop_client_id;
use super::error::HubAuthError;
use super::renew::{acquire_lock, LockError};
use super::state::{HubAuthStatus, Runtime};
use super::store::{self, StoredToken};
use super::HubCtx;
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};

/// 브라우저 인가 한 번의 결과(PKCE verifier는 이 시도에서만 쓴다).
pub(crate) struct Attempt {
    pub(crate) code: String,
    pub(crate) code_verifier: String,
    pub(crate) redirect_uri: String,
}

/// 동시에 두 번 [허브 연결]을 눌러도 루프백 서버·브라우저 탭이 겹치지 않게 한다.
static CONNECTING: AtomicBool = AtomicBool::new(false);

struct ConnectingGuard;
impl ConnectingGuard {
    fn try_acquire() -> Option<Self> {
        CONNECTING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| ConnectingGuard)
    }
}
impl Drop for ConnectingGuard {
    fn drop(&mut self) {
        CONNECTING.store(false, Ordering::SeqCst);
    }
}

/// `{HUB}/oauth/authorize?...` (설계 §3.1 2). `response_type=code`는 설계 목록에는 없지만
/// OAuth 2.1 표준 파라미터라 덧붙인다(서버 authorize 화면은 모르는 파라미터를 무시한다).
pub(crate) fn build_authorize_url(
    origin: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> Result<String, HubAuthError> {
    let mut url = url::Url::parse(origin)
        .and_then(|u| u.join("/oauth/authorize"))
        .map_err(|_| HubAuthError::Flow("허브 주소를 해석하지 못했습니다.".to_string()))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

/// [허브 연결]: 브라우저 인가 → 코드 교환 → 키체인 저장 → self 검증.
pub(crate) async fn connect(
    app: tauri::AppHandle,
    ctx: std::sync::Arc<HubCtx>,
) -> Result<HubAuthStatus, HubAuthError> {
    use crate::loopback_oauth::{
        bind_loopback, code_challenge_from_verifier, generate_random_urlsafe, open_in_browser,
        wait_for_oauth_callback,
    };
    let client_id = desktop_client_id().ok_or(HubAuthError::NotConfigured)?;
    let _guard = ConnectingGuard::try_acquire()
        .ok_or_else(|| HubAuthError::Flow("이미 허브 연결이 진행 중입니다.".to_string()))?;

    let code_verifier = generate_random_urlsafe(64);
    let code_challenge = code_challenge_from_verifier(&code_verifier);
    let state = generate_random_urlsafe(32);
    let (server, redirect_uri) = bind_loopback().map_err(HubAuthError::Flow)?;
    let auth_url = build_authorize_url(
        &ctx.origin,
        client_id,
        &redirect_uri,
        &code_challenge,
        &state,
    )?;
    open_in_browser(&app, &auth_url).map_err(HubAuthError::Flow)?;
    // 사용자가 창을 닫으면 5분 뒤 데드라인으로 취소된다(Google 흐름과 같음, §10).
    let code = wait_for_oauth_callback(app, "허브", server, state)
        .await
        .map_err(HubAuthError::Flow)?;

    let attempt = Attempt {
        code,
        code_verifier,
        redirect_uri,
    };
    super::run_blocking(move || finish_connect(&ctx, client_id, &attempt, Utc::now())).await?
}

/// 코드 교환 이후 단계. 테스트 가능하도록 브라우저·콜백과 분리했다.
pub(crate) fn finish_connect(
    ctx: &HubCtx,
    client_id: &str,
    attempt: &Attempt,
    now: DateTime<Utc>,
) -> Result<HubAuthStatus, HubAuthError> {
    // 갱신 스케줄러와 키체인 쓰기가 겹치지 않게 같은 락을 쓴다.
    let _lock = acquire_lock(&ctx.lock_path, now.timestamp()).map_err(lock_error)?;

    let issued = ctx.api.exchange_code(&CodeExchange {
        client_id,
        code: &attempt.code,
        code_verifier: &attempt.code_verifier,
        redirect_uri: &attempt.redirect_uri,
    })?;
    let new = StoredToken::from_grant(
        &ctx.origin,
        issued.expose_token(),
        &issued.device_token_id,
        &issued.expires_at,
        &issued.renew_after,
    )?;

    // 덮어쓰기 전에 기존 항목을 기억해 둔다(같은 기기에 유효한 토큰이 여러 개 남지 않게,
    // 새 토큰 저장에 성공한 뒤 best-effort로 폐기한다).
    let old = store::load(&*ctx.store, &ctx.origin).ok().flatten();

    if let Err(e) = store::save_verified(&*ctx.store, &new) {
        // 발급 후 키체인 저장 실패: 고아 토큰을 남기지 않도록 즉시 서버 폐기를 시도한다(§10).
        let _ = ctx.api.delete_self(new.expose_token());
        return Err(e);
    }
    if let Some(old) = old {
        if old.device_token_id != new.device_token_id {
            let _ = ctx.api.delete_self(old.expose_token());
        }
    }

    // 토큰이 실제로 동작하는지 서버에 확인한다(§3.1 5).
    match ctx.api.self_info(new.expose_token()) {
        Ok(info) => {
            let mut rt = ctx.rt();
            *rt = Runtime::default();
            if !info.device_name.is_empty() {
                rt.device_name = Some(info.device_name);
            }
        }
        Err(e @ HubAuthError::Http { .. }) => {
            // 서버가 방금 받은 토큰을 거부했다. 쓸 수 없는 항목을 남기지 않고 되돌린다.
            let _ = ctx.api.delete_self(new.expose_token());
            let _ = ctx.store.delete_raw();
            return Err(e);
        }
        Err(_) => {
            // 네트워크 문제로 확인만 못 했다. 토큰은 유효하게 발급됐으므로 유지하고,
            // 다음 사이클(재시도)이 확인한다.
            let mut rt = ctx.rt();
            *rt = Runtime::default();
            rt.retrying = true;
        }
    }
    Ok(ctx.status(now))
}

fn lock_error(e: LockError) -> HubAuthError {
    match e {
        LockError::Held => HubAuthError::Flow(
            "다른 작업(갱신 등)이 진행 중입니다. 잠시 후 다시 시도하세요.".to_string(),
        ),
        LockError::Io(_) => HubAuthError::Flow("잠금 파일을 만들지 못했습니다.".to_string()),
    }
}

/// [연결 해제]: 서버 폐기(best-effort) 후 키체인 항목을 **항상** 삭제한다(§3.4).
/// 서버 폐기가 실패하면 상태에 `SERVER_REVOKE_FAILED` 안내를 한 번 실어 보낸다.
pub(crate) fn disconnect(ctx: &HubCtx, now: DateTime<Utc>) -> Result<HubAuthStatus, HubAuthError> {
    let _lock = acquire_lock(&ctx.lock_path, now.timestamp()).map_err(lock_error)?;

    // SAVE_FAILED면 서버가 알고 있는 최신 토큰은 메모리의 새 토큰이다.
    let pending = ctx.rt().pending.clone();
    let candidate = match pending {
        Some(p) => Some(p),
        None => store::load(&*ctx.store, &ctx.origin).ok().flatten(),
    };
    let server_ok = match candidate {
        None => true,
        Some(t) => matches!(
            ctx.api.delete_self(t.expose_token()),
            // 이미 폐기됐거나(401) 재사용 판정으로 계열이 폐기된 경우(409)도 목적 달성이다(§5.4).
            Ok(())
                | Err(HubAuthError::Http { status: 401, .. })
                | Err(HubAuthError::Http { status: 409, .. })
        ),
    };
    ctx.store.delete_raw()?;
    {
        let mut rt = ctx.rt();
        *rt = Runtime::default();
        if !server_ok {
            rt.notice = Some("SERVER_REVOKE_FAILED");
        }
    }
    Ok(ctx.status(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_auth::consts::HUB_ORIGIN;
    use crate::hub_auth::renew::lock_path_in;
    use crate::hub_auth::renew::testing::*;
    use crate::hub_auth::state::HubState;
    use crate::hub_auth::store::testing::*;
    use std::sync::Arc;

    fn attempt() -> Attempt {
        Attempt {
            code: "code-1".into(),
            code_verifier: "verifier-1".into(),
            redirect_uri: "http://127.0.0.1:5555/callback".into(),
        }
    }

    fn ctx_with(store: Arc<MemoryStore>) -> (Arc<FakeApi>, HubCtx) {
        let api = Arc::new(FakeApi::default());
        let ctx = HubCtx::new_for_test(
            store,
            api.clone(),
            HUB_ORIGIN,
            lock_path_in(&tmp_dir("connect")),
        );
        (api, ctx)
    }

    fn now() -> DateTime<Utc> {
        "2026-10-09T00:00:00Z".parse().unwrap()
    }

    #[test]
    fn authorize_url_has_pkce_state_and_loopback_redirect() {
        let u = build_authorize_url(
            HUB_ORIGIN,
            "cid",
            "http://127.0.0.1:5555/callback",
            "chal",
            "st",
        )
        .unwrap();
        let parsed = url::Url::parse(&u).unwrap();
        assert_eq!(parsed.origin().ascii_serialization(), HUB_ORIGIN);
        assert_eq!(parsed.path(), "/oauth/authorize");
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:5555/callback");
        assert_eq!(q["code_challenge"], "chal");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["state"], "st");
        assert_eq!(q["response_type"], "code");
        assert!(
            !q.contains_key("code_verifier"),
            "verifier는 URL에 실리면 안 된다"
        );
    }

    #[test]
    fn connect_success_stores_verifies_and_reports_metadata_only() {
        let store = Arc::new(MemoryStore::empty());
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        api.self_.lock().unwrap().push_back(self_ok(true));
        let st = finish_connect(&ctx, "cid", &attempt(), now()).unwrap();
        assert_eq!(st.state, HubState::Connected);
        assert_eq!(st.device_name.as_deref(), Some("test-mac"));
        assert!(store.raw().unwrap().contains(&test_token('A')));
        assert_eq!(
            api.calls(),
            vec!["exchange".to_string(), format!("self:{}", test_token('A'))]
        );
        assert!(!contains_token_pattern(
            &serde_json::to_string(&st).unwrap()
        ));
        assert!(!ctx.lock_path.exists());
    }

    #[test]
    fn reconnect_revokes_previous_entry_after_saving_new_one() {
        let store = Arc::new(MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            &test_token('O'),
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        )));
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        api.self_.lock().unwrap().push_back(self_ok(true));
        finish_connect(&ctx, "cid", &attempt(), now()).unwrap();
        assert_eq!(
            api.calls(),
            vec![
                "exchange".to_string(),
                format!("delete:{}", test_token('O')),
                format!("self:{}", test_token('A')),
            ]
        );
        assert!(store.raw().unwrap().contains(&test_token('A')));
    }

    #[test]
    fn keychain_write_failure_revokes_the_fresh_token_and_reports_error() {
        let store = Arc::new(MemoryStore::empty());
        *store.fail_set.lock().unwrap() = Some(HubAuthError::Keychain(
            crate::hub_auth::error::KeychainFailure::Denied,
        ));
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        let err = finish_connect(&ctx, "cid", &attempt(), now()).unwrap_err();
        assert!(matches!(err, HubAuthError::Keychain(_)));
        assert_eq!(
            api.calls(),
            vec![
                "exchange".to_string(),
                format!("delete:{}", test_token('A'))
            ],
            "고아 토큰을 남기지 않는다"
        );
        assert!(store.raw().is_none());
        assert!(!contains_token_pattern(&format!("{err} {err:?}")));
    }

    #[test]
    fn readback_mismatch_also_revokes() {
        let store = Arc::new(MemoryStore::empty());
        *store.drop_writes.lock().unwrap() = true;
        let (api, ctx) = ctx_with(store);
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        assert!(finish_connect(&ctx, "cid", &attempt(), now()).is_err());
        assert!(api.calls().contains(&format!("delete:{}", test_token('A'))));
    }

    #[test]
    fn exchange_failure_leaves_existing_entry_untouched() {
        let before = sample_json(
            HUB_ORIGIN,
            &test_token('O'),
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        );
        let store = Arc::new(MemoryStore::with_raw(&before));
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Err(http(400, "invalid_grant")));
        assert!(finish_connect(&ctx, "cid", &attempt(), now()).is_err());
        assert_eq!(store.raw().unwrap(), before);
        assert_eq!(api.calls(), vec!["exchange".to_string()]);
    }

    #[test]
    fn server_rejecting_fresh_token_rolls_back() {
        let store = Arc::new(MemoryStore::empty());
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(401, "UNAUTHORIZED")));
        assert!(finish_connect(&ctx, "cid", &attempt(), now()).is_err());
        assert!(store.raw().is_none(), "쓸 수 없는 항목을 남기지 않는다");
    }

    #[test]
    fn network_failure_on_verification_keeps_the_token() {
        let store = Arc::new(MemoryStore::empty());
        let (api, ctx) = ctx_with(store.clone());
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(HubAuthError::Network("timeout")));
        let st = finish_connect(&ctx, "cid", &attempt(), now()).unwrap();
        assert!(store.raw().is_some());
        assert_eq!(st.state, HubState::RenewRetrying);
    }

    #[test]
    fn connect_clears_previous_revoked_state() {
        let store = Arc::new(MemoryStore::empty());
        let (api, ctx) = ctx_with(store);
        ctx.rt().terminal = Some((HubState::Revoked, "ALREADY_RENEWED"));
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('A'), "01JA")));
        api.self_.lock().unwrap().push_back(self_ok(true));
        assert_eq!(
            finish_connect(&ctx, "cid", &attempt(), now())
                .unwrap()
                .state,
            HubState::Connected
        );
    }

    #[test]
    fn lock_held_blocks_connect_without_calling_server() {
        let (api, ctx) = ctx_with(Arc::new(MemoryStore::empty()));
        let _held = acquire_lock(&ctx.lock_path, now().timestamp()).unwrap();
        assert!(matches!(
            finish_connect(&ctx, "cid", &attempt(), now()),
            Err(HubAuthError::Flow(_))
        ));
        assert!(api.calls().is_empty());
    }

    // ---- disconnect ----

    fn connected_store() -> Arc<MemoryStore> {
        Arc::new(MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        )))
    }

    #[test]
    fn disconnect_revokes_on_server_then_deletes_entry() {
        let store = connected_store();
        let (api, ctx) = ctx_with(store.clone());
        let st = disconnect(&ctx, now()).unwrap();
        assert_eq!(api.calls(), vec![format!("delete:{TEST_TOKEN}")]);
        assert!(store.raw().is_none());
        assert_eq!(st.state, HubState::NotConnected);
        assert_eq!(st.reason, None);
    }

    #[test]
    fn disconnect_deletes_entry_even_if_server_fails_and_warns() {
        let store = connected_store();
        let (api, ctx) = ctx_with(store.clone());
        api.delete
            .lock()
            .unwrap()
            .push_back(Err(HubAuthError::Network("connect")));
        let st = disconnect(&ctx, now()).unwrap();
        assert!(store.raw().is_none(), "키체인은 항상 삭제");
        assert_eq!(st.reason.as_deref(), Some("SERVER_REVOKE_FAILED"));
        assert_eq!(
            disconnect(&ctx, now()).unwrap().reason,
            None,
            "안내는 한 번만"
        );
    }

    #[test]
    fn disconnect_treats_401_and_409_as_already_revoked() {
        for code in [401u16, 409] {
            let store = connected_store();
            let (api, ctx) = ctx_with(store);
            api.delete.lock().unwrap().push_back(Err(http(code, "X")));
            assert_eq!(disconnect(&ctx, now()).unwrap().reason, None, "{code}");
        }
    }

    #[test]
    fn disconnect_with_nothing_stored_is_a_noop_success() {
        let (api, ctx) = ctx_with(Arc::new(MemoryStore::empty()));
        assert_eq!(
            disconnect(&ctx, now()).unwrap().state,
            HubState::NotConnected
        );
        assert!(api.calls().is_empty());
    }

    #[test]
    fn disconnect_during_save_failed_revokes_the_pending_token() {
        let store = connected_store();
        let (api, ctx) = ctx_with(store.clone());
        ctx.rt().pending = Some(
            StoredToken::from_grant(
                HUB_ORIGIN,
                &test_token('P'),
                "01JP",
                "2099-01-01T00:00:00.000Z",
                "2098-12-01T00:00:00.000Z",
            )
            .unwrap(),
        );
        let st = disconnect(&ctx, now()).unwrap();
        assert_eq!(api.calls(), vec![format!("delete:{}", test_token('P'))]);
        assert!(store.raw().is_none());
        assert_eq!(st.state, HubState::NotConnected);
    }

    #[test]
    fn disconnect_reports_keychain_delete_failure() {
        // 삭제가 실패하면 성공으로 속이지 않는다.
        struct FailingDelete(MemoryStore);
        impl store::TokenStore for FailingDelete {
            fn get_raw(&self) -> Result<Option<String>, HubAuthError> {
                self.0.get_raw()
            }
            fn set_raw(&self, v: &str) -> Result<(), HubAuthError> {
                self.0.set_raw(v)
            }
            fn delete_raw(&self) -> Result<(), HubAuthError> {
                Err(HubAuthError::Keychain(
                    crate::hub_auth::error::KeychainFailure::Denied,
                ))
            }
        }
        let inner = MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        ));
        let api = Arc::new(FakeApi::default());
        let ctx = HubCtx::new_for_test(
            Arc::new(FailingDelete(inner)),
            api,
            HUB_ORIGIN,
            lock_path_in(&tmp_dir("faildel")),
        );
        assert!(matches!(
            disconnect(&ctx, now()),
            Err(HubAuthError::Keychain(_))
        ));
    }
}
