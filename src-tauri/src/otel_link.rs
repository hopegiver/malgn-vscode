// OTLP 수집키 "연동 상태" 모델과 허브 발급·수신확인 인터페이스(trait) — 데이터 계약까지만.
//
// 배경: 향후 허브가 기기 토큰(`mhd_`)으로 인증된 API로 수집키(`mhk_`)를 내려주면 앱이 그것을
// `settings.json`의 OTEL 헤더에 적용하고 첫 수신까지 확인한다. **허브 발급·회전·상태조회 API
// 계약은 아직 없다** — 이 모듈에는 HTTP 호출·URL·응답 필드명이 없고, 허브에 의존하는 두 전이
// (발급, 수신확인)는 trait으로만 있으며 기본 구현은 "아직 지원 안 됨"을 돌려준다.
//
// 상태 모델
//   미연동(unlinked) → 발급중(issuing) → 적용됨(applied) → 수신확인됨(confirmed), 실패(failed, 원인)
//   현재 실제로 도달하는 것은 **미연동 / 적용됨 / 실패**이며 전부 `settings.json` 실물에서 도출한다
//   (`derive_link_status`). 발급중·수신확인됨은 허브 호출이 있어야만 생기는 상태라, trait 구현이
//   붙기 전에는 `issue_and_apply`/`confirm_receipt`가 어떤 경우에도 만들어 내지 않는다.
//
// mhd_/mhk_ 분리 불변식(리뷰 대상, 단위 테스트가 `*_never_contains_device_token`으로 고정):
//   - 기기 토큰(`mhd_`, MCP 전권)은 trait 구현체 내부에서만 쓰인다. 이 모듈의 어떤 인자·반환·상태·
//     에러·Debug에도 실리지 않는다. 발급기는 **이미 검증 가능한 키 문자열**(`SecretString`)만 돌려준다.
//   - 실패 원인은 데이터를 품지 않는 enum(`LinkFailureCause`)이고 사용자 문구는 그 원인의 고정 문구다.
//     허브 응답·입력 문자열·파일 값은 어떤 경로로도 상태 문구에 섞이지 않는다.
//   - 키가 `settings.json`에 들어가는 유일한 경로는 `otel_settings::apply_ingest_key`(→ 수동 입력과
//     같은 `save_to_settings_file` 엔진)이고, 형식 검증에서 `mhd_` 형태는 거부된다.
//   - 상태에는 `mhk_`+4자 마스킹만 싣는다(원문 없음).
//
// 허브 계약이 정해지면 이 파일에서 바뀌는 곳은 `default_issuer`/`default_receipt_checker`가 돌려주는
// 구현체뿐이다(상태 모델·적용 경로·프론트 계약은 그대로).

// 허브 계약 전에는 trait·오케스트레이션이 테스트에서만 호출된다. 구현체를 붙이면 이 줄을 지운다.
#![allow(dead_code)]

use crate::otel_ingest_key::{
    normalize_ingest_key, IngestKeyState, IngestKeyStatus, SecretString,
};
use serde::Serialize;
use std::path::Path;

/// 연동 단계. 직렬화 값: `unlinked | issuing | applied | confirmed | failed`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IngestLinkState {
    /// `Authorization` 헤더 없음.
    Unlinked,
    /// 허브에 키를 요청해 응답을 기다리는 중(허브 호출이 붙기 전에는 만들어지지 않는다).
    Issuing,
    /// `settings.json`에 허브 형식(`mhk_…`) 키가 적용돼 있다(수신은 아직 확인 전).
    Applied,
    /// 허브가 이 기기의 첫 수신을 확인했다(허브 호출이 붙기 전에는 만들어지지 않는다).
    Confirmed,
    /// 실패 — 원인은 `failure`.
    Failed,
}

/// 실패 원인. 데이터를 품지 않는다 — 문구는 `message()`의 고정 문자열뿐이라 어떤 입력·응답도
/// 상태 문구로 새지 않는다. 직렬화 값은 camelCase 식별자(프론트 분기용).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LinkFailureCause {
    /// `settings.json`을 JSON으로 해석하지 못했다(파일 도출).
    SettingsUnreadable,
    /// 허브 형식이 아닌 `Authorization` 헤더가 이미 있다(파일 도출).
    HeaderUnrecognized,
    /// 허브 발급·수신확인이 아직 지원되지 않는다.
    NotSupported,
    /// 허브 연결(기기 토큰)이 없거나 만료돼 요청할 수 없다.
    DeviceTokenUnavailable,
    /// 허브가 요청을 거부했다.
    HubRejected,
    /// 허브에 닿지 못했다(네트워크·타임아웃).
    Unreachable,
    /// 허브가 돌려준 값이 수집키 형식이 아니다.
    InvalidKeyReturned,
    /// 검증된 키를 설정 파일에 적용하지 못했다.
    ApplyFailed,
}

