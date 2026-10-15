// OTel 설정 화면의 실제 데이터 소스 — ~/.claude/settings.json의 관리대상(allowlist)
// 15개 키를 읽고(otel_settings_get) 실제로 저장한다(otel_settings_save). 이전
// 커맨드(OTEL_ 접두사만 훑던 읽기 전용 조회)는 삭제됐다 — CLAUDE_CODE_
// ENABLE_TELEMETRY처럼 OTEL_ 접두사가 아닌 관리대상 키를 놓치던 버그가
// 그 필터에 있었다.
//
// employee.* 조립(퍼센트 인코딩·기존 attribute 보존)은 전적으로 Rust 책임이다
// (설계 §8) — 프론트는 {email, name}만 넘기고 OTEL_RESOURCE_ATTRIBUTES 문자열을
// 절대 조립하지 않는다.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface OtelIdentity {
  readonly email: string;
  readonly name: string | null;
}

// 허브 수집키(`OTEL_EXPORTER_OTLP_HEADERS`의 `Authorization=Bearer mhk_…`)는 원문이
// 프론트로 오지 않는다 — Rust가 상태와 마스킹(`mhk_`+4자)만 내려준다. 그래서 이 키는
// `values`에 없고(managedKeys에는 있다) 저장도 `values`가 아니라 `ingestKey`로만 한다.
export type IngestKeyState = 'unset' | 'set' | 'unrecognized';

export interface IngestKeyStatus {
  // unset: Authorization 헤더 없음 / set: 허브 형식(mhk_…) / unrecognized: 형식이 다른
  // Authorization이 있음(원문은 오지 않는다).
  readonly state: IngestKeyState;
  // state === 'set'일 때만 `mhk_AbCd…` 형태.
  readonly masked: string | null;
}

// 수집키는 설정된 모든 엔드포인트로 나간다 — 유효 엔드포인트(파일 값 → 기본값)의 성격 요약.
// URL 자체는 오지 않는다. 허브 거부 경고는 targetsHub일 때만, 평문/허브 밖 경고는 키가 설정된
// 경우에만 의미가 있다.
export interface IngestEndpointAssessment {
  // 하나라도 허브 origin.
  readonly targetsHub: boolean;
  // 하나라도 평문 http://(루프백 제외) — 키가 암호화 없이 전송될 수 있다.
  readonly insecure: boolean;
  // 하나라도 허브가 아닌 호스트 — 키가 허브 밖으로도 나간다.
  readonly foreignHost: boolean;
}

// keep: 기존 키 유지(생략과 같다) / set: 사용자가 새로 입력한 원문만 / clear: 키 삭제.
// 마스킹된 표시값을 set으로 되돌려 보내지 않는다(Rust도 형식 검증으로 거부한다).
export type IngestKeyChange =
  | { readonly action: 'keep' }
  | { readonly action: 'set'; readonly key: string }
  | { readonly action: 'clear' };

export interface OtelSettings {
  readonly settingsPath: string;
  readonly fileExists: boolean;
  readonly parseError: string | null;
  readonly managedKeys: readonly string[];
  readonly readOnlyKeys: readonly string[];
  // 파일에 실제로 있는 관리대상 키만 담긴다(값 우선순위: values → defaults → 빈값).
  readonly values: Readonly<Record<string, string>>;
  // (a)소스 하드코딩 + (b)빌드타임 주입 기본값의 병합 결과.
  readonly defaults: Readonly<Record<string, string>>;
  // false면 사내 collector 주소가 빌드에 주입되지 않은 상태(포크·secret 없는 CI
  // 빌드) — 엔드포인트 필드는 빈 값 + placeholder로 보여주고 직접 입력을 안내한다.
  readonly endpointDefaultsInjected: boolean;
  readonly ingestKey: IngestKeyStatus;
  readonly ingestEndpoint: IngestEndpointAssessment;
}

export interface OtelSavePayload {
  readonly values: Readonly<Record<string, string>>;
  readonly identity: OtelIdentity | null;
  // 생략하면 기존 수집키를 그대로 둔다.
  readonly ingestKey?: IngestKeyChange;
}

