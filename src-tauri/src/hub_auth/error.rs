// 허브 인증 오류 (설계 §7.1 "오류" 계약).
//
// 이 enum의 어떤 변형도 토큰 원문이나 그 일부를 담지 않는다. 구성은 다음과 같다:
//  - 네트워크 오류는 reqwest::Error를 그대로 들고 있지 않는다. reqwest 오류의 Display는
//    요청 URL을 포함할 수 있으므로 `without_url()` 뒤 분류 라벨만 남긴다.
//  - 서버 오류는 HTTP 상태와 서버가 준 `code`뿐이다. 서버 본문은 신뢰하지 않으므로
//    코드 모양([A-Za-z0-9_]{1,40})을 검사해 통과한 것만 남긴다(응답 에코로 임의 문자열이
//    UI·로그로 새는 것을 막는다).
//  - 키체인 오류는 백엔드가 준 메시지(OSStatus 설명 등)만 담는다.
// Tauri 커맨드의 `Err(String)`은 이 enum의 Display를 쓴다.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeychainFailure {
    /// 접근 거부·잠김(`keyring::Error::NoStorageAccess`).
    Denied,
    /// 그 밖의 백엔드 오류. 메시지는 플랫폼이 준 설명이다(토큰 아님).
    Backend(String),
    /// 저장 직후 다시 읽은 값이 쓴 값과 다름(read-back 불일치).
    ReadBackMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HubAuthError {
    /// 빌드에 desktop client_id가 주입되지 않음.
    NotConfigured,
    /// 연결·타임아웃 등 전송 계층 실패. 라벨만 담는다.
    Network(&'static str),
    /// 서버가 비정상 상태로 응답. `code`는 검증을 통과한 서버 오류 코드(없으면 "UNKNOWN").
    Http {
        status: u16,
        code: String,
    },
    Keychain(KeychainFailure),
    /// 키체인 값이 JSON v1 스키마·허브 origin·토큰 형식 검증을 통과하지 못함.
    Corrupt,
    /// 루프백 로그인 흐름(브라우저 열기, 콜백 대기, state 불일치 등) 실패. 메시지는 앱이 만든 문구다.
    Flow(String),
    /// 서버 응답 본문을 해석하지 못함.
    BadResponse,
    /// 409 RENEW_TOO_EARLY. 서버가 알려 준 갱신 가능 시각(있으면)으로 일정을 다시 잡는다.
    RenewTooEarly {
        renew_after: Option<String>,
    },
}

impl fmt::Display for HubAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HubAuthError::NotConfigured => write!(
                f,
                "허브 연결 설정이 이 빌드에 포함되지 않았습니다(MALGN_HUB_DESKTOP_CLIENT_ID). 관리자에게 문의하세요."
            ),
            HubAuthError::Network(kind) => write!(f, "허브 서버에 연결하지 못했습니다({kind})."),
            HubAuthError::Http { status, code } => {
                write!(f, "허브 서버가 요청을 거부했습니다(HTTP {status}, {code}).")
            }
            HubAuthError::Keychain(KeychainFailure::Denied) => {
                write!(f, "키체인 접근이 거부되었습니다.")
            }
            HubAuthError::Keychain(KeychainFailure::Backend(msg)) => {
                write!(f, "키체인 오류: {msg}")
            }
            HubAuthError::Keychain(KeychainFailure::ReadBackMismatch) => {
                write!(f, "키체인에 저장한 값을 다시 읽어 확인하지 못했습니다.")
            }
            HubAuthError::Corrupt => {
                write!(f, "저장된 허브 연결 정보가 손상되었습니다. 다시 연결하세요.")
            }
            HubAuthError::Flow(msg) => write!(f, "{msg}"),
            HubAuthError::BadResponse => write!(f, "허브 서버 응답을 해석하지 못했습니다."),
            HubAuthError::RenewTooEarly { .. } => write!(f, "아직 갱신 시기가 아닙니다."),
        }
    }
}

impl std::error::Error for HubAuthError {}

/// 서버가 준 오류 코드를 안전한 모양일 때만 통과시킨다.
pub(crate) fn sanitize_server_code(raw: &str) -> String {
    if !raw.is_empty()
        && raw.len() <= 40
        && !raw.starts_with("mhd_")
        && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        raw.to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

/// reqwest 오류를 URL이 없는 분류 라벨로 바꾼다.
pub(crate) fn classify_reqwest(err: reqwest::Error) -> HubAuthError {
    let err = err.without_url();
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_decode() || err.is_body() {
        "body"
    } else if err.is_redirect() {
        "redirect"
    } else {
        "request"
    };
    HubAuthError::Network(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_plain_codes_and_drops_weird_ones() {
        assert_eq!(sanitize_server_code("ALREADY_RENEWED"), "ALREADY_RENEWED");
        assert_eq!(sanitize_server_code("invalid_grant"), "invalid_grant");
        assert_eq!(sanitize_server_code(""), "UNKNOWN");
        assert_eq!(sanitize_server_code("has space"), "UNKNOWN");
        assert_eq!(sanitize_server_code("mhd_abc.def"), "UNKNOWN");
        assert_eq!(sanitize_server_code("mhd_abcdefgh"), "UNKNOWN");
        assert_eq!(sanitize_server_code(&"A".repeat(41)), "UNKNOWN");
    }
}
