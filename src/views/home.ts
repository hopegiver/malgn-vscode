// 홈 대시보드 — 로그인 직후 첫 화면. Terminus 목업(docs/design/terminus-mockup.html)
// 구조를 그대로 옮긴다: 인사말 → 통계 타일 4개 → 2단(최근 세션/자율 작업 큐 |
// 개발 도구/이번 주 토큰 사용량). 데이터 매핑은 docs/design/terminus-shell-ia.md
// §6이 정본이다.
//
// claude CLI 로그인/malgnai-hub 연동/카탈로그 위젯은 목업에 없던 요소지만
// 실제 기능(특히 Anthropic 계정 로그인/로그아웃, MCP 상태)이라 삭제하지
// 않고 하단 보조 그리드로 옮긴다(ui-harness home.mjs가 .home-widget 클래스와
// 원문 텍스트로 11개 시나리오를 검증하므로 클래스/문구를 그대로 유지) — 그
// 외 위젯(프로젝트/세션목록/개발환경/사용량통계/자율업무 진행상황 요약)은
// 이번 라운드에서 신설한 통계 타일·박스가 같은 데이터를 이미 보여주므로
// 중복 제거했다.
import { el, clickable, showToast, confirmDialog, loadingBlock, errorBlock } from '../dom';
import { state, notifyChange } from '../state';
import { navigate } from '../route';
import { computeTodayTokens, computeUsageTotals, dailyUsageTotal, formatTokenCount, loadDailyUsage } from './usage';
import type { DailyUsage } from '../usageApi';
import { asBoolean, asNumber, asString, loadSessions, handleClaudeAuthLoginStart, projectNameFromCwd, sessionTitle, sortedSessions } from './sessions';
import type { ClaudeSessionRecord } from '../sessionsApi';
import { loadCatalog } from './catalog';
import type { AutonomousTask } from '../state';
import { loadAutonomousTasks } from './autonomousTasks';
import { loadDevTools } from './devTools';
import type { DevToolStatus } from '../devToolsApi';
import { loadMcp } from './settings';
import { ensureAppVersionLoaded } from '../sidebar';
import { checkClaudeAuthStatus, openClaudeLoginTerminal, logoutClaudeAuth } from '../sessionsApi';

const MALGNAI_HUB_MCP_NAME = 'plugin:malgn-agent:malgnai-hub';
const RECENT_SESSIONS_LIMIT = 5;
const TASK_QUEUE_LIMIT = 5;

export function renderHomeView(): HTMLElement {
  ensureAppVersionLoaded();

  return el('div', { className: 'home-view' }, [
    renderGreeting(),
    renderStatRow(),
    el('div', { className: 'split' }, [
      el('div', { className: 'col' }, [recentSessionsBox(), taskQueueBox()]),
      el('div', { className: 'col' }, [devToolsBox(), weeklyUsageBox()]),
    ]),
    el('div', { className: 'home-extra-grid' }, [claudeAuthWidget(), mcpHubWidget(), catalogWidget()]),
  ]);
}

// ---------------- 인사말(정적 커서) ----------------
function greetingPhrase(hour: number): string {
  if (hour >= 5 && hour < 12) return '좋은 아침입니다';
  if (hour >= 12 && hour < 18) return '좋은 오후입니다';
  return '좋은 저녁입니다';
}

const WEEKDAY_KO = ['일', '월', '화', '수', '목', '금', '토'] as const;

