// OTLP 수집키 "연동 상태" 모델과 자동 발급 연결 — 상태 도출·발급 전 선검사·Tauri 커맨드·스케줄러.
//
// 구성
//   otel_link.rs          이 파일 — 연동 상태 모델(`IngestLinkStatus`), 파일 실물 기준 수집 준비도,
//                         발급 전 선검사(`mint_gate`), 실제 의존성 조립, Tauri 커맨드, 스케줄러
//   otel_link_engine.rs   허브 계약 D-1~D-15 (점검 분기·메모리 상태·백오프·폴링)
//   otel_hub_client.rs    허브 HTTP 클라이언트 (GET / POST / rotate)
//
// 상태 모델
//   미연동(unlinked) → 발급중(issuing) → 적용됨(applied) → 수신확인됨(confirmed), 실패(failed, 원인)
//   적용됨·미연동·실패는 `settings.json` 실물에서 도출한다(`derive_link_status`). 발급중·수신확인됨·
//   허브 쪽 실패 원인은 엔진의 메모리 상태(`RuntimeView`)를 그 위에 덮어 얻는다(`compose_status`).
//   엔진 상태는 영속하지 않는다 — 앱이 재기동되면 첫 점검이 같은 판단을 다시 만든다.
//
//   "적용됨"은 키가 파일에 있다는 뜻일 뿐 수집을 보장하지 않는다. 수집 준비도는 상태값이 아니라
//   별도 플래그(`collection_ready`·`blockers`)다 — 상태값을 늘리지 않고 플래그로 둔 이유는
//   `IngestLinkStatus` 주석 참고.
//
// 수집 준비도·발급 전 선검사의 기준은 **settings.json 실물**이다 (앱 기본값을 섞지 않는다).
//   설정 화면의 폼은 파일에 값이 없으면 앱 기본값(허브 엔드포인트 등)을 미리 채워 보여 준다. 그러나
//   Claude Code는 그 기본값을 모른다 — 엔드포인트·프로토콜에는 기본값이 없고, 신호별 엔드포인트가 없으면
//   범용 `OTEL_EXPORTER_OTLP_ENDPOINT`를 쓴다. 그래서 여기서는 파일에 실제로 있는 값만 보고(텔레메트리
//   켜짐, metrics·logs exporter가 otlp, 프로토콜 http/protobuf, 신호별 유효 엔드포인트가 허브),
//   파일에 있는 모든 엔드포인트(범용 포함)가 허브 밖·평문이면 발급·회전 자체를 하지 않는다.
//
// 결정 기록
//   N-5 (발급 직후 "수집 불가" 경고가 기본 흐름이 되는 문제): **수집 준비가 안 된 파일에서는 발급하지
//     않는다**(`mint_gate` → `NotReady`) — 키만 있고 수집은 안 되는 상태를 만들지 않는다. 준비는 앱의 기존 첫 실행
//     자동 세팅(`ensureOtelAutoConfigured`, 로그인 계정 정보로 employee.* 귀속까지 함께 쓴다)이 맡고, 그 저장 직후
//     프론트가 `otel_link_recheck`로 점검을 다시 요청한다. Rust가 준비 값을 따로 쓰면 로그인 계정 정보가 없어 귀속
//     속성이 빠진 채 자동 세팅 조건("텔레메트리 키가 파일에 없음")을 막아 버린다.
//   R19 (201 `env` 적용 — 직전 결정 "적용하지 않는다"를 뒤집음): 5초 로그 주기는 같은 PC가 12배 더 자주 POST해 허브
//     레이트 리밋·일일 쿼터(기기 단위 KST 어제+오늘 새 행 5,000 초과 시 키 폐기)를 빨리 쓴다. 그래서 발급·회전 201의 `env` 중
//     **허용 목록 6개 키**(`otel_server_env::SERVER_ENV_KEYS`)가 값 형식까지 맞을 때만 수집키 교체와 **같은 한 번의 원자적
//     쓰기**로 settings.json에 쓴다. 서버 값이 앱 기본값·파일의 기존 값을 이긴다(단 파일의 exporter가 이미 `otlp`를 포함한
//     목록이면 그 키는 덮지 않는다 — `otlp,console`의 `console`을 지킨다). 목록 밖 키(엔드포인트·헤더·`OTEL_RESOURCE_ATTRIBUTES`·
//     프라이버시 4키·비OTEL)와 형식 위반은 쓰지 않고 이름만 `serverEnvRejected`로 남긴다(부분 적용, 수집키 적용은 실패시키지 않는다).
//     바뀐 키 이름은 `serverEnvChanged`로 화면에 알린다.
//     **반영 범위(주의)**: 서버 값은 **발급·회전 201을 받은 때에만** 파일에 닿는다. 이미 키가 적용된 기기는 점검이 "적용됨 → 끝"으로
//     끝나 201이 오지 않으므로(키 앞 8자가 같으면 회전하지 않는다) 허브가 값을 바꿔도 따라가지 못한다 — "서버가 바꾸면 앱 수정
//     없이 자동으로 따라간다"가 아니다. 그래서 **기존 기기의 옛 기본값 5000은 이 변경으로 남는다**: 앱은 그 5000을 조용히
//     고치지 않고(앱이 쓴 5000과 사용자가 정한 5000을 파일에서 가를 수 없다) 설정 화면이 권장값(60000)을 안내만 한다. 5000이
//     바뀌는 것은 그 기기가 새 키를 받을 때(재발급·회전)이고, 그때는 서버 값이 이긴다. 기존 기기까지 전파하려면 허브 상태 응답에
//     env를 싣는 별도 결정이 필요하다(이번 범위 밖).
//   R-4 (수집키 원문의 OS 키체인 사본): 채택하지 않았다. 계약은 1~15번만으로 충족된다. 채택하면 같은 비밀이
//     평문 settings.json 외에 한 곳 더 저장되고(키체인 항목·접근 프롬프트·해제 시 삭제 경로가 늘어난다),
//     그 이득(설정 파일 유실·재설치·쓰기 실패 뒤의 불필요한 회전 방지)은 이 앱의 사용자 규모(사내 50명 미만)와
//     쓰기 실패의 희소성에 비해 작다. 쓰기 실패는 메모리 보관(D-5)으로, 연속 유실은 상한(D-5)으로,
//     그 밖의 회전은 서버의 회전 예산(D-14)이 막는다. 회전 예산 429가 운영에서 관측되면 다시 검토한다.
//
// mhd_/mhk_ 분리 불변식(단위 테스트가 `*_never_contains_device_token`으로 고정):
//   - 기기 토큰(`mhd_`, MCP 전권)은 `DeviceTokens`·`IngestApi` 경계 안에서만 쓰인다. 이 모듈의 상태·에러·
//     Debug·로그에 실리지 않는다. 이 파일에는 로그 출력이 없다.
//   - 실패 원인은 데이터를 품지 않는 enum(`LinkFailureCause`)이고 사용자 문구는 그 원인의 고정 문구다.
//     허브 응답·입력 문자열·파일 값은 어떤 경로로도 상태 문구에 섞이지 않는다.
//   - 키가 `settings.json`에 들어가는 유일한 경로는 `otel_settings::apply_ingest_key`(→ 수동 입력과 같은
//     `save_to_settings_file` 엔진)이고, 형식 검증에서 `mhd_` 형태는 거부된다.
//   - 상태에는 `mhk_`+4자 마스킹만 싣는다(원문 없음).

