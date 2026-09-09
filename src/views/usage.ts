// 사용량 통계 — "일별 사용량"은 실제 ~/.claude/projects/**/*.jsonl 집계(최근
// 30일)이고, 날짜 막대를 클릭하면 그 날짜 하루만 세션/에이전트/툴 단위로
// 재집계한 상세를 그 자리에 펼친다(dailyDetailApi.ts 참고). 실시간 감시는 하지
// 않는다 — "사용량 통계" 메뉴를 클릭할 때마다 새로 불러온다(sidebar.ts).
// 5시간/주간 사용량 패널은 로컬 데이터 소스가 없어 여전히 mockData.ts 샘플이다.
import { el } from '../dom';
import { state, notifyChange } from '../state';
import { MOCK_USAGE } from '../mockData';
import type { UsageSummaryStat, UsageWindow } from '../mockData';
import { fetchDailyUsage } from '../usageApi';
import type { DailyUsage } from '../usageApi';
import { fetchDailyDetail } from '../dailyDetailApi';
import type { AgentUsage, SessionDetail, ToolUsage } from '../dailyDetailApi';

function barFillEl(pct: number): HTMLElement {
  const fill = el('div', { className: 'bar-fill' }, []);
  fill.style.width = `${Math.min(100, Math.max(0, pct))}%`;
  return fill;
}

// 5시간 주기 / 주간 사용량 패널 — 홈 대시보드 축약 위젯과 사용량 통계 전체 화면이
// 같은 컴포넌트를 재사용한다.
export function renderUsageWindowCard(w: UsageWindow, title: string, compact = false): HTMLElement {
  return el('div', { className: `usage-window-card${compact ? ' compact' : ''}` }, [
    el('div', { className: 'usage-window-head' }, [
      el('span', { className: 'usage-window-title' }, [title]),
      el('span', { className: 'usage-window-percent' }, [`${w.usedPercent}%`]),
    ]),
    el('div', { className: 'bar-track' }, [barFillEl(w.usedPercent)]),
    el('div', { className: 'usage-window-foot' }, [el('span', {}, [w.usedLabel]), el('span', {}, [w.resetLabel])]),
  ]);
}

function statCardEl(s: UsageSummaryStat): HTMLElement {
  return el('div', { className: 'stat-card' }, [
    el('div', { className: 'stat-card-label' }, [s.label]),
    el('div', { className: 'stat-card-value' }, [s.value]),
    ...(s.delta ? [el('div', { className: `stat-card-delta ${s.deltaPositive ? 'up' : 'down'}` }, [s.delta])] : []),
  ]);
}

function formatUsd(v: number): string {
  return `$${v.toFixed(2)}`;
}

function formatTokenCount(v: number): string {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(2)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  return String(v);
}

// ---------------- 일별 사용량 (실제 데이터) ----------------

export async function loadDailyUsage(): Promise<void> {
  state.dailyUsage.loading = true;
  state.dailyUsage.error = null;
  notifyChange();
  try {
    state.dailyUsage.items = await fetchDailyUsage();
    state.dailyUsage.loaded = true;
  } catch (err) {
    state.dailyUsage.error = err instanceof Error ? err.message : '사용량 데이터를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.dailyUsage.loading = false;
    notifyChange();
  }
}

function dailyUsageTotal(d: DailyUsage): number {
  return d.inputTokens + d.outputTokens + d.cacheCreationTokens + d.cacheReadTokens;
}

// ---------------- 날짜별 상세 (실제 데이터) ----------------

export async function loadDailyDetail(date: string): Promise<void> {
  state.dailyDetail.loading = true;
  state.dailyDetail.error = null;
  notifyChange();
  try {
    state.dailyDetail.report = await fetchDailyDetail(date);
  } catch (err) {
    state.dailyDetail.error = err instanceof Error ? err.message : '상세 데이터를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.dailyDetail.loading = false;
    notifyChange();
  }
}

function toggleDailyDetail(date: string): void {
  if (state.dailyDetail.selectedDate === date) {
    state.dailyDetail.selectedDate = null;
    state.dailyDetail.report = null;
    state.dailyDetail.error = null;
    notifyChange();
    return;
  }
  state.dailyDetail.selectedDate = date;
  state.dailyDetail.report = null;
  notifyChange();
  void loadDailyDetail(date);
}

function renderAgentUsageTable(agents: readonly AgentUsage[]): HTMLElement {
  if (agents.length === 0) return el('div', { className: 'state-block-desc' }, ['에이전트 사용 기록이 없습니다.']);
  const header = el('div', { className: 'thief-table-row thief-table-row-3 thief-table-head' }, [
    el('span', {}, ['에이전트']),
    el('span', {}, ['턴']),
    el('span', {}, ['토큰']),
    el('span', {}, ['비용']),
  ]);
  const rows = agents.map((a) =>
    el('div', { className: 'thief-table-row thief-table-row-3' }, [
      el('span', { className: 'thief-table-title' }, [a.agentType]),
      el('span', {}, [String(a.turns)]),
      el('span', {}, [formatTokenCount(a.totalTokens)]),
      el('span', {}, [formatUsd(a.costUsd)]),
    ])
  );
  return el('div', { className: 'thief-table' }, [header, ...rows]);
}

