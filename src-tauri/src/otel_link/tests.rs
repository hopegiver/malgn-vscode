// 연동 상태 모델·발급 전 선검사·label·연결 해제·mhd_/mhk_ 비노출 테스트.
// (허브 계약 D-1~D-15 동작은 `otel_link_engine/tests.rs`가 규칙 번호별로 고정한다.)

use super::*;
use crate::fs_atomic::backup_path_for;
use crate::otel_hub_client::{ApiError, IssuePolicy};
use crate::otel_ingest_key::EndpointAssessment;
use crate::otel_link_engine::ProbeResult;
use crate::otel_link_engine::testkit::*;
use crate::otel_settings::{
    apply_ingest_key, read_ingest_inputs, remove_ingest_key, save_to_settings_file, OtelSavePayload,
};

const ALL_CAUSES: [LinkFailureCause; 16] = [
    LinkFailureCause::SettingsUnreadable,
    LinkFailureCause::HeaderUnrecognized,
    LinkFailureCause::EndpointNotHub,
    LinkFailureCause::NotReady,
    LinkFailureCause::FeatureUnavailable,
    LinkFailureCause::ReconnectRequired,
    LinkFailureCause::NotDesktopToken,
    LinkFailureCause::HubRejected,
    LinkFailureCause::Unreachable,
    LinkFailureCause::RateLimited,
    LinkFailureCause::InvalidKeyReturned,
    LinkFailureCause::ApplyFailed,
    LinkFailureCause::Blocked,
    LinkFailureCause::CredentialLimit,
    LinkFailureCause::RotateBudgetExceeded,
    LinkFailureCause::RotationLost,
];

const ORIGINAL: &str =
    r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"SOME_OTHER_VAR":"keep-me"}}"#;

const T0: u64 = 1_000_000;

fn status_of(path: &Path) -> IngestLinkStatus {
    derive_link_status(&read_ingest_inputs(path))
}

fn idle_view() -> RuntimeView {
    RuntimeView {
        phase: Phase::Idle,
        confirmed_prefix: None,
        waiting_first_data: false,
        first_data_timed_out: false,
        revoked_prefix: None,
        env_changed: Vec::new(),
        env_rejected: Vec::new(),
    }
}

/// 상태를 직렬화·Debug 두 형태로 모두 검사한다 — 기기 토큰과 `forbidden`이 어디에도 없다.
fn assert_status_clean(label: &str, status: &IngestLinkStatus, forbidden: &[&str]) {
    let dev = device_token('D');
    let json = serde_json::to_string(status).unwrap();
    let dbg = format!("{status:?}");
    for text in [&json, &dbg] {
        assert!(!text.contains("mhd_"), "{label}: mhd_ 접두어 노출: {text}");
        assert!(!text.contains(&dev), "{label}: 기기 토큰 원문 노출: {text}");
        assert!(!text.contains("DDDD"), "{label}: 기기 토큰 본문 노출: {text}");
        for f in forbidden {
            assert!(!text.contains(f), "{label}: {f:?} 노출: {text}");
        }
    }
}

fn assert_no_device_token(label: &str, text: &str) {
    let dev = device_token('D');
    assert!(!text.contains("mhd_"), "{label}: mhd_ 접두어 노출: {text}");
    assert!(!text.contains(&dev), "{label}: 기기 토큰 원문 노출: {text}");
    assert!(!text.contains("DDDD"), "{label}: 기기 토큰 본문 노출: {text}");
}

fn run(engine: &LinkEngine, rig: &Rig, trigger: Trigger, now: u64) {
    engine.run_check(&rig.env(), trigger, now);
}

fn status_after(engine: &LinkEngine, rig: &Rig) -> IngestLinkStatus {
    compose_status(&read_ingest_inputs(&rig.path), &engine.view(), true, true)
}

// ======================= mhd_ / mhk_ 비노출 (R16 분리 테스트 확장) =======================

#[test]
fn issued_key_flow_never_contains_device_token() {
    let rig = Rig::new("link-issue-ok");
    // 수집 준비가 된 파일(텔레메트리·exporter·프로토콜·허브 엔드포인트) — 키만 비어 있다.
    let original = write_ready(&rig.path, None);
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();

    run(&engine, &rig, Trigger::Startup, T0);

    // 반환 상태: 적용됨 + 마스킹만. 원문·기기 토큰 없음. 수집 준비도 완료.
    let status = status_after(&engine, &rig);
    assert_eq!(status.state, IngestLinkState::Applied);
    assert_eq!(status.masked.as_deref(), Some("mhk_KKKK…"));
    assert!(status.failure.is_none());
    assert!(status.collection_ready && status.blockers.is_empty());
    assert!(status.waiting_first_data);
    assert_status_clean("status", &status, &[&fake_key('K'), "KKKKK"]);

    // settings.json: mhk_ 키만, 정규 형식으로, 기존 설정 보존.
    let saved = std::fs::read_to_string(&rig.path).unwrap();
    assert_no_device_token("settings.json", &saved);
    assert_eq!(saved.matches("mhk_").count(), 1, "{saved}");
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(json["env"]["OTEL_EXPORTER_OTLP_HEADERS"], format!("Authorization=Bearer {}", fake_key('K')));
    assert_eq!(json["env"]["SOME_OTHER_VAR"], "keep-me");
    assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");

    // 백업(롤링 1세대): 저장 직전 원본 그대로 — 키도 기기 토큰도 없다.
    let backup = std::fs::read_to_string(backup_path_for(&rig.path)).unwrap();
    assert_eq!(backup, original);
    assert_no_device_token("backup", &backup);

    // 회전: 두 번째 점검(서버의 현행 키가 다르다) 뒤 백업에는 직전 mhk_ 키가 있을 수 있으나 mhd_는 여전히 없다.
    rig.api.push_get(state_has_current(&prefix_of('Q'), false));
    rig.api.push_rotate(created('R'));
    run(&engine, &rig, Trigger::Periodic, T0 + 3_600_000);
    assert_eq!(status_after(&engine, &rig).masked.as_deref(), Some("mhk_RRRR…"));
    for file in [rig.path.clone(), backup_path_for(&rig.path)] {
        let text = std::fs::read_to_string(&file).unwrap();
        assert_no_device_token(&file.to_string_lossy(), &text);
        assert_eq!(text.matches("mhk_").count(), 1, "{text}");
    }
    // 서버로 간 모든 호출은 기기 토큰으로 인증했지만(가짜 서버 기록), 그것이 파일·상태로 오지는 않았다.
    assert!(rig.api.calls().iter().all(|c| c.ends_with(&device_token('D'))));
}

#[test]
fn every_engine_phase_composes_into_a_status_without_device_token_or_key_material() {
    let rig = Rig::new("link-phases");
    write_ready(&rig.path, Some(&fake_key('K')));
    let inputs = read_ingest_inputs(&rig.path);
    let phases = vec![
        Phase::Idle,
        Phase::Checking,
        Phase::NoDeviceToken,
        Phase::ConfirmRequired,
        Phase::ConfirmDeclined,
        Phase::Blocked(Some(BlockedReason::Quota)),
        Phase::Blocked(None),
        Phase::CredentialLimit,
        Phase::RotateBudget { retry_at: Some("2026-10-16T01:02:03.456Z".into()) },
        Phase::RotationLost,
        Phase::Reconnect,
        Phase::FeatureUnavailable,
        Phase::WriteFailed,
        Phase::NotDesktopToken,
        Phase::HubRejected,
        Phase::Transient,
        Phase::RateLimited,
        Phase::NotReady,
        Phase::EndpointNotHub,
        Phase::InvalidKey,
    ];
    for phase in phases {
        let view = RuntimeView { phase: phase.clone(), ..idle_view() };
        let status = compose_status(&inputs, &view, true, true);
        assert_status_clean(&format!("{phase:?}"), &status, &["KKKKK", "x-team", "keep-me"]);
        if let Some(f) = &status.failure {
            assert_eq!(f.message, f.cause.message());
        }
    }
}

#[test]
fn failure_messages_never_contain_token_prefixes_or_forbidden_claims() {
    for cause in ALL_CAUSES {
        let m = cause.message();
        assert!(!m.contains("mhd_") && !m.contains("mhk_"), "{cause:?}: {m}");
        assert!(!m.is_empty());
        // 시도하지 않은 자동 동작을 말하지 않는다.
        assert!(!m.contains("바꾸지 않았습니다") && !m.contains("자동으로 교체") && !m.contains("자동으로 삭제"), "{cause:?}: {m}");
    }
}

