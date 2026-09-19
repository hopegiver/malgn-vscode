import { el, clickable } from './dom';
import { state, notifyChange } from './state';
import type { SettingsTab, CatalogTab } from './state';
import { navigate } from './route';
import type { Route } from './route';
import { sortedSessions, sessionTitle, asString } from './views/sessions';
import { sortedProjectsByRecency } from './views/projects';
import { loadDailyUsage } from './views/usage';
import { enabledAppLinks, openLink } from './views/appLinks';
import { brandMark } from './brand';
import { applyUpdateFromButton } from './updateApi';

const SETTINGS_TABS: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  { key: 'github', label: 'GitHub 설정' },
  { key: 'cloudflare', label: 'Cloudflare 설정' },
  { key: 'marketplace', label: '마켓플레이스 설정' },
  { key: 'mcp', label: 'MCP 관리' },
  { key: 'applinks', label: '앱링크 설정' },
  { key: 'devtools', label: '개발 환경' },
];

const CATALOG_TABS: readonly { readonly key: CatalogTab; readonly label: string }[] = [
  { key: 'plugins', label: '플러그인 카탈로그' },
  { key: 'global', label: '전역 카탈로그' },
];

const SIDEBAR_SUBLIST_LIMIT = 6;

// 사용 빈도순 배치(로그아웃만 하단에 고정). "개발 환경"은 독립 메뉴가 아니라
// "설정" 하위 탭이다(설정 > 개발 환경).
// "프로젝트"·"세션목록"은 펼치면 실제 로컬 데이터(이미 main.ts가 로그인 직후
// 미리 불러온 state.dashboard.projects/state.sessions.items)를 서브 목록으로
// 바로 보여준다 — 화면 전환은 항목 본문 클릭, 펼침/접힘은 화살표 클릭으로 분리한다.
export function renderSidebar(route: Route): HTMLElement {
  const settingsGroup = navGroup({
    label: '설정',
    active: route.kind === 'settings',
    expanded: state.sidebar.settingsExpanded,
    onToggle: () => {
      state.sidebar.settingsExpanded = !state.sidebar.settingsExpanded;
      notifyChange();
    },
    onNavigate: () => navigate('#/settings'),
    subItems: SETTINGS_TABS.map((t) => ({
      label: t.label,
      active: route.kind === 'settings' && route.tab === t.key,
      onClick: () => navigate(`#/settings/${t.key}`),
    })),
  });

  const catalogGroup = navGroup({
    label: '카탈로그',
    active: route.kind === 'catalog',
    expanded: state.sidebar.catalogExpanded,
    onToggle: () => {
      state.sidebar.catalogExpanded = !state.sidebar.catalogExpanded;
      notifyChange();
    },
    onNavigate: () => navigate('#/catalog'),
    subItems: CATALOG_TABS.map((t) => ({
      label: t.label,
      active: route.kind === 'catalog' && route.tab === t.key,
      onClick: () => navigate(`#/catalog/${t.key}`),
    })),
  });

  const mainItems: HTMLElement[] = [
    navItem('대시보드', route.kind === 'home', () => navigate('#/')),
    renderProjectsGroup(route),
    renderSessionsGroup(route),
    navItem('자율업무', route.kind === 'tasks-list' || route.kind === 'tasks-board' || route.kind === 'tasks-detail', () => navigate('#/tasks')),
    navItem('사용량 통계', route.kind === 'usage', () => {
      navigate('#/usage');
      // 실시간 감시를 없앤 대신 이 메뉴를 누르는 시점마다 새로 불러온다. 이미
      // #/usage에 있으면 해시가 안 바뀌어 라우팅만으로는 재로딩이 안 트리거되니
      // 여기서 직접 부른다(겹쳐 쌓이지 않게 loading 가드).
      if (!state.dailyUsage.loading) void loadDailyUsage();
    }),
    catalogGroup,
    renderAppLinksGroup(),
    settingsGroup,
  ];

  const logoutItem = clickable(
    el('div', { className: 'sidebar-nav-item sidebar-logout' }, ['로그아웃']),
    () => {
      state.authenticated = false;
      state.auth.userEmail = null;
      state.auth.userName = null;
      state.auth.error = null;
      window.location.hash = '';
      notifyChange();
    }
  );

  return el('aside', { className: 'sidebar' }, [
    el('div', { className: 'sidebar-brand' }, [brandMark(), '맑은에이전트']),
    el('nav', { className: 'sidebar-nav' }, mainItems),
    el('div', { className: 'sidebar-footer' }, [
      ...renderUpdateItem(),
      ...(state.auth.userEmail ? [el('div', { className: 'sidebar-user-email' }, [state.auth.userEmail])] : []),
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
    const items = sortedProjectsByRecency().slice(0, SIDEBAR_SUBLIST_LIMIT).map((p) => ({
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
  const active = route.kind === 'sessions-list' || route.kind === 'sessions-detail' || route.kind === 'sessions-draft';

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

// 앱링크 — 대응하는 라우트가 없어(클릭 = 외부 브라우저 열기, 화면 전환 없음)
// 항상 active:false다. navGroup()과 달리 헤더는 라우트를 갖지 않는 순수 토글
// 헤더이고(navGroup의 헤더 구조를 그대로 복제), 로딩 중에는 프로젝트/세션
// 그룹과 동일하게 "불러오는 중…"을 비클릭 항목으로 보여준다(설계 §6-2 —
// subList()의 항목은 전부 clickable()로 감싸지므로 이 비클릭 상태만은 subList를
// 거치지 않고 직접 그린다).
function renderAppLinksGroup(): HTMLElement {
  const expanded = state.sidebar.appLinksExpanded;

  const header = clickable(
    el('div', { className: 'sidebar-nav-item sidebar-nav-group-header' }, [
      el('span', { className: 'sidebar-nav-group-label' }, ['앱링크']),
      el('span', { className: 'sidebar-nav-chevron' }, [expanded ? '▾' : '▸']),
    ]),
    () => {
      state.sidebar.appLinksExpanded = !expanded;
      notifyChange();
    }
  );

  const children: HTMLElement[] = [header];
  if (expanded) {
    if (!state.appLinks.loaded) {
      children.push(el('div', { className: 'sidebar-subnav' }, [el('div', { className: 'sidebar-subnav-empty' }, ['불러오는 중…'])]));
    } else {
      const links = enabledAppLinks();
      const items =
        links.length > 0
          ? links.map((l) => ({ label: l.name, active: false, onClick: () => void openLink(l) }))
          : [{ label: '앱링크 설정에서 추가 →', active: false, onClick: () => navigate('#/settings/applinks') }];
      children.push(subList(items, null, () => {}));
    }
  }

  return el('div', { className: 'sidebar-nav-group' }, children);
}

// 자동 업데이트 배지/버튼 — 평소엔 아무것도 렌더하지 않는다(state.update.available
// === false, updateApi.ts가 실제로 감지·다운로드를 마쳤을 때만 true). 사이드바
// 최하단(로그아웃 위)에 두는 이유: 화면 전환을 일으키지 않는 항목이라 메인
// 내비게이션(mainItems) 목록과 섞이면 "화면"으로 오인될 수 있고, VSCode가 좌측
// 하단 게이지/알림 영역에 조용히 상태를 띄우는 것과 같은 자리라 기존 사용자
// 습관과도 맞는다. 클릭 시 그 자리에서 설치+재시작(installing 동안 중복 클릭
// 방지 — updateApi.applyUpdateFromButton 내부 가드와 이중으로 막는다).
function renderUpdateItem(): HTMLElement[] {
  if (!state.update.available) return [];
  const installing = state.update.installing;
  const label = installing ? '업데이트 적용 중…' : `업데이트 적용${state.update.version ? ` (v${state.update.version})` : ''}`;
  const row = el('div', { className: `sidebar-nav-item sidebar-update-item${installing ? ' disabled' : ''}` }, [
    el('span', { className: 'sidebar-update-dot' }, []),
    label,
  ]);
  return [
    clickable(row, () => {
      if (!installing) void applyUpdateFromButton();
    }),
  ];
}

function navItem(label: string, active: boolean, onClick: () => void): HTMLElement {
  return clickable(el('div', { className: `sidebar-nav-item${active ? ' active' : ''}` }, [label]), onClick);
}

// 헤더 본문 클릭/Enter = 화면 전환, 화살표 클릭/Enter = 펼침/접힘만(전환 없음).
// 캐럿도 clickable()로 키보드 포커스·Enter/Space를 받되 stopPropagation:true를
// 줘서 부모 row(clickable, 네비게이션)까지 이중으로 발동하지 않게 한다.
function expandChevron(expanded: boolean, onToggle: () => void): HTMLElement {
  return clickable(
    el('span', { className: 'sidebar-nav-chevron sidebar-nav-chevron-btn' }, [expanded ? '▾' : '▸']),
    onToggle,
    { stopPropagation: true }
  );
}

function expandableNavItem(label: string, active: boolean, onClick: () => void, expanded: boolean, onToggle: () => void): HTMLElement {
  return clickable(
    el('div', { className: `sidebar-nav-item sidebar-nav-expandable${active ? ' active' : ''}` }, [
      el('span', { className: 'sidebar-nav-group-label' }, [label]),
      expandChevron(expanded, onToggle),
    ]),
    onClick
  );
}

function subList(
  items: readonly { readonly label: string; readonly active: boolean; readonly onClick: () => void }[],
  viewAllLabel: string | null,
  onViewAll: () => void
): HTMLElement {
  return el('div', { className: 'sidebar-subnav' }, [
    ...items.map((s) => clickable(el('div', { className: `sidebar-subnav-item${s.active ? ' active' : ''}` }, [s.label]), s.onClick)),
    ...(viewAllLabel ? [clickable(el('div', { className: 'sidebar-subnav-item sidebar-subnav-viewall' }, [viewAllLabel]), onViewAll)] : []),
  ]);
}

interface NavGroupSpec {
  readonly label: string;
  readonly active: boolean;
  readonly expanded: boolean;
  readonly onToggle: () => void;
  readonly onNavigate: () => void;
  readonly subItems: readonly { readonly label: string; readonly active: boolean; readonly onClick: () => void }[];
}

function navGroup(spec: NavGroupSpec): HTMLElement {
  // 수동 토글 상태이거나, 그 섹션이 현재 활성 라우트면 항상 펼쳐 보여준다.
  const expanded = spec.expanded || spec.active;

  // expandableNavItem()과 동일하게 본문 클릭=navigate / 화살표 클릭=onToggle로
  // 분리한다(P1-1) — 이전에는 헤더 전체가 onToggle만 호출해 "설정"·"카탈로그"
  // 라벨을 클릭해도 화면 전환이 안 됐다.
  const header = clickable(
    el('div', { className: `sidebar-nav-item sidebar-nav-group-header${spec.active ? ' active' : ''}` }, [
      el('span', { className: 'sidebar-nav-group-label' }, [spec.label]),
      expandChevron(expanded, spec.onToggle),
    ]),
    spec.onNavigate
  );

  const children: HTMLElement[] = [header];
  if (expanded) {
    children.push(subList(spec.subItems, null, () => {}));
  }

  return el('div', { className: 'sidebar-nav-group' }, children);
}
