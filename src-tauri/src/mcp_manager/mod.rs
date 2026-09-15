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

use crate::cli_launcher::{open_terminal_program, open_terminal_program_sequence, TerminalLaunchResult};
use add_args::{build_add_args, EnvVarPair};
use catalog::{build_catalog_list, find_catalog_entry, McpCatalogEntry, McpCatalogItem};
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

/// `claude mcp add ...`의 argv를 조립한다(부작용 없음 — claude 바이너리를
/// 건드리지 않고 터미널도 열지 않는다). B1(2라운드 차단): 토큰 배열로만
/// 반환하고 셸 문자열을 조립하지 않는다 — 인용은 전적으로
/// `open_terminal_program_sequence`(`platform::quote_token`) 몫이다. `entry`가
/// `MCP_CATALOG`의 고정 항목뿐이라 지금은 모든 슬롯이 신뢰 가능한 값이지만,
/// argv 배열 구조 자체가 향후 이 표에 외부 데이터가 섞여도 안전하다.
fn build_install_add_args(entry: &McpCatalogEntry) -> [&'static str; 8] {
    [
        "mcp",
        "add",
        "--scope",
        "user",
        "--transport",
        entry.transport,
        entry.label,
        entry.target,
    ]
}

/// `claude mcp login <label>`의 argv — `build_install_add_args`와 짝을 이뤄
/// `mcp_install`이 두 커맨드를 한 터미널 세션에서 이어 실행한다.
fn build_install_login_args(entry: &McpCatalogEntry) -> [&'static str; 3] {
    ["mcp", "login", entry.label]
}

/// catalog_id로 표에서 항목을 찾아 `claude mcp add` → `claude mcp login`을
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

    let add_args = build_install_add_args(entry);
    let login_args = build_install_login_args(entry);
    open_terminal_program_sequence(&[
        (claude_bin.as_str(), &add_args[..]),
        (claude_bin.as_str(), &login_args[..]),
    ])?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: format!("터미널 창에서 {} 설치 및 로그인 절차를 진행해주세요.", entry.label),
    })
}

/// 등록 후(특히 OAuth 방식) 로그인을 마치려면 `claude mcp login <name>`을
/// 실행해야 하는데, 브라우저 인증이 끼는 대화형 흐름이라 카탈로그 설치와
/// 동일하게 사용자가 직접 보는 터미널 창을 연다.
///
/// `mcp_install`의 `catalog_id`(Rust 코드에 고정된 label만 쓰는 불변식)와
/// 달리 이 커맨드의 `name`은 `claude mcp list` 파싱 결과다(`parsing.rs`) —
/// 실질적으로 `~/.claude.json`·프로젝트 `.mcp.json`에 적힌 서버 이름이라
/// 클론한 저장소가 채울 수 있는 외부 출처 데이터다(B1, 2라운드 차단). POSIX
/// 셸 인용 규칙만으로 이스케이프해 셸 문자열 하나로 조립하면, Windows
/// `powershell.exe -Command` 경로에서는 그 규칙이 무효라 인젝션 표면이 된다
/// — `open_terminal_program`에 argv 원소로 그대로 넘겨
/// `platform::quote_token`(플랫폼별 인용 규칙)이 전담하게 한다. 이 함수 안에는
/// 문자열 포맷팅이 전혀 없다.
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

    open_terminal_program(&claude_bin, &["mcp", "login", &name])?;
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

    // ---------------- build_install_add_args / build_install_login_args ----------------
    // B1(2라운드 차단): 셸 문자열을 직접 포맷팅해 조립하던 이전 라운드의 함수
    // 두 개(카탈로그 설치 커맨드 조립 + POSIX 전용 이스케이프)가 이 테스트들이
    // 있던 자리를 대체한다 — 둘 다 함수 자체가 삭제됐으므로(`open_terminal_command`
    // 시그니처가 `&'static str`로 좁아져 컴파일이 깨졌다), argv 배열 조립
    // 로직을 대신 회귀 고정한다. 실제 인용(POSIX/PowerShell 양쪽)의 안전성은
    // `dev_tools::platform`의 `quote_token`/`build_terminal_command_line`
    // 테스트가 실측(sh -c 왕복 포함)으로 검증한다 — 이 파일은 "어떤 토큰이
    // 몇 번째 argv 원소로 들어가는가"만 책임진다.

    #[test]
    fn build_install_add_args_fixes_scope_user_and_reads_transport_label_target_from_entry() {
        let entry = find_catalog_entry("gmail").unwrap();
        assert_eq!(
            build_install_add_args(entry),
            [
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "Gmail",
                "https://gmailmcp.googleapis.com/mcp/v1",
            ]
        );
    }

    #[test]
    fn build_install_add_args_keeps_label_with_parens_as_single_argv_element() {
        // 예전 build_install_command는 label을 `"..."`로 감싼 셸 문자열에 직접
        // interpolate했다 — 이제는 argv 배열의 원소 하나일 뿐이라 괄호·공백이
        // 있어도 별도 인용 처리가 필요 없다(인용은 실행 시점에
        // open_terminal_program_sequence가 담당).
        let entry = find_catalog_entry("atlassian").unwrap();
        let args = build_install_add_args(entry);
        assert_eq!(args[6], "Atlassian (Jira/Confluence)");
        assert_eq!(args.len(), 8, "argv 원소 하나 = label 전체(공백 포함)");
    }

    #[test]
    fn build_install_login_args_reuses_label_as_login_target() {
        let entry = find_catalog_entry("figma").unwrap();
        assert_eq!(build_install_login_args(entry), ["mcp", "login", "Figma"]);
    }
}
