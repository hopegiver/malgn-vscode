// ---------------- 플러그인/마켓플레이스 ----------------
// `lib.rs`에 있던 "설치된 플러그인 카탈로그" + "마켓플레이스" + "플러그인 실제
// 업데이트(사용자 명시 승인)" 세 절을 그대로 옮긴 것이다 — 로직은 한 글자도
// 바꾸지 않았다. `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]`
// 진입점만 이 파일에 두고, 조회 로직은 서브모듈(`installed`/`marketplace`)로
// 옮겼다.
//
// `claude` CLI를 셸을 거치지 않고 프로그램명+인자 배열로 직접 실행한다(인젝션
// 경로 없음). 대부분의 커맨드는 이 파일이 이미 읽어둔 신뢰할 수 있는 값(설치된
// 플러그인 id·마켓플레이스 id)만 인자로 받아 자유 텍스트 입력 경로 자체가 없다.
// 유일한 예외가 `marketplace::add_marketplace`/`remove_marketplace`(마켓플레이스
// 설정 탭의 "추가"/"제거") — 사용자가 직접 입력한 source/name이 argv로 들어가며,
// 셸을 거치지 않는 것은 동일하지만 빈 값·공백만·개행은 marketplace.rs가
// 프런트와 별도로 재검증한다(자세한 내용은 marketplace.rs 상단 주석).

mod global;
mod installed;
mod marketplace;

use crate::dev_tools::platform::{self, Platform};
use crate::process_util::SilentCommand;
use serde::Serialize;

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn list_installed_plugins() -> Vec<installed::InstalledPlugin> {
    tauri::async_runtime::spawn_blocking(installed::read_installed_plugins)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_known_marketplaces() -> Vec<marketplace::MarketplaceInfo> {
    marketplace::read_known_marketplaces()
}

/// 플러그인과 무관하게 개인이 직접 만든 전역(user-level) 에이전트/스킬을
/// 읽기 전용으로 조회한다. `list_known_marketplaces`처럼 스캔 대상이 소수
/// 파일이라 sync로 충분하다.
#[tauri::command]
pub fn list_global_catalog() -> global::GlobalCatalog {
    global::read_global_catalog()
}

#[derive(Serialize, Debug)]
pub(crate) struct CommandResult {
    success: bool,
    message: String,
}

/// `mcp_manager/process.rs`와 동일한 이유(결정 5.2 — 상수 비공유)로 독립적으로 둔다.
/// `dev_tools::DEV_TOOLS`의 Claude 항목과 값은 같지만(같은 실측 근거) 파일을
/// 공유하지 않는다.
///
/// 버그(2026-09-19, hub 이슈 01m2wm459wfe4s6g6kzgcwqd45): 이 상수가 원래
/// mac 후보 3개뿐이었고, `platform_now()`로 Windows 후보를 고르는 분기가
/// 아예 없었다 — `dev_tools::tool_path_candidates(def)`(mod.rs)가
/// `platform_now()`에 따라 `path_candidates`/`windows_path_candidates`를
/// 골라 쓰는 것과 달리, 여기 `run_claude_command`는 Windows에서도 이
/// mac 전용 배열을 그대로 `resolve_binary_expand_home`에 넘겼다. Windows에서
/// 이 3개 후보는 전부 `~/`·`/`로 시작하는 POSIX 경로라
/// `platform::expand_path_tokens`(Win 분기)가 치환할 토큰이 없어 문자열
/// 그대로 반환되고, `path_exists`는 당연히 전부 false다 — 그 결과 claude
/// 탐지가 절대경로 후보 단계를 완전히 건너뛰고 매번 PATH 스캔 폴백
/// (`resolve_binary`의 Windows 분기, 프로세스 PATH 환경변수 의존)에만
/// 의존했다. 이 PATH 환경변수가 GUI로 띄운 이 앱에 정확히 반영되지 않으면
/// (네이티브 설치기가 두는 `%USERPROFILE%\.local\bin`, winget 앱 실행
/// 별칭이 두는 `%LOCALAPPDATA%\Microsoft\WinGet\Links` 등은 npm 전역
/// prefix(`%APPDATA%\npm`)와 달리 `default_path_dirs`에도 없다) claude
/// 실행 자체가 엉뚱한 경로로 흘러가거나 실패한다 — 이 플러그인 설치/업데이트
/// 경로에서만 재현되고 `dev_tools`의 claude 조회(체크 화면)에서는 재현되지
/// 않았던 이유이기도 하다(그쪽은 애초에 `tool_path_candidates`를 쓴다).
const CLAUDE_PATH_CANDIDATES_MAC: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];
/// `dev_tools::DEV_TOOLS`의 Claude `windows_path_candidates`와 동일한 순서·값
/// (네이티브 설치기 → npm 전역 shim → winget 앱 실행 별칭). 설계 §B.5 그대로
/// — 전부 미검증(실기 Windows PC 필요, windows-latest CI도 claude CLI가 없어
/// 이 배열의 "값"만 검증하고 실제 파일 존재 여부는 검증하지 못한다).
const CLAUDE_PATH_CANDIDATES_WIN: [&str; 3] = [
    r"%USERPROFILE%\.local\bin\claude.exe",
    r"%APPDATA%\npm\claude.cmd",
    r"%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe",
];

