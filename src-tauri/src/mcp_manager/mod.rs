// ---------------- MCP 관리 ----------------
// `claude mcp list`/`get`/`add`/`remove` 서브커맨드는 모델을 호출하지 않는
// 순수 연결 헬스체크다(토큰 비용이 있는 `claude -p`와 다르다) — 그대로
// 셸 없이 실행해서 감싼다.
//
// 프로세스 실행 불변식은 `autonomy.rs`/`lib.rs`의 `run_claude_command`와 동일한
// 계약이다: `std::process::Command::new`로 셸을 거치지 않고 직접 실행하므로
// 사용자가 입력한 name/url/command/args를 그대로 인자로 넘겨도 셸 인젝션
// 경로가 없다(자율업무 구현 때 이미 검증된 불변식과 동일).
//
// `cwd`는 항상 `dirs::home_dir()`로 고정한다 — 특정 프로젝트의 `.mcp.json`이
// 우연히 섞여 그 프로젝트에만 있는 서버가 노출/누락되는 것을 방지한다.

mod add_args;
mod catalog;
mod parsing;
mod process;

use crate::cli_launcher::{open_terminal_command, TerminalLaunchResult};
use add_args::{build_add_args, EnvVarPair};
use catalog::{build_catalog_list, build_install_command, find_catalog_entry, McpCatalogItem};
use parsing::{parse_mcp_get, parse_mcp_list, McpServerDetail, McpServerSummary};
use process::{run_mcp_command, run_mcp_command_with_env, CLAUDE_PATH_CANDIDATES};

// ---------------- Tauri 커맨드 ----------------

/// 실패해도(claude 미설치 등) 빈 목록을 돌려준다 — 커맨드 시그니처가
/// `Result`가 아닌 `Vec`으로 고정돼 있다(프론트가 "0개 등록됨"과 "조회
/// 실패"를 구분할 필요가 없는 화면이라는 설계 결정).
fn mcp_list_blocking() -> Vec<McpServerSummary> {
    match run_mcp_command(&["mcp", "list"]) {
        Ok((text, _success)) => parse_mcp_list(&text),
        Err(_) => Vec::new(),
    }
}

/// 예전엔 sync였다(P0 버그와 동일 계열 — non-async 커맨드는 메인 스레드에서
/// 돈다). `dev_tools.rs`의 `check_dev_tools`가 정한 관용구(async +
/// `spawn_blocking`)를 그대로 따른다 — 프론트 `invoke()` 계약은 항상
/// Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn mcp_list() -> Vec<McpServerSummary> {
    tauri::async_runtime::spawn_blocking(mcp_list_blocking)
        .await
        .unwrap_or_default()
}

fn mcp_get_blocking(name: String) -> Result<McpServerDetail, String> {
    let (text, success) = run_mcp_command(&["mcp", "get", &name])?;
    if !success {
        return Err(if text.trim().is_empty() {
            format!("'{name}' MCP 서버 정보를 가져오지 못했습니다.")
        } else {
            text.trim().to_string()
        });
    }
    parse_mcp_get(&name, &text)
        .ok_or_else(|| format!("'{name}' MCP 서버 정보를 해석하지 못했습니다."))
}