use crate::hub_auth::api::HubApi;
use crate::hub_auth::HubCtx;
use crate::otel_hub_client::{BlockedReason, ReqwestIngestApi};
use crate::otel_ingest_key::{EndpointAssessment, IngestKeyState, SecretString};
use crate::otel_link_engine::{
    DeviceTokens, Env, HubSelfProbe, LinkEngine, Phase, RuntimeView, Trigger,
};
use crate::otel_settings::{CollectionFacts, IngestInputs};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// 연동 단계. 직렬화 값: `unlinked | issuing | applied | confirmed | failed`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IngestLinkState {
    /// `Authorization` 헤더 없음.
    Unlinked,
    /// 앱이 수집키를 발급받아 적용하는 중(엔진이 점검 중이고 설정에 키가 없다).
    Issuing,
    /// `settings.json`에 허브 형식(`mhk_…`) 키가 적용돼 있다(수신은 아직 확인 전).
    Applied,
    /// 허브가 이 기기의 첫 데이터 수신을 확인했다(메모리 상태 — 앱 재시작 후 첫 점검이 다시 확인한다).
    Confirmed,
    /// 실패 — 원인은 `failure`.
    Failed,
}

/// 실패 원인. 데이터를 품지 않는다 — 문구는 `message()`의 고정 문자열뿐이라 어떤 입력·응답도
/// 상태 문구로 새지 않는다. 직렬화 값은 camelCase 식별자(프론트 분기용).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LinkFailureCause {
    /// `settings.json`을 JSON으로 해석하지 못했다(파일 도출).
    SettingsUnreadable,
    /// 허브 형식이 아닌 `Authorization` 헤더가 이미 있다(파일 도출).
    HeaderUnrecognized,
    /// 수집 엔드포인트가 허브가 아니거나 평문 http라서 발급 전에 멈췄다 — 키가 허브 밖·평문으로
    /// 나가지 않게 한다.
    EndpointNotHub,
    /// 설정 파일에서 수집이 켜져 있지 않아(텔레메트리·exporter·프로토콜·엔드포인트) 발급 전에 멈췄다.
    NotReady,
    /// 서버에 이 기능이 아직 없다(연결은 정상, 계약 D-7a).
    FeatureUnavailable,
    /// 허브가 이 기기의 연결을 끊었다 — 재연결이 필요하다(D-7).
    ReconnectRequired,
    /// 데스크톱 앱이 발급한 토큰이 아니다(D-11, 재시도하지 않는다).
    NotDesktopToken,
    /// 허브가 요청을 거부했다.
    HubRejected,
    /// 허브에 닿지 못했다(네트워크·타임아웃·5xx) — 자동 재시도한다.
    Unreachable,
    /// 요청 빈도 초과 — `Retry-After` 뒤 자동으로 이어 한다.
    RateLimited,
    /// 허브가 돌려준 값이 수집키 형식이 아니다.
    InvalidKeyReturned,
    /// 검증된 키를 설정 파일에 적용하지 못했다(원문은 메모리에 보관 중, D-5).
    ApplyFailed,
    /// 이 연결로는 발급·회전이 불가능하다(`blocked_reason` 참고, D-1·D-10).
    Blocked,
    /// 사용자 유효 키 상한에 도달했다(D-8).
    CredentialLimit,
    /// 이 기기의 회전 예산이 소진됐다(D-14).
    RotateBudgetExceeded,
    /// 응답이 반복해서 끊겨 자동 회전을 멈췄다(D-5).
    RotationLost,
}

