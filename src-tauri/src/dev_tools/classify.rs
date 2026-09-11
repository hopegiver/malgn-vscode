// ==================== 2. 설치방식 판별(결정 1) ====================
// 어떤 canonical 바이너리 경로가 주어졌을 때 "이 도구가 어떻게 설치되었는지"를
// 판별한다. 이 판별 자체는 `ToolId`나 `plan_table`의 argv 테이블을 몰라도
// 되는 순수 분류 로직이라 별도 모듈로 분리했다 — `plan_table`/`install_resolver`가
// 이 모듈의 `InstallMethod`/`MethodKind`를 소비한다.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallMethod {
    HomebrewFormula {
        formula: String,
        keg_version: String,
        prefix: String,
    },
    HomebrewCask {
        cask: String,
    },
    // 설계 이후 확정된 조사결과 #3: pnpm standalone 설치(canonical이 $PNPM_HOME
    // 또는 ~/Library/pnpm, ~/.local/share/pnpm 아래). 이 머신에서는 pnpm이
    // Homebrew Cellar 설치라 이 분류로 떨어지지 않는다(실기 검증 불가, 아래 참고).
    PnpmStandalone,
    // 조사결과(#3 후속): pnpm이 `pnpm add -g <pkg>`로 다른 CLI를 전역 설치하면
    // shim이 $PNPM_HOME 바로 밑이 아니라 $PNPM_HOME/bin/<name> 아래에 생긴다
    // (pnpm 자기 자신의 standalone 설치는 $PNPM_HOME 바로 밑). Wrangler가 실측
    // 사례(`~/Library/pnpm/bin/wrangler`) — 같은 bin 안에 cf-wrangler/pn/pnpx 등
    // 다른 전역 패키지도 함께 있다.
    PnpmGlobalPackage {
        package: String,
    },
    NpmGlobal {
        package: String,
    },
    ClaudeNative,
    SystemManaged,
    VersionManager(String),
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MethodKind {
    HomebrewFormula,
    HomebrewCask,
    PnpmStandalone,
    PnpmGlobalPackage,
    NpmGlobal,
    ClaudeNative,
    SystemManaged,
    VersionManager,
    Unknown,
}

impl InstallMethod {
    pub(crate) fn kind(&self) -> MethodKind {
        match self {
            InstallMethod::HomebrewFormula { .. } => MethodKind::HomebrewFormula,
            InstallMethod::HomebrewCask { .. } => MethodKind::HomebrewCask,
            InstallMethod::PnpmStandalone => MethodKind::PnpmStandalone,
            InstallMethod::PnpmGlobalPackage { .. } => MethodKind::PnpmGlobalPackage,
            InstallMethod::NpmGlobal { .. } => MethodKind::NpmGlobal,
            InstallMethod::ClaudeNative => MethodKind::ClaudeNative,
            InstallMethod::SystemManaged => MethodKind::SystemManaged,
            InstallMethod::VersionManager(_) => MethodKind::VersionManager,
            InstallMethod::Unknown(_) => MethodKind::Unknown,
        }
    }
}

pub(crate) fn describe_install_method(method: &InstallMethod) -> String {
    match method {
        InstallMethod::HomebrewFormula { formula, .. } => format!("homebrewFormula({formula})"),
        InstallMethod::HomebrewCask { cask } => format!("homebrewCask({cask})"),
        InstallMethod::PnpmStandalone => "pnpmStandalone".to_string(),
        InstallMethod::PnpmGlobalPackage { package } => format!("pnpmGlobalPackage({package})"),
        InstallMethod::NpmGlobal { package } => format!("npmGlobal({package})"),
        InstallMethod::ClaudeNative => "claudeNative".to_string(),
        InstallMethod::SystemManaged => "systemManaged".to_string(),
        InstallMethod::VersionManager(name) => format!("versionManager({name})"),
        InstallMethod::Unknown(path) => format!("unknown({path})"),
    }
}

