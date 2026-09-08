// 메인 창 렌더러 — 프레임워크 없는 바닐라 TS(architecture.md §2.8 "프레임워크 없는
// 바닐라 TS도 유지"). `contextIsolation:true`라 `window.malgn`(preload.ts가
// `contextBridge`로 노출)만 쓸 수 있다. `node:*`·`electron`을 직접 import하지 않는다
// (여기서 import하면 브라우저 번들이 깨진다 — 타입만 type-only import로 가져온다).
//
// [XSS 방어] STATUS.md 원문·프로젝트명은 로컬 파일에서 온 값이라도 "외부 통제
// 문자열"에 준해 다룬다(§2.8 "텍스트로만 렌더" 원칙을 이 화면에도 적용) — 어떤 값도
// innerHTML로 넣지 않고 항상 `textContent`/`el()` 헬퍼로만 DOM에 삽입한다.

import type { ProjectArchiveStatus, ProjectDetailResult, ProjectListItem } from '../ipcContract.js';

declare global {
  interface Window {
    malgn: {
      listProjects: () => Promise<readonly ProjectListItem[]>;
      getProjectDetail: (projectPath: string) => Promise<ProjectDetailResult>;
    };
  }
}

type Filter = 'all' | 'active' | 'archived';

interface AppState {
  projects: readonly ProjectListItem[];
  loading: boolean;
  error: string | null;
  filter: Filter;
  detail: {
    loading: boolean;
    error: string | null;
    data: Extract<ProjectDetailResult, { ok: true }> | null;
  };
}

const state: AppState = {
  projects: [],
  loading: true,
  error: null,
  filter: 'all',
  detail: { loading: false, error: null, data: null },
};

interface ElOptions {
  readonly className?: string;
  readonly onClick?: () => void;
  readonly onKeydown?: (e: KeyboardEvent) => void;
  readonly disabled?: boolean;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  opts: ElOptions = {},
  children: readonly (Node | string)[] = []
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (opts.className) node.className = opts.className;
  if (opts.onClick) node.addEventListener('click', opts.onClick);
  if (opts.onKeydown) node.addEventListener('keydown', opts.onKeydown as EventListener);
  if (opts.disabled && 'disabled' in node) (node as unknown as { disabled: boolean }).disabled = true;
  for (const child of children) {
    node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
  }
  return node;
}

const BADGE_META: Readonly<Record<ProjectArchiveStatus, { readonly label: string; readonly cls: string }>> = {
  active: { label: '진행', cls: 'badge-active' },
  archived: { label: '보관', cls: 'badge-archived' },
  unknown: { label: '상태 없음', cls: 'badge-unknown' },
};

function badge(status: ProjectArchiveStatus): HTMLSpanElement {
  const meta = BADGE_META[status];
  return el('span', { className: `badge ${meta.cls}` }, [meta.label]);
}

function currentProjectPathFromHash(): string | null {
  const hash = window.location.hash;
  const prefix = '#/project/';
  if (!hash.startsWith(prefix)) return null;
  try {
    return decodeURIComponent(hash.slice(prefix.length));
  } catch {
    return null;
  }
}

function navigateToProject(path: string): void {
  window.location.hash = `#/project/${encodeURIComponent(path)}`;
}

function navigateToList(): void {
  window.location.hash = '#/';
}

// ---------------- 사이드바 ----------------

function renderSidebar(): HTMLElement {
  return el('aside', { className: 'sidebar' }, [
    el('div', { className: 'sidebar-brand' }, [el('span', { className: 'sidebar-brand-mark' }, ['M']), 'Malgn']),
    el('div', { className: 'sidebar-nav-item active' }, ['대시보드']),
  ]);
}

// ---------------- 목록 화면 ----------------

function filteredProjects(): readonly ProjectListItem[] {
  if (state.filter === 'all') return state.projects;
  return state.projects.filter((p) => p.archiveStatus === state.filter);
}

