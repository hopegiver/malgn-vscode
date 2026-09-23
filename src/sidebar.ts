// Terminus 셸 — 탭스트립(상단) + 사이드바(선택된 탭의 맥락 보조 내비게이션) +
// 상태줄(하단). docs/design/terminus-shell-ia.md가 정본이다. main.ts가 이 세
// 렌더 함수를 #app 안에 세로로 쌓는다(탭스트립 → shell-body{사이드바+본문} →
// 상태줄).
//
// 사이드바 = "선택된 탭의 맥락별 보조 내비게이션/필터"(IA §2-1, 2026-09-23
// 개정). route.kind(그리고 settings/catalog에서는 route.tab)가 바뀔 때마다
// renderSidebar()가 완전히 다른 콘텐츠 종류로 분기한다 — 예전의 "사이드바=
// 전 탭 공통 워크스페이스 목록"이라는 정체성은 폐기됐다(IA §0 "폐기된 이전
// 결정"). 홈·프로젝트 두 탭만 여전히 워크스페이스 목록을 보여주지만, 이는
// "사이드바의 기본값"이 아니라 그 두 탭이 우연히 같은 콘텐츠 종류를 공유하는
// 것뿐이다(IA §4-1/§4-2).
import { getVersion } from '@tauri-apps/api/app';
import { el, clickable } from './dom';
import { state, notifyChange, resetStateForLogout } from './state';
import { navigate } from './route';
import type { Route } from './route';
import { asBoolean, asString, asNumber, sortedSessions, sessionTitle, projectNameFromCwd, openNewSessionModal, loadSessions } from './views/sessions';
import type { ClaudeSessionRecord } from './sessionsApi';
import { sortedProjectsByRecency } from './views/projects';
import { loadDailyUsage } from './views/usage';
import { highlightDevTool } from './views/devTools';
import type { DevToolStatus } from './devToolsApi';
import type { AutonomousTask } from './state';
import { enabledAppLinks, openLink } from './views/appLinks';
import type { AppLink } from './appLinksApi';
import { TAB_META } from './views/settings';
import { applyUpdateFromButton, checkForUpdateFromButton } from './updateApi';
import type { WorkspaceProject } from './workspaceApi';

// ---------------- 앱 버전 캐시(구 sidebar.ts와 동일한 모듈 스코프 캐싱 패턴) ----------------
// views/home.ts의 페이지 부제(v0.2.x)와 이 파일의 상태줄이 같은 값을 공유한다
// — getVersion() IPC 호출은 앱 수명 동안 정확히 1회만 낸다.
let appVersion: string | null = null;
let appVersionRequested = false;

export function ensureAppVersionLoaded(): void {
  if (appVersionRequested) return;
  appVersionRequested = true;
  getVersion()
    .then((v) => {
      appVersion = v;
      notifyChange();
    })
    .catch((e) => {
      console.debug('getVersion failed (no Tauri bridge?), hiding version display', e);
    });
}

export function getAppVersion(): string | null {
  return appVersion;
}

// ---------------- 상태줄 시계 ----------------
// 전역 notifyChange()를 매초 부르면 앱 전체가 매초 재렌더된다(포커스/스크롤
// 흔들림 위험) — 대신 마지막으로 그려진 <span> 노드 참조만 갱신하고 그
// 텍스트만 직접 바꾼다. renderStatusline()이 호출될 때마다 최신 노드로
// 교체되므로, 이전 렌더에서 분리된(detached) 노드는 자연히 더 이상
// 갱신되지 않고 GC된다 — 인터벌 자체는 앱 수명 동안 1개만 존재한다.
let clockEl: HTMLElement | null = null;
let clockTimerStarted = false;

