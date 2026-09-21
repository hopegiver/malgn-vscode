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

    // 플랫폼 전제: 실제 호스트가 Mac일 때만 성립(PathBuf::join이 실제 OS의
    // 구분자로 렌더링하므로, Platform::Mac을 논리 인자로 넘겨도 Windows
    // 호스트에서는 결과 문자열의 구분자가 달라진다) — CI의 windows-latest에서는
    // 컴파일 자체를 건너뛴다.
    #[cfg(target_os = "macos")]
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
        // 갱신(2026-09-21, hub 이슈 01m30vamyw8z58b9pk8h5epmgh): git 3개 후보
        // 디렉터리(ProgramFiles/ProgramFiles(x86)/LOCALAPPDATA\Programs)가
        // 추가됐다 — `claude`가 내부에서 bare `git`을 호출할 때 이 PATH로
        // 찾는데 git 디렉터리가 전혀 없어 "Command 'git' not found or is in
        // an unsafe location"으로 카탈로그 업데이트가 실패했다.
        //
        // 추가 갱신(같은 날, 같은 이슈의 후속 대조): winget이 Node를
        // portable(zip) 변형으로 설치하면 node.exe가 `WinGet\Links`에만
        // 놓이고 npm/pnpm `.cmd` shim이 내부에서 bare `node`를 호출하므로,
        // 이 디렉터리도 같은 이유로 추가했다(WindowsApps와는 다른 디렉터리).
        assert_eq!(
            dirs,
            vec![
                r"C:\Windows\system32".to_string(),
                r"C:\Windows".to_string(),
                r"C:\Windows\System32\Wbem".to_string(),
                r"C:\Program Files\nodejs".to_string(),
                r"C:\Program Files\Git\cmd".to_string(),
                r"C:\Program Files (x86)\Git\cmd".to_string(),
                r"C:\Users\hopegiver\AppData\Roaming\npm".to_string(),
                r"C:\Users\hopegiver\AppData\Local\pnpm".to_string(),
                r"C:\Users\hopegiver\AppData\Local\Programs\nodejs".to_string(),
                r"C:\Users\hopegiver\AppData\Local\Programs\Git\cmd".to_string(),
                r"C:\Users\hopegiver\AppData\Local\Microsoft\WindowsApps".to_string(),
                r"C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Links".to_string(),
            ]
        );
    }

    // 실사용자 재현 회귀 테스트(2026-09-21, hub 이슈
    // 01m30vamyw8z58b9pk8h5epmgh, 원문 재현): 카탈로그 "업데이트" 버튼 →
    // `claude plugin marketplace update` → 내부 `git clone`이 "Command 'git'
    // not found or is in an unsafe location (current directory)"로 실패했다.
    // `default_path_dirs_win_includes_both_known_node_install_dirs_for_cmd_shim_execution`
    // 이 Node에 대해 하는 것과 같은 형태 — git 설치 디렉터리 셋 중 최소
    // 하나가 아니라 **셋 다**(ProgramFiles/ProgramFiles(x86)/사용자 스코프)
    // 있어야 한다는 사실을 이름으로 못박는다.
    #[test]
    fn default_path_dirs_win_includes_all_three_known_git_install_dirs_for_claude_internal_git_calls() {
        let roots = win_roots();
        let dirs = default_path_dirs(Platform::Win, &roots);
        assert!(
            dirs.contains(&r"C:\Program Files\Git\cmd".to_string()),
            "Git for Windows 64비트 기본 설치 위치가 없습니다 — claude 내부의 \
             bare `git` 호출이 이 디렉터리 없이는 실패합니다"
        );
        assert!(
            dirs.contains(&r"C:\Program Files (x86)\Git\cmd".to_string()),
            "Git for Windows 32비트 설치 위치가 없습니다"
        );
        assert!(
            dirs.contains(&r"C:\Users\hopegiver\AppData\Local\Programs\Git\cmd".to_string()),
            "사용자 스코프(PortableGit류) Git 설치 위치가 없습니다"
        );
    }

    // 구조 연결 대안(2026-09-21, hub 이슈 01m30vamyw8z58b9pk8h5epmgh, (b)
    // 판단): default_path_dirs가 DEV_TOOLS::windows_path_candidates에서
    // 디렉터리를 실행 시점에 자동 유도하도록 만들지 **않았다** — 두 목록은
    // 같은 개념이 아니기 때문이다. DEV_TOOLS.windows_path_candidates는
    // "그 도구 자신의 바이너리가 있을 만한 모든 자리"(탐지용, 넓게 잡아도
    // 안전 — resolve_tool_path가 exists 확인 후에만 쓴다)이고,
    // default_path_dirs는 "다른 스폰된 프로세스(claude.exe, npm .cmd shim
    // 등)가 내부에서 bare-name으로 의존하는 도구를 찾을 최소 PATH
    // 백본"(자식 프로세스에 그대로 주입되는 제어된 좁은 표면, G-1/G-3)이다.
    // 전량 자동 유도는 GitHub CLI 자기 설치 경로나 WinGet\Links(앱 실행
    // 별칭, claude/gh/node 자기 탐지용)처럼 "다른 도구가 의존하지 않는"
    // 위치까지 child PATH에 얹어 탐색 표면을 근거 없이 넓힌다(아래 (c)
    // 대조 결과 참고) — 게다가 mod.rs가 platform.rs를 쓰는 현재의 단방향
    // 참조를 역전시켜야 해서(순환은 cli_launcher.rs 선례처럼 컴파일은
    // 되지만) 이득 대비 불필요한 결합을 늘린다. 대신 이 테스트가 "정본
    // 어긋남 감지" 역할을 한다: DEV_TOOLS의 git 후보에서 유도한 디렉터리가
    // default_path_dirs에 전부 들어있는지 매 테스트 실행마다 대조한다 —
    // git 후보를 늘리거나 바꾸고 default_path_dirs를 갱신하지 않으면 이
    // 테스트가 즉시 실패한다.
    #[test]
    fn default_path_dirs_win_contains_every_dir_derived_from_git_windows_path_candidates() {
        let roots = win_roots();
        let git_def = crate::dev_tools::DEV_TOOLS
            .iter()
            .find(|d| d.key == "git")
            .expect("DEV_TOOLS에 git 항목이 있어야 합니다");
        let dirs = default_path_dirs(Platform::Win, &roots);
        for candidate in git_def.windows_path_candidates {
            let expanded = expand_path_tokens(Platform::Win, candidate, &roots)
                .expect("win_roots()는 모든 토큰의 뿌리를 채워야 합니다");
            let parent = windows_parent_dir(&expanded)
                .expect("git 후보는 항상 '디렉터리\\파일명' 형태여야 합니다");
            assert!(
                dirs.contains(&parent),
                "DEV_TOOLS의 git 후보 '{candidate}'(전개: {expanded})의 디렉터리 \
                 '{parent}'가 default_path_dirs(Windows)에 없습니다 — DEV_TOOLS \
                 쪽 git 후보를 늘리거나 바꿨다면 default_path_dirs도 함께 \
                 갱신해야 합니다(이슈 01m30vamyw8z58b9pk8h5epmgh)."
            );
        }
    }

    // git과 대칭(2026-09-21, 같은 이슈의 후속 대조 — PM 판정): "다른 스폰된
    // 프로세스가 내부에서 bare-name으로 의존한다"는 기준을 WinGet\Links가
    // Node에 대해서는 충족한다 — DEV_TOOLS Node.windows_path_candidates의
    // 셋째 항목(winget portable 설치 시 node.exe가 놓이는 유일한 자리)이고,
    // npm/pnpm `.cmd` shim이 그 node.exe를 bare `node`로 찾는다. 반면
    // Claude(`%USERPROFILE%\.local\bin`)/GitHub CLI(`Program Files\GitHub
    // CLI`)는 앱이 항상 절대경로로 spawn해 이 기준을 충족하지 않으므로
    // default_path_dirs에 추가하지 않았다 — 이 테스트는 Node 쪽만
    // 검사한다(위 git 테스트와 동일하게, DEV_TOOLS의 Node 후보를 늘리거나
    // 바꾸고 default_path_dirs를 갱신하지 않으면 즉시 실패해 드리프트를
    // 잡는다).
    #[test]
    fn default_path_dirs_win_contains_every_dir_derived_from_node_windows_path_candidates() {
        let roots = win_roots();
        let node_def = crate::dev_tools::DEV_TOOLS
            .iter()
            .find(|d| d.key == "node")
            .expect("DEV_TOOLS에 node 항목이 있어야 합니다");
        let dirs = default_path_dirs(Platform::Win, &roots);
        for candidate in node_def.windows_path_candidates {
            let expanded = expand_path_tokens(Platform::Win, candidate, &roots)
                .expect("win_roots()는 모든 토큰의 뿌리를 채워야 합니다");
            let parent = windows_parent_dir(&expanded)
                .expect("node 후보는 항상 '디렉터리\\파일명' 형태여야 합니다");
            assert!(
                dirs.contains(&parent),
                "DEV_TOOLS의 node 후보 '{candidate}'(전개: {expanded})의 디렉터리 \
                 '{parent}'가 default_path_dirs(Windows)에 없습니다 — DEV_TOOLS \
                 쪽 node 후보를 늘리거나 바꿨다면 default_path_dirs도 함께 \
                 갱신해야 합니다(이슈 01m30vamyw8z58b9pk8h5epmgh)."
            );
        }
    }

    // 실사용자 재현 회귀 테스트(2026-09-18): npm 전역 설치 wrangler.cmd는
    // `%APPDATA%\npm`(node.exe 없음)에 있고, 그 shim이 내부에서 bare `node`를
    // 호출한다 — 이 함수가 만드는 기본 PATH에 Node 설치 디렉터리가 반드시
    // 있어야 그 호출이 성공한다. `%ProgramFiles%`/`%LOCALAPPDATA%\Programs`
    // 둘 다 없는 극단적 환경(테스트 편의상 win_roots 그대로 사용)에서도 최소
    // 하나는 포함돼야 한다는 사실 자체를 이름으로 못박는다.
    #[test]
    fn default_path_dirs_win_includes_both_known_node_install_dirs_for_cmd_shim_execution() {
        let roots = win_roots();
        let dirs = default_path_dirs(Platform::Win, &roots);
        assert!(
            dirs.contains(&r"C:\Program Files\nodejs".to_string()),
            "wrangler.cmd 등 npm shim의 내부 bare `node` 호출이 이 디렉터리 없이는 실패합니다"
        );
        assert!(
            dirs.contains(&r"C:\Users\hopegiver\AppData\Local\Programs\nodejs".to_string()),
            "사용자별 Node 설치(공식 인스톨러의 '현재 사용자만' 옵션)도 커버해야 합니다"
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
        // runner_path 자신의 bin 디렉터리(C:\Program Files\nodejs)가
        // default_path_dirs에도 이미 있는 값이라 중복 배제 규칙에 걸려
        // 한 번만 나온다(기존 계약 무변경) — 그래서 이 테스트는 우연히도
        // node 디렉터리가 이미 있던 케이스라 아래 신규 테스트
        // (`compose_path_env_win_finds_node_dir_for_npm_global_wrangler_shim`)가
        // 실제 회귀(런너 자신이 node.exe와 다른 디렉터리에 있는 경우)를 잡는다.
        assert_eq!(
            path,
            r"C:\Program Files\nodejs;C:\Windows\system32;C:\Windows;C:\Windows\System32\Wbem;C:\Program Files\Git\cmd;C:\Program Files (x86)\Git\cmd;C:\Users\hopegiver\AppData\Roaming\npm;C:\Users\hopegiver\AppData\Local\pnpm;C:\Users\hopegiver\AppData\Local\Programs\nodejs;C:\Users\hopegiver\AppData\Local\Programs\Git\cmd;C:\Users\hopegiver\AppData\Local\Microsoft\WindowsApps;C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Links"
        );
    }

    // 실사용자 재현 회귀 테스트(2026-09-18, 원문 재현):
    // `where wrangler` → `C:\Users\user\AppData\Roaming\npm\wrangler.cmd`.
    // 이 경로는 mod.rs DEV_TOOLS의 하드코딩 후보 `%APPDATA%\npm\wrangler.cmd`와
    // 정확히 일치해 `resolve_tool_path`는 이 파일을 정확히 찾아낸다(§B.5
    // 하드코딩 후보 자체는 문제가 없었다) — 그런데도 앱이 "설치 안 됨"으로
    // 표시된 진짜 원인은 그 다음 단계, 즉 `check_tool_version`이 이 경로로
    // `--version`을 실행할 때 주입하는 PATH(`build_child_path_env` →
    // `compose_path_env`)에 Node 설치 디렉터리가 없어 wrangler.cmd 내부의
    // bare `node` 호출이 실패하는 것이었다. 이 테스트는 그 정확한 시나리오를
    // 재현한다: runner_path(=resolved wrangler.cmd)가 node.exe와 **다른**
    // 디렉터리(AppData\Roaming\npm)에 있으므로, 수정 전 코드에서는 이
    // `assert!`가 실패했을 것이다(default_path_dirs에 Node 디렉터리가 전혀
    // 없었으므로).
    #[test]
    fn compose_path_env_win_includes_node_dir_when_npm_global_wrangler_shim_lives_elsewhere() {
        let roots = win_roots();
        let path = compose_path_env(
            Platform::Win,
            Some(r"C:\Users\hopegiver\AppData\Roaming\npm\wrangler.cmd"),
            &roots,
        );
        assert!(
            path.split(';').any(|p| p == r"C:\Program Files\nodejs"),
            "wrangler.cmd shim의 내부 bare `node` 호출이 찾을 수 있는 PATH에 \
             Node 설치 디렉터리가 없습니다 — 이 회귀가 재발하면 npm 전역 \
             설치본이 다시 '설치 안 됨'으로 오판됩니다. 실제 PATH: {path}"
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
    // 검증한다. Mac은 실제 `sh -c`로 왕복시켜(subprocess 실측) 원본이 그대로
    // 복원되는지 확인한다.
    //
    // M3 수정(review-devtools-windows-parity-2026-09-15.md): Win 쪽은 이전에
    // `format!("'{}'", case.replace('\'', "''"))`로 quote_token **자신의
    // 구현식을 그대로 재작성**해 비교했다 — 구현이 틀려도 기대값이 같이
    // 틀려 절대 실패하지 않는 동어반복이었다. 이제 **미리 계산해 고정한
    // 리터럴** 표와 비교한다(quote_token 호출부와 독립적으로 값을 고정) —
    // `quote_token_win_always_single_quotes_and_escapes_embedded_quotes`가
    // 이미 쓰던 것과 같은 패턴이다. 이 표조차도 "PowerShell 파서가 실제로
    // 이렇게 해석하는가"는 검증하지 못한다 — 그건
    // `quote_token_win_round_trips_through_real_powershell_encoded_command`
    // (`#[cfg(windows)]`, 이 파일 하단)가 실제 `powershell.exe`로 왕복시켜
    // 검증한다(이 머신에서는 실행 불가 — windows-latest CI 러너 전용, 그리고
    // 이 브랜치가 아직 push되지 않아 당장은 그 러너에서도 돌지 않는다).
    #[test]
    fn quote_token_table_covers_shell_metacharacters_both_platforms() {
        // (입력, Windows에서 기대하는 고정 리터럴) — quote_token의 코드를
        // 다시 실행해 만들지 않고 손으로 미리 계산해 둔 값이다.
        let win_cases: &[(&str, &str)] = &[
            ("'", "''''"),
            ("has space", "'has space'"),
            ("$HOME", "'$HOME'"),
            ("`whoami`", "'`whoami`'"),
            ("a;b", "'a;b'"),
            ("a&b", "'a&b'"),
            (r"trailing\", r"'trailing\'"),
            ("%VAR%", "'%VAR%'"),
        ];
        for (case, expected_win) in win_cases {
            let win = quote_token(Platform::Win, case);
            assert_eq!(
                win, *expected_win,
                "Win quote_token 결과가 고정된 기대값과 다릅니다: {case:?}"
            );
        }

        for (case, _) in win_cases {
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

    // ── build_chained_terminal_command_line_with_env(env=[] — 체인 구분자 자체의
    // 회귀 고정. 2026-09-18: mcp_install이 이 with_env 버전으로 옮겨가면서
    // env가 없는 무인자 변형(구 `build_chained_terminal_command_line`)은
    // 삭제하고 이 두 테스트를 그대로 옮겼다 — 검증 대상(구분자 선택)은
    // 무변경) ──

    #[test]
    fn build_chained_terminal_command_line_with_env_mac_uses_and_and_separator_when_no_env() {
        let line = build_chained_terminal_command_line_with_env(
            Platform::Mac,
            &[
                (
                    "/opt/homebrew/bin/claude",
                    &["mcp", "add", "foo"] as &[&str],
                    &[] as &[(&str, &str)],
                ),
                (
                    "/opt/homebrew/bin/claude",
                    &["mcp", "login", "foo"] as &[&str],
                    &[] as &[(&str, &str)],
                ),
            ],
        );
        assert_eq!(
            line,
            "/opt/homebrew/bin/claude mcp add foo && /opt/homebrew/bin/claude mcp login foo"
        );
    }

    #[test]
    fn build_chained_terminal_command_line_with_env_win_uses_semicolon_not_and_and_when_no_env() {
        // Windows 내장 powershell.exe(5.1)는 `&&`/`||` 파이프라인 체인
        // 연산자를 지원하지 않는다(PowerShell 7+ 전용) — `;`로 이어야 한다.
        let line = build_chained_terminal_command_line_with_env(
            Platform::Win,
            &[
                (
                    r"C:\claude.exe",
                    &["mcp", "add", "foo"] as &[&str],
                    &[] as &[(&str, &str)],
                ),
                (
                    r"C:\claude.exe",
                    &["mcp", "login", "foo"] as &[&str],
                    &[] as &[(&str, &str)],
                ),
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

        // Windows(powershell.exe 5.1)는 이 머신에서 실행할 수 없다 — `malicious`가
        // 프로세스 id로 만든 런타임 값(마커 경로)을 담고 있어 정적 리터럴로
        // 고정할 수 없으므로, 여기서는 quote_token의 결정적 이스케이프 규칙만
        // 등식으로 확인한다(이 등식 하나만으로는 PowerShell 파서가 실제로
        // 안전하게 해석하는지 증명하지 못한다 — 알려진 한계). 실제
        // `powershell.exe`로 이 시나리오 자체를 왕복 검증하는 것은
        // `build_terminal_command_line_win_neutralizes_malicious_mcp_server_name_via_real_powershell`
        // (`#[cfg(windows)]`, 아래)이 담당한다 — 그게 이 테스트의 Windows
        // 쪽 권위 있는 검증이다.
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

    // ── build_terminal_command_line_with_env / build_chained_terminal_command_line_with_env
    // (2026-09-18, `claude mcp add --client-secret` 실측 이후 도입) ──

    #[test]
    fn build_terminal_command_line_with_env_mac_prefixes_inline_assignment() {
        let line = build_terminal_command_line_with_env(
            Platform::Mac,
            "/opt/homebrew/bin/claude",
            &["mcp", "add", "gmail"],
            &[("MCP_CLIENT_SECRET", "GOCSPX-abc")],
        );
        assert_eq!(
            line,
            "MCP_CLIENT_SECRET=GOCSPX-abc /opt/homebrew/bin/claude mcp add gmail"
        );
    }

    #[test]
    fn build_terminal_command_line_with_env_mac_quotes_special_char_values() {
        let line = build_terminal_command_line_with_env(
            Platform::Mac,
            "/opt/homebrew/bin/claude",
            &["mcp", "add", "gmail"],
            &[("MCP_CLIENT_SECRET", "it's a secret")],
        );
        assert_eq!(
            line,
            r#"MCP_CLIENT_SECRET='it'\''s a secret' /opt/homebrew/bin/claude mcp add gmail"#
        );
    }

    #[test]
    fn build_terminal_command_line_with_env_empty_env_matches_plain_variant_byte_for_byte() {
        let plain = build_terminal_command_line(Platform::Mac, "/bin/echo", &["hi"]);
        let with_env = build_terminal_command_line_with_env(Platform::Mac, "/bin/echo", &["hi"], &[]);
        assert_eq!(plain, with_env);
    }

    #[test]
    fn build_terminal_command_line_with_env_win_sets_and_clears_env_var() {
        let line = build_terminal_command_line_with_env(
            Platform::Win,
            r"C:\claude.exe",
            &["mcp", "add", "gmail"],
            &[("MCP_CLIENT_SECRET", "GOCSPX-abc")],
        );
        assert_eq!(
            line,
            r"$env:MCP_CLIENT_SECRET = 'GOCSPX-abc'; & 'C:\claude.exe' 'mcp' 'add' 'gmail'; Remove-Item Env:MCP_CLIENT_SECRET -ErrorAction SilentlyContinue"
        );
    }

    // POSIX `KEY=value cmd` 인라인 접두사가 실제로 자식 프로세스 환경에
    // 도달하는지 `sh -c` 실측으로 확인한다(추측이 아니라 셸 자체로 검증 —
    // 이 파일의 기존 관례와 동일).
    #[test]
    fn build_terminal_command_line_with_env_mac_actually_injects_env_var_via_real_shell() {
        let line = build_terminal_command_line_with_env(
            Platform::Mac,
            "/bin/sh",
            &["-c", "printf %s \"$MCP_CLIENT_SECRET\""],
            &[("MCP_CLIENT_SECRET", "GOCSPX-real-secret")],
        );
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&line)
            .output()
            .expect("sh must be available");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "GOCSPX-real-secret",
            "인라인 env 접두사가 자식 프로세스에 실제로 전달되지 않았습니다: {line}"
        );
    }

    #[test]
    fn build_chained_terminal_command_line_with_env_mac_applies_env_only_to_first_command() {
        let line = build_chained_terminal_command_line_with_env(
            Platform::Mac,
            &[
                (
                    "/opt/homebrew/bin/claude",
                    &["mcp", "add", "gmail"] as &[&str],
                    &[("MCP_CLIENT_SECRET", "GOCSPX-abc")] as &[(&str, &str)],
                ),
                (
                    "/opt/homebrew/bin/claude",
                    &["mcp", "login", "gmail"] as &[&str],
                    &[] as &[(&str, &str)],
                ),
            ],
        );
        assert_eq!(
            line,
            "MCP_CLIENT_SECRET=GOCSPX-abc /opt/homebrew/bin/claude mcp add gmail && /opt/homebrew/bin/claude mcp login gmail"
        );
    }

    // ── #[cfg(windows)] 실제 PowerShell 왕복 검증(M3 처방 a) ──
    //
    // review-devtools-windows-parity-2026-09-15.md M3: 위 두 테스트의 Win
    // 분기는 quote_token의 구현식을 그대로 재작성하거나(수정 전) 고정
    // 리터럴과 비교할 뿐이라(수정 후) PowerShell **자신의 파서**가 실제로
    // 이 인용을 안전하게 되돌리는지는 검증하지 못한다. 아래 두 테스트가 그
    // 마지막 층을 담당한다 — `#[cfg(windows)]`라 이 머신(Mac)에서는 컴파일도
    // 실행도 되지 않고, windows-latest CI 러너에서만 돈다(그리고 이 브랜치가
    // 아직 push되지 않아 당장은 그 러너에서도 돌지 않는다 — 위임서 제약).
    // 프로덕션 경로와 동일하게 `-Command`가 아니라
    // `platform::encode_powershell_command` + `-EncodedCommand`로 넘긴다(M3
    // 처방 b — cli_launcher.rs::spawn_terminal_window와 동형).
    #[cfg(windows)]
    #[test]
    fn quote_token_win_round_trips_through_real_powershell_encoded_command() {
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
            let quoted = quote_token(Platform::Win, case);
            // Write-Output에 그대로 넘겨 PowerShell 자신이 이 인용을 해석한
            // 결과를 stdout으로 받는다 — quote_token의 이스케이프가 실제
            // PowerShell 파서를 통과한 뒤에도 원본과 바이트 단위로 같아야
            // 한다.
            let script = format!("Write-Output {quoted}");
            let encoded = encode_powershell_command(&script);
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-EncodedCommand", &encoded])
                .output()
                .expect("이 CI 러너에는 powershell.exe가 있어야 합니다");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert_eq!(
                stdout.trim_end_matches(['\r', '\n']),
                *case,
                "PowerShell 왕복 결과가 원본과 다릅니다: {case:?}, script={script:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn build_terminal_command_line_win_neutralizes_malicious_mcp_server_name_via_real_powershell() {
        // Mac 쪽은 위 build_terminal_command_line_neutralizes_malicious_mcp_
        // server_name_as_single_arg가 실제 `sh -c`로 실측한다 — 이 테스트가
        // 그 Windows 대응이다. 악성 MCP 서버 이름(외부 출처, `claude mcp
        // list` 파싱 결과일 수 있다)이 PowerShell 명령 구분자(`;`)로
        // 해석되지 않고 하나의 리터럴 인자로만 도달하는지 실제
        // `powershell.exe`로 확인한다.
        let marker = std::env::temp_dir().join(format!(
            "malgn_vscode_b1_win_injection_marker_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let malicious = format!(
            "x'; New-Item -Path '{}' -ItemType File -Force | Out-Null; '",
            marker.to_string_lossy()
        );

        // Write-Output을 프로그램으로 써서 각 인자를 그대로 되돌려 받는다
        // (call 연산자 `&`는 실행파일뿐 아니라 cmdlet 이름도 호출할 수
        // 있다) — Mac 쪽이 `/bin/echo`로 하는 것과 같은 역할.
        let line =
            build_terminal_command_line(Platform::Win, "Write-Output", &["mcp", "login", &malicious]);
        let encoded = encode_powershell_command(&line);
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-EncodedCommand", &encoded])
            .output()
            .expect("이 CI 러너에는 powershell.exe가 있어야 합니다");

        assert!(
            !marker.exists(),
            "quote_token이 안전하게 인용하지 못해 인젝션된 New-Item이 실행됐습니다: {line}"
        );
        let _ = std::fs::remove_file(&marker);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let last_line = stdout.lines().last().unwrap_or("").trim_end_matches(['\r', '\n']);
        assert_eq!(
            last_line, malicious,
            "Write-Output의 마지막 출력이 악성 인자 원본과 다릅니다(분리/변형된 것으로 의심): {line}"
        );
    }

    // ── encode_powershell_command(M3 처방 b, 순수함수 — 이 머신에서 100% 검증) ──
    //
    // review-devtools-windows-parity-2026-09-15.md M3: 이 함수(UTF-16LE 인코딩
    // + base64)는 PowerShell 실행과 무관한 순수 변환이라, 인코딩 결과를
    // 손으로(이 함수 내부 구현을 다시 호출하지 않고) UTF-16LE→base64
    // 디코딩해 원본과 바이트 단위로 비교할 수 있다. PowerShell이 실제로 이
    // 형식을 기대해 올바르게 디코딩·실행한다는 사실 자체는 Microsoft 공식
    // 문서 근거이며 이 머신에서 실행 검증은 불가능하다(미검증 — 위 두
    // `#[cfg(windows)]` 테스트가 그 부분을 담당한다).
    #[test]
    fn encode_powershell_command_round_trips_via_manual_utf16le_base64_decode() {
        use base64::Engine;
        let cases: &[&str] = &[
            "",
            "auth login",
            "'",
            "has space",
            "$HOME",
            "`whoami`",
            "a;b",
            "a&b",
            r"trailing\",
            "%VAR%",
            "한글 사용자명",
        ];
        for case in cases {
            let encoded = encode_powershell_command(case);
            // base64 표준 알파벳만 쓰는지 — 공백·따옴표·`;`·`&` 같은 특수문자가
            // 이 인자 자체에는 전혀 나타나지 않는다는, 이 처방의 핵심 값어치를
            // 직접 확인한다.
            assert!(
                encoded
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')),
                "base64 출력에 예상 밖 문자가 있습니다: {encoded:?}"
            );

            let decoded_bytes = base64::engine::general_purpose::STANDARD
                .decode(&encoded)
                .expect("encode_powershell_command은 항상 유효한 base64를 내야 합니다");
            assert_eq!(
                decoded_bytes.len() % 2,
                0,
                "UTF-16LE는 항상 짝수 바이트여야 합니다: {case:?}"
            );
            let u16s: Vec<u16> = decoded_bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let decoded = String::from_utf16(&u16s)
                .expect("디코딩된 UTF-16LE가 유효한 문자열이어야 합니다");
            assert_eq!(decoded, *case, "왕복 결과가 원본과 다릅니다: {case:?}");
        }
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

    // 플랫폼 전제: 이 개발 머신이 Mac이라는 사실 자체를 검증하는 테스트라 다른
    // OS에서는 성립할 수 없다 — CI의 windows-latest에서는 컴파일을 건너뛴다.
    #[cfg(target_os = "macos")]
    #[test]
    fn platform_now_is_mac_on_this_development_machine() {
        assert_eq!(platform_now(), Platform::Mac);
    }

    // 플랫폼 전제: 이름대로 non-Windows(Mac) 전용 — CI의 windows-latest에서는
    // 컴파일을 건너뛴다.
    #[cfg(not(windows))]
    #[test]
    fn path_exists_non_windows_matches_is_file_semantics() {
        assert!(path_exists("/usr/bin/env") || path_exists("/bin/sh"));
        assert!(!path_exists("/definitely/does/not/exist/malgn_vscode"));
    }
