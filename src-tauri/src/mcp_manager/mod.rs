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

use crate::cli_launcher::{
    open_terminal_program, open_terminal_program_sequence_with_env, TerminalLaunchResult,
};
use add_args::{build_add_args, EnvVarPair};
use catalog::{build_catalog_list, find_catalog_entry, McpCatalogEntry, McpCatalogItem};
use parsing::{is_claude_ai_account_connector, parse_mcp_get, parse_mcp_list, McpServerDetail, McpServerSummary};
use process::{run_mcp_command, run_mcp_command_with_env, CLAUDE_PATH_CANDIDATES};

// ---------------- Tauri 커맨드 ----------------

/// 실패해도(claude 미설치 등) 빈 목록을 돌려준다 — 커맨드 시그니처가
/// `Result`가 아닌 `Vec`으로 고정돼 있다(프론트가 "0개 등록됨"과 "조회
/// 실패"를 구분할 필요가 없는 화면이라는 설계 결정). claude.ai 계정 커넥터를
/// 포함한 원본 그대로 — 걸러낸 목록은 `mcp_list_for_display_blocking`이
/// 따로 감싼다.
fn mcp_list_blocking() -> Vec<McpServerSummary> {
    match run_mcp_command(&["mcp", "list"]) {
        Ok((text, _success)) => parse_mcp_list(&text),
        Err(_) => Vec::new(),
    }
}

/// `mcp_list_blocking`의 원본에서 claude.ai 계정 커넥터만 뺀다. 이 서버들은
/// 로컬 CLI로 로그인/로그아웃/삭제가 안 통해서(parsing.rs의
/// `is_claude_ai_account_connector` 문서 참조) 눌러볼 액션이 전부 오류로
/// 끝난다 — "등록된 서버" 화면 목록과 카탈로그의 "이미 설치됨" 판정 둘 다
/// 이 필터링된 목록을 쓴다. 카탈로그 쪽 이유: claude.ai 계정 커넥터(예: Gmail)와
/// 카탈로그의 "일반 MCP" 항목(`claude mcp add`로 직접 등록, 로그인/로그아웃/삭제
/// 가능)은 target URL이 우연히 같아도 서로 다른 등록이다 — 계정 커넥터가
/// 연결돼 있다는 이유로 "일반 MCP로 설치"를 막으면 안 된다(사용자가 둘을
/// 구분해서 쓰고 싶어할 수 있다).
fn mcp_list_for_display_blocking() -> Vec<McpServerSummary> {
    mcp_list_blocking()
        .into_iter()
        .filter(|s| !is_claude_ai_account_connector(&s.name))
        .collect()
}

/// 예전엔 sync였다(P0 버그와 동일 계열 — non-async 커맨드는 메인 스레드에서
/// 돈다). `dev_tools.rs`의 `check_dev_tools`가 정한 관용구(async +
/// `spawn_blocking`)를 그대로 따른다 — 프론트 `invoke()` 계약은 항상
/// Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn mcp_list() -> Vec<McpServerSummary> {
    tauri::async_runtime::spawn_blocking(mcp_list_for_display_blocking)
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

/// `claude mcp logout <name>` — 저장된 OAuth 자격증명만 지운다(서버 등록
/// 자체는 유지, `mcp_remove`와 다르다). 브라우저 인증이 없는 로컬 삭제라
/// `mcp_login`처럼 터미널을 열 필요가 없다 — `mcp_remove`와 동일한 방식으로
/// 직접 blocking 실행한다. claude.ai 커넥터(계정 단위 연결)처럼 "평소엔
/// 해제해두고 필요할 때만 연결"하려는 사용자를 위한 버튼의 백엔드다.
fn mcp_logout_blocking(name: String) -> Result<(), String> {
    let (text, success) = run_mcp_command(&["mcp", "logout", &name])?;
    if success {
        Ok(())
    } else if text.trim().is_empty() {
        Err(format!("'{name}' 연결 해제에 실패했습니다."))
    } else {
        Err(text.trim().to_string())
    }
}

