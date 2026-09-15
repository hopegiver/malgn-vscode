// ==================== 플랫폼 순수함수(설계 §D) ====================
// 정본: docs/design/devtools-windows-parity.md §D. 이 개발 머신은 macOS이고
// Windows 실행 경로는 여기서 실행 검증이 불가능하다(cargo check --target
// x86_64-pc-windows-msvc가 ring의 C 빌드로 실패 — 위임서 §1.1 실측). 그래서
// Windows 로직을 `cfg`가 아니라 `Platform` 인자로 분기시켜 macOS `cargo test`가
// Windows 분기의 "로직"까지 전부 실행하도록 이 파일에 모은다 — 이 코드베이스가
// `classify_install_method(canonical, home, pnpm_home, brew_prefixes)`에서 이미
// 쓰는 기법(순수함수 + 주입된 환경 인자)을 플랫폼 축으로 확장한 것뿐이다.
//
// 잔여 `cfg(windows)` 표면(이 파일 밖, CI만 검증 가능)은 의도적으로 얇게
// 유지한다: `platform_now()`(여기, 3줄) + `path_exists()`의 windows 분기(여기,
// ~8줄) + `process_util::SilentCommand::windowed()`(~6줄) + 기존
// `is_writable_by_current_user`/`force_kill_process_group`(무변경, 재사용).

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Platform {
    Mac,
    Win,
}

/// 이 크레이트에서 `cfg!(windows)`를 읽는 **유일한 자리** 중 하나(나머지 하나는
/// `path_exists`). 호출부는 이 값을 받아 순수함수에 전달하기만 한다.
pub(crate) fn platform_now() -> Platform {
    if cfg!(windows) {
        Platform::Win
    } else {
        Platform::Mac
    }
}

/// 경로 템플릿 확장에 쓰는 뿌리들 — 전부 주입 가능(테스트가 가짜 값을 넣는다).
/// `system_root`는 설계 문서 D.1 스케치에는 없던 필드지만, Windows
/// `default_path_dirs`(`%SystemRoot%\system32` 등)를 리터럴이 아니라 값으로
/// 정확히 표현하려면 필요하다 — mac의 `/usr/bin` 같은 고정 상수를 그대로 쓰지
/// 않고, 이 필드가 `None`이면 실측상 항상 참인 `C:\Windows`로 대체한다(§D.2).
pub(crate) struct EnvRoots {
    pub(crate) home: Option<PathBuf>,
    pub(crate) appdata: Option<PathBuf>,
    pub(crate) local_appdata: Option<PathBuf>,
    pub(crate) program_files: Option<PathBuf>,
    #[allow(dead_code)] // %ProgramFiles(x86)% 토큰 확장에서 쓰지만, git 후보는
    // 리터럴 절대경로로 고정돼 있어(B.5) 이 필드가 실제 소비되는 곳은
    // expand_path_tokens 하나뿐이다 — 죽은 필드가 아니라 아직 후보 하나만
    // 쓰는 필드.
    pub(crate) program_files_x86: Option<PathBuf>,
    #[allow(dead_code)] // Runner::Pnpm 해석은 기존 pnpm DevTool 정의를 재사용해
    // pnpm_home을 직접 쓰지 않는다(중복 방지, install_resolver.rs 기존 주석) —
    // classify_install_method_windows가 pnpm 표준 위치 판별에 쓴다.
    pub(crate) pnpm_home: Option<PathBuf>,
    pub(crate) system_root: Option<PathBuf>,
}

