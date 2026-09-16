// 전역 설정 파일(~/.claude/malgn-agent.json)의 실제 데이터 소스 — 조회
// (malgn_agent_config_get)와 저장(malgn_agent_config_save) 둘 다 실제로 호출한다.
// workspaces가 곧 자율업무·프로젝트 스캔 범위 그 자체라("이 프로젝트가 왜 안
// 보이지"를 설명하는 근거) 자율업무 화면(views/autonomousTasks.ts) 상단에 조회
// 배너와 편집 폼을 함께 상시 표시한다.
//
// limits는 Rust 쪽 안전 임계값 정본(src-tauri/src/autonomy/config.rs)을 그대로
// 내려받은 값이다 — 프론트는 이 숫자를 하드코딩하지 않고 이 필드에서만 읽는다.
// 저장 시 숫자값(concurrency/defaultTimeout)도 백엔드가 이 limits 범위로
// clamp한다 — 프론트는 입력 힌트에만 limits를 쓰고 별도 clamp를 하지 않는다.
import { invoke } from '@tauri-apps/api/core';

export interface MalgnAgentConfigLimits {
  readonly minInterval: number;
  readonly maxInterval: number;
  readonly minTimeout: number;
  readonly maxTimeout: number;
  readonly maxConcurrency: number;
  readonly startupGraceMinutes: number;
  // 고정시각 스케줄 "따라잡기 창"(분) — src-tauri/src/config/mod.rs에서
  // 항상 내려주는 비-옵셔널 값이다. 프론트는 이 숫자를 하드코딩하지 않고
  // 이 필드에서만 읽는다(views/autonomousTasks.ts).
  readonly missedRunGraceMinutes: number;
}

export interface MalgnAgentConfigStatus {
  readonly ok: boolean;
  readonly error: string | null;
  readonly configPath: string;
  readonly fileExists: boolean;
  readonly workspaces: readonly string[];
  readonly warnings: readonly string[];
  readonly autonomy: { readonly concurrency: number; readonly defaultTimeout: number };
  readonly logs: { readonly retentionDays: number };
  readonly limits: MalgnAgentConfigLimits;
}

export async function fetchMalgnAgentConfig(): Promise<MalgnAgentConfigStatus> {
  return invoke<MalgnAgentConfigStatus>('malgn_agent_config_get');
}

export interface MalgnAgentConfigInput {
  readonly workspaces: readonly string[];
  readonly autonomy: { readonly concurrency: number; readonly defaultTimeout: number };
  readonly logs: { readonly retentionDays: number };
}

// 실패(Err)는 홈 디렉터리 확인 불가 등 하드 실패만이다 — workspace 경로가
// 존재하지 않는 등은 에러가 아니라 반환된 status.warnings에 담겨 온다(저장
// 자체는 막지 않는다, malgn_agent_config_get과 동일한 관용).
export async function saveMalgnAgentConfig(payload: MalgnAgentConfigInput): Promise<MalgnAgentConfigStatus> {
  return invoke<MalgnAgentConfigStatus>('malgn_agent_config_save', { payload });
}
