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
    // `%ProgramFiles(x86)%` 토큰 확장(expand_path_tokens)과 Windows 기본 PATH의
    // git 32비트 설치 디렉터리 유도(default_path_dirs, 이슈
    // 01m30vamyw8z58b9pk8h5epmgh)에서 쓴다.
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
/// node/npm/pnpm/WindowsApps(winget 앱 실행 별칭) 관련 디렉터리.
///
/// 실사용자 재현 버그(2026-09-18): `wrangler.cmd`가 정확히 하드코딩 후보
/// (`%APPDATA%\npm\wrangler.cmd`)와 100% 일치하는 위치에 있는데도 앱이
/// "설치 안 됨"으로 표시했다. 원인은 후보 탐색이 아니라 **탐지 이후 실행**
/// 단계였다: npm이 생성하는 `.cmd` shim(wrangler.cmd 등)은 자기 옆에
/// `node.exe`가 없으면(실측: `%APPDATA%\npm`에는 없다 — node.exe는 별도
/// 설치 위치에 있다) 내부적으로 `SET "_prog=node"`로 바꿔 bare `node`를
/// 호출하고, 이 호출은 `.cmd`를 해석하는 `cmd.exe` 자신이 **자신의 PATH
/// 환경변수**로 찾는다 — 그 PATH가 바로 이 함수가 만드는 값(자식 프로세스에
/// 주입되는 PATH)이다. 이전에는 이 목록에 Node 설치 디렉터리가 전혀 없어
/// wrangler.cmd 파일 자체는 정확히 찾고도 그 안의 `node` 호출이 실패해
/// `--version`이 통째로 실패했다(→ check_tool_version이 None → installed:
/// false). 아래 두 경로는 mod.rs DEV_TOOLS의 Node windows_path_candidates
/// (`C:\Program Files\nodejs\node.exe`, `%LOCALAPPDATA%\Programs\nodejs\
/// node.exe`)와 디렉터리 부분이 동일하다 — 새 값을 지어낸 게 아니라 이미
/// 이 코드베이스가 "Node가 있을 만한 자리"로 신뢰하는 두 곳을 그대로
/// 재사용한 것이다.
///
/// 실사용자 재현 버그(2026-09-21, hub 이슈 01m30vamyw8z58b9pk8h5epmgh):
/// 카탈로그 "업데이트" 버튼이 `Command 'git' not found or is in an unsafe
/// location (current directory)`로 실패했다. `refresh_marketplaces` →
/// `run_claude_command`(plugins/mod.rs)이 `claude`를 스폰할 때 이 함수가
/// 만든 PATH를 그대로 주입하는데, 그 PATH에 git 설치 디렉터리가 전혀 없어
/// `claude` 내부의 `git clone` 호출이 PATH에서 git을 못 찾았다 — 메시지의
/// "unsafe location(현재 디렉터리)"은 git의 dubious-ownership 경고가 아니라
/// `claude`가 CWD 폴백을 안전상 거부했다는 뜻이다(이 크레이트 `cli_launcher.rs`가
/// Windows bare-name 스폰을 피하는 것과 같은 방어). 아래 세 경로는 mod.rs
/// DEV_TOOLS의 Git windows_path_candidates
/// (`C:\Program Files\Git\cmd\git.exe`, `C:\Program Files (x86)\Git\cmd\
/// git.exe`, `%LOCALAPPDATA%\Programs\Git\cmd\git.exe`)와 디렉터리 부분이
/// 동일하다 — Node 때와 같은 재사용, 새 값을 지어내지 않았다.
/// `default_path_dirs_win_contains_every_dir_derived_from_git_windows_path_candidates`
/// (platform_tests.rs)가 두 목록의 어긋남을 잡는다.
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
            if let Some(program_files) = &roots.program_files {
                let pf = program_files.to_string_lossy();
                dirs.push(win_join(&pf, "nodejs"));
                dirs.push(win_join(&win_join(&pf, "Git"), "cmd"));
            }
            if let Some(program_files_x86) = &roots.program_files_x86 {
                dirs.push(win_join(
                    &win_join(&program_files_x86.to_string_lossy(), "Git"),
                    "cmd",
                ));
            }
            if let Some(appdata) = &roots.appdata {
                dirs.push(win_join(&appdata.to_string_lossy(), "npm"));
            }
            if let Some(local_appdata) = &roots.local_appdata {
                let local = local_appdata.to_string_lossy();
                dirs.push(win_join(&local, "pnpm"));
                dirs.push(win_join(&win_join(&local, "Programs"), "nodejs"));
                dirs.push(win_join(
                    &win_join(&win_join(&local, "Programs"), "Git"),
                    "cmd",
                ));
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

/// PATH 항목이 실제로 절대경로인지 판정한다(B3 — 2라운드 차단). 빈 문자열은
/// 호출부가 이미 걸러내므로 여기서는 다루지 않는다. Mac: `/`로 시작. Win:
/// 드라이브 절대(`C:\...`, `C:/...`) 또는 UNC(`\\server\share`)만 인정한다 —
/// `C:`(드라이브 **상대**, 그 드라이브의 현재 디렉터리를 의미), `.`, `bin` 같은
/// 상대 항목은 전부 거부한다. 반드시 **트림 전** 원본 문자열로 판정해야 한다
/// (`C:\`를 trim_end_matches로 다듬어 `C:`로 만든 뒤 판정하면 길이 검사에
/// 걸려 정상 드라이브 루트까지 오탐 거부된다).
fn is_absolute_dir(plat: Platform, dir: &str) -> bool {
    match plat {
        Platform::Mac => dir.starts_with('/'),
        Platform::Win => {
            if dir.starts_with(r"\\") {
                return true;
            }
            let bytes = dir.as_bytes();
            bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/')
        }
    }
}

/// `which`/`where`의 대체. `path_var`를 분할해 각 디렉터리 × `exe_extensions`
/// 조합으로 후보 절대경로 **문자열**을 만든다 — 파일시스템을 건드리지 않는다
/// (존재 확인은 호출부가 주입한 `exists` 클로저 몫). 빈 디렉터리 항목은
/// 건너뛴다(빈 항목=CWD, G-1/G-3). B3(2라운드 차단): 절대경로가 아닌 항목(빈
/// 문자열, `.`, `bin` 같은 상대 항목, `C:` 드라이브 상대 표기 포함)도 전부
/// 버린다 — Windows PATH는 후행 `;`으로 빈 항목이 흔하고, 빈 항목 + 이름 +
/// 확장자를 합치면 CWD 기준 상대 경로 후보가 나와 `exists()`가 CWD 기준으로
/// true를 줄 수 있다(그 상대 경로가 그대로 "탐지된 도구 경로"가 되어 실행·
/// 화면 표시·명령행 조립까지 이어진다).
pub(crate) fn path_scan_candidates(plat: Platform, path_var: &str, name: &str) -> Vec<String> {
    let sep = dir_separator(plat);
    let mut out = Vec::new();
    for raw_dir in path_var.split(path_separator(plat)) {
        if raw_dir.is_empty() || !is_absolute_dir(plat, raw_dir) {
            continue;
        }
        let dir = raw_dir.trim_end_matches(['/', '\\']);
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

/// 체인 명령행 구분자 — Mac은 로그인 셸(bash/zsh)이 실행하므로 `&&`(앞 명령
/// 실패 시 뒤 명령을 건너뜀)를 쓴다. Windows 내장 `powershell.exe`(5.1)는
/// `&&`/`||` 파이프라인 체인 연산자를 지원하지 않는다(PowerShell 7+에서야
/// 추가됨) — 대신 `;`(문장 구분자)로 잇는다. 이 경우 앞 명령이 실패해도 뒤
/// 명령이 실행된다는 의미 차이가 있지만, 사용자가 직접 보는 대화형 터미널
/// 창이라 실패 시 출력이 그대로 남아 눈에 띈다(치명적이지 않다).
fn chain_separator(plat: Platform) -> &'static str {
    match plat {
        Platform::Mac => " && ",
        Platform::Win => "; ",
    }
}

/// `build_terminal_command_line`에 "이 커맨드에만 적용할 환경변수"를 덧붙인
/// 버전이다(2026-09-18, `claude mcp add --client-secret` 실측 이후 도입 —
/// mcp_manager 설계 §참고). `env`가 비어있으면 `build_terminal_command_line`과
/// 바이트 단위로 동일하다(기존 호출부 무변경).
///
/// 실측(2026-09-18, `claude --version` 2.1.272): `claude mcp add ...
/// --client-secret <값>`처럼 값을 argv로 그냥 붙이면 그 값은 버려지고
/// `--client-secret`은 **항상** bare 플래그로만 동작한다(대화형 프롬프트 또는
/// `MCP_CLIENT_SECRET` 환경변수 중 하나로만 값을 받는다 — `claude mcp add
/// --help`가 명시). 따라서 비밀값을 argv에 절대 넣지 않고, 이 커맨드의 자식
/// 프로세스 환경에만 `MCP_CLIENT_SECRET`을 심어야 한다.
///
/// Mac(POSIX sh/bash/zsh)은 `KEY='value' program args...` 인라인 접두사
/// 문법을 쓴다 — 이 대입은 그 뒤에 오는 단일 simple command의 환경에만
/// 적용되고 셸 세션 전체에는 남지 않는다(POSIX 표준 동작, 별도 정리가
/// 필요 없다). Windows(powershell.exe 5.1)는 이 문법이 없어 `$env:KEY =
/// 'value'; <call>; Remove-Item Env:KEY` 형태의 복합 문장으로 흉내낸다 —
/// `$env:` 대입은 프로세스 전체 세션에 영향을 주므로 뒤이어 체이닝되는 다음
/// 커맨드(예: `mcp login`)에 새지 않도록 실행 직후 `Remove-Item`으로 지운다.
pub(crate) fn build_terminal_command_line_with_env(
    plat: Platform,
    program: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> String {
    let base = build_terminal_command_line(plat, program, args);
    if env.is_empty() {
        return base;
    }
    match plat {
        Platform::Mac => {
            let prefix: String = env
                .iter()
                .map(|(key, value)| format!("{key}={} ", quote_token(plat, value)))
                .collect();
            format!("{prefix}{base}")
        }
        Platform::Win => {
            let set_stmts: String = env
                .iter()
                .map(|(key, value)| format!("$env:{key} = {}; ", quote_token(plat, value)))
                .collect();
            let clear_stmts: String = env
                .iter()
                .map(|(key, _)| format!("; Remove-Item Env:{key} -ErrorAction SilentlyContinue"))
                .collect();
            format!("{set_stmts}{base}{clear_stmts}")
        }
    }
}

/// `build_terminal_command_line_with_env`를 커맨드 여러 개로 체이닝한 버전 —
/// `mcp_install`(claude mcp add → claude mcp login 체인)의 첫 번째 커맨드에만
/// `MCP_CLIENT_SECRET`을 주입할 때 쓴다. 각 원소는 `(program, args, env)`
/// (`TerminalCommandWithEnv` — clippy `type_complexity` 회피용 별칭).
pub(crate) type TerminalCommandWithEnv<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);

pub(crate) fn build_chained_terminal_command_line_with_env(
    plat: Platform,
    commands: &[TerminalCommandWithEnv],
) -> String {
    commands
        .iter()
        .map(|(program, args, env)| build_terminal_command_line_with_env(plat, program, args, env))
        .collect::<Vec<_>>()
        .join(chain_separator(plat))
}

/// `%SystemRoot%`(없으면 `C:\Windows`) 기준 절대경로를 조립한다(B2 — 2라운드
/// 차단). Windows 표준 시스템 유틸(`powershell.exe`/`taskkill.exe` 등)을
/// bare-name으로 스폰하면 Rust std의 실행파일 검색 순서 2번("현재 실행 파일의
/// 디렉터리")에 걸린다 — 이 앱의 Windows 배포물은 단일 포터블 exe라 보통
/// 다운로드 폴더에 놓이고, 거기 동명의 악성 실행파일이 있으면 그것이 대신
/// 실행된다. `relative`는 `SystemRoot` 기준 상대경로 문자열이다(예:
/// `System32\taskkill.exe`).
pub(crate) fn windows_system_tool(roots: &EnvRoots, relative: &str) -> String {
    win_join(&system_root_or_default(roots).to_string_lossy(), relative)
}

// ==================== ⑧-1 PowerShell 인코딩 전달(M3) ====================

/// M3(review-devtools-windows-parity-2026-09-15.md) 처방(b): `cli_launcher::
/// spawn_terminal_window`가 `powershell.exe`에 스크립트를 넘길 때 `-Command
/// <script>`가 아니라 `-EncodedCommand <base64>`를 쓴다. 인코딩된 문자열은
/// base64 표준 알파벳(`A-Za-z0-9+/=`)만 쓰므로, 공백·따옴표·`&`·`;` 같은
/// 특수문자가 이 인자에는 아예 등장하지 않는다 — Rust의 Windows 커맨드라인
/// 인코딩(`Command::args`가 인자마다 다시 하는 재인용)도, `powershell.exe`
/// 자신의 명령행 파서(`-Command`의 값 경계를 정하는 규칙)도 이 인자에서는
/// 특수 처리할 게 없어진다. 남는 파싱 표면은 PowerShell이 **디코딩한 뒤의**
/// 스크립트 텍스트를 해석하는 한 층뿐이다(`quote_token`의 작은따옴표
/// 이스케이프가 여전히 그 층의 안전을 책임진다 — 이 함수는 그 결과 문자열을
/// 그대로 감싸 전달 형식만 바꾼다, 이스케이프 규칙 자체는 무변경).
///
/// 이 함수 자체(UTF-16LE 인코딩 + base64)는 **순수함수**라 이 머신(Mac)에서
/// 100% 실행 검증된다(`platform_tests.rs`의 수동 디코드 왕복 테스트 참고).
/// PowerShell이 실제로 이 인코딩(UTF-16LE, 표준 base64, 패딩 포함)을
/// 기대한다는 사실 자체는 Microsoft 공식 문서 근거이며, 실제 `powershell.exe
/// -EncodedCommand`가 이 출력을 받아 올바르게 디코딩·실행하는지는 이
/// 머신에서 실기 검증할 수 없다(미검증 — Windows PC 필요).
pub(crate) fn encode_powershell_command(script: &str) -> String {
    use base64::Engine;
    let utf16le_bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(utf16le_bytes)
}

/// N1(2라운드 비차단, 이번에 함께 처리): Windows에서 자식 프로세스의 현재
/// 디렉터리를 고정한다. `.cmd` shim(`npm.cmd` 등)은 `cmd.exe`가 해석하는데,
/// `cmd.exe`의 명령 해석은 **현재 디렉터리를 먼저** 본다(Rust std의 실행파일
/// 검색 규칙과는 별개 문제 — `cmd.exe` 자신의 내부 규칙이다). `npm.cmd`가
/// 내부에서 bare `node`로 폴백하면 CWD의 `node.exe`가 하이재킹 표면이 된다.
/// 앱의 CWD는 보통 포터블 exe를 둔 다운로드 폴더를 그대로 물려받으므로,
/// `%SystemRoot%`(사용자 쓰기 권한이 없다)로 고정해 이 표면을 없앤다. Mac은
/// `None`(기존 동작 무변경 — 이 함수를 호출하는 쪽이 Windows에서만 적용한다).
pub(crate) fn child_current_dir(plat: Platform, roots: &EnvRoots) -> Option<PathBuf> {
    match plat {
        Platform::Mac => None,
        Platform::Win => Some(system_root_or_default(roots)),
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

// 1,000줄 규율: 이 파일의 테스트 모듈이 순수함수 본문보다 커져(합치면
// 1,100줄대) 별도 파일로 분리했다 — 정본(순수함수)은 이 파일에 그대로 남고,
// 테스트만 `platform_tests.rs`로 옮겼다.
#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;

