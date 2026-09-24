// ---------------- 단가표 ----------------
// 출처: https://platform.claude.com/docs/en/about-claude/pricing.md
// (WebFetch로 확인, 확인일 2026-09-24). USD/1M 토큰, Anthropic 1st-party
// Claude API(직접 호출) 기준 — Bedrock/Google Cloud/Foundry 등 클라우드
// 파트너 요금제나 배치/Fast mode 할인은 반영하지 않는다(이 로그가 그쪽
// 경로로 호출됐는지 구분할 필드가 없다 — 표준가로 근사한다).
//
// 모델 ID 단위로 정확히 매칭한다("opus"/"sonnet"/"haiku" 3계열로 뭉뚱그리지
// 않는다) — 이전 버전은 계열별 단가(opus=$15/$75, sonnet=$3/$15, 미매칭은
// sonnet 폴백)로 계산해 이 머신의 실제 모델 분포(claude-opus-5, claude-sonnet-5
// 등 5세대) 기준 opus 약 3배·sonnet 약 1.5배 과대청구했다(2026-09-24 PM 실측).
//
// 캐시쓰기는 TTL별로 단가가 다르다: 5분 캐시 = 입력가×1.25, 1시간 캐시 =
// 입력가×2. `usage.cache_creation.ephemeral_5m_input_tokens`/
// `ephemeral_1h_input_tokens` 분해 필드가 없는 구 로그는 전부 5분 단가로
// 계산한다(`split_cache_write_tokens` 참조) — 실제보다 낮게 잡을지언정
// 과대청구 방향은 아니다.
//
// 캐시읽기 배율은 기본 0.1x(=아래 cache_read 열)이지만 예외가 있다:
// Opus 5.5는 0.05x($0.20/MTok), Fable 5.1/Mythos 5.1은 0.025x($0.25/MTok).
// 아래 표의 cache_read 값은 이미 그 배율까지 곱해진 절대 단가다.
//
// 표에 없는 모델(단가표에 없는 구모델·별칭 "opus"/"sonnet"/"haiku" 등)은
// 비용 계산에 섞지 않는다 — `price_for_model`이 `None`을 돌려주고 호출부가
// 비용 0 + `pricing_matched=false`로 처리해 "미산정" 토큰으로 정직하게
// 드러낸다(sonnet 단가로 대체 계산하던 이전 폴백은 제거했다 — 그 폴백 자체가
// 이번 과대청구 버그의 원인이었다).
//
// 실측(이 머신 최근 30일, message.model 분포): claude-sonnet-5, claude-opus-5,
// claude-opus-5-5, claude-haiku-4-5-20251001(날짜 접미사 있음) 네 가지가
// 전부이고 "<synthetic>"(usage 전부 0)이 소수 섞여 있다. PM이 언급한 "opus"/
// "sonnet" 짧은 별칭은 실측 결과 message.model 필드의 실제 값이 아니라
// 도구 호출 프롬프트 문자열(`"model":"opus"` 텍스트가 tool_use input에
// 인용됨) grep 오탐이었다 — `python3 json.loads`로 top-level message.model만
// 정밀 검사해 0건 확인했다. 그래서 이 표는 "opus"/"sonnet"/"haiku" 같은
// 짧은 별칭 키를 별도로 추가하지 않는다: 어느 특정 버전을 가리키는지 알 수
// 없는 채로 단가를 추정하면 이번에 고친 것과 같은 종류의 오류(모르는 걸
// 안다고 가정)를 반복하게 된다 — 대신 "unknown"으로 남겨 미산정 처리한다.

pub(crate) struct ModelPrice {
    /// 사람이 읽을 표시명 — `models` 요약에서 계열(opus/sonnet/haiku)이
    /// 아니라 이 이름 단위로 비중을 보여준다(예: "Opus 5.5", "Sonnet 5").
    pub(crate) display_name: &'static str,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_write_5m: f64,
    pub(crate) cache_write_1h: f64,
    pub(crate) cache_read: f64,
}

