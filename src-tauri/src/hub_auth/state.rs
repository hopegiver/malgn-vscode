// 상태 머신과 프론트에 보내는 상태 타입 (설계 §7.1 반환 계약, §7.2).
//
// `HubAuthStatus`가 Tauri 커맨드가 프론트로 돌려주는 유일한 데이터다. 메타데이터만 담고
// 토큰 필드가 없다(M4). 이 타입에 토큰을 담는 필드를 추가하지 말 것 — 테스트
// `status_serializes_without_token_material`가 직렬화 결과를 검사한다.

use super::error::{HubAuthError, KeychainFailure};
use super::store::{self, StoredToken, TokenStore};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum HubState {
    NotConnected,
    Connected,
    RenewDue,
    RenewRetrying,
    SaveFailed,
    Expired,
    Revoked,
    KeychainDenied,
}

/// 프론트로 나가는 상태. 토큰 필드 없음.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct HubAuthStatus {
    pub(crate) connected: bool,
    pub(crate) state: HubState,
    pub(crate) device_name: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) renew_after: Option<String>,
    /// 사유·안내 코드(예: `ALREADY_RENEWED`, `UNAUTHORIZED`, `SERVER_REVOKE_FAILED`).
    /// 문구는 프론트가 코드로 고른다. 서버 본문 텍스트는 담지 않는다.
    pub(crate) reason: Option<String>,
}

