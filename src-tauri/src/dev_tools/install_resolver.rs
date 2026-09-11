// ==================== 3. argv 검증 ====================
// (이 모듈 하단 "4.5 설치 후보 테이블"의 resolve_args()가 유일한 소비처라 argv
// 검증도 이 파일에 함께 둔다.)

use super::classify::{canonicalize_best_effort, classify_install_method_with_defaults, InstallMethod};
use super::plan_table::{
    compute_action, install_manual_plan, is_writable_by_current_user, Action, Arg, ManualPlan,
    Runner, RunPlan, MANUAL_NOT_WRITABLE, MANUAL_NO_RUNNER, RUN_BREW_INSTALL_GH,
    RUN_NPM_GLOBAL_INSTALL_WRANGLER, RUN_NPM_INSTALL_CLAUDE, RUN_PNPM_GLOBAL_ADD,
};
use super::{resolve_tool_path, tool_definition, ToolId};
use crate::cli_launcher::resolve_binary_expand_home;
use std::path::Path;

/// `^[A-Za-z0-9@][A-Za-z0-9@._+/-]*$`, `..` 불포함, `-` 시작 금지를 수기 구현한다
/// (regex 크레이트를 새로 추가하지 않기 위해 — 이 검사는 6개 문자 클래스만
/// 다루므로 손으로 짜는 편이 의존성 추가보다 싸다).
pub(crate) fn validate_argv_token(s: &str) -> bool {
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
pub(crate) enum ResolvedAction {
    Run { runner_path: String, plan: RunPlan },
    Manual(ManualPlan),
}

pub(crate) struct ResolvedPlan {
    pub(crate) method: InstallMethod,
    pub(crate) action: ResolvedAction,
}

pub(crate) fn resolve_plan(tool_id: ToolId, resolved_tool_path: &str) -> ResolvedPlan {
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
// ① query::check_dev_tools_blocking() 최상단의 `!cfg!(target_os = "macos")` 분기
//   (§5.2 채택 — 화면에 내려가는 데이터 전체를 manual로 고정한다)
// ② preview_dev_tool_update/update_dev_tool/install_dev_tool/
//   open_manual_instruction 네 IPC 커맨드 각각에 있는 동일한 cfg 게이트 — 화면을
//   거치지 않고 커맨드가 직접 호출돼도 막힌다("IPC 커맨드는 화면을 신뢰하면 안
//   된다" 원칙, M3).

pub(crate) struct InstallCandidate {
    runner: Runner,
    plan: RunPlan,
    installer_label: &'static str,
}

/// (도구) → 시도할 run 후보 목록(우선순위 순). 없는 도구는 항상 Manual이다.
/// 설계 §1.1 불변식: 여기 등장하는 모든 RunPlan은 Arg::Lit 전용이라야 한다 —
/// Arg::Formula/Package/PackageLatest 슬롯은 canonical 경로 없이는 채울 수
/// 없으므로(미설치 도구에는 canonical 경로가 없다) 이 테이블에 올릴 수 없다.
pub(crate) fn install_candidates(tool: ToolId) -> &'static [InstallCandidate] {
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
pub(crate) struct ResolvedRunners {
    pub(crate) brew: Option<String>,
    pub(crate) npm: Option<String>,
    pub(crate) pnpm: Option<String>,
}

impl ResolvedRunners {
    pub(crate) fn resolve() -> Self {
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

/// 실행 준비까지 끝난 설치 계획. `query::check_dev_tools_blocking`(actionKind
/// 산출)·`query::build_install_preview`(미설치 분기)·`actions::perform_install`
/// (실행 직전 재계산) 세 곳이 모두 이 함수만 호출한다(요구 1 — 단일 정본). 세
/// 곳이 각자 분기하지 않으므로 plan_id 종류가 어긋날 수 없다(직전 리뷰 지적
/// #3의 "막다른 골목"을 구조적으로 제거).
pub(crate) enum InstallResolution {
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
/// 생략)를 본다 — `plan_table::compute_action`(update 경로)이 이미 하는 검사를
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

pub(crate) fn resolve_install_plan(tool: ToolId, runners: &ResolvedRunners) -> InstallResolution {
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
pub(crate) fn is_git_stub_without_clt(resolved_path: &str) -> bool {
    resolved_path == "/usr/bin/git"
        && !Path::new("/Library/Developer/CommandLineTools").exists()
        && !Path::new("/Applications/Xcode.app").exists()
}

pub(crate) fn resolve_args(args: &[Arg], method: &InstallMethod) -> Result<Vec<String>, String> {
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

pub(crate) fn build_command_display(runner_path: &str, args: &[String]) -> String {
    let runner_name = Path::new(runner_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(runner_path);
    let mut parts = vec![runner_name.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::plan_table::WRANGLER_PACKAGE;

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

    // brew는 기본이 ask 모드라 -y가 반드시 argv에 있어야 한다(조사결과 #1).
    #[test]
    fn brew_run_plan_includes_explicit_yes_flag_and_preview_omits_it() {
        let args = resolve_args(
            super::super::plan_table::RUN_BREW_FORMULA.args,
            &InstallMethod::HomebrewFormula {
                formula: "pnpm".to_string(),
                keg_version: "11.9.0".to_string(),
                prefix: "/opt/homebrew".to_string(),
            },
        )
        .unwrap();
        assert_eq!(args, vec!["upgrade", "-y", "--formula", "pnpm"]);

        let preview_args = resolve_args(
            super::super::plan_table::RUN_BREW_FORMULA
                .preview_args
                .unwrap(),
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
        let result = resolve_args(super::super::plan_table::RUN_BREW_FORMULA.args, &malicious);
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
        assert!(resolve_args(super::super::plan_table::RUN_BREW_FORMULA.args, &wrong_method).is_err());
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

    // ── Wrangler 전용 실설치(§12.5) ──

    // pnpm/npm RunPlan 둘 다 전부 리터럴 인자라 어떤 InstallMethod를 넘겨도(심지어
    // 의미 없는 더미여도) 안전하게 고정된 argv로 풀려야 한다.
    #[test]
    fn wrangler_run_plans_resolve_to_expected_literal_argv() {
        let dummy = InstallMethod::Unknown(String::new());

        let pnpm_args = resolve_args(
            super::super::plan_table::RUN_PNPM_GLOBAL_ADD.args,
            &dummy,
        )
        .unwrap();
        assert_eq!(pnpm_args, vec!["add", "-g", "wrangler"]);

        let npm_args = resolve_args(
            super::super::plan_table::RUN_NPM_GLOBAL_INSTALL_WRANGLER.args,
            &dummy,
        )
        .unwrap();
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
        for def in super::super::DEV_TOOLS.iter() {
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
    // 사전검사가 실제로 동작하는지 실측한다. plan_table 테스트의
    // compute_action_checks_prefix_cellar_and_bin_not_prefix_itself(update
    // 경로)와 같은 패턴: 홈 디렉터리 아래 합성 prefix/Cellar/bin을 만들어
    // "쓰기 가능"과 "쓰기 불가"(chmod) 두 경우 모두 검증한다.
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
                assert_eq!(mp.reason, super::super::plan_table::ManualReason::NotWritable);
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
}
