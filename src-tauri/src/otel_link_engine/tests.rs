// 엔진 단위 테스트 — 허브 계약 D-1~D-15를 규칙마다 하나 이상의 테스트로 고정한다.
// 테스트 이름은 규칙 번호로 시작한다(`d1_…` = 요약 D의 1번). 대응표는 최종 보고에 있다.
//
// 시간은 가짜 `now`(밀리초)로 흘려보낸다 — sleep하지 않는다. 설정 파일은 임시 디렉터리뿐이다.

use super::testkit::*;
use super::*;
use crate::fs_atomic::backup_path_for;
use crate::otel_hub_client::{BlockedReason, IssuePolicy};

const T0: u64 = 1_000_000;
const MIN: u64 = 60_000;
const HOUR: u64 = 3_600_000;

fn run(engine: &LinkEngine, rig: &Rig, trigger: Trigger, now: u64) -> Phase {
    engine.run_check(&rig.env(), trigger, now);
    engine.view().phase
}

fn file(rig: &Rig) -> String {
    std::fs::read_to_string(&rig.path).unwrap()
}

fn file_json(rig: &Rig) -> serde_json::Value {
    serde_json::from_str(&file(rig)).unwrap()
}

fn headers_of(rig: &Rig) -> String {
    file_json(rig)["env"]["OTEL_EXPORTER_OTLP_HEADERS"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

// ======================= D-1 점검 분기 =======================

#[test]
fn d1_auto_posts_label_without_confirm_flag_and_applies_the_key() {
    let rig = Rig::new("d1-auto");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    assert_eq!(rig.api.kinds(), ["get", "create:false"]);
    let create_call = &rig.api.calls()[1];
    assert!(create_call.contains("맑은에이전트 test-mac"), "{create_call}");
    assert_eq!(headers_of(&rig), format!("Authorization=Bearer {}", fake_key('K')));
    assert!(engine.view().waiting_first_data, "적용 직후 첫 데이터 확인을 시작한다");
}

#[test]
fn d1_confirm_required_asks_the_user_and_posts_nothing_until_answered() {
    let rig = Rig::new("d1-confirm");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(rig.api.kinds(), ["get"], "사람이 답하기 전에는 POST하지 않는다");
    assert!(!file(&rig).contains("mhk_"));
}

#[test]
fn d1_confirm_accepted_posts_with_confirm_reissue_true_and_applies() {
    let rig = Rig::new("d1-accept");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);

    engine.accept_confirm(&rig.env(), T0 + 5_000);

    assert_eq!(rig.api.kinds(), ["get", "create:true"]);
    assert_eq!(engine.view().phase, Phase::Idle);
    assert!(file(&rig).contains(&fake_key('K')));
}

#[test]
fn d1_confirm_declined_never_asks_again_this_run_but_user_retry_does() {
    let rig = Rig::new("d1-decline");
    write_ready(&rig.path, None);
    for _ in 0..3 {
        rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    }
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.view().phase, Phase::ConfirmRequired);

    engine.decline_confirm();
    assert_eq!(engine.view().phase, Phase::ConfirmDeclined);

    // 1시간 뒤 정기 점검에서 같은 응답을 받아도 다시 묻지 않는다. POST도 없다.
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + HOUR), Phase::ConfirmDeclined);
    assert_eq!(rig.api.count("create"), 0);

    // 사용자가 직접 [다시 시도]를 누르면 다시 묻는다(명시적 행동).
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + HOUR + 1), Phase::ConfirmRequired);
}

#[test]
fn d1_accept_is_ignored_when_no_confirmation_is_pending() {
    let rig = Rig::new("d1-accept-stale");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    engine.accept_confirm(&rig.env(), T0); // 확인 대기 상태가 아니다
    assert!(rig.api.calls().is_empty());
}

#[test]
fn d1_blocked_stops_automation_and_reports_the_reason() {
    for reason in [BlockedReason::Quota, BlockedReason::SuspectedLeak] {
        let rig = Rig::new("d1-blocked");
        write_ready(&rig.path, None);
        rig.api.push_get(state_policy(IssuePolicy::Blocked, Some(reason)));
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Blocked(Some(reason)));
        assert_eq!(rig.api.kinds(), ["get"]);
    }
}

#[test]
fn no_device_token_makes_no_server_call_and_touches_nothing() {
    let mut rig = Rig::new("no-token");
    let original = write_ready(&rig.path, None);
    rig.tokens = FakeTokens::none();
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::NoDeviceToken);
    assert!(rig.api.calls().is_empty());
    assert_eq!(file(&rig), original);
}

// ======================= D-2 201 적용 =======================

#[test]
fn d2_201_replaces_only_the_authorization_entry_and_keeps_everything_else() {
    let rig = Rig::new("d2-merge");
    let original = write_ready(&rig.path, Some(&fake_key('O')));
    // 서버의 현행 키가 파일의 키와 달라 회전한다.
    rig.api.push_get(state_has_current(&prefix_of('P'), false));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    let json = file_json(&rig);
    // Authorization 항목만 새 키로 바뀌고 다른 헤더 항목(x-team=a)·위치는 그대로다.
    assert_eq!(
        json["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
        format!("x-team=a,Authorization=Bearer {}", fake_key('N'))
    );
    // OTEL_RESOURCE_ATTRIBUTES·무관한 env·최상위 설정은 건드리지 않는다.
    assert_eq!(json["env"]["OTEL_RESOURCE_ATTRIBUTES"], "employee.id=dev,team=platform");
    assert_eq!(json["env"]["SOME_OTHER_VAR"], "keep-me");
    assert_eq!(json["env"]["OTEL_LOGS_EXPORT_INTERVAL"], "5000");
    assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
    // 백업은 저장 직전 원본(재사용되는 롤링 1세대).
    assert_eq!(std::fs::read_to_string(backup_path_for(&rig.path)).unwrap(), original);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&rig.path), 0o600);
        assert_eq!(mode(&backup_path_for(&rig.path)), 0o600);
    }
}

#[test]
fn d2_201_without_env_adds_only_the_authorization_entry_and_leaves_every_other_value() {
    // 응답에 `env`가 없으면 키만 적용한다(R19 이전과 같은 동작) — 파일의 다른 값은 그대로다.
    let rig = Rig::new("d2-add");
    let original = ready_settings(None);
    std::fs::write(&rig.path, &original).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);

    let before: serde_json::Value = serde_json::from_str(&original).unwrap();
    let after = file_json(&rig);
    assert_eq!(after["env"]["OTEL_EXPORTER_OTLP_HEADERS"], format!("Authorization=Bearer {}", fake_key('K')));
    for k in ["OTEL_LOGS_EXPORT_INTERVAL", "OTEL_METRICS_EXPORTER", "CLAUDE_CODE_ENABLE_TELEMETRY", "OTEL_RESOURCE_ATTRIBUTES"] {
        assert_eq!(after["env"][k], before["env"][k], "{k}");
    }
    assert!(after["env"].get("OTEL_METRIC_EXPORT_INTERVAL").is_none(), "env가 없으면 아무 키도 새로 쓰지 않는다");
    let view = engine.view();
    assert!(view.env_changed.is_empty() && view.env_rejected.is_empty());
}

#[test]
fn d2_invalid_key_in_a_201_is_rejected_and_never_written() {
    let dev = device_token('D');
    let bad_values = [dev.clone(), format!("Bearer {dev}"), "mhk_short".to_string(), String::new(), format!("{}A", fake_key('K'))];
    for bad in bad_values {
        let rig = Rig::new("d2-invalid");
        let original = write_ready(&rig.path, None);
        rig.api.push_get(state_policy(IssuePolicy::Auto, None));
        rig.api.push_create(Ok(crate::otel_hub_client::Created { token: SecretString::new(&bad), env: Default::default() }));
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::InvalidKey);
        assert_eq!(file(&rig), original, "형식이 틀린 값은 파일에 쓰지 않는다");
        assert!(!backup_path_for(&rig.path).exists());
        let view = format!("{:?}", engine.view());
        assert!(!view.contains(&dev[4..]), "{view}");
    }
}

// ======================= D-3 응답 유실 후 409 =======================

#[test]
fn d3_post_response_lost_then_409_exists_goes_to_step_4_and_rotates() {
    let rig = Rig::new("d3-lost-post");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(ApiError::Lost));
    rig.api.push_create(Err(http_with(409, "DEVICE_CREDENTIAL_EXISTS", |h| h.current = Some(meta(&prefix_of('X'), false)))));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    assert_eq!(rig.api.kinds(), ["get", "create:false", "create:false", "rotate"]);
    assert!(headers_of(&rig).contains(&fake_key('N')));
}

#[test]
fn d3_409_exists_with_the_same_prefix_as_the_file_is_already_applied() {
    let rig = Rig::new("d3-exists-same");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http_with(409, "DEVICE_CREDENTIAL_EXISTS", |h| h.current = Some(meta(&prefix_of('S'), false)))));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 0);
    assert!(headers_of(&rig).contains(&fake_key('S')));
}

#[test]
fn d3_post_lost_twice_ends_as_a_transient_error_without_counting_toward_the_rotation_limit() {
    let rig = Rig::new("d3-lost-twice");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(ApiError::Lost));
    rig.api.push_create(Err(ApiError::Lost));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(rig.api.count("create"), 2);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
}

// ======================= D-4 prefix 비교 =======================

#[test]
fn d4_same_prefix_means_already_applied_and_no_rotation() {
    let rig = Rig::new("d4-same");
    let original = write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('S'), false));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get"]);
    assert_eq!(file(&rig), original, "이미 적용된 상태는 파일을 건드리지 않는다");
}

#[test]
fn d4_different_prefix_rotates_without_judging_who_rotated() {
    let rig = Rig::new("d4-diff");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('Z'), true));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate"]);
    assert!(headers_of(&rig).contains(&fake_key('N')));
}

#[test]
fn d4_missing_key_in_settings_rotates() {
    let rig = Rig::new("d4-missing");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate"]);
}

// ======================= D-5 rotate 유실·연속 유실·쓰기 실패 =======================

#[test]
fn d5_rotate_response_lost_retries_once_immediately() {
    let rig = Rig::new("d5-retry");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate", "rotate"]);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
}

#[test]
fn d5_rotate_connection_refused_is_not_a_loss_and_is_not_retried_immediately() {
    let rig = Rig::new("d5-notsent");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::NotSent));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(rig.api.count("rotate"), 1);
    assert_eq!(engine.with(|m| m.lost_checks), 0, "서버에 닿지 않은 요청은 유실이 아니다");
}

#[test]
fn d5_two_consecutive_lost_checks_stop_automatic_rotation_with_at_most_three_keys() {
    let rig = Rig::new("d5-limit");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();

    // 점검 1: rotate 유실 → 즉시 재시도 유실 → 유실로 끝남(수=1). 서버에는 요청 2건.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    rig.api.push_rotate(Err(ApiError::Lost));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(rig.api.count("rotate"), 2);
    assert_eq!(engine.with(|m| m.lost_checks), 1);

    // 점검 2: 직전이 유실로 끝났으므로 재시도 없이 1회만 → 유실 → 수=2 → 자동 회전 중지.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    assert_eq!(run(&engine, &rig, Trigger::Resume, T0 + MIN), Phase::RotationLost);
    assert_eq!(rig.api.count("rotate"), 3, "응답 유실만으로 자동 생성되는 키는 최대 3개(2+1)");

    // 점검 3: 중지 상태 — GET만 하고 rotate는 부르지 않는다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + MIN + HOUR), Phase::RotationLost);
    assert_eq!(rig.api.count("rotate"), 3);

    // 사용자의 [다시 시도]는 수를 0으로 하고 1번부터 점검한다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + 2 * HOUR), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 4);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
}

#[test]
fn d5_a_rotate_201_resets_the_lost_check_count() {
    let rig = Rig::new("d5-reset");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    rig.api.push_rotate(Err(ApiError::Lost));
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.with(|m| m.lost_checks), 1);

    // 다음 점검: 재시도 없이 1회 → 201 → 수가 0이 된다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::Resume, T0 + MIN), Phase::Idle);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
    assert_eq!(rig.api.count("rotate"), 3);
}