function formatClock(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function ensureClockTimer(): void {
  if (clockTimerStarted) return;
  clockTimerStarted = true;
  setInterval(() => {
    if (clockEl) clockEl.textContent = formatClock(new Date());
  }, 1000);
}

// ---------------- 탭스트립 ----------------
interface TabSpec {
  readonly label: string;
  readonly active: boolean;
  readonly onClick: () => void;
}

// m6(접근성) — clickable()의 기본 role="button" 대신 탭 시맨틱스(role="tab" +
// aria-selected)를 직접 얹는다. 활성 탭은 지금까지 시각 표시(.active 클래스)뿐
// 스크린리더에는 전달되지 않았다.
function tabEl(spec: TabSpec): HTMLElement {
  const node = clickable(el('div', { className: `tab${spec.active ? ' active' : ''}` }, [el('span', { className: 'dot' }, []), spec.label]), spec.onClick);
  node.setAttribute('role', 'tab');
  node.setAttribute('aria-selected', String(spec.active));
  return node;
}

// 계정 칩 드롭다운 — 열려 있는 동안 바깥 클릭/Escape로 닫히도록 window에
// 리스너를 붙인다(appLinks.ts/projects.ts의 ESC 핸들러 등록/해제 관례와 동일
// 원칙). M1 회귀 수정(2026-09-24): 원래 'mousedown'에 걸었던 바깥 클릭
// 리스너를 'click'으로 옮겼다 — mousedown 시점에 이미 재렌더(notifyChange)가
// 일어나면, 사용자가 실제로 누르고 있던 DOM 노드(예: 다른 탭)가 mouseup 전에
// 교체돼 그 요소의 click 이벤트 자체가 발화하지 않았다(=탭 1회 클릭이
// 흡수됨, M1-b). 'click' 리스너는 버블 단계에서 항상 타깃 자신의 클릭
// 핸들러(예: 탭의 navigate)보다 나중에 window에 도달하므로, 먼저 원래
// 클릭의 동작이 실행된 뒤에 메뉴가 닫힌다.
//
// setTimeout(…, 0)으로 등록을 한 틱 미루는 이유(실측으로 발견한 2차 회귀) —
// 계정 칩 자체의 클릭 핸들러(openAccountMenu 호출)도 'click' 이벤트 버블
// 경로 위에 있다. window.addEventListener를 동시(synchronous)에 부르면, DOM
// 이벤트 경로는 dispatch 시작 시점에 고정되지만 각 노드의 리스너 목록은
// 해당 노드 차례가 될 때 다시 읽힌다 — 그래서 이 클릭이 아직 window까지
// 버블링하는 도중에 "방금 추가한" 이 리스너가 같은 이벤트에 대해 즉시
// 실행돼버린다. 게다가 openAccountMenu()의 notifyChange()가 동기 재렌더로
// 칩 DOM 노드 자체를 교체하므로, 그 시점의 e.target(구 노드)은 새 칩과
// contains() 관계가 아니어서 "바깥 클릭"으로 오판해 메뉴를 열자마자 다시
// 닫아버렸다(실측: 클릭해도 드롭다운이 전혀 안 뜸). setTimeout으로 다음
// 매크로태스크까지 등록을 미루면 그 시점엔 이번 클릭의 dispatch가 이미
// 끝나 있어 이 문제가 사라진다.
let accountMenuOutsideClickHandler: ((e: MouseEvent) => void) | null = null;
let accountMenuEscHandler: ((e: KeyboardEvent) => void) | null = null;

function closeAccountMenu(): void {
  state.sidebar.accountMenuOpen = false;
  if (accountMenuOutsideClickHandler) {
    window.removeEventListener('click', accountMenuOutsideClickHandler);
    accountMenuOutsideClickHandler = null;
  }
  if (accountMenuEscHandler) {
    window.removeEventListener('keydown', accountMenuEscHandler);
    accountMenuEscHandler = null;
  }
  notifyChange();
}

function openAccountMenu(): void {
  state.sidebar.accountMenuOpen = true;
  if (!accountMenuOutsideClickHandler) {
    const handler = (e: MouseEvent): void => {
      const target = e.target as Node | null;
      if (target && !document.querySelector('.tabstrip-account')?.contains(target)) closeAccountMenu();
    };
    accountMenuOutsideClickHandler = handler;
    setTimeout(() => {
      // 그 사이 메뉴가 이미 닫혔으면(예: Escape) 등록하지 않는다.
      if (accountMenuOutsideClickHandler === handler) window.addEventListener('click', handler);
    }, 0);
  }
  if (!accountMenuEscHandler) {
    // m6 — 계정 메뉴 Escape 지원(appLinks.ts/projects.ts와 동일한 등록/해제 관례).
    accountMenuEscHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeAccountMenu();
    };
    window.addEventListener('keydown', accountMenuEscHandler);
  }
  notifyChange();
}

