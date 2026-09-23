// ---------------- GitHub 연동 (CLI 위임, 이 앱은 토큰을 취급하지 않는다) ----------------
// 자격증명은 전부 `gh` CLI 자신의 키체인 저장소가 들고 있다. 이 모듈은 상태를
// "읽기"만 하고(gh auth status / gh api user), 로그인·로그아웃처럼 인증 상태를
// 바꾸는 조작은 앱이 대신 실행하지 않는다 — cli_launcher::open_terminal_command로
// 사용자가 직접 보는 터미널 세션을 열어줄 뿐이다. `gh auth token`은 절대 호출하지
// 않는다(평문 토큰을 stdout으로 낸다).

use crate::cli_launcher::{open_terminal_program, resolve_binary, TerminalLaunchResult};
use crate::process_util::SilentCommand;
use serde::Serialize;
use serde_json::Value;
use std::process::Command;

const GH_CANDIDATES: [&str; 3] = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh", "/usr/bin/gh"];

/// `gh auth login`에 넘길 argv — 상수로 뽑아 테스트에서 그대로 고정한다(플래그가
/// 하나라도 빠지면 그 질문만 다시 대화형으로 돌아오므로, 실수로 빠지는 회귀를
/// 테스트가 잡는다). 각 플래그를 고른 근거는 `github_connect()` 주석 참고.
const GH_LOGIN_ARGS: [&str; 7] = ["auth", "login", "-h", "github.com", "-p", "https", "-w"];

fn resolve_gh() -> Option<String> {
    resolve_binary(&GH_CANDIDATES, "gh")
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct GithubStatus {
    pub installed: bool,
    pub connected: bool,
    pub login: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
}

/// ① 현재 연동 상태 조회. `gh auth status`의 종료코드로만 로그인 여부를 판단한다
/// (텍스트 출력 포맷은 gh 버전에 따라 바뀔 수 있어 파싱하지 않는다 — 종료코드는
/// gh가 공식적으로 문서화한 안정적인 신호다). 로그인 상태일 때만 `gh api user`로
/// 신뢰 가능한 JSON 프로필(login/name/avatar_url)을 덧붙인다.
#[tauri::command]
pub fn github_status() -> GithubStatus {
    let Some(gh) = resolve_gh() else {
        return GithubStatus {
            installed: false,
            ..Default::default()
        };
    };

    let logged_in = Command::new(&gh)
        .args(["auth", "status"])
        .silent()
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !logged_in {
        return GithubStatus {
            installed: true,
            connected: false,
            ..Default::default()
        };
    }

    let profile = Command::new(&gh)
        .args(["api", "user"])
        .silent()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok());

    let (login, name, avatar_url) = match profile {
        Some(v) => (
            v.get("login").and_then(|x| x.as_str()).map(String::from),
            v.get("name").and_then(|x| x.as_str()).map(String::from),
            v.get("avatar_url")
                .and_then(|x| x.as_str())
                .map(String::from),
        ),
        None => (None, None, None),
    };

    GithubStatus {
        installed: true,
        connected: true,
        login,
        name,
        avatar_url,
    }
}

/// ② 연결 시작. `gh auth login`을 앱이 대신 실행하지 않고, 사용자가 직접 보는
/// 터미널 창을 열어 그 창에서 로그인 절차(브라우저 인증 등)를 진행하게 한다.
#[tauri::command]
pub fn github_connect() -> Result<TerminalLaunchResult, String> {
    let Some(gh) = resolve_gh() else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "GitHub CLI(gh)가 설치되어 있지 않습니다. https://cli.github.com 에서 설치한 뒤 다시 시도해주세요.".to_string(),
        });
    };
    // 설계 §E.3: 구조화된 진입점으로 전환 — Windows 경로에 공백이 있으면
    // (`C:\Program Files\...`) 기존 `format!("{gh} auth login")` 문자열 조립이
    // 그대로 깨진다. macOS 출력은 무인용 규칙 덕에 바이트 단위로 동일하다(회귀
    // 없음).
    //
    // `-h/-p/-w` 세 플래그를 명시해 `gh auth login`이 원래 대화형으로 묻는 세
    // 질문(호스트 선택 → 프로토콜 선택 → 인증 방식 선택)을 생략시킨다(실측,
    // `gh auth login --help`, gh 2.100.0):
    //   -h github.com : 호스트 선택 질문 생략. 이 앱은 GitHub Enterprise를
    //     지원하지 않으므로(다른 호스트 후보가 코드 어디에도 없다) gh 자체
    //     기본값과 같은 github.com을 고정한다.
    //   -p https      : 프로토콜 선택 질문 생략. gh가 인터랙티브 흐름에서도
    //     먼저 권하는 기본값이며, ssh를 고르면 로컬에 SSH 키가 없을 때 새로
    //     생성·업로드하겠냐는 추가 프롬프트가 따라붙어 "질문 생략"이라는 이번
    //     요구를 다시 어긴다.
    //   -w            : 인증 방식 선택 질문 생략, 바로 브라우저 인증 플로우로
    //     진입(이 요청의 목표 그 자체).
    // 이 셋 중 하나라도 빠지면 그 질문만 다시 대화형으로 돌아온다 — 세
    // 플래그는 서로 독립적이다.
    open_terminal_program(&gh, &GH_LOGIN_ARGS)?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: "터미널 창에서 GitHub 로그인 절차를 진행해주세요.".to_string(),
    })
}

/// ③ 연결 해제. 같은 이유로 `gh auth logout`도 앱이 조용히 실행하지 않고 터미널
/// 창을 열어 사용자가 직접 확인하며 로그아웃하게 한다.
#[tauri::command]
pub fn github_disconnect() -> Result<TerminalLaunchResult, String> {
    let Some(gh) = resolve_gh() else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "GitHub CLI(gh)가 설치되어 있지 않습니다.".to_string(),
        });
    };
    open_terminal_program(&gh, &["auth", "logout"])?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: "터미널 창에서 GitHub 로그아웃 절차를 진행해주세요.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_tools::platform::{build_terminal_command_line, Platform};

    /// 세 플래그(`-h`/`-p`/`-w`)가 빠지지 않고 순서대로 조립되는지 고정한다 —
    /// 나중에 누가 하나를 빼면(예: 리팩터링 중 실수) 이 테스트가 대화형 질문이
    /// 되살아나는 회귀를 잡는다.
    #[test]
    fn gh_login_args_pins_non_interactive_flags() {
        assert_eq!(
            GH_LOGIN_ARGS,
            ["auth", "login", "-h", "github.com", "-p", "https", "-w"]
        );
    }

    /// Windows 분기에서도 각 인자가 개별 토큰으로 인용되어 그대로 전달되는지
    /// 확인한다 — 공백이 있는 gh 경로(`C:\Program Files\GitHub CLI\gh.exe`)를
    /// 가정해 문자열 조립이 아니라 구조화된 인자 경로를 타는지 검증한다
    /// (`open_terminal_program`이 내부적으로 위임하는 것과 동일한 순수함수).
    #[test]
    fn gh_login_args_survive_windows_quoting_with_spaces_in_program_path() {
        let program = r"C:\Program Files\GitHub CLI\gh.exe";
        let line = build_terminal_command_line(Platform::Win, program, &GH_LOGIN_ARGS);
        assert_eq!(
            line,
            "& 'C:\\Program Files\\GitHub CLI\\gh.exe' 'auth' 'login' '-h' 'github.com' '-p' 'https' '-w'"
        );
    }
}
