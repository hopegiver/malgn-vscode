// plan_table.rs의 테스트 모듈(별도 파일 분리 — 1,000줄 규율, classify.rs/
// classify_tests.rs·platform.rs/platform_tests.rs와 동일한 패턴). `#[path]`로
// 연결된 `mod tests`의 본문이다. 정본(결정표·argv 테이블)은 plan_table.rs에
// 그대로 남는다.

use super::*;

#[test]
fn corepack_guard_overrides_before_table_lookup() {
    assert!(!is_corepack_managed(None));
    assert!(!is_corepack_managed(Some("")));
    assert!(is_corepack_managed(Some(
        "/opt/homebrew/lib/node_modules/corepack"
    )));

    // pnpm이 우연히 HomebrewFormula로도 분류될 수 있는 경로라도, corepack 가드가
    // compute_action에서 최우선으로 걸려야 한다. compute_action은 실제로
    // std::env::var를 읽으므로 여기서는 is_corepack_managed 자체의 우선순위
    // 계약만 검증한다(env 변형은 병렬 테스트 안전성을 위해 하지 않는다).
    let method = InstallMethod::HomebrewFormula {
        formula: "pnpm".to_string(),
        keg_version: "11.9.0".to_string(),
        prefix: "/opt/homebrew".to_string(),
    };
    assert!(matches!(
        lookup_action(ToolId::Pnpm, method.kind()),
        Action::Run(_)
    ));
}

// 보안 리뷰(2026-09-21, hub 이슈 01m2wvpx9v548h6yce9jpxpm0w) — Node의 신규
// WinGet\Links 후보(mod.rs)가 실제로 (Node, WingetPackage) 행에 물려 사실대로
// 안내하는지, MANUAL_CLAUDE_WINGET_UNSUPPORTED와 같은 UnsupportedMethod
// 사유인지(=UnknownMethod로 조용히 강등되지 않는지) 엔드투엔드로 검증한다.
#[test]
fn node_winget_links_candidate_routes_to_unsupported_manual_not_unknown() {
    use super::super::classify::classify_install_method_windows;
    use super::super::platform::{EnvRoots, Platform};
    use std::path::{Path, PathBuf};

    let roots = EnvRoots {
        home: Some(PathBuf::from(r"C:\Users\hopegiver")),
        appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming")),
        local_appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Local")),
        program_files: Some(PathBuf::from(r"C:\Program Files")),
        program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
        pnpm_home: None,
        system_root: Some(PathBuf::from(r"C:\Windows")),
    };
    let expanded = r"C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Links\node.exe";
    let method = classify_install_method_windows(Path::new(expanded), &roots);
    assert_eq!(method.kind(), MethodKind::WingetPackage);

    let action = compute_action_for_platform(ToolId::Node, &method, Platform::Win);
    match action {
        Action::Manual(mp) => {
            assert_eq!(mp.reason, ManualReason::UnsupportedMethod);
            assert_eq!(
                mp.copyable_command,
                Some("winget upgrade --id OpenJS.NodeJS.LTS -e --source winget")
            );
        }
        Action::Run(_) => panic!(
            "이번 라운드는 winget upgrade 자동 실행을 새로 열지 않는다 — Manual이어야 합니다"
        ),
    }
}

// 쓰기 권한 검사는 읽기 전용 syscall이라 실제 경로로 안전하게 검증 가능.
#[test]
fn writability_check_matches_known_real_paths() {
    if let Some(home) = dirs::home_dir() {
        assert!(
            is_writable_by_current_user(&home),
            "홈 디렉터리는 현재 사용자 소유라 쓰기 가능해야 합니다"
        );
    }
    assert!(
        !is_writable_by_current_user(Path::new("/System")),
        "/System은 SIP로 보호되어 일반 사용자가 쓸 수 없어야 합니다"
    );
}

