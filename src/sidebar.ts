// Terminus 셸 — 탭스트립(상단) + 사이드바(워크스페이스 목록) + 상태줄(하단).
// docs/design/terminus-shell-ia.md가 정본이다. main.ts가 이 세 렌더 함수를
// #app 안에 세로로 쌓는다(탭스트립 → shell-body{사이드바+본문} → 상태줄).
//
// 구 버전(아코디언 사이드바)과 달리 이 사이드바는 워크스페이스(=프로젝트)
// 목록 전용이고, 설정/카탈로그/자율업무 하위탭 전환·프로젝트/세션 서브목록·
// 앱링크 바로가기 목록은 전부 본문(탭 콘텐츠) 쪽으로 옮겨졌다(IA §2~4) —
// 이 화면들 자체는 다음 단계(9개 화면 마이그레이션) 범위라 여기서는 탭
// 전환(navigate)만 담당한다.
import { getVersion } from '@tauri-apps/api/app';
import { el, clickable } from './dom';
import { state, notifyChange, resetStateForLogout } from './state';
import { navigate } from './route';
import type { Route } from './route';
import { asBoolean } from './views/sessions';
import { sortedProjectsByRecency } from './views/projects';
import { loadDailyUsage } from './views/usage';
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

function tabEl(spec: TabSpec): HTMLElement {
  return clickable(el('div', { className: `tab${spec.active ? ' active' : ''}` }, [el('span', { className: 'dot' }, []), spec.label]), spec.onClick);
}

// 계정 칩 드롭다운 — 열려 있는 동안 바깥 클릭으로 닫히도록 window에 리스너를
// 1개만 붙인다(appLinks.ts/projects.ts의 ESC 핸들러 등록/해제 관례와 동일 원칙).
let accountMenuOutsideClickHandler: ((e: MouseEvent) => void) | null = null;

function closeAccountMenu(): void {
  state.sidebar.accountMenuOpen = false;
  if (accountMenuOutsideClickHandler) {
    window.removeEventListener('mousedown', accountMenuOutsideClickHandler);
    accountMenuOutsideClickHandler = null;
  }
  notifyChange();
}

function openAccountMenu(): void {
  state.sidebar.accountMenuOpen = true;
  if (!accountMenuOutsideClickHandler) {
    accountMenuOutsideClickHandler = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (target && !document.querySelector('.tabstrip-account')?.contains(target)) closeAccountMenu();
    };
    window.addEventListener('mousedown', accountMenuOutsideClickHandler);
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

  const logoutItem = clickable(
    el('div', { className: 'tabstrip-account-dropdown-item danger sidebar-logout sidebar-logout-btn' }, ['로그아웃']),
    () => {
      closeAccountMenu();
      resetStateForLogout();
      window.location.hash = '';
      notifyChange();
    }
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

  return el('div', { className: 'tabstrip' }, [...tabs.map(tabEl), renderAccountChip()]);
}

// ---------------- 사이드바(워크스페이스 목록 전용) ----------------
// 상태 점: active→good(초록), unknown·archived→idle(무채색) — PM 확정(IA §8-①
// 관련, 노랑(warn)은 실제 경고 전용이라 여기서는 쓰지 않는다).
function dotClassFor(p: WorkspaceProject): 'good' | 'idle' {
  return p.archiveStatus === 'active' ? 'good' : 'idle';
}

// "3일 전"/"1주 전" 류 — 구 git 브랜치/워킹트리 상태 대신 IA §4가 지정한
// 대체 데이터(WorkspaceProject.updatedAt, epoch ms)를 상대시간으로 표시한다.
// 신규 API 호출 없는 순수 프론트 계산(views/autonomousTasks.ts의 유사 패턴
// 재사용 — export되어 있지 않아 이 파일 전용으로 다시 작성).
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

function wsRow(p: WorkspaceProject, route: Route): HTMLElement {
  const current = route.kind === 'projects-detail' && route.path === p.path;
  return clickable(
    el('div', { className: `ws-row${current ? ' current' : ''}` }, [
      el('span', { className: `dot ${dotClassFor(p)}` }, []),
      el('div', { className: 'ws-row-text' }, [
        el('div', { className: 'ws-row-name' }, [p.name]),
        el('div', { className: 'ws-row-meta' }, [formatDaysAgo(p.updatedAt)]),
      ]),
    ]),
    () => navigate(`#/project/${encodeURIComponent(p.path)}`)
  );
}

// 워크스페이스 경로 편집 진입 — 정확히 대응하는 "1클릭 연결" 기능은 없다.
// 가장 가까운 기존 기능인 views/projects.ts의 "workspace 설정" 토글
// (state.malgnAgentConfig.editingWorkspaces)을 그대로 재사용한다(IA §4).
// 그 모달은 프로젝트 목록 화면에서만 그려지므로 먼저 그 라우트로 이동한 뒤
// 플래그를 켠다 — 아직 malgnAgentConfig가 로드되지 않았어도, 로드가 끝나는
// 대로(loadProjects()이 이미 로그인 직후 트리거해둔 로딩이 이어짐) 다음
// 렌더에서 모달이 자동으로 뜬다.
function openWorkspacesManager(): void {
  navigate('#/projects');
  state.malgnAgentConfig.editingWorkspaces = true;
  notifyChange();
}

export function renderSidebar(route: Route): HTMLElement {
  const dash = state.dashboard;
  const projects = sortedProjectsByRecency();

  const head = el('div', { className: 'sidebar-head' }, [`workspace ~ (${dash.loaded ? projects.length : '…'})`]);

  let body: HTMLElement;
  if (dash.error) {
    body = el('div', { className: 'sidebar-empty' }, [`⚠ ${dash.error}`]);
  } else if (!dash.loaded) {
    body = el('div', { className: 'sidebar-empty' }, ['불러오는 중…']);
  } else if (projects.length === 0) {
    body = el('div', { className: 'sidebar-empty' }, ['워크스페이스가 없습니다']);
  } else {
    body = el('div', {}, projects.map((p) => wsRow(p, route)));
  }

  const foot = clickable(el('div', { className: 'sidebar-foot' }, ['워크스페이스 경로 관리']), openWorkspacesManager);

  return el('aside', { className: 'sidebar' }, [head, body, foot]);
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

  const segments: HTMLElement[] = [];

  if (state.dashboard.loaded) {
    segments.push(seg([el('span', { className: 'dot' }, []), `${state.dashboard.projects.length} workspaces`]));
  }

  if (state.sessions.loaded) {
    const running = state.sessions.items.filter((s) => asBoolean(s.running)).length;
    const idle = state.sessions.items.length - running;
    segments.push(seg(['session: ', el('span', { className: 'accent' }, [`${running} running`]), `, ${idle} idle`]));
  }

  const claude = state.devTools.items.find((t) => t.id === 'claude');
  const node = state.devTools.items.find((t) => t.id === 'node');
  if (claude?.version || node?.version) {
    const parts = [claude?.version ? `claude ${claude.version}` : null, node?.version ? `node ${node.version}` : null].filter(Boolean);
    segments.push(seg([parts.join(' · ')]));
  }

  if (route.kind === 'projects-detail') {
    const proj = state.dashboard.projects.find((p) => p.path === route.path);
    if (proj) segments.push(seg([proj.name]));
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

  return el('div', { className: 'statusline' }, [...withSeparators, el('div', { className: 'statusline-fill' }, []), ...rightWithSeparators]);
}
