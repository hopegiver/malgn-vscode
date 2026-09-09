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

use serde::Serialize;

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
    let resolved =
        crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
            .ok_or_else(|| {
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패)."
                    .to_string()
            })?;
    let home = dirs::home_dir().ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    let output = std::process::Command::new(&resolved)
        .args(args)
        .current_dir(&home)
        .env("PATH", &path_env)
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

/// stdio: `mcp add --scope user <name> -- <target> [args...]`
/// http/sse: `mcp add --scope user --transport <http|sse> <name> <target> [-H <header>]`
/// (header가 `Some`이고 비어있지 않을 때만 `-H` 추가). 스코프는 항상
/// "user"로 고정한다(모든 프로젝트에서 보이게 — UI에 스코프 선택지를
/// 노출하지 않기로 결정했다).
fn build_add_args(
    name: &str,
    transport: &str,
    target: &str,
    args: &[String],
    header: &Option<String>,
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
        }
        other => {
            return Err(format!("알 수 없는 transport입니다: {other}"));
        }
    }

    Ok(cmd_args)
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

#[tauri::command]
pub fn mcp_add(
    name: String,
    transport: String,
    target: String,
    args: Vec<String>,
    header: Option<String>,
) -> Result<(), String> {
    let cmd_args = build_add_args(&name, &transport, &target, &args, &header)?;
    let arg_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let (text, success) = run_mcp_command(&arg_refs)?;
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
    fn build_add_args_for_http_without_header_omits_dash_h() {
        let args = build_add_args(
            "my-http",
            "http",
            "https://example.com/mcp",
            &[],
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
        )
        .unwrap();
        assert!(!args.contains(&"-H".to_string()));
    }

    #[test]
    fn build_add_args_rejects_unknown_transport() {
        let result = build_add_args("x", "websocket", "https://example.com", &[], &None);
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
}