// 과제 2(Major-2): compute_action의 brew 쓰기권한 검사가 prefix 자체가 아니라
// <prefix>/Cellar, <prefix>/bin을 보는지 실측으로 확인한다. 홈 디렉터리 아래
// 합성 Cellar/bin을 만들어 "쓰기 가능"과 "존재하지 않음"(→ access(W_OK) 실패)
// 두 경우 모두 검증한다.
#[test]
fn compute_action_checks_prefix_cellar_and_bin_not_prefix_itself() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let tmp_prefix = home.join(format!(
        "malgn_vscode_test_brew_prefix_{}",
        std::process::id()
    ));
    let cellar = tmp_prefix.join("Cellar");
    let bin = tmp_prefix.join("bin");
    std::fs::create_dir_all(&cellar).expect("create synthetic Cellar dir");
    std::fs::create_dir_all(&bin).expect("create synthetic bin dir");

    // 사용자 소유 홈 아래에 만들었으므로 prefix/Cellar, prefix/bin 모두 쓰기
    // 가능해야 한다 — prefix 경로 조립이 `prefix + "/Cellar"`, `prefix + "/bin"`
    // 형태로 올바른지가 핵심 확인 대상이다.
    assert!(is_writable_by_current_user(&cellar));
    assert!(is_writable_by_current_user(&bin));

    let method = InstallMethod::HomebrewFormula {
        formula: "example".to_string(),
        keg_version: "1.0.0".to_string(),
        prefix: tmp_prefix.to_string_lossy().to_string(),
    };
    let action = compute_action(ToolId::Node, &method);
    assert!(
        matches!(action, Action::Run(_)),
        "Cellar·bin 모두 쓰기 가능하면 Manual로 강등되지 않아야 합니다"
    );

    // bin만 지워 "존재하지 않는 경로"(access(W_OK) 실패)를 흉내내면 Manual로
    // 강등되어야 한다 — prefix 자체(tmp_prefix, 여전히 쓰기 가능)만 보는 버그였다면
    // 이 케이스에서 여전히 Action::Run이 나왔을 것이다.
    std::fs::remove_dir_all(&bin).expect("remove synthetic bin dir");
    let action_after_bin_removed = compute_action(ToolId::Node, &method);
    assert!(
        matches!(action_after_bin_removed, Action::Manual(_)),
        "bin이 쓰기 불가(부재)면 Manual로 강등되어야 합니다"
    );

    let _ = std::fs::remove_dir_all(&tmp_prefix);
}

#[test]
fn manual_plan_reasons_are_distinguishable_for_ui_branching() {
    assert_ne!(MANUAL_XCODE_CLT.reason, MANUAL_VERSION_MANAGED.reason);
    assert_ne!(MANUAL_COREPACK_MANAGED.reason, MANUAL_NOT_WRITABLE.reason);
}

// 설계 §B: install_manual_plan()이 이 머신(Mac)에서는 기존 그대로 brew
// 문구를 낸다는 계약(무변경) — install_manual_plan_mac을 직접 호출해
// platform_now()의 실제 플랫폼과 무관하게 회귀를 잡는다.
#[test]
fn install_manual_plan_mac_variant_keeps_brew_wording() {
    let gh = install_manual_plan_mac(ToolId::Gh);
    assert_eq!(gh.copyable_command, Some("brew install gh"));
    let node = install_manual_plan_mac(ToolId::Node);
    assert_eq!(node.copyable_command, Some("brew install node"));
}