#[test]
fn unreadable_settings_stops_before_issuing_and_never_contains_device_token() {
    let rig = Rig::new("link-unreadable");
    // 해석 불가 파일 안에 기기 토큰·수집키 모양 문자열이 들어 있어도 어디로도 나가지 않는다.
    let broken = format!(
        r#"{{"env":{{"SECRET_LIKE":"{}","OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer {}"}}, oops"#,
        device_token('D'),
        fake_key('Z')
    );
    std::fs::write(&rig.path, &broken).unwrap();
    let engine = LinkEngine::new();

    run(&engine, &rig, Trigger::Startup, T0);

    assert!(rig.api.calls().is_empty(), "적용할 수 없는 상태에서는 서버를 부르지 않는다");
    let status = status_after(&engine, &rig);
    assert_eq!(status.state, IngestLinkState::Failed);
    assert_eq!(status.failure.as_ref().unwrap().cause, LinkFailureCause::SettingsUnreadable);
    assert_status_clean("unreadable", &status, &["ZZZZ", "SECRET_LIKE"]);
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), broken, "파일 불변");
}

#[test]
fn foreign_authorization_header_is_not_overwritten_by_issuance() {
    let rig = Rig::new("link-foreign");
    let original = r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#;
    std::fs::write(&rig.path, original).unwrap();
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    assert!(rig.api.calls().is_empty());
    let status = status_after(&engine, &rig);
    assert_eq!(status.failure.as_ref().unwrap().cause, LinkFailureCause::HeaderUnrecognized);
    assert_status_clean("foreign", &status, &["FOREIGNSECRET"]);
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn apply_failure_status_is_a_fixed_message_without_key_or_device_token() {
    use std::os::unix::fs::PermissionsExt;
    let rig = Rig::new("link-apply-fail");
    let original = write_ready(&rig.path, None);
    std::fs::set_permissions(rig.path.parent().unwrap(), std::fs::Permissions::from_mode(0o555)).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();

    run(&engine, &rig, Trigger::Startup, T0);

    let status = status_after(&engine, &rig);
    assert_eq!(status.state, IngestLinkState::Failed);
    let failure = status.failure.as_ref().unwrap();
    assert_eq!(failure.cause, LinkFailureCause::ApplyFailed);
    assert_eq!(failure.message, LinkFailureCause::ApplyFailed.message());
    assert_status_clean("apply-fail", &status, &[&fake_key('K'), "KKKK", "CLAUDE_CODE_ENABLE_TELEMETRY"]);
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), original, "파일 불변");
}

#[test]
fn invalid_key_status_never_echoes_what_the_server_sent() {
    let dev = device_token('D');
    for bad in [dev.clone(), format!("Bearer {dev}"), format!("{},{dev}", fake_key('K')), "mhk_KKKK…".to_string()] {
        let rig = Rig::new("link-invalid");
        let original = write_ready(&rig.path, None);
        rig.api.push_get(state_policy(IssuePolicy::Auto, None));
        rig.api.push_create(Ok(crate::otel_hub_client::Created { token: SecretString::new(&bad), env: Default::default() }));
        let engine = LinkEngine::new();
        run(&engine, &rig, Trigger::Startup, T0);
        let status = status_after(&engine, &rig);
        assert_eq!(status.failure.as_ref().unwrap().cause, LinkFailureCause::InvalidKeyReturned);
        assert_status_clean("invalid", &status, &["KKKK"]);
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), original);
    }
}

// ======================= 단일 진입점: 수동 입력 경로와 주입 경로가 같은 파일을 만든다 =======================

#[test]
fn injected_and_manual_set_paths_write_identical_files() {
    let key = fake_key('K');
    let manual = temp_settings_path("same-manual");
    let injected = temp_settings_path("same-injected");
    std::fs::write(&manual, ORIGINAL).unwrap();
    std::fs::write(&injected, ORIGINAL).unwrap();

    let payload: OtelSavePayload = serde_json::from_str(&format!(
        r#"{{"values":{{}},"identity":null,"ingestKey":{{"action":"set","key":"  Authorization=Bearer {key}\n"}}}}"#
    ))
    .unwrap();
    save_to_settings_file(&manual, payload).unwrap();
    apply_ingest_key(&injected, SecretString::new(&key), &Default::default()).unwrap();

    assert_eq!(std::fs::read_to_string(&manual).unwrap(), std::fs::read_to_string(&injected).unwrap());
    assert_eq!(
        std::fs::read_to_string(backup_path_for(&manual)).unwrap(),
        std::fs::read_to_string(backup_path_for(&injected)).unwrap()
    );
    cleanup(&manual);
    cleanup(&injected);
}

#[test]
fn injected_device_token_is_rejected_by_the_apply_entry_itself() {
    let path = temp_settings_path("entry-rejects-mhd");
    std::fs::write(&path, ORIGINAL).unwrap();
    let dev = device_token('D');
    for input in [dev.clone(), format!("Authorization=Bearer {dev}")] {
        let err = apply_ingest_key(&path, SecretString::new(&input), &Default::default()).unwrap_err();
        assert_no_device_token("entry error", &err);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL);
    }
    cleanup(&path);
}

// ======================= 원자적 쓰기: 임시파일 → rename, 0600 유지 =======================

#[cfg(unix)]
#[test]
fn settings_write_replaces_file_via_temp_rename_not_in_place() {
    use std::os::unix::fs::MetadataExt;
    let path = temp_settings_path("atomic-rename");
    std::fs::write(&path, ORIGINAL).unwrap();
    let held = path.with_file_name("settings.held-link");
    std::fs::hard_link(&path, &held).unwrap();
    let inode_before = std::fs::metadata(&path).unwrap().ino();
    let key = fake_key('K');

    apply_ingest_key(&path, SecretString::new(&key), &Default::default()).unwrap();

    assert_eq!(std::fs::read_to_string(&held).unwrap(), ORIGINAL, "제자리 덮어쓰기면 기존 inode의 내용이 바뀐다");
    assert_ne!(std::fs::metadata(&path).unwrap().ino(), inode_before);
    assert!(std::fs::read_to_string(&path).unwrap().contains(&key));
    cleanup(&path);
}

#[cfg(unix)]
#[test]
fn failed_rename_leaves_no_temp_file_with_key() {
    let path = temp_settings_path("atomic-rename-fail");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("child"), "x").unwrap();
    let key = fake_key('K');

    let err = apply_ingest_key(&path, SecretString::new(&key), &Default::default()).unwrap_err();

    assert!(!err.contains("KKKK"), "에러에 키가 실리면 안 된다: {err}");
    assert!(path.is_dir(), "기존 대상은 그대로");
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".malgn-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "키가 든 임시 파일 잔존: {leftovers:?}");
    cleanup(&path);
}

#[cfg(unix)]
#[test]
fn injected_key_keeps_owner_only_permissions_on_file_and_backup() {
    use std::os::unix::fs::PermissionsExt;
    let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let rig = Rig::new("link-perms");
    write_ready(&rig.path, None);
    std::fs::set_permissions(&rig.path, std::fs::Permissions::from_mode(0o644)).unwrap();
    rig.api.push_get(state_policy(IssuePolicy::Auto, None));
    rig.api.push_create(created('K'));
    let engine = LinkEngine::new();
    run(&engine, &rig, Trigger::Startup, T0);
    let backup = backup_path_for(&rig.path);
    assert_eq!(mode_of(&rig.path), 0o600, "settings.json");
    assert_eq!(mode_of(&backup), 0o600, "백업");

    // 회전 후에도, 백업이 넓은 권한으로 남아 있던 경우에도 0600으로 좁혀진다.
    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
    rig.api.push_get(state_has_current(&prefix_of('Q'), false));
    rig.api.push_rotate(created('R'));
    run(&engine, &rig, Trigger::Periodic, T0 + 3_600_000);
    assert_eq!(mode_of(&rig.path), 0o600);
    assert_eq!(mode_of(&backup), 0o600);
}

// ======================= 상태 도출 (settings.json 실물 → 미연동/적용됨/실패) =======================