/// (정규 모델 ID, 단가). 정규 모델 ID는 날짜 접미사(`-YYYYMMDD`)를 뗀 소문자
/// 문자열이다 — `canonical_model_id` 참조. PM 제공값과 공식 페이지 값이
/// 일치하는 항목(claude-opus-5, claude-sonnet-5, claude-haiku-4-5,
/// claude-fable-5-1, claude-fable-5)은 그대로 옮겼고, 나머지(claude-opus-4-8/
/// 4-7/4-6/4-5/4-1/4, claude-sonnet-4-6/4-5/4, claude-haiku-3-5,
/// claude-mythos-5-1/5)는 페이지에서 직접 확인한 값이다 — PM이 준 구모델
/// 예시(opus-4-8/4-7/4-6, sonnet-4-6)와 페이지 값이 정확히 일치함을 확인했다.
const PRICE_TABLE: &[(&str, ModelPrice)] = &[
    (
        "claude-opus-5-5",
        ModelPrice {
            display_name: "Opus 5.5",
            input: 4.0,
            output: 20.0,
            cache_write_5m: 5.0,
            cache_write_1h: 8.0,
            cache_read: 0.20, // 0.05x 예외
        },
    ),
    (
        "claude-opus-5",
        ModelPrice {
            display_name: "Opus 5",
            input: 5.0,
            output: 25.0,
            cache_write_5m: 6.25,
            cache_write_1h: 10.0,
            cache_read: 0.50,
        },
    ),
    (
        "claude-opus-4-8",
        ModelPrice {
            display_name: "Opus 4.8",
            input: 5.0,
            output: 25.0,
            cache_write_5m: 6.25,
            cache_write_1h: 10.0,
            cache_read: 0.50,
        },
    ),
    (
        "claude-opus-4-7",
        ModelPrice {
            display_name: "Opus 4.7",
            input: 5.0,
            output: 25.0,
            cache_write_5m: 6.25,
            cache_write_1h: 10.0,
            cache_read: 0.50,
        },
    ),
    (
        "claude-opus-4-6",
        ModelPrice {
            display_name: "Opus 4.6",
            input: 5.0,
            output: 25.0,
            cache_write_5m: 6.25,
            cache_write_1h: 10.0,
            cache_read: 0.50,
        },
    ),
    (
        "claude-opus-4-5",
        ModelPrice {
            display_name: "Opus 4.5",
            input: 5.0,
            output: 25.0,
            cache_write_5m: 6.25,
            cache_write_1h: 10.0,
            cache_read: 0.50,
        },
    ),
    (
        "claude-opus-4-1",
        ModelPrice {
            display_name: "Opus 4.1",
            input: 15.0,
            output: 75.0,
            cache_write_5m: 18.75,
            cache_write_1h: 30.0,
            cache_read: 1.50,
        },
    ),
    (
        "claude-opus-4",
        ModelPrice {
            display_name: "Opus 4",
            input: 15.0,
            output: 75.0,
            cache_write_5m: 18.75,
            cache_write_1h: 30.0,
            cache_read: 1.50,
        },
    ),
    (
        "claude-sonnet-5",
        ModelPrice {
            display_name: "Sonnet 5",
            input: 2.0,
            output: 10.0,
            cache_write_5m: 2.50,
            cache_write_1h: 4.0,
            cache_read: 0.20,
        },
    ),
    (
        "claude-sonnet-4-6",
        ModelPrice {
            display_name: "Sonnet 4.6",
            input: 3.0,
            output: 15.0,
            cache_write_5m: 3.75,
            cache_write_1h: 6.0,
            cache_read: 0.30,
        },
    ),
    (
        "claude-sonnet-4-5",
        ModelPrice {
            display_name: "Sonnet 4.5",
            input: 3.0,
            output: 15.0,
            cache_write_5m: 3.75,
            cache_write_1h: 6.0,
            cache_read: 0.30,
        },
    ),
    (
        "claude-sonnet-4",
        ModelPrice {
            display_name: "Sonnet 4",
            input: 3.0,
            output: 15.0,
            cache_write_5m: 3.75,
            cache_write_1h: 6.0,
            cache_read: 0.30,
        },
    ),
    (
        "claude-haiku-4-5",
        ModelPrice {
            display_name: "Haiku 4.5",
            input: 1.0,
            output: 5.0,
            cache_write_5m: 1.25,
            cache_write_1h: 2.0,
            cache_read: 0.10,
        },
    ),
    (
        "claude-haiku-3-5",
        ModelPrice {
            display_name: "Haiku 3.5",
            input: 0.80,
            output: 4.0,
            cache_write_5m: 1.0,
            cache_write_1h: 1.60,
            cache_read: 0.08,
        },
    ),
    (
        "claude-fable-5-1",
        ModelPrice {
            display_name: "Fable 5.1",
            input: 10.0,
            output: 50.0,
            cache_write_5m: 12.50,
            cache_write_1h: 20.0,
            cache_read: 0.25, // 0.025x 예외
        },
    ),
    (
        "claude-fable-5",
        ModelPrice {
            display_name: "Fable 5",
            input: 10.0,
            output: 50.0,
            cache_write_5m: 12.50,
            cache_write_1h: 20.0,
            cache_read: 1.0,
        },
    ),
    (
        "claude-mythos-5-1",
        ModelPrice {
            display_name: "Mythos 5.1",
            input: 10.0,
            output: 50.0,
            cache_write_5m: 12.50,
            cache_write_1h: 20.0,
            cache_read: 0.25, // 0.025x 예외
        },
    ),
    (
        "claude-mythos-5",
        ModelPrice {
            display_name: "Mythos 5",
            input: 10.0,
            output: 50.0,
            cache_write_5m: 12.50,
            cache_write_1h: 20.0,
            cache_read: 1.0,
        },
    ),
];

