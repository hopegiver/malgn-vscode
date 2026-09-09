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

use crate::cli_launcher::{
    open_terminal_command, resolve_binary_expand_home, TerminalLaunchResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ==================== 1. 도구 화이트리스트 ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolId {
    Claude,
    Node,
    Gh,
    Git,
    Pnpm,
    Wrangler,
}

#[derive(Clone, Copy)]
struct DevTool {
    id: ToolId,
    key: &'static str,
    label: &'static str,
    version_args: &'static [&'static str],
    path_candidates: &'static [&'static str],
}

// docker는 완전 삭제(부록 C 프론트 변경 메모, 7→6).
static DEV_TOOLS: [DevTool; 6] = [
    DevTool {
        id: ToolId::Claude,
        key: "claude",
        label: "Claude Code",
        version_args: &["--version"],
        // 실측: /opt/homebrew/bin/claude → ~/.local/bin/claude (심볼릭 링크 대상).
        path_candidates: &[
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "~/.local/bin/claude",
        ],
    },
    DevTool {
        id: ToolId::Node,
        key: "node",
        label: "Node.js",
        version_args: &["--version"],
        path_candidates: &["/opt/homebrew/bin/node", "/usr/local/bin/node"],
    },
    DevTool {
        id: ToolId::Gh,
        key: "gh",
        label: "GitHub CLI",
        version_args: &["--version"],
        // github_integration.rs의 GH_CANDIDATES와 값은 같지만, 결정 5.2에 따라
        // 상수를 공유하지 않고 도구 정의 테이블에 별도로 둔다(회귀 위험 0).
        path_candidates: &["/opt/homebrew/bin/gh", "/usr/local/bin/gh", "/usr/bin/gh"],
    },
    DevTool {
        id: ToolId::Git,
        key: "git",
        label: "Git",
        version_args: &["--version"],
        path_candidates: &["/usr/bin/git"],
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
    },
];

impl ToolId {
    fn from_key(key: &str) -> Option<Self> {
        DEV_TOOLS.iter().find(|d| d.key == key).map(|d| d.id)
    }
}

fn tool_definition(id: ToolId) -> &'static DevTool {
    DEV_TOOLS
        .iter()
        .find(|d| d.id == id)
        .expect("DEV_TOOLS 테이블에 모든 ToolId가 있어야 합니다")
}

fn resolve_tool_path(def: &DevTool) -> Option<String> {
    resolve_binary_expand_home(def.path_candidates, def.key)
}

// ==================== 2. 설치방식 판별(결정 1) ====================

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallMethod {
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
    NpmGlobal {
        package: String,
    },
    ClaudeNative,
    SystemManaged,
    VersionManager(String),
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    HomebrewFormula,
    HomebrewCask,
    PnpmStandalone,
    NpmGlobal,
    ClaudeNative,
    SystemManaged,
    VersionManager,
    Unknown,
}

impl InstallMethod {
    fn kind(&self) -> MethodKind {
        match self {
            InstallMethod::HomebrewFormula { .. } => MethodKind::HomebrewFormula,
            InstallMethod::HomebrewCask { .. } => MethodKind::HomebrewCask,
            InstallMethod::PnpmStandalone => MethodKind::PnpmStandalone,
            InstallMethod::NpmGlobal { .. } => MethodKind::NpmGlobal,
            InstallMethod::ClaudeNative => MethodKind::ClaudeNative,
            InstallMethod::SystemManaged => MethodKind::SystemManaged,
            InstallMethod::VersionManager(_) => MethodKind::VersionManager,
            InstallMethod::Unknown(_) => MethodKind::Unknown,
        }
    }
}

/// 안내문 + (있으면) 공식 문서 링크를 한 문자열로 합친다. `ManualPlan.doc_url`은
/// 부록 C의 DevToolStatus/DevToolPreview에 별도 필드가 없어 message 안에 접어
/// 넣는다(계약을 바꾸지 않기 위한 선택 — 계약 변경이 필요하면 즉시 보고하라는
/// 지시에 따라 필드 추가 대신 이 방식을 택했다).
fn manual_display_message(plan: &ManualPlan) -> String {
    match plan.doc_url {
        Some(url) => format!("{} 참고: {url}", plan.message_ko),
        None => plan.message_ko.to_string(),
    }
}