#[tauri::command]
pub async fn mcp_get(name: String) -> Result<McpServerDetail, String> {
    tauri::async_runtime::spawn_blocking(move || mcp_get_blocking(name))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// `oauth_client_id`/`oauth_client_secret`/`oauth_callback_port`는 http/sse
/// 사내 OAuth MCP 등록에만 쓰인다(stdio는 `build_add_args`가 무시한다).
/// `oauth_client_secret`이 `Some`이면 그 프로세스 실행에만
/// `MCP_CLIENT_SECRET` 환경변수를 주입한다 — 값은 CLI 인자로도, 우리
/// 코드의 로그/에러 메시지로도 별도로 출력하지 않는다(실패 시 반환되는
/// stderr 텍스트에 CLI가 자체적으로 비밀값을 echo하는 경우는 이 코드가
/// 막을 수 없는 CLI 쪽 동작이다).
#[allow(clippy::too_many_arguments)]
fn mcp_add_blocking(
    name: String,
    transport: String,
    target: String,
    args: Vec<String>,
    header: Option<String>,
    env: Vec<EnvVarPair>,
    oauth_client_id: Option<String>,
    oauth_client_secret: Option<String>,
    oauth_callback_port: Option<u16>,
) -> Result<(), String> {
    let cmd_args = build_add_args(
        &name,
        &transport,
        &target,
        &args,
        &header,
        &env,
        &oauth_client_id,
        &oauth_client_secret,
        &oauth_callback_port,
    )?;
    let arg_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let (text, success) = match &oauth_client_secret {
        Some(secret) => run_mcp_command_with_env(&arg_refs, Some(("MCP_CLIENT_SECRET", secret)))?,
        None => run_mcp_command(&arg_refs)?,
    };
    if success {
        Ok(())
    } else if text.trim().is_empty() {
        Err("MCP 서버 추가에 실패했습니다.".to_string())
    } else {
        Err(text.trim().to_string())
    }
}

#[tauri::command]
pub async fn mcp_add(
    name: String,
    transport: String,
    target: String,
    args: Vec<String>,
    header: Option<String>,
    env: Vec<EnvVarPair>,
    oauth_client_id: Option<String>,
    oauth_client_secret: Option<String>,
    oauth_callback_port: Option<u16>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        mcp_add_blocking(
            name,
            transport,
            target,
            args,
            header,
            env,
            oauth_client_id,
            oauth_client_secret,
            oauth_callback_port,
        )
    })
    .await
    .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

fn mcp_remove_blocking(name: String) -> Result<(), String> {
    let (text, success) = run_mcp_command(&["mcp", "remove", &name])?;
    if success {
        Ok(())
    } else if text.trim().is_empty() {
        Err(format!("'{name}' MCP 서버 삭제에 실패했습니다."))
    } else {
        Err(text.trim().to_string())
    }
}

#[tauri::command]
pub async fn mcp_remove(name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || mcp_remove_blocking(name))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// 고정 카탈로그(`MCP_CATALOG`) 5개를 반환하되, `claude mcp list` 결과와
/// target URL로 대조해 이미 등록된 항목은 `installed: true`로 표시한다.
/// `mcp_list`와 동일하게 조회 실패는 "0개 설치됨"으로 느슨하게 처리한다
/// (claude 미설치 상태에서도 카탈로그 자체는 항상 보여줘야 하는 화면이다).
fn mcp_catalog_list_blocking() -> Vec<McpCatalogItem> {
    let installed_targets: Vec<String> =
        mcp_list_blocking().into_iter().map(|s| s.target).collect();
    build_catalog_list(&installed_targets)
}

#[tauri::command]
pub async fn mcp_catalog_list() -> Vec<McpCatalogItem> {
    tauri::async_runtime::spawn_blocking(mcp_catalog_list_blocking)
        .await
        .unwrap_or_default()
}

/// catalog_id로 표에서 항목을 찾아 `claude mcp add && claude mcp login`을
/// 이어 실행하는 터미널 창을 연다. 알 수 없는 catalog_id는 프론트가 임의
/// 문자열을 보낼 수 없다는 불변식을 지키기 위해 즉시 에러로 거절한다
/// (claude 바이너리 탐색보다 먼저 검사 — 미지 id는 claude 설치 여부와
/// 무관하게 항상 즉시 실패해야 한다).
#[tauri::command]
pub fn mcp_install(catalog_id: String) -> Result<TerminalLaunchResult, String> {
    let entry = find_catalog_entry(&catalog_id).ok_or_else(|| "알 수 없는 카탈로그 항목입니다".to_string())?;

    let Some(claude_bin) =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
    else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                .to_string(),
        });
    };

    let command = build_install_command(&claude_bin, entry);
    open_terminal_command(&command)?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: format!("터미널 창에서 {} 설치 및 로그인 절차를 진행해주세요.", entry.label),
    })
}

