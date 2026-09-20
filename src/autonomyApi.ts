// "자율업무" 화면의 실제 데이터 소스. autonomy_* 커맨드가 프로젝트별로 등록된
// 자율업무 "설정"(프롬프트·주기·서브에이전트·타임아웃)을 파일(autonomy.json)에서
// 읽고 쓴다. 실행 상태(현재 실행 중 여부·마지막 실행 결과·다음 실행 시각)는
// 더 이상 그 파일에 영속화되지 않는다 — Rust 프로세스 메모리에만 있고
// autonomy_runtime_status()로 별도 조회한다(설정=파일 / 상태=메모리 / 이력=로그,
// 설계 §2). 이 모듈은 두 조회를 모두 노출할 뿐 스스로 스케줄을 실행하지 않는다.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AutonomyRunStatus = 'success' | 'failed' | 'timeout';

// 'interval' = 이전 실행 완료 후 N분 뒤 재실행(레거시 파일의 유일한 동작이자
// 기본값). 'fixedTime' = 로컬 벽시계 기준 고정 시각(+요일)에 실행. 백엔드가
// 레거시 파일도 'interval'로 채워 항상 내려준다.
// 'hourly' = 매시 정각 기준 M분에 실행(신규, 설계서 §2.2). 'cron' = 표준
// 5필드(분 시 일 월 요일) cron 표현식으로 실행(신규, 설계서 §2.2). 두 값
// 모두 UI 2단 계층("고정 시간" 라디오 아래 하위 탭)에서만 선택된다 — 백엔드
// 계약은 design-autonomy-schedule-cron.md §0 결정 D·§8.1 기준으로 가정했다.
export type AutonomyScheduleMode = 'interval' | 'fixedTime' | 'hourly' | 'cron';