function renderAccountChip(): HTMLElement {
  const email = state.auth.userEmail;
  const chip = clickable(
    el('div', { className: 'tabstrip-account' }, [
      el('span', { className: 'dot' }, []),
      email ?? '계정',
    ]),
    () => {
      if (state.sidebar.accountMenuOpen) closeAccountMenu();
      else openAccountMenu();
    }
  );

  if (!state.sidebar.accountMenuOpen) return chip;

  // M1 수정 — logoutItem은 chip(clickable) 안쪽 자식이다. stopPropagation 없이는
  // 이 클릭이 chip까지 버블링해, chip의 토글 핸들러가 "방금 closeAccountMenu()가
  // false로 바꿔둔 accountMenuOpen"을 보고 openAccountMenu()를 다시 호출한다 —
  // 그 결과 로그인 화면에 바깥 클릭 리스너가 재등록된 채로 남는다(재현 절 참고).
  const logoutItem = clickable(
    el('div', { className: 'tabstrip-account-dropdown-item danger sidebar-logout sidebar-logout-btn' }, ['로그아웃']),
    () => {
      closeAccountMenu();
      resetStateForLogout();
      window.location.hash = '';
      notifyChange();
    },
    { stopPropagation: true }
  );

  const dropdown = el('div', { className: 'tabstrip-account-dropdown' }, [
    ...(email ? [el('div', { className: 'tabstrip-account-dropdown-email' }, [email])] : []),
    logoutItem,
  ]);
  chip.appendChild(dropdown);
  return chip;
}

export function renderTabstrip(route: Route): HTMLElement {
  const tabs: TabSpec[] = [
    { label: '홈', active: route.kind === 'home', onClick: () => navigate('#/') },
    { label: '프로젝트', active: route.kind === 'projects-list' || route.kind === 'projects-detail', onClick: () => navigate('#/projects') },
    {
      label: '세션',
      active: route.kind === 'sessions-list' || route.kind === 'sessions-detail' || route.kind === 'sessions-draft',
      onClick: () => navigate('#/sessions'),
    },
    {
      label: '사용량',
      active: route.kind === 'usage',
      onClick: () => {
        navigate('#/usage');
        // 실시간 감시가 없어 이 탭을 누를 때마다 새로 불러온다(이미 #/usage에
        // 있으면 해시가 안 바뀌어 라우팅만으로는 재로딩이 안 트리거된다) —
        // 구 sidebar.ts의 동일 동작을 그대로 승계.
        if (!state.dailyUsage.loading) void loadDailyUsage();
      },
    },
    { label: '개발 도구', active: route.kind === 'settings' && route.tab === 'devtools', onClick: () => navigate('#/settings/devtools') },
    {
      label: '자율 작업',
      active: route.kind === 'tasks-list' || route.kind === 'tasks-board' || route.kind === 'tasks-detail',
      onClick: () => navigate('#/tasks'),
    },
    { label: '카탈로그', active: route.kind === 'catalog', onClick: () => navigate('#/catalog') },
    { label: '앱 링크', active: route.kind === 'settings' && route.tab === 'applinks', onClick: () => navigate('#/settings/applinks') },
    {
      label: '설정',
      active: route.kind === 'settings' && route.tab !== 'devtools' && route.tab !== 'applinks',
      onClick: () => navigate('#/settings'),
    },
  ];

  // 탭 목록만 별도 스크롤 컨테이너(.tabstrip-tabs)로 감싼다 — 계정 드롭다운
  // 잘림 회귀 수정(styles.css .tabstrip 주석 참고). 계정 칩은 그 컨테이너
  // 밖의 형제라 overflow-y:hidden의 영향을 받지 않는다.
  const tabsContainer = el('div', { className: 'tabstrip-tabs' }, tabs.map(tabEl));
  tabsContainer.setAttribute('role', 'tablist'); // m6 — tabEl의 role="tab"과 짝을 이루는 컨테이너 역할.
  return el('div', { className: 'tabstrip' }, [tabsContainer, renderAccountChip()]);
}

// =====================================================================
// 사이드바 공용 원자 컴포넌트(IA §3) — 탭별 렌더 함수가 재사용한다.
// =====================================================================

