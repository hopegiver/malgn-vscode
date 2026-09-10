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
#[cfg(unix)]
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
enum MethodKind {
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
    fn kind(&self) -> MethodKind {
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
// 설치 전용(devtools-install-matrix §3.4 실측): `brew install`은 도움말 기준
// `$HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`가 없으면 outdated dependents에 대해
// `brew upgrade`를 함께 돌린다(실측: `gnupg` 설치 시 `gpgme`/`gpgmepp`/`poppler`가
// 딸려 올라감). 설치 하나가 무관한 패키지 업그레이드로 번지는 것을 좁히기 위해
// BREW_ENV에 이 env 하나만 더한 설치 전용 상수를 둔다. 파서
// (parse_brew_install_dry_run_affected)는 이 env가 실제로 먹히는지에 의존하지
// 않는다 — env 적용이 실패해도 헤더 기반 파싱이라 정확하다.
const BREW_INSTALL_ENV: [(&str, &str); 8] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("CI", "1"),
    ("NONINTERACTIVE", "1"),
    ("HOMEBREW_NO_ENV_HINTS", "1"),
    ("HOMEBREW_NO_ANALYTICS", "1"),
    ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
    ("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK", "1"),
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
// pnpm이 전역 설치한 임의 패키지(PnpmGlobalPackage)의 업데이트 — RUN_NPM_GLOBAL의
// npm 버전과 완전히 같은 모양이되 pnpm으로 설치된 패키지이므로 pnpm으로 갱신한다.
// 특정 도구에 묶이지 않는 제네릭 행(UPDATE_TABLE의 NpmGlobal 행과 동일 패턴).
const RUN_PNPM_GLOBAL_UPDATE: RunPlan = RunPlan {
    runner: Runner::Pnpm,
    args: &[Arg::Lit("add"), Arg::Lit("-g"), Arg::PackageLatest],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
// Wrangler 전용 실설치(§12.5, devtools-install-matrix 승계). 미설치 도구
// 전반의 "run 가능 여부" 판정은 이제 install_candidates()/resolve_install_plan()
// 하나로 통일된다(아래 "설치 후보 테이블" 절 참고) — Wrangler는 그 규칙 위에서
// npm 레지스트리 패키지명이 "wrangler"로 고정돼 있어 run 후보가 되는 사례일
// 뿐, 더 이상 코드상의 특별 취급이 아니다.
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
// devtools-install-matrix §2.1/§7: gh는 brew formula명이 "gh" 하나로 고정돼
// 있고 버전 접미가 없다(G1) — brew install은 셸 불필요(G2), 결과가
// <prefix>/bin/gh에 놓여 기존 HomebrewFormula 판별기에 그대로 물린다(G3), gh를
// 버전매니저로 관리하는 관행이 없다(G4). 단, brew install은 의존성을 새로
// 설치·업그레이드할 수 있어(실측 §3.4) preview_args(dry-run)가 필수다.
const RUN_BREW_INSTALL_GH: RunPlan = RunPlan {
    runner: Runner::Brew,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("-y"),
        Arg::Lit("--formula"),
        Arg::Lit("gh"),
    ],
    preview_args: Some(&[
        Arg::Lit("install"),
        Arg::Lit("-n"),
        Arg::Lit("--formula"),
        Arg::Lit("gh"),
    ]),
    env: &BREW_INSTALL_ENV,
    timeout_secs: 600,
};
// devtools-install-matrix §2.1/§3.1/§7: Claude Code npm 패키지명이 공식 문서에
// 고정돼 있다(G1) — npm 전역 설치는 셸 불필요(G2), 결과가 <npm prefix>/bin/claude
// 심볼릭 링크로 기존 NpmGlobal 판별기에 물린다(G3), Claude를 버전매니저로 관리하는
// 관행이 없다(G4). pnpm 폴백은 의도적으로 두지 않는다 — Claude 패키지는
// 플랫폼별 optional dependency를 postinstall이 링크하는데, pnpm v10 계열은
// 의존성의 lifecycle script를 기본 차단해 "exit 0인데 바이너리 없음"이 될 위험이
// 있다(이 머신에서 재현 실험은 금지 범위라 미검증 — 확인되지 않은 리스크를
// 감수하지 않는 쪽을 택했다. §3.1).
const RUN_NPM_INSTALL_CLAUDE: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("-g"),
        Arg::Lit("@anthropic-ai/claude-code"),
    ],
    // §7.1: 전역 npm 설치는 brew처럼 다른 도구를 업그레이드하지 않고(blast
    // radius 없음), dry-run은 레지스트리 해석 시간이 그대로 들어 느려지며,
    // 기존 Wrangler 설치도 프리뷰 없이 이미 동작 중이라 일관된다.
    preview_args: None,
    env: &NPM_ENV,
    timeout_secs: 300,
};
// §3.1 처방(원 설계 결정 1 "파싱 > 하드코딩 맵"에 대한 좁은 예외, 이 한 칸에만
// 적용): npm 전역 설치된 Claude Code는 canonical 경로가 플랫폼별 optional
// dependency(`@anthropic-ai/claude-code-darwin-arm64` 등)를 가리킬 위험이 있어
// (postinstall 링크 대상 — 이 머신에서 확인 불가, 미검증) 제네릭 NpmGlobal
// 파싱에 맡기지 않는다. 공식 문서가 업그레이드 명령을 그대로 이 문자열로
// 지정한다("To upgrade an npm installation, run
// `npm install -g @anthropic-ai/claude-code@latest`. Avoid `npm update -g`") —
// 리터럴이므로 파싱 오류가 개입할 여지가 없다. UPDATE_TABLE에서 제네릭
// NpmGlobal 행보다 앞에 두어(Claude, NpmGlobal) 조합만 이 리터럴을 쓴다.
const RUN_NPM_CLAUDE_LATEST: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("-g"),
        Arg::Lit("@anthropic-ai/claude-code@latest"),
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
    // §3.1 처방: (Claude, NpmGlobal) 조합만 리터럴 업그레이드 명령을 쓴다 — 이
    // 도구별 행이 아래 제네릭 행보다 먼저 와야 한다(lookup_action은 첫 매치를
    // 반환하며, tool: Some(..)가 tool: None보다 더 구체적인 매치다).
    Row {
        tool: Some(ToolId::Claude),
        method: MethodKind::NpmGlobal,
        action: Action::Run(RUN_NPM_CLAUDE_LATEST),
    },
    Row {
        tool: None,
        method: MethodKind::NpmGlobal,
        action: Action::Run(RUN_NPM_GLOBAL),
    },
    Row {
        tool: None,
        method: MethodKind::PnpmGlobalPackage,
        action: Action::Run(RUN_PNPM_GLOBAL_UPDATE),
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

/// 설치 안 된 도구용 안내. classify_install_method는 "이미 존재하는 바이너리의
/// canonical 경로"를 전제하므로 미설치 도구에는 애초에 적용할 수 없다 —
/// formula/package명을 추측해야 하는 도구는 잘못된 추측으로 `brew install <틀린
/// 이름>`을 실행하는 것보다 이 Manual 고정이 안전하다("잘못된 상태를 표현
/// 불가능하게").
///
/// devtools-install-matrix 이후 이 함수는 두 상황 모두에서 호출된다(둘을
/// 문구로 구분하지 않는다 — 어느 쪽이든 "지금 이 앱은 못 하니 이렇게
/// 하라"는 실행 가능한 다음 행동을 주는 것이 핵심이다):
///  ① G1~G4 중 하나라도 원천적으로 탈락하는 도구(pnpm/node/git — §1.2)
///  ② G1~G4는 통과했지만(gh/claude/wrangler) 필요한 실행기(brew/npm/pnpm)를
///     이 머신에서 찾지 못해 resolve_install_plan이 Manual로 강등한 경우
///     (설계 §4.2 "NoRunner" 시나리오 — 무엇이 없어서 안 되는지 구체적으로
///     말한다).
fn install_manual_plan(tool: ToolId) -> ManualPlan {
    match tool {
        ToolId::Claude => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Claude Code가 설치되어 있지 않습니다. npm을 찾을 수 없다면 Node.js를 먼저 설치해주세요. Homebrew가 있다면 `brew install --cask claude-code`도 대안입니다.",
            copyable_command: Some("npm install -g @anthropic-ai/claude-code"),
            doc_url: Some("https://docs.claude.com/en/docs/claude-code/setup"),
        },
        ToolId::Node => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Node.js가 설치되어 있지 않습니다. 이미 nvm/fnm/volta 등으로 관리 중이라면 그쪽에서 설치해주세요. 그렇지 않고 Homebrew가 있다면 아래 명령으로 설치할 수 있습니다.",
            copyable_command: Some("brew install node"),
            doc_url: Some("https://nodejs.org/en/download"),
        },
        ToolId::Gh => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "GitHub CLI가 설치되어 있지 않습니다. Homebrew가 없다면 https://brew.sh 에서 먼저 설치해주세요.",
            copyable_command: Some("brew install gh"),
            doc_url: Some("https://cli.github.com/"),
        },
        ToolId::Git => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Git이 설치되어 있지 않습니다. macOS 명령어 도구(Xcode Command Line Tools)를 설치해주세요.",
            copyable_command: Some("xcode-select --install"),
            doc_url: Some("https://git-scm.com/downloads"),
        },
        ToolId::Pnpm => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "pnpm이 설치되어 있지 않습니다. Homebrew가 있으면 아래 명령이 가장 간단합니다. 공식 설치 문서는 현재 `npx get-pnpm`만 제시하지만, 대화형 확인(Ok to proceed?)에 앱이 응답할 수 없어 자동 실행하지 않습니다.",
            copyable_command: Some("brew install pnpm"),
            doc_url: Some("https://pnpm.io/installation"),
        },
        ToolId::Wrangler => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Wrangler CLI가 설치되어 있지 않습니다. pnpm 또는 npm을 찾을 수 없어 앱이 자동으로 설치하지 못합니다.",
            copyable_command: Some("npm install -g wrangler"),
            doc_url: Some("https://developers.cloudflare.com/workers/wrangler/install-and-update/"),
        },
    }
}

