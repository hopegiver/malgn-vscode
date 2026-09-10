// "자율업무" 화면의 실제 데이터 소스. autonomy_* 커맨드가 프로젝트별로 등록된
// 자율업무 "설정"(프롬프트·주기·서브에이전트·타임아웃)을 파일(autonomy.json)에서
// 읽고 쓴다. 실행 상태(현재 실행 중 여부·마지막 실행 결과·다음 실행 시각)는
// 더 이상 그 파일에 영속화되지 않는다 — Rust 프로세스 메모리에만 있고
// autonomy_runtime_status()로 별도 조회한다(설정=파일 / 상태=메모리 / 이력=로그,
// 설계 §2). 이 모듈은 두 조회를 모두 노출할 뿐 스스로 스케줄을 실행하지 않는다.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AutonomyRunStatus = 'success' | 'failed' | 'timeout';

export interface AutonomyTaskConfig {
  id: string;
  name: string;
  prompt: string;
  subagent: string | null;
  interval: number; // 분 — "이전 실행이 끝난 뒤" 대기하는 시간(완료 기준, 시작 기준이 아니다)
  enabled: boolean;
  timeout: number | null; // 분. null이면 전역 기본값(malgn-agent.json의 autonomy.defaultTimeout)을 쓴다.
}

export interface ProjectAutonomyGroup {
  readonly projectPath: string;
  readonly projectName: string;
  readonly tasks: readonly AutonomyTaskConfig[];
}

// 메모리 런타임 상태 1건 — (projectPath, taskId) 복합키로 설정과 병합한다.
// child_pid는 백엔드 내부 종료 처리용이라 프론트에는 애초에 내려오지 않는다.
export interface AutonomyRuntimeStatus {
  readonly projectPath: string;
  readonly taskId: string;
  readonly running: boolean;
  readonly lastStartedAt: string | null; // RFC3339 UTC
  readonly lastFinishedAt: string | null; // RFC3339 UTC
  readonly nextRunAt: string | null; // RFC3339 UTC. enabled=false면 null.
  readonly status: AutonomyRunStatus | null;
  readonly summary: string | null; // stdout/stderr 꼬리 최대 500자
  readonly durationMs: number | null;
  readonly logPath: string | null;
}

// 전역 설정(malgn-agent.json)이 손상되면 Err를 던진다 — 조용한 빈 목록 대체가
// 아니라 화면이 그대로 오류를 보여줘야 한다(설계 §5, fail-closed).
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

// 항상 성공한다(설계 §11) — 손상될 파일이 없는 순수 메모리 스냅숏이라 Err 경로가 없다.
export async function fetchAutonomyRuntimeStatus(): Promise<AutonomyRuntimeStatus[]> {
  return invoke<AutonomyRuntimeStatus[]>('autonomy_runtime_status');
}

// 실행 시작/종료 시 1회씩 emit되는 이벤트 — payload는 바뀐 task 1건. 구독
// 자체가 실패하면(Tauri IPC 브리지가 없는 플레인 브라우저) 이 Promise가
// reject되므로, 호출부(views/autonomousTasks.ts)가 그 경우 폴링으로 대체한다.
export async function onAutonomyRuntimeChanged(callback: (status: AutonomyRuntimeStatus) => void): Promise<void> {
  await listen<AutonomyRuntimeStatus>('autonomy-task-updated', (event) => callback(event.payload));
}
