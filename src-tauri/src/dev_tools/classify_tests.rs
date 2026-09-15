// classify.rs의 테스트 모듈(별도 파일 분리 — 1,000줄 규율, platform.rs/
// platform_tests.rs와 동일한 패턴). `#[path]`로 연결된 `mod tests`의
// 본문이다. 순수함수/분류 로직 정본은 classify.rs에 그대로 남는다.

use super::*;
use crate::dev_tools::install_resolver::resolve_args;
use crate::dev_tools::plan_table::{lookup_action, Action, RUN_PNPM_GLOBAL_UPDATE};
use crate::dev_tools::ToolId;

// ── 완료판정 필수 테스트 ②: node@22 formula 파싱 ──
#[test]
fn classifies_node_at_22_formula_from_real_measured_path() {
    // 실측 canonical: /opt/homebrew/Cellar/node@22/22.23.1/bin/node
    let canonical = PathBuf::from("/opt/homebrew/Cellar/node@22/22.23.1/bin/node");
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];
    let method = classify_install_method(&canonical, None, None, &prefixes);
    assert_eq!(
        method,
        InstallMethod::HomebrewFormula {
            formula: "node@22".to_string(),
            keg_version: "22.23.1".to_string(),
            prefix: "/opt/homebrew".to_string()
        }
    );
    // formula명이 argv에 들어가기 전 검증도 통과해야 한다(악성 아님).
    if let InstallMethod::HomebrewFormula { formula, .. } = method {
        assert!(crate::dev_tools::install_resolver::validate_argv_token(
            &formula
        ));
    }
}

