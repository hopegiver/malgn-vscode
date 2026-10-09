// 갱신 엔진 (설계 §3.3, §7.2, §10).
//
// 한 사이클(`run_cycle`):
//   1. SAVE_FAILED면 메모리의 새 토큰을 키체인에 다시 쓴다(성공하면 끝).
//   2. 키체인을 **다시 읽는다**(캐시 없음, §7.2 M1).
//   3. `GET /self`로 서버가 토큰을 아직 인정하는지 확인한다. 401·409면 그때 항목을
//      지운다 — "401을 받으면 지우기 전에 self로 한 번 확인"(§5.0 전제)은 self가 첫
//      호출이라 자연히 지켜지고, renew가 401을 받은 경우에는 항목을 지우기 전에 self를
//      따로 한 번 더 부른다(`after_renew_unauthorized`).
//   4. 갱신 창이면 프로세스 간 락을 잡고, 키체인을 다시 읽어 아직 갱신 대상인지 확인한 뒤
//      `POST /renew`를 호출한다. 성공하면 키체인에 쓰고 read-back으로 확인한다.
// CLI 모드는 이 모듈을 쓰지 않는다(CLI는 갱신하지 않는다, §7.2).

use super::api::SelfInfo;
use super::error::HubAuthError;
use super::state::HubState;
use super::store::{self, StoredToken};
use super::HubCtx;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::path::{Path, PathBuf};

/// 시작 시 + 6시간 주기(§3.3).
pub(crate) const CYCLE_INTERVAL_SECS: u64 = 6 * 3600;
/// 오프라인·5xx 재시도 간격(§5.2 오류표, §7.2).
pub(crate) const RETRY_INTERVAL_SECS: u64 = 3600;
/// SAVE_FAILED 저장 재시도 간격이자 스케줄러 tick(§7.2).
pub(crate) const SAVE_RETRY_INTERVAL_SECS: u64 = 60;
/// 락 stale 판정(§7.2).
pub(crate) const LOCK_STALE_SECS: i64 = 120;

/// 갱신 대상 판정. `renew_after`와 같은 순간부터 대상이다(경계 포함).
pub(crate) fn needs_renew(now: DateTime<Utc>, renew_after: DateTime<Utc>) -> bool {
    now >= renew_after
}

// ---------------- 프로세스 간 락 ----------------

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockError {
    /// 다른 인스턴스가 쥐고 있다(stale 아님).
    Held,
    Io(String),
}

pub(crate) struct RenewLock {
    path: PathBuf,
}

impl Drop for RenewLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn lock_path_in(dir: &Path) -> PathBuf {
    dir.join("hub-renew.lock")
}

fn read_lock_timestamp(path: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().nth(1)?.trim().parse::<i64>().ok()
}

/// `create_new`로 락 파일을 만든다. 내용은 pid와 시각(초). `LOCK_STALE_SECS`를 넘긴
/// 락은 회수한다(앱이 갱신 도중 죽은 경우). 회수 경쟁(두 프로세스가 동시에 stale을
/// 지우고 만드는 경우)은 락 획득 뒤 키체인 재읽기가 2차로 막는다.
pub(crate) fn acquire_lock(path: &Path, now_secs: i64) -> Result<RenewLock, LockError> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| LockError::Io(e.to_string()))?;
    }
    for attempt in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut f) => {
                let _ = write!(f, "{}\n{}\n", std::process::id(), now_secs);
                return Ok(RenewLock {
                    path: path.to_path_buf(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // 시각을 읽을 수 없으면(내용 손상·쓰는 중) 파일 수정 시각으로 대신한다.
                let ts = read_lock_timestamp(path).or_else(|| {
                    std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                });
                let stale = ts.map(|t| now_secs - t > LOCK_STALE_SECS).unwrap_or(true);
                if stale && attempt == 0 {
                    let _ = std::fs::remove_file(path);
                    continue;
                }
                return Err(LockError::Held);
            }
            Err(e) => return Err(LockError::Io(e.to_string())),
        }
    }
    Err(LockError::Held)
}

// ---------------- 사이클 ----------------

fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// 서버가 토큰을 더는 인정하지 않을 때: 항목을 지우고 사유를 남긴다.
fn drop_entry(ctx: &HubCtx, state: HubState, reason: &'static str) {
    let _ = ctx.store.delete_raw();
    let mut rt = ctx.rt();
    rt.terminal = Some((state, reason));
    rt.retrying = false;
}

/// self 호출이 실패했을 때의 공통 처리(401·409면 폐기 확정, 그 밖에는 재시도 상태).
fn handle_auth_error(ctx: &HubCtx, err: &HubAuthError) {
    match err {
        HubAuthError::Http { status: 401, .. } => {
            drop_entry(ctx, HubState::Revoked, "UNAUTHORIZED")
        }
        HubAuthError::Http { status: 409, code } if code == "ALREADY_RENEWED" => {
            drop_entry(ctx, HubState::Revoked, "ALREADY_RENEWED")
        }
        _ => ctx.rt().retrying = true,
    }
}

/// renew가 401이었을 때: 지우기 전에 같은 토큰으로 self를 한 번 불러 서버가 재사용을
/// 판정할 기회를 준다(§5.0 앱 쪽 전제, §7.2).
fn after_renew_unauthorized(ctx: &HubCtx, token: &str) {
    match ctx.api.self_info(token) {
        // self는 통과하는데 renew만 401인 경우는 정상 서버에서는 없다. 지우지 않고 재시도.
        Ok(_) => ctx.rt().retrying = true,
        Err(e) => handle_auth_error(ctx, &e),
    }
}

fn apply_self_info(ctx: &HubCtx, info: &SelfInfo) {
    let mut rt = ctx.rt();
    if !info.device_name.is_empty() {
        rt.device_name = Some(info.device_name.clone());
    }
    rt.retrying = false;
}

/// 갱신 한 번 시도. 락을 잡고 키체인을 다시 읽은 뒤 서버를 호출한다.
fn renew_under_lock(ctx: &HubCtx, now: DateTime<Utc>, force: bool, device_token_id: &str) {
    let _guard = match acquire_lock(&ctx.lock_path, now.timestamp()) {
        Ok(g) => g,
        // 다른 인스턴스가 갱신 중이다. 그쪽 결과를 다음 사이클에 키체인에서 읽는다.
        Err(_) => return,
    };
    // 락을 잡은 뒤 키체인을 다시 읽는다 — 다른 인스턴스가 이미 갱신했을 수 있다(§3.3 3).
    let fresh = match store::load(&*ctx.store, &ctx.origin) {
        Ok(Some(t)) => t,
        _ => return,
    };
    if fresh.device_token_id != device_token_id {
        return;
    }
    let renew_after = ctx.rt().effective_renew_after(&fresh);
    if !force && !needs_renew(now, renew_after) {
        return;
    }

    match ctx.api.renew(fresh.expose_token()) {
        Ok(issued) => {
            let new = match StoredToken::from_grant(
                &ctx.origin,
                issued.expose_token(),
                &issued.device_token_id,
                &issued.expires_at,
                &issued.renew_after,
            ) {
                Ok(t) => t,
                Err(_) => {
                    // 서버는 이미 교체를 확정했는데 응답을 해석하지 못했다. 새 토큰을 잃었으므로
                    // 다음 시도는 ALREADY_RENEWED가 되어 재연결로 이어진다(§10 "응답 유실"과 같은 경우).
                    ctx.rt().retrying = true;
                    return;
                }
            };
            match store::save_verified(&*ctx.store, &new) {
                Ok(()) => {
                    let mut rt = ctx.rt();
                    rt.pending = None;
                    rt.retrying = false;
                    rt.renew_after_override = None;
                    rt.terminal = None;
                }
                Err(_) => {
                    // SAVE_FAILED: 새 토큰을 메모리에 들고 1분마다 저장을 재시도한다.
                    ctx.rt().pending = Some(new);
                }
            }
        }
        Err(HubAuthError::RenewTooEarly { renew_after }) => {
            let at = renew_after
                .as_deref()
                .and_then(parse_ts)
                .unwrap_or_else(|| now + ChronoDuration::hours(1));
            let mut rt = ctx.rt();
            rt.renew_after_override = Some(at);
            rt.retrying = false;
        }
        Err(HubAuthError::Http { status: 401, .. }) => {
            after_renew_unauthorized(ctx, fresh.expose_token())
        }
        Err(HubAuthError::Http { status: 409, code }) if code == "ALREADY_RENEWED" => {
            drop_entry(ctx, HubState::Revoked, "ALREADY_RENEWED")
        }
        Err(HubAuthError::Http { status: 403, code }) if code == "NOT_RENEWABLE" => {
            // desktop 발급분이 아닌 토큰. 만료 전까지는 쓸 수 있으므로 항목은 두고 재연결을 안내한다.
            ctx.rt().terminal = Some((HubState::Revoked, "NOT_RENEWABLE"));
        }
        // 429, 5xx, 네트워크 등: 기존 토큰을 유지하고 1시간 뒤 재시도한다.
        Err(_) => ctx.rt().retrying = true,
    }
}

