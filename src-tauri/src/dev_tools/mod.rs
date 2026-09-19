// ---------------- 개발 환경 실설치/업데이트 ----------------
// 정본: 프로젝트 루트 scratch-design-devtools-update.md (결정 1~6 + 부록 A/B/C).
// 이 파일은 그 설계를 그대로 구현한다. 핵심 안전장치:
//   - 프론트에서 오는 값은 toolId(+planId 해시)뿐이고, argv에 들어가는 동적 값은
//     전부 "이 파일이 파일시스템에서 실제로 읽은 canonical 경로"에서 파싱된 뒤
//     validate_argv_token()을 통과해야 한다(부록 C의 "임의 문자열 유입 경로 = 0").
//   - 셸을 거치지 않는다(std::process::Command 직접 사용, capabilities 변경 없음).
//   - 실행은 stdin(Stdio::null()) + 프로세스 그룹 kill + 파이프 리더 스레드 +
//     try_wait() 폴링 타임아웃으로 감싼다(결정 6).
//   - 성공 판정은 exit code(주장)가 아니라 실행 전후 버전 재조회 델타(확인)로 한다
//     (결정 4, claimed ≠ verified).
//
// 모듈 구조(`autonomy/`와 동일한 패턴): 이 파일(mod.rs)은 진입점이다 — 도구
// 화이트리스트(어떤 tauri::command도 아닌 순수 데이터/조회 함수)와 5개
// `#[tauri::command]` 진입점만 여기 둔다. 커맨드 함수는 의도적으로 서브모듈로
// 옮기지 않는다: `#[tauri::command]` 매크로가 어노테이션된 함수와 같은 모듈
// 경로에 숨은 등록용 아이템을 생성하므로, 함수만 다른 모듈로 옮기고 `pub use`로
// 재노출하면 `lib.rs`의 `tauri::generate_handler![dev_tools::check_dev_tools, ...]`가
// 그 숨은 아이템을 찾지 못해 컴파일이 깨진다 — `autonomy/mod.rs`가 커맨드를 항상
// mod.rs에 직접 두는 이유와 같다. 무거운 로직(perform_preview/perform_update/
// perform_install/perform_open_manual_instruction/check_dev_tools_blocking)은
// `#[tauri::command]`가 아닌 평범한 함수라 안전하게 서브모듈로 옮겼다.
mod actions;
mod classify;
mod contract;
mod diagnostics;
mod install_resolver;
mod plan_table;
// `pub(crate)`인 이유: cli_launcher.rs(크레이트 루트, dev_tools 밖)가 이 모듈의
// Platform/EnvRoots/quote_token 등을 터미널 열기(§E) + resolve_binary(§D.3)
// 구현에 재사용한다 — dev_tools가 cli_launcher를 쓰고 cli_launcher가
// dev_tools::platform을 쓰는 양방향 참조이지만, Rust는 순환 모듈 참조 자체를
// 금지하지 않는다(타입 순환이 아니라 아이템 조회일 뿐).
pub(crate) mod platform;
mod process;
mod query;
mod runners;
// R-8(review-devtools-windows-parity-2026-09-15-r3.md): classify_tests.rs/
// plan_table_tests.rs가 공유하는 "DEV_TOOLS × windows_path_candidates" 순회
// 헬퍼 — 테스트 전용이라 #[cfg(test)]로만 컴파일된다.
#[cfg(test)]
pub(crate) mod win_candidate_test_support;

use crate::cli_launcher::resolve_binary_expand_home;
use platform::Platform;

// 이 타입들은 dev_tools 밖에서 이름으로 참조되지 않는다(외부는 `#[tauri::command]`
// 경로만 쓴다) — 아래 커맨드 함수 시그니처에만 필요하므로 `pub use`로 재노출하지
// 않고 이 파일 안에서만 쓰는 `use`로 최소화한다.
use contract::{DevToolActionResult, DevToolPreview, DevToolStatus};
pub(crate) use process::{build_child_path_env, run_process_with_timeout_cancellable};

// ==================== 1. 도구 화이트리스트 ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolId {
    Claude,
    Node,
    Gh,
    Git,
    Pnpm,
    Wrangler,
}

