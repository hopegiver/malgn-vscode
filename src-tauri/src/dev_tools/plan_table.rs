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
    // Windows 전용(설계 §B.1/§B.2) — winget으로 gh를 설치/업데이트한다. 이
    // 러너에는 install_prefix_writable이 검사할 "prefix" 개념이 없어(winget이
    // 스스로 설치 경로를 관리) 그 검사는 건너뛴다(runners.rs의 와일드카드
    // 매치가 자동으로 처리한다 — 새 분기가 필요 없다).
    Winget,
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
// ── winget(Windows 전용, 설계 §B) ──
// G-5(보안): `--source winget` 고정(msstore 배제) + `-e`(exact) + `--id` 고정
// 리터럴 + `--accept-*`/`--disable-interactivity`로 모든 프롬프트를 argv에서
// 차단. `--ignore-security-hash`는 이 코드베이스 어디에도 등장하지 않는다(오설치
// = 임의 코드 실행이므로 금지). G-4(보안): 앱은 `runas`/`ShellExecute` 승격을
// 스스로 호출하지 않는다 — 이 RunPlan들은 std::process::Command로만 spawn되고,
// winget 자신이 승격을 요구하면 OS가 사용자에게 묻는다(거부 시 non-zero 종료로
// 드러난다, B.3). 전부 `Arg::Lit`(§1.1 불변식 — 동적 argv 토큰 0개).
// 판단(2라운드 위임 사항 — `--scope user`를 고정할지): 고정하지 **않는다**.
// GitHub CLI의 공식 winget 매니페스트(`GitHub.cli`)는 MSI 인스톨러 기술을
// 쓴다 — winget에서 MSI 인스톨러는 통상 머신 스코프 전용이라(그 패키지
// 매니페스트에 별도 User 스코프 인스톨러 변형이 없는 한) `--scope user`를
// 강제하면 winget이 "No applicable installer found"로 항상 실패할 위험이
// 크다(이 머신에 winget이 없어 실측 불가 — winget 공식 문서의
// 스코프-인스톨러기술 매칭 규칙에 근거한 판단, §8 미검증 항목과 동일 계열).
// 그 실패를 Failed가 아니라 Manual로 강등하는 새 분기를 추가하는 비용 대비,
// scope를 아예 지정하지 않아 winget이 그 패키지가 지원하는 유일한 스코프를
// 스스로 고르게 하는 편이 더 단순하고 안전하다 — 관리자 승인(UAC) 프롬프트는
// 그대로 뜨지만(머신 스코프 설치이므로 예상된 동작), 그건 화면이 거짓을
// 말하는 문제가 아니라 OS 표준 동작이다(기존 Git 매뉴얼 안내 문구 "관리자
// 권한 승인 창이 뜰 수 있습니다"와 같은 전제).
pub(crate) const RUN_WINGET_INSTALL_GH: RunPlan = RunPlan {
    runner: Runner::Winget,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("--id"),
        Arg::Lit("GitHub.cli"),
        Arg::Lit("-e"),
        Arg::Lit("--source"),
        Arg::Lit("winget"),
        Arg::Lit("--accept-source-agreements"),
        Arg::Lit("--accept-package-agreements"),
        Arg::Lit("--disable-interactivity"),
        Arg::Lit("--silent"),
    ],
    // winget에는 `brew install -n`류 dry-run이 없다(B.3) — 프리뷰는
    // query::build_install_preview가 installer_label=="winget"일 때 별도
    // 안내(고정 preview_reliable:false)로 대체한다. 여기서 `winget show`를
    // 실제로 호출하지 않는 이유는 이 머신에 winget이 없어 그 실행 경로
    // 자체가 검증 불가능하기 때문이다(정직 규율) — preview_reliable:false가
    // 이미 "거짓 안심을 주지 않는다"는 B.3의 목표를 달성한다.
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 600,
};
pub(crate) const RUN_WINGET_UPGRADE_GH: RunPlan = RunPlan {
    runner: Runner::Winget,
    args: &[
        Arg::Lit("upgrade"),
        Arg::Lit("--id"),
        Arg::Lit("GitHub.cli"),
        Arg::Lit("-e"),
        Arg::Lit("--source"),
        Arg::Lit("winget"),
        Arg::Lit("--accept-source-agreements"),
        Arg::Lit("--disable-interactivity"),
        Arg::Lit("--silent"),
    ],
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 600,
};