impl LinkFailureCause {
    /// 사용자의 조치 없이 스스로 풀리는 일시 원인(허브에 닿지 못함·요청 빈도 초과·서버에 기능 없음).
    /// 키가 이미 적용돼 있으면 이 원인이 적용·수신 확인 상태를 `failed`로 바꾸지 않는다(`IngestLinkStatus::notice`).
    pub(crate) fn is_transient(self) -> bool {
        matches!(self, Self::Unreachable | Self::RateLimited | Self::FeatureUnavailable)
    }

    /// 사용자 문구. 토큰 접두어 리터럴(`mhd_`/`mhk_`)도 넣지 않는다.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::SettingsUnreadable => {
                "Claude 설정 파일(settings.json)을 해석하지 못했습니다. 파일의 JSON 형식을 확인하세요."
            }
            Self::HeaderUnrecognized => {
                "설정 파일에 허브 수집키 형식이 아닌 Authorization 헤더가 있습니다. 허브로 보내려면 설정 > OTel에서 이 항목을 삭제하세요(허브에 연결되어 있으면 삭제 뒤 앱이 수집키를 발급해 적용합니다. 연결하지 않았다면 직접 받은 수집키로 교체할 수도 있습니다)."
            }
            Self::EndpointNotHub => {
                "수집 엔드포인트가 허브가 아니거나 암호화되지 않은(http) 주소입니다. 수집키가 그 주소로 전송되지 않도록 발급받지 않았습니다. 설정 > OTel에서 엔드포인트를 확인하세요."
            }
            Self::NotReady => {
                "설정 파일에서 수집이 켜져 있지 않아(텔레메트리·exporter·프로토콜·엔드포인트) 수집키를 발급받지 않았습니다. 설정 > OTel에서 저장한 뒤 다시 시도하세요."
            }
            Self::FeatureUnavailable => {
                "허브에 수집키 자동 발급 기능이 아직 없습니다. 허브 연결은 정상이며 1시간 뒤 다시 확인합니다."
            }
            Self::ReconnectRequired => {
                "이 PC의 맑은에이전트 연결이 해제되었습니다. 웹에서 유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기됩니다. 앱에서 다시 로그인해 연결하세요."
            }
            Self::NotDesktopToken => {
                "이 허브 연결은 데스크톱 앱에서 발급된 것이 아니라 수집키를 받을 수 없습니다. 허브 연결을 해제한 뒤 앱에서 다시 연결하세요."
            }
            Self::HubRejected => "허브가 요청을 거부했습니다.",
            Self::Unreachable => "허브에 연결하지 못했습니다. 네트워크 상태를 확인하세요. 잠시 후 다시 시도합니다.",
            Self::RateLimited => "허브 요청이 많아 잠시 후 이어서 시도합니다.",
            Self::InvalidKeyReturned => {
                "허브가 돌려준 값이 수집키 형식이 아니어서 적용하지 않았습니다."
            }
            Self::ApplyFailed => {
                "발급된 수집키를 설정 파일에 적용하지 못했습니다. 키는 앱이 메모리에 보관하고 있어 다시 시도하면 같은 키로 다시 씁니다. 앱을 종료하면 사라지고 다음 실행에서 새 키가 발급됩니다."
            }
            Self::Blocked => {
                "이 연결로는 수집키를 발급하거나 바꿀 수 없습니다. 앱의 허브 연결을 해제한 뒤 다시 로그인해 연결하세요."
            }
            Self::CredentialLimit => {
                "수집키 상한(5개)에 도달했습니다. 허브 웹 /keys에서 쓰지 않는 키를 폐기하세요. 이전 연결에서 만든 '맑은에이전트' 키가 남아 있을 수 있습니다."
            }
            Self::RotateBudgetExceeded => {
                "이 기기의 수집키가 24시간 안에 너무 자주 바뀌었습니다. 네트워크가 불안정해 응답이 반복해서 끊기거나, 설정 파일(~/.claude/settings.json)이 다른 PC와 동기화되는 경우에도 생깁니다. 둘 다 아니라면 이 연결(기기 토큰)이 다른 곳에서 쓰이고 있을 수 있으니 연결 해제 후 다시 연결하세요."
            }
            Self::RotationLost => {
                "허브 응답이 반복해서 끊겨 수집키 자동 교체를 멈췄습니다(네트워크 오류). 네트워크를 확인한 뒤 [다시 시도]하거나 앱을 다시 시작하면 재개합니다."
            }
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct IngestLinkFailure {
    pub cause: LinkFailureCause,
    /// `cause.message()` — 고정 문구.
    pub message: String,
}

impl IngestLinkFailure {
    fn of(cause: LinkFailureCause) -> Self {
        Self {
            cause,
            message: cause.message().to_string(),
        }
    }
}

