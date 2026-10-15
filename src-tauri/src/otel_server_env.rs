// 허브 수집키 발급·회전 201 응답의 `env`(서버 권장 수집 설정)를 settings.json에 쓸 수 있는 것만 걸러내는 관문.
//
// 신뢰 경계: 201 본문은 네트워크에서 온 값이고, 이 값이 사용자 파일(`~/.claude/settings.json`)로 흘러든다.
// 그래서 서버가 무엇을 보내든 **이 파일의 허용 목록(`SERVER_ENV_KEYS`) 밖의 키는 쓰지 않고**, 허용 목록 안의 키도
// 값 형식이 이 파일의 규칙을 통과해야만 쓴다. 허용 목록은 계약(`device-ingest-key-provisioning.md` 201 예시)의
// 6개 키와 정확히 같다 — 엔드포인트(`OTEL_EXPORTER_OTLP_*_ENDPOINT`)·헤더(`OTEL_EXPORTER_OTLP_HEADERS`)·
// `OTEL_RESOURCE_ATTRIBUTES`·프라이버시 4키(`OTEL_LOG_*`)·`CLAUDE_CODE_ENABLE_TELEMETRY` 등 데이터가 어디로,
// 무엇을 담아 나가는지를 정하는 키는 서버가 보내도 절대 통과하지 못한다.
//
// 거부 동작(부분 적용): 키는 서로 독립이므로 통과한 키만 쓰고 나머지는 이름만 기록한다. 이유는 둘이다.
//   1. 한 키의 형식 위반(예: 서버가 앱이 모르는 temporality를 보냄)이 다른 키(예: 호출 주기 조정)까지 막으면
//      "서버가 값을 바꿔도 앱 수정 없이 따라간다"는 목적이 가장 필요한 순간에 무너진다.
//   2. 거부된 키는 파일의 기존 값이 그대로 남으므로 부분 적용 결과가 "일부는 서버 값, 나머지는 이전 상태"일 뿐
//      어느 키도 앱이 모르는 값으로 바뀌지 않는다.
// 거부는 수집키 적용을 실패시키지 않는다(키를 버리면 회전 예산만 소진된다) — 이 모듈은 `Result`를 돌려주지 않는다.
//
// 비노출: 거부된 키는 **이름만** 남기고 값은 읽은 뒤 즉시 버린다(`rejected`에 값이 들어갈 자리가 없다). 이름도
// 영숫자·`_`·`.`·`-` 64자 이하일 때만 그대로 싣고, 아니면 고정 자리표시자로 바꾼다(서버가 이름 자리에 임의 문자열을
// 실어 상태·로그로 흘려보내지 못하게 한다). 통과한 값은 아래 규칙으로 단순 토큰임이 확인된 것이라 비밀이 아니다.

use serde_json::Value;
use std::collections::BTreeMap;

/// 서버가 정할 수 있는 키 6개(허용 목록). 이 목록을 바꾸면 UI 안내문(`src/onboarding.ts`의 `AUTO_LINK_SERVER_KEYS`)도
/// 같이 바꾼다. 각 키는 `otel_settings::MANAGED_OTEL_KEYS`에 있어야 한다(테스트가 고정).
pub(crate) const SERVER_ENV_KEYS: [&str; 6] = [
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_METRICS_EXPORTER",
    "OTEL_LOGS_EXPORTER",
    "OTEL_LOGS_EXPORT_INTERVAL",
    "OTEL_METRIC_EXPORT_INTERVAL",
    "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
];

/// 서버가 정한 내보내기 주기(ms)의 허용 범위. 하한 5초는 앱이 예전에 쓰던 가장 빠른 기본값이다 — 서버(또는 서버를
/// 가장한 응답)가 1ms 같은 값을 보내 허브 레이트 리밋·일일 쿼터를 태우지 못하게 한다. 상한은 설정 화면의 검증과 같다.
const INTERVAL_MIN_MS: u64 = 5_000;
const INTERVAL_MAX_MS: u64 = 3_600_000;
/// 거부된 키 이름을 싣는 최대 개수(그 이상은 개수만 센다).
const MAX_REJECTED_NAMES: usize = 16;
/// 이름 형식이 아닌 키가 왔을 때 대신 싣는 자리표시자.
pub(crate) const UNNAMED_KEY: &str = "(이름 형식 불명)";
/// `env`가 객체가 아닐 때 싣는 이름.
pub(crate) const ENV_NOT_OBJECT: &str = "env";

/// 걸러낸 결과. `Debug`를 derive해도 안전하다 — `accepted`는 검증된 단순 토큰이고 `rejected`는 이름뿐이다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServerEnv {
    /// 허용 목록에 있고 값 형식을 통과한 (키 → 값). settings.json `env`에 그대로 쓴다.
    pub accepted: BTreeMap<String, String>,
    /// 쓰지 않은 키의 **이름**(중복 제거, 정렬). 값은 어디에도 없다. 상한을 넘으면 `"(그 밖 N개)"`가 붙는다.
    pub rejected: Vec<String>,
}

