// ==================== 4. (도구 × 설치방식) → argv 테이블(결정 2) ====================
// "어떤 도구가 어떤 설치방식으로 잡혔을 때 무엇을 실행할지"의 정적 결정표
// (UPDATE_TABLE)와 그 조회(lookup_action) + 실행 직전 동적 가드(compute_action —
// corepack 관리 감지, brew prefix 쓰기권한 사전검사)를 담는다. 미설치 도구 전용
// 판별(`install_resolver`의 설치 후보 테이블)이 이 테이블의 결과(Action)와
// Manual 안내(ManualPlan)를 그대로 소비하므로, 그 타입들은 여기서 pub(crate)로
// 노출한다.

use super::classify::{InstallMethod, MethodKind};
use super::ToolId;
use std::path::Path;

/// 안내문 + (있으면) 공식 문서 링크를 한 문자열로 합친다. `ManualPlan.doc_url`은
/// 부록 C의 DevToolStatus/DevToolPreview에 별도 필드가 없어 message 안에 접어
/// 넣는다(계약을 바꾸지 않기 위한 선택 — 계약 변경이 필요하면 즉시 보고하라는
/// 지시에 따라 필드 추가 대신 이 방식을 택했다).
pub(crate) fn manual_display_message(plan: &ManualPlan) -> String {
    match plan.doc_url {
        Some(url) => format!("{} 참고: {url}", plan.message_ko),
        None => plan.message_ko.to_string(),
    }
}

// ==================== 4. (도구 × 설치방식) → argv 테이블(결정 2) ====================

#[derive(Debug, Clone, Copy)]
pub(crate) enum Arg {
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
pub(crate) enum Runner {
    Brew,
    Npm,
    // Wrangler 전용 실설치(§12.5)에서만 쓰인다 — pnpm을 "다른 패키지를 설치하는
    // 도구"로 실행한다. RUN_PNPM_SELF_UPDATE의 SelfBinary(자기 자신을 갱신)와는
    // 용도가 달라 구분되는 변형이 필요하다.
    Pnpm,
    SelfBinary,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RunPlan {
    pub(crate) runner: Runner,
    pub(crate) args: &'static [Arg],
    pub(crate) preview_args: Option<&'static [Arg]>,
    pub(crate) env: &'static [(&'static str, &'static str)],
    pub(crate) timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualReason {
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
pub(crate) struct ManualPlan {
    pub(crate) reason: ManualReason,
    pub(crate) message_ko: &'static str,
    pub(crate) copyable_command: Option<&'static str>,
    pub(crate) doc_url: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Action {
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
pub(crate) const RUN_BREW_FORMULA: RunPlan = RunPlan {
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
pub(crate) const RUN_PNPM_GLOBAL_UPDATE: RunPlan = RunPlan {
    runner: Runner::Pnpm,
    args: &[Arg::Lit("add"), Arg::Lit("-g"), Arg::PackageLatest],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
// Wrangler 전용 실설치(§12.5, devtools-install-matrix 승계). 미설치 도구
// 전반의 "run 가능 여부" 판정은 이제 install_candidates()/resolve_install_plan()
// 하나로 통일된다(install_resolver 모듈 참고) — Wrangler는 그 규칙 위에서
// npm 레지스트리 패키지명이 "wrangler"로 고정돼 있어 run 후보가 되는 사례일
// 뿐, 더 이상 코드상의 특별 취급이 아니다.
pub(crate) const WRANGLER_PACKAGE: &str = "wrangler";
pub(crate) const RUN_PNPM_GLOBAL_ADD: RunPlan = RunPlan {
    runner: Runner::Pnpm,
    args: &[Arg::Lit("add"), Arg::Lit("-g"), Arg::Lit(WRANGLER_PACKAGE)],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 300,
};
pub(crate) const RUN_NPM_GLOBAL_INSTALL_WRANGLER: RunPlan = RunPlan {
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
pub(crate) const RUN_BREW_INSTALL_GH: RunPlan = RunPlan {
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
pub(crate) const RUN_NPM_INSTALL_CLAUDE: RunPlan = RunPlan {
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
pub(crate) const MANUAL_XCODE_CLT: ManualPlan = ManualPlan {
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
pub(crate) const MANUAL_NOT_WRITABLE: ManualPlan = ManualPlan {
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
pub(crate) const MANUAL_NO_RUNNER: ManualPlan = ManualPlan {
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

pub(crate) fn lookup_action(tool_id: ToolId, kind: MethodKind) -> Action {
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
pub(crate) fn install_manual_plan(tool: ToolId) -> ManualPlan {
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
pub(crate) fn is_writable_by_current_user(path: &Path) -> bool {
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
pub(crate) fn is_writable_by_current_user(path: &Path) -> bool {
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
pub(crate) fn compute_action(tool_id: ToolId, method: &InstallMethod) -> Action {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn manual_plan_reasons_are_distinguishable_for_ui_branching() {
        assert_ne!(MANUAL_XCODE_CLT.reason, MANUAL_VERSION_MANAGED.reason);
        assert_ne!(MANUAL_COREPACK_MANAGED.reason, MANUAL_NOT_WRITABLE.reason);
    }
}
