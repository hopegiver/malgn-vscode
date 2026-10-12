// 키체인 저장소와 저장 형식 (설계 §4 D5, §7.1 store.rs).
//
// 토큰 원문은 이 모듈과 api.rs·cli_mode.rs 밖으로 나가지 않는다(§7.1 M4 계약).
//  - `StoredToken`은 `Serialize`를 derive하지 않는다. 키체인에 쓰는 JSON은 이 파일 안의
//    private 와이어 타입(`Wire`)으로만 만든다. 그래서 Tauri 커맨드 반환 타입이나
//    `serde_json::to_string(&stored)` 형태로 실수로 프론트에 흘려보낼 수 없다.
//  - `Debug`는 수동 구현이고 토큰은 `mhd_***`로만 찍힌다.

use super::consts::{KEYCHAIN_ACCOUNT, KEYCHAIN_SERVICE};
use super::error::{HubAuthError, KeychainFailure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 토큰 형식(설계 §4 D2, §19): `mhd_` + base64url(32바이트) 43자 = 47자.
/// 플러그인 bin/hub-auth-header.mjs의 출력 검증 정규식과 한 벌이다.
pub(crate) const TOKEN_PREFIX: &str = "mhd_";
pub(crate) const TOKEN_LEN: usize = 47;

pub(crate) fn is_valid_token(token: &str) -> bool {
    token.len() == TOKEN_LEN
        && token.starts_with(TOKEN_PREFIX)
        && token[TOKEN_PREFIX.len()..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 키체인 JSON v1의 와이어 형태. 이 타입만 (역)직렬화한다.
#[derive(Serialize, Deserialize)]
struct Wire {
    v: u32,
    hub: String,
    token: String,
    device_token_id: String,
    expires_at: String,
    renew_after: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StoredToken {
    hub: String,
    token: String,
    pub(crate) device_token_id: String,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) renew_after: DateTime<Utc>,
    // 서버가 준 원문 문자열을 보존한다 — 재직렬화 때 밀리초 표기가 달라져 read-back
    // 비교가 흔들리는 일을 막고, 키체인에는 서버 표기 그대로 남긴다.
    expires_at_raw: String,
    renew_after_raw: String,
}

impl std::fmt::Debug for StoredToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredToken")
            .field("hub", &self.hub)
            .field("token", &"mhd_***")
            .field("device_token_id", &self.device_token_id)
            .field("expires_at", &self.expires_at_raw)
            .field("renew_after", &self.renew_after_raw)
            .finish()
    }
}

fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn origin_of(raw: &str) -> Option<String> {
    url::Url::parse(raw)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
        .map(|u| u.origin().ascii_serialization())
}

impl StoredToken {
    /// 서버 응답(발급·갱신)에서 저장 항목을 만든다. 형식이 틀리면 BadResponse.
    pub(crate) fn from_grant(
        hub: &str,
        token: &str,
        device_token_id: &str,
        expires_at: &str,
        renew_after: &str,
    ) -> Result<Self, HubAuthError> {
        if !is_valid_token(token) || device_token_id.is_empty() {
            return Err(HubAuthError::BadResponse);
        }
        Ok(StoredToken {
            hub: hub.to_string(),
            token: token.to_string(),
            device_token_id: device_token_id.to_string(),
            expires_at: parse_ts(expires_at).ok_or(HubAuthError::BadResponse)?,
            renew_after: parse_ts(renew_after).ok_or(HubAuthError::BadResponse)?,
            expires_at_raw: expires_at.to_string(),
            renew_after_raw: renew_after.to_string(),
        })
    }

    /// 키체인에서 읽은 JSON을 해석한다. 스키마 버전·허브 origin·토큰 형식·날짜가
    /// 하나라도 어긋나면 Corrupt(§7.3 종료코드 13). `hub`는 정규화한 origin으로 비교한다.
    pub(crate) fn parse(raw_json: &str, expected_hub_origin: &str) -> Result<Self, HubAuthError> {
        let wire: Wire = serde_json::from_str(raw_json).map_err(|_| HubAuthError::Corrupt)?;
        if wire.v != 1 {
            return Err(HubAuthError::Corrupt);
        }
        let stored_origin = origin_of(&wire.hub).ok_or(HubAuthError::Corrupt)?;
        let expected = origin_of(expected_hub_origin).ok_or(HubAuthError::Corrupt)?;
        if stored_origin != expected {
            return Err(HubAuthError::Corrupt);
        }
        if !is_valid_token(&wire.token) || wire.device_token_id.is_empty() {
            return Err(HubAuthError::Corrupt);
        }
        Ok(StoredToken {
            hub: wire.hub,
            token: wire.token,
            device_token_id: wire.device_token_id,
            expires_at: parse_ts(&wire.expires_at).ok_or(HubAuthError::Corrupt)?,
            renew_after: parse_ts(&wire.renew_after).ok_or(HubAuthError::Corrupt)?,
            expires_at_raw: wire.expires_at,
            renew_after_raw: wire.renew_after,
        })
    }

    /// 키체인에 쓸 JSON. 결과는 토큰 원문을 포함하므로 호출부는 곧바로 `TokenStore::set_raw`에만 넘긴다.
    pub(crate) fn to_keychain_json(&self) -> String {
        serde_json::to_string(&Wire {
            v: 1,
            hub: self.hub.clone(),
            token: self.token.clone(),
            device_token_id: self.device_token_id.clone(),
            expires_at: self.expires_at_raw.clone(),
            renew_after: self.renew_after_raw.clone(),
        })
        .expect("고정 구조체 직렬화는 실패하지 않는다")
    }

    /// 토큰 원문 접근자. api.rs(Authorization 헤더)와 cli_mode.rs(stdout 한 줄)만 쓴다.
    pub(crate) fn expose_token(&self) -> &str {
        &self.token
    }
}

// ---------------- 저장소 추상화 ----------------

/// 저장소는 JSON 문자열 한 덩어리만 다룬다. 해석·검증은 `load`가 맡는다.
pub(crate) trait TokenStore: Send + Sync {
    /// 항목이 없으면 `Ok(None)`.
    fn get_raw(&self) -> Result<Option<String>, HubAuthError>;
    fn set_raw(&self, value: &str) -> Result<(), HubAuthError>;
    /// 항목이 이미 없어도 성공.
    fn delete_raw(&self) -> Result<(), HubAuthError>;
}

/// 키체인을 읽고 검증한다. 항목이 없으면 `Ok(None)`.
pub(crate) fn load(
    store: &dyn TokenStore,
    hub_origin: &str,
) -> Result<Option<StoredToken>, HubAuthError> {
    match store.get_raw()? {
        None => Ok(None),
        Some(raw) => StoredToken::parse(&raw, hub_origin).map(Some),
    }
}

/// 쓰고 다시 읽어 일치하는지 확인한다(read-back, §3.1 5, §3.3 5). 불일치·실패는 오류.
pub(crate) fn save_verified(
    store: &dyn TokenStore,
    token: &StoredToken,
) -> Result<(), HubAuthError> {
    let json = token.to_keychain_json();
    store.set_raw(&json)?;
    match store.get_raw()? {
        Some(read_back) if read_back == json => Ok(()),
        _ => Err(HubAuthError::Keychain(KeychainFailure::ReadBackMismatch)),
    }
}

// ---------------- OS 키체인 구현 ----------------

pub(crate) struct KeyringStore {
    service: String,
    account: String,
}

impl KeyringStore {
    pub(crate) fn new() -> Self {
        KeyringStore {
            service: KEYCHAIN_SERVICE.to_string(),
            account: KEYCHAIN_ACCOUNT.to_string(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, HubAuthError> {
        keyring::Entry::new(&self.service, &self.account).map_err(map_keyring_error)
    }
}

/// keyring 오류를 토큰이 들어갈 수 없는 분류로 바꾼다. `BadEncoding(Vec<u8>)`처럼 값
/// 바이트를 담는 변형은 메시지를 쓰지 않는다.
fn map_keyring_error(err: keyring::Error) -> HubAuthError {
    match err {
        keyring::Error::NoStorageAccess(_) => HubAuthError::Keychain(KeychainFailure::Denied),
        keyring::Error::PlatformFailure(inner) => {
            HubAuthError::Keychain(KeychainFailure::Backend(inner.to_string()))
        }
        keyring::Error::BadEncoding(_) => HubAuthError::Corrupt,
        keyring::Error::NoEntry => {
            HubAuthError::Keychain(KeychainFailure::Backend("항목 없음".to_string()))
        }
        keyring::Error::TooLong(attr, _) | keyring::Error::Invalid(attr, _) => {
            HubAuthError::Keychain(KeychainFailure::Backend(format!("잘못된 속성({attr})")))
        }
        keyring::Error::Ambiguous(_) => HubAuthError::Keychain(KeychainFailure::Backend(
            "같은 이름의 항목이 둘 이상입니다".to_string(),
        )),
        _ => HubAuthError::Keychain(KeychainFailure::Backend("알 수 없는 오류".to_string())),
    }
}

impl TokenStore for KeyringStore {
    fn get_raw(&self) -> Result<Option<String>, HubAuthError> {
        match self.entry()?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_error(e)),
        }
    }

    fn set_raw(&self, value: &str) -> Result<(), HubAuthError> {
        self.entry()?.set_password(value).map_err(map_keyring_error)
    }

    fn delete_raw(&self) -> Result<(), HubAuthError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_error(e)),
        }
    }
}

