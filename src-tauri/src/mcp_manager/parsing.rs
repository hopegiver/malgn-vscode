// ---------------- 파싱 ----------------

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct McpServerSummary {
    pub name: String,
    pub target: String,
    /// "stdio" | "http" | "sse" | "unknown"
    pub transport: String,
    pub connected: bool,
    #[serde(rename = "statusLabel")]
    pub status_label: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct McpServerDetail {
    pub name: String,
    pub scope: String,
    pub status: String,
    pub transport: String,
    pub target: String,
    pub oauth: Option<String>,
}

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

pub(super) fn parse_mcp_list(output: &str) -> Vec<McpServerSummary> {
    output.lines().filter_map(parse_mcp_list_line).collect()
}

/// `claude mcp get <name>` 출력을 파싱한다. `  Key: Value` 형태의 줄들을
/// key-value로 모으되, Scope/Status/Type/URL(또는 Command+Args)만 구조화된
/// 필드로 뽑는다. Environment 등 중첩 하위 목록은 무시한다(중요하지 않음).
/// 데이터를 하나도 못 찾으면 `None`(파싱 실패로 취급 — 호출자가 에러로 매핑).
pub(super) fn parse_mcp_get(requested_name: &str, output: &str) -> Option<McpServerDetail> {
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
}