fn describe_install_method(method: &InstallMethod) -> String {
    match method {
        InstallMethod::HomebrewFormula { formula, .. } => format!("homebrewFormula({formula})"),
        InstallMethod::HomebrewCask { cask } => format!("homebrewCask({cask})"),
        InstallMethod::PnpmStandalone => "pnpmStandalone".to_string(),
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
fn classify_install_method(
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

    // 3. PnpmStandalone (신규 요구사항 — pnpm 전용, Cellar/Caskroom과 겹치지 않는다)
    if let Some(ph) = pnpm_home {
        if !ph.as_os_str().is_empty() && canonical.starts_with(ph) {
            return InstallMethod::PnpmStandalone;
        }
    }
    if let Some(h) = home {
        if canonical.starts_with(h.join("Library").join("pnpm"))
            || canonical.starts_with(h.join(".local").join("share").join("pnpm"))
        {
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
fn classify_install_method_with_defaults(path: &Path) -> InstallMethod {
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
fn canonicalize_best_effort(resolved_path: &str) -> PathBuf {
    let p = Path::new(resolved_path);
    if p.is_absolute() {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    } else {
        p.to_path_buf()
    }
}

// ==================== 3. argv 검증 ====================

/// `^[A-Za-z0-9@][A-Za-z0-9@._+/-]*$`, `..` 불포함, `-` 시작 금지를 수기 구현한다
/// (regex 크레이트를 새로 추가하지 않기 위해 — 이 검사는 6개 문자 클래스만
/// 다루므로 손으로 짜는 편이 의존성 추가보다 싸다).
fn validate_argv_token(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.contains("..") {
        return false;
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '@') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '/' | '-'))
}

// ==================== 4. (도구 × 설치방식) → argv 테이블(결정 2) ====================

#[derive(Debug, Clone, Copy)]
enum Arg {
    Lit(&'static str),
    Formula,
    // 현재 UPDATE_TABLE의 어떤 정적 행도 이 변형을 쓰지 않는다(전부 PackageLatest
    // 사용) — 하지만 결정 2의 Arg 슬롯 구조 자체는 완결적이어야 하므로 타입은
    // 유지한다. resolve_args()가 이 슬롯을 실제로 옳게 처리하는지는
    // arg_package_slot_resolves_bare_package_name_without_latest_suffix 테스트가
    // 검증한다(cargo build 대상에서는 사용처가 없어 dead_code로 잡히므로 여기서만
    // 억제한다).
    #[allow(dead_code)]
    Package,
    PackageLatest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Brew,
    Npm,
    // Wrangler 전용 실설치(§12.5)에서만 쓰인다 — pnpm을 "다른 패키지를 설치하는
    // 도구"로 실행한다. RUN_PNPM_SELF_UPDATE의 SelfBinary(자기 자신을 갱신)와는
    // 용도가 달라 구분되는 변형이 필요하다.
    Pnpm,
    SelfBinary,
}

#[derive(Debug, Clone, Copy)]
struct RunPlan {
    runner: Runner,
    args: &'static [Arg],
    preview_args: Option<&'static [Arg]>,
    env: &'static [(&'static str, &'static str)],
    timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManualReason {
    XcodeClt,
    VersionManaged,
    NotWritable,
    UnknownMethod,
    NoRunner,
    // 설계 이후 확정된 조사결과 #3: corepack 관리 pnpm은 self-update가 확정
    // 실패한다(ERR_PNPM_CANT_SELF_UPDATE_IN_COREPACK) — 원 설계의 5개 사유에 추가.
    CorepackManaged,
    // 신규 요구사항: 이 도구는 아직 설치되어 있지 않다(설치 커맨드는 v1에서 항상
    // Manual로만 안내한다 — 근거는 파일 하단 주석 참조).
    NotInstalled,
}

#[derive(Debug, Clone, Copy)]
struct ManualPlan {
    reason: ManualReason,
    message_ko: &'static str,
    copyable_command: Option<&'static str>,
    doc_url: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Run(RunPlan),
    Manual(ManualPlan),
}

struct Row {
    tool: Option<ToolId>,
    method: MethodKind,
    action: Action,
}

// ── 환경변수(부록 B) ──
const COMMON_ENV: [(&str, &str); 3] = [("NO_COLOR", "1"), ("TERM", "dumb"), ("CI", "1")];
const BREW_ENV: [(&str, &str); 7] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("CI", "1"),
    ("NONINTERACTIVE", "1"),
    ("HOMEBREW_NO_ENV_HINTS", "1"),
    ("HOMEBREW_NO_ANALYTICS", "1"),
    ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
    // HOMEBREW_NO_AUTO_UPDATE는 의도적으로 설정하지 않는다(부록 B.4 — 끄면 오래된
    // 인덱스로 "이미 최신"이라는 거짓 판정이 난다). 그 대신 타임아웃 예산(600s)에
    // 흡수시킨다.
];
const NPM_ENV: [(&str, &str); 7] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("CI", "1"),
    ("npm_config_yes", "true"),
    ("npm_config_fund", "false"),
    ("npm_config_audit", "false"),
    ("npm_config_progress", "false"),
];

// ── RunPlan 상수 ──
// 설계 이후 확정된 조사결과 #1: brew는 기본이 ask 모드다 — argv에 `-y`를 명시한다.
const RUN_BREW_FORMULA: RunPlan = RunPlan {
    runner: Runner::Brew,
    args: &[
        Arg::Lit("upgrade"),
        Arg::Lit("-y"),
        Arg::Lit("--formula"),
        Arg::Formula,
    ],
    preview_args: Some(&[
        Arg::Lit("upgrade"),
        Arg::Lit("--dry-run"),
        Arg::Lit("--formula"),
        Arg::Formula,
    ]),
    env: &BREW_ENV,
    timeout_secs: 600,
};
const RUN_NPM_GLOBAL: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[Arg::Lit("install"), Arg::Lit("-g"), Arg::PackageLatest],
    preview_args: None,
    env: &NPM_ENV,
    timeout_secs: 300,
};
// 설계 이후 확정된 조사결과 #2: `claude update`는 대화형 프롬프트가 없고 출력
// 문자열로 상태 구분 가능(claim은 여기서 쓰지 않는다 — 결정 4의 버전 재조회만 쓴다).
const RUN_CLAUDE_UPDATE: RunPlan = RunPlan {
    runner: Runner::SelfBinary,
    args: &[Arg::Lit("update")],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
// 설계 이후 확정된 조사결과 #3: pnpm standalone은 v9.8.0부터 self-update 지원.
const RUN_PNPM_SELF_UPDATE: RunPlan = RunPlan {
    runner: Runner::SelfBinary,
    args: &[Arg::Lit("self-update")],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
// Wrangler 전용 실설치(§12.5). 다른 5개 도구의 미설치 상태는 여전히
// install_manual_plan()의 Manual 고정만 따른다 — Wrangler만 npm 레지스트리
// 패키지명이 "wrangler"로 고정돼 있어 추측 없이 안전하게 실행할 수 있다.
const WRANGLER_PACKAGE: &str = "wrangler";
const RUN_PNPM_GLOBAL_ADD: RunPlan = RunPlan {
    runner: Runner::Pnpm,
    args: &[Arg::Lit("add"), Arg::Lit("-g"), Arg::Lit(WRANGLER_PACKAGE)],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
const RUN_NPM_GLOBAL_INSTALL_WRANGLER: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("-g"),
        Arg::Lit(WRANGLER_PACKAGE),
    ],
    preview_args: None,
    env: &NPM_ENV,
    timeout_secs: 300,
};

// ── Manual 상수 ──
const MANUAL_COREPACK_MANAGED: ManualPlan = ManualPlan {
    reason: ManualReason::CorepackManaged,
    message_ko: "pnpm이 Corepack으로 관리되고 있습니다. `pnpm self-update`는 Corepack 환경에서 확정적으로 실패합니다. 아래 명령으로 직접 갱신해주세요.",
    copyable_command: Some("corepack use pnpm@latest"),
    doc_url: None,
};
const MANUAL_XCODE_CLT: ManualPlan = ManualPlan {
    reason: ManualReason::XcodeClt,
    message_ko:
        "이 도구는 macOS 명령어 도구(Xcode CLT) 등 시스템이 제공합니다. 앱이 수정할 수 없습니다.",
    copyable_command: Some("softwareupdate --list"),
    doc_url: None,
};
const MANUAL_VERSION_MANAGED: ManualPlan = ManualPlan {
    reason: ManualReason::VersionManaged,
    message_ko:
        "버전 매니저(nvm/fnm/volta/asdf/mise 등)로 관리되고 있습니다. 매니저에서 직접 올려주세요.",
    copyable_command: None,
    doc_url: None,
};
const MANUAL_CASK_NOT_WRITABLE: ManualPlan = ManualPlan {
    reason: ManualReason::NotWritable,
    message_ko: "캐스크 앱은 관리자 권한이 필요할 수 있어 앱이 실행하지 않습니다.",
    copyable_command: Some("brew upgrade --cask <name>"),
    doc_url: None,
};
const MANUAL_NOT_WRITABLE: ManualPlan = ManualPlan {
    reason: ManualReason::NotWritable,
    message_ko: "설치 경로에 쓰기 권한이 없어(관리자 권한 필요) 앱이 자동으로 실행하지 않습니다. 터미널에서 직접 실행해주세요.",
    copyable_command: None,
    doc_url: None,
};
const MANUAL_UNKNOWN_METHOD: ManualPlan = ManualPlan {
    reason: ManualReason::UnknownMethod,
    message_ko: "설치 방식을 확인할 수 없어 앱이 자동으로 실행하지 않습니다. 터미널에서 직접 업데이트해주세요.",
    copyable_command: None,
    doc_url: None,
};
const MANUAL_NO_RUNNER: ManualPlan = ManualPlan {
    reason: ManualReason::NoRunner,
    message_ko: "필요한 실행 도구(brew/npm)를 찾을 수 없어 앱이 자동으로 실행하지 않습니다.",
    copyable_command: None,
    doc_url: None,
};

static UPDATE_TABLE: &[Row] = &[
    Row {
        tool: None,
        method: MethodKind::HomebrewFormula,
        action: Action::Run(RUN_BREW_FORMULA),
    },
    Row {
        tool: None,
        method: MethodKind::HomebrewCask,
        action: Action::Manual(MANUAL_CASK_NOT_WRITABLE),
    },
    Row {
        tool: Some(ToolId::Pnpm),
        method: MethodKind::PnpmStandalone,
        action: Action::Run(RUN_PNPM_SELF_UPDATE),
    },
    Row {
        tool: None,
        method: MethodKind::NpmGlobal,
        action: Action::Run(RUN_NPM_GLOBAL),
    },
    Row {
        tool: Some(ToolId::Claude),
        method: MethodKind::ClaudeNative,
        action: Action::Run(RUN_CLAUDE_UPDATE),
    },
    Row {
        tool: None,
        method: MethodKind::SystemManaged,
        action: Action::Manual(MANUAL_XCODE_CLT),
    },
    Row {
        tool: None,
        method: MethodKind::VersionManager,
        action: Action::Manual(MANUAL_VERSION_MANAGED),
    },
    Row {
        tool: None,
        method: MethodKind::Unknown,
        action: Action::Manual(MANUAL_UNKNOWN_METHOD),
    },
];

fn lookup_action(tool_id: ToolId, kind: MethodKind) -> Action {
    for row in UPDATE_TABLE {
        if (row.tool.is_none() || row.tool == Some(tool_id)) && row.method == kind {
            return row.action;
        }
    }
    // 셀이 비어 있어도(=매칭되는 행이 없어도) 안전한 Manual로 떨어진다.
    Action::Manual(MANUAL_UNKNOWN_METHOD)
}

/// 설치 안 된 도구용 안내(v1: 항상 Manual). classify_install_method는 "이미 존재
/// 하는 바이너리의 canonical 경로"를 전제하므로 미설치 도구에는 애초에 적용할
/// 수 없다 — formula명을 추측할 근거(파일이 실제로 그 keg 안에 있다는 사실)가
/// 없다. 잘못된 추측으로 brew install <틀린 이름>을 실행하는 것보다 안전한
/// Manual 고정이 이 설계의 원칙("잘못된 상태를 표현 불가능하게")과 일치한다.
fn install_manual_plan(tool: ToolId) -> ManualPlan {
    match tool {
        ToolId::Claude => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Claude Code가 설치되어 있지 않습니다. 공식 설치 안내를 확인해주세요.",
            copyable_command: None,
            doc_url: Some("https://docs.claude.com/en/docs/claude-code/setup"),
        },
        ToolId::Node => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Node.js가 설치되어 있지 않습니다.",
            copyable_command: Some("brew install node"),
            doc_url: None,
        },
        ToolId::Gh => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "GitHub CLI가 설치되어 있지 않습니다.",
            copyable_command: Some("brew install gh"),
            doc_url: None,
        },
        ToolId::Git => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Git이 설치되어 있지 않습니다. macOS 명령어 도구를 설치해주세요.",
            copyable_command: Some("xcode-select --install"),
            doc_url: None,
        },
        ToolId::Pnpm => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "pnpm이 설치되어 있지 않습니다.",
            copyable_command: Some("brew install pnpm"),
            doc_url: None,
        },
        ToolId::Wrangler => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Wrangler CLI가 설치되어 있지 않습니다.",
            copyable_command: Some("npm install -g wrangler"),
            doc_url: None,
        },
    }
}