/// 현재 빌드의 keyring 기본 백엔드가 mock이 아닌 OS 네이티브인지 확인한다(설계 §4 D5
/// 가드). keyring 3은 feature를 빠뜨리면 mock으로 조용히 폴백하므로, 이 함수와 아래
/// 테스트가 "feature 누락 → 아무것도 영속되지 않음"을 빌드 시점이 아니라 테스트 시점에
/// 잡는다. 빌더 생성만 확인하고 키체인은 건드리지 않는다.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn default_backend_is_native() -> bool {
    let builder = keyring::default::default_credential_builder();
    #[cfg(target_os = "macos")]
    {
        builder
            .as_any()
            .downcast_ref::<keyring::macos::MacCredentialBuilder>()
            .is_some()
    }
    #[cfg(target_os = "windows")]
    {
        builder
            .as_any()
            .downcast_ref::<keyring::windows::WinCredentialBuilder>()
            .is_some()
    }
}

// 지원 OS(macOS/Windows)에서 네이티브 백엔드가 컴파일에 포함됐는지 컴파일 시점에도
// 한 번 더 막는다: 아래 모듈들은 feature가 빠지면 존재하지 않아 컴파일이 실패한다.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
type _AssertMacBackend = keyring::macos::MacCredentialBuilder;
#[cfg(target_os = "windows")]
#[allow(dead_code)]
type _AssertWinBackend = keyring::windows::WinCredentialBuilder;

