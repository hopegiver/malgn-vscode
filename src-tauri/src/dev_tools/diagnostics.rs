// ==================== 7. plan_id 해시 게이트(부록 A) ====================
// 실행 계획 위조 방지 해시(§7) + brew dry-run 출력 파싱(§8) + 로그인 셸 PATH
// 노출 확인(§9) — 셋 다 "커맨드가 사용자에게 보여줄 값을 검증/가공하는" 보조
// 유틸리티라 한 파일에 묶었다. 서로 의존하지 않는다.

use super::plan_table::ManualPlan;
use super::ToolId;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// `sha256(runner_path ‖ argv ‖ normalized_before)`. 실행 직전 재계산해 대조한다
/// — "확인 눌렀음(bool)"만 보는 게이트와 달리 그 사이 상태가 바뀌면(brew 인덱스
/// 갱신 등) 불일치로 걸러 재프리뷰를 요구한다.
pub(crate) fn compute_plan_id(runner_path: &str, argv: &[String], normalized_before: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(runner_path.as_bytes());
    hasher.update([0u8]);
    hasher.update(argv.join("\u{0}").as_bytes());
    hasher.update([0u8]);
    hasher.update(normalized_before.as_bytes());
    to_hex(&hasher.finalize())
}

/// Manual 계획(아무것도 실행하지 않음)에도 동일한 인터페이스를 유지하기 위한
/// plan_id. 실행할 것이 없어 위조 리스크는 없지만, 프론트 계약이 planId를 항상
/// 요구하므로(부록 C) 일관되게 채워준다.
pub(crate) fn compute_plan_id_for_manual(tool: ToolId, plan: &ManualPlan) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{tool:?}").as_bytes());
    hasher.update([0u8]);
    hasher.update(format!("{:?}", plan.reason).as_bytes());
    to_hex(&hasher.finalize())
}

// ==================== 8. brew --dry-run 파싱(부록 A) ====================

/// `brew upgrade --dry-run` 출력에서 "이름 이전버전 -> 새버전 (크기)" 줄만 골라
/// formula 이름을 추출한다. 실측(이 머신, `brew upgrade --dry-run --formula gh`,
/// 읽기 전용 조회로 확인됨): `gh 2.95.0 -> 2.100.0 (14MB)`.
pub(crate) fn parse_brew_dry_run_affected(stdout: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if !line.contains("->") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(first) = parts.next() else { continue };
        let (Some(_old_version), Some(arrow)) = (parts.next(), parts.next()) else {
            continue;
        };
        if arrow == "->" {
            names.push(first.to_string());
        }
    }
    names
}