// "3일 전"/"1주 전" 류 — 신규 API 호출 없는 순수 프론트 계산. 워크스페이스
// (기존)·세션·사용량 사이드바가 공통으로 쓴다(IA §4 "재사용, 신규 계산 아님").
function formatDaysAgo(updatedAtMs: number): string {
  const diffMs = Date.now() - updatedAtMs;
  const diffMin = Math.round(diffMs / 60000);
  if (diffMin < 1) return '방금 전';
  if (diffMin < 60) return `${diffMin}분 전`;
  const diffHour = Math.round(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 전`;
  const diffDay = Math.round(diffHour / 24);
  if (diffDay < 7) return `${diffDay}일 전`;
  return `${Math.round(diffDay / 7)}주 전`;
}

// 좁은 폭 목록 행(IA §7-1) — 목업의 `.conn`/`.conn.current` 어휘를 그대로
// 재사용한다(styles.css `.ws-row`). 구조가 "dot 1개 + 제목 1줄 + 보조메타
// 1줄"로 정확히 일치해 새 클래스를 만들지 않고 그대로 파생했다 — 결정 근거는
// terminus-design-system.md 신규 절 참고. dotClass 'bad'/'accent'만 이번에
// 새로 추가했다(기존은 good/warn/idle 3종).
type DotClass = 'good' | 'warn' | 'bad' | 'accent' | 'idle';

function narrowRow(opts: {
  readonly dotClass: DotClass;
  readonly name: string;
  readonly meta: string;
  readonly current?: boolean;
  readonly dim?: boolean;
  readonly onClick: () => void;
}): HTMLElement {
  const row = clickable(
    el('div', { className: `ws-row${opts.current ? ' current' : ''}${opts.dim ? ' sidebar-row-dim' : ''}` }, [
      el('span', { className: `dot ${opts.dotClass}` }, []),
      el('div', { className: 'ws-row-text' }, [
        el('div', { className: 'ws-row-name' }, [opts.name]),
        el('div', { className: 'ws-row-meta' }, [opts.meta]),
      ]),
    ]),
    opts.onClick
  );
  return row;
}

// 사이드바 그룹 헤더(IA §7-2, 예: 개발 도구의 "필수 도구"/"선택 도구") — 최상단
// 섹션 헤더(`.sidebar-head`)와 같은 타이포지만, 목록 중간에 다시 나올 때는
// 위 여백이 더 필요해 `.group` 수정자만 새로 추가했다(값은 CSS 참고).
function groupHead(label: string): HTMLElement {
  return el('div', { className: 'sidebar-head group' }, [label]);
}

// 정적 nav 행(IA §7-3, 설정 5종·카탈로그 2종·자율작업 뷰 전환 2종) — 라벨
// 텍스트 하나 + 활성 강조만 있는 행. `.filter-btn.active`(배경 전체 채움)는
// 세로 촘촘한 목록에서 과하다고 판단해(IA §3 3항목) 대신 `.ws-row.current`와
// 같은 "좌측 강조선 + 배경 한 단 밝게" 어휘를 재사용했다 — 새 색 도입 없음.
function navRow(label: string, active: boolean, onClick: () => void): HTMLElement {
  return clickable(el('div', { className: `sidebar-nav-row${active ? ' current' : ''}` }, [label]), onClick);
}

function sidebarEmpty(text: string): HTMLElement {
  return el('div', { className: 'sidebar-empty' }, [text]);
}

// =====================================================================
// §4-1/§4-2 — 홈·프로젝트: 워크스페이스 목록(기존 컴포넌트 그대로 재사용)
// =====================================================================
function dotClassFor(p: WorkspaceProject): 'good' | 'idle' {
  return p.archiveStatus === 'active' ? 'good' : 'idle';
}

function wsRow(p: WorkspaceProject, route: Route): HTMLElement {
  const current = route.kind === 'projects-detail' && route.path === p.path;
  return narrowRow({
    dotClass: dotClassFor(p),
    name: p.name,
    meta: formatDaysAgo(p.updatedAt),
    current,
    onClick: () => navigate(`#/project/${encodeURIComponent(p.path)}`),
  });
}

// 워크스페이스 경로 편집 진입 — 홈·프로젝트 사이드바 공통 하단 CTA(IA §4-1/§4-2).
function openWorkspacesManager(): void {
  navigate('#/projects');
  state.malgnAgentConfig.editingWorkspaces = true;
  notifyChange();
}

function renderWorkspaceSidebar(route: Route): HTMLElement {
  const dash = state.dashboard;
  const projects = sortedProjectsByRecency();

  const head = el('div', { className: 'sidebar-head' }, [`workspace ~ (${dash.loaded ? projects.length : '…'})`]);

  let body: HTMLElement;
  if (dash.error) {
    body = sidebarEmpty(`⚠ ${dash.error}`);
  } else if (!dash.loaded) {
    body = sidebarEmpty('불러오는 중…');
  } else if (projects.length === 0) {
    body = sidebarEmpty('워크스페이스가 없습니다');
  } else {
    body = el('div', {}, projects.map((p) => wsRow(p, route)));
  }

  const foot = clickable(el('div', { className: 'sidebar-foot' }, ['워크스페이스 경로 관리']), openWorkspacesManager);

  return el('aside', { className: 'sidebar' }, [head, body, foot]);
}

