// ---------------- 외부 CLI(gh, wrangler) 위임 공통 유틸 ----------------
// GitHub·Cloudflare 연동은 자격증명을 이 앱이 직접 취급하지 않고 각 CLI에
// 위임한다. 실측으로 확인된 제약을 여기서 한 번에 처리한다:
//
// 1) Finder로 띄운 .app은 launchctl 기본 PATH만 상속하고 그 PATH에 gh·wrangler가
//    없다(`launchctl getenv PATH` 비어있음, 실제 바이너리는 /opt/homebrew/bin 아래).
//    → `Command::new("gh")` 대신 절대경로 후보를 순서대로 확인한다.
// 2) 파이프로 연결된 자식 프로세스에는 TTY가 없다. `gh auth login`·`wrangler login`
//    처럼 사용자 입력을 기다리는 대화형 로그인은 앱이 조용히 spawn하지 않고,
//    사용자가 실제로 보고 조작할 수 있는 터미널 창(macOS: Terminal.app,
//    Windows: PowerShell)에서 실행되도록 연다.
//
// Windows 지원(dev_tools 설계 §D.3/§E): 이 파일의 함수들은 시그니처를 그대로
// 유지한 채 내부적으로 `dev_tools::platform`의 순수함수로 위임한다 — 6개
// 호출부(github_integration.rs, cloudflare_integration.rs, plugins/mod.rs,
// dev_tools/mod.rs·install_resolver.rs 내부, autonomy/mcp_manager/session_chat의
// CLAUDE_PATH_CANDIDATES 소비처)는 전혀 손대지 않는다. macOS 분기는 기존 로직과
// 바이트 단위로 동일하게 유지한다(플랫폼 판정이 이 머신에서는 항상 Mac이라
// 기존 동작이 그대로 보존된다).

use crate::dev_tools::platform::{self, Platform};
use crate::process_util::SilentCommand;
use std::process::Command;

/// 후보 절대경로를 순서대로 확인하고, 전부 없으면(macOS만) PATH 안에 있을
/// 가능성(예: 터미널에서 `pnpm tauri dev`로 실행한 개발 모드)에 대비해 이름
/// 그대로도 한 번 시도한다. Windows는 bare-name 서브프로세스 프로브를 쓰지
/// 않는다 — Windows `CreateProcess`의 기본 검색 경로에 현재 디렉터리(CWD)가
/// 포함되어 있어, 악성 `npm.exe` 등이 CWD에 있으면 그것이 실행될 수 있다(설계
/// G-1). 대신 실제 PATH를 명시적으로 스캔해 절대경로를 확정한 뒤에만
/// 반환한다(`platform::resolve_binary_with`, 프로세스를 하나도 띄우지 않는다).
/// 어느 쪽도 찾지 못하면 `None` — 이때 "미설치"는 예외가 아니라 정상 값이다.
pub fn resolve_binary(absolute_candidates: &[&str], bare_name: &str) -> Option<String> {
    let plat = platform::platform_now();
    let path_var = match plat {
        Platform::Win => {
            let roots = platform::EnvRoots::from_env();
            let process_path = std::env::var("PATH").unwrap_or_default();
            platform::build_search_path_var(plat, &process_path, &roots)
        }
        Platform::Mac => String::new(),
    };

    if let Some(found) = platform::resolve_binary_with(
        plat,
        absolute_candidates,
        &path_var,
        bare_name,
        &|p| platform::path_exists(p),
    ) {
        return Some(found);
    }

    if plat == Platform::Mac {
        // 기존 그대로(변경 없음): PATH 폴백은 존재 여부만 확인하고(버전 출력
        // 내용은 쓰지 않는다) 성공하면 이름을 그대로 돌려준다. 여기서
        // 실행하는 것은 --version 뿐이라 안전하다(비대화형).
        if Command::new(bare_name)
            .arg("--version")
            .silent()
            .output()
            .is_ok()
        {
            return Some(bare_name.to_string());
        }
    }
    None
}

/// `candidates`의 플랫폼별 토큰(mac `~/`, windows `%APPDATA%` 등)을
/// `platform::expand_path_tokens`로 확장한 뒤 `resolve_binary`에 넘기는 얇은
/// 래퍼. `resolve_binary`의 시그니처(`&[&str]`)를 바꾸지 않고 GUI PATH 문제
/// 해법을 그대로 재사용하기 위함(개발 환경 실설치/업데이트 결정 5.5 —
/// claude/pnpm 후보 경로에 `~`가 필요하다). macOS 분기는 기존
/// `dirs::home_dir()` 기반 `~/` 확장과 동일한 값을 낸다.
pub fn resolve_binary_expand_home(candidates: &[&str], bare_name: &str) -> Option<String> {
    let plat = platform::platform_now();
    let roots = platform::EnvRoots::from_env();
    let expanded: Vec<String> = candidates
        .iter()
        .filter_map(|c| platform::expand_path_tokens(plat, c, &roots))
        .collect();
    let refs: Vec<&str> = expanded.iter().map(|s| s.as_str()).collect();
    resolve_binary(&refs, bare_name)
}

