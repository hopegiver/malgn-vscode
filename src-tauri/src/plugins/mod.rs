// ---------------- 플러그인/마켓플레이스 ----------------
// `lib.rs`에 있던 "설치된 플러그인 카탈로그" + "마켓플레이스" + "플러그인 실제
// 업데이트(사용자 명시 승인)" 세 절을 그대로 옮긴 것이다 — 로직은 한 글자도
// 바꾸지 않았다. `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]`
// 진입점만 이 파일에 두고, 조회 로직은 서브모듈(`installed`/`marketplace`)로
// 옮겼다.
//
// `claude` CLI를 셸을 거치지 않고 프로그램명+인자 배열로 직접 실행한다(인젝션
// 경로 없음). 대부분의 커맨드는 이 파일이 이미 읽어둔 신뢰할 수 있는 값(설치된
// 플러그인 id·마켓플레이스 id)만 인자로 받아 자유 텍스트 입력 경로 자체가 없다.
// 유일한 예외가 `marketplace::add_marketplace`/`remove_marketplace`(마켓플레이스
// 설정 탭의 "추가"/"제거") — 사용자가 직접 입력한 source/name이 argv로 들어가며,
// 셸을 거치지 않는 것은 동일하지만 빈 값·공백만·개행은 marketplace.rs가
// 프런트와 별도로 재검증한다(자세한 내용은 marketplace.rs 상단 주석).

mod global;
mod installed;
mod marketplace;

use crate::dev_tools::platform::{self, Platform};
use serde::Serialize;
use std::sync::{Mutex, TryLockError};
use std::time::Duration;

// ---------------- claude 자식 프로세스 상한 시간 ----------------
// 이 모듈이 띄우는 모든 `claude` 자식 프로세스는 아래 셋 중 하나를 상한으로 받는다
// (`run_claude_command`의 필수 인자 — 상한 없는 호출 경로가 구조적으로 없다).
// 초과하면 `dev_tools::run_process_with_timeout_cancellable`이 프로세스 그룹을
// SIGTERM → 3초 유예 → SIGKILL 하고 직속 자식을 wait()한 뒤 사용자에게 "시간 초과"
// 의미의 실패 메시지를 돌려준다. 그룹·트리를 벗어난 자손이 파이프를 물고 있어도
// 출력 수거는 `DRAIN_GRACE`(2초)에서 끊고 반환하므로 호출(과 `PLUGIN_OP_LOCK` 가드)의
// 수명은 유한하다 — 대신 그런 자손은 고아로 남을 수 있다.
//
// 값의 근거(실측값이 아니라 판단 — 이 코드베이스의 기존 상한과 같은 결): 같은
// 앱 안의 `dev_tools` 설치 계획이 npm 전역 설치·`claude update`에 300초를
// (plan_table.rs), 버전 조회에 10초를 쓴다. 정상 완료 시간의 수 배 여유를 두되
// 사용자가 스피너를 하염없이 보지 않는 선에서 끊는다. 거짓 타임아웃의 비용은
// "사용자가 다시 누른다"(세 종류 모두 재실행해도 안전 — 마켓플레이스 갱신은
// git fetch, install/update는 같은 id 재처리)이고, 무한 대기의 비용은 앱
// 프리즈·고아 프로세스라서 상한은 넉넉한 쪽으로 잡았다.
//
// 상한은 아래 세 상수가 정본이다 — 다른 곳에 숫자를 복사해 두지 않는다(시간 초과
// 메시지의 초 값도 인자에서 동적 생성). 값을 바꿀 때 같이 볼 곳은 이 파일의
// `update_plugin`(두 단계 합산 최악 시간)뿐이다.

/// 마켓플레이스 git clone/fetch 계열 — `marketplace add`, `marketplace update`
/// (전체 갱신은 등록된 마켓플레이스를 순차로 fetch하므로 이 계열 최악 케이스),
/// `update_plugin`의 1단계(단일 마켓플레이스 갱신). 네트워크 의존.
pub(crate) const TIMEOUT_MARKETPLACE_SYNC: Duration = Duration::from_secs(180);
/// `marketplace remove` — 로컬 설정/캐시 디렉터리 정리뿐이라 네트워크가 없다.
pub(crate) const TIMEOUT_MARKETPLACE_LOCAL: Duration = Duration::from_secs(30);
/// `plugin install`/`plugin update` — 플러그인 소스 fetch + 캐시 복사.
/// `dev_tools`의 `claude update`/npm 전역 설치 상한(300초)과 같은 값.
pub(crate) const TIMEOUT_PLUGIN_INSTALL: Duration = Duration::from_secs(300);

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn list_installed_plugins() -> Vec<installed::InstalledPlugin> {
    tauri::async_runtime::spawn_blocking(installed::read_installed_plugins)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_known_marketplaces() -> Vec<marketplace::MarketplaceInfo> {
    marketplace::read_known_marketplaces()
}

/// 플러그인과 무관하게 개인이 직접 만든 전역(user-level) 에이전트/스킬을
/// 읽기 전용으로 조회한다. `list_known_marketplaces`처럼 스캔 대상이 소수
/// 파일이라 sync로 충분하다.
#[tauri::command]
pub fn list_global_catalog() -> global::GlobalCatalog {
    global::read_global_catalog()
}

#[derive(Serialize, Debug)]
pub(crate) struct CommandResult {
    success: bool,
    message: String,
}

/// `mcp_manager/process.rs`와 동일한 이유(결정 5.2 — 상수 비공유)로 독립적으로 둔다.
/// `dev_tools::DEV_TOOLS`의 Claude 항목과 값은 같지만(같은 실측 근거) 파일을
/// 공유하지 않는다.
///
/// 버그(2026-09-19, hub 이슈 01m2wm459wfe4s6g6kzgcwqd45): 이 상수가 원래
/// mac 후보 3개뿐이었고, `platform_now()`로 Windows 후보를 고르는 분기가
/// 아예 없었다 — `dev_tools::tool_path_candidates(def)`(mod.rs)가
/// `platform_now()`에 따라 `path_candidates`/`windows_path_candidates`를
/// 골라 쓰는 것과 달리, 여기 `run_claude_command`는 Windows에서도 이
/// mac 전용 배열을 그대로 `resolve_binary_expand_home`에 넘겼다. Windows에서
/// 이 3개 후보는 전부 `~/`·`/`로 시작하는 POSIX 경로라
/// `platform::expand_path_tokens`(Win 분기)가 치환할 토큰이 없어 문자열
/// 그대로 반환되고, `path_exists`는 당연히 전부 false다 — 그 결과 claude
/// 탐지가 절대경로 후보 단계를 완전히 건너뛰고 매번 PATH 스캔 폴백
/// (`resolve_binary`의 Windows 분기, 프로세스 PATH 환경변수 의존)에만
/// 의존했다. 이 PATH 환경변수가 GUI로 띄운 이 앱에 정확히 반영되지 않으면
/// (네이티브 설치기가 두는 `%USERPROFILE%\.local\bin`, winget 앱 실행
/// 별칭이 두는 `%LOCALAPPDATA%\Microsoft\WinGet\Links` 등은 npm 전역
/// prefix(`%APPDATA%\npm`)와 달리 `default_path_dirs`에도 없다) claude
/// 실행 자체가 엉뚱한 경로로 흘러가거나 실패한다 — 이 플러그인 설치/업데이트
/// 경로에서만 재현되고 `dev_tools`의 claude 조회(체크 화면)에서는 재현되지
/// 않았던 이유이기도 하다(그쪽은 애초에 `tool_path_candidates`를 쓴다).
const CLAUDE_PATH_CANDIDATES_MAC: [&str; 4] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
    // P2-3b: 앱이 설치한 Node의 npm 전역 prefix에 설치된 Claude(dev_tools DEV_TOOLS와 같은 값).
    crate::dev_tools::NPM_GLOBAL_CLAUDE_CANDIDATE,
];
/// `dev_tools::DEV_TOOLS`의 Claude `windows_path_candidates`와 동일한 순서·값
/// (네이티브 설치기 → npm 전역 shim → winget 앱 실행 별칭). 설계 §B.5 그대로
/// — 전부 미검증(실기 Windows PC 필요, windows-latest CI도 claude CLI가 없어
/// 이 배열의 "값"만 검증하고 실제 파일 존재 여부는 검증하지 못한다).
const CLAUDE_PATH_CANDIDATES_WIN: [&str; 3] = [
    r"%USERPROFILE%\.local\bin\claude.exe",
    r"%APPDATA%\npm\claude.cmd",
    r"%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe",
];