// Windows 분기(설계 §B.1, 전부 미검증)는 brew가 아니라 winget 명령을
// 내야 한다 — mac 문구가 그대로 새어나가면 이 작업의 핵심 결함(거짓 안내)이
// 재발한 것이다.
//
// T6(review-devtools-windows-parity-2026-09-15-r3.md) 재발 방지: 예전에는
// 이 가드가 6개 도구를 손으로 나열했다 — 오늘은 6/6이라 우연히 전수였지만
// 7번째 도구가 추가되면 조용히 면제된다.
// `windows_installed_tool_actions_never_reach_macos_only_manual_text`와
// 동형으로 macOS 전용 토큰 부재는 DEV_TOOLS 전체를 구조적으로 순회해
// 검사한다(도구마다 제각각이던 검사를 통일 — 이전엔 gh/node만 일부 검사했다).
// 각 도구의 정확한 명령값(어떤 러너로 무엇을 설치하는지)은 구조적으로 유도할
// 수 없는 도구별 사실이라 그 아래에 리터럴로 남긴다.
#[test]
fn install_manual_plan_windows_variant_uses_winget_not_brew() {
    use super::super::DEV_TOOLS;

    const MAC_ONLY_TOKENS: [&str; 5] = ["softwareupdate", "brew", "Xcode", "xcode-select", "Homebrew"];

    for def in DEV_TOOLS.iter() {
        let plan = install_manual_plan_windows(def.id);
        let text = format!("{} {}", plan.message_ko, plan.copyable_command.unwrap_or(""));
        for token in MAC_ONLY_TOKENS {
            assert!(
                !text.contains(token),
                "{}용 install_manual_plan_windows에 macOS 전용 토큰 '{token}'이 있습니다: {text}",
                def.key
            );
        }
    }

    let gh = install_manual_plan_windows(ToolId::Gh);
    assert!(gh
        .copyable_command
        .unwrap_or("")
        .starts_with("winget install --id GitHub.cli"));

    // hub decisionId 01m2wse823xcszvn7km3vap0qs 이후: install_candidates가
    // winget 후보를 갖게 되면서 이 Manual 분기는 "winget을 찾지 못했다"는
    // 뜻이 됐다 — copyable_command도 실제 RunPlan(RUN_WINGET_INSTALL_NODE/GIT)
    // 인자와 동일하게(--id/-e/--source/--accept-*/--disable-interactivity/
    // --silent 전부 포함) 맞춰 사용자가 그대로 복사해 실행할 수 있게 한다.
    let node = install_manual_plan_windows(ToolId::Node);
    assert_eq!(
        node.copyable_command,
        Some("winget install --id OpenJS.NodeJS.LTS -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent")
    );

    let git = install_manual_plan_windows(ToolId::Git);
    assert_eq!(
        git.copyable_command,
        Some("winget install --id Git.Git -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent")
    );

    let pnpm = install_manual_plan_windows(ToolId::Pnpm);
    assert_eq!(pnpm.copyable_command, Some("winget install pnpm.pnpm"));

    // npm 계열 명령은 크로스플랫폼이라 mac과 동일 값을 재사용해도 된다.
    let claude = install_manual_plan_windows(ToolId::Claude);
    assert_eq!(
        claude.copyable_command,
        Some("npm install -g @anthropic-ai/claude-code")
    );
    let wrangler = install_manual_plan_windows(ToolId::Wrangler);
    assert_eq!(wrangler.copyable_command, Some("npm install -g wrangler"));
}

// T3(review-devtools-windows-parity-2026-09-15-r3.md, 2차 m4 승계) 재발 방지:
// 실행기 해석 실패 강등 문구가 플랫폼별로 실제 후보만 말해야 한다 —
// Windows 사용자에게 brew를, macOS 사용자에게 winget을 보여주면 양방향으로
// 샌다.
#[test]
fn manual_no_runner_plan_never_leaks_other_platforms_runner_name() {
    let mac = manual_no_runner_plan(super::super::platform::Platform::Mac);
    assert!(!mac.message_ko.contains("winget"));
    assert!(mac.message_ko.contains("brew"));
    assert_eq!(mac.reason, ManualReason::NoRunner);

    let win = manual_no_runner_plan(super::super::platform::Platform::Win);
    assert!(!win.message_ko.contains("brew"));
    assert!(win.message_ko.contains("winget"));
    assert_eq!(win.reason, ManualReason::NoRunner);
}

// 설계 §B.3: winget "이미 최신" 종료 코드 상수가 실제 문서값(0x8A15002B)과
// 일치하는지 고정한다 — actions.rs가 이 값을 Outcome::AlreadyLatest로
// 매핑하므로 상수가 틀리면 그 매핑 자체가 조용히 무력화된다.
#[test]
fn winget_already_latest_exit_code_matches_documented_hresult() {
    assert_eq!(WINGET_ALREADY_LATEST_EXIT_CODE, -1978335189);
    assert_eq!(WINGET_ALREADY_LATEST_EXIT_CODE as u32, 0x8A15002B);
}

