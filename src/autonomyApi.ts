// "자율업무" 화면의 실제 데이터 소스. Rust 커맨드 autonomy_*가 프로젝트별로 등록된
// 자율업무 정의(프롬프트·주기·서브에이전트 등)를 저장/조회하고, 실제 스케줄 실행
// 엔진(주기마다 claude를 호출하는 백그라운드 러너)도 Rust 쪽에서 돌아간다 — 이
// 모듈은 그 결과(lastRunAt/lastStatus/history)를 읽기만 할 뿐 스스로 실행하지 않는다.
import { invoke } from '@tauri-apps/api/core';

export type AutonomyRunResult = 'success' | 'failed';
export type AutonomyLastStatus = 'success' | 'failed' | 'running';

export interface AutonomyHistoryEntry {
  readonly at: string; // ISO8601 UTC
  readonly result: AutonomyRunResult;
}

export interface AutonomyTaskConfig {
  id: string;
  name: string;
  prompt: string;
  subagent: string | null;
  intervalMinutes: number;
  enabled: boolean;
  preventOverlap: boolean;
  lastRunAt: string | null; // ISO8601 UTC
  lastStatus: AutonomyLastStatus | null;
  lastSummary: string | null;
  history: readonly AutonomyHistoryEntry[]; // 최신이 앞, 최대 10개
}

export interface ProjectAutonomyGroup {
  readonly projectPath: string;
  readonly projectName: string;
  readonly tasks: readonly AutonomyTaskConfig[];
}

export async function fetchAutonomyTasks(): Promise<ProjectAutonomyGroup[]> {
  return invoke<ProjectAutonomyGroup[]>('autonomy_list');
}

// upsert — task.id가 이미 존재하면 갱신, 없으면 새로 추가한다(백엔드 계약).
export async function saveAutonomyTask(projectPath: string, task: AutonomyTaskConfig): Promise<void> {
  return invoke<void>('autonomy_save_task', { projectPath, task });
}

export async function deleteAutonomyTask(projectPath: string, taskId: string): Promise<void> {
  return invoke<void>('autonomy_delete_task', { projectPath, taskId });
}

export async function setAutonomyTaskEnabled(projectPath: string, taskId: string, enabled: boolean): Promise<void> {
  return invoke<void>('autonomy_set_enabled', { projectPath, taskId, enabled });
}