/// winget이 "이미 최신"일 때 돌려주는 종료 코드(`0x8A15002B` =
/// `APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`, 부호있는 i32로는
/// `-1978335189`). 이걸 `Failed`로 두면 화면이 거짓을 말한다 — actions.rs의
/// `perform_update`가 이 값을 `Outcome::AlreadyLatest`로 매핑한다(B.3). 미검증
/// (이 머신에 winget이 없어 실측 불가, microsoft/winget-cli 문서 근거).
pub(crate) const WINGET_ALREADY_LATEST_EXIT_CODE: i32 = -1978335189;

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
    // 설계 §B.2: winget 슬롯 추가로 실행기 후보가 하나 더 늘었으므로 문구도
    // 함께 넓힌다(새 분기를 만들지 않고 기존 강등 경로를 그대로 재사용 — 해석
    // 실패 시 이 상수로 자동 강등된다).
    message_ko: "필요한 실행 도구(brew/npm/winget)를 찾을 수 없어 앱이 자동으로 실행하지 않습니다.",
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
    // Windows 전용(설계 §B.4): winget으로 설치된 gh만 이 행에 매칭된다 —
    // 도구별 행(tool: Some(..))이라 제네릭 행보다 먼저 검사된다.
    Row {
        tool: Some(ToolId::Gh),
        method: MethodKind::WingetPackage,
        action: Action::Run(RUN_WINGET_UPGRADE_GH),
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
    match super::platform::platform_now() {
        super::platform::Platform::Win => install_manual_plan_windows(tool),
        super::platform::Platform::Mac => install_manual_plan_mac(tool),
    }
}

