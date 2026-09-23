// 홈 대시보드 — 로그인 직후 첫 화면. 다른 화면들의 핵심 정보를 카드/위젯으로 모아
// 보여준다. 위젯 클릭 시 해당 상세 화면으로 이동한다. 프로젝트·세션·개발환경·
// 카탈로그·사용량 통계·자율업무 위젯은 전부 실제 로컬 데이터를 쓴다(main.ts가
// 로그인 직후 한 번씩 미리 불러온다).
import { el, clickable, showToast, confirmDialog } from '../dom';
import { state, notifyChange } from '../state';
import { navigate } from '../route';
import { computeTodayTokens, formatTokenCount, loadDailyUsage } from './usage';
import { asBoolean, loadSessions, handleClaudeAuthLoginStart } from './sessions';
import { loadCatalog } from './catalog';
import { loadProjects } from './projects';
import { loadDevTools } from './devTools';
import { loadAutonomousTasks } from './autonomousTasks';
import { loadMcp } from './settings';
import { ensureAppVersionLoaded, getAppVersion } from '../sidebar';
import { checkClaudeAuthStatus, openClaudeLoginTerminal, logoutClaudeAuth } from '../sessionsApi';

const MALGNAI_HUB_MCP_NAME = 'plugin:malgn-agent:malgnai-hub';

// 앱 버전 표시 — sidebar.ts가 이미 갖고 있는 모듈 스코프 캐시(getVersion()을
// 앱 수명 동안 딱 한 번만 호출)를 그대로 재사용한다. 대시보드는 사이드바와 항상
// 함께 렌더되므로(main.ts renderApp) 여기서 다시 getVersion()을 부르면 앱 전체
// 기준 중복 호출이 된다 — ensureAppVersionLoaded()는 이미 요청했으면 즉시
// return하는 가드가 있어 어느 쪽이 먼저 불러도 실제 IPC 호출은 1회로 유지된다.
export function renderHomeView(): HTMLElement {
  ensureAppVersionLoaded();
  const appVersion = getAppVersion();

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['대시보드']),
      el('div', { className: 'page-subtitle-row' }, [
        el('div', { className: 'page-subtitle' }, ['전체 요약']),
        ...(appVersion ? [el('div', { className: 'page-subtitle' }, [`v${appVersion}`])] : []),
      ]),
    ]),
  ]);

  const grid = el('div', { className: 'home-widget-grid' }, [
    claudeAuthWidget(),
    usageWidget(),
    catalogWidget(),
    projectsWidget(),
    sessionsWidget(),
    devToolsWidget(),
    taskBoardWidget(),
    mcpHubWidget(),
  ]);

  return el('div', {}, [header, grid]);
}

function widgetShell(title: string, onClick: () => void, children: readonly (Node | string)[]): HTMLElement {
  const widget = el('div', { className: 'home-widget' }, [el('div', { className: 'home-widget-title' }, [title]), ...children]);
  return clickable(widget, onClick);
}

// error phase 전용 — 위젯 6개가 모두 같은 문구("상태를 불러오지 못했습니다.")를
// 쓰면서도 복구 경로가 없었다(V-10). 카드를 클릭하면 서브페이지로 이동하는 대신
// 그 자리에서 다시 불러오도록 onClick을 loadX()로 바꾸고, 링크 문구도 이동이
// 아니라 재시도임을 알리게 바꾼다.
function widgetErrorShell(title: string, message: string | null, onRetry: () => void): HTMLElement {
  return widgetShell(title, onRetry, [
    el('div', { className: 'home-widget-desc' }, [message ?? '상태를 불러오지 못했습니다.']),
    el('div', { className: 'home-widget-link' }, ['다시 확인하기 →']),
  ]);
}

function isToday(iso: string | null): boolean {
  if (!iso) return false;
  const d = new Date(iso);
  const now = new Date();
  return d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
}

type WidgetPhase = 'loading' | 'error' | 'ready';

