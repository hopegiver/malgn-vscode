// 홈 대시보드 — 로그인 직후 첫 화면. 다른 화면들의 핵심 정보를 카드/위젯으로 모아
// 보여준다. 위젯 클릭 시 해당 상세 화면으로 이동한다. 프로젝트·세션·개발환경·
// 카탈로그·사용량 통계·자율업무 위젯은 전부 실제 로컬 데이터를 쓴다(main.ts가
// 로그인 직후 한 번씩 미리 불러온다).
import { el } from '../dom';
import { state } from '../state';
import { navigate } from '../route';
import { computeTodayTokens, formatTokenCount } from './usage';
import { asBoolean } from './sessions';

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
  const widget = el('div', { className: 'home-widget', onClick }, [el('div', { className: 'home-widget-title' }, [title]), ...children]);
  widget.setAttribute('role', 'button');
  widget.setAttribute('tabindex', '0');
  return widget;
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
    return widgetShell('자율업무 진행상황', () => navigate('#/tasks/board'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['진행상황판 보기 →']),
    ]);
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
  if (!state.mcp.loaded && !state.mcp.error) {
    return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
    ]);
  }
  if (state.mcp.error) {
    return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
    ]);
  }

  const hub = state.mcp.items.find((s) => s.name === MALGNAI_HUB_MCP_NAME);
  if (!hub) {
    return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
      el('span', { className: 'badge badge-unknown' }, ['미설정']),
      el('div', { className: 'home-widget-desc' }, ['malgnai-hub MCP 서버가 등록되어 있지 않습니다.']),
      el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
    ]);
  }

  return widgetShell('malgnai-hub 연동', () => navigate('#/settings/mcp'), [
    el('span', { className: `badge ${hub.connected ? 'badge-active' : 'badge-archived'}` }, [hub.connected ? '연결됨' : '미연결']),
    el('div', { className: 'home-widget-desc' }, [hub.statusLabel]),
    el('div', { className: 'home-widget-link' }, ['MCP 관리 보기 →']),
  ]);
}

// ---------------- 실제 로컬 데이터 ----------------

function usageWidget(): HTMLElement {
  const usage = state.dailyUsage;
  if (!usage.loaded && !usage.error) {
    return widgetShell('사용량 통계', () => navigate('#/usage'), [
      el('div', { className: 'home-widget-desc' }, ['불러오는 중…']),
      el('div', { className: 'home-widget-link' }, ['자세히 보기 →']),
    ]);
  }
  if (usage.error || usage.items.length === 0) {
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
    return widgetShell('카탈로그', () => navigate('#/catalog'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['카탈로그 보기 →']),
    ]);
  }

  const plugins = state.catalog.plugins;
  const totalAgents = plugins.reduce((sum, p) => sum + p.agents.length, 0);
  const totalSkills = plugins.reduce((sum, p) => sum + p.skills.length, 0);
  const totalKnowledge = plugins.reduce((sum, p) => sum + p.knowledge.length, 0);

  return widgetShell('카탈로그', () => navigate('#/catalog'), [
    el('div', { className: 'home-widget-big-number' }, [`${plugins.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 플러그인']),
    el('div', { className: 'home-widget-breakdown' }, [
      el('span', {}, [`에이전트 ${totalAgents}`]),
      el('span', {}, [`스킬 ${totalSkills}`]),
      el('span', {}, [`지식 ${totalKnowledge}`]),
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
    return widgetShell('프로젝트', () => navigate('#/projects'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['프로젝트 보기 →']),
    ]);
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
    return widgetShell('세션목록', () => navigate('#/sessions'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['세션목록 보기 →']),
    ]);
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
    return widgetShell('개발 환경', () => navigate('#/dev-tools'), [
      el('div', { className: 'home-widget-desc' }, ['확인 중…']),
      el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
    ]);
  }
  if (phase === 'error') {
    return widgetShell('개발 환경', () => navigate('#/dev-tools'), [
      el('div', { className: 'home-widget-desc' }, ['상태를 불러오지 못했습니다.']),
      el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
    ]);
  }

  const installedCount = state.devTools.items.filter((t) => t.installed).length;
  const total = state.devTools.items.length;

  return widgetShell('개발 환경', () => navigate('#/dev-tools'), [
    el('div', { className: 'home-widget-big-number' }, [`${installedCount}/${total}`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 CLI 도구']),
    el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
  ]);
}