impl LinkFailureCause {
    /// 사용자 문구. 토큰 접두어 리터럴(`mhd_`/`mhk_`)도 넣지 않는다.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::SettingsUnreadable => {
                "Claude 설정 파일(settings.json)을 해석하지 못했습니다. 파일의 JSON 형식을 확인하세요."
            }
            Self::HeaderUnrecognized => {
                "설정 파일에 허브 수집키 형식이 아닌 Authorization 헤더가 있어 자동으로 바꾸지 않았습니다. 확인 후 직접 교체하거나 삭제하세요."
            }
            Self::NotSupported => "허브 자동 발급과 수신 확인은 아직 지원되지 않습니다.",
            Self::DeviceTokenUnavailable => {
                "허브 연결(기기 토큰)이 없거나 만료되어 요청할 수 없습니다. 허브에 다시 연결하세요."
            }
            Self::HubRejected => "허브가 요청을 거부했습니다.",
            Self::Unreachable => "허브에 연결하지 못했습니다. 네트워크 상태를 확인하세요.",
            Self::InvalidKeyReturned => {
                "허브가 돌려준 값이 수집키 형식이 아니어서 적용하지 않았습니다."
            }
            Self::ApplyFailed => "수집키를 설정 파일에 적용하지 못했습니다.",
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct IngestLinkFailure {
    pub cause: LinkFailureCause,
    /// `cause.message()` — 고정 문구.
    pub message: String,
}

impl IngestLinkFailure {
    fn of(cause: LinkFailureCause) -> Self {
        Self {
            cause,
            message: cause.message().to_string(),
        }
    }
}

/// 프론트로 나가는 연동 상태. 토큰·키 원문 필드가 없다(`masked`는 `mhk_`+4자뿐).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestLinkStatus {
    pub state: IngestLinkState,
    /// `mhk_AbCd…` 형태. 키가 적용돼 있는 상태(applied/confirmed)와, 키가 적용된 채 다음 단계가
    /// 실패한 경우(failed)에 있고, 그 외에는 `null`.
    pub masked: Option<String>,
    /// `state == failed`일 때만 있다.
    pub failure: Option<IngestLinkFailure>,
    /// 허브 자동 발급이 가능한가(UI가 "발급" 동작을 켤지 판단). Tauri 커맨드가 채운다.
    pub issue_supported: bool,
    /// 허브 수신 확인이 가능한가. Tauri 커맨드가 채운다.
    pub receipt_check_supported: bool,
}

impl IngestLinkStatus {
    fn new(state: IngestLinkState, masked: Option<String>) -> Self {
        Self {
            state,
            masked,
            failure: None,
            issue_supported: false,
            receipt_check_supported: false,
        }
    }

    fn failed(cause: LinkFailureCause, masked: Option<String>) -> Self {
        Self {
            failure: Some(IngestLinkFailure::of(cause)),
            ..Self::new(IngestLinkState::Failed, masked)
        }
    }

    pub(crate) fn with_support(mut self, issue: bool, receipt_check: bool) -> Self {
        self.issue_supported = issue;
        self.receipt_check_supported = receipt_check;
        self
    }
}

/// `settings.json` 실물에서 연동 상태를 도출한다(순수 함수 — 도달 가능한 3상태).
pub(crate) fn derive_link_status(
    settings_unreadable: bool,
    key: &IngestKeyStatus,
) -> IngestLinkStatus {
    if settings_unreadable {
        return IngestLinkStatus::failed(LinkFailureCause::SettingsUnreadable, None);
    }
    match key.state {
        IngestKeyState::Unset => IngestLinkStatus::new(IngestLinkState::Unlinked, None),
        IngestKeyState::Set => IngestLinkStatus::new(IngestLinkState::Applied, key.masked.clone()),
        IngestKeyState::Unrecognized => {
            IngestLinkStatus::failed(LinkFailureCause::HeaderUnrecognized, None)
        }
    }
}

fn read_file_status(path: &Path) -> IngestLinkStatus {
    let (unreadable, key) = crate::otel_settings::read_ingest_inputs(path);
    derive_link_status(unreadable, &key)
}

// ---------------- 허브 의존 전이: trait만 (실제 구현 없음) ----------------

/// 허브에서 수집키를 발급받는 쪽. 구현체는 기기 토큰(`mhd_`)을 **자기 안에서만** 쓴다(키체인에서
/// 읽어 인증 헤더로) — 인자·반환·에러 어디로도 내보내지 않는다. 반환은 허브가 내려준 키 문자열을
/// `SecretString`으로 감싼 것이며, 형식 검증은 호출부(`issue_and_apply`)가 `mhk_` 규칙으로 다시 한다.
pub(crate) trait IngestKeyIssuer: Send + Sync {
    /// UI가 발급 동작을 켜도 되는가.
    fn is_supported(&self) -> bool {
        true
    }
    fn issue(&self) -> Result<SecretString, LinkFailureCause>;
}