function renderToolUsageList(tools: readonly ToolUsage[]): HTMLElement {
  if (tools.length === 0) return el('div', { className: 'state-block-desc' }, ['툴 사용 기록이 없습니다.']);
  return el(
    'div',
    { className: 'tool-usage-list' },
    tools.map((t) =>
      el('span', { className: 'tool-usage-chip' }, [`${t.toolName} × ${t.count}`])
    )
  );
}

function renderSessionDetailCard(session: SessionDetail): HTMLElement {
  return el('div', { className: 'overview-card session-detail-card' }, [
    el('div', { className: 'overview-label-row' }, [
      el('div', { className: 'overview-label' }, [session.title]),
      el('span', { className: 'bar-row-value' }, [`${formatTokenCount(session.totalTokens)} · ${formatUsd(session.costUsd)}`]),
    ]),
    renderAgentUsageTable(session.agents),
    renderToolUsageList(session.tools),
  ]);
}

function renderDailyDetailPanel(date: string): HTMLElement {
  if (state.dailyDetail.loading && !state.dailyDetail.report) {
    return el('div', { className: 'daily-detail-panel state-block-desc' }, ['불러오는 중…']);
  }
  if (state.dailyDetail.error) {
    return el('div', { className: 'daily-detail-panel alert' }, [
      el('span', {}, [`⚠ ${state.dailyDetail.error}`]),
      el('button', { className: 'btn', onClick: () => void loadDailyDetail(date) }, ['다시 시도']),
    ]);
  }
  const report = state.dailyDetail.report;
  if (!report || report.sessions.length === 0) {
    return el('div', { className: 'daily-detail-panel state-block-desc' }, ['이 날짜에 세션 기록이 없습니다.']);
  }
  return el(
    'div',
    { className: 'daily-detail-panel' },
    report.sessions.map(renderSessionDetailCard)
  );
}

function renderDailyUsageSection(): HTMLElement {
  const labelRow = el('div', { className: 'overview-label-row' }, [
    el('div', { className: 'overview-label' }, ['일별 사용량 (최근 30일, 실제 로컬 데이터)']),
  ]);

  let body: HTMLElement;
  if (state.dailyUsage.loading && !state.dailyUsage.loaded) {
    body = el('div', { className: 'state-block-desc' }, ['불러오는 중…']);
  } else if (state.dailyUsage.error) {
    body = el('div', { className: 'alert' }, [
      el('span', {}, [`⚠ ${state.dailyUsage.error}`]),
      el('button', { className: 'btn', onClick: () => void loadDailyUsage() }, ['다시 시도']),
    ]);
  } else {
    const days = [...state.dailyUsage.items].sort((a, b) => b.date.localeCompare(a.date));
    if (days.length === 0) {
      body = el('div', { className: 'state-block-desc' }, ['최근 30일 이내 사용 기록이 없습니다.']);
    } else {
      const maxTotal = Math.max(...days.map(dailyUsageTotal));
      const rows: HTMLElement[] = [];
      for (const d of days) {
        const total = dailyUsageTotal(d);
        const pct = maxTotal > 0 ? Math.round((total / maxTotal) * 100) : 0;
        const selected = state.dailyDetail.selectedDate === d.date;
        rows.push(
          el(
            'div',
            { className: `bar-row bar-row-clickable${selected ? ' selected' : ''}`, onClick: () => toggleDailyDetail(d.date) },
            [
              el('div', { className: 'bar-row-label' }, [d.date]),
              el('div', { className: 'bar-track' }, [barFillEl(pct)]),
              el('div', { className: 'bar-row-value' }, [`${total.toLocaleString('ko-KR')} 토큰`]),
            ]
          )
        );
        if (selected) rows.push(renderDailyDetailPanel(d.date));
      }
      body = el('div', { className: 'bar-list' }, rows);
    }
  }

  return el('div', { className: 'overview-card' }, [labelRow, body]);
}

function renderDailyTab(): HTMLElement {
  const windowsGrid = el('div', { className: 'usage-window-grid' }, [
    renderUsageWindowCard(MOCK_USAGE.windows.fiveHour, '5시간 주기 사용량'),
    renderUsageWindowCard(MOCK_USAGE.windows.weekly, '주간 사용량'),
  ]);
  const statCards = el('div', { className: 'stat-grid' }, MOCK_USAGE.summary.map(statCardEl));
  return el('div', {}, [windowsGrid, statCards, renderDailyUsageSection()]);
}

// ---------------- 화면 진입점 ----------------

export function renderUsageView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['사용량 통계']),
      el('div', { className: 'page-subtitle' }, [
        '일별 사용량은 실제 로컬 데이터입니다. 날짜를 클릭하면 그날의 세션·에이전트·툴 상세를 볼 수 있습니다 (5시간/주간 패널은 샘플)',
      ]),
    ]),
  ]);

  return el('div', {}, [header, renderDailyTab()]);
}