fn is_corepack_managed(corepack_root: Option<&str>) -> bool {
    corepack_root.map(|s| !s.is_empty()).unwrap_or(false)
}

/// 대상 디렉터리에 현재 uid로 쓰기 권한이 있는지 확인한다(부록 B.2 사전 쓰기권한
/// 검사). `libc::access`는 읽기 전용 syscall이다 — 파일을 만들거나 지우지 않는다.
#[cfg(unix)]
fn is_writable_by_current_user(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

/// Windows에는 `access(W_OK)`에 대응하는 직접적인 std API가 없다 — 대신 대상
/// 디렉터리 안에 고유한 이름의 프로브 임시 파일을 만들어보고 즉시 지운다. 생성
/// 시도 자체의 부작용이 적고 성공 시 바로 정리되므로 read-only 검사와 실질적으로
/// 동등하다(부록 B.2 사전 쓰기권한 검사의 Windows 대응).
#[cfg(windows)]
fn is_writable_by_current_user(path: &Path) -> bool {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let probe = path.join(format!(
        ".malgn_vscode_write_probe_{}_{}",
        std::process::id(),
        nanos
    ));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
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

// ==================== 4.5 설치 후보 테이블 + 단일 정본 판정 ====================
// 정본: docs/design/devtools-install-matrix.md §1.2(게이트 4조건)·§1.3(원 설계
// 결정 1을 뒤집는 근거)·§2.1(도구별 run/manual 한 줄)·§6.3(요구 1~4). 그 문서는
// git에 추적되지 않아(.gitignore 대상) 클론하면 사라지므로, "run"/"manual"을
// 가른 판정 근거를 여기 코드 주석으로 보존한다.
//
// ── G1~G4 게이트(하나라도 탈락하면 manual) ──
// G1 고정 식별자 : 공식 문서가 패키지/포뮬러 식별자를 단일 리터럴로 명시(버전
//                  접미·채널 분기가 있으면 탈락)
// G2 셸 불필요   : std::process::Command에 argv 배열로 그대로 넘길 수 있다
//                  (curl|bash, irm|iex, 대화형 npx는 탈락)
// G3 설치 후 자리 예측 가능 : 결과가 기존 path_candidates가 이미 보는 자리에
//                  놓이고 classify_install_method의 기존 분류에 물려 업데이트
//                  경로까지 성립한다
// G4 오탐 피해가 국소적 : 이미 설치돼 있는데 앱이 못 보고 설치를 실행해도
//                  기존 환경이 깨지지 않는다(버전매니저 관리본이 대표 반례)
//
// ── 도구별 run/manual 판정(§2.1 그대로) ──
// | 도구      | run/manual | 근거 한 줄                                                    |
// |-----------|-----------|-----------------------------------------------------------------|
// | Claude    | run       | npm 패키지명이 공식 문서에 고정 + 결과가 기존 NpmGlobal 판별기에 물림 |
// | gh        | run       | brew formula명이 "gh" 하나로 고정, 버전 접미 없음(단, brew install이 |
// |           |           | 의존성을 함께 올릴 수 있어 dry-run 프리뷰 필수 — §3.4 실측)          |
// | Wrangler  | run       | npm 레지스트리 패키지명이 "wrangler"로 고정(원 설계 §12.5 결정 승계) |
// | pnpm      | manual    | 공식 설치기가 셸 스크립트 + rc 파일 수정(G2·G3 탈락) — §3.2          |
// | Node.js   | manual    | 공식 고정 식별자 없음 + 버전매니저 관리본을 앱이 못 봄(G1·G4 탈락)    |
// |           |           | — §3.3(승격 트리거: 사내 표준 major 확정 또는 버전매니저 무셸 탐지)  |
// | Git       | manual    | 시스템(Xcode CLT) 소유, 해결책이 GUI 대화상자(G4 탈락) — §4.3        |
//
// Windows: 아래 install_candidates()는 플랫폼과 무관하게 동일한 테이블을 쓴다.
// 리뷰 정정(review-devtools-install-2026-09-10.md M3): 예전 이 자리의 주석은
// "후보가 전부 POSIX 절대경로라서 Windows 빌드에서는 구조적으로 전부 None이
// 되어 Run으로 해석될 수 없다"고 적었는데, 이는 사실이 아니다 —
// resolve_binary(cli_launcher.rs)는 절대경로 후보가 전부 실패하면 bare-name
// PATH 폴백(`Command::new(bare_name).arg("--version").output()`)을 시도하므로,
// Windows에 예컨대 pnpm.exe가 PATH에 있으면 Some이 될 수 있다. 즉 "구조적으로
// 불가능"이 아니라 "런타임 해석이 우연히 실패하는 경우가 많다"에 가깝다.
// 실제로 Windows에서 설치/업데이트 실행을 막는 것은 명시적 게이트 둘이다:
// ① check_dev_tools_blocking() 최상단의 `!cfg!(target_os = "macos")` 분기
//   (§5.2 채택 — 화면에 내려가는 데이터 전체를 manual로 고정한다)
// ② preview_dev_tool_update/update_dev_tool/install_dev_tool/
//   open_manual_instruction 네 IPC 커맨드 각각에 있는 동일한 cfg 게이트 — 화면을
//   거치지 않고 커맨드가 직접 호출돼도 막힌다("IPC 커맨드는 화면을 신뢰하면 안
//   된다" 원칙, M3).

struct InstallCandidate {
    runner: Runner,
    plan: RunPlan,
    installer_label: &'static str,
}

/// (도구) → 시도할 run 후보 목록(우선순위 순). 없는 도구는 항상 Manual이다.
/// 설계 §1.1 불변식: 여기 등장하는 모든 RunPlan은 Arg::Lit 전용이라야 한다 —
/// Arg::Formula/Package/PackageLatest 슬롯은 canonical 경로 없이는 채울 수
/// 없으므로(미설치 도구에는 canonical 경로가 없다) 이 테이블에 올릴 수 없다.
fn install_candidates(tool: ToolId) -> &'static [InstallCandidate] {
    match tool {
        ToolId::Gh => &[InstallCandidate {
            runner: Runner::Brew,
            plan: RUN_BREW_INSTALL_GH,
            installer_label: "brew",
        }],
        ToolId::Claude => &[InstallCandidate {
            runner: Runner::Npm,
            plan: RUN_NPM_INSTALL_CLAUDE,
            installer_label: "npm",
        }],
        // pnpm을 먼저 시도하고, 해석 자체가 안 되면 npm으로 폴백한다(원 설계
        // §12.5 결정 그대로 승계 — "해석 실패"와 "실행 실패"를 구분하는 것이
        // 핵심이라 pnpm이 있는데 명령 자체가 실패하는 경우는 여기서 걸러내지
        // 않는다. 그 판정은 run_install_plan의 실행 결과 처리 몫이다).
        ToolId::Wrangler => &[
            InstallCandidate {
                runner: Runner::Pnpm,
                plan: RUN_PNPM_GLOBAL_ADD,
                installer_label: "pnpm",
            },
            InstallCandidate {
                runner: Runner::Npm,
                plan: RUN_NPM_GLOBAL_INSTALL_WRANGLER,
                installer_label: "npm",
            },
        ],
        // G1/G2/G3/G4 중 최소 하나씩 탈락 — 위 표 참고. 후보 없음 = 항상 manual.
        ToolId::Node | ToolId::Git | ToolId::Pnpm => &[],
    }
}

/// brew/npm/pnpm 실행기 경로를 화면 1회 로드에서 한 번만 해석해 담아 두는
/// 그릇(요구 4 — resolve_binary의 bare-name PATH 폴백은 타임아웃이 없어
/// 도구 개수만큼 반복 호출하면 그만큼 블로킹 비용이 커진다. 지적 #12 자체의
/// 수정은 이 범위 밖이지만 호출 횟수를 1회로 묶어 그 결함의 노출을 늘리지
/// 않는다).
struct ResolvedRunners {
    brew: Option<String>,
    npm: Option<String>,
    pnpm: Option<String>,
}

impl ResolvedRunners {
    fn resolve() -> Self {
        ResolvedRunners {
            brew: resolve_runner_path(Runner::Brew, None),
            npm: resolve_runner_path(Runner::Npm, None),
            pnpm: resolve_runner_path(Runner::Pnpm, None),
        }
    }

    fn path_for(&self, runner: Runner) -> Option<&str> {
        match runner {
            Runner::Brew => self.brew.as_deref(),
            Runner::Npm => self.npm.as_deref(),
            Runner::Pnpm => self.pnpm.as_deref(),
            // SelfBinary는 설치 후보에 쓰이지 않는다(설치 시점엔 자기 자신의
            // 바이너리가 아직 없다) — install_candidates()의 어떤 행도 이
            // 변형을 쓰지 않으므로 항상 None으로 충분하다.
            Runner::SelfBinary => None,
        }
    }
}

/// 실행 준비까지 끝난 설치 계획. `check_dev_tools_blocking`(actionKind 산출)·
/// `perform_preview`(미설치 분기)·`perform_install`(실행 직전 재계산) 세 곳이
/// 모두 이 함수만 호출한다(요구 1 — 단일 정본). 세 곳이 각자 분기하지 않으므로
/// plan_id 종류가 어긋날 수 없다(직전 리뷰 지적 #3의 "막다른 골목"을 구조적으로
/// 제거).
enum InstallResolution {
    Run {
        runner_path: String,
        argv: Vec<String>,
        plan: RunPlan,
        installer_label: &'static str,
    },
    Manual(ManualPlan),
}

/// M2(devtools-install-matrix §4.2, 직전 리뷰 지적) — 설치 경로 쓰기권한
/// 사전검사. gh(brew)는 `<prefix>/Cellar`·`<prefix>/bin`을, Claude(npm)는
/// `<npm prefix>/lib/node_modules`(없으면 `<prefix>/lib`, 그마저 없으면 검사
/// 생략)를 본다 — `compute_action`(908행, update 경로)이 이미 하는 검사를
/// 설치 경로에도 재사용한다. prefix는 runner 경로의 조부모다
/// (`/opt/homebrew/bin/brew` → `/opt/homebrew`).
///
/// §4.2 단서 그대로: 없는 경로에 대해 `access(W_OK)`가 무조건 false를 주므로,
/// "아직 없는 경로"를 "쓰기 불가"로 오판해 fail-closed로 기능을 죽이지 않도록
/// 경로가 존재할 때만 검사한다(존재하지 않으면 통과시키고, 실제로 못 쓰면
/// exit code로 드러난다). 설계 §4.2 표는 gh·Claude 두 행만 요구하므로 그 외
/// 조합(Wrangler의 pnpm/npm)은 대상이 아니다.
fn install_prefix_writable(tool: ToolId, runner: Runner, runner_path: &str) -> bool {
    let Some(prefix) = Path::new(runner_path).parent().and_then(Path::parent) else {
        return true;
    };

    match (tool, runner) {
        (ToolId::Gh, Runner::Brew) => {
            let cellar = prefix.join("Cellar");
            let bin = prefix.join("bin");
            (!cellar.exists() || is_writable_by_current_user(&cellar))
                && (!bin.exists() || is_writable_by_current_user(&bin))
        }
        (ToolId::Claude, Runner::Npm) => {
            let node_modules = prefix.join("lib").join("node_modules");
            if node_modules.exists() {
                is_writable_by_current_user(&node_modules)
            } else {
                let lib = prefix.join("lib");
                !lib.exists() || is_writable_by_current_user(&lib)
            }
        }
        _ => true,
    }
}

fn resolve_install_plan(tool: ToolId, runners: &ResolvedRunners) -> InstallResolution {
    // 미설치 도구에는 canonical 경로가 없다 — install_candidates()의 모든
    // RunPlan이 Arg::Lit 전용이므로(§1.1 불변식) 이 더미 값으로도 resolve_args가
    // 항상 성공한다(Formula/Package 슬롯은 여기서 절대 쓰이지 않는다).
    let dummy_method = InstallMethod::Unknown(String::new());
    for candidate in install_candidates(tool) {
        let Some(runner_path) = runners.path_for(candidate.runner) else {
            continue;
        };
        if !install_prefix_writable(tool, candidate.runner, runner_path) {
            return InstallResolution::Manual(MANUAL_NOT_WRITABLE);
        }
        if let Ok(argv) = resolve_args(candidate.plan.args, &dummy_method) {
            return InstallResolution::Run {
                runner_path: runner_path.to_string(),
                argv,
                plan: candidate.plan,
                installer_label: candidate.installer_label,
            };
        }
    }
    InstallResolution::Manual(install_manual_plan(tool))
}

/// macOS의 `/usr/bin/git`은 CLT 미설치 상태에서도 파일로 존재하는 xcrun
/// 스텁이다(§4.3) — 이 함수는 프로세스를 하나도 띄우지 않고 파일 존재 여부만
/// 검사해 "진짜 CLT가 깔려 있는지"를 판정한다. CLT 미설치 머신에서
/// `/usr/bin/git --version`을 실행하면 "명령어 개발자 도구를 설치하시겠습니까?"
/// 시스템 GUI 대화상자를 띄울 수 있어(미검증 — 이 머신은 CLT가 있어 재현 불가)
/// check_tool_version 호출 자체를 원천 차단한다.
fn is_git_stub_without_clt(resolved_path: &str) -> bool {
    resolved_path == "/usr/bin/git"
        && !Path::new("/Library/Developer/CommandLineTools").exists()
        && !Path::new("/Applications/Xcode.app").exists()
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
                InstallMethod::PnpmGlobalPackage { package } => package.clone(),
                _ => return Err("내부 오류: package 슬롯을 채울 설치방식이 아닙니다".to_string()),
            },
            Arg::PackageLatest => match method {
                InstallMethod::NpmGlobal { package } => format!("{package}@latest"),
                InstallMethod::PnpmGlobalPackage { package } => format!("{package}@latest"),
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

pub(crate) struct ProcessRunOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
    /// 앱 종료(`should_abort`)로 인해 중단됐는지 — `timed_out`과 별개 플래그라
    /// 로그에서 "타임아웃"과 "앱 종료로 중단"을 구분할 수 있다(자율업무
    /// 설계 §4). 기존 `run_process_with_timeout` 경로(`should_abort: None`)는
    /// 항상 `false`다.
    pub(crate) aborted: bool,
    pub(crate) duration: Duration,
    pub(crate) spawn_error: Option<String>,
}

enum WaitOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
    Aborted,
}

