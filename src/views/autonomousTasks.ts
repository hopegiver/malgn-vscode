// 자율업무 — 순수 목업. 실제 스케줄 실행 엔진은 붙어 있지 않다(나중에 별도 작업으로
// 실제 실행기가 연결될 자리). on/off·새 항목 추가는 세션 메모리 상태만 바꾸고
// 새로고침하면 사라진다.
import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import { MOCK_TASK_RUNS } from '../mockData';
import type { AutonomousTask, TaskRun, TaskRunStatus } from '../mockData';
import { navigate } from '../route';

const SCHEDULE_OPTIONS: readonly { readonly value: string; readonly label: string }[] = [
  { value: 'hourly', label: '매시간' },
  { value: 'daily', label: '매일' },
  { value: 'weekly', label: '매주 월요일' },
];

const JIRA_SPACE_OPTIONS: readonly { readonly value: string; readonly label: string }[] = [
  { value: '', label: '연동 안 함' },
  { value: 'PROJ', label: 'PROJ' },
  { value: 'DEV', label: 'DEV' },
  { value: 'OPS', label: 'OPS' },
];

const BOARD_STATUSES: readonly TaskRunStatus[] = ['진행중', '대기중', '성공', '실패'];

// 폼 표시 여부만 다루는 모듈 로컬 UI 상태 — 전역 state까지 갈 필요는 없다.
let showAddForm = false;

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
      el('div', { className: 'page-subtitle' }, [`등록된 자율업무 ${state.autonomousTasks.length}개 (목업 — 실제 스케줄 실행 없음)`]),
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
  body.push(el('div', { className: 'task-list' }, state.autonomousTasks.map(renderTaskRow)));

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
        ...(task.jiraSpace ? [el('span', { className: 'badge badge-unknown' }, [`Jira: ${task.jiraSpace}`])] : []),
      ]),
      el('div', { className: 'task-row-desc' }, [task.description]),
      el('div', { className: 'task-row-schedule' }, [
        el('span', {}, [`주기: ${task.scheduleLabel}`]),
        el('span', {}, [`마지막 실행: ${task.lastRunLabel ?? '없음'}`]),
        el('span', {}, [`다음 실행: ${task.nextRunLabel}`]),
      ]),
    ]
  );
  main.setAttribute('role', 'button');
  main.setAttribute('tabindex', '0');

  return el('div', { className: 'task-row' }, [
    main,
    toggleSwitch(task.enabled, () => {
      task.enabled = !task.enabled;
      showToast(`${task.name} ${task.enabled ? '활성화' : '비활성화'}됨`);
      notifyChange();
    }),
  ]);
}

function renderAddForm(): HTMLElement {
  const nameInput = document.createElement('input');
  nameInput.className = 'settings-input';
  nameInput.placeholder = '예: 야간 빌드 상태 점검';

  const descInput = document.createElement('textarea');
  descInput.className = 'settings-input task-form-textarea';
  descInput.placeholder = '이 자율업무가 하는 일을 설명하세요';
  descInput.rows = 3;

  const scheduleSelect = document.createElement('select');
  scheduleSelect.className = 'settings-input';
  for (const opt of SCHEDULE_OPTIONS) {
    const optionEl = document.createElement('option');
    optionEl.value = opt.value;
    optionEl.textContent = opt.label;
    scheduleSelect.appendChild(optionEl);
  }

  const timeInput = document.createElement('input');
  timeInput.type = 'time';
  timeInput.className = 'settings-input';
  timeInput.value = '09:00';

  const jiraSelect = document.createElement('select');
  jiraSelect.className = 'settings-input';
  for (const opt of JIRA_SPACE_OPTIONS) {
    const optionEl = document.createElement('option');
    optionEl.value = opt.value;
    optionEl.textContent = opt.label;
    jiraSelect.appendChild(optionEl);
  }

  const form = el('form', { className: 'settings-form task-add-form' }, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['설명 / 할 일']), descInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 주기']), scheduleSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['시각']), timeInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['Jira 스페이스']), jiraSelect]),
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = nameInput.value.trim() || '이름 없는 자율업무';
    const scheduleLabelMap: Readonly<Record<string, string>> = {
      hourly: '매시간',
      daily: `매일 ${timeInput.value}`,
      weekly: `매주 월요일 ${timeInput.value}`,
    };
    const newTask: AutonomousTask = {
      id: `task-custom-${Date.now()}`,
      name,
      description: descInput.value.trim() || '(설명 없음)',
      scheduleLabel: scheduleLabelMap[scheduleSelect.value] ?? '매일',
      lastRunLabel: null,
      nextRunLabel: '등록 후 첫 실행 대기',
      enabled: true,
      jiraSpace: jiraSelect.value || undefined,
    };
    state.autonomousTasks.unshift(newTask);
    showAddForm = false;
    showToast(`"${name}" 자율업무가 추가되었습니다 (목업 — 실제로 저장되지 않습니다)`);
    notifyChange();
  });

  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  return el('div', { className: 'settings-card task-add-card' }, [form]);
}