impl ServerEnv {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.accepted.is_empty() && self.rejected.is_empty()
    }
}

/// 값이 안전한 단순 토큰인가: 1~64자의 가시 ASCII(공백·제어문자·비ASCII 없음). 앞뒤 공백을 다듬어 받아주지 않는다.
fn is_plain_token(v: &str) -> bool {
    !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| (0x21..=0x7E).contains(&b))
}

/// 키별 값 규칙. 모두 "앱이 아는 값"만 통과시킨다.
///   - 주기: 선행 0 없는 10진 숫자만, `INTERVAL_MIN_MS..=INTERVAL_MAX_MS`.
///   - 프로토콜: 허브 `/v1/*`가 받는 `http/protobuf`뿐(수집 준비도 판정도 이 값을 요구한다).
///   - exporter: `otlp`뿐(`none`/`console`은 수집을 끈다 — 서버가 켜고 끄는 키가 아니다).
///   - temporality: 앱 설정 화면이 아는 `cumulative|delta|lowmemory`.
fn value_ok(key: &str, v: &str) -> bool {
    if !is_plain_token(v) {
        return false;
    }
    match key {
        "OTEL_LOGS_EXPORT_INTERVAL" | "OTEL_METRIC_EXPORT_INTERVAL" => {
            v.bytes().all(|b| b.is_ascii_digit())
                && !v.starts_with('0')
                && v.parse::<u64>()
                    .map(|n| (INTERVAL_MIN_MS..=INTERVAL_MAX_MS).contains(&n))
                    .unwrap_or(false)
        }
        "OTEL_EXPORTER_OTLP_PROTOCOL" => v == "http/protobuf",
        "OTEL_METRICS_EXPORTER" | "OTEL_LOGS_EXPORTER" => v == "otlp",
        "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE" => {
            matches!(v, "cumulative" | "delta" | "lowmemory")
        }
        _ => false,
    }
}

fn name_for_report(key: &str) -> String {
    let ok = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
    if ok {
        key.to_string()
    } else {
        UNNAMED_KEY.to_string()
    }
}

