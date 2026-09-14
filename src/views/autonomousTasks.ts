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
import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import type { AutonomousTask } from '../state';
import {
  fetchAutonomyTasks,
  saveAutonomyTask,
  deleteAutonomyTask,
  setAutonomyTaskEnabled,
  fetchAutonomyRuntimeStatus,
  onAutonomyRuntimeChanged,
} from '../autonomyApi';
import type { AutonomyTaskConfig, ProjectAutonomyGroup, AutonomyRuntimeStatus, AutonomyRunStatus } from '../autonomyApi';
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

// 폼 표시 여부만 다루는 모듈 로컬 UI 상태 — 전역 state까지 갈 필요는 없다.
let showAddForm = false;

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

// 자율업무 화면을 완전히 떠날 때(다른 라우트로 이동) main.ts에서 호출한다 —
// 모달이 열려 있었다면 window에 남은 ESC 리스너를 정리한다.
export function leaveAutonomousTasksListView(): void {
  state.malgnAgentConfig.editingAutonomy = false;
  detachAutonomyConfigModalEscHandler();
}

// ---------------- (projectPath, taskId) 복합키 ----------------

function runtimeKey(projectPath: string, taskId: string): string {
  return `${projectPath}\0${taskId}`;
}

// ---------------- 데이터 로딩 + 표시용 변환 ----------------

function computeScheduleLabel(interval: number): string {
  if (interval >= 1440 && interval % 1440 === 0) return `이전 실행 완료 후 ${interval / 1440}일 뒤 재실행`;
  if (interval >= 60 && interval % 60 === 0) return `이전 실행 완료 후 ${interval / 60}시간 뒤 재실행`;
  return `이전 실행 완료 후 ${interval}분 뒤 재실행`;
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

function computeNextRunLabel(nextRunAt: string | null, enabled: boolean): string {
  if (!enabled) return '중지됨';
  if (!nextRunAt) return '등록 후 첫 실행 대기';
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
    scheduleLabel: computeScheduleLabel(task.interval),
    lastRunLabel: computeLastRunLabel(running, lastStartedAt, lastFinishedAt),
    nextRunLabel: computeNextRunLabel(nextRunAt, task.enabled),
  };
}

function toDisplayTask(group: ProjectAutonomyGroup, task: AutonomyTaskConfig, runtime: AutonomyRuntimeStatus | null): AutonomousTask {
  return mergeRuntime(
    {
      id: task.id,
      projectPath: group.projectPath,
      projectName: group.projectName,
      name: task.name,
      prompt: task.prompt,
      subagent: task.subagent,
      interval: task.interval,
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
      scheduleLabel: computeScheduleLabel(task.interval),
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
function renderConfigEditModal(status: MalgnAgentConfigStatus): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['자율업무 전역 설정']),
      el('button', { className: 'modal-close-btn', onClick: closeAutonomyConfigModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderConfigEditForm(status)]),
  ]);
  modalBox.addEventListener('click', (e) => e.stopPropagation());

  const overlay = el('div', { className: 'modal-overlay', onClick: closeAutonomyConfigModal }, [modalBox]);
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  return overlay;
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
    task.nextRunLabel = computeNextRunLabel(task.nextRunAt, task.enabled);
    showToast(`${task.name} ${next ? '활성화' : '비활성화'}됨`);
  } catch (err) {
    showToast(err instanceof Error ? err.message : '상태 변경에 실패했습니다');
  } finally {
    notifyChange();
  }
}

