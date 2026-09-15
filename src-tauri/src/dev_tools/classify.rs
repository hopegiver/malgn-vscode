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
    // 설계 §B.4(Windows 전용): winget으로 설치된 패키지. 경로에서 winget 패키지
    // id를 파싱하는 것은 불가능하고 시도하지도 않는다(`...\WinGet\Links\gh.exe`에
    // "GitHub.cli"가 적혀 있지 않다) — 그리고 그 id는 애초에 필요하지도 않다:
    // UPDATE_TABLE의 (tool=Gh, method=WingetPackage) 행이 이미 리터럴 argv를
    // 고정해 두므로(`RUN_WINGET_UPGRADE_GH`), 어떤 명령을 실행할지는 도구 id +
    // 이 discriminant만으로 결정된다. 그래서 설계 스케치의 `WingetPackage {
    // id: String }`과 달리 데이터를 갖지 않는다 — 쓰이지 않을 값을 지어내지
    // 않기 위한 의도적 단순화다.
    WingetPackage,
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
    WingetPackage,
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
            InstallMethod::WingetPackage => MethodKind::WingetPackage,
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
        InstallMethod::WingetPackage => "wingetPackage".to_string(),
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
/// 얇은 래퍼. 여기만 env/fs에 닿고, 분류 로직 자체는 순수하게 유지한다. macOS
/// 분기는 기존 로직과 완전히 동일하다(무변경) — Windows는 별도 순수함수
/// `classify_install_method_windows`로 위임한다(설계 §B.4, Homebrew 전용 개념인
/// Cellar/Caskroom 세그먼트 스캔과 섞지 않는다).
pub(crate) fn classify_install_method_with_defaults(path: &Path) -> InstallMethod {
    match super::platform::platform_now() {
        super::platform::Platform::Win => {
            let roots = super::platform::EnvRoots::from_env();
            classify_install_method_windows(path, &roots)
        }
        super::platform::Platform::Mac => {
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
    }
}

/// **중요**: 이 함수 전체가 `std::path::Path`의 구조적 API(`.join()`,
/// `.parent()`, `.strip_prefix()`, `.components()`, `.file_stem()`)를 쓰지
/// 않는다 — 그 API들은 **컴파일 호스트**(이 크레이트는 macOS에서 빌드된다)의
/// 구분자 규칙(`/`)을 따르므로, `\`로 구분된 Windows 경로 문자열에 적용하면
/// 전체 문자열을 구분자 없는 파일명 하나로 오인한다(실측: 최초 구현에서 이
/// 함수의 모든 분기가 `Unknown`으로 떨어지는 회귀를 `cargo test`가 그대로
/// 잡아냈다 — "macOS에서 Windows 로직을 실행 검증한다"는 설계 §D의 전제가
/// 바로 이런 함정을 잡으라고 있는 것이다). 그래서 순수 문자열 연산(대소문자
/// 무시 접두사 매칭 + `split('\\')`)만 쓴다.
fn win_dir_with_trailing_sep(dir: &str) -> String {
    let mut p = dir.to_string();
    if !p.ends_with('\\') {
        p.push('\\');
    }
    p
}

/// `full`이 `dir`(디렉터리, 대소문자 무시) 바로 밑에 있으면 그 뒷부분을
/// 돌려준다. 바이트 길이 기반 슬라이스라 `full`의 해당 위치가 UTF-8 문자
/// 경계가 아니면(비ASCII 사용자명 등) 안전하게 실패(`None`)한다 — 패닉 대신
/// 분류 실패로 처리한다(§4 완결성 — fail-closed와 같은 정신, 잘못된 상태를
/// 표현하지 않는다).
fn win_strip_dir_prefix<'a>(full: &'a str, dir: &str) -> Option<&'a str> {
    let prefix = win_dir_with_trailing_sep(dir);
    if full.len() < prefix.len() || !full.is_char_boundary(prefix.len()) {
        return None;
    }
    if full[..prefix.len()].eq_ignore_ascii_case(&prefix) {
        Some(&full[prefix.len()..])
    } else {
        None
    }
}

