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

export async function fetchOtelSettings(): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_get');
}

export async function saveOtelSettings(payload: OtelSavePayload): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_save', { payload });
}