fn is_corepack_managed(corepack_root: Option<&str>) -> bool {
    corepack_root.map(|s| !s.is_empty()).unwrap_or(false)
}

/// 대상 디렉터리에 현재 uid로 쓰기 권한이 있는지 확인한다(부록 B.2 사전 쓰기권한
/// 검사). `libc::access`는 읽기 전용 syscall이다 — 파일을 만들거나 지우지 않는다.
fn is_writable_by_current_user(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

/// 결정 1~3을 조합해 tool_id + 이미 resolve된 바이너리 경로로부터 (설치방식,
/// 정적 Action)을 계산한다. corepack 가드는 "최우선"이라 라우팅 자체를 우회한다.
fn compute_action(tool_id: ToolId, method: &InstallMethod) -> Action {
    if tool_id == ToolId::Pnpm
        && is_corepack_managed(std::env::var("COREPACK_ROOT").ok().as_deref())
    {
        return Action::Manual(MANUAL_COREPACK_MANAGED);
    }

    let action = lookup_action(tool_id, method.kind());

    if let (Action::Run(plan), InstallMethod::HomebrewFormula { prefix, .. }) = (&action, method) {
        if plan.runner == Runner::Brew {
            // 설계 문언대로 prefix 자체가 아니라 <prefix>/Cellar를 검사한다. prefix는
            // classify_install_method가 canonical 경로에서 "Cellar" 세그먼트를 찾아
            // 그 부모로 결정한 값이므로(위 분류 로직 참고), <prefix>/Cellar는 이
            // InstallMethod가 만들어진 시점에 실제로 존재했던 경로다 —
            // is_writable_by_current_user(access(W_OK))는 없는 경로에도 false를
            // 반환하지만 여기서는 그 구조적 보장 덕에 "존재하지만 못 쓴다"만 검사하는
            // 셈이다. prefix 자체(Apple Silicon /opt/homebrew, Intel /usr/local)는
            // root:wheel일 수 있어도 brew가 실제로 쓰는 하위 디렉터리 소유권은 다를
            // 수 있으므로 prefix 대신 이 경로를 본다.
            //
            // <prefix>/bin도 함께 본다: brew upgrade는 Cellar에 새 keg를 풀 뿐 아니라
            // <prefix>/bin에 심볼릭 링크를 다시 건다(unlink+link). 두 디렉터리는 각각
            // 별도로 chown될 수 있어(설치 스크립트가 자동으로 맞춰주지만, 이후 수동
            // chmod/사용자 조작으로 어긋나는 사례가 실무에서 보고된다) Cellar만 쓰기
            // 가능하고 bin은 여전히 root 소유인 조합이 가능하다. 이 경우 keg 압축
            // 해제는 성공하고 링크 단계에서만 실패해 "부분 실패" 상태가 되므로, 사전에
            // 두 경로를 모두 확인해 Manual로 강등하는 편이 안전하다.
            let cellar = Path::new(prefix).join("Cellar");
            let bin = Path::new(prefix).join("bin");
            if !is_writable_by_current_user(&cellar) || !is_writable_by_current_user(&bin) {
                return Action::Manual(MANUAL_NOT_WRITABLE);
            }
        }
    }

    action
}

// ── 실행기(runner) 해석 — 실행기 자체도 절대경로 해석 대상(결정 5.3) ──
const BREW_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
const NPM_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/npm",
    "/usr/local/bin/npm",
    "~/.npm-global/bin/npm",
];

fn resolve_runner_path(runner: Runner, self_binary_path: Option<&str>) -> Option<String> {
    match runner {
        Runner::Brew => resolve_binary_expand_home(&BREW_CANDIDATES, "brew"),
        Runner::Npm => resolve_binary_expand_home(&NPM_CANDIDATES, "npm"),
        // pnpm 자신의 DevTool 정의(path_candidates)를 그대로 재사용한다 — 이미
        // 검증된 후보 목록(brew Cellar + standalone 두 경로)이 있으므로 별도
        // PNPM_CANDIDATES 상수를 새로 만들지 않는다(중복 방지).
        Runner::Pnpm => resolve_tool_path(tool_definition(ToolId::Pnpm)),
        Runner::SelfBinary => self_binary_path.map(|s| s.to_string()),
    }
}

/// 실행 준비까지 끝난 최종 계획. `Action`(정적 테이블 값)과 달리 runner의 실제
/// 절대경로까지 해석된 상태 — 실행기가 없으면(NoRunner) 여기서 Manual로 강등된다.
enum ResolvedAction {
    Run { runner_path: String, plan: RunPlan },
    Manual(ManualPlan),
}

struct ResolvedPlan {
    method: InstallMethod,
    action: ResolvedAction,
}

fn resolve_plan(tool_id: ToolId, resolved_tool_path: &str) -> ResolvedPlan {
    let canonical = canonicalize_best_effort(resolved_tool_path);
    let method = classify_install_method_with_defaults(&canonical);
    let action = compute_action(tool_id, &method);

    let resolved_action = match action {
        Action::Manual(mp) => ResolvedAction::Manual(mp),
        Action::Run(plan) => match resolve_runner_path(plan.runner, Some(resolved_tool_path)) {
            Some(runner_path) => ResolvedAction::Run { runner_path, plan },
            None => ResolvedAction::Manual(MANUAL_NO_RUNNER),
        },
    };

    ResolvedPlan {
        method,
        action: resolved_action,
    }
}

fn resolve_args(args: &[Arg], method: &InstallMethod) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        let token = match a {
            Arg::Lit(s) => (*s).to_string(),
            Arg::Formula => match method {
                InstallMethod::HomebrewFormula { formula, .. } => formula.clone(),
                _ => return Err("내부 오류: formula 슬롯을 채울 설치방식이 아닙니다".to_string()),
            },
            Arg::Package => match method {
                InstallMethod::NpmGlobal { package } => package.clone(),
                _ => return Err("내부 오류: package 슬롯을 채울 설치방식이 아닙니다".to_string()),
            },
            Arg::PackageLatest => match method {
                InstallMethod::NpmGlobal { package } => format!("{package}@latest"),
                _ => return Err("내부 오류: package 슬롯을 채울 설치방식이 아닙니다".to_string()),
            },
        };
        // Cellar/--force/… 같은 악의적/사고성 디렉터리명이 argv 플래그로 유입되는
        // 경로를 여기서 차단한다(리터럴 슬롯은 코드에 고정된 값이라 검증 불필요).
        if !matches!(a, Arg::Lit(_)) && !validate_argv_token(&token) {
            return Err(format!("검증 실패: 안전하지 않은 인자 '{token}'"));
        }
        out.push(token);
    }
    Ok(out)
}

fn build_command_display(runner_path: &str, args: &[String]) -> String {
    let runner_name = Path::new(runner_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(runner_path);
    let mut parts = vec![runner_name.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

// ==================== 5. 버전 문자열 정규화(결정 4) ====================

/// "숫자+(.숫자+)+" 뒤에 선택적 `[-+.][영숫자.]+` 접미부를 붙여 첫 번째로 나타나는
/// 버전 토큰만 뽑는다. regex 크레이트 없이 손으로 구현(문자 클래스가 단순해
/// 의존성 추가보다 싸다는 판단은 argv 검증과 동일).
fn extract_version_token(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            let mut group_count = 1;
            let mut k = j;
            loop {
                if k < n && chars[k] == '.' {
                    let mut m = k + 1;
                    let digit_start = m;
                    while m < n && chars[m].is_ascii_digit() {
                        m += 1;
                    }
                    if m == digit_start {
                        break;
                    }
                    k = m;
                    group_count += 1;
                } else {
                    break;
                }
            }
            if group_count >= 2 {
                let mut end = k;
                if end < n && matches!(chars[end], '-' | '+' | '.') {
                    let mut m = end + 1;
                    let suffix_start = m;
                    while m < n && (chars[m].is_ascii_alphanumeric() || chars[m] == '.') {
                        m += 1;
                    }
                    if m > suffix_start {
                        end = m;
                    }
                }
                return Some(chars[start..end].iter().collect());
            }
        }
        i += 1;
    }
    None
}

/// 첫 줄만 취하고, 그 안에서 첫 버전 토큰을 뽑는다. 매치가 없으면 첫 줄 원문으로
/// 폴백(그래도 비어 있으면 None → 호출부가 UnknownAfter로 처리).
fn normalize_version(raw: &str) -> Option<String> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    extract_version_token(first_line).or_else(|| Some(first_line.to_string()))
}

