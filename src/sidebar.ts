import { getVersion } from '@tauri-apps/api/app';
import { el, clickable } from './dom';
import { state, notifyChange, resetStateForLogout } from './state';
import type { SettingsTab, CatalogTab } from './state';
import { navigate } from './route';
import type { Route } from './route';
import { sortedSessions, sessionTitle, asString } from './views/sessions';
import { sortedProjectsByRecency } from './views/projects';
import { loadDailyUsage } from './views/usage';
import { enabledAppLinks, openLink } from './views/appLinks';
import { brandMark } from './brand';
import { applyUpdateFromButton, checkForUpdateFromButton } from './updateApi';

// 앱 버전 표시 — Tauri가 tauri.conf.json의 version을 읽어주는 getVersion()을
// 그대로 쓴다(런타임 내내 바뀌지 않으므로 한 번만 읽어 모듈 스코프에 캐싱).
// Tauri IPC 브리지가 없는 환경(하네스의 순수 브라우저 + __TAURI_INTERNALS__
// 스텁 등)에서는 invoke 자체가 실패할 수 있으므로, authApi.tryDevAutoLogin()과
// 동일하게 실패를 조용히 흡수하고(console.debug만 남김) 그 경우 버전 영역
// 자체를 렌더하지 않는다 — 나머지 화면 동작에는 영향을 주지 않는다.
let appVersion: string | null = null;
let appVersionRequested = false;

function ensureAppVersionLoaded(): void {
  if (appVersionRequested) return;
  appVersionRequested = true;
  getVersion()
    .then((v) => {
      appVersion = v;
      notifyChange();
    })
    .catch((e) => {
      console.debug('getVersion failed (no Tauri bridge?), hiding sidebar version', e);
    });
}

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
  ensureAppVersionLoaded();

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
      // G1(리뷰 v0.2.5) — 인증 필드만 지우던 이전 구현은 채팅 스트리밍
      // 리스너를 해제하지 않고(unlisten 0건 실측) 이전 사용자의 대화 전문을
      // 메모리에 남겼다. resetStateForLogout()이 sessionChat을 제외한 모든
      // 데이터 슬라이스를 초기값으로 되돌리고(다음 사용자에게 이전 데이터가
      // 보이지 않도록), 해시를 리셋해 발생하는 hashchange가
      // main.ts의 handleNavigation()을 거쳐 실제 리스너 해제
      // (leaveSessionChatView() 등)까지 수행한다(main.ts 참고).
      resetStateForLogout();
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
      ...(appVersion ? [renderVersionRow(appVersion)] : []),
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
      // B1(리뷰 v0.2.5) — items.length === 0만으로 "불러오는 중…"을 단정하면
      // 로드가 끝났는데 프로젝트가 진짜 0건인 사용자(신규 입사자 첫 실행이
      // 정확히 이 상태)에게 이 문구가 영원히 뜬다. 같은 파일의 앱링크 그룹
      // (renderAppLinksGroup, `!state.appLinks.loaded`)이 이미 올바른 선례라
      // 그대로 따른다.
      children.push(
        el('div', { className: 'sidebar-subnav' }, [
          el('div', { className: 'sidebar-subnav-empty' }, [state.dashboard.loaded ? '프로젝트가 없습니다' : '불러오는 중…']),
        ])
      );
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
      // B1(리뷰 v0.2.5) — 프로젝트 그룹과 동일한 원인·동일한 수정.
      children.push(
        el('div', { className: 'sidebar-subnav' }, [
          el('div', { className: 'sidebar-subnav-empty' }, [state.sessions.loaded ? '세션이 없습니다' : '불러오는 중…']),
        ])
      );
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

// 앱 버전 옆 "새 버전 확인" 버튼 — 지금까지는 자동(부팅 1회 + 1시간 주기)뿐이라
// 사용자가 원할 때 직접 확인할 방법이 없었다. updateApi.checkForUpdateFromButton()을
// 그대로 호출한다(강제 체크·결과 토스트는 그쪽 책임, 여기서는 로컬 UI 상태만
// 반영). 결과가 "새 버전 있음"이면 renderUpdateItem()이 다음 렌더에서 그 자리에
// 나타나므로 이 버튼 쪽에서 별도 문구를 더할 필요가 없다.
//
// 라벨을 "업데이트 확인"이 아니라 "새 버전 확인"으로 둔 이유(실측 회귀) — 이
// 사이드바는 모든 화면에서 함께 렌더되는데, 개발 환경 패널(devTools.ts)의 CLI
// 도구별 "업데이트 확인" 버튼과 카탈로그 플러그인 카드의 "업데이트" 버튼이
// 이미 같은 화면 트리에 존재한다. Playwright의 getByRole name 매칭은 기본이
// 부분일치라 "업데이트"를 포함하는 라벨을 쓰면 DOM에서 더 앞서 렌더되는(사이드바가
// 본문보다 먼저 appendChild됨) 이 버튼이 기존 하네스 시나리오의 `.first()`
// 선택자를 가로채 버린다(majorReview20260921.mjs 등에서 실제로 재현됨) — 겹치지
// 않는 라벨을 쓰는 것으로 해결한다.
function renderVersionRow(version: string): HTMLElement {
  const checking = state.update.checking;
  const checkBtn = el(
    'button',
    {
      className: 'btn btn-sm sidebar-version-check-btn',
      onClick: () => {
        if (!checking) void checkForUpdateFromButton();
      },
      disabled: checking || state.update.installing,
    },
    [checking ? '확인 중…' : '새 버전 확인']
  );
  checkBtn.type = 'button';
  return el('div', { className: 'sidebar-version-row' }, [el('span', { className: 'sidebar-version-label' }, [`v${version}`]), checkBtn]);
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
