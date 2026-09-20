// 자율업무 — 프로젝트별로 등록한 자율업무 "설정"(프롬프트·주기·서브에이전트·
// 타임아웃)을 자율업무 실행 상태와 병합해 보여준다. 설정은 autonomy.json(파일),
// 실행 상태(현재 실행 중 여부·마지막 실행 결과·다음 실행 시각)는 Rust 프로세스
// 메모리(autonomy_runtime_status)로 완전히 분리됐다(설계 §1·§2) — 이 화면은 그
// 둘을 (projectPath, taskId) 키로 합쳐 그릴 뿐, 스스로 스케줄을 실행하지 않고
// preventOverlap 같은 옵션도 없다(직렬 실행이 백엔드 구조로 이미 보장된다).
//
// 과거 실행 이력은 프로젝트의 .claude/logs/autonomy/<날짜>/ 아래 로그 파일에서
// autonomy_task_history(projectPath, taskId, limit?)로 조회해 상세 화면의
// "기록" 카드에 최근 N건(기본 20건, "더 보기" 클릭 시 20건씩 증가)을 그린다
// (재설계 docs/design/autonomy-task-detail-redesign.md §4). 설정/런타임 상태와는
// 분리된 3번째 IPC라 이력 조회 실패가 나머지 블록 표시를 막지 않는다 — 조회
// 결과는 (projectPath, taskId) 키의 모듈 스코프 캐시(historyCache)에 저장해,
// 이 화면의 전체 재렌더 구조(notifyChange)에서도 이미 로드했거나 로딩 중이면
// 재조회하지 않는다.
import { el, showToast, toggleSwitch, createModalOverlay, confirmDialog, loadingBlock, errorBlock, boundField, boundChecked } from '../dom';
import { state, notifyChange } from '../state';
import type { AutonomousTask } from '../state';
import {
  fetchAutonomyTasks,
  saveAutonomyTask,
  deleteAutonomyTask,
  setAutonomyTaskEnabled,
  fetchAutonomyRuntimeStatus,
  onAutonomyRuntimeChanged,
  runAutonomyTaskNow,
  fetchAutonomyTaskHistory,
} from '../autonomyApi';
import type {
  AutonomyTaskConfig,
  ProjectAutonomyGroup,
  AutonomyRuntimeStatus,
  AutonomyScheduleMode,
  RunHistoryEntry,
  AutonomyHistoryResult,
} from '../autonomyApi';
import { fetchMalgnAgentConfig, saveMalgnAgentConfig } from '../configApi';
import type { MalgnAgentConfigInput, MalgnAgentConfigStatus } from '../configApi';
import { describeWorkspaceScanScope, summarizeSkippedProjects } from '../workspaceScanHint';
import { navigate } from '../route';

const INTERVAL_OPTIONS: readonly { readonly value: string; readonly label: string }[] = [
  { value: '5', label: '5분마다' },
  { value: '15', label: '15분마다' },
  { value: '30', label: '30분마다' },
  { value: '60', label: '1시간마다' },
  { value: '360', label: '6시간마다' },
  { value: '1440', label: '24시간마다' },
];

type BoardColumn = 'running' | 'success' | 'failed' | 'waiting';
const BOARD_COLUMNS: readonly { readonly key: BoardColumn; readonly label: string; readonly statusClass: string }[] = [
  { key: 'running', label: '진행중', statusClass: 'running' },
  { key: 'success', label: '성공', statusClass: 'ok' },
  { key: 'failed', label: '실패', statusClass: 'fail' },
  { key: 'waiting', label: '대기중', statusClass: 'pending' },
];

// 상세 화면 재설계(§4-2)로 "마지막 실행 결과"는 더 이상 메타 카드에 단독
// 표시되지 않고 이력 리스트의 결과 배지(RUN_HISTORY_META, 파일 하단)로만
// 표현한다 — 그래서 AutonomyRunStatus 전용 라벨 상수는 더 이상 필요 없다.

// 0=일…6=토 — 백엔드(chrono::Weekday::num_days_from_sunday)와 동일한 축. 순서를
// 바꾸지 않는다(설계서 §3·§4.1).
const WEEKDAY_LABELS: readonly string[] = ['일', '월', '화', '수', '목', '금', '토'];
const WEEKDAY_PRESET_WEEKDAYS: readonly number[] = [1, 2, 3, 4, 5];
const WEEKDAY_PRESET_WEEKEND: readonly number[] = [0, 6];

// 고정시간(fixedTime 상위 라디오) 아래 2단 하위 탭 — 설계서 §2.2 매핑표 정본.
// '매일'/'매주'는 백엔드로 둘 다 scheduleMode='fixedTime'을 보내고 days로만
// 구분한다(§2.2). '매시간'='hourly', '사용자 지정'='cron'.
type FixedTimeSubTab = 'hourly' | 'daily' | 'weekly' | 'custom';

// 전역 설정 편집 모달 — 열림 상태 자체는 이 화면 전용 값이라 기존 전역 state
// (state.malgnAgentConfig.editingAutonomy)를 그대로 재사용하지만(sessions.ts의
// metaModalOpen과 달리 다른 화면과 공유되지 않는다), ESC 리스너는 sessions.ts
// 세션 메타데이터 모달 패턴과 동일하게 모듈 스코프로 둔다.
let autonomyConfigModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachAutonomyConfigModalEscHandler(): void {
  if (autonomyConfigModalEscHandler) {
    window.removeEventListener('keydown', autonomyConfigModalEscHandler);
    autonomyConfigModalEscHandler = null;
  }
}

function closeAutonomyConfigModal(): void {
  state.malgnAgentConfig.editingAutonomy = false;
  autonomyConfigDraft = null;
  detachAutonomyConfigModalEscHandler();
  notifyChange();
}

// 입력 중 드래프트 — workspacesDraft(projects.ts)와 동일한 원칙: 모달이 열려
// 있는 동안 한 번만 초기화되고, 닫힐 때 버려진다(dom.ts의 boundField 참고).
interface AutonomyConfigFormDraft {
  concurrency: string;
  defaultTimeout: string;
  retentionDays: string;
}
let autonomyConfigDraft: AutonomyConfigFormDraft | null = null;

// 입력 중 드래프트 — 이 폼은 필드가 많아(이름·프롬프트·프로젝트·실행방식·
// 하위탭 4종·서브에이전트·타임아웃) 개별 필드마다 draft를 따로 두지 않고 폼
// 전체를 하나의 객체로 관리한다. openTaskFormModal에서 editingTask를 기반으로
// 딱 한 번 초기화되고, closeTaskFormModal에서 버려진다(dom.ts의 boundField/
// boundChecked 참고). 제출 검증/변환 로직(renderTaskForm의 submit 핸들러)은
// 그대로 DOM 노드의 `.value`/`.checked`를 읽는다 — draft는 "재렌더에서 살아남는
// 초기값 저장소" 역할만 하고, 제출 시점의 정본은 여전히 DOM이다.
interface TaskFormDraft {
  name: string;
  prompt: string;
  projectPath: string; // 추가 모드(프로젝트 select)에서만 쓰인다 — 수정 모드는 editingTask.projectPath 고정
  interval: string;
  scheduleMode: 'interval' | 'fixedTime';
  activeSubTab: FixedTimeSubTab;
  hourlyMinute: string;
  time: string;
  days: number[];
  cron: string;
  subagent: string;
  timeout: string;
}

// 편집 모달을 열 때 어느 하위 탭을 선택할지 판정한다(설계서 §2.3 역매핑
// 정본). fixedTime은 days.length로 매일/매주를 가른다 — 7개 전부(레거시
// "매일" 저장 형태)도 매일로 접는다(computeFixedTimeScheduleLabel과 동일한
// 판단 기준).
function initialFixedTimeSubTabFor(editingTask: AutonomousTask | null): FixedTimeSubTab {
  if (!editingTask) return 'daily';
  if (editingTask.scheduleMode === 'hourly') return 'hourly';
  if (editingTask.scheduleMode === 'cron') return 'custom';
  if (editingTask.scheduleMode === 'fixedTime') {
    return editingTask.days.length === 0 || editingTask.days.length >= 7 ? 'daily' : 'weekly';
  }
  return 'daily';
}

function buildInitialTaskFormDraft(editingTask: AutonomousTask | null): TaskFormDraft {
  const initialWeeklyDays: readonly number[] =
    editingTask && editingTask.scheduleMode === 'fixedTime' && editingTask.days.length > 0 && editingTask.days.length < 7
      ? editingTask.days
      : WEEKDAY_PRESET_WEEKDAYS;
  return {
    name: editingTask?.name ?? '',
    prompt: editingTask?.prompt ?? '',
    projectPath: '', // renderTaskForm이 첫 렌더에서 실제 프로젝트 목록의 첫 항목으로 채운다
    interval: editingTask ? String(editingTask.interval) : '60',
    scheduleMode: editingTask && editingTask.scheduleMode !== 'interval' ? 'fixedTime' : 'interval',
    activeSubTab: initialFixedTimeSubTabFor(editingTask),
    hourlyMinute: editingTask?.hourlyMinute !== null && editingTask?.hourlyMinute !== undefined ? String(editingTask.hourlyMinute) : '0',
    time: editingTask?.atTime ?? '09:00',
    days: [...initialWeeklyDays],
    cron: editingTask?.cron ?? '',
    subagent: editingTask?.subagent ?? '',
    timeout: editingTask?.timeout !== null && editingTask?.timeout !== undefined ? String(editingTask.timeout) : '',
  };
}

// "새 자율업무" / "자율업무 수정" 겸용 모달 — editingTask가 null이면 추가 모드,
// 채워져 있으면 그 task를 프리필한 수정 모드다(제출 시 같은 id로 upsert).
// 목록 화면의 헤더 버튼과 상세 화면의 "수정" 버튼이 둘 다 이 모달을 연다 —
// 열림 상태를 이 화면 전용 값이라 전역 state까지 보낼 필요는 없다(config
// 모달과 달리 다른 화면과 공유되지 않으므로 모듈 로컬로 충분하다).
let taskFormModal: { readonly editingTask: AutonomousTask | null; readonly draft: TaskFormDraft } | null = null;
let taskFormModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachTaskFormModalEscHandler(): void {
  if (taskFormModalEscHandler) {
    window.removeEventListener('keydown', taskFormModalEscHandler);
    taskFormModalEscHandler = null;
  }
}

function openTaskFormModal(editingTask: AutonomousTask | null): void {
  taskFormModal = { editingTask, draft: buildInitialTaskFormDraft(editingTask) };
  notifyChange();
}

function closeTaskFormModal(): void {
  taskFormModal = null;
  detachTaskFormModalEscHandler();
  notifyChange();
}

// 목록/상세 렌더 함수 양쪽에서 공유하는 attach-and-render 헬퍼 — 열려 있지
// 않으면 ESC 리스너를 정리하고 null을 반환, 열려 있으면 필요 시 리스너를 붙이고
// 모달 엘리먼트를 반환한다.
function renderTaskFormModalIfOpen(): HTMLElement | null {
  if (!taskFormModal) {
    detachTaskFormModalEscHandler();
    return null;
  }
  if (!taskFormModalEscHandler) {
    taskFormModalEscHandler = (e) => {
      if (e.key === 'Escape') closeTaskFormModal();
    };
    window.addEventListener('keydown', taskFormModalEscHandler);
  }
  return renderTaskFormModal(taskFormModal.editingTask, taskFormModal.draft);
}