#[test]
fn d5_an_already_applied_check_is_not_a_lost_check_and_resets_the_count() {
    let rig = Rig::new("d5-applied-resets");
    write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    engine.with(|m| m.lost_checks = 1);
    rig.api.push_get(state_has_current(&prefix_of('S'), false));
    run(&engine, &rig, Trigger::Periodic, T0);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
}

#[cfg(unix)]
fn make_dir_read_only(rig: &Rig) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(rig.path.parent().unwrap(), std::fs::Permissions::from_mode(0o555)).unwrap();
}

#[cfg(unix)]
fn make_dir_writable(rig: &Rig) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(rig.path.parent().unwrap(), std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn d5_write_failure_keeps_the_raw_key_in_memory_and_later_checks_only_get_then_rewrite() {
    let rig = Rig::new("d5-write-fail");
    let original = write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    make_dir_read_only(&rig);

    // 201은 받았지만 쓰기 실패 → WriteFailed, 원문은 메모리에.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::WriteFailed);
    assert_eq!(file(&rig), original, "쓰지 못했으니 파일은 그대로다");
    assert!(engine.with(|m| m.pending_write.is_some()));
    assert!(!format!("{:?}", engine.view()).contains("NNNN"), "원문이 UI 상태로 새면 안 된다");

    // 플래그가 켜진 동안의 점검: 서버의 현행 키 앞 8자가 보관 원문과 같으면 같은 원문으로 다시 쓰기만 시도한다.
    // 여전히 실패하면 rotate하지 않는다(예산 보호).
    rig.api.push_get(state_has_current(&prefix_of('N'), false));
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + MIN), Phase::WriteFailed);
    assert_eq!(rig.api.count("rotate"), 1);

    // 쓰기가 되는 상태로 돌아오면 같은 원문으로 쓰고 플래그를 내린다. 원문은 메모리에서 지워진다.
    make_dir_writable(&rig);
    rig.api.push_get(state_has_current(&prefix_of('N'), false));
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + 2 * MIN), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 1, "같은 원문으로 다시 쓰므로 회전이 더 일어나지 않는다");
    assert!(headers_of(&rig).contains(&fake_key('N')));
    assert!(engine.with(|m| m.pending_write.is_none()));
}

#[cfg(unix)]
#[test]
fn d5_write_failure_with_a_changed_server_key_drops_the_raw_and_judges_from_step_1() {
    let rig = Rig::new("d5-write-changed");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    make_dir_read_only(&rig);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    run(&engine, &rig, Trigger::Startup, T0);
    assert!(engine.with(|m| m.pending_write.is_some()));
    make_dir_writable(&rig);

    // 그 사이 서버의 현행 키가 바뀌었다(웹 폐기·회전) → 보관 원문을 버리고 4번(다르면 회전)으로 간다.
    rig.api.push_get(state_has_current(&prefix_of('Q'), false));
    rig.api.push_rotate(created('R'));
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + MIN), Phase::Idle);
    assert!(headers_of(&rig).contains(&fake_key('R')));
    assert!(engine.with(|m| m.pending_write.is_none()));

    // 현행 키가 아예 없으면(auto) 원문을 버리고 발급 경로로 간다.
    let rig2 = Rig::new("d5-write-gone");
    write_ready(&rig2.path, None);
    let engine2 = LinkEngine::new();
    make_dir_read_only(&rig2);
    rig2.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig2.api.push_rotate(created('N'));
    run(&engine2, &rig2, Trigger::Startup, T0);
    make_dir_writable(&rig2);
    rig2.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig2.api.push_create(created('C'));
    assert_eq!(run(&engine2, &rig2, Trigger::User, T0 + MIN), Phase::Idle);
    assert!(engine2.with(|m| m.pending_write.is_none()));
    assert!(headers_of(&rig2).contains(&fake_key('C')));
}

#[cfg(unix)]
#[test]
fn d5_write_failure_state_is_not_persisted_a_new_engine_starts_clean() {
    // "플래그와 원문은 영속하지 않는다": 앱이 재기동되면(새 엔진) 보관 원문이 없고 첫 점검의 4번에서 회전한다.
    let rig = Rig::new("d5-restart");
    write_ready(&rig.path, None);
    let first = LinkEngine::new();
    make_dir_read_only(&rig);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    run(&first, &rig, Trigger::Startup, T0);
    make_dir_writable(&rig);
    drop(first);

    let second = LinkEngine::new();
    assert!(second.with(|m| m.pending_write.is_none()));
    rig.api.push_get(state_has_current(&prefix_of('N'), false));
    rig.api.push_rotate(created('R'));
    assert_eq!(run(&second, &rig, Trigger::Startup, T0 + HOUR), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 2, "재기동 뒤 첫 점검의 4번에서 회전 1회");
}

#[test]
fn d5_one_check_sends_at_most_two_rotate_requests_including_the_loss_retry() {
    // 계약 D 머리말: "한 번의 점검에서 rotate는 5번의 재시도를 포함해 최대 2회".
    // ALREADY_ROTATED로 재판정한 뒤의 rotate가 유실돼도 3번째 rotate(재시도)를 보내지 않는다.
    let rig = Rig::new("d5-max-two");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(409, "ALREADY_ROTATED")));
    rig.api.push_get(state_has_current(&prefix_of('Y'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(rig.api.count("rotate"), 2, "세 번째 rotate는 없다");
    assert_eq!(engine.with(|m| m.lost_checks), 1, "유실로 끝난 점검으로 센다");

    // 유실 재시도 2회로 상한을 채운 뒤에는 재판정(ALREADY_ROTATED)이 와도 더 보내지 않는다.
    let rig = Rig::new("d5-max-two-b");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(ApiError::Lost));
    rig.api.push_rotate(Err(http(409, "ALREADY_ROTATED")));
    rig.api.push_get(state_has_current(&prefix_of('Y'), false));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 2);
}

// ======================= D-6 TOKEN_SUPERSEDED =======================

#[test]
fn d6_token_superseded_rereads_the_latest_device_token_and_retries_once() {
    let mut rig = Rig::new("d6-rotate");
    write_ready(&rig.path, None);
    let (t1, t2) = (device_token('A'), device_token('B'));
    rig.tokens = FakeTokens::new(&t1).then(&t1).then(&t2);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(409, "TOKEN_SUPERSEDED")));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    let calls = rig.api.calls();
    assert_eq!(calls[1], format!("rotate:{t1}"));
    assert_eq!(calls[2], format!("rotate:{t2}"), "키체인에서 다시 읽은 최신 토큰으로 재시도");
}

#[test]
fn d6_a_second_token_superseded_in_the_same_check_is_not_retried_again() {
    let rig = Rig::new("d6-twice");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(409, "TOKEN_SUPERSEDED")));
    rig.api.push_rotate(Err(http(409, "TOKEN_SUPERSEDED")));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(rig.api.count("rotate"), 2);
}

#[test]
fn d6_token_superseded_on_post_rereads_the_token_too() {
    let mut rig = Rig::new("d6-post");
    write_ready(&rig.path, None);
    let (t1, t2) = (device_token('A'), device_token('B'));
    rig.tokens = FakeTokens::new(&t1).then(&t1).then(&t2);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(409, "TOKEN_SUPERSEDED")));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert!(rig.api.calls()[2].ends_with(&t2));
}

// ======================= D-7 401 / ALREADY_RENEWED =======================

#[test]
fn d7a_401_with_self_200_means_the_feature_is_missing_and_never_asks_to_relogin() {
    let rig = Rig::new("d7a-alive");
    write_ready(&rig.path, None);
    rig.api.push_get(Err(http(401, "UNAUTHORIZED")));
    let rig = rig.with_probe(&[ProbeResult::Alive]);
    let engine = LinkEngine::new();

    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::FeatureUnavailable);

    assert_eq!(rig.probe.count(), 1);
    assert_eq!(rig.probe.calls.lock().unwrap()[0], device_token('D'), "같은 mhd_로 확인한다");
    // 1시간 뒤(또는 다음 앱 기동 시) 1번부터 다시 점검한다.
    assert_eq!(engine.due(T0 + HOUR - 1), None);
    assert_eq!(engine.due(T0 + HOUR), Some(Trigger::Periodic));
}

#[test]
fn d7a_401_with_self_401_or_already_renewed_means_reconnect() {
    let rig = Rig::new("d7a-dead");
    write_ready(&rig.path, None);
    rig.api.push_get(Err(http(401, "UNAUTHORIZED")));
    let rig = rig.with_probe(&[ProbeResult::Dead]);
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Reconnect);
}

#[test]
fn d7a_401_with_self_5xx_or_network_error_is_a_transient_error_not_a_relogin() {
    let rig = Rig::new("d7a-unknown");
    let original = write_ready(&rig.path, None);
    rig.api.push_get(Err(http(401, "UNAUTHORIZED")));
    let rig = rig.with_probe(&[ProbeResult::Unknown]);
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    assert_eq!(file(&rig), original);
    assert_eq!(engine.due(T0 + MIN), Some(Trigger::Resume), "백오프 1분 뒤 다시 시도");
}

#[test]
fn d7a_401_from_post_and_rotate_is_judged_by_the_same_self_probe() {
    // POST 401
    let rig = Rig::new("d7a-post");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(401, "UNAUTHORIZED")));
    let rig = rig.with_probe(&[ProbeResult::Alive]);
    assert_eq!(run(&LinkEngine::new(), &rig, Trigger::Startup, T0), Phase::FeatureUnavailable);
    // rotate 401
    let rig = Rig::new("d7a-rotate");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(401, "UNAUTHORIZED")));
    let rig = rig.with_probe(&[ProbeResult::Dead]);
    assert_eq!(run(&LinkEngine::new(), &rig, Trigger::Startup, T0), Phase::Reconnect);
}

#[test]
fn d7a_the_401_message_text_is_never_used_to_decide() {
    // 서버 문구가 무엇이든(code=UNKNOWN) 판정은 /self 확인 호출이 한다.
    let rig = Rig::new("d7a-message");
    write_ready(&rig.path, None);
    rig.api.push_get(Err(http(401, "UNKNOWN")));
    let rig = rig.with_probe(&[ProbeResult::Alive]);
    assert_eq!(run(&LinkEngine::new(), &rig, Trigger::Startup, T0), Phase::FeatureUnavailable);
    assert_eq!(rig.probe.count(), 1);
}

#[test]
fn d7b_already_renewed_reconnects_without_a_confirmation_call_on_all_three_paths() {
    for path in ["get", "create", "rotate"] {
        let rig = Rig::new("d7b");
        write_ready(&rig.path, None);
        match path {
            "get" => {
                rig.api.push_get(Err(http(409, "ALREADY_RENEWED")));
            }
            "create" => {
                rig.api.push_get(state_policy(IssuePolicy::Auto, None));
                rig.api.push_create(Err(http(409, "ALREADY_RENEWED")));
            }
            _ => {
                rig.api.push_get(state_has_current(&prefix_of('Z'), false));
                rig.api.push_rotate(Err(http(409, "ALREADY_RENEWED")));
            }
        }
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Reconnect, "{path}");
        assert_eq!(rig.probe.count(), 0, "{path}: 확인 호출 없이 재연결 흐름");
    }
}

#[test]
fn d7_reconnecting_is_treated_as_a_new_connection_and_forces_an_immediate_check() {
    let rig = Rig::new("d7-reconnect");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    engine.with(|m| {
        m.declined = true;
        m.lost_checks = 2;
        m.budget_hold = Some((u64::MAX, None));
        m.pending_write = Some(PendingWrite { key: SecretString::new(&fake_key('P')), server_env: Default::default() });
        m.last_check = Some(T0);
        m.phase = Phase::Reconnect;
    });
    engine.on_connected();
    assert_eq!(engine.due(T0 + 1), Some(Trigger::Connected));
    assert!(engine.with(|m| !m.declined && m.lost_checks == 0 && m.budget_hold.is_none() && m.pending_write.is_none()));
    let _ = rig;
}

