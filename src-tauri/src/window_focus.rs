// ---------------- 인증 완료 시 앱 창을 앞으로 가져오기 ----------------
// 브라우저에서 인증을 마치고 이 앱으로 돌아오는 순간, 앱 창이 떠 있어도
// 브라우저 뒤에 가려져 있으면 사용자가 직접 앱을 찾아 전환해야 한다. 두
// 인증 흐름(claude CLI 로그인 `claude_auth.rs`, 앱 자체 Google 로그인
// `google_oauth/mod.rs`)이 공통으로 필요로 하는 지점이라 이 모듈로 뽑았다.
//
// 창 라벨: `tauri.conf.json`의 `app.windows`에 label을 지정하지 않았다 —
// Tauri는 이 경우 첫 창에 기본 라벨 "main"을 부여한다(이 앱은 창이 하나뿐인
// 구조). 창을 늘려 라벨 체계가 바뀌면 이 상수도 반드시 같이 바꿔야 한다.
//
// `pub(crate)`: `lib.rs`의 setup 훅(Windows 타이틀바 다크 테마 적용,
// `window_titlebar.rs`)도 같은 메인 창을 조회해야 해서 라벨 문자열을
// 두 번째 사본으로 흩어두지 않도록 공유한다.
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";

/// 창을 보이게 하고(최소화 해제 포함) 포커스를 요청한다.
///
/// 호출 정책은 각 인증 흐름(`claude_auth.rs`/`google_oauth/mod.rs`)이
/// 스스로 판단한다 — 이 함수는 "언제 부를지"가 아니라 "부르면 무엇을
/// 하는지"만 책임진다.
///
/// 실패해도(창을 찾지 못함, OS가 포커스 스틸링을 거부) 인증 흐름 자체를
/// 막지 않도록 모든 에러를 조용히 무시한다 — 이 함수는 UX 보조 기능이지
/// 인증 로직의 일부가 아니다.
///
/// macOS/Windows 차이: `set_focus()`는 OS의 포커스 스틸링 방지 정책에 따라
/// 무시될 수 있다 — 특히 Windows는 포그라운드에 있지 않던 프로세스가 임의로
/// 다른 창을 앞으로 가져오는 것을 기본적으로 제한하며, 이 경우 실제로는
/// 작업표시줄 아이콘 깜빡임으로 대체될 수 있다(Tauri 문서에 명시된 플랫폼
/// 제약 — 이 환경에서 실기 검증은 못했다). macOS는 이런 제한이 상대적으로
/// 느슨해 `set_focus()`가 대체로 그대로 동작한다.
///
/// 이미 포커스된 상태에서 다시 호출해도 부작용은 없다(멱등) — `show()`는
/// 이미 보이는 창에, `unminimize()`는 이미 최소화 해제된 창에, `set_focus()`는
/// 이미 포커스된 창에 각각 아무 효과가 없는 것이 두 OS 공통의 표준 동작이다.
pub fn focus_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}
