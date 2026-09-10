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

use crate::cli_launcher::{open_terminal_command, TerminalLaunchResult};
use serde::{Deserialize, Serialize};

/// `dev_tools.rs`/`autonomy.rs`가 이미 정한 관례(결정 5.2 — 상수를 공유하지
/// 않고 각자 별도로 둔다) 그대로 이 모듈에도 독립적으로 둔다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct McpServerSummary {
    pub name: String,
    pub target: String,
    /// "stdio" | "http" | "sse" | "unknown"
    pub transport: String,
    pub connected: bool,
    #[serde(rename = "statusLabel")]
    pub status_label: String,
}

/// stdio 서버에 전달할 환경변수 한 쌍. 프론트에서 camelCase JSON(`{key,
/// value}`)으로 넘어온다. `key`가 빈 문자열이면 `build_add_args`가 방어적으로
/// 건너뛴다(값만 있고 키가 없는 입력 행 등).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct EnvVarPair {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct McpServerDetail {
    pub name: String,
    pub scope: String,
    pub status: String,
    pub transport: String,
    pub target: String,
    pub oauth: Option<String>,
}

// ---------------- 프로세스 실행 ----------------

/// `claude <args>`를 `resolve_binary_expand_home` + `build_child_path_env` +
/// `current_dir(home)`로 실행한다. 반환값은 (성공 시 stdout, 실패 시 stderr —
/// 둘 다 비어있으면 나머지 쪽, success 여부). 바이너리를 찾지 못하거나 홈
/// 디렉터리를 확인할 수 없으면 `Err`.
fn run_mcp_command(args: &[&str]) -> Result<(String, bool), String> {
    run_mcp_command_with_env(args, None)
}

/// `run_mcp_command`와 동일하되, `extra_env`가 `Some((key, value))`이면 그
/// 자식 프로세스에만 환경변수를 하나 추가로 주입한다(이 호출에만 적용되고
/// 다른 `run_mcp_command` 호출자에는 영향이 없다 — OAuth client secret을
/// `MCP_CLIENT_SECRET`으로 넘기는 `mcp_add` 전용 경로).
fn run_mcp_command_with_env(
    args: &[&str],
    extra_env: Option<(&str, &str)>,
) -> Result<(String, bool), String> {
    let resolved =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
            .ok_or_else(|| {
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                    .to_string()
            })?;
    let home = dirs::home_dir().ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    let mut command = std::process::Command::new(&resolved);
    command.args(args).current_dir(&home).env("PATH", &path_env);
    if let Some((key, value)) = extra_env {
        command.env(key, value);
    }

    let output = command
        .output()
        .map_err(|e| format!("claude 명령을 실행하지 못했습니다: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();
    let text = if success || stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    Ok((text, success))
}

// ---------------- 파싱 ----------------

/// TYPE 표기(괄호 안, HTTP/SSE)를 커맨드 시그니처가 약속한 세 값 중 하나로
/// 정규화한다. 알 수 없는 값은 "unknown"으로 느슨하게 처리한다(문서화되지
/// 않은 새 transport가 CLI에 추가돼도 파싱 자체가 깨지지 않게).
fn normalize_transport(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "http" => "http".to_string(),
        "sse" => "sse".to_string(),
        _ => "unknown".to_string(),
    }
}

/// `claude mcp list` 출력 한 줄을 파싱한다. 첫 줄("Checking...")과 빈 줄은
/// `None`. 데이터 줄은 `<name>: <target> - <status>` 또는
/// `<name>: <target> (<TYPE>) - <status>` 형태.
fn parse_mcp_list_line(line: &str) -> Option<McpServerSummary> {
    let line = line.trim_end();
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("Checking") {
        return None;
    }

    let (name, rest) = line.split_once(": ")?;
    let name = name.trim().to_string();

    // status는 마지막 " - " 뒤 전체 문자열 그대로 보존한다.
    let sep_idx = rest.rfind(" - ")?;
    let target_and_type = rest[..sep_idx].trim();
    let status_label = rest[sep_idx + 3..].to_string();

    let (target, transport) = if target_and_type.ends_with(')') {
        match target_and_type.rfind('(') {
            Some(open_idx) => {
                let type_str = &target_and_type[open_idx + 1..target_and_type.len() - 1];
                let target = target_and_type[..open_idx].trim().to_string();
                (target, normalize_transport(type_str))
            }
            None => (target_and_type.to_string(), "stdio".to_string()),
        }
    } else {
        (target_and_type.to_string(), "stdio".to_string())
    };

    let connected = status_label.contains('✔') || status_label.contains("Connected");

    Some(McpServerSummary {
        name,
        target,
        transport,
        connected,
        status_label,
    })
}

fn parse_mcp_list(output: &str) -> Vec<McpServerSummary> {
    output.lines().filter_map(parse_mcp_list_line).collect()
}

/// `claude mcp get <name>` 출력을 파싱한다. `  Key: Value` 형태의 줄들을
/// key-value로 모으되, Scope/Status/Type/URL(또는 Command+Args)만 구조화된
/// 필드로 뽑는다. Environment 등 중첩 하위 목록은 무시한다(중요하지 않음).
/// 데이터를 하나도 못 찾으면 `None`(파싱 실패로 취급 — 호출자가 에러로 매핑).
fn parse_mcp_get(requested_name: &str, output: &str) -> Option<McpServerDetail> {
    let mut name = requested_name.to_string();
    let mut scope = String::new();
    let mut status = String::new();
    let mut transport = String::new();
    let mut url: Option<String> = None;
    let mut command: Option<String> = None;
    let mut args: Option<String> = None;
    let mut oauth: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 헤더 줄("<name>:")은 들여쓰기 없이 시작하고 콜론으로 끝난다.
        if !line.starts_with(' ') && !line.starts_with('\t') && trimmed.ends_with(':') {
            name = trimmed.trim_end_matches(':').trim().to_string();
            continue;
        }
        let Some((key, value)) = trimmed.split_once(": ") else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "Scope" => scope = value,
            "Status" => status = value,
            "Type" => transport = value.to_lowercase(),
            "URL" => url = Some(value),
            "Command" => command = Some(value),
            "Args" => args = Some(value),
            "OAuth" => oauth = Some(value),
            _ => {}
        }
    }

    if scope.is_empty() && status.is_empty() && transport.is_empty() {
        return None;
    }

    let target = url.unwrap_or_else(|| match (&command, &args) {
        (Some(c), Some(a)) if !a.is_empty() => format!("{c} {a}"),
        (Some(c), _) => c.clone(),
        _ => String::new(),
    });

    Some(McpServerDetail {
        name,
        scope,
        status,
        transport,
        target,
        oauth,
    })
}