// 자율업무 화면을 완전히 떠날 때(다른 라우트로 이동) main.ts에서 호출한다 —
// 열려 있던 모달(전역 설정 / 추가·수정)이 있었다면 window에 남은 ESC 리스너를
// 정리한다.
export function leaveAutonomousTasksListView(): void {
  state.malgnAgentConfig.editingAutonomy = false;
  detachAutonomyConfigModalEscHandler();
  taskFormModal = null;
  detachTaskFormModalEscHandler();
}

// ---------------- (projectPath, taskId) 복합키 ----------------

function runtimeKey(projectPath: string, taskId: string): string {
  return `${projectPath}\0${taskId}`;
}

// ---------------- 데이터 로딩 + 표시용 변환 ----------------

// 기존 상대간격(interval) 문구 — 신규 모드 추가와 무관하게 그대로 유지한다.
function computeIntervalScheduleLabel(interval: number): string {
  if (interval >= 1440 && interval % 1440 === 0) return `이전 실행 완료 후 ${interval / 1440}일 뒤 재실행`;
  if (interval >= 60 && interval % 60 === 0) return `이전 실행 완료 후 ${interval / 60}시간 뒤 재실행`;
  return `이전 실행 완료 후 ${interval}분 뒤 재실행`;
}

// 고정시각 모드 문구 — "매일 09:00" / "매주 화·목 09:00"(설계서 §5 문구 형식).
// days가 비었거나 7개 전부면 "매일"로 접는다(정규화 규칙과 동일한 판단, 설계서 §3).
function computeFixedTimeScheduleLabel(atTime: string | null, days: readonly number[]): string {
  const time = atTime ?? '--:--';
  if (days.length === 0 || days.length >= 7) return `매일 ${time}`;
  const sortedLabels = [...days].sort((a, b) => a - b).map((d) => WEEKDAY_LABELS[d] ?? '?');
  return `매주 ${sortedLabels.join('·')} ${time}`;
}

// 매시간 모드 문구 — "매시 M분"(설계서 §6.4). hourlyMinute이 없으면(손편집 파일
// 등 이상 데이터) 재설정을 안내한다.
function computeHourlyScheduleLabel(hourlyMinute: number | null): string {
  if (hourlyMinute === null) return '스케줄을 다시 지정해 주세요';
  return `매시 ${hourlyMinute}분`;
}

// 사용자 지정(cron) 모드 문구 — 표현식 원문을 그대로 보여준다(설계서 §6.4).
function computeCronScheduleLabel(cron: string | null): string {
  if (!cron) return '스케줄을 다시 지정해 주세요';
  return cron;
}

function computeScheduleLabel(task: {
  scheduleMode: AutonomyScheduleMode;
  interval: number;
  atTime: string | null;
  days: readonly number[];
  hourlyMinute: number | null;
  cron: string | null;
}): string {
  if (task.scheduleMode === 'fixedTime') return computeFixedTimeScheduleLabel(task.atTime, task.days);
  if (task.scheduleMode === 'hourly') return computeHourlyScheduleLabel(task.hourlyMinute);
  if (task.scheduleMode === 'cron') return computeCronScheduleLabel(task.cron);
  return computeIntervalScheduleLabel(task.interval);
}

