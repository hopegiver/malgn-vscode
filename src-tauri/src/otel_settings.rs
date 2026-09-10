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

pub(crate) const MANAGED_OTEL_KEYS: [&str; 14] = [
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
];

/// 프라이버시 4개 — UI에서 값만 보여주고 편집 불가로 렌더한다(설계 §6). 이
/// 값을 1로 올리면 프롬프트 원문·툴 입출력이 collector로 나간다.
const READ_ONLY_KEYS: [&str; 4] = [
    "OTEL_LOG_USER_PROMPTS",
    "OTEL_LOG_TOOL_CONTENT",
    "OTEL_LOG_TOOL_DETAILS",
    "OTEL_LOG_RAW_API_BODIES",
];

/// (b) `option_env!()`로 주입하는 사내 collector 주소 — 소스에 리터럴로 넣지
/// 않는다(이 저장소는 public). `build.rs`가 `.env`(로컬)나 CI repo secret에서
/// 읽어 컴파일타임 상수로 굳힌다. 값이 없으면(포크·secret 없는 CI 빌드)
/// `endpoint_defaults_injected: false`가 그 신호가 되고, UI가 빈 값 +
/// placeholder로 렌더한다.
const OTEL_COLLECTOR_BASE: Option<&str> = option_env!("MALGN_OTEL_COLLECTOR_BASE");

fn endpoint_defaults() -> Option<(String, String)> {
    let base = OTEL_COLLECTOR_BASE?.trim_end_matches('/');
    Some((format!("{base}/v1/metrics"), format!("{base}/v1/logs")))
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
    let injected = if let Some((metrics_ep, logs_ep)) = endpoint_defaults() {
        map.insert(
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT".to_string(),
            metrics_ep,
        );
        map.insert("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT".to_string(), logs_ep);
        true
    } else {
        false
    };
    (map, injected)
}

fn settings_path() -> Option<PathBuf> {
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
}

fn read_managed_values(path: &Path) -> (std::collections::BTreeMap<String, String>, Option<String>) {
    let mut values = std::collections::BTreeMap::new();
    let mut parse_error = None;
    if let Ok(content) = std::fs::read_to_string(path) {
        match serde_json::from_str::<Value>(&content) {
            Ok(root) => {
                if let Some(env) = root.get("env").and_then(|v| v.as_object()) {
                    for key in MANAGED_OTEL_KEYS {
                        if let Some(v) = env.get(key).and_then(|v| v.as_str()) {
                            values.insert(key.to_string(), v.to_string());
                        }
                    }
                }
            }
            Err(e) => parse_error = Some(format!("설정 파일을 해석하지 못했습니다: {e}")),
        }
    }
    (values, parse_error)
}

fn build_status(path: &Path, path_display: String, file_exists: bool) -> OtelSettings {
    let (defaults, injected) = build_defaults();
    let (values, parse_error) = read_managed_values(path);
    OtelSettings {
        settings_path: path_display,
        file_exists,
        parse_error,
        managed_keys: MANAGED_OTEL_KEYS.iter().map(|s| s.to_string()).collect(),
        read_only_keys: READ_ONLY_KEYS.iter().map(|s| s.to_string()).collect(),
        values,
        defaults,
        endpoint_defaults_injected: injected,
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

#[derive(Deserialize, Clone, Debug)]
pub struct OtelSavePayload {
    pub values: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub identity: Option<OtelIdentity>,
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
/// 있는 `(k, v)` 목록으로 만들고 `employee.id/name/email` 3개만 upsert한 뒤
/// 나머지(예: `team=…`)는 원래 순서대로 남긴다(설계 §8, 통째로 덮어쓰지 않음).
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
    upsert("employee.email", percent_encode_value(email));
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

fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    path.with_file_name(format!("{file_name}.malgn-bak"))
}

/// 원자적 쓰기 — 같은 디렉터리에 임시 파일을 쓰고 `fs::rename`한다.
fn write_atomically(path: &Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "설정 파일의 상위 디렉터리를 확인할 수 없습니다.".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    let tmp_name = format!(
        ".{}.malgn-tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("settings.json"),
        std::process::id()
    );
    let tmp_path = dir.join(tmp_name);
    std::fs::write(&tmp_path, content).map_err(|e| format!("임시 파일을 쓰지 못했습니다: {e}"))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("설정 파일 교체에 실패했습니다: {e}"))?;
    Ok(())
}

/// 저장 알고리즘(순서 고정, 설계 §7):
/// 1. 파싱 실패 시 즉시 `Err` — 파싱 못 한 파일을 절대 덮어쓰지 않는다.
/// 2. allowlist 밖 키·readOnly 키는 `Err`(조용히 무시하지 않는다).
/// 3. 키별 형식 검증.
/// 4. `identity`가 있으면 `OTEL_RESOURCE_ATTRIBUTES`를 조립해 주입한다
///    (프론트가 이 키를 직접 보내면 `Err`).
/// 5. `env` 객체에 upsert(빈 문자열=삭제 신호). allowlist 밖 키·최상위 키는
///    전부 보존한다(이 함수는 "env"라는 이름의 하위 객체 하나만 건드리고,
///    그 안에서도 allowlist 키만 만진다 — 그 외 모든 값은 손대지 않는다).
/// 6. 쓰기 전 원본을 `settings.json.malgn-bak`으로 백업(롤링 1세대).
/// 7. 원자적 쓰기.
fn save_to_settings_file(path: &Path, payload: OtelSavePayload) -> Result<(), String> {
    let existing_content = std::fs::read_to_string(path).ok();
    let mut root: Value = match &existing_content {
        Some(content) => serde_json::from_str(content)
            .map_err(|e| format!("기존 설정 파일을 해석하지 못해 저장을 중단합니다: {e}"))?,
        None => Value::Object(Map::new()),
    };
    if !root.is_object() {
        return Err("설정 파일의 최상위 값이 객체가 아닙니다.".to_string());
    }

    for key in payload.values.keys() {
        if key == "OTEL_RESOURCE_ATTRIBUTES" {
            return Err("OTEL_RESOURCE_ATTRIBUTES는 identity를 통해서만 설정할 수 있습니다.".to_string());
        }
        if !MANAGED_OTEL_KEYS.contains(&key.as_str()) {
            return Err(format!("'{key}'는 관리 대상 키가 아닙니다."));
        }
        if READ_ONLY_KEYS.contains(&key.as_str()) {
            return Err(format!("'{key}'는 읽기 전용 키라 저장할 수 없습니다."));
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
        std::fs::write(&backup_path, content)
            .map_err(|e| format!("백업 파일을 쓰지 못했습니다: {e}"))?;
    }

    let pretty = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("설정을 직렬화하지 못했습니다: {e}"))?;
    write_atomically(path, &pretty)
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
        }
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
}