/// 수집이 안 될 수 있는 원인(파일 실물 기준). 직렬화 값은 camelCase 식별자 — 문구는 프론트가 고른다.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReadinessBlocker {
    /// 설정 파일에 `CLAUDE_CODE_ENABLE_TELEMETRY=1`이 없다(셸 환경변수로 켠 경우는 보지 못한다).
    TelemetryOff,
    /// metrics·logs exporter 중 `otlp`가 아닌 것이 있다.
    ExporterNotOtlp,
    /// `OTEL_EXPORTER_OTLP_PROTOCOL`이 `http/protobuf`가 아니다.
    ProtocolMismatch,
    /// metrics 또는 logs의 유효 엔드포인트(신호별 값 → 범용 값)가 파일에 없다.
    EndpointMissing,
    /// 유효 엔드포인트가 허브가 아니다(허브 밖·해석 불가).
    EndpointNotHub,
    /// 유효 엔드포인트는 허브인데 파일에 허브 밖 엔드포인트가 함께 있다(예: 범용 값).
    EndpointForeign,
    /// 평문 http 엔드포인트가 있다(루프백 제외).
    EndpointInsecure,
}

pub(crate) fn readiness_blockers(f: &CollectionFacts) -> Vec<ReadinessBlocker> {
    let mut out = Vec::new();
    if !f.telemetry_enabled {
        out.push(ReadinessBlocker::TelemetryOff);
    }
    if !(f.metrics_exporter_otlp && f.logs_exporter_otlp) {
        out.push(ReadinessBlocker::ExporterNotOtlp);
    }
    if !f.protocol_http_protobuf {
        out.push(ReadinessBlocker::ProtocolMismatch);
    }
    if !(f.metrics_endpoint_present && f.logs_endpoint_present) {
        out.push(ReadinessBlocker::EndpointMissing);
    } else if !(f.metrics_endpoint_hub && f.logs_endpoint_hub) {
        out.push(ReadinessBlocker::EndpointNotHub);
    } else if f.any_foreign_host {
        out.push(ReadinessBlocker::EndpointForeign);
    }
    if f.any_insecure {
        out.push(ReadinessBlocker::EndpointInsecure);
    }
    out
}

/// 프론트로 나가는 연동 상태. 토큰·키 원문 필드가 없다(`masked`는 `mhk_`+4자뿐).
///
/// 수집 준비도(`telemetry_enabled`·`endpoint`·`collection_ready`·`blockers`)는 `state`와 직교하는 사실이라
/// 새 상태값(예: "적용됐지만 수집 안 됨")이 아니라 플래그로 둔다. 상태값으로 만들면 ① 미연동·
/// 실패·수신확인됨 각각에 변형이 생겨 유니온이 곱으로 늘고 ② "키 적용"이라는 `applied`의 의미
/// (파일 실물)가 흐려지며 ③ 프론트가 상태 분기를 전부 고쳐야 한다. 플래그는 기존 분기를 그대로 두고
/// `applied`/`confirmed` 행에서만 경고를 덧붙이게 한다. 값은 불리언/열거형뿐이다(URL·키 없음).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestLinkStatus {
    pub state: IngestLinkState,
    /// `mhk_AbCd…` 형태. 키가 적용돼 있는 상태(applied/confirmed)와, 키가 적용된 채 다음 단계가
    /// 실패한 경우(failed)에 있고, 그 외에는 `null`.
    pub masked: Option<String>,
    /// `state == failed`일 때만 있다.
    pub failure: Option<IngestLinkFailure>,
    /// 앱이 수집키를 자동 발급할 수 있는 환경인가(허브 연결 기능이 초기화됨).
    pub issue_supported: bool,
    /// 허브 수신 확인이 가능한가(발급과 같은 조건).
    pub receipt_check_supported: bool,
    /// `settings.json`의 `CLAUDE_CODE_ENABLE_TELEMETRY`가 `"1"`인가(파일에 없으면 false).
    pub telemetry_enabled: bool,
    /// 파일 실물 기준 엔드포인트 성격(URL은 싣지 않는다). `targets_hub`는 metrics·logs **둘 다** 허브일 때만
    /// true(설정 화면의 `ingestEndpoint`는 "하나라도"라 의미가 다르다). `insecure`·`foreign_host`는 파일의
    /// 모든 엔드포인트 값(범용 포함) 기준. 파일을 해석하지 못했으면 전부 false.
    pub endpoint: EndpointAssessment,
    /// `blockers`가 비어 있는가. 키가 적용된 상태(applied/confirmed)에서 false이면 "적용됐지만 수집되지
    /// 않을 수 있음"이다. 앱 밖(셸 환경변수 등)에서 켜진 텔레메트리는 보지 못한다.
    pub collection_ready: bool,
    pub blockers: Vec<ReadinessBlocker>,
    /// 허브 기기 토큰이 있다(연결됨).
    pub hub_connected: bool,
    /// 웹에서 폐기된 키라 사람의 확인을 기다린다 — 프론트가 확인창을 띄운다(D-1).
    pub confirm_required: bool,
    /// 사용자가 확인을 거절했다(이번 앱 실행 동안 다시 묻지 않는다).
    pub confirm_declined: bool,
    /// 허브가 이 기기의 파일 키를 웹에서 폐기된 것으로 판정했고 그 판정이 아직 이어지고 있다. `confirm_required`·
    /// `confirm_declined`가 켜져 있으면 항상 true이고, 그 밖에 폐기 판정을 받은 키가 파일에 그대로 남은 채 다음 점검이
    /// 일시 오류로 끝나 확인 여부를 아직 모를 때도 true다(RV-09). 허브가 다른 판정을 주거나 키·연결이 바뀌면 false.
    pub revoked_by_hub: bool,
    /// 적용된 키의 첫 데이터를 기다리는 중(D-12, 최대 15분).
    pub waiting_first_data: bool,
    /// 15분 안에 첫 데이터가 오지 않아 확인을 멈췄다 — "새 Claude Code 세션을 열면 수집이 시작됩니다".
    pub first_data_timed_out: bool,
    /// `failure.cause == blocked`일 때 차단 사유.
    pub blocked_reason: Option<BlockedReason>,
    /// `failure.cause == rotateBudgetExceeded`일 때 다시 회전할 수 있는 시각(RFC 3339, 서버가 준 값).
    pub retry_at: Option<String>,
    /// 키가 적용돼 있는 상태(applied/confirmed)에서 마지막 점검이 **일시적인 원인**(허브에 닿지 못함·요청 빈도 초과·
    /// 서버에 기능 없음)으로 끝났다 — 적용·수신 확인 사실은 그대로이고 상태를 `failed`로 덮지 않는다(RV-07).
    /// 화면은 상태 문구 뒤에 보조 안내로만 붙인다. 그 밖에는 `null`.
    pub notice: Option<IngestLinkFailure>,
    /// 마지막 발급·회전 201을 적용하며 서버 권장값으로 **바뀐** env 키 이름(이전 파일 값과 달랐던 것만). 이름만 있고 값은 없다.
    /// 앱을 다시 시작하면 비워진다(영속하지 않는다).
    pub server_env_changed: Vec<String>,
    /// 마지막 201의 `env` 중 허용 목록·값 형식 검사에서 거부되어 **쓰지 않은** 키 이름. 이름만 있고 값은 없다.
    pub server_env_rejected: Vec<String>,
}