function formatElapsedSince(targetMs: number): string {
  const diffMin = Math.round((Date.now() - targetMs) / 60000);
  if (diffMin < 1) return '방금 전';
  if (diffMin < 60) return `${diffMin}분 전`;
  const diffHour = Math.round(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 전`;
  return `${Math.round(diffHour / 24)}일 전`;
}

function formatRelativeFromNow(targetMs: number): string {
  const diffMin = Math.round((targetMs - Date.now()) / 60000);
  if (diffMin <= 0) return '실행 대기 중';
  if (diffMin < 60) return `${diffMin}분 후`;
  const diffHour = Math.round(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 후`;
  return `${Math.round(diffHour / 24)}일 후`;
}

function computeLastRunLabel(running: boolean, lastStartedAt: string | null, lastFinishedAt: string | null): string {
  if (running) {
    if (!lastStartedAt) return '실행 중';
    const t = Date.parse(lastStartedAt);
    return Number.isNaN(t) ? '실행 중' : `실행 중 (시작 ${formatElapsedSince(t)})`;
  }
  if (!lastFinishedAt) return '없음';
  const t = Date.parse(lastFinishedAt);
  return Number.isNaN(t) ? lastFinishedAt : formatElapsedSince(t);
}

function computeNextRunLabel(nextRunAt: string | null, enabled: boolean, scheduleMode: AutonomyScheduleMode): string {
  if (!enabled) return '중지됨';
  if (!nextRunAt) {
    // 벽시계 기반 모드(fixedTime/hourly/cron)는 "등록 직후라 아직 계산 전"이
    // 아니라 스케줄 값 파싱 실패로 next_run_at이 영구히 None인 경우가 실제로
    // 있다(설계서 §4.5 #14, §6.4) — interval 모드의 "첫 실행 대기"와 다른
    // 문구로 구분한다. 네 모드 중 null이 정상인 모드는 interval뿐이다.
    return scheduleMode === 'fixedTime' || scheduleMode === 'hourly' || scheduleMode === 'cron' ? '실행 시각이 올바르지 않습니다' : '등록 후 첫 실행 대기';
  }
  const t = Date.parse(nextRunAt);
  if (Number.isNaN(t)) return '알 수 없음';
  return formatRelativeFromNow(t);
}

// 설정값(interval/enabled)과 런타임 값(running/nextRunAt 등)을 합쳐 화면 표시용
// 필드까지 계산한다. runtime이 null이면(아직 한 번도 tick을 안 돈 신규 task 등)
// 전부 "기록 없음" 계열 기본값으로 채운다 — 실제로 실행된 적이 있는 것처럼
// 거짓 표시하지 않는다.
function mergeRuntime(task: AutonomousTask, runtime: AutonomyRuntimeStatus | null): AutonomousTask {
  const running = runtime?.running ?? false;
  const lastStartedAt = runtime?.lastStartedAt ?? null;
  const lastFinishedAt = runtime?.lastFinishedAt ?? null;
  const nextRunAt = runtime?.nextRunAt ?? null;
  return {
    ...task,
    running,
    lastStartedAt,
    lastFinishedAt,
    nextRunAt,
    status: runtime?.status ?? null,
    summary: runtime?.summary ?? null,
    durationMs: runtime?.durationMs ?? null,
    logPath: runtime?.logPath ?? null,
    scheduleLabel: computeScheduleLabel(task),
    lastRunLabel: computeLastRunLabel(running, lastStartedAt, lastFinishedAt),
    nextRunLabel: computeNextRunLabel(nextRunAt, task.enabled, task.scheduleMode),
  };
}

function toDisplayTask(group: ProjectAutonomyGroup, task: AutonomyTaskConfig, runtime: AutonomyRuntimeStatus | null): AutonomousTask {
  // autonomyApi.ts의 atTime/days는 `?`로 선언돼 있다(옵셔널 불일치 함정 방지) —
  // 여기서 `?? null`/`?? []`로 명시 정규화해 화면 모델(AutonomousTask)은
  // 항상 비-옵셔널 필드를 갖도록 한다(설계서 §7).
  const scheduleMode = task.scheduleMode;
  const atTime = task.atTime ?? null;
  const days = task.days ?? [];
  const hourlyMinute = task.hourlyMinute ?? null;
  const cron = task.cron ?? null;
  return mergeRuntime(
    {
      id: task.id,
      projectPath: group.projectPath,
      projectName: group.projectName,
      name: task.name,
      prompt: task.prompt,
      subagent: task.subagent,
      interval: task.interval,
      scheduleMode,
      atTime,
      days,
      hourlyMinute,
      cron,
      enabled: task.enabled,
      timeout: task.timeout,
      running: false,
      lastStartedAt: null,
      lastFinishedAt: null,
      nextRunAt: null,
      status: null,
      summary: null,
      durationMs: null,
      logPath: null,
      scheduleLabel: computeScheduleLabel({ scheduleMode, interval: task.interval, atTime, days, hourlyMinute, cron }),
      lastRunLabel: '없음',
      nextRunLabel: '알 수 없음',
    },
    runtime
  );
}

export async function loadAutonomousTasks(): Promise<void> {
  ensureRuntimeWatcher();

  state.autonomousTasks.loading = true;
  state.autonomousTasks.error = null;
  notifyChange();
  try {
    // 설정(autonomy_list)과 런타임(autonomy_runtime_status)은 서로 다른 저장소
    // (파일 / 메모리)라 별도 커맨드다 — 병렬로 조회해 IPC 왕복을 한 번만 더 쓴다
    // (설계 §2).
    const [groups, runtimeList] = await Promise.all([fetchAutonomyTasks(), fetchAutonomyRuntimeStatus()]);
    const runtimeByKey = new Map<string, AutonomyRuntimeStatus>();
    for (const r of runtimeList) runtimeByKey.set(runtimeKey(r.projectPath, r.taskId), r);

    state.autonomousTasks.items = groups.flatMap((group) =>
      group.tasks.map((task) => toDisplayTask(group, task, runtimeByKey.get(runtimeKey(group.projectPath, task.id)) ?? null))
    );
    state.autonomousTasks.loaded = true;
  } catch (err) {
    // 전역 설정(malgn-agent.json)이 손상되면 autonomy_list가 Err를 던진다
    // (fail-closed) — 목록을 비우는 대신 오류 배너로 원인을 보여준다.
    state.autonomousTasks.error = err instanceof Error ? err.message : '자율업무 목록을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.autonomousTasks.loading = false;
    notifyChange();
  }

  // 전역 설정 표시(§5-3)는 자율업무 목록 조회와 독립적으로 갱신한다 — 목록
  // 조회가 실패해도 workspace 안내는 그대로 최신 상태를 유지한다.
  void loadMalgnAgentConfigStatus();
}

// ---------------- 전역 설정(malgn-agent.json) 상시 표시 + 편집 ----------------
// 설정 화면(views/settings.ts)이 아니라 이 화면에 둔 이유: 동시 실행 개수·
// 기본 타임아웃·로그 보존 일수가 이 화면이 실행하는 자율업무 자체의 동작
// 방식이라 "왜 이렇게 동작하지"를 바로 옆에서 설명해줄 수 있다. workspaces
// (프로젝트 스캔 범위) 편집은 더 이상 여기 없다 — 프로젝트 화면
// (views/projects.ts)의 헤더로 옮겨졌다.

export async function loadMalgnAgentConfigStatus(): Promise<void> {
  state.malgnAgentConfig.loading = true;
  notifyChange();
  try {
    state.malgnAgentConfig.status = await fetchMalgnAgentConfig();
    state.malgnAgentConfig.error = null;
    state.malgnAgentConfig.loaded = true;
  } catch (err) {
    // 계약상 malgn_agent_config_get은 항상 성공하고 실패를 ok:false로 표현하지만
    // (§11), Tauri IPC 자체를 못 쓰는 환경(플레인 브라우저) 대비 방어적으로
    // catch도 둔다.
    state.malgnAgentConfig.error = err instanceof Error ? err.message : '전역 설정을 불러오지 못했습니다.';
  } finally {
    state.malgnAgentConfig.loading = false;
    notifyChange();
  }
}

function renderConfigStatusBanner(): HTMLElement | null {
  const cfg = state.malgnAgentConfig;
  if (cfg.error) {
    return el('div', { className: 'alert' }, [
      el('span', {}, [`⚠ 전역 설정을 불러오지 못했습니다: ${cfg.error}`]),
      el('button', { className: 'btn', onClick: () => void loadMalgnAgentConfigStatus() }, ['다시 시도']),
    ]);
  }
  if (!cfg.status) {
    return cfg.loading ? el('div', { className: 'settings-form-hint' }, ['전역 설정(~/.claude/malgn-agent.json) 불러오는 중…']) : null;
  }

  const status = cfg.status;
  if (!status.ok) {
    return el('div', { className: 'alert' }, [
      el('span', {}, [
        `⚠ 전역 설정(${status.configPath}) 오류: ${status.error ?? '알 수 없는 오류'} — workspace가 0개로 처리되어 스캔·실행이 되지 않습니다.`,
      ]),
      el('button', { className: 'btn', onClick: () => void loadMalgnAgentConfigStatus() }, ['다시 시도']),
    ]);
  }

  const infoLine = el('div', { className: 'settings-form-hint' }, [
    status.workspaces.length > 0
      ? `감시 중인 workspace ${status.workspaces.length}개: ${status.workspaces.join(', ')} (${status.configPath}) — workspace 목록 편집은 "프로젝트" 화면에서 합니다.`
      : `감시 중인 workspace 0개 (${status.configPath}) — workspace 목록 편집은 "프로젝트" 화면에서 합니다.`,
  ]);

  const parts: HTMLElement[] = [infoLine];
  if (status.warnings.length > 0) {
    parts.push(el('div', { className: 'alert' }, [`⚠ 전역 설정 경고: ${status.warnings.join(' / ')}`]));
  }
  return el('div', {}, parts);
}

// 헤더의 "설정 편집" 버튼 — sessions.ts의 openMetaModal처럼 항상 열기만 하는
// 단일 버튼이다(닫기는 모달 자체의 X/배경클릭/ESC로만 한다). state.malgnAgentConfig.status가
// 로드돼 있고 에러가 없을 때만 노출한다(renderConfigStatusBanner가 error/null을
// 처리하는 조건과 동일).
function renderConfigEditToggleBtn(): HTMLElement | null {
  const cfg = state.malgnAgentConfig;
  if (cfg.error || !cfg.status || !cfg.status.ok) return null;
  return el(
    'button',
    {
      className: 'btn',
      onClick: () => {
        state.malgnAgentConfig.editingAutonomy = true;
        notifyChange();
      },
    },
    ['설정 편집']
  );
}

async function handleSaveMalgnAgentConfig(payload: MalgnAgentConfigInput): Promise<void> {
  state.malgnAgentConfig.saving = true;
  notifyChange();
  try {
    state.malgnAgentConfig.status = await saveMalgnAgentConfig(payload);
    closeAutonomyConfigModal();
    showToast('전역 설정을 저장했습니다.');
  } catch (err) {
    showToast(`전역 설정 저장에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.malgnAgentConfig.saving = false;
    notifyChange();
  }
}

// OTel 패널(views/settings.ts의 renderOtelPanel)과 같은 settings-form 계열
// 클래스를 재사용해 시각적으로 통일한다.
function renderConfigEditForm(status: MalgnAgentConfigStatus): HTMLElement {
  const { limits } = status;

  if (!autonomyConfigDraft) {
    autonomyConfigDraft = {
      concurrency: String(status.autonomy.concurrency),
      defaultTimeout: String(status.autonomy.defaultTimeout),
      retentionDays: String(status.logs.retentionDays),
    };
  }
  const draft = autonomyConfigDraft;

  const concurrencyInput = boundField(
    document.createElement('input'),
    () => draft.concurrency,
    (v) => {
      draft.concurrency = v;
    }
  );
  concurrencyInput.id = 'malgn-config-concurrency';
  concurrencyInput.className = 'settings-input';
  concurrencyInput.type = 'number';
  concurrencyInput.min = '1';
  concurrencyInput.max = String(limits.maxConcurrency);
  concurrencyInput.autocomplete = 'off';

  const defaultTimeoutInput = boundField(
    document.createElement('input'),
    () => draft.defaultTimeout,
    (v) => {
      draft.defaultTimeout = v;
    }
  );
  defaultTimeoutInput.id = 'malgn-config-default-timeout';
  defaultTimeoutInput.className = 'settings-input';
  defaultTimeoutInput.type = 'number';
  defaultTimeoutInput.min = String(limits.minTimeout);
  defaultTimeoutInput.max = String(limits.maxTimeout);
  defaultTimeoutInput.autocomplete = 'off';

  const retentionDaysInput = boundField(
    document.createElement('input'),
    () => draft.retentionDays,
    (v) => {
      draft.retentionDays = v;
    }
  );
  retentionDaysInput.id = 'malgn-config-retention-days';
  retentionDaysInput.className = 'settings-input';
  retentionDaysInput.type = 'number';
  retentionDaysInput.min = '1';
  retentionDaysInput.max = '365';
  retentionDaysInput.autocomplete = 'off';

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, [`전역 설정 파일(${status.configPath})을 직접 수정합니다.`]),
    el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['동시 실행 개수 (concurrency)']),
      concurrencyInput,
      el('div', { className: 'settings-form-hint' }, [`허용 범위: 1 ~ ${limits.maxConcurrency}`]),
    ]),
    el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['기본 타임아웃 (분)']),
      defaultTimeoutInput,
      el('div', { className: 'settings-form-hint' }, [`허용 범위: ${limits.minTimeout}분 ~ ${limits.maxTimeout}분`]),
    ]),
    el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['로그 보존 일수']),
      retentionDaysInput,
      el('div', { className: 'settings-form-hint' }, ['허용 범위: 1일 ~ 365일']),
    ]),
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    // 이 폼은 workspaces를 다루지 않는다(프로젝트 화면으로 이전) — 현재
    // 로드된 값을 그대로 실어 보내야 저장 시 workspaces가 날아가지 않는다
    // (저장 API가 4개 필드 전체를 항상 덮어쓰는 풀 오버라이트라서).
    const payload: MalgnAgentConfigInput = {
      workspaces: state.malgnAgentConfig.status?.workspaces ?? [],
      autonomy: {
        concurrency: Number(concurrencyInput.value),
        defaultTimeout: Number(defaultTimeoutInput.value),
      },
      logs: { retentionDays: Number(retentionDaysInput.value) },
    };
    void handleSaveMalgnAgentConfig(payload);
  });

  const saveBtn = el('button', { className: 'btn btn-primary', disabled: state.malgnAgentConfig.saving }, [
    state.malgnAgentConfig.saving ? '저장 중…' : '저장',
  ]);
  saveBtn.type = 'submit';
  const cancelBtn = el(
    'button',
    {
      className: 'btn',
      disabled: state.malgnAgentConfig.saving,
      onClick: closeAutonomyConfigModal,
    },
    ['취소']
  );
  cancelBtn.type = 'button';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn, cancelBtn]));

  return form;
}

// sessions.ts의 renderMetaModal과 동일한 구조(modal-overlay/modal-box/modal-header
// +modal-close-btn/modal-body) — 배경 클릭·ESC·닫기 버튼 3가지 경로로 닫힌다.
// 오버레이 생성 자체는 dom.ts의 createModalOverlay로 공용화했다(드래그 중 바깥으로
// 나가도 닫히지 않는 안전 처리 포함).
function renderConfigEditModal(status: MalgnAgentConfigStatus): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['자율업무 전역 설정']),
      el('button', { className: 'modal-close-btn', onClick: closeAutonomyConfigModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderConfigEditForm(status)]),
  ]);
  return createModalOverlay(modalBox, closeAutonomyConfigModal);
}

// ---------------- 실시간 런타임 갱신 (이벤트 구독, 실패 시 폴링 열화) ----------------
// 이벤트 이름은 기존 그대로 "autonomy-task-updated"를 쓴다(설계 §2) — payload가
// AutonomyRuntimeStatus 1건으로 바뀌었을 뿐이다. 앱이 켜져 있는 동안 딱 한 번만
// 구독한다(main.ts의 initLiveWatchers와 동일한 원칙이지만, main.ts를 건드리지
// 않기로 되어 있어 이 화면의 데이터 로딩 진입점(loadAutonomousTasks)에서 지연
// 초기화한다).

let runtimeWatcherInitialized = false;
let pollTimer: ReturnType<typeof setInterval> | null = null;

function applyRuntimeUpdate(update: AutonomyRuntimeStatus): void {
  const key = runtimeKey(update.projectPath, update.taskId);
  const idx = state.autonomousTasks.items.findIndex((t) => runtimeKey(t.projectPath, t.id) === key);
  if (idx === -1) return; // 아직 설정 목록에 없는 task(신규 등록 직후 등) — 다음 전체 재조회 때 합류한다.
  state.autonomousTasks.items[idx] = mergeRuntime(state.autonomousTasks.items[idx], update);
  // "지금 실행" 직후 새 이력이 상세 화면에 바로 반영되도록, 런타임 상태가
  // 갱신될 때마다 이력도 함께 재조회한다(설계 §7). 상세 화면을 아직 연 적
  // 없는 task는 historyCache에 항목이 없으므로 refreshHistoryIfTracked가
  // 아무 일도 하지 않는다 — 방문한 적 없는 task까지 미리 불러오지 않는다.
  refreshHistoryIfTracked(update.projectPath, update.taskId);
  notifyChange();
}

function ensureRuntimeWatcher(): void {
  if (runtimeWatcherInitialized) return;
  runtimeWatcherInitialized = true;

  onAutonomyRuntimeChanged((update) => applyRuntimeUpdate(update)).catch(() => {
    // Tauri IPC 브리지가 없는 환경(플레인 브라우저) — 이벤트 구독이 실패하면
    // "실시간"이라고 거짓 표시하지 않고 30초 폴링으로 열화한다(sessionsApi.ts가
    // live 플래그를 다루는 것과 같은 원칙: 실패를 숨기지 않는다).
    if (pollTimer !== null) return;
    pollTimer = setInterval(() => {
      if (!state.autonomousTasks.loading) {
        void loadAutonomousTasks();
        // 이벤트 구독이 안 되는 환경에서는 폴링이 유일한 갱신 경로다 — 이미
        // 상세 화면에서 열어본 적 있는(historyCache에 있는) task들의 이력도
        // 함께 재조회한다(설계 §7 "폴백 폴링 시 이력도 함께 재조회").
        for (const key of historyCache.keys()) {
          const sep = key.indexOf('\0');
          if (sep === -1) continue;
          refreshHistoryIfTracked(key.slice(0, sep), key.slice(sep + 1));
        }
      }
    }, 30000);
  });
}

async function handleToggleTask(task: AutonomousTask): Promise<void> {
  const next = !task.enabled;
  try {
    await setAutonomyTaskEnabled(task.projectPath, task.id, next);
    task.enabled = next;
    task.nextRunLabel = computeNextRunLabel(task.nextRunAt, task.enabled, task.scheduleMode);
    showToast(`${task.name} ${next ? '활성화' : '비활성화'}됨`);
  } catch (err) {
    showToast(err instanceof Error ? err.message : '상태 변경에 실패했습니다');
  } finally {
    notifyChange();
  }
}

// "지금 실행" 클릭~응답 사이의 짧은 창을 막는 순수 UI 상태(전역 state가 아니라
// 모듈 로컬로 충분하다 — 다른 화면과 공유되지 않는다). task.running은 백엔드
// 런타임 상태가 반영된 *이후*에만 true가 되므로, 요청을 보낸 직후~응답 도착
// 사이에는 아직 false일 수 있어 이 Set으로 1차 방어한다(백엔드의 "이미 실행
// 중" 거부가 최종 방어선).
const runNowPending = new Set<string>();

async function handleRunNow(task: AutonomousTask): Promise<void> {
  const key = runtimeKey(task.projectPath, task.id);
  if (task.running || runNowPending.has(key)) return;
  runNowPending.add(key);
  notifyChange();
  try {
    await runAutonomyTaskNow(task.projectPath, task.id);
    showToast(`"${task.name}" 실행을 요청했습니다`);
    // 실행 시작 반영은 이 화면이 이미 구독 중인 런타임 채널(이벤트 또는 폴링
    // 열화, ensureRuntimeWatcher)이 처리한다 — 여기서 별도로 상태를 조회하거나
    // 새 폴링을 만들지 않는다.
  } catch (err) {
    // 백엔드 계약: 실패 시 한국어 에러 메시지로 reject된다 — 그 문구를 그대로
    // 토스트에 보여준다(프론트에서 문구를 새로 짓지 않는다).
    showToast(err instanceof Error ? err.message : String(err));
  } finally {
    runNowPending.delete(key);
    notifyChange();
  }
}

async function handleDeleteTask(task: AutonomousTask, afterDelete?: () => void): Promise<void> {
  if (!(await confirmDialog(`"${task.name}" 자율업무를 삭제할까요? 되돌릴 수 없습니다.`, { danger: true }))) return;
  try {
    await deleteAutonomyTask(task.projectPath, task.id);
    state.autonomousTasks.items = state.autonomousTasks.items.filter((t) => !(t.projectPath === task.projectPath && t.id === task.id));
    showToast(`"${task.name}" 자율업무가 삭제되었습니다`);
    afterDelete?.();
  } catch (err) {
    showToast(err instanceof Error ? err.message : '삭제에 실패했습니다');
  } finally {
    notifyChange();
  }
}

// cron 입력의 가벼운 즉시 피드백 — 보안 방어선이 아니라 UX 편의다(설계서
// §7.4, 지시서 필수 요구사항). 빈 값/필드 개수만 본다 — 정교한 cron 파서는
// 만들지 않는다(신규 의존성 추가 금지이기도 하다). 실제 문법 검증은 저장
// 시점에 백엔드가 하고, 그 한국어 에러 메시지를 그대로 노출한다.
function validateCronLight(expr: string): string | null {
  if (!expr) return 'cron 표현식을 입력하세요.';
  const fields = expr.split(/\s+/).filter((f) => f.length > 0);
  if (fields.length !== 5) return 'cron 표현식은 5칸(분 시 일 월 요일)으로 적어야 합니다.';
  return null;
}

function tabsRow(active: 'list' | 'board'): HTMLElement {
  return el('div', { className: 'filter-group' }, [
    el('button', { className: `filter-btn${active === 'list' ? ' active' : ''}`, onClick: () => navigate('#/tasks') }, ['목록']),
    el('button', { className: `filter-btn${active === 'board' ? ' active' : ''}`, onClick: () => navigate('#/tasks/board') }, ['진행상황판']),
  ]);
}

// ---------------- 목록 탭 ----------------

export function renderAutonomousTasksListView(): HTMLElement {
  const addBtn = el('button', { className: 'btn btn-primary', onClick: () => openTaskFormModal(null) }, ['+ 새 자율업무']);
  const configEditToggleBtn = renderConfigEditToggleBtn();

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['자율업무']),
      el('div', { className: 'page-subtitle' }, [`등록된 자율업무 ${state.autonomousTasks.items.length}개`]),
    ]),
    el('div', { className: 'devtool-header-actions' }, [addBtn, ...(configEditToggleBtn ? [configEditToggleBtn] : [])]),
  ]);

  const body: HTMLElement[] = [];
  const configBanner = renderConfigStatusBanner();
  if (configBanner) body.push(configBanner);
  body.push(tabsRow('list'));

  if (state.autonomousTasks.loading && state.autonomousTasks.items.length === 0) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]));
  } else if (state.autonomousTasks.error) {
    const retry = el('button', { className: 'btn', onClick: () => void loadAutonomousTasks() }, ['다시 시도']);
    body.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.autonomousTasks.error}`]), retry]));
  } else if (state.autonomousTasks.items.length === 0) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['등록된 자율업무가 없습니다']),
        el('div', { className: 'state-block-desc' }, ['"+ 새 자율업무"로 프로젝트별 자율업무를 등록하세요.']),
      ])
    );
  } else {
    body.push(el('div', { className: 'task-list' }, state.autonomousTasks.items.map(renderTaskRow)));
  }

  const rootChildren: HTMLElement[] = [header, ...body];
  if (state.malgnAgentConfig.editingAutonomy && state.malgnAgentConfig.status?.ok) {
    if (!autonomyConfigModalEscHandler) {
      autonomyConfigModalEscHandler = (e) => {
        if (e.key === 'Escape') closeAutonomyConfigModal();
      };
      window.addEventListener('keydown', autonomyConfigModalEscHandler);
    }
    rootChildren.push(renderConfigEditModal(state.malgnAgentConfig.status));
  } else {
    detachAutonomyConfigModalEscHandler();
  }

  const taskFormModalEl = renderTaskFormModalIfOpen();
  if (taskFormModalEl) rootChildren.push(taskFormModalEl);

  return el('div', {}, rootChildren);
}

