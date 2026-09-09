import { el } from './dom';
import { state, notifyChange } from './state';
import type { SettingsTab } from './state';
import { navigate } from './route';
import type { Route } from './route';
import { sortedSessions, sessionTitle, asString } from './views/sessions';

const SETTINGS_TABS: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  { key: 'github', label: 'GitHub 설정' },
  { key: 'cloudflare', label: 'Cloudflare 설정' },
  { key: 'jira', label: 'Jira 설정' },
  { key: 'google', label: 'Google Workspace 설정' },
  { key: 'marketplace', label: '마켓플레이스 설정' },
];

const SIDEBAR_SUBLIST_LIMIT = 6;

// 사용 빈도순 배치. "설정"은 관례상 항상 맨 아래, 구분선으로 나머지와 분리한다.
// "프로젝트"·"세션목록"은 펼치면 실제 로컬 데이터(이미 main.ts가 로그인 직후
// 미리 불러온 state.dashboard.projects/state.sessions.items)를 서브 목록으로
// 바로 보여준다 — 화면 전환은 항목 본문 클릭, 펼침/접힘은 화살표 클릭으로 분리한다.
export function renderSidebar(route: Route): HTMLElement {
  const mainItems: HTMLElement[] = [
    navItem('대시보드', route.kind === 'home', () => navigate('#/')),
    renderProjectsGroup(route),
    renderSessionsGroup(route),
    navItem('자율업무', route.kind === 'tasks-list' || route.kind === 'tasks-board' || route.kind === 'tasks-detail', () => navigate('#/tasks')),
    navItem('사용량 통계', route.kind === 'usage', () => navigate('#/usage')),
    navItem('카탈로그', route.kind === 'catalog', () => navigate('#/catalog')),
    navItem('개발 환경', route.kind === 'dev-tools', () => navigate('#/dev-tools')),
  ];

  const settingsGroup = navGroup({
    label: '설정',
    active: route.kind === 'settings',
    expanded: state.sidebar.settingsExpanded,
    onToggle: () => {
      state.sidebar.settingsExpanded = !state.sidebar.settingsExpanded;
      notifyChange();
    },
    subItems: SETTINGS_TABS.map((t) => ({
      label: t.label,
      active: route.kind === 'settings' && route.tab === t.key,
      onClick: () => navigate(`#/settings/${t.key}`),
    })),
  });

  const logoutItem = el(
    'div',
    {
      className: 'sidebar-nav-item sidebar-logout',
      onClick: () => {
        state.authenticated = false;
        state.auth.userEmail = null;
        state.auth.userName = null;
        state.auth.error = null;
        window.location.hash = '';
        notifyChange();
      },
    },
    ['로그아웃']
  );

  return el('aside', { className: 'sidebar' }, [
    el('div', { className: 'sidebar-brand' }, [el('span', { className: 'sidebar-brand-mark' }, ['M']), '맑은에이전트']),
    el('nav', { className: 'sidebar-nav' }, mainItems),
    el('div', { className: 'sidebar-footer' }, [
      ...(state.auth.userEmail ? [el('div', { className: 'sidebar-user-email' }, [state.auth.userEmail])] : []),
      settingsGroup,
      logoutItem,
    ]),
  ]);
}

function renderProjectsGroup(route: Route): HTMLElement {
  const expanded = state.sidebar.projectsExpanded;
  const active = route.kind === 'projects-list' || route.kind === 'projects-detail';

  const header = expandableNavItem('프로젝트', active, () => navigate('#/projects'), expanded, () => {
    state.sidebar.projectsExpanded = !expanded;
    notifyChange();
  });

  const children: HTMLElement[] = [header];
  if (expanded) {
    const items = state.dashboard.projects.slice(0, SIDEBAR_SUBLIST_LIMIT).map((p) => ({
      label: p.name,
      active: route.kind === 'projects-detail' && route.path === p.path,
      onClick: () => navigate(`#/project/${encodeURIComponent(p.path)}`),
    }));
    if (items.length === 0) {
      children.push(el('div', { className: 'sidebar-subnav' }, [el('div', { className: 'sidebar-subnav-empty' }, ['불러오는 중…'])]));
    } else {
      children.push(subList(items, state.dashboard.projects.length > SIDEBAR_SUBLIST_LIMIT ? '전체 보기 →' : null, () => navigate('#/projects')));
    }
  }

  return el('div', { className: 'sidebar-nav-group' }, children);
}