// =====================================================================
// §4-3 — 세션: 세션 목록(좁은 폭 목록 행)
// =====================================================================
function sessionNarrowRow(s: ClaudeSessionRecord, route: Route): HTMLElement {
  const sessionId = asString(s.sessionId);
  const current = route.kind === 'sessions-detail' && route.sessionId === sessionId;
  const running = asBoolean(s.running);
  const updatedAt = asNumber(s.updatedAt) ?? asNumber(s.startedAt);
  return narrowRow({
    dotClass: running ? 'good' : 'idle',
    name: sessionTitle(s),
    meta: `${projectNameFromCwd(asString(s.cwd))} · ${updatedAt !== null ? formatDaysAgo(updatedAt) : '-'}`,
    current,
    onClick: () => navigate(`#/sessions/${encodeURIComponent(sessionId)}`),
  });
}

// 세션목록 화면 밖(세션 상세/draft)에서도 CTA를 눌러 모달을 열 수 있어야
// 한다 — projects.ts의 openWorkspacesManager와 같은 "먼저 목록 라우트로
// 이동 후 모듈 상태를 연다" 패턴을 그대로 따른다.
function openNewSessionFromSidebar(): void {
  navigate('#/sessions');
  openNewSessionModal();
}

function renderSessionsSidebar(route: Route): HTMLElement {
  const sessions = state.sessions;
  const head = el('div', { className: 'sidebar-head' }, [`sessions (${sessions.loaded ? sessions.items.length : '…'})`]);

  let body: HTMLElement;
  if (sessions.error) {
    body = sidebarEmpty(`⚠ ${sessions.error}`);
  } else if (!sessions.loaded) {
    body = sidebarEmpty('불러오는 중…');
  } else if (sessions.items.length === 0) {
    body = sidebarEmpty('세션이 없습니다');
  } else {
    body = el('div', {}, sortedSessions().map((s) => sessionNarrowRow(s, route)));
  }

  const foot = clickable(el('div', { className: 'sidebar-foot' }, ['+ 새 세션']), openNewSessionFromSidebar);
  // 조회 실패 시에도 재시도 동선을 준다(홈/프로젝트 사이드바에는 없던 것이지만
  // 세션은 이 사이드바가 유일한 "다시 시도" 진입점일 수 있다 — 본문도 동일
  // 조회를 다시 트리거하는 버튼을 갖고 있어 중복이 아니라 보조 동선이다).
  if (sessions.error) {
    return el('aside', { className: 'sidebar' }, [
      head,
      body,
      clickable(el('div', { className: 'sidebar-foot' }, ['다시 시도']), () => void loadSessions()),
      foot,
    ]);
  }
  return el('aside', { className: 'sidebar' }, [head, body, foot]);
}

// =====================================================================
// §4-4 — 사용량: "일별 사용량" 단일 항목(PM 결정, 2026-09-24) — 예전의
// 기간 토글(7일/30일)과 "최근 활동일" 퀵점프 목록은 폐기됐다. 이 탭
// 본문(usage.ts)이 곧 "일별 사용량" 전체이므로 사이드바는 그 사실을 알려주는
// 정적 nav 행 하나만 둔다(다른 목적지가 없어 클릭 동작은 없다).
// =====================================================================
function renderUsageSidebar(): HTMLElement {
  const head = el('div', { className: 'sidebar-head' }, ['usage']);
  const body = el('div', { className: 'sidebar-nav-row current' }, ['일별 사용량']);
  return el('aside', { className: 'sidebar' }, [head, body]);
}

// =====================================================================
// §4-5 — 개발 도구: 그룹 헤더(필수/선택) + 압축 상태 표시 + 일시 강조
// =====================================================================
function devToolNarrowRow(t: DevToolStatus): HTMLElement {
  // 압축 상태 표시(IA §7-5) — 좁은 폭에서 텍스트 플래그(`.blist-devtool-flag`)
  // 대신 행의 dot 색으로 설치 여부를 표현한다: 설치됨=good, 필수인데 미설치=bad,
  // 선택인데 미설치=idle. 별도 아이콘 요소를 추가하지 않고 이미 있는 dot
  // 채널을 재사용한 것이 이번 결정이다.
  const dotClass: DotClass = t.installed ? 'good' : t.required ? 'bad' : 'idle';
  return narrowRow({
    dotClass,
    name: t.name,
    meta: t.installed ? (t.version ?? '설치됨') : '미설치',
    onClick: () => highlightDevTool(t.id),
  });
}