/// POSIX 셸 싱글쿼트 이스케이프 — `s`에 들어있는 모든 `'`를 `'\''`로 치환한
/// 뒤 전체를 `'...'`로 감싼다. `mcp_login`의 `name`처럼 사용자가 자유
/// 입력한 문자열을 `open_terminal_command`가 만드는 AppleScript `do script`
/// 문자열(그 안에서 다시 사용자의 로그인 셸로 실행된다)에 안전하게
/// 끼워넣기 위한 순수함수다. 이 함수를 거치지 않고 `name`을 문자열
/// 포맷팅에 직접 넣으면 셸 메타문자 인젝션으로 이어질 수 있다.
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 등록 후(특히 OAuth 방식) 로그인을 마치려면 `claude mcp login <name>`을
/// 실행해야 하는데, 브라우저 인증이 끼는 대화형 흐름이라 카탈로그 설치와
/// 동일하게 `open_terminal_command`로 사용자가 직접 보는 터미널 창을 연다.
///
/// `mcp_install`의 `catalog_id`(Rust 코드에 고정된 label만 쓰는 불변식)와
/// 달리 이 커맨드의 `name`은 사용자가 자유롭게 입력한 내부 서버 이름이다
/// — 반드시 `shell_single_quote`로 이스케이프한 뒤에만 명령 문자열에
/// 넣는다(이 함수 없이 직접 포맷팅하지 않는다).
#[tauri::command]
pub fn mcp_login(name: String) -> Result<TerminalLaunchResult, String> {
    let Some(claude_bin) =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
    else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                .to_string(),
        });
    };

    let command = format!("{claude_bin} mcp login {}", shell_single_quote(&name));
    open_terminal_command(&command)?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: format!("터미널 창에서 '{name}' 로그인 절차를 진행해주세요."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_install_rejects_unknown_catalog_id_without_touching_claude() {
        // catalog_id 검사가 claude 바이너리 탐색보다 먼저이므로, 이 머신에
        // claude가 설치돼 있는지와 무관하게 항상 Err여야 한다.
        let result = mcp_install("not-a-real-catalog-id".to_string());
        assert_eq!(result.unwrap_err(), "알 수 없는 카탈로그 항목입니다");
    }

    // ---------------- shell_single_quote ----------------

    #[test]
    fn shell_single_quote_wraps_plain_name_in_single_quotes() {
        assert_eq!(shell_single_quote("my-internal-mcp"), "'my-internal-mcp'");
    }

    #[test]
    fn shell_single_quote_wraps_name_with_spaces() {
        assert_eq!(
            shell_single_quote("my internal mcp"),
            "'my internal mcp'"
        );
    }

    #[test]
    fn shell_single_quote_escapes_malicious_shell_metacharacters() {
        // 표준 POSIX 싱글쿼트 이스케이프: 각 `'`를 `'\''`로 치환한다.
        // `'; rm -rf ~ #` -> `'\''; rm -rf ~ #`를 전체 싱글쿼트로 감싼 결과.
        let malicious = "'; rm -rf ~ #";
        let escaped = shell_single_quote(malicious);
        assert_eq!(escaped, "''\\''; rm -rf ~ #'");
        // 결과 문자열을 셸이 그대로 파싱하면 원래 문자열이 안전하게
        // 하나의 인자로 복원된다는 것을 sh -c로 직접 검증한다.
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {escaped}"))
            .output()
            .expect("sh must be available to verify the escape round-trips");
        assert_eq!(String::from_utf8_lossy(&output.stdout), malicious);
    }

    #[test]
    fn shell_single_quote_handles_multiple_embedded_quotes() {
        let input = "it's a 'test'";
        let escaped = shell_single_quote(input);
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {escaped}"))
            .output()
            .expect("sh must be available to verify the escape round-trips");
        assert_eq!(String::from_utf8_lossy(&output.stdout), input);
    }

    #[test]
    fn mcp_login_message_and_command_never_panic_on_malicious_name() {
        // 실제 claude 바이너리/터미널을 건드리지 않고 shell_single_quote를
        // 통해 안전하게 이스케이프된 명령 문자열이 만들어지는지 확인한다
        // (mcp_login 자체는 claude 바이너리 탐색·터미널 오픈이라는 부작용이
        // 있어 여기서는 순수 조립 로직만 shell_single_quote로 회귀 고정한다).
        let name = "'; rm -rf ~ #";
        let command = format!("/opt/homebrew/bin/claude mcp login {}", shell_single_quote(name));
        assert_eq!(
            command,
            "/opt/homebrew/bin/claude mcp login ''\\''; rm -rf ~ #'"
        );
    }
}