#[derive(Clone, Copy)]
pub(crate) struct DevTool {
    pub(crate) id: ToolId,
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) version_args: &'static [&'static str],
    /// macOS 후보 — 무변경(기존 그대로).
    pub(crate) path_candidates: &'static [&'static str],
    /// Windows 후보(설계 §B.5, 전부 미검증 — 실기 Windows PC 필요). `%APPDATA%`
    /// 등 토큰은 `resolve_tool_path`가 `resolve_binary_expand_home`을 거쳐
    /// `platform::expand_path_tokens`로 확장한다.
    pub(crate) windows_path_candidates: &'static [&'static str],
    /// "필수" 표시(devTools.ts UI)의 단일 정본. 사용자 결정(hub decisionId
    /// 01m2wse823xcszvn7km3vap0qs): 직원 대다수가 개발환경 설정에 익숙하지 않아
    /// Node.js/Git을 "있으면 편한 선택"이 아니라 앱/업무가 돌아가는 데 필요한
    /// 전제조건으로 본다. 이 필드를 프런트 상수로 따로 두지 않고 여기 두는
    /// 이유: label/path_candidates 등 도구 메타데이터가 이미 이 테이블
    /// 하나에 모여 있고(단일 정본), "필수 여부"도 도구 자체에 귀속되는 같은
    /// 성격의 정적 사실이라 프런트가 별도 id 목록을 유지하면 ToolId가 바뀔 때
    /// 드리프트할 위험이 생긴다. Claude Code는 이번 결정(Node/Git 자동설치
    /// 전환)의 대상이 아니라서 포함하지 않았다 — 필요하면 별도 판단으로
    /// 추가한다.
    pub(crate) required: bool,
}

// docker는 완전 삭제(부록 C 프론트 변경 메모, 7→6).
pub(crate) static DEV_TOOLS: [DevTool; 6] = [
    DevTool {
        id: ToolId::Claude,
        key: "claude",
        label: "Claude Code",
        version_args: &["--version"],
        // 실측: /opt/homebrew/bin/claude → ~/.local/bin/claude (심볼릭 링크 대상).
        // 뒤 2개는 설계 §7 신규 요구 — npm 커스텀 prefix(~/.npm-global)나 레거시
        // 로컬 설치(~/.claude/local)를 앱이 못 보고 npm install -g로 두 번째
        // 사본을 얹는 것(G4 완화)을 막는다.
        path_candidates: &[
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "~/.local/bin/claude",
            "~/.npm-global/bin/claude",
            "~/.claude/local/claude",
        ],
        // 설계 §B.5(미검증): 네이티브 설치기(~/.local/bin과 동형), npm 전역
        // shim, winget 앱 실행 별칭 순.
        windows_path_candidates: &[
            r"%USERPROFILE%\.local\bin\claude.exe",
            r"%APPDATA%\npm\claude.cmd",
            r"%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe",
        ],
        required: false,
    },
    DevTool {
        id: ToolId::Node,
        key: "node",
        label: "Node.js",
        version_args: &["--version"],
        path_candidates: &["/opt/homebrew/bin/node", "/usr/local/bin/node"],
        // 설계 §B.5(미검증): nvm-windows 등 버전매니저 관리본은 앱이 못 보는 게
        // 의도된 동작이다(G4 — devtools-install-matrix §3.3과 동일 이유).
        windows_path_candidates: &[
            r"C:\Program Files\nodejs\node.exe",
            r"%LOCALAPPDATA%\Programs\nodejs\node.exe",
        ],
        required: true,
    },
    DevTool {
        id: ToolId::Gh,
        key: "gh",
        label: "GitHub CLI",
        version_args: &["--version"],
        // github_integration.rs의 GH_CANDIDATES와 값은 같지만, 결정 5.2에 따라
        // 상수를 공유하지 않고 도구 정의 테이블에 별도로 둔다(회귀 위험 0).
        path_candidates: &["/opt/homebrew/bin/gh", "/usr/local/bin/gh", "/usr/bin/gh"],
        // 설계 §B.5(미검증): winget 머신 스코프 설치본 + winget 앱 실행 별칭.
        windows_path_candidates: &[
            r"C:\Program Files\GitHub CLI\gh.exe",
            r"%LOCALAPPDATA%\Microsoft\WinGet\Links\gh.exe",
        ],
        required: false,
    },
    DevTool {
        id: ToolId::Git,
        key: "git",
        label: "Git",
        version_args: &["--version"],
        // 설계 §4.3 처방 1: 실제 바이너리 후보를 스텁보다 먼저 둔다. macOS의
        // `/usr/bin/git`은 CLT(Xcode Command Line Tools) 미설치 상태에서도
        // 파일로 존재하는 xcrun 스텁이라(실측: `xcode-select -p` 확인 없이도
        // 항상 Some을 반환) 정상 머신에서는 앞의 세 후보가 먼저 잡혀 스텁을
        // 아예 건드리지 않는다. 앞 셋은 canonical이 각각 Cellar/CLT 경로라
        // 기존 분류(HomebrewFormula / SystemManaged)가 그대로 동작한다.
        path_candidates: &[
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/Library/Developer/CommandLineTools/usr/bin/git",
            "/usr/bin/git",
        ],
        // 설계 §B.5(미검증): 64/32비트 설치 경로 + PortableGit(Program Files
        // 밖) 순.
        windows_path_candidates: &[
            r"C:\Program Files\Git\cmd\git.exe",
            r"C:\Program Files (x86)\Git\cmd\git.exe",
            r"%LOCALAPPDATA%\Programs\Git\cmd\git.exe",
        ],
        required: true,
    },
    DevTool {
        id: ToolId::Pnpm,
        key: "pnpm",
        label: "pnpm",
        version_args: &["--version"],
        path_candidates: &[
            "/opt/homebrew/bin/pnpm",
            "/usr/local/bin/pnpm",
            "~/Library/pnpm/pnpm",
            "~/.local/share/pnpm/pnpm",
        ],
        // 설계 §B.5(미검증): standalone 설치(winget/공식 스크립트) + npm 경유
        // 설치 순.
        windows_path_candidates: &[
            r"%LOCALAPPDATA%\pnpm\pnpm.exe",
            r"%APPDATA%\npm\pnpm.cmd",
        ],
        required: false,
    },
    DevTool {
        id: ToolId::Wrangler,
        key: "wrangler",
        label: "Wrangler",
        version_args: &["--version"],
        // pnpm 전역 설치(`pnpm add -g wrangler`) 시 바이너리가 brew 경로가 아니라
        // pnpm 전역 bin(=$PNPM_HOME/bin, 기본값 두 가지)에 놓인다 — Wrangler 전용
        // 실설치 기능(§12.5)이 설치 직후 재조회(check_tool_version)로 찾아낼 수
        // 있어야 한다. pnpm은 shim을 $PNPM_HOME 바로 밑이 아니라 $PNPM_HOME/bin
        // 아래에 만든다(실측: `which wrangler` → ~/Library/pnpm/bin/wrangler).
        path_candidates: &[
            "/opt/homebrew/bin/wrangler",
            "/usr/local/bin/wrangler",
            "~/Library/pnpm/bin/wrangler",
            "~/.local/share/pnpm/bin/wrangler",
        ],
        // 설계 §B.5(미검증): pnpm 전역 shim + npm 전역 shim 순(설치 후보 우선순위
        // — install_resolver::install_candidates의 Wrangler 행과 동일 순서).
        windows_path_candidates: &[
            r"%LOCALAPPDATA%\pnpm\wrangler.cmd",
            r"%APPDATA%\npm\wrangler.cmd",
        ],
        required: false,
    },
];

