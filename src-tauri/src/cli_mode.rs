// CLI 모드: `malgn-agent --hub-auth-header` (설계 §7.3).
//
// Claude Code의 MCP `headersHelper`가 이 모드로 실행 파일을 부른다(플러그인의
// bin/hub-auth-header.mjs 래퍼 경유). `main()`이 Tauri를 초기화하기 **전에** 이 모듈로
// 분기하므로 창·WebView·Dock 아이콘·updater·스케줄러가 전혀 생기지 않는다. 이 모듈은
// Tauri Builder, 스레드 스폰(키체인 읽기 하나 제외), 네트워크, 파일 쓰기를 쓰지 않는다.
//
// 계약
//   stdout  성공 때만 정확히 한 줄 `{"Authorization":"Bearer mhd_…"}\n`. 실패 시 아무것도 쓰지 않는다.
//   stderr  실패·경고 때 `malgn-agent: <CODE>: <한국어 안내>` 한 줄. 토큰과 그 일부를 절대 쓰지 않는다
//           (문구는 전부 상수라 토큰이 들어갈 경로가 없다).
//   종료코드 0 OK / 10 NOT_CONNECTED / 11 EXPIRED / 12 KEYCHAIN_ERROR / 13 CORRUPT /
//           14 WRONG_AUDIENCE / 1 INTERNAL
//   이 표는 플러그인 bin/hub-auth-header.mjs의 안내 매핑과 한 벌이다(설계 §19).
//
// 순서: (1) 대상 서버(audience) 확인 — 키체인을 읽기 **전에** (2) 키체인 읽기(별도
// 스레드, 5초 데드라인) (3) 만료 확인 (4) 출력. CLI는 갱신하지 않는다(§7.2).

use crate::hub_auth::consts::{hub_origin, MCP_PATH};
use crate::hub_auth::error::{HubAuthError, KeychainFailure};
use crate::hub_auth::store::{self, KeyringStore, TokenStore};
use chrono::{DateTime, Utc};
use std::ffi::OsString;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const FLAG: &str = "--hub-auth-header";
/// 키체인 읽기 데드라인(§7.3 "10초 보장"). 래퍼가 8초, Claude Code가 10초에 끊는다.
pub(crate) const KEYCHAIN_DEADLINE: Duration = Duration::from_secs(5);

pub(crate) const EXIT_OK: i32 = 0;
pub(crate) const EXIT_INTERNAL: i32 = 1;
pub(crate) const EXIT_NOT_CONNECTED: i32 = 10;
pub(crate) const EXIT_EXPIRED: i32 = 11;
pub(crate) const EXIT_KEYCHAIN_ERROR: i32 = 12;
pub(crate) const EXIT_CORRUPT: i32 = 13;
pub(crate) const EXIT_WRONG_AUDIENCE: i32 = 14;

/// `argv[1] == "--hub-auth-header"`와 **정확히** 일치하고 다른 인자가 없을 때만 CLI 모드다.
/// `--hub-auth-header=x`, 추가 인자, 플래그 없음은 전부 기존 GUI 경로로 간다.
pub(crate) fn is_cli_invocation(args: &[OsString]) -> bool {
    args.len() == 2 && args[1] == FLAG
}

/// `main()`이 `run()`보다 먼저 호출한다. CLI 모드면 종료코드를, 아니면 None을 돌려준다.
pub fn dispatch() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().collect();
    if !is_cli_invocation(&args) {
        return None;
    }
    // 변수가 설정돼 있는지 자체가 판단 근거이므로 "설정됨"과 "UTF-8 아님"을 구분하지 않고
    // 손실 변환한다(변환된 값은 audience 비교에서 어차피 떨어진다 → 14, fail-closed).
    let server_url =
        std::env::var_os("CLAUDE_CODE_MCP_SERVER_URL").map(|v| v.to_string_lossy().into_owned());
    let store: Arc<dyn TokenStore> = Arc::new(KeyringStore::new());
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let code = run(
        store,
        &CliInput {
            server_url,
            hub_origin: hub_origin(),
            now: Utc::now(),
        },
        KEYCHAIN_DEADLINE,
        &mut stdout.lock(),
        &mut stderr.lock(),
    );
    Some(code)
}