/// 프론트엔드에 그대로 돌려줄 결과 — "앱이 대신 로그인/로그아웃을 실행했는가"가
/// 아니라 "사용자가 조작할 터미널 창을 여는 데 성공했는가"를 뜻한다.
#[derive(serde::Serialize, Clone, Debug)]
pub struct TerminalLaunchResult {
    pub opened: bool,
    pub message: String,
}

/// macOS: Terminal.app 새 창에서, Windows: PowerShell 새 콘솔 창에서
/// `shell_command`를 실행한다. 두 플랫폼 모두 앱이 자식 프로세스로 조용히
/// spawn해 출력을 캡처하는 것과 달리 사용자가 그 창을 직접 보고 입력(예: 브라우저
/// 인증 코드 확인, 2FA)할 수 있는 진짜 대화형 세션을 연다 — RFC 8252가 전제하는
/// 것과 같은 상호작용 방식이다.
///
/// `shell_command`는 이 모듈 안에서 절대경로로 해석된 신뢰 가능한 바이너리 + 고정
/// 서브커맨드 문자열, 또는 설치 안내 테이블의 리터럴 문자열만 넘어온다(사용자
/// 자유입력 없음) — 그럼에도 macOS AppleScript 문자열 이스케이프는 방어적으로
/// 처리한다(Windows는 PowerShell에 문자열을 그대로 넘기고 별도 이스케이프가
/// 필요 없다 — `-Command` 인자 자체가 argv 배열 원소이지 셸이 다시 파싱하는
/// 텍스트가 아니다).
///
/// 두 플랫폼 모두 의도적으로 `SilentCommand::silent()`를 쓰지 않는다 — 이
/// 파일의 다른 호출부(`resolve_binary`)와 달리, 애초에 이 창을 사용자가 보고
/// 조작하게 하는 것이 목적이라 "조용히" 실행하면 기능 자체가 무너진다. Windows는
/// 대신 `.windowed()`로 `CREATE_NEW_CONSOLE`을 **명시**한다(암묵적 콘솔 자동
/// 생성에 기대지 않는다 — 설계 §E.1). Windows 분기는 이 머신에서 실행
/// 검증이 불가능하다(미검증 — 실기 Windows PC 필요, 설계 §8 항목 5).
pub fn open_terminal_command(shell_command: &str) -> Result<(), String> {
    match platform::platform_now() {
        Platform::Win => Command::new("powershell.exe")
            .args(["-NoLogo", "-NoExit", "-Command", shell_command])
            .windowed()
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("터미널을 여는 데 실패했습니다: {e}")),
        Platform::Mac => {
            // 기존 그대로(변경 없음).
            let escaped = shell_command.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!(
                "tell application \"Terminal\"\n  activate\n  do script \"{escaped}\"\nend tell"
            );
            Command::new("osascript")
                .arg("-e")
                .arg(script)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("터미널을 여는 데 실패했습니다: {e}"))
        }
    }
}

/// 구조화된 진입점(설계 §E.3) — `open_terminal_command`가 문자열을 조립해
/// 받는 것과 달리 프로그램 경로와 인자를 분리된 값으로 받는다. Windows 경로에
/// 공백이 있으면(`C:\Program Files\...`) 문자열 `format!()` 조립이 그대로
/// 깨지는 문제를 구조적으로 없앤다. 인용은 `platform::quote_token`/
/// `build_terminal_command_line`이 담당한다 — macOS 출력은 무인용 규칙 덕에
/// 기존 `format!("{program} {args}")` 문자열과 바이트 단위로 동일하다(회귀
/// 없음).
pub fn open_terminal_program(program: &str, args: &[&str]) -> Result<(), String> {
    let plat = platform::platform_now();
    let line = platform::build_terminal_command_line(plat, program, args);
    open_terminal_command(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신(Mac)에서 실제로 설치된 도구 중 하나로 절대경로 탐지가 여전히
    // 동작하는지 확인한다(기존 6개 호출부가 신뢰하는 계약 — 플랫폼 파라미터화
    // 이후에도 macOS 결과가 바뀌면 안 된다).
    #[test]
    fn resolve_binary_finds_git_by_absolute_candidate_on_this_machine() {
        let candidates = [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/usr/bin/git",
        ];
        assert!(
            resolve_binary(&candidates, "git").is_some(),
            "이 머신에는 git이 세 후보 중 하나에 반드시 있어야 합니다"
        );
    }

    #[test]
    fn resolve_binary_returns_none_for_unknown_tool() {
        assert_eq!(
            resolve_binary(&["/definitely/not/a/real/path/tool"], "malgn_vscode_fake_tool_xyz"),
            None
        );
    }

    #[test]
    fn resolve_binary_expand_home_expands_tilde_candidates_on_this_machine() {
        // git 후보에는 `~/`가 없어 순수 절대경로 매칭만 검증하지만, 확장 자체가
        // 패닉 없이 동작하는지는 `~/`가 포함된 실제 DEV_TOOLS 후보(예: claude)로
        // 별도 확인한다 — 결과가 None이어도(이 머신에 없을 수 있음) 패닉하지
        // 않아야 한다는 계약만 검증한다.
        let candidates = ["~/.local/bin/claude", "/opt/homebrew/bin/claude"];
        let _ = resolve_binary_expand_home(&candidates, "claude");
    }
}
