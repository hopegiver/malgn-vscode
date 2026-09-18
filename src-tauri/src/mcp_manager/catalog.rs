// ---------------- 공개 MCP 카탈로그(원클릭 설치) ----------------
//
// 5개 항목 전부 OAuth 로그인이 필요하다 — `claude mcp add`만으로는 끝나지
// 않고 브라우저 인증이 끼는 `claude mcp login <name>`까지 이어서 실행해야
// 한다. 이건 GitHub/Cloudflare 연동(`github_integration::github_connect`)과
// 같은 이유로 앱이 조용히 백그라운드 spawn하지 않고 `cli_launcher::
// open_terminal_program_sequence_with_env`로 사용자가 직접 보는 터미널 창을 연다(B1,
// 2라운드 — 예전에는 이 표의 값들로 셸 문자열을 직접 포맷팅해 `open_terminal_
// command`에 넘겼지만, 그 함수는 이제 `&'static str` 리터럴만 받는다).
//
// 프론트는 `catalog_id: String` 하나만 보낼 수 있고, transport/URL 조합은
// 이 표 밖으로 절대 나가지 않는다 — `mcp_install`(mod.rs)이 이 표의 label/
// transport/target을 argv 배열의 원소로만 조립하고, 인용은
// `open_terminal_program_sequence_with_env`(`platform::quote_token`)가 전담한다.
// 프론트가 침해돼도 임의 셸 커맨드를 터미널에 주입할 경로가 없다.

use serde::Serialize;

/// 웹 검색 + 이 머신에서의 실측(`claude mcp list`)으로 검증된 고정 값이다.
/// 추측/변형 없이 이 값 그대로 유지한다.
pub(super) struct McpCatalogEntry {
    /// `mcp_manager::mod`의 `build_install_add_args`/`build_install_login_args`가
    /// `claude mcp add`/`claude mcp login`의 name 인자로 직접 읽는다 — `claude
    /// mcp add`는 이름에 letters/numbers/hyphens/underscores만 허용해 label(사람이
    /// 읽는 표시 문자열, 공백·괄호 포함 가능)을 쓸 수 없다.
    pub(super) id: &'static str,
    /// `mcp_manager::mod`의 `mcp_install`이 사람에게 보여주는 성공 메시지 조립에만
    /// 쓴다 — CLI에 넘기는 name 인자에는 쓰지 않는다.
    pub(super) label: &'static str,
    /// "http" | "sse" — `build_add_args`가 받는 transport 값과 동일한 어휘.
    /// `mcp_install`이 `claude mcp add --transport <값>` argv 조립에 직접 읽는다.
    pub(super) transport: &'static str,
    /// `mcp_install`이 `claude mcp add <target>` argv 조립에 직접 읽는다.
    pub(super) target: &'static str,
    /// 사내 발급 Google OAuth 클라이언트 ID/Secret — `build.rs`가 `.env`/CI
    /// secret(`GOOGLE_MCP_OAUTH_CLIENT_ID`/`_SECRET`)에서 `option_env!()`로
    /// 컴파일타임에 읽어들인 값이다. gmail/google-drive/google-calendar
    /// 3개 항목이 이 값을 공유한다 — Google Cloud OAuth 클라이언트는 API별이
    /// 아니라 애플리케이션 단위라서(사용자 확인 사실) 해당 GCP 프로젝트에서
    /// Gmail/Drive/Calendar API가 활성화돼 있고 필요 스코프가 동의화면에
    /// 등록돼 있으면 클라이언트 하나로 세 엔드포인트 모두를 인증할 수 있다.
    /// 이 세 엔드포인트(gmailmcp/drivemcp/calendarmcp.googleapis.com)는 RFC
    /// 7591 동적 클라이언트 등록(DCR)을 지원하지 않아서, 값이 없으면 `claude
    /// mcp add`가 자동 DCR을 시도하다 "Incompatible auth server: does not
    /// support dynamic client registration"으로 실패한다(atlassian/figma는
    /// DCR을 지원해 항상 None — 건드리지 않는다).
    /// `mcp_install`(mod.rs)이 `Some`이면 `--client-id`/`--client-secret`을
    /// argv에 값째로 포함시킨다 — `mcp_add_blocking`(mod.rs 상단 문서 참조)의
    /// bare-flag + env 주입 방식과 달리, 이 경로는 사용자가 직접 보는
    /// 터미널 창에 `claude` 커맨드를 그대로 실행해 그 프로세스에만 조용히
    /// 환경변수를 심을 채널이 없기 때문이다(값이 argv에 그대로 보인다 —
    /// `ps`로 다른 로컬 사용자에게 노출될 수 있다는 트레이드오프를 감수한다.
    /// 재검토 결론은 mod.rs의 `build_install_add_args` 문서 참조).
    pub(super) oauth_client_id: Option<&'static str>,
    pub(super) oauth_client_secret: Option<&'static str>,
}

