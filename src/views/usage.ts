// 사용량 통계 — 사이드바 서브 라우트 3개(daily/projects/models, route.ts
// UsageTab)를 공유하는 화면이다.
//   - 토큰 사용량(daily, 기본): 실제 ~/.claude/projects/**/*.jsonl 집계(최근
//     30일)이고, 날짜 막대를 클릭하면 그 날짜 하루만 세션/에이전트/툴 단위로
//     재집계한 상세를 그 자리에 펼친다(dailyDetailApi.ts 참고). 카드 통계(총
//     토큰/활동일수/일평균/캐시 히트율/최고 사용일/캐시 절대량/입출력 비율)는
//     usageApi.ts의 DailyUsage[]만으로 계산한 실제 값이다 — 비교할 이전 기간
//     데이터가 없어 전월 대비 등 증감 배지는 두지 않는다.
//   - 프로젝트별(projects): 30일 요약(usageSummaryApi.ts)의 Top5 랭킹 + 나머지
//     합. 프로젝트를 클릭하면 그 프로젝트만의 30일 일별 추이를 펼친다.
//   - 모델·도구(models): 30일 요약의 모델별 비중·비용 + 자주 쓴 툴 Top5.
// 30일 요약(get_usage_summary)은 jsonl 전체를 1회 스캔해 수 초 걸릴 수 있다 —
// main.ts handleNavigation()이 "사용량" 탭에 새로 진입할 때만 불러오고,
// daily/projects/models 서브뷰 사이를 옮겨 다닐 때는 state.usageSummary 캐시를
// 그대로 쓴다(재스캔하지 않는다). 실시간 감시는 하지 않는다 — "사용량" 탭에
// 새로 진입할 때마다 다시 불러온다(sidebar.ts).
import { el, clickable } from '../dom';
import { state, notifyChange } from '../state';
import type { UsageTab } from '../state';
import { fetchDailyUsage } from '../usageApi';
import type { DailyUsage } from '../usageApi';
import { fetchDailyDetail } from '../dailyDetailApi';
import type { AgentUsage, SessionDetail, ToolUsage } from '../dailyDetailApi';
import { fetchUsageSummary, fetchProjectDailyTrend } from '../usageSummaryApi';
import type { ModelUsageSummary, ProjectUsageSummary, UsageSummaryReport } from '../usageSummaryApi';

function barFillEl(pct: number): HTMLElement {
  const fill = el('div', { className: 'bar-fill' }, []);
  fill.style.width = `${Math.min(100, Math.max(0, pct))}%`;
  return fill;
}

interface UsageStat {
  readonly label: string;
  readonly value: string;
  // M3-r 수정(리뷰 2026-09-24 2차) — 값에 한글 단위("일" 등)가 붙는 경우
  // 여기 넣는다. .stat-value 전체는 JetBrains Mono(font-numeric)인데 한글
  // 단위까지 그 안에서 그대로 렌더되면 모노 폴백으로 나간다(확정 방침 위반).
  // 숫자만 .stat-value에 두고 단위는 별도 <small>로 분리해 font-body를 준다.
  // unit은 짧은 한 단어(예: "일")에만 쓴다 — 좁은 카드에서 값과 같은 줄에
  // 붙이려다 중간에 줄바꿈되면 값과 단위가 어긋나 보인다(예: 최고 사용일
  // 카드의 " · 69.4K"). 그런 보조 수치는 sub로 별도 줄에 둔다.
  readonly unit?: string;
  readonly sub?: string;
}

// design-system.md §3.3 — home.ts의 통계 타일(.stat-row/.stat)과 동일한
// 캐노니컬 클래스로 이관한다(구 .stat-grid/.stat-card 폐기, JetBrains
// Mono+tabular-nums가 이미 .stat-value CSS에 붙어 있다).
function statTile(s: UsageStat): HTMLElement {
  return el('div', { className: 'stat' }, [
    el('div', { className: 'stat-label' }, [s.label]),
    el('div', { className: 'stat-value' }, [s.value, ...(s.unit ? [el('small', {}, [s.unit])] : [])]),
    ...(s.sub ? [el('div', { className: 'stat-sub' }, [s.sub])] : []),
  ]);
}