#[tauri::command]
pub async fn mcp_logout(name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || mcp_logout_blocking(name))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// 고정 카탈로그(`MCP_CATALOG`) 5개를 반환하되, `claude mcp list` 결과와
/// target URL로 대조해 이미 등록된 항목은 `installed: true`로 표시한다.
/// `mcp_list`와 동일하게 조회 실패는 "0개 설치됨"으로 느슨하게 처리한다
/// (claude 미설치 상태에서도 카탈로그 자체는 항상 보여줘야 하는 화면이다).
fn mcp_catalog_list_blocking() -> Vec<McpCatalogItem> {
    let installed_targets: Vec<String> = mcp_list_for_display_blocking()
        .into_iter()
        .map(|s| s.target)
        .collect();
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
/// `open_terminal_program_sequence_with_env`(`platform::quote_token`) 몫이다. `entry`가
/// `MCP_CATALOG`의 고정 항목뿐이라 지금은 모든 슬롯이 신뢰 가능한 값이지만,
/// argv 배열 구조 자체가 향후 이 표에 외부 데이터가 섞여도 안전하다.
///
/// `entry.oauth_client_id`/`oauth_client_secret`이 `Some`이면(현재는
/// gmail/google-drive/google-calendar 3개가 공유하는 값, `catalog.rs` 문서
/// 참조) `--client-id`를 값째로, `--client-secret`은 **bare 플래그만**
/// 덧붙인다 — 길이가 고정 8이 아니게 되어 반환 타입을 `Vec`로 바꿨다.
///
/// 정정(2026-09-18, 실측): 이전 라운드는 이 자리에 `--client-secret
/// <값>`처럼 비밀값을 argv로 그대로 붙였는데, 실제로 `claude mcp add
/// --help`(버전 2.1.272)를 실행해 확인한 결과 `--client-secret`은 **값을
/// 받지 않는 bare 플래그**다("Prompt for OAuth client secret (or set
/// MCP_CLIENT_SECRET env var)") — 뒤에 붙인 리터럴 값은 그냥 버려지고
/// 터미널이 실제로 인터랙티브 프롬프트를 띄운다(`MCP_CLIENT_SECRET` 환경변수
/// 없이 비-TTY로 실행하면 "No TTY available to prompt for client secret. Set
/// MCP_CLIENT_SECRET env var instead."로 즉시 실패하는 것도 실측 확인 —
/// 즉 이전 구현은 "보안 노출 트레이드오프"가 아니라 **애초에 동작하지
/// 않는** 코드였다). 반대로 `MCP_CLIENT_SECRET` 환경변수를 심고 bare
/// `--client-secret`만 주면 프롬프트 없이 값이 저장되고(`claude mcp get`의
/// "client_secret configured" 출력으로 확인), 뒤이은 `claude mcp login`도
/// 그 저장된 값을 재사용해 시크릿을 다시 묻지 않는다(둘 다 실제
/// `claude mcp add`/`get`/`login` 실행으로 확인 — dummy HTTP 엔드포인트 사용,
/// 완료 후 `claude mcp remove`로 정리했다).
///
/// 이전 라운드가 보류했던 이유("인라인 env 접두사가 화면 노출을 줄이지
/// 못한다")는 여전히 유효하지만, 이제는 기능적으로 반드시 필요해졌다 —
/// `mcp_add_blocking`(직접 Rust 프로세스 spawn 경로)과 동일하게 bare
/// flag + 환경변수 주입 방식을 쓰되, 이 경로(`mcp_install` →
/// `open_terminal_program_sequence_with_env` → osascript/powershell.exe가 새
/// 터미널 창 안에서 해석할 셸 문자열)는 `Command::env()`가 그 터미널 세션
/// 안의 `claude` 프로세스까지 닿지 않으므로, 셸 문자열 자체에 인라인 env
/// 접두사(`MCP_CLIENT_SECRET=... claude ...`)를 넣는다. 플랫폼별 문법 차이
/// (POSIX `VAR=val cmd` vs PowerShell `$env:VAR='val'; cmd`)는
/// `platform::build_terminal_command_line_with_env`/
/// `build_chained_terminal_command_line_with_env`가 전담한다(B1과 동일하게
/// 인용 책임을 한 곳으로 모은다) — 비밀값은 `build_install_add_env`가 반환한
/// env 쌍으로만 전달되고 이 함수(`build_install_add_args`)의 반환값에는
/// 절대 등장하지 않는다.
fn build_install_add_args(entry: &McpCatalogEntry) -> Vec<&'static str> {
    let mut args = vec![
        "mcp",
        "add",
        "--scope",
        "user",
        "--transport",
        entry.transport,
        entry.id,
        entry.target,
    ];
    if let Some(client_id) = entry.oauth_client_id {
        args.push("--client-id");
        args.push(client_id);
    }
    if entry.oauth_client_secret.is_some() {
        // 값은 절대 argv에 넣지 않는다 — `claude mcp add --client-secret`은
        // bare 플래그라 값을 argv로 받지 않고(실측, 위 문서 참조), 실제
        // 비밀값은 `build_install_add_env`가 만드는 `MCP_CLIENT_SECRET`
        // 환경변수로만 전달한다.
        args.push("--client-secret");
    }
    args
}