// ---------------- 테스트용 메모리 구현 ----------------

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    pub(crate) const TEST_TOKEN: &str = "mhd_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_ABCDe";

    pub(crate) fn test_token(seed: char) -> String {
        let mut t = String::from("mhd_");
        t.extend(std::iter::repeat(seed).take(43));
        t
    }

    /// 메모리 저장소. get 호출 횟수를 세고(재읽기·audience 선차단 검증), 쓰기·읽기
    /// 실패와 블로킹을 주입할 수 있다.
    pub(crate) struct MemoryStore {
        pub(crate) value: Mutex<Option<String>>,
        pub(crate) get_calls: AtomicUsize,
        pub(crate) fail_get: Mutex<Option<HubAuthError>>,
        pub(crate) fail_set: Mutex<Option<HubAuthError>>,
        /// 설정하면 get_raw가 이 시간만큼 잠든다(데드라인 시험).
        pub(crate) block_get: Mutex<Option<std::time::Duration>>,
        /// true면 set은 성공하지만 값이 저장되지 않는다(read-back 불일치 시험).
        pub(crate) drop_writes: Mutex<bool>,
    }

    impl MemoryStore {
        pub(crate) fn empty() -> Self {
            MemoryStore {
                value: Mutex::new(None),
                get_calls: AtomicUsize::new(0),
                fail_get: Mutex::new(None),
                fail_set: Mutex::new(None),
                block_get: Mutex::new(None),
                drop_writes: Mutex::new(false),
            }
        }
        pub(crate) fn with_raw(raw: &str) -> Self {
            let s = Self::empty();
            *s.value.lock().unwrap() = Some(raw.to_string());
            s
        }
        pub(crate) fn get_count(&self) -> usize {
            self.get_calls.load(Ordering::SeqCst)
        }
        pub(crate) fn raw(&self) -> Option<String> {
            self.value.lock().unwrap().clone()
        }
    }

    impl TokenStore for MemoryStore {
        fn get_raw(&self) -> Result<Option<String>, HubAuthError> {
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            let delay = *self.block_get.lock().unwrap();
            if let Some(d) = delay {
                std::thread::sleep(d);
            }
            if let Some(e) = self.fail_get.lock().unwrap().clone() {
                return Err(e);
            }
            Ok(self.value.lock().unwrap().clone())
        }
        fn set_raw(&self, value: &str) -> Result<(), HubAuthError> {
            if let Some(e) = self.fail_set.lock().unwrap().clone() {
                return Err(e);
            }
            if !*self.drop_writes.lock().unwrap() {
                *self.value.lock().unwrap() = Some(value.to_string());
            }
            Ok(())
        }
        fn delete_raw(&self) -> Result<(), HubAuthError> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }

    pub(crate) fn sample_json(hub: &str, token: &str, expires: &str, renew: &str) -> String {
        serde_json::json!({
            "v": 1, "hub": hub, "token": token, "device_token_id": "01JTEST",
            "expires_at": expires, "renew_after": renew
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::hub_auth::consts::HUB_ORIGIN;

    const FAR: &str = "2099-01-01T00:00:00.000Z";
    const FAR_RENEW: &str = "2098-12-01T00:00:00.000Z";

    #[test]
    fn token_shape_validation() {
        assert!(is_valid_token(TEST_TOKEN));
        assert!(!is_valid_token("mhd_short"));
        assert!(!is_valid_token(&format!("xxx_{}", "A".repeat(43))));
        assert!(!is_valid_token(&format!("mhd_{}\n", "A".repeat(42))));
        assert!(!is_valid_token(&format!("mhd_{}", "A".repeat(44))));
        assert!(!is_valid_token(&format!("mhd_{}\"", "A".repeat(42))));
    }

    #[test]
    fn json_v1_roundtrip_preserves_all_fields() {
        let t = StoredToken::from_grant(HUB_ORIGIN, TEST_TOKEN, "01JABC", FAR, FAR_RENEW).unwrap();
        let json = t.to_keychain_json();
        let back = StoredToken::parse(&json, HUB_ORIGIN).unwrap();
        assert_eq!(t, back);
        assert_eq!(back.expose_token(), TEST_TOKEN);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["v"], 1);
        assert_eq!(v["hub"], HUB_ORIGIN);
        assert_eq!(v["device_token_id"], "01JABC");
        assert_eq!(v["expires_at"], FAR);
        assert_eq!(v["renew_after"], FAR_RENEW);
    }

    #[test]
    fn hub_origin_mismatch_is_corrupt() {
        let json = sample_json("https://attacker.example", TEST_TOKEN, FAR, FAR_RENEW);
        assert_eq!(
            StoredToken::parse(&json, HUB_ORIGIN).unwrap_err(),
            HubAuthError::Corrupt
        );
    }

    #[test]
    fn hub_origin_compared_after_normalization() {
        let json = sample_json(
            "HTTPS://MALGNAI-HUB.apiserver.kr:443/",
            TEST_TOKEN,
            FAR,
            FAR_RENEW,
        );
        assert!(StoredToken::parse(&json, HUB_ORIGIN).is_ok());
    }

    #[test]
    fn malformed_values_are_corrupt() {
        let ok = sample_json(HUB_ORIGIN, TEST_TOKEN, FAR, FAR_RENEW);
        assert!(StoredToken::parse(&ok, HUB_ORIGIN).is_ok());
        for bad in [
            "".to_string(),
            "not json".to_string(),
            "{}".to_string(),
            sample_json(HUB_ORIGIN, "mhd_short", FAR, FAR_RENEW),
            sample_json(HUB_ORIGIN, TEST_TOKEN, "tomorrow", FAR_RENEW),
            sample_json(HUB_ORIGIN, TEST_TOKEN, FAR, "later"),
            ok.replace("\"v\":1", "\"v\":2"),
        ] {
            assert_eq!(
                StoredToken::parse(&bad, HUB_ORIGIN).unwrap_err(),
                HubAuthError::Corrupt,
                "{bad}"
            );
        }
    }

    #[test]
    fn debug_output_masks_token() {
        let t = StoredToken::from_grant(HUB_ORIGIN, TEST_TOKEN, "01J", FAR, FAR_RENEW).unwrap();
        let dbg = format!("{t:?}");
        assert!(dbg.contains("mhd_***"));
        assert!(!dbg.contains(TEST_TOKEN));
        assert!(!dbg.contains(&TEST_TOKEN[4..]));
        let pretty = format!("{t:#?}");
        assert!(!pretty.contains(&TEST_TOKEN[4..]));
    }

    #[test]
    fn load_returns_none_when_empty_and_reads_each_time() {
        let store = MemoryStore::empty();
        assert!(load(&store, HUB_ORIGIN).unwrap().is_none());
        assert_eq!(store.get_count(), 1);
        let _ = load(&store, HUB_ORIGIN);
        assert_eq!(store.get_count(), 2, "load는 캐시 없이 매번 store를 읽는다");
    }

    #[test]
    fn save_verified_detects_dropped_write() {
        let store = MemoryStore::empty();
        *store.drop_writes.lock().unwrap() = true;
        let t = StoredToken::from_grant(HUB_ORIGIN, TEST_TOKEN, "01J", FAR, FAR_RENEW).unwrap();
        assert_eq!(
            save_verified(&store, &t).unwrap_err(),
            HubAuthError::Keychain(KeychainFailure::ReadBackMismatch)
        );
    }

    #[test]
    fn save_verified_succeeds_with_normal_store() {
        let store = MemoryStore::empty();
        let t = StoredToken::from_grant(HUB_ORIGIN, TEST_TOKEN, "01J", FAR, FAR_RENEW).unwrap();
        save_verified(&store, &t).unwrap();
        assert_eq!(load(&store, HUB_ORIGIN).unwrap().unwrap(), t);
    }

    // keyring 3은 기본 백엔드가 없다 — feature 누락 시 mock으로 조용히 폴백한다(설계 D5).
    // 빌더 생성은 키체인을 건드리지 않으므로 기본 실행에 포함한다.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn keyring_default_backend_is_native_not_mock() {
        assert!(
            default_backend_is_native(),
            "keyring 기본 백엔드가 OS 네이티브가 아니다 — Cargo.toml의 apple-native/windows-native feature를 확인하라"
        );
    }

    // 실제 OS 키체인을 쓰기·읽기·삭제하는 왕복 시험. 기본 실행에서 제외한다:
    //  - macOS에서는 키체인 접근 프롬프트가 뜰 수 있고(ad-hoc 서명 바이너리), CI 러너에는
    //    로그인 키체인이 없을 수 있다.
    //  - 개발자의 실제 항목(com.malgnsoft.agent.hub)을 덮어쓰지 않도록 시험 전용 service
    //    이름을 쓴다.
    // 실행: cargo test --manifest-path src-tauri/Cargo.toml -- --ignored keyring_real_roundtrip
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    #[ignore = "실제 OS 키체인을 건드린다(프롬프트 가능). macOS/Windows 실기에서 --ignored로 수동 실행"]
    fn keyring_real_roundtrip() {
        assert!(default_backend_is_native());
        let store = KeyringStore {
            service: "com.malgnsoft.agent.hub.test".to_string(),
            account: format!("device-token-test-{}", std::process::id()),
        };
        let t = StoredToken::from_grant(HUB_ORIGIN, TEST_TOKEN, "01JREAL", FAR, FAR_RENEW).unwrap();
        save_verified(&store, &t).unwrap();
        assert_eq!(load(&store, HUB_ORIGIN).unwrap().unwrap(), t);
        store.delete_raw().unwrap();
        assert!(store.get_raw().unwrap().is_none());
        store.delete_raw().unwrap(); // 이미 없어도 성공
    }
}