function renderTaskRow(task: AutonomousTask): HTMLElement {
  const goToDetail = (): void => navigate(`#/tasks/item/${encodeURIComponent(task.id)}`);

  const main = el(
    'div',
    {
      className: 'task-row-main',
      onClick: goToDetail,
      onKeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') goToDetail();
      },
    },
    [
      el('div', { className: 'task-row-top' }, [
        el('span', { className: 'task-row-name' }, [task.name]),
        el('span', { className: `badge ${task.running ? 'badge-active' : task.enabled ? 'badge-unknown' : 'badge-archived'}` }, [
          task.running ? '실행 중' : task.enabled ? '대기 중' : '중지됨',
        ]),
        el('span', { className: 'badge badge-unknown' }, [task.projectName]),
      ]),
      el('div', { className: 'task-row-desc' }, [task.prompt]),
      el('div', { className: 'task-row-schedule' }, [
        el('span', {}, [task.scheduleLabel]),
        el('span', {}, [`마지막 실행: ${task.lastRunLabel}`]),
        el('span', {}, [`다음 실행: ${task.nextRunLabel}`]),
      ]),
    ]
  );
  main.setAttribute('role', 'button');
  main.setAttribute('tabindex', '0');

  const editBtn = el('button', { className: 'btn', onClick: () => openTaskFormModal(task) }, ['수정']);
  const deleteBtn = el('button', { className: 'btn', onClick: () => void handleDeleteTask(task) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';

  return el('div', { className: 'task-row' }, [
    main,
    el('div', { className: 'task-row-actions' }, [toggleSwitch(task.enabled, () => void handleToggleTask(task)), editBtn, deleteBtn]),
  ]);
}

// 자율업무는 이 앱(맑은에이전트)이 켜져 있는 동안 띄운 백그라운드 스레드에서만
// 돈다 — OS 스케줄러/launchd 연동은 의도적으로 범위 밖이다(src-tauri/src/autonomy/mod.rs
// 상단 주석). 클로드 앱의 "온라인 상태" 조건과 달리 이 제품은 로컬 claude CLI를
// 실행할 뿐이라 그 문구를 그대로 쓰지 않는다 — 실제 제약(앱 실행 여부)만 안내한다.
// 기존 배너 컴포넌트(.alert)를 재사용한다 — 이 화면이 이미 에러/경고에 쓰는
// 유일한 박스형 배너라, 새 스타일을 만들지 않고 그대로 가져다 쓴다.
function renderAppMustBeRunningBanner(): HTMLElement {
  return el('div', { className: 'alert' }, [
    el('span', {}, [
      '자율업무는 맑은에이전트 앱이 실행 중인 동안에만 동작합니다. 앱을 종료한 상태에서는 예약된 시각이 지나도 실행되지 않습니다(OS 스케줄러 연동 없음).',
    ]),
  ]);
}

// 추가/수정 겸용 폼 — editingTask가 있으면 그 값으로 프리필하고 제출 시 같은
// id·projectPath로 upsert한다(project는 바꿀 수 없다 — 바꾸면 원래 프로젝트의
// autonomy.json에 고아 항목이 남고 다른 쪽엔 중복 항목이 생기므로, 그 경우엔
// 삭제 후 새로 등록하도록 안내한다).
function renderTaskForm(editingTask: AutonomousTask | null, draft: TaskFormDraft): HTMLElement {
  const nameInput = boundField(
    document.createElement('input'),
    () => draft.name,
    (v) => {
      draft.name = v;
    }
  );
  nameInput.className = 'settings-input';
  nameInput.placeholder = '예: 야간 프로젝트 상태 점검';

  const promptInput = boundField(
    document.createElement('textarea'),
    () => draft.prompt,
    (v) => {
      draft.prompt = v;
    }
  );
  promptInput.className = 'settings-input task-form-textarea';
  promptInput.placeholder = '이 자율업무가 실제로 실행될 때 claude에게 전달할 지시문을 구체적으로 적으세요';
  promptInput.rows = 4;

  let projectSelect: HTMLSelectElement | null = null;
  let projectField: HTMLElement;
  if (editingTask) {
    const projectDisplay = document.createElement('input');
    projectDisplay.className = 'settings-input';
    projectDisplay.value = editingTask.projectName;
    projectDisplay.disabled = true;
    projectField = el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['프로젝트']),
      projectDisplay,
      el('div', { className: 'settings-form-hint' }, ['등록된 프로젝트는 수정할 수 없습니다. 다른 프로젝트로 옮기려면 삭제 후 새로 등록하세요.']),
    ]);
  } else {
    projectSelect = boundField(
      document.createElement('select'),
      () => draft.projectPath,
      (v) => {
        draft.projectPath = v;
      },
      'change'
    );
    projectSelect.className = 'settings-input';
    const projects = state.dashboard.projects;
    const projectFieldChildren: HTMLElement[] = [el('span', { className: 'settings-field-label' }, ['프로젝트']), projectSelect];
    if (projects.length === 0) {
      const placeholderOption = document.createElement('option');
      placeholderOption.value = '';
      placeholderOption.textContent = state.dashboard.loading ? '프로젝트 목록 불러오는 중…' : '등록된 프로젝트가 없습니다';
      projectSelect.appendChild(placeholderOption);
      projectSelect.disabled = true;
      // 정작 사용자가 "프로젝트가 없다"를 실제로 겪은 화면이 여기다 — projects.ts/
      // sessions.ts의 빈 상태 안내와 같은 문구·톤을 재사용한다(hub 이슈
      // 01m2wm499xmh3046rnvx4cyn8n). 불러오는 중일 때는 조건 설명 대신 로딩
      // 안내만 이미 있으므로 중복 표시하지 않는다.
      if (!state.dashboard.loading) {
        projectFieldChildren.push(el('div', { className: 'settings-form-hint' }, [describeWorkspaceScanScope()]));
        const skipSummary = summarizeSkippedProjects(state.dashboard.skipped);
        if (skipSummary) {
          projectFieldChildren.push(el('div', { className: 'settings-form-hint' }, [skipSummary]));
        }
      }
    } else {
      for (const project of projects) {
        const optionEl = document.createElement('option');
        optionEl.value = project.path;
        optionEl.textContent = project.name;
        projectSelect.appendChild(optionEl);
      }
      // 드래프트가 아직 프로젝트를 고르지 않았거나(최초 오픈), 이전에 고른
      // 프로젝트가 지금 로드된 목록에 더 이상 없으면(드문 경합) 첫 항목으로
      // 되돌린다 — 이전에는 <select> 기본 동작(첫 옵션 자동 선택)에만 기대던
      // 초기 선택을, 재렌더에서도 그대로 유지되도록 드래프트에 명시적으로 써둔다.
      if (!projects.some((p) => p.path === draft.projectPath)) {
        draft.projectPath = projects[0].path;
      }
    }
    projectSelect.value = draft.projectPath;
    projectField = el('label', { className: 'settings-field' }, projectFieldChildren);
  }

  const intervalSelect = boundField(
    document.createElement('select'),
    () => draft.interval,
    (v) => {
      draft.interval = v;
    },
    'change'
  );
  intervalSelect.className = 'settings-input';
  for (const opt of INTERVAL_OPTIONS) {
    const optionEl = document.createElement('option');
    optionEl.value = opt.value;
    optionEl.textContent = opt.label;
    intervalSelect.appendChild(optionEl);
  }
  intervalSelect.value = draft.interval;
  const intervalHint = el('div', { className: 'settings-form-hint' }, [
    '이전 실행이 끝난 뒤(시작 시점이 아니라 완료 시점 기준) 이 시간만큼 지나야 다시 실행됩니다.',
  ]);
  const intervalFields = el('div', {}, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 주기']), intervalSelect, intervalHint]),
  ]);

  // ---- 실행 방식(스케줄 모드) 라디오 + 고정 시간 하위 탭 ----
  // 1단 라디오(상대간격/고정시간)는 기존 구조 그대로 유지한다 — 상대간격
  // 선택 시 동작·문구는 아래 두 요소(intervalSelect/intervalFields)를 그대로
  // 재사용하고 손대지 않는다(지시서 금지 범위). 고정시간 선택 시에만 그 아래
  // 2단 하위 탭(매시간/매일/매주/사용자 지정, 설계서 §2.2)이 나타난다.
  const modeIntervalRadio = boundChecked(
    document.createElement('input'),
    () => draft.scheduleMode === 'interval',
    (checked) => {
      if (checked) draft.scheduleMode = 'interval';
    }
  );
  modeIntervalRadio.type = 'radio';
  modeIntervalRadio.name = 'task-form-schedule-mode';
  modeIntervalRadio.value = 'interval';

  const modeFixedRadio = boundChecked(
    document.createElement('input'),
    () => draft.scheduleMode === 'fixedTime',
    (checked) => {
      if (checked) draft.scheduleMode = 'fixedTime';
    }
  );
  modeFixedRadio.type = 'radio';
  modeFixedRadio.name = 'task-form-schedule-mode';
  modeFixedRadio.value = 'fixedTime';

  const modeField = el('div', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 방식']),
    el('div', { className: 'filter-group' }, [
      el('label', {}, [modeIntervalRadio, ' 상대 간격 (이전 실행 완료 후 N분 뒤)']),
      el('label', {}, [modeFixedRadio, ' 고정 시간 (매시간/매일/매주/사용자 지정)']),
    ]),
  ]);

  let activeSubTab: FixedTimeSubTab = draft.activeSubTab;

  const SUB_TAB_DEFS: readonly { readonly key: FixedTimeSubTab; readonly label: string }[] = [
    { key: 'hourly', label: '매시간' },
    { key: 'daily', label: '매일' },
    { key: 'weekly', label: '매주' },
    { key: 'custom', label: '사용자 지정' },
  ];
  const subTabButtons: HTMLButtonElement[] = SUB_TAB_DEFS.map((def) => {
    const btn = el('button', { className: `filter-btn${activeSubTab === def.key ? ' active' : ''}`, onClick: () => setActiveSubTab(def.key) }, [
      def.label,
    ]);
    btn.type = 'button';
    return btn;
  });
  const subTabsRow = el('div', { className: 'filter-group' }, subTabButtons);

  // 매시간 탭 — 분(0~59) 1개. 벽시계 정각 기준이라 "상대 간격 60분"과
  // 의미가 다르다는 점을 힌트 문구로 고지한다(지시서 필수 요구사항).
  const hourlyMinuteInput = boundField(
    document.createElement('input'),
    () => draft.hourlyMinute,
    (v) => {
      draft.hourlyMinute = v;
    }
  );
  hourlyMinuteInput.type = 'number';
  hourlyMinuteInput.min = '0';
  hourlyMinuteInput.max = '59';
  hourlyMinuteInput.className = 'settings-input';
  hourlyMinuteInput.autocomplete = 'off';
  const hourlyMinuteField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 분 (매시)']),
    hourlyMinuteInput,
    el('div', { className: 'settings-form-hint' }, [
      '매시 이 분에 실행합니다(예: 30 → 1:30, 2:30…). 상대 간격 60분과 달리 벽시계 정각에 맞춰 실행합니다.',
    ]),
  ]);

  // <input type="time">의 value는 브라우저 로케일과 무관하게 항상 "HH:MM"
  // 24시간 형식이다 — 백엔드 parse_hhmm과 계약이 정확히 맞으므로 별도
  // 파서·입력 마스킹을 두지 않는다(설계서 §7 필수 계약). 매일/매주 두 탭이
  // 이 입력 하나를 공유한다 — 둘 다 "실행 시각"이라는 같은 의미이므로 탭을
  // 오가도 값이 재해석되지 않는다(지시서 필수 요구사항 2).
  const timeInput = boundField(
    document.createElement('input'),
    () => draft.time,
    (v) => {
      draft.time = v;
    }
  );
  timeInput.type = 'time';
  timeInput.className = 'settings-input';
  const timeField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 시각']),
    timeInput,
    el('div', { className: 'settings-form-hint' }, ['기기의 현재 시간대 기준 벽시계 시각입니다(예: 09:00 = 오전 9시). 기기 시간대가 바뀌면 다음 계산부터 새 시간대의 같은 시각을 따릅니다.']),
  ]);

  // 요일 인덱스는 0=일요일…6=토요일(백엔드와 동일 축, 설계서 §3) — 순서를
  // 바꾸지 않는다. "매일" 프리셋은 독립 탭으로 승격되어 불필요해졌으므로
  // 없앴다(지시서의 PM 판단) — 평일/주말 프리셋만 매주 탭 전용으로 남긴다.
  // 새로 추가할 때는 평일(월~금)을 기본값으로 보여준다.
  const dayCheckboxes: HTMLInputElement[] = WEEKDAY_LABELS.map((_, idx) => {
    const cb = boundChecked(
      document.createElement('input'),
      () => draft.days.includes(idx),
      (checked) => {
        draft.days = checked ? [...new Set([...draft.days, idx])] : draft.days.filter((d) => d !== idx);
      }
    );
    cb.type = 'checkbox';
    cb.value = String(idx);
    return cb;
  });
  function setDayCheckboxes(days: readonly number[]): void {
    draft.days = [...days];
    dayCheckboxes.forEach((cb, idx) => {
      cb.checked = draft.days.includes(idx);
    });
  }
  const dayPresetWeekdaysBtn = el('button', { className: 'filter-btn', onClick: () => setDayCheckboxes(WEEKDAY_PRESET_WEEKDAYS) }, ['평일']);
  dayPresetWeekdaysBtn.type = 'button';
  const dayPresetWeekendBtn = el('button', { className: 'filter-btn', onClick: () => setDayCheckboxes(WEEKDAY_PRESET_WEEKEND) }, ['주말']);
  dayPresetWeekendBtn.type = 'button';
  const daysField = el('div', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 요일']),
    el('div', { className: 'filter-group' }, [dayPresetWeekdaysBtn, dayPresetWeekendBtn]),
    el(
      'div',
      { className: 'filter-group' },
      dayCheckboxes.map((cb, idx) => el('label', {}, [cb, ` ${WEEKDAY_LABELS[idx]}`]))
    ),
    el('div', { className: 'settings-form-hint' }, ['최소 한 요일을 선택하세요.']),
  ]);

  // 사용자 지정(cron) 탭 — 표준 5필드 표현식 원문. 프론트 검증은 UX 편의일
  // 뿐이라(백엔드가 이중 검증) 빈 값·필드 개수만 가볍게 본다(validateCronLight).
  // 실패 시 인라인 에러(cronError)를 필드 아래에 표시하고, 저장 시점에
  // 백엔드가 거부하면 그 한국어 메시지를 그대로 여기에 노출한다(프론트에서
  // 새 문구를 짓지 않는다, 지시서 필수 요구사항).
  const cronInput = boundField(
    document.createElement('input'),
    () => draft.cron,
    (v) => {
      draft.cron = v;
    }
  );
  cronInput.type = 'text';
  cronInput.className = 'settings-input';
  cronInput.placeholder = '0 9 * * 1-5 (평일 오전 9시)';
  cronInput.autocomplete = 'off';
  const cronError = el('div', { className: 'settings-field-error' }, ['']);
  cronInput.addEventListener('input', () => {
    cronError.textContent = '';
  });
  const cronField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['cron 표현식']),
    cronInput,
    el('div', { className: 'settings-form-hint' }, [
      '분 시 일 월 요일, 표준 5칸입니다. 예: 0 9 * * 1-5 (평일 오전 9시). 요일은 0~6이며 일요일은 0입니다(7은 쓸 수 없습니다). ' +
        '요일 3글자 약어(MON~SUN, 대소문자 무관)와 MON-FRI 같은 범위는 사용할 수 있습니다. ' +
        '월 이름(JAN 등)과 요일 풀네임(Sunday 등), @daily 같은 축약형, L·?·1#2, 역방향 범위(FRI-MON)는 지원하지 않습니다. ' +
        'DST(서머타임) 전환으로 그 시각이 사라지는 날에는 그날 회차가 실행되지 않고 건너뛰어집니다. ' +
        '일(day)과 요일을 동시에 좁히면 둘 다 맞는 날에만 실행됩니다(AND).',
    ]),
    cronError,
  ]);

  const fixedTimeFields = el('div', {}, [subTabsRow, hourlyMinuteField, timeField, daysField, cronField]);

  function setActiveSubTab(tab: FixedTimeSubTab): void {
    activeSubTab = tab;
    draft.activeSubTab = tab;
    subTabButtons.forEach((btn, idx) => btn.classList.toggle('active', SUB_TAB_DEFS[idx].key === tab));
    updateSubTabVisibility();
  }

  // 하위 탭 전환은 지역적 표시/숨김만 하고 전체 재렌더를 트리거하지 않는다
  // (지시서 필수 요구사항 3 — 이 프로젝트의 전체 재렌더 구조에서 포커스가
  // body로 리셋되는 것을 악화시키지 않기 위함).
  function updateSubTabVisibility(): void {
    hourlyMinuteField.style.display = activeSubTab === 'hourly' ? '' : 'none';
    timeField.style.display = activeSubTab === 'daily' || activeSubTab === 'weekly' ? '' : 'none';
    daysField.style.display = activeSubTab === 'weekly' ? '' : 'none';
    cronField.style.display = activeSubTab === 'custom' ? '' : 'none';
  }

  const subagentInput = boundField(
    document.createElement('input'),
    () => draft.subagent,
    (v) => {
      draft.subagent = v;
    }
  );
  subagentInput.className = 'settings-input';
  subagentInput.placeholder = '예: malgn-agent:qa-engineer (비워두면 지정 안 함)';

  const limits = state.malgnAgentConfig.status?.limits ?? null;
  const defaultTimeout = state.malgnAgentConfig.status?.autonomy.defaultTimeout ?? null;
  const timeoutInput = boundField(
    document.createElement('input'),
    () => draft.timeout,
    (v) => {
      draft.timeout = v;
    }
  );
  timeoutInput.className = 'settings-input';
  timeoutInput.type = 'number';
  timeoutInput.min = String(limits?.minTimeout ?? 1);
  timeoutInput.max = String(limits?.maxTimeout ?? 480);
  timeoutInput.placeholder = defaultTimeout !== null ? `비우면 전역 기본값(${defaultTimeout}분) 사용` : '비우면 전역 기본값 사용';
  const timeoutHint = el('div', { className: 'settings-form-hint' }, [
    limits ? `선택 항목입니다. 범위: ${limits.minTimeout}~${limits.maxTimeout}분.` : '선택 항목입니다 — 비우면 전역 기본값을 사용합니다.',
  ]);

  // startupHint는 모드별로 문구가 갈리고, 고정시각 모드의 따라잡기 창(분)은
  // 절대 하드코딩하지 않는다 — state.malgnAgentConfig.status.limits.missedRunGraceMinutes에서
  // 읽는다. missedRunGraceMinutes 자체는 비-옵셔널이지만 `status`는 아직 설정을
  // 로드하지 못했거나(초기 상태) 로드에 실패했을 때 null일 수 있다 — 그 경우
  // 숫자 없는 일반 문구로 대체한다(startupGrace와 동일한 이유).
  const startupGrace = state.malgnAgentConfig.status?.limits.startupGraceMinutes ?? null;
  const missedRunGraceMinutes = state.malgnAgentConfig.status?.limits.missedRunGraceMinutes ?? null;
  const startupHint = el('div', { className: 'settings-form-hint' }, ['']);
  function updateStartupHint(): void {
    if (modeFixedRadio.checked) {
      startupHint.textContent =
        missedRunGraceMinutes !== null
          ? `앱이 꺼져 있던 동안 지난 회차는 실행하지 않습니다. 단 최근 ${missedRunGraceMinutes}분 이내에 지난 회차는 앱을 켠 뒤 1회 실행합니다.`
          : '앱이 꺼져 있던 동안 지난 회차는 실행하지 않습니다. 단 최근에 지난 회차 1건은 앱을 켠 뒤 1회 실행합니다.';
    } else {
      startupHint.textContent =
        startupGrace !== null
          ? `등록 후 약 ${startupGrace}분 뒤 첫 실행됩니다(앱을 새로 시작한 직후에도 동일합니다 — 재시작 즉시 전체 자율업무가 몰려 실행되는 것을 막기 위한 유예 시간입니다).`
          : '등록 직후 바로 실행되지 않고, 짧은 유예 시간 뒤에 첫 실행됩니다.';
    }
  }

  // 라디오 선택에 따라 상대간격/고정시간 전용 필드 블록을 토글하고 힌트
  // 문구를 갱신한다. 고정시간 4개 하위 탭은 모두 같은 따라잡기 문구를 쓴다
  // (updateStartupHint는 modeFixedRadio.checked만 보고 하위 탭은 보지 않는다).
  function updateScheduleModeVisibility(): void {
    const isFixed = modeFixedRadio.checked;
    intervalFields.style.display = isFixed ? 'none' : '';
    fixedTimeFields.style.display = isFixed ? '' : 'none';
    updateSubTabVisibility();
    updateStartupHint();
  }
  modeIntervalRadio.addEventListener('change', updateScheduleModeVisibility);
  modeFixedRadio.addEventListener('change', updateScheduleModeVisibility);
  updateScheduleModeVisibility();

  const form = el('form', { className: 'settings-form task-add-form' }, [
    renderAppMustBeRunningBanner(),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프롬프트']), promptInput]),
    projectField,
    modeField,
    intervalFields,
    fixedTimeFields,
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['서브에이전트 (선택)']), subagentInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['타임아웃 (분, 선택)']), timeoutInput, timeoutHint]),
    ...(editingTask ? [] : [startupHint]),
  ]);

  const saveBtn = el('button', { className: 'btn btn-primary' }, [editingTask ? '저장' : '추가']);
  saveBtn.type = 'submit';
  const cancelBtn = el('button', { className: 'btn', onClick: closeTaskFormModal }, ['취소']);
  cancelBtn.type = 'button';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn, cancelBtn]));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = nameInput.value.trim();
    const prompt = promptInput.value.trim();
    const projectPath = editingTask ? editingTask.projectPath : (projectSelect?.value ?? '');
    if (!name || !prompt || !projectPath) {
      showToast('이름, 프롬프트, 프로젝트를 모두 입력하세요');
      return;
    }

    const timeoutRaw = timeoutInput.value.trim();
    let timeout: number | null = null;
    if (timeoutRaw) {
      const parsed = Number(timeoutRaw);
      if (!Number.isInteger(parsed) || parsed <= 0) {
        showToast('타임아웃은 1 이상의 정수(분)로 입력하세요.');
        return;
      }
      timeout = parsed;
    }

    // 제출 검증 + scheduleMode 결정: 1단 라디오(상대간격/고정시간) × 2단
    // 하위 탭(매시간/매일/매주/사용자 지정) → 백엔드 모드 4개(설계서 §2.2
    // 매핑표). "선택된 탭이 소유한 필드만 보낸다"(지시서 필수 요구사항 2) —
    // 소유하지 않는 필드는 전부 null/[]로 비워 보낸다.
    let scheduleMode: AutonomyScheduleMode = 'interval';
    let atTime: string | null = null;
    let days: number[] = [];
    let hourlyMinute: number | null = null;
    let cron: string | null = null;

    if (modeFixedRadio.checked) {
      if (activeSubTab === 'hourly') {
        scheduleMode = 'hourly';
        const raw = hourlyMinuteInput.value.trim();
        const parsed = Number(raw);
        if (raw === '' || !Number.isInteger(parsed) || parsed < 0 || parsed > 59) {
          showToast('매시간 모드는 0~59 사이의 분을 지정해야 합니다.');
          return;
        }
        hourlyMinute = parsed;
      } else if (activeSubTab === 'custom') {
        scheduleMode = 'cron';
        const rawCron = cronInput.value.trim();
        const lightError = validateCronLight(rawCron);
        if (lightError) {
          cronError.textContent = lightError;
          return;
        }
        cron = rawCron;
      } else {
        // daily | weekly — 둘 다 백엔드 모드는 fixedTime을 공유한다(§2.2).
        // 매일은 days=[](§2.3 역매핑과 대칭되는 저장 형태), 매주는 체크된
        // 요일을 그대로 보낸다.
        scheduleMode = 'fixedTime';
        atTime = timeInput.value || null;
        if (activeSubTab === 'weekly') {
          days = dayCheckboxes.reduce<number[]>((acc, cb, idx) => {
            if (cb.checked) acc.push(idx);
            return acc;
          }, []);
          if (!atTime || days.length === 0) {
            showToast('매주 탭은 실행 시각과 최소 한 요일을 선택해야 합니다.');
            return;
          }
        } else {
          days = [];
          if (!atTime) {
            showToast('매일 탭은 실행 시각을 선택해야 합니다.');
            return;
          }
        }
      }
    }

    const task: AutonomyTaskConfig = {
      id: editingTask ? editingTask.id : crypto.randomUUID(),
      name,
      prompt,
      subagent: subagentInput.value.trim() || null,
      interval: Number(intervalSelect.value),
      scheduleMode,
      atTime,
      days,
      hourlyMinute,
      cron,
      enabled: editingTask ? editingTask.enabled : true,
      timeout,
    };

    saveBtn.disabled = true;
    void (async () => {
      try {
        await saveAutonomyTask(projectPath, task);
        showToast(editingTask ? `"${name}" 자율업무를 수정했습니다` : `"${name}" 자율업무가 추가되었습니다`);
        closeTaskFormModal();
        await loadAutonomousTasks();
      } catch (err) {
        // cron 탭은 백엔드가 돌려준 한국어 에러 메시지를 필드 아래(cronError)에
        // 그대로 노출한다(지시서 필수 요구사항 — 프론트에서 문구를 새로 짓지
        // 않는다). 그 외 탭은 기존과 동일하게 토스트로 보여준다.
        const message = err instanceof Error ? err.message : '자율업무 저장에 실패했습니다';
        if (scheduleMode === 'cron') {
          cronError.textContent = message;
        } else {
          showToast(message);
        }
        saveBtn.disabled = false;
        notifyChange();
      }
    })();
  });

  return form;
}