function renderGreeting(): HTMLElement {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  const dateLabel = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())} (${WEEKDAY_KO[now.getDay()]}) ${pad(now.getHours())}:${pad(now.getMinutes())}`;
  const name = state.auth.userName ? `, ${state.auth.userName}님` : '';

  const subParts: (Node | string)[] = [`${dateLabel} · `];
  if (state.dashboard.loaded) {
    subParts.push(el('b', {}, [String(state.dashboard.projects.length)]), '개 워크스페이스 연결됨 · ');
  }
  if (state.sessions.loaded) {
    const running = state.sessions.items.filter((s) => asBoolean(s.running)).length;
    subParts.push('실행 중 세션 ', el('b', {}, [String(running)]), '개');
  }

  return el('div', { className: 'greet' }, [
    el('div', { className: 'greet-line' }, [el('span', { className: 'sym' }, ['❯']), `${greetingPhrase(now.getHours())}${name}`, el('span', { className: 'cursor' }, [])]),
    el('div', { className: 'greet-sub' }, subParts),
  ]);
}

// ---------------- 통계 타일 ----------------
function statTile(label: string, value: string, trend?: { readonly text: string; readonly tone: 'up' | 'warn' | 'flat' }): HTMLElement {
  return el('div', { className: 'stat' }, [
    el('div', { className: 'stat-label' }, [label]),
    el('div', { className: 'stat-value' }, [value]),
    ...(trend ? [el('div', { className: `stat-trend ${trend.tone}` }, [trend.text])] : []),
  ]);
}

function renderStatRow(): HTMLElement {
  const dash = state.dashboard;
  const activeValue = dash.loaded ? `${dash.projects.filter((p) => p.archiveStatus === 'active').length}` : '—';
  const totalValue = dash.loaded ? ` / ${dash.projects.length}` : '';

  const runningSessions = state.sessions.loaded ? String(state.sessions.items.filter((s) => asBoolean(s.running)).length) : '—';

  const usageTotals = computeUsageTotals(state.dailyUsage.items);
  const todayTokens = state.dailyUsage.loaded ? formatTokenCount(computeTodayTokens(state.dailyUsage.items)) : '—';
  const cacheHitTrend =
    state.dailyUsage.loaded && usageTotals.cacheHitRate !== null
      ? { text: `캐시 히트율 ${usageTotals.cacheHitRate}%`, tone: 'flat' as const }
      : undefined;

  const waitingTasks = state.autonomousTasks.loaded
    ? String(state.autonomousTasks.items.filter((t) => t.enabled && !t.running).length)
    : '—';

  return el('div', { className: 'stat-row' }, [
    statTile('활성 프로젝트', `${activeValue}${totalValue}`),
    statTile('실행 중 세션', runningSessions),
    statTile('오늘 토큰 사용량', todayTokens, cacheHitTrend),
    statTile('대기 중 자율 작업', waitingTasks),
  ]);
}

// ---------------- 박스 공용 헤더 ----------------
function boxHead(title: string, right: Node | string): HTMLElement {
  return el('div', { className: 'box-head' }, [el('span', { className: 'box-title' }, [title]), right]);
}

// ---------------- 최근 세션 ----------------
function shortSessionId(id: string): string {
  return id.length <= 8 ? `#${id}` : `#${id.slice(0, 8)}…`;
}

function formatCompactElapsed(ms: number): string {
  const diffMin = Math.floor((Date.now() - ms) / 60000);
  if (diffMin < 1) return '방금';
  if (diffMin < 60) return `${diffMin}m`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) {
    const remMin = diffMin % 60;
    return remMin > 0 ? `${diffHour}h ${String(remMin).padStart(2, '0')}m` : `${diffHour}h`;
  }
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay === 1) return '어제';
  return `${diffDay}일 전`;
}

function sessionBlistRow(s: ClaudeSessionRecord): HTMLElement {
  const sessionId = asString(s.sessionId);
  const running = asBoolean(s.running);
  const updatedAt = asNumber(s.updatedAt) ?? asNumber(s.startedAt);
  const row = el('div', { className: 'blist-row blist-row--sessions' }, [
    el('span', { className: 'blist-id' }, [shortSessionId(sessionId)]),
    el('span', { className: 'blist-accent' }, [projectNameFromCwd(asString(s.cwd))]),
    el('span', { className: 'blist-body' }, [sessionTitle(s)]),
    el('span', { className: `blist-status ${running ? 'status-run' : 'status-done'}` }, [el('span', { className: 'dot' }, []), running ? '실행중' : '완료']),
    el('span', { className: 'blist-time' }, [updatedAt !== null ? formatCompactElapsed(updatedAt) : '-']),
  ]);
  return clickable(row, () => navigate(`#/sessions/${encodeURIComponent(sessionId)}`));
}

function recentSessionsBox(): HTMLElement {
  const sessions = state.sessions;
  let body: HTMLElement;
  let countLabel = '전체 보기 →';

  if (sessions.error) {
    body = errorBlock(sessions.error, () => void loadSessions());
  } else if (!sessions.loaded) {
    body = loadingBlock();
  } else {
    const items = sortedSessions().slice(0, RECENT_SESSIONS_LIMIT);
    if (items.length === 0) {
      body = el('div', { className: 'state-block-desc' }, ['최근 세션이 없습니다.']);
    } else {
      body = el('div', { className: 'blist' }, items.map(sessionBlistRow));
      countLabel = `${items.length}건 표시 · 전체 보기 →`;
    }
  }

  return el('div', { className: 'box' }, [
    boxHead('최근 세션', clickable(el('span', { className: 'count' }, [countLabel]), () => navigate('#/sessions'))),
    el('div', { className: 'box-body' }, [body]),
  ]);
}

// ---------------- 자율 작업 큐 ----------------
function taskQueueRow(t: AutonomousTask): HTMLElement {
  const row = el('div', { className: 'blist-row blist-row--tasks' }, [
    el('span', { className: 'blist-id' }, [t.id]),
    el('span', { className: 'blist-body' }, [`${t.projectName} · ${t.name}`]),
    el('span', { className: `blist-badge ${t.running ? 'run' : 'wait'}` }, [t.running ? '진행중' : '대기']),
  ]);
  return clickable(row, () => navigate(`#/tasks/item/${encodeURIComponent(t.id)}`));
}

