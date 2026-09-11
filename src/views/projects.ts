// "프로젝트" — 사이드바+카드그리드+필터+상세패널 화면(옛 이름 "대시보드").
// 이 화면은 이제 목업이 아니다 — Rust 커맨드 list_workspace_projects()가 실제
// ~/workspace 아래를 스캔한 결과를 그대로 쓴다(workspaceApi.ts, src-tauri/src/lib.rs
// 주석 참고). "세션목록"에 이은 이 앱의 두 번째 실동작 화면.
import { el } from '../dom';
import { state, notifyChange } from '../state';
import type { ArchiveStatus } from '../state';
import { fetchWorkspaceProjects, fetchProjectTree, fetchFilePreview } from '../workspaceApi';
import type { WorkspaceProject, ProjectTreeNode } from '../workspaceApi';
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
    state.dashboard.projects = await fetchWorkspaceProjects();
  } catch (err) {
    state.dashboard.error = err instanceof Error ? err.message : '프로젝트 목록을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.dashboard.loading = false;
    notifyChange();
  }
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

export function renderProjectsListView(): HTMLElement {
  const list = filteredProjects();

  const refreshBtn = el(
    'button',
    { className: 'btn', onClick: () => void loadProjects(), disabled: state.dashboard.loading },
    [state.dashboard.loading ? '새로고침 중…' : '↻ 새로고침']
  );

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['프로젝트']),
      ...(state.dashboard.loading || state.dashboard.error
        ? []
        : [el('div', { className: 'page-subtitle' }, [`내 프로젝트 ${state.dashboard.projects.length}개`])]),
    ]),
    refreshBtn,
  ]);

  const body: HTMLElement[] = [];

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
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['아직 인식된 프로젝트가 없습니다']),
        el('div', { className: 'state-block-desc' }, ['~/workspace 아래 CLAUDE.md가 있는 폴더가 malgn-agent 프로젝트로 표시됩니다.']),
      ])
    );
  }

  return el('div', {}, [header, ...body]);
}

export function renderProjectsDetailView(path: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/projects') }, ['← 프로젝트']);

  const project = state.dashboard.projects.find((p) => p.path === path);

  if (!project) {
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
    const row = el(
      'div',
      { className: `tree-row tree-file${isSelected ? ' active' : ''}`, onClick: () => void loadFilePreview(projectPath, node.relativePath) },
      [el('span', { className: 'tree-icon' }, ['📄']), el('span', { className: 'tree-name' }, [node.name])]
    );
    row.style.paddingLeft = `${depth * 16 + 8}px`;
    row.setAttribute('role', 'button');
    row.setAttribute('tabindex', '0');
    return row;
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
