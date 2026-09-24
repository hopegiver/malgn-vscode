// ---------------- Windows 네이티브 타이틀바 다크 테마 ----------------
// 이 파일은 `#[cfg(target_os = "windows")]`로만 컴파일된다(lib.rs의 mod
// 선언 참조) — macOS 빌드에는 전혀 관여하지 않는다.
//
// Tauri 설정(`tauri.conf.json`)만으로는 타이틀바 색을 바꿀 수 없다 —
// Windows의 DWM(Desktop Window Manager) API를 직접 호출해야 한다. macOS는
// 커스텀 decorations 없이도 다크 타이틀바가 본문과 자연스럽게 어울리지만,
// Windows는 기본적으로 밝은/시스템색 타이틀바가 그려져 앱 본문(다크 테마)과
// 어긋난다는 사용자 요청에 대응한다.
//
// 색상은 `src/styles.css`의 다크 테마 토큰과 동일하게 맞춘다 — 정본은 그
// 파일이며 아래 상수는 그 값의 사본이다(정본 값이 바뀌면 같이 바꿔야 한다):
// - 타이틀바 배경 `#111217`(`--tabstrip-bg`) — 타이틀바 바로 아래에 오는
//   탭스트립과 같은 색이라 경계 없이 자연스럽게 이어진다.
// - 타이틀바 텍스트(캡션) `#dadde3`(`--text-primary`).
//
// 두 단계로 나눠 호출한다(단계별 실패 허용 범위가 다르다):
// 1. `DWMWA_USE_IMMERSIVE_DARK_MODE` — 일반 다크모드 타이틀바(검은 계열).
//    Windows 10 1809+에서 대체로 동작한다. 실패해도 무해하므로(그보다
//    구버전은 이 속성 자체가 없을 수 있다) HRESULT 실패를 조용히 무시한다.
// 2. `DWMWA_CAPTION_COLOR`/`DWMWA_TEXT_COLOR` — 정확한 브랜드 색 지정.
//    **Windows 11 22H2(빌드 22621) 이상에서만 존재**하는 속성이라 그보다
//    구버전에서는 호출이 실패하는데, 이것도 조용히 무시한다(패닉·에러
//    다이얼로그 금지). 결과적으로 22H2 미만에서는 1번의 일반 다크모드(검은
//    계열)만 적용되고, 22H2 이상에서만 정확한 브랜드 색이 적용되는 것이
//    의도된 동작이다.
//
// 호출 시점: `lib.rs`의 setup 훅에서 메인 윈도우 생성 직후 1회. 인증 완료
// 후 창을 앞으로 가져오는 `window_focus::focus_main_window`는 기존 창을
// 보이게 하고 포커스만 줄 뿐 새 창을 만들지 않으므로(그 모듈의 문서 주석
// 참조) 이 함수를 재호출할 필요가 없다.

use tauri::WebviewWindow;
// windows-rs 0.60+부터 BOOL은 Win32::Foundation에서 빠지고 windows::core로
// 옮겨졌다(GitHub Actions windows-latest 빌드에서 E0432로 실측 확인, 2026-09-24).
use windows::core::BOOL;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWINDOWATTRIBUTE,
};

/// `src/styles.css`의 `--tabstrip-bg: #111217`을 COLORREF(`0x00BBGGRR`,
/// RGB의 역순 바이트 배치)로 재배열한 값. R=0x11,G=0x12,B=0x17 →
/// 0x00_17_12_11.
const TITLEBAR_BG_COLORREF: u32 = 0x0017_1211;

/// `src/styles.css`의 `--text-primary: #dadde3`를 COLORREF로 재배열한 값.
/// R=0xda,G=0xdd,B=0xe3 → 0x00_e3_dd_da.
const TITLEBAR_TEXT_COLORREF: u32 = 0x00e3_ddda;

/// 메인 윈도우의 네이티브 타이틀바를 앱 다크 테마 색으로 맞춘다.
///
/// 모든 단계의 실패를 조용히 무시한다 — 구버전 Windows에서 일부(또는 전부)
/// 속성이 지원되지 않는 것은 정상적인 상황이지 버그가 아니다(위 모듈
/// 문서 참조).
pub fn apply_dark_titlebar(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    set_dwm_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &BOOL(1));
    set_dwm_attribute(hwnd, DWMWA_CAPTION_COLOR, &COLORREF(TITLEBAR_BG_COLORREF));
    set_dwm_attribute(hwnd, DWMWA_TEXT_COLOR, &COLORREF(TITLEBAR_TEXT_COLORREF));
}

/// `DwmSetWindowAttribute` 호출 한 건을 감싸는 헬퍼.
///
/// 실패(속성 미지원 등)는 이 모듈의 계약대로 조용히 무시한다 — 반환값을
/// 버리는 것은 실수가 아니라 의도다.
fn set_dwm_attribute<T>(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: &T) {
    // SAFETY: FFI 호출. `value`는 이 함수가 반환하기 전까지 유효한 스택
    // 참조이며, `size_of::<T>()`가 가리키는 버퍼 크기와 정확히 일치한다.
    // `hwnd`는 호출부(`apply_dark_titlebar`)가 `WebviewWindow::hwnd()`로
    // 갓 조회한 유효한 핸들이다.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            value as *const T as *const core::ffi::c_void,
            std::mem::size_of::<T>() as u32,
        )
    };
}