// ---------------- add 인자 조립 ----------------

/// stdio: `mcp add --scope user <name> [-e KEY=VALUE ...] -- <target> [args...]`
/// http/sse: `mcp add --scope user --transport <http|sse> <name> <target> [-H <header>]`
/// (header가 `Some`이고 비어있지 않을 때만 `-H` 추가). 스코프는 항상
/// "user"로 고정한다(모든 프로젝트에서 보이게 — UI에 스코프 선택지를
/// 노출하지 않기로 결정했다).
///
/// `env`는 stdio 분기에서만 반영한다(공식 `claude mcp add` 예시 순서를
/// 그대로 따라 `name` 다음, `--` 구분자 이전에 `-e KEY=VALUE`를 하나씩
/// 넣는다). http/sse는 env를 지원하지 않으므로 그대로 무시한다(이번
/// 요구사항 범위 밖). 빈 key는 건너뛴다(값만 있고 키가 비었으면 무시).
///
/// `oauth_client_id`/`oauth_client_secret`/`oauth_callback_port`는 사내
/// OAuth MCP 등록용으로, http/sse 분기에서만 의미가 있다(stdio는 그대로
/// 무시 — `claude mcp add`가 stdio에 이 옵션을 지원하지 않는다).
/// `oauth_client_secret`은 **값 유무와 무관하게 `Some`이면** bare
/// `--client-secret` 플래그만 추가한다 — 실제 비밀값은 인자로 절대 넣지
/// 않고 호출자가 별도로 `MCP_CLIENT_SECRET` 환경변수로 자식 프로세스에
/// 주입한다(`run_mcp_command_with_env`).
fn build_add_args(
    name: &str,
    transport: &str,
    target: &str,
    args: &[String],
    header: &Option<String>,
    env: &[EnvVarPair],
    oauth_client_id: &Option<String>,
    oauth_client_secret: &Option<String>,
    oauth_callback_port: &Option<u16>,
) -> Result<Vec<String>, String> {
    let transport_norm = transport.trim().to_lowercase();
    let mut cmd_args: Vec<String> = vec![
        "mcp".to_string(),
        "add".to_string(),
        "--scope".to_string(),
        "user".to_string(),
    ];

    match transport_norm.as_str() {
        "stdio" => {
            cmd_args.push(name.to_string());
            for pair in env {
                if pair.key.is_empty() {
                    continue;
                }
                cmd_args.push("-e".to_string());
                cmd_args.push(format!("{}={}", pair.key, pair.value));
            }
            cmd_args.push("--".to_string());
            cmd_args.push(target.to_string());
            cmd_args.extend(args.iter().cloned());
        }
        "http" | "sse" => {
            cmd_args.push("--transport".to_string());
            cmd_args.push(transport_norm.clone());
            cmd_args.push(name.to_string());
            cmd_args.push(target.to_string());
            if let Some(h) = header {
                if !h.trim().is_empty() {
                    cmd_args.push("-H".to_string());
                    cmd_args.push(h.clone());
                }
            }
            if let Some(client_id) = oauth_client_id {
                cmd_args.push("--client-id".to_string());
                cmd_args.push(client_id.clone());
            }
            if oauth_client_secret.is_some() {
                // 값은 절대 인자로 넣지 않는다 — 비밀값은 호출자가 자식
                // 프로세스 환경변수(MCP_CLIENT_SECRET)로만 전달한다.
                cmd_args.push("--client-secret".to_string());
            }
            if let Some(port) = oauth_callback_port {
                cmd_args.push("--callback-port".to_string());
                cmd_args.push(port.to_string());
            }
        }
        other => {
            return Err(format!("알 수 없는 transport입니다: {other}"));
        }
    }

    Ok(cmd_args)
}