/// `build_install_add_args`와 짝을 이뤄, 그 커맨드의 자식 프로세스에만 심을
/// 환경변수를 만든다. `oauth_client_secret`이 `Some`일 때만 `MCP_CLIENT_SECRET`
/// 하나를 반환한다 — `claude mcp add`의 `--client-secret`이 bare 플래그라 값을
/// 이 환경변수로만 받는다(2026-09-18 실측, `build_install_add_args` 문서
/// 참조).
fn build_install_add_env(entry: &McpCatalogEntry) -> Vec<(&'static str, &'static str)> {
    match entry.oauth_client_secret {
        Some(secret) => vec![("MCP_CLIENT_SECRET", secret)],
        None => Vec::new(),
    }
}

/// `claude mcp login <id>`의 argv — `build_install_add_args`와 짝을 이뤄
/// `mcp_install`이 두 커맨드를 한 터미널 세션에서 이어 실행한다. `claude mcp
/// add`가 등록한 서버 이름은 `entry.id`이므로(위 참조) 로그인도 같은 값을
/// 써야 한다 — `entry.label`(예: "Atlassian (Jira/Confluence)")은 공백·괄호가
/// 섞여 있어 "Invalid name"으로 거절된다.
fn build_install_login_args(entry: &McpCatalogEntry) -> [&'static str; 3] {
    ["mcp", "login", entry.id]
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
    let add_env = build_install_add_env(entry);
    let login_args = build_install_login_args(entry);
    open_terminal_program_sequence_with_env(&[
        (claude_bin.as_str(), &add_args[..], &add_env[..]),
        (claude_bin.as_str(), &login_args[..], &[]),
    ])?;

    let mut message = format!("터미널 창에서 {} 설치 및 로그인 절차를 진행해주세요.", entry.label);
    // gmail/google-drive/google-calendar 3개는 DCR(동적 클라이언트 등록)
    // 미지원 엔드포인트라 사내 OAuth 클라이언트가 없으면 인증이 항상
    // 실패한다(catalog.rs 문서 참조) — 실패를 겪기 전에 원인을 미리
    // 안내한다. atlassian/figma는 이 필드가 항상 None이라 영향받지 않는다
    // (id로 명시 분기해 실수로 걸리지 않게 한다).
    let is_google_dcr_endpoint =
        matches!(entry.id, "gmail" | "google-drive" | "google-calendar");
    if is_google_dcr_endpoint && entry.oauth_client_id.is_none() && entry.oauth_client_secret.is_none() {
        message.push_str(
            " (참고: 이 항목용 Google OAuth 클라이언트가 설정되지 않았습니다 — 이 Google MCP 엔드포인트는 동적 클라이언트 등록을 지원하지 않아 이 상태로는 인증이 실패합니다. src-tauri/.env에 GOOGLE_MCP_OAUTH_CLIENT_ID/GOOGLE_MCP_OAUTH_CLIENT_SECRET을 설정한 뒤 다시 빌드해주세요.)",
        );
    }
    Ok(TerminalLaunchResult {
        opened: true,
        message,
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
    fn build_install_add_args_fixes_scope_user_and_reads_transport_id_target_from_entry() {
        // atlassian은 oauth_client_id/secret이 항상 None(구조상 Google과
        // 무관 — DCR 지원 엔드포인트)이라 이 회귀 테스트가 build 환경(.env
        // 유무)과 무관하게 안정적으로 8개 고정 길이를 유지한다. gmail/
        // google-drive/google-calendar 3개는 로컬 .env의
        // GOOGLE_MCP_OAUTH_CLIENT_ID/SECRET 설정 여부에 따라 길이가
        // 달라지므로 이 테스트 대상에서 제외했다(아래 전용 테스트 참조).
        let entry = find_catalog_entry("atlassian").unwrap();
        assert_eq!(
            build_install_add_args(entry),
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "sse",
                "atlassian",
                "https://mcp.atlassian.com/v1/sse",
            ]
        );
    }

    #[test]
    fn build_install_add_args_uses_id_not_label_for_name_argv() {
        // `claude mcp add`는 이름에 letters/numbers/hyphens/underscores만
        // 허용한다("Invalid name" 에러) — label(공백·괄호 포함)이 아니라
        // 카탈로그 슬러그 id를 name 인자로 써야 한다.
        let entry = find_catalog_entry("atlassian").unwrap();
        let args = build_install_add_args(entry);
        assert_eq!(args[6], "atlassian");
        assert_eq!(args.len(), 8, "atlassian은 oauth 필드가 항상 None이라 원소 8개 고정");
    }

    #[test]
    fn build_install_add_args_appends_client_id_valued_and_client_secret_bare_when_entry_has_them() {
        // 실제 컴파일 환경의 GOOGLE_MCP_OAUTH_CLIENT_ID/SECRET 유무에 기대지
        // 않기 위해, gmail 카탈로그 엔트리를 그대로 쓰지 않고 동일 구조의
        // 임시 엔트리를 만들어 oauth 필드가 채워졌을 때의 조립 로직만
        // 검증한다(google-drive/google-calendar도 같은 필드를 공유하므로
        // 로직 자체는 이 하나의 테스트로 충분히 대표된다).
        //
        // 2026-09-18 실측 정정: `--client-secret`은 `claude mcp add --help`가
        // 명시한 대로 bare 플래그라 값을 argv로 받지 않는다 — 비밀값
        // "test-client-secret"은 여기 등장하지 않고 `build_install_add_env`가
        // 만드는 환경변수로만 전달된다.
        let entry = McpCatalogEntry {
            id: "gmail",
            label: "Gmail",
            transport: "http",
            target: "https://gmailmcp.googleapis.com/mcp/v1",
            oauth_client_id: Some("test-client-id"),
            oauth_client_secret: Some("test-client-secret"),
        };
        let args = build_install_add_args(&entry);
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "gmail",
                "https://gmailmcp.googleapis.com/mcp/v1",
                "--client-id",
                "test-client-id",
                "--client-secret",
            ]
        );
        assert!(
            !args.iter().any(|a| *a == "test-client-secret"),
            "비밀값이 argv에 그대로 노출되면 안 됩니다: {args:?}"
        );
    }

    #[test]
    fn build_install_add_env_returns_mcp_client_secret_pair_when_entry_has_secret() {
        let entry = McpCatalogEntry {
            id: "gmail",
            label: "Gmail",
            transport: "http",
            target: "https://gmailmcp.googleapis.com/mcp/v1",
            oauth_client_id: Some("test-client-id"),
            oauth_client_secret: Some("test-client-secret"),
        };
        assert_eq!(
            build_install_add_env(&entry),
            vec![("MCP_CLIENT_SECRET", "test-client-secret")]
        );
    }

    #[test]
    fn build_install_add_env_returns_empty_when_entry_has_no_secret() {
        let entry = McpCatalogEntry {
            id: "atlassian",
            label: "Atlassian",
            transport: "sse",
            target: "https://mcp.atlassian.com/v1/sse",
            oauth_client_id: None,
            oauth_client_secret: None,
        };
        assert!(build_install_add_env(&entry).is_empty());
    }

    #[test]
    fn build_install_add_args_omits_client_flags_when_entry_has_none() {
        let entry = McpCatalogEntry {
            id: "gmail",
            label: "Gmail",
            transport: "http",
            target: "https://gmailmcp.googleapis.com/mcp/v1",
            oauth_client_id: None,
            oauth_client_secret: None,
        };
        let args = build_install_add_args(&entry);
        assert!(!args.iter().any(|a| a.starts_with("--client")));
        assert_eq!(args.len(), 8);
    }

    #[test]
    fn build_install_login_args_reuses_id_as_login_target() {
        let entry = find_catalog_entry("figma").unwrap();
        assert_eq!(build_install_login_args(entry), ["mcp", "login", "figma"]);
    }
}

