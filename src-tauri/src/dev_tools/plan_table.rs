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
    // M1(review-devtools-windows-parity-2026-09-15.md) 재발 방지: 설치방식은
    // 정확히 분류됐지만(Unknown이 아님) 이 (도구, 방식) 조합에 자동 실행을
    // 의도적으로 제공하지 않는 경우. 일반 UnknownMethod(=분류 자체가
    // 실패했다는 뜻)와 의미가 다르므로 별도 사유로 구분한다.
    UnsupportedMethod,
    // N1(review-devtools-windows-parity-2026-09-15-r2.md) 재발 방지: Windows에서
    // SystemManaged로 분류되는 경로(Program Files\Git, \nodejs 등 — 설치기가
    // 시스템 전역에 심는 자리)는 macOS의 Xcode CLT와 근본적으로 다른 사실이다.
    // UPDATE_TABLE의 SystemManaged 행 하나(MANUAL_XCODE_CLT)를 그대로 보여주면
    // 존재하지 않는 OS 안내가 된다 — 별도 사유로 구분해 두 문구가 섞이지 않게 한다.
    WindowsSystemManaged,
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
// pnpm 전용 실설치(install_resolver.rs 상단 G1~G4 표 주석 참조): pnpm의 공식
// 설치기는 대화형 셸 스크립트 + rc 파일 수정이라 G2/G3 탈락으로 manual이다.
// 그런데 Node/npm이 이미 있으면 `npm install -g pnpm`은 비대화형·rc파일
// 무수정으로 설치되어(npm 레지스트리 패키지명 "pnpm" 고정 — G1, argv 배열로
// 그대로 실행 — G2, 결과가 기존 NpmGlobal 판별기 자리에 놓임 — G3) 그 위험을
// 피해 간다. corepack
// (`corepack enable pnpm`)은 의도적으로 후보에 넣지 않는다 — Windows에서
// Node가 `Program Files\nodejs`에 있으면 corepack이 그 디렉터리에 shim을
// 만들어야 해 관리자 권한이 필요하고, 비관리자로 뜬 GUI 자식 프로세스가
// 거기서 막힌다. `npm install -g pnpm`은 `%APPDATA%\npm`(사용자 쓰기 가능)에
// 설치되어 그 문제가 없다. 이 예외는 pnpm에만 적용한다 — Node/Git은 여전히
// manual이다(그 둘은 진짜 위험한 설치기뿐이라 이 우회가 없다).
pub(crate) const PNPM_PACKAGE: &str = "pnpm";
pub(crate) const RUN_NPM_GLOBAL_INSTALL_PNPM: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[Arg::Lit("install"), Arg::Lit("-g"), Arg::Lit(PNPM_PACKAGE)],
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
    // query::no_dry_run_install_notes가 이 RunPlan의 runner(Runner::Winget)를
    // winget_preview_is_reliable로 판정해 별도 안내(고정
    // preview_reliable:false)로 대체한다(T2①, review-devtools-windows-
    // parity-2026-09-15-r3.md — 예전엔 installer_label 문자열 비교였다).
    // 여기서 `winget show`를
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