const MCP_CATALOG: [McpCatalogEntry; 5] = [
    McpCatalogEntry {
        id: "gmail",
        label: "Gmail",
        transport: "http",
        target: "https://gmailmcp.googleapis.com/mcp/v1",
        oauth_client_id: option_env!("GOOGLE_MCP_OAUTH_CLIENT_ID"),
        oauth_client_secret: option_env!("GOOGLE_MCP_OAUTH_CLIENT_SECRET"),
    },
    McpCatalogEntry {
        id: "google-drive",
        label: "Google Drive",
        transport: "http",
        target: "https://drivemcp.googleapis.com/mcp/v1",
        oauth_client_id: option_env!("GOOGLE_MCP_OAUTH_CLIENT_ID"),
        oauth_client_secret: option_env!("GOOGLE_MCP_OAUTH_CLIENT_SECRET"),
    },
    McpCatalogEntry {
        id: "google-calendar",
        label: "Google Calendar",
        transport: "http",
        target: "https://calendarmcp.googleapis.com/mcp/v1",
        oauth_client_id: option_env!("GOOGLE_MCP_OAUTH_CLIENT_ID"),
        oauth_client_secret: option_env!("GOOGLE_MCP_OAUTH_CLIENT_SECRET"),
    },
    McpCatalogEntry {
        id: "atlassian",
        label: "Atlassian (Jira/Confluence)",
        transport: "sse",
        target: "https://mcp.atlassian.com/v1/sse",
        oauth_client_id: None,
        oauth_client_secret: None,
    },
    McpCatalogEntry {
        id: "figma",
        label: "Figma",
        transport: "http",
        target: "https://mcp.figma.com/mcp",
        oauth_client_id: None,
        oauth_client_secret: None,
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
    fn gmail_drive_calendar_share_the_same_google_oauth_fields_atlassian_figma_never_do() {
        // 컴파일 환경에 GOOGLE_MCP_OAUTH_CLIENT_ID/SECRET이 설정돼 있든
        // 없든(Some/None 어느 쪽이든), 세 Google 항목은 항상 같은 값을
        // 공유해야 하고 atlassian/figma는 항상 None이어야 한다 — 값 자체가
        // 아니라 "누가 공유하고 누가 관여하지 않는가"라는 구조를 고정한다.
        let gmail = find_catalog_entry("gmail").unwrap();
        let drive = find_catalog_entry("google-drive").unwrap();
        let calendar = find_catalog_entry("google-calendar").unwrap();
        assert_eq!(gmail.oauth_client_id, drive.oauth_client_id);
        assert_eq!(gmail.oauth_client_id, calendar.oauth_client_id);
        assert_eq!(gmail.oauth_client_secret, drive.oauth_client_secret);
        assert_eq!(gmail.oauth_client_secret, calendar.oauth_client_secret);

        let atlassian = find_catalog_entry("atlassian").unwrap();
        let figma = find_catalog_entry("figma").unwrap();
        assert_eq!(atlassian.oauth_client_id, None);
        assert_eq!(atlassian.oauth_client_secret, None);
        assert_eq!(figma.oauth_client_id, None);
        assert_eq!(figma.oauth_client_secret, None);
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
}