function renderSessionsGroup(route: Route): HTMLElement {
  const expanded = state.sidebar.sessionsExpanded;
  const active = route.kind === 'sessions-list' || route.kind === 'sessions-detail';

  const header = expandableNavItem('세션목록', active, () => navigate('#/sessions'), expanded, () => {
    state.sidebar.sessionsExpanded = !expanded;
    notifyChange();
  });

  const children: HTMLElement[] = [header];
  if (expanded) {
    const sessions = sortedSessions().slice(0, SIDEBAR_SUBLIST_LIMIT);
    const items = sessions.map((s) => {
      const sessionId = asString(s.sessionId);
      return {
        label: sessionTitle(s),
        active: route.kind === 'sessions-detail' && route.sessionId === sessionId,
        onClick: () => navigate(`#/sessions/${encodeURIComponent(sessionId)}`),
      };
    });
    if (items.length === 0) {
      children.push(el('div', { className: 'sidebar-subnav' }, [el('div', { className: 'sidebar-subnav-empty' }, ['불러오는 중…'])]));
    } else {
      children.push(subList(items, state.sessions.items.length > SIDEBAR_SUBLIST_LIMIT ? '전체 보기 →' : null, () => navigate('#/sessions')));
    }
  }

  return el('div', { className: 'sidebar-nav-group' }, children);
}

function navItem(label: string, active: boolean, onClick: () => void): HTMLElement {
  return el('div', { className: `sidebar-nav-item${active ? ' active' : ''}`, onClick }, [label]);
}

// 헤더 본문 클릭 = 화면 전환, 화살표 클릭 = 펼침/접힘만(전환 없음). 화살표는
// el() 헬퍼가 이벤트 객체를 넘기지 않아 stopPropagation을 못 걸므로 직접 DOM으로
// 만든다 — 그래야 화살표 클릭이 상위 row의 onClick(네비게이션)까지 트리거하지 않는다.
function expandChevron(expanded: boolean, onToggle: () => void): HTMLElement {
  const span = document.createElement('span');
  span.className = 'sidebar-nav-chevron sidebar-nav-chevron-btn';
  span.textContent = expanded ? '▾' : '▸';
  span.addEventListener('click', (e) => {
    e.stopPropagation();
    onToggle();
  });
  return span;
}

function expandableNavItem(label: string, active: boolean, onClick: () => void, expanded: boolean, onToggle: () => void): HTMLElement {
  return el('div', { className: `sidebar-nav-item sidebar-nav-expandable${active ? ' active' : ''}`, onClick }, [
    el('span', { className: 'sidebar-nav-group-label' }, [label]),
    expandChevron(expanded, onToggle),
  ]);
}

function subList(
  items: readonly { readonly label: string; readonly active: boolean; readonly onClick: () => void }[],
  viewAllLabel: string | null,
  onViewAll: () => void
): HTMLElement {
  return el('div', { className: 'sidebar-subnav' }, [
    ...items.map((s) => el('div', { className: `sidebar-subnav-item${s.active ? ' active' : ''}`, onClick: s.onClick }, [s.label])),
    ...(viewAllLabel ? [el('div', { className: 'sidebar-subnav-item sidebar-subnav-viewall', onClick: onViewAll }, [viewAllLabel])] : []),
  ]);
}

interface NavGroupSpec {
  readonly label: string;
  readonly active: boolean;
  readonly expanded: boolean;
  readonly onToggle: () => void;
  readonly subItems: readonly { readonly label: string; readonly active: boolean; readonly onClick: () => void }[];
}

function navGroup(spec: NavGroupSpec): HTMLElement {
  // 수동 토글 상태이거나, 그 섹션이 현재 활성 라우트면 항상 펼쳐 보여준다.
  const expanded = spec.expanded || spec.active;

  const header = el(
    'div',
    { className: `sidebar-nav-item sidebar-nav-group-header${spec.active ? ' active' : ''}`, onClick: spec.onToggle },
    [
      el('span', { className: 'sidebar-nav-group-label' }, [spec.label]),
      el('span', { className: 'sidebar-nav-chevron' }, [expanded ? '▾' : '▸']),
    ]
  );

  const children: HTMLElement[] = [header];
  if (expanded) {
    children.push(subList(spec.subItems, null, () => {}));
  }

  return el('div', { className: 'sidebar-nav-group' }, children);
}
