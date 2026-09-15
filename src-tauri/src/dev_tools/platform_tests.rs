// platform.rs의 테스트 모듈(별도 파일 분리 — 1,000줄 규율). `#[path]`로 연결된
// `mod tests`의 본문이다. 순수함수 정본은 platform.rs에 있다.

    use super::*;

    fn mac_roots() -> EnvRoots {
        EnvRoots {
            home: Some(PathBuf::from("/Users/hopegiver")),
            appdata: None,
            local_appdata: None,
            program_files: None,
            program_files_x86: None,
            pnpm_home: None,
            system_root: None,
        }
    }

    fn win_roots() -> EnvRoots {
        EnvRoots {
            home: Some(PathBuf::from(r"C:\Users\hopegiver")),
            appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming")),
            local_appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Local")),
            program_files: Some(PathBuf::from(r"C:\Program Files")),
            program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
            pnpm_home: None,
            system_root: Some(PathBuf::from(r"C:\Windows")),
        }
    }

    // ── expand_path_tokens ──

    #[test]
    fn expand_path_tokens_mac_expands_tilde_and_passes_through_absolute() {
        let roots = mac_roots();
        assert_eq!(
            expand_path_tokens(Platform::Mac, "~/.local/bin/claude", &roots),
            Some("/Users/hopegiver/.local/bin/claude".to_string())
        );
        assert_eq!(
            expand_path_tokens(Platform::Mac, "/opt/homebrew/bin/claude", &roots),
            Some("/opt/homebrew/bin/claude".to_string())
        );
    }

    #[test]
    fn expand_path_tokens_mac_without_home_fails_tilde_but_not_absolute() {
        let roots = EnvRoots {
            home: None,
            appdata: None,
            local_appdata: None,
            program_files: None,
            program_files_x86: None,
            pnpm_home: None,
            system_root: None,
        };
        assert_eq!(expand_path_tokens(Platform::Mac, "~/.local/bin/claude", &roots), None);
        assert_eq!(
            expand_path_tokens(Platform::Mac, "/opt/homebrew/bin/claude", &roots),
            Some("/opt/homebrew/bin/claude".to_string())
        );
    }

    #[test]
    fn expand_path_tokens_win_expands_known_tokens() {
        let roots = win_roots();
        assert_eq!(
            expand_path_tokens(Platform::Win, r"%USERPROFILE%\.local\bin\claude.exe", &roots),
            Some(r"C:\Users\hopegiver\.local\bin\claude.exe".to_string())
        );
        assert_eq!(
            expand_path_tokens(Platform::Win, r"%APPDATA%\npm\claude.cmd", &roots),
            Some(r"C:\Users\hopegiver\AppData\Roaming\npm\claude.cmd".to_string())
        );
        assert_eq!(
            expand_path_tokens(
                Platform::Win,
                r"%LOCALAPPDATA%\Microsoft\WinGet\Links\gh.exe",
                &roots
            ),
            Some(r"C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Links\gh.exe".to_string())
        );
        // 리터럴(토큰 없음)은 그대로 통과한다.
        assert_eq!(
            expand_path_tokens(Platform::Win, r"C:\Program Files\Git\cmd\git.exe", &roots),
            Some(r"C:\Program Files\Git\cmd\git.exe".to_string())
        );
    }

    #[test]
    fn expand_path_tokens_win_distinguishes_program_files_and_x86() {
        let roots = win_roots();
        assert_eq!(
            expand_path_tokens(Platform::Win, r"%ProgramFiles(x86)%\Git\cmd\git.exe", &roots),
            Some(r"C:\Program Files (x86)\Git\cmd\git.exe".to_string())
        );
        assert_eq!(
            expand_path_tokens(Platform::Win, r"%ProgramFiles%\Git\cmd\git.exe", &roots),
            Some(r"C:\Program Files\Git\cmd\git.exe".to_string())
        );
    }

    #[test]
    fn expand_path_tokens_win_missing_root_returns_none() {
        let roots = EnvRoots {
            home: None,
            appdata: None,
            local_appdata: None,
            program_files: None,
            program_files_x86: None,
            pnpm_home: None,
            system_root: None,
        };
        assert_eq!(
            expand_path_tokens(Platform::Win, r"%APPDATA%\npm\claude.cmd", &roots),
            None,
            "필요한 토큰의 뿌리가 없으면 그럴듯한 값을 지어내지 않고 None이어야 합니다"
        );
    }

    // ── exe_extensions / path_separator ──

    #[test]
    fn exe_extensions_prefers_exe_over_cmd_over_bat_on_windows() {
        assert_eq!(exe_extensions(Platform::Win), &[".exe", ".cmd", ".bat"]);
        assert_eq!(exe_extensions(Platform::Mac), &[""]);
    }

    #[test]
    fn path_separator_matches_os_convention() {
        assert_eq!(path_separator(Platform::Mac), ':');
        assert_eq!(path_separator(Platform::Win), ';');
    }

    // ── default_path_dirs ──

    #[test]
    fn default_path_dirs_mac_matches_existing_six_literals_exactly() {
        let roots = mac_roots();
        assert_eq!(
            default_path_dirs(Platform::Mac, &roots),
            vec![
                "/opt/homebrew/bin".to_string(),
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
                "/bin".to_string(),
                "/usr/sbin".to_string(),
                "/sbin".to_string(),
            ]
        );
    }

    #[test]
    fn default_path_dirs_win_includes_system_and_npm_pnpm_winget_dirs() {
        let roots = win_roots();
        let dirs = default_path_dirs(Platform::Win, &roots);
        assert_eq!(
            dirs,
            vec![
                r"C:\Windows\system32".to_string(),
                r"C:\Windows".to_string(),
                r"C:\Windows\System32\Wbem".to_string(),
                r"C:\Users\hopegiver\AppData\Roaming\npm".to_string(),
                r"C:\Users\hopegiver\AppData\Local\pnpm".to_string(),
                r"C:\Users\hopegiver\AppData\Local\Microsoft\WindowsApps".to_string(),
            ]
        );
    }

    #[test]
    fn default_path_dirs_win_falls_back_to_c_windows_when_system_root_unset() {
        let roots = EnvRoots {
            home: None,
            appdata: None,
            local_appdata: None,
            program_files: None,
            program_files_x86: None,
            pnpm_home: None,
            system_root: None,
        };
        let dirs = default_path_dirs(Platform::Win, &roots);
        assert_eq!(dirs[0], r"C:\Windows\system32");
    }

    // ── compose_path_env(골든 테스트 — 설계 §C.3.2) ──

    #[test]
    fn compose_path_env_mac_matches_full_string_exactly() {
        let roots = mac_roots();
        let path = compose_path_env(Platform::Mac, Some("/opt/homebrew/bin/brew"), &roots);
        // runner_path의 bin 디렉터리(/opt/homebrew/bin)가 default_path_dirs에도
        // 이미 있는 값이라 중복 배제 규칙(dirs.contains 검사)이 걸려 한 번만
        // 나와야 한다 — build_child_path_env_prioritizes_runner_bin_dir(기존
        // 테스트)의 전제와 동일하다.
        assert_eq!(
            path,
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        );
    }

    #[test]
    fn compose_path_env_mac_none_runner_matches_defaults_exactly() {
        let roots = mac_roots();
        let path = compose_path_env(Platform::Mac, None, &roots);
        assert_eq!(
            path,
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        );
    }

    #[test]
    fn compose_path_env_win_prioritizes_runner_bin_dir() {
        let roots = win_roots();
        let path = compose_path_env(
            Platform::Win,
            Some(r"C:\Program Files\nodejs\npm.cmd"),
            &roots,
        );
        assert_eq!(
            path,
            r"C:\Program Files\nodejs;C:\Windows\system32;C:\Windows;C:\Windows\System32\Wbem;C:\Users\hopegiver\AppData\Roaming\npm;C:\Users\hopegiver\AppData\Local\pnpm;C:\Users\hopegiver\AppData\Local\Microsoft\WindowsApps"
        );
    }

    #[test]
    fn compose_path_env_excludes_empty_entry_for_bare_name_on_both_platforms() {
        let roots = mac_roots();
        let mac_path = compose_path_env(Platform::Mac, Some("brew"), &roots);
        assert!(!mac_path.split(':').any(|p| p.is_empty()));

        let win_roots = win_roots();
        let win_path = compose_path_env(Platform::Win, Some("gh"), &win_roots);
        assert!(!win_path.split(';').any(|p| p.is_empty()));
    }

    // ── path_scan_candidates ──

    #[test]
    fn path_scan_candidates_win_produces_extension_combinations_per_dir() {
        let candidates = path_scan_candidates(
            Platform::Win,
            r"C:\Windows\system32;C:\Users\hopegiver\AppData\Roaming\npm",
            "npm",
        );
        assert_eq!(
            candidates,
            vec![
                r"C:\Windows\system32\npm.exe".to_string(),
                r"C:\Windows\system32\npm.cmd".to_string(),
                r"C:\Windows\system32\npm.bat".to_string(),
                r"C:\Users\hopegiver\AppData\Roaming\npm\npm.exe".to_string(),
                r"C:\Users\hopegiver\AppData\Roaming\npm\npm.cmd".to_string(),
                r"C:\Users\hopegiver\AppData\Roaming\npm\npm.bat".to_string(),
            ]
        );
    }

    #[test]
    fn path_scan_candidates_skips_empty_dir_entries_to_avoid_cwd_search() {
        // 선행 리뷰(G-1/G-3): PATH의 빈 항목은 CWD를 의미하므로, 빈 항목이 있어도
        // 그 자리에서 후보를 만들어내면 안 된다(CWD 하이재킹 표면 생성 금지).
        let candidates = path_scan_candidates(Platform::Win, r";C:\Windows;", "gh");
        assert_eq!(
            candidates,
            vec![
                r"C:\Windows\gh.exe".to_string(),
                r"C:\Windows\gh.cmd".to_string(),
                r"C:\Windows\gh.bat".to_string(),
            ]
        );
    }

    #[test]
    fn path_scan_candidates_mac_has_no_extension() {
        let candidates = path_scan_candidates(Platform::Mac, "/usr/bin:/bin", "gh");
        assert_eq!(
            candidates,
            vec!["/usr/bin/gh".to_string(), "/bin/gh".to_string()]
        );
    }

    // ── path_scan_candidates 절대경로 전용 골든 테스트(B3, 2라운드 차단) ──

    #[test]
    fn path_scan_candidates_golden_mixed_empty_dot_relative_and_absolute_entries() {
        // 합성 PATH: 빈 항목(연속 세미콜론) + "." + 상대 디렉터리("bin") +
        // 절대경로 2개를 섞는다 — 절대경로 항목에서만 후보가 나와야 한다.
        let candidates = path_scan_candidates(Platform::Win, r"C:\a;;.;bin;C:\b", "gh");
        assert_eq!(
            candidates,
            vec![
                r"C:\a\gh.exe".to_string(),
                r"C:\a\gh.cmd".to_string(),
                r"C:\a\gh.bat".to_string(),
                r"C:\b\gh.exe".to_string(),
                r"C:\b\gh.cmd".to_string(),
                r"C:\b\gh.bat".to_string(),
            ]
        );
    }

    #[test]
    fn path_scan_candidates_rejects_drive_relative_dot_and_lone_backslash_entries() {
        // "C:"(드라이브 상대), "."/".."(상대), "bin"(상대), "\notunc"(UNC가
        // 아닌 역슬래시 하나짜리)는 전부 절대경로가 아니므로 후보를 만들면
        // 안 된다.
        let candidates = path_scan_candidates(Platform::Win, r"C:;.;..;bin;\notunc", "npm");
        assert!(
            candidates.is_empty(),
            "절대경로가 아닌 PATH 항목에서 후보가 나왔습니다: {candidates:?}"
        );
    }

    #[test]
    fn path_scan_candidates_accepts_unc_path_entries() {
        let candidates = path_scan_candidates(Platform::Win, r"\\fileserver\tools", "gh");
        assert_eq!(
            candidates,
            vec![
                r"\\fileserver\tools\gh.exe".to_string(),
                r"\\fileserver\tools\gh.cmd".to_string(),
                r"\\fileserver\tools\gh.bat".to_string(),
            ]
        );
    }

    #[test]
    fn path_scan_candidates_mac_rejects_relative_entries() {
        let candidates = path_scan_candidates(Platform::Mac, "bin:.:/usr/bin", "gh");
        assert_eq!(candidates, vec!["/usr/bin/gh".to_string()]);
    }

    // ── dir_in_path_var ──

    #[test]
    fn dir_in_path_var_win_is_case_insensitive_and_trailing_slash_tolerant() {
        let path_var = r"C:\Windows\system32;c:\users\hopegiver\appdata\roaming\npm\";
        assert!(dir_in_path_var(
            Platform::Win,
            r"C:\Users\hopegiver\AppData\Roaming\npm",
            path_var
        ));
        assert!(!dir_in_path_var(Platform::Win, r"C:\NotThere", path_var));
    }

    #[test]
    fn dir_in_path_var_mac_is_exact() {
        let path_var = "/opt/homebrew/bin:/usr/bin";
        assert!(dir_in_path_var(Platform::Mac, "/opt/homebrew/bin", path_var));
        assert!(!dir_in_path_var(Platform::Mac, "/usr/local/bin", path_var));
    }

    // ── quote_token / build_terminal_command_line ──

    #[test]
    fn quote_token_mac_leaves_bare_paths_unquoted_matching_current_behavior() {
        assert_eq!(quote_token(Platform::Mac, "/opt/homebrew/bin/gh"), "/opt/homebrew/bin/gh");
        assert_eq!(quote_token(Platform::Mac, "auth"), "auth");
    }

    #[test]
    fn quote_token_mac_escapes_special_characters() {
        assert_eq!(
            quote_token(Platform::Mac, "has space"),
            "'has space'"
        );
        assert_eq!(quote_token(Platform::Mac, "it's"), r#"'it'\''s'"#);
    }

    #[test]
    fn quote_token_win_always_single_quotes_and_escapes_embedded_quotes() {
        assert_eq!(quote_token(Platform::Win, "auth"), "'auth'");
        assert_eq!(
            quote_token(Platform::Win, r"C:\Program Files\GitHub CLI\gh.exe"),
            r"'C:\Program Files\GitHub CLI\gh.exe'"
        );
        assert_eq!(quote_token(Platform::Win, "it's"), "'it''s'");
    }

    #[test]
    fn build_terminal_command_line_mac_matches_existing_format_string_byte_for_byte() {
        // 기존 github_integration.rs가 만들던 `format!("{gh} auth login")`과
        // 바이트 단위로 동일해야 한다(회귀 없음, E.3).
        let line = build_terminal_command_line(Platform::Mac, "/opt/homebrew/bin/gh", &["auth", "login"]);
        assert_eq!(line, "/opt/homebrew/bin/gh auth login");
    }

    #[test]
    fn build_terminal_command_line_win_uses_call_operator_and_quotes_program() {
        let line = build_terminal_command_line(
            Platform::Win,
            r"C:\Program Files\GitHub CLI\gh.exe",
            &["auth", "login"],
        );
        assert_eq!(
            line,
            r"& 'C:\Program Files\GitHub CLI\gh.exe' 'auth' 'login'"
        );
    }

    #[test]
    fn quote_token_win_neutralizes_dollar_and_backtick_injection_attempts() {
        // G-7: 작은따옴표 안에서는 PowerShell이 변수·서브식 확장을 하지 않는다
        // — 악성 토큰을 넣어도 리터럴 텍스트로만 남아야 한다.
        let malicious = "$(Remove-Item -Recurse C:\\)";
        let quoted = quote_token(Platform::Win, malicious);
        assert_eq!(quoted, "'$(Remove-Item -Recurse C:\\)'");
        assert!(!quoted.contains("''") || quoted == "'$(Remove-Item -Recurse C:\\)'");
    }

    // 보강 지시: `'`, 공백, `$`, 백틱, `;`, `&`, 후행 백슬래시, `%VAR%`를 표로
    // 검증한다. Win은 계산된 이스케이프 규칙과 정확히 일치하는지(순수함수
    // 검증), Mac은 실제 `sh -c`로 왕복시켜(subprocess 실측) 원본이 그대로
    // 복원되는지 확인한다.
    #[test]
    fn quote_token_table_covers_shell_metacharacters_both_platforms() {
        let cases: &[&str] = &[
            "'",
            "has space",
            "$HOME",
            "`whoami`",
            "a;b",
            "a&b",
            r"trailing\",
            "%VAR%",
        ];
        for case in cases {
            let win = quote_token(Platform::Win, case);
            assert_eq!(
                win,
                format!("'{}'", case.replace('\'', "''")),
                "Win quote_token 결과가 예상 이스케이프 규칙과 다릅니다: {case:?}"
            );

            let mac = quote_token(Platform::Mac, case);
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {mac}"))
                .output()
                .expect("sh must be available to verify the escape round-trips");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                *case,
                "Mac quote_token 왕복 결과가 원본과 다릅니다: {case:?}"
            );
        }
    }

    // ── build_chained_terminal_command_line ──

    #[test]
    fn build_chained_terminal_command_line_mac_uses_and_and_separator() {
        let line = build_chained_terminal_command_line(
            Platform::Mac,
            &[
                ("/opt/homebrew/bin/claude", &["mcp", "add", "foo"]),
                ("/opt/homebrew/bin/claude", &["mcp", "login", "foo"]),
            ],
        );
        assert_eq!(
            line,
            "/opt/homebrew/bin/claude mcp add foo && /opt/homebrew/bin/claude mcp login foo"
        );
    }

    #[test]
    fn build_chained_terminal_command_line_win_uses_semicolon_not_and_and() {
        // Windows 내장 powershell.exe(5.1)는 `&&`/`||` 파이프라인 체인
        // 연산자를 지원하지 않는다(PowerShell 7+ 전용) — `;`로 이어야 한다.
        let line = build_chained_terminal_command_line(
            Platform::Win,
            &[
                (r"C:\claude.exe", &["mcp", "add", "foo"]),
                (r"C:\claude.exe", &["mcp", "login", "foo"]),
            ],
        );
        assert_eq!(
            line,
            r"& 'C:\claude.exe' 'mcp' 'add' 'foo'; & 'C:\claude.exe' 'mcp' 'login' 'foo'"
        );
        assert!(!line.contains("&&"));
    }

    // B1 회귀(2라운드 차단): mcp_login의 `name`은 `claude mcp list` 파싱 결과라
    // 외부(클론한 저장소의 .mcp.json)가 채울 수 있는 값이다. argv 배열의 한
    // 원소로만 들어가면(문자열 포맷팅 조립이 아니라) 명령 구분자로 해석되지
    // 않아야 한다 — 실제 `sh -c`로 실행해 부작용(파일 생성)이 일어나지 않는지
    // 확인한다(추측이 아니라 셸 자체로 실측 — Mac 분기).
    #[test]
    fn build_terminal_command_line_neutralizes_malicious_mcp_server_name_as_single_arg() {
        let marker = std::env::temp_dir().join(format!(
            "malgn_vscode_b1_injection_marker_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let malicious = format!("x'; touch {} ; '", marker.to_string_lossy());

        let mac_line =
            build_terminal_command_line(Platform::Mac, "/bin/echo", &["mcp", "login", &malicious]);
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&mac_line)
            .output()
            .expect("sh must be available");
        assert!(
            !marker.exists(),
            "quote_token이 안전하게 인용하지 못해 인젝션된 touch가 실행됐습니다: {mac_line}"
        );
        let _ = std::fs::remove_file(&marker);
        assert!(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .ends_with(&malicious));

        // Windows(powershell.exe 5.1)는 이 머신에서 실행할 수 없다 — quote_token의
        // 결정적 이스케이프 규칙(작은따옴표 안에서는 `;`가 문장 구분자로
        // 해석되지 않는다, G-7)을 순수함수 등식으로 대신 검증한다.
        let win_line =
            build_terminal_command_line(Platform::Win, r"C:\claude.exe", &["mcp", "login", &malicious]);
        assert_eq!(
            win_line,
            format!(
                "& 'C:\\claude.exe' 'mcp' 'login' '{}'",
                malicious.replace('\'', "''")
            )
        );
    }

    // ── windows_system_tool(B2, 2라운드 차단) ──

    #[test]
    fn windows_system_tool_joins_system_root_with_relative_path() {
        let roots = win_roots();
        assert_eq!(
            windows_system_tool(&roots, r"System32\taskkill.exe"),
            r"C:\Windows\System32\taskkill.exe"
        );
        assert_eq!(
            windows_system_tool(&roots, r"System32\WindowsPowerShell\v1.0\powershell.exe"),
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
        );
    }

    #[test]
    fn windows_system_tool_falls_back_to_c_windows_when_system_root_unset() {
        let roots = EnvRoots {
            home: None,
            appdata: None,
            local_appdata: None,
            program_files: None,
            program_files_x86: None,
            pnpm_home: None,
            system_root: None,
        };
        assert_eq!(
            windows_system_tool(&roots, r"System32\taskkill.exe"),
            r"C:\Windows\System32\taskkill.exe"
        );
    }

    // ── child_current_dir(N1, 2라운드 비차단) ──

    #[test]
    fn child_current_dir_mac_is_none_no_behavior_change() {
        let roots = mac_roots();
        assert_eq!(child_current_dir(Platform::Mac, &roots), None);
    }

    #[test]
    fn child_current_dir_win_pins_to_system_root() {
        let roots = win_roots();
        assert_eq!(
            child_current_dir(Platform::Win, &roots),
            Some(PathBuf::from(r"C:\Windows"))
        );
    }

    // ── resolve_binary_with ──

    #[test]
    fn resolve_binary_with_mac_only_checks_absolute_candidates_never_scans_path() {
        let found = resolve_binary_with(
            Platform::Mac,
            &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"],
            "/usr/bin:/bin",
            "gh",
            &|p| p == "/usr/local/bin/gh",
        );
        assert_eq!(found, Some("/usr/local/bin/gh".to_string()));

        // 절대 후보가 전부 실패하면 Mac은 None(서브프로세스 프로브는 이 함수
        // 밖에서 처리된다 — 이 함수는 순수해야 하므로 스폰하지 않는다).
        let not_found = resolve_binary_with(
            Platform::Mac,
            &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"],
            "/usr/bin:/bin",
            "gh",
            &|_| false,
        );
        assert_eq!(not_found, None);
    }

    #[test]
    fn resolve_binary_with_win_falls_back_to_path_scan_without_spawning_anything() {
        let target = r"C:\Users\hopegiver\AppData\Roaming\npm\gh.cmd";
        let found = resolve_binary_with(
            Platform::Win,
            &[r"C:\Program Files\GitHub CLI\gh.exe"],
            r"C:\Windows\system32;C:\Users\hopegiver\AppData\Roaming\npm",
            "gh",
            &|p| p == target,
        );
        assert_eq!(found, Some(target.to_string()));
    }

    #[test]
    fn resolve_binary_with_win_returns_none_when_nothing_matches() {
        let found = resolve_binary_with(
            Platform::Win,
            &[r"C:\Program Files\GitHub CLI\gh.exe"],
            r"C:\Windows\system32",
            "gh",
            &|_| false,
        );
        assert_eq!(found, None);
    }

    // ── platform_now (잔여 cfg 표면 자체는 CI만 검증 가능하지만, 이 머신에서
    // 실행되는 값이 Mac이어야 한다는 사실 자체는 검증할 수 있다) ──

    #[test]
    fn platform_now_is_mac_on_this_development_machine() {
        assert_eq!(platform_now(), Platform::Mac);
    }

    #[test]
    fn path_exists_non_windows_matches_is_file_semantics() {
        assert!(path_exists("/usr/bin/env") || path_exists("/bin/sh"));
        assert!(!path_exists("/definitely/does/not/exist/malgn_vscode"));
    }