/// 모델 ID에서 날짜 접미사(`-` + 숫자 8자리, 예: `-20251001`)를 뗀 정규형을
/// 돌려준다. 접미사가 없으면 소문자화만 해서 그대로 돌려준다.
/// "claude-haiku-4-5-20251001" -> "claude-haiku-4-5".
pub(crate) fn canonical_model_id(model: &str) -> String {
    let lower = model.to_lowercase();
    if let Some(idx) = lower.rfind('-') {
        let tail = &lower[idx + 1..];
        if tail.len() == 8 && tail.chars().all(|c| c.is_ascii_digit()) {
            return lower[..idx].to_string();
        }
    }
    lower
}

/// 정규화된 모델 ID로 단가표를 정확히(exact match) 찾는다. 계열 키워드
/// 포함 여부로 느슨하게 매칭하지 않는다 — "claude-opus-5"가 "claude-opus-5-5"에
/// 잘못 매칭되는 사고를 막기 위해서다.
pub(crate) fn price_for_model(model: &str) -> Option<&'static ModelPrice> {
    let canonical = canonical_model_id(model);
    PRICE_TABLE
        .iter()
        .find(|(id, _)| *id == canonical)
        .map(|(_, p)| p)
}

pub(crate) struct CostResult {
    pub(crate) cost_usd: f64,
    /// false면 모델 ID가 단가표에 없어 비용을 계산하지 않았다(0으로 둠) —
    /// 호출부가 이 값을 근거로 "미산정" 토큰량을 별도로 집계해야 한다.
    pub(crate) pricing_matched: bool,
}