/// 허브가 이 기기의 수집(첫 수신)을 확인했는지 묻는 쪽. 기기 토큰 취급은 `IngestKeyIssuer`와 같다.
pub(crate) trait ReceiptChecker: Send + Sync {
    fn is_supported(&self) -> bool {
        true
    }
    fn check(&self) -> Result<ReceiptOutcome, LinkFailureCause>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReceiptOutcome {
    /// 허브가 이 기기에서 온 데이터를 수신했다.
    Received,
    /// 아직 수신 기록이 없다.
    NotYet,
}

/// 허브 계약 전 기본 구현 — 호출하면 "아직 지원 안 됨"을 돌려준다.
pub(crate) struct UnsupportedIssuer;

impl IngestKeyIssuer for UnsupportedIssuer {
    fn is_supported(&self) -> bool {
        false
    }
    fn issue(&self) -> Result<SecretString, LinkFailureCause> {
        Err(LinkFailureCause::NotSupported)
    }
}

pub(crate) struct UnsupportedReceiptChecker;

impl ReceiptChecker for UnsupportedReceiptChecker {
    fn is_supported(&self) -> bool {
        false
    }
    fn check(&self) -> Result<ReceiptOutcome, LinkFailureCause> {
        Err(LinkFailureCause::NotSupported)
    }
}

/// 허브 발급 계약 확정 후 실제 발급기(`hub_auth` 토큰 저장소 + 허브 HTTP)를 돌려주도록 바꾸는 지점.
pub(crate) fn default_issuer() -> Box<dyn IngestKeyIssuer> {
    Box::new(UnsupportedIssuer)
}

/// 허브 수신 확인 계약 확정 후 실제 구현을 돌려주도록 바꾸는 지점.
pub(crate) fn default_receipt_checker() -> Box<dyn ReceiptChecker> {
    Box::new(UnsupportedReceiptChecker)
}

// ---------------- 오케스트레이션 ----------------

/// 발급기에서 키를 받아 `settings.json`에 적용하고, 적용 후 실물에서 도출한 상태를 돌려준다.
///
/// 순서(실패하면 파일은 건드리지 않는다):
/// 1. 설정 파일이 해석 불가이거나 허브 형식이 아닌 `Authorization`이 있으면 **발급 전에** 멈춘다 —
///    적용할 수 없는 키를 허브에서 발급받지 않고, 사용자가 둔 다른 서비스 인증 헤더를 자동으로
///    덮어쓰지 않는다(수동 입력은 사용자의 명시적 행동이라 교체를 허용한다 — 그 경로는 그대로).
/// 2. `issuer.issue()` — 실패 원인은 그대로 상태로.
/// 3. 반환값을 `mhk_` 규칙으로 검증한다. `mhd_` 형태·잘못된 길이·마스킹 문자열·빈 값은 거부.
/// 4. `apply_ingest_key`(수동 입력과 같은 엔진: 백업·원자적 쓰기·`0600`).
pub(crate) fn issue_and_apply(path: &Path, issuer: &dyn IngestKeyIssuer) -> IngestLinkStatus {
    let base = read_file_status(path);
    if matches!(
        base.failure.as_ref().map(|f| f.cause),
        Some(LinkFailureCause::SettingsUnreadable | LinkFailureCause::HeaderUnrecognized)
    ) {
        return base;
    }
    let key = match issuer.issue() {
        Ok(key) => key,
        Err(cause) => return IngestLinkStatus::failed(cause, base.masked),
    };
    if normalize_ingest_key(key.expose()).is_err() {
        return IngestLinkStatus::failed(LinkFailureCause::InvalidKeyReturned, base.masked);
    }
    match crate::otel_settings::apply_ingest_key(path, key) {
        Ok(()) => read_file_status(path),
        // 저장 단계의 Err 문자열은 쓰지 않는다 — 상태 문구는 고정 문구만.
        Err(_) => IngestLinkStatus::failed(LinkFailureCause::ApplyFailed, base.masked),
    }
}

/// 적용된 키가 허브에 실제로 수신됐는지 확인한다. 키가 적용돼 있지 않으면(미연동·실패) 확인할
/// 대상이 없으므로 checker를 부르지 않고 파일 상태를 그대로 돌려준다.
pub(crate) fn confirm_receipt(path: &Path, checker: &dyn ReceiptChecker) -> IngestLinkStatus {
    let base = read_file_status(path);
    if base.state != IngestLinkState::Applied {
        return base;
    }
    match checker.check() {
        Ok(ReceiptOutcome::Received) => {
            IngestLinkStatus::new(IngestLinkState::Confirmed, base.masked)
        }
        Ok(ReceiptOutcome::NotYet) => base,
        Err(cause) => IngestLinkStatus::failed(cause, base.masked),
    }
}

/// 프론트가 연동 상태를 조회하는 커맨드. `settings.json`만 읽는다 — 허브를 부르지 않는다.
#[tauri::command]
pub fn otel_ingest_link_status() -> IngestLinkStatus {
    let status = match crate::otel_settings::settings_path() {
        Some(path) => read_file_status(&path),
        None => IngestLinkStatus::new(IngestLinkState::Unlinked, None),
    };
    status.with_support(
        default_issuer().is_supported(),
        default_receipt_checker().is_supported(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_atomic::backup_path_for;
    use crate::hub_auth::store::testing::test_token;
    use crate::otel_settings::{apply_ingest_key, save_to_settings_file, OtelSavePayload};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ALL_CAUSES: [LinkFailureCause; 8] = [
        LinkFailureCause::SettingsUnreadable,
        LinkFailureCause::HeaderUnrecognized,
        LinkFailureCause::NotSupported,
        LinkFailureCause::DeviceTokenUnavailable,
        LinkFailureCause::HubRejected,
        LinkFailureCause::Unreachable,
        LinkFailureCause::InvalidKeyReturned,
        LinkFailureCause::ApplyFailed,
    ];

    /// 테스트 전용 가짜 수집키(`mhk_` + 43자). 실제 키가 아니다.
    fn fake_key(fill: char) -> String {
        format!("mhk_{}", fill.to_string().repeat(43))
    }

    /// 테스트 전용 가짜 기기 토큰(`mhd_` + 43자). 실제 토큰이 아니다.
    fn fake_device_token() -> String {
        test_token('D')
    }

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-otel-link-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir.join("settings.json")
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    const ORIGINAL: &str =
        r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"SOME_OTHER_VAR":"keep-me"}}"#;

    /// 테스트 전용 가짜 발급기 — 가짜 `mhd_` 토큰을 입력으로 받고 정해진 결과를 돌려준다.
    #[derive(Debug)]
    struct FakeIssuer {
        device_token: SecretString,
        outcome: Result<SecretString, LinkFailureCause>,
        calls: AtomicUsize,
    }

    impl FakeIssuer {
        fn returning(key: &str) -> Self {
            Self::with(Ok(SecretString::new(key)))
        }
        fn failing(cause: LinkFailureCause) -> Self {
            Self::with(Err(cause))
        }
        fn with(outcome: Result<SecretString, LinkFailureCause>) -> Self {
            Self {
                device_token: SecretString::new(&fake_device_token()),
                outcome,
                calls: AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl IngestKeyIssuer for FakeIssuer {
        fn issue(&self) -> Result<SecretString, LinkFailureCause> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // 실제 발급기는 이 토큰으로 허브에 인증한다. 형식이 틀리면 연결 없음으로 본다.
            if !crate::hub_auth::store::is_valid_token(self.device_token.expose()) {
                return Err(LinkFailureCause::DeviceTokenUnavailable);
            }
            self.outcome.clone()
        }
    }

    struct FakeChecker {
        outcome: Result<ReceiptOutcome, LinkFailureCause>,
        calls: AtomicUsize,
    }

    impl ReceiptChecker for FakeChecker {
        fn check(&self) -> Result<ReceiptOutcome, LinkFailureCause> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.outcome
        }
    }

    /// 상태·에러·Debug 문자열 어디에도 기기 토큰이 없다.
    fn assert_no_device_token(label: &str, text: &str) {
        let dev = fake_device_token();
        assert!(!text.contains("mhd_"), "{label}: mhd_ 접두어 노출: {text}");
        assert!(!text.contains(&dev), "{label}: 기기 토큰 원문 노출: {text}");
        assert!(!text.contains("DDDD"), "{label}: 기기 토큰 본문 노출: {text}");
    }

    /// 상태를 직렬화·Debug 두 형태로 모두 검사한다.
    fn assert_status_clean(label: &str, status: &IngestLinkStatus, forbidden: &[&str]) {
        let json = serde_json::to_string(status).unwrap();
        let dbg = format!("{status:?}");
        for text in [&json, &dbg] {
            assert_no_device_token(label, text);
            for f in forbidden {
                assert!(!text.contains(f), "{label}: {f:?} 노출: {text}");
            }
        }
    }

    // ---- (1) 발급 → 적용: 기기 토큰이 어디에도 없고 settings.json에는 mhk_만 있다 ----

    #[test]
    fn issued_key_flow_never_contains_device_token() {
        let path = temp_settings_path("issue-ok");
        std::fs::write(&path, ORIGINAL).unwrap();
        let key = fake_key('K');

        let issuer = FakeIssuer::returning(&key);
        assert_eq!(issuer.calls(), 0);
        let status = issue_and_apply(&path, &issuer);
        assert_eq!(issuer.calls(), 1);

        // 반환 상태: 적용됨 + 마스킹만. 원문·기기 토큰 없음.
        assert_eq!(status.state, IngestLinkState::Applied);
        assert_eq!(status.masked.as_deref(), Some("mhk_KKKK…"));
        assert!(status.failure.is_none());
        assert_status_clean("status", &status, &[&key, "KKKKK"]);
        // 발급기 Debug(기기 토큰을 품은 객체)도 값을 가린다.
        assert_no_device_token("issuer Debug", &format!("{issuer:?}"));

        // settings.json: mhk_ 키만, 정규 형식으로, 기존 설정 보존.
        let saved = std::fs::read_to_string(&path).unwrap();
        assert_no_device_token("settings.json", &saved);
        assert_eq!(saved.matches("mhk_").count(), 1, "{saved}");
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            json["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {key}")
        );
        assert_eq!(json["env"]["SOME_OTHER_VAR"], "keep-me");
        assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");

        // 백업(롤링 1세대): 저장 직전 원본 그대로 — 키도 기기 토큰도 없다.
        let backup = std::fs::read_to_string(backup_path_for(&path)).unwrap();
        assert_eq!(backup, ORIGINAL);
        assert_no_device_token("backup", &backup);

        // 회전: 두 번째 발급 후 백업에는 직전 mhk_ 키가 있을 수 있으나 mhd_는 여전히 없다.
        let rotated = fake_key('R');
        let status = issue_and_apply(&path, &FakeIssuer::returning(&rotated));
        assert_eq!(status.masked.as_deref(), Some("mhk_RRRR…"));
        for file in [path.clone(), backup_path_for(&path)] {
            let text = std::fs::read_to_string(&file).unwrap();
            assert_no_device_token(&file.to_string_lossy(), &text);
            assert_eq!(text.matches("mhk_").count(), 1, "{text}");
        }
        cleanup(&path);
    }

    // ---- (2) 실패/잘못된 키는 거부되고 원문이 어디에도 없다 ----

    #[test]
    fn issuer_failure_never_contains_device_token() {
        for cause in ALL_CAUSES {
            let path = temp_settings_path("issue-fail");
            std::fs::write(&path, ORIGINAL).unwrap();
            let issuer = FakeIssuer::failing(cause);

            let status = issue_and_apply(&path, &issuer);

            assert_eq!(status.state, IngestLinkState::Failed, "{cause:?}");
            let failure = status.failure.as_ref().expect("실패 원인이 있어야 한다");
            assert_eq!(failure.cause, cause);
            assert_eq!(failure.message, cause.message());
            assert_status_clean(&format!("{cause:?}"), &status, &[]);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL, "{cause:?}: 파일 불변");
            assert!(!backup_path_for(&path).exists(), "{cause:?}: 백업도 만들지 않는다");
            cleanup(&path);
        }
    }