// ---------------- 공개 MCP 카탈로그(원클릭 설치) ----------------
//
// 5개 항목 전부 OAuth 로그인이 필요하다 — `claude mcp add`만으로는 끝나지
// 않고 브라우저 인증이 끼는 `claude mcp login <name>`까지 이어서 실행해야
// 한다. 이건 GitHub/Cloudflare 연동(`github_integration::github_connect`)과
// 같은 이유로 앱이 조용히 백그라운드 spawn하지 않고 `cli_launcher::
// open_terminal_command`로 사용자가 직접 보는 터미널 창을 연다.
//
// 프론트는 `catalog_id: String` 하나만 보낼 수 있고, transport/URL 조합은
// 이 표 밖으로 절대 나가지 않는다 — `open_terminal_command`의 불변식("절대
// 경로로 해석된 신뢰 가능한 바이너리 + 고정 서브커맨드 문자열만 넘어온다,
// 사용자 자유입력 없음")을 지키기 위해서다. 프론트가 침해돼도 임의 셸
// 커맨드를 터미널에 주입할 경로가 없다.

/// 웹 검색 + 이 머신에서의 실측(`claude mcp list`)으로 검증된 고정 값이다.
/// 추측/변형 없이 이 값 그대로 유지한다.
struct McpCatalogEntry {
    id: &'static str,
    label: &'static str,
    /// "http" | "sse" — `build_add_args`가 받는 transport 값과 동일한 어휘.
    transport: &'static str,
    target: &'static str,
}

const MCP_CATALOG: [McpCatalogEntry; 5] = [
    McpCatalogEntry {
        id: "gmail",
        label: "Gmail",
        transport: "http",
        target: "https://gmailmcp.googleapis.com/mcp/v1",
    },
    McpCatalogEntry {
        id: "google-drive",
        label: "Google Drive",
        transport: "http",
        target: "https://drivemcp.googleapis.com/mcp/v1",
    },
    McpCatalogEntry {
        id: "google-calendar",
        label: "Google Calendar",
        transport: "http",
        target: "https://calendarmcp.googleapis.com/mcp/v1",
    },
    McpCatalogEntry {
        id: "atlassian",
        label: "Atlassian (Jira/Confluence)",
        transport: "sse",
        target: "https://mcp.atlassian.com/v1/sse",
    },
    McpCatalogEntry {
        id: "figma",
        label: "Figma",
        transport: "http",
        target: "https://mcp.figma.com/mcp",
    },
];

fn find_catalog_entry(catalog_id: &str) -> Option<&'static McpCatalogEntry> {
    MCP_CATALOG.iter().find(|e| e.id == catalog_id)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct McpCatalogItem {
    pub id: String,
    pub label: String,
    pub transport: String,
    pub target: String,
    pub installed: bool,
}

