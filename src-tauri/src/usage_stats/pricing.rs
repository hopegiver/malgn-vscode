// ---------------- 단가표 ----------------
// ~/workspace/malgnai/server/lib/pricing.js를 그대로 옮긴 것이다(가족별
// USD/1M 토큰 — opus/sonnet/haiku, 캐시쓰기·캐시읽기 별도 단가, 미매칭 모델은
// sonnet 폴백). `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도
// 바꾸지 않았다.

struct ModelPrice {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

fn model_family(model: &str) -> &'static str {
    let m = model.to_lowercase();
    if m.contains("opus") {
        "opus"
    } else if m.contains("haiku") {
        "haiku"
    } else {
        // sonnet이거나 미매칭 — pricing.js의 modelFamily()와 동일하게 sonnet 폴백.
        "sonnet"
    }
}

fn price_for_family(family: &str) -> ModelPrice {
    match family {
        "opus" => ModelPrice {
            input: 15.0,
            output: 75.0,
            cache_write: 18.75,
            cache_read: 1.5,
        },
        "haiku" => ModelPrice {
            input: 1.0,
            output: 5.0,
            cache_write: 1.25,
            cache_read: 0.1,
        },
        _ => ModelPrice {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.3,
        },
    }
}

pub(crate) fn cost_for_usage(
    model: &str,
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
) -> f64 {
    let p = price_for_family(model_family(model));
    const M: f64 = 1_000_000.0;
    (input as f64 / M) * p.input
        + (output as f64 / M) * p.output
        + (cache_creation as f64 / M) * p.cache_write
        + (cache_read as f64 / M) * p.cache_read
}

#[cfg(test)]
mod tests {
    use super::*;

    // "토큰 도둑" 단가 계산 — pricing.js PRICING 표를 정확히 옮겼는지 결정론적으로
    // 고정한다(비용 계산이라 숫자가 틀리면 안 되는 부분). 1M 토큰씩 넣으면 family별
    // 단가 합과 정확히 같아야 한다.
    #[test]
    fn calculates_cost_using_ported_pricing_table() {
        let opus_cost = cost_for_usage(
            "claude-opus-4-8",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (opus_cost - (15.0 + 75.0 + 18.75 + 1.5)).abs() < 1e-9,
            "opus 단가 계산이 pricing.js와 다릅니다: {opus_cost}"
        );

        let sonnet_cost = cost_for_usage(
            "claude-sonnet-4-6",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (sonnet_cost - (3.0 + 15.0 + 3.75 + 0.3)).abs() < 1e-9,
            "sonnet 단가 계산이 pricing.js와 다릅니다: {sonnet_cost}"
        );

        let haiku_cost = cost_for_usage(
            "claude-haiku-4-5",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        );
        assert!(
            (haiku_cost - (1.0 + 5.0 + 1.25 + 0.1)).abs() < 1e-9,
            "haiku 단가 계산이 pricing.js와 다릅니다: {haiku_cost}"
        );

        // 미매칭 모델명은 pricing.js의 modelFamily()처럼 sonnet으로 폴백해야 한다.
        let unknown_cost = cost_for_usage("some-unreleased-model", 1_000_000, 0, 0, 0);
        assert!(
            (unknown_cost - 3.0).abs() < 1e-9,
            "미매칭 모델 폴백이 sonnet 단가가 아닙니다: {unknown_cost}"
        );
    }
}
