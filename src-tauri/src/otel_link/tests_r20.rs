// R20 — 재발급 결과(RT-4)의 프론트 계약과, 폐기·연결 해제 안내 문구의 사실성.
// (분기별 엔진 상태는 `otel_link_engine/tests_r20.rs`가 목 서버로 고정한다.)

use super::*;
use crate::otel_link_engine::testkit::*;
use crate::otel_link_engine::{NotStarted, Phase, ReissueResult};

#[test]
fn r20_reissue_outcome_maps_each_engine_result_to_exactly_one_frontend_outcome() {
    use IngestReissueOutcome as O;
    let failed = |cause: LinkFailureCause, transient: bool| O::Failed { failure: IngestLinkFailure::of(cause), transient };
    let cases: Vec<(ReissueResult, IngestReissueOutcome)> = vec![
        (ReissueResult::Minted { prefix: prefix_of('N') }, O::Minted { masked: "mhk_NNNN…".to_string() }),
        (ReissueResult::NotStarted(NotStarted::Busy), O::NotStarted { reason: ReissueNotStarted::Busy }),
        (ReissueResult::NotStarted(NotStarted::NotPending), O::NotStarted { reason: ReissueNotStarted::NotPending }),
        (ReissueResult::Ended(Phase::NoDeviceToken), O::NoConnection),
        (ReissueResult::Ended(Phase::Idle), O::NotApplied),
        (ReissueResult::Ended(Phase::Transient), failed(LinkFailureCause::Unreachable, true)),
        (ReissueResult::Ended(Phase::RateLimited), failed(LinkFailureCause::RateLimited, true)),
        (ReissueResult::Ended(Phase::FeatureUnavailable), failed(LinkFailureCause::FeatureUnavailable, true)),
        (ReissueResult::Ended(Phase::Reconnect), failed(LinkFailureCause::ReconnectRequired, false)),
        (ReissueResult::Ended(Phase::Blocked(Some(BlockedReason::SuspectedLeak))), failed(LinkFailureCause::Blocked, false)),
        (ReissueResult::Ended(Phase::CredentialLimit), failed(LinkFailureCause::CredentialLimit, false)),
        (ReissueResult::Ended(Phase::HubRejected), failed(LinkFailureCause::HubRejected, false)),
        (ReissueResult::Ended(Phase::InvalidKey), failed(LinkFailureCause::InvalidKeyReturned, false)),
        (ReissueResult::Ended(Phase::WriteFailed), failed(LinkFailureCause::ApplyFailed, false)),
        (ReissueResult::Ended(Phase::RotateBudget { retry_at: None }), failed(LinkFailureCause::RotateBudgetExceeded, false)),
    ];
    for (result, want) in cases {
        let label = format!("{result:?}");
        assert_eq!(reissue_outcome(result), want, "{label}");
    }
}

#[test]
fn r20_reissue_answer_json_shape_is_the_frontend_contract_and_carries_no_key_material() {
    let answer = |outcome| IngestReissueAnswer {
        status: IngestLinkStatus::new(IngestLinkState::Applied, Some("mhk_NNNN…".to_string())),
        outcome,
    };
    let v = serde_json::to_value(answer(reissue_outcome(ReissueResult::Minted { prefix: prefix_of('N') }))).unwrap();
    assert_eq!(v["outcome"], serde_json::json!({ "kind": "minted", "masked": "mhk_NNNN…" }));
    assert_eq!(v["status"]["state"], "applied");

    let shapes = [
        (IngestReissueOutcome::Declined, serde_json::json!({ "kind": "declined" })),
        (
            IngestReissueOutcome::NotStarted { reason: ReissueNotStarted::NotPending },
            serde_json::json!({ "kind": "notStarted", "reason": "notPending" }),
        ),
        (IngestReissueOutcome::NoConnection, serde_json::json!({ "kind": "noConnection" })),
        (IngestReissueOutcome::NotApplied, serde_json::json!({ "kind": "notApplied" })),
        (
            reissue_outcome(ReissueResult::Ended(Phase::Reconnect)),
            serde_json::json!({
                "kind": "failed",
                "failure": { "cause": "reconnectRequired", "message": LinkFailureCause::ReconnectRequired.message() },
                "transient": false,
            }),
        ),
    ];
    for (outcome, want) in shapes {
        assert_eq!(serde_json::to_value(&outcome).unwrap(), want);
    }
    // 어떤 결과도 기기 토큰·수집키 원문을 싣지 않는다(마스킹 `mhk_`+4자…만).
    for outcome in [
        reissue_outcome(ReissueResult::Minted { prefix: prefix_of('N') }),
        reissue_outcome(ReissueResult::Ended(Phase::Reconnect)),
        reissue_outcome(ReissueResult::Ended(Phase::InvalidKey)),
    ] {
        let text = serde_json::to_string(&answer(outcome)).unwrap();
        assert!(!text.contains("mhd_") && !text.contains(&fake_key('N')), "{text}");
    }
}

#[test]
fn r20_connection_lost_message_states_the_facts_and_offers_relogin_not_reissue() {
    let m = LinkFailureCause::ReconnectRequired.message();
    assert!(m.contains("이 PC의 맑은에이전트 연결이 해제되었습니다"), "{m}");
    assert!(m.contains("유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기"), "{m}");
    assert!(m.contains("앱에서 다시 로그인해 연결하세요"), "{m}");
    assert!(!m.contains("다시 발급"), "연결이 끊긴 상태의 답은 재발급이 아니다: {m}");
    // 연결을 다시 만들어야 하는 다른 원인도 같은 말(다시 로그인해 연결)을 쓰고 재발급을 권하지 않는다.
    for cause in [LinkFailureCause::Blocked, LinkFailureCause::NotDesktopToken] {
        let m = cause.message();
        assert!(m.contains("다시 로그인해 연결") || m.contains("다시 연결"), "{cause:?}: {m}");
        assert!(!m.contains("다시 발급"), "{cause:?}: {m}");
    }
}

#[test]
fn r20_feature_unavailable_message_says_the_connection_is_fine_and_asks_for_nothing() {
    let m = LinkFailureCause::FeatureUnavailable.message();
    assert!(m.contains("허브 연결은 정상"), "{m}");
    assert!(!m.contains("로그인") && !m.contains("다시 발급") && !m.contains("다시 연결"), "{m}");
    assert!(LinkFailureCause::FeatureUnavailable.is_transient(), "기능 없음은 앱이 스스로 다시 확인하는 일시 원인이다");
    assert!(!LinkFailureCause::ReconnectRequired.is_transient(), "재연결은 사용자 조치가 필요하다");
}