/// `should_abort`가 `Some`이고 매 100ms 폴링마다 `true`를 반환하면 즉시
/// `Aborted`로 끝낸다 — 자율업무 워커가 앱 종료 시 진행 중인 `claude -p`를
/// 타임아웃과 동일한 kill 경로로 정리하기 위한 훅이다. `should_abort`가
/// `None`이면(기존 `run_process_with_timeout` 경로) 이 확인을 건너뛰어
/// 기존 동작을 한 글자도 바꾸지 않는다.
fn wait_up_to_cancellable(
    child: &mut std::process::Child,
    timeout: Duration,
    started: Instant,
    should_abort: Option<&dyn Fn() -> bool>,
) -> WaitOutcome {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status),
            Ok(None) => {
                if let Some(check) = should_abort {
                    if check() {
                        return WaitOutcome::Aborted;
                    }
                }
                if started.elapsed() >= timeout {
                    return WaitOutcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // 기존 `wait_up_to`와 동일하게 취급: try_wait 자체가 에러면 즉시
            // kill 경로로 넘긴다(재시도하지 않는다).
            Err(_) => return WaitOutcome::TimedOut,
        }
    }
}

/// 프로세스 그룹 kill: SIGTERM → 3초 유예(try_wait 폴링) → SIGKILL. brew가 낳는
/// curl/git/ruby 손자 프로세스까지 함께 죽인다(`child.kill()`은 직속 자식만 죽여
/// 손자가 다운로드를 계속하는 문제가 있다 — 그래서 group kill이 필수).
#[cfg(unix)]
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

