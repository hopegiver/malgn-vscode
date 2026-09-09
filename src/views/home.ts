// 홈 대시보드 — 로그인 직후 첫 화면. 다른 화면들의 핵심 정보를 카드/위젯으로 모아
// 보여준다. 위젯 클릭 시 해당 상세 화면으로 이동한다. 프로젝트·세션·개발환경·
// 카탈로그·사용량 통계·자율업무 위젯은 전부 실제 로컬 데이터를 쓴다(main.ts가
// 로그인 직후 한 번씩 미리 불러온다).
import { el } from '../dom';
import { state } from '../state';
import { navigate } from '../route';
import { computeTodayTokens, formatTokenCount } from './usage';

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

function taskBoardWidget(): HTMLElement {
  const tasks = state.autonomousTasks.items;
  const running = tasks.filter((t) => t.lastStatus === 'running').length;
  const todayFail = tasks.filter((t) => t.lastStatus === 'failed' && isToday(t.lastRunAt)).length;

  return widgetShell('자율업무 진행상황', () => navigate('#/tasks/board'), [
    el('div', { className: 'home-widget-big-number' }, [`${running}개`]),
    el('div', { className: 'home-widget-desc' }, ['현재 실행 중 (실제 로컬 데이터)']),
    el('div', { className: 'home-widget-breakdown' }, [el('span', {}, [`오늘 실패 ${todayFail}건`])]),
    el('div', { className: 'home-widget-link' }, ['진행상황판 보기 →']),
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
    el('div', { className: 'home-widget-desc' }, ['오늘 총 토큰 (실제 로컬 데이터)']),
    el('div', { className: 'home-widget-link' }, ['자세히 보기 →']),
  ]);
}

function catalogWidget(): HTMLElement {
  const plugins = state.catalog.plugins;
  const totalAgents = plugins.reduce((sum, p) => sum + p.agents.length, 0);
  const totalSkills = plugins.reduce((sum, p) => sum + p.skills.length, 0);
  const totalKnowledge = plugins.reduce((sum, p) => sum + p.knowledge.length, 0);

  return widgetShell('카탈로그', () => navigate('#/catalog'), [
    el('div', { className: 'home-widget-big-number' }, [`${plugins.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 플러그인 (실제 로컬 데이터)']),
    el('div', { className: 'home-widget-breakdown' }, [
      el('span', {}, [`에이전트 ${totalAgents}`]),
      el('span', {}, [`스킬 ${totalSkills}`]),
      el('span', {}, [`지식 ${totalKnowledge}`]),
    ]),
    el('div', { className: 'home-widget-link' }, ['카탈로그 보기 →']),
  ]);
}

function projectsWidget(): HTMLElement {
  const projects = state.dashboard.projects;
  const activeProjects = projects.filter((p) => p.archiveStatus === 'active').slice(0, 3);

  return widgetShell('프로젝트', () => navigate('#/projects'), [
    el('div', { className: 'home-widget-big-number' }, [`${projects.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['전체 등록 프로젝트 (실제 로컬 데이터)']),
    el(
      'div',
      { className: 'home-widget-list' },
      activeProjects.length > 0
        ? activeProjects.map((p) => el('div', { className: 'home-widget-list-item' }, [p.name]))
        : [el('div', { className: 'home-widget-list-item muted' }, ['불러오는 중…'])]
    ),
    el('div', { className: 'home-widget-link' }, ['프로젝트 보기 →']),
  ]);
}

function sessionsWidget(): HTMLElement {
  return widgetShell('세션목록', () => navigate('#/sessions'), [
    el('div', { className: 'home-widget-big-number' }, [`${state.sessions.items.length}개`]),
    el('div', { className: 'home-widget-desc' }, ['~/.claude/sessions/*.json (실제 로컬 데이터)']),
    el('div', { className: 'home-widget-link' }, ['세션목록 보기 →']),
  ]);
}

function devToolsWidget(): HTMLElement {
  const installedCount = state.devTools.items.filter((t) => t.installed).length;
  const total = state.devTools.items.length;

  return widgetShell('개발 환경', () => navigate('#/dev-tools'), [
    el('div', { className: 'home-widget-big-number' }, [`${installedCount}/${total || 7}`]),
    el('div', { className: 'home-widget-desc' }, ['설치된 CLI 도구 (실제 조회값)']),
    el('div', { className: 'home-widget-link' }, ['개발 환경 보기 →']),
  ]);
}
