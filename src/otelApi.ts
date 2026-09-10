// OTel 설정 화면의 실제 데이터 소스 — ~/.claude/settings.json의 관리대상(allowlist)
// 14개 키를 읽고(otel_settings_get) 실제로 저장한다(otel_settings_save). 이전
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
}

export interface OtelSavePayload {
  readonly values: Readonly<Record<string, string>>;
  readonly identity: OtelIdentity | null;
}

export async function fetchOtelSettings(): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_get');
}

export async function saveOtelSettings(payload: OtelSavePayload): Promise<OtelSettings> {
  return invoke<OtelSettings>('otel_settings_save', { payload });
}
