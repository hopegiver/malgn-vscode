// "프로젝트" — 사이드바+카드그리드+필터+상세패널 화면(옛 이름 "대시보드").
// 이 화면은 이제 목업이 아니다 — Rust 커맨드 list_workspace_projects()가 실제
// ~/workspace 아래를 스캔한 결과를 그대로 쓴다(workspaceApi.ts, src-tauri/src/lib.rs
// 주석 참고). "세션목록"에 이은 이 앱의 두 번째 실동작 화면.
import { el, clickable, showToast, createModalOverlay, boundField, restoreModalFocus } from '../dom';
import { state, notifyChange } from '../state';
import type { ArchiveStatus } from '../state';
import { fetchWorkspaceProjects, fetchProjectTree, fetchFilePreview } from '../workspaceApi';
import type { WorkspaceProject, ProjectTreeNode } from '../workspaceApi';
import { saveMalgnAgentConfig } from '../configApi';
import type { MalgnAgentConfigInput, MalgnAgentConfigStatus } from '../configApi';
import { loadMalgnAgentConfigStatus } from './autonomousTasks';
import { describeWorkspaceScanScope, summarizeSkippedProjects } from '../workspaceScanHint';
import { navigate } from '../route';

const BADGE_META: Readonly<Record<ArchiveStatus, { readonly label: string; readonly cls: string }>> = {
  active: { label: '진행', cls: 'badge-active' },
  archived: { label: '보관', cls: 'badge-archived' },
  unknown: { label: '상태 없음', cls: 'badge-unknown' },
};

function badge(status: ArchiveStatus): HTMLSpanElement {
  const meta = BADGE_META[status];
  return el('span', { className: `badge ${meta.cls}` }, [meta.label]);
}

function filteredProjects(): readonly WorkspaceProject[] {
  const filtered =
    state.dashboard.filter === 'all' ? state.dashboard.projects : state.dashboard.projects.filter((p) => p.archiveStatus === state.dashboard.filter);
  const sorted = [...filtered];
  if (state.dashboard.sort === 'name') {
    sorted.sort((a, b) => a.name.localeCompare(b.name, 'ko'));
  } else {
    sorted.sort((a, b) => b.updatedAt - a.updatedAt);
  }
  return sorted;
}

// 사이드바 서브목록 전용 — 사용자가 프로젝트 화면에서 고른 정렬(state.dashboard.sort,
// 이름순일 수도 있음)과 무관하게 항상 최신 업데이트순으로 보여준다(세션목록
// 사이드바의 sortedSessions()와 같은 성격). updatedAt은 Rust list_workspace_projects()가
// CLAUDE.md/STATUS.md 중 더 최근 mtime으로 채우는 실측 필드다(src-tauri/src/lib.rs).
export function sortedProjectsByRecency(): readonly WorkspaceProject[] {
  return [...state.dashboard.projects].sort((a, b) => b.updatedAt - a.updatedAt);
}

export async function loadProjects(): Promise<void> {
  state.dashboard.loading = true;
  state.dashboard.error = null;
  notifyChange();
  try {
    const result = await fetchWorkspaceProjects();
    state.dashboard.projects = result.projects;
    state.dashboard.skipped = result.skipped;
    state.dashboard.loaded = true;
  } catch (err) {
    state.dashboard.error = err instanceof Error ? err.message : '프로젝트 목록을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.dashboard.loading = false;
    notifyChange();
  }

  // 전역 설정(workspaces 편집)을 이 화면에서도 쓴다 — 자율업무 화면을 아직
  // 한 번도 안 열었으면 state.malgnAgentConfig.status가 비어 있으므로 여기서도
  // 독립적으로 갱신한다(목록 조회 실패와 무관하게).
  if (!state.malgnAgentConfig.loaded) void loadMalgnAgentConfigStatus();
}

