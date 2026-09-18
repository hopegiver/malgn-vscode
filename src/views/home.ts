// 홈 대시보드 — 로그인 직후 첫 화면. 다른 화면들의 핵심 정보를 카드/위젯으로 모아
// 보여준다. 위젯 클릭 시 해당 상세 화면으로 이동한다. 프로젝트·세션·개발환경·
// 카탈로그·사용량 통계·자율업무 위젯은 전부 실제 로컬 데이터를 쓴다(main.ts가
// 로그인 직후 한 번씩 미리 불러온다).
import { el, clickable } from '../dom';
import { state } from '../state';
import { navigate } from '../route';
import { computeTodayTokens, formatTokenCount, loadDailyUsage } from './usage';
import { asBoolean, loadSessions } from './sessions';
import { loadCatalog } from './catalog';
import { loadProjects } from './projects';
import { loadDevTools } from './devTools';
import { loadAutonomousTasks } from './autonomousTasks';
import { loadMcp } from './settings';

const MALGNAI_HUB_MCP_NAME = 'plugin:malgn-agent:malgnai-hub';

export function renderHomeView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['대시보드']), el('div', { className: 'page-subtitle' }, ['전체 요약'])]),
  ]);

  const grid = el('div', { className: 'home-widget-grid' }, [
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
function widgetErrorShell(title: string, onRetry: () => void): HTMLElement {
  return widgetShell(title, onRetry, [
    el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
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
    return widgetErrorShell('자율업무 진행상황', () => void loadAutonomousTasks());
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
    return widgetErrorShell('malgnai-hub 연동', () => void loadMcp());
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
    return widgetErrorShell('사용량 통계', () => void loadDailyUsage());
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
    return widgetErrorShell('카탈로그', () => void loadCatalog());
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
    return widgetErrorShell('프로젝트', () => void loadProjects());
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
    return widgetErrorShell('세션목록', () => void loadSessions());
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
    return widgetErrorShell('개발 환경', () => void loadDevTools());
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