function taskQueueBox(): HTMLElement {
  const tasks = state.autonomousTasks;
  let body: HTMLElement;
  let countLabel = '전체 보기 →';

  if (tasks.error) {
    body = errorBlock(tasks.error, () => void loadAutonomousTasks());
  } else if (!tasks.loaded) {
    body = loadingBlock();
  } else {
    const queued = tasks.items.filter((t) => t.running || (t.enabled && !t.running)).slice(0, TASK_QUEUE_LIMIT);
    if (queued.length === 0) {
      body = el('div', { className: 'state-block-desc' }, ['대기 중인 자율 작업이 없습니다.']);
    } else {
      body = el('div', { className: 'blist' }, queued.map(taskQueueRow));
      countLabel = `${queued.length}건 대기 · 전체 보기 →`;
    }
  }

  return el('div', { className: 'box' }, [
    boxHead('자율 작업 큐', clickable(el('span', { className: 'count' }, [countLabel]), () => navigate('#/tasks'))),
    el('div', { className: 'box-body' }, [body]),
  ]);
}

// ---------------- 개발 도구 ----------------
function devToolBlistRow(t: DevToolStatus): HTMLElement {
  const flag = t.installed
    ? el('span', { className: 'blist-devtool-flag ok' }, ['✓ 설치됨'])
    : el('span', { className: `blist-devtool-flag ${t.required ? 'bad' : 'missing'}` }, ['미설치']);
  return el('div', { className: 'blist-row blist-row--devtools' }, [
    el('span', { className: 'blist-devtool-sym' }, ['$']),
    el('span', { className: 'blist-body' }, [t.name]),
    el('span', { className: 'blist-devtool-ver' }, [t.version ?? '-']),
    flag,
  ]);
}

function devToolsBox(): HTMLElement {
  const devTools = state.devTools;
  let body: HTMLElement;

  if (devTools.error) {
    body = errorBlock(devTools.error, () => void loadDevTools());
  } else if (!devTools.loaded) {
    body = loadingBlock();
  } else if (devTools.items.length === 0) {
    body = el('div', { className: 'state-block-desc' }, ['조회된 개발 도구가 없습니다.']);
  } else {
    body = el('div', { className: 'blist' }, devTools.items.map(devToolBlistRow));
  }

  return el('div', { className: 'box' }, [
    boxHead('개발 도구', clickable(el('span', { className: 'count' }, ['전체 보기 →']), () => navigate('#/settings/devtools'))),
    el('div', { className: 'box-body' }, [body]),
  ]);
}