function renderSkeletonGrid(): HTMLElement {
  const grid = el('div', { className: 'skeleton-grid' });
  for (let i = 0; i < 6; i++) {
    const line1 = el('div', { className: 'skeleton-line' });
    line1.style.width = '55%';
    line1.style.height = '15px';
    line1.style.marginBottom = '10px';
    const line2 = el('div', { className: 'skeleton-line' });
    line2.style.width = '35%';
    line2.style.height = '11px';
    grid.appendChild(el('div', { className: 'skeleton-card' }, [line1, line2]));
  }
  return grid;
}

// 카드 자체는 비-인터랙티브 컨테이너다(role="button"/tabindex/전체 onClick을
// 걷어냈다) — "상세 보기"와 "새 세션"을 각각 독립된 형제 버튼으로 두어 중첩
// 인터랙티브 요소(카드 안에 버튼)를 만들지 않는다. 두 버튼 모두 네이티브
// <button>이라 Tab으로 각각 개별 접근되고 Enter/Space로 각각 독립 실행된다.
function renderProjectCard(project: WorkspaceProject): HTMLElement {
  const goToProject = (): void => navigate(`#/project/${encodeURIComponent(project.path)}`);
  const goToNewSession = (): void => navigate(`#/sessions/new/${encodeURIComponent(project.path)}`);

  return el('div', { className: 'project-card' }, [
    el('div', { className: 'project-card-top' }, [el('span', { className: 'project-card-name' }, [project.name]), badge(project.archiveStatus)]),
    el('div', { className: 'project-card-desc' }, [project.path]),
    el('div', { className: 'project-card-footer' }, [
      el('button', { className: 'project-card-link', onClick: goToProject }, ['상세 보기 →']),
      el('button', { className: 'btn btn-sm', onClick: goToNewSession }, ['+ 새 세션']),
    ]),
  ]);
}

// 세션 메타데이터 모달(sessions.ts)과 동일한 패턴 — 열림 상태는 기존 전역
// state(state.malgnAgentConfig.editingWorkspaces)를 재사용하고, ESC 리스너만
// 모듈 스코프로 둔다.
let workspacesModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachWorkspacesModalEscHandler(): void {
  if (workspacesModalEscHandler) {
    window.removeEventListener('keydown', workspacesModalEscHandler);
    workspacesModalEscHandler = null;
  }
}

function closeWorkspacesModal(): void {
  state.malgnAgentConfig.editingWorkspaces = false;
  workspacesDraft = null;
  detachWorkspacesModalEscHandler();
  restoreModalFocus(closeWorkspacesModal); // C3: 열기 직전 포커스로 복원
  notifyChange();
}

// 입력 중 드래프트 — 모달이 열려 있는 동안(editingWorkspaces=true) 한 번만
// status.workspaces로 초기화되고, 닫힐 때(closeWorkspacesModal) 버려진다.
// 배경 이벤트로 인한 재렌더는 이 값을 건드리지 않으므로 타이핑 중이던 내용이
// 보존된다(dom.ts의 boundField 참고).
let workspacesDraft: string | null = null;

// 프로젝트 화면을 완전히 떠날 때(다른 라우트로 이동) main.ts에서 호출한다 —
// 모달이 열려 있었다면 window에 남은 ESC 리스너를 정리한다.
export function leaveProjectsListView(): void {
  state.malgnAgentConfig.editingWorkspaces = false;
  detachWorkspacesModalEscHandler();
}

// 전역 설정(malgn-agent.json)의 workspace 루트 수준 경고(존재하지 않음/디렉터리
// 아님/중복 등, config/user_config.rs::validate_workspace_entries)를 보여준다.
// 자율업무 화면(renderConfigStatusBanner)에는 이미 떠 있었지만 이 화면에는
// 없었다 — 실제 사용자가 "프로젝트가 안 보인다"를 겪은 화면이 여기라 먼저
// 띄운다(루트 안쪽 개별 폴더 제외 사유는 아래 빈 상태 블록의 별도 채널이
// 맡는다 — 중복 표시 방지를 위한 역할 분리).
function renderWorkspaceWarningsBanner(): HTMLElement | null {
  const status = state.malgnAgentConfig.status;
  if (!status || !status.ok || status.warnings.length === 0) return null;
  return el('div', { className: 'alert' }, [`⚠ 전역 설정 경고: ${status.warnings.join(' / ')}`]);
}