async function loadProjects(): Promise<void> {
  state.loading = true;
  state.error = null;
  render();
  try {
    state.projects = await window.malgn.listProjects();
  } catch (err) {
    state.error = err instanceof Error ? err.message : '프로젝트 목록을 불러오지 못했습니다.';
  } finally {
    state.loading = false;
    render();
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

function renderProjectCard(project: ProjectListItem): HTMLElement {
  const goToProject = (): void => navigateToProject(project.path);
  const card = el(
    'div',
    {
      className: 'project-card',
      onClick: goToProject,
      onKeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') goToProject();
      },
    },
    [
      el('div', { className: 'project-card-top' }, [el('span', { className: 'project-card-name' }, [project.name]), badge(project.archiveStatus)]),
      el('div', { className: 'project-card-footer' }, ['상세 보기 →']),
    ]
  );
  card.setAttribute('role', 'button');
  card.setAttribute('tabindex', '0');
  return card;
}

function renderListView(): HTMLElement {
  const list = filteredProjects();

  const refreshBtn = el('button', { className: 'btn', onClick: () => void loadProjects(), disabled: state.loading }, [
    state.loading ? '새로고침 중…' : '↻ 새로고침',
  ]);

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['대시보드']),
      ...(state.loading || state.error ? [] : [el('div', { className: 'page-subtitle' }, [`내 프로젝트 ${state.projects.length}개`])]),
    ]),
    refreshBtn,
  ]);

  const body: HTMLElement[] = [];

  if (!state.loading && !state.error && state.projects.length > 0) {
    const filterGroup = el('div', { className: 'filter-group' });
    const filters: readonly { readonly key: Filter; readonly label: string }[] = [
      { key: 'all', label: '전체' },
      { key: 'active', label: '진행' },
      { key: 'archived', label: '보관' },
    ];
    for (const f of filters) {
      filterGroup.appendChild(
        el(
          'button',
          {
            className: `filter-btn${state.filter === f.key ? ' active' : ''}`,
            onClick: () => {
              state.filter = f.key;
              render();
            },
          },
          [f.label]
        )
      );
    }
    body.push(filterGroup);
  }

  if (state.loading) {
    body.push(renderSkeletonGrid());
  } else if (state.error) {
    const retry = el('button', { className: 'btn', onClick: () => void loadProjects() }, ['다시 시도']);
    body.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.error}`]), retry]));
  } else if (list.length > 0) {
    const grid = el('div', { className: 'project-grid' });
    for (const p of list) grid.appendChild(renderProjectCard(p));
    body.push(grid);
  } else if (state.projects.length > 0) {
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

// ---------------- 상세 화면 ----------------

async function loadProjectDetail(path: string): Promise<void> {
  state.detail = { loading: true, error: null, data: null };
  render();
  try {
    const result = await window.malgn.getProjectDetail(path);
    if (result.ok) {
      state.detail = { loading: false, error: null, data: result };
    } else {
      state.detail = { loading: false, error: '프로젝트 정보를 불러오지 못했습니다.', data: null };
    }
  } catch (err) {
    state.detail = { loading: false, error: err instanceof Error ? err.message : '프로젝트 정보를 불러오지 못했습니다.', data: null };
  }
  render();
}

function overviewSection(label: string, body: string | null, danger = false): HTMLElement | null {
  if (!body) return null;
  return el('div', { className: 'overview-section' }, [
    el('div', { className: `overview-label${danger ? ' danger' : ''}` }, [label]),
    el('div', { className: 'overview-body' }, [body]),
  ]);
}

function renderDetailView(path: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigateToList() }, ['← 대시보드']);

  if (state.detail.loading) {
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])])]);
  }

  if (state.detail.error || !state.detail.data) {
    const retry = el('button', { className: 'btn', onClick: () => void loadProjectDetail(path) }, ['다시 시도']);
    return el('div', {}, [back, el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${state.detail.error ?? '알 수 없는 오류'}`]), retry])]);
  }

  const { name, path: projectPath, hasStatus, sections } = state.detail.data;

  const header = el('div', { className: 'detail-header' }, [
    el('div', {}, [el('h1', { className: 'detail-title' }, [name]), el('div', { className: 'detail-path' }, [projectPath])]),
    badge(hasStatus ? (sections?.parsed ? 'active' : 'unknown') : 'unknown'),
  ]);

  if (!hasStatus) {
    return el('div', {}, [
      back,
      header,
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['상태 없음']),
        el('div', { className: 'state-block-desc' }, ['이 프로젝트에는 STATUS.md가 없습니다(gitignore 대상일 수 있습니다).']),
      ]),
    ]);
  }

  if (!sections || !sections.parsed) {
    // 파싱 실패 폴백 — 원문을 그대로 보여준다("죽지 말고 원문을 그대로 보여주는 폴백").
    return el('div', {}, [
      back,
      header,
      el('div', { className: 'raw-fallback-note' }, ['STATUS.md 구조를 인식하지 못해 원문을 그대로 표시합니다.']),
      el('pre', { className: 'raw-fallback-pre' }, [sections?.raw ?? '']),
    ]);
  }

  const sectionEls = [
    overviewSection('현재 상태', sections.current),
    overviewSection('최근 완료', sections.recentDone),
    overviewSection('진행 중', sections.inProgress),
    overviewSection('막힌 것', sections.blocked, true),
  ].filter((n): n is HTMLElement => n !== null);

  return el('div', {}, [
    back,
    header,
    el('div', { className: 'overview-card' }, sectionEls.length > 0 ? sectionEls : [el('div', { className: 'overview-body' }, ['표시할 섹션이 없습니다.'])]),
  ]);
}

// ---------------- 라우팅 + 렌더 루프 ----------------

function render(): void {
  const root = document.getElementById('app');
  if (!root) return;
  root.replaceChildren();

  const projectPath = currentProjectPathFromHash();
  const content = el('main', { className: 'content' }, [projectPath ? renderDetailView(projectPath) : renderListView()]);

  root.appendChild(renderSidebar());
  root.appendChild(content);
}

function route(): void {
  const projectPath = currentProjectPathFromHash();
  if (projectPath) {
    void loadProjectDetail(projectPath);
    return;
  }
  render();
  if (state.projects.length === 0) void loadProjects();
}

window.addEventListener('hashchange', route);
window.addEventListener('DOMContentLoaded', route);
