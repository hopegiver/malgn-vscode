// 사용량 화면 "사용 추이"(WBS P3-1 주간 요약 / P3-2 이상치) — 순수 계산 모듈.
//
// 원칙(바꾸기 전에 읽을 것):
//   1. 입력은 fetchDailyUsage()가 이미 돌려주는 최근 30일 DailyUsage[]뿐이다. 새
//      수집·새 로그 파싱·비용 계산을 하지 않는다(비용·세션 수·툴 호출은 범위 밖).
//   2. 계산된 수치만 낸다. 원인을 추정하지 않는다("~때문" 금지) — 이 코드는 원인을 모른다.
//   3. 경계(0 나누기·그 전 7일 0·데이터 부족)는 NaN/Infinity 대신 null + 사유 상태로 낸다.
//      반환값의 모든 number는 유한수(finite)다.
//   4. DOM/Tauri/다른 모듈에 의존하지 않는다(import 없음) — node/하네스가 Vite가 서빙하는
//      변환 결과를 그대로 로드해 경계값을 검증한다. 앱 전체에서 단 하나뿐인 정의인
//      "일 토큰 합"(dailyUsageTotal)과 "캐시 히트율"(computeCacheHitRate)은 views/usage.ts
//      것을 deps로 주입받아 그대로 쓴다(여기서 재정의하지 않는다).
//   5. 임계값은 이 파일 맨 위 TREND_THRESHOLDS 한 곳에만 둔다.
//   6. 날짜 키는 앱 전체와 같은 로컬 'YYYY-MM-DD'다. 날짜 산술은 UTC 달력 연산으로만 해
//      서머타임·타임존 영향을 받지 않는다.

// ---------------- 임계값(한 곳, 근거 주석) ----------------

export const TREND_THRESHOLDS = {
  // 추이를 그리는 구간 길이(일). 입력 데이터가 최근 30일이라 그에 맞춘다.
  windowDays: 30,
  // 주간 비교 단위(일).
  weekDays: 7,

  // 이상치 판정의 기준선: 그 날 직전 baselineLookbackDays일 중 "사용 있는 날"의 일 토큰
  // 중앙값. 평균 대신 중앙값을 쓰는 이유는 직전에 큰 급증이 있어도 기준선이 끌려가지 않게
  // 하려는 것이고, 사용 없는 날(0)을 기준선에서 빼는 이유는 주말·휴가 때문에 기준선이
  // 낮아져 월요일마다 "급증"으로 잡히는 것을 막으려는 것이다. 값 자체는 경험적 보수치다
  // (통계적 유의성 계산이 아님).
  baselineLookbackDays: 14,
  // 기준선을 만들 최소 "사용 있는 날" 수. 이보다 적으면 그 날은 판정하지 않는다.
  minBaselineActiveDays: 5,
  // 급증: 그 날 토큰 >= 기준선 x spikeRatio. 급락: 그 날 토큰 x dropDivisor <= 기준선
  // (= 기준선의 1/3 이하). 나눗셈 대신 곱셈으로 비교해 1/3의 부동소수 오차가 경계를 흔들지 않게 한다.
  spikeRatio: 3,
  dropDivisor: 3,
  // 절대 편차 하한(토큰): |그 날 - 기준선| 이 이 값 미만이면 배수가 커도 판정하지 않는다.
  // 기준선이 작을 때(예: 1K 토큰대) 3배가 의미 없는 잡음인 것을 걸러낸다. 10만은
  // efficiencyGuide.ts의 표본 하한(minSampleTokens)과 같은 규모의 보수치다.
  minAbsDeviationTokens: 100_000,
} as const;

// ---------------- 타입 ----------------

// DailyUsage(usageApi.ts)의 구조적 부분집합 — 그대로 넘길 수 있다.
export interface TrendDayInput {
  readonly date: string;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly cacheCreationTokens: number;
  readonly cacheReadTokens: number;
}

export interface TrendDeps {
  // views/usage.ts dailyUsageTotal
  readonly dayTotal: (d: TrendDayInput) => number;
  // views/usage.ts computeCacheHitRate — 정수(%) 또는 분모 0이면 null
  readonly cacheHitRate: (input: number, cacheCreation: number, cacheRead: number) => number | null;
}

// 0으로 채워 연속화한 일별 점. 기록이 없던 날도 한 점이다(total=0).
export interface SeriesPoint {
  readonly date: string;
  readonly input: number;
  readonly output: number;
  readonly cacheCreation: number;
  readonly cacheRead: number;
  readonly total: number;
}