impl EnvRoots {
    /// 실제 환경을 읽는 **유일한 자리**. 여기만 `std::env::var`/`dirs::home_dir`에
    /// 닿고, 이 파일의 나머지 함수는 전부 순수하다.
    pub(crate) fn from_env() -> Self {
        EnvRoots {
            home: dirs::home_dir(),
            appdata: std::env::var("APPDATA").ok().map(PathBuf::from),
            local_appdata: std::env::var("LOCALAPPDATA").ok().map(PathBuf::from),
            program_files: std::env::var("ProgramFiles").ok().map(PathBuf::from),
            program_files_x86: std::env::var("ProgramFiles(x86)").ok().map(PathBuf::from),
            pnpm_home: std::env::var("PNPM_HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            system_root: std::env::var("SystemRoot").ok().map(PathBuf::from),
        }
    }
}

fn system_root_or_default(roots: &EnvRoots) -> PathBuf {
    roots
        .system_root
        .clone()
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

// ==================== ① 경로 토큰 확장 ====================

/// `~/`(Mac), `%USERPROFILE%`/`%APPDATA%`/`%LOCALAPPDATA%`/`%ProgramFiles%`/
/// `%ProgramFiles(x86)%`(Win) 확장. 셸을 거치지 않고 문자열 치환만 한다. 필요한
/// 토큰이 있는데 그 뿌리가 `None`이면(예: 이 머신에 `%APPDATA%`가 없음)
/// `None`을 돌려준다 — 잘못된 경로를 "그럴듯하게" 만들어내지 않는다.
pub(crate) fn expand_path_tokens(plat: Platform, template: &str, roots: &EnvRoots) -> Option<String> {
    match plat {
        Platform::Mac => {
            if let Some(rest) = template.strip_prefix("~/") {
                let home = roots.home.as_ref()?;
                Some(home.join(rest).to_string_lossy().to_string())
            } else {
                Some(template.to_string())
            }
        }
        Platform::Win => {
            let mut result = template.to_string();
            // `%ProgramFiles(x86)%`을 `%ProgramFiles%`보다 먼저 확인한다 — 두
            // 문자열은 서로 접두 관계가 아니라 순서 자체는 안전하지만, 의도를
            // 명확히 하기 위해 더 구체적인 토큰을 앞에 둔다.
            let subs: [(&str, &Option<PathBuf>); 5] = [
                ("%USERPROFILE%", &roots.home),
                ("%APPDATA%", &roots.appdata),
                ("%LOCALAPPDATA%", &roots.local_appdata),
                ("%ProgramFiles(x86)%", &roots.program_files_x86),
                ("%ProgramFiles%", &roots.program_files),
            ];
            for (token, value) in subs {
                if result.contains(token) {
                    let v = value.as_ref()?;
                    result = result.replace(token, &v.to_string_lossy());
                }
            }
            Some(result)
        }
    }
}

// ==================== ② 실행파일 확장자 ====================

pub(crate) fn exe_extensions(plat: Platform) -> &'static [&'static str] {
    match plat {
        // ".exe"가 ".cmd"보다 먼저 — BatBadBut류 인자 이스케이프 위험이 없는
        // 네이티브 실행파일을 우선한다(설계 §D.2).
        Platform::Win => &[".exe", ".cmd", ".bat"],
        Platform::Mac => &[""],
    }
}

// ==================== ③ PATH 구분자 ====================

pub(crate) fn path_separator(plat: Platform) -> char {
    match plat {
        Platform::Mac => ':',
        Platform::Win => ';',
    }
}

fn dir_separator(plat: Platform) -> char {
    match plat {
        Platform::Mac => '/',
        Platform::Win => '\\',
    }
}

// ==================== ④ 기본 PATH 디렉터리 ====================

/// **중요**: Windows 경로 조립은 `std::path::Path`/`PathBuf::join`을 쓰지
/// 않는다 — `Path`/`PathBuf`는 **컴파일 호스트**의 구분자 규칙을 따른다.
/// 이 크레이트가 macOS에서 빌드되는 한 `PathBuf::join`은 이 함수가 다루는
/// 문자열에 `\`가 아니라 `/`를 끼워 넣는다(실측: 최초 구현에서 골든 테스트가
/// 이 사실을 그대로 잡아냈다 — `C:\Windows/system32`처럼 구분자가 섞여
/// 나왔다). 그래서 Windows 경로는 전부 **문자열 포맷**으로만 조립한다.
fn win_join(base: &str, suffix: &str) -> String {
    format!("{}\\{}", base.trim_end_matches('\\'), suffix.trim_matches('\\'))
}

/// Mac은 현행 6개 리터럴 그대로(`process::build_child_path_env`가 이 값을
/// 그대로 물려받는다 — 무변경 보장의 핵심). Win은 시스템 기본 3개 +
/// npm/pnpm/WindowsApps(winget 앱 실행 별칭) 3개.
pub(crate) fn default_path_dirs(plat: Platform, roots: &EnvRoots) -> Vec<String> {
    match plat {
        Platform::Mac => [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        Platform::Win => {
            let sys_root = system_root_or_default(roots).to_string_lossy().to_string();
            let mut dirs = vec![
                win_join(&sys_root, "system32"),
                sys_root.clone(),
                win_join(&win_join(&sys_root, "System32"), "Wbem"),
            ];
            if let Some(appdata) = &roots.appdata {
                dirs.push(win_join(&appdata.to_string_lossy(), "npm"));
            }
            if let Some(local_appdata) = &roots.local_appdata {
                let local = local_appdata.to_string_lossy();
                dirs.push(win_join(&local, "pnpm"));
                dirs.push(win_join(&win_join(&local, "Microsoft"), "WindowsApps"));
            }
            dirs
        }
    }
}

// ==================== ⑤ PATH 합성(자식 프로세스에 물려줄 PATH) ====================

/// `runner_path`에서 마지막 구분자 앞부분(디렉터리)을 뽑는다. **문자열 기반**
/// — `Path::new(rp).parent()`를 쓰면 위 `win_join` 주석과 같은 이유로 컴파일
/// 호스트가 macOS일 때 `\`를 구분자로 인식하지 못해 전체 문자열을 파일명
/// 하나로 오인한다(`.parent()`가 빈 문자열/None을 반환 — 실측으로 확인된
/// 회귀).
fn windows_parent_dir(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('\\');
    trimmed.rfind('\\').map(|idx| trimmed[..idx].to_string())
}

/// `build_child_path_env`(dev_tools/process.rs)의 정본. `runner_path`의 bin
/// 디렉터리를 최우선으로 넣고 `default_path_dirs`를 뒤에 덧붙인다. 빈 문자열
/// 항목은 배제한다 — POSIX/Windows 모두 PATH의 빈 항목은 CWD를 의미하고, CWD
/// 검색은 이 설계가 구조적으로 제거하려는 하이재킹 표면이다(G-1/G-3).
pub(crate) fn compose_path_env(plat: Platform, runner_path: Option<&str>, roots: &EnvRoots) -> String {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(rp) = runner_path {
        let bin_dir = match plat {
            // mac은 기존 std::path::Path 기반 로직 그대로(무변경) — 이 머신이
            // 곧 macOS이므로 Path의 호스트 구분자(`/`)와 실제 의미가 일치한다.
            Platform::Mac => std::path::Path::new(rp)
                .parent()
                .map(|p| p.to_string_lossy().to_string()),
            Platform::Win => windows_parent_dir(rp),
        };
        if let Some(s) = bin_dir {
            if !s.is_empty() {
                dirs.push(s);
            }
        }
    }
    for d in default_path_dirs(plat, roots) {
        if !d.is_empty() && !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs.join(&path_separator(plat).to_string())
}

/// 탐지(=이미 설치된 위치를 찾는) 목적으로만 쓰는 PATH 문자열. `compose_path_env`
/// (자식에게 물려줄 통제된 PATH)와 달리, 실제 로그인 사용자의 PATH를 최대한
/// 넓게 반영해야 GUI 앱의 제한된 PATH(예: macOS Finder 실행 시 launchctl 기본
/// PATH만 상속하는 것과 같은 문제, Windows도 동일 위험)를 보완할 수 있다. 빈
/// 항목은 동일하게 배제한다.
pub(crate) fn effective_search_path_dirs(plat: Platform, process_path: &str, roots: &EnvRoots) -> Vec<String> {
    let mut dirs: Vec<String> = process_path
        .split(path_separator(plat))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for d in default_path_dirs(plat, roots) {
        if !d.is_empty() && !dirs.iter().any(|existing| paths_equal(plat, existing, &d)) {
            dirs.push(d);
        }
    }
    dirs
}

pub(crate) fn build_search_path_var(plat: Platform, process_path: &str, roots: &EnvRoots) -> String {
    effective_search_path_dirs(plat, process_path, roots).join(&path_separator(plat).to_string())
}

fn paths_equal(plat: Platform, a: &str, b: &str) -> bool {
    let norm = |s: &str| -> String {
        let t = s.trim_end_matches(['/', '\\']);
        match plat {
            Platform::Win => t.to_lowercase(),
            Platform::Mac => t.to_string(),
        }
    };
    norm(a) == norm(b)
}

// ==================== ⑥ PATH 스캔(bare-name → 절대경로 후보) ====================

/// `which`/`where`의 대체. `path_var`를 분할해 각 디렉터리 × `exe_extensions`
/// 조합으로 후보 절대경로 **문자열**을 만든다 — 파일시스템을 건드리지 않는다
/// (존재 확인은 호출부가 주입한 `exists` 클로저 몫). 빈 디렉터리 항목은
/// 건너뛴다(빈 항목=CWD, G-1/G-3).
pub(crate) fn path_scan_candidates(plat: Platform, path_var: &str, name: &str) -> Vec<String> {
    let sep = dir_separator(plat);
    let mut out = Vec::new();
    for dir in path_var.split(path_separator(plat)) {
        if dir.is_empty() {
            continue;
        }
        let dir = dir.trim_end_matches(['/', '\\']);
        for ext in exe_extensions(plat) {
            out.push(format!("{dir}{sep}{name}{ext}"));
        }
    }
    out
}

// ==================== ⑦ PATH 가시성 판정(Windows 정본) ====================

/// `compute_path_visibility`(dev_tools/diagnostics.rs)의 Windows 분기가 쓰는
/// 순수 판정. Windows는 대소문자 무시 + 후행 구분자 정규화 비교.
pub(crate) fn dir_in_path_var(plat: Platform, dir: &str, path_var: &str) -> bool {
    path_var
        .split(path_separator(plat))
        .any(|entry| !entry.is_empty() && paths_equal(plat, entry, dir))
}

// ==================== ⑧ 인용 규칙(터미널에 넘길 명령행) ====================

fn is_mac_bare_token(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | ':' | '@' | '=' | '-'))
}

/// Win: PowerShell 작은따옴표(`'`→`''`) — 작은따옴표 안에서는 변수·서브식
/// 확장이 일어나지 않아 `$`/`` ` ``가 무해해진다(G-7). Mac: 토큰이
/// `[A-Za-z0-9._/:@=-]+`이면 무인용(현행과 바이트 동일 유지), 그 외엔 POSIX
/// 작은따옴표(`'`→`'\''`).
pub(crate) fn quote_token(plat: Platform, token: &str) -> String {
    match plat {
        Platform::Win => format!("'{}'", token.replace('\'', "''")),
        Platform::Mac => {
            if is_mac_bare_token(token) {
                token.to_string()
            } else {
                format!("'{}'", token.replace('\'', "'\\''"))
            }
        }
    }
}

/// Win: `& '<program>' 'arg1' 'arg2' ...`(호출 연산자 `&` + 프로그램도 항상
/// 인용). Mac: 공백으로 이어붙이되 각 토큰은 `quote_token` 규칙을 따른다 —
/// 절대경로·서브커맨드처럼 특수문자가 없는 값은 무인용이라 기존
/// `format!("{gh} auth login")` 출력과 바이트 단위로 동일하다.
pub(crate) fn build_terminal_command_line(plat: Platform, program: &str, args: &[&str]) -> String {
    match plat {
        Platform::Win => {
            let mut parts = vec![format!("& {}", quote_token(plat, program))];
            parts.extend(args.iter().map(|a| quote_token(plat, a)));
            parts.join(" ")
        }
        Platform::Mac => {
            let mut parts = vec![quote_token(plat, program)];
            parts.extend(args.iter().map(|a| quote_token(plat, a)));
            parts.join(" ")
        }
    }
}

// ==================== ⑨ 파일 존재 확인(잔여 cfg 표면 — 얇게 유지) ====================

/// `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe`는 앱 실행 별칭(APPEXECLINK
/// reparse point)이라 `Path::is_file()`이 false를 줄 수 있다(설계 §D.2, §8
/// 미검증 항목 1) — `symlink_metadata`는 reparse point를 따라가지 않고 그
/// 자체의 존재만 보므로 이 문제를 피한다. 이 함수 자체는 CI(Windows 실기)에서만
/// 검증 가능하다.
#[cfg(windows)]
pub(crate) fn path_exists(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

#[cfg(not(windows))]
pub(crate) fn path_exists(path: &str) -> bool {
    std::path::Path::new(path).is_file()
}

// ==================== ⑩ resolve_binary의 플랫폼 정본(D.3) ====================

/// `cli_launcher::resolve_binary`가 감싸는 순수(-에 가까운) 정본. 절대 후보를
/// 먼저 `exists`로 확인하고(주입된 클로저 — 테스트는 가짜 파일시스템을 넣는다),
/// 실패하면 Windows만 `path_scan_candidates`로 PATH×확장자 조합을 확인한다
/// (프로세스를 하나도 띄우지 않는다 — G-1 bare-name 폴백 폐지). Mac의 bare-name
/// `--version` 서브프로세스 프로브는 이 함수 밖(cli_launcher.rs)에 남는다 —
/// 그 자체가 실제 프로세스를 스폰하는 부수효과라 순수함수로 옮길 수 없다.
pub(crate) fn resolve_binary_with(
    plat: Platform,
    candidates: &[&str],
    path_var: &str,
    bare_name: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    for candidate in candidates {
        if exists(candidate) {
            return Some((*candidate).to_string());
        }
    }
    match plat {
        Platform::Mac => None,
        Platform::Win => {
            for scanned in path_scan_candidates(plat, path_var, bare_name) {
                if exists(&scanned) {
                    return Some(scanned);
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
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
}
