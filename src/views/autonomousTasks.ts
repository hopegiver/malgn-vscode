// 자율업무 — 프로젝트별로 등록한 자율업무 "설정"(프롬프트·주기·서브에이전트·
// 타임아웃)을 자율업무 실행 상태와 병합해 보여준다. 설정은 autonomy.json(파일),
// 실행 상태(현재 실행 중 여부·마지막 실행 결과·다음 실행 시각)는 Rust 프로세스
// 메모리(autonomy_runtime_status)로 완전히 분리됐다(설계 §1·§2) — 이 화면은 그
// 둘을 (projectPath, taskId) 키로 합쳐 그릴 뿐, 스스로 스케줄을 실행하지 않고
// preventOverlap 같은 옵션도 없다(직렬 실행이 백엔드 구조로 이미 보장된다).
//
// 과거 실행 이력은 더 이상 이 화면이 아니라 프로젝트의
// .claude/logs/autonomy/<날짜>/ 아래 로그 파일에 남는다(설계 §9) — 그래서 상세
// 화면은 "마지막 실행 1건"의 요약/로그 경로만 보여주고, 히스토리 목록을 그리지
// 않는다.
import { el, showToast, toggleSwitch, createModalOverlay, confirmDialog } from '../dom';
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
} from '../autonomyApi';
import type { AutonomyTaskConfig, ProjectAutonomyGroup, AutonomyRuntimeStatus, AutonomyRunStatus, AutonomyScheduleMode } from '../autonomyApi';
import { fetchMalgnAgentConfig, saveMalgnAgentConfig } from '../configApi';
import type { MalgnAgentConfigInput, MalgnAgentConfigStatus } from '../configApi';
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

const RUN_STATUS_LABEL: Readonly<Record<AutonomyRunStatus, string>> = { success: '성공', failed: '실패', timeout: '타임아웃' };

// 0=일…6=토 — 백엔드(chrono::Weekday::num_days_from_sunday)와 동일한 축. 순서를
// 바꾸지 않는다(설계서 §3·§4.1).
const WEEKDAY_LABELS: readonly string[] = ['일', '월', '화', '수', '목', '금', '토'];
const WEEKDAY_PRESET_DAILY: readonly number[] = [0, 1, 2, 3, 4, 5, 6];
const WEEKDAY_PRESET_WEEKDAYS: readonly number[] = [1, 2, 3, 4, 5];
const WEEKDAY_PRESET_WEEKEND: readonly number[] = [0, 6];

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
  detachAutonomyConfigModalEscHandler();
  notifyChange();
}

// "새 자율업무" / "자율업무 수정" 겸용 모달 — editingTask가 null이면 추가 모드,
// 채워져 있으면 그 task를 프리필한 수정 모드다(제출 시 같은 id로 upsert).
// 목록 화면의 헤더 버튼과 상세 화면의 "수정" 버튼이 둘 다 이 모달을 연다 —
// 열림 상태를 이 화면 전용 값이라 전역 state까지 보낼 필요는 없다(config
// 모달과 달리 다른 화면과 공유되지 않으므로 모듈 로컬로 충분하다).
let taskFormModal: { readonly editingTask: AutonomousTask | null } | null = null;
let taskFormModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachTaskFormModalEscHandler(): void {
  if (taskFormModalEscHandler) {
    window.removeEventListener('keydown', taskFormModalEscHandler);
    taskFormModalEscHandler = null;
  }
}