/// `rest`의 첫 세그먼트(다음 `\`까지, 없으면 끝까지) — 빈 문자열이면 `None`.
fn win_first_segment(rest: &str) -> Option<&str> {
    let seg = rest.split('\\').next().unwrap_or("");
    if seg.is_empty() {
        None
    } else {
        Some(seg)
    }
}

/// Windows 전용 분류(설계 §B.4, 순수함수 — `EnvRoots`를 주입받아 이 머신(Mac)의
/// `cargo test`에서도 합성 경로로 전부 실행 검증된다). 모든 비교는 대소문자
/// 무시(Windows 파일시스템 관행)로 하고, `canonicalize_best_effort`가 이미
/// `dunce::canonicalize`로 `\\?\` 확장 길이 접두어를 제거한 값을 넘겨받는다고
/// 가정한다.
pub(crate) fn classify_install_method_windows(
    canonical: &Path,
    roots: &super::platform::EnvRoots,
) -> InstallMethod {
    let full = canonical.to_string_lossy().to_string();
    let root_str = |root: &Option<PathBuf>| -> Option<String> {
        root.as_ref().map(|p| p.to_string_lossy().to_string())
    };

    // 1. WingetPackage: %LOCALAPPDATA%\Microsoft\WinGet\Links\ 또는 \Packages\
    if let Some(la) = root_str(&roots.local_appdata) {
        let links_dir = format!(r"{la}\Microsoft\WinGet\Links");
        let packages_dir = format!(r"{la}\Microsoft\WinGet\Packages");
        if win_strip_dir_prefix(&full, &links_dir).is_some()
            || win_strip_dir_prefix(&full, &packages_dir).is_some()
        {
            return InstallMethod::WingetPackage;
        }
    }

    // 2. PnpmStandalone / PnpmGlobalPackage: %LOCALAPPDATA%\pnpm\ 아래 —
    // bin\ 서브디렉터리 유무로 "pnpm 자기 자신"과 "pnpm이 설치한 다른 패키지"를
    // 가른다(POSIX 분기와 동일한 구조적 신호, classify_install_method 참고).
    if let Some(la) = root_str(&roots.local_appdata) {
        let pnpm_dir = format!(r"{la}\pnpm");
        if win_strip_dir_prefix(&full, &pnpm_dir).is_some() {
            let bin_dir = format!(r"{pnpm_dir}\bin");
            if let Some(bin_rest) = win_strip_dir_prefix(&full, &bin_dir) {
                if let Some(name) = win_first_segment(bin_rest) {
                    return InstallMethod::PnpmGlobalPackage {
                        package: name.to_string(),
                    };
                }
            }
            return InstallMethod::PnpmStandalone;
        }
    }

    // 3. NpmGlobal: %APPDATA%\npm\node_modules\<package>\... (npm 전역 설치의
    // 실제 패키지 디렉터리) 또는 %APPDATA%\npm\<name>.cmd(전역 shim 자체 — 이
    // 경우 스코프 없는 패키지명을 shim 파일명에서 근사한다. 스코프 패키지의
    // 정확한 이름은 shim 파일명만으로 복원할 수 없어 이 근사가 한계다 — 미검증).
    if let Some(appdata) = root_str(&roots.appdata) {
        let npm_dir = format!(r"{appdata}\npm");
        let node_modules_dir = format!(r"{npm_dir}\node_modules");
        if let Some(rest) = win_strip_dir_prefix(&full, &node_modules_dir) {
            if let Some(first) = win_first_segment(rest) {
                let package = if let Some(scope) = first.strip_prefix('@') {
                    win_first_segment(&rest[first.len() + 1..])
                        .map(|pkg| format!("@{scope}/{pkg}"))
                        .unwrap_or_else(|| first.to_string())
                } else {
                    first.to_string()
                };
                return InstallMethod::NpmGlobal { package };
            }
        } else if let Some(rest) = win_strip_dir_prefix(&full, &npm_dir) {
            // 하위 세그먼트가 없어야(=shim 파일 자체) 이 분기가 적용된다.
            if !rest.is_empty() && !rest.contains('\\') {
                let stem = rest.rsplit_once('.').map(|(s, _)| s).unwrap_or(rest);
                return InstallMethod::NpmGlobal {
                    package: stem.to_string(),
                };
            }
        }
    }

    // 4. ClaudeNative: %USERPROFILE%\.local\bin\claude.exe(POSIX 네이티브
    // 설치기와 동형 구조).
    if let Some(home) = root_str(&roots.home) {
        let claude_dir = format!(r"{home}\.local\bin");
        if win_strip_dir_prefix(&full, &claude_dir).is_some() {
            return InstallMethod::ClaudeNative;
        }
    }

    // 5. SystemManaged: Program Files 아래 Git/nodejs(설치기가 시스템 전역에
    // 심는 자리) — 리터럴 후보(B.5)는 대소문자만 무시하고 그대로 비교한다.
    let full_lower = full.to_lowercase();
    let system_prefixes = [r"c:\program files\git\", r"c:\program files\nodejs\"];
    if system_prefixes.iter().any(|p| full_lower.starts_with(p)) {
        return InstallMethod::SystemManaged;
    }
    // Program Files (x86) 아래 Git도 동일하게 취급한다.
    if let Some(x86) = root_str(&roots.program_files_x86) {
        let git_dir = format!(r"{x86}\Git");
        if win_strip_dir_prefix(&full, &git_dir).is_some() {
            return InstallMethod::SystemManaged;
        }
    }

    // 6. VersionManager: %USERPROFILE%\.nvm, \fnm\, \volta\ 세그먼트(G4 근거 —
    // devtools-install-matrix §3.3과 동일 이유로, 앱이 관리본을 못 보는 것이
    // 의도된 동작임을 먼저 인지해야 한다).
    const VERSION_MANAGER_MARKERS: [(&str, &str); 3] =
        [(r"\.nvm\", "nvm"), (r"\fnm\", "fnm"), (r"\volta\", "volta")];
    for (marker, name) in VERSION_MANAGER_MARKERS {
        if full_lower.contains(marker) {
            return InstallMethod::VersionManager(name.to_string());
        }
    }

    // 7. Unknown — canonicalize 실패/깨진 링크/미해석 경로도 여기로 강등된다.
    InstallMethod::Unknown(full)
}

/// canonicalize 실패(깨진 심볼릭 링크·권한)나 절대경로가 아닌 경우(PATH 폴백으로
/// 얻은 bare name) 모두 원본 문자열을 그대로 분류에 넘긴다 — 어차피 알려진 마커에
/// 걸리지 않아 자연히 Unknown으로 강등된다(에러가 아니다). `dunce::canonicalize`를
/// 쓰는 이유: Windows `std::fs::canonicalize()`는 `\\?\`(확장 길이) 접두 경로를
/// 반환하는데, 이 값이 그대로 UI에 노출되면 사용자가 혼란스럽고 이후 문자열
/// 비교(§B.4의 접두어 매칭)가 깨진다 — `config/user_config.rs`가 이미 겪은
/// 문제와 동일하다. mac/linux에서 `dunce::canonicalize`는 `std::fs::canonicalize`와
/// 동일하게 동작한다(dunce 크레이트 문서) — 기존 동작과 바이트 단위로 같다.
pub(crate) fn canonicalize_best_effort(resolved_path: &str) -> PathBuf {
    let p = Path::new(resolved_path);
    if p.is_absolute() {
        dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
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

    #[test]
    fn classifies_pnpm_standalone_and_global_package_on_windows() {
        let roots = win_roots();
        let standalone = classify_install_method_windows(
            &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\pnpm\pnpm.exe"),
            &roots,
        );
        assert_eq!(standalone, InstallMethod::PnpmStandalone);

        let global_package = classify_install_method_windows(
            &PathBuf::from(r"C:\Users\hopegiver\AppData\Local\pnpm\bin\wrangler.cmd"),
            &roots,
        );
        assert_eq!(
            global_package,
            InstallMethod::PnpmGlobalPackage {
                package: "wrangler.cmd".to_string()
            }
        );
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
}
