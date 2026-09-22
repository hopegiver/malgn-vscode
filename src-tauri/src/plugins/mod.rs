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

/// 이슈 01m30vamyw8z58b9pk8h5epmgh 2단계: `run_claude_command`가 실패했을 때
/// "우리가 실제로 조립한 값"을 사용자가 보는 실패 메시지에 그대로 덧붙인다.
/// v0.2.8에 git PATH 보강(cdea6eb)과 WinGet\Links 보강(487cda8)이 둘 다
/// 들어갔는데도 같은 오류가 실기에서 재현되는데, 코드 정적 분석만으로는 더
/// 좁혀지지 않는다(위임서 1단계 기록 참고) — 다음 실패 재현에서 사용자가 이
/// 블록 하나만 붙여줘도 "PATH에 git 디렉터리가 실제로 없었는지" vs "PATH엔
/// 있는데 claude가 못 찾았는지" vs "CWD가 원인인지"를 코드를 더 안 읽고도
/// 구분할 수 있게 한다.
///
/// `resolved`/`path_env`는 자식에게 실제로 전달한 값 그대로(재계산·추정
/// 없음). `EnvRoots::from_env()`를 여기서 별도로 다시 읽는 것은 PATH 조립에
/// 쓰인 개별 환경변수 원본값(예: `ProgramFiles`가 아예 비어있었는지, 다른
/// 값이었는지)을 조립된 PATH 문자열과 나란히 보여주기 위해서다 — 조립된
/// 문자열만으로는 "어느 뿌리가 비어서 그 디렉터리가 빠졌는지"를 역산하기
/// 어렵다.
///
/// `child_cwd`는 `run_claude_command`가 실제로 `.current_dir()`에 넘긴 값
/// 그대로(재계산 없음) — `Some`이면 `crate::global_cwd::default_global_cwd()`가
/// 고른 경로를 명시적으로 고정했다는 뜻이고, `None`이면 그 함수가 존재하는
/// 후보를 하나도 찾지 못해 `.current_dir()` 자체를 호출하지 않았다는 뜻이다
/// (이때는 이 프로세스(앱)의 현재 작업 디렉터리를 자식이 그대로 물려받는다 —
/// 예전엔 이 호출부가 이 폴백 경로 없이 매번 이 상태였다. Windows 실기에서
/// 이 값이 `C:\Windows\system32`로 관측됐다, hub decision
/// 01m33w4pp3gyrjapczh1q7j4wm — 다만 이 값 하나만으로 그 실기에서 난 `claude`
/// 실행 실패의 인과가 확정되는 것은 아니다, 진단 블록의 나머지 두 값(주입 PATH
/// 전문, `ProgramFiles`)이 아직 회신되지 않았다).
///
/// ⚠ PATH·환경변수 값에는 사용자명이 포함될 수 있다 — devTools.ts 로그
/// 패널의 기존 경고와 같은 문구를 여기서도 그대로 붙인다.
fn diagnostic_block(resolved: &str, path_env: &str, child_cwd: Option<&std::path::Path>) -> String {
    let roots = platform::EnvRoots::from_env();
    let cwd_line = match child_cwd {
        Some(dir) => format!("{}(명시적으로 고정됨)", dir.to_string_lossy()),
        None => {
            let app_cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|e| format!("(읽기 실패: {e})"));
            format!("{app_cwd}(고정 후보를 찾지 못해 앱의 현재 작업 디렉터리를 그대로 상속)")
        }
    };
    let show = |v: &Option<std::path::PathBuf>| -> String {
        v.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(미설정)".to_string())
    };
    format!(
        "\n\n[진단 정보 — 사용자명·사내 경로가 포함될 수 있습니다. 외부(예: GitHub 이슈)에 붙여넣기 전 확인하세요]\n\
         - claude 실행 파일(해석됨): {resolved}\n\
         - 자식 프로세스에 주입한 PATH: {path_env}\n\
         - 자식 프로세스 CWD: {cwd_line}\n\
         - ProgramFiles: {}\n\
         - ProgramFiles(x86): {}\n\
         - LOCALAPPDATA: {}\n\
         - APPDATA: {}\n\
         - SystemRoot: {}",
        show(&roots.program_files),
        show(&roots.program_files_x86),
        show(&roots.local_appdata),
        show(&roots.appdata),
        show(&roots.system_root),
    )
}

