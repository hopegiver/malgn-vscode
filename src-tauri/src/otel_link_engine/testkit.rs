// 엔진·연동 상태 테스트가 같이 쓰는 가짜 의존성과 픽스처. 실제 허브·키체인·`~/.claude`는 건드리지 않는다
// (설정 파일은 임시 디렉터리뿐, 서버는 가짜 객체 또는 127.0.0.1 목 서버).

use super::{DeviceTokens, Env, ProbeResult, SelfProbe};
use crate::hub_auth::store::testing::test_token;
use crate::otel_hub_client::{
    ApiError, BlockedReason, CredentialMeta, CredentialState, Created, HttpError, IngestApi,
    IssuePolicy,
};
use crate::otel_ingest_key::SecretString;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// 테스트 전용 가짜 수집키(`mhk_` + 43자). 실제 키가 아니다.
pub(crate) fn fake_key(fill: char) -> String {
    format!("mhk_{}", fill.to_string().repeat(43))
}

/// `fake_key`의 앞 8자(= 허브 `token_prefix`).
pub(crate) fn prefix_of(fill: char) -> String {
    format!("mhk_{}", fill.to_string().repeat(4))
}

/// 테스트 전용 가짜 기기 토큰(`mhd_` + 43자).
pub(crate) fn device_token(fill: char) -> String {
    test_token(fill)
}

pub(crate) fn temp_settings_path(name: &str) -> PathBuf {
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "malgn-vscode-otel-engine-test-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
    dir.join("settings.json")
}

pub(crate) fn cleanup(path: &Path) {
    let dir = path.parent().unwrap();
    // 읽기 전용으로 바꿔 둔 디렉터리도 지울 수 있게 되돌린다.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_dir_all(dir);
}

pub(crate) fn hub_endpoint(path: &str) -> String {
    format!("{}{path}", crate::hub_auth::consts::hub_origin())
}

/// 수집이 켜져 있는(준비 완료) settings.json 내용. `key`가 있으면 그 수집키를 적용한 상태.
/// 프라이버시·다른 설정이 보존되는지 보도록 무관한 항목도 함께 둔다.
pub(crate) fn ready_settings(key: Option<&str>) -> String {
    let mut env = serde_json::json!({
        "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
        "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
        "OTEL_METRICS_EXPORTER": "otlp",
        "OTEL_LOGS_EXPORTER": "otlp",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": hub_endpoint("/v1/metrics"),
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT": hub_endpoint("/v1/logs"),
        "OTEL_LOGS_EXPORT_INTERVAL": "5000",
        "OTEL_RESOURCE_ATTRIBUTES": "employee.id=dev,team=platform",
        "SOME_OTHER_VAR": "keep-me",
    });
    if let Some(k) = key {
        env["OTEL_EXPORTER_OTLP_HEADERS"] = format!("x-team=a,Authorization=Bearer {k}").into();
    }
    serde_json::json!({ "permissions": { "allow": ["Bash(ls)"] }, "env": env }).to_string()
}

pub(crate) fn write_ready(path: &Path, key: Option<&str>) -> String {
    let content = ready_settings(key);
    std::fs::write(path, &content).unwrap();
    content
}

// ---------------- 가짜 기기 토큰 ----------------

/// `latest()`가 큐의 값을 먼저 돌려주고, 비면 기본값을 돌려준다. 읽은 횟수를 센다(D-6 재읽기 시험).
pub(crate) struct FakeTokens {
    queue: Mutex<VecDeque<Option<String>>>,
    default: Mutex<Option<String>>,
    pub reads: AtomicUsize,
}

impl FakeTokens {
    pub(crate) fn new(token: &str) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            default: Mutex::new(Some(token.to_string())),
            reads: AtomicUsize::new(0),
        }
    }
    pub(crate) fn none() -> Self {
        let t = Self::new("");
        *t.default.lock().unwrap() = None;
        t
    }
    pub(crate) fn then(self, token: &str) -> Self {
        self.queue.lock().unwrap().push_back(Some(token.to_string()));
        self
    }
}

impl DeviceTokens for FakeTokens {
    fn latest(&self) -> Option<SecretString> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let next = self.queue.lock().unwrap().pop_front();
        let v = match next {
            Some(v) => v,
            None => self.default.lock().unwrap().clone(),
        };
        v.map(|s| SecretString::new(&s))
    }
}

// ---------------- 가짜 허브 API ----------------

pub(crate) type GetReply = Result<CredentialState, ApiError>;
pub(crate) type CreateReply = Result<Created, ApiError>;

/// 호출마다 큐에서 스크립트된 응답을 꺼낸다. 큐가 비었는데 불리면 시험이 예상하지 못한 호출이다 — 패닉.
#[derive(Default)]
pub(crate) struct FakeApi {
    pub get: Mutex<VecDeque<GetReply>>,
    pub create: Mutex<VecDeque<CreateReply>>,
    pub rotate: Mutex<VecDeque<CreateReply>>,
    /// "get:<토큰>", "create:<label>|<confirm>:<토큰>", "rotate:<토큰>" 순서 기록.
    pub calls: Mutex<Vec<String>>,
}

impl FakeApi {
    pub(crate) fn push_get(&self, r: GetReply) -> &Self {
        self.get.lock().unwrap().push_back(r);
        self
    }
    pub(crate) fn push_create(&self, r: CreateReply) -> &Self {
        self.create.lock().unwrap().push_back(r);
        self
    }
    pub(crate) fn push_rotate(&self, r: CreateReply) -> &Self {
        self.rotate.lock().unwrap().push_back(r);
        self
    }
    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    /// 토큰을 가린 호출 순서("get", "create:false", "rotate" …).
    pub(crate) fn kinds(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .map(|c| {
                let mut parts = c.splitn(3, ':');
                let kind = parts.next().unwrap_or("").to_string();
                match kind.as_str() {
                    "create" => format!("create:{}", parts.next().unwrap_or("")),
                    _ => kind,
                }
            })
            .collect()
    }
    pub(crate) fn count(&self, kind: &str) -> usize {
        self.kinds().iter().filter(|k| k.as_str() == kind || k.starts_with(&format!("{kind}:"))).count()
    }
}