#[test]
fn link_status_is_derived_from_settings_file() {
    let path = temp_settings_path("derive");

    let s = status_of(&path);
    assert_eq!((s.state, s.masked.clone(), s.failure.clone()), (IngestLinkState::Unlinked, None, None));

    std::fs::write(&path, r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"x-team=a"}}"#).unwrap();
    assert_eq!(status_of(&path).state, IngestLinkState::Unlinked);

    let key = fake_key('K');
    std::fs::write(&path, format!(r#"{{"env":{{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer {key}"}}}}"#)).unwrap();
    let s = status_of(&path);
    assert_eq!(s.state, IngestLinkState::Applied);
    assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"));
    assert!(s.failure.is_none());
    assert_status_clean("applied", &s, &[&key, "KKKKK"]);

    std::fs::write(&path, r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#).unwrap();
    let s = status_of(&path);
    assert_eq!(s.state, IngestLinkState::Failed);
    assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::HeaderUnrecognized);
    assert!(s.masked.is_none());
    assert_status_clean("foreign", &s, &["FOREIGNSECRET"]);

    std::fs::write(&path, "{ not json").unwrap();
    let s = status_of(&path);
    assert_eq!(s.state, IngestLinkState::Failed);
    assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::SettingsUnreadable);
    cleanup(&path);
}

#[test]
fn link_status_json_shape_is_the_frontend_contract() {
    let applied = IngestLinkStatus::new(IngestLinkState::Applied, Some("mhk_KKKK…".into()));
    assert_eq!(
        serde_json::to_string(&applied).unwrap(),
        r#"{"state":"applied","masked":"mhk_KKKK…","failure":null,"issueSupported":false,"receiptCheckSupported":false,"telemetryEnabled":false,"endpoint":{"targetsHub":false,"insecure":false,"foreignHost":false},"collectionReady":false,"blockers":[],"hubConnected":false,"confirmRequired":false,"confirmDeclined":false,"revokedByHub":false,"waitingFirstData":false,"firstDataTimedOut":false,"blockedReason":null,"retryAt":null,"notice":null,"serverEnvChanged":[],"serverEnvRejected":[]}"#
    );
    let failed = IngestLinkStatus::failed(LinkFailureCause::Blocked, None);
    let json = serde_json::to_value(&failed).unwrap();
    assert_eq!(json["state"], "failed");
    assert_eq!(json["failure"]["cause"], "blocked");
    assert_eq!(json["failure"]["message"], LinkFailureCause::Blocked.message());
    let names: Vec<String> = [
        IngestLinkState::Unlinked,
        IngestLinkState::Issuing,
        IngestLinkState::Applied,
        IngestLinkState::Confirmed,
        IngestLinkState::Failed,
    ]
    .iter()
    .map(|s| serde_json::to_value(s).unwrap().as_str().unwrap().to_string())
    .collect();
    assert_eq!(names, ["unlinked", "issuing", "applied", "confirmed", "failed"]);
    let causes: Vec<String> = ALL_CAUSES
        .iter()
        .map(|c| serde_json::to_value(c).unwrap().as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        causes,
        [
            "settingsUnreadable",
            "headerUnrecognized",
            "endpointNotHub",
            "notReady",
            "featureUnavailable",
            "reconnectRequired",
            "notDesktopToken",
            "hubRejected",
            "unreachable",
            "rateLimited",
            "invalidKeyReturned",
            "applyFailed",
            "blocked",
            "credentialLimit",
            "rotateBudgetExceeded",
            "rotationLost"
        ]
    );
    let blockers: Vec<String> = [
        ReadinessBlocker::TelemetryOff,
        ReadinessBlocker::ExporterNotOtlp,
        ReadinessBlocker::ProtocolMismatch,
        ReadinessBlocker::EndpointMissing,
        ReadinessBlocker::EndpointNotHub,
        ReadinessBlocker::EndpointForeign,
        ReadinessBlocker::EndpointInsecure,
    ]
    .iter()
    .map(|b| serde_json::to_value(b).unwrap().as_str().unwrap().to_string())
    .collect();
    assert_eq!(
        blockers,
        ["telemetryOff", "exporterNotOtlp", "protocolMismatch", "endpointMissing", "endpointNotHub", "endpointForeign", "endpointInsecure"]
    );
    assert_eq!(serde_json::to_value(BlockedReason::SuspectedLeak).unwrap(), "suspectedLeak");
    assert_eq!(serde_json::to_value(BlockedReason::Quota).unwrap(), "quota");
}

// ======================= 오버레이: 엔진 단계 → 프론트 상태 =======================

#[test]
fn compose_maps_each_engine_phase_to_the_documented_state_and_cause() {
    let rig = Rig::new("compose");
    let key = fake_key('K');
    let unlinked = {
        write_ready(&rig.path, None);
        read_ingest_inputs(&rig.path)
    };
    let applied = {
        write_ready(&rig.path, Some(&key));
        read_ingest_inputs(&rig.path)
    };
    let compose = |inputs: &IngestInputs, view: RuntimeView| compose_status(inputs, &view, true, true);
    let with_phase = |phase: Phase| RuntimeView { phase, ..idle_view() };

    // 미연동 + 점검 중 → 발급중. 적용됨 + 점검 중 → 적용됨 유지(정기 점검이 깜빡이지 않게).
    assert_eq!(compose(&unlinked, with_phase(Phase::Checking)).state, IngestLinkState::Issuing);
    assert_eq!(compose(&applied, with_phase(Phase::Checking)).state, IngestLinkState::Applied);

    // 실패 계열: 상태 failed + 원인, 적용된 키의 마스킹은 유지.
    let table = [
        (Phase::CredentialLimit, LinkFailureCause::CredentialLimit),
        (Phase::RotationLost, LinkFailureCause::RotationLost),
        (Phase::Reconnect, LinkFailureCause::ReconnectRequired),
        (Phase::WriteFailed, LinkFailureCause::ApplyFailed),
        (Phase::NotDesktopToken, LinkFailureCause::NotDesktopToken),
        (Phase::HubRejected, LinkFailureCause::HubRejected),
        (Phase::NotReady, LinkFailureCause::NotReady),
        (Phase::EndpointNotHub, LinkFailureCause::EndpointNotHub),
        (Phase::InvalidKey, LinkFailureCause::InvalidKeyReturned),
    ];
    for (phase, cause) in table {
        let s = compose(&applied, with_phase(phase.clone()));
        assert_eq!((s.state, s.failure.as_ref().map(|f| f.cause)), (IngestLinkState::Failed, Some(cause)), "{phase:?}");
        assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"), "{phase:?}");
    }

    // 일시 원인(허브에 닿지 못함·요청 빈도 초과·서버에 기능 없음)은 키가 없을 때만 실패다 — 키가 적용돼 있으면 RV-07 테스트.
    for (phase, cause) in [
        (Phase::FeatureUnavailable, LinkFailureCause::FeatureUnavailable),
        (Phase::Transient, LinkFailureCause::Unreachable),
        (Phase::RateLimited, LinkFailureCause::RateLimited),
    ] {
        let s = compose(&unlinked, with_phase(phase.clone()));
        assert_eq!((s.state, s.failure.as_ref().map(|f| f.cause)), (IngestLinkState::Failed, Some(cause)), "{phase:?}");
        assert!(s.notice.is_none(), "{phase:?}");
    }

    // 차단·예산은 부가 필드를 싣는다.
    let blocked = compose(&unlinked, with_phase(Phase::Blocked(Some(BlockedReason::SuspectedLeak))));
    assert_eq!(blocked.failure.as_ref().unwrap().cause, LinkFailureCause::Blocked);
    assert_eq!(blocked.blocked_reason, Some(BlockedReason::SuspectedLeak));
    let budget = compose(&applied, with_phase(Phase::RotateBudget { retry_at: Some("2026-10-16T01:02:03.456Z".into()) }));
    assert_eq!(budget.failure.as_ref().unwrap().cause, LinkFailureCause::RotateBudgetExceeded);
    assert_eq!(budget.retry_at.as_deref(), Some("2026-10-16T01:02:03.456Z"));

    // 확인 요구·거절은 실패가 아니다.
    let confirm = compose(&unlinked, with_phase(Phase::ConfirmRequired));
    assert!(confirm.confirm_required && confirm.failure.is_none() && confirm.state == IngestLinkState::Unlinked);
    let declined = compose(&unlinked, with_phase(Phase::ConfirmDeclined));
    assert!(declined.confirm_declined && !declined.confirm_required && declined.failure.is_none());

    // 허브 미연결은 실패가 아니라 hubConnected=false다.
    let no_token = compose_status(&unlinked, &with_phase(Phase::NoDeviceToken), false, true);
    assert!(!no_token.hub_connected && no_token.failure.is_none());
}