/// `dev_tools::tool_path_candidates`와 동일한 패턴(순수함수 + `Platform` 인자
/// 주입, §D) — `platform_now()`를 직접 호출하는 `claude_path_candidates()`가
/// 이 함수를 감싼다. 인자로 분리해 두면 이 머신(Mac)의 `cargo test`에서도
/// `Platform::Win` 분기를 직접 실행 검증할 수 있다.
fn claude_path_candidates_for(plat: Platform) -> &'static [&'static str] {
    match plat {
        Platform::Mac => &CLAUDE_PATH_CANDIDATES_MAC,
        Platform::Win => &CLAUDE_PATH_CANDIDATES_WIN,
    }
}

fn claude_path_candidates() -> &'static [&'static str] {
    claude_path_candidates_for(platform::platform_now())
}

/// 이슈 01m30vamyw8z58b9pk8h5epmgh 2단계: `run_claude_command`가 실패했을 때
/// "우리가 실제로 조립한 값"을 사용자가 보는 실패 메시지에 그대로 덧붙인다.
/// v0.2.8에 git PATH 보강(cdea6eb)과 WinGet\Links 보강(487cda8)이 둘 다
/// 들어갔는데도 같은 오류가 실기에서 재현되는데, 코드 정적 분석만으로는 더
/// 좁혀지지 않는다(위임서 1단계 기록 참고) — 다음 실패 재현에서 사용자가 이
/// 블록 하나만 붙여줘도 "PATH에 git 디렉터리가 실제로 없었는지" vs "PATH엔
/// 있는데 claude가 못 찾았는지" vs "CWD가 원인인지"를 코드를 더 안 읽고도
/// 구분할 수 있게 한다.
///
/// `resolved`/`path_env`는 자식에게 실제로 전달한 값 그대로(재계산·추정
/// 없음). `EnvRoots::from_env()`를 여기서 별도로 다시 읽는 것은 PATH 조립에
/// 쓰인 개별 환경변수 원본값(예: `ProgramFiles`가 아예 비어있었는지, 다른
/// 값이었는지)을 조립된 PATH 문자열과 나란히 보여주기 위해서다 — 조립된
/// 문자열만으로는 "어느 뿌리가 비어서 그 디렉터리가 빠졌는지"를 역산하기
/// 어렵다.
///
/// `child_cwd`는 `run_claude_command`가 실제로 `.current_dir()`에 넘긴 값
/// 그대로(재계산 없음) — `Some`이면 `crate::global_cwd::default_global_cwd()`가
/// 고른 경로를 명시적으로 고정했다는 뜻이고, `None`이면 그 함수가 존재하는
/// 후보를 하나도 찾지 못해 `.current_dir()` 자체를 호출하지 않았다는 뜻이다
/// (이때는 이 프로세스(앱)의 현재 작업 디렉터리를 자식이 그대로 물려받는다 —
/// 예전엔 이 호출부가 이 폴백 경로 없이 매번 이 상태였다. Windows 실기에서
/// 이 값이 `C:\Windows\system32`로 관측됐다, hub decision
/// 01m33w4pp3gyrjapczh1q7j4wm — 다만 이 값 하나만으로 그 실기에서 난 `claude`
/// 실행 실패의 인과가 확정되는 것은 아니다, 진단 블록의 나머지 두 값(주입 PATH
/// 전문, `ProgramFiles`)이 아직 회신되지 않았다).
///
/// ⚠ PATH·환경변수 값에는 사용자명이 포함될 수 있다 — devTools.ts 로그
/// 패널의 기존 경고와 같은 문구를 여기서도 그대로 붙인다.
fn diagnostic_block(resolved: &str, path_env: &str, child_cwd: Option<&std::path::Path>) -> String {
    let roots = platform::EnvRoots::from_env();
    let cwd_line = match child_cwd {
        Some(dir) => format!("{}(명시적으로 고정됨)", dir.to_string_lossy()),
        None => {
            let app_cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|e| format!("(읽기 실패: {e})"));
            format!("{app_cwd}(고정 후보를 찾지 못해 앱의 현재 작업 디렉터리를 그대로 상속)")
        }
    };
    let show = |v: &Option<std::path::PathBuf>| -> String {
        v.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(미설정)".to_string())
    };
    format!(
        "\n\n[진단 정보 — 사용자명·사내 경로가 포함될 수 있습니다. 외부(예: GitHub 이슈)에 붙여넣기 전 확인하세요]\n\
         - claude 실행 파일(해석됨): {resolved}\n\
         - 자식 프로세스에 주입한 PATH: {path_env}\n\
         - 자식 프로세스 CWD: {cwd_line}\n\
         - ProgramFiles: {}\n\
         - ProgramFiles(x86): {}\n\
         - LOCALAPPDATA: {}\n\
         - APPDATA: {}\n\
         - SystemRoot: {}",
        show(&roots.program_files),
        show(&roots.program_files_x86),
        show(&roots.local_appdata),
        show(&roots.appdata),
        show(&roots.system_root),
    )
}

/// claude 실행 파일 자체를 못 찾은 경우(절대경로 후보 + PATH 스캔 모두
/// 실패)의 진단 — `resolved`/`path_env`가 아직 없으므로 `diagnostic_block`과
/// 별도로 CWD + 환경변수 원본만 보여준다. `claude_path_candidates()`가 이
/// 실행에서 실제로 어떤 후보 목록을 썼는지(플랫폼 분기가 제대로 먹었는지)도
/// 함께 보여준다.
fn diagnostic_block_unresolved() -> String {
    let roots = platform::EnvRoots::from_env();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|e| format!("(읽기 실패: {e})"));
    let candidates = claude_path_candidates().join(", ");
    let show = |v: &Option<std::path::PathBuf>| -> String {
        v.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(미설정)".to_string())
    };
    format!(
        "\n\n[진단 정보 — 사용자명·사내 경로가 포함될 수 있습니다. 외부(예: GitHub 이슈)에 붙여넣기 전 확인하세요]\n\
         - 시도한 절대경로 후보: {candidates}\n\
         - 프로세스 CWD: {cwd}\n\
         - ProgramFiles: {}\n\
         - ProgramFiles(x86): {}\n\
         - LOCALAPPDATA: {}\n\
         - APPDATA: {}\n\
         - SystemRoot: {}\n\
         - PATH(원본, 스캔에 쓰인 값): {}",
        show(&roots.program_files),
        show(&roots.program_files_x86),
        show(&roots.local_appdata),
        show(&roots.appdata),
        show(&roots.system_root),
        std::env::var("PATH").unwrap_or_else(|_| "(미설정)".to_string()),
    )
}