export interface WeekMetrics {
  readonly startDate: string;
  readonly endDate: string;
  readonly totalTokens: number;
  readonly activeDays: number;
  readonly cacheHitRate: number | null; // 정수(%), 분모 0이면 null
}

export interface WeekDelta {
  readonly tokensAbs: number; // 최근 완료 7일 - 그 전 7일
  readonly tokensPct: number | null; // 그 전 7일 토큰이 0이면 null(0 나누기 방지), 소수 1자리
  readonly activeDaysAbs: number;
  readonly cacheHitPointDiff: number | null; // 퍼센트포인트(%p) 차, 어느 한쪽이 null이면 null
}

// 경과일은 "어제까지 끝난 날"만 센다(오늘은 하루가 끝나지 않아 제외).
// insufficient: 경과일 < 7 — 주간 요약 자체를 내지 않는다
// no-baseline:  7 <= 경과일 < 14 — 최근 완료 7일만 보이고 그 전 7일과는 비교하지 않는다
// ok:           경과일 >= 14 — 그 전 7일과 비교한다(그 전 7일 토큰이 0일 수는 있다)
export type WeeklyStatus = 'insufficient' | 'no-baseline' | 'ok';

export interface TodayPartial {
  readonly date: string;
  readonly totalTokens: number; // 아직 끝나지 않은 오늘의 누적 — 주간 비교 창에 넣지 않는다
}

export interface WeeklySummary {
  readonly status: WeeklyStatus;
  // 최근 30일 창 안에서 처음 사용한 날부터 어제까지의 경과일(양끝 포함). 어제까지 사용한
  // 날이 없으면 0. (창 밖 — 31일 전 이전 — 의 사용은 입력에 없어 알 수 없다.)
  readonly spanDays: number;
  readonly current: WeekMetrics | null; // 어제까지 끝난 7일. insufficient이면 null
  readonly previous: WeekMetrics | null; // 그 전 7일. ok일 때만
  readonly delta: WeekDelta | null; // ok일 때만
  readonly today: TodayPartial | null; // 집계 중인 오늘(표 밖 표기용). 시계열이 비었으면 null
}

export type AnomalyKind = 'spike' | 'drop' | 'normal' | 'unjudged';
export type UnjudgedReason = 'zero-day' | 'insufficient-baseline' | 'today-partial';

export interface AnomalyPoint {
  readonly date: string;
  readonly total: number;
  readonly kind: AnomalyKind;
  readonly baselineMedian: number | null; // 기준선을 만들 수 있었을 때만
  readonly baselineDays: number; // 기준선에 쓴 "사용 있는 날" 수
  readonly ratio: number | null; // total / baselineMedian, 소수 2자리 (기준선이 있을 때만)
  readonly reason: UnjudgedReason | null; // kind === 'unjudged'일 때만
}

export interface AnomalyReport {
  readonly points: readonly AnomalyPoint[]; // series와 같은 길이·순서(오래된 날 -> 최근)
  readonly judgedCount: number; // spike/drop/normal로 실제 판정한 날 수
  readonly anomalies: readonly AnomalyPoint[]; // spike/drop만(최근 날짜 먼저)
}

export interface UsageTrend {
  readonly series: readonly SeriesPoint[];
  readonly weekly: WeeklySummary;
  readonly anomalies: AnomalyReport;
}

// ---------------- 날짜 산술(UTC 달력 연산) ----------------

const DATE_KEY = /^(\d{4})-(\d{2})-(\d{2})$/;

function parseDateKey(key: string): number | null {
  const m = DATE_KEY.exec(key);
  if (!m) return null;
  const t = Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  if (!Number.isFinite(t)) return null;
  // 2026-02-31 같은 존재하지 않는 날짜 거르기(Date.UTC는 3월 3일로 굴려버린다).
  return formatUtc(t) === key ? t : null;
}

function formatUtc(ms: number): string {
  const d = new Date(ms);
  return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
}

const DAY_MS = 86_400_000;

export function addDays(dateKey: string, n: number): string | null {
  const t = parseDateKey(dateKey);
  return t === null ? null : formatUtc(t + n * DAY_MS);
}

// a부터 b까지 일수(b - a). 형식이 잘못되면 null.
export function daysBetween(a: string, b: string): number | null {
  const ta = parseDateKey(a);
  const tb = parseDateKey(b);
  return ta === null || tb === null ? null : Math.round((tb - ta) / DAY_MS);
}

function finiteOrZero(n: number): number {
  return Number.isFinite(n) && n > 0 ? n : 0;
}