pub(crate) struct CliInput {
    /// 환경변수 `CLAUDE_CODE_MCP_SERVER_URL` (없으면 None, 빈 문자열도 "설정됨"이다).
    pub(crate) server_url: Option<String>,
    pub(crate) hub_origin: String,
    pub(crate) now: DateTime<Utc>,
}

/// 대상 서버(audience) 판정(§7.3 M3). 환경 읽기와 분리한 순수 함수다.
///  - 변수가 없으면 허용(사람이 터미널에서 직접 실행하는 경우)
///  - 있으면 origin이 허브와 정규화 후 같고, path가 정확히 `/mcp`이며, query·fragment가
///    없고, 사용자 정보(`user:pass@`)가 없을 때만 허용. 빈 값·파싱 실패·`REDACTED`는 거부.
/// 호스트 대소문자와 기본 포트 `:443` 표기는 같은 서버로 본다(url 크레이트의 정규화).
pub(crate) fn audience_ok(server_url: Option<&str>, hub_origin: &str) -> bool {
    let Some(raw) = server_url else { return true };
    let Ok(url) = url::Url::parse(raw) else {
        return false;
    };
    let Ok(hub) = url::Url::parse(hub_origin) else {
        return false;
    };
    url.origin().is_tuple()
        && url.origin() == hub.origin()
        && url.path() == MCP_PATH
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn fail(err: &mut dyn Write, code: i32, name: &str, msg: &str) -> i32 {
    // 문구는 전부 상수다. 토큰은 이 함수에 전달되지도 않는다.
    let _ = writeln!(err, "malgn-agent: {name}: {msg}");
    let _ = err.flush();
    code
}

/// 키체인 읽기를 별도 스레드에서 돌리고 데드라인까지만 기다린다. 초과하면 스레드를
/// 버리고(프로세스 종료 시 정리된다) 12로 끝낸다.
fn read_with_deadline(
    store: Arc<dyn TokenStore>,
    origin: String,
    deadline: Duration,
) -> Result<Option<store::StoredToken>, HubAuthError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(store::load(&*store, &origin));
    });
    match rx.recv_timeout(deadline) {
        Ok(r) => r,
        // 데드라인 초과 또는 스레드 패닉(채널 끊김): 키체인 문제로 취급한다.
        Err(_) => Err(HubAuthError::Keychain(KeychainFailure::Backend(
            "timeout".to_string(),
        ))),
    }
}