/// 사이클 한 번. `force`는 [지금 갱신]에서 갱신 창과 무관하게 서버에 물어볼 때 쓴다
/// (서버가 TOO_EARLY로 교정해 준다).
pub(crate) fn run_cycle(ctx: &HubCtx, now: DateTime<Utc>, force: bool) {
    // 1) SAVE_FAILED: 저장하지 못한 새 토큰이 있으면 그것부터 저장한다.
    let pending = ctx.rt().pending.clone();
    if let Some(p) = pending {
        if store::save_verified(&*ctx.store, &p).is_ok() {
            let mut rt = ctx.rt();
            rt.pending = None;
            rt.retrying = false;
            rt.renew_after_override = None;
            rt.terminal = None;
        }
        return;
    }

    // 2) 키체인 재읽기
    let stored = match store::load(&*ctx.store, &ctx.origin) {
        Ok(Some(t)) => t,
        Ok(None) => return,
        // 접근 거부·백엔드 오류·손상은 상태 계산(compute_status)이 키체인을 다시 읽어
        // KEYCHAIN_DENIED / CORRUPT로 보여 준다. 여기서는 서버를 부르지 않고 끝낸다.
        Err(_) => return,
    };
    if stored.expires_at <= now {
        return; // EXPIRED — 상태 계산이 보여 준다. 재연결만 가능.
    }

    // 3) self
    match ctx.api.self_info(stored.expose_token()) {
        Ok(info) => {
            apply_self_info(ctx, &info);
            if !info.renewable {
                return;
            }
        }
        Err(e) => {
            handle_auth_error(ctx, &e);
            return;
        }
    }

    // 4) 갱신
    let renew_after = ctx.rt().effective_renew_after(&stored);
    if force || needs_renew(now, renew_after) {
        renew_under_lock(ctx, now, force, &stored.device_token_id);
    }
}

// ---------------- 스케줄러 ----------------