impl IngestLinkStatus {
    fn new(state: IngestLinkState, masked: Option<String>) -> Self {
        Self {
            state,
            masked,
            failure: None,
            issue_supported: false,
            receipt_check_supported: false,
            telemetry_enabled: false,
            endpoint: EndpointAssessment::default(),
            collection_ready: false,
            blockers: Vec::new(),
            hub_connected: false,
            confirm_required: false,
            confirm_declined: false,
            revoked_by_hub: false,
            waiting_first_data: false,
            first_data_timed_out: false,
            blocked_reason: None,
            retry_at: None,
            notice: None,
            server_env_changed: Vec::new(),
            server_env_rejected: Vec::new(),
        }
    }

    fn failed(cause: LinkFailureCause, masked: Option<String>) -> Self {
        Self {
            failure: Some(IngestLinkFailure::of(cause)),
            ..Self::new(IngestLinkState::Failed, masked)
        }
    }

    /// 파일에서 읽은 수집 준비도를 싣는다(`collection_ready`·`blockers`는 여기서만 계산한다).
    fn with_readiness(mut self, facts: &CollectionFacts) -> Self {
        self.telemetry_enabled = facts.telemetry_enabled;
        self.endpoint = EndpointAssessment {
            targets_hub: facts.metrics_endpoint_hub && facts.logs_endpoint_hub,
            insecure: facts.any_insecure,
            foreign_host: facts.any_foreign_host,
        };
        self.blockers = readiness_blockers(facts);
        self.collection_ready = self.blockers.is_empty();
        self
    }

    /// 이 상태(키·수집 준비도는 그대로)를 실패로 바꾼다 — 단계 실패 시 파일 사실을 잃지 않는다.
    fn failed_from(self, cause: LinkFailureCause) -> Self {
        Self {
            state: IngestLinkState::Failed,
            failure: Some(IngestLinkFailure::of(cause)),
            ..self
        }
    }
}

/// `settings.json` 실물에서 연동 상태를 도출한다(순수 함수 — 미연동/적용됨/실패).
pub(crate) fn derive_link_status(inputs: &IngestInputs) -> IngestLinkStatus {
    let status = if inputs.unreadable {
        IngestLinkStatus::failed(LinkFailureCause::SettingsUnreadable, None)
    } else {
        match inputs.key.state {
            IngestKeyState::Unset => IngestLinkStatus::new(IngestLinkState::Unlinked, None),
            IngestKeyState::Set => {
                IngestLinkStatus::new(IngestLinkState::Applied, inputs.key.masked.clone())
            }
            IngestKeyState::Unrecognized => {
                IngestLinkStatus::failed(LinkFailureCause::HeaderUnrecognized, None)
            }
        }
    };
    status.with_readiness(&inputs.facts)
}

fn masked_prefix(masked: &Option<String>) -> Option<String> {
    masked
        .as_deref()
        .map(|m| m.trim_end_matches('…').to_string())
}

fn cause_of(phase: &Phase) -> Option<LinkFailureCause> {
    Some(match phase {
        Phase::Blocked(_) => LinkFailureCause::Blocked,
        Phase::CredentialLimit => LinkFailureCause::CredentialLimit,
        Phase::RotateBudget { .. } => LinkFailureCause::RotateBudgetExceeded,
        Phase::RotationLost => LinkFailureCause::RotationLost,
        Phase::Reconnect => LinkFailureCause::ReconnectRequired,
        Phase::FeatureUnavailable => LinkFailureCause::FeatureUnavailable,
        Phase::WriteFailed => LinkFailureCause::ApplyFailed,
        Phase::NotDesktopToken => LinkFailureCause::NotDesktopToken,
        Phase::HubRejected => LinkFailureCause::HubRejected,
        Phase::Transient => LinkFailureCause::Unreachable,
        Phase::RateLimited => LinkFailureCause::RateLimited,
        Phase::NotReady => LinkFailureCause::NotReady,
        Phase::EndpointNotHub => LinkFailureCause::EndpointNotHub,
        Phase::InvalidKey => LinkFailureCause::InvalidKeyReturned,
        Phase::Idle
        | Phase::Checking
        | Phase::NoDeviceToken
        | Phase::ConfirmRequired
        | Phase::ConfirmDeclined => return None,
    })
}

