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

use serde::Serialize;

/// 웹 검색 + 이 머신에서의 실측(`claude mcp list`)으로 검증된 고정 값이다.
/// 추측/변형 없이 이 값 그대로 유지한다.
pub(super) struct McpCatalogEntry {
    id: &'static str,
    /// `mcp_manager::mod`의 `mcp_install`이 성공 메시지 조립에 직접 읽는다.
    pub(super) label: &'static str,
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

pub(super) fn find_catalog_entry(catalog_id: &str) -> Option<&'static McpCatalogEntry> {
    MCP_CATALOG.iter().find(|e| e.id == catalog_id)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct McpCatalogItem {
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
pub(super) fn build_catalog_list(installed_targets: &[String]) -> Vec<McpCatalogItem> {
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
pub(super) fn build_install_command(claude_bin: &str, entry: &McpCatalogEntry) -> String {
    format!(
        "{claude_bin} mcp add --scope user --transport {transport} \"{label}\" {target} && {claude_bin} mcp login \"{label}\"",
        claude_bin = claude_bin,
        transport = entry.transport,
        label = entry.label,
        target = entry.target,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
