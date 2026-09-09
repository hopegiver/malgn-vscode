// 자율업무 — 프로젝트별로 등록한 자율업무 설정을 실제 로컬 데이터로 보여준다
// (autonomyApi.ts, Rust 커맨드 autonomy_*). 실제 스케줄 실행 엔진(주기마다 claude를
// 호출하는 백그라운드 러너)은 Rust 쪽에 있다 — 이 화면은 그 설정을 조회/추가/
// 수정/삭제/on-off하고, 마지막 실행 결과(lastRunAt/lastStatus/history)를 읽어 보여줄
// 뿐 스스로 실행하지 않는다.
import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import type { AutonomousTask } from '../state';
import { fetchAutonomyTasks, saveAutonomyTask, deleteAutonomyTask, setAutonomyTaskEnabled } from '../autonomyApi';
import type { AutonomyTaskConfig, ProjectAutonomyGroup, AutonomyLastStatus } from '../autonomyApi';
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

// 폼 표시 여부만 다루는 모듈 로컬 UI 상태 — 전역 state까지 갈 필요는 없다.
let showAddForm = false;

// ---------------- 데이터 로딩 + 표시용 변환 ----------------

function computeScheduleLabel(intervalMinutes: number): string {
  if (intervalMinutes >= 1440 && intervalMinutes % 1440 === 0) return `매 ${intervalMinutes / 1440}일마다`;
  if (intervalMinutes >= 60 && intervalMinutes % 60 === 0) return `매 ${intervalMinutes / 60}시간마다`;
  return `매 ${intervalMinutes}분마다`;
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

function computeLastRunLabel(lastRunAt: string | null): string {
  if (!lastRunAt) return '없음';
  const t = Date.parse(lastRunAt);
  return Number.isNaN(t) ? lastRunAt : formatElapsedSince(t);
}

function computeNextRunLabel(lastRunAt: string | null, intervalMinutes: number, enabled: boolean): string {
  if (!enabled) return '중지됨';
  if (!lastRunAt) return '등록 후 첫 실행 대기';
  const t = Date.parse(lastRunAt);
  if (Number.isNaN(t)) return '알 수 없음';
  return formatRelativeFromNow(t + intervalMinutes * 60000);
}

function toDisplayTask(group: ProjectAutonomyGroup, task: AutonomyTaskConfig): AutonomousTask {
  return {
    id: task.id,
    projectPath: group.projectPath,
    projectName: group.projectName,
    name: task.name,
    prompt: task.prompt,
    subagent: task.subagent,
    intervalMinutes: task.intervalMinutes,
    enabled: task.enabled,
    preventOverlap: task.preventOverlap,
    lastRunAt: task.lastRunAt,
    lastStatus: task.lastStatus,
    lastSummary: task.lastSummary,
    history: task.history,
    scheduleLabel: computeScheduleLabel(task.intervalMinutes),
    lastRunLabel: computeLastRunLabel(task.lastRunAt),
    nextRunLabel: computeNextRunLabel(task.lastRunAt, task.intervalMinutes, task.enabled),
  };
}

export async function loadAutonomousTasks(): Promise<void> {
  state.autonomousTasks.loading = true;
  state.autonomousTasks.error = null;
  notifyChange();
  try {
    const groups = await fetchAutonomyTasks();
    state.autonomousTasks.items = groups.flatMap((group) => group.tasks.map((task) => toDisplayTask(group, task)));
    state.autonomousTasks.loaded = true;
  } catch (err) {
    state.autonomousTasks.error = err instanceof Error ? err.message : '자율업무 목록을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.autonomousTasks.loading = false;
    notifyChange();
  }
}

async function handleToggleTask(task: AutonomousTask): Promise<void> {
  const next = !task.enabled;
  try {
    await setAutonomyTaskEnabled(task.projectPath, task.id, next);
    task.enabled = next;
    task.nextRunLabel = computeNextRunLabel(task.lastRunAt, task.intervalMinutes, task.enabled);
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
    state.autonomousTasks.items = state.autonomousTasks.items.filter((t) => t.id !== task.id);
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
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['자율업무']),
      el('div', { className: 'page-subtitle' }, [`등록된 자율업무 ${state.autonomousTasks.items.length}개`]),
    ]),
    el(
      'button',
      {
        className: 'btn btn-primary',
        onClick: () => {
          showAddForm = !showAddForm;
          notifyChange();
        },
      },
      [showAddForm ? '취소' : '+ 새 자율업무']
    ),
  ]);

  const body: HTMLElement[] = [tabsRow('list')];
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

  return el('div', {}, [header, ...body]);
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
        el('span', { className: `badge ${task.enabled ? 'badge-active' : 'badge-archived'}` }, [task.enabled ? '실행 중' : '중지됨']),
        el('span', { className: 'badge badge-unknown' }, [task.projectName]),
      ]),
      el('div', { className: 'task-row-desc' }, [task.prompt]),
      el('div', { className: 'task-row-schedule' }, [
        el('span', {}, [`주기: ${task.scheduleLabel}`]),
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

  const subagentInput = document.createElement('input');
  subagentInput.className = 'settings-input';
  subagentInput.placeholder = '예: malgn-agent:qa-engineer (비워두면 지정 안 함)';

  const preventOverlapInput = document.createElement('input');
  preventOverlapInput.type = 'checkbox';
  preventOverlapInput.checked = true;
  const preventOverlapLabel = document.createElement('label');
  preventOverlapLabel.className = 'settings-field';
  preventOverlapLabel.style.flexDirection = 'row';
  preventOverlapLabel.style.alignItems = 'center';
  preventOverlapLabel.style.gap = '8px';
  const preventOverlapText = document.createElement('span');
  preventOverlapText.className = 'settings-field-label';
  preventOverlapText.textContent = '중복 실행 방지 (이전 실행이 끝나기 전에는 새로 실행하지 않음)';
  preventOverlapLabel.append(preventOverlapInput, preventOverlapText);

  const form = el('form', { className: 'settings-form task-add-form' }, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프롬프트']), promptInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['프로젝트']), projectSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 주기']), intervalSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['서브에이전트 (선택)']), subagentInput]),
    preventOverlapLabel,
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

    const task: AutonomyTaskConfig = {
      id: crypto.randomUUID(),
      name,
      prompt,
      subagent: subagentInput.value.trim() || null,
      intervalMinutes: Number(intervalSelect.value),
      enabled: true,
      preventOverlap: preventOverlapInput.checked,
      lastRunAt: null,
      lastStatus: null,
      lastSummary: null,
      history: [],
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
  if (!task.enabled) return 'waiting';
  const status: AutonomyLastStatus | null = task.lastStatus;
  if (status === 'running') return 'running';
  if (status === 'success') return 'success';
  if (status === 'failed') return 'failed';
  return 'waiting';
}