/// 파일 실물에서 도출한 상태 위에 엔진의 메모리 상태를 얹어 프론트 상태를 만든다(순수 함수).
/// 파일에서 도출한 실패(해석 불가·형식이 다른 Authorization)는 엔진 상태보다 우선한다 — 그 파일로는
/// 어떤 발급도 하지 않기 때문이다.
pub(crate) fn compose_status(
    inputs: &IngestInputs,
    view: &RuntimeView,
    hub_connected: bool,
    supported: bool,
) -> IngestLinkStatus {
    let mut s = derive_link_status(inputs);
    s.hub_connected = hub_connected;
    s.issue_supported = supported;
    s.receipt_check_supported = supported;
    s.server_env_changed = view.env_changed.clone();
    s.server_env_rejected = view.env_rejected.clone();
    if s.state == IngestLinkState::Failed {
        return s;
    }
    // 웹에서 폐기됐다는 허브의 판정(confirm_required)이 파일 사실보다 우선한다 — 파일에 키가 남아 있어도 그 키는
    // 수집되지 않는다. 같은 실행에서 이전에 확인한 수신(`confirmed_prefix`)·첫 데이터 대기 표시도 이 상태를
    // 가리지 않는다(RV-01). 상태값(`applied`/`unlinked`)은 파일 사실 그대로 두고 플래그로 알린다.
    // 폐기 판정을 받은 키가 파일에 그대로 남아 있으면(`revoked_prefix`) 다음 점검이 일시 오류로 끝나 phase가 바뀌었어도
    // 판정은 이어진다 — 키가 바뀌었거나(앞 8자 불일치) 허브가 다른 판정을 줄 때(엔진이 비운다)까지(RV-09).
    let file_prefix = masked_prefix(&s.masked);
    let revoked_kept = file_prefix.is_some() && file_prefix == view.revoked_prefix;
    let revoked_by_hub = revoked_kept || matches!(view.phase, Phase::ConfirmRequired | Phase::ConfirmDeclined);
    s.revoked_by_hub = revoked_by_hub;
    if s.state == IngestLinkState::Applied && !revoked_by_hub {
        if file_prefix.is_some() && file_prefix == view.confirmed_prefix {
            s.state = IngestLinkState::Confirmed;
        } else {
            s.waiting_first_data = view.waiting_first_data;
            s.first_data_timed_out = view.first_data_timed_out;
        }
    }
    match &view.phase {
        Phase::Checking if s.state == IngestLinkState::Unlinked => {
            s.state = IngestLinkState::Issuing;
        }
        Phase::ConfirmRequired => s.confirm_required = true,
        Phase::ConfirmDeclined => s.confirm_declined = true,
        other => {
            if let Some(cause) = cause_of(other) {
                let key_applied = matches!(s.state, IngestLinkState::Applied | IngestLinkState::Confirmed);
                if key_applied && cause.is_transient() {
                    // 일시 원인은 키가 적용된 사실·수신 확인을 지우지 않는다(RV-07).
                    s.notice = Some(IngestLinkFailure::of(cause));
                } else {
                    s = s.failed_from(cause);
                    match other {
                        Phase::Blocked(reason) => s.blocked_reason = *reason,
                        Phase::RotateBudget { retry_at } => s.retry_at = retry_at.clone(),
                        _ => {}
                    }
                }
            }
        }
    }
    s
}

// ---------------- 파일 실물 질의 (엔진이 쓴다) ----------------

/// 점검을 시작하기 전에 파일이 이미 연동을 막고 있는가 — 해석 불가·형식이 다른 Authorization.
/// 이 경우 서버를 부르지 않는다(적용할 수 없는 키를 발급받지 않고, 사용자가 둔 다른 서비스의 인증 헤더를
/// 덮어쓰지 않는다). 상태 표시는 파일에서 도출한 실패가 맡으므로 `Phase::Idle`을 돌려준다.
pub(crate) fn file_blocks_linking(path: &Path) -> Result<(), Phase> {
    let inputs = crate::otel_settings::read_ingest_inputs(path);
    if inputs.unreadable || inputs.key.state == IngestKeyState::Unrecognized {
        return Err(Phase::Idle);
    }
    Ok(())
}