/// `brew install <-n|-y> --formula <name>` 출력에서 "Would install"/"Would
/// upgrade" 헤더 다음에 나열되는 패키지 이름들을 모은다(devtools-install-matrix
/// §3.4 — 위임서가 이 설계 문서의 "install dry-run에는 `->`가 나오지 않는다"는
/// 단언을 실측으로 교정했다). `parse_brew_dry_run_affected`(update 전용, `->`
/// 형태만 인식)를 재사용하면 안 되는 이유가 바로 이 함수가 필요한 이유다.
///
/// 실측 픽스처(위임서 그대로, `HOMEBREW_NO_AUTO_UPDATE=1 brew install -n
/// --formula gnupg`):
/// ```text
/// gnupg 2.5.20 is already installed but outdated (so it will be upgraded).
/// ==> Would install 1 formula:
/// gnupg
/// ==> Would upgrade 4 dependencies for gnupg:
/// p11-kit
/// libgcrypt
/// libksba
/// pinentry
/// ==> Would upgrade 3 dependents of upgraded formula:
/// Disable this behaviour by setting `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1`.
/// Hide these hints with `HOMEBREW_NO_ENV_HINTS=1` (see `man brew`).
/// gpgme    2.1.2   -> 2.2.0
/// gpgmepp  2.1.0   -> 2.2.0
/// poppler  26.06.0 -> 26.09.0
/// ==> Would install 1 formula:      (전체 블록이 한 번 더 반복 출력된다)
/// ...
/// ```
/// 확인된 사실(테스트 `parses_real_measured_brew_install_dry_run_output_with_dependents_fixture`
/// 로 고정):
///  ① `==>`로 시작하고 "Would install"/"Would upgrade"를 포함하는 헤더 줄에서만
///     수집을 시작한다.
///  ② 이어지는 줄에서 선택적 " old -> new" 접미(예: "gpgme 2.1.2 -> 2.2.0")를
///     잘라내고 첫 토큰만 이름으로 취한다.
///  ③ "Disable this behaviour by setting …"/"Hide these hints with …" 같은
///     힌트 산문 줄(백틱 포함)이 헤더와 이름 줄 사이에 끼어든다 — 이름으로
///     오인하지 않는다.
///  ④ 전체 블록이 반복 출력되므로 중복을 제거한다.
///  ⑤ 대상 포뮬러 자신(`target`)은 목록에서 빼고 개수를 센다(직전 리뷰 지적
///     #8의 off-by-one을 신규 코드에서 반복하지 않기 위해 — 호출부가 대상
///     자신을 affected 리스트 맨 앞에 별도로 붙인다).
///
/// `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1`이 실제로 적용됐는지에 이 파서는
/// 의존하지 않는다 — env 적용이 실패해도(dependents 섹션이 그대로 나와도)
/// 헤더 기반 수집이라 정확히 파싱된다.
pub(crate) fn parse_brew_install_dry_run_affected(stdout: &str, target: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut collecting = false;
    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.starts_with("==>") {
            collecting = line.contains("Would install") || line.contains("Would upgrade");
            continue;
        }
        if !collecting || line.is_empty() {
            continue;
        }
        if line.contains('`')
            || line.starts_with("Disable this behaviour")
            || line.starts_with("Hide these hints")
        {
            continue;
        }
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name.is_empty() || name == target {
            continue;
        }
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

// ==================== 9. 로그인 셸 PATH 노출 확인(신규 요구사항) ====================
// 셸을 스폰하지 않고 읽기 전용 파일 검사로만 판정한다(설계 결정 5가 기각한
// "로그인 셸 상속" 대안과 같은 이유 — 사용자 .zshrc가 무슨 짓을 할지 앱이 통제
// 못 한다). /etc/paths + /etc/paths.d/*를 읽고, 존재하는 rc 파일들을 읽어서
// 대상 디렉터리 문자열이 등장하는지만 확인한다. rc 파일 자동 수정 기능은 없다.

const RC_FILE_NAMES: [&str; 7] = [
    "~/.zshrc",
    "~/.zprofile",
    "~/.zshenv",
    "~/.bash_profile",
    "~/.bashrc",
    "~/.profile",
    "~/.config/fish/config.fish",
];

fn read_etc_paths_entries() -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(content) = std::fs::read_to_string("/etc/paths") {
        entries.extend(
            content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty()),
        );
    }
    if let Ok(rd) = std::fs::read_dir("/etc/paths.d") {
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        for f in files {
            if let Ok(content) = std::fs::read_to_string(&f) {
                entries.extend(
                    content
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty()),
                );
            }
        }
    }
    entries
}

/// rc 파일 이름 목록을 실제 파일 존재 여부와 상관없이 순회하며 `dir` 문자열이
/// 등장하는 파일이 있는지 확인하는 순수-ish 헬퍼. `home`을 인자로 받아 테스트가
/// 실제 홈 디렉터리 대신 임시 디렉터리를 넣어 검증할 수 있게 한다.
fn rc_files_contain(dir: &str, home: &Path) -> bool {
    for rc in RC_FILE_NAMES {
        let Some(rest) = rc.strip_prefix("~/") else {
            continue;
        };
        let path = home.join(rest);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains(dir) {
                return true;
            }
        }
    }
    false
}

fn is_dir_in_shell_path(dir: &str, etc_entries: &[String], home: Option<&Path>) -> bool {
    let trimmed_dir = dir.trim_end_matches('/');
    if etc_entries
        .iter()
        .any(|e| e.trim_end_matches('/') == trimmed_dir)
    {
        return true;
    }
    match home {
        Some(h) => rc_files_contain(trimmed_dir, h),
        None => false,
    }
}

fn hint_target_for_shell(shell_value: &str) -> &'static str {
    if shell_value.contains("zsh") {
        "~/.zprofile"
    } else if shell_value.contains("bash") {
        "~/.bash_profile"
    } else if shell_value.contains("fish") {
        "~/.config/fish/config.fish"
    } else {
        "~/.profile"
    }
}

pub(crate) struct PathVisibility {
    pub(crate) visible: bool,
    pub(crate) hint: Option<String>,
    pub(crate) hint_target: Option<String>,
}