// 위젯 5개(카탈로그·프로젝트·세션·개발환경·자율업무)가 loaded/error를 보지 않고
// items만 읽어 실패·로딩 중에도 "0개"를 단정하던 문제(U-01) — 3분기 판단
// 로직만 이 헬퍼로 공유하고, 각 phase에서 무엇을 보여줄지는 usageWidget/
// mcpHubWidget 선례처럼 위젯마다 따로 그린다.
function pickState(slice: { readonly loaded: boolean; readonly loading: boolean; readonly error: string | null }): WidgetPhase {
  if (slice.error) return 'error';
  if (!slice.loaded) return 'loading';
  return 'ready';
}

function taskBoardWidget(): HTMLElement {
  const phase = pickState(state.autonomousTasks);
  if (phase === 'loading') {
    return widgetShell('자율업무 진행상황', () => navigate('#/tasks/board'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['진행상황판 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('자율업무 진행상황', state.autonomousTasks.error, () => void loadAutonomousTasks());
  }

  const tasks = state.autonomousTasks.items;
  const running = tasks.filter((t) => t.running).length;
  const todayFail = tasks.filter((t) => t.status === 'failed' && isToday(t.lastFinishedAt)).length;

  return widgetShell('자율업무 진행상황', () => navigate('#/tasks/board'), [
    el('div', { className: 'home-widget-big-number' }, [`${running}개`]),
    el('div', { className: 'home-widget-desc' }, ['현재 실행 중']),
    el('div', { className: 'home-widget-breakdown' }, [el('span', {}, [`오늘 실패 ${todayFail}건`])]),
    el('div', { className: 'home-widget-link' }, ['진행상황판 보기 →']),
  ]);
}

// malgnai-hub MCP 서버 상태 — 이 조직이 가장 많이 쓰는 핵심 MCP 서버(모든
// 결정/이슈/작업이력을 기록하는 곳)만 골라 연결 여부를 보여준다. mcp_list()
// 결과에 그 이름의 서버가 없으면(예: 아직 등록 전) 에러를 던지지 않고
// "미설정"으로 조용히 처리한다.
function mcpHubWidget(): HTMLElement {
  const phase = pickState(state.mcp);
  if (phase === 'loading') {
    return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('malgnai-hub 연동', state.mcp.error, () => void loadMcp());
  }

  // R-01′: malgnai-hub 하나만 보고 끝내지 않고, 등록된 MCP 서버 전체에서 미연결
  // 상태를 집계해 조치 신호로 올린다. 이상 없으면(0건) 기존 문구 그대로 둔다.
  const disconnectedCount = state.mcp.items.filter((s) => !s.connected).length;
  const disconnectedBreakdown =
    disconnectedCount > 0
      ? [el('div', { className: 'home-widget-breakdown' }, [el('span', {}, [`⚠ 미연결 MCP ${disconnectedCount}개`])])]
      : [];

  const hub = state.mcp.items.find((s) => s.name === MALGNAI_HUB_MCP_NAME);
  if (!hub) {
    return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
      el('span', { className: 'badge badge-unknown' }, ['미설정']),
      el('div', { className: 'home-widget-desc' }, ['malgnai-hub MCP 서버가 등록되어 있지 않습니다.']),
      ...disconnectedBreakdown,
      el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
    ]);
  }

  return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
    el('span', { className: `badge ${hub.connected ? 'badge-active' : 'badge-archived'}` }, [hub.connected ? '연결됨' : '미연결']),
    el('div', { className: 'home-widget-desc' }, [hub.statusLabel]),
    ...disconnectedBreakdown,
    el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
  ]);
}