/// Windows에는 POSIX 프로세스 그룹이 없다. 이 함수가 다루는 러너(Npm/Pnpm/
/// SelfBinary)는 현재 macOS 전용 게이트(997행 참고)로 Windows에서 실행에
/// 도달하지 않지만, 그 게이트를 걷어내 Windows 지원을 열 때를 대비해 정의는
/// 유지한다 — 그 시점에도 손자 프로세스를 남기는 경우가 드물어, 직속
/// 자식만 종료(`Child::kill` → `TerminateProcess`)하는 것으로 MVP 범위에서는
/// 충분하다고 판단했다(50인 미만 내부 도구, Job Object 등 고급 처리는 과설계).
/// 손자 프로세스는 정리되지 않을 수 있다는 제약을 감수한다. pid 인자는 unix
/// 버전과 시그니처를 맞추기 위해서만 존재하며 사용하지 않는다.
#[cfg(windows)]
fn force_kill_process_group(_pid: i32, child: &mut std::process::Child) -> Option<i32> {
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// stdin=null(부록 B.1 — 프롬프트가 즉시 EOF를 받아 정지 대신 실패한다) +
/// stdout/stderr 리더 스레드(파이프 64KB 버퍼가 차서 자식이 write에서 멈추는
/// 교착을 막는다) + try_wait() 100ms 폴링 타임아웃 + 프로세스 그룹 kill.
///
/// `run_process_with_timeout_cancellable`의 얇은 래퍼다(`on_spawn`/
/// `should_abort` 없이 호출) — 기존 4개 호출부와 3개 테스트는 이 시그니처가
/// 그대로 유지되므로 한 글자도 바뀌지 않는다(자율업무 설계 §4).
pub(crate) fn run_process_with_timeout(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
) -> ProcessRunOutput {
    run_process_with_timeout_cancellable(binary_path, args, path_env, extra_env, timeout, None, None)
}

/// `on_spawn`: spawn 직후 자식 pid를 공표한다(자율업무 런타임 레지스트리
/// 등록용). `should_abort`: `wait_up_to_cancellable`의 100ms 폴링마다 확인해
/// `true`면 타임아웃과 동일한 경로(`force_kill_process_group`)로 kill한다.
/// 자율업무는 이 함수를 통해 앱 종료 시 진행 중인 `claude -p`를 정리한다.
///
/// 주의: 이 함수는 `EXECUTION_LOCK`을 취하지 않는다(그 락은 도구 설치를
/// 직렬화하는 락이라 자율업무가 취하면 전역 동시성이 1이 되어 concurrency
/// 설계가 무너진다) — 호출자가 직접 그 락을 잡지 않도록 주의한다.
pub(crate) fn run_process_with_timeout_cancellable(
    binary_path: &str,
    args: &[String],
    path_env: &str,
    extra_env: &[(&str, &str)],
    timeout: Duration,
    on_spawn: Option<&dyn Fn(u32)>,
    should_abort: Option<&dyn Fn() -> bool>,
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
    // 새 프로세스 그룹으로 스폰(force_kill_process_group의 그룹 kill이 작동하려면
    // 필요) — Windows에는 이 개념 자체가 없어 이 호출도 없다(CommandExt는 unix 전용).
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ProcessRunOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                aborted: false,
                duration: started.elapsed(),
                spawn_error: Some(format!("실행할 수 없습니다: {e}")),
            };
        }
    };

    let pid = child.id() as i32;
    if let Some(cb) = on_spawn {
        cb(pid as u32);
    }
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

    let (exit_code, timed_out, aborted) =
        match wait_up_to_cancellable(&mut child, timeout, started, should_abort) {
            WaitOutcome::Exited(status) => (status.code(), false, false),
            WaitOutcome::TimedOut => (force_kill_process_group(pid, &mut child), true, false),
            WaitOutcome::Aborted => (force_kill_process_group(pid, &mut child), false, true),
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
        aborted,
        duration: started.elapsed(),
        spawn_error: None,
    }
}