function renderDevToolsSidebar(): HTMLElement {
  const dt = state.devTools;
  const head = el('div', { className: 'sidebar-head' }, ['dev tools']);
  const body: HTMLElement[] = [];

  if (dt.error) {
    body.push(sidebarEmpty(`⚠ ${dt.error}`));
  } else if (!dt.loaded) {
    body.push(sidebarEmpty('불러오는 중…'));
  } else {
    const required = dt.items.filter((t) => t.required);
    const optional = dt.items.filter((t) => !t.required);
    if (required.length > 0) {
      body.push(groupHead('필수 도구'));
      body.push(el('div', {}, required.map(devToolNarrowRow)));
    }
    if (optional.length > 0) {
      body.push(groupHead('선택 도구'));
      body.push(el('div', {}, optional.map(devToolNarrowRow)));
    }
  }

  return el('aside', { className: 'sidebar' }, [head, ...body]);
}

// =====================================================================
// §4-6 — 자율 작업: 뷰 전환(정적 nav) + 작업 큐(전체, 실행중→활성→비활성 순,
// 비활성은 흐리게 — PM 확정, IA §8-③ 대체)
// =====================================================================
function taskGroupOf(t: AutonomousTask): 0 | 1 | 2 {
  if (t.running) return 0;
  if (t.enabled) return 1;
  return 2;
}

function taskNarrowRow(t: AutonomousTask, route: Route): HTMLElement {
  const current = route.kind === 'tasks-detail' && route.taskId === t.id;
  // m1 수정(리뷰 2026-09-24) — running=green(good)으로 통일한다. 이전엔
  // home.ts taskQueueRow(.blist-badge.run)의 accent(cyan)를 그대로 따랐는데,
  // home.ts 쪽이 세션의 "running=green" 관례와 달라 화면마다 상태색이 달랐다
  // (리뷰 m1). home.ts도 이번에 green으로 맞췄으므로 여기도 같이 맞춘다.
  const dotClass: DotClass = t.running ? 'good' : 'idle';
  return narrowRow({
    dotClass,
    name: t.name,
    meta: `${t.projectName} · ${t.running ? '실행 중' : t.enabled ? '대기' : '중지됨'}`,
    current,
    dim: !t.enabled,
    onClick: () => navigate(`#/tasks/item/${encodeURIComponent(t.id)}`),
  });
}

function renderTasksSidebar(route: Route): HTMLElement {
  const tasks = state.autonomousTasks;
  const head = el('div', { className: 'sidebar-head' }, ['autonomy']);
  const viewToggle = el('div', { className: 'sidebar-nav-group' }, [
    navRow('목록', route.kind === 'tasks-list', () => navigate('#/tasks')),
    navRow('진행상황판', route.kind === 'tasks-board', () => navigate('#/tasks/board')),
  ]);

  const body: HTMLElement[] = [viewToggle];
  if (tasks.error) {
    body.push(sidebarEmpty(`⚠ ${tasks.error}`));
  } else if (!tasks.loaded) {
    body.push(sidebarEmpty('불러오는 중…'));
  } else if (tasks.items.length === 0) {
    // m5 수정 — 이 사이드바는 "큐=전체 작업"(비활성도 흐리게 포함)이라 0건은
    // "대기 중인 게 없다"가 아니라 "등록된 게 없다"는 뜻이다.
    body.push(sidebarEmpty('등록된 자율 작업이 없습니다'));
  } else {
    const sorted = [...tasks.items].sort((a, b) => taskGroupOf(a) - taskGroupOf(b));
    body.push(groupHead('작업 큐'));
    body.push(el('div', {}, sorted.map((t) => taskNarrowRow(t, route))));
  }

  return el('aside', { className: 'sidebar' }, [head, ...body]);
}

// =====================================================================
// §4-7 — 카탈로그: 정적 nav 행 2종(카운트 배지 없음 — PM 결정, IA §8-①)
// =====================================================================
type CatalogRoute = Extract<Route, { kind: 'catalog' }>;

function renderCatalogSidebar(route: CatalogRoute): HTMLElement {
  return el('aside', { className: 'sidebar' }, [
    el('div', { className: 'sidebar-head' }, ['catalog']),
    navRow('플러그인 카탈로그', route.tab === 'plugins', () => navigate('#/catalog/plugins')),
    navRow('전역 카탈로그', route.tab === 'global', () => navigate('#/catalog/global')),
  ]);
}

// =====================================================================
// §4-8 — 앱 링크: 플랫 퀵오픈 목록(클릭 = 외부 브라우저로 즉시 열기)
// =====================================================================
function appLinkNarrowRow(link: AppLink): HTMLElement {
  return narrowRow({
    dotClass: 'idle',
    name: link.name,
    meta: link.url,
    onClick: () => void openLink(link),
  });
}

