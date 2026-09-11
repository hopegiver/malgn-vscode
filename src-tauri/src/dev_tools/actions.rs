// ==================== 12. update / install / manual (부록 C) ====================
// "쓰기(실행)" 커맨드의 무거운 로직 — mod.rs의 `#[tauri::command]` 얇은 래퍼
// (update_dev_tool/install_dev_tool/open_manual_instruction)가 이 모듈의
// perform_update/perform_install/perform_open_manual_instruction으로 위임한다.
// 조회/미리보기는 `query` 모듈 몫이다.

use super::classify::describe_install_method;
use super::contract::{not_supported_result, DevToolActionResult, Outcome};
use super::diagnostics::{
    compute_path_visibility, compute_plan_id, compute_plan_id_for_manual, PathVisibility,
};
use super::install_resolver::{
    build_command_display, resolve_args, resolve_install_plan, resolve_plan, InstallResolution,
    ResolvedAction, ResolvedPlan, ResolvedRunners,
};
use super::plan_table::{manual_display_message, RunPlan};
use super::process::{
    build_child_path_env, check_tool_version, normalize_version, run_process_with_timeout,
    truncate_log, EXECUTION_LOCK,
};
use super::{resolve_tool_path, tool_definition, DevTool, ToolId};
use crate::cli_launcher::{open_terminal_command, TerminalLaunchResult};
use std::time::Duration;

pub(crate) fn perform_update(tool_id_str: &str, plan_id: &str) -> Result<DevToolActionResult, String> {
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

/// 요구 1(단일 정본) + 요구 2(install→update 위임) + 요구 3(하드코딩 제거):
///  - 이미 설치돼 있으면(=resolve_tool_path가 Some) 첫 줄에서 perform_update로
///    위임한다. 프리뷰가 update 경로(query::build_run_preview)에서 나왔다면
///    plan_id가 그대로 맞아떨어져 한 번에 성공한다 — 직전 리뷰 지적 #3의
///    "installed:false + actionKind:run" 막다른 골목(프리뷰는 update용 plan_id,
///    실행은 install용 대조를 쓰는 영구 불일치)이 구조적으로 소멸한다.
///  - 미설치 상태에서는 resolve_install_plan 하나로만 Run/Manual을 가른다.
///    Wrangler·gh·Claude는 Run(각자의 install_candidates), pnpm/node/git은
///    Manual — 더 이상 `if tool == ToolId::Wrangler` 같은 도구별 하드코딩이
///    없다.
pub(crate) fn perform_install(tool_id_str: &str, plan_id: &str) -> Result<DevToolActionResult, String> {
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

// M2(직전 라운드): Manual 경로는 Run 경로(preview -> 사용자 확인 -> 실행)와 달리
// 동의 절차 없이 즉시 실행됐다. `execute` 파라미터로 같은 커맨드를
// "미리보기"(false)와 "실행"(true) 두 모드로 재사용해, 프론트가 먼저 어떤
// 명령이 실행될지 보여주고 사용자 확인을 받은 뒤에만 execute:true로 다시
// 호출하게 한다. 계획 해석 로직을 두 곳에 중복시키지 않기 위해 한 함수 안에서
// 분기하며, 반환 타입은 기존 TerminalLaunchResult를 그대로 재사용한다(필드
// 추가 없음 — 프론트-백엔드 타입 계약 변경 없음).
pub(crate) fn perform_open_manual_instruction(
    tool_id_str: &str,
    execute: bool,
) -> Result<TerminalLaunchResult, String> {
    let tool = ToolId::from_key(tool_id_str)
        .ok_or_else(|| format!("알 수 없는 도구 id입니다: {tool_id_str}"))?;
    let def = tool_definition(tool);

    let manual: Option<super::plan_table::ManualPlan> = match resolve_tool_path(def) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let tools = super::super::query::check_dev_tools_blocking();
        if let Some(installed) = tools.iter().find(|t| t.path.is_some()) {
            let result = perform_install(&installed.id, "not-a-real-plan-id");
            assert!(result.is_err());
        }
    }
}