export interface AutonomyTaskConfig {
  id: string;
  name: string;
  prompt: string;
  subagent: string | null;
  interval: number; // 분 — "이전 실행이 끝난 뒤" 대기하는 시간(완료 기준, 시작 기준이 아니다). scheduleMode와 무관하게 항상 전송한다(fixedTime 모드에서는 사용되지 않을 뿐).
  scheduleMode: AutonomyScheduleMode;
  // 'HH:MM' 로컬 벽시계(24시간, 0패딩). interval 모드면 키 자체가 없을 수
  // 있다 — 기존 subagent/timeout과 같은 "옵셔널 필드" 함정을 반복하지 않도록
  // `?`로 선언한다. 화면 모델(state.ts의 AutonomousTask)에서는
  // toDisplayTask()가 `?? null`로 명시 정규화해 비-옵셔널로 만든다.
  atTime?: string | null;
  // 0=일…6=토. 빈 배열/키 부재 = 매일. 위와 동일한 이유로 `?`로 선언한다.
  days?: number[];
  // Hourly 모드의 실행 "분"(0~59, 매시 이 분에 실행). scheduleMode가 'hourly'가
  // 아니면 키 자체가 없을 수 있다(백엔드 skip_serializing_if=None, 설계서 §8.2)
  // — 위 atTime/days와 동일한 이유로 `?`로 선언한다.
  hourlyMinute?: number | null;
  // Cron 모드의 표준 5필드(분 시 일 월 요일) 표현식. scheduleMode가 'cron'이
  // 아니면 키 자체가 없을 수 있다(설계서 §8.2) — 위와 동일한 이유로 `?`로 선언한다.
  cron?: string | null;
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

// next_run_at 도래를 기다리지 않고 즉시 실행을 요청한다. 성공해도 실행 완료를
// 기다리지 않고 바로 resolve된다(실제 실행 시작/진행은 항상 그랬듯
// autonomy_runtime_status 폴링 또는 onAutonomyRuntimeChanged 이벤트로 반영된다
// — 이 함수는 트리거일 뿐 상태 채널이 아니다). 실패(이미 실행 중 / 동시 실행
// 한도 초과 등)는 한국어 에러 메시지로 reject되며, 그 메시지를 그대로
// 사용자에게 보여준다(백엔드 계약, 프론트에서 문구를 새로 짓지 않는다).
export async function runAutonomyTaskNow(projectPath: string, taskId: string): Promise<void> {
  return invoke<void>('autonomy_run_now', { projectPath, taskId });
}

// 실행 시작/종료 시 1회씩 emit되는 이벤트 — payload는 바뀐 task 1건. 구독
// 자체가 실패하면(Tauri IPC 브리지가 없는 플레인 브라우저) 이 Promise가
// reject되므로, 호출부(views/autonomousTasks.ts)가 그 경우 폴링으로 대체한다.
export async function onAutonomyRuntimeChanged(callback: (status: AutonomyRuntimeStatus) => void): Promise<void> {
  await listen<AutonomyRuntimeStatus>('autonomy-task-updated', (event) => callback(event.payload));
}

// 'success'/'failed'/'timeout'은 AutonomyRunStatus와 값이 겹치지만, 'aborted'
// (앱 종료로 인한 강제 중단)는 과거 실행 1건 단위로만 의미가 있어 런타임
// 상태(AutonomyRunStatus, 항상 최신 1건만 가리킴)와 타입을 분리했다.
export type AutonomyHistoryResult = 'success' | 'failed' | 'timeout' | 'aborted';

// 과거 실행 이력 1건 — 로그 디렉터리(.claude/logs/autonomy/<날짜>/)에서 읽어온
// 완료된 실행 기록이다. summary(stdout/stderr 꼬리)는 이 항목에 포함되지 않는다
// — AutonomyRuntimeStatus.summary는 "가장 최근 실행 1건"에만 한정된 메모리
// 값이고, 과거 이력 각 건의 요약은 백엔드가 보존하지 않는다(로그 파일 경로로만
// 추적 가능). 화면(views/autonomousTasks.ts)도 이 항목을 펼쳤을 때 요약 없이
// 로그 경로만 보여준다.
export interface RunHistoryEntry {
  readonly startedAt: string; // RFC3339
  readonly finishedAt: string; // RFC3339
  readonly durationMs: number;
  readonly result: AutonomyHistoryResult;
  readonly logPath: string;
}

// 최신순 정렬, limit 생략 시 백엔드 기본값(최근 20건). 로그 디렉터리가 없거나
// 읽기에 실패해도 Err가 아니라 빈 배열을 반환한다 — 그 경우는 "이력 없음"
// 정상 상태로 수렴한다. Err는 projectPath가 워크스페이스 밖이거나 존재하지
// 않을 때만 발생한다(백엔드 계약 — 설계서 §4-6 표와 달리 이쪽이 실제 동작).
export async function fetchAutonomyTaskHistory(projectPath: string, taskId: string, limit?: number): Promise<RunHistoryEntry[]> {
  return invoke<RunHistoryEntry[]>('autonomy_task_history', { projectPath, taskId, limit });
}

// 스케줄러 heartbeat(리뷰 D1 권고 ③) — src-tauri/src/autonomy/mod.rs의
// SchedulerHealth를 그대로 옮긴 타입이다. lastTickAt/now는 chrono
// DateTime<Utc>가 RFC3339 문자열로 직렬화된 값(serde 기본 동작 — 이 프로젝트는
// #[serde(rename)]만 걸었을 뿐 커스텀 직렬화가 없다), tickSeconds는 u64라
// number로 받는다. lastTickAt은 `Option<DateTime<Utc>>`이므로 스케줄러가 아직
// 한 번도 tick을 돌지 않은 상태(앱 기동 직후)에서는 null이다 — 이 상태를
// "정체"로 오판하면 안 된다(호출부 판단 로직, views/autonomousTasks.ts 참고).
// 이 커맨드는 순수 메모리 읽기라 Result가 아니다(항상 성공) — 다른 "항상 성공"
// 커맨드(autonomy_runtime_status)와 동일하게 Rust 시그니처에 Err 경로가 없다.
export interface SchedulerHealth {
  readonly lastTickAt: string | null; // RFC3339 UTC
  readonly now: string; // RFC3339 UTC — 클라이언트 시계가 아니라 서버 시각. 클럭 스큐를 피하려면 반드시 이 값으로 diff를 계산한다(Date.now()로 비교하지 않는다).
  readonly tickSeconds: number;
}

export async function fetchSchedulerHealth(): Promise<SchedulerHealth> {
  return invoke<SchedulerHealth>('autonomy_scheduler_health');
}