// sessions.ts의 renderMetaModal / 이 파일의 renderConfigEditModal과 동일한
// 구조(modal-overlay/modal-box/modal-header+modal-close-btn/modal-body) —
// 배경 클릭·ESC·닫기 버튼·취소 버튼 4가지 경로로 닫힌다. 오버레이 생성은
// dom.ts의 createModalOverlay로 공용화했다.
function renderTaskFormModal(editingTask: AutonomousTask | null, draft: TaskFormDraft): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, [editingTask ? '자율업무 수정' : '새 자율업무 추가']),
      el('button', { className: 'modal-close-btn', onClick: closeTaskFormModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderTaskForm(editingTask, draft)]),
  ]);
  return createModalOverlay(modalBox, closeTaskFormModal);
}

// ---------------- 진행상황판 탭 ----------------

function boardColumnOf(task: AutonomousTask): BoardColumn {
  if (task.running) return 'running';
  if (!task.enabled) return 'waiting';
  if (task.status === 'success') return 'success';
  if (task.status === 'failed' || task.status === 'timeout') return 'failed';
  return 'waiting';
}

export function renderAutonomousTaskBoardView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['자율업무']), el('div', { className: 'page-subtitle' }, ['진행상황판'])]),
  ]);

  const body: HTMLElement[] = [];
  const configBanner = renderConfigStatusBanner();
  if (configBanner) body.push(configBanner);
  body.push(tabsRow('board'));

  if (state.autonomousTasks.loading && state.autonomousTasks.items.length === 0) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]));
  } else if (state.autonomousTasks.error) {
    const retry = el('button', { className: 'btn', onClick: () => void loadAutonomousTasks() }, ['다시 시도']);
    body.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.autonomousTasks.error}`]), retry]));
  } else {
    const board = el(
      'div',
      { className: 'task-board' },
      BOARD_COLUMNS.map((column) => {
        const tasks = state.autonomousTasks.items.filter((t) => boardColumnOf(t) === column.key);
        return el('div', { className: 'task-board-column' }, [
          el('div', { className: 'task-board-column-head' }, [el('span', {}, [column.label]), el('span', { className: 'task-board-count' }, [String(tasks.length)])]),
          el(
            'div',
            { className: 'task-board-cards' },
            tasks.length > 0 ? tasks.map((t) => renderBoardCard(t, column.statusClass)) : [el('div', { className: 'task-board-empty' }, ['없음'])]
          ),
        ]);
      })
    );
    body.push(board);
  }

  return el('div', {}, [header, ...body]);
}

function renderBoardCard(task: AutonomousTask, statusClass: string): HTMLElement {
  return el(
    'div',
    { className: `task-run-card status-${statusClass}`, onClick: () => navigate(`#/tasks/item/${encodeURIComponent(task.id)}`) },
    [
      el('div', { className: 'task-run-name' }, [task.name]),
      el('div', { className: 'task-run-time' }, [task.projectName]),
      el('div', { className: 'task-run-time' }, [`마지막 실행 ${task.lastRunLabel}`]),
    ]
  );
}

