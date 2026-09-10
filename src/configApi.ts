// 전역 설정 파일(~/.claude/malgn-agent.json)의 실제 데이터 소스 — 읽기 전용이다.
// 이 앱은 이 파일에 쓰지 않는다(설계 §13 범위 밖: malgn-agent.json 쓰기 UI는
// 이번 작업에 포함되지 않는다). workspaces가 곧 자율업무·프로젝트 스캔 범위
// 그 자체라("이 프로젝트가 왜 안 보이지"를 설명하는 근거) 자율업무 화면
// (views/autonomousTasks.ts) 상단에 상시 표시한다.
//
// limits는 Rust 쪽 안전 임계값 정본(src-tauri/src/autonomy/config.rs)을 그대로
// 내려받은 값이다 — 프론트는 이 숫자를 하드코딩하지 않고 이 필드에서만 읽는다.
import { invoke } from '@tauri-apps/api/core';

export interface MalgnAgentConfigLimits {
  readonly minInterval: number;
  readonly maxInterval: number;
  readonly minTimeout: number;
  readonly maxTimeout: number;
  readonly maxConcurrency: number;
  readonly startupGraceMinutes: number;
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