#[test]
fn compose_confirms_only_the_key_that_actually_received_data_and_flags_waiting_and_timeout() {
    let rig = Rig::new("compose-confirm");
    write_ready(&rig.path, Some(&fake_key('K')));
    let inputs = read_ingest_inputs(&rig.path);

    let confirmed = compose_status(&inputs, &RuntimeView { confirmed_prefix: Some(prefix_of('K')), ..idle_view() }, true, true);
    assert_eq!(confirmed.state, IngestLinkState::Confirmed);
    assert!(!confirmed.waiting_first_data);
    // 다른 키의 확인은 현재 파일의 키를 확인한 것으로 세지 않는다(회전 직후 옛 키의 기록).
    let other = compose_status(&inputs, &RuntimeView { confirmed_prefix: Some(prefix_of('Q')), ..idle_view() }, true, true);
    assert_eq!(other.state, IngestLinkState::Applied);

    let waiting = compose_status(&inputs, &RuntimeView { waiting_first_data: true, ..idle_view() }, true, true);
    assert!(waiting.waiting_first_data && !waiting.first_data_timed_out);
    let timed_out = compose_status(&inputs, &RuntimeView { first_data_timed_out: true, ..idle_view() }, true, true);
    assert!(timed_out.first_data_timed_out && !timed_out.waiting_first_data);
}

#[test]
fn file_derived_failures_win_over_engine_phases() {
    let rig = Rig::new("compose-file-wins");
    std::fs::write(&rig.path, r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#).unwrap();
    let s = compose_status(
        &read_ingest_inputs(&rig.path),
        &RuntimeView { phase: Phase::Transient, ..idle_view() },
        true,
        true,
    );
    assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::HeaderUnrecognized);
}

// ======================= 수집 준비도 (RV-003 / RV-005): 파일 실물만 본다 =======================

fn readiness_file(env: serde_json::Value) -> (PathBuf, IngestLinkStatus) {
    let path = temp_settings_path("readiness");
    let mut e = serde_json::json!({
        "OTEL_EXPORTER_OTLP_HEADERS": format!("Authorization=Bearer {}", fake_key('K')),
    });
    for (k, v) in env.as_object().unwrap() {
        e[k] = v.clone();
    }
    std::fs::write(&path, serde_json::json!({ "env": e }).to_string()).unwrap();
    let s = status_of(&path);
    (path, s)
}

