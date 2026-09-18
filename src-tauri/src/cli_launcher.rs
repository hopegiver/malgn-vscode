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
/// 않는다 — (정정, 2라운드: Windows `CreateProcessW`에 이름을 넘기지 않고
/// Rust std가 자체 해석하며, 그 검색 순서에 현재 작업 디렉터리(CWD)는
/// 포함되지 않는다. 실제 위험은 검색 순서 2번 "현재 실행 파일(이 앱)의
/// 디렉터리"다 — 이 앱의 Windows 배포물은 단일 포터블 exe라 보통 다운로드
/// 폴더에 놓이고, 거기 악성 `npm.exe` 등이 있으면 bare-name spawn이 그것을
/// 실행할 수 있다). 그래서 Windows는 bare-name으로 프로세스를 스폰(프로브)조차
/// 하지 않고, 실제 PATH를 명시적으로 스캔해 절대경로를 확정한 뒤에만
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

/// `open_terminal_command`/`open_terminal_program`/`open_terminal_program_sequence_with_env`
/// 세 공개 진입점이 최종적으로 위임하는 실제 OS 스폰 로직(사설) — macOS:
/// Terminal.app 새 창, Windows: PowerShell 새 콘솔 창에서 `shell_command`를
/// 실행한다. 두 플랫폼 모두 앱이 자식 프로세스로 조용히 spawn해 출력을
/// 캡처하는 것과 달리 사용자가 그 창을 직접 보고 입력(예: 브라우저 인증 코드
/// 확인, 2FA)할 수 있는 진짜 대화형 세션을 연다 — RFC 8252가 전제하는 것과
/// 같은 상호작용 방식이다.
///
/// 두 플랫폼 모두 의도적으로 `SilentCommand::silent()`를 쓰지 않는다 — 이
/// 파일의 다른 호출부(`resolve_binary`)와 달리, 애초에 이 창을 사용자가 보고
/// 조작하게 하는 것이 목적이라 "조용히" 실행하면 기능 자체가 무너진다. Windows는
/// 대신 `.windowed()`로 `CREATE_NEW_CONSOLE`을 **명시**한다(암묵적 콘솔 자동
/// 생성에 기대지 않는다 — 설계 §E.1). B2(2라운드 차단): `powershell.exe`를
/// bare-name으로 스폰하지 않는다 — `platform::windows_system_tool`로
/// `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` 절대경로를
/// 조립해 실행한다(포터블 exe 배포물을 다운로드 폴더에 두고 쓰는 이 앱의
/// 실제 배포 형태에서 "현재 실행 파일의 디렉터리"에 동명의 악성 실행파일이
/// 놓이는 표면을 차단한다). `-NoProfile`도 함께 준다 — PowerShell 프로필
/// (`%USERPROFILE%\Documents\WindowsPowerShell\profile.ps1`)은 사용자 쓰기
/// 가능하고 OneDrive 동기화 대상인 경우가 흔한데, 하필 이 창은 gh/wrangler
/// 인증 흐름이 뜨는 창이다. Windows 분기는 이 머신에서 실행 검증이
/// 불가능하다(미검증 — 실기 Windows PC 필요, 설계 §8 항목 5).
fn spawn_terminal_window(shell_command: &str) -> Result<(), String> {
    match platform::platform_now() {
        Platform::Win => {
            let roots = platform::EnvRoots::from_env();
            let powershell =
                platform::windows_system_tool(&roots, r"System32\WindowsPowerShell\v1.0\powershell.exe");
            // M3(review-devtools-windows-parity-2026-09-15.md): `-Command
            // shell_command` 대신 `-EncodedCommand`(base64 UTF-16LE)로
            // 넘긴다(platform::encode_powershell_command 참고) — 이미
            // quote_token으로 인용된 스크립트 텍스트를 그대로 인코딩만
            // 감싸 전달한다(인용 규칙 자체는 무변경).
            let encoded = platform::encode_powershell_command(shell_command);
            Command::new(powershell)
                .args(["-NoLogo", "-NoProfile", "-NoExit", "-EncodedCommand", &encoded])
                .windowed()
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("터미널을 여는 데 실패했습니다: {e}"))
        }
        Platform::Mac => {
            // 기존 그대로(변경 없음) — 인용 순서는 반드시 "셸 인용(quote_token
            // 이 이미 적용된 shell_command) → 그 결과 전체를 AppleScript
            // 이스케이프"다. 순서를 뒤집으면 백슬래시가 어긋난다.
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

/// B1(2라운드 차단): 외부 출처 문자열(예: MCP 서버 이름 — `claude mcp list`
/// 파싱 결과, 클론한 저장소의 `.mcp.json`이 채울 수 있다)이 이 경로로 조립되어
/// 들어오면 POSIX 인용 규칙(mac)과 PowerShell 인용 규칙(win)이 어긋나는
/// 인젝션 표면이 생긴다(macOS는 지금 방어가 맞게 작동하지만, Windows 분기를
/// 켜는 순간 같은 데이터에 대해 방어가 무효가 된다 — PowerShell은 POSIX
/// 작은따옴표 규칙을 따르지 않는다). 시그니처를 `&'static str`로 좁혀
/// "리터럴만 이 경로로 간다"는 전제를 주석이 아니라 **타입**으로 강제한다 —
/// 동적으로 조립해야 하는 호출부(예: mcp_install/mcp_login)는 컴파일이 깨져
/// 구조적으로 `open_terminal_program`/`open_terminal_program_sequence_with_env`
/// (인용 책임이 `platform::quote_token` 한 곳으로 모이는 구조화된 진입점)로
/// 이관된다.
pub fn open_terminal_command(shell_command: &'static str) -> Result<(), String> {
    spawn_terminal_window(shell_command)
}

/// 구조화된 진입점(설계 §E.3) — `open_terminal_command`가 리터럴 문자열만
/// 받는 것과 달리 프로그램 경로와 인자를 분리된 값(둘 다 런타임에 동적으로
/// 조립 가능)으로 받는다. Windows 경로에 공백이 있으면(`C:\Program Files\...`)
/// 문자열 `format!()` 조립이 그대로 깨지는 문제를 구조적으로 없앤다. 인용은
/// `platform::quote_token`/`build_terminal_command_line`이 전담한다 — macOS
/// 출력은 무인용 규칙 덕에 기존 `format!("{program} {args}")` 문자열과
/// 바이트 단위로 동일하다(회귀 없음).
pub fn open_terminal_program(program: &str, args: &[&str]) -> Result<(), String> {
    let plat = platform::platform_now();
    let line = platform::build_terminal_command_line(plat, program, args);
    spawn_terminal_window(&line)
}

/// `open_terminal_program`의 다중 커맨드 버전 — `commands`(각 원소는
/// `(program, args)`)를 순서대로 실행하는 체인 명령을 한 터미널 세션에서
/// 연다(예: `claude mcp add` → `claude mcp login`, B1). 각 커맨드의 인용도
/// 동일하게 `platform::quote_token`이 전담한다.
/// 커맨드별 환경변수를 함께 실행하는 다중 커맨드 체인(2026-09-18 도입). 이
/// 함수를 거치는 유일한 호출부는 `mcp_manager::mcp_install`이다 — 카탈로그
/// 원클릭 설치가 `claude mcp add --client-secret`에 OAuth 비밀값을 넘겨야
/// 하는데, 이 CLI 플래그는 argv 값을 받지 않는 bare 플래그라(실측,
/// `dev_tools::platform::build_terminal_command_line_with_env` 문서 참조)
/// 자식 프로세스 환경변수(`MCP_CLIENT_SECRET`)로만 값을 넘길 수 있다. 이
/// 경로는 Rust가 `claude`를 직접 spawn하지 않고 osascript/powershell.exe가 새
/// 터미널 창 안에서 해석할 셸 문자열을 만들 뿐이라 `Command::env()`가 닿지
/// 않는다 — 대신 셸 문자열 안에 인라인 env 접두사를 넣어야 하고, 그 조립은
/// 전적으로 `platform::build_chained_terminal_command_line_with_env`(POSIX
/// 인라인 대입 vs PowerShell `$env:` 대입+해제)가 전담한다. env가 필요 없는
/// 커맨드는 빈 슬라이스(`&[]`)를 넘긴다.
pub fn open_terminal_program_sequence_with_env(
    commands: &[platform::TerminalCommandWithEnv],
) -> Result<(), String> {
    let plat = platform::platform_now();
    let line = platform::build_chained_terminal_command_line_with_env(plat, commands);
    spawn_terminal_window(&line)
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