/// `claude`를 찾아 `run_claude_binary`로 실행한다. `timeout`은 필수 — 작업 종류별
/// 상수(`TIMEOUT_*`)를 호출부가 고른다.
///
/// 동기 함수다(자식이 끝나거나 상한에 걸릴 때까지 블로킹) — tauri 커맨드
/// 진입점은 반드시 `run_blocking`(spawn_blocking)을 거쳐 부른다.
pub(crate) fn run_claude_command(args: &[&str], timeout: Duration) -> CommandResult {
    let Some(resolved) =
        crate::cli_launcher::resolve_binary_expand_home(claude_path_candidates(), "claude")
    else {
        return CommandResult {
            success: false,
            message: format!(
                "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).{}",
                diagnostic_block_unresolved()
            ),
        };
    };
    let path_env = crate::dev_tools::build_child_path_env(Some(&resolved));

    // 마켓플레이스 갱신/설치·업데이트처럼 특정 프로젝트에 묶이지 않는 전역
    // 호출이라 `crate::global_cwd::default_global_cwd()`(workspace 루트 →
    // 홈 디렉터리 순, 존재 확인 후 첫 값)로 CWD를 명시적으로 고정한다 —
    // 이전엔 이 호출부만 `.current_dir()`을 부르지 않아 앱 프로세스의 CWD를
    // 그대로 물려받았다(구조적 비대칭, `diagnostic_block` 문서 참고). 후보가
    // 모두 존재하지 않으면 `None`이 와서 `.current_dir()`을 아예 호출하지
    // 않는다 — 존재하지 않는 경로를 넘겨 spawn 자체를 실패시키는 사고(포터블
    // exe·workspace 미설정 사용자 등)를 피하기 위해서다.
    let cwd = crate::global_cwd::default_global_cwd();

    run_claude_binary(&resolved, args, &path_env, cwd.as_deref(), timeout)
}

/// 이미 해석된 실행 파일로 자식을 띄우고 상한 시간 안에 결과를 받는다. 타임아웃·
/// kill·파이프 소비는 `dev_tools::run_process_with_timeout_cancellable`을
/// 재사용한다(stdin=null, stdout/stderr 리더 스레드 동시 소비로 파이프 버퍼
/// 교착 없음, 새 프로세스 그룹 스폰 + 상한 초과 시 그룹 kill→wait, Windows는
/// `taskkill /T /F`로 트리 종료 + `Child::kill` 폴백 + `.silent()`, 종료 뒤 출력 수거는
/// `DRAIN_GRACE`에서 끊는다). `.output()` 직접 호출은 상한도
/// kill도 없어 CLI가 멈추면 워커 스레드가 영구히 묶였다.
///
/// 테스트가 실제 `claude` 없이 `/bin/sh` 등으로 이 경로를 검증할 수 있게
/// `run_claude_command`와 분리했다.
fn run_claude_binary(
    resolved: &str,
    args: &[&str],
    path_env: &str,
    cwd: Option<&std::path::Path>,
    timeout: Duration,
) -> CommandResult {
    let owned_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let output = crate::dev_tools::run_process_with_timeout_cancellable(
        resolved,
        &owned_args,
        path_env,
        &[],
        timeout,
        None,
        None,
        cwd,
    );

    if let Some(spawn_error) = output.spawn_error {
        // spawn_error는 "실행할 수 없습니다: {e}" 형태 — 기존 메시지와 같은 문장이 된다.
        return CommandResult {
            success: false,
            message: format!("claude 명령을 {spawn_error}{}", diagnostic_block(resolved, path_env, cwd)),
        };
    }

    let stdout = output.stdout.trim().to_string();
    let stderr = output.stderr.trim().to_string();

    if output.timed_out {
        return CommandResult {
            success: false,
            message: format!(
                "{}{}",
                timeout_message(args, timeout, &stdout, &stderr),
                diagnostic_block(resolved, path_env, cwd)
            ),
        };
    }

    if output.exit_code == Some(0) {
        CommandResult {
            success: true,
            message: if stdout.is_empty() {
                "완료되었습니다.".to_string()
            } else {
                stdout
            },
        }
    } else {
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "알 수 없는 오류로 실패했습니다.".to_string()
        };
        CommandResult {
            success: false,
            message: format!("{msg}{}", diagnostic_block(resolved, path_env, cwd)),
        }
    }
}

/// 상한 초과 시 사용자에게 보이는 첫 문단. "시간 초과"와 "프로세스를 종료했다"를
/// 명시하고, 그때까지 나온 출력의 끝부분을 덧붙여 어디서 멈췄는지 가늠하게 한다
/// (git clone 진행 로그 등). 상한 초는 인자에서 계산해 상수와 어긋날 수 없다.
fn timeout_message(args: &[&str], timeout: Duration, stdout: &str, stderr: &str) -> String {
    let partial = if !stderr.is_empty() { stderr } else { stdout };
    let tail = tail_chars(partial, 500);
    let mut msg = format!(
        "시간 초과: `claude {}` 명령이 {}초 안에 끝나지 않아 중단하고 프로세스를 종료했습니다. \
         네트워크 상태를 확인한 뒤 다시 시도하세요.",
        args.join(" "),
        timeout.as_secs()
    );
    if !tail.is_empty() {
        msg.push_str(&format!("\n\n[중단 직전 출력]\n{tail}"));
    }
    msg
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    format!("…{}", text.chars().skip(count - max_chars).collect::<String>())
}

/// 자식 프로세스를 띄우는 플러그인 커맨드(update/refresh/install/add/remove
/// marketplace)의 동시 실행을 막는 전역 락.
///
/// 왜 필요한가: 이 커맨드들이 sync였을 때는 메인 스레드가 서로를 자연 직렬화했지만
/// async + spawn_blocking으로 옮기며 그 보장이 사라졌다. 같은 마켓플레이스의
/// 플러그인 둘을 연달아 "업데이트"하면 `claude plugin marketplace update`가 같은
/// git 클론에서 동시에 돌고(index.lock 충돌), CLI의 `installed_plugins.json`·
/// `known_marketplaces.json` read-modify-write가 겹치면 항목이 유실될 수 있다.
///
/// `dev_tools::EXECUTION_LOCK`과 일부러 공유하지 않는다: 그 락은 brew/winget/npm
/// 계열 도구 설치·업데이트의 패키지 매니저 전역 락 충돌을 막는 용도라, 공유하면
/// 최대 8분(SYNC 180 + INSTALL 300초)짜리 플러그인 작업이 무관한 도구 업데이트를
/// 거부시키고 그 반대도 마찬가지다. 두 작업군이 건드리는 자원(플러그인/마켓플레이스
/// 상태 파일·git 클론 vs 패키지 매니저 상태)이 겹치지 않는다. 남는 위험은 `claude
/// update`가 바이너리를 교체하는 동안 플러그인 작업이 도는 경우뿐인데, 사용자가
/// 터미널에서 같은 조합을 실행해도 생기는 상황이고 어느 쪽이든 실패가 사용자에게
/// 그대로 보이며 재실행해도 안전하다.
static PLUGIN_OP_LOCK: Mutex<()> = Mutex::new(());

const PLUGIN_OP_BUSY_MESSAGE: &str =
    "다른 플러그인 작업이 진행 중입니다. 완료 후 다시 시도해주세요.";

