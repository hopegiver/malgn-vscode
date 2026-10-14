// 흐름 — 사용량 화면 "사용 추이"(WBS P3-1 주간 요약 / P3-2 이상치) — src/usageTrend.ts(순수 계산) +
// src/views/usage.ts renderUsageTrend(daily 탭).
//
// 검증 범위: TS/DOM/CSS 층만(하네스 공통 한계). 합성 픽스처(실사용 로그 아님)로 정상/급증·급락/
// 데이터 부족/그 전 7일 비교 불가/그 전 7일 0 상태를 만들고, 화면에 나온 숫자를 이 파일이 독립적으로
// 다시 계산한 기대값과 대조한다. 순수 함수 경계값은 lib/trendBoundary.mjs를 Vite가 서빙하는
// 실제 모듈에 대해 실행한다(boundary 시나리오).
// 비용·세션 수·도구 호출 지표(추정 비용 ≥ 표기, 미산정 안내, 차트 지표 전환, 일별 목록 열)도 같은
// 방식으로 독립 계산 기대값과 대조한다.
import { usageSummarySample } from '../lib/fixtures.mjs';
import { runTrendBoundaryChecks } from '../lib/trendBoundary.mjs';

export const flowId = 'usageTrend';
export const startHash = '#/usage';

// 앱(views/usage.ts localDateKey)과 같은 로컬 'YYYY-MM-DD'.
function localKey(offset) {
  const d = new Date(Date.now() - offset * 86400_000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// 앱의 formatTokenCount 사본(화면 문자열 대조용).
function fmt(v) {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(2)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  return String(v);
}

// 합성 한 줄: total 토큰을 input 10% / creation 5% / output 5% / read(나머지, readShare)로 나눈다.
// 비용·도구 호출·세션 수는 기본값(토큰 비례)을 두고 extra로 덮어쓴다. 비용은 센트 단위(합산 오차 방지).
function rec(offset, total, readShare, extra = {}) {
  const read = Math.round(total * readShare);
  const input = Math.round(total * 0.1);
  const creation = Math.round(total * 0.05);
  const output = total - read - input - creation;
  return {
    date: localKey(offset),
    inputTokens: input,
    outputTokens: output,
    cacheCreationTokens: creation,
    cacheReadTokens: read,
    costUsd: Math.round(total * 0.00001 * 100) / 100,
    unpricedTokens: 0,
    toolCalls: Math.round(total / 6000),
    sessionCount: 2,
    ...extra,
  };
}
const tot = (r) => r.inputTokens + r.outputTokens + r.cacheCreationTokens + r.cacheReadTokens;

// offset 0..29, 6/7번째 날(i%7 in {5,6})은 기록 없음(주말 모사). 최근 완료 7일(어제=1 ~ 7)은 x1.25, 캐시 read 80% / 그 이전 70%.
// 오늘(offset 0)은 주간 비교에 들어가지 않는 부분일이다.
// 새 지표는 토큰과 다른 방향/크기로 움직이게 한다: 비용 x1.1 단가(토큰 +25%인데 비용은 더 크게), 도구 호출은
// 토큰 대비 x0.8(토큰은 늘었는데 호출은 덜 늘거나 줄 수 있음), 세션 수는 최근 3 / 이전 2.
function steadyItems() {
  const out = [];
  for (let i = 0; i < 30; i++) {
    if (i % 7 === 5 || i % 7 === 6) continue;
    const base = 200_000 + ((i * 37) % 11) * 20_000;
    const recent = i >= 1 && i <= 7;
    const total = recent ? Math.round(base * 1.25) : base;
    out.push(
      rec(i, total, recent ? 0.8 : 0.7, {
        costUsd: Math.round(total * 0.00001 * (recent ? 1.1 : 1) * 100) / 100,
        toolCalls: Math.round((total / 6000) * (recent ? 0.8 : 1)),
        sessionCount: recent ? 3 : 2,
      })
    );
  }
  return out;
}

// steadyItems에서 offsets 날짜에 단가 미산정 토큰을 얹는다(비용은 그대로 — 미산정분은 비용에서 빠져 있다).
function withUnpriced(offsets, unpriced = 120_000) {
  const dates = new Set(offsets.map(localKey));
  return steadyItems().map((r) => (dates.has(r.date) ? { ...r, unpricedTokens: unpriced } : r));
}

function withSpikeAndDrop() {
  const items = steadyItems().filter((r) => r.date !== localKey(3) && r.date !== localKey(10));
  items.push(rec(3, 1_600_000, 0.75)); // 급증 후보
  items.push(rec(10, 60_000, 0.7)); // 급락 후보
  return items;
}

function expectedWeekly(items) {
  const sum = (from, to) => items.filter((r) => { for (let o = from; o <= to; o++) if (r.date === localKey(o)) return true; return false; });
  const cur = sum(1, 7); // 어제까지 끝난 7일(오늘 제외)
  const prev = sum(8, 14);
  const t = (rs) => rs.reduce((a, r) => a + tot(r), 0);
  const f = (rs, k) => rs.reduce((a, r) => a + r[k], 0);
  const hit = (rs) => {
    const i = rs.reduce((a, r) => a + r.inputTokens, 0);
    const c = rs.reduce((a, r) => a + r.cacheCreationTokens, 0);
    const rd = rs.reduce((a, r) => a + r.cacheReadTokens, 0);
    return i + c + rd > 0 ? Math.round((rd / (i + c + rd)) * 100) : null;
  };
  const todayRows = sum(0, 0);
  return {
    todayTotal: t(todayRows),
    curTotal: t(cur),
    prevTotal: t(prev),
    curActive: cur.length,
    prevActive: prev.length,
    curHit: hit(cur),
    prevHit: hit(prev),
    curCost: f(cur, 'costUsd'),
    prevCost: f(prev, 'costUsd'),
    curTools: f(cur, 'toolCalls'),
    prevTools: f(prev, 'toolCalls'),
    curSessions: f(cur, 'sessionCount'),
    prevSessions: f(prev, 'sessionCount'),
    curUnpriced: f(cur, 'unpricedTokens'),
    prevUnpriced: f(prev, 'unpricedTokens'),
  };
}

const usd = (v) => `$${v.toFixed(2)}`;
const cnt = (v) => v.toLocaleString('ko-KR');
const sgn = (v, fmtFn) => (v > 0 ? `+${fmtFn(v)}` : v < 0 ? `-${fmtFn(-v)}` : fmtFn(0));
const pct1 = (abs, base) => (base > 0 ? `${abs > 0 ? '+' : ''}${((abs / base) * 100).toFixed(1)}%` : null);

async function tallShot(page, shot, name) {
  await page.setViewportSize({ width: 1280, height: 1320 });
  await page.waitForTimeout(200);
  await shot(name);
}

async function text(page, selector) {
  return page.locator(selector).first().textContent().catch(() => null);
}

// 주간 표의 새 지표 행(추정 비용·세션 수·도구 호출)을 독립 계산 기대값과 대조한다.
async function checkWeeklyNewMetrics(page, t, e, bugs, { partial }) {
  const rows = await page.locator('.trend-week-row').evaluateAll((els) => els.map((r) => r.innerText.replace(/\n/g, ' | ')));
  const row = (label) => rows.find((r) => r.startsWith(label)) ?? '';
  const costRow = row('추정 비용');
  const sessRow = row('세션 수(일별 합)');
  const toolRow = row('도구 호출');
  if (!costRow || !sessRow || !toolRow) {
    bugs.push({ severity: 'Critical', symptom: `주간 표에 추정 비용/세션 수/도구 호출 행이 없음: ${JSON.stringify(rows.map((r) => r.slice(0, 12)))}`, file: 'src/views/usage.ts:renderWeeklySummary' });
    return;
  }
  // 세션 수는 날짜별 고유 세션 수의 합이라 표 라벨 자체가 "(일별 합)"임을 드러낸다 — 각주에만 의존하지 않는다.
  if (!rows.some((r) => r.startsWith('세션 수(일별 합) |'))) {
    bugs.push({ severity: 'Major', symptom: `주간 표 세션 행 라벨이 "세션 수(일별 합)"가 아님: ${JSON.stringify(rows.map((r) => r.slice(0, 12)))}`, file: 'src/views/usage.ts:renderWeeklySummary' });
  }
  const mark = partial ? '≥' : '';
  const costAbs = Math.round((e.curCost - e.prevCost) * 100) / 100;
  const want = [
    [costRow, `${mark}${usd(e.curCost)}`, '추정 비용 현재값'],
    [costRow, `${mark}${usd(e.prevCost)}`, '추정 비용 직전값'],
    [costRow, sgn(costAbs, usd), '추정 비용 절대 증감'],
    [costRow, pct1(e.curCost - e.prevCost, e.prevCost), '추정 비용 증감%'],
    [sessRow, `${cnt(e.curSessions)}개`, '세션 수 현재값'],
    [sessRow, `${cnt(e.prevSessions)}개`, '세션 수 직전값'],
    [sessRow, sgn(e.curSessions - e.prevSessions, (v) => `${cnt(v)}개`), '세션 수 절대 증감'],
    [sessRow, pct1(e.curSessions - e.prevSessions, e.prevSessions), '세션 수 증감%'],
    [toolRow, `${cnt(e.curTools)}회`, '도구 호출 현재값'],
    [toolRow, `${cnt(e.prevTools)}회`, '도구 호출 직전값'],
    [toolRow, sgn(e.curTools - e.prevTools, (v) => `${cnt(v)}회`), '도구 호출 절대 증감'],
    [toolRow, pct1(e.curTools - e.prevTools, e.prevTools), '도구 호출 증감%'],
  ];
  for (const [r, w, what] of want) {
    if (w !== null && !r.includes(w)) bugs.push({ severity: 'Major', symptom: `${what} "${w}"이(가) 없음: ${r}`, file: 'src/views/usage.ts:renderWeeklySummary' });
  }
  if (!costRow.includes('추정')) bugs.push({ severity: 'Major', symptom: '비용 행 라벨에 "추정" 표기가 없음', file: 'src/views/usage.ts' });
}

// 차트 지표 전환 — 네 버튼, 눌러진 지표가 aria-pressed/data 속성/막대 높이에 반영되고 토큰만 이상치 막대를 가진다.
async function checkMetricSwitch(page, items, bugs, { partialDays, onCost }) {
  const keys = await page.locator('[data-trend-metric]').evaluateAll((els) => els.map((b) => b.getAttribute('data-trend-metric')));
  if (keys.join(',') !== 'tokens,cost,sessions,toolCalls') bugs.push({ severity: 'Major', symptom: `지표 전환 버튼이 tokens,cost,sessions,toolCalls가 아님: ${keys}`, file: 'src/views/usage.ts:renderMetricSwitch' });
  if ((await page.locator('.trend-chart').getAttribute('data-trend-chart-metric')) !== 'tokens') bugs.push({ severity: 'Major', symptom: '처음 차트 지표가 tokens가 아님', file: 'src/views/usage.ts' });
  for (const key of ['cost', 'sessions', 'toolCalls']) {
    await page.locator(`[data-trend-metric="${key}"]`).click();
    await page.waitForTimeout(150);
    const cur = await page.locator('.trend-chart').getAttribute('data-trend-chart-metric');
    const pressed = await page.locator(`[data-trend-metric="${key}"]`).getAttribute('aria-pressed');
    if (cur !== key || pressed !== 'true') bugs.push({ severity: 'Major', symptom: `${key} 버튼 클릭 후 차트 지표=${cur}, aria-pressed=${pressed}`, file: 'src/views/usage.ts:renderMetricSwitch' });
    const bars = await page.locator('.trend-bar').count();
    if (bars !== 30) bugs.push({ severity: 'Major', symptom: `${key} 지표에서 막대가 30개가 아님(${bars})`, file: 'src/views/usage.ts' });
    // 막대 높이가 그 지표의 값에 비례하는가: 가장 큰 값의 날이 100%
    const field = { cost: 'costUsd', sessions: 'sessionCount', toolCalls: 'toolCalls' }[key];
    const heights = await page.locator('.trend-bar').evaluateAll((els) => els.map((b) => b.style.height));
    const max = Math.max(...items.map((r) => r[field]));
    const maxDate = items.find((r) => r[field] === max)?.date;
    const idx = [...Array(30).keys()].find((i) => localKey(29 - i) === maxDate);
    if (idx === undefined || heights[idx] !== '100%') bugs.push({ severity: 'Major', symptom: `${key} 지표 최대일(${maxDate}) 막대 높이가 100%가 아님(${heights[idx]})`, file: 'src/views/usage.ts:renderDailyTrendChart' });
    const axis = (await text(page, '.trend-axis')) ?? '';
    const maxFmt = { cost: usd(max), sessions: `${cnt(max)}개`, toolCalls: `${cnt(max)}회` }[key];
    if (!axis.includes(`일 최대 ${maxFmt}`) && !axis.includes(`일 최대 ≥${maxFmt}`)) bugs.push({ severity: 'Major', symptom: `${key} 지표 축 라벨에 "일 최대 ${maxFmt}"가 없음: ${axis}`, file: 'src/views/usage.ts' });
    if (!axis.includes('(오늘, 집계 중')) bugs.push({ severity: 'Minor', symptom: `축 오른쪽에 오늘 집계 중 수치가 없음: ${axis}`, file: 'src/views/usage.ts' });
    if ((await page.locator('.trend-bar-spike, .trend-bar-drop').count()) !== 0) bugs.push({ severity: 'Major', symptom: `${key} 지표인데 토큰 기준 급증/급락 막대 강조가 남아 있음`, file: 'src/views/usage.ts' });
    const chartNote = (await text(page, '.trend-chart-wrap')) ?? '';
    if (!chartNote.includes('토큰 기준 판정')) bugs.push({ severity: 'Major', symptom: `${key} 지표에서 이상치가 토큰 기준이라는 안내가 없음`, file: 'src/views/usage.ts' });
    if (key === 'cost') {
      if (onCost) await onCost();
      const pd = await page.locator('.trend-chart-wrap').getAttribute('data-trend-partial-days');
      const striped = await page.locator('.trend-bar-partial').count();
      if (pd !== String(partialDays) || striped !== partialDays) bugs.push({ severity: 'Major', symptom: `미산정이 있는 날 줄무늬 막대가 ${partialDays}개여야 하는데 attr=${pd}, 막대 ${striped}개`, file: 'src/views/usage.ts' });
      if (!chartNote.includes('추정치')) bugs.push({ severity: 'Major', symptom: '비용 지표에 추정치 안내가 없음', file: 'src/views/usage.ts' });
      if (partialDays > 0 && !chartNote.includes('실제보다 작게')) bugs.push({ severity: 'Major', symptom: '줄무늬 막대(과소 표기) 설명이 없음', file: 'src/views/usage.ts' });
      if (partialDays === 0 && chartNote.includes('줄무늬')) bugs.push({ severity: 'Minor', symptom: '미산정이 없는데 줄무늬 설명이 나옴', file: 'src/views/usage.ts' });
    } else if ((await page.locator('.trend-bar-partial').count()) !== 0) {
      bugs.push({ severity: 'Major', symptom: `${key} 지표에 비용 과소 줄무늬가 섞임`, file: 'src/views/usage.ts' });
    }
  }
  // 막대 툴팁은 어느 지표에서도 그날 네 수치를 모두 담는다.
  const title = await page.locator('.trend-bar').last().getAttribute('title');
  if (!title || !title.includes('토큰') || !title.includes('추정 비용') || !title.includes('세션') || !title.includes('도구 호출')) bugs.push({ severity: 'Major', symptom: `막대 툴팁에 네 지표가 없음: ${title}`, file: 'src/views/usage.ts' });
}

// 모든 시나리오 공통 — 화면에 NaN/Infinity/undefined, 원인 단정이 없고 기존 8타일이 유지되는가.
async function commonChecks(page, bugs, { expectBox }) {
  const boxCount = await page.locator('.trend-box').count();
  if (expectBox && boxCount !== 1) {
    bugs.push({ severity: 'Critical', symptom: `사용 추이 박스가 1개여야 하는데 ${boxCount}개`, file: 'src/views/usage.ts:renderDailyTab' });
    return '';
  }
  const t = (await text(page, '.trend-box')) ?? '';
  if (/NaN|Infinity|undefined|null/.test(t)) bugs.push({ severity: 'Critical', symptom: `사용 추이 영역에 NaN/Infinity/undefined/null 문자열 노출: ${t.slice(0, 200)}`, file: 'src/views/usage.ts' });
  if (/때문|원인은 .*입니다|덕분/.test(t.replace('원인은 판정하지 않으며', ''))) bugs.push({ severity: 'Major', symptom: '사용 추이 문구에 인과 단정 표현이 있음', file: 'src/views/usage.ts' });
  // 비용은 추정임이 드러나야 한다: 달러 표기가 있으면 "추정"이라는 말도 함께 있어야 한다.
  if (/\$/.test(t) && !t.includes('추정')) bugs.push({ severity: 'Major', symptom: '사용 추이 영역에 달러($) 표기가 있는데 "추정" 표기가 없음', file: 'src/views/usage.ts' });
  if (/\$/.test(t) && !t.includes('실제 청구액이 아닙니다')) bugs.push({ severity: 'Major', symptom: '비용이 청구액이 아닌 추정임을 밝히는 문구가 없음', file: 'src/views/usage.ts' });
  const statCount = await page.locator('.stat-row .stat').count();
  if (statCount !== 8) bugs.push({ severity: 'Major', symptom: `사용 추이 추가 후 통계 타일이 8개가 아님(${statCount})`, file: 'src/views/usage.ts' });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
  if (overflow) bugs.push({ severity: 'Major', symptom: '가로 스크롤(오버플로)이 생김', file: 'src/styles.css' });
  const cmds = await page.evaluate(() => [...new Set((window.__invokeLog ?? []).map((e) => e.cmd))]);
  const newCmd = cmds.filter((c) => !['get_daily_usage', 'get_usage_summary'].includes(c) && /usage|trend/i.test(c));
  if (newCmd.length > 0) bugs.push({ severity: 'Critical', symptom: `추이용으로 보이는 신규 커맨드 호출: ${newCmd.join(',')}`, file: 'src/views/usage.ts' });
  return t;
}

export function scenarios(base) {
  const common = { ...base, get_usage_summary: usageSummarySample() };
  return [
    {
      id: 'normal',
      fixtures: { ...common, get_daily_usage: steadyItems() },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-normal');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const e = expectedWeekly(steadyItems());
        const status = await page.locator('.trend-box').getAttribute('data-trend-weekly-status');
        if (status !== 'ok') bugs.push({ severity: 'Critical', symptom: `30일 데이터인데 주간 상태가 ok가 아님(${status})`, file: 'src/usageTrend.ts' });
        // 화면 숫자 = 독립 계산 기대값
        for (const want of [fmt(e.curTotal), fmt(e.prevTotal), `${e.curActive}일`, `${e.prevActive}일`, `${e.curHit}%`, `${e.prevHit}%`]) {
          if (!t.includes(want)) bugs.push({ severity: 'Major', symptom: `주간 요약에 기대값 "${want}"이(가) 없음`, file: 'src/views/usage.ts:renderWeeklySummary' });
        }
        // 오늘 부분일은 표에 들어가지 않고 따로 '집계 중'으로 표기된다.
        const todayAttr = await page.locator('[data-trend-today]').getAttribute('data-trend-today');
        if (todayAttr !== String(e.todayTotal) || !t.includes('집계 중') || !t.includes(`${fmt(e.todayTotal)} 토큰`)) bugs.push({ severity: 'Major', symptom: `오늘 '집계 중' 표기(${fmt(e.todayTotal)}) 없음(attr=${todayAttr})`, file: 'src/views/usage.ts:renderWeeklySummary' });
        if (!t.includes('최근 완료 7일') || !t.includes('그 전 7일')) bugs.push({ severity: 'Major', symptom: '주간 표 헤더가 "최근 완료 7일 / 그 전 7일"이 아님', file: 'src/views/usage.ts:renderWeeklySummary' });
        if (e.todayTotal === 0) bugs.push({ severity: 'Minor', symptom: '픽스처 오류: 오늘 사용이 0이면 오늘 격리 검증이 의미 없음', file: 'scripts/ui-harness/flows/usageTrend.mjs' });
        const absDelta = e.curTotal - e.prevTotal;
        const pct = ((absDelta / e.prevTotal) * 100).toFixed(1);
        if (!t.includes(`+${fmt(absDelta)}`) || !t.includes(`+${pct}%`)) bugs.push({ severity: 'Major', symptom: `전주 대비 절대(+${fmt(absDelta)})/%(+${pct}%) 표기 없음`, file: 'src/views/usage.ts' });
        if (!t.includes(`+${e.curHit - e.prevHit}%p`)) bugs.push({ severity: 'Major', symptom: `캐시 히트율 증감(+${e.curHit - e.prevHit}%p) 표기 없음`, file: 'src/views/usage.ts' });
        // 이 화면의 기존 캐시 히트율 타일과 같은 정의인가(30일 전체 값과 일치 확인은 별도 — 여기선 정의 대조): 7일 값이 30일 값과 다른 채로 같은 식인지 기대값 일치로 갈음.
        const bars = await page.locator('.trend-bar').count();
        if (bars !== 30) bugs.push({ severity: 'Major', symptom: `일별 막대가 30개여야 하는데 ${bars}개(0채움으로 날짜 연속이어야 함)`, file: 'src/views/usage.ts:renderDailyTrendChart' });
        const zeroBars = await page.locator('.trend-bar-zero').count();
        const zeroExpected = 30 - steadyItems().length;
        if (zeroBars !== zeroExpected) bugs.push({ severity: 'Major', symptom: `기록 없는 날 막대(0채움)가 ${zeroExpected}개여야 하는데 ${zeroBars}개`, file: 'src/views/usage.ts' });
        const state = await page.locator('[data-trend-anomalies]').getAttribute('data-trend-anomalies');
        const flagged = await page.locator('.trend-bar-spike, .trend-bar-drop').count();
        if (state !== 'none' || flagged !== 0) bugs.push({ severity: 'Major', symptom: `이상치 없는 안정 픽스처인데 이상치 상태=${state}, 강조 막대 ${flagged}개`, file: 'src/usageTrend.ts' });
        if (!t.includes('모두 기준 이내')) bugs.push({ severity: 'Minor', symptom: '이상치 없음 안내("판정한 N일 모두 기준 이내")가 없음', file: 'src/views/usage.ts:renderAnomalies' });

        // ---- 비용·세션 수·도구 호출 (주간 비교)
        await checkWeeklyNewMetrics(page, t, e, bugs, { partial: false });
        if (t.includes('≥') || (await page.locator('[data-trend-cost-partial]').count())) bugs.push({ severity: 'Major', symptom: '미산정 토큰이 없는데 "≥"/과소 안내가 나옴', file: 'src/views/usage.ts' });
        // 오늘 한 줄에도 새 지표가 같이 나온다(표에는 넣지 않는다).
        const todayRow = steadyItems().find((r) => r.date === localKey(0));
        if (!t.includes(`추정 비용 ${usd(todayRow.costUsd)}`) || !t.includes(`세션 ${todayRow.sessionCount}개`) || !t.includes(`도구 호출 ${cnt(todayRow.toolCalls)}회`)) {
          bugs.push({ severity: 'Major', symptom: `오늘 집계 중 줄에 새 지표(${usd(todayRow.costUsd)}/세션 ${todayRow.sessionCount}/도구 ${todayRow.toolCalls})가 없음`, file: 'src/views/usage.ts:renderWeeklySummary' });
        }

        // ---- 차트 지표 전환
        await checkMetricSwitch(page, steadyItems(), bugs, { partialDays: 0, onCost: () => tallShot(page, shot, '02-trend-metric-cost') });

        // ---- 일별 목록 열: 머리글 + 날짜 수만큼 행, 값이 그날 픽스처와 일치
        const items = steadyItems();
        const listRows = await page.locator('.bar-row-daily:not(.bar-row-head)').count();
        if (listRows !== items.length) bugs.push({ severity: 'Major', symptom: `일별 목록 행이 ${items.length}개여야 하는데 ${listRows}개`, file: 'src/views/usage.ts:renderDailyUsageSection' });
        const headTxt = (await text(page, '.bar-row-head')) ?? '';
        if (!['토큰', '추정 비용', '세션', '도구 호출'].every((h) => headTxt.includes(h))) bugs.push({ severity: 'Major', symptom: `일별 목록 머리글에 열 이름이 없음: ${headTxt}`, file: 'src/views/usage.ts' });
        const firstRow = await page.locator('.bar-row-daily:not(.bar-row-head)').first().innerText();
        const newest = [...items].sort((a, b) => b.date.localeCompare(a.date))[0];
        for (const want of [newest.date, usd(newest.costUsd), String(newest.sessionCount), cnt(newest.toolCalls)]) {
          if (!firstRow.includes(want)) bugs.push({ severity: 'Major', symptom: `일별 목록 최신 행에 "${want}"이(가) 없음: ${firstRow.replace(/\n/g, ' | ')}`, file: 'src/views/usage.ts:renderDailyUsageSection' });
        }
      },
    },
    {
      id: 'spike',

      fixtures: { ...common, get_daily_usage: withSpikeAndDrop() },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-spike-drop');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const state = await page.locator('[data-trend-anomalies]').getAttribute('data-trend-anomalies');
        if (state !== 'found') bugs.push({ severity: 'Critical', symptom: `급증/급락 픽스처인데 이상치 상태=${state}`, file: 'src/usageTrend.ts' });
        const lines = await page.locator('.trend-anomaly').evaluateAll((els) => els.map((e) => ({ kind: e.getAttribute('data-anomaly'), text: e.textContent })));
        const spike = lines.filter((l) => l.kind === 'spike');
        const drop = lines.filter((l) => l.kind === 'drop');
        const md = (o) => localKey(o).slice(5).replace('-', '/');
        if (lines.length !== 2 || spike.length !== 1 || drop.length !== 1) {
          bugs.push({ severity: 'Critical', symptom: `급증 1건·급락 1건이어야 하는데 ${JSON.stringify(lines.map((l) => l.text))}`, file: 'src/usageTrend.ts' });
        } else {
          if (!spike[0].text.includes(md(3)) || !spike[0].text.includes('1.60M')) bugs.push({ severity: 'Major', symptom: `급증 줄에 날짜(${md(3)})/값(1.60M)이 없음: ${spike[0].text}`, file: 'src/views/usage.ts:anomalyLine' });
          if (!drop[0].text.includes(md(10)) || !drop[0].text.includes('60.0K')) bugs.push({ severity: 'Major', symptom: `급락 줄에 날짜(${md(10)})/값(60.0K)이 없음: ${drop[0].text}`, file: 'src/views/usage.ts:anomalyLine' });
          if (!/기준 .*중앙값.*배/.test(spike[0].text)) bugs.push({ severity: 'Major', symptom: `급증 줄에 기준선·배수 근거가 없음: ${spike[0].text}`, file: 'src/views/usage.ts:anomalyLine' });
        }
        if ((await page.locator('.trend-bar-spike').count()) !== 1 || (await page.locator('.trend-bar-drop').count()) !== 1) {
          bugs.push({ severity: 'Major', symptom: '차트의 급증/급락 막대 강조가 각 1개가 아님', file: 'src/views/usage.ts:renderDailyTrendChart' });
        }
        if (!t.includes('규칙:') || !t.includes('직전 14일')) bugs.push({ severity: 'Major', symptom: '이상치 판정 규칙 설명이 화면에 없음', file: 'src/views/usage.ts:renderAnomalies' });
        // 토큰 이상치 판정은 새 지표 때문에 바뀌지 않는다: 지표를 바꿨다가 토큰으로 돌아오면 같은 막대가 강조된다.
        await page.locator('[data-trend-metric="toolCalls"]').click();
        await page.waitForTimeout(150);
        if ((await page.locator('.trend-bar-spike, .trend-bar-drop').count()) !== 0) bugs.push({ severity: 'Major', symptom: '도구 호출 지표에서 급증/급락 막대가 남아 있음', file: 'src/views/usage.ts' });
        if ((await page.locator('[data-anomaly]').count()) !== 2) bugs.push({ severity: 'Major', symptom: '지표 전환이 아래 이상치 목록(토큰 기준)을 바꿈', file: 'src/views/usage.ts' });
        await page.locator('[data-trend-metric="tokens"]').click();
        await page.waitForTimeout(150);
        if ((await page.locator('.trend-bar-spike').count()) !== 1 || (await page.locator('.trend-bar-drop').count()) !== 1) bugs.push({ severity: 'Major', symptom: '토큰 지표로 돌아와도 급증/급락 막대가 복원되지 않음', file: 'src/views/usage.ts' });
      },
    },
    {
      id: 'insufficient',
      // 최근 4일(오늘~오늘-3)만 사용 — 어제까지 끝난 날은 3일이라 주간 요약·이상치 모두 판정하지 않아야 한다.
      fixtures: { ...common, get_daily_usage: [rec(0, 350_000, 0.7), rec(1, 280_000, 0.7), rec(2, 410_000, 0.7), rec(3, 190_000, 0.7)] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-insufficient');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const status = await page.locator('.trend-box').getAttribute('data-trend-weekly-status');
        if (status !== 'insufficient') bugs.push({ severity: 'Critical', symptom: `4일 데이터인데 주간 상태가 insufficient가 아님(${status})`, file: 'src/usageTrend.ts' });
        if (!t.includes('첫 사용일부터 어제까지 3일') || !t.includes('7일 이상')) bugs.push({ severity: 'Major', symptom: `데이터 부족 안내 문구가 없음: ${t.slice(0, 160)}`, file: 'src/views/usage.ts:renderWeeklySummary' });
        if (await page.locator('.trend-week-table').count()) bugs.push({ severity: 'Major', symptom: '데이터 7일 미만인데 주간 비교 표가 그려짐', file: 'src/views/usage.ts' });
        const state = await page.locator('[data-trend-anomalies]').getAttribute('data-trend-anomalies');
        if (state !== 'insufficient' || !t.includes('판정하지 않았습니다') || !t.includes('이상치가 없다는 뜻이 아닙니다')) bugs.push({ severity: 'Critical', symptom: `이상치 "판정 안 함" 안내가 아님(state=${state})`, file: 'src/views/usage.ts:renderAnomalies' });
        if (await page.locator('.trend-bar-spike, .trend-bar-drop, .trend-anomaly').count()) bugs.push({ severity: 'Critical', symptom: '표본 부족인데 이상치가 표시됨', file: 'src/usageTrend.ts' });
        if ((await page.locator('.trend-bar').count()) !== 30) bugs.push({ severity: 'Major', symptom: '데이터가 적어도 30일 연속 막대가 그려져야 함', file: 'src/views/usage.ts' });
      },
    },
    {
      id: 'no-baseline',
      // 최초 사용일이 오늘-9 -> 어제까지 끝난 날 9일: 최근 완료 7일은 보이고 그 전 7일은 비교 불가.
      fixtures: { ...common, get_daily_usage: Array.from({ length: 10 }, (_, i) => rec(i, 300_000 + i * 10_000, 0.7)) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-no-baseline');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const status = await page.locator('.trend-box').getAttribute('data-trend-weekly-status');
        if (status !== 'no-baseline') bugs.push({ severity: 'Critical', symptom: `10일 데이터인데 주간 상태가 no-baseline이 아님(${status})`, file: 'src/usageTrend.ts' });
        if (!t.includes('비교하지 않습니다')) bugs.push({ severity: 'Major', symptom: '그 전 7일 비교 불가 안내가 없음', file: 'src/views/usage.ts' });
        if (/[+-]\d+(\.\d)?%(?!p)/.test(t.replace(/\s/g, '')) && /전주 대비\s*[+-]/.test(t)) bugs.push({ severity: 'Major', symptom: '그 전 7일이 없는데 증감%가 표시됨', file: 'src/views/usage.ts' });
        // 새 지표도 최근 완료 7일 값만 보이고 그 전 7일·증감은 '—'.
        const items = Array.from({ length: 10 }, (_, i) => rec(i, 300_000 + i * 10_000, 0.7));
        const cur = items.filter((r) => { for (let o = 1; o <= 7; o++) if (r.date === localKey(o)) return true; return false; });
        const rows = await page.locator('.trend-week-row').evaluateAll((els) => els.map((r) => [...r.children].map((c) => c.innerText.replace(/\n/g, ' '))));
        for (const [label, wantCur] of [
          ['추정 비용', usd(cur.reduce((a, r) => a + r.costUsd, 0))],
          ['세션 수(일별 합)', `${cnt(cur.reduce((a, r) => a + r.sessionCount, 0))}개`],
          ['도구 호출', `${cnt(cur.reduce((a, r) => a + r.toolCalls, 0))}회`],
        ]) {
          const r = rows.find((c) => c[0] === label);
          if (!r || r[1] !== wantCur || r[2] !== '—' || r[3] !== '—') bugs.push({ severity: 'Major', symptom: `no-baseline ${label} 행이 [${wantCur}, —, —]이어야 하는데 ${JSON.stringify(r)}`, file: 'src/views/usage.ts:renderWeeklySummary' });
        }
      },
    },
    {
      id: 'prev-zero',
      // 그 전 7일(오늘-8~-14)에 사용이 없고, 오늘-20과 최근 완료 7일(오늘-1~-7)에 사용 -> 전주 0: % 계산 불가를 정직하게 표시.
      // 오늘(offset 0) 100K는 주간 비교에 들어가지 않아야 한다(+250.0K이지 +350.0K가 아니다).
      fixtures: { ...common, get_daily_usage: [rec(20, 300_000, 0.7), rec(2, 250_000, 0.8), rec(0, 100_000, 0.8)] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-prev-zero');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const status = await page.locator('.trend-box').getAttribute('data-trend-weekly-status');
        if (status !== 'ok') bugs.push({ severity: 'Critical', symptom: `전주 0 픽스처가 ok 상태가 아님(${status})`, file: 'src/usageTrend.ts' });
        if (!t.includes('그 전 7일 0') || !t.includes('% 계산 안 함')) bugs.push({ severity: 'Major', symptom: '전주 0일 때 "% 계산 안 함" 표기가 없음', file: 'src/views/usage.ts' });
        if (!t.includes('+250.0K')) bugs.push({ severity: 'Major', symptom: '전주 0이어도 절대 증감(+250.0K, 오늘 제외)은 표시되어야 함', file: 'src/views/usage.ts' });
        if (t.includes('+350.0K')) bugs.push({ severity: 'Critical', symptom: '오늘 부분일(100K)이 주간 비교에 섞임(+350.0K)', file: 'src/usageTrend.ts:summarizeWeeks' });
        // 새 지표: 그 전 7일이 0이면 절대 증감은 값 그대로, %는 계산하지 않는다(토큰·비용·세션·도구 호출 4행 모두).
        const zeroPct = (t.match(/그 전 7일 0 — % 계산 안 함/g) ?? []).length;
        if (zeroPct !== 4) bugs.push({ severity: 'Major', symptom: `그 전 7일 0일 때 "% 계산 안 함"이 4행(토큰·비용·세션·도구)에 나와야 하는데 ${zeroPct}번`, file: 'src/views/usage.ts' });
        const r2 = rec(2, 250_000, 0.8);
        if (!t.includes(`+${usd(r2.costUsd)}`) || t.includes(`+${usd(r2.costUsd + 1)}`)) bugs.push({ severity: 'Major', symptom: `전주 0 비용 절대 증감(+${usd(r2.costUsd)}, 오늘 제외)이 아님`, file: 'src/views/usage.ts' });
        if (!t.includes(`+${r2.sessionCount}개`) || !t.includes(`+${cnt(r2.toolCalls)}회`)) bugs.push({ severity: 'Major', symptom: `전주 0 세션/도구 호출 절대 증감(+${r2.sessionCount}개/+${cnt(r2.toolCalls)}회)이 없음`, file: 'src/views/usage.ts' });
        const rows = await page.locator('.trend-week-row').evaluateAll((els) => els.map((r) => [...r.children].map((c) => c.innerText.replace(/\n/g, ' '))));
        const costRow = rows.find((c) => c[0] === '추정 비용');
        if (!costRow || costRow[2] !== usd(0)) bugs.push({ severity: 'Major', symptom: `그 전 7일 비용이 $0.00으로 표시되어야 함(${JSON.stringify(costRow)})`, file: 'src/views/usage.ts' });
      },
    },
    {
      id: 'unpriced',
      // 비용 미산정 토큰이 있는 날(최근 완료 7일 안 2일 -> 어제가 아닌 offset 2, 그 전 7일 안 offset 9, 창 밖 18).
      // 숨기지 않는다: 값은 "≥"로, 칸마다 제외된 토큰 수, 표 아래 과소 안내, 비용 차트의 줄무늬 막대, 일별 목록 "≥".
      fixtures: { ...common, get_daily_usage: withUnpriced([2, 9, 18]) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-unpriced');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const items = withUnpriced([2, 9, 18]);
        const e = expectedWeekly(items);
        await checkWeeklyNewMetrics(page, t, e, bugs, { partial: true });
        if (e.curUnpriced !== 120_000 || e.prevUnpriced !== 120_000) bugs.push({ severity: 'Minor', symptom: '픽스처 오류: 각 창에 미산정 120K 1일이어야 함', file: 'scripts/ui-harness/flows/usageTrend.mjs' });
        const subs = (t.match(/미산정 120\.0K 토큰 제외/g) ?? []).length;
        if (subs < 2) bugs.push({ severity: 'Major', symptom: `각 주 비용 칸에 "미산정 120.0K 토큰 제외"가 있어야 하는데 ${subs}번`, file: 'src/views/usage.ts:renderWeeklySummary' });
        if (!t.includes('미산정 포함 — 증감은 참고용')) bugs.push({ severity: 'Major', symptom: '비용 증감이 참고용이라는 표기가 없음', file: 'src/views/usage.ts' });
        const note = (await text(page, '[data-trend-cost-partial]')) ?? '';
        if (!note.includes('실제보다 작게') || !note.includes('최근 완료 7일 120.0K') || !note.includes('그 전 7일 120.0K')) bugs.push({ severity: 'Major', symptom: `과소 안내(창별 미산정 토큰 수 포함)가 없음: ${note}`, file: 'src/views/usage.ts' });
        // 토큰·세션·도구 호출 칸은 영향받지 않는다(≥가 붙지 않는다).
        const sessRow = await page.locator('.trend-week-row').evaluateAll((els) => els.map((r) => r.innerText).find((x) => x.startsWith('세션 수(일별 합)')) ?? '');
        if (sessRow.includes('≥')) bugs.push({ severity: 'Major', symptom: '세션 수 행에 ≥가 붙음', file: 'src/views/usage.ts' });
        await checkMetricSwitch(page, items, bugs, { partialDays: 3, onCost: () => tallShot(page, shot, '02-trend-unpriced-cost-chart') });
        // 일별 목록: 미산정 있는 3일만 "≥", title에 제외 토큰 수.
        const listCosts = await page.locator('.bar-row-daily:not(.bar-row-head)').evaluateAll((els) => els.filter((r) => r.innerText.includes('≥')).map((r) => r.children[3].title));
        if (listCosts.length !== 3 || !listCosts.every((x) => x.includes('미산정 120.0K 토큰 제외') && x.includes('실제보다 작게'))) bugs.push({ severity: 'Major', symptom: `일별 목록의 ≥ 행이 3개(title에 제외 토큰 수)여야 하는데 ${JSON.stringify(listCosts)}`, file: 'src/views/usage.ts:dailyCostCell' });
        // 이상치 판정(토큰)은 미산정 토큰과 무관하다.
        if ((await page.locator('[data-trend-anomalies]').getAttribute('data-trend-anomalies')) !== 'none') bugs.push({ severity: 'Major', symptom: '미산정 토큰이 이상치 판정을 바꿈', file: 'src/usageTrend.ts' });
      },
    },
    {
      id: 'today-only-judgeable',
      // 어제까지 5일(오늘-1~-5) 300K + 오늘 300K: 기준선 5일을 갖춘 날은 오늘뿐이고 오늘은 급증이 아니다.
      // 판정을 미룬 사유는 기준선 부족이 아니라 "하루가 끝나지 않음"이어야 한다(옛 문구 "그런 날이 아직 없습니다"는 사실과 다름).
      fixtures: { ...common, get_daily_usage: [1, 2, 3, 4, 5, 0].map((o) => rec(o, 300_000, 0.7)) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await tallShot(page, shot, '01-trend-today-only-judgeable');
        const t = await commonChecks(page, bugs, { expectBox: true });
        const state = await page.locator('[data-trend-anomalies]').getAttribute('data-trend-anomalies');
        if (state !== 'today-pending') bugs.push({ severity: 'Major', symptom: `판정 가능일이 오늘뿐인데 이상치 상태가 today-pending이 아님(${state})`, file: 'src/views/usage.ts:renderAnomalies' });
        if (!t.includes('오늘은 하루가 끝나지 않아 판정을 미뤘습니다') || !t.includes('이상치가 없다는 뜻이 아닙니다')) bugs.push({ severity: 'Major', symptom: `오늘 판정 보류 문구가 없음: ${t.slice(0, 200)}`, file: 'src/views/usage.ts:renderAnomalies' });
        if (t.includes('그런 날이 아직 없습니다')) bugs.push({ severity: 'Major', symptom: '오늘이 기준선을 갖췄는데 "그런 날이 아직 없습니다"라는 사실과 다른 문구가 나옴', file: 'src/views/usage.ts:renderAnomalies' });
        if (await page.locator('.trend-bar-spike, .trend-bar-drop, .trend-anomaly').count()) bugs.push({ severity: 'Critical', symptom: '급증이 아닌 오늘에 이상치가 표시됨', file: 'src/usageTrend.ts' });
      },
    },
    {
      id: 'empty',
      fixtures: { ...common, get_daily_usage: [] },
      async run(page, { bugs }) {
        await page.waitForTimeout(300);
        if (await page.locator('.trend-box').count()) bugs.push({ severity: 'Minor', symptom: '사용 기록 0건인데 사용 추이 박스가 보임(빈 상태는 기존 안내만)', file: 'src/views/usage.ts:renderDailyTab' });
      },
    },
    {
      id: 'boundary',
      fixtures: { ...common, get_daily_usage: steadyItems() },
      async run(page, { bugs }) {
        // 실제 앱이 쓰는 모듈 그대로 — Vite dev 서버가 TS를 변환해 서빙하는 JS를 받아 Node에서
        // 로드한다(usageTrend.ts는 DOM/Tauri/다른 모듈 import가 없다).
        const baseUrl = process.env.UI_HARNESS_BASE_URL || 'http://localhost:1420';
        const js = await (await fetch(`${baseUrl}/src/usageTrend.ts`)).text();
        if (/^\s*import\s/m.test(js)) bugs.push({ severity: 'Major', symptom: 'usageTrend.ts가 다른 모듈을 import함(순수 함수 계약 위반)', file: 'src/usageTrend.ts' });
        const mod = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
        const result = runTrendBoundaryChecks(mod);
        for (const row of result.rows) console.log(`[usageTrend/boundary] ${row}`);
        console.log(`[usageTrend/boundary] ${result.total - result.failures.length}/${result.total} PASS`);
        for (const f of result.failures) bugs.push({ severity: 'Critical', symptom: `경계값 검사 실패: ${f}`, file: 'src/usageTrend.ts' });
        if (result.total < 50) bugs.push({ severity: 'Major', symptom: `경계값 케이스 수가 너무 적음(${result.total})`, file: 'scripts/ui-harness/lib/trendBoundary.mjs' });
      },
    },
  ];
}