/// claude 실행 파일 자체를 못 찾은 경우(절대경로 후보 + PATH 스캔 모두
/// 실패)의 진단 — `resolved`/`path_env`가 아직 없으므로 `diagnostic_block`과
/// 별도로 CWD + 환경변수 원본만 보여준다. `claude_path_candidates()`가 이
/// 실행에서 실제로 어떤 후보 목록을 썼는지(플랫폼 분기가 제대로 먹었는지)도
/// 함께 보여준다.
fn diagnostic_block_unresolved() -> String {
    let roots = platform::EnvRoots::from_env();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|e| format!("(읽기 실패: {e})"));
    let candidates = claude_path_candidates().join(", ");
    let show = |v: &Option<std::path::PathBuf>| -> String {
        v.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(미설정)".to_string())
    };
    format!(
        "\n\n[진단 정보 — 사용자명·사내 경로가 포함될 수 있습니다. 외부(예: GitHub 이슈)에 붙여넣기 전 확인하세요]\n\
         - 시도한 절대경로 후보: {candidates}\n\
         - 프로세스 CWD: {cwd}\n\
         - ProgramFiles: {}\n\
         - ProgramFiles(x86): {}\n\
         - LOCALAPPDATA: {}\n\
         - APPDATA: {}\n\
         - SystemRoot: {}\n\
         - PATH(원본, 스캔에 쓰인 값): {}",
        show(&roots.program_files),
        show(&roots.program_files_x86),
        show(&roots.local_appdata),
        show(&roots.appdata),
        show(&roots.system_root),
        std::env::var("PATH").unwrap_or_else(|_| "(미설정)".to_string()),
    )
}