// 헤더의 "workspace 설정" 버튼 — 항상 열기만 하는 단일 버튼이다(닫기는 모달
// 자체의 X/배경클릭/ESC로만 한다). 전역 설정이 로드돼 있고 에러가 없을 때만
// 노출한다(자율업무 화면의 renderConfigEditToggleBtn과 동일한 조건).
function renderWorkspacesToggleBtn(): HTMLElement | null {
  const cfg = state.malgnAgentConfig;
  if (cfg.error || !cfg.status || !cfg.status.ok) return null;
  return el(
    'button',
    {
      className: 'btn',
      onClick: () => {
        state.malgnAgentConfig.editingWorkspaces = true;
        notifyChange();
      },
    },
    ['workspace 설정']
  );
}

// 자율업무 화면의 handleSaveMalgnAgentConfig와는 별도 함수다 — 그쪽은 성공 시
// editingAutonomy를 닫는데, 이 화면은 editingWorkspaces를 닫아야 한다. 저장
// API가 4개 필드 전체를 항상 덮어쓰는 풀 오버라이트라 이 화면이 건드리지
// 않는 autonomy/logs는 현재 로드된 status 값을 그대로 실어 보낸다.
async function handleSaveWorkspaces(workspaces: readonly string[]): Promise<void> {
  const current = state.malgnAgentConfig.status;
  state.malgnAgentConfig.saving = true;
  notifyChange();
  try {
    const payload: MalgnAgentConfigInput = {
      workspaces,
      autonomy: current?.autonomy ?? { concurrency: 1, defaultTimeout: 30 },
      logs: current?.logs ?? { retentionDays: 30 },
    };
    state.malgnAgentConfig.status = await saveMalgnAgentConfig(payload);
    closeWorkspacesModal();
    showToast('전역 설정을 저장했습니다.');
  } catch (err) {
    showToast(`전역 설정 저장에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.malgnAgentConfig.saving = false;
    notifyChange();
  }
}

function renderWorkspacesEditForm(status: MalgnAgentConfigStatus): HTMLElement {
  if (workspacesDraft === null) workspacesDraft = status.workspaces.join('\n');

  const workspacesInput = boundField(
    document.createElement('textarea'),
    () => workspacesDraft ?? '',
    (v) => {
      workspacesDraft = v;
    }
  );
  workspacesInput.id = 'malgn-config-workspaces';
  workspacesInput.className = 'settings-input';
  workspacesInput.rows = 4;
  workspacesInput.autocomplete = 'off';

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, [`전역 설정 파일(${status.configPath})의 workspace 목록을 직접 수정합니다.`]),
    el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['Workspaces (한 줄에 하나씩)']),
      workspacesInput,
      el('div', { className: 'settings-form-hint' }, ['자율 작업·프로젝트 화면이 스캔할 절대경로를 한 줄에 하나씩 입력하세요. 빈 줄은 무시됩니다.']),
    ]),
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const workspaces = workspacesInput.value
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    void handleSaveWorkspaces(workspaces);
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
      onClick: closeWorkspacesModal,
    },
    ['취소']
  );
  cancelBtn.type = 'button';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn, cancelBtn]));

  return form;
}

// sessions.ts의 renderMetaModal과 동일한 구조 — 배경 클릭·ESC·닫기 버튼 3가지
// 경로로 닫힌다. 오버레이 생성은 dom.ts의 createModalOverlay로 공용화했다.
function renderWorkspacesModal(status: MalgnAgentConfigStatus): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['Workspace 설정']),
      el('button', { className: 'modal-close-btn', onClick: closeWorkspacesModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderWorkspacesEditForm(status)]),
  ]);
  return createModalOverlay(modalBox, closeWorkspacesModal);
}

export function renderProjectsListView(): HTMLElement {
  const list = filteredProjects();

  const refreshBtn = el(
    'button',
    { className: 'btn', onClick: () => void loadProjects(), disabled: state.dashboard.loading },
    [state.dashboard.loading ? '새로고침 중…' : '↻ 새로고침']
  );
  const workspacesToggleBtn = renderWorkspacesToggleBtn();

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['프로젝트']),
      ...(state.dashboard.loading || state.dashboard.error
        ? []
        : [el('div', { className: 'page-subtitle' }, [`내 프로젝트 ${state.dashboard.projects.length}개`])]),
    ]),
    el('div', { className: 'devtool-header-actions' }, [refreshBtn, ...(workspacesToggleBtn ? [workspacesToggleBtn] : [])]),
  ]);

  const body: HTMLElement[] = [];

  const warningsBanner = renderWorkspaceWarningsBanner();
  if (warningsBanner) body.push(warningsBanner);

  if (!state.dashboard.loading && !state.dashboard.error && state.dashboard.projects.length > 0) {
    const filterGroup = el('div', { className: 'filter-group' });
    const filters: readonly { readonly key: typeof state.dashboard.filter; readonly label: string }[] = [
      { key: 'all', label: '전체' },
      { key: 'active', label: '진행' },
      { key: 'archived', label: '보관' },
    ];
    for (const f of filters) {
      filterGroup.appendChild(
        el(
          'button',
          {
            className: `filter-btn${state.dashboard.filter === f.key ? ' active' : ''}`,
            onClick: () => {
              state.dashboard.filter = f.key;
              notifyChange();
            },
          },
          [f.label]
        )
      );
    }

    const sortGroup = el('div', { className: 'filter-group' });
    const sorts: readonly { readonly key: typeof state.dashboard.sort; readonly label: string }[] = [
      { key: 'updated', label: '최신 업데이트' },
      { key: 'name', label: '이름순' },
    ];
    for (const s of sorts) {
      sortGroup.appendChild(
        el(
          'button',
          {
            className: `filter-btn${state.dashboard.sort === s.key ? ' active' : ''}`,
            onClick: () => {
              state.dashboard.sort = s.key;
              notifyChange();
            },
          },
          [s.label]
        )
      );
    }

    body.push(el('div', { className: 'filter-row' }, [filterGroup, sortGroup]));
  }

  if (state.dashboard.loading) {
    body.push(renderSkeletonGrid());
  } else if (state.dashboard.error) {
    const retry = el('button', { className: 'btn', onClick: () => void loadProjects() }, ['다시 시도']);
    body.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.dashboard.error}`]), retry]));
  } else if (list.length > 0) {
    const grid = el('div', { className: 'project-grid' });
    for (const p of list) grid.appendChild(renderProjectCard(p));
    body.push(grid);
  } else if (state.dashboard.projects.length > 0) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['해당 조건의 프로젝트가 없습니다'])]));
  } else {
    const emptyStateChildren: HTMLElement[] = [
      el('div', { className: 'state-block-title' }, ['아직 인식된 프로젝트가 없습니다']),
      el('div', { className: 'state-block-desc' }, [describeWorkspaceScanScope()]),
    ];
    // 후보 폴더를 찾았지만 CLAUDE.md가 없어(또는 workspace 루트를 읽지 못해)
    // 제외된 경우 그 사실과 사유를 보여준다 — 예전엔 이 경로가 조용히 "프로젝트
    // 없음"으로만 보였다(hub 이슈 01m2wm499xmh3046rnvx4cyn8n).
    const skipSummary = summarizeSkippedProjects(state.dashboard.skipped);
    if (skipSummary) {
      emptyStateChildren.push(el('div', { className: 'state-block-desc' }, [skipSummary]));
    }
    body.push(el('div', { className: 'state-block' }, emptyStateChildren));
  }

  const rootChildren: HTMLElement[] = [header, ...body];
  if (state.malgnAgentConfig.editingWorkspaces && state.malgnAgentConfig.status?.ok) {
    if (!workspacesModalEscHandler) {
      workspacesModalEscHandler = (e) => {
        if (e.key === 'Escape') closeWorkspacesModal();
      };
      window.addEventListener('keydown', workspacesModalEscHandler);
    }
    rootChildren.push(renderWorkspacesModal(state.malgnAgentConfig.status));
  } else {
    detachWorkspacesModalEscHandler();
  }

  return el('div', {}, rootChildren);
}