impl ToolId {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        DEV_TOOLS.iter().find(|d| d.key == key).map(|d| d.id)
    }
}

pub(crate) fn tool_definition(id: ToolId) -> &'static DevTool {
    DEV_TOOLS
        .iter()
        .find(|d| d.id == id)
        .expect("DEV_TOOLS 테이블에 모든 ToolId가 있어야 합니다")
}

/// 설계 §D.3/§B.5: 이 도구가 실제로 있을 만한 자리(플랫폼별 후보)를 고른다.
/// macOS 분기는 기존 `def.path_candidates`를 그대로 넘겨(무변경) 기존 탐지
/// 동작을 한 글자도 바꾸지 않는다. Windows 분기는 §B.5 후보(전부 미검증)를
/// 넘긴다 — 실제 확장(`%APPDATA%` 등)과 최종 판정은
/// `resolve_binary_expand_home`(cli_launcher.rs)이 플랫폼을 다시 읽어 수행한다.
pub(crate) fn tool_path_candidates(def: &DevTool) -> &'static [&'static str] {
    match platform::platform_now() {
        Platform::Mac => def.path_candidates,
        Platform::Win => def.windows_path_candidates,
    }
}

pub(crate) fn resolve_tool_path(def: &DevTool) -> Option<String> {
    resolve_binary_expand_home(tool_path_candidates(def), def.key)
}

// ==================== #[tauri::command] 진입점 ====================