function round(n: number, digits: number): number {
  const p = 10 ** digits;
  return Math.round(n * p) / p;
}

// ---------------- P3-1: 0채움 일별 시계열 ----------------

/**
 * today를 마지막 날로 하는 windowDays일의 연속 일별 시계열을 만든다(오래된 날 -> 오늘).
 * 기록이 없는 날은 0으로 채운다. 구간 밖 날짜·형식이 잘못된 날짜는 무시하고, 같은 날짜가
 * 여러 번 오면 합산한다. 음수·NaN·Infinity 토큰은 0으로 본다. today가 유효한 날짜 키가
 * 아니면 빈 배열이다.
 */
export function buildDailySeries(items: readonly TrendDayInput[], today: string, deps: TrendDeps): SeriesPoint[] {
  const n = TREND_THRESHOLDS.windowDays;
  const first = addDays(today, -(n - 1));
  if (first === null) return [];
  const byDate = new Map<string, { input: number; output: number; cacheCreation: number; cacheRead: number }>();
  for (const d of items) {
    if (parseDateKey(d.date) === null) continue;
    const acc = byDate.get(d.date) ?? { input: 0, output: 0, cacheCreation: 0, cacheRead: 0 };
    acc.input += finiteOrZero(d.inputTokens);
    acc.output += finiteOrZero(d.outputTokens);
    acc.cacheCreation += finiteOrZero(d.cacheCreationTokens);
    acc.cacheRead += finiteOrZero(d.cacheReadTokens);
    byDate.set(d.date, acc);
  }
  const series: SeriesPoint[] = [];
  for (let i = 0; i < n; i++) {
    const date = addDays(first, i) as string;
    const v = byDate.get(date) ?? { input: 0, output: 0, cacheCreation: 0, cacheRead: 0 };
    const total = finiteOrZero(
      deps.dayTotal({ date, inputTokens: v.input, outputTokens: v.output, cacheCreationTokens: v.cacheCreation, cacheReadTokens: v.cacheRead })
    );
    series.push({ date, input: v.input, output: v.output, cacheCreation: v.cacheCreation, cacheRead: v.cacheRead, total });
  }
  return series;
}

function weekMetrics(points: readonly SeriesPoint[], deps: TrendDeps): WeekMetrics {
  let total = 0;
  let input = 0;
  let creation = 0;
  let read = 0;
  let active = 0;
  for (const p of points) {
    total += p.total;
    input += p.input;
    creation += p.cacheCreation;
    read += p.cacheRead;
    if (p.total > 0) active += 1;
  }
  return {
    startDate: points[0].date,
    endDate: points[points.length - 1].date,
    totalTokens: total,
    activeDays: active,
    cacheHitRate: deps.cacheHitRate(input, creation, read),
  };
}

/**
 * 어제까지 끝난 7일과 그 전 7일을 요약하고 증감을 낸다. 아직 끝나지 않은 오늘은 두 창
 * 어디에도 넣지 않는다(하루 중 이른 시각일수록 "이번 주"가 낮게 잡혀 전주 대비 부호가
 * 뒤집히는 편향을 막기 위해) — 오늘 수치는 today로 따로 돌려준다.
 * 활동 시작(최근 30일 창 안 최초 사용일) 이후 "완료된 날" 경과일로 비교 가능 여부를 정한다 —
 * 앞선 주가 "사용 안 한 주"인지 "아직 수집 전인 주"인지 입력만으로는 구분할 수 없어서,
 * 최초 사용일 이전 구간은 비교 기준으로 쓰지 않는다.
 * series의 마지막 점이 오늘이라고 가정한다(buildDailySeries가 그렇게 만든다).
 */
export function summarizeWeeks(series: readonly SeriesPoint[], deps: TrendDeps): WeeklySummary {
  const W = TREND_THRESHOLDS.weekDays;
  const today: TodayPartial | null = series.length > 0 ? { date: series[series.length - 1].date, totalTokens: series[series.length - 1].total } : null;
  const done = series.slice(0, Math.max(0, series.length - 1)); // 어제까지 끝난 날
  const firstActive = done.findIndex((p) => p.total > 0);
  const spanDays = firstActive < 0 ? 0 : done.length - firstActive;
  if (done.length < W * 2 || spanDays < W) {
    return { status: 'insufficient', spanDays, current: null, previous: null, delta: null, today };
  }
  const cur = weekMetrics(done.slice(done.length - W), deps);
  if (spanDays < W * 2) {
    return { status: 'no-baseline', spanDays, current: cur, previous: null, delta: null, today };
  }
  const prev = weekMetrics(done.slice(done.length - W * 2, done.length - W), deps);
  const tokensAbs = cur.totalTokens - prev.totalTokens;
  const delta: WeekDelta = {
    tokensAbs,
    tokensPct: prev.totalTokens > 0 ? round((tokensAbs / prev.totalTokens) * 100, 1) : null,
    activeDaysAbs: cur.activeDays - prev.activeDays,
    cacheHitPointDiff: cur.cacheHitRate !== null && prev.cacheHitRate !== null ? cur.cacheHitRate - prev.cacheHitRate : null,
  };
  return { status: 'ok', spanDays, current: cur, previous: prev, delta, today };
}