// design-system.md §3.1 — 박스 공용 헤더(home.ts boxHead()와 동일 패턴, 이
// 화면은 우측 부가 콘텐츠가 없어 제목만 받는 단순 버전).
function boxHead(title: string): HTMLElement {
  return el('div', { className: 'box-head' }, [el('span', { className: 'box-title' }, [title])]);
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
  readonly cacheCreationTokens: number;
  readonly cacheReadTokens: number;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly peakDay: { readonly date: string; readonly total: number } | null;
  readonly inputOutputRatio: { readonly inputPct: number; readonly outputPct: number } | null;
}

export function computeUsageTotals(items: readonly DailyUsage[]): UsageTotals {
  if (items.length === 0) {
    return {
      totalTokens: 0,
      activeDays: 0,
      avgPerDay: 0,
      cacheHitRate: null,
      cacheCreationTokens: 0,
      cacheReadTokens: 0,
      inputTokens: 0,
      outputTokens: 0,
      peakDay: null,
      inputOutputRatio: null,
    };
  }
  let totalTokens = 0;
  let cacheRead = 0;
  let cacheCreate = 0;
  let inputTokens = 0;
  let outputTokens = 0;
  let peakDay: { date: string; total: number } | null = null;
  for (const d of items) {
    const total = dailyUsageTotal(d);
    totalTokens += total;
    cacheRead += d.cacheReadTokens;
    cacheCreate += d.cacheCreationTokens;
    inputTokens += d.inputTokens;
    outputTokens += d.outputTokens;
    if (!peakDay || total > peakDay.total) peakDay = { date: d.date, total };
  }
  const activeDays = items.length;
  const avgPerDay = Math.round(totalTokens / activeDays);
  const cacheDenom = cacheRead + cacheCreate;
  const cacheHitRate = cacheDenom > 0 ? Math.round((cacheRead / cacheDenom) * 100) : null;
  const ioDenom = inputTokens + outputTokens;
  const inputOutputRatio =
    ioDenom > 0 ? { inputPct: Math.round((inputTokens / ioDenom) * 100), outputPct: Math.round((outputTokens / ioDenom) * 100) } : null;
  return {
    totalTokens,
    activeDays,
    avgPerDay,
    cacheHitRate,
    cacheCreationTokens: cacheCreate,
    cacheReadTokens: cacheRead,
    inputTokens,
    outputTokens,
    peakDay,
    inputOutputRatio,
  };
}

function formatMonthDay(dateKey: string): string {
  const parts = dateKey.split('-');
  return parts.length === 3 ? `${parts[1]}/${parts[2]}` : dateKey;
}

// 총계에 unpricedTokens(단가 미등록 토큰)가 있을 때만 그리는 정직성 안내 —
// daily/projects/models 세 서브뷰가 공통으로 쓴다. 바로 위 목록의 마지막
// 행(예: "Top 5 외 나머지")에 달린 부속 설명처럼 보이지 않도록 구분선(.
// unpriced-note)과 "※"로 이 화면 합계 전체에 대한 주석임을 드러낸다.
function unpricedNote(unpricedTokens: number): HTMLElement {
  return el('div', { className: 'unpriced-note' }, [`※ 이 화면 비용 합계 기준 — ${formatTokenCount(unpricedTokens)} 토큰은 단가 미등록으로 비용 미산정`]);
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
    state.dailyUsage.error = err instanceof Error ? err.message : '사용량 데이터를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.dailyUsage.loading = false;
    notifyChange();
  }
}

// ---------------- 30일 요약 (프로젝트별/모델·도구 서브뷰) ----------------

export async function loadUsageSummary(): Promise<void> {
  state.usageSummary.loading = true;
  state.usageSummary.error = null;
  notifyChange();
  try {
    state.usageSummary.report = await fetchUsageSummary();
    state.usageSummary.loaded = true;
  } catch (err) {
    state.usageSummary.error = err instanceof Error ? err.message : '30일 요약 데이터를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.usageSummary.loading = false;
    notifyChange();
  }
}

// ---------------- 프로젝트별 30일 추이 (Top5 행 클릭 시에만) ----------------