pub(crate) fn compute_path_visibility(resolved_binary_path: &str) -> PathVisibility {
    let Some(dir) = Path::new(resolved_binary_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
    else {
        return PathVisibility {
            visible: false,
            hint: None,
            hint_target: None,
        };
    };
    let etc_entries = read_etc_paths_entries();
    let home = dirs::home_dir();
    if is_dir_in_shell_path(&dir, &etc_entries, home.as_deref()) {
        PathVisibility {
            visible: true,
            hint: None,
            hint_target: None,
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_default();
        PathVisibility {
            visible: false,
            hint: Some(format!("export PATH=\"{dir}:$PATH\"")),
            hint_target: Some(hint_target_for_shell(&shell).to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 부록 A: plan_id는 입력이 같으면 안정적이고, 입력이 다르면 달라져야 한다.
    #[test]
    fn plan_id_is_stable_and_input_sensitive() {
        let a = compute_plan_id(
            "/opt/homebrew/bin/brew",
            &[
                "upgrade".to_string(),
                "-y".to_string(),
                "--formula".to_string(),
                "pnpm".to_string(),
            ],
            "11.9.0",
        );
        let b = compute_plan_id(
            "/opt/homebrew/bin/brew",
            &[
                "upgrade".to_string(),
                "-y".to_string(),
                "--formula".to_string(),
                "pnpm".to_string(),
            ],
            "11.9.0",
        );
        assert_eq!(a, b);

        let c = compute_plan_id(
            "/opt/homebrew/bin/brew",
            &[
                "upgrade".to_string(),
                "-y".to_string(),
                "--formula".to_string(),
                "pnpm".to_string(),
            ],
            "11.8.0",
        );
        assert_ne!(a, c);
    }

    // 실측(읽기 전용 `brew upgrade --dry-run --formula gh`)으로 확인된 실제 출력
    // 형식을 그대로 파싱 테스트에 쓴다.
    #[test]
    fn parses_real_measured_brew_dry_run_output() {
        let sample = "==> Would run `brew cleanup` which has not been run in the last 30 days\n\
Disable this behaviour by setting `HOMEBREW_NO_INSTALL_CLEANUP=1`.\n\
Hide these hints with `HOMEBREW_NO_ENV_HINTS=1` (see `man brew`).\n\
==> Would upgrade 1 requested outdated package\n\
gh 2.95.0 -> 2.100.0 (14MB)\n";
        let affected = parse_brew_dry_run_affected(sample);
        assert_eq!(affected, vec!["gh".to_string()]);
    }

    #[test]
    fn parses_multiple_affected_formulae_for_dependency_chain_warning() {
        let sample = "pnpm 11.8.0 -> 11.9.0 (5MB)\nnode 22.22.0 -> 22.23.1 (30MB)\n";
        let affected = parse_brew_dry_run_affected(sample);
        assert_eq!(affected, vec!["pnpm".to_string(), "node".to_string()]);
    }

    // devtools-install-matrix §3.4: 위임자가 이 머신에서 실측한 install dry-run
    // 출력 전문을 그대로 고정 픽스처로 쓴다(`HOMEBREW_NO_AUTO_UPDATE=1 brew
    // install -n --formula gnupg`). 이 출력은 원 설계 §3.4의 "`->`가 한 번도
    // 나오지 않는다"는 단언이 틀렸음을 보여준 실측이다 — dependents 섹션은
    // `이름 old -> new` 형태로 나오고, 힌트 산문 줄(백틱 포함)이 헤더와 이름
    // 줄 사이에 끼어들며, 전체 블록이 한 번 더 반복 출력된다.
    #[test]
    fn parses_real_measured_brew_install_dry_run_output_with_dependents_fixture() {
        let sample = "\
gnupg 2.5.20 is already installed but outdated (so it will be upgraded).
==> Would install 1 formula:
gnupg
==> Would upgrade 4 dependencies for gnupg:
p11-kit
libgcrypt
libksba
pinentry
==> Would upgrade 3 dependents of upgraded formula:
Disable this behaviour by setting `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1`.
Hide these hints with `HOMEBREW_NO_ENV_HINTS=1` (see `man brew`).
gpgme    2.1.2   -> 2.2.0
gpgmepp  2.1.0   -> 2.2.0
poppler  26.06.0 -> 26.09.0
==> Would install 1 formula:
gnupg
==> Would upgrade 4 dependencies for gnupg:
p11-kit
libgcrypt
libksba
pinentry
==> Would upgrade 3 dependents of upgraded formula:
Disable this behaviour by setting `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1`.
Hide these hints with `HOMEBREW_NO_ENV_HINTS=1` (see `man brew`).
gpgme    2.1.2   -> 2.2.0
gpgmepp  2.1.0   -> 2.2.0
poppler  26.06.0 -> 26.09.0
";
        let affected = parse_brew_install_dry_run_affected(sample, "gnupg");
        // 대상(gnupg)은 빠지고, 나머지가 등장 순서대로 중복 없이 모여야 한다.
        assert_eq!(
            affected,
            vec![
                "p11-kit".to_string(),
                "libgcrypt".to_string(),
                "libksba".to_string(),
                "pinentry".to_string(),
                "gpgme".to_string(),
                "gpgmepp".to_string(),
                "poppler".to_string(),
            ]
        );
        // 힌트 산문 줄이 이름으로 잘못 섞여 들어가지 않아야 한다.
        assert!(!affected.iter().any(|n| n.contains("Disable")));
        assert!(!affected.iter().any(|n| n.contains("Hide")));
        // 대상 자신은 목록에서 빠져야 한다(off-by-one 재발 방지, 리뷰 지적 #8).
        assert!(!affected.contains(&"gnupg".to_string()));
    }

    // env(HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK)가 실제로 걸렸을 때의 출력 —
    // dependents 섹션 자체가 사라진다(위임자 실측: "->" 매치 0건). 이 경우에도
    // 정확히 파싱되어야 한다(파서가 env 적용 성패에 의존하지 않는다는 요구).
    #[test]
    fn parses_brew_install_dry_run_output_without_dependents_section() {
        let sample = "\
==> Would install 1 formula:
gh
==> Would install 1 dependency for gh:
libpsl
";
        let affected = parse_brew_install_dry_run_affected(sample, "gh");
        assert_eq!(affected, vec!["libpsl".to_string()]);
    }

    // 새 파서(설치용)와 기존 파서(업데이트용)를 헷갈리지 않는지 확인 — install
    // dry-run 출력을 기존 parse_brew_dry_run_affected(→만 인식)에 넣으면
    // dependents 줄만 잡고 "Would install" 섹션의 대상 자신(gh)은 놓친다는
    // 차이를 문서화하는 회귀 테스트다(§3.4 지적 2 — 기존 파서를 재사용하면
    // 안 되는 이유).
    #[test]
    fn install_dry_run_parser_differs_from_update_parser_on_no_arrow_lines() {
        let sample = "==> Would install 1 formula:\ngh\n";
        assert_eq!(parse_brew_dry_run_affected(sample), Vec::<String>::new());
        assert_eq!(
            parse_brew_install_dry_run_affected(sample, "gh"),
            Vec::<String>::new()
        );
    }

    // 신규 요구사항: PATH 노출 확인은 셸을 스폰하지 않고 파일 읽기만으로 판정한다.
    #[test]
    fn path_visibility_detects_dir_via_etc_paths_entries() {
        let etc_entries = vec!["/opt/homebrew/bin".to_string(), "/usr/bin".to_string()];
        assert!(is_dir_in_shell_path(
            "/opt/homebrew/bin",
            &etc_entries,
            None
        ));
        assert!(!is_dir_in_shell_path("/some/other/dir", &etc_entries, None));
    }

    #[test]
    fn path_visibility_detects_dir_via_rc_file_content() {
        let tmp_home = std::env::temp_dir().join(format!(
            "malgn-vscode-devtools-rc-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp_home);
        std::fs::create_dir_all(&tmp_home).expect("임시 홈 디렉터리 생성 실패");
        std::fs::write(
            tmp_home.join(".zshrc"),
            "export PATH=\"/Users/hopegiver/Library/pnpm/bin:$PATH\"\n",
        )
        .expect("rc 파일 쓰기 실패");

        assert!(is_dir_in_shell_path(
            "/Users/hopegiver/Library/pnpm/bin",
            &[],
            Some(&tmp_home)
        ));
        assert!(!is_dir_in_shell_path(
            "/never/mentioned/dir",
            &[],
            Some(&tmp_home)
        ));

        let _ = std::fs::remove_dir_all(&tmp_home);
    }

    #[test]
    fn hint_target_follows_login_shell() {
        assert_eq!(hint_target_for_shell("/bin/zsh"), "~/.zprofile");
        assert_eq!(hint_target_for_shell("/bin/bash"), "~/.bash_profile");
        assert_eq!(
            hint_target_for_shell("/usr/local/bin/fish"),
            "~/.config/fish/config.fish"
        );
        assert_eq!(hint_target_for_shell(""), "~/.profile");
    }
}