// ---------------- 상세 화면 ----------------
// 재설계(docs/design/autonomy-task-detail-redesign.md) §1~§7 참고. 좌측
// 메타 카드(상태/프로젝트/반복/타임아웃/서브에이전트 + 수정·삭제)와 우측
// 지침(프롬프트)+기록(실행 이력) 2컬럼으로 구성한다(§2 대안 A).

function overviewRow(label: string, body: string): HTMLElement {
  return el('div', { className: 'overview-section' }, [el('div', { className: 'overview-label' }, [label]), el('div', { className: 'overview-body' }, [body])]);
}

// ---------------- 실행 이력 캐시 (모듈 스코프, (projectPath, taskId) 키) ----------------
// 이 화면은 notifyChange()가 불릴 때마다 전체가 다시 그려지는 구조라, 렌더
// 함수 안에서 무조건 fetch하면 무한 재조회 루프가 된다. task 폼 모달의
// taskFormModal 등과 동일한 원칙으로, 이력 로딩중/결과/에러/현재 limit을
// 모듈 스코프 Map에 캐시해 이미 로드됐거나 로딩 중이면 재조회하지 않는다.
interface HistoryCacheEntry {
  readonly loading: boolean;
  readonly error: string | null;
  readonly items: readonly RunHistoryEntry[];
  // 이번에 요청한 limit — undefined는 "백엔드 기본값(20건) 사용"을 뜻하며
  // 최초 조회에서만 쓴다("더 보기"부터는 항상 명시적인 숫자를 보낸다).
  readonly requestedLimit: number | undefined;
  readonly hasMore: boolean;
}

