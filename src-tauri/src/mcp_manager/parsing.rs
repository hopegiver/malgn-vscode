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
    } else if target_and_type.starts_with("http://") || target_and_type.starts_with("https://") {
        // `claude.ai <이름>` 커넥터(Gmail/Drive/Calendar/Atlassian Rovo 등)는
        // `(TYPE)` 표기 없이 URL만 찍힌다 — 로컬 커맨드가 아니라 계정에 연결된
        // 원격 서버이므로, 括호가 없어도 대상이 URL이면 "stdio"로 잘못
        // 분류하지 않는다(그러면 로그인/로그아웃 버튼이 아예 안 뜬다). 정확한
        // http/sse 구분을 CLI가 안 알려주므로 "http"로 느슨하게 잡는다 —
        // 이 값은 프론트에서 "stdio가 아니다"라는 판정에만 쓰인다.
        (target_and_type.to_string(), "http".to_string())
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

/// `claude.ai <이름>`(Gmail/Google Drive/Google Calendar/Atlassian Rovo/Claude
/// Docs 등)은 로컬 CLI가 관리하는 서버가 아니라 사용자의 claude.ai 계정에
/// 연결된 커넥터다(`claude mcp get`의 `Scope: claude.ai config`로 실측
/// 확인) — 그래서 이 앱이 노출하는 `claude mcp login/logout/remove`가 전부
/// 이 스코프에는 통하지 않고 실행 시 오류만 낸다(로컬 PC 단위가 아니라
/// 계정 단위 연결이라 관리 자체가 claude.ai 쪽 몫). 관리 불가능한 행에
/// 인증/재인증/해제/삭제 버튼을 보여줘서 사용자가 눌러보고 오류를 겪게
/// 하는 대신 "등록된 서버" 화면 목록에서는 아예 제외한다(`mod.rs`의
/// `mcp_list_blocking`이 이 필터를 적용). 이 함수 자체(`parse_mcp_list`)는
/// 원본 그대로 파싱만 한다 — 카탈로그의 "이미 설치됨" 판정(`mcp_catalog_list_
/// blocking`)이 claude.ai Gmail 등의 target URL도 여전히 봐야 "설치 가능"
/// 목록에 중복으로 다시 뜨지 않기 때문에, 필터링 안 된 원본이 필요하다.
pub(super) fn is_claude_ai_account_connector(name: &str) -> bool {
    name.starts_with("claude.ai ")
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
        // parse_mcp_list 자체는 필터링하지 않는다(카탈로그의 "이미 설치됨"
        // 판정이 claude.ai Gmail 등의 target URL도 봐야 한다 — 위 doc 주석
        // 참조). "등록된 서버" 화면에서 이 항목을 빼는 필터는 `mod.rs`의
        // `mcp_list_blocking`이 `is_claude_ai_account_connector`로 따로 건다.
        let servers = parse_mcp_list(SAMPLE_LIST_OUTPUT);
        assert_eq!(servers.len(), 3);

        assert_eq!(servers[0].name, "claude.ai Gmail");
        assert_eq!(servers[0].target, "https://gmailmcp.googleapis.com/mcp/v1");
        assert_eq!(
            servers[0].transport, "http",
            "괄호 TYPE 표기가 없어도 URL 타겟은 stdio로 잘못 분류하면 안 된다"
        );
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
    fn is_claude_ai_account_connector_matches_prefix_only() {
        assert!(is_claude_ai_account_connector("claude.ai Gmail"));
        assert!(is_claude_ai_account_connector("claude.ai Atlassian Rovo"));
        assert!(!is_claude_ai_account_connector("atlassian"));
        assert!(!is_claude_ai_account_connector("plugin:malgn-agent:malgnai-hub"));
    }

    #[test]
    fn list_line_without_parenthesized_type_is_stdio() {
        let parsed =
            parse_mcp_list_line("malgnai-mcp: /opt/homebrew/bin/node index.js - ✔ Connected")
                .unwrap();
        assert_eq!(parsed.transport, "stdio");
    }

    // 신규 — claude.ai 커넥터(Atlassian Rovo 등)는 括호 TYPE 표기가 없는 URL
    // 타겟이다. "미연결" 상태라 로그인 버튼이 아예 안 뜨던 실사용 버그의 회귀
    // 고정: 이 경우 절대 "stdio"가 되면 안 된다(그러면 인증 버튼이 사라진다).
    #[test]
    fn list_line_with_bare_url_and_no_type_is_not_stdio() {
        let parsed = parse_mcp_list_line(
            "claude.ai Atlassian Rovo: https://mcp.atlassian.com/v1/mcp/authv2 - ! Needs authentication",
        )
        .unwrap();
        assert_ne!(parsed.transport, "stdio");
        assert!(!parsed.connected);
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