fn run_claude_command(args: &[&str]) -> CommandResult {
    let Some(resolved) =
        crate::cli_launcher::resolve_binary_expand_home(claude_path_candidates(), "claude")
    else {
        return CommandResult {
            success: false,
            message: format!(
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).{}",
                diagnostic_block_unresolved()
            ),
        };
    };
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    // 마켓플레이스 갱신/설치·업데이트처럼 특정 프로젝트에 묶이지 않는 전역
    // 호출이라 `crate::global_cwd::default_global_cwd()`(workspace 루트 →
    // 홈 디렉터리 순, 존재 확인 후 첫 값)로 CWD를 명시적으로 고정한다 —
    // 이전엔 이 호출부만 `.current_dir()`을 부르지 않아 앱 프로세스의 CWD를
    // 그대로 물려받았다(구조적 비대칭, `diagnostic_block` 문서 참고). 후보가
    // 모두 존재하지 않으면 `None`이 와서 `.current_dir()`을 아예 호출하지
    // 않는다 — 존재하지 않는 경로를 넘겨 spawn 자체를 실패시키는 사고(포터블
    // exe·workspace 미설정 사용자 등)를 피하기 위해서다.
    let cwd = crate::global_cwd::default_global_cwd();

    let mut command = std::process::Command::new(&resolved);
    command.args(args).env("PATH", &path_env).silent();
    if let Some(dir) = &cwd {
        command.current_dir(dir);
    }

    match command.output() {
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
                    message: format!("{msg}{}", diagnostic_block(&resolved, &path_env, cwd.as_deref())),
                }
            }
        }
        Err(e) => CommandResult {
            success: false,
            message: format!(
                "claude 명령을 실행할 수 없습니다: {e}{}",
                diagnostic_block(&resolved, &path_env, cwd.as_deref())
            ),
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

// 이슈 01m30vamyw8z58b9pk8h5epmgh 2단계 검증: 진단 블록에 기대 항목이 실제로
// 들어가는지만 확인한다(값 자체는 이 머신의 실제 환경변수를 읽으므로 문자열
// 일치가 아니라 "라벨이 등장하는지"만 검증 — CI 머신마다 ProgramFiles 등의
// 실제 값은 다르다).
#[cfg(test)]
mod diagnostic_block_tests {
    use super::*;

    #[test]
    fn diagnostic_block_contains_resolved_path_env_and_cwd_note() {
        let block = diagnostic_block("/opt/homebrew/bin/claude", "/opt/homebrew/bin:/usr/bin", None);
        assert!(block.contains("claude 실행 파일(해석됨): /opt/homebrew/bin/claude"));
        assert!(block.contains("자식 프로세스에 주입한 PATH: /opt/homebrew/bin:/usr/bin"));
        assert!(block.contains("자식 프로세스 CWD"));
        assert!(block.contains("ProgramFiles:"));
        assert!(block.contains("ProgramFiles(x86):"));
        assert!(block.contains("LOCALAPPDATA:"));
        assert!(block.contains("APPDATA:"));
        assert!(block.contains("SystemRoot:"));
        // 사용자명/경로 유출 경고 문구 자체도 빠지면 안 된다.
        assert!(block.contains("사용자명·사내 경로가 포함될 수 있습니다"));
    }

    /// `child_cwd`가 `Some`이면(고정 성공) 그 경로가 "명시적으로 고정됨" 표시와
    /// 함께 그대로 보여야 한다 — 앱 자신의 CWD가 아니라 실제로 자식에게 넘긴
    /// 값이어야 신뢰할 수 있는 진단이 된다.
    #[test]
    fn diagnostic_block_shows_pinned_cwd_when_some() {
        let dir = std::path::PathBuf::from("/Users/tester/workspace");
        let block = diagnostic_block("claude", "PATH", Some(&dir));
        assert!(block.contains("자식 프로세스 CWD: /Users/tester/workspace(명시적으로 고정됨)"));
    }

    /// `child_cwd`가 `None`이면(존재하는 후보를 찾지 못함) "상속" 문구로
    /// 정직하게 표시해야 한다 — 이전 동작(항상 상속)을 오인시키지 않는다.
    #[test]
    fn diagnostic_block_shows_inherited_note_when_none() {
        let block = diagnostic_block("claude", "PATH", None);
        assert!(block.contains("고정 후보를 찾지 못해 앱의 현재 작업 디렉터리를 그대로 상속"));
    }

    #[test]
    fn diagnostic_block_unresolved_contains_tried_candidates_and_raw_path() {
        let block = diagnostic_block_unresolved();
        assert!(block.contains("시도한 절대경로 후보:"));
        assert!(block.contains("프로세스 CWD:"));
        assert!(block.contains("PATH(원본, 스캔에 쓰인 값):"));
        assert!(block.contains("사용자명·사내 경로가 포함될 수 있습니다"));
    }

    /// 실패 메시지 조립 지점(`run_claude_command`)이 실제로 이 블록을 이어
    /// 붙이는지는 여기서 직접 부르지 않는다(파일 상단 경고대로 실제 claude
    /// 프로세스를 스폰하는 테스트는 의도적으로 두지 않는다) — 대신 `format!`
    /// 조립 자체가 패닉 없이 동작하고 원본 메시지가 앞부분에 그대로 남는지는
    /// 문자열 조립 계약으로 고정한다.
    #[test]
    fn diagnostic_block_appends_after_original_message_without_losing_it() {
        let original = "Failed to refresh marketplace 'malgnsoft-plugins': some git error";
        let combined = format!("{original}{}", diagnostic_block("claude", "PATH", None));
        assert!(combined.starts_with(original));
        assert!(combined.len() > original.len());
    }
}