pub(crate) fn run(
    store: Arc<dyn TokenStore>,
    input: &CliInput,
    deadline: Duration,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    // (1) audience — 키체인을 읽기 전에, 환경변수와 문자열 비교만으로 끝낸다.
    if !audience_ok(input.server_url.as_deref(), &input.hub_origin) {
        return fail(
            err,
            EXIT_WRONG_AUDIENCE,
            "WRONG_AUDIENCE",
            "허브가 아닌 서버에는 토큰을 제공하지 않습니다. MCP 설정의 headersHelper를 확인하세요.",
        );
    }

    // (2) 키체인 읽기
    let token = match read_with_deadline(store, input.hub_origin.clone(), deadline) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return fail(
                err,
                EXIT_NOT_CONNECTED,
                "NOT_CONNECTED",
                "맑은에이전트 앱에서 허브에 연결하세요.",
            )
        }
        Err(HubAuthError::Keychain(_)) => return fail(
            err,
            EXIT_KEYCHAIN_ERROR,
            "KEYCHAIN_ERROR",
            "키체인에서 토큰을 읽지 못했습니다. 맑은에이전트 앱을 한 번 실행해 접근을 허용하세요.",
        ),
        Err(HubAuthError::Corrupt) => {
            return fail(
                err,
                EXIT_CORRUPT,
                "CORRUPT",
                "저장된 허브 연결 정보가 손상되었습니다. 앱에서 다시 연결하세요.",
            )
        }
        Err(_) => return fail(err, EXIT_INTERNAL, "INTERNAL", "맑은에이전트 CLI 내부 오류"),
    };

    // (3) 만료
    if token.expires_at <= input.now {
        return fail(
            err,
            EXIT_EXPIRED,
            "EXPIRED",
            "허브 토큰이 만료되었습니다. 맑은에이전트 앱에서 다시 연결하세요.",
        );
    }

    // (4) 출력: 성공 때만, 정확히 한 줄. JSON 이스케이프는 serde_json에 맡긴다
    //     (토큰 형식은 저장 시 검증돼 따옴표·개행이 들어갈 수 없다).
    let line = serde_json::json!({ "Authorization": format!("Bearer {}", token.expose_token()) })
        .to_string();
    if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
        return fail(err, EXIT_INTERNAL, "INTERNAL", "맑은에이전트 CLI 내부 오류");
    }

    // 갱신 창이면 경고만 덧붙인다(종료코드 0). CLI는 갱신하지 않는다.
    if input.now >= token.renew_after {
        let secs = (token.expires_at - input.now).num_seconds().max(0);
        let days = (secs + 86_399) / 86_400;
        let _ = writeln!(
            err,
            "malgn-agent: OK: 맑은에이전트 앱을 실행하면 허브 토큰이 자동 갱신됩니다(만료 D-{days})."
        );
        let _ = err.flush();
    }
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_auth::consts::HUB_ORIGIN;
    use crate::hub_auth::store::testing::*;

    fn now() -> DateTime<Utc> {
        "2026-10-09T00:00:00Z".parse().unwrap()
    }

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    fn input(url: Option<&str>) -> CliInput {
        CliInput {
            server_url: url.map(String::from),
            hub_origin: HUB_ORIGIN.to_string(),
            now: now(),
        }
    }

    struct Run {
        code: i32,
        out: String,
        err: String,
    }

    fn run_with(store: Arc<dyn TokenStore>, url: Option<&str>) -> Run {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(
            store,
            &input(url),
            Duration::from_secs(2),
            &mut out,
            &mut err,
        );
        Run {
            code,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        }
    }

    fn connected(expires: &str, renew: &str) -> Arc<MemoryStore> {
        Arc::new(MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN, TEST_TOKEN, expires, renew,
        )))
    }

    // ---- dispatch ----
    #[test]
    fn dispatch_matches_only_the_exact_flag_alone() {
        assert!(is_cli_invocation(&args(&[
            "malgn-agent",
            "--hub-auth-header"
        ])));
        assert!(!is_cli_invocation(&args(&["malgn-agent"])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            "--hub-auth-header=x"
        ])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            "--hub-auth-header",
            "extra"
        ])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            "extra",
            "--hub-auth-header"
        ])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            "--HUB-AUTH-HEADER"
        ])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            " --hub-auth-header"
        ])));
        assert!(!is_cli_invocation(&args(&[
            "malgn-agent",
            "--hub-auth-header "
        ])));
        assert!(!is_cli_invocation(&args(&[])));
    }

    // ---- 종료코드 매핑 ----
    #[test]
    fn exit_0_prints_exactly_one_line_with_exactly_one_key() {
        let r = run_with(
            connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z"),
            None,
        );
        assert_eq!(r.code, 0);
        assert_eq!(r.out.matches('\n').count(), 1);
        assert!(r.out.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(r.out.trim_end()).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["Authorization"], format!("Bearer {TEST_TOKEN}"));
        assert_eq!(r.err, "", "갱신 창이 아니면 stderr는 비어 있다");
    }

    #[test]
    fn exit_0_in_renew_window_warns_on_stderr_without_the_token() {
        let r = run_with(
            connected("2026-10-20T00:00:00.000Z", "2026-10-01T00:00:00.000Z"),
            None,
        );
        assert_eq!(r.code, 0);
        assert!(r.out.contains(TEST_TOKEN));
        assert!(r.err.contains("D-11"), "{}", r.err);
        assert!(!r.err.contains(&TEST_TOKEN[4..]) && !r.err.contains("mhd_"));
    }

    #[test]
    fn exit_10_not_connected() {
        let r = run_with(Arc::new(MemoryStore::empty()), None);
        assert_eq!((r.code, r.out.as_str()), (10, ""));
        assert!(r.err.starts_with("malgn-agent: NOT_CONNECTED: "));
        assert!(r.err.contains("허브에 연결하세요"));
    }

    #[test]
    fn exit_11_expired_including_exact_boundary() {
        for exp in ["2026-10-08T00:00:00.000Z", "2026-10-09T00:00:00.000Z"] {
            let r = run_with(connected(exp, "2026-09-09T00:00:00.000Z"), None);
            assert_eq!((r.code, r.out.as_str()), (11, ""), "{exp}");
            assert!(r.err.starts_with("malgn-agent: EXPIRED: "));
        }
    }

    #[test]
    fn exit_12_keychain_error_and_denied() {
        for e in [
            KeychainFailure::Denied,
            KeychainFailure::Backend("OSStatus -25308".into()),
        ] {
            let s = MemoryStore::empty();
            *s.fail_get.lock().unwrap() = Some(HubAuthError::Keychain(e));
            let r = run_with(Arc::new(s), None);
            assert_eq!((r.code, r.out.as_str()), (12, ""));
            assert!(r.err.starts_with("malgn-agent: KEYCHAIN_ERROR: "));
        }
    }

    #[test]
    fn exit_12_when_keychain_read_exceeds_deadline() {
        let s = MemoryStore::with_raw(&sample_json(
            HUB_ORIGIN,
            TEST_TOKEN,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        ));
        *s.block_get.lock().unwrap() = Some(Duration::from_secs(3));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let started = std::time::Instant::now();
        let code = run(
            Arc::new(s),
            &input(None),
            Duration::from_millis(150),
            &mut out,
            &mut err,
        );
        assert_eq!(code, 12);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "데드라인에서 끊어야 한다"
        );
        assert!(out.is_empty());
    }

    #[test]
    fn exit_13_corrupt_json_and_hub_mismatch() {
        let bad_json = Arc::new(MemoryStore::with_raw("not json"));
        let r = run_with(bad_json, None);
        assert_eq!((r.code, r.out.as_str()), (13, ""));
        let other_hub = Arc::new(MemoryStore::with_raw(&sample_json(
            "https://attacker.example",
            TEST_TOKEN,
            "2027-10-09T00:00:00.000Z",
            "2027-09-09T00:00:00.000Z",
        )));
        let r = run_with(other_hub, None);
        assert_eq!((r.code, r.out.as_str()), (13, ""));
        assert!(r.err.starts_with("malgn-agent: CORRUPT: "));
    }

    // ---- audience (M3) ----
    #[test]
    fn audience_pure_function_table() {
        let ok = [
            None,
            Some("https://malgnai-hub.apiserver.kr/mcp"),
            Some("HTTPS://MALGNAI-HUB.apiserver.kr:443/mcp"),
            Some("https://malgnai-hub.apiserver.kr:443/mcp"),
        ];
        for u in ok {
            assert!(audience_ok(u, HUB_ORIGIN), "{u:?}");
        }
        let bad = [
            "https://attacker.example/mcp",
            "http://malgnai-hub.apiserver.kr/mcp",
            "https://malgnai-hub.apiserver.kr:8443/mcp",
            "https://malgnai-hub.apiserver.kr.evil.example/mcp",
            "https://malgnai-hub.apiserver.kr@evil.example/mcp",
            "https://evil.example@malgnai-hub.apiserver.kr/mcp",
            "https://malgnai-hub.apiserver.kr/mcp/",
            "https://malgnai-hub.apiserver.kr/MCP",
            "https://malgnai-hub.apiserver.kr/mcp/x",
            "https://malgnai-hub.apiserver.kr/",
            "https://malgnai-hub.apiserver.kr",
            "https://malgnai-hub.apiserver.kr/mcp?x=1",
            "https://malgnai-hub.apiserver.kr/mcp?",
            "https://malgnai-hub.apiserver.kr/mcp#frag",
            "https://malgnai-hub.apiserver.kr/mcp#",
            "",
            "   ",
            "not a url",
            "REDACTED",
            "https://REDACTED/mcp",
            "https://malgnai-hub.apiserver.kr/mcp?token=REDACTED",
            "file:///mcp",
            "data:text/plain,mcp",
        ];
        for u in bad {
            assert!(!audience_ok(Some(u), HUB_ORIGIN), "{u:?} must be rejected");
        }
    }

    #[test]
    fn exit_14_for_wrong_audience_never_touches_the_keychain() {
        for url in [
            "https://attacker.example/mcp",
            "http://malgnai-hub.apiserver.kr/mcp",
            "https://malgnai-hub.apiserver.kr:8443/mcp",
            "https://malgnai-hub.apiserver.kr.evil.example/mcp",
            "https://malgnai-hub.apiserver.kr/mcp/",
            "https://malgnai-hub.apiserver.kr/mcp?x",
            "",
            "REDACTED",
        ] {
            let store = connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
            let r = run_with(store.clone(), Some(url));
            assert_eq!((r.code, r.out.as_str()), (14, ""), "{url:?}");
            assert_eq!(store.get_count(), 0, "{url:?}: 키체인을 읽으면 안 된다");
            assert!(r.err.starts_with("malgn-agent: WRONG_AUDIENCE: "));
            assert!(!r.err.contains("mhd_"));
        }
    }

    #[test]
    fn correct_audience_reads_the_keychain_once_and_succeeds() {
        let store = connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
        let r = run_with(store.clone(), Some("https://malgnai-hub.apiserver.kr/mcp"));
        assert_eq!(r.code, 0);
        assert_eq!(store.get_count(), 1);
    }

    #[test]
    fn debug_override_origin_is_what_audience_is_compared_against() {
        let local = "http://127.0.0.1:8787";
        assert!(audience_ok(Some("http://127.0.0.1:8787/mcp"), local));
        assert!(!audience_ok(
            Some("https://malgnai-hub.apiserver.kr/mcp"),
            local
        ));
        assert!(!audience_ok(Some("http://127.0.0.1:8788/mcp"), local));
    }

    // ---- stderr에 토큰 없음(모든 종료코드) ----
    #[test]
    fn stderr_never_contains_token_for_any_exit_code() {
        let cases: Vec<(Arc<MemoryStore>, Option<&str>)> = vec![
            (
                connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z"),
                None,
            ),
            (
                connected("2026-10-20T00:00:00.000Z", "2026-10-01T00:00:00.000Z"),
                None,
            ),
            (
                connected("2026-10-01T00:00:00.000Z", "2026-09-01T00:00:00.000Z"),
                None,
            ),
            (Arc::new(MemoryStore::empty()), None),
            (Arc::new(MemoryStore::with_raw("garbage")), None),
            (
                connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z"),
                Some("https://evil.example/mcp"),
            ),
        ];
        for (store, url) in cases {
            let r = run_with(store, url);
            assert!(
                !r.err.contains("mhd_") && !r.err.contains(&TEST_TOKEN[4..]),
                "code {}: {}",
                r.code,
                r.err
            );
            assert!(r.err.lines().count() <= 1, "stderr는 한 줄: {}", r.err);
            if r.code != 0 {
                assert_eq!(r.out, "", "실패 시 stdout은 비어 있다 (code {})", r.code);
            }
        }
    }

    #[test]
    fn broken_stdout_pipe_is_an_internal_error_not_a_panic() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let store = connected("2027-10-09T00:00:00.000Z", "2027-09-09T00:00:00.000Z");
        let mut err = Vec::new();
        let code = run(
            store,
            &input(None),
            Duration::from_secs(1),
            &mut Broken,
            &mut err,
        );
        assert_eq!(code, 1);
    }
}