/// 앱 실행 중에만 유지하는 휘발성 상태. 키체인이 정본이고, 여기에는 키체인에 없는 것만 둔다.
#[derive(Default)]
pub(crate) struct Runtime {
    /// 항목이 지워졌거나(401, 재사용) 쓸 수 없게 된 사유. 앱을 껐다 켜면 사라진다.
    pub(crate) terminal: Option<(HubState, &'static str)>,
    pub(crate) device_name: Option<String>,
    /// SAVE_FAILED: 갱신은 성공했지만 키체인에 쓰지 못한 새 토큰(메모리에만 존재).
    pub(crate) pending: Option<StoredToken>,
    pub(crate) retrying: bool,
    /// 서버가 RENEW_TOO_EARLY로 알려 준 갱신 가능 시각(로컬 시계 오차 교정, §10).
    pub(crate) renew_after_override: Option<DateTime<Utc>>,
    /// 일회성 안내 코드(연결 해제 시 서버 폐기 실패 등). 다음 상태 계산 때 한 번 노출된다.
    pub(crate) notice: Option<&'static str>,
}

impl Runtime {
    pub(crate) fn effective_renew_after(&self, stored: &StoredToken) -> DateTime<Utc> {
        match self.renew_after_override {
            Some(o) if o > stored.renew_after => o,
            _ => stored.renew_after,
        }
    }
}

fn iso(d: &DateTime<Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn status_for(
    state: HubState,
    token: Option<&StoredToken>,
    rt: &Runtime,
    reason: Option<&str>,
) -> HubAuthStatus {
    let connected = matches!(
        state,
        HubState::Connected | HubState::RenewDue | HubState::RenewRetrying | HubState::SaveFailed
    );
    HubAuthStatus {
        connected,
        state,
        device_name: if token.is_some() {
            rt.device_name.clone()
        } else {
            None
        },
        expires_at: token.map(|t| iso(&t.expires_at)),
        renew_after: token.map(|t| iso(&rt.effective_renew_after(t))),
        reason: reason.map(|s| s.to_string()),
    }
}

/// 현재 상태를 계산한다. 키체인을 매번 다시 읽는다(캐시 금지, §7.2 M1). 유일한 예외는
/// SAVE_FAILED의 메모리 토큰이다.
pub(crate) fn compute_status(
    store: &dyn TokenStore,
    origin: &str,
    rt: &mut Runtime,
    now: DateTime<Utc>,
) -> HubAuthStatus {
    let notice = rt.notice.take();
    if let Some(pending) = rt.pending.clone() {
        return status_for(HubState::SaveFailed, Some(&pending), rt, notice);
    }
    let loaded = store::load(store, origin);
    match loaded {
        Err(HubAuthError::Keychain(KeychainFailure::Denied)) => {
            status_for(HubState::KeychainDenied, None, rt, Some("DENIED"))
        }
        Err(HubAuthError::Corrupt) => status_for(HubState::Revoked, None, rt, Some("CORRUPT")),
        Err(_) => status_for(HubState::KeychainDenied, None, rt, Some("KEYCHAIN_ERROR")),
        Ok(None) => match rt.terminal {
            Some((state, reason)) => status_for(state, None, rt, Some(reason)),
            None => status_for(HubState::NotConnected, None, rt, notice),
        },
        Ok(Some(t)) => {
            if let Some((state, reason)) = rt.terminal {
                // 항목이 남아 있는 종결 사유(NOT_RENEWABLE)만 유효하다. 그 밖의 오래된
                // 종결 사유는 다른 인스턴스가 새로 연결한 경우이므로 무시한다.
                if reason == "NOT_RENEWABLE" {
                    return status_for(state, Some(&t), rt, Some(reason));
                }
            }
            if t.expires_at <= now {
                status_for(HubState::Expired, Some(&t), rt, Some("EXPIRED"))
            } else if rt.retrying {
                status_for(HubState::RenewRetrying, Some(&t), rt, notice)
            } else if now >= rt.effective_renew_after(&t) {
                status_for(HubState::RenewDue, Some(&t), rt, notice)
            } else {
                status_for(HubState::Connected, Some(&t), rt, notice)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_auth::consts::HUB_ORIGIN;
    use crate::hub_auth::store::testing::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 9, 0, 0, 0).unwrap()
    }

    fn store_with(expires: &str, renew: &str) -> MemoryStore {
        MemoryStore::with_raw(&sample_json(HUB_ORIGIN, TEST_TOKEN, expires, renew))
    }

    #[test]
    fn empty_store_is_not_connected() {
        let s = MemoryStore::empty();
        let st = compute_status(&s, HUB_ORIGIN, &mut Runtime::default(), now());
        assert_eq!(st.state, HubState::NotConnected);
        assert!(!st.connected);
    }

    #[test]
    fn connected_renew_due_and_expired_by_dates() {
        let mut rt = Runtime::default();
        let s = store_with("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut rt, now()).state,
            HubState::Connected
        );
        let s = store_with("2026-10-20T00:00:00.000Z", "2026-10-09T00:00:00.000Z");
        // renew_after == now 경계는 갱신 대상
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut rt, now()).state,
            HubState::RenewDue
        );
        let s = store_with("2026-10-09T00:00:00.000Z", "2026-09-09T00:00:00.000Z");
        let st = compute_status(&s, HUB_ORIGIN, &mut rt, now());
        assert_eq!(st.state, HubState::Expired);
        assert!(!st.connected);
    }

    #[test]
    fn retrying_and_override_and_pending() {
        let s = store_with("2026-10-20T00:00:00.000Z", "2026-10-01T00:00:00.000Z");
        let mut rt = Runtime {
            retrying: true,
            ..Default::default()
        };
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut rt, now()).state,
            HubState::RenewRetrying
        );
        rt.retrying = false;
        rt.renew_after_override = Some(Utc.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).unwrap());
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut rt, now()).state,
            HubState::Connected
        );
        rt.pending = Some(
            StoredToken::from_grant(
                HUB_ORIGIN,
                &test_token('P'),
                "01JP",
                "2099-01-01T00:00:00.000Z",
                "2098-12-01T00:00:00.000Z",
            )
            .unwrap(),
        );
        let st = compute_status(&s, HUB_ORIGIN, &mut rt, now());
        assert_eq!(st.state, HubState::SaveFailed);
        assert!(
            st.connected,
            "저장 실패여도 메모리 토큰으로 서버 연결은 유효"
        );
    }

    #[test]
    fn terminal_reason_is_shown_while_store_is_empty() {
        let s = MemoryStore::empty();
        let mut rt = Runtime {
            terminal: Some((HubState::Revoked, "ALREADY_RENEWED")),
            ..Default::default()
        };
        let st = compute_status(&s, HUB_ORIGIN, &mut rt, now());
        assert_eq!(
            (st.state, st.reason.as_deref()),
            (HubState::Revoked, Some("ALREADY_RENEWED"))
        );
    }

    #[test]
    fn keychain_errors_and_corrupt_map_to_states() {
        let s = MemoryStore::empty();
        *s.fail_get.lock().unwrap() = Some(HubAuthError::Keychain(KeychainFailure::Denied));
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut Runtime::default(), now()).state,
            HubState::KeychainDenied
        );
        let s = MemoryStore::with_raw("garbage");
        let st = compute_status(&s, HUB_ORIGIN, &mut Runtime::default(), now());
        assert_eq!(
            (st.state, st.reason.as_deref()),
            (HubState::Revoked, Some("CORRUPT"))
        );
    }

    #[test]
    fn status_reads_the_keychain_on_every_call_without_caching() {
        let s = store_with("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
        let mut rt = Runtime::default();
        for _ in 0..3 {
            compute_status(&s, HUB_ORIGIN, &mut rt, now());
        }
        assert_eq!(s.get_count(), 3);
    }

    #[test]
    fn status_serializes_without_token_material() {
        let s = store_with("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
        let mut rt = Runtime {
            device_name: Some("mbp".into()),
            ..Default::default()
        };
        let st = compute_status(&s, HUB_ORIGIN, &mut rt, now());
        let json = serde_json::to_string(&st).unwrap();
        assert!(
            json.contains("\"state\":\"CONNECTED\"") && json.contains("mbp"),
            "{json}"
        );
        assert!(
            !json.contains("mhd_") && !json.contains(&TEST_TOKEN[4..]),
            "{json}"
        );
        let dbg = format!("{st:?}");
        assert!(!dbg.contains("mhd_"));
        // 필드 목록 고정: 토큰성 필드가 추가되면 이 테스트가 깨진다.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "connected",
                "device_name",
                "expires_at",
                "reason",
                "renew_after",
                "state"
            ]
        );
    }

    #[test]
    fn notice_is_one_shot() {
        let s = MemoryStore::empty();
        let mut rt = Runtime {
            notice: Some("SERVER_REVOKE_FAILED"),
            ..Default::default()
        };
        assert_eq!(
            compute_status(&s, HUB_ORIGIN, &mut rt, now())
                .reason
                .as_deref(),
            Some("SERVER_REVOKE_FAILED")
        );
        assert_eq!(compute_status(&s, HUB_ORIGIN, &mut rt, now()).reason, None);
    }
}
