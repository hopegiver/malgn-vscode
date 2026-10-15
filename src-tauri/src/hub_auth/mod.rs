// 허브 연결(기기 토큰) — 설계 docs/design/device-token-auth.md §7.
//
// 이 모듈의 Tauri 커맨드 4개는 전부 `HubAuthStatus`(메타데이터)만 돌려준다. 토큰 원문은
// Rust 경계(store.rs / api.rs / cli_mode.rs) 밖으로 나가지 않는다(§7.1 M4): WebView는
// 원격 데이터를 렌더하므로 XSS 하나로 `invoke`를 부를 수 있고, 커맨드가 토큰을 돌려주면
// 키체인 보호가 의미를 잃는다. 오류 문자열도 `HubAuthError`의 Display만 쓴다.
//
// 구성
//   consts.rs   허브 origin·키체인 식별자·desktop client_id(빌드타임)
//   error.rs    토큰이 들어갈 수 없는 오류 enum
//   store.rs    키체인 JSON v1, TokenStore 트레이트, keyring 구현
//   api.rs      허브 HTTP 클라이언트(발급·갱신·self·해제)
//   state.rs    상태 머신과 프론트 반환 타입
//   renew.rs    갱신 엔진·프로세스 간 락·스케줄러
//   connect.rs  PKCE 루프백 연결·해제
//   locator.rs  CLI 경로 기록

pub(crate) mod api;
pub(crate) mod connect;
pub(crate) mod consts;
pub(crate) mod error;
pub(crate) mod locator;
pub(crate) mod renew;
pub(crate) mod state;
pub(crate) mod store;

use api::{HubApi, ReqwestApi};
use chrono::{DateTime, Utc};
use error::HubAuthError;
use state::{compute_status, HubAuthStatus, Runtime};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use store::{KeyringStore, TokenStore};

/// 앱 전역 허브 인증 컨텍스트. 저장소와 서버 클라이언트는 트레이트 객체라 테스트에서
/// 메모리 저장소·가짜 서버로 바꿔 끼운다.
pub(crate) struct HubCtx {
    pub(crate) store: Arc<dyn TokenStore>,
    pub(crate) api: Arc<dyn HubApi>,
    pub(crate) origin: String,
    pub(crate) runtime: Mutex<Runtime>,
    pub(crate) lock_path: PathBuf,
}

impl HubCtx {
    /// 휘발성 상태 접근. 다른 스레드가 패닉으로 락을 독살해도 상태 조회는 계속 가능해야 한다.
    pub(crate) fn rt(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn status(&self, now: DateTime<Utc>) -> HubAuthStatus {
        let mut rt = self.rt();
        compute_status(&*self.store, &self.origin, &mut rt, now)
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        store: Arc<dyn TokenStore>,
        api: Arc<dyn HubApi>,
        origin: &str,
        lock_path: PathBuf,
    ) -> Self {
        HubCtx {
            store,
            api,
            origin: origin.to_string(),
            runtime: Mutex::new(Runtime::default()),
            lock_path,
        }
    }
}

static CTX: OnceLock<Arc<HubCtx>> = OnceLock::new();

/// 앱 시작(`setup`)에서 한 번 호출한다: 컨텍스트 생성, locator 기록, 갱신 스케줄러 기동.
/// 실패해도 앱 시작을 막지 않는다 — 커맨드가 오류를 돌려줄 뿐이다.
pub(crate) fn init(config_dir: PathBuf) {
    locator::write_current(&config_dir);
    // keyring 3은 feature가 빠지면 mock으로 조용히 폴백한다 — 그 상태로 토큰을 "저장"하면
    // 아무것도 영속되지 않으므로 기능 자체를 켜지 않는다(설계 §4 D5 가드).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if !store::default_backend_is_native() {
        return;
    }
    let origin = consts::hub_origin();
    let api = match ReqwestApi::new(&origin) {
        Ok(a) => a,
        Err(_) => return,
    };
    let ctx = Arc::new(HubCtx {
        store: Arc::new(KeyringStore::new()),
        api: Arc::new(api),
        origin,
        runtime: Mutex::new(Runtime::default()),
        lock_path: renew::lock_path_in(&config_dir),
    });
    if CTX.set(ctx.clone()).is_ok() {
        renew::spawn_scheduler(ctx);
    }
}

/// 앱 전역 컨텍스트(초기화되지 않았거나 지원하지 않는 OS면 None). 수집키 자동 연동(`otel_link`)이
/// 최신 기기 토큰을 읽고 `GET /api/devices/self`로 연결 상태를 확인하는 데 쓴다.
pub(crate) fn current_ctx() -> Option<Arc<HubCtx>> {
    CTX.get().cloned()
}

fn ctx() -> Result<Arc<HubCtx>, String> {
    CTX.get()
        .cloned()
        .ok_or_else(|| "허브 연결 기능을 초기화하지 못했습니다.".to_string())
}