// devtools-install-matrix §1.2/§2.1 결정 뒤집기(hub decisionId
// 01m2wse823xcszvn7km3vap0qs): Node/Git은 원래 "공식 설치기가 대화형 + rc
// 파일 수정이라 위험"해서 G2/G3 탈락으로 manual이었다 — 그 근거는 winget
// 경로에는 적용되지 않는다(비대화형, rc 파일 무수정). 패키지 id는
// MANUAL_WINDOWS_SYSTEM_MANAGED_NODE/GIT(위 §업데이트 경로)이 이미 쓰는 값과
// 동일하게 맞춘다(OpenJS.NodeJS.LTS, Git.Git — 새 id를 지어내지 않는다).
// RUN_WINGET_INSTALL_GH와 인자 구조가 완전히 동일하다(G-5 보안: `--id` 고정
// 리터럴 + `-e`(exact) + `--source winget` 고정(msstore 배제) +
// `--accept-*`/`--disable-interactivity`로 모든 프롬프트를 argv에서 차단,
// `--silent`). Windows 전용(macOS는 Runner::Winget이 구조적으로 항상
// 미해석이라 이 두 RunPlan은 macOS에서 절대 선택되지 않는다 — runners.rs
// 상단 주석 참고, mac은 기존 xcode-select/brew 안내 문구가 무변경이다).
pub(crate) const RUN_WINGET_INSTALL_NODE: RunPlan = RunPlan {
    runner: Runner::Winget,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("--id"),
        Arg::Lit("OpenJS.NodeJS.LTS"),
        Arg::Lit("-e"),
        Arg::Lit("--source"),
        Arg::Lit("winget"),
        Arg::Lit("--accept-source-agreements"),
        Arg::Lit("--accept-package-agreements"),
        Arg::Lit("--disable-interactivity"),
        Arg::Lit("--silent"),
    ],
    // winget에는 dry-run이 없다(B.3, RUN_WINGET_INSTALL_GH와 동일 근거) —
    // query::no_dry_run_install_notes가 Runner::Winget을 보고 자동으로
    // preview_reliable:false로 처리한다(별도 분기 불필요).
    preview_args: None,
    env: &COMMON_ENV,
    timeout_secs: 600,
};
pub(crate) const RUN_WINGET_INSTALL_GIT: RunPlan = RunPlan {
    runner: Runner::Winget,
    args: &[
        Arg::Lit("install"),
        Arg::Lit("--id"),
        Arg::Lit("Git.Git"),
        Arg::Lit("-e"),
        Arg::Lit("--source"),
        Arg::Lit("winget"),
        Arg::Lit("--accept-source-agreements"),
        Arg::Lit("--accept-package-agreements"),
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

/// Node/Git 자동설치(hub decisionId 01m2wse823xcszvn7km3vap0qs) 도입 배경:
/// `winget install`은 UAC 프롬프트를 띄운다 — 그 자체는 문제가 아니다(Windows
/// 표준 설치 경험). 문제는 사용자가 취소하거나 응답하지 않을 때 앱이 "실행이
/// 실패했습니다(알 수 없는 코드)"처럼 원인을 숨기는 것이다. UAC 프롬프트를
/// 거부하면 Windows는 HRESULT `0x800704C7`(Win32 `ERROR_CANCELLED`, "The
/// operation was canceled by the user.")을 돌려준다 — 이 HRESULT는 Microsoft
/// 공식 트러블슈팅 문서(learn.microsoft.com Q&A, winget 설치 실패 스레드)에
/// "취소됨"으로 명시돼 있어 WINGET_ALREADY_LATEST_EXIT_CODE와 같은 근거
/// 강도로 고정한다(이 머신에 winget이 없어 실측 자체는 불가 — 문서 근거).
/// 부호있는 i32 변환: `0x800704C7` → `-2147023673`. 이 코드가 아닌 다른 실패는
/// 여기서 추측으로 새 분기를 만들지 않는다 — 기존 일반 Failed 분기가 원본
/// 종료코드를 그대로 메시지에 담아 보존한다("사실만 말하고 원문을 덧붙인다").
pub(crate) const WINGET_USER_CANCELLED_EXIT_CODE: i32 = -2147023673;

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
// N1(review-devtools-windows-parity-2026-09-15-r2.md): classify_install_method_windows
// 5번 분기(SystemManaged)에 실제로 도달하는 도구는 Git과 Node뿐이다(Program
// Files\Git\, Program Files (x86)\Git\, Program Files\nodejs\). 각 도구별
// winget 패키지 id는 이미 install_manual_plan_windows(아래)가 쓰는 값과
// 동일하게 맞춘다(Git.Git, OpenJS.NodeJS.LTS) — 새 id를 지어내지 않는다.
// copyable_command는 사용자가 터미널에서 직접 실행하는 안내일 뿐 앱이 자동
// 실행하지 않는다(Manual) — 실행 경계를 넓히는 구조적 변경이 아니다.
const MANUAL_WINDOWS_SYSTEM_MANAGED_GIT: ManualPlan = ManualPlan {
    reason: ManualReason::WindowsSystemManaged,
    message_ko: "Git이 시스템 설치 경로(Program Files)에 설치되어 있습니다. winget으로 설치했다면 아래 명령으로 업데이트하거나, Git 공식 설치 프로그램을 다시 실행해주세요.",
    copyable_command: Some("winget upgrade --id Git.Git -e --source winget"),
    doc_url: Some("https://git-scm.com/downloads"),
};
const MANUAL_WINDOWS_SYSTEM_MANAGED_NODE: ManualPlan = ManualPlan {
    reason: ManualReason::WindowsSystemManaged,
    message_ko: "Node.js가 시스템 설치 경로(Program Files)에 설치되어 있습니다. winget으로 설치했다면 아래 명령으로 업데이트하거나, Node.js 공식 설치 프로그램을 다시 실행해주세요.",
    copyable_command: Some("winget upgrade --id OpenJS.NodeJS.LTS -e --source winget"),
    doc_url: Some("https://nodejs.org/en/download"),
};
// 현재 classify_install_method_windows는 Git/Node 외 도구를 SystemManaged로
// 분류하지 않는다 — 그래도 분류기가 바뀌어 다른 도구가 이 분기로 들어오는
// 경우를 대비해 안전한 일반 문구를 둔다(§4 완결성, 잘못된 상태를 표현하지
// 않는다 — macOS 문구가 새어나가는 대신 이 문구가 나간다).
const MANUAL_WINDOWS_SYSTEM_MANAGED_GENERIC: ManualPlan = ManualPlan {
    reason: ManualReason::WindowsSystemManaged,
    message_ko: "이 도구는 시스템 설치 경로에 설치되어 있어 앱이 자동으로 업데이트하지 않습니다. 설치 프로그램에서 직접 업데이트해주세요.",
    copyable_command: None,
    doc_url: None,
};

/// N1 재발 방지: Windows에서 SystemManaged로 분류된 도구에 macOS 전용
/// MANUAL_XCODE_CLT 대신 실제 사실(설치기 관리 경로)을 안내하는 플랫폼별
/// ManualPlan을 고른다. `compute_action`이 lookup_action보다 먼저 이 함수로
/// 분기한다(UPDATE_TABLE의 SystemManaged 행은 macOS 전용으로 남긴다).
fn windows_system_managed_plan(tool_id: ToolId) -> ManualPlan {
    match tool_id {
        ToolId::Git => MANUAL_WINDOWS_SYSTEM_MANAGED_GIT,
        ToolId::Node => MANUAL_WINDOWS_SYSTEM_MANAGED_NODE,
        _ => MANUAL_WINDOWS_SYSTEM_MANAGED_GENERIC,
    }
}
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
    // 3차 리뷰(review-devtools-windows-parity-2026-09-15-r3.md) "system_prefixes
    // 확장 보류" 판정이 붙인 조건: 분류 규칙(classify.rs)은 실기 1호 PC 검증
    // 전에 추측으로 넓히지 않되(오분류 시 잘못된 러너로 실행되는 피해가 더
    // 크다), 다음 행동이 0개인 막다른 골목만은 해소한다. 실제 설치 경로를
    // 사용자가 보고할 자리를 주면 그 실측이 나중에 system_prefixes를 안전하게
    // (추측이 아니라 관측으로) 넓힐 근거가 된다.
    doc_url: Some("https://github.com/hopegiver/malgn-vscode/issues/new"),
};
// M1(review-devtools-windows-parity-2026-09-15.md): mod.rs DEV_TOOLS의 Claude
// Windows 후보 3번째(`%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe`)가
// 실제로 존재하면 WingetPackage로 분류된다. 설계 §B.2 "대안 A" 기각 근거대로
// Claude Code는 winget upgrade가 아니라 `claude update`(자기 자신 갱신)가
// 정본이고, 두 경로가 경쟁하게 두지 않기로 이미 결정했다 — 그래서 여기서
// winget upgrade로 라우팅하지 않는다(새 실행 경로 추가는 이 라운드의 범위
// 밖 — 구조적 판단이라 PM 승인 없이 넓히지 않는다). 대신 이 조합을 일반
// UnknownMethod 문구("설치 방식을 확인할 수 없습니다" — 사실과 다르다, 이미
// 정확히 알고 있다)로 조용히 흘려보내지 않고 사실 그대로 말하는 전용 문구로
// 명시적으로 라우팅한다(아래 UPDATE_TABLE 행 참고).
const MANUAL_CLAUDE_WINGET_UNSUPPORTED: ManualPlan = ManualPlan {
    reason: ManualReason::UnsupportedMethod,
    message_ko: "Claude Code가 winget으로 설치된 것으로 보이지만, 이 방식의 자동 업데이트는 아직 지원하지 않습니다. 터미널에서 `claude update`를 직접 실행해주세요.",
    copyable_command: Some("claude update"),
    doc_url: Some("https://docs.claude.com/en/docs/claude-code/setup"),
};
// T3(review-devtools-windows-parity-2026-09-15-r3.md, 2차 m4 승계) 재발 방지:
// 예전에는 "brew/npm/winget"을 한 문구에 전부 나열했다 — Windows 사용자는
// 존재하지 않는 brew를, macOS 사용자는 존재하지 않는 winget을 보는 양방향
// 누출이었고, 신규 가드 2종(N1/N2 재발 방지)도 이 강등 계층(`resolve_plan`)을
// 보지 않아 걸리지 않았다. 실제 후보만 플랫폼별로 나눠 말한다.
pub(crate) const MANUAL_NO_RUNNER_MAC: ManualPlan = ManualPlan {
    reason: ManualReason::NoRunner,
    message_ko: "필요한 실행 도구(brew/npm)를 찾을 수 없어 앱이 자동으로 실행하지 않습니다.",
    copyable_command: None,
    doc_url: None,
};
pub(crate) const MANUAL_NO_RUNNER_WIN: ManualPlan = ManualPlan {
    reason: ManualReason::NoRunner,
    message_ko: "필요한 실행 도구(winget/npm)를 찾을 수 없어 앱이 자동으로 실행하지 않습니다.",
    copyable_command: None,
    doc_url: None,
};

/// `resolve_plan`이 실행기 해석에 실패해 강등할 때 부르는 정본 — 플랫폼별로
/// 실제 존재할 수 있는 러너만 언급한다.
pub(crate) fn manual_no_runner_plan(platform: super::platform::Platform) -> ManualPlan {
    match platform {
        super::platform::Platform::Win => MANUAL_NO_RUNNER_WIN,
        super::platform::Platform::Mac => MANUAL_NO_RUNNER_MAC,
    }
}

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
    // M1 재발 방지(위 MANUAL_CLAUDE_WINGET_UNSUPPORTED 주석 참고): Claude가
    // WingetPackage로 분류돼도 winget upgrade로 라우팅하지 않는다 — 대신
    // 이 행이 명시적으로 매칭돼 일반 UnknownMethod 폴백(사실과 다른 문구)이
    // 아니라 전용 문구로 안내한다.
    Row {
        tool: Some(ToolId::Claude),
        method: MethodKind::WingetPackage,
        action: Action::Manual(MANUAL_CLAUDE_WINGET_UNSUPPORTED),
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
///  ① G1~G4 중 하나라도 원천적으로 탈락하는 도구/플랫폼 조합(mac의 node/git —
///     §1.2. hub decisionId 01m2wse823xcszvn7km3vap0qs 이후 win의 node/git은
///     더 이상 여기 속하지 않는다 — winget 경로로 G1~G4를 통과한다)
///  ② G1~G4는 통과했지만(win의 node/git 포함 — gh/claude/wrangler/pnpm) 필요한
///     실행기(brew/npm/pnpm/winget)를 이 머신에서 찾지 못해 resolve_install_plan이
///     Manual로 강등한 경우(설계 §4.2 "NoRunner" 시나리오 — 무엇이 없어서 안
///     되는지 구체적으로 말한다). pnpm은 npm 러너 후보를 가지므로(install_resolver
///     상단 G1~G4 표 참고) npm이 없을 때만 이 ②경로로 들어온다.
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
            message_ko: "pnpm이 설치되어 있지 않습니다. Node.js(npm)가 있으면 앱이 자동으로 설치 버튼을 보여주는데, 이 머신에서는 npm을 찾지 못했습니다. Node.js를 먼저 설치하거나, Homebrew가 있으면 아래 명령으로 직접 설치해주세요.",
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
        // devtools-install-matrix §1.2 결정 뒤집기(hub decisionId
        // 01m2wse823xcszvn7km3vap0qs) 이후: install_candidates(ToolId::Node)가
        // winget 후보 하나를 갖게 되면서(위 RUN_WINGET_INSTALL_NODE), 이
        // Manual 분기에 실제로 도달하는 경우는 사실상 "이 머신에 winget이
        // 없다"뿐이다(resolve_install_plan이 러너를 못 찾아 여기로 강등) —
        // gh의 기존 Windows manual 문구("...winget(앱 설치 관리자)을 찾을 수
        // 없습니다")와 같은 이유를 명시한다(§0.1 사고① 재발 방지: 이유 없이
        // "안 됩니다"만 말하지 않는다).
        ToolId::Node => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Node.js가 설치되어 있지 않거나 winget(앱 설치 관리자)을 찾을 수 없습니다. 이미 nvm-windows/fnm/volta 등으로 관리 중이라면 그쪽에서 설치해주세요. 그렇지 않다면 Microsoft Store에서 '앱 설치 관리자'를 업데이트한 뒤 아래 명령을 시도해주세요.",
            copyable_command: Some(
                "winget install --id OpenJS.NodeJS.LTS -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent",
            ),
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
        // Node와 동일한 이유(§ 위 주석): install_candidates(ToolId::Git)가
        // winget 후보를 가지므로 이 Manual 분기는 사실상 "winget을 찾지
        // 못했다"는 뜻이다. UAC 승인 창 안내는 기존 문구를 그대로 유지한다.
        ToolId::Git => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "Git이 설치되어 있지 않거나 winget(앱 설치 관리자)을 찾을 수 없습니다. Microsoft Store에서 '앱 설치 관리자'를 업데이트한 뒤 아래 명령을 시도해주세요. 관리자 권한 승인 창이 뜰 수 있습니다(머신 스코프 설치).",
            copyable_command: Some(
                "winget install --id Git.Git -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent",
            ),
            doc_url: Some("https://git-scm.com/downloads"),
        },
        ToolId::Pnpm => ManualPlan {
            reason: ManualReason::NotInstalled,
            message_ko: "pnpm이 설치되어 있지 않습니다. Node.js(npm)가 있으면 앱이 자동으로 설치 버튼을 보여주는데, 이 머신에서는 npm을 찾지 못했습니다. Node.js를 먼저 설치하거나, 아래 명령으로 직접 설치할 수 있습니다(winget). 설치 후 새 터미널을 열어야 PATH가 반영됩니다.",
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
/// 정적 Action)을 계산한다. 실제 실행 경로는 항상 이 함수를 쓴다 — 아래
/// `compute_action_for_platform`에 실제 플랫폼(`platform_now()`)을 주입하는
/// 얇은 래퍼일 뿐이다.
pub(crate) fn compute_action(tool_id: ToolId, method: &InstallMethod) -> Action {
    compute_action_for_platform(tool_id, method, super::platform::platform_now())
}

/// `compute_action`의 순수 버전 — platform을 인자로 주입받는다.
/// `classify_install_method_windows`/`install_manual_plan_windows`와 동일한
/// "순수함수 + 플랫폼 주입" 패턴(§D)이라, 이 머신(Mac)의 `cargo test`에서도
/// `Platform::Win` 분기를 직접 실행 검증할 수 있다 — `compute_action` 자체는
/// `platform_now()`가 항상 Mac을 반환해 Windows 분기를 테스트에서 통과시킬
/// 방법이 없었다(N1 재발 방지 가드가 이 함수를 직접 호출하는 이유).
pub(crate) fn compute_action_for_platform(
    tool_id: ToolId,
    method: &InstallMethod,
    platform: super::platform::Platform,
) -> Action {
    if tool_id == ToolId::Pnpm
        && is_corepack_managed(std::env::var("COREPACK_ROOT").ok().as_deref())
    {
        return Action::Manual(MANUAL_COREPACK_MANAGED);
    }

    // N1(review-devtools-windows-parity-2026-09-15-r2.md): UPDATE_TABLE의
    // SystemManaged 행은 macOS Xcode CLT 문구 하나로 고정돼 있다 — Windows에서
    // SystemManaged로 분류되는 것은 전혀 다른 사실(설치기 관리 경로)이므로
    // lookup_action에 맡기지 않고 여기서 먼저 갈라낸다(corepack 가드와 같은
    // "최우선 override" 위치).
    if method.kind() == MethodKind::SystemManaged && platform == super::platform::Platform::Win {
        return Action::Manual(windows_system_managed_plan(tool_id));
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
#[path = "plan_table_tests.rs"]
mod tests;
