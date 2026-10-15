// OTel 설정 읽기/저장(설계 §6·§7·§8). `~/.claude/settings.json`은 권한·훅 등
// OTel과 무관한 설정도 담고 있어 전체를 프론트에 보여주지 않는다 — allowlist
// 상수(`MANAGED_OTEL_KEYS`) 하나가 읽기·쓰기·검증을 모두 지배한다.
//
// 가장 위험한 사고 경로: 파싱 못 한 `settings.json`을 덮어써 사용자의
// hooks/permissions를 날리는 것. 그래서 저장 알고리즘의 1단계는 항상
// "파싱 실패 시 즉시 Err, 아무것도 쓰지 않는다"다.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use crate::otel_ingest_key::{
    apply_change, assess_endpoints, EndpointAssessment, HeadersEdit, IngestKeyChange,
    IngestKeyStatus, SecretString, HEADERS_ENV_KEY,
};

pub(crate) const MANAGED_OTEL_KEYS: [&str; 15] = [
    "CLAUDE_CODE_ENABLE_TELEMETRY",
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_METRICS_EXPORTER",
    "OTEL_LOGS_EXPORTER",
    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
    "OTEL_METRIC_EXPORT_INTERVAL",
    "OTEL_LOGS_EXPORT_INTERVAL",
    "OTEL_LOG_USER_PROMPTS",
    "OTEL_LOG_TOOL_CONTENT",
    "OTEL_LOG_TOOL_DETAILS",
    "OTEL_LOG_RAW_API_BODIES",
    "OTEL_RESOURCE_ATTRIBUTES",
    // 허브 수집키(`Authorization=Bearer mhk_…`) — allowlist에는 있지만 `values`로는 읽지도
    // 쓰지도 않는다. 원문 비노출 불변식 때문에 `otel_ingest_key` 모듈이 전담한다
    // (응답은 `ingestKey` 상태, 저장은 `ingestKey` 변경 요청으로만).
    HEADERS_ENV_KEY,
];

/// 프라이버시 4개 — UI에서 값만 보여주고 편집 불가로 렌더한다(설계 §6). 이
/// 값을 1로 올리면 프롬프트 원문·툴 입출력이 collector로 나간다.
const READ_ONLY_KEYS: [&str; 4] = [
    "OTEL_LOG_USER_PROMPTS",
    "OTEL_LOG_TOOL_CONTENT",
    "OTEL_LOG_TOOL_DETAILS",
    "OTEL_LOG_RAW_API_BODIES",
];

/// (b) 허브(malgnai-hub)의 OTLP 수신 엔드포인트 — 기본값으로 소스에 고정한다.
/// 허브 주소는 공개값이라(플러그인 매니페스트·`hub_auth`에도 이미 있다) 빌드 시점
/// 주입(`option_env!`)이 필요 없다. 빌드 환경변수로 덮어쓰는 경로를 두지 않는 것은
/// 의도다 — CI에 옛 collector 주소 secret이 남아 있어도 릴리스가 그것을 따르지 않게 한다.
const DEFAULT_METRICS_ENDPOINT: &str = "https://malgnai-hub.apiserver.kr/v1/metrics";
const DEFAULT_LOGS_ENDPOINT: &str = "https://malgnai-hub.apiserver.kr/v1/logs";

fn endpoint_defaults() -> (String, String) {
    (
        DEFAULT_METRICS_ENDPOINT.to_string(),
        DEFAULT_LOGS_ENDPOINT.to_string(),
    )
}

/// (a) 소스에 하드코딩하는 범용 기본값 — 사내 정보가 전혀 없는 프로토콜·
/// 프라이버시 값만.
fn hardcoded_defaults() -> Vec<(&'static str, &'static str)> {
    vec![
        ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
        ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf"),
        ("OTEL_METRICS_EXPORTER", "otlp"),
        ("OTEL_LOGS_EXPORTER", "otlp"),
        (
            "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
            "cumulative",
        ),
        ("OTEL_METRIC_EXPORT_INTERVAL", "60000"),
        ("OTEL_LOGS_EXPORT_INTERVAL", "5000"),
        ("OTEL_LOG_USER_PROMPTS", "0"),
        ("OTEL_LOG_TOOL_CONTENT", "0"),
        ("OTEL_LOG_TOOL_DETAILS", "0"),
        ("OTEL_LOG_RAW_API_BODIES", "0"),
    ]
}

fn build_defaults() -> (std::collections::BTreeMap<String, String>, bool) {
    let mut map: std::collections::BTreeMap<String, String> = hardcoded_defaults()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let (metrics_ep, logs_ep) = endpoint_defaults();
    map.insert(
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT".to_string(),
        metrics_ep,
    );
    map.insert("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT".to_string(), logs_ep);
    // 엔드포인트 기본값은 이제 항상 존재한다 — 프론트 계약(endpointDefaultsInjected)은 유지한다.
    (map, true)
}

pub(crate) fn settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("settings.json"))
}

#[derive(Serialize, Clone, Debug)]
pub struct OtelSettings {
    #[serde(rename = "settingsPath")]
    pub settings_path: String,
    #[serde(rename = "fileExists")]
    pub file_exists: bool,
    #[serde(rename = "parseError")]
    pub parse_error: Option<String>,
    #[serde(rename = "managedKeys")]
    pub managed_keys: Vec<String>,
    #[serde(rename = "readOnlyKeys")]
    pub read_only_keys: Vec<String>,
    pub values: std::collections::BTreeMap<String, String>,
    pub defaults: std::collections::BTreeMap<String, String>,
    #[serde(rename = "endpointDefaultsInjected")]
    pub endpoint_defaults_injected: bool,
    /// 허브 수집키 상태(설정됨/미설정/형식 불명 + 마스킹). 원문은 어디에도 싣지 않는다.
    #[serde(rename = "ingestKey")]
    pub ingest_key: IngestKeyStatus,
    /// 유효 엔드포인트(파일 값 → 기본값)가 허브인지·평문인지·허브 밖인지 — 수집키가 어디로
    /// 나가는지에 대한 화면 경고 근거. URL 자체는 싣지 않는다.
    #[serde(rename = "ingestEndpoint")]
    pub ingest_endpoint: EndpointAssessment,
}