/// 카탈로그 표를 `installed_targets`(이미 등록된 서버들의 target URL 목록)와
/// 대조해 `installed` 플래그를 채운다. name이 아니라 target URL로 매칭한다
/// — 사용자가 설치 시 표시 이름을 임의로 바꿀 수 있어 name은 신뢰할 수
/// 없다. 실제 `claude mcp list` 호출 없이 순수하게 동작해 테스트가 가볍다.
fn build_catalog_list(installed_targets: &[String]) -> Vec<McpCatalogItem> {
    MCP_CATALOG
        .iter()
        .map(|entry| {
            let installed = installed_targets.iter().any(|t| t == entry.target);
            McpCatalogItem {
                id: entry.id.to_string(),
                label: entry.label.to_string(),
                transport: entry.transport.to_string(),
                target: entry.target.to_string(),
                installed,
            }
        })
        .collect()
}

/// `claude mcp add --scope user --transport <transport> "<label>" <target> &&
/// claude mcp login "<label>"` 형태의 고정 셸 커맨드 문자열을 조립한다.
/// `claude_bin`은 호출자가 이미 절대경로로 해석해 넘긴 값이고, `entry`는
/// `MCP_CATALOG`의 고정 항목뿐이다 — 둘 다 사용자 자유입력이 섞이지 않는다.
/// 프로세스를 실행하지 않는 순수 문자열 조립이라 테스트가 실제 claude/
/// 터미널 없이 돈다.
fn build_install_command(claude_bin: &str, entry: &McpCatalogEntry) -> String {
    format!(
        "{claude_bin} mcp add --scope user --transport {transport} \"{label}\" {target} && {claude_bin} mcp login \"{label}\"",
        claude_bin = claude_bin,
        transport = entry.transport,
        label = entry.label,
        target = entry.target,
    )
}

// ---------------- Tauri 커맨드 ----------------