// ---------------- 진행상황판 탭 ----------------

export function renderAutonomousTaskBoardView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['자율업무']), el('div', { className: 'page-subtitle' }, ['진행상황판 — 최근 실행 이력 샘플'])]),
  ]);

  const board = el(
    'div',
    { className: 'task-board' },
    BOARD_STATUSES.map((status) => {
      const runs = MOCK_TASK_RUNS.filter((r) => r.status === status);
      return el('div', { className: 'task-board-column' }, [
        el('div', { className: 'task-board-column-head' }, [el('span', {}, [status]), el('span', { className: 'task-board-count' }, [String(runs.length)])]),
        el('div', { className: 'task-board-cards' }, runs.length > 0 ? runs.map(renderRunCard) : [el('div', { className: 'task-board-empty' }, ['없음'])]),
      ]);
    })
  );

  return el('div', {}, [header, tabsRow('board'), board]);
}

function statusClass(status: TaskRunStatus): string {
  const map: Readonly<Record<TaskRunStatus, string>> = { 진행중: 'running', 성공: 'ok', 실패: 'fail', 대기중: 'pending' };
  return map[status];
}

function renderRunCard(run: TaskRun): HTMLElement {
  return el('div', { className: `task-run-card status-${statusClass(run.status)}` }, [
    el('div', { className: 'task-run-name' }, [run.taskName]),
    el('div', { className: 'task-run-time' }, [`시작 ${run.startedAt}`]),
    ...(run.endedAt ? [el('div', { className: 'task-run-time' }, [`종료 ${run.endedAt}`])] : []),
  ]);
}

// ---------------- 상세 화면 ----------------

function overviewRow(label: string, body: string): HTMLElement {
  return el('div', { className: 'overview-section' }, [el('div', { className: 'overview-label' }, [label]), el('div', { className: 'overview-body' }, [body])]);
}

export function renderAutonomousTaskDetailView(taskId: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/tasks') }, ['← 자율업무']);
  const task = state.autonomousTasks.find((t) => t.id === taskId);

  if (!task) {
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['자율업무를 찾을 수 없습니다'])])]);
  }

  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [task.name]), el('div', { className: 'detail-path' }, [task.scheduleLabel])]),
    el('span', { className: `badge ${task.enabled ? 'badge-active' : 'badge-archived'}` }, [task.enabled ? '실행 중' : '중지됨']),
  ]);

  const overviewRows = [
    overviewRow('설명', task.description),
    overviewRow('마지막 실행', task.lastRunLabel ?? '없음'),
    overviewRow('다음 실행', task.nextRunLabel),
    ...(task.jiraSpace ? [overviewRow('Jira 스페이스', task.jiraSpace)] : []),
  ];
  const overview = el('div', { className: 'overview-card' }, overviewRows);

  const sections: HTMLElement[] = [header, overview];

  if (task.history && task.history.length > 0) {
    sections.push(
      el('div', { className: 'overview-card' }, [
        el('div', { className: 'overview-label' }, ['실행 이력']),
        el(
          'div',
          { className: 'task-history-list' },
          task.history.map((h) =>
            el('div', { className: 'task-history-row' }, [
              el('span', { className: 'task-history-time' }, [h.at]),
              el('span', { className: `task-history-result ${h.result === '성공' ? 'ok' : 'fail'}` }, [h.result]),
            ])
          )
        ),
      ])
    );
  }

  return el('div', {}, [back, ...sections]);
}