/// 기존 내용 그대로(함수명만 분리) — macOS 안내 문구를 Windows에 그대로
/// 보여주는 것(brew 명령 등)은 이 작업 전체가 고치려는 바로 그 종류의 거짓
/// 표시이므로, 플랫폼별로 분리한다.
fn install_manual_plan_mac(tool: ToolId) -> ManualPlan {
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

/// Windows 안내(설계 §B.1 매뉴얼 가이드 열 — 전부 미검증, 실기 Windows PC
/// 필요). Claude/Wrangler의 `npm install -g ...` 명령은 크로스플랫폼으로
/// PowerShell에서도 그대로 유효해 mac과 동일한 copyable_command를 쓴다.
fn install_manual_plan_windows(tool: ToolId) -> ManualPlan {
    match tool {
        ToolId::Claude => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Claude Code가 설치되어 있지 않습니다. npm을 찾을 수 없다면 Node.js를 먼저 설치해주세요(winget install OpenJS.NodeJS.LTS).",
            copyable_command: Some("npm install -g @anthropic-ai/claude-code"),
            doc_url: Some("https://docs.claude.com/en/docs/claude-code/setup"),
        },
        ToolId::Node => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Node.js가 설치되어 있지 않습니다. 이미 nvm-windows/fnm/volta 등으로 관리 중이라면 그쪽에서 설치해주세요. 그렇지 않다면 아래 명령으로 설치할 수 있습니다(winget).",
            copyable_command: Some("winget install OpenJS.NodeJS.LTS"),
            doc_url: Some("https://nodejs.org/en/download"),
        },
        ToolId::Gh => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "GitHub CLI가 설치되어 있지 않거나 winget(앱 설치 관리자)을 찾을 수 없습니다. Microsoft Store에서 '앱 설치 관리자'를 업데이트한 뒤 아래 명령을 시도해주세요.",
            copyable_command: Some(
                "winget install --id GitHub.cli -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent",
            ),
            doc_url: Some("https://cli.github.com/"),
        },
        ToolId::Git => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Git이 설치되어 있지 않습니다. 관리자 권한 승인 창이 뜰 수 있습니다(머신 스코프 설치).",
            copyable_command: Some("winget install --id Git.Git -e --source winget"),
            doc_url: Some("https://git-scm.com/downloads"),
        },
        ToolId::Pnpm => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "pnpm이 설치되어 있지 않습니다. 아래 명령으로 설치할 수 있습니다(winget). 설치 후 새 터미널을 열어야 PATH가 반영됩니다.",
            copyable_command: Some("winget install pnpm.pnpm"),
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

    // 설계 §B: install_manual_plan()이 이 머신(Mac)에서는 기존 그대로 brew
    // 문구를 낸다는 계약(무변경) — install_manual_plan_mac을 직접 호출해
    // platform_now()의 실제 플랫폼과 무관하게 회귀를 잡는다.
    #[test]
    fn install_manual_plan_mac_variant_keeps_brew_wording() {
        let gh = install_manual_plan_mac(ToolId::Gh);
        assert_eq!(gh.copyable_command, Some("brew install gh"));
        let node = install_manual_plan_mac(ToolId::Node);
        assert_eq!(node.copyable_command, Some("brew install node"));
    }

    // Windows 분기(설계 §B.1, 전부 미검증)는 brew가 아니라 winget 명령을
    // 내야 한다 — mac 문구가 그대로 새어나가면 이 작업의 핵심 결함(거짓 안내)이
    // 재발한 것이다.
    #[test]
    fn install_manual_plan_windows_variant_uses_winget_not_brew() {
        let gh = install_manual_plan_windows(ToolId::Gh);
        assert!(gh
            .copyable_command
            .unwrap_or("")
            .starts_with("winget install --id GitHub.cli"));
        assert!(!gh.copyable_command.unwrap_or("").contains("brew"));

        let node = install_manual_plan_windows(ToolId::Node);
        assert_eq!(node.copyable_command, Some("winget install OpenJS.NodeJS.LTS"));
        assert!(!node.message_ko.contains("Homebrew"));

        let git = install_manual_plan_windows(ToolId::Git);
        assert_eq!(
            git.copyable_command,
            Some("winget install --id Git.Git -e --source winget")
        );

        let pnpm = install_manual_plan_windows(ToolId::Pnpm);
        assert_eq!(pnpm.copyable_command, Some("winget install pnpm.pnpm"));

        // npm 계열 명령은 크로스플랫폼이라 mac과 동일 값을 재사용해도 된다.
        let claude = install_manual_plan_windows(ToolId::Claude);
        assert_eq!(
            claude.copyable_command,
            Some("npm install -g @anthropic-ai/claude-code")
        );
        let wrangler = install_manual_plan_windows(ToolId::Wrangler);
        assert_eq!(wrangler.copyable_command, Some("npm install -g wrangler"));
    }

    // 설계 §B.3: winget "이미 최신" 종료 코드 상수가 실제 문서값(0x8A15002B)과
    // 일치하는지 고정한다 — actions.rs가 이 값을 Outcome::AlreadyLatest로
    // 매핑하므로 상수가 틀리면 그 매핑 자체가 조용히 무력화된다.
    #[test]
    fn winget_already_latest_exit_code_matches_documented_hresult() {
        assert_eq!(WINGET_ALREADY_LATEST_EXIT_CODE, -1978335189);
        assert_eq!(WINGET_ALREADY_LATEST_EXIT_CODE as u32, 0x8A15002B);
    }

    // 설계 §B.4: (Gh, WingetPackage) 조합이 UPDATE_TABLE에서 Run으로 매칭되고,
    // 다른 도구는 이 method로 매칭되지 않아야 한다(도구별 행이 제네릭 행보다
    // 우선 매칭되는 기존 lookup_action 규칙과 동일).
    #[test]
    fn winget_package_method_maps_to_run_only_for_gh() {
        assert!(matches!(
            lookup_action(ToolId::Gh, MethodKind::WingetPackage),
            Action::Run(plan) if plan.runner == Runner::Winget
        ));
    }

    // G-5(보안): winget RunPlan 어디에도 `--ignore-security-hash`가 없어야
    // 한다(오설치=임의 코드 실행 위험) — 문자열 검색으로 고정한다.
    #[test]
    fn winget_run_plans_never_include_ignore_security_hash_or_msstore_source() {
        for plan in [RUN_WINGET_INSTALL_GH, RUN_WINGET_UPGRADE_GH] {
            for arg in plan.args {
                if let Arg::Lit(s) = arg {
                    assert_ne!(*s, "--ignore-security-hash");
                    assert_ne!(*s, "msstore");
                }
            }
            // --source가 있다면 반드시 다음 리터럴이 "winget"이어야 한다.
            let lits: Vec<&str> = plan
                .args
                .iter()
                .filter_map(|a| match a {
                    Arg::Lit(s) => Some(*s),
                    _ => None,
                })
                .collect();
            if let Some(idx) = lits.iter().position(|s| *s == "--source") {
                assert_eq!(lits.get(idx + 1), Some(&"winget"));
            }
        }
    }

    // N3(2라운드 비차단): 위 테스트는 `[RUN_WINGET_INSTALL_GH, RUN_WINGET_UPGRADE_GH]`를
    // 손으로 나열한다 — 나중에 winget 행이 하나 더 추가되고 이 배열에 빠뜨려도
    // 이 테스트는 여전히 통과한다(가드가 무의미해진다). 이 테스트는 UPDATE_TABLE
    // (여기, 내부 테이블)과 install_candidates()(install_resolver.rs, 도구
    // 하드코딩 없이 DEV_TOOLS 전체를 순회 — `install_candidates_run_plan_slots_are_
    // literal_only`와 동일 패턴)를 **구조적으로** 순회해 Runner::Winget인 모든
    // RunPlan을 자동으로 모은다. 새 winget 행이 어느 테이블에 추가되든 이
    // 가드가 자동으로 걸린다.
    #[test]
    fn all_winget_run_plans_in_install_and_update_tables_pass_security_gate() {
        use super::super::install_resolver::install_candidates;
        use super::super::DEV_TOOLS;

        let mut winget_plans: Vec<RunPlan> = Vec::new();

        // UPDATE_TABLE(이 파일의 내부 정적 테이블) 전체를 순회한다.
        for row in UPDATE_TABLE {
            if let Action::Run(plan) = row.action {
                if plan.runner == Runner::Winget {
                    winget_plans.push(plan);
                }
            }
        }
        // install_candidates()(도구별 설치 후보 테이블)도 DEV_TOOLS 전체를
        // 순회해 하드코딩 없이 모은다.
        for def in DEV_TOOLS.iter() {
            for candidate in install_candidates(def.id) {
                if candidate.plan.runner == Runner::Winget {
                    winget_plans.push(candidate.plan);
                }
            }
        }

        assert!(
            !winget_plans.is_empty(),
            "winget RunPlan이 하나도 발견되지 않았습니다 — 이 가드가 무의미해집니다"
        );

        for plan in winget_plans {
            // ③ 전부 Arg::Lit.
            let lits: Vec<&str> = plan
                .args
                .iter()
                .filter_map(|a| match a {
                    Arg::Lit(s) => Some(*s),
                    _ => None,
                })
                .collect();
            assert_eq!(
                lits.len(),
                plan.args.len(),
                "winget RunPlan.args는 전부 Arg::Lit이어야 합니다: {:?}",
                plan.args
            );

            // ② --ignore-security-hash / --scope machine 부재(scope 자체를
            // 아예 지정하지 않는 현재 판단 — 위 주석 참고. 그래도 향후 실수로
            // "--scope machine"이 추가되는 것은 막는다).
            assert!(
                !lits.contains(&"--ignore-security-hash"),
                "winget RunPlan에 --ignore-security-hash가 있으면 안 됩니다(오설치=임의 코드 실행)"
            );
            assert!(
                !lits.contains(&"msstore"),
                "winget 소스가 msstore이면 안 됩니다"
            );
            if let Some(idx) = lits.iter().position(|s| *s == "--scope") {
                assert_ne!(
                    lits.get(idx + 1),
                    Some(&"machine"),
                    "--scope machine을 명시적으로 고정하면 안 됩니다(UAC 승격을 앱이 유도하는 모양이 된다)"
                );
            }

            // ① --source winget 존재(있다면 다음 리터럴이 반드시 "winget").
            let source_idx = lits.iter().position(|s| *s == "--source");
            assert_eq!(
                source_idx.and_then(|i| lits.get(i + 1)),
                Some(&"winget"),
                "winget RunPlan은 --source winget을 고정해야 합니다: {lits:?}"
            );
        }
    }
}