/// 201 본문의 `env`(없으면 `None`)를 걸러낸다. 없거나 `null`이거나 빈 객체면 결과도 비어 있다(= 키만 적용, 기존 동작).
pub(crate) fn sanitize_server_env(raw: Option<&Value>) -> ServerEnv {
    let mut out = ServerEnv::default();
    let obj = match raw {
        None | Some(Value::Null) => return out,
        Some(Value::Object(o)) => o,
        Some(_) => {
            out.rejected.push(ENV_NOT_OBJECT.to_string());
            return out;
        }
    };
    let mut rejected: Vec<String> = Vec::new();
    for (key, value) in obj {
        let allowed = SERVER_ENV_KEYS.contains(&key.as_str());
        match value.as_str() {
            Some(v) if allowed && value_ok(key, v) => {
                out.accepted.insert(key.clone(), v.to_string());
            }
            // 허용 목록 밖 / 문자열이 아닌 JSON 타입 / 값 형식 위반 — 이름만 남기고 값은 버린다.
            _ => rejected.push(name_for_report(key)),
        }
    }
    rejected.sort();
    rejected.dedup();
    if rejected.len() > MAX_REJECTED_NAMES {
        let extra = rejected.len() - MAX_REJECTED_NAMES;
        rejected.truncate(MAX_REJECTED_NAMES);
        rejected.push(format!("(그 밖 {extra}개)"));
    }
    out.rejected = rejected;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sanitize(v: Value) -> ServerEnv {
        sanitize_server_env(Some(&v))
    }

    fn contract_env() -> Value {
        json!({
            "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
            "OTEL_METRICS_EXPORTER": "otlp",
            "OTEL_LOGS_EXPORTER": "otlp",
            "OTEL_LOGS_EXPORT_INTERVAL": "60000",
            "OTEL_METRIC_EXPORT_INTERVAL": "60000",
            "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE": "cumulative"
        })
    }

    #[test]
    fn contract_example_passes_in_full() {
        let e = sanitize(contract_env());
        assert_eq!(e.accepted.len(), 6);
        assert!(e.rejected.is_empty());
        assert_eq!(e.accepted["OTEL_LOGS_EXPORT_INTERVAL"], "60000");
    }

    #[test]
    fn missing_null_and_empty_env_are_empty_results() {
        assert!(sanitize_server_env(None).is_empty());
        assert!(sanitize(Value::Null).is_empty());
        assert!(sanitize(json!({})).is_empty());
    }

    #[test]
    fn non_object_env_is_rejected_by_name_only() {
        for v in [json!("x"), json!([1, 2]), json!(5), json!(true)] {
            let e = sanitize(v);
            assert!(e.accepted.is_empty());
            assert_eq!(e.rejected, [ENV_NOT_OBJECT]);
        }
    }

    #[test]
    fn keys_outside_the_allowlist_never_pass_even_with_valid_looking_values() {
        let e = sanitize(json!({
            "OTEL_EXPORTER_OTLP_ENDPOINT": "https://evil.example",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": "https://evil.example/v1/metrics",
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": "https://evil.example/v1/logs",
            "OTEL_EXPORTER_OTLP_HEADERS": "Authorization=Bearer mhk_SECRETSECRETSECRET",
            "OTEL_RESOURCE_ATTRIBUTES": "employee.id=x",
            "OTEL_LOG_USER_PROMPTS": "1",
            "OTEL_LOG_TOOL_CONTENT": "1",
            "OTEL_LOG_TOOL_DETAILS": "1",
            "OTEL_LOG_RAW_API_BODIES": "1",
            "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
            "ANTHROPIC_API_KEY": "sk-x",
            "PATH": "/tmp",
            "otel_logs_export_interval": "60000",
            "OTEL_LOGS_EXPORT_INTERVAL ": "60000",
        }));
        assert!(e.accepted.is_empty(), "{e:?}");
        assert_eq!(e.rejected.len(), 14);
        assert!(e.rejected.contains(&"OTEL_EXPORTER_OTLP_HEADERS".to_string()));
        // 이름 형식이 아닌 키(끝 공백)는 자리표시자로 바뀐다.
        assert!(e.rejected.contains(&UNNAMED_KEY.to_string()));
    }

    #[test]
    fn rejected_report_and_debug_never_contain_values() {
        let secret = "Authorization=Bearer mhk_SECRETSECRETSECRETSECRETSECRETSECRETSECRE";
        let e = sanitize(json!({
            "OTEL_EXPORTER_OTLP_HEADERS": secret,
            "OTEL_LOGS_EXPORT_INTERVAL": "not-a-number-xyzzy",
            "OTEL_METRICS_EXPORTER": { "nested": "nested-secret-xyzzy" },
        }));
        let dump = format!("{e:?} {:?}", e.rejected);
        for leak in ["mhk_", "SECRET", "xyzzy", "Bearer"] {
            assert!(!dump.contains(leak), "{leak} leaked: {dump}");
        }
        assert_eq!(e.rejected.len(), 3);
    }

    #[test]
    fn format_violations_are_rejected_per_key_and_others_still_pass() {
        let bad: [(&str, Value); 17] = [
            ("OTEL_LOGS_EXPORT_INTERVAL", json!(60000)), // 숫자 JSON 타입
            ("OTEL_LOGS_EXPORT_INTERVAL", json!(null)),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!(["60000"])),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("abc")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("-1")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("+60000")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("060000")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!(" 60000")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("60000\n")),
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("1000")), // 하한 미만
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("3600001")), // 상한 초과
            ("OTEL_LOGS_EXPORT_INTERVAL", json!("99999999999999999999999")),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", json!("grpc")),
            ("OTEL_LOGS_EXPORTER", json!("console")),
            ("OTEL_METRICS_EXPORTER", json!("none")),
            ("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", json!("Cumulative")),
        ];
        for (key, value) in bad {
            let mut env = contract_env();
            env[key] = value.clone();
            let e = sanitize(env);
            assert!(!e.accepted.contains_key(key), "{key}={value}");
            assert_eq!(e.rejected, [key], "{key}={value}");
            assert_eq!(e.accepted.len(), 5, "나머지 키는 통과한다: {key}={value}");
        }
    }

    #[test]
    fn control_characters_and_non_ascii_values_are_rejected() {
        for v in ["otlp\u{0}", "otl\u{7f}p", "otlp\r\n", "ot lp", "otlp\u{200b}", "오tlp"] {
            let e = sanitize(json!({ "OTEL_LOGS_EXPORTER": v }));
            assert!(e.accepted.is_empty(), "{v:?}");
            assert_eq!(e.rejected, ["OTEL_LOGS_EXPORTER"]);
        }
    }

    #[test]
    fn interval_bounds_are_inclusive() {
        for (v, ok) in [("5000", true), ("4999", false), ("3600000", true), ("3600001", false)] {
            let e = sanitize(json!({ "OTEL_METRIC_EXPORT_INTERVAL": v }));
            assert_eq!(e.accepted.contains_key("OTEL_METRIC_EXPORT_INTERVAL"), ok, "{v}");
        }
    }

    #[test]
    fn many_rejected_names_are_capped_and_counted() {
        let mut o = serde_json::Map::new();
        for i in 0..40 {
            o.insert(format!("EXTRA_{i:02}"), json!("x"));
        }
        let e = sanitize(Value::Object(o));
        assert_eq!(e.rejected.len(), MAX_REJECTED_NAMES + 1);
        assert_eq!(e.rejected.last().unwrap(), "(그 밖 24개)");
    }
}