/// `dev_tools::tool_path_candidates`와 동일한 패턴(순수함수 + `Platform` 인자
/// 주입, §D) — `platform_now()`를 직접 호출하는 `claude_path_candidates()`가
/// 이 함수를 감싼다. 인자로 분리해 두면 이 머신(Mac)의 `cargo test`에서도
/// `Platform::Win` 분기를 직접 실행 검증할 수 있다.
fn claude_path_candidates_for(plat: Platform) -> &'static [&'static str] {
    match plat {
        Platform::Mac => &CLAUDE_PATH_CANDIDATES_MAC,
        Platform::Win => &CLAUDE_PATH_CANDIDATES_WIN,
    }
}

fn claude_path_candidates() -> &'static [&'static str] {
    claude_path_candidates_for(platform::platform_now())
}

fn run_claude_command(args: &[&str]) -> CommandResult {
    let Some(resolved) =
        crate::cli_launcher::resolve_binary_expand_home(claude_path_candidates(), "claude")
    else {
        return CommandResult {
            success: false,
            message: "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                .to_string(),
        };
    };
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    match std::process::Command::new(&resolved)
        .args(args)
        .env("PATH", &path_env)
        .silent()
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                CommandResult {
                    success: true,
                    message: if stdout.is_empty() {
                        "완료되었습니다.".to_string()
                    } else {
                        stdout
                    },
                }
            } else {
                let msg = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    "알 수 없는 오류로 실패했습니다.".to_string()
                };
                CommandResult {
                    success: false,
                    message: msg,
                }
            }
        }
        Err(e) => CommandResult {
            success: false,
            message: format!("claude 명령을 실행할 수 없습니다: {e}"),
        },
    }
}

/// `plugin_id`는 `installed_plugins.json`의 키(예: "malgn-agent@malgnsoft-plugins")
/// 형식 그대로 받는다. `claude plugin update`는 로컬에 캐시된 마켓플레이스
/// 메타데이터만 보고 "최신 버전"을 판단하므로, 먼저 `plugin_id`에서 "@" 뒤의
/// 마켓플레이스 이름만 뽑아 `claude plugin marketplace update <marketplace>`로
/// 캐시를 갱신한 뒤에 실제 `claude plugin update`를 실행한다 — 안 그러면 마켓플레이스에
/// 새 버전이 올라와도 캐시가 이를 모른 채 "이미 최신"으로 판단해 아무 변경 없이
/// 성공을 반환한다. 마켓플레이스 갱신이 실패하면 그 결과를 그대로 반환하고
/// `plugin update`는 실행하지 않는다(캐시가 stale한 채로 업데이트해봐야 신뢰 불가).
/// 성공해도 재시작해야 적용된다 — 프론트엔드가 안내 문구를 붙인다.
#[tauri::command]
pub fn update_plugin(plugin_id: String) -> CommandResult {
    if let Some((_, marketplace)) = plugin_id.split_once('@') {
        let refresh_result =
            run_claude_command(&["plugin", "marketplace", "update", marketplace]);
        if !refresh_result.success {
            return refresh_result;
        }
    }
    run_claude_command(&["plugin", "update", &plugin_id])
}