async function handleDeleteTask(task: AutonomousTask, afterDelete?: () => void): Promise<void> {
  if (!window.confirm(`"${task.name}" 자율업무를 삭제할까요? 되돌릴 수 없습니다.`)) return;
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
  // V-06: 폼이 열린 상태에서 라벨만 "취소"로 바뀌고 className은 primary
  // 그대로라 화면에서 가장 강조된 버튼이 "취소"가 됐다. 열린 상태에서는
  // 보조 버튼으로 낮춘다.
  const addToggleBtn = el(
    'button',
    {
      className: showAddForm ? 'btn' : 'btn btn-primary',
      onClick: () => {
        showAddForm = !showAddForm;
        notifyChange();
      },
    },
    [showAddForm ? '취소' : '+ 새 자율업무']
  );
  const configEditToggleBtn = renderConfigEditToggleBtn();

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['자율업무']),
      el('div', { className: 'page-subtitle' }, [`등록된 자율업무 ${state.autonomousTasks.items.length}개`]),
    ]),
    el('div', { className: 'devtool-header-actions' }, [addToggleBtn, ...(configEditToggleBtn ? [configEditToggleBtn] : [])]),
  ]);

  const body: HTMLElement[] = [];
  const configBanner = renderConfigStatusBanner();
  if (configBanner) body.push(configBanner);
  body.push(tabsRow('list'));
  if (showAddForm) body.push(renderAddForm());

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

  const deleteBtn = el('button', { className: 'btn', onClick: () => void handleDeleteTask(task) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';

  return el('div', { className: 'task-row' }, [
    main,
    el('div', { className: 'task-row-actions' }, [toggleSwitch(task.enabled, () => void handleToggleTask(task)), deleteBtn]),
  ]);
}

function renderAddForm(): HTMLElement {
  const nameInput = document.createElement('input');
  nameInput.className = 'settings-input';
  nameInput.placeholder = '예: 야간 프로젝트 상태 점검';

  const promptInput = document.createElement('textarea');
  promptInput.className = 'settings-input task-form-textarea';
  promptInput.placeholder = '이 자율업무가 실제로 실행될 때 claude에게 전달할 지시문을 구체적으로 적으세요';
  promptInput.rows = 4;

  const projectSelect = document.createElement('select');
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

  const intervalSelect = document.createElement('select');
  intervalSelect.className = 'settings-input';
  for (const opt of INTERVAL_OPTIONS) {
    const optionEl = document.createElement('option');
    optionEl.value = opt.value;
    optionEl.textContent = opt.label;
    intervalSelect.appendChild(optionEl);
  }
  intervalSelect.value = '60';
  const intervalHint = el('div', { className: 'settings-form-hint' }, [
    '이전 실행이 끝난 뒤(시작 시점이 아니라 완료 시점 기준) 이 시간만큼 지나야 다시 실행됩니다.',
  ]);

  const subagentInput = document.createElement('input');
  subagentInput.className = 'settings-input';
  subagentInput.placeholder = '예: malgn-agent:qa-engineer (비워두면 지정 안 함)';

  const limits = state.malgnAgentConfig.status?.limits ?? null;
  const defaultTimeout = state.malgnAgentConfig.status?.autonomy.defaultTimeout ?? null;
  const timeoutInput = document.createElement('input');
  timeoutInput.className = 'settings-input';
  timeoutInput.type = 'number';
  timeoutInput.min = String(limits?.minTimeout ?? 1);
  timeoutInput.max = String(limits?.maxTimeout ?? 480);
  timeoutInput.placeholder = defaultTimeout !== null ? `비우면 전역 기본값(${defaultTimeout}분) 사용` : '비우면 전역 기본값 사용';
  const timeoutHint = el('div', { className: 'settings-form-hint' }, [
    limits ? `선택 항목입니다. 범위: ${limits.minTimeout}~${limits.maxTimeout}분.` : '선택 항목입니다 — 비우면 전역 기본값을 사용합니다.',
  ]);

  const startupGrace = state.malgnAgentConfig.status?.limits.startupGraceMinutes ?? null;
  const startupHint = el('div', { className: 'settings-form-hint' }, [
    startupGrace !== null
      ? `등록 후 약 ${startupGrace}분 뒤 첫 실행됩니다(앱을 새로 시작한 직후에도 동일합니다 — 재시작 즉시 전체 자율업무가 몰려 실행되는 것을 막기 위한 유예 시간입니다).`
      : '등록 직후 바로 실행되지 않고, 짧은 유예 시간 뒤에 첫 실행됩니다.',
  ]);

  const form = el('form', { className: 'settings-form task-add-form' }, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프롬프트']), promptInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프로젝트']), projectSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 주기']), intervalSelect, intervalHint]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['서브에이전트 (선택)']), subagentInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['타임아웃 (분, 선택)']), timeoutInput, timeoutHint]),
    startupHint,
  ]);

  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = nameInput.value.trim();
    const prompt = promptInput.value.trim();
    const projectPath = projectSelect.value;
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

    const task: AutonomyTaskConfig = {
      id: crypto.randomUUID(),
      name,
      prompt,
      subagent: subagentInput.value.trim() || null,
      interval: Number(intervalSelect.value),
      enabled: true,
      timeout,
    };

    saveBtn.disabled = true;
    void (async () => {
      try {
        await saveAutonomyTask(projectPath, task);
        showToast(`"${name}" 자율업무가 추가되었습니다`);
        showAddForm = false;
        await loadAutonomousTasks();
      } catch (err) {
        showToast(err instanceof Error ? err.message : '자율업무 저장에 실패했습니다');
        saveBtn.disabled = false;
        notifyChange();
      }
    })();
  });

  return el('div', { className: 'settings-card task-add-card' }, [form]);
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
  const actions = el('div', { className: 'settings-form-actions' }, [
    el('button', { className: 'btn', onClick: () => void handleToggleTask(task) }, [task.enabled ? '중지' : '재개']),
    deleteBtn,
  ]);

  const overviewRows = [
    overviewRow('프롬프트', task.prompt),
    overviewRow('프로젝트', task.projectName),
    overviewRow('서브에이전트', task.subagent ?? '지정 안 함'),
    overviewRow('실행 주기', task.scheduleLabel),
    overviewRow('타임아웃', task.timeout !== null ? `${task.timeout}분` : '전역 기본값 사용'),
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

  return el('div', {}, [back, header, actions, overview, logHint]);
}