/// `lock`을 대기 없이 잡아 `job`을 실행한다. 이미 잡혀 있으면 `job`을 실행하지 않고
/// 즉시 "작업 중" 실패를 돌려준다(무기한 대기·대기열 없음 — 겹친 요청은 사용자가
/// 끝난 뒤 다시 누르면 된다). 가드는 `job`이 끝날 때(타임아웃 kill·wait 포함)까지
/// 유지되고, `job`이 패닉해도 unwind로 해제된다. 락이 보호하는 데이터가 `()`뿐이라
/// poison은 무시하고 그대로 진행한다(한 번의 패닉이 이후 모든 플러그인 작업을
/// 영구 거부하면 안 된다).
fn with_op_lock<F>(lock: &Mutex<()>, job: F) -> CommandResult
where
    F: FnOnce() -> CommandResult,
{
    let _guard = match lock.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return CommandResult {
                success: false,
                message: PLUGIN_OP_BUSY_MESSAGE.to_string(),
            }
        }
    };
    job()
}

/// 블로킹 작업(자식 프로세스 대기)을 tokio 워커가 아니라 blocking 풀에서 돌린다 —
/// `list_installed_plugins`/`usage_stats`와 같은 관용구. 락은 반드시 blocking 풀의
/// 클로저 안에서 잡는다(메인 스레드·async 워커를 막지 않고, 가드 수명 = 자식 프로세스
/// 수명). 조인 실패(작업 패닉)는 프론트가 그대로 보여줄 수 있는 실패 결과로 변환한다.
async fn run_blocking<F>(job: F) -> CommandResult
where
    F: FnOnce() -> CommandResult + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || with_op_lock(&PLUGIN_OP_LOCK, job))
        .await
        .unwrap_or_else(|e| CommandResult {
            success: false,
            message: format!("작업 스레드가 비정상 종료되었습니다: {e}"),
        })
}

/// `plugin_id`는 `installed_plugins.json`의 키(예: "malgn-agent@malgnsoft-plugins")
/// 형식 그대로 받는다. `claude plugin update`는 로컬에 캐시된 마켓플레이스
/// 메타데이터만 보고 "최신 버전"을 판단하므로, 먼저 `plugin_id`에서 "@" 뒤의
/// 마켓플레이스 이름만 뽑아 `claude plugin marketplace update <marketplace>`로
/// 캐시를 갱신한 뒤에 실제 `claude plugin update`를 실행한다 — 안 그러면 마켓플레이스에
/// 새 버전이 올라와도 캐시가 이를 모른 채 "이미 최신"으로 판단해 아무 변경 없이
/// 성공을 반환한다. 마켓플레이스 갱신이 실패하면 그 결과를 그대로 반환하고
/// `plugin update`는 실행하지 않는다(캐시가 stale한 채로 업데이트해봐야 신뢰 불가).
/// 성공해도 재시작해야 적용된다 — 프론트엔드가 안내 문구를 붙인다.
#[tauri::command]
pub async fn update_plugin(plugin_id: String) -> CommandResult {
    run_blocking(move || {
        // 두 단계가 각자 상한을 가진다(최악 합계 = SYNC + INSTALL).
        if let Some((_, marketplace)) = plugin_id.split_once('@') {
            let refresh_result = run_claude_command(
                &["plugin", "marketplace", "update", marketplace],
                TIMEOUT_MARKETPLACE_SYNC,
            );
            if !refresh_result.success {
                return refresh_result;
            }
        }
        run_claude_command(&["plugin", "update", &plugin_id], TIMEOUT_PLUGIN_INSTALL)
    })
    .await
}

#[tauri::command]
pub async fn refresh_marketplaces() -> CommandResult {
    run_blocking(|| run_claude_command(&["plugin", "marketplace", "update"], TIMEOUT_MARKETPLACE_SYNC)).await
}

/// 마켓플레이스 설정 탭의 "추가" — 실행 로직·입력 검증·고정 마켓플레이스 보호는
/// 전부 `marketplace::run_add_marketplace`에 있다(marketplace.rs 참고). 여기서는
/// `#[tauri::command]` 진입점만 둔다(이 파일의 다른 커맨드들과 동일한 얇은 래퍼
/// 패턴 — `list_known_marketplaces`가 `marketplace::read_known_marketplaces()`를
/// 감싸는 것과 같다). 자식 프로세스를 띄우므로 async + spawn_blocking.
#[tauri::command]
pub async fn add_marketplace(source: String) -> CommandResult {
    run_blocking(move || marketplace::run_add_marketplace(source)).await
}

/// 마켓플레이스 설정 탭의 "제거" — 실행 로직·입력 검증·고정 마켓플레이스 보호는
/// 전부 `marketplace::run_remove_marketplace`에 있다.
#[tauri::command]
pub async fn remove_marketplace(name: String) -> CommandResult {
    run_blocking(move || marketplace::run_remove_marketplace(name)).await
}

/// `plugin_id`는 "name@marketplace" 형식(예: "malgn-agent@malgnsoft-plugins")만 받는다
/// — update_plugin과 동일한 id 포맷. `-y`로 비대화형 확인을 건너뛰고,
/// `-s user`로 설치 스코프를 user로 고정한다(installed_plugins.json에서
/// scope="user"만 "설치된 플러그인"으로 취급하는 기존 규칙과 맞춘다 — plugins/installed.rs 참고).
///
/// 구버전 claude CLI는 `-y`/`-s`를 모르는 옵션으로 거부한다(Windows 실기:
/// "-y 옵션이 없다"). 그래서 먼저 두 플래그를 모두 붙여 실행하고(신버전 = 기존
/// 동작 그대로, `-y`가 없으면 비대화 실행이 확인 프롬프트에서 멈출 수 있다),
/// CLI가 특정 플래그를 "unknown option"으로 거부했을 때만 그 플래그를 뺀 인자로
/// 다시 시도한다. 사전 `--help` 파싱 대신 거부 신호를 쓰는 이유: 도움말 문구 파싱은
/// 버전마다 깨질 수 있지만, 거부 메시지가 안 맞으면 폴백이 안 일어날 뿐 신버전 경로는
/// 어떤 경우에도 이전과 동일하다(fail-safe).
#[tauri::command]
pub async fn install_plugin(plugin_id: String) -> CommandResult {
    run_blocking(move || {
        install_with_flag_fallback(&plugin_id, |args| {
            run_claude_command(args, TIMEOUT_PLUGIN_INSTALL)
        })
    })
    .await
}

/// `plugin install`에 붙일 선택 플래그 집합. 기본은 둘 다 사용(신버전 동작).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InstallFlags {
    /// `-y`(marketplace 선언 command 수락 — 비대화 실행용)
    yes: bool,
    /// `-s user`(설치 스코프 고정)
    scope_user: bool,
}

impl InstallFlags {
    const ALL: InstallFlags = InstallFlags { yes: true, scope_user: true };
}

/// 순수함수 — 플래그 집합에 맞는 `claude` 인자 배열을 만든다. 순서는 기존
/// (`plugin install <id> -y -s user`)과 같다.
fn install_args(plugin_id: &str, flags: InstallFlags) -> Vec<String> {
    let mut args = vec!["plugin".to_string(), "install".to_string(), plugin_id.to_string()];
    if flags.yes {
        args.push("-y".to_string());
    }
    if flags.scope_user {
        args.push("-s".to_string());
        args.push("user".to_string());
    }
    args
}

/// 순수함수 — 실패 메시지(CLI stderr)가 "알 수 없는 옵션" 거부이고 그 옵션이
/// 아직 붙어 있는 `-y`/`--yes` 또는 `-s`/`--scope`이면, 그 플래그를 뺀 다음 시도용
/// 플래그 집합을 돌려준다. 그 외(다른 실패·다른 옵션·이미 뺀 옵션)는 `None` —
/// 재시도하지 않고 원래 실패를 그대로 보고한다.
///
/// 거부 문구는 commander 계열 CLI의 `error: unknown option '-y'`(대소문자·따옴표
/// 종류·`unrecognized option` 변형 허용)를 기준으로 한다.
fn next_flags_after_rejection(stderr: &str, current: InstallFlags) -> Option<InstallFlags> {
    let token = unknown_option_token(stderr)?;
    match token.as_str() {
        "-y" | "--yes" if current.yes => Some(InstallFlags { yes: false, ..current }),
        "-s" | "--scope" if current.scope_user => Some(InstallFlags { scope_user: false, ..current }),
        _ => None,
    }
}