async function loadProjectTrend(projectKey: string): Promise<void> {
  state.usageProjectTrend.loading = true;
  state.usageProjectTrend.error = null;
  notifyChange();
  try {
    const items = await fetchProjectDailyTrend(projectKey);
    // 응답이 오는 사이 사용자가 다른 프로젝트를 클릭했으면(=선택이 바뀌었으면)
    // 이 늦게 도착한 응답은 버린다.
    if (state.usageProjectTrend.projectKey !== projectKey) return;
    state.usageProjectTrend.items = items;
  } catch (err) {
    if (state.usageProjectTrend.projectKey !== projectKey) return;
    state.usageProjectTrend.error = err instanceof Error ? err.message : '프로젝트별 추이를 불러오지 못했습니다.';
  } finally {
    if (state.usageProjectTrend.projectKey === projectKey) state.usageProjectTrend.loading = false;
    notifyChange();
  }
}

function toggleProjectTrend(projectKey: string): void {
  if (state.usageProjectTrend.projectKey === projectKey) {
    state.usageProjectTrend.projectKey = null;
    state.usageProjectTrend.items = [];
    state.usageProjectTrend.error = null;
    notifyChange();
    return;
  }
  state.usageProjectTrend.projectKey = projectKey;
  state.usageProjectTrend.items = [];
  state.usageProjectTrend.error = null;
  notifyChange();
  void loadProjectTrend(projectKey);
}

export function dailyUsageTotal(d: DailyUsage): number {
  return d.inputTokens + d.outputTokens + d.cacheCreationTokens + d.cacheReadTokens;
}

function localDateKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// 대시보드 홈 위젯 전용 — 30일 총합이 아니라 "오늘" 하루치 토큰만 뽑는다.
export function computeTodayTokens(items: readonly DailyUsage[]): number {
  const todayKey = localDateKey(new Date());
  const today = items.find((d) => d.date === todayKey);
  return today ? dailyUsageTotal(today) : 0;
}

// ---------------- 날짜별 상세 (실제 데이터) ----------------

export async function loadDailyDetail(date: string): Promise<void> {
  state.dailyDetail.loading = true;
  state.dailyDetail.error = null;
  notifyChange();
  try {
    state.dailyDetail.report = await fetchDailyDetail(date);
  } catch (err) {
    state.dailyDetail.error = err instanceof Error ? err.message : '상세 데이터를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.dailyDetail.loading = false;
    notifyChange();
  }
}

// 사용량 사이드바는 "일별 사용량" 단일 항목만 둔다(PM 결정, 2026-09-24) —
// 예전에 있던 "최근 7일/최근 30일" 기간 토글과 "최근 활동일" 날짜 퀵점프
// 목록은 폐기됐다. 이 화면은 항상 최근 30일 전체를 보여주는 main 브랜치
// 원래 동작으로 되돌아간다(사이드바에서 이 화면을 제어하지 않는다).
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
          clickable(
            el('div', { className: `bar-row bar-row-clickable${selected ? ' selected' : ''}` }, [
              // U-08: role/tabindex/keydown이 전무해 키보드로 도달조차 안 됐다.
              // 펼침 가능함을 시각적으로도 알리는 캐럿을 라벨 셀 안에 넣는다(3열
              // 고정 grid라 새 컬럼을 추가하면 레이아웃이 깨진다).
              el('div', { className: 'bar-row-label' }, [`${selected ? '▾' : '▸'} ${d.date}`]),
              el('div', { className: 'bar-track' }, [barFillEl(pct)]),
              el('div', { className: 'bar-row-value' }, [
                el('span', { className: 'bar-row-value-num' }, [total.toLocaleString('ko-KR')]),
                ' 토큰',
              ]),
            ]),
            () => toggleDailyDetail(d.date)
          )
        );
        if (selected) rows.push(renderDailyDetailPanel(d.date));
      }
      body = el('div', { className: 'bar-list' }, rows);
    }
  }

  return el('div', { className: 'box' }, [boxHead('토큰 사용량 (최근 30일)'), el('div', { className: 'box-body' }, [body])]);
}

