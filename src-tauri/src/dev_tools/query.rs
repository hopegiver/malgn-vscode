// ==================== 11. check_dev_tools (기존 이름 유지 + 필드 확장) ====================
// "조회/미리보기"(읽기 전용) 커맨드의 무거운 로직 — mod.rs의 `#[tauri::command]`
// 얇은 래퍼(check_dev_tools/preview_dev_tool_update)가 이 모듈의
// `check_dev_tools_blocking`/`perform_preview`로 위임한다. 실제 실행(update/
// install/manual)은 `actions` 모듈 몫이다.

use super::classify::{describe_install_method, InstallMethod};
use super::contract::{DevToolPreview, DevToolStatus};
use super::diagnostics::{
    compute_plan_id, compute_plan_id_for_manual, parse_brew_dry_run_affected,
    parse_brew_install_dry_run_affected,
};
use super::install_resolver::{
    build_command_display, is_git_stub_without_clt, resolve_args, resolve_install_plan,
    resolve_plan, InstallResolution, ResolvedAction, ResolvedPlan, ResolvedRunners,
};
use super::plan_table::{install_manual_plan, manual_display_message, Runner, MANUAL_XCODE_CLT};
use super::process::{build_child_path_env, check_tool_version, normalize_version, run_process_with_timeout};
use super::{resolve_tool_path, tool_definition, tool_path_candidates, DevTool, ToolId, DEV_TOOLS};
use std::time::Duration;