impl IngestApi for FakeApi {
    fn get(&self, device_token: &str) -> GetReply {
        self.calls.lock().unwrap().push(format!("get:{device_token}"));
        self.get.lock().unwrap().pop_front().expect("예상하지 못한 GET 호출")
    }
    fn create(&self, device_token: &str, label: &str, confirm: bool) -> CreateReply {
        self.calls
            .lock()
            .unwrap()
            .push(format!("create:{confirm}:{label}:{device_token}"));
        self.create.lock().unwrap().pop_front().expect("예상하지 못한 POST(발급) 호출")
    }
    fn rotate(&self, device_token: &str) -> CreateReply {
        self.calls.lock().unwrap().push(format!("rotate:{device_token}"));
        self.rotate.lock().unwrap().pop_front().expect("예상하지 못한 rotate 호출")
    }
}

pub(crate) struct FakeProbe {
    pub results: Mutex<VecDeque<ProbeResult>>,
    pub calls: Mutex<Vec<String>>,
}

impl FakeProbe {
    pub(crate) fn new(results: &[ProbeResult]) -> Self {
        Self {
            results: Mutex::new(results.iter().copied().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }
    pub(crate) fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl SelfProbe for FakeProbe {
    fn probe(&self, device_token: &str) -> ProbeResult {
        self.calls.lock().unwrap().push(device_token.to_string());
        self.results.lock().unwrap().pop_front().expect("예상하지 못한 /self 확인 호출")
    }
}

// ---------------- 응답 빌더 ----------------

pub(crate) fn meta(prefix: &str, signal: bool) -> CredentialMeta {
    CredentialMeta {
        token_prefix: prefix.to_string(),
        last_signal_at_metrics: signal.then(|| "2026-10-15T01:00:00.000Z".to_string()),
        last_signal_at_logs: None,
    }
}

pub(crate) fn state_has_current(prefix: &str, signal: bool) -> GetReply {
    Ok(CredentialState {
        current: Some(meta(prefix, signal)),
        issue_policy: IssuePolicy::HasCurrent,
        blocked_reason: None,
    })
}

pub(crate) fn state_policy(policy: IssuePolicy, blocked: Option<BlockedReason>) -> GetReply {
    Ok(CredentialState {
        current: None,
        issue_policy: policy,
        blocked_reason: blocked,
    })
}

pub(crate) fn created(fill: char) -> CreateReply {
    Ok(Created {
        token: SecretString::new(&fake_key(fill)),
        env: Default::default(),
    })
}

/// 실제 응답 해석(`parse_created`)을 거친 201 — `env`는 서버가 보낸 JSON 그대로 걸러진다. 계약의 201 본문 모양을 쓴다.
#[allow(clippy::result_large_err)] // `ApiError`는 기존 시험 도우미(`created`)와 같은 큰 오류 타입을 쓴다.
pub(crate) fn created_with_env(fill: char, env: serde_json::Value) -> CreateReply {
    let body = serde_json::json!({
        "id": "01k7abc",
        "token_prefix": prefix_of(fill),
        "label": "x",
        "created_at": "2026-10-15T01:02:03.456Z",
        "token": fake_key(fill),
        "env": env,
        "notice": "n",
        "scope": {},
    });
    crate::otel_hub_client::parse_created(&body.to_string())
}

/// 계약 201 예시의 `env` 6개 키(허브가 지금 보내는 값).
pub(crate) fn contract_env() -> serde_json::Value {
    serde_json::json!({
        "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
        "OTEL_METRICS_EXPORTER": "otlp",
        "OTEL_LOGS_EXPORTER": "otlp",
        "OTEL_LOGS_EXPORT_INTERVAL": "60000",
        "OTEL_METRIC_EXPORT_INTERVAL": "60000",
        "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE": "cumulative"
    })
}

pub(crate) fn http(status: u16, code: &str) -> ApiError {
    ApiError::Http(HttpError {
        status,
        code: code.to_string(),
        blocked_reason: None,
        retry_after_secs: None,
        retry_at: None,
        current: None,
    })
}

pub(crate) fn http_with(status: u16, code: &str, f: impl FnOnce(&mut HttpError)) -> ApiError {
    let mut h = HttpError {
        status,
        code: code.to_string(),
        blocked_reason: None,
        retry_after_secs: None,
        retry_at: None,
        current: None,
    };
    f(&mut h);
    ApiError::Http(h)
}

/// 시험에서 쓰는 한 벌의 의존성.
pub(crate) struct Rig {
    pub path: PathBuf,
    pub tokens: FakeTokens,
    pub api: FakeApi,
    pub probe: FakeProbe,
    pub label: String,
}

impl Rig {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            path: temp_settings_path(name),
            tokens: FakeTokens::new(&device_token('D')),
            api: FakeApi::default(),
            probe: FakeProbe::new(&[]),
            label: "맑은에이전트 test-mac".to_string(),
        }
    }
    pub(crate) fn with_probe(mut self, results: &[ProbeResult]) -> Self {
        self.probe = FakeProbe::new(results);
        self
    }
    pub(crate) fn env(&self) -> Env<'_> {
        Env {
            path: &self.path,
            tokens: &self.tokens,
            api: &self.api,
            probe: &self.probe,
            label: &self.label,
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        cleanup(&self.path);
    }
}