// 최근 30일 예상 비용 타일 — usageApi.ts가 아니라 별도 로딩 사이클인
// state.usageSummary에서 값을 가져오므로(30일 요약, jsonl 1회 스캔) 다른
// 타일과 로딩/에러 상태가 독립적이다. 라벨의 "예상"이 비용이 실측이 아니라
// 단가표 기반 추정임을 드러낸다(정직성 원칙).
function costStat(): UsageStat {
  const s = state.usageSummary;
  if (s.loading && !s.loaded) return { label: '최근 30일 예상 비용', value: '계산 중…' };
  if (s.error || !s.report) return { label: '최근 30일 예상 비용', value: '—' };
  return { label: '최근 30일 예상 비용', value: formatUsd(s.report.totalCostUsd) };
}

function renderUsageStatCards(): HTMLElement {
  if (state.dailyUsage.loading && !state.dailyUsage.loaded) {
    return el('div', { className: 'state-block-desc' }, ['불러오는 중…']);
  }
  if (state.dailyUsage.error || state.dailyUsage.items.length === 0) {
    return el('div', {}, []);
  }
  const t = computeUsageTotals(state.dailyUsage.items);
  // 기존 핵심 지표 줄 — 값이 짧아(퍼센트/정수/축약 토큰량 1개) 기존 .stat-row
  // (130px 최소폭, 900px 최소 창에서도 4개가 한 줄에 들어가도록 튜닝된 값)를
  // 그대로 쓴다.
  const coreStats: UsageStat[] = [
    { label: '최근 30일 총 토큰', value: formatTokenCount(t.totalTokens) },
    { label: '최근 30일 활동일수', value: String(t.activeDays), unit: '일' },
    { label: '일평균 토큰', value: formatTokenCount(t.avgPerDay) },
    { label: '캐시 히트율', value: t.cacheHitRate !== null ? `${t.cacheHitRate}%` : '—' },
  ];
  // 세부 지표 줄 — "2.4K / 156.0K"처럼 두 수치를 슬래시로 이어붙인 값이
  // 섞여 있어 더 넓은 .stat-row-detail(210px 최소폭)을 쓴다(styles.css 참고).
  const detailStats: UsageStat[] = [
    {
      label: '최고 사용일',
      value: t.peakDay ? formatMonthDay(t.peakDay.date) : '—',
      sub: t.peakDay ? `${formatTokenCount(t.peakDay.total)} 토큰` : undefined,
    },
    { label: '캐시 생성 / 읽기(절대량)', value: `${formatTokenCount(t.cacheCreationTokens)} / ${formatTokenCount(t.cacheReadTokens)}` },
    {
      label: '입력 / 출력 비율',
      value: t.inputOutputRatio ? `${t.inputOutputRatio.inputPct}% / ${t.inputOutputRatio.outputPct}%` : '—',
    },
    costStat(),
  ];
  const summary = state.usageSummary;
  const unpriced = summary.report && summary.report.unpricedTokens > 0 ? unpricedNote(summary.report.unpricedTokens) : null;
  return el('div', { className: 'col' }, [
    el('div', { className: 'stat-row' }, coreStats.map(statTile)),
    // "stat-row stat-row-detail" — 하네스(usage.mjs)가 `.stat-row .stat` 개수
    // 8개를 카드 총수 검사로 쓰므로 stat-row 클래스는 유지하고, 더 넓은 최소
    // 폭은 뒤에 정의된 .stat-row-detail이 grid-template-columns만 덮어써
    // 적용한다(styles.css).
    el('div', { className: 'stat-row stat-row-detail' }, detailStats.map(statTile)),
    ...(unpriced ? [unpriced] : []),
  ]);
}

function renderDailyTab(): HTMLElement {
  return el('div', { className: 'col' }, [renderUsageStatCards(), renderDailyUsageSection()]);
}

// ---------------- 프로젝트별 (Top5 랭킹 + 클릭 시 30일 추이) ----------------

function projectMaxTokens(projects: readonly ProjectUsageSummary[]): number {
  return Math.max(1, ...projects.map((p) => p.tokens));
}