// ---------------- P3-2: 이상치 ----------------

function median(sortedAsc: readonly number[]): number {
  const n = sortedAsc.length;
  const mid = Math.floor(n / 2);
  return n % 2 === 1 ? sortedAsc[mid] : (sortedAsc[mid - 1] + sortedAsc[mid]) / 2;
}

/**
 * 일별 토큰의 급증·급락을 판정한다. 규칙(TREND_THRESHOLDS):
 *   기준선 = 그 날 직전 14일 중 토큰 > 0인 날들의 일 토큰 중앙값 (그 날 자신은 제외).
 *   급증  = 그 날 >= 기준선 x 3  그리고  (그 날 - 기준선) >= 100,000 토큰
 *   급락  = 그 날 <= 기준선 / 3  그리고  (기준선 - 그 날) >= 100,000 토큰 (그 날 > 0)
 * 판정하지 않는 경우(kind='unjudged'): 사용 없는 날(0 — 휴무와 구분할 수 없다), 기준선에
 * 쓸 사용일이 5일 미만, 오늘(하루가 끝나지 않았다 — 급증만 판정한다. 이미 쌓인 양이 기준
 * 이상이면 하루가 끝나도 줄지 않기 때문이다. 급증이 아니면 '기준 이내'도 '급락'도 단정하지
 * 않고 today-partial로 둔다). 기준선은 입력 구간(최근 30일) 안의 날만 쓴다.
 */
export function detectAnomalies(series: readonly SeriesPoint[], today: string): AnomalyReport {
  const T = TREND_THRESHOLDS;
  const points: AnomalyPoint[] = [];
  for (let i = 0; i < series.length; i++) {
    const p = series[i];
    const lookback = series.slice(Math.max(0, i - T.baselineLookbackDays), i).filter((q) => q.total > 0);
    const base = { date: p.date, total: p.total, baselineDays: lookback.length };
    if (p.total <= 0) {
      points.push({ ...base, kind: 'unjudged', baselineMedian: null, ratio: null, reason: 'zero-day' });
      continue;
    }
    if (lookback.length < T.minBaselineActiveDays) {
      points.push({ ...base, kind: 'unjudged', baselineMedian: null, ratio: null, reason: 'insufficient-baseline' });
      continue;
    }
    const m = median(lookback.map((q) => q.total).sort((a, b) => a - b));
    const ratio = m > 0 ? round(p.total / m, 2) : null;
    const absDev = Math.abs(p.total - m);
    const isToday = p.date === today;
    let kind: AnomalyKind = 'normal';
    if (m > 0 && p.total >= m * T.spikeRatio && absDev >= T.minAbsDeviationTokens) kind = 'spike';
    else if (isToday) kind = 'unjudged';
    else if (m > 0 && p.total * T.dropDivisor <= m && absDev >= T.minAbsDeviationTokens) kind = 'drop';
    points.push({ ...base, kind, baselineMedian: m, ratio, reason: kind === 'unjudged' ? 'today-partial' : null });
  }
  const judgedCount = points.filter((p) => p.kind !== 'unjudged').length;
  const anomalies = points.filter((p) => p.kind === 'spike' || p.kind === 'drop').reverse();
  return { points, judgedCount, anomalies };
}

// ---------------- 진입점 ----------------

/**
 * @param items  fetchDailyUsage() 결과(최근 30일, 기록 있는 날만).
 * @param today  로컬 'YYYY-MM-DD' (화면이 localDateKey(new Date())로 넘긴다 — 테스트에서 고정 가능).
 * @param deps   views/usage.ts의 dailyUsageTotal·computeCacheHitRate.
 */
export function computeUsageTrend(items: readonly TrendDayInput[], today: string, deps: TrendDeps): UsageTrend {
  const series = buildDailySeries(items, today, deps);
  return { series, weekly: summarizeWeeks(series, deps), anomalies: detectAnomalies(series, today) };
}