/// 메시지에서 "unknown/unrecognized option" 바로 뒤의 옵션 토큰(`-y` 등)을 뽑는다.
fn unknown_option_token(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for marker in ["unknown option", "unrecognized option"] {
        let Some(pos) = lower.find(marker) else { continue };
        // 마커는 ASCII라 소문자 변환 전후 바이트 오프셋이 같다(비ASCII가 앞에 있어도
        // to_lowercase가 길이를 바꿀 수 있어 원문이 아니라 lower에서 계속 자른다).
        let rest = &lower[pos + marker.len()..];
        let token: String = rest
            .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '\'' | '"' | '`'))
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '\'' | '"' | '`' | ',' | ';' | ')'))
            .collect();
        if token.starts_with('-') {
            return Some(token);
        }
    }
    None
}

/// 설치 실행 + 구버전 폴백 루프. `run`이 실제 프로세스 실행을 맡는다(테스트는 가짜
/// 실행기를 주입해 프로세스를 띄우지 않는다). 플래그는 매 반복 최소 하나씩
/// 줄어들므로(최대 2회 재시도) 무한 루프가 없다. 성공하거나 폴백 대상이 아닌
/// 실패는 즉시 그 결과를 돌려준다.
fn install_with_flag_fallback<F>(plugin_id: &str, mut run: F) -> CommandResult
where
    F: FnMut(&[&str]) -> CommandResult,
{
    let mut flags = InstallFlags::ALL;
    loop {
        let args = install_args(plugin_id, flags);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut result = run(&arg_refs);
        if result.success {
            return with_fallback_note(result, flags);
        }
        match next_flags_after_rejection(&result.message, flags) {
            Some(next) => flags = next,
            None => {
                result = with_fallback_note(result, flags);
                return result;
            }
        }
    }
}

/// 순수함수 — 폴백으로 플래그를 뺀 채 끝난 시도라면(`flags != ALL`) 최종 결과 메시지
/// 끝에 그 사실(뺀 플래그 + `claude update` 권장)을 덧붙인다. 성공·실패 모두 해당
/// (실패 원인이 다른 문제여도 "플래그를 빼고 재시도했다"는 맥락이 진단에 필요하고,
/// `-s user`를 빼면 설치 스코프가 user가 아닐 수 있어 성공이어도 알려야 한다).
/// 폴백이 없었으면(`flags == ALL`) 메시지를 그대로 둔다.
fn with_fallback_note(mut result: CommandResult, flags: InstallFlags) -> CommandResult {
    let mut dropped: Vec<&str> = Vec::new();
    if !flags.yes {
        dropped.push("`-y`");
    }
    if !flags.scope_user {
        dropped.push("`-s user`");
    }
    if dropped.is_empty() {
        return result;
    }
    let scope_hint = if flags.scope_user {
        ""
    } else {
        " 설치 범위가 user가 아닐 수 있어 설치 목록에 보이지 않으면 claude를 업데이트한 뒤 다시 설치해주세요."
    };
    result.message = format!(
        "{}\n\n(구버전 claude CLI 호환: {} 옵션을 거부해 해당 옵션을 빼고 다시 시도했습니다. `claude update`로 업데이트를 권장합니다.{})",
        result.message,
        dropped.join(", "),
        scope_hint
    );
    result
}

// ⚠️ update_plugin()/refresh_marketplaces()/install_plugin()을 실제로 호출하는
// 테스트는 의도적으로 두지 않는다 — 이 머신에 실제 설치된 malgn-agent 플러그인
// (이 에이전트 자신이 로드되어 있는 바로 그 플러그인)을 `cargo test`를 돌릴 때마다
// 매번 실제로 업데이트/재설치해버리는 부작용은 누구도 원하지 않는다. 컴파일 통과
// (`cargo check`)로만 검증했고, 실제 클릭 검증은 사용자가 직접 GUI에서 해야 한다.
//
// 아래 `claude_path_candidates_for*` 테스트는 실제 프로세스를 하나도 띄우지
// 않는 순수함수만 검증한다 — 위 경고와 무관하게 이 머신(Mac)의 `cargo test`가
// Windows 분기의 "후보 배열 선택 로직"까지 실행 검증할 수 있다(§D 패턴,
// `dev_tools::platform_tests.rs`와 동일 기법). 다만 그 경로들이 실제
// Windows 파일시스템에 존재하는지, `claude.cmd`/`claude.exe`를 진짜로
// 찾아내는지는 windows-latest CI(또는 실기)에서만 확인 가능하다.
#[cfg(test)]
mod claude_path_candidates_tests {
    use super::*;
    use crate::dev_tools::platform::EnvRoots;

    fn fake_win_roots() -> EnvRoots {
        EnvRoots {
            home: Some(std::path::PathBuf::from(r"C:\Users\tester")),
            appdata: Some(std::path::PathBuf::from(r"C:\Users\tester\AppData\Roaming")),
            local_appdata: Some(std::path::PathBuf::from(r"C:\Users\tester\AppData\Local")),
            program_files: Some(std::path::PathBuf::from(r"C:\Program Files")),
            program_files_x86: Some(std::path::PathBuf::from(r"C:\Program Files (x86)")),
            pnpm_home: None,
            system_root: Some(std::path::PathBuf::from(r"C:\Windows")),
        }
    }

    /// 회귀 가드: mac 배열은 무변경이어야 한다(기존 호출부 계약 유지).
    #[test]
    fn claude_path_candidates_for_mac_unchanged() {
        assert_eq!(
            claude_path_candidates_for(Platform::Mac),
            &CLAUDE_PATH_CANDIDATES_MAC[..]
        );
    }

    /// 이번 버그의 핵심 재발 방지 가드: Windows 분기가 mac 전용(POSIX `/`·`~/`)
    /// 경로를 절대 반환하면 안 된다 — 원래 버그가 정확히 이 상태였다(Windows에서도
    /// CLAUDE_PATH_CANDIDATES_MAC 3개가 그대로 쓰였다). `~`/`/`로 시작하는 문자열이
    /// 하나라도 섞여 있으면 `platform::expand_path_tokens`(Win 분기)가 치환할
    /// 토큰이 없어 그대로 통과되고, 그 결과 `path_exists`가 항상 false를 반환해
    /// 절대경로 탐지 단계 전체가 무력화된다.
    #[test]
    fn claude_path_candidates_for_win_has_no_mac_style_paths() {
        for candidate in claude_path_candidates_for(Platform::Win) {
            assert!(
                !candidate.starts_with('/') && !candidate.starts_with('~'),
                "Windows 후보에 mac 전용 경로가 섞여 있습니다: {candidate}"
            );
        }
    }

    /// Windows 후보는 `platform::exe_extensions(Win)`이 인정하는 실행 확장자
    /// (.exe/.cmd/.bat) 중 하나로 끝나야 한다 — 확장자 없는 파일명은 Windows에서
    /// `CreateProcessW`가 직접 찾지 못한다.
    #[test]
    fn claude_path_candidates_for_win_end_with_known_windows_extension() {
        for candidate in claude_path_candidates_for(Platform::Win) {
            assert!(
                candidate.ends_with(".exe") || candidate.ends_with(".cmd") || candidate.ends_with(".bat"),
                "Windows 후보에 실행 확장자가 없습니다: {candidate}"
            );
        }
    }