// ======================= D-8 CREDENTIAL_LIMIT =======================

#[test]
fn d8_credential_limit_is_reported_and_handled_manually() {
    for via in ["create", "rotate"] {
        let rig = Rig::new("d8");
        let original = write_ready(&rig.path, None);
        if via == "create" {
            rig.api.push_get(state_policy(IssuePolicy::Auto, None));
            rig.api.push_create(Err(http(409, "CREDENTIAL_LIMIT")));
        } else {
            rig.api.push_get(state_has_current(&prefix_of('Z'), false));
            rig.api.push_rotate(Err(http(409, "CREDENTIAL_LIMIT")));
        }
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::CredentialLimit, "{via}");
        assert_eq!(file(&rig), original);
        assert_eq!(rig.api.count("create") + rig.api.count("rotate"), 1, "{via}: 자동 처리·재시도 없음");
    }
}

// ======================= D-9 ALREADY_ROTATED / 404 =======================

#[test]
fn d9_already_rotated_calls_get_again_and_judges_from_step_1() {
    let rig = Rig::new("d9-rotated");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(409, "ALREADY_ROTATED")));
    // 동시 회전에서 졌다 → 다시 GET하면 다른 쪽이 만든 현행 키가 있다 → 4번(다르면 회전).
    rig.api.push_get(state_has_current(&prefix_of('Y'), false));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate", "get", "rotate"]);
}

#[test]
fn d9_a_second_already_rotated_or_404_in_the_same_check_ends_the_check() {
    let rig = Rig::new("d9-twice");
    let original = write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(409, "ALREADY_ROTATED")));
    rig.api.push_get(state_has_current(&prefix_of('Y'), false));
    rig.api.push_rotate(Err(http(404, "NO_DEVICE_CREDENTIAL")));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate", "get", "rotate"], "세 번째 GET은 없다");
    assert_eq!(file(&rig), original);
}

#[test]
fn d9_404_no_device_credential_goes_back_to_get_and_never_straight_to_post() {
    let rig = Rig::new("d9-404-auto");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(404, "NO_DEVICE_CREDENTIAL")));
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('C'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "rotate", "get", "create:false"]);
}

#[test]
fn d9_404_after_a_quota_block_follows_the_blocked_policy_without_a_wasted_post() {
    let rig = Rig::new("d9-404-blocked");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(404, "NO_DEVICE_CREDENTIAL")));
    rig.api.push_get(state_policy(IssuePolicy::Blocked, Some(BlockedReason::Quota)));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Blocked(Some(BlockedReason::Quota)));
    assert_eq!(rig.api.count("create"), 0, "POST는 409 DEVICE_REISSUE_BLOCKED로 1왕복을 낭비한다");
}

#[test]
fn d9_404_leading_to_confirm_required_asks_instead_of_posting() {
    let rig = Rig::new("d9-404-confirm");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(404, "NO_DEVICE_CREDENTIAL")));
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::ConfirmRequired);
    assert_eq!(rig.api.count("create"), 0);
}

// ======================= D-10 REISSUE_CONFIRMATION_REQUIRED / DEVICE_REISSUE_BLOCKED =======================

#[test]
fn d10_reissue_confirmation_required_409_is_handled_like_confirm_required() {
    let rig = Rig::new("d10-confirm");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(409, "REISSUE_CONFIRMATION_REQUIRED")));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::ConfirmRequired);
    // 거절한 뒤에는 같은 409가 와도 다시 묻지 않는다.
    engine.decline_confirm();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(409, "REISSUE_CONFIRMATION_REQUIRED")));
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + HOUR), Phase::ConfirmDeclined);
}

#[test]
fn d10_a_confirmed_post_that_is_asked_to_confirm_again_is_not_retried() {
    let rig = Rig::new("d10-loop");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    rig.api.push_create(Err(http(409, "REISSUE_CONFIRMATION_REQUIRED")));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    engine.accept_confirm(&rig.env(), T0 + 1);
    assert_eq!(engine.view().phase, Phase::HubRejected);
    assert_eq!(rig.api.count("create"), 1);
}

#[test]
fn d10_device_reissue_blocked_on_post_and_on_rotate_after_has_current_is_blocked() {
    let blocked = |reason| http_with(409, "DEVICE_REISSUE_BLOCKED", move |h| h.blocked_reason = Some(reason));
    // POST
    let rig = Rig::new("d10-blocked-post");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(blocked(BlockedReason::Quota)));
    assert_eq!(run(&LinkEngine::new(), &rig, Trigger::Startup, T0), Phase::Blocked(Some(BlockedReason::Quota)));
    // GET이 has_current여도 rotate가 줄 수 있다(유예 키만 quota로 폐기된 경우).
    let rig = Rig::new("d10-blocked-rotate");
    write_ready(&rig.path, None);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(blocked(BlockedReason::SuspectedLeak)));
    assert_eq!(run(&LinkEngine::new(), &rig, Trigger::Startup, T0), Phase::Blocked(Some(BlockedReason::SuspectedLeak)));
}

// ======================= D-11 403 =======================

#[test]
fn d11_403_not_desktop_token_is_never_retried_on_any_path() {
    for path in ["get", "create", "rotate"] {
        let rig = Rig::new("d11");
        write_ready(&rig.path, None);
        match path {
            "get" => {
                rig.api.push_get(Err(http(403, "NOT_DESKTOP_TOKEN")));
            }
            "create" => {
                rig.api.push_get(state_policy(IssuePolicy::Auto, None));
                rig.api.push_create(Err(http(403, "NOT_DESKTOP_TOKEN")));
            }
            _ => {
                rig.api.push_get(state_has_current(&prefix_of('Z'), false));
                rig.api.push_rotate(Err(http(403, "NOT_DESKTOP_TOKEN")));
            }
        }
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::NotDesktopToken, "{path}");
        assert_eq!(engine.due(T0 + MIN), None, "{path}: 짧은 백오프 재시도가 없다");
        assert_eq!(rig.probe.count(), 0);
    }
}

// ======================= D-12 첫 데이터 확인 =======================

fn applied_engine_with_watch(rig: &Rig) -> LinkEngine {
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    engine.run_check(&rig.env(), Trigger::Startup, T0);
    assert!(engine.view().waiting_first_data);
    engine
}

#[test]
fn d12_polls_get_at_least_ten_seconds_apart_until_a_signal_arrives() {
    let rig = Rig::new("d12-poll");
    let engine = applied_engine_with_watch(&rig);
    let stamps = [
        (T0 + 1_000, false),  // 첫 폴링
        (T0 + 5_000, false),  // 간격 미달 — 호출 없음
        (T0 + 9_999, false),  // 간격 미달
        (T0 + 16_000, false), // 15초 뒤 두 번째 폴링
        (T0 + 31_000, true),  // 세 번째 폴링 — 신호 도착
    ];
    let mut poll_times = Vec::new();
    for (now, signal) in stamps {
        let before = rig.api.count("get");
        let cur_signal = signal;
        rig.api.push_get(state_has_current(&prefix_of('K'), cur_signal));
        engine.poll_first_data(&rig.env(), now);
        if rig.api.count("get") > before {
            poll_times.push(now);
        } else {
            rig.api.get.lock().unwrap().clear();
        }
    }
    assert_eq!(poll_times, [T0 + 1_000, T0 + 16_000, T0 + 31_000]);
    assert!(poll_times.windows(2).all(|w| w[1] - w[0] >= 10_000), "10초 이상 간격");
    let view = engine.view();
    assert_eq!(view.confirmed_prefix.as_deref(), Some(prefix_of('K').as_str()));
    assert!(!view.waiting_first_data, "도착하면 폴링을 멈춘다");
    // 멈춘 뒤에는 더 부르지 않는다.
    let before = rig.api.count("get");
    engine.poll_first_data(&rig.env(), T0 + 60_000);
    assert_eq!(rig.api.count("get"), before);
}

#[test]
fn d12_stops_after_fifteen_minutes_and_flags_the_timeout_notice() {
    let rig = Rig::new("d12-timeout");
    let engine = applied_engine_with_watch(&rig);
    let mut now = T0;
    while now < T0 + WATCH_LIMIT_MS + 60_000 {
        now += POLL_INTERVAL_MS;
        rig.api.get.lock().unwrap().clear();
        rig.api.push_get(state_has_current(&prefix_of('K'), false));
        engine.poll_first_data(&rig.env(), now);
    }
    let view = engine.view();
    assert!(!view.waiting_first_data);
    assert!(view.first_data_timed_out, "15분이 지나면 '새 세션을 열면 수집이 시작됩니다'를 안내한다");
    assert_eq!(view.confirmed_prefix, None);
    let gets = rig.api.count("get");
    engine.poll_first_data(&rig.env(), now + HOUR);
    assert_eq!(rig.api.count("get"), gets, "중단 뒤에는 폴링하지 않는다");
    // 최대 호출 수: 15분 / 15초 + 첫 점검 GET.
    assert!(gets <= 1 + (WATCH_LIMIT_MS / POLL_INTERVAL_MS) as usize + 1, "{gets}");
}

#[test]
fn d12_next_startup_check_confirms_with_a_single_get_when_a_signal_exists() {
    let rig = Rig::new("d12-startup-signal");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('S'), true));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.view().confirmed_prefix.as_deref(), Some(prefix_of('S').as_str()));
    assert!(!engine.view().waiting_first_data);
    assert_eq!(rig.api.count("get"), 1);
}

#[test]
fn d12_startup_check_without_a_signal_starts_watching_again() {
    let rig = Rig::new("d12-startup-nosignal");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('S'), false));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert!(engine.view().waiting_first_data);
}

#[test]
fn d12_logs_only_signal_also_counts_and_a_changed_current_key_stops_the_watch() {
    let rig = Rig::new("d12-logs");
    let engine = applied_engine_with_watch(&rig);
    let logs_only = crate::otel_hub_client::CredentialMeta {
        token_prefix: prefix_of('K'),
        last_signal_at_metrics: None,
        last_signal_at_logs: Some("2026-10-15T01:00:00.000Z".into()),
    };
    rig.api.push_get(Ok(crate::otel_hub_client::CredentialState {
        current: Some(logs_only),
        issue_policy: IssuePolicy::HasCurrent,
        blocked_reason: None,
    }));
    engine.poll_first_data(&rig.env(), T0 + 1_000);
    assert!(engine.view().confirmed_prefix.is_some());

    let rig = Rig::new("d12-changed");
    let engine = applied_engine_with_watch(&rig);
    rig.api.push_get(state_has_current(&prefix_of('Q'), true));
    engine.poll_first_data(&rig.env(), T0 + 1_000);
    assert_eq!(engine.view().confirmed_prefix, None, "다른 키의 신호를 이 키의 확인으로 세지 않는다");
    assert!(!engine.view().waiting_first_data);
}

#[test]
fn d12_poll_errors_other_than_loss_stop_the_watch_and_leave_judging_to_the_next_check() {
    let rig = Rig::new("d12-poll-err");
    let engine = applied_engine_with_watch(&rig);
    rig.api.push_get(Err(ApiError::Lost));
    engine.poll_first_data(&rig.env(), T0 + 1_000);
    assert!(engine.view().waiting_first_data, "유실은 다음 폴링에서 다시 본다");
    rig.api.push_get(Err(http(401, "UNAUTHORIZED")));
    engine.poll_first_data(&rig.env(), T0 + 20_000);
    assert!(!engine.view().waiting_first_data);
}

// ======================= D-13 연결 해제 =======================