const historyCache = new Map<string, HistoryCacheEntry>();
const DEFAULT_HISTORY_LIMIT = 20;
const HISTORY_LIMIT_STEP = 20;

function loadTaskHistory(projectPath: string, taskId: string, requestedLimit: number | undefined): void {
  const key = runtimeKey(projectPath, taskId);
  const prev = historyCache.get(key);
  historyCache.set(key, { loading: true, error: null, items: prev?.items ?? [], requestedLimit, hasMore: prev?.hasMore ?? false });
  // ensureTaskHistoryLoaded()가 렌더 함수(renderRunHistoryCard) 안에서 이 함수를
  // 최초 1회 호출하므로, 여기서 notifyChange()를 동기 호출하면 바깥
  // renderApp()이 아직 return하기 전에 안쪽 renderApp()이 재진입해 사이드바+본문이
  // 두 벌 그려진다(main.ts:145-147이 저장소 차원에서 금지하는 패턴, 리뷰
  // M1 — playwright 실측으로 #app 자식 2→4 재현됨). queueMicrotask로 미루면
  // 현재 콜 스택(=바깥 renderApp())이 완전히 반환한 뒤 마이크로태스크 큐에서
  // notifyChange()가 실행되므로 재진입 없이 "로딩중 → 데이터 도착" 흐름은
  // 그대로 유지된다. 더 근본적인 대안(최초 조회를 handleNavigation으로 이전)은
  // 이 함수가 캐시 유무에 따라 렌더 중 호출되는 구조 자체를 바꿔야 해 범위가
  // 커지므로, 이번 수정은 재진입만 끊는 최소 처방을 택했다.
  queueMicrotask(notifyChange);
  void (async () => {
    try {
      const items = await fetchAutonomyTaskHistory(projectPath, taskId, requestedLimit);
      const effectiveLimit = requestedLimit ?? DEFAULT_HISTORY_LIMIT;
      // "더 보기" 숨김 트리거 = 이번 조회 결과 건수가 요청한 limit보다 작다
      // (설계 §4-6·§8 — 별도의 "총 개수" API 없이도 정확하다).
      historyCache.set(key, { loading: false, error: null, items, requestedLimit, hasMore: items.length >= effectiveLimit });
    } catch (err) {
      historyCache.set(key, {
        loading: false,
        error: err instanceof Error ? err.message : '알 수 없는 오류가 발생했습니다.',
        items: prev?.items ?? [],
        requestedLimit,
        hasMore: prev?.hasMore ?? false,
      });
    } finally {
      notifyChange();
    }
  })();
}

// 렌더 함수에서 호출한다 — 캐시가 없을 때만 최초 조회(limit 생략)를 시작하고,
// 이미 있으면(로딩중/완료/에러 무엇이든) 그 값을 그대로 반환해 재조회하지 않는다.
function ensureTaskHistoryLoaded(task: AutonomousTask): HistoryCacheEntry {
  const key = runtimeKey(task.projectPath, task.id);
  const cached = historyCache.get(key);
  if (cached) return cached;
  loadTaskHistory(task.projectPath, task.id, undefined);
  return { loading: true, error: null, items: [], requestedLimit: undefined, hasMore: false };
}

function handleLoadMoreHistory(task: AutonomousTask): void {
  const key = runtimeKey(task.projectPath, task.id);
  const cached = historyCache.get(key);
  if (cached?.loading) return;
  const nextLimit = (cached?.requestedLimit ?? DEFAULT_HISTORY_LIMIT) + HISTORY_LIMIT_STEP;
  loadTaskHistory(task.projectPath, task.id, nextLimit);
}

function handleRetryHistory(task: AutonomousTask): void {
  const key = runtimeKey(task.projectPath, task.id);
  const cached = historyCache.get(key);
  loadTaskHistory(task.projectPath, task.id, cached?.requestedLimit);
}

// 런타임 갱신 이벤트/폴백 폴링(§7)에서만 호출한다 — 상세 화면을 한 번이라도
// 연 적 있는(historyCache에 항목이 있는) task에 한해서만 재조회하고, 방문한
// 적 없는 task까지 미리 불러오지는 않는다. 이미 로딩 중이면 중복 호출하지 않는다.
function refreshHistoryIfTracked(projectPath: string, taskId: string): void {
  const key = runtimeKey(projectPath, taskId);
  const cached = historyCache.get(key);
  if (!cached || cached.loading) return;
  loadTaskHistory(projectPath, taskId, cached.requestedLimit);
}

// 이력 항목별 "세부 보기" 펼침 상태 — logPath는 실행 1건당 유일한 파일이라
// 이 키로 충분하다(더 보기로 배열이 늘어나도 기존 항목의 순서는 유지된다 —
// 백엔드가 항상 "최신순 상위 N건"을 돌려주는 계약이라 앞쪽 항목이 안 바뀐다).
const expandedHistoryRows = new Set<string>();
function historyRowKey(taskKey: string, entry: RunHistoryEntry): string {
  return `${taskKey}::${entry.logPath}`;
}

function handleCopyLogPath(path: string): void {
  navigator.clipboard.writeText(path).then(
    () => showToast('로그 경로를 복사했습니다'),
    () => showToast('경로 복사에 실패했습니다')
  );
}

// ---------------- 이력 표기 규칙 (설계 §4-2~§4-4) ----------------

const RUN_HISTORY_META: Readonly<Record<AutonomyHistoryResult, { readonly icon: string; readonly label: string; readonly badgeClass: string }>> = {
  success: { icon: '✓', label: '성공', badgeClass: 'badge-run-success' },
  failed: { icon: '✕', label: '실패', badgeClass: 'badge-run-failed' },
  timeout: { icon: '⏱', label: '타임아웃', badgeClass: 'badge-run-timeout' },
  aborted: { icon: '⊘', label: '중단됨', badgeClass: 'badge-run-aborted' },
};

function isSameCalendarDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

// "오늘/어제/그 외" 판정은 24시간 이내가 아니라 로컬 달력 날짜 비교다(자정
// 직후 실행 건이 "어제"로 잘못 표기되는 것을 방지, 설계 §4-3).
function formatHistoryTimestamp(startedAtIso: string): { readonly label: string; readonly title: string } {
  const t = Date.parse(startedAtIso);
  if (Number.isNaN(t)) return { label: startedAtIso, title: startedAtIso };
  const d = new Date(t);
  const now = new Date();
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  const timeOfDay = d.toLocaleTimeString('ko-KR', { hour: 'numeric', minute: '2-digit', hour12: true });

  let label: string;
  if (isSameCalendarDay(d, now)) {
    label = `오늘 ${timeOfDay}`;
  } else if (isSameCalendarDay(d, yesterday)) {
    label = `어제 ${timeOfDay}`;
  } else {
    const yearPrefix = d.getFullYear() === now.getFullYear() ? '' : `${d.getFullYear()}/`;
    label = `${yearPrefix}${d.getMonth() + 1}/${d.getDate()} ${timeOfDay}`;
  }
  return { label, title: startedAtIso };
}