function renderProjectTrendPanel(): HTMLElement {
  const pt = state.usageProjectTrend;
  if (pt.loading && pt.items.length === 0) {
    return el('div', { className: 'daily-detail-panel state-block-desc' }, ['불러오는 중…']);
  }
  if (pt.error) {
    return el('div', { className: 'daily-detail-panel alert' }, [
      el('span', {}, [`⚠ ${pt.error}`]),
      el('button', { className: 'btn', onClick: () => { if (pt.projectKey) void loadProjectTrend(pt.projectKey); } }, ['다시 시도']),
    ]);
  }
  if (pt.items.length === 0) {
    return el('div', { className: 'daily-detail-panel state-block-desc' }, ['이 프로젝트의 최근 30일 사용 기록이 없습니다.']);
  }
  const days = [...pt.items].sort((a, b) => b.date.localeCompare(a.date));
  const maxTotal = Math.max(1, ...days.map(dailyUsageTotal));
  const rows = days.map((d) => {
    const total = dailyUsageTotal(d);
    const pct = maxTotal > 0 ? Math.round((total / maxTotal) * 100) : 0;
    return el('div', { className: 'bar-row' }, [
      el('div', { className: 'bar-row-label' }, [d.date]),
      el('div', { className: 'bar-track' }, [barFillEl(pct)]),
      el('div', { className: 'bar-row-value' }, [el('span', { className: 'bar-row-value-num' }, [total.toLocaleString('ko-KR')]), ' 토큰']),
    ]);
  });
  return el('div', { className: 'daily-detail-panel' }, [el('div', { className: 'bar-list' }, rows)]);
}

function renderProjectRankingRow(p: ProjectUsageSummary, maxTokens: number): HTMLElement {
  const pct = maxTokens > 0 ? Math.round((p.tokens / maxTokens) * 100) : 0;
  const selected = state.usageProjectTrend.projectKey === p.projectKey;
  return clickable(
    el('div', { className: `bar-row bar-row-clickable${selected ? ' selected' : ''}` }, [
      el('div', { className: 'bar-row-label' }, [`${selected ? '▾' : '▸'} ${p.displayName}`]),
      el('div', { className: 'bar-track' }, [barFillEl(pct)]),
      el('div', { className: 'bar-row-value' }, [
        el('span', { className: 'bar-row-value-num' }, [formatTokenCount(p.tokens)]),
        ` 토큰 · ${formatUsd(p.costUsd)}`,
      ]),
    ]),
    () => toggleProjectTrend(p.projectKey)
  );
}

function renderOtherProjectsRow(report: UsageSummaryReport, maxTokens: number): HTMLElement | null {
  if (report.otherProjectsTokens <= 0) return null;
  const pct = maxTokens > 0 ? Math.round((report.otherProjectsTokens / maxTokens) * 100) : 0;
  return el('div', { className: 'bar-row' }, [
    el('div', { className: 'bar-row-label' }, ['Top 5 외 나머지']),
    el('div', { className: 'bar-track' }, [barFillEl(pct)]),
    el('div', { className: 'bar-row-value' }, [
      el('span', { className: 'bar-row-value-num' }, [formatTokenCount(report.otherProjectsTokens)]),
      ` 토큰 · ${formatUsd(report.otherProjectsCostUsd)}`,
    ]),
  ]);
}