/// 블로킹 작업(키체인·HTTP)을 전용 스레드에서 돌리고 결과를 기다린다. reqwest blocking
/// 클라이언트는 tokio 런타임 스레드에서 쓰면 안 되므로 `spawn_blocking`이 아니라 일반
/// 스레드를 쓴다.
pub(crate) async fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, HubAuthError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await
        .map_err(|_| HubAuthError::Flow("내부 오류: 작업이 비정상 종료되었습니다.".to_string()))
}

// ---------------- 커맨드 본체 (테스트 가능하도록 AppHandle과 분리) ----------------

pub(crate) fn status_impl(ctx: &HubCtx, now: DateTime<Utc>) -> HubAuthStatus {
    ctx.status(now)
}

pub(crate) fn renew_now_impl(ctx: &HubCtx, now: DateTime<Utc>) -> HubAuthStatus {
    renew::run_cycle(ctx, now, true);
    ctx.status(now)
}

// ---------------- Tauri 커맨드 (반환은 HubAuthStatus뿐) ----------------

#[tauri::command]
pub async fn hub_auth_status() -> Result<HubAuthStatus, String> {
    let ctx = ctx()?;
    run_blocking(move || status_impl(&ctx, Utc::now()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hub_auth_connect(app: tauri::AppHandle) -> Result<HubAuthStatus, String> {
    let ctx = ctx()?;
    let status = connect::connect(app, ctx).await.map_err(|e| e.to_string())?;
    // 새 연결로 취급해 수집키 자동 연동이 곧바로 점검한다(수집키 계약 D-7).
    crate::otel_link::on_hub_connected();
    Ok(status)
}

#[tauri::command]
pub async fn hub_auth_disconnect() -> Result<HubAuthStatus, String> {
    let ctx = ctx()?;
    let status = run_blocking(move || connect::disconnect(&ctx, Utc::now()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    // 연결 해제: 서버가 이 기기의 수집키를 함께 폐기하므로(수집키 계약 D-13) settings.json의
    // Authorization 항목도 지운다. 해제 자체가 성공한 뒤에만 한다(잠금 경합 등으로 아무 일도 안 한
    // 해제 실패에서는 키를 지우지 않는다).
    let _ = run_blocking(|| {
        if let Some(path) = crate::otel_settings::settings_path() {
            let _ = crate::otel_link::after_hub_disconnect(&path);
        }
    })
    .await;
    Ok(status)
}

/// [지금 갱신]: 갱신 창과 무관하게 서버에 물어본다(서버가 RENEW_TOO_EARLY로 교정한다).
#[tauri::command]
pub async fn hub_auth_renew_now() -> Result<HubAuthStatus, String> {
    let ctx = ctx()?;
    run_blocking(move || renew_now_impl(&ctx, Utc::now()))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::error::{HubAuthError, KeychainFailure};
    use super::renew::testing::*;
    use super::store::testing::*;
    use super::store::{StoredToken, TokenStore};
    use super::*;
    use crate::hub_auth::consts::HUB_ORIGIN;

    fn now() -> DateTime<Utc> {
        "2026-10-09T00:00:00Z".parse().unwrap()
    }

    /// 설계 §14.2 "토큰 원문 비노출(M4)": 토큰이 든 저장소로 커맨드 본체 4개를 실행하고
    /// 반환값(JSON)과 오류 문자열에 토큰 모양이 없음을 단언한다.
    #[test]
    fn no_command_return_value_contains_token_material() {
        use std::sync::Arc;
        let token = test_token('S');
        let raw = sample_json(
            HUB_ORIGIN,
            &token,
            "2026-10-20T00:00:00.000Z",
            "2026-10-01T00:00:00.000Z",
        );
        let store = Arc::new(MemoryStore::with_raw(&raw));
        let api = Arc::new(FakeApi::default());
        let ctx = HubCtx::new_for_test(
            store.clone(),
            api.clone(),
            HUB_ORIGIN,
            renew::lock_path_in(&tmp_dir("m4")),
        );

        let mut outputs: Vec<String> = Vec::new();
        let mut push = |st: &HubAuthStatus| {
            outputs.push(serde_json::to_string(st).unwrap());
            outputs.push(format!("{st:?}"));
        };

        // status
        push(&status_impl(&ctx, now()));
        // renew_now (self + renew 성공 → 새 토큰도 키체인에 들어가지만 반환에는 없다)
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('T'), "01JT")));
        push(&renew_now_impl(&ctx, now()));
        assert!(
            store.raw().unwrap().contains(&test_token('T')),
            "새 토큰은 키체인에만"
        );
        // renew_now 실패 경로
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(500, "UNKNOWN")));
        push(&renew_now_impl(&ctx, now()));
        // connect 마무리(finish_connect)
        let attempt = connect::Attempt {
            code: "c".into(),
            code_verifier: "v".into(),
            redirect_uri: "http://127.0.0.1:1/callback".into(),
        };
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('U'), "01JU")));
        api.self_.lock().unwrap().push_back(self_ok(true));
        push(&connect::finish_connect(&ctx, "cid", &attempt, now()).unwrap());
        // disconnect
        push(&connect::disconnect(&ctx, now()).unwrap());

        for o in &outputs {
            assert!(!contains_token_pattern(o), "토큰 모양 노출: {o}");
            for seed in ['S', 'T', 'U'] {
                assert!(!o.contains(&test_token(seed)[4..]), "토큰 본문 노출: {o}");
            }
        }
        assert_eq!(outputs.len(), 10);
    }

    #[test]
    fn every_error_variant_is_free_of_token_patterns() {
        let variants: Vec<HubAuthError> = vec![
            HubAuthError::NotConfigured,
            HubAuthError::Network("timeout"),
            HubAuthError::Http {
                status: 401,
                code: "UNAUTHORIZED".into(),
            },
            HubAuthError::Keychain(KeychainFailure::Denied),
            HubAuthError::Keychain(KeychainFailure::Backend("OSStatus -25293".into())),
            HubAuthError::Keychain(KeychainFailure::ReadBackMismatch),
            HubAuthError::Corrupt,
            HubAuthError::Flow("로그인 대기 시간이 초과되었습니다(5분).".into()),
            HubAuthError::BadResponse,
            HubAuthError::RenewTooEarly {
                renew_after: Some("2099-01-01T00:00:00.000Z".into()),
            },
        ];
        for v in &variants {
            for s in [v.to_string(), format!("{v:?}"), format!("{v:#?}")] {
                assert!(!contains_token_pattern(&s), "{s}");
            }
        }
    }

    #[test]
    fn errors_produced_by_failing_flows_with_a_token_present_do_not_leak_it() {
        use std::sync::Arc;
        let token = test_token('L');
        let raw = sample_json(
            HUB_ORIGIN,
            &token,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        );
        let store = Arc::new(MemoryStore::with_raw(&raw));
        let api = Arc::new(FakeApi::default());
        let ctx = HubCtx::new_for_test(
            store.clone(),
            api.clone(),
            HUB_ORIGIN,
            renew::lock_path_in(&tmp_dir("m4b")),
        );
        // 키체인 쓰기 실패·read-back 불일치·교환 실패에서 나오는 오류 문자열
        let attempt = connect::Attempt {
            code: "c".into(),
            code_verifier: "v".into(),
            redirect_uri: "http://127.0.0.1:1/callback".into(),
        };
        api.exchange
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('M'), "01JM")));
        *store.drop_writes.lock().unwrap() = true;
        let e1 = connect::finish_connect(&ctx, "cid", &attempt, now()).unwrap_err();
        *store.drop_writes.lock().unwrap() = false;
        api.exchange
            .lock()
            .unwrap()
            .push_back(Err(http(400, "invalid_grant")));
        let e2 = connect::finish_connect(&ctx, "cid", &attempt, now()).unwrap_err();
        for e in [e1, e2] {
            let s = format!("{e} {e:?}");
            assert!(
                !contains_token_pattern(&s) && !s.contains(&token[4..]),
                "{s}"
            );
        }
        // 손상된 키체인 값(토큰이 일부 들어 있는 JSON)을 읽을 때의 오류
        let store2 = MemoryStore::with_raw(&format!(
            r#"{{"v":1,"hub":"https://x.example","token":"{token}"}}"#
        ));
        let e3 = super::store::load(&store2, HUB_ORIGIN).unwrap_err();
        let s = format!("{e3} {e3:?}");
        assert!(!contains_token_pattern(&s));
    }

    #[test]
    fn stored_token_debug_masks_and_has_no_public_serialize() {
        let t = StoredToken::from_grant(
            HUB_ORIGIN,
            &test_token('D'),
            "01J",
            "2099-01-01T00:00:00.000Z",
            "2098-12-01T00:00:00.000Z",
        )
        .unwrap();
        let dbg = format!("{t:?}");
        assert!(dbg.contains("mhd_***") && !contains_token_pattern(&dbg));
    }

    // ---- 설계 §14.2 "SAVE_FAILED에서만 메모리 토큰을 쓴다" ----
    #[test]
    fn memory_token_exists_only_in_save_failed_state() {
        use std::sync::Arc;
        let store = Arc::new(MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2026-10-20T00:00:00.000Z",
            "2026-10-01T00:00:00.000Z",
        )));
        let api = Arc::new(FakeApi::default());
        let ctx = HubCtx::new_for_test(
            store.clone(),
            api.clone(),
            HUB_ORIGIN,
            renew::lock_path_in(&tmp_dir("sf")),
        );
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('N'), "01JN")));
        *store.drop_writes.lock().unwrap() = true;
        let st = renew_now_impl(&ctx, now());
        assert_eq!(st.state, state::HubState::SaveFailed);
        assert!(ctx.rt().pending.is_some());
        // 정상 복구 후에는 메모리 토큰이 비워진다
        *store.drop_writes.lock().unwrap() = false;
        renew::run_cycle(&ctx, now(), false);
        assert!(ctx.rt().pending.is_none());
        let _ = store.get_raw();
    }
}