function renderAppLinksSidebar(): HTMLElement {
  const links = state.appLinks;
  const head = el('div', { className: 'sidebar-head' }, ['app links']);

  let body: HTMLElement;
  if (links.error) {
    body = sidebarEmpty(`⚠ ${links.error}`);
  } else if (!links.loaded) {
    body = sidebarEmpty('불러오는 중…');
  } else {
    const enabled = enabledAppLinks();
    body = enabled.length === 0 ? sidebarEmpty('등록된 앱링크가 없습니다') : el('div', {}, enabled.map(appLinkNarrowRow));
  }

  return el('aside', { className: 'sidebar' }, [head, body]);
}

// =====================================================================
// §4-9 — 설정: 정적 nav 행 5종(devtools/applinks는 독립 탭으로 승격돼 있어
// 여기서 걸러낸다 — 넣으면 같은 화면 진입 경로가 3개로 늘어난다, IA §4-9)
// =====================================================================
type SettingsRoute = Extract<Route, { kind: 'settings' }>;

function renderSettingsSidebar(route: SettingsRoute): HTMLElement {
  const items = TAB_META.filter((t) => t.key !== 'devtools' && t.key !== 'applinks');
  return el('aside', { className: 'sidebar' }, [
    el('div', { className: 'sidebar-head' }, ['settings']),
    ...items.map((t) => navRow(t.label, route.tab === t.key, () => navigate(`#/settings/${t.key}`))),
  ]);
}

// ---------------- 사이드바 진입점 ----------------
// route.kind(그리고 settings/catalog에서는 route.tab)로 콘텐츠 종류를
// 분기한다(IA §2-1). 9개 탭 전부 실제 콘텐츠를 가지므로 "빈 사이드바"가
// 발생하는 탭이 없다(IA §2-3) — 이 switch는 Route의 12개 kind를 모두
// 다루는 exhaustive 분기다.
export function renderSidebar(route: Route): HTMLElement {
  switch (route.kind) {
    case 'home':
    case 'projects-list':
    case 'projects-detail':
      return renderWorkspaceSidebar(route);
    case 'sessions-list':
    case 'sessions-detail':
    case 'sessions-draft':
      return renderSessionsSidebar(route);
    case 'usage':
      return renderUsageSidebar();
    case 'settings':
      if (route.tab === 'devtools') return renderDevToolsSidebar();
      if (route.tab === 'applinks') return renderAppLinksSidebar();
      return renderSettingsSidebar(route);
    case 'tasks-list':
    case 'tasks-board':
    case 'tasks-detail':
      return renderTasksSidebar(route);
    case 'catalog':
      return renderCatalogSidebar(route);
  }
}

// ---------------- 상태줄 ----------------
function seg(children: readonly (Node | string)[], className = 'seg'): HTMLElement {
  return el('span', { className }, children);
}

function sep(): HTMLElement {
  return el('span', { className: 'sep' }, ['│']);
}