/// 실패해도(claude 미설치 등) 빈 목록을 돌려준다 — 커맨드 시그니처가
/// `Result`가 아닌 `Vec`으로 고정돼 있다(프론트가 "0개 등록됨"과 "조회
/// 실패"를 구분할 필요가 없는 화면이라는 설계 결정).
#[tauri::command]
pub fn mcp_list() -> Vec<McpServerSummary> {
    match run_mcp_command(&["mcp", "list"]) {
        Ok((text, _success)) => parse_mcp_list(&text),
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
pub fn mcp_get(name: String) -> Result<McpServerDetail, String> {
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

/// `oauth_client_id`/`oauth_client_secret`/`oauth_callback_port`는 http/sse
/// 사내 OAuth MCP 등록에만 쓰인다(stdio는 `build_add_args`가 무시한다).
/// `oauth_client_secret`이 `Some`이면 그 프로세스 실행에만
/// `MCP_CLIENT_SECRET` 환경변수를 주입한다 — 값은 CLI 인자로도, 우리
/// 코드의 로그/에러 메시지로도 별도로 출력하지 않는다(실패 시 반환되는
/// stderr 텍스트에 CLI가 자체적으로 비밀값을 echo하는 경우는 이 코드가
/// 막을 수 없는 CLI 쪽 동작이다).
#[tauri::command]
pub fn mcp_add(
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
pub fn mcp_remove(name: String) -> Result<(), String> {
    let (text, success) = run_mcp_command(&["mcp", "remove", &name])?;
    if success {
        Ok(())
    } else if text.trim().is_empty() {
        Err(format!("'{name}' MCP 서버 삭제에 실패했습니다."))
    } else {
        Err(text.trim().to_string())
    }
}

/// 고정 카탈로그(`MCP_CATALOG`) 5개를 반환하되, `claude mcp list` 결과와
/// target URL로 대조해 이미 등록된 항목은 `installed: true`로 표시한다.
/// `mcp_list`와 동일하게 조회 실패는 "0개 설치됨"으로 느슨하게 처리한다
/// (claude 미설치 상태에서도 카탈로그 자체는 항상 보여줘야 하는 화면이다).
#[tauri::command]
pub fn mcp_catalog_list() -> Vec<McpCatalogItem> {
    let installed_targets: Vec<String> = mcp_list().into_iter().map(|s| s.target).collect();
    build_catalog_list(&installed_targets)
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

    const SAMPLE_LIST_OUTPUT: &str = "Checking MCP server health…\n\nclaude.ai Gmail: https://gmailmcp.googleapis.com/mcp/v1 - ✔ Connected\nplugin:malgn-agent:malgnai-hub: https://malgnai-hub.apiserver.kr/mcp (HTTP) - ✔ Connected\nmalgnai-mcp: /opt/homebrew/bin/node /Users/hopegiver/workspace/malgnai/mcp/index.js - ✔ Connected\n";

    const SAMPLE_GET_HTTP_OUTPUT: &str = "plugin:malgn-agent:malgnai-hub:\n  Scope: Dynamic config (from command line)\n  Status: ✔ Connected\n  Type: http\n  URL: https://malgnai-hub.apiserver.kr/mcp\n  OAuth: client_id configured\n";

    const SAMPLE_GET_STDIO_OUTPUT: &str = "malgnai-mcp:\n  Scope: User config (available in all your projects)\n  Status: ✔ Connected\n  Type: stdio\n  Command: /opt/homebrew/bin/node\n  Args: /Users/hopegiver/workspace/malgnai/mcp/index.js\n  Environment:\n    MALGNAI_DB_PATH=/Users/hopegiver/workspace/malgnai/data/malgnai.db\n";

    #[test]
    fn parses_full_list_sample_into_three_servers() {
        let servers = parse_mcp_list(SAMPLE_LIST_OUTPUT);
        assert_eq!(servers.len(), 3);

        assert_eq!(servers[0].name, "claude.ai Gmail");
        assert_eq!(servers[0].target, "https://gmailmcp.googleapis.com/mcp/v1");
        assert_eq!(servers[0].transport, "stdio");
        assert!(servers[0].connected);
        assert_eq!(servers[0].status_label, "✔ Connected");

        assert_eq!(servers[1].name, "plugin:malgn-agent:malgnai-hub");
        assert_eq!(servers[1].target, "https://malgnai-hub.apiserver.kr/mcp");
        assert_eq!(servers[1].transport, "http");
        assert!(servers[1].connected);

        assert_eq!(servers[2].name, "malgnai-mcp");
        assert_eq!(
            servers[2].target,
            "/opt/homebrew/bin/node /Users/hopegiver/workspace/malgnai/mcp/index.js"
        );
        assert_eq!(servers[2].transport, "stdio");
        assert!(servers[2].connected);
    }

    #[test]
    fn list_line_without_parenthesized_type_is_stdio() {
        let parsed =
            parse_mcp_list_line("malgnai-mcp: /opt/homebrew/bin/node index.js - ✔ Connected")
                .unwrap();
        assert_eq!(parsed.transport, "stdio");
    }

    #[test]
    fn list_line_with_sse_type_is_parsed() {
        let parsed = parse_mcp_list_line("svc: https://example.com/sse (SSE) - ✔ Connected").unwrap();
        assert_eq!(parsed.transport, "sse");
        assert_eq!(parsed.target, "https://example.com/sse");
    }

    #[test]
    fn list_line_with_unrecognized_parenthesized_type_is_unknown() {
        let parsed = parse_mcp_list_line("svc: https://example.com (WS) - ✔ Connected").unwrap();
        assert_eq!(parsed.transport, "unknown");
    }

    #[test]
    fn list_line_with_pending_approval_status_is_not_connected() {
        let parsed =
            parse_mcp_list_line("svc: https://example.com - ⏸ Pending approval").unwrap();
        assert!(!parsed.connected);
        assert_eq!(parsed.status_label, "⏸ Pending approval");
    }

    #[test]
    fn list_ignores_header_line_and_blank_lines() {
        assert!(parse_mcp_list_line("Checking MCP server health…").is_none());
        assert!(parse_mcp_list_line("").is_none());
        assert!(parse_mcp_list_line("   ").is_none());
    }

    #[test]
    fn parses_get_http_sample() {
        let detail = parse_mcp_get("plugin:malgn-agent:malgnai-hub", SAMPLE_GET_HTTP_OUTPUT)
            .expect("http fixture must parse");
        assert_eq!(detail.name, "plugin:malgn-agent:malgnai-hub");
        assert_eq!(detail.scope, "Dynamic config (from command line)");
        assert_eq!(detail.status, "✔ Connected");
        assert_eq!(detail.transport, "http");
        assert_eq!(detail.target, "https://malgnai-hub.apiserver.kr/mcp");
        assert_eq!(detail.oauth.as_deref(), Some("client_id configured"));
    }

    #[test]
    fn parses_get_stdio_sample_combining_command_and_args_as_target() {
        let detail =
            parse_mcp_get("malgnai-mcp", SAMPLE_GET_STDIO_OUTPUT).expect("stdio fixture must parse");
        assert_eq!(detail.name, "malgnai-mcp");
        assert_eq!(detail.scope, "User config (available in all your projects)");
        assert_eq!(detail.status, "✔ Connected");
        assert_eq!(detail.transport, "stdio");
        assert_eq!(
            detail.target,
            "/opt/homebrew/bin/node /Users/hopegiver/workspace/malgnai/mcp/index.js"
        );
        assert_eq!(detail.oauth, None);
    }

    #[test]
    fn parse_get_returns_none_for_empty_or_unrecognized_output() {
        assert!(parse_mcp_get("nope", "").is_none());
        assert!(parse_mcp_get("nope", "some unrelated text\nwith no colons here").is_none());
    }

    #[test]
    fn build_add_args_for_stdio_uses_double_dash_separator() {
        let args = build_add_args(
            "my-server",
            "stdio",
            "/usr/local/bin/node",
            &["index.js".to_string(), "--flag".to_string()],
            &None,
            &[],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp", "add", "--scope", "user", "my-server", "--", "/usr/local/bin/node",
                "index.js", "--flag"
            ]
        );
    }

    #[test]
    fn build_add_args_for_stdio_with_env_inserts_dash_e_pairs_before_separator() {
        // 공식 예시(`claude mcp add grafana --scope user -e GRAFANA_URL=... -e
        // GRAFANA_SERVICE_ACCOUNT_TOKEN=... -- mcp-grafana`)와 동일한 순서:
        // name 다음, `--` 구분자 이전에 -e KEY=VALUE 쌍이 온다.
        let args = build_add_args(
            "grafana",
            "stdio",
            "mcp-grafana",
            &[],
            &None,
            &[
                EnvVarPair {
                    key: "GRAFANA_URL".to_string(),
                    value: "http://localhost:3000".to_string(),
                },
                EnvVarPair {
                    key: "GRAFANA_SERVICE_ACCOUNT_TOKEN".to_string(),
                    value: "secret-token".to_string(),
                },
            ],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "grafana",
                "-e",
                "GRAFANA_URL=http://localhost:3000",
                "-e",
                "GRAFANA_SERVICE_ACCOUNT_TOKEN=secret-token",
                "--",
                "mcp-grafana",
            ]
        );
    }

    #[test]
    fn build_add_args_for_stdio_skips_blank_key_env_pairs() {
        let args = build_add_args(
            "my-server",
            "stdio",
            "/usr/local/bin/node",
            &[],
            &None,
            &[
                EnvVarPair {
                    key: "".to_string(),
                    value: "orphan-value".to_string(),
                },
                EnvVarPair {
                    key: "REAL_KEY".to_string(),
                    value: "real-value".to_string(),
                },
            ],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "my-server",
                "-e",
                "REAL_KEY=real-value",
                "--",
                "/usr/local/bin/node",
            ]
        );
    }

    #[test]
    fn build_add_args_for_http_ignores_env_pairs() {
        // http/sse는 env를 지원하지 않는다 — env가 전달돼도 조용히 무시하고
        // 기존 동작(헤더 처리) 그대로여야 한다.
        let args = build_add_args(
            "my-http",
            "http",
            "https://example.com/mcp",
            &[],
            &None,
            &[EnvVarPair {
                key: "IGNORED".to_string(),
                value: "value".to_string(),
            }],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "my-http",
                "https://example.com/mcp"
            ]
        );
    }

    #[test]
    fn build_add_args_for_http_without_header_omits_dash_h() {
        let args = build_add_args(
            "my-http",
            "http",
            "https://example.com/mcp",
            &[],
            &None,
            &[],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "my-http",
                "https://example.com/mcp"
            ]
        );
    }

    #[test]
    fn build_add_args_for_sse_with_header_appends_dash_h() {
        let args = build_add_args(
            "my-sse",
            "SSE",
            "https://example.com/sse",
            &[],
            &Some("Authorization: Bearer token".to_string()),
            &[],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "sse",
                "my-sse",
                "https://example.com/sse",
                "-H",
                "Authorization: Bearer token"
            ]
        );
    }

    #[test]
    fn build_add_args_with_blank_header_omits_dash_h() {
        let args = build_add_args(
            "my-http",
            "http",
            "https://example.com/mcp",
            &[],
            &Some("   ".to_string()),
            &[],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert!(!args.contains(&"-H".to_string()));
    }

    #[test]
    fn build_add_args_rejects_unknown_transport() {
        let result = build_add_args(
            "x",
            "websocket",
            "https://example.com",
            &[],
            &None,
            &[],
            &None,
            &None,
            &None,
        );
        assert!(result.is_err());
    }

    /// GUI(.app) 실행 시 PATH가 제한될 수 있다는 `cli_launcher.rs`의 실측
    /// 문제를 이 모듈도 겪지 않는지 회귀로 고정한다 — `autonomy.rs`의 동일
    /// 테스트와 같은 취지다.
    #[test]
    fn claude_path_candidates_resolve_to_something_on_a_machine_with_claude_installed() {
        let resolved =
            crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude");
        assert!(
            resolved.is_some(),
            "이 개발 머신에는 claude가 설치돼 있어야 하는데 절대경로 후보와 PATH 폴백 모두 실패했습니다"
        );
    }

    // ---------------- 카탈로그 ----------------

    #[test]
    fn catalog_has_the_five_verified_entries_with_exact_values() {
        assert_eq!(MCP_CATALOG.len(), 5);

        let gmail = find_catalog_entry("gmail").expect("gmail must exist");
        assert_eq!(gmail.label, "Gmail");
        assert_eq!(gmail.transport, "http");
        assert_eq!(gmail.target, "https://gmailmcp.googleapis.com/mcp/v1");

        let drive = find_catalog_entry("google-drive").expect("google-drive must exist");
        assert_eq!(drive.label, "Google Drive");
        assert_eq!(drive.transport, "http");
        assert_eq!(drive.target, "https://drivemcp.googleapis.com/mcp/v1");

        let calendar = find_catalog_entry("google-calendar").expect("google-calendar must exist");
        assert_eq!(calendar.label, "Google Calendar");
        assert_eq!(calendar.transport, "http");
        assert_eq!(calendar.target, "https://calendarmcp.googleapis.com/mcp/v1");

        let atlassian = find_catalog_entry("atlassian").expect("atlassian must exist");
        assert_eq!(atlassian.label, "Atlassian (Jira/Confluence)");
        assert_eq!(atlassian.transport, "sse");
        assert_eq!(atlassian.target, "https://mcp.atlassian.com/v1/sse");

        let figma = find_catalog_entry("figma").expect("figma must exist");
        assert_eq!(figma.label, "Figma");
        assert_eq!(figma.transport, "http");
        assert_eq!(figma.target, "https://mcp.figma.com/mcp");
    }

    #[test]
    fn find_catalog_entry_returns_none_for_unknown_id() {
        assert!(find_catalog_entry("outlook").is_none());
        assert!(find_catalog_entry("").is_none());
    }

    #[test]
    fn build_catalog_list_marks_installed_by_target_url_not_name() {
        // 실측(claude mcp list): claude.ai Gmail/Google Drive/Google Calendar는
        // 카탈로그 target URL과 정확히 일치하는 값으로 이미 등록돼 있었다.
        // 이름이 카탈로그 label("Gmail")과 다르게 등록돼도(name이 아니라
        // target으로 매칭하므로) installed로 잡혀야 한다.
        let installed_targets = vec![
            "https://gmailmcp.googleapis.com/mcp/v1".to_string(),
            "https://drivemcp.googleapis.com/mcp/v1".to_string(),
        ];
        let list = build_catalog_list(&installed_targets);
        assert_eq!(list.len(), 5);

        let gmail = list.iter().find(|i| i.id == "gmail").unwrap();
        assert!(gmail.installed);

        let drive = list.iter().find(|i| i.id == "google-drive").unwrap();
        assert!(drive.installed);

        let calendar = list.iter().find(|i| i.id == "google-calendar").unwrap();
        assert!(!calendar.installed);

        let atlassian = list.iter().find(|i| i.id == "atlassian").unwrap();
        assert!(!atlassian.installed);

        let figma = list.iter().find(|i| i.id == "figma").unwrap();
        assert!(!figma.installed);
    }

    #[test]
    fn build_catalog_list_with_no_installed_targets_marks_everything_false() {
        let list = build_catalog_list(&[]);
        assert!(list.iter().all(|i| !i.installed));
    }

    #[test]
    fn build_install_command_for_http_entry_chains_add_and_login() {
        let entry = find_catalog_entry("gmail").unwrap();
        let command = build_install_command("/opt/homebrew/bin/claude", entry);
        assert_eq!(
            command,
            "/opt/homebrew/bin/claude mcp add --scope user --transport http \"Gmail\" https://gmailmcp.googleapis.com/mcp/v1 && /opt/homebrew/bin/claude mcp login \"Gmail\""
        );
    }

    #[test]
    fn build_install_command_for_sse_entry_quotes_label_with_special_chars() {
        let entry = find_catalog_entry("atlassian").unwrap();
        let command = build_install_command("/opt/homebrew/bin/claude", entry);
        assert_eq!(
            command,
            "/opt/homebrew/bin/claude mcp add --scope user --transport sse \"Atlassian (Jira/Confluence)\" https://mcp.atlassian.com/v1/sse && /opt/homebrew/bin/claude mcp login \"Atlassian (Jira/Confluence)\""
        );
    }

    #[test]
    fn mcp_install_rejects_unknown_catalog_id_without_touching_claude() {
        // catalog_id 검사가 claude 바이너리 탐색보다 먼저이므로, 이 머신에
        // claude가 설치돼 있는지와 무관하게 항상 Err여야 한다.
        let result = mcp_install("not-a-real-catalog-id".to_string());
        assert_eq!(result.unwrap_err(), "알 수 없는 카탈로그 항목입니다");
    }

    // ---------------- 사내 OAuth MCP 등록 (client-id/client-secret/callback-port) ----------------

    #[test]
    fn build_add_args_for_http_with_all_oauth_params_appends_them_after_header() {
        let args = build_add_args(
            "internal-mcp",
            "http",
            "https://internal.example.com/mcp",
            &[],
            &None,
            &[],
            &Some("my-client-id".to_string()),
            &Some("super-secret-value".to_string()),
            &Some(9876),
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "internal-mcp",
                "https://internal.example.com/mcp",
                "--client-id",
                "my-client-id",
                "--client-secret",
                "--callback-port",
                "9876",
            ]
        );
        // 비밀값 자체는 인자 배열 어디에도 나타나지 않는다 — env로만 전달된다.
        assert!(!args.iter().any(|a| a == "super-secret-value"));
    }

    #[test]
    fn build_add_args_for_sse_with_client_id_only() {
        let args = build_add_args(
            "internal-mcp",
            "sse",
            "https://internal.example.com/sse",
            &[],
            &None,
            &[],
            &Some("only-id".to_string()),
            &None,
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "sse",
                "internal-mcp",
                "https://internal.example.com/sse",
                "--client-id",
                "only-id",
            ]
        );
    }

    #[test]
    fn build_add_args_for_http_with_client_secret_only_adds_bare_flag_without_value() {
        let args = build_add_args(
            "internal-mcp",
            "http",
            "https://internal.example.com/mcp",
            &[],
            &None,
            &[],
            &None,
            &Some("another-secret".to_string()),
            &None,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "internal-mcp",
                "https://internal.example.com/mcp",
                "--client-secret",
            ]
        );
        assert!(!args.iter().any(|a| a == "another-secret"));
    }

    #[test]
    fn build_add_args_for_http_with_callback_port_only() {
        let args = build_add_args(
            "internal-mcp",
            "http",
            "https://internal.example.com/mcp",
            &[],
            &None,
            &[],
            &None,
            &None,
            &Some(4000),
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "internal-mcp",
                "https://internal.example.com/mcp",
                "--callback-port",
                "4000",
            ]
        );
    }

    #[test]
    fn build_add_args_with_no_oauth_params_omits_all_oauth_flags() {
        let args = build_add_args(
            "internal-mcp",
            "http",
            "https://internal.example.com/mcp",
            &[],
            &None,
            &[],
            &None,
            &None,
            &None,
        )
        .unwrap();
        assert!(!args.iter().any(|a| a.starts_with("--client")));
        assert!(!args.contains(&"--callback-port".to_string()));
    }

    #[test]
    fn build_add_args_for_stdio_ignores_oauth_params() {
        // stdio는 `claude mcp add`가 OAuth 옵션을 지원하지 않으므로, 셋 다
        // 채워져 있어도 결과 인자 배열에 전혀 반영되지 않아야 한다.
        let args = build_add_args(
            "my-server",
            "stdio",
            "/usr/local/bin/node",
            &["index.js".to_string()],
            &None,
            &[],
            &Some("ignored-client-id".to_string()),
            &Some("ignored-secret".to_string()),
            &Some(1234),
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "my-server",
                "--",
                "/usr/local/bin/node",
                "index.js",
            ]
        );
        assert!(!args.iter().any(|a| a.contains("client")));
        assert!(!args.iter().any(|a| a.contains("callback")));
        assert!(!args.iter().any(|a| a == "ignored-secret"));
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