// ---------------- 이번 주 토큰 사용량 ----------------
function localDateKeyOf(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function weekdayLabelFor(dateKey: string): string {
  const d = new Date(`${dateKey}T00:00:00`);
  if (Number.isNaN(d.getTime())) return dateKey;
  return WEEKDAY_KO[d.getDay()];
}

function buildSparkline(days: readonly DailyUsage[]): HTMLElement {
  const values = days.map(dailyUsageTotal);
  const width = 280;
  const height = 64;
  const pad = 6;
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const range = Math.max(max - min, 1);
  const n = values.length;
  const stepX = n > 1 ? width / (n - 1) : 0;
  const points = values.map((v, i) => ({
    x: n > 1 ? i * stepX : width / 2,
    y: height - pad - ((v - min) / range) * (height - pad * 2),
  }));

  const svgNS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(svgNS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', '최근 일별 토큰 사용량 추이');

  const polygon = document.createElementNS(svgNS, 'polygon');
  polygon.setAttribute(
    'points',
    [`0,${height}`, ...points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`), `${width},${height}`].join(' ')
  );
  polygon.setAttribute('fill', 'var(--accent)');
  polygon.setAttribute('opacity', '0.12');
  svg.appendChild(polygon);

  const polyline = document.createElementNS(svgNS, 'polyline');
  polyline.setAttribute('points', points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '));
  polyline.setAttribute('fill', 'none');
  polyline.setAttribute('stroke', 'var(--accent)');
  polyline.setAttribute('stroke-width', '2');
  polyline.setAttribute('stroke-linejoin', 'round');
  polyline.setAttribute('stroke-linecap', 'round');
  svg.appendChild(polyline);

  const todayKey = localDateKeyOf(new Date());
  points.forEach((p, i) => {
    const circle = document.createElementNS(svgNS, 'circle');
    circle.setAttribute('cx', p.x.toFixed(1));
    circle.setAttribute('cy', p.y.toFixed(1));
    circle.setAttribute('r', i === points.length - 1 ? '3.2' : '2.4');
    circle.setAttribute('fill', 'var(--accent)');
    const title = document.createElementNS(svgNS, 'title');
    title.textContent = `${days[i].date === todayKey ? '오늘' : weekdayLabelFor(days[i].date)} ${formatTokenCount(values[i])}`;
    circle.appendChild(title);
    svg.appendChild(circle);
  });

  const dayLabels = el(
    'div',
    { className: 'spark-days' },
    days.map((d) => el('span', {}, [d.date === todayKey ? '오늘' : weekdayLabelFor(d.date)]))
  );

  return el('div', {}, [el('div', { className: 'spark-wrap' }, [svg]), dayLabels]);
}

function weeklyUsageBox(): HTMLElement {
  const usage = state.dailyUsage;
  let body: HTMLElement;

  if (usage.error) {
    body = errorBlock(usage.error, () => void loadDailyUsage());
  } else if (!usage.loaded) {
    body = loadingBlock();
  } else if (usage.items.length === 0) {
    body = el('div', { className: 'state-block-desc' }, ['최근 7일 이내 사용 기록이 없습니다.']);
  } else {
    const last7 = [...usage.items].sort((a, b) => a.date.localeCompare(b.date)).slice(-7);
    const weekTotal = last7.reduce((sum, d) => sum + dailyUsageTotal(d), 0);
    const totals = computeUsageTotals(last7);
    body = el('div', {}, [
      el('div', { className: 'usage-figures' }, [
        el('div', { className: 'usage-figure' }, [el('span', { className: 'v' }, [formatTokenCount(weekTotal)]), el('span', { className: 'l' }, ['이번 주'])]),
        ...(totals.cacheHitRate !== null
          ? [el('div', { className: 'usage-figure' }, [el('span', { className: 'v' }, [`${totals.cacheHitRate}%`]), el('span', { className: 'l' }, ['캐시 히트율'])])]
          : []),
      ]),
      buildSparkline(last7),
    ]);
  }

  return el('div', { className: 'box' }, [
    boxHead('이번 주 토큰 사용량', clickable(el('span', { className: 'count' }, ['자세히 보기 →']), () => navigate('#/usage'))),
    el('div', { className: 'box-body' }, [body]),
  ]);
}

// ---------------- 보조 위젯(목업에 없던 기존 기능 — 삭제하지 않고 승계) ----------------
function widgetShell(title: string, onClick: () => void, children: readonly (Node | string)[]): HTMLElement {
  const widget = el('div', { className: 'home-widget' }, [el('div', { className: 'home-widget-title' }, [title]), ...children]);
  return clickable(widget, onClick);
}

function widgetErrorShell(title: string, message: string | null, onRetry: () => void): HTMLElement {
  return widgetShell(title, onRetry, [
    el('div', { className: 'home-widget-desc' }, [message ?? '상태를 불러오지 못했습니다.']),
    el('div', { className: 'home-widget-link' }, ['다시 확인하기 →']),
  ]);
}

type WidgetPhase = 'loading' | 'error' | 'ready';

function pickState(slice: { readonly loaded: boolean; readonly loading: boolean; readonly error: string | null }): WidgetPhase {
  if (slice.error) return 'error';
  if (!slice.loaded) return 'loading';
  return 'ready';
}

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

// claude CLI 로그인 상태 — `check_claude_auth_status`(claude_auth.rs, `claude
// auth status --json` 위임 조회)만 재사용한다. 로그인 시작 버튼은 세션 화면
// (views/sessions.ts)의 검증된 "앱에서 로그인" 흐름을 그대로 재사용한다.
// (아래 세 상태 구분/로그아웃 확인 절차는 구 home.ts와 동일 — ui-harness
// home.mjs 11개 시나리오가 그대로 검증한다.)
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

async function handleOpenClaudeLoginTerminalFromHome(): Promise<void> {
  try {
    const result = await openClaudeLoginTerminal();
    showToast(result.message);
  } catch (err) {
    showToast(`터미널을 여는 데 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  }
}

async function handleClaudeAuthLogout(): Promise<void> {
  if (state.claudeAuth.loggingOut) return;

  const proceed = await confirmDialog(
    'claude CLI의 Anthropic 계정 로그인을 해제합니다. 다시 사용하려면 브라우저로 재로그인해야 합니다. 계속할까요?',
    { title: 'Anthropic 계정 로그아웃', confirmLabel: '로그아웃 확인', danger: true }
  );
  if (!proceed) return;

  state.claudeAuth.loggingOut = true;
  notifyChange();
  try {
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
    showToast(`로그아웃 상태를 확인하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.claudeAuth.loggingOut = false;
    notifyChange();
  }
}

/** main.ts가 로그인 직후(또는 홈으로 돌아올 때마다) 다른 위젯과 같은 가드
 * 패턴으로 호출한다. */
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