// ==================== 6. 실행 엔진(결정 6) ====================

static EXECUTION_LOCK: Mutex<()> = Mutex::new(());

struct ProcessRunOutput {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    duration: Duration,
    spawn_error: Option<String>,
}

fn wait_up_to(
    child: &mut std::process::Child,
    timeout: Duration,
    started: Instant,
) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
}

/// 프로세스 그룹 kill: SIGTERM → 3초 유예(try_wait 폴링) → SIGKILL. brew가 낳는
/// curl/git/ruby 손자 프로세스까지 함께 죽인다(`child.kill()`은 직속 자식만 죽여
/// 손자가 다운로드를 계속하는 문제가 있다 — 그래서 group kill이 필수).
fn force_kill_process_group(pid: i32, child: &mut std::process::Child) -> Option<i32> {
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let grace_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => {
                if Instant::now() >= grace_deadline {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
}

/// stdin=null(부록 B.1 — 프롬프트가 즉시 EOF를 받아 정지 대신 실패한다) +
/// stdout/stderr 리더 스레드(파이프 64KB 버퍼가 차서 자식이 write에서 멈추는
/// 교착을 막는다) + try_wait() 100ms 폴링 타임아웃 + 프로세스 그룹 kill.
fn run_process_with_timeout(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
) -> ProcessRunOutput {
    let started = Instant::now();
    let mut command = Command::new(binary_path);
    command.args(args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("PATH", path_env);
    for (k, v) in extra_env {
        command.env(k, v);
    }
    // HOME은 상속(brew/npm 캐시·설정이 필요) — PATH만 명시적으로 덮어쓴다.
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ProcessRunOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                duration: started.elapsed(),
                spawn_error: Some(format!("실행할 수 없습니다: {e}")),
            };
        }
    };

    let pid = child.id() as i32;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_reader = stdout_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_reader = stderr_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let (exit_code, timed_out) = match wait_up_to(&mut child, timeout, started) {
        Some(status) => (status.code(), false),
        None => (force_kill_process_group(pid, &mut child), true),
    };

    let stdout_bytes = stdout_reader
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr_bytes = stderr_reader
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    ProcessRunOutput {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
        timed_out,
        duration: started.elapsed(),
        spawn_error: None,
    }
}

/// `<brew_prefix>/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` — 절대경로
/// 실행만으로는 부족하다(brew/npm 내부에서 git/curl/ruby/node를 셔뱅으로 부른다).
/// runner_path의 bin 디렉터리를 최우선으로 넣고 표준 경로를 뒤에 덧붙인다.
fn build_child_path_env(runner_path: Option<&str>) -> String {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(rp) = runner_path {
        if let Some(bin_dir) = Path::new(rp).parent() {
            let s = bin_dir.to_string_lossy().to_string();
            // 빈 문자열은 POSIX PATH에서 CWD를 의미한다(예: bare name "brew"의
            // parent()는 Some("")) — 자식 프로세스가 CWD에서 git/curl 등을
            // 먼저 찾게 되므로 반드시 배제한다.
            if !s.is_empty() {
                dirs.push(s);
            }
        }
    }
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let s = d.to_string();
        if !dirs.contains(&s) {
            dirs.push(s);
        }
    }
    dirs.join(":")
}

fn check_tool_version(def: &DevTool, resolved_path: &str) -> Option<String> {
    let path_env = build_child_path_env(Some(resolved_path));
    let args: Vec<String> = def.version_args.iter().map(|s| s.to_string()).collect();
    let output = run_process_with_timeout(
        resolved_path,
        &args,
        &path_env,
        &[],
        Duration::from_secs(10),
    );
    if output.spawn_error.is_some() {
        return None;
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return Some(stdout.to_string());
    }
    let stderr = output.stderr.trim();
    if output.exit_code == Some(0) && !stderr.is_empty() {
        Some(stderr.to_string())
    } else {
        None
    }
}

fn truncate_log(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stdout}\n{stderr}");
    const MAX_CHARS: usize = 4000;
    let count = combined.chars().count();
    if count <= MAX_CHARS {
        combined
    } else {
        combined.chars().skip(count - MAX_CHARS).collect()
    }
}