#[test]
fn d13_disconnect_clears_in_memory_state_including_the_unwritten_raw_key() {
    let engine = LinkEngine::new();
    engine.with(|m| {
        m.pending_write = Some(PendingWrite { key: SecretString::new(&fake_key('P')), server_env: Default::default() });
        m.declined = true;
        m.watch = Some(Watch { prefix: prefix_of('P'), started: 1, last_poll: None });
        m.confirmed_prefix = Some(prefix_of('P'));
    });
    engine.on_disconnected(T0);
    assert!(engine.with(|m| m.pending_write.is_none() && !m.declined && m.watch.is_none() && m.confirmed_prefix.is_none()));
    assert_eq!(engine.view().phase, Phase::NoDeviceToken);
    assert_eq!(engine.due(T0 + 1), None, "연결이 없으면 기동 점검을 되살리지 않는다");
}

// ======================= D-14 429 =======================

#[test]
fn d14_rate_limited_waits_retry_after_and_resumes_without_growing_the_backoff() {
    let rig = Rig::new("d14-rate");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    rig.api.push_get(Err(http_with(429, "RATE_LIMITED", |h| h.retry_after_secs = Some(30))));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::RateLimited);
    assert_eq!(engine.due(T0 + 29_999), None);
    assert_eq!(engine.due(T0 + 30_000), Some(Trigger::Resume));

    // 같은 요청을 이어 보낸다. 다시 429여도 대기는 Retry-After 그대로(백오프 2배가 아니다).
    rig.api.push_get(Err(http_with(429, "RATE_LIMITED", |h| h.retry_after_secs = Some(30))));
    run(&engine, &rig, Trigger::Resume, T0 + 30_000);
    assert_eq!(engine.due(T0 + 59_999), None);
    assert_eq!(engine.due(T0 + 60_000), Some(Trigger::Resume));
    assert_eq!(engine.with(|m| m.backoff_ms), 0);
}

#[test]
fn d14_rate_limited_on_rotate_resends_after_retry_after() {
    let rig = Rig::new("d14-rate-rotate");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http_with(429, "RATE_LIMITED", |_| {})));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::RateLimited);
    assert_eq!(engine.due(T0 + 59_999), None, "Retry-After가 없으면 60초(계약 F)");
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::Resume, T0 + 60_000), Phase::Idle);
}

#[test]
fn d14_rotate_budget_exceeded_holds_rotation_until_retry_at_and_leaves_settings_alone() {
    let rig = Rig::new("d14-budget");
    let original = write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http_with(429, "ROTATE_BUDGET_EXCEEDED", |h| {
        h.retry_after_secs = Some(7_200);
        h.retry_at = Some("2026-10-16T01:02:03.456Z".into());
    })));
    let phase = run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(phase, Phase::RotateBudget { retry_at: Some("2026-10-16T01:02:03.456Z".into()) });
    assert_eq!(file(&rig), original, "settings.json은 그대로 둔다");

    // retry_at 전의 점검(사용자 재시도 포함)은 rotate하지 않는다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    assert!(matches!(run(&engine, &rig, Trigger::Periodic, T0 + HOUR), Phase::RotateBudget { .. }));
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    assert!(matches!(run(&engine, &rig, Trigger::User, T0 + HOUR + 1), Phase::RotateBudget { .. }));
    assert_eq!(rig.api.count("rotate"), 1);

    // 시각이 지나면 다시 rotate한다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + 2 * HOUR), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 2);
}

// ======================= D-15 500 / 네트워크 백오프 =======================

#[test]
fn d15_backoff_starts_at_one_minute_doubles_and_caps_at_one_hour() {
    let rig = Rig::new("d15-backoff");
    let original = write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    let mut now = T0;
    let mut waits = Vec::new();
    for i in 0..9 {
        let reply = match i % 3 {
            0 => Err(http(500, "INTERNAL_ERROR")),
            1 => Err(ApiError::NotSent),
            _ => Err(ApiError::Lost), // GET 응답 유실도 일시 오류
        };
        rig.api.push_get(reply);
        let trigger = if i == 0 { Trigger::Startup } else { Trigger::Resume };
        assert_eq!(run(&engine, &rig, trigger, now), Phase::Transient);
        let next = engine.with(|m| m.next_retry_at).unwrap();
        waits.push(next - now);
        now = next;
    }
    assert_eq!(waits, [MIN, 2 * MIN, 4 * MIN, 8 * MIN, 16 * MIN, 32 * MIN, HOUR, HOUR, HOUR]);
    // 재로그인하지 않고(확인 호출 없음) settings.json도 바꾸지 않는다.
    assert_eq!(rig.probe.count(), 0);
    assert_eq!(file(&rig), original);

    // 성공하면 간격이 처음(1분)으로 돌아간다.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    assert_eq!(run(&engine, &rig, Trigger::Resume, now), Phase::Idle);
    rig.api.push_get(Err(http(502, "UNKNOWN")));
    run(&engine, &rig, Trigger::Periodic, now + HOUR);
    assert_eq!(engine.with(|m| m.next_retry_at).unwrap() - (now + HOUR), MIN);
}

#[test]
fn d15_5xx_on_post_and_rotate_use_the_same_backoff() {
    let rig = Rig::new("d15-mint");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(503, "UNKNOWN")));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Transient);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(Err(http(500, "INTERNAL_ERROR")));
    assert_eq!(run(&engine, &rig, Trigger::Resume, T0 + MIN), Phase::Transient);
    assert_eq!(engine.with(|m| m.backoff_ms), 2 * MIN);
    assert_eq!(engine.with(|m| m.lost_checks), 0, "5xx는 유실이 아니다");
}

#[test]
fn unexpected_4xx_is_rejected_without_automatic_retry() {
    let rig = Rig::new("other-4xx");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(Err(http(400, "VALIDATION_ERROR")));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::HubRejected);
    assert_eq!(engine.due(T0 + 10 * MIN), None);
}

// ======================= 점검 시점 =======================

#[test]
fn schedule_startup_then_one_hour_or_more_between_regular_checks() {
    let rig = Rig::new("schedule");
    write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    assert_eq!(engine.due(T0), Some(Trigger::Startup), "앱 기동 시");
    rig.api.push_get(state_has_current(&prefix_of('S'), true));
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.due(T0 + HOUR - 1), None);
    assert_eq!(engine.due(T0 + HOUR), Some(Trigger::Periodic), "그 뒤로는 1시간 이상 간격");
    // 점검 중에는 겹쳐 시작하지 않는다.
    engine.with(|m| m.phase = Phase::Checking);
    assert_eq!(engine.due(T0 + 10 * HOUR), None);
}

#[test]
fn a_check_never_runs_concurrently_with_another() {
    let rig = Rig::new("single-flight");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    let guard = engine.run.lock().unwrap();
    engine.run_check(&rig.env(), Trigger::User, T0);
    assert!(rig.api.calls().is_empty(), "이미 점검이 돌고 있으면 새 점검은 아무것도 하지 않는다");
    drop(guard);
}

// ======================= 선검사 =======================

#[test]
fn a_file_that_blocks_linking_stops_before_any_server_call() {
    let rig = Rig::new("file-blocks");
    // 형식이 다른 Authorization — 사용자가 둔 다른 서비스의 헤더를 덮어쓰지 않는다.
    let foreign = r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#;
    std::fs::write(&rig.path, foreign).unwrap();
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert!(rig.api.calls().is_empty());
    assert_eq!(file(&rig), foreign);
    // 해석 불가 파일
    std::fs::write(&rig.path, "{ not json").unwrap();
    run(&engine, &rig, Trigger::Resume, T0 + 1);
    assert!(rig.api.calls().is_empty());
    assert_eq!(file(&rig), "{ not json");
}