/// 이 커맨드가 예전엔 sync였다(P0 버그: bare `Command::new` + 메인 스레드 실행).
/// 결정 6의 일반 원칙(Tauri v2에서 non-async 커맨드는 메인 스레드에서 돈다)을
/// 여기에도 적용해 async + spawn_blocking으로 옮긴다 — 프론트 `invoke()` 계약은
/// 어차피 항상 Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn check_dev_tools() -> Vec<DevToolStatus> {
    tauri::async_runtime::spawn_blocking(query::check_dev_tools_blocking)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn preview_dev_tool_update(tool_id: String) -> Result<DevToolPreview, String> {
    tauri::async_runtime::spawn_blocking(move || query::perform_preview(&tool_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

#[tauri::command]
pub async fn update_dev_tool(
    tool_id: String,
    plan_id: String,
) -> Result<DevToolActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || actions::perform_update(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

#[tauri::command]
pub async fn install_dev_tool(
    tool_id: String,
    plan_id: String,
) -> Result<DevToolActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || actions::perform_install(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// M4(review-devtools-install-2026-09-10.md): 이 커맨드가 예전엔 non-async
/// `pub fn`이었다 — 이번 변경(ResolvedRunners::resolve() 호출 추가)으로
/// 타임아웃 없는 하위 프로세스 spawn 경로(bare-name PATH 폴백,
/// cli_launcher.rs:27)를 새로 얻었는데, non-async 커맨드는 Tauri v2에서
/// 메인 스레드에서 돈다 — 이 파일이 check_dev_tools를 async + spawn_blocking으로
/// 옮기며 스스로 "P0 버그"로 규정한 바로 그 패턴이다. 같은 방식으로 옮긴다 —
/// 프론트 `invoke()`는 어차피 항상 Promise라 계약이 바뀌지 않는다.
#[tauri::command]
pub async fn open_manual_instruction(
    tool_id: String,
    execute: bool,
) -> Result<crate::cli_launcher::TerminalLaunchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        actions::perform_open_manual_instruction(&tool_id, execute)
    })
    .await
    .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtool_table_has_exactly_six_entries_without_docker() {
        assert_eq!(DEV_TOOLS.len(), 6);
        assert!(DEV_TOOLS.iter().all(|d| d.key != "docker"));
    }

    // 골든 테스트(설계 §C.3.2): macOS path_candidates 배열은 한 글자도 바뀌면
    // 안 된다 — 전체 배열 내용을 고정해 부분 문자열 매치(starts_with/contains)로
    // 우연히 통과하는 것을 막는다.
    #[test]
    fn macos_path_candidates_are_frozen_exactly() {
        let claude = tool_definition(ToolId::Claude);
        assert_eq!(
            claude.path_candidates,
            &[
                "/opt/homebrew/bin/claude",
                "/usr/local/bin/claude",
                "~/.local/bin/claude",
                "~/.npm-global/bin/claude",
                "~/.claude/local/claude",
            ]
        );
        let node = tool_definition(ToolId::Node);
        assert_eq!(
            node.path_candidates,
            &["/opt/homebrew/bin/node", "/usr/local/bin/node"]
        );
    }

    // 설계 §B.5: 6개 도구 전부 Windows 후보가 최소 1개는 있어야 한다(빈 배열은
    // 탐지가 구조적으로 항상 실패한다는 뜻이라, Phase 1의 "정확한 탐지" 목표에
    // 어긋난다).
    #[test]
    fn every_tool_has_at_least_one_windows_path_candidate() {
        for def in DEV_TOOLS.iter() {
            assert!(
                !def.windows_path_candidates.is_empty(),
                "{}은(는) windows_path_candidates가 비어 있습니다",
                def.key
            );
        }
    }

    // resolve_tool_path의 플랫폼 선택 로직 자체를 이 머신(Mac)에서 직접
    // 검증한다 — tool_path_candidates가 platform_now()에 따라 서로 다른 배열을
    // 돌려준다는 계약.
    // 플랫폼 전제: 이 머신이 Mac이라는 전제 — CI의 windows-latest에서는 컴파일을
    // 건너뛴다.
    #[cfg(target_os = "macos")]
    #[test]
    fn tool_path_candidates_selects_mac_array_on_this_machine() {
        for def in DEV_TOOLS.iter() {
            assert_eq!(tool_path_candidates(def), def.path_candidates);
        }
    }

    // hub decisionId 01m2wse823xcszvn7km3vap0qs: "필수" 정의의 단일 정본이 이
    // 테이블이라는 계약 자체를 고정한다 — Node/Git만 true, 나머지는 false.
    #[test]
    fn only_node_and_git_are_marked_required() {
        for def in DEV_TOOLS.iter() {
            let expected = matches!(def.id, ToolId::Node | ToolId::Git);
            assert_eq!(
                def.required, expected,
                "{}의 required 플래그가 기대값과 다릅니다",
                def.key
            );
        }
    }
}