/// 키를 새로 만들기(발급·회전) 전의 선검사. 서버를 부르기 전에 멈춘다.
///   1. 파일 해석 불가·형식이 다른 Authorization → 멈춤(`Phase::Idle`, 상태는 파일에서 도출)
///   2. 파일의 엔드포인트가 허브 밖이거나 평문 http → `EndpointNotHub` — `OTEL_EXPORTER_OTLP_HEADERS`는
///      설정된 모든 엔드포인트로 나가므로 이 자동 경로(경고 화면 없음)가 허브가 발급한 키를 허브 밖·평문으로
///      흘리지 않게 한다. 수동 입력은 화면 경고로 알리고 허용한다.
///   3. 수집 준비가 안 됨(`blockers` 비어 있지 않음) → `NotReady` — 키만 있고 수집은 안 되는 상태를 만들지 않는다.
pub(crate) fn mint_gate(path: &Path) -> Result<(), Phase> {
    let inputs = crate::otel_settings::read_ingest_inputs(path);
    if inputs.unreadable || inputs.key.state == IngestKeyState::Unrecognized {
        return Err(Phase::Idle);
    }
    if inputs.facts.any_foreign_host || inputs.facts.any_insecure {
        return Err(Phase::EndpointNotHub);
    }
    if !readiness_blockers(&inputs.facts).is_empty() {
        return Err(Phase::NotReady);
    }
    Ok(())
}

/// 설정 파일의 수집키 앞 8자(`mhk_`+4자). 키가 없거나 형식이 다르면 None.
pub(crate) fn file_key_prefix(path: &Path) -> Option<String> {
    let inputs = crate::otel_settings::read_ingest_inputs(path);
    masked_prefix(&inputs.key.masked)
}

// ---------------- label (계약 E) ----------------

const LABEL_PREFIX: &str = "맑은에이전트 ";
const LABEL_MAX_UNITS: usize = 60;
const LABEL_HOST_MAX_UNITS: usize = 52;

fn utf16_units(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 발급 요청의 label을 만든다(계약 E).
///   1. host = OS 호스트명을 trim하고 제어문자(U+0000–U+001F, U+007F)를 제거한 값
///   2. host가 비어 있으면 `"맑은에이전트"`
///   3. `"맑은에이전트 " + host`가 UTF-16 60 unit 이하이면 그대로
///   4. 넘으면 host를 문자 단위로 앞에서부터 담되(서로게이트 쌍은 쪼개지 않는다) 52 unit을 넘기 직전에
///      멈추고 `"…"`(U+2026)을 붙인다 — 결과는 최대 60 unit.
pub(crate) fn ingest_label(raw_host: &str) -> String {
    let host: String = raw_host
        .trim()
        .chars()
        .filter(|c| !matches!(*c, '\u{0000}'..='\u{001F}' | '\u{007F}'))
        .collect();
    if host.is_empty() {
        return LABEL_PREFIX.trim_end().to_string();
    }
    let full = format!("{LABEL_PREFIX}{host}");
    if utf16_units(&full) <= LABEL_MAX_UNITS {
        return full;
    }
    let mut kept = String::new();
    let mut units = 0usize;
    for c in host.chars() {
        let u = c.len_utf16();
        if units + u > LABEL_HOST_MAX_UNITS {
            break;
        }
        kept.push(c);
        units += u;
    }
    format!("{LABEL_PREFIX}{kept}…")
}

#[cfg(unix)]
fn os_hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: buf는 256바이트이고 gethostname은 그 길이를 넘기지 않는다(넘치면 잘리고 NUL 종료가 보장되지 않을 수 있어
    // 마지막 바이트를 항상 0으로 둔다).
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() - 1) };
    if rc != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len() - 1);
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(not(unix))]
fn os_hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

// ---------------- 실제 의존성 조립 ----------------

/// 허브 연결 저장소(키체인)에서 최신 기기 토큰을 읽는다. 호출마다 다시 읽는다(D-6).
struct CtxTokens(Arc<HubCtx>);

impl DeviceTokens for CtxTokens {
    fn latest(&self) -> Option<SecretString> {
        // SAVE_FAILED(키체인 쓰기 실패)면 서버가 아는 최신 토큰은 메모리의 새 토큰이다.
        if let Some(p) = self.0.rt().pending.clone() {
            return Some(SecretString::new(p.expose_token()));
        }
        match crate::hub_auth::store::load(&*self.0.store, &self.0.origin) {
            Ok(Some(t)) if t.expires_at > chrono::Utc::now() => {
                Some(SecretString::new(t.expose_token()))
            }
            _ => None,
        }
    }
}

struct LiveDeps {
    tokens: CtxTokens,
    api: ReqwestIngestApi,
    hub: Arc<dyn HubApi>,
    label: String,
    path: PathBuf,
}

fn engine() -> &'static LinkEngine {
    static ENGINE: OnceLock<LinkEngine> = OnceLock::new();
    ENGINE.get_or_init(LinkEngine::new)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 허브 연결 기능이 초기화되기 전(또는 지원하지 않는 OS)에는 None — 엔진은 아무것도 하지 않는다.
fn live() -> Option<Arc<LiveDeps>> {
    static LIVE: Mutex<Option<Arc<LiveDeps>>> = Mutex::new(None);
    let mut guard = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let ctx = crate::hub_auth::current_ctx()?;
        let api = ReqwestIngestApi::new(&ctx.origin).ok()?;
        let path = crate::otel_settings::settings_path()?;
        *guard = Some(Arc::new(LiveDeps {
            hub: ctx.api.clone(),
            tokens: CtxTokens(ctx),
            api,
            label: ingest_label(&os_hostname()),
            path,
        }));
    }
    guard.clone()
}

fn with_env<R>(f: impl FnOnce(&Env) -> R) -> Option<R> {
    let deps = live()?;
    let probe = HubSelfProbe(&*deps.hub);
    let env = Env {
        path: &deps.path,
        tokens: &deps.tokens,
        api: &deps.api,
        probe: &probe,
        label: &deps.label,
    };
    Some(f(&env))
}

