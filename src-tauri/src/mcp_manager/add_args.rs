// ---------------- add 인자 조립 ----------------

use serde::Deserialize;

/// stdio 서버에 전달할 환경변수 한 쌍. 프론트에서 camelCase JSON(`{key,
/// value}`)으로 넘어온다. `key`가 빈 문자열이면 `build_add_args`가 방어적으로
/// 건너뛴다(값만 있고 키가 없는 입력 행 등).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct EnvVarPair {
    pub key: String,
    pub value: String,
}

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
pub(super) fn build_add_args(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
