// 허브 기기 토큰 인증 상수 (설계 docs/design/device-token-auth.md §7.1 consts.rs, §19).
//
// 같이 바꿔야 하는 값(§19): HUB_ORIGIN / MCP_PATH는 플러그인 plugin.json의 url,
// 플러그인 bin/hub-auth-header.mjs의 audience 사전 차단과 한 벌이다. 이 값을 바꾸면
// 세 곳을 같이 바꾸지 않는 한 CLI가 14(WRONG_AUDIENCE)로 토큰을 내주지 않는다
// (fail-closed, 장애로 드러남).

/// 허브 origin 정본(앱 쪽). 끝에 `/`가 없다.
pub(crate) const HUB_ORIGIN: &str = "https://malgnai-hub.apiserver.kr";
/// 허브 MCP 엔드포인트 경로. CLI audience 확인이 정확일치로 비교한다.
pub(crate) const MCP_PATH: &str = "/mcp";

/// 키체인 항목 식별자(설계 §4 D5). E2 실측의 `security find-generic-password -s …`가 이 값을 쓴다.
pub(crate) const KEYCHAIN_SERVICE: &str = "com.malgnsoft.agent.hub";
pub(crate) const KEYCHAIN_ACCOUNT: &str = "device-token";

/// 서버 env `TRUSTED_MALGN_DESKTOP_CLIENT_ID`와 같은 값(공개값, 비밀 아님).
/// 이 저장소는 public이라 소스에 리터럴로 두지 않고 다른 빌드타임 값과 같은 패턴으로
/// 주입한다 — build.rs가 `src-tauri/.env`(gitignored) 또는 CI 환경변수에서 읽어
/// `cargo:rustc-env`로 넘기고, 여기서는 `option_env!`로만 받는다. 값이 없으면 None이고
/// `hub_auth_connect`가 패닉 없이 NotConfigured 오류를 돌려준다(기존 Google 클라이언트
/// 시크릿과 같은 처리). 두 값이 서버와 다르면 동의 단계에서 거부된다(§19, fail-closed).
/// CLI 모드는 이 값을 쓰지 않는다.
const DESKTOP_CLIENT_ID_RAW: Option<&str> = option_env!("MALGN_HUB_DESKTOP_CLIENT_ID");

/// CI에서 repo secret이 미등록이면 빈 문자열이 주입된다(`${{ secrets.X }}`는 빈 값으로
/// 확장) — 빈 값도 "설정 안 됨"으로 취급해야 [허브 연결]이 빈 client_id로 서버를 부르지
/// 않는다.
pub(crate) fn non_empty(v: Option<&'static str>) -> Option<&'static str> {
    v.map(str::trim).filter(|s| !s.is_empty())
}

pub(crate) fn desktop_client_id() -> Option<&'static str> {
    non_empty(DESKTOP_CLIENT_ID_RAW)
}

/// 허브 origin을 정한다. 순수 함수로 분리해 "release에서는 오버라이드가 무효"를 단위
/// 테스트로 고정한다. `allow_override`는 호출부에서 `cfg!(debug_assertions)`로 넣는다.
/// 오버라이드 값은 http/https origin만 받고(경로·쿼리는 버린다), 해석할 수 없으면
/// 기본값으로 돌아간다.
pub(crate) fn resolve_hub_origin(override_value: Option<&str>, allow_override: bool) -> String {
    if allow_override {
        if let Some(raw) = override_value {
            if let Ok(url) = url::Url::parse(raw.trim()) {
                if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
                    return url.origin().ascii_serialization();
                }
            }
        }
    }
    HUB_ORIGIN.to_string()
}

/// 현재 빌드에서 유효한 허브 origin. debug 빌드에서만 `MALGN_HUB_ORIGIN`으로 바꿀 수
/// 있다(e2e에서 로컬 서버를 가리키는 용도, 설계 §15). release 빌드는 환경변수를 읽지도
/// 않는다 — `cfg!`가 아니라 `#[cfg]`로 읽는 줄 자체를 분리해, 최적화 여부와 무관하게
/// release 바이너리에 오버라이드 경로가 존재하지 않게 한다.
pub(crate) fn hub_origin() -> String {
    #[cfg(debug_assertions)]
    let override_value = std::env::var("MALGN_HUB_ORIGIN").ok();
    #[cfg(not(debug_assertions))]
    let override_value: Option<String> = None;
    resolve_hub_origin(override_value.as_deref(), cfg!(debug_assertions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_is_ignored_when_not_allowed_release_behavior() {
        // release 빌드(allow_override=false)에서는 어떤 값이 와도 기본 origin이다.
        assert_eq!(
            resolve_hub_origin(Some("http://127.0.0.1:8787"), false),
            HUB_ORIGIN
        );
        assert_eq!(
            resolve_hub_origin(Some("https://evil.example"), false),
            HUB_ORIGIN
        );
    }

    #[test]
    fn override_is_applied_only_when_allowed_and_normalized() {
        assert_eq!(
            resolve_hub_origin(Some("http://127.0.0.1:8787/some/path?x=1"), true),
            "http://127.0.0.1:8787"
        );
        assert_eq!(resolve_hub_origin(None, true), HUB_ORIGIN);
    }

    #[test]
    fn unparsable_or_non_http_override_falls_back_to_default() {
        for bad in [
            "",
            "not a url",
            "ftp://host",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert_eq!(resolve_hub_origin(Some(bad), true), HUB_ORIGIN, "{bad}");
        }
    }

    #[test]
    fn empty_client_id_counts_as_not_configured() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("")), None);
        assert_eq!(non_empty(Some("  ")), None);
        assert_eq!(non_empty(Some("abc")), Some("abc"));
    }

    #[test]
    fn hub_origin_constant_has_no_trailing_slash_or_path() {
        let u = url::Url::parse(HUB_ORIGIN).unwrap();
        assert_eq!(u.origin().ascii_serialization(), HUB_ORIGIN);
        assert_eq!(u.path(), "/");
    }
}