    /// `dev_tools::DEV_TOOLS`의 Claude `windows_path_candidates`와 값이
    /// 일치하는지 직접 대조한다 — 결정 5.2(상수 비공유)로 파일은 분리돼
    /// 있지만 "같은 실측 근거"라는 주석의 주장을 코드로도 못박는다. 한쪽만
    /// 고치고 다른 쪽을 깜빡하는 재발을 잡아낸다.
    #[test]
    fn claude_path_candidates_for_win_matches_dev_tools_claude_definition() {
        use crate::dev_tools::{tool_definition, ToolId};
        let dev_tools_def = tool_definition(ToolId::Claude);
        assert_eq!(
            claude_path_candidates_for(Platform::Win),
            dev_tools_def.windows_path_candidates,
            "plugins::CLAUDE_PATH_CANDIDATES_WIN과 dev_tools::DEV_TOOLS Claude 항목이 어긋났습니다"
        );
    }

    /// 후보 문자열의 Windows 토큰(`%APPDATA%` 등)이 실제로 절대경로로 확장되는지
    /// 직접 확인한다 — `resolve_binary_with`가 이 결과를 그대로 `exists()`에
    /// 넘기므로, 여기서 확장이 깨지면 실제 파일이 정확히 그 자리에 있어도 영원히
    /// 못 찾는다. 가짜 `EnvRoots`를 주입해 이 머신(Mac)에서도 Windows 토큰 치환
    /// 로직 자체를 실행 검증한다(파일 존재 여부는 검증하지 않는다 — 그건 CI/실기 몫).
    #[test]
    fn claude_path_candidates_for_win_tokens_expand_to_absolute_paths() {
        let roots = fake_win_roots();
        let expected = [
            r"C:\Users\tester\.local\bin\claude.exe",
            r"C:\Users\tester\AppData\Roaming\npm\claude.cmd",
            r"C:\Users\tester\AppData\Local\Microsoft\WinGet\Links\claude.exe",
        ];
        for (candidate, want) in claude_path_candidates_for(Platform::Win).iter().zip(expected) {
            let expanded = platform::expand_path_tokens(Platform::Win, candidate, &roots);
            assert_eq!(
                expanded.as_deref(),
                Some(want),
                "토큰 확장 결과가 기대와 다릅니다: {candidate}"
            );
        }
    }
}

// 이슈 01m30vamyw8z58b9pk8h5epmgh 2단계 검증: 진단 블록에 기대 항목이 실제로
// 들어가는지만 확인한다(값 자체는 이 머신의 실제 환경변수를 읽으므로 문자열
// 일치가 아니라 "라벨이 등장하는지"만 검증 — CI 머신마다 ProgramFiles 등의
// 실제 값은 다르다).
#[cfg(test)]
mod diagnostic_block_tests {
    use super::*;

    #[test]
    fn diagnostic_block_contains_resolved_path_env_and_cwd_note() {
        let block = diagnostic_block("/opt/homebrew/bin/claude", "/opt/homebrew/bin:/usr/bin", None);
        assert!(block.contains("claude 실행 파일(해석됨): /opt/homebrew/bin/claude"));
        assert!(block.contains("자식 프로세스에 주입한 PATH: /opt/homebrew/bin:/usr/bin"));
        assert!(block.contains("자식 프로세스 CWD"));
        assert!(block.contains("ProgramFiles:"));
        assert!(block.contains("ProgramFiles(x86):"));
        assert!(block.contains("LOCALAPPDATA:"));
        assert!(block.contains("APPDATA:"));
        assert!(block.contains("SystemRoot:"));
        // 사용자명/경로 유출 경고 문구 자체도 빠지면 안 된다.
        assert!(block.contains("사용자명·사내 경로가 포함될 수 있습니다"));
    }

    /// `child_cwd`가 `Some`이면(고정 성공) 그 경로가 "명시적으로 고정됨" 표시와
    /// 함께 그대로 보여야 한다 — 앱 자신의 CWD가 아니라 실제로 자식에게 넘긴
    /// 값이어야 신뢰할 수 있는 진단이 된다.
    #[test]
    fn diagnostic_block_shows_pinned_cwd_when_some() {
        let dir = std::path::PathBuf::from("/Users/tester/workspace");
        let block = diagnostic_block("claude", "PATH", Some(&dir));
        assert!(block.contains("자식 프로세스 CWD: /Users/tester/workspace(명시적으로 고정됨)"));
    }

    /// `child_cwd`가 `None`이면(존재하는 후보를 찾지 못함) "상속" 문구로
    /// 정직하게 표시해야 한다 — 이전 동작(항상 상속)을 오인시키지 않는다.
    #[test]
    fn diagnostic_block_shows_inherited_note_when_none() {
        let block = diagnostic_block("claude", "PATH", None);
        assert!(block.contains("고정 후보를 찾지 못해 앱의 현재 작업 디렉터리를 그대로 상속"));
    }

    #[test]
    fn diagnostic_block_unresolved_contains_tried_candidates_and_raw_path() {
        let block = diagnostic_block_unresolved();
        assert!(block.contains("시도한 절대경로 후보:"));
        assert!(block.contains("프로세스 CWD:"));
        assert!(block.contains("PATH(원본, 스캔에 쓰인 값):"));
        assert!(block.contains("사용자명·사내 경로가 포함될 수 있습니다"));
    }

    /// 실패 메시지 조립 지점(`run_claude_command`)이 실제로 이 블록을 이어
    /// 붙이는지는 여기서 직접 부르지 않는다(파일 상단 경고대로 실제 claude
    /// 프로세스를 스폰하는 테스트는 의도적으로 두지 않는다) — 대신 `format!`
    /// 조립 자체가 패닉 없이 동작하고 원본 메시지가 앞부분에 그대로 남는지는
    /// 문자열 조립 계약으로 고정한다.
    #[test]
    fn diagnostic_block_appends_after_original_message_without_losing_it() {
        let original = "Failed to refresh marketplace 'malgnsoft-plugins': some git error";
        let combined = format!("{original}{}", diagnostic_block("claude", "PATH", None));
        assert!(combined.starts_with(original));
        assert!(combined.len() > original.len());
    }
}

