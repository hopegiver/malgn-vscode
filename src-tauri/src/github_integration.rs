// ---------------- GitHub 연동 (CLI 위임, 이 앱은 토큰을 취급하지 않는다) ----------------
// 자격증명은 전부 `gh` CLI 자신의 키체인 저장소가 들고 있다. 이 모듈은 상태를
// "읽기"만 하고(gh auth status / gh api user), 로그인·로그아웃처럼 인증 상태를
// 바꾸는 조작은 앱이 대신 실행하지 않는다 — cli_launcher::open_terminal_command로
// 사용자가 직접 보는 터미널 세션을 열어줄 뿐이다. `gh auth token`은 절대 호출하지
// 않는다(평문 토큰을 stdout으로 낸다).

use crate::cli_launcher::{open_terminal_command, resolve_binary, TerminalLaunchResult};
use serde::Serialize;
use serde_json::Value;
use std::process::Command;

const GH_CANDIDATES: [&str; 3] = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh", "/usr/bin/gh"];

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
        return GithubStatus { installed: false, ..Default::default() };
    };

    let logged_in = Command::new(&gh).args(["auth", "status"]).output().map(|o| o.status.success()).unwrap_or(false);
    if !logged_in {
        return GithubStatus { installed: true, connected: false, ..Default::default() };
    }

    let profile = Command::new(&gh)
        .args(["api", "user"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok());

    let (login, name, avatar_url) = match profile {
        Some(v) => (
            v.get("login").and_then(|x| x.as_str()).map(String::from),
            v.get("name").and_then(|x| x.as_str()).map(String::from),
            v.get("avatar_url").and_then(|x| x.as_str()).map(String::from),
        ),
        None => (None, None, None),
    };

    GithubStatus { installed: true, connected: true, login, name, avatar_url }
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
    open_terminal_command(&format!("{gh} auth login"))?;
    Ok(TerminalLaunchResult { opened: true, message: "터미널 창에서 GitHub 로그인 절차를 진행해주세요.".to_string() })
}

/// ③ 연결 해제. 같은 이유로 `gh auth logout`도 앱이 조용히 실행하지 않고 터미널
/// 창을 열어 사용자가 직접 확인하며 로그아웃하게 한다.
#[tauri::command]
pub fn github_disconnect() -> Result<TerminalLaunchResult, String> {
    let Some(gh) = resolve_gh() else {
        return Ok(TerminalLaunchResult { opened: false, message: "GitHub CLI(gh)가 설치되어 있지 않습니다.".to_string() });
    };
    open_terminal_command(&format!("{gh} auth logout"))?;
    Ok(TerminalLaunchResult { opened: true, message: "터미널 창에서 GitHub 로그아웃 절차를 진행해주세요.".to_string() })
}
