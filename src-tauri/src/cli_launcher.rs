// ---------------- 외부 CLI(gh, wrangler) 위임 공통 유틸 ----------------
// GitHub·Cloudflare 연동은 자격증명을 이 앱이 직접 취급하지 않고 각 CLI에
// 위임한다. 실측으로 확인된 두 가지 제약을 여기서 한 번에 처리한다:
//
// 1) Finder로 띄운 .app은 launchctl 기본 PATH만 상속하고 그 PATH에 gh·wrangler가
//    없다(`launchctl getenv PATH` 비어있음, 실제 바이너리는 /opt/homebrew/bin 아래).
//    → `Command::new("gh")` 대신 절대경로 후보를 순서대로 확인한다.
// 2) 파이프로 연결된 자식 프로세스에는 TTY가 없다. `gh auth login`·`wrangler login`
//    처럼 사용자 입력을 기다리는 대화형 로그인은 앱이 조용히 spawn하지 않고,
//    사용자가 실제로 보고 조작할 수 있는 Terminal.app 창에서 실행되도록 연다.
//    (macOS 전용 — 이 프로젝트는 macOS 데스크톱 앱이다.)

use std::path::Path;
use std::process::Command;

/// 후보 절대경로를 순서대로 확인하고, 전부 없으면 PATH 안에 있을 가능성(예: 터미널에서
/// `pnpm tauri dev`로 실행한 개발 모드)에 대비해 이름 그대로도 한 번 시도한다.
/// 어느 쪽도 실행 가능하지 않으면 `None` — 이때 "미설치"는 예외가 아니라 정상 값이다.
pub fn resolve_binary(absolute_candidates: &[&str], bare_name: &str) -> Option<String> {
    for candidate in absolute_candidates {
        if Path::new(candidate).is_file() {
            return Some((*candidate).to_string());
        }
    }
    // PATH 폴백: 존재 여부만 확인하고(버전 출력 내용은 쓰지 않는다) 성공하면 이름을
    // 그대로 돌려준다. 여기서 실행하는 것은 --version 뿐이라 안전하다(비대화형).
    if Command::new(bare_name).arg("--version").output().is_ok() {
        return Some(bare_name.to_string());
    }
    None
}

/// 프론트엔드에 그대로 돌려줄 결과 — "앱이 대신 로그인/로그아웃을 실행했는가"가
/// 아니라 "사용자가 조작할 터미널 창을 여는 데 성공했는가"를 뜻한다.
#[derive(serde::Serialize, Clone, Debug)]
pub struct TerminalLaunchResult {
    pub opened: bool,
    pub message: String,
}

/// macOS Terminal.app 새 창에서 `shell_command`를 실행한다. `do script`가 실제로
/// 명령을 실행시키긴 하지만, 앱이 자식 프로세스로 조용히 spawn해 출력을 캡처하는
/// 것과 달리 사용자가 그 창을 직접 보고 입력(예: 브라우저 인증 코드 확인, 2FA)할
/// 수 있는 진짜 대화형 세션이다 — RFC 8252가 전제하는 것과 같은 상호작용 방식이다.
///
/// `shell_command`는 이 모듈 안에서 절대경로로 해석된 신뢰 가능한 바이너리 + 고정
/// 서브커맨드 문자열만 넘어온다(사용자 자유입력 없음) — AppleScript 문자열 이스케이프는
/// 그럼에도 방어적으로 처리한다.
pub fn open_terminal_command(shell_command: &str) -> Result<(), String> {
    let escaped = shell_command.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("tell application \"Terminal\"\n  activate\n  do script \"{escaped}\"\nend tell");
    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("터미널을 여는 데 실패했습니다: {e}"))
}