export function renderProjectsDetailView(path: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/projects') }, ['← 프로젝트']);

  const project = state.dashboard.projects.find((p) => p.path === path);

  if (!project) {
    if (state.dashboard.loading) {
      return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])])]);
    }
    if (state.dashboard.error) {
      const retry = el('button', { className: 'btn', onClick: () => void loadProjects() }, ['다시 시도']);
      return el('div', {}, [back, el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.dashboard.error}`]), retry])]);
    }
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['프로젝트를 찾을 수 없습니다'])])]);
  }

  const { name, path: projectPath, hasStatus, sections } = project;

  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [name]), el('div', { className: 'detail-path' }, [projectPath])]),
    badge(hasStatus ? (sections?.parsed ? project.archiveStatus : 'unknown') : 'unknown'),
  ]);

  // STATUS.md 요약은 오른쪽 트리 미리보기 패널에 기본으로 열리므로(loadProjectTree
  // 참고) 여기서는 STATUS.md 자체가 없는 프로젝트에 대한 짧은 안내만 남긴다.
  const statusEls: HTMLElement[] = [];
  if (!hasStatus) {
    statusEls.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['상태 없음']),
        el('div', { className: 'state-block-desc' }, ['이 프로젝트에는 STATUS.md가 없습니다(gitignore 대상일 수 있습니다).']),
      ])
    );
  }

  return el('div', {}, [back, header, ...statusEls, renderProjectTreeSection(projectPath)]);
}

// ---------------- 폴더 구조 + 파일 미리보기 (실제 로컬 파일, 읽기 전용) ----------------

export async function loadProjectTree(projectPath: string): Promise<void> {
  state.projectTree.projectPath = projectPath;
  state.projectTree.loading = true;
  state.projectTree.error = null;
  state.projectTree.nodes = [];
  state.projectTree.expanded = {};
  state.projectTree.selectedPath = null;
  state.projectTree.preview = null;
  state.projectTree.previewError = null;
  notifyChange();
  try {
    state.projectTree.nodes = await fetchProjectTree(projectPath);
  } catch (err) {
    state.projectTree.error = err instanceof Error ? err.message : '폴더 구조를 불러오지 못했습니다.';
  } finally {
    state.projectTree.loading = false;
    notifyChange();
  }

  // 상세보기 기본 미리보기: 루트 레벨(depth 0)에 STATUS.md 파일이 있으면 자동으로
  // 한 번 열어둔다. 이 로드 함수가 진입/새로고침 시 selectedPath를 이미 null로
  // 리셋해두었으므로, 사용자가 이후 다른 파일을 수동으로 클릭해도 이 호출로
  // 덮어써지지 않는다(loadProjectTree가 다시 호출되기 전까지는 1회성).
  const statusNode = state.projectTree.nodes.find((n) => !n.isDirectory && n.name === 'STATUS.md');
  if (statusNode) {
    await loadFilePreview(projectPath, statusNode.relativePath);
  }
}

async function loadFilePreview(projectPath: string, relativePath: string): Promise<void> {
  state.projectTree.selectedPath = relativePath;
  state.projectTree.previewLoading = true;
  state.projectTree.previewError = null;
  state.projectTree.preview = null;
  notifyChange();
  try {
    state.projectTree.preview = await fetchFilePreview(projectPath, relativePath);
  } catch (err) {
    state.projectTree.previewError = err instanceof Error ? err.message : '파일을 불러오지 못했습니다.';
  } finally {
    state.projectTree.previewLoading = false;
    notifyChange();
  }
}

function renderProjectTreeSection(projectPath: string): HTMLElement {
  const treePanel: HTMLElement[] = [el('div', { className: 'overview-label' }, ['폴더 구조'])];

  if (state.projectTree.loading) {
    treePanel.push(el('div', { className: 'state-block-desc' }, ['불러오는 중…']));
  } else if (state.projectTree.error) {
    treePanel.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.projectTree.error}`])]));
  } else if (state.projectTree.nodes.length === 0) {
    treePanel.push(el('div', { className: 'state-block-desc' }, ['빈 폴더입니다.']));
  } else {
    treePanel.push(el('div', { className: 'tree-root' }, state.projectTree.nodes.map((n) => renderTreeNode(n, projectPath, 0))));
  }

  const previewPanel: HTMLElement[] = [
    el('div', { className: 'overview-label' }, [state.projectTree.selectedPath ? `미리보기 — ${state.projectTree.selectedPath}` : '미리보기']),
    renderFilePreviewBody(),
  ];

  return el('div', { className: 'project-tree-layout' }, [
    el('div', { className: 'project-tree-panel' }, treePanel),
    el('div', { className: 'project-tree-panel' }, previewPanel),
  ]);
}