#[test]
fn mint_gate_stops_issuing_and_rotation_for_unready_or_foreign_settings() {
    // (설명, 파일, 기대 단계)
    let hub_m = hub_endpoint("/v1/metrics");
    let hub_l = hub_endpoint("/v1/logs");
    let base = |extra: serde_json::Value| {
        let mut env = serde_json::json!({
            "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
            "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
            "OTEL_METRICS_EXPORTER": "otlp",
            "OTEL_LOGS_EXPORTER": "otlp",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": hub_m,
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": hub_l,
        });
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() {
                env.as_object_mut().unwrap().remove(k);
            } else {
                env[k] = v.clone();
            }
        }
        serde_json::json!({"env": env}).to_string()
    };
    let cases: Vec<(&str, String, Phase)> = vec![
        ("사내 collector(https)", base(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":"https://collector.corp.example/v1/metrics"})), Phase::EndpointNotHub),
        ("평문 http", base(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":"http://collector.corp.example/v1/logs"})), Phase::EndpointNotHub),
        ("루프백 http(허브 아님)", base(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":"http://localhost:4318/v1/metrics"})), Phase::EndpointNotHub),
        ("해석 불가 URL", base(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":"not a url"})), Phase::EndpointNotHub),
        ("범용 엔드포인트가 허브 밖(신호별 값이 있어도 키가 실릴 수 있다)", base(serde_json::json!({"OTEL_EXPORTER_OTLP_ENDPOINT":"https://collector.corp.example"})), Phase::EndpointNotHub),
        ("신호별 없음 + 범용이 허브 밖(RV-005)", base(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_ENDPOINT":"https://collector.corp.example"})), Phase::EndpointNotHub),
        ("텔레메트리 항목 없음", base(serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY":null})), Phase::NotReady),
        ("텔레메트리 0", base(serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY":"0"})), Phase::NotReady),
        ("엔드포인트 전부 없음(앱 기본값을 믿지 않는다 — RV-005)", base(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null})), Phase::NotReady),
        ("logs 엔드포인트만 없음", base(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null})), Phase::NotReady),
        ("metrics exporter 없음", base(serde_json::json!({"OTEL_METRICS_EXPORTER":null})), Phase::NotReady),
        ("logs exporter가 console", base(serde_json::json!({"OTEL_LOGS_EXPORTER":"console"})), Phase::NotReady),
        ("프로토콜 없음", base(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":null})), Phase::NotReady),
        ("프로토콜 grpc", base(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":"grpc"})), Phase::NotReady),
        // RV-04: traces 엔드포인트는 수집 대상이 아니어도 헤더(수집키)가 그 주소로 나간다 — 선검사가 멈춘다.
        ("traces 엔드포인트가 허브 밖", base(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":"https://collector.corp.example/v1/traces"})), Phase::EndpointNotHub),
        ("traces 엔드포인트가 평문 http", base(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":"http://collector.corp.example/v1/traces"})), Phase::EndpointNotHub),
        ("traces 엔드포인트가 해석 불가 URL", base(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":"not a url"})), Phase::EndpointNotHub),
        // RV-04: 범용은 http/protobuf여도 한 신호의 프로토콜이 grpc면 그 신호는 허브가 받지 못한다.
        ("logs 신호별 프로토콜이 grpc", base(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_PROTOCOL":"grpc"})), Phase::NotReady),
        ("범용 프로토콜 없음 + metrics만 신호별 지정", base(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":null,"OTEL_EXPORTER_OTLP_METRICS_PROTOCOL":"http/protobuf"})), Phase::NotReady),
        ("빈 파일", "{}".to_string(), Phase::NotReady),
    ];
    for (label, content, expected) in cases {
        for mint in ["create", "rotate"] {
            let rig = Rig::new("gate");
            std::fs::write(&rig.path, &content).unwrap();
            if mint == "create" {
                rig.api.push_get(state_policy(IssuePolicy::Auto, None));
            } else {
                rig.api.push_get(state_has_current(&prefix_of('Z'), false));
            }
            let engine = LinkEngine::new();
            assert_eq!(run(&engine, &rig, Trigger::Startup, T0), expected, "{label}/{mint}");
            assert_eq!(rig.api.count("create") + rig.api.count("rotate"), 0, "{label}/{mint}: 키를 만들지 않는다");
            assert_eq!(file(&rig), content, "{label}/{mint}: 파일 불변");
            assert!(!backup_path_for(&rig.path).exists(), "{label}/{mint}");
        }
    }
}

#[test]
fn mint_gate_passes_when_the_file_alone_proves_collection_can_happen() {
    // 범용 엔드포인트만 허브로 설정된 파일도 준비 완료다(신호별 값이 없을 때의 유효 엔드포인트).
    let rig = Rig::new("gate-generic-hub");
    let content = serde_json::json!({"env": {
        "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
        "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
        "OTEL_METRICS_EXPORTER": "otlp,console",
        "OTEL_LOGS_EXPORTER": "otlp",
        "OTEL_EXPORTER_OTLP_ENDPOINT": crate::hub_auth::consts::hub_origin(),
    }})
    .to_string();
    std::fs::write(&rig.path, &content).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert!(headers_of(&rig).contains(&fake_key('K')));
}

#[test]
fn an_already_applied_key_is_recognized_without_the_issuance_gate() {
    // 이미 적용된 상태(prefix 일치)는 키를 새로 만들지 않으므로 수집 준비가 안 돼 있어도 서버 확인은 한다.
    let rig = Rig::new("applied-not-ready");
    std::fs::write(
        &rig.path,
        serde_json::json!({"env": {"OTEL_EXPORTER_OTLP_HEADERS": format!("Authorization=Bearer {}", fake_key('S'))}}).to_string(),
    )
    .unwrap();
    rig.api.push_get(state_has_current(&prefix_of('S'), false));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 0);
}

// ======================= mhd_/mhk_ 비노출 =======================

#[test]
fn device_token_never_reaches_settings_backup_status_or_debug_output() {
    let rig = Rig::new("no-mhd");
    let dev = device_token('D');
    let original = write_ready(&rig.path, Some(&fake_key('O')));
    let engine = LinkEngine::new();
    // 모든 경로를 한 번씩 돈다: 회전 성공, 쓰기 성공, 오류 경로.
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created('N'));
    run(&engine, &rig, Trigger::Startup, T0);
    rig.api.push_get(Err(http(500, "INTERNAL_ERROR")));
    run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
    rig.api.push_get(state_policy(IssuePolicy::Blocked, Some(BlockedReason::Quota)));
    run(&engine, &rig, Trigger::Periodic, T0 + 3 * HOUR);

    let mut texts = vec![
        file(&rig),
        std::fs::read_to_string(backup_path_for(&rig.path)).unwrap(),
        format!("{:?}", engine.view()),
        format!("{:?}", engine.mem),
    ];
    texts.push(original);
    for t in &texts {
        assert!(!t.contains("mhd_") && !t.contains(&dev[4..]), "기기 토큰 노출: {t}");
    }
    // 수집키 원문은 settings.json과 (회전 직전 원본인) 백업에만 있다 — 엔진 상태 Debug에는 없다.
    for t in &texts[2..4] {
        assert!(!t.contains(&fake_key('N')) && !t.contains(&fake_key('O')), "수집키 원문 노출: {t}");
    }
}

// ======================= 계약 C표 상태코드 시나리오: 엔진 ↔ 실제 HTTP 클라이언트 ↔ 목 서버 =======================
//
// 가짜 객체가 아니라 실제 `ReqwestIngestApi`가 127.0.0.1 목 서버와 왕복한다. 응답 해석뿐 아니라 요청 측
// (메서드·경로 정확일치·쿼리 없음·`Authorization: Bearer mhd_` 하나뿐)도 매 시나리오 검증한다.

mod http_scenarios {
    use super::*;
    use crate::otel_hub_client::testing::{json, spawn, Reply};
    use crate::otel_hub_client::{ReqwestIngestApi, CREDENTIAL_PATH, ROTATE_PATH};

    const NONE: &str = "null";

    fn get_body(policy: &str, current_prefix: Option<&str>, blocked: Option<&str>) -> String {
        let current = match current_prefix {
            Some(p) => format!(r#"{{"id":"01k7abc","label":"맑은에이전트 mbp","token_prefix":"{p}","status":"active","last_signal_at_metrics":null,"last_signal_at_logs":null,"scope":{{"id":"otlp.ingest.write"}}}}"#),
            None => NONE.to_string(),
        };
        let blocked = blocked.map(|b| format!(r#""{b}""#)).unwrap_or_else(|| NONE.to_string());
        format!(r#"{{"current":{current},"last_revoked":null,"issue_policy":"{policy}","blocked_reason":{blocked}}}"#)
    }

    fn created_body(fill: char) -> String {
        format!(r#"{{"id":"01k7abc","token_prefix":"{}","label":"x","created_at":"2026-10-15T01:02:03.456Z","token":"{}","env":{{"OTEL_METRICS_EXPORTER":"otlp"}},"notice":"n","scope":{{}}}}"#, prefix_of(fill), fake_key(fill))
    }

    fn err(status: u16, code: &str) -> Reply {
        json(status, &format!(r#"{{"error":{{"code":"{code}","message":"서버 문구"}}}}"#))
    }

    struct Case {
        name: &'static str,
        file_key: Option<char>,
        replies: Vec<Reply>,
        probe: Vec<ProbeResult>,
        phase: Phase,
        /// (메서드, 경로) 순서.
        requests: Vec<(&'static str, &'static str)>,
        /// 설정 파일에 새로 적용된 키의 채움 문자(없으면 키가 안 바뀐다).
        applied: Option<char>,
    }

    fn run_case(case: Case) {
        let path = temp_settings_path("http-scenario");
        let original = write_ready(&path, case.file_key.map(fake_key).as_deref());
        let (origin, seen) = spawn(case.replies);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let tokens = FakeTokens::new(&device_token('D'));
        let probe = FakeProbe::new(&case.probe);
        let env = Env { path: &path, tokens: &tokens, api: &api, probe: &probe, label: "맑은에이전트 test-mac" };
        let engine = LinkEngine::new();

        engine.run_check(&env, Trigger::Startup, T0);

        assert_eq!(engine.view().phase, case.phase, "{}", case.name);
        let seen = seen.lock().unwrap();
        let got: Vec<(String, String)> = seen.iter().map(|s| (s.method.clone(), s.url.clone())).collect();
        let want: Vec<(String, String)> = case.requests.iter().map(|(m, p)| (m.to_string(), p.to_string())).collect();
        assert_eq!(got, want, "{}: 요청 순서·경로(정확일치, 쿼리 없음)", case.name);
        for s in seen.iter() {
            assert!(!s.url.contains('?') && !s.url.ends_with('/'), "{}: {}", case.name, s.url);
            assert_eq!(s.header("authorization"), Some(format!("Bearer {}", device_token('D')).as_str()), "{}", case.name);
            // 토큰은 Authorization 하나로만 간다.
            for (name, value) in &s.headers {
                if name != "authorization" {
                    assert!(!value.contains(&device_token('D')[4..]), "{}: {name}", case.name);
                }
            }
            assert!(!s.body.contains("mhd_"), "{}", case.name);
        }
        match case.applied {
            Some(fill) => assert!(std::fs::read_to_string(&path).unwrap().contains(&fake_key(fill)), "{}", case.name),
            None => assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "{}: 파일 불변", case.name),
        }
        assert_eq!(probe.count(), case.probe.len(), "{}: /self 확인 호출 수", case.name);
        cleanup(&path);
    }

    const GET: (&str, &str) = ("GET", CREDENTIAL_PATH);
    const POST: (&str, &str) = ("POST", CREDENTIAL_PATH);
    const ROTATE: (&str, &str) = ("POST", ROTATE_PATH);

    #[test]
    fn status_code_table_200_and_201() {
        run_case(Case { name: "200 auto → 201 발급", file_key: None,
            replies: vec![json(200, &get_body("auto", None, None)), json(201, &created_body('K'))],
            probe: vec![], phase: Phase::Idle, requests: vec![GET, POST], applied: Some('K') });
        run_case(Case { name: "200 has_current(다름) → 201 rotate", file_key: Some('O'),
            replies: vec![json(200, &get_body("has_current", Some(&prefix_of('Z')), None)), json(201, &created_body('N'))],
            probe: vec![], phase: Phase::Idle, requests: vec![GET, ROTATE], applied: Some('N') });
        run_case(Case { name: "200 has_current(같음) → 요청 1건", file_key: Some('S'),
            replies: vec![json(200, &get_body("has_current", Some(&prefix_of('S')), None))],
            probe: vec![], phase: Phase::Idle, requests: vec![GET], applied: None });
        run_case(Case { name: "200 confirm_required", file_key: None,
            replies: vec![json(200, &get_body("confirm_required", None, None))],
            probe: vec![], phase: Phase::ConfirmRequired, requests: vec![GET], applied: None });
        run_case(Case { name: "200 blocked/quota", file_key: None,
            replies: vec![json(200, &get_body("blocked", None, Some("quota")))],
            probe: vec![], phase: Phase::Blocked(Some(BlockedReason::Quota)), requests: vec![GET], applied: None });
        run_case(Case { name: "200 blocked/suspected_leak", file_key: None,
            replies: vec![json(200, &get_body("blocked", None, Some("suspected_leak")))],
            probe: vec![], phase: Phase::Blocked(Some(BlockedReason::SuspectedLeak)), requests: vec![GET], applied: None });
    }

    #[test]
    fn status_code_table_400_401_403() {
        run_case(Case { name: "400 VALIDATION_ERROR(POST)", file_key: None,
            replies: vec![json(200, &get_body("auto", None, None)), err(400, "VALIDATION_ERROR")],
            probe: vec![], phase: Phase::HubRejected, requests: vec![GET, POST], applied: None });
        run_case(Case { name: "401 + self 200 = 기능 없음", file_key: None,
            replies: vec![err(401, "UNAUTHORIZED")],
            probe: vec![ProbeResult::Alive], phase: Phase::FeatureUnavailable, requests: vec![GET], applied: None });
        run_case(Case { name: "401 + self 401 = 재연결", file_key: None,
            replies: vec![err(401, "UNAUTHORIZED")],
            probe: vec![ProbeResult::Dead], phase: Phase::Reconnect, requests: vec![GET], applied: None });
        run_case(Case { name: "401 + self 5xx/네트워크 = 일시 오류", file_key: None,
            replies: vec![err(401, "UNAUTHORIZED")],
            probe: vec![ProbeResult::Unknown], phase: Phase::Transient, requests: vec![GET], applied: None });
        run_case(Case { name: "401(rotate) + self 200", file_key: None,
            replies: vec![json(200, &get_body("has_current", Some(&prefix_of('Z')), None)), err(401, "UNAUTHORIZED")],
            probe: vec![ProbeResult::Alive], phase: Phase::FeatureUnavailable, requests: vec![GET, ROTATE], applied: None });
        run_case(Case { name: "403 NOT_DESKTOP_TOKEN", file_key: None,
            replies: vec![err(403, "NOT_DESKTOP_TOKEN")],
            probe: vec![], phase: Phase::NotDesktopToken, requests: vec![GET], applied: None });
    }

    #[test]
    fn status_code_table_404_and_409() {
        run_case(Case { name: "404 NO_DEVICE_CREDENTIAL(rotate) → 재판정 → auto → 발급", file_key: None,
            replies: vec![
                json(200, &get_body("has_current", Some(&prefix_of('Z')), None)),
                err(404, "NO_DEVICE_CREDENTIAL"),
                json(200, &get_body("auto", None, None)),
                json(201, &created_body('C')),
            ],
            probe: vec![], phase: Phase::Idle, requests: vec![GET, ROTATE, GET, POST], applied: Some('C') });
        run_case(Case { name: "409 ALREADY_RENEWED", file_key: None,
            replies: vec![err(409, "ALREADY_RENEWED")],
            probe: vec![], phase: Phase::Reconnect, requests: vec![GET], applied: None });
        run_case(Case { name: "409 DEVICE_CREDENTIAL_EXISTS(본문 current) → rotate", file_key: None,
            replies: vec![
                json(200, &get_body("auto", None, None)),
                json(409, &format!(r#"{{"error":{{"code":"DEVICE_CREDENTIAL_EXISTS","message":"m"}},"current":{{"token_prefix":"{}"}}}}"#, prefix_of('X'))),
                json(201, &created_body('N')),
            ],
            probe: vec![], phase: Phase::Idle, requests: vec![GET, POST, ROTATE], applied: Some('N') });
        run_case(Case { name: "409 DEVICE_REISSUE_BLOCKED(blocked_reason)", file_key: None,
            replies: vec![
                json(200, &get_body("auto", None, None)),
                json(409, r#"{"error":{"code":"DEVICE_REISSUE_BLOCKED","message":"m","blocked_reason":"quota"}}"#),
            ],
            probe: vec![], phase: Phase::Blocked(Some(BlockedReason::Quota)), requests: vec![GET, POST], applied: None });
        run_case(Case { name: "409 REISSUE_CONFIRMATION_REQUIRED", file_key: None,
            replies: vec![json(200, &get_body("auto", None, None)), err(409, "REISSUE_CONFIRMATION_REQUIRED")],
            probe: vec![], phase: Phase::ConfirmRequired, requests: vec![GET, POST], applied: None });
        run_case(Case { name: "409 CREDENTIAL_LIMIT", file_key: None,
            replies: vec![json(200, &get_body("auto", None, None)), err(409, "CREDENTIAL_LIMIT")],
            probe: vec![], phase: Phase::CredentialLimit, requests: vec![GET, POST], applied: None });
        run_case(Case { name: "409 ALREADY_ROTATED → GET 재판정(이미 적용)", file_key: Some('S'),
            replies: vec![
                json(200, &get_body("has_current", Some(&prefix_of('Z')), None)),
                err(409, "ALREADY_ROTATED"),
                json(200, &get_body("has_current", Some(&prefix_of('S')), None)),
            ],
            probe: vec![], phase: Phase::Idle, requests: vec![GET, ROTATE, GET], applied: None });
    }

    #[test]
    fn status_code_table_409_token_superseded_rereads_the_token() {
        let path = temp_settings_path("http-superseded");
        write_ready(&path, None);
        let (t1, t2) = (device_token('A'), device_token('B'));
        let (origin, seen) = spawn(vec![
            json(200, &get_body("has_current", Some(&prefix_of('Z')), None)),
            err(409, "TOKEN_SUPERSEDED"),
            json(201, &created_body('N')),
        ]);
        let api = ReqwestIngestApi::new(&origin).unwrap();
        let tokens = FakeTokens::new(&t1).then(&t1).then(&t2);
        let probe = FakeProbe::new(&[]);
        let env = Env { path: &path, tokens: &tokens, api: &api, probe: &probe, label: "l" };
        let engine = LinkEngine::new();
        engine.run_check(&env, Trigger::Startup, T0);
        assert_eq!(engine.view().phase, Phase::Idle);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1].header("authorization"), Some(format!("Bearer {t1}").as_str()));
        assert_eq!(seen[2].header("authorization"), Some(format!("Bearer {t2}").as_str()), "재시도는 키체인에서 다시 읽은 최신 토큰");
        cleanup(&path);
    }

    #[test]
    fn status_code_table_429_and_500() {
        run_case(Case { name: "429 RATE_LIMITED(Retry-After 60)", file_key: None,
            replies: vec![Reply::Respond(429, r#"{"error":{"code":"RATE_LIMITED","message":"m"}}"#.into(), vec![("Retry-After", "60".into())])],
            probe: vec![], phase: Phase::RateLimited, requests: vec![GET], applied: None });
        run_case(Case { name: "429 ROTATE_BUDGET_EXCEEDED(Retry-After + retry_at)", file_key: None,
            replies: vec![
                json(200, &get_body("has_current", Some(&prefix_of('Z')), None)),
                Reply::Respond(429, r#"{"error":{"code":"ROTATE_BUDGET_EXCEEDED","message":"m","retry_at":"2026-10-16T01:02:03.456Z"}}"#.into(), vec![("Retry-After", "3600".into())]),
            ],
            probe: vec![], phase: Phase::RotateBudget { retry_at: Some("2026-10-16T01:02:03.456Z".into()) }, requests: vec![GET, ROTATE], applied: None });
        run_case(Case { name: "500 INTERNAL_ERROR(GET)", file_key: None,
            replies: vec![err(500, "INTERNAL_ERROR")],
            probe: vec![], phase: Phase::Transient, requests: vec![GET], applied: None });
        run_case(Case { name: "500(rotate)", file_key: None,
            replies: vec![json(200, &get_body("has_current", Some(&prefix_of('Z')), None)), err(500, "INTERNAL_ERROR")],
            probe: vec![], phase: Phase::Transient, requests: vec![GET, ROTATE], applied: None });
    }

    #[test]
    fn a_201_with_a_malformed_key_over_http_is_rejected_and_never_written() {
        run_case(Case { name: "201 token 형식 불량", file_key: None,
            replies: vec![
                json(200, &get_body("auto", None, None)),
                json(201, r#"{"id":"x","token_prefix":"mhk_aaaa","token":"mhk_short"}"#),
            ],
            probe: vec![], phase: Phase::InvalidKey, requests: vec![GET, POST], applied: None });
    }

    #[test]
    fn a_dropped_connection_over_http_is_classified_as_lost_for_the_engine() {
        // 재전송 규칙(D-5) 자체는 가짜 API로 고정했다(`d5_rotate_response_lost_*`). 여기서는 실제 HTTP 클라이언트가
        // 요청을 읽은 뒤 끊긴 연결을 `Lost`로 분류한다는 연결 고리만 확인한다.
        let (origin, received) = crate::otel_hub_client::testing::spawn_dropping();
        let api = ReqwestIngestApi::new(&origin).unwrap();
        assert_eq!(api.rotate(&device_token('D')).unwrap_err(), ApiError::Lost);
        assert_eq!(*received.lock().unwrap(), 1);
    }
}

#[test]
fn rv04_per_signal_protocols_and_a_hub_traces_endpoint_pass_the_gate() {
    // 범용 프로토콜이 없어도 metrics·logs 각각이 http/protobuf면 준비된 파일이다(이전에는 NotReady로 잘못 막았다).
    let rig = Rig::new("rv04-pass");
    let hub = crate::hub_auth::consts::hub_origin();
    let content = serde_json::json!({"env": {
        "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
        "OTEL_METRICS_EXPORTER": "otlp",
        "OTEL_LOGS_EXPORTER": "otlp",
        "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL": "http/protobuf",
        "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL": "http/protobuf",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": format!("{hub}/v1/metrics"),
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": format!("{hub}/v1/logs"),
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT": format!("{hub}/v1/traces"),
    }})
    .to_string();
    std::fs::write(&rig.path, &content).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(rig.api.kinds(), ["get", "create:false"]);
    assert!(file(&rig).contains(&fake_key('K')));
}

// ======================= R17 리뷰 RV-01: 폐기 판정은 이전 수신 확인을 지운다 =======================

#[test]
fn rv01_confirm_required_forgets_the_earlier_confirmation_watch_and_timeout() {
    let rig = Rig::new("rv01-engine");
    write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('S'), true));
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.view().confirmed_prefix.as_deref(), Some(prefix_of('S').as_str()));
    engine.with(|m| {
        m.watch = Some(Watch { prefix: prefix_of('S'), started: T0, last_poll: None });
        m.first_data_timed_out = true;
    });

    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + HOUR), Phase::ConfirmRequired);

    let view = engine.view();
    assert_eq!(view.confirmed_prefix, None);
    assert!(!view.waiting_first_data && !view.first_data_timed_out);
    // 거절 뒤에도 마찬가지다(점검이 다시 거절 상태로 끝나도 비워진다).
    engine.with(|m| m.confirmed_prefix = Some(prefix_of('S')));
    engine.decline_confirm();
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    assert_eq!(run(&engine, &rig, Trigger::Periodic, T0 + 2 * HOUR), Phase::ConfirmDeclined);
    assert_eq!(engine.view().confirmed_prefix, None);
}

// ======================= R17 리뷰 RV-03: D-12 15분 타임아웃 뒤에는 GET 1회씩만 =======================

#[test]
fn rv03_periodic_checks_after_the_fifteen_minute_timeout_do_not_restart_polling() {
    let rig = Rig::new("rv03-periodic");
    let engine = applied_engine_with_watch(&rig); // 키 K 적용 + 첫 데이터 확인 시작(T0)
    engine.poll_first_data(&rig.env(), T0 + WATCH_LIMIT_MS + 1); // 15분 경과 — 호출 없이 중단
    assert!(engine.view().first_data_timed_out && !engine.view().waiting_first_data);
    let gets_before = rig.api.count("get");

    // 이후 6번의 정기 점검(6시간): 점검마다 GET 1회뿐이고 15분 폴링은 다시 열리지 않는다.
    for hour in 1..=6u64 {
        let at = T0 + hour * HOUR;
        rig.api.push_get(state_has_current(&prefix_of('K'), false));
        assert_eq!(engine.due(at), Some(Trigger::Periodic));
        assert_eq!(run(&engine, &rig, Trigger::Periodic, at), Phase::Idle);
        let view = engine.view();
        assert!(!view.waiting_first_data, "{hour}시간 뒤: 폴링을 다시 열면 안 된다");
        assert!(view.first_data_timed_out, "{hour}시간 뒤: 타임아웃 안내는 유지된다");
        for t in [1_000, 16_000, 31_000, 5 * MIN] {
            engine.poll_first_data(&rig.env(), at + t);
        }
    }
    assert_eq!(rig.api.count("get") - gets_before, 6, "정기 점검 GET 6회가 전부다(키당 하루 약 1,460회가 아니라 24회)");

    // 신호가 도착하면 정기 점검의 GET 1회로 확인된다.
    rig.api.push_get(state_has_current(&prefix_of('K'), true));
    run(&engine, &rig, Trigger::Periodic, T0 + 7 * HOUR);
    let view = engine.view();
    assert_eq!(view.confirmed_prefix.as_deref(), Some(prefix_of('K').as_str()));
    assert!(!view.first_data_timed_out && !view.waiting_first_data);
}

#[test]
fn rv03_user_retry_and_a_fresh_start_reopen_the_first_data_check() {
    // 사용자의 [다시 시도] — 명시적 요청이므로 타임아웃 뒤에도 다시 확인을 시작한다.
    let rig = Rig::new("rv03-user");
    let engine = applied_engine_with_watch(&rig);
    engine.poll_first_data(&rig.env(), T0 + WATCH_LIMIT_MS + 1);
    rig.api.push_get(state_has_current(&prefix_of('K'), false));
    run(&engine, &rig, Trigger::User, T0 + HOUR);
    let view = engine.view();
    assert!(view.waiting_first_data && !view.first_data_timed_out);

    // 앱 재기동(메모리 없음)의 첫 점검도 다시 시작한다 — 기존 d12_startup_* 와 같은 동작.
    let fresh = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('K'), false));
    run(&fresh, &rig, Trigger::Startup, T0 + 2 * HOUR);
    assert!(fresh.view().waiting_first_data);
}

// ======================= R17 리뷰 RV-05: 응답을 기다리는 동안 연결이 바뀌어도 해제된 키가 되살아나지 않는다 =======================

/// 서버 호출이 오가는 도중(요청을 보낸 뒤 응답을 돌려주기 전) 연결 해제·재연결이 일어나는 상황을 만드는 가짜 API.
struct ConnectionChangesMidCall<'a> {
    engine: &'a LinkEngine,
    path: std::path::PathBuf,
    inner: &'a FakeApi,
    /// 어느 호출에서 연결을 바꿀지("get"|"create"|"rotate").
    on: &'static str,
    /// true면 재연결, false면 연결 해제(키 삭제 포함).
    reconnect: bool,
}

impl ConnectionChangesMidCall<'_> {
    fn change(&self, kind: &str) {
        if kind != self.on {
            return;
        }
        if self.reconnect {
            self.engine.on_connected();
        } else {
            // 실제 해제 경로와 같다: 세대 증가 + 메모리 비우기 + Authorization 항목 삭제를 한 임계구역에서.
            let path = self.path.clone();
            let _ = self.engine.disconnect_with(T0 + 1, || crate::otel_settings::remove_ingest_key(&path));
        }
    }
}

impl IngestApi for ConnectionChangesMidCall<'_> {
    fn get(&self, device_token: &str) -> GetReply {
        let r = self.inner.get(device_token);
        self.change("get");
        r
    }
    fn create(&self, device_token: &str, label: &str, confirm: bool) -> CreateReply {
        let r = self.inner.create(device_token, label, confirm);
        self.change("create");
        r
    }
    fn rotate(&self, device_token: &str) -> CreateReply {
        let r = self.inner.rotate(device_token);
        self.change("rotate");
        r
    }
}

fn run_with_api(engine: &LinkEngine, rig: &Rig, api: &dyn IngestApi, trigger: Trigger, now: u64) {
    let env = Env { path: &rig.path, tokens: &rig.tokens, api, probe: &rig.probe, label: &rig.label };
    engine.run_check(&env, trigger, now);
}

#[test]
fn rv05_a_created_key_that_arrives_after_disconnect_is_not_written_back() {
    let rig = Rig::new("rv05-create");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "create", reconnect: false };

    run_with_api(&engine, &rig, &api, Trigger::Startup, T0);

    assert!(!file(&rig).contains("mhk_"), "해제 뒤에 도착한 201이 키를 다시 쓰면 안 된다: {}", file(&rig));
    let view = engine.view();
    assert_eq!(view.phase, Phase::NoDeviceToken, "낡은 점검이 해제 상태를 Idle 등으로 덮으면 안 된다");
    assert!(!view.waiting_first_data && view.confirmed_prefix.is_none());
    assert!(engine.with(|m| m.pending_write.is_none()), "버린 키 원문을 메모리에 보관하지 않는다");
    assert_eq!(engine.due(T0 + 2), None, "연결이 없으니 점검을 다시 예약하지 않는다");
}

#[test]
fn rv05_a_rotated_key_that_arrives_after_disconnect_is_not_written_and_the_old_key_stays_removed() {
    let rig = Rig::new("rv05-rotate");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('Z'), true));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "rotate", reconnect: false };

    run_with_api(&engine, &rig, &api, Trigger::Startup, T0);

    let saved = file(&rig);
    assert!(!saved.contains("mhk_"), "해제로 지운 키도, 뒤늦게 온 새 키도 없어야 한다: {saved}");
    assert_eq!(engine.view().phase, Phase::NoDeviceToken);
    assert_eq!(rig.api.count("rotate"), 1, "해제 뒤 응답 유실 재시도 같은 추가 요청을 보내지 않는다");
}

#[test]
fn rv05_a_created_201_carrying_env_that_arrives_after_disconnect_writes_neither_key_nor_env() {
    // 해제 뒤에 도착한 201의 env도 파일에 닿지 않는다 — 키와 env는 같은 쓰기라 세대 확인에서 함께 버려진다.
    let rig = Rig::new("rv05-create-env");
    let original = write_ready(&rig.path, None); // 파일의 로그 주기는 5000
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', contract_env()));
    let engine = LinkEngine::new();
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "create", reconnect: false };

    run_with_api(&engine, &rig, &api, Trigger::Startup, T0);

    assert!(!file(&rig).contains("mhk_"), "{}", file(&rig));
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("5000"), "서버 값(60000)이 해제 뒤에 쓰이면 안 된다");
    assert_eq!(env_str(&rig, "OTEL_METRIC_EXPORT_INTERVAL"), None);
    assert_eq!(env_str(&rig, "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE"), None);
    let before: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(file_json(&rig)["env"], before["env"], "env 전체가 해제 전 그대로");
    let view = engine.view();
    assert_eq!(view.phase, Phase::NoDeviceToken);
    assert!(view.env_changed.is_empty(), "버린 응답의 바뀐 키를 화면에 알리면 안 된다: {:?}", view.env_changed);
    assert!(engine.with(|m| m.pending_write.is_none()), "버린 키·env를 메모리에 보관하지 않는다");
}

#[test]
fn rv05_a_lost_rotate_response_does_not_trigger_a_retry_after_disconnect() {
    let rig = Rig::new("rv05-lost");
    write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('Z'), true));
    rig.api.push_rotate(Err(ApiError::Lost));
    let engine = LinkEngine::new();
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "rotate", reconnect: false };

    run_with_api(&engine, &rig, &api, Trigger::Startup, T0);

    assert_eq!(rig.api.count("rotate"), 1, "해제된 연결의 토큰으로 D-5 재시도 회전을 보내면 예산만 쓴다");
    assert_eq!(engine.view().phase, Phase::NoDeviceToken);
    assert_eq!(engine.with(|m| m.lost_checks), 0);
}

#[test]
fn rv05_a_check_from_the_previous_connection_cannot_overwrite_the_new_connection() {
    let rig = Rig::new("rv05-reconnect");
    let original = write_ready(&rig.path, Some(&fake_key('S')));
    rig.api.push_get(state_has_current(&prefix_of('Z'), true));
    rig.api.push_rotate(created('N'));
    let engine = LinkEngine::new();
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "rotate", reconnect: true };

    run_with_api(&engine, &rig, &api, Trigger::Startup, T0);

    assert_eq!(file(&rig), original, "이전 연결의 201은 새 연결의 파일을 바꾸지 않는다");
    assert_eq!(engine.view().phase, Phase::Idle, "재연결이 비운 상태를 낡은 결과가 덮지 않는다");
    assert_eq!(engine.due(T0 + 1), Some(Trigger::Connected), "재연결의 첫 점검은 그대로 예약돼 있다");
}

#[test]
fn rv05_a_poll_answered_after_disconnect_does_not_confirm_anything() {
    let rig = Rig::new("rv05-poll");
    let engine = applied_engine_with_watch(&rig);
    rig.api.push_get(state_has_current(&prefix_of('K'), true));
    let api = ConnectionChangesMidCall { engine: &engine, path: rig.path.clone(), inner: &rig.api, on: "get", reconnect: false };
    let env = Env { path: &rig.path, tokens: &rig.tokens, api: &api, probe: &rig.probe, label: &rig.label };

    engine.poll_first_data(&env, T0 + 1_000);

    let view = engine.view();
    assert_eq!(view.confirmed_prefix, None);
    assert!(!view.waiting_first_data);
    assert!(!file(&rig).contains("mhk_"));
}

#[test]
fn rv05_disconnect_without_a_check_in_flight_still_clears_key_and_memory_in_one_step() {
    let rig = Rig::new("rv05-plain");
    let engine = applied_engine_with_watch(&rig);
    assert!(file(&rig).contains(&fake_key('K')));
    let removed = engine.disconnect_with(T0 + 5, || crate::otel_settings::remove_ingest_key(&rig.path));
    assert_eq!(removed, Ok(true));
    assert!(!file(&rig).contains("mhk_"));
    let view = engine.view();
    assert_eq!((view.phase, view.waiting_first_data), (Phase::NoDeviceToken, false));
}


// ======================= R19: 201 `env` 적용 =======================

fn env_str(rig: &Rig, key: &str) -> Option<String> {
    file_json(rig)["env"].get(key).and_then(|v| v.as_str()).map(str::to_string)
}

#[cfg(unix)]
fn mode_of(p: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn r19_issue_201_writes_the_key_and_the_six_server_env_keys_in_one_write_and_keeps_the_rest() {
    let rig = Rig::new("r19-issue");
    let original = write_ready(&rig.path, None); // 파일에는 앱 옛 기본값 5000, METRIC 주기·temporality는 없음
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', contract_env()));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    let json = file_json(&rig);
    assert_eq!(json["env"]["OTEL_EXPORTER_OTLP_HEADERS"], format!("Authorization=Bearer {}", fake_key('K')));
    for (k, v) in contract_env().as_object().unwrap() {
        assert_eq!(json["env"][k.as_str()], *v, "{k}");
    }
    // 서버 값이 앱 옛 기본값 5000을 이긴다.
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("60000"));
    // 목록 밖의 값·최상위 설정은 그대로.
    assert_eq!(json["env"]["OTEL_RESOURCE_ATTRIBUTES"], "employee.id=dev,team=platform");
    assert_eq!(json["env"]["SOME_OTHER_VAR"], "keep-me");
    assert_eq!(json["env"]["OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"], hub_endpoint("/v1/logs"));
    assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
    // 백업은 저장 직전 원본 1세대(키 없는 원본이므로 새 키가 백업에 있을 수 없다), 0600.
    assert_eq!(std::fs::read_to_string(backup_path_for(&rig.path)).unwrap(), original);
    #[cfg(unix)]
    {
        assert_eq!(mode_of(&rig.path), 0o600);
        assert_eq!(mode_of(&backup_path_for(&rig.path)), 0o600);
    }
    // 바뀐 키 이름(이전 파일 값과 달랐던 것)만 상태로 나온다: 5000→60000, 새로 생긴 METRIC 주기·temporality.
    let view = engine.view();
    assert_eq!(
        view.env_changed,
        [
            "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
            "OTEL_LOGS_EXPORT_INTERVAL",
            "OTEL_METRIC_EXPORT_INTERVAL"
        ]
    );
    assert!(view.env_rejected.is_empty());
}

#[test]
fn r19_rotate_201_applies_server_env_too_and_the_server_value_wins_over_a_user_value() {
    let rig = Rig::new("r19-rotate-conflict");
    // 사용자가 정한 값(30000)과 옛 키가 있다. 서버는 60000을 보낸다 → 서버가 이긴다.
    let mut v: serde_json::Value = serde_json::from_str(&ready_settings(Some(&fake_key('O')))).unwrap();
    v["env"]["OTEL_LOGS_EXPORT_INTERVAL"] = "30000".into();
    v["env"]["OTEL_METRIC_EXPORT_INTERVAL"] = "120000".into();
    std::fs::write(&rig.path, v.to_string()).unwrap();
    rig.api.push_get(state_has_current(&prefix_of('P'), false));
    rig.api.push_rotate(created_with_env('N', contract_env()));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("60000"));
    assert_eq!(env_str(&rig, "OTEL_METRIC_EXPORT_INTERVAL").as_deref(), Some("60000"));
    assert_eq!(
        env_str(&rig, "OTEL_EXPORTER_OTLP_HEADERS").unwrap(),
        format!("x-team=a,Authorization=Bearer {}", fake_key('N')),
        "Authorization 항목만 교체되고 다른 헤더 항목은 그대로"
    );
    assert_eq!(
        engine.view().env_changed,
        [
            "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
            "OTEL_LOGS_EXPORT_INTERVAL",
            "OTEL_METRIC_EXPORT_INTERVAL"
        ]
    );
}

#[test]
fn r19_rotation_with_env_keeps_the_new_key_and_the_server_values_out_of_the_backup() {
    // 백업은 저장 직전 원본 1세대다(롤백 가치) — 회전 뒤 48시간 유예(또는 다음 회전)까지 유효한 옛 키가 거기 한 세대 남는 것은
    // 설계다(본 파일과 같은 0600, 수집키 삭제(Clear)일 때만 백업에서도 뺀다).
    // 지켜야 할 것은 반대 방향: env가 실린 회전에서도 새 키 원문과 서버 값이 백업으로 새지 않는다.
    let rig = Rig::new("r19-rotate-backup");
    let mut v: serde_json::Value = serde_json::from_str(&ready_settings(Some(&fake_key('O')))).unwrap();
    v["env"]["OTEL_LOGS_EXPORT_INTERVAL"] = "30000".into();
    let original = v.to_string();
    std::fs::write(&rig.path, &original).unwrap();
    rig.api.push_get(state_has_current(&prefix_of('P'), false));
    rig.api.push_rotate(created_with_env('N', contract_env()));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    let backup = std::fs::read_to_string(backup_path_for(&rig.path)).unwrap();
    assert_eq!(backup, original, "백업은 저장 직전 원본 그대로");
    assert!(!backup.contains(&fake_key('N')), "새 키 원문이 백업에 있으면 안 된다");
    let backup_json: serde_json::Value = serde_json::from_str(&backup).unwrap();
    assert_eq!(backup_json["env"]["OTEL_LOGS_EXPORT_INTERVAL"], "30000", "서버 값(60000)이 백업에 있으면 안 된다");
    assert!(backup_json["env"].get("OTEL_METRIC_EXPORT_INTERVAL").is_none());
    // 본 파일은 새 키 + 서버 값, 옛 키는 없다.
    assert!(!file(&rig).contains(&fake_key('O')), "{}", file(&rig));
    assert!(headers_of(&rig).contains(&fake_key('N')));
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("60000"));
    #[cfg(unix)]
    assert_eq!(mode_of(&backup_path_for(&rig.path)), 0o600);
}

#[test]
fn r19_keys_the_server_does_not_send_keep_their_file_values() {
    // 서버가 일부 키만 보내면 그 키만 바뀐다 — 보내지 않은 키(여기서는 주기)는 사용자의 값이 남는다.
    let rig = Rig::new("r19-partial-send");
    let mut v: serde_json::Value = serde_json::from_str(&ready_settings(None)).unwrap();
    v["env"]["OTEL_LOGS_EXPORT_INTERVAL"] = "30000".into();
    std::fs::write(&rig.path, v.to_string()).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', serde_json::json!({ "OTEL_METRIC_EXPORT_INTERVAL": "90000" })));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("30000"));
    assert_eq!(env_str(&rig, "OTEL_METRIC_EXPORT_INTERVAL").as_deref(), Some("90000"));
}

#[test]
fn r19_disallowed_keys_and_privacy_keys_never_reach_the_file_but_the_key_is_still_applied() {
    let rig = Rig::new("r19-reject");
    let original = write_ready(&rig.path, None);
    // 가짜 키도 리터럴로 두지 않는다(pre-push 시크릿 스캔 패턴과 같은 형태가 된다).
    let secret_header_owned = format!("Authorization=Bearer mhk_{}", "LEAK".repeat(11));
    let secret_header = secret_header_owned.as_str();
    let mut env = contract_env();
    for (k, v) in [
        ("OTEL_EXPORTER_OTLP_HEADERS", secret_header),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://evil.example"),
        ("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT", "https://evil.example/v1/metrics"),
        ("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", "https://evil.example/v1/logs"),
        ("OTEL_RESOURCE_ATTRIBUTES", "employee.id=evil"),
        ("OTEL_LOG_USER_PROMPTS", "1"),
        ("OTEL_LOG_TOOL_CONTENT", "1"),
        ("OTEL_LOG_TOOL_DETAILS", "1"),
        ("OTEL_LOG_RAW_API_BODIES", "1"),
        ("CLAUDE_CODE_ENABLE_TELEMETRY", "0"),
        ("ANTHROPIC_BASE_URL", "https://evil.example"),
    ] {
        env[k] = v.into();
    }
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', env));
    let engine = LinkEngine::new();
    // (a) env 거부는 수집키 적용을 실패시키지 않는다.
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);

    let after = file_json(&rig);
    let before: serde_json::Value = serde_json::from_str(&original).unwrap();
    // 헤더는 우리 Authorization 하나뿐(서버가 보낸 헤더 값이 섞이지 않음), 나머지 금지 키는 파일 값 그대로/미생성.
    assert_eq!(after["env"]["OTEL_EXPORTER_OTLP_HEADERS"], format!("Authorization=Bearer {}", fake_key('K')));
    for k in [
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        "OTEL_RESOURCE_ATTRIBUTES",
        "CLAUDE_CODE_ENABLE_TELEMETRY",
    ] {
        assert_eq!(after["env"][k], before["env"][k], "{k}");
    }
    for k in [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_LOG_USER_PROMPTS",
        "OTEL_LOG_TOOL_CONTENT",
        "OTEL_LOG_TOOL_DETAILS",
        "OTEL_LOG_RAW_API_BODIES",
        "ANTHROPIC_BASE_URL",
    ] {
        assert!(after["env"].get(k).is_none(), "{k}가 쓰였다");
    }
    let raw = file(&rig);
    assert!(!raw.contains("evil.example") && !raw.contains("LEAKLEAK"), "서버가 보낸 값이 파일에 섞였다");
    // (c) 부분 적용: 허용 목록 안의 올바른 값은 같은 201에서 그대로 쓰였다.
    assert_eq!(after["env"]["OTEL_LOGS_EXPORT_INTERVAL"], "60000");
    // (b) 거부된 키 이름은 상태로 관측되고, 값은 상태·Debug 어디에도 없다.
    let view = engine.view();
    assert_eq!(view.env_rejected.len(), 11);
    assert!(view.env_rejected.contains(&"OTEL_EXPORTER_OTLP_HEADERS".to_string()));
    assert!(view.env_rejected.contains(&"OTEL_LOG_USER_PROMPTS".to_string()));
    let dump = format!("{view:?} {:?}", engine.mem.lock().unwrap());
    for leak in ["evil.example", "LEAKLEAK", "Bearer"] {
        assert!(!dump.contains(leak), "{leak} 노출: {dump}");
    }
}

#[test]
fn r19_format_violations_are_not_written_and_do_not_fail_the_key() {
    let rig = Rig::new("r19-format");
    let original = write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env(
        'K',
        serde_json::json!({
            "OTEL_EXPORTER_OTLP_PROTOCOL": "grpc",
            "OTEL_METRICS_EXPORTER": "none",
            "OTEL_LOGS_EXPORTER": ["otlp"],
            "OTEL_LOGS_EXPORT_INTERVAL": 60000,
            "OTEL_METRIC_EXPORT_INTERVAL": "1",
            "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE": "cumulative\nOTEL_X=1"
        }),
    ));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    let after = file_json(&rig);
    let before: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(after["env"]["OTEL_EXPORTER_OTLP_HEADERS"], format!("Authorization=Bearer {}", fake_key('K')));
    for k in [
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        "OTEL_METRICS_EXPORTER",
        "OTEL_LOGS_EXPORTER",
        "OTEL_LOGS_EXPORT_INTERVAL",
    ] {
        assert_eq!(after["env"][k], before["env"][k], "{k}는 파일 값 그대로");
    }
    assert!(after["env"].get("OTEL_METRIC_EXPORT_INTERVAL").is_none());
    assert!(after["env"].get("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE").is_none());
    assert_eq!(engine.view().env_rejected.len(), 6);
    assert!(engine.view().env_changed.is_empty());
}

#[test]
fn r19_missing_null_empty_and_non_object_env_apply_the_key_only() {
    let cases: Vec<(&str, Option<serde_json::Value>)> = vec![
        ("absent", None),
        ("null", Some(serde_json::Value::Null)),
        ("empty", Some(serde_json::json!({}))),
        ("array", Some(serde_json::json!(["OTEL_LOGS_EXPORT_INTERVAL", "60000"]))),
        ("string", Some(serde_json::json!("OTEL_LOGS_EXPORT_INTERVAL=60000"))),
    ];
    for (name, env) in cases {
        let rig = Rig::new(&format!("r19-env-{name}"));
        write_ready(&rig.path, None);
        rig.api.push_get(state_policy(IssuePolicy::Auto, None));
        match env {
            Some(e) => rig.api.push_create(created_with_env('K', e)),
            None => rig.api.push_create(created('K')),
        };
        let engine = LinkEngine::new();
        assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle, "{name}");
        assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("5000"), "{name}: 파일의 기존 값은 그대로");
        assert!(headers_of(&rig).contains(&fake_key('K')), "{name}");
        assert!(engine.view().env_changed.is_empty(), "{name}");
    }
}

#[test]
fn r19_response_without_the_env_field_at_all_parses_and_applies_the_key() {
    // 서버가 `env` 필드를 아예 보내지 않는 응답(구버전 서버)도 키는 적용된다.
    let rig = Rig::new("r19-no-field");
    write_ready(&rig.path, None);
    let body = serde_json::json!({ "id": "x", "token": fake_key('K') }).to_string();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(crate::otel_hub_client::parse_created(&body));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert!(headers_of(&rig).contains(&fake_key('K')));
}

#[cfg(unix)]
#[test]
fn r19_a_failed_write_leaves_neither_the_key_nor_the_env_and_the_retry_writes_both_at_once() {
    let rig = Rig::new("r19-atomic");
    let original = write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    make_dir_read_only(&rig);
    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created_with_env('N', contract_env()));
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::WriteFailed);
    assert_eq!(file(&rig), original, "키도 env도 쓰이지 않았다(중간 상태 없음)");
    assert!(engine.view().env_changed.is_empty());

    // 다시 쓸 때는 같은 201의 env가 함께 쓰인다(회전은 더 일어나지 않는다).
    make_dir_writable(&rig);
    rig.api.push_get(state_has_current(&prefix_of('N'), false));
    assert_eq!(run(&engine, &rig, Trigger::User, T0 + MIN), Phase::Idle);
    assert_eq!(rig.api.count("rotate"), 1);
    assert!(headers_of(&rig).contains(&fake_key('N')));
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("60000"));
    assert_eq!(env_str(&rig, "OTEL_METRIC_EXPORT_INTERVAL").as_deref(), Some("60000"));
    assert!(!engine.view().env_changed.is_empty());
}

#[test]
fn r19_without_a_device_token_nothing_is_requested_so_no_env_is_written() {
    let mut rig = Rig::new("r19-no-mhd");
    let original = write_ready(&rig.path, None);
    rig.tokens = FakeTokens::none();
    // 큐에 환경 포함 201이 있어도 호출되지 않는다.
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', contract_env()));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::NoDeviceToken);
    assert!(rig.api.calls().is_empty());
    assert_eq!(file(&rig), original);
    assert!(!backup_path_for(&rig.path).exists());
    let view = engine.view();
    assert!(view.env_changed.is_empty() && view.env_rejected.is_empty());
}

#[test]
fn r19_a_check_that_finds_the_key_already_applied_does_not_rewrite_env() {
    // 201이 없는 점검(이미 적용됨)은 env를 건드리지 않는다 — 옛 기본값 5000도 그대로(조용한 마이그레이션 없음).
    let rig = Rig::new("r19-no-201");
    let original = write_ready(&rig.path, Some(&fake_key('O')));
    rig.api.push_get(state_has_current(&prefix_of('O'), false));
    let engine = LinkEngine::new();
    assert_eq!(run(&engine, &rig, Trigger::Startup, T0), Phase::Idle);
    assert_eq!(file(&rig), original);
    assert_eq!(env_str(&rig, "OTEL_LOGS_EXPORT_INTERVAL").as_deref(), Some("5000"));
}

#[test]
fn r19_a_new_201_replaces_the_previous_env_report() {
    let rig = Rig::new("r19-report-reset");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', serde_json::json!({ "OTEL_RESOURCE_ATTRIBUTES": "x", "OTEL_LOGS_EXPORT_INTERVAL": "60000" })));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(engine.view().env_rejected, ["OTEL_RESOURCE_ATTRIBUTES"]);
    assert_eq!(engine.view().env_changed, ["OTEL_LOGS_EXPORT_INTERVAL"]);

    rig.api.push_get(state_has_current(&prefix_of('Z'), false));
    rig.api.push_rotate(created_with_env('N', serde_json::json!({ "OTEL_LOGS_EXPORT_INTERVAL": "60000" })));
    run(&engine, &rig, Trigger::User, T0 + HOUR);
    assert!(engine.view().env_rejected.is_empty(), "이전 201의 거부 기록이 남지 않는다");
    assert!(engine.view().env_changed.is_empty(), "이미 60000이라 바뀐 키가 없다");
}

#[test]
fn r19_disconnect_clears_the_env_report() {
    let rig = Rig::new("r19-disconnect");
    write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created_with_env('K', serde_json::json!({ "OTEL_LOG_USER_PROMPTS": "1" })));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert!(!engine.view().env_rejected.is_empty());
    engine.disconnect_with(T0 + MIN, || ());
    assert!(engine.view().env_rejected.is_empty());
}