export function renderAutonomousTaskBoardView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['자율업무']), el('div', { className: 'page-subtitle' }, ['진행상황판'])]),
  ]);

  const body: HTMLElement[] = [tabsRow('board')];

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

const HISTORY_RESULT_LABEL: Readonly<Record<'success' | 'failed', string>> = { success: '성공', failed: '실패' };

export function renderAutonomousTaskDetailView(taskId: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/tasks') }, ['← 자율업무']);
  const task = state.autonomousTasks.items.find((t) => t.id === taskId);

  if (!task) {
    const message = state.autonomousTasks.loading ? '불러오는 중…' : '자율업무를 찾을 수 없습니다';
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, [message])])]);
  }

  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [task.name]), el('div', { className: 'detail-path' }, [`${task.projectName} · ${task.scheduleLabel}`])]),
    el('span', { className: `badge ${task.enabled ? 'badge-active' : 'badge-archived'}` }, [task.enabled ? '실행 중' : '중지됨']),
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
    overviewRow('중복 실행 방지', task.preventOverlap ? '켜짐' : '꺼짐'),
    overviewRow('마지막 실행', task.lastRunLabel),
    overviewRow('다음 실행', task.nextRunLabel),
    ...(task.lastSummary ? [overviewRow('마지막 실행 요약', task.lastSummary)] : []),
  ];
  const overview = el('div', { className: 'overview-card' }, overviewRows);

  const sections: HTMLElement[] = [header, actions, overview];

  if (task.history.length > 0) {
    sections.push(
      el('div', { className: 'overview-card' }, [
        el('div', { className: 'overview-label' }, ['실행 이력']),
        el(
          'div',
          { className: 'task-history-list' },
          task.history.map((h) =>
            el('div', { className: 'task-history-row' }, [
              el('span', { className: 'task-history-time' }, [h.at]),
              el('span', { className: `task-history-result ${h.result === 'success' ? 'ok' : 'fail'}` }, [HISTORY_RESULT_LABEL[h.result]]),
            ])
          )
        ),
      ])
    );
  }

  return el('div', {}, [back, ...sections]);
}