function renderTreeNode(node: ProjectTreeNode, projectPath: string, depth: number): HTMLElement {
  if (!node.isDirectory) {
    const isSelected = state.projectTree.selectedPath === node.relativePath;
    const row = el('div', { className: `tree-row tree-file${isSelected ? ' active' : ''}` }, [
      el('span', { className: 'tree-icon' }, ['📄']),
      el('span', { className: 'tree-name' }, [node.name]),
    ]);
    row.style.paddingLeft = `${depth * 16 + 8}px`;
    return clickable(row, () => void loadFilePreview(projectPath, node.relativePath));
  }

  const expanded = state.projectTree.expanded[node.relativePath] ?? false;
  const header = el(
    'div',
    {
      className: 'tree-row tree-dir',
      onClick: () => {
        state.projectTree.expanded[node.relativePath] = !expanded;
        notifyChange();
      },
    },
    [
      el('span', { className: 'tree-icon' }, [expanded ? '📂' : '📁']),
      el('span', { className: 'tree-name' }, [node.name]),
      ...(node.truncated ? [el('span', { className: 'tree-truncated-note' }, [node.children === null ? '(더 깊은 항목 생략됨)' : '(제외됨)'])] : []),
    ]
  );
  header.style.paddingLeft = `${depth * 16 + 8}px`;

  const groupChildren: HTMLElement[] = [header];
  if (expanded && node.children && node.children.length > 0) {
    for (const child of node.children) groupChildren.push(renderTreeNode(child, projectPath, depth + 1));
  }
  return el('div', { className: 'tree-node-group' }, groupChildren);
}