#[tauri::command]
pub fn refresh_marketplaces() -> CommandResult {
    run_claude_command(&["plugin", "marketplace", "update"])
}

/// 마켓플레이스 설정 탭의 "추가" — 실행 로직·입력 검증·고정 마켓플레이스 보호는
/// 전부 `marketplace::run_add_marketplace`에 있다(marketplace.rs 참고). 여기서는
/// `#[tauri::command]` 진입점만 둔다(이 파일의 다른 커맨드들과 동일한 얇은 래퍼
/// 패턴 — `list_known_marketplaces`가 `marketplace::read_known_marketplaces()`를
/// 감싸는 것과 같다).
#[tauri::command]
pub fn add_marketplace(source: String) -> CommandResult {
    marketplace::run_add_marketplace(source)
}

/// 마켓플레이스 설정 탭의 "제거" — 실행 로직·입력 검증·고정 마켓플레이스 보호는
/// 전부 `marketplace::run_remove_marketplace`에 있다.
#[tauri::command]
pub fn remove_marketplace(name: String) -> CommandResult {
    marketplace::run_remove_marketplace(name)
}

/// `plugin_id`는 "name@marketplace" 형식(예: "malgn-agent@malgnsoft-plugins")만 받는다
/// — update_plugin과 동일한 id 포맷. `-y`로 비대화형 확인을 건너뛰고,
/// `-s user`로 설치 스코프를 user로 고정한다(installed_plugins.json에서
/// scope="user"만 "설치된 플러그인"으로 취급하는 기존 규칙과 맞춘다 — plugins/installed.rs 참고).
#[tauri::command]
pub fn install_plugin(plugin_id: String) -> CommandResult {
    run_claude_command(&["plugin", "install", &plugin_id, "-y", "-s", "user"])
}

// ⚠️ update_plugin()/refresh_marketplaces()/install_plugin()을 실제로 호출하는
// 테스트는 의도적으로 두지 않는다 — 이 머신에 실제 설치된 malgn-agent 플러그인
// (이 에이전트 자신이 로드되어 있는 바로 그 플러그인)을 `cargo test`를 돌릴 때마다
// 매번 실제로 업데이트/재설치해버리는 부작용은 누구도 원하지 않는다. 컴파일 통과
// (`cargo check`)로만 검증했고, 실제 클릭 검증은 사용자가 직접 GUI에서 해야 한다.
//
// 아래 `claude_path_candidates_for*` 테스트는 실제 프로세스를 하나도 띄우지
// 않는 순수함수만 검증한다 — 위 경고와 무관하게 이 머신(Mac)의 `cargo test`가
// Windows 분기의 "후보 배열 선택 로직"까지 실행 검증할 수 있다(§D 패턴,
// `dev_tools::platform_tests.rs`와 동일 기법). 다만 그 경로들이 실제
// Windows 파일시스템에 존재하는지, `claude.cmd`/`claude.exe`를 진짜로
// 찾아내는지는 windows-latest CI(또는 실기)에서만 확인 가능하다.
#[cfg(test)]
mod claude_path_candidates_tests {
    use super::*;
    use crate::dev_tools::platform::EnvRoots;

    fn fake_win_roots() -> EnvRoots {
        EnvRoots {
            home: Some(std::path::PathBuf::from(r"C:\Users\tester")),
            appdata: Some(std::path::PathBuf::from(r"C:\Users\tester\AppData\Roaming")),
            local_appdata: Some(std::path::PathBuf::from(r"C:\Users\tester\AppData\Local")),
            program_files: Some(std::path::PathBuf::from(r"C:\Program Files")),
            program_files_x86: Some(std::path::PathBuf::from(r"C:\Program Files (x86)")),
            pnpm_home: None,
            system_root: Some(std::path::PathBuf::from(r"C:\Windows")),
        }
    }

    /// 회귀 가드: mac 배열은 무변경이어야 한다(기존 호출부 계약 유지).
    #[test]
    fn claude_path_candidates_for_mac_unchanged() {
        assert_eq!(
            claude_path_candidates_for(Platform::Mac),
            &CLAUDE_PATH_CANDIDATES_MAC[..]
        );
    }