function renderProjectsTab(): HTMLElement {
  const s = state.usageSummary;
  if (s.loading && !s.loaded) {
    return el('div', { className: 'col' }, [
      el('div', { className: 'state-block-desc' }, ['불러오는 중… (최근 30일 전체를 훑어 집계하는 중이라 몇 초 걸릴 수 있습니다)']),
    ]);
  }
  if (s.error) {
    return el('div', { className: 'col' }, [
      el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${s.error}`]), el('button', { className: 'btn', onClick: () => void loadUsageSummary() }, ['다시 시도'])]),
    ]);
  }
  if (!s.report || s.report.projects.length === 0) {
    return el('div', { className: 'col' }, [el('div', { className: 'state-block-desc' }, ['최근 30일 이내 프로젝트별 사용 기록이 없습니다.'])]);
  }
  const maxTokens = projectMaxTokens(s.report.projects);
  const rows: HTMLElement[] = [];
  for (const p of s.report.projects) {
    rows.push(renderProjectRankingRow(p, maxTokens));
    if (state.usageProjectTrend.projectKey === p.projectKey) rows.push(renderProjectTrendPanel());
  }
  const otherRow = renderOtherProjectsRow(s.report, maxTokens);
  if (otherRow) rows.push(otherRow);
  const unpriced = s.report.unpricedTokens > 0 ? unpricedNote(s.report.unpricedTokens) : null;
  return el('div', { className: 'col' }, [
    el('div', { className: 'box' }, [
      boxHead('프로젝트별 사용량 Top 5 (최근 30일)'),
      el('div', { className: 'box-body' }, [el('div', { className: 'bar-list' }, rows), ...(unpriced ? [unpriced] : [])]),
    ]),
  ]);
}

// ---------------- 모델·도구 (모델별 비중/비용 + 자주 쓴 툴 Top5) ----------------

function renderModelUsageTable(models: readonly ModelUsageSummary[], totalTokens: number): HTMLElement {
  if (models.length === 0) return el('div', { className: 'state-block-desc' }, ['모델 사용 기록이 없습니다.']);
  const header = el('div', { className: 'thief-table-row thief-table-row-3 thief-table-head' }, [
    el('span', {}, ['모델']),
    el('span', {}, ['비중']),
    el('span', {}, ['토큰']),
    el('span', {}, ['비용']),
  ]);
  const rows = models.map((m) => {
    const pct = totalTokens > 0 ? Math.round((m.tokens / totalTokens) * 100) : 0;
    return el('div', { className: 'thief-table-row thief-table-row-3' }, [
      el('span', { className: 'thief-table-title' }, [m.displayName]),
      el('span', {}, [`${pct}%`]),
      el('span', {}, [formatTokenCount(m.tokens)]),
      // pricingMatched=false면 단가표에 없는 모델이다 — sonnet 단가로 조용히
      // 대체하지 않고 "미산정"으로 정직하게 표시한다(usageSummaryApi.ts 계약).
      el('span', {}, [m.pricingMatched ? formatUsd(m.costUsd) : '미산정']),
    ]);
  });
  return el('div', { className: 'thief-table' }, [header, ...rows]);
}

function renderModelsTab(): HTMLElement {
  const s = state.usageSummary;
  if (s.loading && !s.loaded) {
    return el('div', { className: 'col' }, [
      el('div', { className: 'state-block-desc' }, ['불러오는 중… (최근 30일 전체를 훑어 집계하는 중이라 몇 초 걸릴 수 있습니다)']),
    ]);
  }
  if (s.error) {
    return el('div', { className: 'col' }, [
      el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${s.error}`]), el('button', { className: 'btn', onClick: () => void loadUsageSummary() }, ['다시 시도'])]),
    ]);
  }
  if (!s.report) {
    return el('div', { className: 'col' }, [el('div', { className: 'state-block-desc' }, ['최근 30일 이내 사용 기록이 없습니다.'])]);
  }
  const unpriced = s.report.unpricedTokens > 0 ? unpricedNote(s.report.unpricedTokens) : null;
  return el('div', { className: 'col' }, [
    el('div', { className: 'box' }, [
      boxHead('모델별 사용 비중 (최근 30일)'),
      el('div', { className: 'box-body' }, [renderModelUsageTable(s.report.models, s.report.totalTokens), ...(unpriced ? [unpriced] : [])]),
    ]),
    el('div', { className: 'box' }, [boxHead('자주 쓴 툴 Top 5 (최근 30일)'), el('div', { className: 'box-body' }, [renderToolUsageList(s.report.tools)])]),
  ]);
}

// ---------------- 화면 진입점 ----------------

const SUBTITLES: Record<UsageTab, string> = {
  daily: '날짜를 클릭하면 그날의 세션·에이전트·툴 상세를 볼 수 있습니다',
  projects: '프로젝트를 클릭하면 그 프로젝트의 최근 30일 일별 추이를 볼 수 있습니다',
  models: '모델별 사용 비중·비용과 자주 쓴 툴을 확인할 수 있습니다',
};

export function renderUsageView(tab: UsageTab): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['사용량 통계']), el('div', { className: 'page-subtitle' }, [SUBTITLES[tab]])]),
  ]);

  const body = tab === 'daily' ? renderDailyTab() : tab === 'projects' ? renderProjectsTab() : renderModelsTab();
  return el('div', {}, [header, body]);
}