/// `usage.cache_creation`(TTL 분해: `ephemeral_5m_input_tokens`/
/// `ephemeral_1h_input_tokens`) 객체가 있으면 그 값을 그대로 쓰고, 없는
/// 구 로그는 전체 `cache_creation_input_tokens`를 5분 캐시쓰기로 취급한다
/// (1시간 캐시가 생기기 전 로그이거나 필드 누락 — 과소 추정이지 과대청구
/// 방향은 아니다). 반환값 `(5분 캐시쓰기, 1시간 캐시쓰기)`.
pub(crate) fn split_cache_write_tokens(usage: &serde_json::Value) -> (u64, u64) {
    if let Some(cc) = usage.get("cache_creation") {
        let m5 = cc
            .get("ephemeral_5m_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let m1h = cc
            .get("ephemeral_1h_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        return (m5, m1h);
    }
    let legacy = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    (legacy, 0)
}

pub(crate) fn cost_for_usage(
    model: &str,
    input: u64,
    output: u64,
    cache_write_5m: u64,
    cache_write_1h: u64,
    cache_read: u64,
) -> CostResult {
    const M: f64 = 1_000_000.0;
    match price_for_model(model) {
        Some(p) => CostResult {
            cost_usd: (input as f64 / M) * p.input
                + (output as f64 / M) * p.output
                + (cache_write_5m as f64 / M) * p.cache_write_5m
                + (cache_write_1h as f64 / M) * p.cache_write_1h
                + (cache_read as f64 / M) * p.cache_read,
            pricing_matched: true,
        },
        None => CostResult {
            cost_usd: 0.0,
            pricing_matched: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // claude-opus-5 — input=100_000, output=50_000, cache_write_5m=20_000,
    // cache_write_1h=0, cache_read=30_000
    //   = (100_000/1e6)*5.0 + (50_000/1e6)*25.0 + (20_000/1e6)*6.25 + 0 + (30_000/1e6)*0.50
    //   = 0.1*5.0=0.5 + 0.05*25.0=1.25 + 0.02*6.25=0.125 + 0.03*0.50=0.015
    //   = 1.89
    #[test]
    fn calculates_opus_5_cost() {
        let r = cost_for_usage("claude-opus-5", 100_000, 50_000, 20_000, 0, 30_000);
        assert!(r.pricing_matched);
        assert!((r.cost_usd - 1.89).abs() < 1e-9, "opus-5 비용: {}", r.cost_usd);
    }

    // claude-opus-5-5 — 캐시읽기 예외(0.05x=0.20/MTok) 확인.
    // input=1_000_000, output=0, cache_write=0, cache_read=1_000_000
    //   = 1.0*4.0 + 0 + 0 + 0 + 1.0*0.20 = 4.20
    #[test]
    fn calculates_opus_5_5_cost_with_cache_read_exception() {
        let r = cost_for_usage("claude-opus-5-5", 1_000_000, 0, 0, 0, 1_000_000);
        assert!(r.pricing_matched);
        assert!(
            (r.cost_usd - 4.20).abs() < 1e-9,
            "opus-5-5 비용: {}",
            r.cost_usd
        );
    }

    // claude-sonnet-5 — input=200_000, output=80_000, cache_write_5m=10_000,
    // cache_write_1h=0, cache_read=500_000
    //   = 0.2*2.0=0.4 + 0.08*10.0=0.8 + 0.01*2.50=0.025 + 0 + 0.5*0.20=0.1
    //   = 1.325
    #[test]
    fn calculates_sonnet_5_cost() {
        let r = cost_for_usage("claude-sonnet-5", 200_000, 80_000, 10_000, 0, 500_000);
        assert!(r.pricing_matched);
        assert!(
            (r.cost_usd - 1.325).abs() < 1e-9,
            "sonnet-5 비용: {}",
            r.cost_usd
        );
    }

    // 날짜 접미사가 붙은 haiku-4-5 — claude-haiku-4-5-20251001도
    // claude-haiku-4-5 단가로 매칭돼야 한다.
    // input=1_000_000, output=1_000_000, cache_write=0, cache_read=0
    //   = 1.0*1.0 + 1.0*5.0 = 6.0
    #[test]
    fn calculates_haiku_4_5_cost_with_date_suffix() {
        let r = cost_for_usage(
            "claude-haiku-4-5-20251001",
            1_000_000,
            1_000_000,
            0,
            0,
            0,
        );
        assert!(r.pricing_matched);
        assert!(
            (r.cost_usd - 6.0).abs() < 1e-9,
            "haiku-4-5(날짜접미사) 비용: {}",
            r.cost_usd
        );
    }

    // claude-fable-5-1 — 캐시읽기 예외(0.025x=0.25/MTok) 확인.
    // input=1_000_000, cache_read=1_000_000
    //   = 1.0*10.0 + 1.0*0.25 = 10.25
    #[test]
    fn calculates_fable_5_1_cost_with_cache_read_exception() {
        let r = cost_for_usage("claude-fable-5-1", 1_000_000, 0, 0, 0, 1_000_000);
        assert!(r.pricing_matched);
        assert!(
            (r.cost_usd - 10.25).abs() < 1e-9,
            "fable-5-1 비용: {}",
            r.cost_usd
        );
    }

    // 5분/1시간 캐시쓰기 분해가 있는 줄 — claude-sonnet-4-6, cache_write_5m=100_000,
    // cache_write_1h=200_000.
    //   = 0.1*3.75 + 0.2*6.0 = 0.375 + 1.2 = 1.575 (input/output/cache_read=0)
    #[test]
    fn calculates_cost_with_5m_and_1h_cache_write_split() {
        let r = cost_for_usage("claude-sonnet-4-6", 0, 0, 100_000, 200_000, 0);
        assert!(r.pricing_matched);
        assert!(
            (r.cost_usd - 1.575).abs() < 1e-9,
            "5m/1h 분해 비용: {}",
            r.cost_usd
        );
    }

    // 분해 필드 없는 구 로그 — cache_creation 객체가 없고 cache_creation_input_tokens만
    // 있으면 전부 5분 단가로 계산돼야 한다.
    #[test]
    fn split_cache_write_tokens_falls_back_to_5m_for_legacy_logs() {
        let usage = json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_creation_input_tokens": 40_000,
            "cache_read_input_tokens": 0
        });
        let (m5, m1h) = split_cache_write_tokens(&usage);
        assert_eq!(m5, 40_000);
        assert_eq!(m1h, 0);
    }

    // 분해 필드가 있으면 그 값을 그대로 쓴다.
    #[test]
    fn split_cache_write_tokens_uses_breakdown_when_present() {
        let usage = json!({
            "cache_creation": {
                "ephemeral_5m_input_tokens": 1000,
                "ephemeral_1h_input_tokens": 2000
            }
        });
        let (m5, m1h) = split_cache_write_tokens(&usage);
        assert_eq!(m5, 1000);
        assert_eq!(m1h, 2000);
    }

    // 미등록 모델은 비용 0 + pricing_matched=false — sonnet 단가 등으로 조용히
    // 대체되면 안 된다(이번에 고친 과대청구 버그의 핵심 원인).
    #[test]
    fn unmatched_model_contributes_zero_cost_and_is_flagged_unmatched() {
        let r = cost_for_usage("some-unreleased-model", 1_000_000, 1_000_000, 0, 0, 0);
        assert!(!r.pricing_matched);
        assert_eq!(r.cost_usd, 0.0);
    }

    #[test]
    fn canonical_model_id_strips_date_suffix_only() {
        assert_eq!(
            canonical_model_id("claude-haiku-4-5-20251001"),
            "claude-haiku-4-5"
        );
        // 날짜 접미사가 없으면 원문(소문자화만) 그대로.
        assert_eq!(canonical_model_id("claude-opus-5-5"), "claude-opus-5-5");
        // "claude-opus-5"가 "claude-opus-5-5"로 잘못 매칭되지 않는지(exact match) 확인.
        assert!(price_for_model("claude-opus-5").unwrap().display_name == "Opus 5");
        assert!(price_for_model("claude-opus-5-5").unwrap().display_name == "Opus 5.5");
    }
}
