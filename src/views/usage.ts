// 사용량 통계 — "일별 사용량"은 실제 ~/.claude/projects/**/*.jsonl 집계(최근
// 30일)이고, 날짜 막대를 클릭하면 그 날짜 하루만 세션/에이전트/툴 단위로
// 재집계한 상세를 그 자리에 펼친다(dailyDetailApi.ts 참고). 카드 통계(최근 30일
// 총 토큰/활동일수/일평균 토큰/캐시 히트율)도 이 데이터만으로 계산한 실제 값이다
// — 비교할 이전 기간 데이터가 없어 전월 대비 등 증감 배지는 두지 않는다. 실시간
// 감시는 하지 않는다 — "사용량 통계" 메뉴를 클릭할 때마다 새로 불러온다(sidebar.ts).
import { el } from '../dom';
import { state, notifyChange } from '../state';
import { fetchDailyUsage } from '../usageApi';
import type { DailyUsage } from '../usageApi';
import { fetchDailyDetail } from '../dailyDetailApi';
import type { AgentUsage, SessionDetail, ToolUsage } from '../dailyDetailApi';

function barFillEl(pct: number): HTMLElement {
  const fill = el('div', { className: 'bar-fill' }, []);
  fill.style.width = `${Math.min(100, Math.max(0, pct))}%`;
  return fill;
}

interface UsageStat {
  readonly label: string;
  readonly value: string;
}

function statCardEl(s: UsageStat): HTMLElement {
  return el('div', { className: 'stat-card' }, [
    el('div', { className: 'stat-card-label' }, [s.label]),
    el('div', { className: 'stat-card-value' }, [s.value]),
  ]);
}

export function formatUsd(v: number): string {
  return `$${v.toFixed(2)}`;
}

export function formatTokenCount(v: number): string {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(2)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  return String(v);
}

// 최근 30일 items만으로 정직하게 계산 가능한 지표만 다룬다 — API 호출 수·활성
// 프로젝트 수처럼 이 데이터로 계산 불가능한 지표는 만들어내지 않는다.
export interface UsageTotals {
  readonly totalTokens: number;
  readonly activeDays: number;
  readonly avgPerDay: number;
  readonly cacheHitRate: number | null; // 캐시 생성/읽기 토큰이 전혀 없으면 null
}

export function computeUsageTotals(items: readonly DailyUsage[]): UsageTotals {
  if (items.length === 0) return { totalTokens: 0, activeDays: 0, avgPerDay: 0, cacheHitRate: null };
  let totalTokens = 0;
  let cacheRead = 0;
  let cacheCreate = 0;
  for (const d of items) {
    totalTokens += dailyUsageTotal(d);
    cacheRead += d.cacheReadTokens;
    cacheCreate += d.cacheCreationTokens;
  }
  const activeDays = items.length;
  const avgPerDay = Math.round(totalTokens / activeDays);
  const cacheDenom = cacheRead + cacheCreate;
  const cacheHitRate = cacheDenom > 0 ? Math.round((cacheRead / cacheDenom) * 100) : null;
  return { totalTokens, activeDays, avgPerDay, cacheHitRate };
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

function renderUsageStatCards(): HTMLElement {
  if (state.dailyUsage.loading && !state.dailyUsage.loaded) {
    return el('div', { className: 'state-block-desc' }, ['불러오는 중…']);
  }
  if (state.dailyUsage.error || state.dailyUsage.items.length === 0) {
    return el('div', {}, []);
  }
  const t = computeUsageTotals(state.dailyUsage.items);
  const stats: UsageStat[] = [
    { label: '최근 30일 총 토큰', value: formatTokenCount(t.totalTokens) },
    { label: '최근 30일 활동일수', value: `${t.activeDays}일` },
    { label: '일평균 토큰', value: formatTokenCount(t.avgPerDay) },
    { label: '캐시 히트율', value: t.cacheHitRate !== null ? `${t.cacheHitRate}%` : '—' },
  ];
  return el('div', { className: 'stat-grid' }, stats.map(statCardEl));
}

function renderDailyTab(): HTMLElement {
  return el('div', {}, [renderUsageStatCards(), renderDailyUsageSection()]);
}

// ---------------- 화면 진입점 ----------------

export function renderUsageView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['사용량 통계']),
      el('div', { className: 'page-subtitle' }, [
        '일별 사용량과 카드 통계는 모두 실제 로컬 데이터입니다. 날짜를 클릭하면 그날의 세션·에이전트·툴 상세를 볼 수 있습니다',
      ]),
    ]),
  ]);

  return el('div', {}, [header, renderDailyTab()]);
}