// 설계 §B.4: (Gh, WingetPackage) 조합이 UPDATE_TABLE에서 Run으로 매칭되고,
// 다른 도구는 이 method로 매칭되지 않아야 한다(도구별 행이 제네릭 행보다
// 우선 매칭되는 기존 lookup_action 규칙과 동일).
#[test]
fn winget_package_method_maps_to_run_only_for_gh() {
    assert!(matches!(
        lookup_action(ToolId::Gh, MethodKind::WingetPackage),
        Action::Run(plan) if plan.runner == Runner::Winget
    ));
}

// G-5(보안): winget RunPlan 어디에도 `--ignore-security-hash`가 없어야
// 한다(오설치=임의 코드 실행 위험) — 문자열 검색으로 고정한다.
#[test]
fn winget_run_plans_never_include_ignore_security_hash_or_msstore_source() {
    for plan in [RUN_WINGET_INSTALL_GH, RUN_WINGET_UPGRADE_GH] {
        for arg in plan.args {
            if let Arg::Lit(s) = arg {
                assert_ne!(*s, "--ignore-security-hash");
                assert_ne!(*s, "msstore");
            }
        }
        // --source가 있다면 반드시 다음 리터럴이 "winget"이어야 한다.
        let lits: Vec<&str> = plan
            .args
            .iter()
            .filter_map(|a| match a {
                Arg::Lit(s) => Some(*s),
                _ => None,
            })
            .collect();
        if let Some(idx) = lits.iter().position(|s| *s == "--source") {
            assert_eq!(lits.get(idx + 1), Some(&"winget"));
        }
    }
}