    #[test]
    fn issuer_returning_device_token_or_malformed_key_is_rejected_never_contains_device_token() {
        let dev = fake_device_token();
        let key = fake_key('K');
        let inputs: Vec<String> = vec![
            dev.clone(),                                  // 기기 토큰 자체를 수집키 자리에
            format!("Bearer {dev}"),
            format!("Authorization=Bearer {dev}"),
            format!("  {dev}\n"),
            format!("{key},{dev}"),                       // 키와 기기 토큰을 한 문자열에
            key[..46].to_string(),                        // 46자
            format!("{key}A"),                            // 48자
            "mhk_KKKK…".to_string(),                      // 마스킹 표시값
            format!("mhx_{}", "K".repeat(43)),            // 다른 접두어
            String::new(),
            "   ".to_string(),
        ];
        for input in inputs {
            let path = temp_settings_path("issue-bad-key");
            std::fs::write(&path, ORIGINAL).unwrap();
            let issuer = FakeIssuer::returning(&input);

            let status = issue_and_apply(&path, &issuer);

            let label = format!("input-len-{}", input.len());
            assert_eq!(status.state, IngestLinkState::Failed, "{label}");
            assert_eq!(
                status.failure.as_ref().unwrap().cause,
                LinkFailureCause::InvalidKeyReturned,
                "{label}"
            );
            // 입력 원문이 상태 문구·직렬화·Debug 어디에도 없다.
            let mut forbidden: Vec<&str> = vec!["KKKK"];
            if !input.trim().is_empty() {
                forbidden.push(input.trim());
            }
            assert_status_clean(&label, &status, &forbidden);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL, "{label}: 파일 불변");
            assert!(!backup_path_for(&path).exists(), "{label}");
            cleanup(&path);
        }
    }

    #[test]
    fn unreadable_settings_stops_before_issuing_and_never_contains_device_token() {
        let path = temp_settings_path("issue-unreadable");
        // 해석 불가 파일 안에 기기 토큰·수집키 모양 문자열이 들어 있어도 어디로도 나가지 않는다.
        let broken = format!(
            r#"{{"env":{{"SECRET_LIKE":"{}","OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer {}"}}, oops"#,
            fake_device_token(),
            fake_key('Z')
        );
        std::fs::write(&path, &broken).unwrap();
        let issuer = FakeIssuer::returning(&fake_key('K'));

        let status = issue_and_apply(&path, &issuer);

        assert_eq!(issuer.calls(), 0, "적용할 수 없는 상태에서는 키를 발급받지 않는다");
        assert_eq!(status.state, IngestLinkState::Failed);
        assert_eq!(
            status.failure.as_ref().unwrap().cause,
            LinkFailureCause::SettingsUnreadable
        );
        assert_status_clean("unreadable", &status, &["ZZZZ", "SECRET_LIKE"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken, "파일 불변");
        cleanup(&path);
    }

    #[test]
    fn foreign_authorization_header_is_not_overwritten_by_issuance() {
        let path = temp_settings_path("issue-foreign");
        let original =
            r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#;
        std::fs::write(&path, original).unwrap();
        let issuer = FakeIssuer::returning(&fake_key('K'));

        let status = issue_and_apply(&path, &issuer);

        assert_eq!(issuer.calls(), 0);
        assert_eq!(
            status.failure.as_ref().unwrap().cause,
            LinkFailureCause::HeaderUnrecognized
        );
        assert_status_clean("foreign", &status, &["FOREIGNSECRET"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        cleanup(&path);
    }

    #[test]
    fn apply_failure_status_is_fixed_message_never_contains_device_token_or_key() {
        let path = temp_settings_path("issue-apply-fail");
        // 텔레메트리가 켜져 있는데 엔드포인트가 없으면 저장 엔진이 거부한다(기존 규칙).
        let original = format!(
            r#"{{"env":{{"CLAUDE_CODE_ENABLE_TELEMETRY":"1","OTHER":"{}"}}}}"#,
            fake_device_token()
        );
        std::fs::write(&path, &original).unwrap();
        let key = fake_key('K');

        let status = issue_and_apply(&path, &FakeIssuer::returning(&key));

        assert_eq!(status.state, IngestLinkState::Failed);
        let failure = status.failure.as_ref().unwrap();
        assert_eq!(failure.cause, LinkFailureCause::ApplyFailed);
        assert_eq!(failure.message, LinkFailureCause::ApplyFailed.message());
        assert_status_clean("apply-fail", &status, &[&key, "KKKK", "CLAUDE_CODE_ENABLE_TELEMETRY"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "파일 불변");
        cleanup(&path);
    }

    #[test]
    fn failure_messages_never_contain_token_prefixes() {
        for cause in ALL_CAUSES {
            let m = cause.message();
            assert!(!m.contains("mhd_") && !m.contains("mhk_"), "{cause:?}: {m}");
            assert!(!m.is_empty());
        }
    }

    // ---- 단일 진입점: 수동 입력 경로와 주입 경로가 같은 파일을 만든다 ----

    #[test]
    fn injected_and_manual_set_paths_write_identical_files() {
        let key = fake_key('K');
        let manual = temp_settings_path("same-manual");
        let injected = temp_settings_path("same-injected");
        std::fs::write(&manual, ORIGINAL).unwrap();
        std::fs::write(&injected, ORIGINAL).unwrap();

        // 수동: 프론트가 보내는 JSON 그대로.
        let payload: OtelSavePayload = serde_json::from_str(&format!(
            r#"{{"values":{{}},"identity":null,"ingestKey":{{"action":"set","key":"  Authorization=Bearer {key}\n"}}}}"#
        ))
        .unwrap();
        save_to_settings_file(&manual, payload).unwrap();
        // 주입: 발급 응답 경로.
        apply_ingest_key(&injected, SecretString::new(&key)).unwrap();

        assert_eq!(
            std::fs::read_to_string(&manual).unwrap(),
            std::fs::read_to_string(&injected).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(backup_path_for(&manual)).unwrap(),
            std::fs::read_to_string(backup_path_for(&injected)).unwrap()
        );
        cleanup(&manual);
        cleanup(&injected);
    }

    #[test]
    fn injected_device_token_is_rejected_by_the_apply_entry_itself() {
        // 오케스트레이션의 선검증을 우회해 진입점에 직접 기기 토큰을 넣어도 거부된다.
        let path = temp_settings_path("entry-rejects-mhd");
        std::fs::write(&path, ORIGINAL).unwrap();
        let dev = fake_device_token();
        for input in [dev.clone(), format!("Authorization=Bearer {dev}")] {
            let err = apply_ingest_key(&path, SecretString::new(&input)).unwrap_err();
            assert_no_device_token("entry error", &err);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL);
        }
        cleanup(&path);
    }

    // ---- 원자적 쓰기: 임시파일 → rename, 0600 유지 ----

    // 같은 inode에 덮어쓰는(truncate+write) 구현이면 하드링크로 잡아 둔 이전 내용이 바뀐다.
    // 임시 파일을 새로 만들어 rename으로 교체하면 하드링크는 이전 내용을 그대로 갖는다.
    #[cfg(unix)]
    #[test]
    fn settings_write_replaces_file_via_temp_rename_not_in_place() {
        use std::os::unix::fs::MetadataExt;
        let path = temp_settings_path("atomic-rename");
        std::fs::write(&path, ORIGINAL).unwrap();
        let held = path.with_file_name("settings.held-link");
        std::fs::hard_link(&path, &held).unwrap();
        let inode_before = std::fs::metadata(&path).unwrap().ino();
        let key = fake_key('K');

        apply_ingest_key(&path, SecretString::new(&key)).unwrap();

        assert_eq!(
            std::fs::read_to_string(&held).unwrap(),
            ORIGINAL,
            "제자리 덮어쓰기면 기존 inode의 내용이 바뀐다"
        );
        assert_ne!(
            std::fs::metadata(&path).unwrap().ino(),
            inode_before,
            "임시 파일을 rename으로 교체해 inode가 바뀌어야 한다"
        );
        assert!(std::fs::read_to_string(&path).unwrap().contains(&key));
        cleanup(&path);
    }

    // rename 단계가 실패해도 키가 든 임시 파일이 남지 않고 기존 대상은 그대로다.
    // (대상 경로가 디렉터리라 rename이 실패한다 — 임시 파일을 쓴 뒤 rename으로 교체하는 순서의 증거.)
    #[cfg(unix)]
    #[test]
    fn failed_rename_leaves_no_temp_file_with_key() {
        let path = temp_settings_path("atomic-rename-fail");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("child"), "x").unwrap();
        let key = fake_key('K');

        let err = apply_ingest_key(&path, SecretString::new(&key)).unwrap_err();

        assert!(!err.contains("KKKK"), "에러에 키가 실리면 안 된다: {err}");
        assert!(path.is_dir(), "기존 대상은 그대로");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".malgn-tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "키가 든 임시 파일 잔존: {leftovers:?}");
        cleanup(&path);
    }

    #[cfg(unix)]
    #[test]
    fn injected_key_keeps_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let path = temp_settings_path("perms");
        std::fs::write(&path, ORIGINAL).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        issue_and_apply(&path, &FakeIssuer::returning(&fake_key('K')));
        assert_eq!(mode_of(&path), 0o600, "settings.json");
        let backup = backup_path_for(&path);
        assert_eq!(mode_of(&backup), 0o600, "백업");

        // 회전 후에도, 백업이 넓은 권한으로 남아 있던 경우에도 0600으로 좁혀진다.
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
        issue_and_apply(&path, &FakeIssuer::returning(&fake_key('R')));
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(mode_of(&backup), 0o600);
        cleanup(&path);
    }

    // ---- 상태 도출(settings.json 실물 → 미연동/적용됨/실패) ----

    #[test]
    fn link_status_is_derived_from_settings_file() {
        let path = temp_settings_path("derive");

        // 파일 없음 → 미연동.
        let s = read_file_status(&path);
        assert_eq!((s.state, s.masked.clone(), s.failure.clone()), (IngestLinkState::Unlinked, None, None));

        // 헤더 없는 파일 → 미연동. Authorization 없는 다른 헤더만 있어도 미연동.
        std::fs::write(&path, r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"x-team=a"}}"#).unwrap();
        assert_eq!(read_file_status(&path).state, IngestLinkState::Unlinked);

        // 허브 형식 키 → 적용됨 + 마스킹.
        let key = fake_key('K');
        std::fs::write(
            &path,
            format!(r#"{{"env":{{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer {key}"}}}}"#),
        )
        .unwrap();
        let s = read_file_status(&path);
        assert_eq!(s.state, IngestLinkState::Applied);
        assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"));
        assert!(s.failure.is_none());
        assert_status_clean("applied", &s, &[&key, "KKKKK"]);

        // 형식이 다른 Authorization → 실패(원인 headerUnrecognized), 원문 없음.
        std::fs::write(
            &path,
            r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#,
        )
        .unwrap();
        let s = read_file_status(&path);
        assert_eq!(s.state, IngestLinkState::Failed);
        assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::HeaderUnrecognized);
        assert!(s.masked.is_none());
        assert_status_clean("foreign", &s, &["FOREIGNSECRET"]);

        // 해석 불가 → 실패(원인 settingsUnreadable).
        std::fs::write(&path, "{ not json").unwrap();
        let s = read_file_status(&path);
        assert_eq!(s.state, IngestLinkState::Failed);
        assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::SettingsUnreadable);
        cleanup(&path);
    }

    #[test]
    fn link_status_json_shape_is_the_frontend_contract() {
        let key = fake_key('K');
        let applied = IngestLinkStatus::new(IngestLinkState::Applied, Some("mhk_KKKK…".into()))
            .with_support(false, false);
        assert_eq!(
            serde_json::to_string(&applied).unwrap(),
            r#"{"state":"applied","masked":"mhk_KKKK…","failure":null,"issueSupported":false,"receiptCheckSupported":false}"#
        );
        let failed = IngestLinkStatus::failed(LinkFailureCause::NotSupported, None);
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["state"], "failed");
        assert_eq!(json["failure"]["cause"], "notSupported");
        assert_eq!(json["failure"]["message"], LinkFailureCause::NotSupported.message());
        // 다섯 상태의 직렬화 값(프론트 유니온 타입과 한 벌).
        let names: Vec<String> = [
            IngestLinkState::Unlinked,
            IngestLinkState::Issuing,
            IngestLinkState::Applied,
            IngestLinkState::Confirmed,
            IngestLinkState::Failed,
        ]
        .iter()
        .map(|s| serde_json::to_value(s).unwrap().as_str().unwrap().to_string())
        .collect();
        assert_eq!(names, ["unlinked", "issuing", "applied", "confirmed", "failed"]);
        let causes: Vec<String> = ALL_CAUSES
            .iter()
            .map(|c| serde_json::to_value(c).unwrap().as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            causes,
            [
                "settingsUnreadable",
                "headerUnrecognized",
                "notSupported",
                "deviceTokenUnavailable",
                "hubRejected",
                "unreachable",
                "invalidKeyReturned",
                "applyFailed"
            ]
        );
        assert!(!serde_json::to_string(&applied).unwrap().contains(&key));
    }

    // ---- 허브 의존 전이는 인터페이스만 — 기본 구현은 "아직 지원 안 됨" ----

    #[test]
    fn default_providers_report_not_supported_without_touching_settings() {
        assert!(!default_issuer().is_supported());
        assert!(!default_receipt_checker().is_supported());
        assert_eq!(UnsupportedIssuer.issue().unwrap_err(), LinkFailureCause::NotSupported);
        assert_eq!(
            UnsupportedReceiptChecker.check().unwrap_err(),
            LinkFailureCause::NotSupported
        );

        let path = temp_settings_path("unsupported");
        std::fs::write(&path, ORIGINAL).unwrap();
        let s = issue_and_apply(&path, &UnsupportedIssuer);
        assert_eq!(s.state, IngestLinkState::Failed);
        assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::NotSupported);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL);
        assert!(!backup_path_for(&path).exists());
        cleanup(&path);
    }

    #[test]
    fn receipt_confirmation_transitions_and_never_contains_device_token() {
        let path = temp_settings_path("receipt");
        let checker = |outcome| FakeChecker {
            outcome,
            calls: AtomicUsize::new(0),
        };

        // 미연동에서는 확인할 대상이 없어 checker를 부르지 않는다.
        let c = checker(Ok(ReceiptOutcome::Received));
        let s = confirm_receipt(&path, &c);
        assert_eq!(s.state, IngestLinkState::Unlinked);
        assert_eq!(c.calls.load(Ordering::SeqCst), 0);

        let key = fake_key('K');
        std::fs::write(&path, ORIGINAL).unwrap();
        issue_and_apply(&path, &FakeIssuer::returning(&key));

        // 수신 없음 → 적용됨 유지. 수신 확인 → 수신확인됨. 실패 → 실패(키는 적용된 채라 마스킹 유지).
        let s = confirm_receipt(&path, &checker(Ok(ReceiptOutcome::NotYet)));
        assert_eq!(s.state, IngestLinkState::Applied);
        let s = confirm_receipt(&path, &checker(Ok(ReceiptOutcome::Received)));
        assert_eq!(s.state, IngestLinkState::Confirmed);
        assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"));
        assert_status_clean("confirmed", &s, &[&key, "KKKKK"]);
        let s = confirm_receipt(&path, &checker(Err(LinkFailureCause::Unreachable)));
        assert_eq!(s.state, IngestLinkState::Failed);
        assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::Unreachable);
        assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"));
        assert_status_clean("receipt-fail", &s, &[&key, "KKKKK"]);

        // 기본(미지원) checker는 "아직 지원 안 됨".
        let s = confirm_receipt(&path, &UnsupportedReceiptChecker);
        assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::NotSupported);
        cleanup(&path);
    }
}