/// Windows 완전 지원(devtools-windows-parity.md Phase 1): macOS 전용 게이트를
/// 제거했으므로 이 함수는 항상 실제 탐지를 수행한다 — macOS 분기는 아래
/// `resolve_tool_path`/`resolve_install_plan` 호출이 기존과 완전히 동일한 값을
/// 낸다(무변경). Windows 분기는 `tool_path_candidates`/`classify_install_method_windows`
/// 등 플랫폼 인자를 받는 순수함수가 처리한다(§D).
pub(crate) fn check_dev_tools_blocking() -> Vec<DevToolStatus> {
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

// ==================== 12. preview (부록 C) ====================

/// N2(review-devtools-windows-parity-2026-09-15-r2.md) 재발 방지 — 정본:
/// winget은 사전 시뮬레이션(dry-run)이 없으므로(§B.3) `preview_args`가 없는
/// winget RunPlan은 install/update 어느 경로든 "신뢰할 수 있다"고 말하면
/// 안 된다. `no_dry_run_install_notes`(설치 경로)와 `build_run_preview`의
/// `None` 폴백(업데이트 경로) 둘 다 이 함수 하나로 판정을 공유해, 새 winget
/// 행이 어느 쪽에 추가되든 이 판정을 놓치지 않게 한다.
pub(crate) fn winget_preview_is_reliable(runner: Runner) -> bool {
    runner != Runner::Winget
}

fn build_run_preview(
    def: &DevTool,
    tool_id: ToolId,
    runner_path: String,
    plan: super::plan_table::RunPlan,
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
        None => {
            // N2 재발 방지: preview_args가 없다고 무조건 "신뢰 가능"으로 두지
            // 않는다 — winget처럼 사전 시뮬레이션 자체가 없는 러너는 업데이트
            // 경로에서도 install 경로(no_dry_run_install_notes)와 동일하게
            // preview_reliable:false + 이유를 밝히는 notes를 낸다.
            let reliable = winget_preview_is_reliable(plan.runner);
            let notes = if reliable {
                String::new()
            } else {
                "winget은 사전 시뮬레이션을 제공하지 않아 함께 변경될 항목을 미리 확인할 수 없습니다.".to_string()
            };
            (vec![def.label.to_string()], notes, reliable)
        }
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

/// 설계 §B.3 "프리뷰(무엇이 함께 바뀌나)": `preview_args`가 없는 설치 후보의
/// notes/preview_reliable을 만드는 순수함수. winget은 brew `-n`/`--dry-run`류
/// 사전 시뮬레이션이 없어 다른 러너와 문구·신뢰도가 달라야 한다 — 이 분기를
/// 순수함수로 떼어내 winget이 없는 이 머신(Mac)에서도 텍스트 로직 자체는
/// 테스트로 검증한다(실제 `winget show` 호출 여부는 이 함수가 관여하지 않는다
/// — RUN_WINGET_INSTALL_GH.preview_args가 애초에 None이라 이 분기까지 오는
/// 것 자체가 "호출하지 않기로 한 결정"의 결과다).
///
/// T2①(review-devtools-windows-parity-2026-09-15-r3.md) 재발 방지: 이 함수는
/// 원래 installer_label 문자열이 리터럴 "winget"과 같은지 비교해 신뢰도를
/// 판정했다 —
/// `winget_preview_is_reliable(Runner)`가 "install/update 두 경로가 판정을
/// 공유한다"고 주석에 적은 것과 실제로 다른 정본이 하나 더 있었던 것이다(N2와
/// 같은 결함이 재발할 수 있는 통로). `runner: Runner`를 받아
/// `winget_preview_is_reliable`을 직접 호출하도록 고쳐 정본을 실제로 하나로
/// 만든다. `installer_label`은 여전히 사람이 읽는 문구("npm(으)로 ...")에만
/// 쓰인다 — winget 문구는 신뢰 불가 사유가 고정돼 있어 이 문자열을 쓰지 않는다.
fn no_dry_run_install_notes(runner: Runner, installer_label: &str, tool_label: &str, searched: &str) -> (String, bool) {
    if !winget_preview_is_reliable(runner) {
        (
            format!(
                "winget(으)로 {tool_label}을(를) 전역 설치합니다. 패키지 id가 공식 문서에 고정돼 있어 자동 실행이 안전합니다. winget은 사전 시뮬레이션을 제공하지 않아 함께 변경될 항목을 미리 확인할 수 없습니다. 앱이 확인한 경로: {searched}"
            ),
            false,
        )
    } else {
        (
            format!(
                "{installer_label}(으)로 {tool_label}을(를) 전역 설치합니다. 패키지명이 공식 문서에 고정돼 있어 자동 실행이 안전합니다. 앱이 확인한 경로: {searched}"
            ),
            true,
        )
    }
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
                    // 스스로 잡아낼 수 있게 한다. tool_path_candidates로
                    // 플랫폼에 맞는 후보를 보여준다(mac은 기존 def.path_candidates와
                    // 동일한 값 — 무변경).
                    let searched = tool_path_candidates(def).join(", ");
                    let (notes, preview_reliable) =
                        no_dry_run_install_notes(plan.runner, installer_label, def.label, &searched);
                    (vec![def.label.to_string()], notes, preview_reliable)
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

pub(crate) fn perform_preview(tool_id_str: &str) -> Result<DevToolPreview, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::install_resolver::install_candidates;

    // ── 완료판정 필수 테스트 ①: 존재하지 않는 toolId → 안전한 Err ──
    #[test]
    fn unknown_tool_id_is_rejected_everywhere() {
        assert!(
            ToolId::from_key("docker").is_none(),
            "docker는 결정에 따라 완전히 삭제되어야 합니다"
        );
        assert!(ToolId::from_key("nope").is_none());
        assert!(perform_preview("nope").is_err());
        assert!(super::super::actions::perform_update("nope", "anyplanid").is_err());
        assert!(super::super::actions::perform_install("nope", "anyplanid").is_err());
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

    // Windows 완전 지원 전환(devtools-windows-parity.md Phase 1+2)으로
    // macOS 전용 게이트와 그 배후 함수(windows_unsupported_dev_tools_status,
    // MACOS_ONLY_MESSAGE)가 삭제됐다 — 이 테스트가 검증하던 "Windows에서는
    // 항상 미지원 안내만 보여준다"는 계약 자체가 이번 작업의 목적과 정반대라
    // 함께 제거한다(설계 §C.1 Phase 표, §9 미해결쟁점 4가 이 삭제를 이미
    // 예정해 두었다). 대체 검증은 이 파일의
    // `check_dev_tools_blocking_follows_run_manual_policy_and_never_returns_none`
    // 및 `platform.rs`/`classify.rs`의 Windows 순수함수 테스트가 맡는다.

    // 설계 §B.3: winget 경로는 preview_reliable:false + "사전 시뮬레이션을
    // 제공하지 않아" 문구를 내야 한다 — 거짓 안심을 주지 않는다는 목표를
    // 텍스트 로직만 떼어내 이 머신(winget 없음)에서도 검증한다.
    #[test]
    fn no_dry_run_install_notes_marks_winget_as_unreliable_preview() {
        let (notes, preview_reliable) =
            no_dry_run_install_notes(Runner::Winget, "winget", "GitHub CLI", "C:\\gh.exe");
        assert!(!preview_reliable);
        assert!(notes.contains("사전 시뮬레이션을 제공하지 않아"));
        assert!(notes.contains("winget"));
    }

    #[test]
    fn no_dry_run_install_notes_keeps_other_installers_reliable() {
        let (notes, preview_reliable) =
            no_dry_run_install_notes(Runner::Npm, "npm", "Claude Code", "/opt/homebrew/bin/claude");
        assert!(preview_reliable);
        assert!(notes.contains("npm"));
        assert!(!notes.contains("사전 시뮬레이션"));
    }

    // N2(review-devtools-windows-parity-2026-09-15-r2.md) 재발 방지 — 정본
    // 함수 자체의 계약: winget만 신뢰 불가, 나머지 러너는 신뢰 가능.
    #[test]
    fn winget_preview_is_reliable_is_false_only_for_winget_runner() {
        assert!(!winget_preview_is_reliable(Runner::Winget));
        assert!(winget_preview_is_reliable(Runner::Brew));
        assert!(winget_preview_is_reliable(Runner::Npm));
        assert!(winget_preview_is_reliable(Runner::Pnpm));
        assert!(winget_preview_is_reliable(Runner::SelfBinary));
    }

    // N2 핵심 재발 방지 — 통합 테스트: build_run_preview의 실제 None 폴백
    // 분기가(수정 전에는 무조건 `true`였다) RUN_WINGET_UPGRADE_GH(업데이트
    // 경로)에 대해 preview_reliable=false를 실제로 반환하는지 확인한다.
    // preview_args가 None이라 dry-run 프로세스를 spawn하지 않으므로 이
    // 머신(winget 없음)에서도 안전하게 실행된다.
    #[test]
    fn build_run_preview_marks_winget_update_path_as_unreliable() {
        use super::super::plan_table::RUN_WINGET_UPGRADE_GH;

        let def = tool_definition(ToolId::Gh);
        // RUN_WINGET_UPGRADE_GH.args는 전부 Arg::Lit이라 어떤 method를 넘겨도
        // resolve_args가 성공한다(install_resolver의 dummy_method와 동일 전제).
        let method = InstallMethod::Unknown(String::new());
        let preview = build_run_preview(
            def,
            ToolId::Gh,
            "gh_runner_path_placeholder".to_string(),
            RUN_WINGET_UPGRADE_GH,
            method,
        )
        .expect("RUN_WINGET_UPGRADE_GH의 인자는 전부 리터럴이라 항상 성공해야 합니다");

        assert!(
            !preview.preview_reliable,
            "winget 업데이트 프리뷰가 신뢰 가능하다고 잘못 표시됩니다(N2 재발)"
        );
        assert!(preview.notes.contains("winget"));
        assert!(preview.notes.contains("사전 시뮬레이션"));
    }

    // N2 클래스 가드 — 구조적 순회: (설치 경로) install_candidates() +
    // (업데이트 경로) lookup_action(모든 도구 × 모든 MethodKind 조합)을 함께
    // 순회해 Runner::Winget인 RunPlan을 하드코딩 없이 모은다. UPDATE_TABLE은
    // plan_table 모듈 밖에서 직접 순회할 수 없으므로(비공개 static)
    // lookup_action(공개 조회 함수)으로 같은 효과를 낸다. MethodKind 전수성은
    // 컴파일 타임에 강제한다 — 새 variant가 추가되면 `assert_kind_is_covered`의
    // match가 컴파일에 실패한다.
    //
    // T2②(review-devtools-windows-parity-2026-09-15-r3.md) 재발 방지: 이 가드의
    // 핵심 단언은 원래 `if plan.runner == Winget { assert!(!(runner != Winget)) }`
    // 꼴이라 항상 참이었다(항진명제 — 실제로는 `winget_preview_is_reliable`
    // 정의 자체를 그대로 되풀이할 뿐 아무것도 검증하지 않았다). 업데이트
    // 경로는 `build_run_preview`를, 설치 경로는 T2①로 정본이 통일된
    // `no_dry_run_install_notes`를 **실제로 호출**해 반환된 `preview_reliable`을
    // 검사하도록 바꾼다 — 두 함수 모두 `preview_args: None`인 winget 플랜에서는
    // 프로세스를 spawn하지 않아(이 머신에 winget이 없어도) 안전하다.
    #[test]
    fn all_winget_run_plans_report_unreliable_preview_in_both_install_and_update_paths() {
        use super::super::classify::MethodKind;
        use super::super::plan_table::{lookup_action, Action};
        use super::super::DEV_TOOLS;

        // T5(review-devtools-windows-parity-2026-09-15-r3.md) 재발 방지: 각
        // arm의 본문을 비워두지 않는다 — 새 MethodKind 변형이 추가되면 이
        // match가 먼저 컴파일 에러를 낸다(arm 나열 강제). 거기서 그치지 않고
        // 본문이 `ALL_METHOD_KINDS` 등재 여부를 런타임에도 확인한다 — 컴파일
        // 에러를 arm만 추가해 넘기고 배열 등재를 잊는 시나리오를 위한 이중
        // 방어다(완전한 방어는 아니다: 이 함수는 오직 ALL_METHOD_KINDS 자신을
        // 순회하며 호출되므로 배열에 없는 값으로는 애초에 호출되지 않는다 —
        // 진짜 방어선은 여전히 컴파일 타임 exhaustive match다).
        fn assert_kind_is_covered(kind: MethodKind) {
            match kind {
                MethodKind::HomebrewFormula
                | MethodKind::HomebrewCask
                | MethodKind::PnpmStandalone
                | MethodKind::PnpmGlobalPackage
                | MethodKind::NpmGlobal
                | MethodKind::ClaudeNative
                | MethodKind::SystemManaged
                | MethodKind::VersionManager
                | MethodKind::WingetPackage
                | MethodKind::Unknown => {
                    assert!(
                        ALL_METHOD_KINDS.contains(&kind),
                        "{kind:?}를 ALL_METHOD_KINDS에 추가하세요"
                    );
                }
            }
        }
        const ALL_METHOD_KINDS: [MethodKind; 10] = [
            MethodKind::HomebrewFormula,
            MethodKind::HomebrewCask,
            MethodKind::PnpmStandalone,
            MethodKind::PnpmGlobalPackage,
            MethodKind::NpmGlobal,
            MethodKind::ClaudeNative,
            MethodKind::SystemManaged,
            MethodKind::VersionManager,
            MethodKind::WingetPackage,
            MethodKind::Unknown,
        ];
        for k in ALL_METHOD_KINDS {
            assert_kind_is_covered(k);
        }

        let mut winget_plans_found = 0usize;

        // 업데이트 경로: 도구 × MethodKind 전수 조합을 lookup_action으로 순회하고,
        // winget RunPlan을 찾으면 build_run_preview를 실제로 호출한다.
        for def in DEV_TOOLS.iter() {
            for kind in ALL_METHOD_KINDS {
                if let Action::Run(plan) = lookup_action(def.id, kind) {
                    if plan.runner == Runner::Winget {
                        winget_plans_found += 1;
                        assert!(
                            plan.preview_args.is_none(),
                            "{}/{kind:?} winget RunPlan에 preview_args가 생겼습니다 — 이 \
                             경우 build_run_preview의 Some 분기(실제 dry-run 실행 결과)가 \
                             신뢰도를 정하므로 이 가드의 None-폴백 전제가 더 이상 맞지 \
                             않습니다. no_dry_run_install_notes/winget_preview_is_reliable \
                             전제를 재검토하세요.",
                            def.key
                        );
                        let method = InstallMethod::Unknown(String::new());
                        let preview = build_run_preview(
                            def,
                            def.id,
                            "winget_runner_path_placeholder".to_string(),
                            plan,
                            method,
                        )
                        .expect("winget RunPlan.args는 전부 리터럴이라 항상 성공해야 합니다");
                        assert!(
                            !preview.preview_reliable,
                            "{}/{kind:?} winget 업데이트 프리뷰가 신뢰 가능하다고 잘못 \
                             표시됩니다(N2 재발)",
                            def.key
                        );
                    }
                }
            }
        }

        // 설치 경로: install_candidates()를 DEV_TOOLS 전체로 순회하고, winget
        // RunPlan을 찾으면 no_dry_run_install_notes(T2①로 install/update 공통
        // 정본이 됐다)를 실제로 호출한다.
        for def in DEV_TOOLS.iter() {
            for candidate in install_candidates(def.id) {
                if candidate.plan.runner == Runner::Winget {
                    winget_plans_found += 1;
                    assert!(
                        candidate.plan.preview_args.is_none(),
                        "{}의 install winget RunPlan에 preview_args가 생겼습니다 — \
                         no_dry_run_install_notes 경로를 거치지 않게 됩니다",
                        def.key
                    );
                    let (_, preview_reliable) =
                        no_dry_run_install_notes(candidate.plan.runner, "test", def.label, "test");
                    assert!(
                        !preview_reliable,
                        "{}의 install winget 프리뷰가 신뢰 가능하다고 잘못 표시됩니다(N2 재발)",
                        def.key
                    );
                }
            }
        }

        assert!(
            winget_plans_found > 0,
            "winget RunPlan이 하나도 발견되지 않았습니다 — 이 가드가 무의미해집니다"
        );
    }
}
