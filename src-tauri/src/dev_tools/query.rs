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
use super::plan_table::{install_manual_plan, manual_display_message, MANUAL_XCODE_CLT};
use super::process::{build_child_path_env, check_tool_version, normalize_version, run_process_with_timeout};
use super::{resolve_tool_path, tool_definition, DevTool, ToolId, DEV_TOOLS, MACOS_ONLY_MESSAGE};
use std::time::Duration;

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

pub(crate) fn check_dev_tools_blocking() -> Vec<DevToolStatus> {
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

// ==================== 12. preview (부록 C) ====================

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
}