function renderFilePreviewBody(): HTMLElement {
  if (!state.projectTree.selectedPath) {
    return el('div', { className: 'state-block-desc' }, ['왼쪽 트리에서 파일을 클릭하면 미리보기가 표시됩니다.']);
  }
  if (state.projectTree.previewLoading) {
    return el('div', { className: 'state-block-desc' }, ['불러오는 중…']);
  }
  if (state.projectTree.previewError) {
    return el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.projectTree.previewError}`])]);
  }
  const preview = state.projectTree.preview;
  if (!preview) return el('div', { className: 'state-block-desc' }, ['-']);

  switch (preview.kind) {
    case 'text':
      return el('pre', { className: 'raw-fallback-pre file-preview-pre' }, [preview.content]);
    case 'tooLarge':
      return el('div', { className: 'state-block-desc' }, [`파일이 너무 커서(${Math.round(preview.size / 1024)}KB) 미리보기를 지원하지 않습니다.`]);
    case 'binary':
      return el('div', { className: 'state-block-desc' }, ['미리보기를 지원하지 않는 파일 형식입니다.']);
    case 'notFound':
      return el('div', { className: 'state-block-desc' }, ['파일을 찾을 수 없습니다.']);
    case 'denied':
      return el('div', { className: 'state-block-desc' }, ['이 파일은 접근이 거부되었습니다.']);
  }
}