export function renderStatusline(route: Route): HTMLElement {
  ensureAppVersionLoaded();
  ensureClockTimer();

  // M2 수정(2026-09-24) — 왼쪽 세그먼트는 우선순위가 있다: 뒤에 나올수록(배열
  // 끝에 가까울수록) 창이 좁아질 때 먼저 잘려나간다(.statusline-left가 아래서
  // overflow:hidden + min-width:0로 실제 내용보다 좁게 줄어들 수 있고, flex
  // 레이아웃은 넘치는 뒤쪽 자식부터 컨테이너 밖으로 밀어내기 때문). 그래서 낮은
  // 우선순위(버전 정보·개발 도구 확인 링크)를 배열 맨 끝 두 자리에 둔다 —
  // 업데이트 적용/새 버전 확인/시계는 아예 별도의 `.statusline-right`(flex:none)
  // 로 분리해 이 축소 대상에서 제외한다(900px 최소 창에서도 항상 완전히 보여야
  // 한다는 PM 결정, 리뷰 M2).
  const segments: HTMLElement[] = [];

  if (state.dashboard.loaded) {
    segments.push(seg([el('span', { className: 'dot' }, []), `${state.dashboard.projects.length} workspaces`]));
  }

  if (state.sessions.loaded) {
    const running = state.sessions.items.filter((s) => asBoolean(s.running)).length;
    const idle = state.sessions.items.length - running;
    segments.push(seg(['session: ', el('span', { className: 'accent' }, [`${running} running`]), `, ${idle} idle`]));
  }

  if (route.kind === 'projects-detail') {
    const proj = state.dashboard.projects.find((p) => p.path === route.path);
    if (proj) segments.push(seg([proj.name]));
  }

  // 아래 두 세그먼트(claude/node 버전, "개발 도구 확인 →")는 의도적으로 배열
  // 맨 끝에 둔다 — 위 주석 참고, 가장 먼저 잘려나가야 하는 낮은 우선순위다.
  const claude = state.devTools.items.find((t) => t.id === 'claude');
  const node = state.devTools.items.find((t) => t.id === 'node');
  if (claude?.version || node?.version) {
    const parts = [claude?.version ? `claude ${claude.version}` : null, node?.version ? `node ${node.version}` : null].filter(Boolean);
    segments.push(seg([parts.join(' · ')]));
  }

  // pnpm/gh 등 개별 CLI 업데이트 가용 여부는 로그인 시점에 일괄 조회할 방법이
  // 없다(devToolsApi.ts DevToolStatus에 "최신 버전" 필드 없음, IA §5) — 가짜
  // 배지 대신 탭 이동만 하는 정적 링크로 대체한다.
  segments.push(clickable(seg(['개발 도구 확인 →']), () => navigate('#/settings/devtools')));

  const rightSegments: HTMLElement[] = [];
  const version = getAppVersion();
  if (version) rightSegments.push(seg([`v${version}`]));

  // 자동 감지된 업데이트 배지 — 클릭 시 그 자리에서 설치+재시작(§8-④, 업데이트
  // 세그먼트만 클릭 가능). 클래스명 sidebar-update-item은 구 sidebar.ts
  // 시절 이름을 그대로 유지한다 — ui-harness(updateCheck.mjs)가 이 클래스로
  // count()/textContent를 조회하므로 이동만 하고 이름은 바꾸지 않는다.
  if (state.update.available) {
    const installing = state.update.installing;
    rightSegments.push(
      clickable(
        seg([installing ? '업데이트 적용 중…' : `업데이트 적용${state.update.version ? ` (v${state.update.version})` : ''}`], 'seg statusline-update sidebar-update-item'),
        () => {
          if (!installing) void applyUpdateFromButton();
        }
      )
    );
  }

  // 수동 "새 버전 확인" 버튼 — 구 sidebar.ts renderVersionRow()와 동일한
  // 동작(연타 방지·"확인 중…" 표시)을 그대로 옮긴다. 클래스명도 유지한다
  // (ui-harness updateCheck.mjs가 getByRole('button', name:'새 버전 확인') +
  // .sidebar-version-check-btn 셀렉터로 8개 시나리오를 검증한다).
  if (version) {
    const checking = state.update.checking;
    const checkBtn = el(
      'button',
      {
        className: 'sidebar-version-check-btn',
        onClick: () => {
          if (!checking) void checkForUpdateFromButton();
        },
        disabled: checking || state.update.installing,
      },
      [checking ? '확인 중…' : '새 버전 확인']
    );
    checkBtn.type = 'button';
    rightSegments.push(checkBtn);
  }

  clockEl = el('span', { className: 'clock' }, [formatClock(new Date())]);
  rightSegments.push(clockEl);

  const withSeparators: HTMLElement[] = [];
  segments.forEach((s, i) => {
    if (i > 0) withSeparators.push(sep());
    withSeparators.push(s);
  });
  const rightWithSeparators: HTMLElement[] = [];
  rightSegments.forEach((s, i) => {
    if (i > 0) rightWithSeparators.push(sep());
    rightWithSeparators.push(s);
  });

  // M2 수정 — 왼쪽 그룹(.statusline-left)만 min-width:0 + overflow:hidden으로
  // 실제로 줄어들 수 있게 하고, 오른쪽 그룹(.statusline-right)은 flex:none +
  // margin-left:auto로 항상 자기 내용 전체 폭을 요구한다. 창이 좁아지면 왼쪽
  // 그룹의 폭 자체가 줄어들며(내부 세그먼트가 배열 순서대로, 즉 낮은 우선순위부터
  // 컨테이너 오른쪽 밖으로 밀려 시각적으로 잘린다) 오른쪽 그룹은 항상 자기 폭을
  // 온전히 확보한다 — 옛 `.statusline-fill` 스페이서는 제거(같은 폭 배분 역할을
  // margin-left:auto가 대신한다).
  const left = el('div', { className: 'statusline-left' }, withSeparators);
  const right = el('div', { className: 'statusline-right' }, rightWithSeparators);
  return el('div', { className: 'statusline' }, [left, right]);
}