/// 수집키가 실려 나가는 유효 엔드포인트의 성격. 파일에 값이 있으면 그것, 없으면 기본값을 본다
/// (Claude Code가 실제로 쓰는 값과 같은 우선순위).
fn assess_effective_endpoints(
    values: &std::collections::BTreeMap<String, String>,
    defaults: &std::collections::BTreeMap<String, String>,
) -> EndpointAssessment {
    let effective = [
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    ]
    .into_iter()
    .filter_map(|k| values.get(k).or_else(|| defaults.get(k)))
    .map(String::as_str);
    assess_endpoints(effective, &crate::hub_auth::consts::hub_origin())
}

fn read_managed_values(
    path: &Path,
) -> (
    std::collections::BTreeMap<String, String>,
    Option<String>,
    IngestKeyStatus,
) {
    let mut values = std::collections::BTreeMap::new();
    let mut parse_error = None;
    let mut raw_headers: Option<String> = None;
    if let Ok(content) = std::fs::read_to_string(path) {
        match serde_json::from_str::<Value>(&content) {
            Ok(root) => {
                if let Some(env) = root.get("env").and_then(|v| v.as_object()) {
                    for key in MANAGED_OTEL_KEYS {
                        let Some(v) = env.get(key).and_then(|v| v.as_str()) else {
                            continue;
                        };
                        if key == HEADERS_ENV_KEY {
                            // 수집키가 든 헤더 문자열은 `values`에 절대 싣지 않는다 —
                            // 상태/마스킹으로만 가공해 내보낸다.
                            raw_headers = Some(v.to_string());
                        } else {
                            values.insert(key.to_string(), v.to_string());
                        }
                    }
                }
            }
            Err(e) => parse_error = Some(format!("설정 파일을 해석하지 못했습니다: {e}")),
        }
    }
    let ingest_key = IngestKeyStatus::from_headers(raw_headers.as_deref());
    (values, parse_error, ingest_key)
}

fn build_status(path: &Path, path_display: String, file_exists: bool) -> OtelSettings {
    let (defaults, injected) = build_defaults();
    let (values, parse_error, ingest_key) = read_managed_values(path);
    let ingest_endpoint = assess_effective_endpoints(&values, &defaults);
    OtelSettings {
        settings_path: path_display,
        file_exists,
        parse_error,
        managed_keys: MANAGED_OTEL_KEYS.iter().map(|s| s.to_string()).collect(),
        read_only_keys: READ_ONLY_KEYS.iter().map(|s| s.to_string()).collect(),
        values,
        defaults,
        endpoint_defaults_injected: injected,
        ingest_key,
        ingest_endpoint,
    }
}

#[tauri::command]
pub fn otel_settings_get() -> OtelSettings {
    let Some(path) = settings_path() else {
        return build_status(Path::new(""), String::new(), false);
    };
    let file_exists = path.is_file();
    let display = path.to_string_lossy().to_string();
    build_status(&path, display, file_exists)
}