fn current_status() -> IngestLinkStatus {
    let view = engine().view();
    let deps = live();
    let hub_connected = deps.as_ref().map(|d| d.tokens.latest().is_some()).unwrap_or(false);
    let supported = deps.is_some();
    match crate::otel_settings::settings_path() {
        Some(path) => {
            let inputs = crate::otel_settings::read_ingest_inputs(&path);
            compose_status(&inputs, &view, hub_connected, supported)
        }
        None => {
            let mut s = IngestLinkStatus::new(IngestLinkState::Unlinked, None);
            s.hub_connected = hub_connected;
            s.issue_supported = supported;
            s.receipt_check_supported = supported;
            s
        }
    }
}

// ---------------- 스케줄러·훅 ----------------

/// 앱 시작(`setup`)에서 한 번 호출한다. 10초마다 엔진의 한 박자(`tick`)를 돈다 — 점검이 필요하면 점검하고,
/// 아니면 첫 데이터 폴링을 한다. 허브 연결 기능이 초기화되지 않았으면 아무것도 하지 않는다.
pub(crate) fn spawn_scheduler(app: tauri::AppHandle) {
    use tauri::Emitter;
    engine().set_notifier(Box::new(move || {
        let _ = app.emit("otel-link-changed", ());
    }));
    std::thread::spawn(|| {
        // 앱 시작 직후의 디스크·네트워크 경합을 피한다.
        std::thread::sleep(std::time::Duration::from_secs(8));
        loop {
            with_env(|env| engine().tick(env, now_ms()));
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });
}

/// 허브를 (재)연결했다 — 새 연결로 취급하고 곧바로 점검한다(D-7).
pub(crate) fn on_hub_connected() {
    engine().on_connected();
}

/// 허브 연결을 해제했다 — 이 기기의 수집키는 서버가 함께 폐기했으므로(D-13) 설정 파일의 `Authorization`
/// 항목만 지운다(다른 헤더 항목·다른 설정은 그대로). 형식이 다른 Authorization은 이 앱이 만든 것이
/// 아니므로 남긴다. 메모리 상태(보관 중인 미기록 원문 포함)도 비운다.
///
/// 세대 증가·메모리 비우기·키 삭제를 한 임계구역에서 한다(`LinkEngine::disconnect_with`). 서버 응답을 기다리던 점검이
/// 해제 뒤에 201을 받아도 키를 다시 쓰지 못한다(RV-05).
pub(crate) fn after_hub_disconnect(path: &Path) -> Result<bool, String> {
    engine().disconnect_with(now_ms(), || crate::otel_settings::remove_ingest_key(path))
}

/// 허브에 연결되어 앱이 수집키를 자동 관리하는 중인가. 이 동안 직접 붙여넣은 수집키는 다음 점검에서 앱이 발급한
/// 키로 교체되고(D-4) 회전 예산을 쓴다 — 그래서 수동 입력을 받지 않는다(RV-02). 키체인을 읽으므로 blocking 컨텍스트에서
/// 부른다. 허브 연결 기능이 초기화되지 않았으면(지원 안 하는 OS 등) false.
pub(crate) fn hub_manages_ingest_key() -> bool {
    live().map(|d| d.tokens.latest().is_some()).unwrap_or(false)
}

// ---------------- Tauri 커맨드 ----------------

/// 프론트가 연동 상태를 조회하는 커맨드. `settings.json`과 엔진의 메모리 상태만 읽는다 — 허브를 부르지 않는다.
#[tauri::command]
pub async fn otel_ingest_link_status() -> Result<IngestLinkStatus, String> {
    crate::hub_auth::run_blocking(current_status)
        .await
        .map_err(|e| e.to_string())
}

/// [다시 시도]: 연속 유실 수·백오프·거절 기록을 비우고 1번부터 점검한다(D-5). 점검이 끝난 상태를 돌려준다.
#[tauri::command]
pub async fn otel_link_retry() -> Result<IngestLinkStatus, String> {
    crate::hub_auth::run_blocking(|| {
        with_env(|env| engine().run_check(env, Trigger::User, now_ms()));
        current_status()
    })
    .await
    .map_err(|e| e.to_string())
}

/// 설정 저장 뒤 호출한다. 직전 점검이 선검사(설정 파일 상태)에서 멈춘 경우에만 다시 점검한다 — 그 밖의
/// 경우에는 아무것도 하지 않아 점검 시점(기동·연결 직후·1시간 간격)을 늘리지 않는다.
#[tauri::command]
pub async fn otel_link_recheck() -> Result<IngestLinkStatus, String> {
    crate::hub_auth::run_blocking(|| {
        if matches!(engine().view().phase, Phase::NotReady | Phase::EndpointNotHub) {
            with_env(|env| engine().run_check(env, Trigger::Settings, now_ms()));
        }
        current_status()
    })
    .await
    .map_err(|e| e.to_string())
}

/// 확인창(D-1)의 답. 거절하면 이번 앱 실행 동안 다시 묻지 않는다.
#[tauri::command]
pub async fn otel_link_answer_confirm(accept: bool) -> Result<IngestLinkStatus, String> {
    crate::hub_auth::run_blocking(move || {
        if accept {
            with_env(|env| engine().accept_confirm(env, now_ms()));
        } else {
            engine().decline_confirm();
        }
        current_status()
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