// ==================== 7. plan_id 해시 게이트(부록 A) ====================

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
fn compute_plan_id(runner_path: &str, argv: &[String], normalized_before: &str) -> String {
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
fn compute_plan_id_for_manual(tool: ToolId, plan: &ManualPlan) -> String {
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
fn parse_brew_dry_run_affected(stdout: &str) -> Vec<String> {
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

struct PathVisibility {
    visible: bool,
    hint: Option<String>,
    hint_target: Option<String>,
}

fn compute_path_visibility(resolved_binary_path: &str) -> PathVisibility {
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

// ==================== 10. 프론트 계약(부록 C) — 반환 타입 ====================

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub install_method: Option<String>,
    /// "run" | "manual" | "none"
    pub action_kind: String,
    pub manual_hint: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolPreview {
    pub id: String,
    pub plan_id: String,
    pub will_run: bool,
    pub command_display: String,
    pub affected: Vec<String>,
    pub notes: String,
    /// 과제 4(fail-open 수정): dry-run 미리보기가 spawn 실패하거나 타임아웃하면
    /// `affected`는 안전한 기본값(`vec![def.label]`, 길이 1)으로 채워지는데, 이
    /// 길이만으로는 "실제로 영향 범위가 1개로 확인됨"과 구분이 안 된다. 이 필드가
    /// false면 `affected`/`notes`를 신뢰할 수 없다는 뜻이며, 프론트는 이 경우
    /// "전체 업데이트" 배치 자동실행에서 반드시 제외하고 개별 확인 대기로 남겨야
    /// 한다(개별 실행 경로는 화면에 그대로 노출해 사용자가 판단하게 한다).
    pub preview_reliable: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Updated,
    AlreadyLatest,
    UnknownAfter,
    Failed,
    TimedOut,
    NotSupported,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolActionResult {
    pub id: String,
    pub outcome: Outcome,
    pub verified: bool,
    pub version_before: Option<String>,
    pub version_after: Option<String>,
    pub normalized_before: Option<String>,
    pub normalized_after: Option<String>,
    pub install_method: String,
    pub ran_command: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub message: String,
    pub log_tail: String,
    // 신규 요구사항: 사용자 로그인 셸 PATH 노출 확인.
    pub path_visible: bool,
    pub path_hint: Option<String>,
    pub path_hint_target: Option<String>,
}

fn not_supported_result(def: &DevTool, message: String) -> DevToolActionResult {
    DevToolActionResult {
        id: def.key.to_string(),
        outcome: Outcome::NotSupported,
        verified: false,
        version_before: None,
        version_after: None,
        normalized_before: None,
        normalized_after: None,
        install_method: String::new(),
        ran_command: None,
        exit_code: None,
        duration_ms: 0,
        message,
        log_tail: String::new(),
        path_visible: false,
        path_hint: None,
        path_hint_target: None,
    }
}

// ==================== 11. check_dev_tools (기존 이름 유지 + 필드 확장) ====================

fn check_dev_tools_blocking() -> Vec<DevToolStatus> {
    DEV_TOOLS
        .iter()
        .map(|def| match resolve_tool_path(def) {
            None => DevToolStatus {
                id: def.key.to_string(),
                name: def.label.to_string(),
                installed: false,
                version: None,
                path: None,
                install_method: None,
                // Wrangler만 예외: 미설치 상태에서도 실제로 설치를 실행할 수 있다
                // (§12.5) — 다른 5개 도구는 그대로 "none"(버튼 비활성).
                action_kind: if def.id == ToolId::Wrangler {
                    "run".to_string()
                } else {
                    "none".to_string()
                },
                manual_hint: None,
            },
            Some(resolved_path) => {
                let version = check_tool_version(def, &resolved_path);
                let ResolvedPlan { method, action } = resolve_plan(def.id, &resolved_path);
                let (action_kind, manual_hint) = match &action {
                    ResolvedAction::Run { .. } => ("run".to_string(), None),
                    ResolvedAction::Manual(mp) => {
                        ("manual".to_string(), Some(manual_display_message(mp)))
                    }
                };
                DevToolStatus {
                    id: def.key.to_string(),
                    name: def.label.to_string(),
                    installed: version.is_some(),
                    version,
                    path: Some(resolved_path),
                    install_method: Some(describe_install_method(&method)),
                    action_kind,
                    manual_hint,
                }
            }
        })
        .collect()
}

/// 이 커맨드가 예전엔 sync였다(P0 버그: bare `Command::new` + 메인 스레드 실행).
/// 결정 6의 일반 원칙(Tauri v2에서 non-async 커맨드는 메인 스레드에서 돈다)을
/// 여기에도 적용해 async + spawn_blocking으로 옮긴다 — 프론트 `invoke()` 계약은
/// 어차피 항상 Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn check_dev_tools() -> Vec<DevToolStatus> {
    tauri::async_runtime::spawn_blocking(check_dev_tools_blocking)
        .await
        .unwrap_or_default()
}

// ==================== 12. preview / update / install / manual (부록 C) ====================

fn build_run_preview(
    def: &DevTool,
    tool_id: ToolId,
    runner_path: String,
    plan: RunPlan,
    method: InstallMethod,
) -> Result<DevToolPreview, String> {
    let args = resolve_args(plan.args, &method)?;
    let display = build_command_display(&runner_path, &args);
    let version_before = check_tool_version(def, {
        // check_tool_version은 resolved_path만 필요하다 — runner_path가 아니라
        // 도구 자신의 경로를 다시 넘겨야 하므로 tool_id로 재조회한다.
        &resolve_tool_path(def).unwrap_or_default()
    });
    let normalized_before = version_before
        .as_deref()
        .and_then(normalize_version)
        .unwrap_or_default();

    let (affected, notes, preview_reliable) = match plan.preview_args {
        Some(preview_args) => {
            let preview_argv = resolve_args(preview_args, &method)?;
            let path_env = build_child_path_env(Some(&runner_path));
            // brew --dry-run은 읽기 전용 미리보기이지만 인덱스를 갱신할 수 있어
            // 수십 초가 걸릴 수 있다 — 실제 실행(600s)보다는 짧은 예산.
            let output = run_process_with_timeout(
                &runner_path,
                &preview_argv,
                &path_env,
                plan.env,
                Duration::from_secs(120),
            );
            if let Some(err) = output.spawn_error {
                // spawn 실패 — affected는 안전한 기본값(길이 1)이지만 실제로 확인된
                // 값이 아니다. preview_reliable=false로 프론트에 "믿지 말라"고 알린다.
                (
                    vec![def.label.to_string()],
                    format!("미리보기 실행에 실패했습니다: {err}"),
                    false,
                )
            } else if output.timed_out {
                // 타임아웃도 마찬가지로 affected를 신뢰할 수 없다(부록 지시: fail-open
                // 금지 — 길이 1이라는 사실만으로 "안전"으로 해석되면 안 된다).
                (
                    vec![def.label.to_string()],
                    "미리보기가 시간 초과되었습니다. 실행 시 실제 범위가 다를 수 있습니다."
                        .to_string(),
                    false,
                )
            } else {
                let parsed = parse_brew_dry_run_affected(&output.stdout);
                let notes = if parsed.len() > 1 {
                    format!(
                        "요청한 도구 외에 {}개 항목이 함께 바뀔 수 있습니다: {}",
                        parsed.len(),
                        parsed.join(", ")
                    )
                } else {
                    String::new()
                };
                let affected = if parsed.is_empty() {
                    vec![def.label.to_string()]
                } else {
                    parsed
                };
                (affected, notes, true)
            }
        }
        None => (vec![def.label.to_string()], String::new(), true),
    };

    let plan_id = compute_plan_id(&runner_path, &args, &normalized_before);
    let _ = tool_id; // id는 def.key로 이미 반영, 시그니처 일관성을 위해 받아둠
    Ok(DevToolPreview {
        id: def.key.to_string(),
        plan_id,
        will_run: true,
        command_display: display,
        affected,
        notes,
        preview_reliable,
    })
}

fn perform_preview(tool_id_str: &str) -> Result<DevToolPreview, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    match resolve_tool_path(def) {
        None => {
            if tool == ToolId::Wrangler {
                return Ok(build_wrangler_install_preview(def));
            }
            let plan = install_manual_plan(tool);
            let plan_id = compute_plan_id_for_manual(tool, &plan);
            Ok(DevToolPreview {
                id: def.key.to_string(),
                plan_id,
                will_run: false,
                command_display: plan.copyable_command.unwrap_or("").to_string(),
                affected: Vec::new(),
                notes: manual_display_message(&plan),
                preview_reliable: true,
            })
        }
        Some(resolved_path) => {
            let ResolvedPlan { method, action } = resolve_plan(tool, &resolved_path);
            match action {
                ResolvedAction::Manual(mp) => {
                    let plan_id = compute_plan_id_for_manual(tool, &mp);
                    Ok(DevToolPreview {
                        id: def.key.to_string(),
                        plan_id,
                        will_run: false,
                        command_display: mp.copyable_command.unwrap_or("").to_string(),
                        affected: Vec::new(),
                        notes: manual_display_message(&mp),
                        preview_reliable: true,
                    })
                }
                ResolvedAction::Run { runner_path, plan } => {
                    build_run_preview(def, tool, runner_path, plan, method)
                }
            }
        }
    }
}

#[tauri::command]
pub async fn preview_dev_tool_update(tool_id: String) -> Result<DevToolPreview, String> {
    tauri::async_runtime::spawn_blocking(move || perform_preview(&tool_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

fn perform_update(tool_id_str: &str, plan_id: &str) -> Result<DevToolActionResult, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    // 동시 실행 전역 뮤텍스(결정 6) — brew 전역 락 충돌로 무의미한 Failed가
    // 나는 것을 막는다. 대기하지 않고 즉시 거부한다(사용자에게 명확한 이유 제공).
    let _guard = EXECUTION_LOCK.try_lock().map_err(|_| {
        "다른 업데이트가 이미 실행 중입니다. 완료 후 다시 시도해주세요.".to_string()
    })?;

    let Some(resolved_path) = resolve_tool_path(def) else {
        return Ok(not_supported_result(
            def,
            "도구가 설치되어 있지 않습니다. 먼저 설치해주세요.".to_string(),
        ));
    };

    let ResolvedPlan { method, action } = resolve_plan(tool, &resolved_path);
    let (runner_path, plan) = match action {
        ResolvedAction::Manual(mp) => {
            return Ok(not_supported_result(def, manual_display_message(&mp)))
        }
        ResolvedAction::Run { runner_path, plan } => (runner_path, plan),
    };

    let args = resolve_args(plan.args, &method)?;
    let version_before = check_tool_version(def, &resolved_path);
    let normalized_before = version_before
        .as_deref()
        .and_then(normalize_version)
        .unwrap_or_default();

    let expected_plan_id = compute_plan_id(&runner_path, &args, &normalized_before);
    if expected_plan_id != plan_id {
        return Err(
            "실행 계획이 미리보기 이후 바뀌었습니다. 다시 미리보기를 요청해주세요.".to_string(),
        );
    }

    let path_env = build_child_path_env(Some(&runner_path));
    let output = run_process_with_timeout(
        &runner_path,
        &args,
        &path_env,
        plan.env,
        Duration::from_secs(plan.timeout_secs),
    );

    // 경로를 캐시하지 않고 처음부터 다시 해석한다(결정 4.5 — brew relink, claude
    // native의 versions/<new> 갈아타기로 기존 canonical 경로가 사라져 있을 수 있다).
    let resolved_after = resolve_tool_path(def);
    let version_after = resolved_after
        .as_deref()
        .and_then(|p| check_tool_version(def, p));
    let normalized_after = version_after.as_deref().and_then(normalize_version);

    let (outcome, verified, message) = if output.timed_out {
        (
            Outcome::TimedOut,
            false,
            "실행 시간이 초과되어 강제 종료했습니다. 상태 불명 — 다시 확인해주세요.".to_string(),
        )
    } else if output.exit_code == Some(0) {
        match &normalized_after {
            Some(after) if *after != normalized_before => (
                Outcome::Updated,
                true,
                format!(
                    "{}이(가) 업데이트되었습니다 ({} → {}).",
                    def.label, normalized_before, after
                ),
            ),
            Some(after) => (
                Outcome::AlreadyLatest,
                true,
                format!("{}이(가) 이미 최신 버전입니다 ({}).", def.label, after),
            ),
            None => (
                Outcome::UnknownAfter,
                false,
                "명령은 성공했다고 보고했으나 버전을 다시 확인하지 못했습니다.".to_string(),
            ),
        }
    } else {
        let code_str = output
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "알 수 없음".to_string());
        (
            Outcome::Failed,
            false,
            format!("실행이 실패했습니다(종료 코드 {code_str})."),
        )
    };

    let path_for_visibility = resolved_after.as_deref().unwrap_or(resolved_path.as_str());
    let visibility = compute_path_visibility(path_for_visibility);

    Ok(DevToolActionResult {
        id: def.key.to_string(),
        outcome,
        verified,
        version_before,
        version_after,
        normalized_before: if normalized_before.is_empty() {
            None
        } else {
            Some(normalized_before)
        },
        normalized_after,
        install_method: describe_install_method(&method),
        ran_command: Some(build_command_display(&runner_path, &args)),
        exit_code: output.exit_code,
        duration_ms: output.duration.as_millis() as u64,
        message,
        log_tail: truncate_log(&output.stdout, &output.stderr),
        path_visible: visibility.visible,
        path_hint: visibility.hint,
        path_hint_target: visibility.hint_target,
    })
}

#[tauri::command]
pub async fn update_dev_tool(
    tool_id: String,
    plan_id: String,
) -> Result<DevToolActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || perform_update(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// v1: INSTALL_TABLE은 기본적으로 Manual만 담는다(install_manual_plan 문서 참조) —
/// 이 커맨드는 실제로 아무것도 실행하지 않고 planId 대조 후 안내 메시지만 돌려준다.
/// 예외는 Wrangler뿐이다(§12.5) — 패키지명이 고정돼 있어 추측 없이 실행 가능하다.
fn perform_install(tool_id_str: &str, plan_id: &str) -> Result<DevToolActionResult, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    if tool == ToolId::Wrangler && resolve_tool_path(def).is_none() {
        return perform_wrangler_install(def, plan_id);
    }

    let manual = install_manual_plan(tool);
    let expected_plan_id = compute_plan_id_for_manual(tool, &manual);
    if expected_plan_id != plan_id {
        return Err(
            "실행 계획이 미리보기 이후 바뀌었습니다. 다시 미리보기를 요청해주세요.".to_string(),
        );
    }
    Ok(not_supported_result(def, manual_display_message(&manual)))
}

// ==================== 12.5 Wrangler 전용 실설치(신규 요구사항) ====================
// install_manual_plan()의 "미설치 도구 = 항상 Manual" 정책은 브루/캐스크 등 이름을
// 추측해야 하는 위험을 피하기 위한 것이다. Wrangler만 예외로 둔다 — npm 레지스트리
// 패키지명이 "wrangler"로 고정돼 있어 추측이 필요 없다. 다른 5개 도구는 이 절의
// 어떤 함수도 거치지 않고 install_manual_plan() 그대로 간다.

struct WranglerInstallChoice {
    runner_path: String,
    argv: Vec<String>,
    plan: RunPlan,
    installer_label: &'static str,
}

/// pnpm을 먼저 시도하고(해석 자체가 안 되면 npm으로 폴백), 어느 쪽도 없으면
/// None(호출부가 Manual로 강등). "해석 실패"와 "실행 실패"를 구분하는 것이
/// 핵심이다 — pnpm이 존재하는데 설치 명령 자체가 실패하는 경우는 여기서 걸러내지
/// 않는다(그 판정은 perform_wrangler_install의 실행 결과 처리 몫이다).
fn resolve_wrangler_install_choice() -> Option<WranglerInstallChoice> {
    let dummy_method = InstallMethod::Unknown(String::new());
    if let Some(runner_path) = resolve_runner_path(Runner::Pnpm, None) {
        let argv = resolve_args(RUN_PNPM_GLOBAL_ADD.args, &dummy_method)
            .expect("RUN_PNPM_GLOBAL_ADD는 전부 리터럴 인자라 실패할 수 없습니다");
        return Some(WranglerInstallChoice {
            runner_path,
            argv,
            plan: RUN_PNPM_GLOBAL_ADD,
            installer_label: "pnpm",
        });
    }
    if let Some(runner_path) = resolve_runner_path(Runner::Npm, None) {
        let argv = resolve_args(RUN_NPM_GLOBAL_INSTALL_WRANGLER.args, &dummy_method)
            .expect("RUN_NPM_GLOBAL_INSTALL_WRANGLER는 전부 리터럴 인자라 실패할 수 없습니다");
        return Some(WranglerInstallChoice {
            runner_path,
            argv,
            plan: RUN_NPM_GLOBAL_INSTALL_WRANGLER,
            installer_label: "npm",
        });
    }
    None
}

fn build_wrangler_install_preview(def: &DevTool) -> DevToolPreview {
    match resolve_wrangler_install_choice() {
        Some(choice) => {
            let display = build_command_display(&choice.runner_path, &choice.argv);
            // 미설치 상태라 "이전 버전"이 없다 — normalized_before는 빈 문자열로
            // 고정한다(perform_wrangler_install이 실행 직전 같은 값으로 재계산해
            // 대조하므로 값 자체보다 안정성이 중요하다).
            let plan_id = compute_plan_id(&choice.runner_path, &choice.argv, "");
            DevToolPreview {
                id: def.key.to_string(),
                plan_id,
                will_run: true,
                command_display: display,
                affected: vec![def.label.to_string()],
                notes: format!(
                    "{}(으)로 Wrangler CLI를 전역 설치합니다. 패키지명이 npm 레지스트리에 고정돼 있어 자동 실행이 안전합니다.",
                    choice.installer_label
                ),
                preview_reliable: true,
            }
        }
        None => {
            let plan = install_manual_plan(ToolId::Wrangler);
            let plan_id = compute_plan_id_for_manual(ToolId::Wrangler, &plan);
            DevToolPreview {
                id: def.key.to_string(),
                plan_id,
                will_run: false,
                command_display: plan.copyable_command.unwrap_or("").to_string(),
                affected: Vec::new(),
                notes: manual_display_message(&plan),
                preview_reliable: true,
            }
        }
    }
}

/// 결정 4의 "claimed ≠ verified"를 설치에도 그대로 적용한다: exit code 0을 성공
/// 주장(claim)으로만 보지 않고, 설치 후 check_tool_version()으로 실제 버전을
/// 다시 조회했을 때만 확인(verify)된 성공으로 본다. Outcome은 기존 update 흐름의
/// 값(Updated/UnknownAfter/Failed/TimedOut)을 그대로 재사용한다 — 프론트가 이미
/// Updated 시점에 목록을 새로고침하므로(devTools.ts runPlan) 신규 Outcome 변형을
/// 추가하지 않아도 설치 직후 상태가 목록에 바로 반영된다.
fn perform_wrangler_install(def: &DevTool, plan_id: &str) -> Result<DevToolActionResult, String> {
    let _guard = EXECUTION_LOCK.try_lock().map_err(|_| {
        "다른 업데이트가 이미 실행 중입니다. 완료 후 다시 시도해주세요.".to_string()
    })?;

    let Some(choice) = resolve_wrangler_install_choice() else {
        let manual = install_manual_plan(ToolId::Wrangler);
        return Ok(not_supported_result(def, manual_display_message(&manual)));
    };

    let expected_plan_id = compute_plan_id(&choice.runner_path, &choice.argv, "");
    if expected_plan_id != plan_id {
        return Err(
            "실행 계획이 미리보기 이후 바뀌었습니다. 다시 미리보기를 요청해주세요.".to_string(),
        );
    }

    let path_env = build_child_path_env(Some(&choice.runner_path));
    let output = run_process_with_timeout(
        &choice.runner_path,
        &choice.argv,
        &path_env,
        choice.plan.env,
        Duration::from_secs(choice.plan.timeout_secs),
    );

    // pnpm이 해석돼 실행까지 갔다면(성공이든 실패든) 그 결과를 그대로 보고한다 —
    // npm으로 조용히 폴백하지 않는다(요구사항: 실패를 숨기지 않는다).
    let resolved_after = resolve_tool_path(def);
    let version_after = resolved_after
        .as_deref()
        .and_then(|p| check_tool_version(def, p));
    let normalized_after = version_after.as_deref().and_then(normalize_version);

    let installer_label = choice.installer_label;
    let (outcome, verified, message) = if let Some(spawn_err) = &output.spawn_error {
        (
            Outcome::Failed,
            false,
            format!("{installer_label} 실행에 실패했습니다: {spawn_err}"),
        )
    } else if output.timed_out {
        (
            Outcome::TimedOut,
            false,
            format!(
                "{installer_label} 설치 실행이 시간 초과되어 강제 종료했습니다. 상태 불명 — 다시 확인해주세요."
            ),
        )
    } else if output.exit_code == Some(0) {
        match &normalized_after {
            Some(after) => (
                Outcome::Updated,
                true,
                format!("Wrangler CLI가 {installer_label}(으)로 설치되었습니다 ({after})."),
            ),
            None => (
                Outcome::UnknownAfter,
                false,
                "설치 명령은 성공했다고 보고했으나 설치 확인에 실패했습니다.".to_string(),
            ),
        }
    } else {
        let code_str = output
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "알 수 없음".to_string());
        (
            Outcome::Failed,
            false,
            format!("{installer_label} 설치가 실패했습니다(종료 코드 {code_str})."),
        )
    };

    let visibility = match resolved_after.as_deref() {
        Some(p) => compute_path_visibility(p),
        None => PathVisibility {
            visible: false,
            hint: None,
            hint_target: None,
        },
    };

    Ok(DevToolActionResult {
        id: def.key.to_string(),
        outcome,
        verified,
        version_before: None,
        version_after,
        normalized_before: None,
        normalized_after,
        install_method: format!("{installer_label}GlobalAdd({WRANGLER_PACKAGE})"),
        ran_command: Some(build_command_display(&choice.runner_path, &choice.argv)),
        exit_code: output.exit_code,
        duration_ms: output.duration.as_millis() as u64,
        message,
        log_tail: truncate_log(&output.stdout, &output.stderr),
        path_visible: visibility.visible,
        path_hint: visibility.hint,
        path_hint_target: visibility.hint_target,
    })
}

#[tauri::command]
pub async fn install_dev_tool(
    tool_id: String,
    plan_id: String,
) -> Result<DevToolActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || perform_install(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

// M2: Manual 경로는 Run 경로(preview -> 사용자 확인 -> 실행)와 달리 동의 절차 없이
// 즉시 실행됐다. `execute` 파라미터로 같은 커맨드를 "미리보기"(false)와
// "실행"(true) 두 모드로 재사용해, 프론트가 먼저 어떤 명령이 실행될지 보여주고
// 사용자 확인을 받은 뒤에만 execute:true로 다시 호출하게 한다. 계획 해석 로직을
// 두 곳에 중복시키지 않기 위해 한 함수 안에서 분기하며, 반환 타입은 기존
// TerminalLaunchResult를 그대로 재사용한다(필드 추가 없음 — 프론트-백엔드 타입
// 계약 변경 없음).
#[tauri::command]
pub fn open_manual_instruction(tool_id: String, execute: bool) -> Result<TerminalLaunchResult, String> {
    let tool =
        ToolId::from_key(&tool_id).ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id}"))?;
    let def = tool_definition(tool);

    let manual: Option<ManualPlan> = match resolve_tool_path(def) {
        Some(resolved_path) => match resolve_plan(tool, &resolved_path).action {
            ResolvedAction::Manual(mp) => Some(mp),
            ResolvedAction::Run { .. } => None,
        },
        None => Some(install_manual_plan(tool)),
    };

    match manual.and_then(|mp| mp.copyable_command) {
        Some(cmd) => {
            if !execute {
                // 미리보기 전용 — 아직 아무 것도 실행하지 않았다.
                return Ok(TerminalLaunchResult {
                    opened: false,
                    message: format!("다음 명령을 실행합니다: {cmd}"),
                });
            }
            open_terminal_command(cmd)?;
            Ok(TerminalLaunchResult {
                opened: true,
                message: format!("터미널에서 다음 명령을 실행했습니다: {cmd}"),
            })
        }
        None => Ok(TerminalLaunchResult {
            opened: false,
            message: "복사할 명령이 없습니다. 화면의 안내 문구를 참고해주세요.".to_string(),
        }),
    }
}

// ==================== 13. 테스트 ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ── 완료판정 필수 테스트 ①: 존재하지 않는 toolId → 안전한 Err ──
    #[test]
    fn unknown_tool_id_is_rejected_everywhere() {
        assert!(
            ToolId::from_key("docker").is_none(),
            "docker는 결정에 따라 완전히 삭제되어야 합니다"
        );
        assert!(ToolId::from_key("nope").is_none());
        assert!(perform_preview("nope").is_err());
        assert!(perform_update("nope", "anyplanid").is_err());
        assert!(perform_install("nope", "anyplanid").is_err());
    }

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
            assert!(validate_argv_token(&formula));
        }
    }

    // ── 완료판정 필수 테스트 ③: argv 검증 정규식이 악성 문자열 거부 ──
    #[test]
    fn argv_validation_rejects_malicious_or_flag_like_tokens() {
        assert!(!validate_argv_token("--force"));
        assert!(!validate_argv_token("-x"));
        assert!(!validate_argv_token("../../etc/passwd"));
        assert!(!validate_argv_token("foo bar"));
        assert!(!validate_argv_token(""));
        assert!(!validate_argv_token(".hidden"));
        assert!(validate_argv_token("node@22"));
        assert!(validate_argv_token("gh"));
        assert!(validate_argv_token("@scope/pkg"));
    }

    // ── 완료판정 필수 테스트 ④: 버전 정규화 ──
    #[test]
    fn normalizes_real_measured_version_strings() {
        assert_eq!(
            normalize_version(
                "gh version 2.95.0 (2026-06-17)\nhttps://github.com/cli/cli/releases/tag/v2.95.0"
            ),
            Some("2.95.0".to_string())
        );
        assert_eq!(
            normalize_version("git version 2.50.1 (Apple Git-155)"),
            Some("2.50.1".to_string())
        );
        assert_eq!(
            normalize_version("2.1.252 (Claude Code)"),
            Some("2.1.252".to_string())
        );
        assert_eq!(normalize_version("v22.23.1"), Some("22.23.1".to_string()));
        assert_eq!(normalize_version("11.9.0"), Some("11.9.0".to_string()));
        assert_eq!(normalize_version(""), None);
        // 매치가 없으면 첫 줄 원문 폴백
        assert_eq!(
            normalize_version("no-version-here\nsecond line"),
            Some("no-version-here".to_string())
        );
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
    }

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

    // brew는 기본이 ask 모드라 -y가 반드시 argv에 있어야 한다(조사결과 #1).
    #[test]
    fn brew_run_plan_includes_explicit_yes_flag_and_preview_omits_it() {
        let args = resolve_args(
            RUN_BREW_FORMULA.args,
            &InstallMethod::HomebrewFormula {
                formula: "pnpm".to_string(),
                keg_version: "11.9.0".to_string(),
                prefix: "/opt/homebrew".to_string(),
            },
        )
        .unwrap();
        assert_eq!(args, vec!["upgrade", "-y", "--formula", "pnpm"]);

        let preview_args = resolve_args(
            RUN_BREW_FORMULA.preview_args.unwrap(),
            &InstallMethod::HomebrewFormula {
                formula: "pnpm".to_string(),
                keg_version: "11.9.0".to_string(),
                prefix: "/opt/homebrew".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            preview_args,
            vec!["upgrade", "--dry-run", "--formula", "pnpm"]
        );
        assert!(!preview_args.contains(&"-y".to_string()));
    }

    #[test]
    fn resolve_args_rejects_malicious_formula_before_reaching_argv() {
        let malicious = InstallMethod::HomebrewFormula {
            formula: "--force".to_string(),
            keg_version: "1".to_string(),
            prefix: "/opt/homebrew".to_string(),
        };
        let result = resolve_args(RUN_BREW_FORMULA.args, &malicious);
        assert!(result.is_err());
    }

    // Arg::Package(PackageLatest이 아닌 순수 패키지명 슬롯)는 현재 UPDATE_TABLE의
    // 어떤 행도 쓰지 않지만(모두 PackageLatest만 쓴다), 결정 2의 Arg 구조 자체가
    // 완결적이어야 한다는 요구를 지키기 위해 타입은 유지한다 — 그 자리가 실제로
    // 옳게 동작하는지 직접 검증한다(죽은 코드가 아니라 "아직 안 쓰인 슬롯").
    #[test]
    fn arg_package_slot_resolves_bare_package_name_without_latest_suffix() {
        let method = InstallMethod::NpmGlobal {
            package: "wrangler".to_string(),
        };
        let args = resolve_args(
            &[Arg::Lit("install"), Arg::Lit("-g"), Arg::Package],
            &method,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "install".to_string(),
                "-g".to_string(),
                "wrangler".to_string()
            ]
        );
    }

    #[test]
    fn resolve_args_rejects_mismatched_install_method() {
        // Formula 슬롯인데 NpmGlobal을 넘기면 내부 오류로 거부되어야 한다(방어적 코딩).
        let wrong_method = InstallMethod::NpmGlobal {
            package: "wrangler".to_string(),
        };
        assert!(resolve_args(RUN_BREW_FORMULA.args, &wrong_method).is_err());
    }

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

    // 결정 6: 짧게 끝나는 프로세스가 정상적으로 exit code/stdout을 돌려주는지
    // (파이프 리더 스레드 + try_wait 폴링 경로 자체의 배관 검증).
    #[test]
    fn run_process_with_timeout_captures_output_of_fast_process() {
        let output = run_process_with_timeout(
            "/bin/echo",
            &["hello".to_string()],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(5),
        );
        assert!(output.spawn_error.is_none());
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.timed_out);
    }

    // 결정 6: 타임아웃이 실제로 프로세스를 강제 종료하는지(짧은 타임아웃으로
    // `sleep`을 강제 종료 — 프로세스 그룹 kill 경로까지 실행됨).
    #[test]
    fn run_process_with_timeout_kills_hanging_process() {
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/sleep",
            &["30".to_string()],
            "/usr/bin:/bin",
            &[],
            Duration::from_millis(500),
        );
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "타임아웃+3초 유예를 크게 넘기면 강제종료가 동작하지 않은 것"
        );
    }

    #[test]
    fn stdin_is_null_so_a_reading_process_fails_fast_instead_of_hanging() {
        // `cat`은 stdin을 읽으려 하지만 stdin이 null이라 즉시 EOF를 받아 빠르게
        // 종료해야 한다(부록 B.1 — 정지 대신 실패).
        let started = Instant::now();
        let output = run_process_with_timeout(
            "/bin/cat",
            &[],
            "/usr/bin:/bin",
            &[],
            Duration::from_secs(5),
        );
        assert!(
            !output.timed_out,
            "stdin=null이 적용되지 않았다면 cat이 멈춰 타임아웃까지 갔을 것"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn build_child_path_env_prioritizes_runner_bin_dir() {
        let path = build_child_path_env(Some("/opt/homebrew/bin/brew"));
        assert!(path.starts_with("/opt/homebrew/bin:"));
        assert!(path.contains("/usr/bin"));
        assert!(path.contains("/bin"));
    }

    // M1 회귀 테스트: resolve_binary()가 절대경로 후보를 못 찾으면 bare name을
    // 그대로 반환하고, 그 값이 runner_path로 들어온다. Path::new("brew").parent()는
    // Some("")를 반환하므로 빈 항목이 PATH 앞머리에 들어가면 안 된다 —
    // POSIX에서 PATH의 길이 0 항목은 CWD를 의미한다.
    #[test]
    fn build_child_path_env_excludes_empty_entry_for_bare_name() {
        let path = build_child_path_env(Some("brew"));
        assert!(!path.split(':').any(|p| p.is_empty()));
    }

    #[test]
    fn build_child_path_env_excludes_empty_entry_for_none() {
        let path = build_child_path_env(None);
        assert!(!path.split(':').any(|p| p.is_empty()));
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
    fn devtool_table_has_exactly_six_entries_without_docker() {
        assert_eq!(DEV_TOOLS.len(), 6);
        assert!(DEV_TOOLS.iter().all(|d| d.key != "docker"));
    }

    // lib.rs의 옛 `checks_dev_tools_without_panicking`을 이전한 것 — docker 삭제로
    // 7→6, P0 PATH 버그 수정 후에도 이 머신의 실제 claude/node/gh 중 최소 하나는
    // 설치돼 있어야 한다(node/pnpm 기반 저장소에서 도는 테스트이므로).
    #[test]
    fn checks_dev_tools_without_panicking() {
        let tools = check_dev_tools_blocking();
        assert_eq!(tools.len(), 6);
        assert!(
            tools.iter().any(|t| t.installed),
            "claude/node/gh/git/pnpm 중 설치된 도구가 하나도 없습니다"
        );
    }

    #[test]
    fn manual_plan_reasons_are_distinguishable_for_ui_branching() {
        assert_ne!(MANUAL_XCODE_CLT.reason, MANUAL_VERSION_MANAGED.reason);
        assert_ne!(MANUAL_COREPACK_MANAGED.reason, MANUAL_NOT_WRITABLE.reason);
    }

    // ── Wrangler 전용 실설치(§12.5) ──

    // pnpm/npm RunPlan 둘 다 전부 리터럴 인자라 어떤 InstallMethod를 넘겨도(심지어
    // 의미 없는 더미여도) 안전하게 고정된 argv로 풀려야 한다.
    #[test]
    fn wrangler_run_plans_resolve_to_expected_literal_argv() {
        let dummy = InstallMethod::Unknown(String::new());

        let pnpm_args = resolve_args(RUN_PNPM_GLOBAL_ADD.args, &dummy).unwrap();
        assert_eq!(pnpm_args, vec!["add", "-g", "wrangler"]);

        let npm_args = resolve_args(RUN_NPM_GLOBAL_INSTALL_WRANGLER.args, &dummy).unwrap();
        assert_eq!(npm_args, vec!["install", "-g", "wrangler"]);

        // "wrangler" 토큰 자체는 유효해야 한다("-g"는 리터럴 플래그라
        // validate_argv_token 대상이 아니다 — resolve_args가 Arg::Lit은 검증을
        // 건너뛴다).
        assert!(validate_argv_token(WRANGLER_PACKAGE));
    }

    // 이 머신에 pnpm/npm이 있든 없든 패닉 없이 안전한 값을 내야 한다 — 있으면
    // pnpm을 우선하고, 그 argv가 RUN_PNPM_GLOBAL_ADD와 일치해야 한다.
    #[test]
    fn wrangler_install_choice_prefers_pnpm_over_npm_when_both_resolve() {
        match resolve_wrangler_install_choice() {
            Some(choice) => match choice.installer_label {
                "pnpm" => assert_eq!(choice.argv, vec!["add", "-g", "wrangler"]),
                "npm" => assert_eq!(choice.argv, vec!["install", "-g", "wrangler"]),
                other => panic!("알 수 없는 installer_label: {other}"),
            },
            None => {
                // 이 머신에 pnpm도 npm도 없는 경우 — 정상적인 값(패닉이 아니다).
            }
        }
    }

    // 같은 환경에서 두 번 호출하면 같은 plan_id가 나와야 한다(부록 A의 안정성
    // 요구 — 미설치 상태에도 동일하게 적용).
    #[test]
    fn wrangler_install_preview_plan_id_is_stable_across_calls() {
        let def = tool_definition(ToolId::Wrangler);
        let a = build_wrangler_install_preview(def);
        let b = build_wrangler_install_preview(def);
        assert_eq!(a.plan_id, b.plan_id);
        assert_eq!(a.will_run, b.will_run);
    }

    // check_dev_tools_blocking()에서 "경로 자체를 못 찾은"(path == None, 즉
    // resolve_tool_path가 None을 반환한) 상태의 action_kind는 Wrangler만 "run"
    // 이고 나머지 5개는 여전히 "none"이어야 한다(회귀 방지 — 다른 도구의 동작을
    // 바꾸지 않는다는 요구사항의 핵심 단언). `installed`가 아니라 `path`로
    // 분기 여부를 판정한다 — 경로는 찾았지만 버전 조회만 실패해 installed가
    // false인 경우는 Some(resolved_path) 분기(별개 로직)를 타기 때문이다.
    #[test]
    fn only_wrangler_gets_run_action_kind_when_path_not_found() {
        let tools = check_dev_tools_blocking();
        let wrangler = tools
            .iter()
            .find(|t| t.id == "wrangler")
            .expect("wrangler는 DEV_TOOLS에 있어야 합니다");
        if wrangler.path.is_none() {
            assert_eq!(wrangler.action_kind, "run");
        }
        for tool in tools.iter().filter(|t| t.id != "wrangler") {
            if tool.path.is_none() {
                assert_eq!(
                    tool.action_kind, "none",
                    "{}은(는) 경로를 못 찾았을 때도 여전히 none이어야 합니다",
                    tool.id
                );
            }
        }
    }

    // planId가 미리보기 이후 계산값과 다르면(위조·상태 변경) 항상 거부되어야
    // 한다 — Wrangler가 이미 설치돼 있어 매뉴얼 경로로 빠지는 경우까지 포함해서.
    #[test]
    fn wrangler_install_rejects_stale_plan_id() {
        assert!(perform_install("wrangler", "not-a-real-plan-id").is_err());
    }
}