function formatHistoryDuration(durationMs: number): string {
  const totalSeconds = Math.round(durationMs / 1000);
  if (totalSeconds < 60) return `${totalSeconds}초`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}분 ${String(seconds).padStart(2, '0')}초`;
}

// ---------------- 좌측 메타 카드 / 우측 프롬프트·기록 카드 ----------------

function renderTaskMetaCard(task: AutonomousTask): HTMLElement {
  const statusLabel = task.running ? '실행 중' : task.enabled ? '대기 중' : '중지됨';
  const sections = [
    overviewRow('상태', `● ${statusLabel}\n다음 실행 ${task.nextRunLabel}`),
    overviewRow('프로젝트', `${task.projectName}\n${task.projectPath}`),
    overviewRow('반복', task.scheduleLabel),
    overviewRow('타임아웃', task.timeout != null ? `${task.timeout}분` : '전역 기본값 사용'),
    overviewRow('서브에이전트', task.subagent ?? '지정 안 함'),
  ];

  // 수정/삭제는 헤더에서 좌측 메타 카드 하단으로 이동했다(설계 §3) — 설정을
  // 확인하다가 고치고 싶을 때 접근하는 동선이 자연스럽고, 파괴적인 삭제는
  // "지금 실행"·"중지" 같은 자주 쓰는 조작 동선에서 최대한 멀리 둔다.
  const editBtn = el('button', { className: 'btn', onClick: () => openTaskFormModal(task) }, ['수정']);
  const deleteBtn = el('button', { className: 'btn', onClick: () => void handleDeleteTask(task, () => navigate('#/tasks')) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';
  const footerActions = el('div', { className: 'overview-card-footer-actions' }, [editBtn, deleteBtn]);

  return el('div', { className: 'overview-card' }, [...sections, footerActions]);
}

function renderPromptCard(task: AutonomousTask): HTMLElement {
  return el('div', { className: 'overview-card' }, [el('div', { className: 'overview-body' }, [task.prompt])]);
}

// 세부 보기 펼침 블록 — 로그 파일 경로 + 복사 버튼을 보여준다. 설계 §4-1·§1
// 위계표 10번은 "요약(summary)도 함께 보여준다"고 적었지만, 실제 백엔드 계약
// (autonomy_task_history의 RunHistoryEntry)에는 summary 필드가 없다 — summary는
// AutonomyRuntimeStatus에만 있는 "가장 최근 실행 1건" 전용 메모리 값이라 과거
// 이력 각 건에는 없는 데이터다(PM 승인 이탈 (b), 없는 필드를 지어내지 않는다).
//
// [리뷰 M2 조치] 다만 그 "가장 최근 실행 1건"의 요약 자체는 실재하는 데이터이고,
// 재설계 전 화면에는 있었는데 지금은 어디에도 렌더되지 않아 회귀였다(설계 §1이
// 이 화면을 여는 두 목적 중 하나로 든 "왜 실패했는지 진단"에 필요). 위계 판정이
// "3차 — 이력 항목을 펼쳤을 때만 보이면 충분"이라 명시했으므로, 상시 노출 블록을
// 새로 만들지 않고 원래 의도대로 이 펼침 블록에 되살린다. 다만 이력 각 건에는
// summary가 없으므로, latestSummary는 이 entry가 "가장 최근 실행 1건"과 동일한
// 실행인지(logPath 완전 일치 — 실행 1건당 유일한 파일이라 startedAt 문자열
// 비교보다 정확하다, historyRowKey와 같은 근거) 호출부에서 판정해 넘겨준다.
// 일치하지 않으면(예: 방금 끝난 실행이 아직 이력 목록에 반영되지 않은 경합)
// 아무 데도 지어내 붙이지 않는다 — null이면 이 블록에 요약 자체를 만들지 않는다.
function renderHistoryDetail(entry: RunHistoryEntry, latestSummary: string | null): HTMLElement {
  const pathRow = el('div', {}, [
    (() => {
      const pathEl = el('span', { className: 'detail-path' }, [entry.logPath]);
      pathEl.style.overflow = 'hidden';
      pathEl.style.textOverflow = 'ellipsis';
      pathEl.style.whiteSpace = 'nowrap';
      pathEl.style.flex = '1';
      pathEl.title = entry.logPath;
      return pathEl;
    })(),
    el('button', { className: 'btn btn-sm', onClick: () => handleCopyLogPath(entry.logPath) }, ['경로 복사']),
  ]);
  pathRow.style.display = 'flex';
  pathRow.style.alignItems = 'center';
  pathRow.style.gap = '8px';

  const children: HTMLElement[] = [];
  if (latestSummary) {
    // .devtool-log(개발도구 화면의 stdout/stderr 미리보기)를 그대로 재사용한다
    // — 신규 클래스·신규 CSS 변수 없이 이미 있는 "로그 미리보기" 시각 패턴과
    // 통일한다. 설계 §4-1이 명시한 "최대 500자, white-space: pre-wrap"은
    // .devtool-log에 이미 있다.
    const summaryLabel = el('div', { className: 'overview-label' }, ['가장 최근 실행 요약']);
    const summaryBox = el('pre', { className: 'devtool-log' }, [latestSummary]);
    children.push(summaryLabel, summaryBox);
  }
  children.push(pathRow);

  const wrap = el('div', {}, children);
  wrap.style.marginTop = '8px';
  wrap.style.paddingLeft = '4px';
  wrap.style.display = 'flex';
  wrap.style.flexDirection = 'column';
  wrap.style.gap = '6px';
  return wrap;
}

function renderHistoryRow(taskKey: string, entry: RunHistoryEntry): HTMLElement {
  const meta = RUN_HISTORY_META[entry.result];
  const { label: timeLabel, title: timeTitle } = formatHistoryTimestamp(entry.startedAt);
  const expanded = expandedHistoryRows.has(historyRowKey(taskKey, entry));

  const badge = el('span', { className: `badge ${meta.badgeClass}` }, [`${meta.icon} ${meta.label}`]);
  const timeEl = el('span', {}, [timeLabel]);
  timeEl.style.flex = '1';
  timeEl.title = timeTitle;
  const durationEl = el('span', {}, [formatHistoryDuration(entry.durationMs)]);
  durationEl.style.textAlign = 'right';
  const toggleBtn = el(
    'button',
    {
      className: 'btn btn-sm',
      onClick: () => {
        const rowKey = historyRowKey(taskKey, entry);
        if (expandedHistoryRows.has(rowKey)) expandedHistoryRows.delete(rowKey);
        else expandedHistoryRows.add(rowKey);
        notifyChange();
      },
    },
    [expanded ? '세부 보기 ▴' : '세부 보기 ▾']
  );

  return el('div', { className: 'run-history-row' }, [badge, timeEl, durationEl, toggleBtn]);
}

function renderRunHistoryCard(task: AutonomousTask): HTMLElement {
  const taskKey = runtimeKey(task.projectPath, task.id);
  const cache = ensureTaskHistoryLoaded(task);

  const body: HTMLElement[] = [];

  if (cache.loading && cache.items.length === 0) {
    body.push(loadingBlock());
  } else if (cache.error) {
    // 이력 조회 실패는 이 카드 안에만 국한된다 — 좌측 메타/우측 프롬프트
    // 표시를 막지 않는다(설계 §4-6·§7).
    body.push(errorBlock(`실행 이력을 불러오지 못했습니다 — ${cache.error}`, () => handleRetryHistory(task)));
  } else if (cache.items.length === 0) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['아직 실행 이력이 없습니다']),
        el('div', { className: 'state-block-desc' }, ['상단의 "지금 실행"을 눌러 첫 실행을 시작하세요.']),
      ])
    );
  } else {
    // 각 항목 뒤에 펼침 상세 블록을 평평하게(같은 리스트의 형제 노드로) 이어
    // 붙인다 — 별도 wrapper div로 감싸면 마지막 자식이 .run-history-row가
    // 아니게 되어 ":last-child { border-bottom: none }" 규칙이 어긋난다.
    const rowEls: HTMLElement[] = [];
    cache.items.forEach((entry) => {
      const row = renderHistoryRow(taskKey, entry);
      rowEls.push(row);
      if (expandedHistoryRows.has(historyRowKey(taskKey, entry))) {
        // logPath는 실행 1건당 유일한 파일이라(§1162-1164 주석과 동일 근거),
        // task.logPath(런타임의 "가장 최근 실행 1건" 로그 경로)와 일치하는 딱
        // 한 건에만 task.summary를 붙인다 — 그 밖의 이력 건에는 summary가 없다.
        const latestSummary = task.logPath !== null && task.logPath === entry.logPath ? task.summary : null;
        rowEls.push(renderHistoryDetail(entry, latestSummary));
      }
    });
    // 마지막 실제 DOM 자식(펼쳐진 상세 블록일 수도 있다)의 구분선을 확실히
    // 지운다 — CSS :last-child만으로는 펼침 상태에 따라 어긋날 수 있어서다.
    const lastEl = rowEls[rowEls.length - 1];
    if (lastEl) lastEl.style.borderBottom = 'none';
    body.push(el('div', { className: 'run-history-list' }, rowEls));

    if (cache.hasMore) {
      const moreBtn = el('button', { className: 'btn', disabled: cache.loading, onClick: () => handleLoadMoreHistory(task) }, [
        cache.loading ? '불러오는 중…' : '더 보기',
      ]);
      const moreWrap = el('div', {}, [moreBtn]);
      moreWrap.style.marginTop = '12px';
      moreWrap.style.textAlign = 'center';
      body.push(moreWrap);
    }
  }

  return el('div', { className: 'overview-card run-history-card' }, body);
}

export function renderAutonomousTaskDetailView(taskId: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/tasks') }, ['← 자율업무']);
  const task = state.autonomousTasks.items.find((t) => t.id === taskId);

  if (!task) {
    if (state.autonomousTasks.loading) {
      return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])])]);
    }
    if (state.autonomousTasks.error) {
      const retry = el('button', { className: 'btn', onClick: () => void loadAutonomousTasks() }, ['다시 시도']);
      return el('div', {}, [back, el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.autonomousTasks.error}`]), retry])]);
    }
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['자율업무를 찾을 수 없습니다'])])]);
  }

  const statusLabel = task.running ? '실행 중' : task.enabled ? '대기 중' : '중지됨';
  const badgeEl = el('span', { className: `badge ${task.running ? 'badge-active' : task.enabled ? 'badge-unknown' : 'badge-archived'}` }, [statusLabel]);

  // "지금 실행"은 enabled(자동 스케줄 on/off)와 독립이다 — enabled=false는
  // "다음 예약 실행을 하지 않는다"는 뜻일 뿐, 사용자가 지금 당장 수동으로
  // 한 번 돌려보는 것까지 막을 이유는 없다(프롬프트/설정을 확인하려고 잠시
  // 꺼둔 task를 시험 실행하는 흔한 사용 패턴). 백엔드 계약도 "이미 실행 중 /
  // 동시 실행 한도 초과"만 거부 사유로 명시할 뿐 enabled를 조건으로 걸지
  // 않는다 — 그래서 running 여부만으로 disable한다.
  const runNowPendingHere = runNowPending.has(runtimeKey(task.projectPath, task.id));
  const runNowBtn = el(
    'button',
    { className: 'btn btn-primary', disabled: task.running || runNowPendingHere, onClick: () => void handleRunNow(task) },
    [task.running ? '실행 중…' : runNowPendingHere ? '요청 중…' : '지금 실행']
  );
  const toggleBtn = el('button', { className: 'btn', onClick: () => void handleToggleTask(task) }, [task.enabled ? '중지' : '재개']);
  // 프라이머리(지금 실행) 버튼 바로 왼쪽에 세컨더리(중지/재개)를 둔다(설계 §3).
  const headerActions = el('div', { className: 'settings-form-actions' }, [toggleBtn, runNowBtn]);

  // 헤더 우측 영역 — 상태 배지(윗줄) + 액션 버튼(아랫줄)을 세로로 쌓아
  // 오른쪽 정렬한다(와이어프레임 §2-1). .detail-header 자체는 그대로
  // 재사용하고(신규 클래스 없음), 이 wrapper만 인라인 스타일로 정렬한다.
  const headerRight = el('div', {}, [badgeEl, headerActions]);
  headerRight.style.display = 'flex';
  headerRight.style.flexDirection = 'column';
  headerRight.style.alignItems = 'flex-end';
  headerRight.style.gap = '8px';

  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [task.name]), el('div', { className: 'detail-path' }, [`${task.projectName} · ${task.scheduleLabel}`])]),
    headerRight,
  ]);

  // 우측 컬럼 — "지침(프롬프트)"/"기록" 섹션 제목은 이미 있는
  // .plugin-section-label(카탈로그 화면의 소제목)을 그대로 재사용한다(신규
  // 클래스 없음). `.overview-card + .plugin-section-label`에 margin-top:20px를
  // 추가해 두 카드 사이 간격(설계 §5)을 기존 인접 형제 선택자 패턴 그대로 준다.
  const rightColumn = el('div', {}, [
    el('div', { className: 'plugin-section-label' }, ['지침(프롬프트)']),
    renderPromptCard(task),
    el('div', { className: 'plugin-section-label' }, ['기록']),
    renderRunHistoryCard(task),
  ]);

  const grid = el('div', { className: 'task-detail-grid' }, [renderTaskMetaCard(task), rightColumn]);

  const detailChildren: HTMLElement[] = [back, header, grid];
  const taskFormModalEl = renderTaskFormModalIfOpen();
  if (taskFormModalEl) detailChildren.push(taskFormModalEl);

  return el('div', {}, detailChildren);
}