// ---- 수집키 연동 상태(허브 자동 발급·회전·수신확인 경로의 데이터 계약) ----
//
// 상태 문구/원인은 기기 토큰(mhd_)·수집키(mhk_) 원문을 담지 않는다 — masked는 `mhk_`+4자뿐이고
// failure.message는 Rust가 원인(cause)별로 고정한 문구다.
//
// 상태는 settings.json 실물에서 도출한 값(unlinked / applied / failed(파일 원인)) 위에 앱 안의 점검 엔진이
// 메모리로 들고 있는 단계(issuing / confirmed / 허브 쪽 실패 원인)를 얹은 것이다. 엔진 상태는 영속하지
// 않는다 — 앱을 다시 시작하면 첫 점검이 같은 판단을 다시 만든다. 변경은 `otel-link-changed` 이벤트로 온다.
//
// 'applied'는 "키가 파일에 있다"는 뜻일 뿐 수집을 보장하지 않는다. 수집 준비도는 상태값이 아니라
// 별도 플래그(telemetryEnabled / endpoint / collectionReady / blockers)로 온다 — applied(또는 confirmed)인데
// collectionReady === false 이면 "적용됐지만 수집되지 않을 수 있음"으로 구분해 보여 준다.
export type IngestLinkState = 'unlinked' | 'issuing' | 'applied' | 'confirmed' | 'failed';

export type IngestLinkFailureCause =
  | 'settingsUnreadable' // settings.json을 JSON으로 해석하지 못함(파일 도출)
  | 'headerUnrecognized' // 허브 형식이 아닌 Authorization 헤더가 있음(파일 도출)
  | 'endpointNotHub' // 수집 엔드포인트가 허브 밖이거나 평문 http라 발급 전에 멈춤
  | 'notReady' // 설정 파일에서 수집이 켜져 있지 않아(텔레메트리·exporter·프로토콜·엔드포인트) 발급 전에 멈춤
  | 'featureUnavailable' // 허브에 이 기능이 아직 없음(연결은 정상, 1시간 뒤 다시 확인)
  | 'reconnectRequired' // 허브가 이 기기의 연결을 끊음 — 재연결 필요
  | 'notDesktopToken' // 데스크톱 앱이 발급한 토큰이 아님(재시도하지 않음)
  | 'hubRejected' // 허브가 요청을 거부함
  | 'unreachable' // 허브에 닿지 못함(자동 재시도)
  | 'rateLimited' // 요청 빈도 초과(잠시 후 자동으로 이어 함)
  | 'invalidKeyReturned' // 허브가 돌려준 값이 수집키 형식이 아님
  | 'applyFailed' // 발급된 키를 설정 파일에 쓰지 못함(원문은 메모리에 보관 중)
  | 'blocked' // 이 연결로는 발급·회전 불가 — blockedReason 참고
  | 'credentialLimit' // 사용자 유효 키 상한(5)에 도달
  | 'rotateBudgetExceeded' // 이 기기의 회전 예산 소진 — retryAt까지 회전하지 않음
  | 'rotationLost'; // 응답이 반복해서 끊겨 자동 회전을 멈춤

// 수집이 안 될 수 있는 원인(파일 실물 기준) — 문구는 onboarding.ts가 고른다.
export type ReadinessBlocker =
  | 'telemetryOff'
  | 'exporterNotOtlp'
  | 'protocolMismatch'
  | 'endpointMissing'
  | 'endpointNotHub'
  | 'endpointForeign'
  | 'endpointInsecure';

export type IngestBlockedReason = 'suspectedLeak' | 'quota';

export interface IngestLinkFailure {
  readonly cause: IngestLinkFailureCause;
  // 원인별 고정 문구(그대로 화면에 써도 된다).
  readonly message: string;
}