// claude CLI 로그인 상태 — `check_claude_auth_status`(claude_auth.rs, `claude
// auth status --json` 위임 조회)만 재사용한다(새 백엔드 커맨드를 만들지 않는다,
// 작업 지시). 로그인 시작 버튼은 세션 화면(views/sessions.ts)의 검증된
// "앱에서 로그인" 흐름을 그대로 재사용한다(handleClaudeAuthLoginStart →
// start_claude_auth_login) — 로직을 이 파일에 복제하지 않는다.
//
// 예전엔 이 위젯이 "되돌린 헤드리스 spawn(start_claude_auth_login)은 절대
// 호출하지 않는다"고 못박았었다(a7cc71a) — 그 시점엔 로그인 진행 패널(코드
// 입력창)이 세션 상세/draft 화면의 bottomFixed에만 있어서, 대시보드에서
// 로그인을 시작하면 붙여넣을 자리 자체가 없는 화면에 사용자가 갇혔기
// 때문이다. 이제 그 UI는 main.ts renderApp()이 라우트와 무관하게 앱 셸
// 최상위에서 모달로 그리므로(sessions.ts renderClaudeAuthLoginModal) 그
// 제약이 풀렸다 — 모달을 닫아도 재진입 배너(renderClaudeAuthLoginReopenBanner)가
// 화면 어디서든 다시 열 수 있게 해준다. "터미널 열기"는 이 환경에서 앱
// 로그인이 아예 안 될 때의 폴백으로 여전히 나란히 남겨둔다.
//
// 세 상태를 구분해 보여준다(직전 라운드 회귀의 핵심 교훈 — "없음"이 아니라
// "있음"으로 판정한다):
//   1) 확인 중  — 아직 한 번도 응답을 못 받은 상태(loaded=false, loading=true).
//   2) 로그인됨 — `loaded && !error && status.loggedIn === true`일 때만.
//      즉 "명시적 긍정 신호"가 있어야만 로그인됨으로 표시한다.
//   3) 로그인 안 됨·확인 실패 — 그 외 전부(조회 실패, loggedIn===false, 또는
//      아직 loaded===false인데 loading도 아닌 초기 렌더 프레임). "모름"과
//      "인증됨"을 구분하기 위해 이 버킷을 기본값으로 삼는다 — 근거 없이
//      "정상"처럼 보이는 표시가 v0.2.11 회귀의 본질이었다. 이 버킷에서 로그인이
//      진행 중이 아니면 "앱에서 로그인"/"터미널 열기 (claude login)" 두
//      버튼을 나란히 노출한다(sessions.ts renderChatErrorBlock과 동일 패턴).
//      이미 진행 중이면(state.claudeAuthLogin.active) 버튼 대신 안내만
//      보여준다 — 시작 버튼을 다시 눌러 중복 시작할 이유가 없고, 진행 상황은
//      모달 또는 재진입 배너가 이미 화면 어딘가에 떠 있다.
function claudeAuthWidget(): HTMLElement {
  const auth = state.claudeAuth;
  const login = state.claudeAuthLogin;
  const title = el('div', { className: 'home-widget-title' }, ['claude CLI 로그인']);

  if (!auth.loaded && auth.loading) {
    return el('div', { className: 'home-widget' }, [title, el('div', { className: 'home-widget-desc' }, ['확인 중…'])]);
  }

  const confirmedLoggedIn = auth.loaded && !auth.error && auth.status?.loggedIn === true;

  if (confirmedLoggedIn) {
    const email = auth.status?.email;
    // 라벨을 "Anthropic 계정 로그아웃"으로 명시한다 — 사이드바 좌하단의
    // "로그아웃"(이 앱 자체 Google 계정, sidebar.ts)과 같은 화면에 함께 떠
    // 있으므로 "로그아웃"만 쓰면 둘 다 같은 문구가 돼 어느 버튼이 무엇을
    // 끊는지 알 수 없다(작업 지시). 실측: 사이드바 로그아웃은 모든 화면에
    // 함께 렌더되는 `span[role=button]`이라, 여기서도 e2e 테스트를 작성할
    // 때는 page.getByRole 전역 조회가 아니라 이 위젯(.home-widget) 스코프로
    // 좁혀야 한다(settingsMcp.mjs의 `hubRow.getByRole(...)` 선례와 동일한
    // 이유 — 부분일치로 사이드바 요소를 잘못 집는 회귀를 피한다).
    return el('div', { className: 'home-widget' }, [
      title,
      el('span', { className: 'badge badge-active' }, ['로그인됨']),
      el('div', { className: 'home-widget-desc' }, [email ?? '인증된 상태입니다.']),
      el('div', { className: 'home-widget-btn-row' }, [
        el(
          'button',
          { className: 'btn', disabled: auth.loggingOut, onClick: () => void handleClaudeAuthLogout() },
          [auth.loggingOut ? '로그아웃 중…' : 'Anthropic 계정 로그아웃']
        ),
      ]),
    ]);
  }

  const reasonText = auth.error
    ? `로그인 상태를 확인하지 못했습니다: ${auth.error}`
    : 'claude CLI에 로그인되어 있지 않습니다.';

  const children: HTMLElement[] = [
    title,
    el('span', { className: 'badge badge-archived' }, ['로그인 필요']),
    el('div', { className: 'home-widget-desc' }, [reasonText]),
  ];

  if (login.active) {
    children.push(el('div', { className: 'home-widget-desc' }, ['로그인이 진행 중입니다 — 화면 우측 하단 안내에서 계속하세요.']));
  } else {
    if (login.error) {
      children.push(el('div', { className: 'home-widget-desc' }, [`⚠ ${login.error}`]));
    }
    children.push(
      el('div', { className: 'home-widget-btn-row' }, [
        el('button', { className: 'btn btn-primary', onClick: () => void handleClaudeAuthLoginStart() }, ['앱에서 로그인']),
        el('button', { className: 'btn', onClick: () => void handleOpenClaudeLoginTerminalFromHome() }, ['터미널 열기 (claude login)']),
      ])
    );
  }

  return el('div', { className: 'home-widget' }, children);
}