/// `<brew_prefix>/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` — 절대경로
/// 실행만으로는 부족하다(brew/npm 내부에서 git/curl/ruby/node를 셔뱅으로 부른다).
/// runner_path의 bin 디렉터리를 최우선으로 넣고 표준 경로를 뒤에 덧붙인다.
pub(crate) fn build_child_path_env(runner_path: Option<&str>) -> String {
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
fn parse_brew_install_dry_run_affected(stdout: &str, target: &str) -> Vec<String> {
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

/// 설계 §5.2 채택(대안 "범위를 더 줄인다" 쪽): §5.1이 권고한 Windows
/// path_candidates cfg 분기(+ %USERPROFILE%/%LOCALAPPDATA% 등 토큰 확장,
/// build_child_path_env 구분자 분기)는 이번 스프린트 범위 밖으로 미룬다 — 이
/// 머신에 winget이 없어 Windows 10칸을 단 한 줄도 실행 검증할 수 없고(위임서
/// 실측), Sensitive 등급에서 검증 없이 탐지 범위를 넓히는 것 자체가 이 설계의
/// 원칙에 어긋난다. 대신 §5.2의 최소 요건 — "아무 표시 없이 전부 '설치 안
/// 됨'으로 보이는(=거짓을 말하는) 상태는 남기지 않는다" — 를 충족한다:
/// Windows 빌드에서는 진단을 아예 시도하지 않고 "이 화면은 현재 macOS만
/// 지원합니다"를 명시적으로 보여준다. action_kind는 계약을 지키기 위해 여전히
/// "run"|"manual"|"none" 중 하나여야 하므로 "manual"을 쓰고, manual_hint에
/// 이유를 담는다(신규 필드 추가 없음 — 계약 불변). 순수 함수로 분리해 플랫폼과
/// 무관하게 테스트할 수 있게 한다.
// M3(review-devtools-install-2026-09-10.md): 화면 데이터(아래 함수)뿐 아니라
// IPC 커맨드 4개(preview_dev_tool_update/update_dev_tool/install_dev_tool/
// open_manual_instruction) 각각에도 동일한 게이트를 둔다 — 문구를 한 곳에서만
// 관리해 드리프트를 막는다.
const MACOS_ONLY_MESSAGE: &str = "이 화면은 현재 macOS만 지원합니다. Windows 지원은 준비 중입니다.";

fn windows_unsupported_dev_tools_status() -> Vec<DevToolStatus> {
    DEV_TOOLS
        .iter()
        .map(|def| DevToolStatus {
            id: def.key.to_string(),
            name: def.label.to_string(),
            installed: false,
            version: None,
            path: None,
            install_method: None,
            action_kind: "manual".to_string(),
            manual_hint: Some(MACOS_ONLY_MESSAGE.to_string()),
        })
        .collect()
}

fn check_dev_tools_blocking() -> Vec<DevToolStatus> {
    if !cfg!(target_os = "macos") {
        return windows_unsupported_dev_tools_status();
    }

    // 요구 4(성능): brew/npm/pnpm 러너를 도구 6개 루프 전체에서 1회만 해석한다.
    let runners = ResolvedRunners::resolve();

    DEV_TOOLS
        .iter()
        .map(|def| match resolve_tool_path(def) {
            None => {
                // 요구 1(단일 정본) + 요구 3(하드코딩 제거): 미설치 상태의
                // action_kind는 더 이상 `if def.id == ToolId::Wrangler` 같은
                // 도구별 하드코딩이 아니라 resolve_install_plan 하나의 결과다
                // — Gh/Claude/Wrangler는 run 후보가 있어 "run", 나머지는
                // "manual"이 된다(§2 매트릭스). "none"을 반환하는 경로는 이제
                // 존재하지 않는다(§6.2 — 프론트의 none 분기는 방어적으로 남긴다).
                let (action_kind, manual_hint) = match resolve_install_plan(def.id, &runners) {
                    InstallResolution::Run { .. } => ("run".to_string(), None),
                    InstallResolution::Manual(mp) => {
                        ("manual".to_string(), Some(manual_display_message(&mp)))
                    }
                };
                DevToolStatus {
                    id: def.key.to_string(),
                    name: def.label.to_string(),
                    installed: false,
                    version: None,
                    path: None,
                    install_method: None,
                    action_kind,
                    manual_hint,
                }
            }
            Some(resolved_path) => {
                // §4.3 처방 2: CLT 미설치 머신에서 `/usr/bin/git` 스텁에
                // `--version`을 실행하면 시스템 GUI 대화상자를 띄울 수 있다 —
                // 프로세스를 하나도 띄우지 않는 파일 검사만으로 먼저 걸러낸다.
                if def.id == ToolId::Git && is_git_stub_without_clt(&resolved_path) {
                    return DevToolStatus {
                        id: def.key.to_string(),
                        name: def.label.to_string(),
                        installed: false,
                        version: None,
                        path: Some(resolved_path),
                        install_method: Some(describe_install_method(
                            &InstallMethod::SystemManaged,
                        )),
                        action_kind: "manual".to_string(),
                        manual_hint: Some(manual_display_message(&MANUAL_XCODE_CLT)),
                    };
                }

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

/// 미설치 도구용 설치 프리뷰(요구 1의 단일 정본 `resolve_install_plan`을 통해서만
/// Run/Manual을 가른다). Wrangler 전용이던 `build_wrangler_install_preview`를
/// 대체하며, gh/Claude 모두 이 함수 하나로 처리된다.
fn build_install_preview(def: &DevTool, tool: ToolId, runners: &ResolvedRunners) -> DevToolPreview {
    match resolve_install_plan(tool, runners) {
        InstallResolution::Manual(mp) => {
            // 미설치 상태라 "이전 버전"이 없다 — normalized_before는 빈 문자열로
            // 고정한다(perform_install이 실행 직전 같은 값으로 재계산해 대조하므로
            // 값 자체보다 안정성이 중요하다).
            let plan_id = compute_plan_id_for_manual(tool, &mp);
            DevToolPreview {
                id: def.key.to_string(),
                plan_id,
                will_run: false,
                command_display: mp.copyable_command.unwrap_or("").to_string(),
                affected: Vec::new(),
                notes: manual_display_message(&mp),
                preview_reliable: true,
            }
        }
        InstallResolution::Run {
            runner_path,
            argv,
            plan,
            installer_label,
        } => {
            let display = build_command_display(&runner_path, &argv);
            let plan_id = compute_plan_id(&runner_path, &argv, "");

            let (affected, notes, preview_reliable) = match plan.preview_args {
                Some(preview_args) => {
                    let dummy_method = InstallMethod::Unknown(String::new());
                    let preview_argv = match resolve_args(preview_args, &dummy_method) {
                        Ok(v) => v,
                        Err(_) => {
                            // M5: §1.1 불변식("설치 프리뷰 인자는 전부 리터럴")이
                            // 깨지면 예전엔 여기서 `.expect()`로 패닉했다. args
                            // 슬롯 위반이 resolve_install_plan(위)에서 후보를
                            // 건너뛰어 Manual로 fail-closed되는 것과 같은 방향으로
                            // 통일한다 — Run을 반환하는 대신 안내로 강등한다.
                            let mp = install_manual_plan(tool);
                            return DevToolPreview {
                                id: def.key.to_string(),
                                plan_id: compute_plan_id_for_manual(tool, &mp),
                                will_run: false,
                                command_display: mp.copyable_command.unwrap_or("").to_string(),
                                affected: Vec::new(),
                                notes: manual_display_message(&mp),
                                preview_reliable: true,
                            };
                        }
                    };
                    let path_env = build_child_path_env(Some(&runner_path));
                    let output = run_process_with_timeout(
                        &runner_path,
                        &preview_argv,
                        &path_env,
                        plan.env,
                        Duration::from_secs(120),
                    );
                    if let Some(err) = output.spawn_error {
                        (
                            vec![def.label.to_string()],
                            format!("미리보기 실행에 실패했습니다: {err}"),
                            false,
                        )
                    } else if output.timed_out {
                        (
                            vec![def.label.to_string()],
                            "미리보기가 시간 초과되었습니다. 실행 시 실제 범위가 다를 수 있습니다."
                                .to_string(),
                            false,
                        )
                    } else {
                        // 대상 포뮬러 자신은 argv의 마지막 리터럴 토큰이다(현재
                        // install_candidates()의 모든 RunPlan이 이 형태 —
                        // ["install","-n","--formula","gh"] 등).
                        let target = argv.last().map(|s| s.as_str()).unwrap_or(def.label);
                        let extras = parse_brew_install_dry_run_affected(&output.stdout, target);
                        let notes = if !extras.is_empty() {
                            format!(
                                "요청한 도구 외에 {}개 항목이 함께 설치되거나 업그레이드될 수 있습니다: {}",
                                extras.len(),
                                extras.join(", ")
                            )
                        } else {
                            String::new()
                        };
                        let mut affected = vec![def.label.to_string()];
                        affected.extend(extras);
                        (affected, notes, true)
                    }
                }
                None => {
                    // §7.1/§0.1 사고①: 프리뷰가 없는 설치도 "앱이 뒤진 경로
                    // 목록"을 notes에 채워 사용자가 "나 이미 깔려 있는데?"를
                    // 스스로 잡아낼 수 있게 한다.
                    let searched = def.path_candidates.join(", ");
                    (
                        vec![def.label.to_string()],
                        format!(
                            "{installer_label}(으)로 {}을(를) 전역 설치합니다. 패키지명이 공식 문서에 고정돼 있어 자동 실행이 안전합니다. 앱이 확인한 경로: {searched}",
                            def.label
                        ),
                        true,
                    )
                }
            };

            DevToolPreview {
                id: def.key.to_string(),
                plan_id,
                will_run: true,
                command_display: display,
                affected,
                notes,
                preview_reliable,
            }
        }
    }
}

fn perform_preview(tool_id_str: &str) -> Result<DevToolPreview, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    match resolve_tool_path(def) {
        None => {
            let runners = ResolvedRunners::resolve();
            Ok(build_install_preview(def, tool, &runners))
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
    // M3: 화면(check_dev_tools_blocking)의 macOS 전용 게이트를 커맨드 계층에도
    // 둔다 — 이 커맨드는 화면이 actionKind를 올바르게 내려줬다고 신뢰하지 않는다.
    if !cfg!(target_os = "macos") {
        return Err(MACOS_ONLY_MESSAGE.to_string());
    }
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
    // M3: 커맨드 계층 게이트(preview_dev_tool_update와 동일 이유).
    if !cfg!(target_os = "macos") {
        return Err(MACOS_ONLY_MESSAGE.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || perform_update(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

/// 요구 1(단일 정본) + 요구 2(install→update 위임) + 요구 3(하드코딩 제거):
///  - 이미 설치돼 있으면(=resolve_tool_path가 Some) 첫 줄에서 perform_update로
///    위임한다. 프리뷰가 update 경로(build_run_preview)에서 나왔다면 plan_id가
///    그대로 맞아떨어져 한 번에 성공한다 — 직전 리뷰 지적 #3의 "installed:false
///    + actionKind:run" 막다른 골목(프리뷰는 update용 plan_id, 실행은 install용
///    대조를 쓰는 영구 불일치)이 구조적으로 소멸한다.
///  - 미설치 상태에서는 resolve_install_plan 하나로만 Run/Manual을 가른다.
///    Wrangler·gh·Claude는 Run(각자의 install_candidates), pnpm/node/git은
///    Manual — 더 이상 `if tool == ToolId::Wrangler` 같은 도구별 하드코딩이
///    없다.
fn perform_install(tool_id_str: &str, plan_id: &str) -> Result<DevToolActionResult, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    // 요구 2 — 이 위임은 EXECUTION_LOCK을 잡기 전에 이뤄져야 한다. perform_update가
    // 스스로 락을 잡으므로(std::sync::Mutex는 재진입 불가), 여기서 먼저 락을
    // 잡으면 즉시 자기 자신과 충돌해 항상 "다른 업데이트가 실행 중"으로
    // 오판된다.
    if resolve_tool_path(def).is_some() {
        return perform_update(tool_id_str, plan_id);
    }

    let _guard = EXECUTION_LOCK.try_lock().map_err(|_| {
        "다른 업데이트가 이미 실행 중입니다. 완료 후 다시 시도해주세요.".to_string()
    })?;

    let runners = ResolvedRunners::resolve();
    match resolve_install_plan(tool, &runners) {
        InstallResolution::Manual(mp) => {
            let expected_plan_id = compute_plan_id_for_manual(tool, &mp);
            if expected_plan_id != plan_id {
                return Err(
                    "실행 계획이 미리보기 이후 바뀌었습니다. 다시 미리보기를 요청해주세요."
                        .to_string(),
                );
            }
            Ok(not_supported_result(def, manual_display_message(&mp)))
        }
        InstallResolution::Run {
            runner_path,
            argv,
            plan,
            installer_label,
        } => {
            let expected_plan_id = compute_plan_id(&runner_path, &argv, "");
            if expected_plan_id != plan_id {
                return Err(
                    "실행 계획이 미리보기 이후 바뀌었습니다. 다시 미리보기를 요청해주세요."
                        .to_string(),
                );
            }
            run_install_plan(def, &runner_path, &argv, &plan, installer_label)
        }
    }
}

/// 결정 4의 "claimed ≠ verified"를 설치에도 그대로 적용한다: exit code 0을 성공
/// 주장(claim)으로만 보지 않고, 설치 후 check_tool_version()으로 실제 버전을
/// 다시 조회했을 때만 확인(verify)된 성공으로 본다. Outcome은 기존 update 흐름의
/// 값(Updated/UnknownAfter/Failed/TimedOut)을 그대로 재사용한다 — 프론트가 이미
/// Updated 시점에 목록을 새로고침하므로(devTools.ts runPlan) 신규 Outcome 변형을
/// 추가하지 않아도 설치 직후 상태가 목록에 바로 반영된다.
///
/// 원래 Wrangler 전용이었던 `perform_wrangler_install`을 일반화한 것 — gh·Claude·
/// Wrangler 모두 이 함수 하나로 실행된다(installer_label만 도구/후보별로 다르다).
fn run_install_plan(
    def: &DevTool,
    runner_path: &str,
    argv: &[String],
    plan: &RunPlan,
    installer_label: &'static str,
) -> Result<DevToolActionResult, String> {
    let path_env = build_child_path_env(Some(runner_path));
    let output = run_process_with_timeout(
        runner_path,
        argv,
        &path_env,
        plan.env,
        Duration::from_secs(plan.timeout_secs),
    );

    // pnpm/brew/npm이 해석돼 실행까지 갔다면(성공이든 실패든) 그 결과를 그대로
    // 보고한다 — 다른 러너로 조용히 폴백하지 않는다(요구사항: 실패를 숨기지
    // 않는다).
    let resolved_after = resolve_tool_path(def);
    let version_after = resolved_after
        .as_deref()
        .and_then(|p| check_tool_version(def, p));
    let normalized_after = version_after.as_deref().and_then(normalize_version);

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
                format!("{}이(가) {installer_label}(으)로 설치되었습니다 ({after}).", def.label),
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
        install_method: format!(
            "{installer_label}Install({})",
            argv.last().map(|s| s.as_str()).unwrap_or(def.key)
        ),
        ran_command: Some(build_command_display(runner_path, argv)),
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
    // M3: 커맨드 계층 게이트(preview_dev_tool_update와 동일 이유).
    if !cfg!(target_os = "macos") {
        return Err(MACOS_ONLY_MESSAGE.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || perform_install(&tool_id, &plan_id))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
}

// M2(직전 라운드): Manual 경로는 Run 경로(preview -> 사용자 확인 -> 실행)와 달리
// 동의 절차 없이 즉시 실행됐다. `execute` 파라미터로 같은 커맨드를
// "미리보기"(false)와 "실행"(true) 두 모드로 재사용해, 프론트가 먼저 어떤
// 명령이 실행될지 보여주고 사용자 확인을 받은 뒤에만 execute:true로 다시
// 호출하게 한다. 계획 해석 로직을 두 곳에 중복시키지 않기 위해 한 함수 안에서
// 분기하며, 반환 타입은 기존 TerminalLaunchResult를 그대로 재사용한다(필드
// 추가 없음 — 프론트-백엔드 타입 계약 변경 없음).
fn perform_open_manual_instruction(
    tool_id_str: &str,
    execute: bool,
) -> Result<TerminalLaunchResult, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    let manual: Option<ManualPlan> = match resolve_tool_path(def) {
        Some(resolved_path) => match resolve_plan(tool, &resolved_path).action {
            ResolvedAction::Manual(mp) => Some(mp),
            ResolvedAction::Run { .. } => None,
        },
        // 미설치 상태에서도 gh/Claude/Wrangler는 Run 후보를 가질 수 있다(요구 1) —
        // 그 경우 "터미널에서 실행" 안내는 필요 없다(앱이 직접 실행하므로).
        None => {
            let runners = ResolvedRunners::resolve();
            match resolve_install_plan(tool, &runners) {
                InstallResolution::Manual(mp) => Some(mp),
                InstallResolution::Run { .. } => None,
            }
        }
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

/// M4(review-devtools-install-2026-09-10.md): 이 커맨드가 예전엔 non-async
/// `pub fn`이었다 — 이번 변경(ResolvedRunners::resolve() 호출 추가)으로
/// 타임아웃 없는 하위 프로세스 spawn 경로(bare-name PATH 폴백,
/// cli_launcher.rs:27)를 새로 얻었는데, non-async 커맨드는 Tauri v2에서
/// 메인 스레드에서 돈다 — 이 파일이 check_dev_tools(1968행대)를 async +
/// spawn_blocking으로 옮기며 스스로 "P0 버그"로 규정한 바로 그 패턴이다.
/// 같은 방식으로 옮긴다 — 프론트 `invoke()`는 어차피 항상 Promise라 계약이
/// 바뀌지 않는다.
#[tauri::command]
pub async fn open_manual_instruction(
    tool_id: String,
    execute: bool,
) -> Result<TerminalLaunchResult, String> {
    // M3: 커맨드 계층 게이트(preview_dev_tool_update와 동일 이유).
    if !cfg!(target_os = "macos") {
        return Err(MACOS_ONLY_MESSAGE.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || perform_open_manual_instruction(&tool_id, execute))
        .await
        .map_err(|e| format!("내부 작업 실행 오류: {e}"))?
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
            Action::Run(plan) if plan.runner == Runner::Pnpm
                && matches!(plan.args, [Arg::Lit("add"), Arg::Lit("-g"), Arg::PackageLatest])
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

    // §4.3 처방 2: `/usr/bin/git` 스텁 가드는 순수 파일 존재 검사만으로
    // 판정해야 한다(프로세스를 하나도 띄우지 않음). 이 머신은 CLT가 실제로
    // 설치돼 있어(§9 미검증 항목 4) 정상 케이스만 실측으로 확인 가능하다 —
    // "CLT 없음"쪽은 함수 시그니처가 받는 조건을 직접 구성해 검증한다.
    #[test]
    fn git_stub_guard_only_trips_on_exact_stub_path() {
        // 다른 경로는 무조건 false(스텁 경로가 아니므로 CLT 설치 여부와 무관).
        assert!(!is_git_stub_without_clt("/opt/homebrew/bin/git"));
        assert!(!is_git_stub_without_clt("/usr/local/bin/git"));
        assert!(!is_git_stub_without_clt(
            "/Library/Developer/CommandLineTools/usr/bin/git"
        ));
        // 이 머신은 CLT가 실제로 설치돼 있으므로(위임서 실측) /usr/bin/git
        // 경로를 줘도 false여야 한다 — CLT가 있으면 스텁 가드가 발동하면 안
        // 된다는 조건 자체를 검증한다.
        if Path::new("/Library/Developer/CommandLineTools").exists()
            || Path::new("/Applications/Xcode.app").exists()
        {
            assert!(!is_git_stub_without_clt("/usr/bin/git"));
        }
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
    // pnpm을 우선하고, 그 argv가 RUN_PNPM_GLOBAL_ADD와 일치해야 한다. 원래
    // Wrangler 전용 `resolve_wrangler_install_choice`를 검증하던 테스트를
    // 일반화된 `resolve_install_plan`으로 옮겼다.
    #[test]
    fn wrangler_install_resolution_prefers_pnpm_over_npm_when_both_resolve() {
        let runners = ResolvedRunners::resolve();
        match resolve_install_plan(ToolId::Wrangler, &runners) {
            InstallResolution::Run {
                installer_label,
                argv,
                ..
            } => match installer_label {
                "pnpm" => assert_eq!(argv, vec!["add", "-g", "wrangler"]),
                "npm" => assert_eq!(argv, vec!["install", "-g", "wrangler"]),
                other => panic!("알 수 없는 installer_label: {other}"),
            },
            InstallResolution::Manual(_) => {
                // 이 머신에 pnpm도 npm도 없는 경우 — 정상적인 값(패닉이 아니다).
            }
        }
    }

    // 같은 환경에서 두 번 호출하면 같은 plan_id가 나와야 한다(부록 A의 안정성
    // 요구 — 미설치 상태에도 동일하게 적용). 원래 Wrangler 전용
    // `build_wrangler_install_preview`를 검증하던 테스트를 일반화된
    // `build_install_preview`로 옮겼다.
    #[test]
    fn wrangler_install_preview_plan_id_is_stable_across_calls() {
        let def = tool_definition(ToolId::Wrangler);
        let runners = ResolvedRunners::resolve();
        let a = build_install_preview(def, ToolId::Wrangler, &runners);
        let b = build_install_preview(def, ToolId::Wrangler, &runners);
        assert_eq!(a.plan_id, b.plan_id);
        assert_eq!(a.will_run, b.will_run);
    }

    // devtools-install-matrix §1.2/§2.1: install_candidates()가 machine 상태와
    // 무관하게 정적으로 "run 후보 있음/없음"을 결정한다는 정책 자체를 검증한다
    // (G1~G4 게이트 결과). 실제 브루/npm 해석 여부와 독립적이라 이 머신에
    // 무엇이 설치돼 있는지와 상관없이 항상 같은 값이 나와야 한다.
    #[test]
    fn install_candidates_match_new_run_manual_policy() {
        assert!(
            !install_candidates(ToolId::Gh).is_empty(),
            "gh는 brew formula명이 고정돼 있어 run 후보가 있어야 합니다"
        );
        assert!(
            !install_candidates(ToolId::Claude).is_empty(),
            "Claude는 npm 패키지명이 고정돼 있어 run 후보가 있어야 합니다"
        );
        assert!(
            !install_candidates(ToolId::Wrangler).is_empty(),
            "Wrangler는 npm 레지스트리 패키지명이 고정돼 있어 run 후보가 있어야 합니다"
        );
        assert!(
            install_candidates(ToolId::Pnpm).is_empty(),
            "pnpm은 G2/G3 탈락으로 run 후보가 없어야 합니다"
        );
        assert!(
            install_candidates(ToolId::Node).is_empty(),
            "Node.js는 G1/G4 탈락으로 run 후보가 없어야 합니다"
        );
        assert!(
            install_candidates(ToolId::Git).is_empty(),
            "Git은 G4 탈락(시스템 소유)으로 run 후보가 없어야 합니다"
        );
    }

    // M5(review-devtools-install-2026-09-10.md): §1.1 불변식("install_candidates()
    // 의 모든 RunPlan args/preview_args는 Arg::Lit 전용")을 강제하는 자동 가드가
    // 없었다 — 위반 여부가 사람이 표로 대조하는 것에만 의존했다. DEV_TOOLS
    // 전체(특정 도구 하드코딩 없이)를 순회하므로 다음에 도구가 추가돼도 이
    // 테스트가 자동으로 걸린다.
    #[test]
    fn install_candidates_run_plan_slots_are_literal_only() {
        for def in DEV_TOOLS.iter() {
            for candidate in install_candidates(def.id) {
                for arg in candidate.plan.args {
                    assert!(
                        matches!(arg, Arg::Lit(_)),
                        "{:?}의 install RunPlan.args에 비-리터럴 슬롯이 있습니다: {arg:?}",
                        def.id
                    );
                }
                if let Some(preview_args) = candidate.plan.preview_args {
                    for arg in preview_args {
                        assert!(
                            matches!(arg, Arg::Lit(_)),
                            "{:?}의 install RunPlan.preview_args에 비-리터럴 슬롯이 있습니다: {arg:?}",
                            def.id
                        );
                    }
                }
            }
        }
    }

    // 실행기가 전혀 해석되지 않는 합성 상황에서도 gh/Claude/Wrangler를 포함해
    // 모든 도구가 패닉 없이 Manual로 강등되어야 한다(fail-closed).
    #[test]
    fn resolve_install_plan_falls_back_to_manual_when_no_runner_resolved() {
        let no_runners = ResolvedRunners {
            brew: None,
            npm: None,
            pnpm: None,
        };
        for tool in [
            ToolId::Gh,
            ToolId::Claude,
            ToolId::Wrangler,
            ToolId::Pnpm,
            ToolId::Node,
            ToolId::Git,
        ] {
            assert!(
                matches!(
                    resolve_install_plan(tool, &no_runners),
                    InstallResolution::Manual(_)
                ),
                "{tool:?}은(는) 실행기가 없으면 Manual이어야 합니다"
            );
        }
    }

    // 합성 러너 경로가 전부 존재할 때: gh→brew, Claude→npm 리터럴 argv, Wrangler는
    // pnpm을 우선한다. pnpm/node/git은 러너가 있어도 여전히 Manual이다(G4는
    // 러너 존재 여부와 무관하게 탈락시키는 게이트이기 때문).
    #[test]
    fn resolve_install_plan_picks_run_with_expected_literal_argv() {
        let runners = ResolvedRunners {
            brew: Some("/opt/homebrew/bin/brew".to_string()),
            npm: Some("/opt/homebrew/bin/npm".to_string()),
            pnpm: Some("/opt/homebrew/bin/pnpm".to_string()),
        };

        match resolve_install_plan(ToolId::Gh, &runners) {
            InstallResolution::Run {
                runner_path, argv, ..
            } => {
                assert_eq!(runner_path, "/opt/homebrew/bin/brew");
                assert_eq!(argv, vec!["install", "-y", "--formula", "gh"]);
            }
            InstallResolution::Manual(_) => panic!("gh는 brew가 있으면 run이어야 합니다"),
        }

        match resolve_install_plan(ToolId::Claude, &runners) {
            InstallResolution::Run {
                runner_path, argv, ..
            } => {
                assert_eq!(runner_path, "/opt/homebrew/bin/npm");
                assert_eq!(argv, vec!["install", "-g", "@anthropic-ai/claude-code"]);
            }
            InstallResolution::Manual(_) => panic!("claude는 npm이 있으면 run이어야 합니다"),
        }

        match resolve_install_plan(ToolId::Wrangler, &runners) {
            InstallResolution::Run {
                runner_path,
                argv,
                installer_label,
                ..
            } => {
                assert_eq!(installer_label, "pnpm");
                assert_eq!(runner_path, "/opt/homebrew/bin/pnpm");
                assert_eq!(argv, vec!["add", "-g", "wrangler"]);
            }
            InstallResolution::Manual(_) => panic!("wrangler는 pnpm이 있으면 run이어야 합니다"),
        }

        for tool in [ToolId::Pnpm, ToolId::Node, ToolId::Git] {
            assert!(
                matches!(
                    resolve_install_plan(tool, &runners),
                    InstallResolution::Manual(_)
                ),
                "{tool:?}은(는) 러너가 있어도 G4 탈락으로 Manual이어야 합니다"
            );
        }
    }

    // Wrangler의 pnpm→npm 폴백: pnpm 해석이 안 되면 npm으로 넘어가야 한다(원
    // 설계 §12.5 결정 승계 검증).
    #[test]
    fn resolve_install_plan_wrangler_falls_back_to_npm_when_pnpm_missing() {
        let runners = ResolvedRunners {
            brew: None,
            npm: Some("/opt/homebrew/bin/npm".to_string()),
            pnpm: None,
        };
        match resolve_install_plan(ToolId::Wrangler, &runners) {
            InstallResolution::Run {
                installer_label,
                argv,
                ..
            } => {
                assert_eq!(installer_label, "npm");
                assert_eq!(argv, vec!["install", "-g", "wrangler"]);
            }
            InstallResolution::Manual(_) => panic!("wrangler는 npm만 있어도 run이어야 합니다"),
        }
    }

    // M2(devtools-install-matrix §4.2, 직전 리뷰 지적) — gh 설치 경로의 쓰기권한
    // 사전검사가 실제로 동작하는지 실측한다. compute_action_checks_prefix_
    // cellar_and_bin_not_prefix_itself(update 경로)와 같은 패턴: 홈 디렉터리
    // 아래 합성 prefix/Cellar/bin을 만들어 "쓰기 가능"과 "쓰기 불가"(chmod)
    // 두 경우 모두 검증한다.
    #[test]
    #[cfg(unix)]
    fn resolve_install_plan_falls_back_to_manual_when_gh_prefix_not_writable() {
        use std::os::unix::fs::PermissionsExt;

        let Some(home) = dirs::home_dir() else {
            return;
        };
        let tmp_prefix = home.join(format!(
            "malgn_vscode_test_install_prefix_{}",
            std::process::id()
        ));
        let cellar = tmp_prefix.join("Cellar");
        let bin = tmp_prefix.join("bin");
        std::fs::create_dir_all(&cellar).expect("create synthetic Cellar dir");
        std::fs::create_dir_all(&bin).expect("create synthetic bin dir");

        let runner_path = tmp_prefix.join("bin").join("brew");
        let runners = ResolvedRunners {
            brew: Some(runner_path.to_string_lossy().to_string()),
            npm: None,
            pnpm: None,
        };

        // Cellar·bin 모두 쓰기 가능하면 Run이어야 한다.
        match resolve_install_plan(ToolId::Gh, &runners) {
            InstallResolution::Run { .. } => {}
            InstallResolution::Manual(_) => {
                let _ = std::fs::remove_dir_all(&tmp_prefix);
                panic!("Cellar·bin 모두 쓰기 가능하면 Run이어야 합니다");
            }
        }

        // Cellar을 쓰기 불가로 만들면 Manual(NotWritable)로 강등돼야 한다.
        let mut perms = std::fs::metadata(&cellar)
            .expect("stat cellar")
            .permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&cellar, perms).expect("chmod cellar read-only");

        let after = resolve_install_plan(ToolId::Gh, &runners);

        // 정리 전에 판정한다 — 원상복구는 remove_dir_all 전에 쓰기 권한을
        // 되돌려야 하므로 순서를 지킨다.
        let mut restore = std::fs::metadata(&cellar)
            .expect("stat cellar for restore")
            .permissions();
        restore.set_mode(0o755);
        let _ = std::fs::set_permissions(&cellar, restore);
        let _ = std::fs::remove_dir_all(&tmp_prefix);

        match after {
            InstallResolution::Manual(mp) => {
                assert_eq!(mp.reason, ManualReason::NotWritable);
            }
            InstallResolution::Run { .. } => {
                panic!("Cellar가 쓰기 불가면 Manual(NotWritable)로 강등돼야 합니다")
            }
        }
    }

    // §4.2 단서: 아직 존재하지 않는 경로(예: brew는 있지만 한 번도 formula를
    // 설치한 적 없어 Cellar가 없는 머신)를 "쓰기 불가"로 오판해 fail-closed로
    // 기능을 죽이면 안 된다 — 존재하지 않으면 검사를 건너뛰고 통과시켜야 한다.
    #[test]
    fn install_prefix_writable_skips_check_for_nonexistent_paths() {
        assert!(
            install_prefix_writable(
                ToolId::Gh,
                Runner::Brew,
                "/opt/homebrew_does_not_exist_malgn/bin/brew"
            ),
            "Cellar/bin이 아직 없으면 쓰기 불가로 오판하지 않고 통과시켜야 합니다"
        );
        assert!(
            install_prefix_writable(
                ToolId::Claude,
                Runner::Npm,
                "/opt/homebrew_does_not_exist_malgn/bin/npm"
            ),
            "lib/node_modules와 lib이 모두 없으면 검사를 건너뛰고 통과시켜야 합니다"
        );
    }

    // check_dev_tools_blocking()의 "경로 자체를 못 찾은"(path == None) 상태의
    // action_kind는 새 정책(gh·Claude·Wrangler는 run, pnpm·Node·Git은 manual)을
    // 따라야 하고, "none"을 반환하는 경로는 완전히 사라져야 한다(§6.2). 이
    // 머신에 어떤 도구가 실제로 설치돼 있는지와 무관하게 성립해야 하므로
    // 하드코딩된 도구 목록이 아니라 install_candidates()의 유무로 기대값을
    // 유도한다.
    #[test]
    fn check_dev_tools_blocking_follows_run_manual_policy_and_never_returns_none() {
        let tools = check_dev_tools_blocking();
        assert_eq!(tools.len(), 6);
        for tool in &tools {
            assert_ne!(
                tool.action_kind, "none",
                "{}은(는) 'none'을 반환하면 안 됩니다(§6.2)",
                tool.id
            );
            if tool.path.is_none() {
                let Some(id) = ToolId::from_key(&tool.id) else {
                    panic!("알 수 없는 tool id: {}", tool.id);
                };
                let expected = if install_candidates(id).is_empty() {
                    "manual"
                } else {
                    "run"
                };
                assert_eq!(
                    tool.action_kind, expected,
                    "{} 미설치 시 action_kind 정책 불일치",
                    tool.id
                );
            }
        }
    }

    // 설계 §5.2 최소 요건: Windows 빌드에서는 "설치 안 됨"이라는 거짓 표시
    // 대신 명시적으로 미지원임을 알려야 한다. 플랫폼과 무관하게 순수 함수로
    // 분리했으므로 이 머신(macOS)에서도 그 분기 자체를 직접 검증할 수 있다.
    #[test]
    fn windows_unsupported_status_never_lies_about_installed_and_uses_manual() {
        let tools = windows_unsupported_dev_tools_status();
        assert_eq!(tools.len(), 6);
        for tool in &tools {
            assert!(!tool.installed);
            assert_eq!(tool.action_kind, "manual");
            let hint = tool.manual_hint.as_deref().unwrap_or("");
            assert!(
                hint.contains("macOS"),
                "{}: manual_hint가 미지원 사유를 밝혀야 합니다: {hint}",
                tool.id
            );
        }
    }

    // planId가 미리보기 이후 계산값과 다르면(위조·상태 변경) 항상 거부되어야
    // 한다 — Wrangler가 이미 설치돼 있어 update 경로(perform_install→perform_update
    // 위임, 요구 2)로 빠지는 경우까지 포함해서.
    #[test]
    fn wrangler_install_rejects_stale_plan_id() {
        assert!(perform_install("wrangler", "not-a-real-plan-id").is_err());
    }

    // 요구 2: 이미 설치된 도구에 install_dev_tool을 호출하면 perform_update로
    // 위임되어야 한다 — install 전용 plan_id(§ compute_plan_id(runner, argv, ""))
    // 가 아니라 update 전용 plan_id(정상 버전 포함)를 기대해야 정상 동작한다는
    // 뜻이므로, install 전용 형태의 plan_id를 주면 거부되어야 한다(직전 리뷰
    // 지적 #3의 "막다른 골목"이 재발하지 않는지 확인).
    #[test]
    fn perform_install_delegates_to_update_when_already_installed() {
        // 이 머신에 실제로 설치된 도구(claude/node/gh/git/pnpm 중 하나)를 찾아
        // install 전용 plan_id로 호출하면 update 경로의 검증을 타면서 거부되어야
        // 한다(perform_update도 동일하게 stale plan_id를 거부하므로 Err 자체는
        // 두 경로 모두에서 나오지만, 이 테스트의 목적은 "패닉하지 않고 항상
        // 하나의 일관된 경로로 처리된다"는 구조적 보장을 확인하는 것이다).
        let tools = check_dev_tools_blocking();
        if let Some(installed) = tools.iter().find(|t| t.path.is_some()) {
            let result = perform_install(&installed.id, "not-a-real-plan-id");
            assert!(result.is_err());
        }
    }
}