/// 시작 시 한 번, 이후 6시간마다(재시도 중이면 1시간마다, SAVE_FAILED면 1분마다) 사이클을
/// 돈다. 앱이 켜져 있는 동안만 돈다. 전용 스레드라 네트워크가 느려도 UI를 막지 않는다.
pub(crate) fn spawn_scheduler(ctx: std::sync::Arc<HubCtx>) {
    std::thread::spawn(move || {
        use std::time::{Duration, Instant};
        let mut next_cycle = Instant::now();
        loop {
            let has_pending = ctx.rt().pending.is_some();
            if has_pending || Instant::now() >= next_cycle {
                run_cycle(&ctx, Utc::now(), false);
                let (pending, retrying) = {
                    let rt = ctx.rt();
                    (rt.pending.is_some(), rt.retrying)
                };
                let wait = if pending {
                    SAVE_RETRY_INTERVAL_SECS
                } else if retrying {
                    RETRY_INTERVAL_SECS
                } else {
                    CYCLE_INTERVAL_SECS
                };
                next_cycle = Instant::now() + Duration::from_secs(wait);
            }
            std::thread::sleep(Duration::from_secs(SAVE_RETRY_INTERVAL_SECS));
        }
    });
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::hub_auth::api::{CodeExchange, HubApi, IssuedToken};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// 호출 순서와 인자를 기록하고, 미리 넣어 둔 결과를 순서대로 돌려주는 가짜 서버.
    #[derive(Default)]
    pub(crate) struct FakeApi {
        pub(crate) exchange: Mutex<VecDeque<Result<IssuedToken, HubAuthError>>>,
        pub(crate) renew: Mutex<VecDeque<Result<IssuedToken, HubAuthError>>>,
        pub(crate) self_: Mutex<VecDeque<Result<SelfInfo, HubAuthError>>>,
        pub(crate) delete: Mutex<VecDeque<Result<(), HubAuthError>>>,
        /// "renew:<token>", "self:<token>", "delete:<token>", "exchange"
        pub(crate) calls: Mutex<Vec<String>>,
    }

    impl FakeApi {
        pub(crate) fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl HubApi for FakeApi {
        fn exchange_code(&self, _req: &CodeExchange<'_>) -> Result<IssuedToken, HubAuthError> {
            self.calls.lock().unwrap().push("exchange".into());
            self.exchange
                .lock()
                .unwrap()
                .pop_front()
                .expect("exchange 응답이 준비되지 않았다")
        }
        fn renew(&self, token: &str) -> Result<IssuedToken, HubAuthError> {
            self.calls.lock().unwrap().push(format!("renew:{token}"));
            self.renew
                .lock()
                .unwrap()
                .pop_front()
                .expect("renew 응답이 준비되지 않았다")
        }
        fn self_info(&self, token: &str) -> Result<SelfInfo, HubAuthError> {
            self.calls.lock().unwrap().push(format!("self:{token}"));
            self.self_
                .lock()
                .unwrap()
                .pop_front()
                .expect("self 응답이 준비되지 않았다")
        }
        fn delete_self(&self, token: &str) -> Result<(), HubAuthError> {
            self.calls.lock().unwrap().push(format!("delete:{token}"));
            self.delete.lock().unwrap().pop_front().unwrap_or(Ok(()))
        }
    }

    pub(crate) fn self_ok(renewable: bool) -> Result<SelfInfo, HubAuthError> {
        Ok(SelfInfo {
            device_token_id: "01JSELF".into(),
            device_name: "test-mac".into(),
            expires_at: "2099-01-01T00:00:00.000Z".into(),
            renew_after: "2098-12-01T00:00:00.000Z".into(),
            renewable,
        })
    }

    pub(crate) fn issued(token: &str, id: &str) -> IssuedToken {
        crate::hub_auth::api::parse_issued(
            &serde_json::json!({
                "device_token": token, "device_token_id": id,
                "expires_at": "2099-06-01T00:00:00.000Z", "renew_after": "2099-05-01T00:00:00.000Z"
            })
            .to_string(),
            false,
        )
        .unwrap()
    }

    pub(crate) fn http(status: u16, code: &str) -> HubAuthError {
        HubAuthError::Http {
            status,
            code: code.into(),
        }
    }

    pub(crate) fn tmp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "malgn-hub-test-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// `mhd_` 뒤에 영숫자·`-`·`_`가 4자 이상 이어지는 토큰 모양이 있는지(설계 §14.2 패턴).
    pub(crate) fn contains_token_pattern(s: &str) -> bool {
        let b = s.as_bytes();
        let mut i = 0;
        while let Some(pos) = s[i..].find("mhd_") {
            let start = i + pos + 4;
            let n = b[start..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == b'-' || **c == b'_')
                .count();
            if n >= 4 {
                return true;
            }
            i = start;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::hub_auth::api::HubApi;
    use crate::hub_auth::consts::HUB_ORIGIN;
    use crate::hub_auth::store::testing::*;
    use chrono::TimeZone;
    use std::sync::Arc;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 9, 0, 0, 0).unwrap()
    }

    /// 갱신 창에 든 항목을 가진 컨텍스트.
    fn due_ctx() -> (Arc<MemoryStore>, Arc<FakeApi>, HubCtx) {
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
            lock_path_in(&tmp_dir("ctx")),
        );
        (store, api, ctx)
    }

    fn fresh_ctx() -> (Arc<MemoryStore>, Arc<FakeApi>, HubCtx) {
        let (store, api, ctx) = due_ctx();
        *store.value.lock().unwrap() = Some(sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        ));
        (store, api, ctx)
    }

    // ---- needs_renew 경계 ----
    #[test]
    fn needs_renew_boundaries() {
        let ra = now();
        assert!(!needs_renew(ra - ChronoDuration::seconds(1), ra), "직전");
        assert!(needs_renew(ra, ra), "같음");
        assert!(needs_renew(ra + ChronoDuration::seconds(1), ra), "직후");
    }

    // ---- 락 ----
    #[test]
    fn lock_second_acquire_fails_then_succeeds_after_release() {
        let p = lock_path_in(&tmp_dir("lock1"));
        let g = acquire_lock(&p, 1_000).unwrap();
        assert_eq!(acquire_lock(&p, 1_010).err(), Some(LockError::Held));
        drop(g);
        assert!(acquire_lock(&p, 1_020).is_ok());
    }

    #[test]
    fn stale_lock_older_than_120s_is_reclaimed_but_exactly_120s_is_not() {
        let p = lock_path_in(&tmp_dir("lock2"));
        let g = acquire_lock(&p, 1_000).unwrap();
        std::mem::forget(g); // 앱이 죽어 락이 남은 상황
        assert_eq!(
            acquire_lock(&p, 1_000 + 120).err(),
            Some(LockError::Held),
            "정확히 120초는 아직 stale 아님"
        );
        assert!(acquire_lock(&p, 1_000 + 121).is_ok(), "120초 초과는 회수");
    }

    #[test]
    fn lock_with_garbage_content_falls_back_to_mtime() {
        let p = lock_path_in(&tmp_dir("lock3"));
        std::fs::write(&p, "garbage").unwrap();
        let now_real = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(
            acquire_lock(&p, now_real).err(),
            Some(LockError::Held),
            "방금 만든 파일은 stale 아님"
        );
        assert!(acquire_lock(&p, now_real + 1000).is_ok());
    }

    // ---- 사이클 ----
    #[test]
    fn happy_path_renews_and_replaces_keychain_value_with_readback() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('N'), "01JNEWID")));
        run_cycle(&ctx, now(), false);
        assert_eq!(
            api.calls(),
            vec![format!("self:{TEST_TOKEN}"), format!("renew:{TEST_TOKEN}")]
        );
        let saved = store.raw().unwrap();
        assert!(
            saved.contains(&test_token('N'))
                && saved.contains("01JNEWID")
                && !saved.contains(&TEST_TOKEN[4..])
        );
        assert!(!ctx.lock_path.exists(), "락이 해제돼야 한다");
        assert!(ctx.rt().pending.is_none());
    }

    #[test]
    fn not_due_calls_self_only() {
        let (_store, api, ctx) = fresh_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        run_cycle(&ctx, now(), false);
        assert_eq!(api.calls(), vec![format!("self:{TEST_TOKEN}")]);
    }

    #[test]
    fn force_renews_even_when_not_due_and_too_early_reschedules() {
        let (_store, api, ctx) = fresh_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Err(HubAuthError::RenewTooEarly {
                renew_after: Some("2027-09-10T00:00:00.000Z".into()),
            }));
        run_cycle(&ctx, now(), true);
        assert_eq!(api.calls().len(), 2);
        assert_eq!(
            ctx.rt().renew_after_override,
            Some(Utc.with_ymd_and_hms(2027, 9, 10, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn too_early_override_pushes_next_renewal_out() {
        let (_store, api, ctx) = due_ctx();
        ctx.rt().renew_after_override = Some(now() + ChronoDuration::days(5));
        api.self_.lock().unwrap().push_back(self_ok(true));
        run_cycle(&ctx, now(), false);
        assert_eq!(
            api.calls().len(),
            1,
            "override가 미래면 renew를 부르지 않는다"
        );
    }

    #[test]
    fn self_401_deletes_entry_and_marks_revoked() {
        let (store, api, ctx) = due_ctx();
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(401, "UNAUTHORIZED")));
        run_cycle(&ctx, now(), false);
        assert_eq!(
            api.calls(),
            vec![format!("self:{TEST_TOKEN}")],
            "renew는 부르지 않는다"
        );
        assert!(store.raw().is_none());
        assert_eq!(ctx.rt().terminal, Some((HubState::Revoked, "UNAUTHORIZED")));
    }

    #[test]
    fn self_409_already_renewed_deletes_entry() {
        let (store, api, ctx) = due_ctx();
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(409, "ALREADY_RENEWED")));
        run_cycle(&ctx, now(), false);
        assert!(store.raw().is_none());
        assert_eq!(
            ctx.rt().terminal,
            Some((HubState::Revoked, "ALREADY_RENEWED"))
        );
    }

    #[test]
    fn renew_401_calls_self_before_deleting_the_entry() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Err(http(401, "UNAUTHORIZED")));
        // 확인용 self: 서버가 재사용으로 판정해 409를 준다
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(409, "ALREADY_RENEWED")));
        run_cycle(&ctx, now(), false);
        let calls = api.calls();
        assert_eq!(calls.len(), 3);
        assert!(
            calls[0].starts_with("self:")
                && calls[1].starts_with("renew:")
                && calls[2].starts_with("self:")
        );
        assert!(store.raw().is_none());
        assert_eq!(
            ctx.rt().terminal,
            Some((HubState::Revoked, "ALREADY_RENEWED"))
        );
    }

    #[test]
    fn renew_401_with_self_401_marks_unauthorized() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Err(http(401, "UNAUTHORIZED")));
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(http(401, "UNAUTHORIZED")));
        run_cycle(&ctx, now(), false);
        assert!(store.raw().is_none());
        assert_eq!(ctx.rt().terminal, Some((HubState::Revoked, "UNAUTHORIZED")));
    }

    #[test]
    fn renew_409_already_renewed_deletes_entry() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Err(http(409, "ALREADY_RENEWED")));
        run_cycle(&ctx, now(), false);
        assert!(store.raw().is_none());
        assert_eq!(
            ctx.rt().terminal,
            Some((HubState::Revoked, "ALREADY_RENEWED"))
        );
    }

    #[test]
    fn renew_403_not_renewable_keeps_entry_but_flags_reconnect() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Err(http(403, "NOT_RENEWABLE")));
        run_cycle(&ctx, now(), false);
        assert!(store.raw().is_some());
        assert_eq!(
            ctx.rt().terminal,
            Some((HubState::Revoked, "NOT_RENEWABLE"))
        );
    }

    #[test]
    fn network_and_5xx_keep_entry_and_mark_retrying() {
        for err in [
            HubAuthError::Network("timeout"),
            http(503, "UNKNOWN"),
            http(429, "RATE_LIMITED"),
        ] {
            let (store, api, ctx) = due_ctx();
            api.self_.lock().unwrap().push_back(self_ok(true));
            api.renew.lock().unwrap().push_back(Err(err));
            run_cycle(&ctx, now(), false);
            assert!(store.raw().unwrap().contains(TEST_TOKEN), "기존 토큰 유지");
            assert!(ctx.rt().retrying);
        }
        let (store, api, ctx) = due_ctx();
        api.self_
            .lock()
            .unwrap()
            .push_back(Err(HubAuthError::Network("connect")));
        run_cycle(&ctx, now(), false);
        assert!(store.raw().is_some() && ctx.rt().retrying);
        assert_eq!(api.calls().len(), 1, "self 실패 시 renew는 시도하지 않는다");
    }

    #[test]
    fn not_renewable_token_is_never_renewed() {
        let (_s, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(false));
        run_cycle(&ctx, now(), false);
        assert_eq!(api.calls().len(), 1);
    }

    #[test]
    fn save_failure_after_renew_keeps_new_token_in_memory_and_retries_save() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('N'), "01JNEWID")));
        *store.drop_writes.lock().unwrap() = true; // 쓰기는 성공처럼 보이지만 read-back이 어긋난다
        run_cycle(&ctx, now(), false);
        assert!(ctx.rt().pending.is_some(), "SAVE_FAILED");
        assert!(
            store.raw().unwrap().contains(TEST_TOKEN),
            "키체인에는 아직 직전 토큰"
        );

        // 다음 사이클: 서버를 부르지 않고 저장만 재시도한다
        let calls_before = api.calls().len();
        run_cycle(&ctx, now(), false);
        assert_eq!(api.calls().len(), calls_before);
        assert!(ctx.rt().pending.is_some(), "여전히 실패");

        *store.drop_writes.lock().unwrap() = false;
        run_cycle(&ctx, now(), false);
        assert!(ctx.rt().pending.is_none());
        assert!(store.raw().unwrap().contains(&test_token('N')));
        assert_eq!(
            api.calls().len(),
            calls_before,
            "저장 재시도는 서버 호출이 없다"
        );
    }

    #[test]
    fn every_server_call_is_preceded_by_a_fresh_keychain_read() {
        let (store, api, ctx) = due_ctx();
        api.self_.lock().unwrap().push_back(self_ok(true));
        api.renew
            .lock()
            .unwrap()
            .push_back(Ok(issued(&test_token('N'), "01JNEWID")));
        run_cycle(&ctx, now(), false);
        // 사이클 시작 1회 + 락 획득 후 재확인 1회 + read-back 1회
        assert!(store.get_count() >= 3, "get_count={}", store.get_count());

        // 키체인 값이 바뀌면 다음 사이클은 바뀐 값을 쓴다(메모리 캐시 없음)
        let other = test_token('Q');
        *store.value.lock().unwrap() = Some(sample_json(
            HUB_ORIGIN,
            &other,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        ));
        api.self_.lock().unwrap().push_back(self_ok(true));
        run_cycle(&ctx, now(), false);
        assert_eq!(api.calls().last().unwrap(), &format!("self:{other}"));
    }

    #[test]
    fn token_changed_by_another_instance_before_lock_skips_renew() {
        // self와 락 사이에 다른 인스턴스가 이미 갱신한 상황: self 응답 직후 키체인이 바뀐다.
        struct SwapApi {
            inner: Arc<FakeApi>,
            store: Arc<MemoryStore>,
        }
        impl HubApi for SwapApi {
            fn exchange_code(
                &self,
                r: &crate::hub_auth::api::CodeExchange<'_>,
            ) -> Result<crate::hub_auth::api::IssuedToken, HubAuthError> {
                self.inner.exchange_code(r)
            }
            fn renew(&self, t: &str) -> Result<crate::hub_auth::api::IssuedToken, HubAuthError> {
                self.inner.renew(t)
            }
            fn self_info(&self, t: &str) -> Result<SelfInfo, HubAuthError> {
                let r = self.inner.self_info(t);
                let mut json: serde_json::Value =
                    serde_json::from_str(&self.store.raw().unwrap()).unwrap();
                json["device_token_id"] = "01JOTHER".into();
                json["renew_after"] = "2027-09-09T00:00:00.000Z".into();
                *self.store.value.lock().unwrap() = Some(json.to_string());
                r
            }
            fn delete_self(&self, t: &str) -> Result<(), HubAuthError> {
                self.inner.delete_self(t)
            }
        }
        let store = Arc::new(MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2026-10-20T00:00:00.000Z",
            "2026-10-01T00:00:00.000Z",
        )));
        let inner = Arc::new(FakeApi::default());
        inner.self_.lock().unwrap().push_back(self_ok(true));
        let api = Arc::new(SwapApi {
            inner: inner.clone(),
            store: store.clone(),
        });
        let ctx = HubCtx::new_for_test(
            store.clone(),
            api,
            HUB_ORIGIN,
            lock_path_in(&tmp_dir("swap")),
        );
        run_cycle(&ctx, now(), false);
        assert_eq!(
            inner.calls().len(),
            1,
            "다른 인스턴스가 갱신했으면 renew를 부르지 않는다"
        );
    }

    #[test]
    fn held_lock_skips_renew_without_touching_server() {
        let (_store, api, ctx) = due_ctx();
        let _held = acquire_lock(&ctx.lock_path, now().timestamp()).unwrap();
        api.self_.lock().unwrap().push_back(self_ok(true));
        run_cycle(&ctx, now(), false);
        assert_eq!(api.calls().len(), 1, "self만 호출");
    }

    #[test]
    fn expired_local_entry_makes_no_server_call() {
        let (store, api, ctx) = due_ctx();
        *store.value.lock().unwrap() = Some(sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2026-10-08T00:00:00.000Z",
            "2026-09-08T00:00:00.000Z",
        ));
        run_cycle(&ctx, now(), false);
        assert!(api.calls().is_empty());
    }

    #[test]
    fn keychain_denied_makes_no_server_call() {
        let (store, api, ctx) = due_ctx();
        *store.fail_get.lock().unwrap() = Some(HubAuthError::Keychain(
            crate::hub_auth::error::KeychainFailure::Denied,
        ));
        run_cycle(&ctx, now(), false);
        assert!(api.calls().is_empty());
    }
}