// ---------------- 상한 시간 · kill · 파이프 교착 테스트 ----------------
// 실제 `claude`는 절대 띄우지 않는다(위 경고) — `run_claude_binary`가 실행 파일
// 경로를 인자로 받으므로 `/bin/sh`·`ping` 같은 무해한 프로세스로 같은 경로(
// 타임아웃 → 그룹 kill → wait → 실패 결과)를 그대로 실행 검증한다.
#[cfg(test)]
mod run_claude_binary_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn timeout_message_mentions_timeout_seconds_and_partial_output() {
        let msg = timeout_message(
            &["plugin", "marketplace", "update"],
            Duration::from_secs(180),
            "",
            "Cloning into 'x'...",
        );
        assert!(msg.starts_with("시간 초과"));
        assert!(msg.contains("180초"));
        assert!(msg.contains("claude plugin marketplace update"));
        assert!(msg.contains("종료"));
        assert!(msg.contains("Cloning into 'x'..."));
    }

    #[test]
    fn timeout_message_omits_output_block_when_nothing_was_printed() {
        let msg = timeout_message(&["plugin", "install", "a@b"], Duration::from_secs(300), "", "");
        assert!(!msg.contains("중단 직전 출력"));
    }

    #[test]
    fn tail_chars_keeps_only_the_end_with_ellipsis() {
        assert_eq!(tail_chars("abc", 10), "abc");
        assert_eq!(tail_chars("abcdefghij", 3), "…hij");
        // 멀티바이트도 문자 경계에서 자른다(패닉 없음).
        assert_eq!(tail_chars("가나다라마", 2), "…라마");
    }

    #[test]
    fn timeouts_are_ordered_by_task_kind() {
        // 로컬 정리 < 네트워크 갱신 < 설치/업데이트.
        assert!(TIMEOUT_MARKETPLACE_LOCAL < TIMEOUT_MARKETPLACE_SYNC);
        assert!(TIMEOUT_MARKETPLACE_SYNC < TIMEOUT_PLUGIN_INSTALL);
    }

    /// 정상 종료: stdout이 메시지로, exit 0이 success로 매핑된다.
    #[cfg(unix)]
    #[test]
    fn success_maps_stdout_to_message() {
        let r = run_claude_binary("/bin/sh", &["-c", "echo done"], "/usr/bin:/bin", None, Duration::from_secs(5));
        assert!(r.success, "{}", r.message);
        assert_eq!(r.message, "done");
    }

    /// 비정상 종료: stderr가 메시지 앞에, 진단 블록이 뒤에 붙는다(기존 계약).
    #[cfg(unix)]
    #[test]
    fn nonzero_exit_maps_stderr_to_message_with_diagnostics() {
        let r = run_claude_binary(
            "/bin/sh",
            &["-c", "echo boom >&2; exit 3"],
            "/usr/bin:/bin",
            None,
            Duration::from_secs(5),
        );
        assert!(!r.success);
        assert!(r.message.starts_with("boom"));
        assert!(r.message.contains("[진단 정보"));
        assert!(!r.message.contains("시간 초과"));
    }

    #[cfg(unix)]
    #[test]
    fn spawn_failure_reports_cannot_execute() {
        let r = run_claude_binary(
            "/nonexistent/claude-binary",
            &["plugin", "update"],
            "/usr/bin:/bin",
            None,
            Duration::from_secs(5),
        );
        assert!(!r.success);
        assert!(r.message.starts_with("claude 명령을 실행할 수 없습니다"));
    }

    /// 핵심: 멈춘 자식은 상한에 끊기고 "시간 초과" 실패로 돌아오며, 자식(sh)과
    /// 손자(sleep) 모두 실제로 종료·회수된다(고아 없음). sh가 자기 pid와 손자
    /// pid를 파일에 남기게 해 사후에 `pid_alive`로 확인한다. 손자는 sh 사망 후
    /// init이 회수하므로 비동기 — 최대 2초 폴링한다.
    #[cfg(unix)]
    #[test]
    fn hanging_child_is_killed_and_reaped_with_timeout_result() {
        use crate::process_util::pid_alive;
        let dir = std::env::temp_dir().join(format!("plugins_timeout_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pid_file, grandchild_file) = (dir.join("pid"), dir.join("grandchild"));
        let script = format!(
            "echo $$ > '{}'; sleep 30 & echo $! > '{}'; wait",
            pid_file.display(),
            grandchild_file.display()
        );

        let started = Instant::now();
        let r = run_claude_binary(
            "/bin/sh",
            &["-c", &script],
            "/usr/bin:/bin",
            None,
            Duration::from_millis(600),
        );
        let elapsed = started.elapsed();

        assert!(!r.success);
        assert!(r.message.starts_with("시간 초과"), "{}", r.message);
        // 상한 0.6초 + SIGTERM 유예(최대 3초) 안에 끝나야 한다 — 30초 sleep을 기다렸다면 kill 실패.
        assert!(elapsed < Duration::from_secs(5), "kill이 동작하지 않음: {elapsed:?}");

        let pid: u32 = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        let grandchild: u32 = std::fs::read_to_string(&grandchild_file).unwrap().trim().parse().unwrap();
        // 직속 자식은 run_claude_binary 반환 시점에 wait()까지 끝나 있어야 한다.
        assert!(!pid_alive(pid), "직속 자식이 회수되지 않았다");
        let deadline = Instant::now() + Duration::from_secs(2);
        while pid_alive(grandchild) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!pid_alive(grandchild), "손자 프로세스가 고아로 남았다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 파이프 교착: 64KB 파이프 버퍼를 훨씬 넘는 stdout·stderr를 동시에 쏟아도
    /// (둘 중 하나를 먼저 다 읽으려는 순차 소비라면 교착) 상한 안에 끝난다.
    #[cfg(unix)]
    #[test]
    fn large_stdout_and_stderr_do_not_deadlock() {
        let started = Instant::now();
        let r = run_claude_binary(
            "/bin/sh",
            &["-c", "head -c 2000000 /dev/zero | tr '\\0' 'o' >&2 & head -c 2000000 /dev/zero | tr '\\0' 'x'; wait"],
            "/usr/bin:/bin",
            None,
            Duration::from_secs(20),
        );
        assert!(r.success, "{}", &r.message[..r.message.len().min(200)]);
        assert!(r.message.len() >= 2_000_000);
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    /// Windows 등가물 — `dev_tools` 테스트와 같은 `PING.EXE` 기법. 미검증:
    /// Windows CI에서만 컴파일·실행된다.
    #[cfg(windows)]
    #[test]
    fn hanging_child_times_out_on_windows() {
        let started = Instant::now();
        let r = run_claude_binary(
            r"C:\Windows\System32\PING.EXE",
            &["-n", "31", "127.0.0.1"],
            r"C:\Windows\System32;C:\Windows",
            None,
            Duration::from_millis(500),
        );
        assert!(!r.success);
        assert!(r.message.starts_with("시간 초과"), "{}", r.message);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}

// ---------------- 동시 실행 락 테스트 ----------------
// 실제 `claude`는 띄우지 않는다 — 전역 락 대신 테스트 전용 로컬 락을 `with_op_lock`에
// 넘겨 같은 코드 경로를 검증한다.
#[cfg(test)]
mod op_lock_tests {
    use super::*;
    use std::sync::mpsc;

    fn ok(msg: &str) -> CommandResult {
        CommandResult { success: true, message: msg.to_string() }
    }

    #[test]
    fn second_job_fails_fast_with_busy_while_first_holds_lock_then_succeeds_after_release() {
        static LOCK: Mutex<()> = Mutex::new(());
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();

        let first = std::thread::spawn(move || {
            with_op_lock(&LOCK, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap(); // 해제 신호까지 락을 쥔다
                ok("first")
            })
        });
        started_rx.recv().unwrap();

        let mut ran = false;
        let busy = with_op_lock(&LOCK, || {
            ran = true;
            ok("second")
        });
        assert!(!busy.success);
        assert_eq!(busy.message, PLUGIN_OP_BUSY_MESSAGE);
        assert!(!ran, "Busy면 job 자체를 실행하지 않아야 한다");

        release_tx.send(()).unwrap();
        assert!(first.join().unwrap().success);

        let after = with_op_lock(&LOCK, || ok("third"));
        assert!(after.success);
        assert_eq!(after.message, "third");
    }

    #[test]
    fn lock_is_released_and_poison_ignored_after_job_panics() {
        static LOCK: Mutex<()> = Mutex::new(());
        let panicked = std::thread::spawn(|| with_op_lock(&LOCK, || panic!("job panic"))).join();
        assert!(panicked.is_err());
        assert!(LOCK.is_poisoned());

        let after = with_op_lock(&LOCK, || ok("recovered"));
        assert!(after.success);
        assert_eq!(after.message, "recovered");
    }
}

// install 플래그 폴백의 순수 로직 검증 — 가짜 실행기를 주입하므로 실제 `claude`
// 프로세스를 띄우지 않는다(위 ⚠️ 경고 준수).
#[cfg(test)]
mod install_flag_fallback_tests {
    use super::*;

    const ID: &str = "malgn-agent@malgnsoft-plugins";

    fn fail(msg: &str) -> CommandResult {
        CommandResult { success: false, message: msg.to_string() }
    }
    fn ok() -> CommandResult {
        CommandResult { success: true, message: "완료되었습니다.".to_string() }
    }

    #[test]
    fn install_args_all_flags_matches_legacy_argv() {
        assert_eq!(
            install_args(ID, InstallFlags::ALL),
            ["plugin", "install", ID, "-y", "-s", "user"]
        );
    }

    #[test]
    fn install_args_drops_individual_flags() {
        let no_yes = InstallFlags { yes: false, scope_user: true };
        assert_eq!(install_args(ID, no_yes), ["plugin", "install", ID, "-s", "user"]);
        let no_scope = InstallFlags { yes: true, scope_user: false };
        assert_eq!(install_args(ID, no_scope), ["plugin", "install", ID, "-y"]);
        let none = InstallFlags { yes: false, scope_user: false };
        assert_eq!(install_args(ID, none), ["plugin", "install", ID]);
    }

    #[test]
    fn rejection_of_y_drops_only_yes() {
        let next = next_flags_after_rejection("error: unknown option '-y'", InstallFlags::ALL);
        assert_eq!(next, Some(InstallFlags { yes: false, scope_user: true }));
    }

    #[test]
    fn rejection_of_s_drops_only_scope() {
        let next = next_flags_after_rejection("error: unknown option '-s'\n", InstallFlags::ALL);
        assert_eq!(next, Some(InstallFlags { yes: true, scope_user: false }));
    }

    #[test]
    fn rejection_variants_are_recognized() {
        for msg in [
            "Error: Unknown option \"-y\"",
            "error: unrecognized option `-y`",
            "error: unknown option '--yes'",
            "error: unknown option '-y'\n(Did you mean --yes?)",
            "error: unknown option '-y'\n\n[진단 정보 — ...]",
        ] {
            assert_eq!(
                next_flags_after_rejection(msg, InstallFlags::ALL),
                Some(InstallFlags { yes: false, scope_user: true }),
                "{msg}"
            );
        }
        assert_eq!(
            next_flags_after_rejection("error: unknown option '--scope'", InstallFlags::ALL),
            Some(InstallFlags { yes: true, scope_user: false })
        );
    }

    #[test]
    fn unrelated_failures_do_not_trigger_fallback() {
        for msg in [
            "",
            "알 수 없는 오류로 실패했습니다.",
            "Plugin not found in marketplace",
            "시간 초과: `claude plugin install a@b -y -s user` 명령이 300초 안에 끝나지 않아",
            "error: unknown option '--bogus'",
            "error: unknown option",
            "error: unknown command 'install'",
            "Error: network unreachable (-y not supported by proxy)",
        ] {
            assert_eq!(next_flags_after_rejection(msg, InstallFlags::ALL), None, "{msg}");
        }
    }

    #[test]
    fn already_dropped_flag_rejection_is_not_retried_again() {
        let no_yes = InstallFlags { yes: false, scope_user: true };
        assert_eq!(next_flags_after_rejection("error: unknown option '-y'", no_yes), None);
    }

    #[test]
    fn new_cli_succeeds_first_try_with_legacy_argv_and_no_retry() {
        let mut calls: Vec<Vec<String>> = Vec::new();
        let result = install_with_flag_fallback(ID, |args| {
            calls.push(args.iter().map(|s| s.to_string()).collect());
            ok()
        });
        assert!(result.success);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ["plugin", "install", ID, "-y", "-s", "user"]);
        // 폴백이 없으면 메시지는 기존과 동일(노트 없음).
        assert_eq!(result.message, "완료되었습니다.");
    }

    #[test]
    fn old_cli_rejecting_y_then_s_installs_with_bare_args() {
        let mut calls: Vec<Vec<String>> = Vec::new();
        let result = install_with_flag_fallback(ID, |args| {
            calls.push(args.iter().map(|s| s.to_string()).collect());
            if args.contains(&"-y") {
                fail("error: unknown option '-y'")
            } else if args.contains(&"-s") {
                fail("error: unknown option '-s'")
            } else {
                ok()
            }
        });
        assert!(result.success);
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[1], ["plugin", "install", ID, "-s", "user"]);
        assert_eq!(calls[2], ["plugin", "install", ID]);
    }

    #[test]
    fn old_cli_rejecting_only_y_retries_once_keeping_scope() {
        let mut calls = 0;
        let result = install_with_flag_fallback(ID, |args| {
            calls += 1;
            if args.contains(&"-y") { fail("error: unknown option '-y'") } else { ok() }
        });
        assert!(result.success);
        assert_eq!(calls, 2);
    }

    #[test]
    fn non_flag_failure_is_returned_as_is_without_retry() {
        let mut calls = 0;
        let result = install_with_flag_fallback(ID, |_| {
            calls += 1;
            fail("Plugin not found")
        });
        assert!(!result.success);
        assert_eq!(result.message, "Plugin not found");
        assert_eq!(calls, 1);
    }

    #[test]
    fn fallback_success_message_names_dropped_flag_and_recommends_update() {
        let result = install_with_flag_fallback(ID, |args| {
            if args.contains(&"-y") { fail("error: unknown option '-y'") } else { ok() }
        });
        assert!(result.success);
        assert!(result.message.starts_with("완료되었습니다."), "{}", result.message);
        assert!(result.message.contains("`-y`"), "{}", result.message);
        assert!(!result.message.contains("`-s user`"), "{}", result.message);
        assert!(result.message.contains("claude update"), "{}", result.message);
    }

    #[test]
    fn fallback_then_unrelated_failure_keeps_original_error_plus_note() {
        let result = install_with_flag_fallback(ID, |args| {
            if args.contains(&"-y") {
                fail("error: unknown option '-y'")
            } else {
                fail("Plugin not found in marketplace")
            }
        });
        assert!(!result.success);
        assert!(result.message.starts_with("Plugin not found in marketplace"), "{}", result.message);
        assert!(result.message.contains("`-y`"), "{}", result.message);
        assert!(result.message.contains("claude update"), "{}", result.message);
    }

    #[test]
    fn dropping_scope_note_warns_about_install_scope() {
        let result = install_with_flag_fallback(ID, |args| {
            if args.contains(&"-y") {
                fail("error: unknown option '-y'")
            } else if args.contains(&"-s") {
                fail("error: unknown option '-s'")
            } else {
                ok()
            }
        });
        assert!(result.success);
        assert!(result.message.contains("`-y`, `-s user`"), "{}", result.message);
        assert!(result.message.contains("설치 범위"), "{}", result.message);
        assert!(result.message.contains("claude update"), "{}", result.message);
    }

    #[test]
    fn no_fallback_failure_message_is_unchanged() {
        let result = install_with_flag_fallback(ID, |_| fail("Plugin not found"));
        assert_eq!(result.message, "Plugin not found");
    }

    #[test]
    fn persistent_rejection_terminates_after_bounded_attempts() {
        // CLI가 어떤 인자에도 같은 거부를 내는 병적 상황 — 무한 루프 없이 끝나야 한다.
        let mut calls = 0;
        let result = install_with_flag_fallback(ID, |_| {
            calls += 1;
            fail("error: unknown option '-y'")
        });
        assert!(!result.success);
        assert_eq!(calls, 2); // ALL → (-y 제거) → 같은 -y 거부는 재시도 대상 아님
    }
}