// views/sessions.ts의 handleClaudeLogin과 동일한 로직(openClaudeLoginTerminal
// + 토스트) — 그 함수는 그 파일 안에서만 쓰는 비공개 함수라 복제 대신 같은
// sessionsApi 호출을 여기서 독립적으로 감싼다. 앱이 대신 로그인하지 않고
// 사용자가 직접 보는 터미널 창을 여는 데까지만 관여하는 것도 동일하다.
async function handleOpenClaudeLoginTerminalFromHome(): Promise<void> {
  try {
    const result = await openClaudeLoginTerminal();
    showToast(result.message);
  } catch (err) {
    showToast(`터미널을 여는 데 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// claude CLI 자신의 Anthropic 계정 로그인을 끊는다(`claude auth logout` 위임
// 실행, sessionsApi.ts logoutClaudeAuth → claude_auth.rs logout_claude_auth).
// sidebar.ts의 앱 자체 로그아웃(resetStateForLogout)과 완전히 무관 — 이
// 핸들러는 state.claudeAuth 슬라이스만 건드린다.
//
// 확인 단계를 둔 이유: 로그아웃하면 이 위젯의 "앱에서 로그인" 버튼으로 다시
// 브라우저 왕복을 거쳐야 한다 — 45명 중 대부분이 CLI에 익숙하지 않아 실수로
// 누르면 자기가 무엇을 끊었는지 모른 채 작업이 막힐 수 있다. 이 프로젝트가
// 이미 삭제류 동작(MCP 서버 삭제, 자율업무 삭제 등)에 confirmDialog를 쓰는
// 것과 동일한 기준으로, "다시 사용하려면 재로그인이 필요하다"는 되돌리기
// 비용이 있는 동작이라 판단해 여기도 confirmDialog를 건다.
async function handleClaudeAuthLogout(): Promise<void> {
  if (state.claudeAuth.loggingOut) return; // 이중 클릭 방지

  // confirmLabel을 굳이 "로그아웃 확인"으로 구체화한다 — 이 모달이 열려 있는
  // 동안에도 사이드바의 정확히 "로그아웃"인 span[role=button]은 화면에 함께
  // 남아 있다(사이드바는 모든 화면에 렌더). 두 버튼 텍스트가 같으면
  // getByRole 부분일치 테스트가 어느 쪽을 눌렀는지 구분하지 못하는 회귀
  // 조건을 그대로 재현하게 된다(작업 지시 경고) — 라벨 자체를 다르게 두면
  // 스코프 없는 조회로도 모호함이 줄어든다.
  const proceed = await confirmDialog(
    'claude CLI의 Anthropic 계정 로그인을 해제합니다. 다시 사용하려면 브라우저로 재로그인해야 합니다. 계속할까요?',
    { title: 'Anthropic 계정 로그아웃', confirmLabel: '로그아웃 확인', danger: true }
  );
  if (!proceed) return;

  state.claudeAuth.loggingOut = true;
  notifyChange();
  try {
    // 완료 판정은 종료코드가 아니라 백엔드가 재조회한 `claude auth status
    // --json` 결과다(claude_auth.rs logout_claude_auth) — 그 결과를 그대로
    // state.claudeAuth.status에 반영하면 배지가 "로그인됨"/"로그인 필요"
    // 판정 로직(auth.status?.loggedIn)을 그대로 재사용해 즉시 갱신된다.
    const status = await logoutClaudeAuth();
    state.claudeAuth.status = status;
    state.claudeAuth.loaded = true;
    state.claudeAuth.error = null;
    showToast(
      status.loggedIn
        ? '로그아웃이 완료되지 않았습니다. 잠시 후 다시 시도해주세요.'
        : 'Anthropic 계정에서 로그아웃했습니다.'
    );
  } catch (err) {
    // 재확인 자체가 실패(확인 불가)한 경우 — "로그아웃됨"으로 단정하지 않는다
    // (claude_auth.rs 상단 경계와 동일 원칙). 기존 status는 그대로 두고
    // 에러만 알린다 — 배지가 실제로 확인되지 않은 상태를 "로그인 필요"처럼
    // 보여주면 안 된다.
    showToast(`로그아웃 상태를 확인하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.claudeAuth.loggingOut = false;
    notifyChange();
  }
}

/** main.ts가 로그인 직후(또는 홈으로 돌아올 때마다, 이전 시도가 에러로 끝나
 * loaded=false로 남아 있으면) 다른 위젯과 같은 가드 패턴으로 호출한다. */
export async function loadClaudeAuthStatus(): Promise<void> {
  state.claudeAuth.loading = true;
  state.claudeAuth.error = null;
  notifyChange();
  try {
    state.claudeAuth.status = await checkClaudeAuthStatus();
    state.claudeAuth.loaded = true;
  } catch (err) {
    state.claudeAuth.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.claudeAuth.loading = false;
    notifyChange();
  }
}

// ---------------- 실제 로컬 데이터 ----------------

function usageWidget(): HTMLElement {
  const usage = state.dailyUsage;
  const phase = pickState(usage);
  if (phase === 'loading') {
    return widgetShell('사용량 통계', () => navigate('#/usage'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['자세히 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('사용량 통계', state.dailyUsage.error, () => void loadDailyUsage());
  }
  if (usage.items.length === 0) {
    return widgetShell('사용량 통계', () => navigate('#/usage'), [
      el('div', { className: 'home-widget-desc' }, ['최근 30일 이내 사용 기록이 없습니다.']),
      el('div', { className: 'home-widget-link' }, ['자세히 보기 →']),
    ]);
  }
  const todayTokens = computeTodayTokens(usage.items);
  return widgetShell('사용량 통계', () => navigate('#/usage'), [
    el('div', { className: 'home-widget-big-number' }, [formatTokenCount(todayTokens)]),
    el('div', { className: 'home-widget-desc' }, ['오늘 총 토큰']),
    el('div', { className: 'home-widget-link' }, ['자세히 보기 →']),
  ]);
}

function catalogWidget(): HTMLElement {
  const phase = pickState(state.catalog);
  if (phase === 'loading') {
    return widgetShell('카탈로그', () => navigate('#/catalog'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['카탈로그 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('카탈로그', state.catalog.error, () => void loadCatalog());
  }

  const plugins = state.catalog.plugins;
  const totalAgents = plugins.reduce((sum, p) => sum + p.agents.length, 0);
  const totalSkills = plugins.reduce((sum, p) => sum + p.skills.length, 0);
  const totalKnowledge = plugins.reduce((sum, p) => sum + p.knowledge.length, 0);

  // R-01′: 형식 오류(status === 'invalid')는 카탈로그 화면에서만 보였다 —
  // 데이터는 이미 로그인 직후 globalCatalog로 프리페치돼 있으므로(main.ts) 홈이
  // 손에 쥔 값을 숫자로만 말하지 않고 조치 신호로 끌어올린다. 이상 없으면(0건)
  // 기존 breakdown 그대로 둔다.
  const globalEntries = state.globalCatalog.data ? [...state.globalCatalog.data.agents, ...state.globalCatalog.data.skills] : [];
  const invalidCount = globalEntries.filter((e) => e.status === 'invalid').length;

  return widgetShell('카탈로그', () => navigate('#/catalog'), [
    el('div', { className: 'home-widget-big-number' }, [`${plugins.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 플러그인']),
    el('div', { className: 'home-widget-breakdown' }, [
      el('span', {}, [`에이전트 ${totalAgents}`]),
      el('span', {}, [`스킬 ${totalSkills}`]),
      el('span', {}, [`지식 ${totalKnowledge}`]),
      ...(invalidCount > 0 ? [el('span', {}, [`⚠ 형식 오류 ${invalidCount}건`])] : []),
    ]),
    el('div', { className: 'home-widget-link' }, ['카탈로그 보기 →']),
  ]);
}

function projectsWidget(): HTMLElement {
  const phase = pickState(state.dashboard);
  if (phase === 'loading') {
    return widgetShell('프로젝트', () => navigate('#/projects'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['프로젝트 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('프로젝트', state.dashboard.error, () => void loadProjects());
  }

  const projects = state.dashboard.projects;
  const activeProjects = projects.filter((p) => p.archiveStatus === 'active').slice(0, 3);

  return widgetShell('프로젝트', () => navigate('#/projects'), [
    el('div', { className: 'home-widget-big-number' }, [`${projects.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['전체 등록 프로젝트']),
    el(
      'div',
      { className: 'home-widget-list' },
      activeProjects.length > 0
        ? activeProjects.map((p) => el('div', { className: 'home-widget-list-item' }, [p.name]))
        : [el('div', { className: 'home-widget-list-item muted' }, ['진행 중인 프로젝트가 없습니다.'])]
    ),
    el('div', { className: 'home-widget-link' }, ['프로젝트 보기 →']),
  ]);
}

// 세션목록의 정본은 registry(지금 실행 중인 프로세스) → jsonl 대화 이력이라
// 백엔드가 최근 30일/최대 100건을 항상 채워 돌려준다 — 전체 개수는 곧 상한
// (100)에 수렴해 "몇 개"로는 아무 정보도 주지 못한다. 이 위젯은 사용자가
// 실제로 궁금해할 "지금 뭔가 돌고 있나"에 답하도록 running===true인 항목만
// 센다(taskBoardWidget과 같은 패턴).
function sessionsWidget(): HTMLElement {
  const phase = pickState(state.sessions);
  if (phase === 'loading') {
    return widgetShell('세션목록', () => navigate('#/sessions'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['세션목록 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('세션목록', state.sessions.error, () => void loadSessions());
  }

  const runningCount = state.sessions.items.filter((s) => asBoolean(s.running)).length;
  return widgetShell('세션목록', () => navigate('#/sessions'), [
    el('div', { className: 'home-widget-big-number' }, [`${runningCount}개`]),
    el('div', { className: 'home-widget-desc' }, ['지금 실행 중인 세션']),
    el('div', { className: 'home-widget-link' }, ['세션목록 보기 →']),
  ]);
}

function devToolsWidget(): HTMLElement {
  const phase = pickState(state.devTools);
  if (phase === 'loading') {
    return widgetShell('개발 환경', () => navigate('#/settings/devtools'), [
      el('div', { className: 'home-widget-desc' }, ['확인 중…']),
      el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetErrorShell('개발 환경', state.devTools.error, () => void loadDevTools());
  }

  // R-01′: 몇 개가 설치됐는지가 아니라 "무엇이 미설치인가"가 조치 신호다 —
  // 설정 > 개발 환경 화면에서만 보이던 이름을 홈으로 끌어올린다. 전부 설치돼 있으면
  // (0건) 기존 문구·breakdown 없음 그대로 둔다.
  const installed = state.devTools.items.filter((t) => t.installed);
  const uninstalled = state.devTools.items.filter((t) => !t.installed);
  const total = state.devTools.items.length;

  return widgetShell('개발 환경', () => navigate('#/settings/devtools'), [
    el('div', { className: 'home-widget-big-number' }, [`${installed.length}/${total} 설치됨`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 CLI 도구']),
    ...(uninstalled.length > 0
      ? [el('div', { className: 'home-widget-breakdown' }, uninstalled.map((t) => el('span', {}, [`⚠ ${t.name} 미설치`])))]
      : []),
    el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
  ]);
}