/// 순수 함수(파일시스템 접근 없음) — home/pnpm_home/brew_prefixes를 인자로 받아
/// 테스트가 실제 홈 디렉터리·환경변수 없이도 합성 경로로 분류 로직을 검증할 수
/// 있게 한다. 우선순위: HomebrewFormula > HomebrewCask > PnpmStandalone >
/// NpmGlobal > ClaudeNative > SystemManaged > VersionManager > Unknown.
pub(crate) fn classify_install_method(
    canonical: &Path,
    home: Option<&Path>,
    pnpm_home: Option<&Path>,
    brew_prefixes: &[String],
) -> InstallMethod {
    let full = canonical.to_string_lossy().to_string();
    let segments: Vec<&str> = full.split('/').filter(|s| !s.is_empty()).collect();

    // 1. HomebrewFormula: "Cellar"의 부모가 알려진 brew prefix일 때만 채택.
    if let Some(idx) = segments.iter().position(|s| *s == "Cellar") {
        if idx >= 1 && idx + 2 < segments.len() {
            let prefix = format!("/{}", segments[..idx].join("/"));
            if brew_prefixes.iter().any(|p| p == &prefix) {
                return InstallMethod::HomebrewFormula {
                    formula: segments[idx + 1].to_string(),
                    keg_version: segments[idx + 2].to_string(),
                    prefix,
                };
            }
        }
    }

    // 2. HomebrewCask
    if let Some(idx) = segments.iter().position(|s| *s == "Caskroom") {
        if idx + 1 < segments.len() {
            return InstallMethod::HomebrewCask {
                cask: segments[idx + 1].to_string(),
            };
        }
    }

    // 3. PnpmStandalone / PnpmGlobalPackage (신규 요구사항 — pnpm 전용,
    // Cellar/Caskroom과 겹치지 않는다). pnpm은 자기 자신의 standalone 바이너리를
    // 루트 바로 밑($PNPM_HOME/pnpm)에 두고, `pnpm add -g <pkg>`로 설치한 다른
    // 패키지의 shim은 루트/bin 아래($PNPM_HOME/bin/<pkg>)에 둔다 — 이 bin/
    // 서브디렉터리 유무가 둘을 가르는 구조적 신호다.
    let mut pnpm_home_roots: Vec<PathBuf> = Vec::new();
    if let Some(ph) = pnpm_home {
        if !ph.as_os_str().is_empty() {
            pnpm_home_roots.push(ph.to_path_buf());
        }
    }
    if let Some(h) = home {
        pnpm_home_roots.push(h.join("Library").join("pnpm"));
        pnpm_home_roots.push(h.join(".local").join("share").join("pnpm"));
    }
    for root in &pnpm_home_roots {
        let bin_dir = root.join("bin");
        if canonical.starts_with(&bin_dir) {
            if let Ok(rest) = canonical.strip_prefix(&bin_dir) {
                if let Some(std::path::Component::Normal(name)) = rest.components().next() {
                    return InstallMethod::PnpmGlobalPackage {
                        package: name.to_string_lossy().to_string(),
                    };
                }
            }
        }
        if canonical.starts_with(root) {
            return InstallMethod::PnpmStandalone;
        }
    }

    // 4. NpmGlobal: <prefix>/lib/node_modules/<package>
    if let Some(idx) = segments.iter().position(|s| *s == "node_modules") {
        if idx >= 1 && segments[idx - 1] == "lib" && idx + 1 < segments.len() {
            let package = if let Some(scope) = segments[idx + 1].strip_prefix('@') {
                if idx + 2 < segments.len() {
                    format!("@{scope}/{}", segments[idx + 2])
                } else {
                    segments[idx + 1].to_string()
                }
            } else {
                segments[idx + 1].to_string()
            };
            return InstallMethod::NpmGlobal { package };
        }
    }

    // 5. ClaudeNative
    if let Some(h) = home {
        let candidates = [
            h.join(".local")
                .join("share")
                .join("claude")
                .join("versions"),
            h.join(".local").join("bin").join("claude"),
            h.join(".claude").join("local"),
        ];
        if candidates.iter().any(|c| canonical.starts_with(c)) {
            return InstallMethod::ClaudeNative;
        }
    }

    // 6. SystemManaged
    const SYSTEM_PREFIXES: [&str; 6] = [
        "/usr/bin/",
        "/bin/",
        "/sbin/",
        "/usr/sbin/",
        "/Library/Developer/CommandLineTools/",
        "/Applications/Xcode.app/",
    ];
    if SYSTEM_PREFIXES.iter().any(|p| full.starts_with(p)) {
        return InstallMethod::SystemManaged;
    }

    // 7. VersionManager
    const VERSION_MANAGER_MARKERS: [(&str, &str); 6] = [
        ("/.nvm/", "nvm"),
        ("/.fnm/", "fnm"),
        ("/.volta/", "volta"),
        ("/.asdf/", "asdf"),
        ("/mise/", "mise"),
        ("/n/versions/", "n"),
    ];
    for (marker, name) in VERSION_MANAGER_MARKERS {
        if full.contains(marker) {
            return InstallMethod::VersionManager(name.to_string());
        }
    }

    // 8. Unknown — canonicalize 실패/깨진 심볼릭 링크/미해석 경로도 여기로 강등된다
    // (호출부가 canonicalize 실패 시 원본 문자열을 그대로 넘기기 때문).
    InstallMethod::Unknown(full)
}

/// 실제 환경(홈 디렉터리, PNPM_HOME, HOMEBREW_PREFIX)을 읽어 순수 함수에 주입하는
/// 얇은 래퍼. 여기만 env/fs에 닿고, 분류 로직 자체는 순수하게 유지한다.
pub(crate) fn classify_install_method_with_defaults(path: &Path) -> InstallMethod {
    let home = dirs::home_dir();
    let pnpm_home = std::env::var("PNPM_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut prefixes = vec!["/opt/homebrew".to_string(), "/usr/local".to_string()];
    if let Ok(p) = std::env::var("HOMEBREW_PREFIX") {
        if !p.is_empty() && !prefixes.contains(&p) {
            prefixes.push(p);
        }
    }
    classify_install_method(path, home.as_deref(), pnpm_home.as_deref(), &prefixes)
}

/// canonicalize 실패(깨진 심볼릭 링크·권한)나 절대경로가 아닌 경우(PATH 폴백으로
/// 얻은 bare name) 모두 원본 문자열을 그대로 분류에 넘긴다 — 어차피 알려진 마커에
/// 걸리지 않아 자연히 Unknown으로 강등된다(에러가 아니다).
pub(crate) fn canonicalize_best_effort(resolved_path: &str) -> PathBuf {
    let p = Path::new(resolved_path);
    if p.is_absolute() {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    } else {
        p.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
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
}