export interface IngestLinkStatus {
  readonly state: IngestLinkState;
  // `mhk_AbCd…` 형태. 키가 적용된 상태(applied/confirmed)이거나 키가 적용된 채 다음 단계가
  // 실패한 경우에만 있고, 그 외에는 null.
  readonly masked: string | null;
  // state === 'failed'일 때만 있다.
  readonly failure: IngestLinkFailure | null;
  // 앱이 수집키를 자동 발급할 수 있는 환경인가 / 수신 확인이 가능한가(허브 연결 기능이 초기화됨).
  readonly issueSupported: boolean;
  readonly receiptCheckSupported: boolean;
  // settings.json의 CLAUDE_CODE_ENABLE_TELEMETRY가 "1"인가(파일에 없으면 false).
  readonly telemetryEnabled: boolean;
  // 파일 실물 기준 엔드포인트 성격(URL은 오지 않는다). targetsHub는 metrics·logs 둘 다 허브일 때만 true —
  // OtelSettings.ingestEndpoint(설정 폼 기준, "하나라도")와 의미가 다르다.
  readonly endpoint: IngestEndpointAssessment;
  // blockers가 비어 있는가. 앱 밖(셸 환경변수 등)에서 켜진 텔레메트리는 보지 못한다.
  readonly collectionReady: boolean;
  readonly blockers: readonly ReadinessBlocker[];
  // 허브 기기 토큰이 있다(허브에 연결됨).
  readonly hubConnected: boolean;
  // 웹에서 폐기된 키라 사람의 확인을 기다린다 — 확인창을 띄운다.
  readonly confirmRequired: boolean;
  // 사용자가 확인을 거절했다(이번 앱 실행 동안 다시 묻지 않는다).
  readonly confirmDeclined: boolean;
  // 허브가 파일에 남은 키를 웹에서 폐기된 것으로 판정했고 그 판정이 이어지고 있다. confirmRequired/confirmDeclined이면 항상 true이고,
  // 그 밖에 폐기 판정을 받은 키가 파일에 남은 채 다음 점검이 일시 오류로 끝나 재확인을 못 한 경우에도 true다.
  readonly revokedByHub: boolean;
  // 적용된 키의 첫 데이터를 기다리는 중(최대 15분) / 15분 안에 오지 않아 확인을 멈춤.
  readonly waitingFirstData: boolean;
  readonly firstDataTimedOut: boolean;
  // failure.cause === 'blocked'일 때 차단 사유.
  readonly blockedReason: IngestBlockedReason | null;
  // failure.cause === 'rotateBudgetExceeded'일 때 다시 회전할 수 있는 시각(RFC 3339).
  readonly retryAt: string | null;
  // 키가 적용돼 있는(applied/confirmed) 상태에서 마지막 점검이 일시적인 원인(허브에 닿지 못함·요청 빈도 초과·
  // 서버에 기능 없음)으로 끝났다 — 적용·수신 확인 사실은 그대로이고 state를 failed로 덮지 않는다. 상태 문구 뒤에 보조 안내로만 붙인다.
  readonly notice: IngestLinkFailure | null;
  // 마지막 발급·회전 201을 적용하며 허브 권장값으로 바뀐 설정 키 이름(이전 파일 값과 달랐던 것만, 값은 오지 않는다).
  // 앱을 다시 시작하면 비워진다.
  readonly serverEnvChanged: readonly string[];
  // 마지막 201에서 허브가 보냈지만 허용 목록·값 형식 검사를 통과하지 못해 쓰지 않은 키 이름(값은 오지 않는다).
  readonly serverEnvRejected: readonly string[];
}

// settings.json과 엔진의 메모리 상태만 읽는다(허브를 부르지 않는다).
export async function fetchIngestLinkStatus(): Promise<IngestLinkStatus> {
  return invoke<IngestLinkStatus>('otel_ingest_link_status');
}

// [다시 시도]: 연속 유실 수·백오프·거절 기록을 비우고 1번부터 점검한다. 점검이 끝난 상태를 돌려준다.
export async function retryIngestLink(): Promise<IngestLinkStatus> {
  return invoke<IngestLinkStatus>('otel_link_retry');
}

// 설정 저장 뒤 호출한다. 직전 점검이 설정 파일 상태(수집 준비 안 됨·허브 밖 엔드포인트)에서 멈춘 경우에만
// 백엔드가 다시 점검한다 — 그 밖에는 아무것도 하지 않는다.
export async function recheckIngestLink(): Promise<IngestLinkStatus> {
  return invoke<IngestLinkStatus>('otel_link_recheck');
}

// 재발급 확인창(confirm_required)의 답. 거절하면 이번 앱 실행 동안 다시 묻지 않는다.
export async function answerIngestReissueConfirm(accept: boolean): Promise<IngestLinkStatus> {
  return invoke<IngestLinkStatus>('otel_link_answer_confirm', { accept });
}

// 백엔드가 연동 상태를 바꿀 때마다 오는 이벤트(점검 시작·끝, 첫 데이터 확인 등).
export async function onIngestLinkChanged(callback: () => void): Promise<void> {
  await listen('otel-link-changed', () => callback());
}

export async function fetchOtelSettings(): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_get');
}

export async function saveOtelSettings(payload: OtelSavePayload): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_save', { payload });
}