// N3(2라운드 비차단): 위 테스트는 `[RUN_WINGET_INSTALL_GH, RUN_WINGET_UPGRADE_GH]`를
// 손으로 나열한다 — 나중에 winget 행이 하나 더 추가되고 이 배열에 빠뜨려도
// 이 테스트는 여전히 통과한다(가드가 무의미해진다). 이 테스트는 UPDATE_TABLE
// (여기, 내부 테이블)과 install_candidates()(install_resolver.rs, 도구
// 하드코딩 없이 DEV_TOOLS 전체를 순회 — `install_candidates_run_plan_slots_are_
// literal_only`와 동일 패턴)를 **구조적으로** 순회해 Runner::Winget인 모든
// RunPlan을 자동으로 모은다. 새 winget 행이 어느 테이블에 추가되든 이
// 가드가 자동으로 걸린다.
#[test]
fn all_winget_run_plans_in_install_and_update_tables_pass_security_gate() {
    use super::super::install_resolver::install_candidates;
    use super::super::DEV_TOOLS;

    let mut winget_plans: Vec<RunPlan> = Vec::new();

    // UPDATE_TABLE(이 파일의 내부 정적 테이블) 전체를 순회한다.
    for row in UPDATE_TABLE {
        if let Action::Run(plan) = row.action {
            if plan.runner == Runner::Winget {
                winget_plans.push(plan);
            }
        }
    }
    // install_candidates()(도구별 설치 후보 테이블)도 DEV_TOOLS 전체를
    // 순회해 하드코딩 없이 모은다.
    for def in DEV_TOOLS.iter() {
        for candidate in install_candidates(def.id) {
            if candidate.plan.runner == Runner::Winget {
                winget_plans.push(candidate.plan);
            }
        }
    }

    assert!(
        !winget_plans.is_empty(),
        "winget RunPlan이 하나도 발견되지 않았습니다 — 이 가드가 무의미해집니다"
    );

    for plan in winget_plans {
        // ③ 전부 Arg::Lit.
        let lits: Vec<&str> = plan
            .args
            .iter()
            .filter_map(|a| match a {
                Arg::Lit(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(
            lits.len(),
            plan.args.len(),
            "winget RunPlan.args는 전부 Arg::Lit이어야 합니다: {:?}",
            plan.args
        );

        // ② --ignore-security-hash / --scope machine 부재(scope 자체를
        // 아예 지정하지 않는 현재 판단 — 위 주석 참고. 그래도 향후 실수로
        // "--scope machine"이 추가되는 것은 막는다).
        assert!(
            !lits.contains(&"--ignore-security-hash"),
            "winget RunPlan에 --ignore-security-hash가 있으면 안 됩니다(오설치=임의 코드 실행)"
        );
        assert!(
            !lits.contains(&"msstore"),
            "winget 소스가 msstore이면 안 됩니다"
        );
        if let Some(idx) = lits.iter().position(|s| *s == "--scope") {
            assert_ne!(
                lits.get(idx + 1),
                Some(&"machine"),
                "--scope machine을 명시적으로 고정하면 안 됩니다(UAC 승격을 앱이 유도하는 모양이 된다)"
            );
        }

        // ① --source winget 존재(있다면 다음 리터럴이 반드시 "winget").
        let source_idx = lits.iter().position(|s| *s == "--source");
        assert_eq!(
            source_idx.and_then(|i| lits.get(i + 1)),
            Some(&"winget"),
            "winget RunPlan은 --source winget을 고정해야 합니다: {lits:?}"
        );
    }
}

// ── N1(review-devtools-windows-parity-2026-09-15-r2.md) 재발 방지 ──
//
// windows_system_managed_plan 자체가 macOS 전용 토큰(softwareupdate/Xcode 등)을
// 절대 내지 않는지 직접 고정한다 — MANUAL_XCODE_CLT와 달리 이 함수가 실제로
// Windows SystemManaged 분류(Git/Node)의 안내문 정본이다.
#[test]
fn windows_system_managed_plan_never_mentions_macos_terms() {
    use super::super::DEV_TOOLS;

    const MAC_ONLY_TOKENS: [&str; 5] = ["softwareupdate", "brew", "Xcode", "xcode-select", "Homebrew"];

    for def in DEV_TOOLS.iter() {
        let plan = windows_system_managed_plan(def.id);
        let text = format!("{} {}", plan.message_ko, plan.copyable_command.unwrap_or(""));
        for token in MAC_ONLY_TOKENS {
            assert!(
                !text.contains(token),
                "{}용 windows_system_managed_plan에 macOS 전용 토큰 '{token}'이 있습니다: {text}",
                def.key
            );
        }
        assert_eq!(plan.reason, ManualReason::WindowsSystemManaged);
    }

    // Git/Node는 각각 winget 패키지 id를 install_manual_plan_windows와
    // 동일하게(Git.Git / OpenJS.NodeJS.LTS) 안내해야 한다 — 새 id를 지어내지
    // 않는다는 계약.
    assert_eq!(
        windows_system_managed_plan(ToolId::Git).copyable_command,
        Some("winget upgrade --id Git.Git -e --source winget")
    );
    assert_eq!(
        windows_system_managed_plan(ToolId::Node).copyable_command,
        Some("winget upgrade --id OpenJS.NodeJS.LTS -e --source winget")
    );
}

// N1 핵심 재발 방지 가드: DEV_TOOLS × windows_path_candidates 전체를
// **구조적으로** 순회해 classify_install_method_windows로 분류한 뒤,
// compute_action_for_platform(..., Platform::Win)이 실제로 반환하는 Action을
// 검사한다 — windows_system_managed_plan을 올바로 작성해도 compute_action이
// 그 함수를 호출하도록 배선하지 않으면(예: 이 override를 실수로 지우면) 잡지
// 못하는 결함까지 잡는 통합 테스트다. `all_winget_run_plans_in_install_and_
// update_tables_pass_security_gate`와 마찬가지로 "업데이트 경로"(이미 설치된
// 도구를 다시 조회하는 흐름)를 덮는다 — install_candidates()는 미설치 경로만
// 본다.
#[test]
fn windows_installed_tool_actions_never_reach_macos_only_manual_text() {
    use super::super::classify::classify_install_method_windows;
    use super::super::platform::Platform;
    use super::super::win_candidate_test_support::{for_every_windows_candidate, synthetic_windows_env_roots};
    use std::path::Path;

    const MAC_ONLY_TOKENS: [&str; 5] = ["softwareupdate", "brew", "Xcode", "xcode-select", "Homebrew"];

    let roots = synthetic_windows_env_roots();
    let mut violations: Vec<String> = Vec::new();

    // R-8(3차): 순회 자체(EnvRoots 조립 + windows_path_candidates 전수 + 빈
    // 배열 방지)는 win_candidate_test_support로 통일했다 — 이 테스트는 검사
    // 로직만 클로저로 넘긴다.
    let checked = for_every_windows_candidate(|def, template, expanded| {
        let method = classify_install_method_windows(Path::new(&expanded), &roots);
        let action = compute_action_for_platform(def.id, &method, Platform::Win);

        match action {
            Action::Manual(mp) => {
                let text = format!("{} {}", mp.message_ko, mp.copyable_command.unwrap_or(""));
                for token in MAC_ONLY_TOKENS {
                    if text.contains(token) {
                        violations.push(format!(
                            "{}의 Windows 후보 {template:?}(분류: {method:?})가 macOS 전용 \
                             토큰 '{token}'을 포함한 Manual 문구를 냅니다: {text}",
                            def.key
                        ));
                    }
                }
            }
            Action::Run(plan) => {
                if plan.runner == Runner::Brew {
                    violations.push(format!(
                        "{}의 Windows 후보 {template:?}(분류: {method:?})가 존재하지 않는 \
                         Brew 러너로 라우팅됩니다",
                        def.key
                    ));
                }
            }
        }
    });

    assert!(
        checked > 0,
        "DEV_TOOLS 순회가 실행되지 않았습니다 — 이 가드가 무의미해집니다"
    );
    assert!(violations.is_empty(), "\n{}", violations.join("\n"));
}

// N1 반대 방향(reviewer 지적 — "mac 플랜에 winget·.exe도 같이 걸어라"): mac
// path_candidates는 canonical 심볼릭 링크 대상이 아니라 "탐색 위치"라 대부분
// Unknown으로 떨어지지만(symlink 해석 전 단계), Git의 `/usr/bin/git`·
// `/Library/Developer/CommandLineTools/usr/bin/git`처럼 그 자체가 이미
// SystemManaged 리터럴 경로인 후보는 실제로 분류된다 — DEV_TOOLS 전체를
// 구조적으로 순회해 mac 쪽에서 Windows 전용 토큰이 새어나가지 않는지 고정한다.
#[test]
fn mac_installed_tool_actions_never_reach_windows_only_manual_text() {
    use super::super::classify::classify_install_method;
    use super::super::DEV_TOOLS;
    use std::path::{Path, PathBuf};

    const WIN_ONLY_TOKENS: [&str; 3] = ["winget", ".exe", "Program Files"];

    let home = PathBuf::from("/Users/malgn_vscode_test_user");
    let brew_prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];

    let mut checked = 0usize;
    let mut violations: Vec<String> = Vec::new();
    let total_candidates: usize = DEV_TOOLS.iter().map(|d| d.path_candidates.len()).sum();

    for def in DEV_TOOLS.iter() {
        for template in def.path_candidates {
            let expanded = if let Some(rest) = template.strip_prefix("~/") {
                home.join(rest).to_string_lossy().to_string()
            } else {
                (*template).to_string()
            };
            let method =
                classify_install_method(Path::new(&expanded), Some(&home), None, &brew_prefixes);
            let action = compute_action_for_platform(def.id, &method, super::super::platform::Platform::Mac);
            checked += 1;

            match action {
                Action::Manual(mp) => {
                    let text = format!("{} {}", mp.message_ko, mp.copyable_command.unwrap_or(""));
                    for token in WIN_ONLY_TOKENS {
                        if text.contains(token) {
                            violations.push(format!(
                                "{}의 mac 후보 {template:?}(분류: {method:?})가 Windows 전용 \
                                 토큰 '{token}'을 포함한 Manual 문구를 냅니다: {text}",
                                def.key
                            ));
                        }
                    }
                }
                Action::Run(plan) => {
                    if plan.runner == Runner::Winget {
                        violations.push(format!(
                            "{}의 mac 후보 {template:?}(분류: {method:?})가 존재하지 않는 \
                             winget 러너로 라우팅됩니다",
                            def.key
                        ));
                    }
                }
            }
        }
    }

    assert_eq!(
        checked, total_candidates,
        "DEV_TOOLS 순회가 예상과 다르게 실행됐습니다 — 이 가드가 무의미해집니다"
    );
    assert!(violations.is_empty(), "\n{}", violations.join("\n"));
}