#[derive(Deserialize, Clone, Debug)]
pub struct OtelIdentity {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// `Debug`는 derive해도 안전하다 — 수집키는 `ingest_key`의 `SecretString`으로만 들어오고
/// 그 Debug는 값을 가린다(`values`에는 수집키 키가 들어올 수 없고 저장 시 거부된다).
#[derive(Deserialize, Clone, Debug)]
pub struct OtelSavePayload {
    pub values: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub identity: Option<OtelIdentity>,
    /// 수집키 변경 요청. 생략하면 `Keep`(기존 키 그대로) — 마스킹된 표시값이 실제 키를
    /// 덮어쓰는 경로가 없다.
    #[serde(default, rename = "ingestKey")]
    pub ingest_key: IngestKeyChange,
}

/// `A-Za-z0-9-_.~`를 제외한 모든 바이트를 `%XX`(대문자 hex)로 바꾸는 헬퍼
/// (설계 §8) — `OTEL_RESOURCE_ATTRIBUTES`는 W3C Baggage 문법이라 비ASCII
/// 원문을 값에 그대로 넣는 것은 규격 위반이다. `url`/`form_urlencoded` crate를
/// 새로 끌어오지 않고 직접 구현한다(`form_urlencoded`는 공백을 `+`로 바꿔
/// baggage에 부적합, `url`은 이 용도의 인코더를 공개하지 않는다).
pub(crate) fn percent_encode_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let b = *byte;
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 기존 값 보존 병합 — 파일의 `OTEL_RESOURCE_ATTRIBUTES`를 `,`로 분해해 순서
/// 있는 `(k, v)` 목록으로 만들고 `employee.id/name` 2개만 upsert한 뒤
/// 나머지(예: `team=…`)는 원래 순서대로 남긴다(설계 §8, 통째로 덮어쓰지 않음).
/// `identity.email`은 `employee.id`를 뽑아내는 데만 쓰고 그 자체를 속성으로
/// 쓰지는 않는다.
pub(crate) fn merge_resource_attributes(
    existing: &str,
    identity: &OtelIdentity,
) -> Result<String, String> {
    let email = identity.email.trim();
    let Some(at_pos) = email.find('@') else {
        return Err("identity.email에 '@'가 없습니다.".to_string());
    };
    let id = &email[..at_pos];

    let mut pairs: Vec<(String, String)> = Vec::new();
    if !existing.trim().is_empty() {
        for part in existing.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once('=') {
                pairs.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }

    let mut upsert = |key: &str, value: String| {
        if let Some(existing_pair) = pairs.iter_mut().find(|(k, _)| k == key) {
            existing_pair.1 = value;
        } else {
            pairs.push((key.to_string(), value));
        }
    };

    upsert("employee.id", percent_encode_value(id));
    let name_trimmed = identity.name.as_deref().unwrap_or("").trim();
    if !name_trimmed.is_empty() {
        upsert("employee.name", percent_encode_value(name_trimmed));
    }

    Ok(pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(","))
}

fn validate_value(key: &str, value: &str) -> Result<(), String> {
    match key {
        "CLAUDE_CODE_ENABLE_TELEMETRY"
        | "OTEL_LOG_USER_PROMPTS"
        | "OTEL_LOG_TOOL_CONTENT"
        | "OTEL_LOG_TOOL_DETAILS"
        | "OTEL_LOG_RAW_API_BODIES" => {
            if value != "0" && value != "1" {
                return Err(format!("'{key}' 값은 0 또는 1이어야 합니다."));
            }
        }
        "OTEL_METRIC_EXPORT_INTERVAL" | "OTEL_LOGS_EXPORT_INTERVAL" => {
            let n: i64 = value
                .parse()
                .map_err(|_| format!("'{key}' 값은 정수여야 합니다."))?;
            if !(1..=3_600_000).contains(&n) {
                return Err(format!("'{key}' 값은 1~3,600,000 범위여야 합니다."));
            }
        }
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT" | "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT" => {
            if !(value.starts_with("http://") || value.starts_with("https://")) {
                return Err(format!(
                    "'{key}' 값은 http:// 또는 https://로 시작해야 합니다."
                ));
            }
        }
        "OTEL_METRICS_EXPORTER" | "OTEL_LOGS_EXPORTER" => {
            if !["otlp", "console", "none"].contains(&value) {
                return Err(format!(
                    "'{key}' 값은 otlp|console|none 중 하나여야 합니다."
                ));
            }
        }
        "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE" => {
            if !["cumulative", "delta", "lowmemory"].contains(&value) {
                return Err(format!(
                    "'{key}' 값은 cumulative|delta|lowmemory 중 하나여야 합니다."
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

// 원자적 쓰기 + 백업 경로 유틸은 `crate::fs_atomic`로 추출됨(3번째 소비자인
// `app_links`가 생기면서 공용 모듈화 — `docs/design-app-links.md` §2-4).
use crate::fs_atomic::{backup_path_for, write_atomically_private};

/// 수집키 헤더 변경을 `env` 객체에 적용한다(본 파일과 Clear 백업이 같은 규칙을 쓴다).
fn apply_headers_edit(env_obj: &mut Map<String, Value>, edit: &HeadersEdit) {
    match edit {
        HeadersEdit::Unchanged => {}
        HeadersEdit::Remove => {
            env_obj.remove(HEADERS_ENV_KEY);
        }
        HeadersEdit::Replace(headers) => {
            env_obj.insert(HEADERS_ENV_KEY.to_string(), Value::String(headers.clone()));
        }
    }
}

/// 직전 원본에서 수집키(`Authorization`)만 뺀 백업 내용. `Authorization` 외 헤더와 나머지
/// 설정은 그대로 둔다(키가 든 `env.OTEL_EXPORTER_OTLP_HEADERS`에만 본 저장과 같은 변경을 적용).
/// 이미 파싱에 성공한 파일이라 해석 실패는 일어나지 않지만, 실패하면 원문을 쓰는 대신 Err로
/// 멈춘다(키를 백업에 남기는 쪽으로 조용히 폴백하지 않는다).
fn backup_without_ingest_key(content: &str, edit: &HeadersEdit) -> Result<String, String> {
    let mut root: Value = serde_json::from_str(content)
        .map_err(|e| format!("백업을 만들지 못해 저장을 중단합니다: {e}"))?;
    if let Some(env_obj) = root.get_mut("env").and_then(|v| v.as_object_mut()) {
        apply_headers_edit(env_obj, edit);
    }
    serde_json::to_string_pretty(&root).map_err(|e| format!("백업을 직렬화하지 못했습니다: {e}"))
}

/// 저장 알고리즘(순서 고정, 설계 §7):
/// 1. 파싱 실패 시 즉시 `Err` — 파싱 못 한 파일을 절대 덮어쓰지 않는다.
/// 2. allowlist 밖 키는 `Err`(조용히 무시하지 않는다). readOnly 키(프라이버시
///    4개)는 `"0"`(끔) 또는 빈 문자열(삭제)만 허용하고 그 외 값은 `Err` — 이
///    앱을 통해서는 절대 `"1"`로 켤 수 없다는 불변식을 여기 한 곳에서 지킨다.
/// 3. 키별 형식 검증.
/// 4. `identity`가 있으면 `OTEL_RESOURCE_ATTRIBUTES`를 조립해 주입한다
///    (프론트가 이 키를 직접 보내면 `Err`).
/// 5. `env` 객체에 upsert(빈 문자열=삭제 신호). allowlist 밖 키·최상위 키는
///    전부 보존한다(이 함수는 "env"라는 이름의 하위 객체 하나만 건드리고,
///    그 안에서도 allowlist 키만 만진다 — 그 외 모든 값은 손대지 않는다).
/// 6. 쓰기 전 원본을 `settings.json.malgn-bak`으로 백업(롤링 1세대). 수집키 삭제(Clear)일
///    때만 백업에서도 `Authorization`을 뺀다.
/// 7. 원자적 쓰기. 수집키(`OTEL_EXPORTER_OTLP_HEADERS`)가 이 파일과 백업에 들어갈 수 있어
///    둘 다 unix에서 `0600`으로 쓴다(`write_atomically_private`) — 키가 없는 저장에서도
///    일관되게 적용한다(파일에 키를 손으로 넣은 사용자의 경우까지 덮는다).
///
/// 수집키는 `values`가 아니라 `payload.ingest_key`로만 바꾼다(원문 비노출 불변식,
/// `otel_ingest_key.rs` 머리말). 기존 헤더의 `Authorization` 외 항목은 보존한다.
pub(crate) fn save_to_settings_file(path: &Path, payload: OtelSavePayload) -> Result<(), String> {
    let existing_content = std::fs::read_to_string(path).ok();
    let mut root: Value = match &existing_content {
        Some(content) => serde_json::from_str(content)
            .map_err(|e| format!("기존 설정 파일을 해석하지 못해 저장을 중단합니다: {e}"))?,
        None => Value::Object(Map::new()),
    };
    if !root.is_object() {
        return Err("설정 파일의 최상위 값이 객체가 아닙니다.".to_string());
    }

    for (key, value) in &payload.values {
        if key == "OTEL_RESOURCE_ATTRIBUTES" {
            return Err("OTEL_RESOURCE_ATTRIBUTES는 identity를 통해서만 설정할 수 있습니다.".to_string());
        }
        if key == HEADERS_ENV_KEY {
            // 값은 에러에 인용하지 않는다(수집키 원문이 실려 있을 수 있다).
            return Err(format!(
                "{HEADERS_ENV_KEY}는 수집키 입력(ingestKey)을 통해서만 설정할 수 있습니다."
            ));
        }
        if !MANAGED_OTEL_KEYS.contains(&key.as_str()) {
            return Err(format!("'{key}'는 관리 대상 키가 아닙니다."));
        }
        if READ_ONLY_KEYS.contains(&key.as_str()) && !(value.is_empty() || value == "0") {
            return Err(format!(
                "'{key}'는 프라이버시 보호를 위해 이 앱에서 0(끔)으로만 저장할 수 있습니다."
            ));
        }
    }
    for (key, value) in &payload.values {
        if !value.is_empty() {
            validate_value(key, value)?;
        }
    }

    let mut values = payload.values.clone();
    if let Some(identity) = &payload.identity {
        let existing_attrs = root
            .get("env")
            .and_then(|e| e.get("OTEL_RESOURCE_ATTRIBUTES"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let merged = merge_resource_attributes(existing_attrs, identity)?;
        values.insert("OTEL_RESOURCE_ATTRIBUTES".to_string(), merged);
    }

    // 수집키 변경 — 입력 검증 실패는 파일을 건드리기 전에 Err로 끝난다.
    let existing_headers = root
        .get("env")
        .and_then(|e| e.get(HEADERS_ENV_KEY))
        .and_then(|v| v.as_str());
    let headers_edit = apply_change(existing_headers, &payload.ingest_key)?;

    let obj = root.as_object_mut().expect("위에서 object임을 확인했다");
    let env_value = obj.entry("env").or_insert_with(|| Value::Object(Map::new()));
    if !env_value.is_object() {
        *env_value = Value::Object(Map::new());
    }
    let env_obj = env_value.as_object_mut().expect("방금 object로 만들었다");
    for (key, value) in values {
        if value.is_empty() {
            env_obj.remove(&key);
        } else {
            env_obj.insert(key, Value::String(value));
        }
    }
    apply_headers_edit(env_obj, &headers_edit);

    // 저장 검증: 텔레메트리가 켜져 있는데 엔드포인트가 비었거나 형식이
    // 틀리면 저장을 거부한다(반쯤 켜진 상태로 저장되면 원인 모를 미수집이 된다).
    let telemetry_enabled = env_obj
        .get("CLAUDE_CODE_ENABLE_TELEMETRY")
        .and_then(|v| v.as_str())
        == Some("1");
    if telemetry_enabled {
        for ep_key in [
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        ] {
            let ep = env_obj.get(ep_key).and_then(|v| v.as_str()).unwrap_or("");
            if ep.is_empty() || !(ep.starts_with("http://") || ep.starts_with("https://")) {
                return Err(format!(
                    "텔레메트리가 켜져 있는데 '{ep_key}'가 비어 있거나 올바른 URL이 아닙니다."
                ));
            }
        }
    }

    if let Some(content) = &existing_content {
        let backup_path = backup_path_for(path);
        // 수집키 삭제(Clear)는 직전 키가 백업에 남지 않게 한다 — 안 그러면 "삭제"해도 키가
        // `settings.json.malgn-bak`에 평문으로 한 세대 남고, 다음 저장 때까지 지워지지 않는다.
        // 그 밖의 저장은 백업이 "저장 직전 원본"이다(롤백 가치 유지).
        let backup_content = if matches!(payload.ingest_key, IngestKeyChange::Clear) {
            backup_without_ingest_key(content, &headers_edit)?
        } else {
            content.clone()
        };
        write_atomically_private(&backup_path, &backup_content)
            .map_err(|e| format!("백업 파일을 쓰지 못했습니다: {e}"))?;
    }

    let pretty = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("설정을 직렬화하지 못했습니다: {e}"))?;
    write_atomically_private(path, &pretty)
}

/// 외부에서 주입받은 수집키(`mhk_…`)를 `settings.json`에 적용하는 진입점.
///
/// 수집키가 설정 파일에 들어가는 길은 `IngestKeyChange::Set`을 담은 `save_to_settings_file`
/// 하나뿐이다 — 검증(`normalize_ingest_key`)·`Authorization` 헤더 병합·백업(롤링 1세대)·
/// 원자적 쓰기·`0600`이 전부 거기 있다. 수동 입력(`otel_settings_save`, 다른 설정값과 한 번에
/// 저장)과 이 함수(키만 적용)는 같은 엔진으로 합류하며, 둘이 같은 입력에 같은 파일을 만든다는
/// 것은 `otel_link`의 테스트가 고정한다. 키의 출처(사용자 붙여넣기 / 허브 발급 응답)는 여기서
/// 구분하지 않고, 기기 토큰(`mhd_`)은 형식 검증에서 거부된다(수집키 형식이 아니다).
///
/// 실패 문자열은 입력을 인용하지 않는 고정 문구다(`otel_ingest_key.rs` 불변식).
#[allow(dead_code)] // 허브 발급 계약 후 `otel_link::issue_and_apply`가 실제 호출한다(현재는 테스트만).
pub(crate) fn apply_ingest_key(path: &Path, key: SecretString) -> Result<(), String> {
    save_to_settings_file(
        path,
        OtelSavePayload {
            values: Default::default(),
            identity: None,
            ingest_key: IngestKeyChange::Set { key },
        },
    )
}

/// 연동 상태 도출용 읽기 — (`settings.json` 해석 실패 여부, 수집키 상태). 값 문자열은 내보내지 않는다.
pub(crate) fn read_ingest_inputs(path: &Path) -> (bool, IngestKeyStatus) {
    let (_values, parse_error, ingest_key) = read_managed_values(path);
    (parse_error.is_some(), ingest_key)
}

#[tauri::command]
pub fn otel_settings_save(payload: OtelSavePayload) -> Result<OtelSettings, String> {
    let path = settings_path().ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;
    save_to_settings_file(&path, payload)?;
    Ok(otel_settings_get())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-otel-settings-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir.join("settings.json")
    }

    fn sample_payload(values: &[(&str, &str)]) -> OtelSavePayload {
        OtelSavePayload {
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            identity: None,
            ingest_key: IngestKeyChange::Keep,
        }
    }

    // 엔드포인트 기본값은 빌드 환경과 무관하게 허브 주소로 고정된다.
    #[test]
    fn endpoint_defaults_point_to_hub() {
        let (defaults, injected) = build_defaults();
        assert!(injected);
        assert_eq!(
            defaults["OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"],
            "https://malgnai-hub.apiserver.kr/v1/metrics"
        );
        assert_eq!(
            defaults["OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"],
            "https://malgnai-hub.apiserver.kr/v1/logs"
        );
    }

    // 신규(최소) — MANAGED_OTEL_KEYS 밖 키 저장 거부.
    #[test]
    fn save_rejects_key_outside_allowlist() {
        let path = temp_settings_path("outside-allowlist");
        let result = save_to_settings_file(&path, sample_payload(&[("NOT_MANAGED_KEY", "x")]));
        assert!(result.is_err());
        assert!(!path.exists(), "검증 실패 시 파일을 쓰면 안 된다");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_rejects_read_only_key() {
        let path = temp_settings_path("read-only");
        let result = save_to_settings_file(&path, sample_payload(&[("OTEL_LOG_USER_PROMPTS", "1")]));
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // 신규 — readOnly 키라도 "0"(끔)은 저장을 허용한다(자동 기본값 채우기 용도).
    #[test]
    fn save_allows_read_only_key_when_zero() {
        let path = temp_settings_path("read-only-zero");
        let result = save_to_settings_file(&path, sample_payload(&[("OTEL_LOG_USER_PROMPTS", "0")]));
        assert!(result.is_ok());
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["env"]["OTEL_LOG_USER_PROMPTS"], "0");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // 신규 — readOnly 키는 "0" 외의 값(예: "1")은 여전히 거부한다. 위 테스트와
    // 짝을 이뤄 "0만 예외"라는 불변식의 양쪽 경계를 함께 고정한다.
    #[test]
    fn save_rejects_read_only_key_when_one() {
        let path = temp_settings_path("read-only-one");
        let result = save_to_settings_file(&path, sample_payload(&[("OTEL_LOG_USER_PROMPTS", "1")]));
        assert!(result.is_err());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // 신규(최소) — 파싱 불가 settings.json에 저장 시도 시 파일이 바뀌지 않고 Err.
    // 가장 위험한 사고 경로(hooks/permissions를 날리는 것)를 회귀로 고정한다.
    #[test]
    fn save_does_not_overwrite_unparseable_settings_file() {
        let path = temp_settings_path("unparseable");
        let original = "{ this is not valid json";
        std::fs::write(&path, original).unwrap();

        let result = save_to_settings_file(
            &path,
            sample_payload(&[("CLAUDE_CODE_ENABLE_TELEMETRY", "0")]),
        );

        assert!(result.is_err(), "파싱 실패한 파일에 저장하면 Err여야 한다");
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, original, "파싱 못 한 파일은 절대 덮어쓰면 안 된다");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // 신규(최소) — settings.json 저장 시 최상위 키 전부 보존.
    #[test]
    fn save_preserves_unrelated_top_level_keys() {
        let path = temp_settings_path("preserve-top-level");
        let original = serde_json::json!({
            "permissions": { "allow": ["Bash(git *)"] },
            "hooks": { "PreToolUse": [] },
            "model": "sonnet",
            "env": { "SOME_OTHER_VAR": "keep-me" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        save_to_settings_file(&path, sample_payload(&[("CLAUDE_CODE_ENABLE_TELEMETRY", "0")]))
            .expect("저장에 성공해야 합니다");

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["permissions"], original["permissions"]);
        assert_eq!(saved["hooks"], original["hooks"]);
        assert_eq!(saved["model"], original["model"]);
        assert_eq!(saved["env"]["SOME_OTHER_VAR"], "keep-me");
        assert_eq!(saved["env"]["CLAUDE_CODE_ENABLE_TELEMETRY"], "0");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_creates_backup_file_on_existing_settings() {
        let path = temp_settings_path("backup");
        let original = serde_json::json!({ "env": {} });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        save_to_settings_file(&path, sample_payload(&[("CLAUDE_CODE_ENABLE_TELEMETRY", "0")]))
            .expect("저장에 성공해야 합니다");

        let backup = backup_path_for(&path);
        assert!(backup.is_file(), "백업 파일이 생성되어야 한다");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_rejects_when_telemetry_enabled_without_valid_endpoints() {
        let path = temp_settings_path("telemetry-guard");
        let result = save_to_settings_file(
            &path,
            sample_payload(&[("CLAUDE_CODE_ENABLE_TELEMETRY", "1")]),
        );
        assert!(
            result.is_err(),
            "엔드포인트 없이 텔레메트리를 켜면 거부해야 한다"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_empty_string_value_deletes_key() {
        let path = temp_settings_path("delete-key");
        let original = serde_json::json!({ "env": { "OTEL_METRICS_EXPORTER": "otlp" } });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        save_to_settings_file(&path, sample_payload(&[("OTEL_METRICS_EXPORTER", "")]))
            .expect("저장에 성공해야 합니다");

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(saved["env"].get("OTEL_METRICS_EXPORTER").is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // 신규(최소) — percent-encoding 한글・,・=.
    #[test]
    fn percent_encode_value_handles_korean_comma_and_equals() {
        let encoded = percent_encode_value("김,철=수");
        assert!(!encoded.contains(','));
        assert!(!encoded.contains('='));
        assert!(encoded.contains("%2C"), "쉼표는 %2C로 인코딩되어야 한다");
        assert!(encoded.contains("%3D"), "등호는 %3D로 인코딩되어야 한다");
    }

    #[test]
    fn percent_encode_value_keeps_unreserved_ascii_untouched() {
        assert_eq!(percent_encode_value("dev-user_01.test~x"), "dev-user_01.test~x");
    }

    // 신규(최소) — OTEL_RESOURCE_ATTRIBUTES의 비-employee 항목 보존.
    #[test]
    fn merge_resource_attributes_preserves_non_employee_entries() {
        let identity = OtelIdentity {
            email: "dev@malgnsoft.com".to_string(),
            name: Some("하근호".to_string()),
        };
        let merged = merge_resource_attributes("team=platform,region=kr", &identity).unwrap();
        assert!(merged.contains("team=platform"));
        assert!(merged.contains("region=kr"));
        assert!(merged.contains("employee.id=dev"));
        assert!(
            !merged.contains("employee.email"),
            "employee.email은 별도 속성으로 추가하지 않는다"
        );
        assert!(merged.starts_with("team=platform"), "기존 순서를 유지해야 한다");
    }

    #[test]
    fn merge_resource_attributes_rejects_email_without_at_sign() {
        let identity = OtelIdentity {
            email: "not-an-email".to_string(),
            name: None,
        };
        assert!(merge_resource_attributes("", &identity).is_err());
    }

    #[test]
    fn merge_resource_attributes_omits_name_when_blank() {
        let identity = OtelIdentity {
            email: "dev@malgnsoft.com".to_string(),
            name: Some("   ".to_string()),
        };
        let merged = merge_resource_attributes("", &identity).unwrap();
        assert!(!merged.contains("employee.name"));
    }

    #[test]
    fn save_rejects_direct_resource_attributes_from_frontend() {
        let path = temp_settings_path("reject-direct-attrs");
        let result = save_to_settings_file(&path, sample_payload(&[("OTEL_RESOURCE_ATTRIBUTES", "x=y")]));
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // ---------------- 허브 수집키(OTEL_EXPORTER_OTLP_HEADERS) ----------------

    /// 테스트 전용 가짜 키(`mhk_` + `A`×43). 실제 키가 아니다.
    fn fake_ingest_key() -> String {
        format!("mhk_{}", "A".repeat(43))
    }

    fn set_key_payload(key: &str) -> OtelSavePayload {
        OtelSavePayload {
            values: Default::default(),
            identity: None,
            ingest_key: IngestKeyChange::Set {
                key: crate::otel_ingest_key::SecretString::new(key),
            },
        }
    }

    fn keep_key_payload(values: &[(&str, &str)]) -> OtelSavePayload {
        sample_payload(values)
    }

    fn clear_key_payload() -> OtelSavePayload {
        OtelSavePayload {
            values: Default::default(),
            identity: None,
            ingest_key: IngestKeyChange::Clear,
        }
    }

    fn read_saved(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn headers_env_key_is_in_allowlist() {
        assert!(MANAGED_OTEL_KEYS.contains(&"OTEL_EXPORTER_OTLP_HEADERS"));
        assert_eq!(MANAGED_OTEL_KEYS.len(), 15);
    }

    // 저장 결과 파일 값은 Claude Code가 읽는 형식 `Authorization=Bearer <key>`.
    #[test]
    fn set_key_writes_authorization_bearer_format() {
        let path = temp_settings_path("ingest-set");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let saved = read_saved(&path);
        assert_eq!(
            saved["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {}", fake_ingest_key())
        );
        cleanup(&path);
    }

    // 형식 검증 경계 — 저장 단계에서도 46/48자·금지 문자·접두사 불일치는 거부하고 파일을 쓰지 않는다.
    #[test]
    fn set_key_rejects_invalid_formats_without_writing() {
        let k = fake_ingest_key();
        let mut with_plus = k.clone();
        with_plus.replace_range(10..11, "+");
        let cases = [
            k[..46].to_string(),
            format!("{k}A"),
            with_plus,
            format!("mhx_{}", "A".repeat(43)),
            String::new(),
        ];
        for bad in cases {
            let path = temp_settings_path("ingest-invalid");
            let err = save_to_settings_file(&path, set_key_payload(&bad)).unwrap_err();
            assert!(!path.exists(), "검증 실패 시 파일을 쓰면 안 된다");
            assert!(!err.contains("AAAA"), "에러에 입력 원문이 실리면 안 된다: {err}");
            cleanup(&path);
        }
    }

    // 요구 3(c): 어떤 경로의 Err에도 키 원문이 없다 — values 경유 시도·파싱 불가 파일 포함.
    #[test]
    fn errors_never_contain_raw_key() {
        let key = fake_ingest_key();
        // (1) values로 수집키를 직접 보내면 거부 — 에러에 값이 없어야 한다.
        let path = temp_settings_path("ingest-values-route");
        let err = save_to_settings_file(
            &path,
            sample_payload(&[(
                "OTEL_EXPORTER_OTLP_HEADERS",
                &format!("Authorization=Bearer {key}"),
            )]),
        )
        .unwrap_err();
        assert!(!err.contains(&key) && !err.contains("AAAA"), "{err}");
        assert!(!path.exists());
        cleanup(&path);

        // (2) 파싱 불가 파일 + 새 키 저장 — 파일 불변, 에러에 새 키·기존 키 모두 없음.
        let path = temp_settings_path("ingest-unparseable");
        let broken = format!(
            r#"{{"env":{{"OTEL_EXPORTER_OTLP_HEADERS":"Authorization=Bearer {key}"}}, oops"#
        );
        std::fs::write(&path, &broken).unwrap();
        let new_key = format!("mhk_{}", "B".repeat(43));
        let err = save_to_settings_file(&path, set_key_payload(&new_key)).unwrap_err();
        assert!(!err.contains("AAAA") && !err.contains("BBBB"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        // (3) 읽기 쪽 parseError에도 없다.
        let status = build_status(&path, path.to_string_lossy().to_string(), true);
        let parse_error = status.parse_error.expect("파싱 실패여야 한다");
        assert!(!parse_error.contains("AAAA"), "{parse_error}");
        cleanup(&path);
    }

    // 요구 3(a): 응답(직렬화)에 원문 없음, `values`에도 헤더 키가 없다. 마스킹 + 상태만.
    #[test]
    fn response_has_masked_status_and_no_raw_key() {
        let path = temp_settings_path("ingest-response");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let status = build_status(&path, path.to_string_lossy().to_string(), true);

        assert!(!status.values.contains_key("OTEL_EXPORTER_OTLP_HEADERS"));
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains(&fake_ingest_key()), "원문 노출: {json}");
        assert!(!json.contains("AAAAA"), "마스킹 이상 노출: {json}");
        assert!(json.contains(r#""masked":"mhk_AAAA…""#), "{json}");
        assert!(json.contains(r#""state":"set""#), "{json}");
        // (d) Debug 출력.
        let dbg = format!("{status:?}");
        assert!(!dbg.contains("AAAAA"), "{dbg}");
        cleanup(&path);
    }

    // 요구 3(d): 요청 페이로드 Debug에 원문이 없다.
    #[test]
    fn payload_debug_does_not_leak_key() {
        let payload = set_key_payload(&fake_ingest_key());
        let dbg = format!("{payload:?}");
        assert!(!dbg.contains("AAAA"), "{dbg}");
        assert!(dbg.contains("REDACTED"));
        // JSON으로 들어온 실제 요청 형태.
        let json = format!(
            r#"{{"values":{{}},"identity":null,"ingestKey":{{"action":"set","key":"{}"}}}}"#,
            fake_ingest_key()
        );
        let parsed: OtelSavePayload = serde_json::from_str(&json).unwrap();
        assert!(!format!("{parsed:?}").contains("AAAA"));
    }

    #[test]
    fn unset_when_no_headers() {
        let path = temp_settings_path("ingest-unset");
        save_to_settings_file(&path, keep_key_payload(&[("CLAUDE_CODE_ENABLE_TELEMETRY", "0")]))
            .unwrap();
        let status = build_status(&path, String::new(), true);
        let json = serde_json::to_value(&status.ingest_key).unwrap();
        assert_eq!(json["state"], "unset");
        assert!(json["masked"].is_null());
        cleanup(&path);
    }

    // 요구 4: 새 키 없이 한 저장(ingestKey 생략 = Keep)은 기존 키를 그대로 둔다.
    // 마스킹 문자열이 values로 돌아오는 경우는 거부돼 덮어쓰기가 불가능하다.
    #[test]
    fn save_without_new_key_keeps_existing_key() {
        let path = temp_settings_path("ingest-keep");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();

        // 프론트가 ingestKey 필드를 아예 생략한 JSON — serde default로 Keep.
        let payload: OtelSavePayload = serde_json::from_str(
            r#"{"values":{"OTEL_METRIC_EXPORT_INTERVAL":"30000"},"identity":null}"#,
        )
        .unwrap();
        save_to_settings_file(&path, payload).unwrap();
        let saved = read_saved(&path);
        assert_eq!(
            saved["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {}", fake_ingest_key())
        );
        assert_eq!(saved["env"]["OTEL_METRIC_EXPORT_INTERVAL"], "30000");

        // 명시적 keep도 동일.
        let payload: OtelSavePayload =
            serde_json::from_str(r#"{"values":{},"ingestKey":{"action":"keep"}}"#).unwrap();
        save_to_settings_file(&path, payload).unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {}", fake_ingest_key())
        );

        // 마스킹 표시값(`mhk_AAAA…`)을 values로 되돌려 보내도, Set으로 보내도 거부된다.
        assert!(save_to_settings_file(
            &path,
            sample_payload(&[("OTEL_EXPORTER_OTLP_HEADERS", "mhk_AAAA…")])
        )
        .is_err());
        assert!(save_to_settings_file(&path, set_key_payload("mhk_AAAA…")).is_err());
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {}", fake_ingest_key())
        );
        cleanup(&path);
    }

    // 요구 5: Authorization 외 `k=v`는 보존, 기존 키 교체 시에도 유지.
    #[test]
    fn set_key_preserves_other_headers() {
        let path = temp_settings_path("ingest-other-headers");
        let original = serde_json::json!({
            "env": { "OTEL_EXPORTER_OTLP_HEADERS": "x-team=platform,Authorization=Bearer old,x-env=prod" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("x-team=platform,Authorization=Bearer {},x-env=prod", fake_ingest_key())
        );

        // 삭제는 Authorization만 지우고 나머지는 남긴다.
        save_to_settings_file(&path, clear_key_payload()).unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            "x-team=platform,x-env=prod"
        );
        cleanup(&path);
    }

    #[test]
    fn clear_key_removes_env_entry_when_nothing_else_remains() {
        let path = temp_settings_path("ingest-clear");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        save_to_settings_file(&path, clear_key_payload()).unwrap();
        let saved = read_saved(&path);
        assert!(saved["env"].get("OTEL_EXPORTER_OTLP_HEADERS").is_none());
        let status = build_status(&path, String::new(), true);
        assert_eq!(serde_json::to_value(&status.ingest_key).unwrap()["state"], "unset");
        cleanup(&path);
    }

    // 요구 5(후단): 기존 Authorization이 mhk_ 형식이 아니면 상태만 "unrecognized" —
    // 원문(다른 서비스 토큰일 수 있다)은 응답·Debug 어디에도 없다.
    #[test]
    fn foreign_authorization_is_reported_without_raw_value() {
        let path = temp_settings_path("ingest-foreign");
        let original = serde_json::json!({
            "env": { "OTEL_EXPORTER_OTLP_HEADERS": "Authorization=Bearer FOREIGNSECRETTOKEN,x-a=1" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        let status = build_status(&path, path.to_string_lossy().to_string(), true);
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains(r#""state":"unrecognized""#), "{json}");
        assert!(!json.contains("FOREIGNSECRET"), "{json}");
        assert!(!format!("{status:?}").contains("FOREIGNSECRET"));
        assert!(!status.values.contains_key("OTEL_EXPORTER_OTLP_HEADERS"));

        // 다른 설정만 저장해도 외부 값은 건드리지 않고, 새 키 저장 시에만 교체된다.
        save_to_settings_file(&path, keep_key_payload(&[("OTEL_METRIC_EXPORT_INTERVAL", "1000")]))
            .unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            "Authorization=Bearer FOREIGNSECRETTOKEN,x-a=1"
        );
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {},x-a=1", fake_ingest_key())
        );
        cleanup(&path);
    }

    // 키 저장 시에도 settings.json의 무관한 최상위 설정(hooks/permissions)은 보존된다.
    #[test]
    fn set_key_preserves_unrelated_settings() {
        let path = temp_settings_path("ingest-preserve");
        let original = serde_json::json!({
            "permissions": { "allow": ["Bash(git *)"] },
            "hooks": { "PreToolUse": [] },
            "env": { "SOME_OTHER_VAR": "keep-me" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let saved = read_saved(&path);
        assert_eq!(saved["permissions"], original["permissions"]);
        assert_eq!(saved["hooks"], original["hooks"]);
        assert_eq!(saved["env"]["SOME_OTHER_VAR"], "keep-me");
        cleanup(&path);
    }

    // 요구 6: 키가 담긴 settings.json과 백업이 0600. 기존 파일이 0644였어도 교체 후 0600.
    #[cfg(unix)]
    #[test]
    fn settings_and_backup_are_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        let path = temp_settings_path("ingest-perms");
        // 기존 파일을 일부러 넓은 권한(0644)으로 만들어 둔다.
        std::fs::write(&path, r#"{"env":{}}"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        assert_eq!(mode_of(&path), 0o600, "settings.json");
        let backup = backup_path_for(&path);
        assert!(backup.is_file());
        assert_eq!(mode_of(&backup), 0o600, "백업");

        // 키가 이미 들어 있는 상태에서 키를 바꿔 저장 — 백업(이전 키 포함)도 0600 유지.
        // 백업이 이미 0644로 존재했던 경우도 좁혀진다.
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
        let rotated = format!("mhk_{}", "B".repeat(43));
        save_to_settings_file(&path, set_key_payload(&rotated)).unwrap();
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(mode_of(&backup), 0o600, "기존 백업 파일도 좁혀져야 한다");
        let backup_content = std::fs::read_to_string(&backup).unwrap();
        assert!(backup_content.contains(&fake_ingest_key()), "교체 저장의 백업은 직전 원본");
        cleanup(&path);
    }

    // RV-004: 수집키 삭제는 백업(.malgn-bak)에도 직전 키를 남기지 않는다. 앞선 저장이 남긴
    // 백업(키 포함 가능)도 이번 백업으로 덮이고, Authorization 외 헤더·설정은 그대로다.
    #[cfg(unix)]
    #[test]
    fn clear_key_leaves_no_key_in_backup() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_settings_path("ingest-clear-backup");
        let original = serde_json::json!({
            "permissions": { "allow": ["Bash(ls)"] },
            "env": { "OTEL_EXPORTER_OTLP_HEADERS": "x-team=platform" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let backup = backup_path_for(&path);

        // 키가 든 본 파일을 지운다. 이 시점 백업(set 직전 원본)에는 키가 없지만, 한 번 더 키를
        // 바꿔 백업에 직전 키가 있는 상태를 만든 뒤 삭제해 덮이는지 본다.
        let rotated = format!("mhk_{}", "B".repeat(43));
        save_to_settings_file(&path, set_key_payload(&rotated)).unwrap();
        assert!(std::fs::read_to_string(&backup).unwrap().contains(&fake_ingest_key()));

        save_to_settings_file(&path, clear_key_payload()).unwrap();
        let backup_content = std::fs::read_to_string(&backup).unwrap();
        assert!(!backup_content.contains(&fake_ingest_key()), "이전 백업의 키가 남음");
        assert!(!backup_content.contains(&rotated), "직전 키가 백업에 남음");
        assert!(!backup_content.contains("mhk_"), "백업에 mhk_ 키가 남음");
        let backup_json: Value = serde_json::from_str(&backup_content).unwrap();
        assert_eq!(backup_json["env"]["OTEL_EXPORTER_OTLP_HEADERS"], "x-team=platform");
        assert_eq!(backup_json["permissions"], original["permissions"]);
        let mode = std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // 본 파일도 동일하게 키가 없다.
        assert!(!std::fs::read_to_string(&path).unwrap().contains("mhk_"));
        cleanup(&path);
    }

    // RV-004: 헤더가 키뿐이던 파일을 지우면 백업에서도 env 항목 자체가 사라진다.
    #[test]
    fn clear_key_backup_drops_headers_entry_when_only_key() {
        let path = temp_settings_path("ingest-clear-backup-only");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        save_to_settings_file(&path, clear_key_payload()).unwrap();
        let backup_json: Value =
            serde_json::from_str(&std::fs::read_to_string(backup_path_for(&path)).unwrap()).unwrap();
        assert!(backup_json["env"].get("OTEL_EXPORTER_OTLP_HEADERS").is_none());
        cleanup(&path);
    }

    // RV-004 대조군: Clear가 아닌 저장(keep)의 백업은 여전히 "저장 직전 원본"이다.
    #[test]
    fn keep_save_backup_is_verbatim_original() {
        let path = temp_settings_path("ingest-keep-backup");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        save_to_settings_file(&path, keep_key_payload(&[("OTEL_METRICS_EXPORTER", "none")])).unwrap();
        assert_eq!(std::fs::read_to_string(backup_path_for(&path)).unwrap(), before);
        cleanup(&path);
    }

    // RV-002: 유효 엔드포인트(파일 값 → 기본값) 성격이 상태에 실린다.
    #[test]
    fn status_reports_endpoint_assessment() {
        // 파일 없음 → 기본값(허브) 기준.
        let path = temp_settings_path("ingest-endpoint-default");
        let st = build_status(&path, String::new(), false);
        assert!(st.ingest_endpoint.targets_hub);
        assert!(!st.ingest_endpoint.insecure && !st.ingest_endpoint.foreign_host);
        cleanup(&path);

        // 사내 http collector로 바꾸면 허브 경고 근거가 꺼지고 평문·허브 밖이 켜진다.
        let path = temp_settings_path("ingest-endpoint-http");
        let original = serde_json::json!({
            "env": {
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": "http://collector.corp.example:4318/v1/metrics",
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": "http://collector.corp.example:4318/v1/logs"
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();
        let st = build_status(&path, String::new(), true);
        assert!(!st.ingest_endpoint.targets_hub);
        assert!(st.ingest_endpoint.insecure && st.ingest_endpoint.foreign_host);
        let json = serde_json::to_string(&st).unwrap();
        assert!(json.contains(r#""ingestEndpoint":{"targetsHub":false,"insecure":true,"foreignHost":true}"#), "{json}");
        cleanup(&path);
    }

    // 임시 파일이 남지 않는다(자격증명이 든 .malgn-tmp-* 잔존 방지).
    #[test]
    fn no_temp_file_left_behind() {
        let path = temp_settings_path("ingest-tmp");
        save_to_settings_file(&path, set_key_payload(&fake_ingest_key())).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".malgn-tmp-"))
            .collect();
        assert!(leftovers.is_empty());
        cleanup(&path);
    }

    // 붙여넣기 형태: Authorization= 전체를 붙여도 파일에는 정규형 하나만 기록.
    #[test]
    fn pasted_full_header_is_normalized_in_file() {
        let path = temp_settings_path("ingest-paste");
        save_to_settings_file(
            &path,
            set_key_payload(&format!("  Authorization=Bearer {}\n", fake_ingest_key())),
        )
        .unwrap();
        assert_eq!(
            read_saved(&path)["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            format!("Authorization=Bearer {}", fake_ingest_key())
        );
        cleanup(&path);
    }
}