function openTaskFormModal(editingTask: AutonomousTask | null): void {
  taskFormModal = { editingTask };
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
  return renderTaskFormModal(taskFormModal.editingTask);
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

function computeScheduleLabel(task: { scheduleMode: AutonomyScheduleMode; interval: number; atTime: string | null; days: readonly number[] }): string {
  if (task.scheduleMode === 'fixedTime') return computeFixedTimeScheduleLabel(task.atTime, task.days);
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
    // 고정시각 모드는 "등록 직후라 아직 계산 전"이 아니라 atTime 파싱 실패로
    // next_run_at이 영구히 None인 경우가 실제로 있다(설계서 §4.5 #14) —
    // interval 모드의 "첫 실행 대기"와 다른 문구로 구분한다.
    return scheduleMode === 'fixedTime' ? '실행 시각이 올바르지 않습니다' : '등록 후 첫 실행 대기';
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
      scheduleLabel: computeScheduleLabel({ scheduleMode, interval: task.interval, atTime, days }),
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
    state.autonomousTasks.error = err instanceof Error ? err.message : '자율업무 목록을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
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

  const concurrencyInput = document.createElement('input');
  concurrencyInput.id = 'malgn-config-concurrency';
  concurrencyInput.className = 'settings-input';
  concurrencyInput.type = 'number';
  concurrencyInput.min = '1';
  concurrencyInput.max = String(limits.maxConcurrency);
  concurrencyInput.value = String(status.autonomy.concurrency);
  concurrencyInput.autocomplete = 'off';

  const defaultTimeoutInput = document.createElement('input');
  defaultTimeoutInput.id = 'malgn-config-default-timeout';
  defaultTimeoutInput.className = 'settings-input';
  defaultTimeoutInput.type = 'number';
  defaultTimeoutInput.min = String(limits.minTimeout);
  defaultTimeoutInput.max = String(limits.maxTimeout);
  defaultTimeoutInput.value = String(status.autonomy.defaultTimeout);
  defaultTimeoutInput.autocomplete = 'off';

  const retentionDaysInput = document.createElement('input');
  retentionDaysInput.id = 'malgn-config-retention-days';
  retentionDaysInput.className = 'settings-input';
  retentionDaysInput.type = 'number';
  retentionDaysInput.min = '1';
  retentionDaysInput.max = '365';
  retentionDaysInput.value = String(status.logs.retentionDays);
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
      if (!state.autonomousTasks.loading) void loadAutonomousTasks();
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
function renderTaskForm(editingTask: AutonomousTask | null): HTMLElement {
  const nameInput = document.createElement('input');
  nameInput.className = 'settings-input';
  nameInput.placeholder = '예: 야간 프로젝트 상태 점검';
  nameInput.value = editingTask?.name ?? '';

  const promptInput = document.createElement('textarea');
  promptInput.className = 'settings-input task-form-textarea';
  promptInput.placeholder = '이 자율업무가 실제로 실행될 때 claude에게 전달할 지시문을 구체적으로 적으세요';
  promptInput.rows = 4;
  promptInput.value = editingTask?.prompt ?? '';

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
    projectSelect = document.createElement('select');
    projectSelect.className = 'settings-input';
    const projects = state.dashboard.projects;
    if (projects.length === 0) {
      const placeholderOption = document.createElement('option');
      placeholderOption.value = '';
      placeholderOption.textContent = state.dashboard.loading ? '프로젝트 목록 불러오는 중…' : '등록된 프로젝트가 없습니다';
      projectSelect.appendChild(placeholderOption);
      projectSelect.disabled = true;
    } else {
      for (const project of projects) {
        const optionEl = document.createElement('option');
        optionEl.value = project.path;
        optionEl.textContent = project.name;
        projectSelect.appendChild(optionEl);
      }
    }
    projectField = el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프로젝트']), projectSelect]);
  }

  const intervalSelect = document.createElement('select');
  intervalSelect.className = 'settings-input';
  for (const opt of INTERVAL_OPTIONS) {
    const optionEl = document.createElement('option');
    optionEl.value = opt.value;
    optionEl.textContent = opt.label;
    intervalSelect.appendChild(optionEl);
  }
  intervalSelect.value = editingTask ? String(editingTask.interval) : '60';
  const intervalHint = el('div', { className: 'settings-form-hint' }, [
    '이전 실행이 끝난 뒤(시작 시점이 아니라 완료 시점 기준) 이 시간만큼 지나야 다시 실행됩니다.',
  ]);
  const intervalFields = el('div', {}, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 주기']), intervalSelect, intervalHint]),
  ]);

  // ---- 실행 방식(스케줄 모드) 라디오 + 고정 시각 전용 필드 ----
  // 기존 상대간격(interval) 동작·문구는 위 두 요소를 그대로 재사용한다 —
  // 신규 모드 추가일 뿐 기존 모드를 바꾸지 않는다(지시서 금지 범위).
  const initialScheduleMode: AutonomyScheduleMode = editingTask?.scheduleMode ?? 'interval';

  const modeIntervalRadio = document.createElement('input');
  modeIntervalRadio.type = 'radio';
  modeIntervalRadio.name = 'task-form-schedule-mode';
  modeIntervalRadio.value = 'interval';
  modeIntervalRadio.checked = initialScheduleMode === 'interval';

  const modeFixedRadio = document.createElement('input');
  modeFixedRadio.type = 'radio';
  modeFixedRadio.name = 'task-form-schedule-mode';
  modeFixedRadio.value = 'fixedTime';
  modeFixedRadio.checked = initialScheduleMode === 'fixedTime';

  const modeField = el('div', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 방식']),
    el('div', { className: 'filter-group' }, [
      el('label', {}, [modeIntervalRadio, ' 상대 간격 (이전 실행 완료 후 N분 뒤)']),
      el('label', {}, [modeFixedRadio, ' 고정 시각 (매일/특정 요일 지정 시각)']),
    ]),
  ]);

  // <input type="time">의 value는 브라우저 로케일과 무관하게 항상 "HH:MM"
  // 24시간 형식이다 — 백엔드 parse_hhmm과 계약이 정확히 맞으므로 별도
  // 파서·입력 마스킹을 두지 않는다(설계서 §7 필수 계약).
  const timeInput = document.createElement('input');
  timeInput.type = 'time';
  timeInput.className = 'settings-input';
  timeInput.value = editingTask?.atTime ?? '09:00';
  const timeField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 시각']),
    timeInput,
    el('div', { className: 'settings-form-hint' }, ['기기의 현재 시간대 기준 벽시계 시각입니다(예: 09:00 = 오전 9시). 기기 시간대가 바뀌면 다음 계산부터 새 시간대의 같은 시각을 따릅니다.']),
  ]);

  // 요일 인덱스는 0=일요일…6=토요일(백엔드와 동일 축, 설계서 §3) — 순서를
  // 바꾸지 않는다. days가 비어 있으면(레거시/손편집 경로에서만 발생) "매일"로
  // 취급해 전체 선택 상태로 보여준다 — 빈 상태로 보이면 왕복 손실처럼
  // 오인되기 때문이다(설계서 §3 정규화 규칙).
  const initialDays: readonly number[] =
    initialScheduleMode === 'fixedTime' && editingTask && editingTask.days.length > 0 ? editingTask.days : WEEKDAY_PRESET_DAILY;
  const dayCheckboxes: HTMLInputElement[] = WEEKDAY_LABELS.map((_, idx) => {
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.value = String(idx);
    cb.checked = initialDays.includes(idx);
    return cb;
  });
  function setDayCheckboxes(days: readonly number[]): void {
    dayCheckboxes.forEach((cb, idx) => {
      cb.checked = days.includes(idx);
    });
  }
  const dayPresetDailyBtn = el('button', { className: 'filter-btn', onClick: () => setDayCheckboxes(WEEKDAY_PRESET_DAILY) }, ['매일']);
  dayPresetDailyBtn.type = 'button';
  const dayPresetWeekdaysBtn = el('button', { className: 'filter-btn', onClick: () => setDayCheckboxes(WEEKDAY_PRESET_WEEKDAYS) }, ['평일']);
  dayPresetWeekdaysBtn.type = 'button';
  const dayPresetWeekendBtn = el('button', { className: 'filter-btn', onClick: () => setDayCheckboxes(WEEKDAY_PRESET_WEEKEND) }, ['주말']);
  dayPresetWeekendBtn.type = 'button';
  const daysField = el('div', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['실행 요일']),
    el('div', { className: 'filter-group' }, [dayPresetDailyBtn, dayPresetWeekdaysBtn, dayPresetWeekendBtn]),
    el(
      'div',
      { className: 'filter-group' },
      dayCheckboxes.map((cb, idx) => el('label', {}, [cb, ` ${WEEKDAY_LABELS[idx]}`]))
    ),
    el('div', { className: 'settings-form-hint' }, ['최소 한 요일을 선택하세요.']),
  ]);
  const fixedTimeFields = el('div', {}, [timeField, daysField]);

  const subagentInput = document.createElement('input');
  subagentInput.className = 'settings-input';
  subagentInput.placeholder = '예: malgn-agent:qa-engineer (비워두면 지정 안 함)';
  subagentInput.value = editingTask?.subagent ?? '';

  const limits = state.malgnAgentConfig.status?.limits ?? null;
  const defaultTimeout = state.malgnAgentConfig.status?.autonomy.defaultTimeout ?? null;
  const timeoutInput = document.createElement('input');
  timeoutInput.className = 'settings-input';
  timeoutInput.type = 'number';
  timeoutInput.min = String(limits?.minTimeout ?? 1);
  timeoutInput.max = String(limits?.maxTimeout ?? 480);
  timeoutInput.placeholder = defaultTimeout !== null ? `비우면 전역 기본값(${defaultTimeout}분) 사용` : '비우면 전역 기본값 사용';
  timeoutInput.value = editingTask?.timeout !== null && editingTask?.timeout !== undefined ? String(editingTask.timeout) : '';
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

  // 라디오 선택에 따라 상대간격/고정시각 전용 필드 블록을 토글하고 힌트
  // 문구를 갱신한다.
  function updateScheduleModeVisibility(): void {
    const isFixed = modeFixedRadio.checked;
    intervalFields.style.display = isFixed ? 'none' : '';
    fixedTimeFields.style.display = isFixed ? '' : 'none';
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

    // 제출 검증: 고정시간 모드인데 시각이 비었거나 요일 0개면 차단한다(지시서 계약).
    const scheduleMode: AutonomyScheduleMode = modeFixedRadio.checked ? 'fixedTime' : 'interval';
    let atTime: string | null = null;
    let days: number[] = [];
    if (scheduleMode === 'fixedTime') {
      atTime = timeInput.value || null;
      days = dayCheckboxes.reduce<number[]>((acc, cb, idx) => {
        if (cb.checked) acc.push(idx);
        return acc;
      }, []);
      if (!atTime || days.length === 0) {
        showToast('고정 시각 모드는 실행 시각과 최소 한 요일을 선택해야 합니다.');
        return;
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
        showToast(err instanceof Error ? err.message : '자율업무 저장에 실패했습니다');
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
function renderTaskFormModal(editingTask: AutonomousTask | null): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, [editingTask ? '자율업무 수정' : '새 자율업무 추가']),
      el('button', { className: 'modal-close-btn', onClick: closeTaskFormModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderTaskForm(editingTask)]),
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

function overviewRow(label: string, body: string): HTMLElement {
  return el('div', { className: 'overview-section' }, [el('div', { className: 'overview-label' }, [label]), el('div', { className: 'overview-body' }, [body])]);
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
  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [task.name]), el('div', { className: 'detail-path' }, [`${task.projectName} · ${task.scheduleLabel}`])]),
    el('span', { className: `badge ${task.running ? 'badge-active' : task.enabled ? 'badge-unknown' : 'badge-archived'}` }, [statusLabel]),
  ]);

  const deleteBtn = el(
    'button',
    { className: 'btn', onClick: () => void handleDeleteTask(task, () => navigate('#/tasks')) },
    ['삭제']
  );
  deleteBtn.style.color = 'var(--color-danger)';
  const editBtn = el('button', { className: 'btn', onClick: () => openTaskFormModal(task) }, ['수정']);
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
  const actions = el('div', { className: 'settings-form-actions' }, [
    runNowBtn,
    el('button', { className: 'btn', onClick: () => void handleToggleTask(task) }, [task.enabled ? '중지' : '재개']),
    editBtn,
    deleteBtn,
  ]);

  const overviewRows = [
    overviewRow('프롬프트', task.prompt),
    overviewRow('프로젝트', task.projectName),
    overviewRow('서브에이전트', task.subagent ?? '지정 안 함'),
    overviewRow('실행 주기', task.scheduleLabel),
    overviewRow('타임아웃', task.timeout != null ? `${task.timeout}분` : '전역 기본값 사용'),
    overviewRow('마지막 실행', task.lastRunLabel),
    overviewRow('마지막 실행 결과', task.status ? RUN_STATUS_LABEL[task.status] : '기록 없음'),
    overviewRow('다음 실행', task.nextRunLabel),
    ...(task.durationMs !== null ? [overviewRow('마지막 실행 소요 시간', `${Math.round(task.durationMs / 1000)}초`)] : []),
    ...(task.summary ? [overviewRow('마지막 실행 요약', task.summary)] : []),
    ...(task.logPath ? [overviewRow('실행 로그 파일', task.logPath)] : []),
  ];
  const overview = el('div', { className: 'overview-card' }, overviewRows);
  const logHint = el('div', { className: 'settings-form-hint' }, [
    '과거 실행 이력 전체는 이 화면에 쌓이지 않습니다 — 프로젝트의 .claude/logs/autonomy/ 아래 날짜별 로그 파일에서 확인하세요.',
  ]);

  const detailChildren: HTMLElement[] = [back, header, actions, overview, logHint];
  const taskFormModalEl = renderTaskFormModalIfOpen();
  if (taskFormModalEl) detailChildren.push(taskFormModalEl);

  return el('div', {}, detailChildren);
}