    /// 이번 버그의 핵심 재발 방지 가드: Windows 분기가 mac 전용(POSIX `/`·`~/`)
    /// 경로를 절대 반환하면 안 된다 — 원래 버그가 정확히 이 상태였다(Windows에서도
    /// CLAUDE_PATH_CANDIDATES_MAC 3개가 그대로 쓰였다). `~`/`/`로 시작하는 문자열이
    /// 하나라도 섞여 있으면 `platform::expand_path_tokens`(Win 분기)가 치환할
    /// 토큰이 없어 그대로 통과되고, 그 결과 `path_exists`가 항상 false를 반환해
    /// 절대경로 탐지 단계 전체가 무력화된다.
    #[test]
    fn claude_path_candidates_for_win_has_no_mac_style_paths() {
        for candidate in claude_path_candidates_for(Platform::Win) {
            assert!(
                !candidate.starts_with('/') && !candidate.starts_with('~'),
                "Windows 후보에 mac 전용 경로가 섞여 있습니다: {candidate}"
            );
        }
    }

    /// Windows 후보는 `platform::exe_extensions(Win)`이 인정하는 실행 확장자
    /// (.exe/.cmd/.bat) 중 하나로 끝나야 한다 — 확장자 없는 파일명은 Windows에서
    /// `CreateProcessW`가 직접 찾지 못한다.
    #[test]
    fn claude_path_candidates_for_win_end_with_known_windows_extension() {
        for candidate in claude_path_candidates_for(Platform::Win) {
            assert!(
                candidate.ends_with(".exe") || candidate.ends_with(".cmd") || candidate.ends_with(".bat"),
                "Windows 후보에 실행 확장자가 없습니다: {candidate}"
            );
        }
    }

    /// `dev_tools::DEV_TOOLS`의 Claude `windows_path_candidates`와 값이
    /// 일치하는지 직접 대조한다 — 결정 5.2(상수 비공유)로 파일은 분리돼
    /// 있지만 "같은 실측 근거"라는 주석의 주장을 코드로도 못박는다. 한쪽만
    /// 고치고 다른 쪽을 깜빡하는 재발을 잡아낸다.
    #[test]
    fn claude_path_candidates_for_win_matches_dev_tools_claude_definition() {
        use crate::dev_tools::{tool_definition, ToolId};
        let dev_tools_def = tool_definition(ToolId::Claude);
        assert_eq!(
            claude_path_candidates_for(Platform::Win),
            dev_tools_def.windows_path_candidates,
            "plugins::CLAUDE_PATH_CANDIDATES_WIN과 dev_tools::DEV_TOOLS Claude 항목이 어긋났습니다"
        );
    }

    /// 후보 문자열의 Windows 토큰(`%APPDATA%` 등)이 실제로 절대경로로 확장되는지
    /// 직접 확인한다 — `resolve_binary_with`가 이 결과를 그대로 `exists()`에
    /// 넘기므로, 여기서 확장이 깨지면 실제 파일이 정확히 그 자리에 있어도 영원히
    /// 못 찾는다. 가짜 `EnvRoots`를 주입해 이 머신(Mac)에서도 Windows 토큰 치환
    /// 로직 자체를 실행 검증한다(파일 존재 여부는 검증하지 않는다 — 그건 CI/실기 몫).
    #[test]
    fn claude_path_candidates_for_win_tokens_expand_to_absolute_paths() {
        let roots = fake_win_roots();
        let expected = [
            r"C:\Users\tester\.local\bin\claude.exe",
            r"C:\Users\tester\AppData\Roaming\npm\claude.cmd",
            r"C:\Users\tester\AppData\Local\Microsoft\WinGet\Links\claude.exe",
        ];
        for (candidate, want) in claude_path_candidates_for(Platform::Win).iter().zip(expected) {
            let expanded = platform::expand_path_tokens(Platform::Win, candidate, &roots);
            assert_eq!(
                expanded.as_deref(),
                Some(want),
                "토큰 확장 결과가 기대와 다릅니다: {candidate}"
            );
        }
    }
}