// ── 완료판정 필수 테스트 ⑤: 이 머신 실측 경로 설치방식 분류 ──
#[test]
fn classifies_real_measured_paths_on_this_machine() {
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];
    let home = PathBuf::from("/Users/hopegiver");

    let claude = classify_install_method(
        &PathBuf::from("/Users/hopegiver/.local/bin/claude"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(claude, InstallMethod::ClaudeNative);

    let pnpm = classify_install_method(
        &PathBuf::from("/opt/homebrew/Cellar/pnpm/11.9.0/bin/pnpm"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(
        pnpm,
        InstallMethod::HomebrewFormula {
            formula: "pnpm".to_string(),
            keg_version: "11.9.0".to_string(),
            prefix: "/opt/homebrew".to_string()
        }
    );

    let gh = classify_install_method(
        &PathBuf::from("/opt/homebrew/Cellar/gh/2.95.0/bin/gh"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(
        gh,
        InstallMethod::HomebrewFormula {
            formula: "gh".to_string(),
            keg_version: "2.95.0".to_string(),
            prefix: "/opt/homebrew".to_string()
        }
    );

    let git =
        classify_install_method(&PathBuf::from("/usr/bin/git"), Some(&home), None, &prefixes);
    assert_eq!(git, InstallMethod::SystemManaged);

    let node = classify_install_method(
        &PathBuf::from("/opt/homebrew/Cellar/node@22/22.23.1/bin/node"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(
        node,
        InstallMethod::HomebrewFormula {
            formula: "node@22".to_string(),
            keg_version: "22.23.1".to_string(),
            prefix: "/opt/homebrew".to_string()
        }
    );

    // 각 분류가 UPDATE_TABLE에서 실제로 기대한 액션 종류로 이어지는지도 확인.
    assert!(matches!(
        lookup_action(ToolId::Claude, claude.kind()),
        Action::Run(_)
    ));
    assert!(matches!(
        lookup_action(ToolId::Pnpm, pnpm.kind()),
        Action::Run(_)
    ));
    assert!(matches!(
        lookup_action(ToolId::Git, git.kind()),
        Action::Manual(_)
    ));
}

// ── 신규 조사결과 검증(실기 불가, 합성 경로로 분류 로직만 검증) ──
#[test]
fn pnpm_standalone_paths_classify_correctly_but_are_unverified_on_this_machine() {
    let home = PathBuf::from("/Users/hopegiver");
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];

    let via_library = classify_install_method(
        &PathBuf::from("/Users/hopegiver/Library/pnpm/pnpm"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(via_library, InstallMethod::PnpmStandalone);

    let via_local_share = classify_install_method(
        &PathBuf::from("/Users/hopegiver/.local/share/pnpm/pnpm"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(via_local_share, InstallMethod::PnpmStandalone);

    let via_pnpm_home_env = classify_install_method(
        &PathBuf::from("/custom/pnpm/pnpm"),
        Some(&home),
        Some(Path::new("/custom/pnpm")),
        &prefixes,
    );
    assert_eq!(via_pnpm_home_env, InstallMethod::PnpmStandalone);

    assert!(matches!(
        lookup_action(ToolId::Pnpm, MethodKind::PnpmStandalone),
        Action::Run(_)
    ));

    // 회귀 방지: pnpm 자기 자신($PNPM_HOME 바로 밑, bin/ 아님)은 여전히
    // PnpmStandalone으로 분류되어야 한다 — bin/ 분기 신설이 이 경로를
    // 건드리면 안 된다.
    let pnpm_self_via_pnpm_home_env = classify_install_method(
        &PathBuf::from("/custom/pnpm/pnpm"),
        Some(&home),
        Some(Path::new("/custom/pnpm")),
        &prefixes,
    );
    assert_eq!(pnpm_self_via_pnpm_home_env, InstallMethod::PnpmStandalone);
}

// 신규 요구사항: pnpm이 전역 설치한 다른 CLI(Wrangler 등)는 $PNPM_HOME/bin
// 아래에 shim이 생기며 PnpmStandalone이 아니라 PnpmGlobalPackage로
// 분류되어야 하고, 업데이트 테이블에서 Action::Run(RUN_PNPM_GLOBAL_UPDATE)로
// 매칭되어야 한다(이전에는 매칭 행이 없어 Manual(MANUAL_UNKNOWN_METHOD)로
// 잘못 떨어졌다).
#[test]
fn pnpm_global_package_paths_classify_separately_from_pnpm_standalone() {
    let home = PathBuf::from("/Users/hopegiver");
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];

    // 실측 경로: ~/Library/pnpm/bin/wrangler
    let wrangler = classify_install_method(
        &PathBuf::from("/Users/hopegiver/Library/pnpm/bin/wrangler"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(
        wrangler,
        InstallMethod::PnpmGlobalPackage {
            package: "wrangler".to_string()
        }
    );

    // $PNPM_HOME 환경변수 경유 + .local/share/pnpm 폴백 경로도 동일하게
    // bin/ 유무로 구분되어야 한다.
    let via_pnpm_home_env = classify_install_method(
        &PathBuf::from("/custom/pnpm/bin/cf-wrangler"),
        Some(&home),
        Some(Path::new("/custom/pnpm")),
        &prefixes,
    );
    assert_eq!(
        via_pnpm_home_env,
        InstallMethod::PnpmGlobalPackage {
            package: "cf-wrangler".to_string()
        }
    );

    let via_local_share = classify_install_method(
        &PathBuf::from("/Users/hopegiver/.local/share/pnpm/bin/wrangler"),
        Some(&home),
        None,
        &prefixes,
    );
    assert_eq!(
        via_local_share,
        InstallMethod::PnpmGlobalPackage {
            package: "wrangler".to_string()
        }
    );

    // Wrangler + PnpmGlobalPackage 조합이 UPDATE_TABLE에서 제네릭 행에
    // 매칭되어 Action::Run(RUN_PNPM_GLOBAL_UPDATE)로 떨어져야 한다.
    assert!(matches!(
        lookup_action(ToolId::Wrangler, MethodKind::PnpmGlobalPackage),
        Action::Run(plan) if plan.runner == crate::dev_tools::plan_table::Runner::Pnpm
            && matches!(plan.args, [
                crate::dev_tools::plan_table::Arg::Lit("add"),
                crate::dev_tools::plan_table::Arg::Lit("-g"),
                crate::dev_tools::plan_table::Arg::PackageLatest
            ])
    ));

    // resolve_args가 PnpmGlobalPackage에서 실제로 "add -g wrangler@latest"를
    // 만들어내는지 확인한다.
    let args = resolve_args(
        RUN_PNPM_GLOBAL_UPDATE.args,
        &InstallMethod::PnpmGlobalPackage {
            package: "wrangler".to_string(),
        },
    )
    .unwrap();
    assert_eq!(args, vec!["add", "-g", "wrangler@latest"]);
}

#[test]
fn homebrew_cask_and_version_manager_and_unknown_fall_back_to_manual() {
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];
    let cask = classify_install_method(
        &PathBuf::from("/opt/homebrew/Caskroom/some-app/1.0/App.app"),
        None,
        None,
        &prefixes,
    );
    assert_eq!(
        cask,
        InstallMethod::HomebrewCask {
            cask: "some-app".to_string()
        }
    );
    assert!(matches!(
        lookup_action(ToolId::Node, cask.kind()),
        Action::Manual(_)
    ));

    let nvm = classify_install_method(
        &PathBuf::from("/Users/hopegiver/.nvm/versions/node/v20.0.0/bin/node"),
        None,
        None,
        &prefixes,
    );
    assert_eq!(nvm, InstallMethod::VersionManager("nvm".to_string()));
    assert!(matches!(
        lookup_action(ToolId::Node, nvm.kind()),
        Action::Manual(_)
    ));

    let unknown = classify_install_method(
        &PathBuf::from("/some/weird/path/node"),
        None,
        None,
        &prefixes,
    );
    assert!(matches!(unknown, InstallMethod::Unknown(_)));
    assert!(matches!(
        lookup_action(ToolId::Node, unknown.kind()),
        Action::Manual(_)
    ));
}

#[test]
fn npm_global_scoped_and_unscoped_package_parsing() {
    let prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];
    let unscoped = classify_install_method(
        &PathBuf::from("/opt/homebrew/lib/node_modules/wrangler/bin/wrangler"),
        None,
        None,
        &prefixes,
    );
    assert_eq!(
        unscoped,
        InstallMethod::NpmGlobal {
            package: "wrangler".to_string()
        }
    );

    let scoped = classify_install_method(
        &PathBuf::from("/opt/homebrew/lib/node_modules/@anthropic/claude/bin/claude"),
        None,
        None,
        &prefixes,
    );
    assert_eq!(
        scoped,
        InstallMethod::NpmGlobal {
            package: "@anthropic/claude".to_string()
        }
    );
}

#[test]
fn canonicalize_failure_or_relative_path_degrades_to_unknown_not_error() {
    // 존재하지 않는 절대경로 — canonicalize 실패 → 원본 문자열로 폴백 → Unknown.
    let resolved = canonicalize_best_effort("/definitely/does/not/exist/binary");
    let method = classify_install_method_with_defaults(&resolved);
    assert!(matches!(method, InstallMethod::Unknown(_)));

    // PATH 폴백으로 얻은 bare name(절대경로 아님) — canonicalize 자체를 건너뛴다.
    let resolved_bare = canonicalize_best_effort("gh");
    assert_eq!(resolved_bare, PathBuf::from("gh"));
}

// ── Windows 분류(설계 §B.4, 합성 경로 — 이 머신(Mac)에서 100% 실행 검증) ──

fn win_roots() -> super::super::platform::EnvRoots {
    super::super::platform::EnvRoots {
        home: Some(PathBuf::from(r"C:\Users\hopegiver")),
        appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming")),
        local_appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Local")),
        program_files: Some(PathBuf::from(r"C:\Program Files")),
        program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
        pnpm_home: None,
        system_root: Some(PathBuf::from(r"C:\Windows")),
    }
}

#[test]
fn classifies_winget_links_and_packages_as_winget_package() {
    let roots = win_roots();
    let links = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Links\gh.exe"),
        &roots,
    );
    assert_eq!(links, InstallMethod::WingetPackage);

    let packages = classify_install_method_windows(
        &PathBuf::from(
            r"C:\Users\hopegiver\AppData\Local\Microsoft\WinGet\Packages\GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe\gh.exe",
        ),
        &roots,
    );
    assert_eq!(packages, InstallMethod::WingetPackage);

    // UPDATE_TABLE에서 (Gh, WingetPackage) 조합이 실제로 Run으로 이어지는지도
    // 함께 확인한다 — Windows용 신규 행이 실제로 물리는지 회귀 방지.
    assert!(matches!(
        super::super::plan_table::lookup_action(
            super::super::ToolId::Gh,
            MethodKind::WingetPackage
        ),
        super::super::plan_table::Action::Run(_)
    ));
}

// M1 수정(review-devtools-windows-parity-2026-09-15.md): 이 테스트는
// 원래 `...\pnpm\bin\wrangler.cmd`(\bin\ 있음) 입력 하나만으로
// `package: "wrangler.cmd"`(확장자 미제거)를 기대했다 — 리뷰가 지목한
// 대로 그 입력은 이 앱의 mod.rs::DEV_TOOLS가 절대 만들어내지 않는
// 경로였다(실제 Wrangler Windows 후보는 `%LOCALAPPDATA%\pnpm\wrangler.cmd`,
// \bin\ 없음). 새 규칙(파일명 기반 + 확장자 제거)에 맞춰 기대값을
// "wrangler"로 고치고, 앱이 실제로 선언한 두 후보(있음/없음 레이아웃)를
// 직접 고정한다 — 어느 레이아웃이 실제인지는 이 머신에서 검증 불가지만,
// 둘 다 같은 결과로 방어적으로 처리됨을 확인한다(정직 규율: "실제 이렇게
// 동작한다"가 아니라 "두 레이아웃 모두 이렇게 분류되도록 짰다"는 주장).
#[test]
fn classifies_pnpm_standalone_and_global_package_on_windows() {
    let roots = win_roots();
    let standalone = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\pnpm\pnpm.exe"),
        &roots,
    );
    assert_eq!(standalone, InstallMethod::PnpmStandalone);

    // 앱의 실제 Wrangler Windows 후보 1번(mod.rs DEV_TOOLS, \bin\ 없음).
    let global_package_no_bin = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\pnpm\wrangler.cmd"),
        &roots,
    );
    assert_eq!(
        global_package_no_bin,
        InstallMethod::PnpmGlobalPackage {
            package: "wrangler".to_string()
        }
    );

    // \bin\ 있는 레이아웃(가상 — 방어적으로 여전히 같은 결과가 나와야 한다).
    let global_package_with_bin = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\pnpm\bin\wrangler.cmd"),
        &roots,
    );
    assert_eq!(
        global_package_with_bin,
        InstallMethod::PnpmGlobalPackage {
            package: "wrangler".to_string()
        }
    );

    // M1 회귀 방지: 두 레이아웃 모두 UPDATE_TABLE에서 실제로 Run으로
    // 이어져야 한다(분류는 맞아도 라우팅이 없으면 여전히 manual로
    // 조용히 강등된다 — 이게 M1의 실제 증상이었다).
    assert!(matches!(
        super::super::plan_table::lookup_action(
            super::super::ToolId::Wrangler,
            MethodKind::PnpmGlobalPackage
        ),
        super::super::plan_table::Action::Run(_)
    ));
}

#[test]
fn classifies_npm_global_shim_and_node_modules_package_on_windows() {
    let roots = win_roots();
    // 전역 shim 파일 자체(npm 루트 바로 밑) — 파일명(확장자 제외)으로 근사.
    let shim = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming\npm\wrangler.cmd"),
        &roots,
    );
    assert_eq!(
        shim,
        InstallMethod::NpmGlobal {
            package: "wrangler".to_string()
        }
    );

    // node_modules 아래 실제 패키지 디렉터리(스코프 없음).
    let node_modules = classify_install_method_windows(
        &PathBuf::from(
            r"C:\Users\hopegiver\AppData\Roaming\npm\node_modules\wrangler\bin\wrangler.js",
        ),
        &roots,
    );
    assert_eq!(
        node_modules,
        InstallMethod::NpmGlobal {
            package: "wrangler".to_string()
        }
    );

    // 스코프 패키지.
    let scoped = classify_install_method_windows(
        &PathBuf::from(
            r"C:\Users\hopegiver\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.js",
        ),
        &roots,
    );
    assert_eq!(
        scoped,
        InstallMethod::NpmGlobal {
            package: "@anthropic-ai/claude-code".to_string()
        }
    );
}

#[test]
fn classifies_claude_native_git_system_managed_and_version_manager_on_windows() {
    let roots = win_roots();

    let claude_native = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\.local\bin\claude.exe"),
        &roots,
    );
    assert_eq!(claude_native, InstallMethod::ClaudeNative);

    let git = classify_install_method_windows(
        &PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"),
        &roots,
    );
    assert_eq!(git, InstallMethod::SystemManaged);

    let git_x86 = classify_install_method_windows(
        &PathBuf::from(r"C:\Program Files (x86)\Git\cmd\git.exe"),
        &roots,
    );
    assert_eq!(git_x86, InstallMethod::SystemManaged);

    let node = classify_install_method_windows(
        &PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
        &roots,
    );
    assert_eq!(node, InstallMethod::SystemManaged);

    let nvm = classify_install_method_windows(
        &PathBuf::from(r"C:\Users\hopegiver\.nvm\versions\node\v20.0.0\node.exe"),
        &roots,
    );
    assert_eq!(nvm, InstallMethod::VersionManager("nvm".to_string()));
}

#[test]
fn classifies_unknown_windows_path_without_panicking() {
    let roots = win_roots();
    let unknown =
        classify_install_method_windows(&PathBuf::from(r"D:\portable\tools\git.exe"), &roots);
    assert!(matches!(unknown, InstallMethod::Unknown(_)));
}

#[test]
fn classify_install_method_windows_case_insensitive_matching() {
    // Windows 파일시스템은 대소문자를 구분하지 않는 것이 관행이다 — 후보
    // 경로가 실제 canonical과 대소문자가 달라도 매칭돼야 한다.
    let roots = win_roots();
    let mixed_case = classify_install_method_windows(
        &PathBuf::from(r"c:\users\hopegiver\appdata\local\microsoft\winget\links\GH.EXE"),
        &roots,
    );
    assert_eq!(mixed_case, InstallMethod::WingetPackage);
}

// ── M1(review-devtools-windows-parity-2026-09-15.md) 교차 불변식 ──
//
// 이전에는 mod.rs::DEV_TOOLS가 선언한 Wrangler Windows 후보
// (`%LOCALAPPDATA%\pnpm\wrangler.cmd`, \bin\ 없음)와 이 파일의 분류기(\bin\
// 유무로 pnpm 자신/다른 패키지를 판별)가 서로 어긋나, 분류는 됐지만(
// PnpmStandalone) UPDATE_TABLE에 (Wrangler, PnpmStandalone) 행이 없어
// 조용히 "설치 방식을 확인할 수 없습니다"(MANUAL_UNKNOWN_METHOD)로
// 강등됐다. 이 테스트는 DEV_TOOLS 전체를 순회해(하드코딩 목록 없이) 같은
// 클래스의 결함이 다른 도구/후보에서 재발하면 구조적으로 잡는다.
//
// 범위: `InstallMethod::Unknown`으로 분류되는 후보는 이 테스트에서
// **제외**한다 — Unknown은 `Row{tool:None, method:Unknown,
// Manual(MANUAL_UNKNOWN_METHOD)}`로 이어지는 **의도된** 폴백이지(§4
// 완결성 — 모르는 경로는 정직하게 모른다고 말한다), 이 테스트가 잡으려는
// "분류는 됐는데 라우팅이 없어 조용히 같은 문구로 강등되는" 결함과는
// 다르다. 현재 이 제외 대상에 걸리는 후보가 있다(gh의 `C:\Program
// Files\GitHub CLI\gh.exe`, git/node의 `%LOCALAPPDATA%\Programs\...`) —
// winget/설치기의 실제 레이아웃이 이 머신에서 검증 불가라 방어적으로
// 다루려면 별도 조사가 필요하고, 이번 라운드의 M1 범위(Wrangler/pnpm +
// 같은 패턴이 즉시 드러난 Claude/WingetPackage)를 넘어선다고 판단해 이번
// 커밋에서는 손대지 않았다 — 후속 조사 대상으로 남긴다(미검증 명시).
#[test]
fn windows_candidates_never_silently_fall_to_unknown_method_manual() {
    use super::super::platform::{expand_path_tokens, EnvRoots, Platform};
    use super::super::plan_table::{Action, ManualReason};
    use super::super::DEV_TOOLS;
    use std::path::Path;

    let roots = EnvRoots {
        home: Some(PathBuf::from(r"C:\Users\hopegiver")),
        appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming")),
        local_appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Local")),
        program_files: Some(PathBuf::from(r"C:\Program Files")),
        program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
        pnpm_home: None,
        system_root: Some(PathBuf::from(r"C:\Windows")),
    };

    let mut checked = 0usize;
    let mut violations: Vec<String> = Vec::new();
    let total_candidates: usize = DEV_TOOLS
        .iter()
        .map(|d| d.windows_path_candidates.len())
        .sum();

    for def in DEV_TOOLS.iter() {
        for template in def.windows_path_candidates {
            let expanded = expand_path_tokens(Platform::Win, template, &roots)
                .unwrap_or_else(|| {
                    panic!(
                        "{}의 후보 {template:?}가 토큰 확장에 실패했습니다(테스트용 \
                         EnvRoots가 불완전합니다)",
                        def.key
                    )
                });
            let method = classify_install_method_windows(Path::new(&expanded), &roots);
            checked += 1;

            if matches!(method, InstallMethod::Unknown(_)) {
                // 위 문서화 참고 — 이 테스트의 범위 밖(별도 조사 필요, 미검증).
                continue;
            }

            let action = lookup_action(def.id, method.kind());
            let silently_unrouted = matches!(
                action,
                Action::Manual(plan) if plan.reason == ManualReason::UnknownMethod
            );
            if silently_unrouted {
                violations.push(format!(
                    "{}의 Windows 후보 {template:?}(확장: {expanded})가 {method:?}로 \
                     분류됐지만 UPDATE_TABLE에 (tool={:?}, method={:?}) 행이 없어 조용히 \
                     \"설치 방식을 확인할 수 없습니다\"로 강등됩니다 — M1과 같은 결함입니다.",
                    def.key,
                    def.id,
                    method.kind()
                ));
            }
        }
    }
    assert_eq!(
        checked, total_candidates,
        "DEV_TOOLS 순회가 예상과 다르게 실행됐습니다 — 이 가드가 무의미해집니다"
    );
    // 첫 위반에서 멈추지 않고 전부 모아 한 번에 보고한다 — 여러 도구가
    // 동시에 어긋나도 한 번의 실행으로 전체 그림을 알 수 있다.
    assert!(
        violations.is_empty(),
        "\n{}",
        violations.join("\n")
    );
}