#[test]
fn readiness_is_judged_from_the_file_only_never_from_app_defaults() {
    let hub_m = hub_endpoint("/v1/metrics");
    let hub_l = hub_endpoint("/v1/logs");
    let hub_origin = crate::hub_auth::consts::hub_origin();
    let full = |extra: serde_json::Value| {
        let mut base = serde_json::json!({
            "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
            "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
            "OTEL_METRICS_EXPORTER": "otlp",
            "OTEL_LOGS_EXPORTER": "otlp",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": hub_m,
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": hub_l,
        });
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() {
                base.as_object_mut().unwrap().remove(k);
            } else {
                base[k] = v.clone();
            }
        }
        base
    };
    use ReadinessBlocker::*;
    // (설명, env, 기대 blockers)
    let cases: Vec<(&str, serde_json::Value, Vec<ReadinessBlocker>)> = vec![
        ("모두 갖춤", full(serde_json::json!({})), vec![]),
        ("범용 엔드포인트만 허브", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_ENDPOINT":hub_origin})), vec![]),
        ("metrics는 신호별, logs는 범용(허브)", full(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_ENDPOINT":hub_origin})), vec![]),
        ("exporter 쉼표 목록에 otlp 포함", full(serde_json::json!({"OTEL_METRICS_EXPORTER":"console, otlp"})), vec![]),
        // RV-005 ①: 텔레메트리만 켜고 엔드포인트가 없는 파일을 '허브/준비됨'으로 보면 안 된다(앱 기본값은 Claude Code가 모른다).
        ("텔레메트리 1 + 엔드포인트 없음", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null})), vec![EndpointMissing]),
        ("한쪽 엔드포인트만 없음", full(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null})), vec![EndpointMissing]),
        // RV-005 ②: exporter·protocol 확인.
        ("exporter 없음", full(serde_json::json!({"OTEL_METRICS_EXPORTER":null,"OTEL_LOGS_EXPORTER":null})), vec![ExporterNotOtlp]),
        ("exporter console", full(serde_json::json!({"OTEL_LOGS_EXPORTER":"console"})), vec![ExporterNotOtlp]),
        ("exporter none", full(serde_json::json!({"OTEL_METRICS_EXPORTER":"none"})), vec![ExporterNotOtlp]),
        ("프로토콜 없음", full(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":null})), vec![ProtocolMismatch]),
        ("프로토콜 grpc", full(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":"grpc"})), vec![ProtocolMismatch]),
        ("텔레메트리 항목 없음", full(serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY":null})), vec![TelemetryOff]),
        ("텔레메트리 0", full(serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY":"0"})), vec![TelemetryOff]),
        ("텔레메트리 true(1만 켜짐)", full(serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY":"true"})), vec![TelemetryOff]),
        // RV-005 ③: 범용 엔드포인트.
        ("범용만 있고 허브 밖", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":null,"OTEL_EXPORTER_OTLP_ENDPOINT":"https://collector.corp.example"})), vec![EndpointNotHub]),
        ("신호별은 허브인데 범용이 허브 밖", full(serde_json::json!({"OTEL_EXPORTER_OTLP_ENDPOINT":"https://collector.corp.example"})), vec![EndpointForeign]),
        ("사내 collector(https)", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":"https://collector.corp.example/v1/metrics","OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":"https://collector.corp.example/v1/logs"})), vec![EndpointNotHub]),
        ("한쪽만 허브 밖", full(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT":"https://collector.corp.example/v1/logs"})), vec![EndpointNotHub]),
        ("평문 http", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":"http://collector.corp.example/v1/metrics"})), vec![EndpointNotHub, EndpointInsecure]),
        ("해석 불가 URL", full(serde_json::json!({"OTEL_EXPORTER_OTLP_METRICS_ENDPOINT":"not a url"})), vec![EndpointNotHub]),
        ("아무 설정 없음", serde_json::json!({}), vec![TelemetryOff, ExporterNotOtlp, ProtocolMismatch, EndpointMissing]),
        // RV-04: traces 엔드포인트 — 수집 대상은 아니지만 헤더(수집키)가 그 주소로도 나간다.
        ("traces 엔드포인트가 허브 밖", full(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":"https://collector.corp.example/v1/traces"})), vec![EndpointForeign]),
        ("traces 엔드포인트가 평문 http", full(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":"http://collector.corp.example/v1/traces"})), vec![EndpointForeign, EndpointInsecure]),
        ("traces 엔드포인트가 허브", full(serde_json::json!({"OTEL_EXPORTER_OTLP_TRACES_ENDPOINT":hub_endpoint("/v1/traces")})), vec![]),
        // RV-04: 신호별 프로토콜.
        ("범용 프로토콜 없음 + 신호별 둘 다 http/protobuf", full(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":null,"OTEL_EXPORTER_OTLP_METRICS_PROTOCOL":"http/protobuf","OTEL_EXPORTER_OTLP_LOGS_PROTOCOL":"http/protobuf"})), vec![]),
        ("범용은 http/protobuf인데 logs가 grpc", full(serde_json::json!({"OTEL_EXPORTER_OTLP_LOGS_PROTOCOL":"grpc"})), vec![ProtocolMismatch]),
        ("범용 프로토콜 없음 + metrics만 지정", full(serde_json::json!({"OTEL_EXPORTER_OTLP_PROTOCOL":null,"OTEL_EXPORTER_OTLP_METRICS_PROTOCOL":"http/protobuf"})), vec![ProtocolMismatch]),
    ];
    for (label, env, expected) in cases {
        let (path, s) = readiness_file(env);
        assert_eq!(s.state, IngestLinkState::Applied, "{label}");
        assert_eq!(s.blockers, expected, "{label}");
        assert_eq!(s.collection_ready, expected.is_empty(), "{label}");
        // URL·키 원문은 상태(직렬화/Debug)에 없다 — 불리언과 식별자뿐.
        assert_status_clean(label, &s, &["collector.corp.example", "KKKKK", "not a url", "localhost"]);
        cleanup(&path);
    }
}

#[test]
fn readiness_is_all_false_when_settings_is_unreadable() {
    let path = temp_settings_path("readiness-unreadable");
    std::fs::write(&path, "{ not json").unwrap();
    let s = status_of(&path);
    assert_eq!(s.failure.as_ref().unwrap().cause, LinkFailureCause::SettingsUnreadable);
    assert!(!s.telemetry_enabled && !s.collection_ready);
    assert_eq!(s.endpoint, EndpointAssessment::default());
    cleanup(&path);
}

#[test]
fn a_missing_settings_file_is_unlinked_and_not_ready_without_inventing_defaults() {
    let path = temp_settings_path("readiness-missing");
    let s = status_of(&path);
    assert_eq!(s.state, IngestLinkState::Unlinked);
    assert!(!s.collection_ready && !s.endpoint.targets_hub);
    cleanup(&path);
}

#[test]
fn the_settings_screen_keeps_its_form_based_endpoint_assessment() {
    // 설정 화면(`ingestEndpoint`)은 폼 기준(파일 값 → 앱 기본값) 그대로다 — 연동 상태 판정만 파일 실물 기준이다.
    let path = temp_settings_path("form-assess");
    std::fs::write(&path, r#"{"env":{"CLAUDE_CODE_ENABLE_TELEMETRY":"1"}}"#).unwrap();
    let form = crate::otel_settings::otel_settings_get_for_test(&path);
    assert!(form.ingest_endpoint.targets_hub, "폼은 앱 기본값(허브)을 미리 채워 보여 준다");
    assert!(!status_of(&path).endpoint.targets_hub, "그러나 Claude Code는 기본값을 모른다");
    cleanup(&path);
}

// ======================= 발급 전 선검사 문구 =======================

#[test]
fn endpoint_stop_message_says_only_that_nothing_was_issued() {
    let m = LinkFailureCause::EndpointNotHub.message();
    assert!(m.contains("발급받지 않았습니다") && !m.contains("바꾸") && !m.contains("교체"), "{m}");
    let n = LinkFailureCause::NotReady.message();
    assert!(n.contains("발급받지 않았습니다"), "{n}");
}

#[test]
fn file_derived_header_message_states_the_current_fact_without_implying_automation() {
    let m = LinkFailureCause::HeaderUnrecognized.message();
    assert!(!m.contains("자동"), "{m}");
    assert!(!m.contains("않았습니다"), "하지 않은 동작을 말하지 않는다: {m}");
}

#[test]
fn rotate_budget_message_names_benign_causes_before_suspecting_misuse() {
    // 계약 D-14: 양성 원인(네트워크 불안정·설정 파일 동기화)을 먼저 알린다.
    let m = LinkFailureCause::RotateBudgetExceeded.message();
    let benign = m.find("네트워크가 불안정").unwrap();
    let suspicious = m.find("다른 곳에서 쓰이고 있을 수").unwrap();
    assert!(benign < suspicious && m.contains("~/.claude/settings.json") && m.contains("연결 해제 후 다시 연결"), "{m}");
}

#[test]
fn credential_limit_message_points_to_web_keys_and_old_app_keys() {
    let m = LinkFailureCause::CredentialLimit.message();
    assert!(m.contains("/keys") && m.contains("맑은에이전트"), "{m}");
}

// ======================= label (계약 E) =======================

fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

#[test]
fn label_uses_prefix_and_host_when_short() {
    assert_eq!(ingest_label("hagunho-mbp"), "맑은에이전트 hagunho-mbp");
    assert_eq!(ingest_label("  hagunho-mbp \n"), "맑은에이전트 hagunho-mbp", "trim");
}

#[test]
fn label_without_a_usable_host_is_just_the_prefix_word() {
    for raw in ["", "   ", "\u{0001}\u{007f}", " \t\u{0000} "] {
        assert_eq!(ingest_label(raw), "맑은에이전트", "{raw:?}");
    }
}

#[test]
fn label_strips_control_characters_u0000_to_u001f_and_u007f() {
    assert_eq!(ingest_label("a\u{0000}b\u{001F}c\u{007F}d\te"), "맑은에이전트 abcde");
    // 제어문자가 아닌 것은 남긴다(0x20·0x80 이상).
    assert_eq!(ingest_label("a b\u{0080}c"), "맑은에이전트 a b\u{0080}c");
}

#[test]
fn label_prefix_is_seven_utf16_units() {
    assert_eq!(units("맑은에이전트 "), 7);
}

#[test]
fn label_exactly_sixty_units_is_kept_and_sixty_one_is_cut_to_fifty_two_plus_ellipsis() {
    let host53 = "a".repeat(53); // 7 + 53 = 60
    let label = ingest_label(&host53);
    assert_eq!(units(&label), 60);
    assert_eq!(label, format!("맑은에이전트 {host53}"));

    let host54 = "a".repeat(54); // 7 + 54 = 61 → 잘림
    let label = ingest_label(&host54);
    assert_eq!(units(&label), 60);
    assert_eq!(label, format!("맑은에이전트 {}…", "a".repeat(52)));
}

#[test]
fn label_never_splits_a_surrogate_pair_when_cutting() {
    // 😀는 UTF-16 2 unit. 51 unit 뒤에 오면 52를 넘기므로 통째로 빠진다.
    let host = format!("{}😀{}", "a".repeat(51), "b".repeat(20));
    let label = ingest_label(&host);
    assert_eq!(label, format!("맑은에이전트 {}…", "a".repeat(51)));
    assert!(units(&label) <= 60);
    // 50 unit 뒤에 오면 정확히 52 unit이 되어 들어간다.
    let host = format!("{}😀{}", "a".repeat(50), "b".repeat(20));
    let label = ingest_label(&host);
    assert_eq!(label, format!("맑은에이전트 {}😀…", "a".repeat(50)));
    assert_eq!(units(&label), 60);
    // 서로게이트 쌍이 연속이어도 쌍 단위로만 자른다.
    let label = ingest_label(&"😀".repeat(40));
    assert!(label.chars().all(|c| c != '\u{FFFD}'));
    assert_eq!(units(&label), 7 + 52 + 1);
    assert!(label.ends_with('…'));
}

#[test]
fn label_counts_hangul_as_one_unit_each_and_never_exceeds_sixty() {
    let host = "가".repeat(80);
    let label = ingest_label(&host);
    assert_eq!(label, format!("맑은에이전트 {}…", "가".repeat(52)));
    assert_eq!(units(&label), 60);
    for n in 0..120 {
        assert!(units(&ingest_label(&"가".repeat(n))) <= 60, "{n}");
        assert!(units(&ingest_label(&"😀".repeat(n))) <= 60, "{n}");
    }
}

// ======================= D-13 연결 해제: Authorization 항목만 지운다 =======================

#[test]
fn d13_disconnect_removes_only_the_authorization_entry_and_keeps_other_headers_and_settings() {
    let path = temp_settings_path("d13-keep");
    let key = fake_key('K');
    let content = serde_json::json!({
        "permissions": {"allow": ["Bash(ls)"]},
        "env": {
            "OTEL_EXPORTER_OTLP_HEADERS": format!("x-team=a,Authorization=Bearer {key},x-env=prod"),
            "OTEL_RESOURCE_ATTRIBUTES": "employee.id=dev,team=platform",
            "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": hub_endpoint("/v1/metrics"),
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": hub_endpoint("/v1/logs"),
        }
    })
    .to_string();
    std::fs::write(&path, &content).unwrap();

    assert_eq!(remove_ingest_key(&path), Ok(true));

    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["env"]["OTEL_EXPORTER_OTLP_HEADERS"], "x-team=a,x-env=prod");
    assert_eq!(json["env"]["OTEL_RESOURCE_ATTRIBUTES"], "employee.id=dev,team=platform");
    assert_eq!(json["env"]["CLAUDE_CODE_ENABLE_TELEMETRY"], "1");
    assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
    // 지운 키가 직전 원본 백업에 남지 않는다.
    let backup = std::fs::read_to_string(backup_path_for(&path)).unwrap();
    assert!(!backup.contains(&key), "백업에 남은 키: {backup}");
    assert!(!std::fs::read_to_string(&path).unwrap().contains("mhk_"));
    cleanup(&path);
}

#[test]
fn d13_disconnect_drops_the_headers_entry_entirely_when_authorization_was_the_only_item() {
    let path = temp_settings_path("d13-only");
    write_ready_only_key(&path);
    assert_eq!(remove_ingest_key(&path), Ok(true));
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(json["env"].get("OTEL_EXPORTER_OTLP_HEADERS").is_none());
    assert_eq!(status_of(&path).state, IngestLinkState::Unlinked);
    cleanup(&path);
}

fn write_ready_only_key(path: &Path) {
    std::fs::write(
        path,
        serde_json::json!({"env": {"OTEL_EXPORTER_OTLP_HEADERS": format!("Authorization=Bearer {}", fake_key('K')), "SOME": "x"}}).to_string(),
    )
    .unwrap();
}

#[test]
fn d13_disconnect_does_not_touch_files_without_an_app_key() {
    let path = temp_settings_path("d13-noop");
    // 키 없음 / 다른 서비스의 Authorization / 해석 불가 — 파일을 한 바이트도 바꾸지 않고 백업도 만들지 않는다.
    for content in [
        ORIGINAL.to_string(),
        r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#.to_string(),
        "{ not json".to_string(),
    ] {
        std::fs::write(&path, &content).unwrap();
        assert_eq!(remove_ingest_key(&path), Ok(false), "{content}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        assert!(!backup_path_for(&path).exists());
    }
    // 파일이 없어도 오류 없이 지나간다.
    let _ = std::fs::remove_file(&path);
    assert_eq!(remove_ingest_key(&path), Ok(false));
    assert!(!path.exists());
    cleanup(&path);
}

#[test]
fn d13_after_disconnect_hook_clears_the_key_and_the_engine_memory() {
    let path = temp_settings_path("d13-hook");
    write_ready_only_key(&path);
    assert_eq!(after_hub_disconnect(&path), Ok(true));
    assert_eq!(status_of(&path).state, IngestLinkState::Unlinked);
    cleanup(&path);
}

// ======================= 키 접두어 질의 =======================

#[test]
fn file_key_prefix_is_the_masked_form_without_the_ellipsis() {
    let path = temp_settings_path("prefix");
    assert_eq!(file_key_prefix(&path), None);
    write_ready(&path, Some(&fake_key('S')));
    assert_eq!(file_key_prefix(&path), Some("mhk_SSSS".to_string()));
    std::fs::write(&path, r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGN"}}"#).unwrap();
    assert_eq!(file_key_prefix(&path), None, "형식이 다른 값은 앞 8자로도 쓰지 않는다");
    cleanup(&path);
}

// ======================= 라이브 GET 대조 장치 (수동 실행, 읽기 전용) =======================

mod live_probe {
    //! 이 PC 키체인의 기존 기기 토큰으로 **`GET /api/devices/self/ingest-credential`만** 호출해 응답의
    //! 키 구조와 타입을 출력한다. 값은 출력하지 않는다(허용 목록의 열거형 값과 길이만).
    //!
    //! 이 모듈에는 GET 호출만 있다 — POST·rotate·DELETE를 부르는 코드가 없고, `reqwest` 클라이언트의
    //! 메서드 호출이 `.get(`뿐이다. 실행: `cargo test -- --ignored live_get_structure_probe --nocapture`
    //!
    //! 주의: 테스트 바이너리는 서명된 앱이 아니라서 macOS가 키체인 접근 허용 대화상자를 띄울 수 있다(사람이
    //! 허용해야 진행된다). 같은 일을 서명된 앱의 CLI 모드로 하는 `scripts/live-get-ingest-credential.sh`가
    //! 기본 수단이다.

    use crate::hub_auth::consts::hub_origin;
    use crate::hub_auth::store::{load, KeyringStore};
    use crate::otel_hub_client::CREDENTIAL_PATH;

    /// 값을 그대로 보여도 되는 필드(비밀이 아닌 열거형).
    const SAFE_ENUM_FIELDS: [&str; 6] = ["issue_policy", "blocked_reason", "status", "revoke_reason", "access", "last_temporality"];

    pub(super) fn describe(path: &str, key: &str, v: &serde_json::Value, out: &mut Vec<String>) {
        use serde_json::Value;
        match v {
            Value::Null => out.push(format!("{path}: null")),
            Value::Bool(b) => out.push(format!("{path}: bool = {b}")),
            Value::Number(_) => out.push(format!("{path}: number")),
            Value::String(s) => {
                if SAFE_ENUM_FIELDS.contains(&key) {
                    out.push(format!("{path}: string = {s:?}"));
                } else {
                    out.push(format!("{path}: string(len={})", s.chars().count()));
                }
            }
            Value::Array(items) => {
                out.push(format!("{path}: array(len={})", items.len()));
                for (i, item) in items.iter().enumerate().take(3) {
                    describe(&format!("{path}[{i}]"), key, item, out);
                }
            }
            Value::Object(map) => {
                out.push(format!("{path}: object(keys={})", map.len()));
                for (k, child) in map {
                    describe(&format!("{path}.{k}"), k, child, out);
                }
            }
        }
    }

    pub(super) fn live_get() -> Result<(u16, Vec<String>), String> {
        let store = KeyringStore::new();
        let origin = hub_origin();
        let stored = load(&store, &origin)
            .map_err(|e| format!("키체인에서 기기 토큰을 읽지 못했습니다: {e}"))?
            .ok_or_else(|| "이 PC에는 허브 연결(기기 토큰)이 없습니다.".to_string())?;
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(35))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "HTTP 클라이언트를 만들지 못했습니다.".to_string())?;
        let resp = client
            .get(format!("{origin}{CREDENTIAL_PATH}"))
            .bearer_auth(stored.expose_token())
            .send()
            .map_err(|e| format!("요청 실패: {}", e.without_url()))?;
        let status = resp.status().as_u16();
        let body = resp.text().map_err(|_| "응답 본문을 읽지 못했습니다.".to_string())?;
        let mut lines = Vec::new();
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => describe("$", "", &v, &mut lines),
            Err(_) => lines.push(format!("$: JSON 아님(len={})", body.len())),
        }
        Ok((status, lines))
    }
}

#[test]
fn the_shape_the_live_hub_returns_for_a_device_without_a_key_parses_as_auto() {
    // 실제 허브(GET, 이 PC 기기 토큰)가 돌려준 구조: 키 4개(current·last_revoked·issue_policy·blocked_reason),
    // current=null, last_revoked=null, issue_policy="auto", blocked_reason=null.
    let s = crate::otel_hub_client::parse_state(r#"{"current":null,"last_revoked":null,"issue_policy":"auto","blocked_reason":null}"#).unwrap();
    assert_eq!((s.current, s.issue_policy, s.blocked_reason), (None, IssuePolicy::Auto, None));
}

#[test]
fn live_probe_describe_masks_values_and_keeps_only_structure() {
    let v = serde_json::json!({
        "current": {"id": "01k7abcSECRETISH", "token_prefix": "mhk_Ab3x", "status": "active", "last_signal_at_metrics": null, "content_seen": false, "scope": {"endpoints": ["POST /v1/metrics"]}},
        "issue_policy": "has_current", "blocked_reason": null
    });
    let mut lines = Vec::new();
    live_probe::describe("$", "", &v, &mut lines);
    let text = lines.join("\n");
    assert!(text.contains("$.current.token_prefix: string(len=8)"), "{text}");
    assert!(text.contains("$.current.id: string(len=16)"));
    assert!(text.contains(r#"$.issue_policy: string = "has_current""#));
    assert!(text.contains("$.current.content_seen: bool = false"));
    assert!(!text.contains("Ab3x") && !text.contains("SECRETISH") && !text.contains("POST /v1/metrics"), "{text}");
}

#[test]
#[ignore = "라이브 허브에 GET 1회를 보낸다. 수동 실행: cargo test -- --ignored live_get_structure_probe --nocapture"]
fn live_get_structure_probe() {
    match live_probe::live_get() {
        Ok((status, lines)) => {
            println!("[live GET {CRED}] HTTP {status}", CRED = crate::otel_hub_client::CREDENTIAL_PATH);
            for l in lines {
                println!("  {l}");
            }
        }
        Err(e) => println!("[live GET] 실행하지 못했습니다: {e}"),
    }
}

// ======================= R17 리뷰 RV-01: 웹에서 폐기된 키가 파일에 남은 경우 =======================

fn compose_with(inputs: &IngestInputs, view: RuntimeView) -> IngestLinkStatus {
    compose_status(inputs, &view, true, true)
}

#[test]
fn rv01_revoked_key_still_on_file_is_never_reported_as_confirmed_or_waiting() {
    let rig = Rig::new("rv01-compose");
    write_ready(&rig.path, Some(&fake_key('K')));
    let inputs = read_ingest_inputs(&rig.path);
    let view = |phase: Phase| RuntimeView {
        phase,
        // 같은 실행에서 이전에 확인한 수신·대기·타임아웃이 남아 있어도 폐기 판정을 가리지 않는다.
        confirmed_prefix: Some(prefix_of('K')),
        waiting_first_data: true,
        first_data_timed_out: true,
        revoked_prefix: None,
        env_changed: Vec::new(),
        env_rejected: Vec::new(),
    };
    for phase in [Phase::ConfirmRequired, Phase::ConfirmDeclined] {
        // applied + confirm 플래그 / (이전에 확인된 수신이 있던) confirmed + confirm 플래그 둘 다 같은 결과다.
        let s = compose_with(&inputs, view(phase.clone()));
        assert_eq!(s.state, IngestLinkState::Applied, "{phase:?}: 확인됨(confirmed)으로 보이면 안 된다");
        assert_eq!(s.masked.as_deref(), Some("mhk_KKKK…"), "{phase:?}: 파일의 키는 그대로 보인다");
        assert!(!s.waiting_first_data && !s.first_data_timed_out, "{phase:?}");
        assert!(s.failure.is_none() && s.notice.is_none(), "{phase:?}");
        assert_eq!((s.confirm_required, s.confirm_declined), (phase == Phase::ConfirmRequired, phase == Phase::ConfirmDeclined));
        assert_status_clean("rv01", &s, &[&fake_key('K'), "KKKKK"]);
    }
    // 폐기 판정이 없으면 같은 입력이 정상 확인(confirmed)이다 — 위 결과가 입력 때문이 아니라 판정 때문임을 고정한다.
    assert_eq!(compose_with(&inputs, view(Phase::Idle)).state, IngestLinkState::Confirmed);
}

// 엔진 → 상태 전체 경로: 확인된 키가 웹에서 폐기되면 정기 점검 뒤 화면 상태가 바뀌고, 거절해도 재시작 경로가 남는다.
#[test]
fn rv01_confirmed_key_revoked_on_the_web_then_declined_still_offers_a_way_to_reissue() {
    let rig = Rig::new("rv01-flow");
    write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    rig.api.push_get(state_has_current(&prefix_of('S'), true));
    run(&engine, &rig, Trigger::Startup, T0);
    assert_eq!(status_after(&engine, &rig).state, IngestLinkState::Confirmed);

    // 웹에서 폐기 → 1시간 뒤 점검이 confirm_required.
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, &rig, Trigger::Periodic, T0 + 3_600_000);
    let s = status_after(&engine, &rig);
    assert_eq!(s.state, IngestLinkState::Applied, "더 이상 '수신 확인됨'이 아니다");
    assert!(s.confirm_required && !s.confirm_declined);
    assert!(rig.api.count("create") == 0 && std::fs::read_to_string(&rig.path).unwrap().contains(&fake_key('S')), "묻기 전에 파일을 바꾸지 않는다");

    // 거절 → 이번 실행 동안 다시 묻지 않지만 상태는 '폐기됨'을 계속 말한다(applied/confirmed로 돌아가지 않는다).
    engine.decline_confirm();
    let s = status_after(&engine, &rig);
    assert!(s.confirm_declined && !s.confirm_required && s.state == IngestLinkState::Applied);
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, &rig, Trigger::Periodic, T0 + 2 * 3_600_000);
    assert!(status_after(&engine, &rig).confirm_declined, "다음 점검도 여전히 폐기 상태다");

    // 사용자의 [다시 시도]가 거절 기록을 비우고 다시 묻는다 — 화면이 이 경로를 버튼으로 연다.
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, &rig, Trigger::User, T0 + 2 * 3_600_000 + 1);
    assert!(status_after(&engine, &rig).confirm_required);
}

// ======================= R17 리뷰 RV-07: 일시 원인이 적용·수신 확인 상태를 지우지 않는다 =======================

#[test]
fn rv07_transient_causes_keep_applied_and_confirmed_and_only_attach_a_notice() {
    let rig = Rig::new("rv07");
    write_ready(&rig.path, Some(&fake_key('K')));
    let inputs = read_ingest_inputs(&rig.path);
    for (phase, cause) in [
        (Phase::Transient, LinkFailureCause::Unreachable),
        (Phase::RateLimited, LinkFailureCause::RateLimited),
        (Phase::FeatureUnavailable, LinkFailureCause::FeatureUnavailable),
    ] {
        // 적용됨(수신 미확인)
        let applied = compose_with(&inputs, RuntimeView { phase: phase.clone(), ..idle_view() });
        assert_eq!(applied.state, IngestLinkState::Applied, "{phase:?}");
        assert!(applied.failure.is_none(), "{phase:?}: 실패로 덮지 않는다");
        assert_eq!(applied.notice.as_ref().map(|n| n.cause), Some(cause));
        assert_eq!(applied.notice.as_ref().map(|n| n.message.as_str()), Some(cause.message()));
        assert_eq!(applied.masked.as_deref(), Some("mhk_KKKK…"));
        // 수신 확인됨
        let confirmed = compose_with(&inputs, RuntimeView { phase: phase.clone(), confirmed_prefix: Some(prefix_of('K')), ..idle_view() });
        assert_eq!(confirmed.state, IngestLinkState::Confirmed, "{phase:?}: 확인된 사실을 지우지 않는다");
        assert!(confirmed.failure.is_none() && confirmed.notice.is_some());
        assert_status_clean("rv07", &confirmed, &[&fake_key('K')]);
    }
    // 일시 원인이 아니면(연결 끊김 등) 키가 적용돼 있어도 실패다.
    let reconnect = compose_with(&inputs, RuntimeView { phase: Phase::Reconnect, confirmed_prefix: Some(prefix_of('K')), ..idle_view() });
    assert_eq!(reconnect.state, IngestLinkState::Failed);
    assert!(reconnect.notice.is_none());
    // 보조 안내가 없는 정상 상태는 notice가 null이다.
    assert!(compose_with(&inputs, idle_view()).notice.is_none());
}

#[test]
fn rv07_transient_cause_classification_is_exactly_the_self_healing_ones() {
    let transient: Vec<LinkFailureCause> = ALL_CAUSES.iter().copied().filter(|c| c.is_transient()).collect();
    assert_eq!(transient, [LinkFailureCause::FeatureUnavailable, LinkFailureCause::Unreachable, LinkFailureCause::RateLimited]);
}

// ======================= R17 리뷰 RV-05/06: 연결 해제 =======================

#[test]
fn rv06_disconnect_reports_what_actually_happened_to_the_key_on_this_pc() {
    use crate::hub_auth::IngestKeyRemoval;
    let path = temp_settings_path("rv06-removal");
    // 앱이 적용한 키가 있었다 → Removed
    write_ready_only_key(&path);
    assert_eq!(IngestKeyRemoval::from_result(after_hub_disconnect(&path)), IngestKeyRemoval::Removed);
    // 이미 없다 / 다른 서비스의 Authorization / 해석 불가 → 지운 것이 없다(NotPresent), 파일 불변
    for content in [
        ORIGINAL.to_string(),
        r#"{"env":{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer FOREIGNSECRETTOKEN"}}"#.to_string(),
        "{ not json".to_string(),
    ] {
        std::fs::write(&path, &content).unwrap();
        assert_eq!(IngestKeyRemoval::from_result(after_hub_disconnect(&path)), IngestKeyRemoval::NotPresent, "{content}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
    // 지우려 했지만 쓰지 못했다 → Failed (읽기 전용 디렉터리에서는 임시 파일을 만들 수 없다)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        write_ready_only_key(&path);
        let dir = path.parent().unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let writable = std::fs::File::create(dir.join("probe")).is_ok(); // root 등 권한이 무시되는 환경 방어
        let result = IngestKeyRemoval::from_result(after_hub_disconnect(&path));
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !writable {
            assert_eq!(result, IngestKeyRemoval::Failed);
            assert!(std::fs::read_to_string(&path).unwrap().contains("mhk_"), "쓰기 실패 시 파일은 그대로다");
        }
    }
    cleanup(&path);
}

#[test]
fn rv06_disconnect_response_flattens_the_hub_status_and_adds_the_key_outcome() {
    use crate::hub_auth::state::{HubAuthStatus, HubState};
    use crate::hub_auth::{HubDisconnectResult, IngestKeyRemoval};
    let result = HubDisconnectResult {
        status: HubAuthStatus {
            connected: false,
            state: HubState::NotConnected,
            device_name: None,
            expires_at: None,
            renew_after: None,
            reason: Some("SERVER_REVOKE_FAILED".into()),
        },
        ingest_key: IngestKeyRemoval::Removed,
    };
    let json = serde_json::to_value(&result).unwrap();
    // 기존 HubAuthStatus 필드는 그대로 최상위에 있다(프론트의 status.reason 등이 계속 동작한다).
    assert_eq!(json["connected"], false);
    assert_eq!(json["reason"], "SERVER_REVOKE_FAILED");
    assert_eq!(json["ingest_key"], "removed");
    assert_eq!(serde_json::to_value(IngestKeyRemoval::NotPresent).unwrap(), "notPresent");
    assert_eq!(serde_json::to_value(IngestKeyRemoval::Failed).unwrap(), "failed");
}

// ======================= R17 리뷰 RV-09: 폐기 판정은 일시 오류 점검으로 사라지지 않는다 =======================

const HOUR: u64 = 3_600_000;

/// 웹에서 폐기된 키 `S`가 파일에 남고, 허브가 `confirm_required`로 판정한 상태의 엔진.
fn engine_with_revoked_key_on_file(rig: &Rig) -> LinkEngine {
    write_ready(&rig.path, Some(&fake_key('S')));
    let engine = LinkEngine::new();
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, rig, Trigger::Startup, T0);
    let s = status_after(&engine, rig);
    assert!(s.confirm_required && s.revoked_by_hub, "전제: 폐기 판정");
    engine
}

/// 폐기 표시가 유지되는지: 파일 키는 그대로 보이되 확인됨/대기가 아니고, 일시 원인은 notice로 덧붙는다.
fn assert_still_revoked(label: &str, s: &IngestLinkStatus, cause: LinkFailureCause) {
    assert!(s.revoked_by_hub, "{label}: 폐기 표시가 사라졌다");
    assert_eq!(s.state, IngestLinkState::Applied, "{label}");
    assert_eq!(s.masked.as_deref(), Some("mhk_SSSS…"), "{label}");
    assert!(!s.waiting_first_data && !s.first_data_timed_out, "{label}: 폐기된 키를 수신 대기로 보여선 안 된다");
    assert!(s.failure.is_none(), "{label}: 일시 오류는 실패로 덮지 않는다");
    assert_eq!(s.notice.as_ref().map(|n| n.cause), Some(cause), "{label}: 일시 원인은 덧붙여 알린다");
}

#[test]
fn rv09_accepted_reissue_that_fails_transiently_leaves_the_revoked_key_marked_revoked() {
    for (label, reply, cause) in [
        ("5xx", Err(http(503, "UNAVAILABLE")), LinkFailureCause::Unreachable),
        ("네트워크", Err(ApiError::NotSent), LinkFailureCause::Unreachable),
        ("429", Err(http(429, "RATE_LIMITED")), LinkFailureCause::RateLimited),
    ] {
        let rig = Rig::new("rv09-post");
        let engine = engine_with_revoked_key_on_file(&rig);
        rig.api.push_create(reply);

        engine.accept_confirm(&rig.env(), T0 + 5_000);

        assert_eq!(rig.api.kinds(), ["get", "create:true"], "{label}");
        let s = status_after(&engine, &rig);
        assert_still_revoked(label, &s, cause);
        assert!(std::fs::read_to_string(&rig.path).unwrap().contains(&fake_key('S')), "{label}: 파일은 바뀌지 않았다");
        assert_status_clean("rv09", &s, &[&fake_key('S')]);
    }
}

#[test]
fn rv09_periodic_and_retry_checks_that_fail_transiently_keep_the_revoked_mark() {
    let rig = Rig::new("rv09-get");
    let engine = engine_with_revoked_key_on_file(&rig);

    // 1시간 정기 점검의 GET이 끊긴다.
    rig.api.push_get(Err(ApiError::NotSent));
    run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
    assert_still_revoked("정기 점검 GET 실패", &status_after(&engine, &rig), LinkFailureCause::Unreachable);

    // 거절 → [다시 시도]의 GET이 끊겨도 폐기 표시는 이어진다(확인창 대기가 아니라 '재확인 필요').
    engine.decline_confirm();
    rig.api.push_get(Err(http(500, "INTERNAL")));
    run(&engine, &rig, Trigger::User, T0 + 2 * HOUR);
    let s = status_after(&engine, &rig);
    assert_still_revoked("다시 시도 GET 실패", &s, LinkFailureCause::Unreachable);
    assert!(!s.confirm_required, "확인창을 다시 띄우지 않는다(accept할 대기 상태가 아니다)");

    // 기능 없음(401 + 연결 정상)도 일시 원인이다.
    let rig2 = Rig::new("rv09-feature").with_probe(&[ProbeResult::Alive]);
    let engine2 = engine_with_revoked_key_on_file(&rig2);
    rig2.api.push_get(Err(http(401, "UNAUTHORIZED")));
    run(&engine2, &rig2, Trigger::Periodic, T0 + HOUR);
    assert_still_revoked("기능 없음", &status_after(&engine2, &rig2), LinkFailureCause::FeatureUnavailable);
}

#[test]
fn rv09_revoked_mark_ends_when_the_hub_answers_differently_or_the_key_or_connection_changes() {
    // (a) 성공 점검이 다른 판정(현행 키가 파일 키와 같다)을 준다.
    let rig = Rig::new("rv09-end-a");
    let engine = engine_with_revoked_key_on_file(&rig);
    rig.api.push_get(Err(ApiError::NotSent));
    run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
    assert!(status_after(&engine, &rig).revoked_by_hub);
    rig.api.push_get(state_has_current(&prefix_of('S'), true));
    run(&engine, &rig, Trigger::Resume, T0 + 2 * HOUR);
    let s = status_after(&engine, &rig);
    assert!(!s.revoked_by_hub && s.state == IngestLinkState::Confirmed, "{s:?}");

    // (b) 재발급이 성공해 새 키가 적용된다.
    let rig = Rig::new("rv09-end-b");
    let engine = engine_with_revoked_key_on_file(&rig);
    rig.api.push_create(Err(ApiError::NotSent));
    engine.accept_confirm(&rig.env(), T0 + 5_000);
    assert!(status_after(&engine, &rig).revoked_by_hub);
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, &rig, Trigger::User, T0 + 10_000);
    rig.api.push_create(created('N'));
    engine.accept_confirm(&rig.env(), T0 + 15_000);
    let s = status_after(&engine, &rig);
    assert!(!s.revoked_by_hub && !s.confirm_required, "{s:?}");
    assert_eq!(s.masked.as_deref(), Some("mhk_NNNN…"));

    // (c) 파일의 키가 바뀌었다(앞 8자 불일치) — 이전 키의 판정을 새 키에 씌우지 않는다.
    let rig = Rig::new("rv09-end-c");
    let engine = engine_with_revoked_key_on_file(&rig);
    rig.api.push_get(Err(ApiError::NotSent));
    run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
    apply_ingest_key(&rig.path, crate::otel_ingest_key::SecretString::new(&fake_key('M')), &Default::default()).unwrap();
    let s = status_after(&engine, &rig);
    assert!(!s.revoked_by_hub, "{s:?}");
    assert_eq!(s.masked.as_deref(), Some("mhk_MMMM…"));

    // (d) 연결이 바뀐다(재연결·해제).
    for disconnect in [false, true] {
        let rig = Rig::new("rv09-end-d");
        let engine = engine_with_revoked_key_on_file(&rig);
        rig.api.push_get(Err(ApiError::NotSent));
        run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
        assert!(status_after(&engine, &rig).revoked_by_hub);
        if disconnect {
            engine.on_disconnected(T0 + 2 * HOUR);
        } else {
            engine.on_connected();
        }
        assert!(!status_after(&engine, &rig).revoked_by_hub, "disconnect={disconnect}");
    }
}

#[test]
fn rv09_no_key_on_file_means_nothing_to_keep_marked() {
    // 폐기 판정 당시 파일에 키가 없었다 — 이후 일시 오류가 나도 '폐기된 키가 남음' 표시를 만들지 않는다.
    let rig = Rig::new("rv09-nokey");
    write_ready(&rig.path, None);
    let engine = LinkEngine::new();
    rig.api.push_get(state_policy(IssuePolicy::ConfirmRequired, None));
    run(&engine, &rig, Trigger::Startup, T0);
    rig.api.push_get(Err(ApiError::NotSent));
    run(&engine, &rig, Trigger::Periodic, T0 + HOUR);
    assert!(!status_after(&engine, &rig).revoked_by_hub);
}
