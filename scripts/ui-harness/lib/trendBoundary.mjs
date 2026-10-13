// 사용 추이(src/usageTrend.ts) 순수 함수의 경계값 검사 — P3-1 0채움·주 경계·전주 0, P3-2 이상치 규칙 경계.
//
// runTrendBoundaryChecks(mod)는 DOM/클로저 없이 mod(usageTrend 모듈)만 받는다.
//   1) 하네스 flows/usageTrend.mjs boundary 시나리오: Vite dev 서버가 서빙하는 실제
//      모듈(/src/usageTrend.ts 변환 결과)을 받아 Node에서 로드해 실행
//   2) node 단독(dev 서버 없이, Node 22.6+):
//        node --experimental-strip-types -e "
//          const m = await import('./src/usageTrend.ts');
//          const { runTrendBoundaryChecks } = await import('./scripts/ui-harness/lib/trendBoundary.mjs');
//          const r = runTrendBoundaryChecks(m); console.log(r.rows.join('\n')); console.log(r.total - r.failures.length + '/' + r.total, 'PASS');
//          process.exit(r.failures.length ? 1 : 0);"
// 반환: { total, failures: string[], rows: string[] } (rows는 케이스별 PASS/FAIL 한 줄)
//
// deps(dayTotal/cacheHitRate)는 views/usage.ts의 dailyUsageTotal/computeCacheHitRate와 같은
// 정의를 여기에 사본으로 둔다 — views/usage.ts는 DOM/Tauri를 import해서 Node에서 로드할 수
// 없기 때문이다. 앱에서는 실제 함수가 주입되고, 두 정의가 어긋나면 화면 시나리오
// (flows/usageTrend.mjs)의 타일 대조 단언이 잡는다.
export function runTrendBoundaryChecks(mod) {
  const { computeUsageTrend, buildDailySeries, summarizeWeeks, detectAnomalies, addDays, daysBetween, TREND_THRESHOLDS: T } = mod;
  const rows = [];
  const failures = [];
  let total = 0;
  const check = (name, cond, detail) => {
    total += 1;
    const line = `${cond ? 'PASS' : 'FAIL'} ${name}${cond ? '' : ` — ${detail}`}`;
    rows.push(line);
    if (!cond) failures.push(line);
  };

  const deps = {
    dayTotal: (d) => d.inputTokens + d.outputTokens + d.cacheCreationTokens + d.cacheReadTokens,
    cacheHitRate: (input, creation, read) => {
      const denom = input + creation + read;
      return denom > 0 ? Math.round((read / denom) * 100) : null;
    },
  };
  const TODAY = '2026-10-13';
  const day = (offset) => addDays(TODAY, -offset); // offset 0 = 오늘
  const rec = (offset, total_, extra = {}) => ({ date: day(offset), inputTokens: total_, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0, ...extra });
  const trendOf = (items, today = TODAY) => computeUsageTrend(items, today, deps);

  // 결과 객체 안의 모든 number가 유한수인가(NaN/Infinity가 화면에 나올 수 없음).
  const allFinite = (v) => {
    if (typeof v === 'number') return Number.isFinite(v);
    if (Array.isArray(v)) return v.every(allFinite);
    if (v && typeof v === 'object') return Object.values(v).every(allFinite);
    return true;
  };

  // ---- 날짜 산술 ----
  check('addDays: 월말 넘김 2026-10-31 +1 = 2026-11-01', addDays('2026-10-31', 1) === '2026-11-01', addDays('2026-10-31', 1));
  check('addDays: 연말 넘김 2026-01-01 -1 = 2025-12-31', addDays('2026-01-01', -1) === '2025-12-31', addDays('2026-01-01', -1));
  check('addDays: 윤년 2024-03-01 -1 = 2024-02-29', addDays('2024-03-01', -1) === '2024-02-29', addDays('2024-03-01', -1));
  check('addDays: 평년 2026-03-01 -1 = 2026-02-28', addDays('2026-03-01', -1) === '2026-02-28', addDays('2026-03-01', -1));
  check('addDays: 존재하지 않는 날짜 2026-02-31은 null', addDays('2026-02-31', 0) === null, String(addDays('2026-02-31', 0)));
  check('addDays: 형식 오류는 null', addDays('2026-1-5', 0) === null && addDays('', 0) === null && addDays('abc', 1) === null, 'null 아님');
  check('daysBetween: 서머타임 구간도 정확히 일수 (2026-03-07~2026-03-09 = 2)', daysBetween('2026-03-07', '2026-03-09') === 2, String(daysBetween('2026-03-07', '2026-03-09')));

  // ---- P3-1: 0채움 ----
  {
    const s = buildDailySeries([rec(0, 500), rec(10, 300)], TODAY, deps);
    check('0채움: 길이 = windowDays(30)', s.length === T.windowDays, String(s.length));
    check('0채움: 마지막 날 = 오늘, 첫 날 = 오늘-29', s[29]?.date === TODAY && s[0]?.date === day(29), `${s[0]?.date}..${s[29]?.date}`);
    let continuous = true;
    for (let i = 1; i < s.length; i++) if (daysBetween(s[i - 1].date, s[i].date) !== 1) continuous = false;
    check('0채움: 모든 날짜가 하루씩 연속(빈 날 없음)', continuous, '날짜 간격이 1일이 아닌 곳이 있음');
    check('0채움: 기록 없는 날 total=0, 기록 있는 날 값 보존', s[19].total === 300 && s[29].total === 500 && s[28].total === 0 && s[0].total === 0, JSON.stringify([s[0].total, s[19].total, s[28].total, s[29].total]));
    check('0채움: 합계 보존(500+300)', s.reduce((a, p) => a + p.total, 0) === 800, String(s.reduce((a, p) => a + p.total, 0)));
  }
  {
    const s = buildDailySeries([], TODAY, deps);
    check('0채움: 입력 0건이어도 30일 전부 0으로 채움', s.length === 30 && s.every((p) => p.total === 0), `len=${s.length}`);
  }
  {
    const s = buildDailySeries([rec(30, 999), rec(29, 100), rec(-1, 777)], TODAY, deps);
    check('구간 경계: 오늘-29는 포함, 오늘-30과 미래(오늘+1)는 무시', s[0].total === 100 && s.reduce((a, p) => a + p.total, 0) === 100, JSON.stringify(s.map((p) => p.total).filter(Boolean)));
  }
  {
    const s = buildDailySeries(
      [{ date: '2026-13-45', inputTokens: 1, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0 }, { date: '2026-02-31', inputTokens: 1, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0 }, { date: '', inputTokens: 1, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0 }],
      TODAY,
      deps
    );
    check('잘못된 날짜 키는 무시하고 던지지 않음', s.length === 30 && s.every((p) => p.total === 0), 'total != 0');
  }
  {
    const s = buildDailySeries([rec(0, 100), rec(0, 50)], TODAY, deps);
    check('같은 날짜가 여러 건이면 합산', s[29].total === 150, String(s[29].total));
  }
  {
    const s = buildDailySeries([{ date: TODAY, inputTokens: NaN, outputTokens: -5, cacheCreationTokens: Infinity, cacheReadTokens: 40 }], TODAY, deps);
    check('NaN/음수/Infinity 토큰은 0으로 취급(합계 40)', s[29].total === 40 && allFinite(s), String(s[29].total));
  }
  check('today가 잘못된 형식이면 빈 시계열(던지지 않음)', buildDailySeries([rec(0, 1)], 'not-a-date', deps).length === 0, 'length != 0');
  {
    const s = buildDailySeries([], '2026-03-01', deps);
    check('월 경계(윤년 아님): 2026-03-01 기준 시계열에 2026-02-28 포함, 02-29 없음', s.some((p) => p.date === '2026-02-28') && !s.some((p) => p.date === '2026-02-29'), '');
    const s2 = buildDailySeries([], '2024-03-01', deps);
    check('월 경계(윤년): 2024-03-01 기준 시계열에 2024-02-29 포함', s2.some((p) => p.date === '2024-02-29'), '');
  }

  // ---- P3-1: 주간 요약 — 데이터 길이 경계 ----
  // 경과일은 "어제까지 끝난 날"만 센다(오늘은 하루가 끝나지 않아 제외). rec(k)의 최초 사용일이
  // 오늘-k이면 경과일 = k (오늘-k ~ 어제).
  {
    const w = trendOf([]).weekly;
    check('주간: 데이터 0건 -> insufficient, spanDays 0, current null', w.status === 'insufficient' && w.spanDays === 0 && w.current === null && w.delta === null, JSON.stringify(w));
  }
  {
    const w = trendOf([rec(0, 400)]).weekly;
    check('주간: 오늘만 사용 -> 끝난 날 없음(spanDays 0, insufficient), 오늘 400은 today로만 노출', w.status === 'insufficient' && w.spanDays === 0 && w.current === null && w.today?.totalTokens === 400 && w.today.date === TODAY, JSON.stringify(w));
  }
  {
    const w = trendOf([rec(5, 100)]).weekly; // 오늘-5 ~ 어제 = 5일
    check('주간: 끝난 날 5일 -> insufficient', w.status === 'insufficient' && w.spanDays === 5, `${w.status}/${w.spanDays}`);
  }
  {
    const w = trendOf([rec(6, 100)]).weekly; // 6일
    check('주간: 끝난 날 6일 -> insufficient(오늘을 세면 7일이지만 세지 않는다)', w.status === 'insufficient' && w.spanDays === 6, `${w.status}/${w.spanDays}`);
  }
  {
    const w = trendOf([rec(7, 100)]).weekly; // 7일
    check('주간: 끝난 날 정확히 7일 -> no-baseline(최근 완료 7일만, 그 전 7일 null)', w.status === 'no-baseline' && w.spanDays === 7 && w.current?.totalTokens === 100 && w.previous === null && w.delta === null, `${w.status}/${w.spanDays}`);
  }
  {
    const w = trendOf([rec(13, 100)]).weekly; // 13일
    check('주간: 끝난 날 13일 -> 아직 no-baseline', w.status === 'no-baseline' && w.spanDays === 13, `${w.status}/${w.spanDays}`);
  }
  {
    const w = trendOf([rec(14, 100)]).weekly; // 14일
    check('주간: 끝난 날 정확히 14일 -> ok(그 전 7일과 비교)', w.status === 'ok' && w.spanDays === 14 && w.previous !== null && w.delta !== null, `${w.status}/${w.spanDays}`);
  }
  {
    // 최초 사용일 이후 하루만 0이어도 경과일은 첫 사용일 기준이다. 30일 창의 첫 날(오늘-29)부터 어제까지 29일.
    const w = trendOf([rec(29, 10), rec(1, 10)]).weekly;
    check('주간: 최초 사용일이 창의 첫 날(오늘-29)이면 spanDays=29(오늘 제외)', w.spanDays === 29 && w.status === 'ok', `${w.status}/${w.spanDays}`);
  }

  // ---- P3-1: 주 경계(어느 날이 어느 주에 들어가는가) ----
  {
    const items = [rec(0, 1), rec(1, 10), rec(7, 100), rec(8, 1000), rec(14, 10_000), rec(15, 100_000)];
    const w = trendOf(items).weekly;
    check('주 경계: 최근 완료 7일 = 오늘-1~오늘-7 (10+100), 오늘(1)은 제외', w.current?.totalTokens === 110 && w.current.startDate === day(7) && w.current.endDate === day(1), JSON.stringify(w.current));
    check('주 경계: 그 전 7일 = 오늘-8~오늘-14 (1000+10000), 오늘-15는 제외', w.previous?.totalTokens === 11_000 && w.previous.startDate === day(14) && w.previous.endDate === day(8), JSON.stringify(w.previous));
    check('주 경계: 활동일 수(최근 2, 그 전 2)', w.current?.activeDays === 2 && w.previous?.activeDays === 2, `${w.current?.activeDays}/${w.previous?.activeDays}`);
    check('주 경계: 오늘 수치는 today로 따로(날짜·값)', w.today?.date === TODAY && w.today.totalTokens === 1, JSON.stringify(w.today));
  }
  {
    // 오늘 부분일이 주간 비교(총 토큰·활동일·캐시 히트율·증감)에 들어가지 않는다.
    const base = [rec(1, 200, { cacheReadTokens: 200 }), rec(8, 100, { cacheReadTokens: 100 }), rec(14, 100)];
    const without = trendOf(base).weekly;
    const withToday = trendOf([...base, rec(0, 9_999_999, { cacheReadTokens: 5_000_000 })]).weekly;
    const strip = (w) => JSON.stringify({ status: w.status, spanDays: w.spanDays, current: w.current, previous: w.previous, delta: w.delta });
    check('오늘 부분일 격리: 오늘에 큰 값을 더해도 주간 표(상태·경과일·두 주·증감) 불변', strip(without) === strip(withToday), `${strip(without)} vs ${strip(withToday)}`);
    check('오늘 부분일 격리: 오늘 값은 today에만 반영', without.today?.totalTokens === 0 && withToday.today?.totalTokens === 14_999_999, JSON.stringify([without.today, withToday.today]));
    // 오전(오늘 아직 0)에도 부호가 뒤집히지 않는다 — 같은 완료일 데이터면 같은 증감.
    check('오늘 부분일 격리: 오늘이 0이든 큰 값이든 증감%가 같다', without.delta?.tokensPct === withToday.delta?.tokensPct && without.delta?.activeDaysAbs === withToday.delta?.activeDaysAbs, JSON.stringify([without.delta, withToday.delta]));
  }

  // ---- P3-1: 증감 계산 ----
  {
    const w = trendOf([rec(14, 100), rec(1, 150)]).weekly;
    check('증감: 100 -> 150 은 +50, +50.0%', w.delta?.tokensAbs === 50 && w.delta.tokensPct === 50, JSON.stringify(w.delta));
  }
  {
    const w = trendOf([rec(14, 3), rec(1, 4)]).weekly;
    check('증감: 3 -> 4 는 +33.3% (소수 1자리)', w.delta?.tokensPct === 33.3, String(w.delta?.tokensPct));
  }
  {
    const w = trendOf([rec(14, 100), rec(1, 0)]).weekly;
    check('증감: 최근 완료 7일 0 -> -100.0%, 절대 -100', w.delta?.tokensAbs === -100 && w.delta.tokensPct === -100, JSON.stringify(w.delta));
  }
  {
    const w = trendOf([rec(20, 500), rec(1, 70)]).weekly; // 그 전 7일(오늘-8~-14)은 비어 있음, 그 전에 사용 있음
    check('전주 0: status ok, 절대 증감은 값 그대로, % 는 null(0 나누기 없음)', w.status === 'ok' && w.previous?.totalTokens === 0 && w.delta?.tokensAbs === 70 && w.delta.tokensPct === null, JSON.stringify(w.delta));
    check('전주 0: 그 전 7일 캐시 히트율 null, 캐시 증감 null', w.previous?.cacheHitRate === null && w.delta?.cacheHitPointDiff === null, JSON.stringify(w.previous));
    check('전주 0: 결과의 모든 숫자가 유한수', allFinite(w), 'NaN/Infinity 존재');
  }
  {
    const w = trendOf([rec(20, 500)]).weekly; // 최근 14일 모두 0
    check('양주 모두 0: ok, 증감 0, % null, 유한수', w.status === 'ok' && w.delta?.tokensAbs === 0 && w.delta.tokensPct === null && allFinite(w), JSON.stringify(w.delta));
  }
  {
    // 캐시 히트율: 최근 완료 7일 read 80 / (10+10+80)=80%, 그 전 7일 read 50 / (25+25+50)=50% -> +30%p
    const w = trendOf([
      rec(1, 10, { cacheCreationTokens: 10, cacheReadTokens: 80 }),
      rec(14, 25, { cacheCreationTokens: 25, cacheReadTokens: 50 }),
    ]).weekly;
    check('캐시 히트율: 기존 정의(read/(input+creation+read)) 그대로 80% vs 50% -> +30%p', w.current?.cacheHitRate === 80 && w.previous?.cacheHitRate === 50 && w.delta?.cacheHitPointDiff === 30, JSON.stringify([w.current?.cacheHitRate, w.previous?.cacheHitRate, w.delta?.cacheHitPointDiff]));
  }
  {
    // 출력 토큰만 있는 주 -> 분모 0 -> null
    const w = trendOf([{ date: day(1), inputTokens: 0, outputTokens: 500, cacheCreationTokens: 0, cacheReadTokens: 0 }, rec(14, 20, { cacheReadTokens: 80 })]).weekly;
    check('캐시 히트율: 분모 0(출력 토큰만)이면 null, 증감도 null', w.current?.cacheHitRate === null && w.delta?.cacheHitPointDiff === null && allFinite(w), JSON.stringify(w.current));
  }
  {
    const w = trendOf([rec(14, 20, { cacheReadTokens: 80 }), rec(1, 50, { cacheReadTokens: 50 })]).weekly;
    check('캐시 히트율 하락은 음수 %p (80% -> 50% = -30)', w.delta?.cacheHitPointDiff === -30, String(w.delta?.cacheHitPointDiff));
  }

  // ---- P3-2: 이상치 ----
  // target 날(offset)에 값을 넣고, baselineOffsets(과거 offset들)에 baselineValues를 채운 뒤 그 날의 AnomalyPoint를 돌려준다.
  const probe = (targetOffset, targetValue, baselineOffsets, baselineValues) => {
    const items = [rec(targetOffset, targetValue), ...baselineOffsets.map((o, i) => rec(o, baselineValues[i % baselineValues.length]))];
    const t = trendOf(items);
    return t.anomalies.points.find((p) => p.date === day(targetOffset));
  };
  const five = [2, 3, 4, 5, 6]; // 어제 직전부터 5일 (target offset 1 기준 기준선 후보)

  check('이상치 규칙 상수: 14일/5일/3배/1/3/100,000', T.baselineLookbackDays === 14 && T.minBaselineActiveDays === 5 && T.spikeRatio === 3 && T.dropDivisor === 3 && T.minAbsDeviationTokens === 100_000, JSON.stringify(T));
  {
    const p = probe(1, 900_000, five, [300_000]);
    check('급증: 정확히 기준선 x3 (900K vs 300K) -> spike, ratio 3', p.kind === 'spike' && p.ratio === 3 && p.baselineMedian === 300_000, JSON.stringify(p));
  }
  {
    const p = probe(1, 899_999, five, [300_000]);
    check('급증 경계: 기준선 x3 바로 아래(899,999) -> normal', p.kind === 'normal', JSON.stringify(p));
  }
  {
    const p = probe(1, 60_000, five, [20_000]);
    check('급증: 3배지만 절대 편차 40K < 10만 -> normal(잡음 제외)', p.kind === 'normal', JSON.stringify(p));
  }
  {
    const p = probe(1, 140_000, five, [40_000]);
    check('급증 경계: 편차 정확히 100,000(3.5배) -> spike', p.kind === 'spike', JSON.stringify(p));
    const q = probe(1, 139_999, five, [40_000]);
    check('급증 경계: 편차 99,999 -> normal', q.kind === 'normal', JSON.stringify(q));
  }
  {
    const p = probe(1, 100_000, five, [300_000]);
    check('급락: 정확히 기준선 1/3 (100K vs 300K) -> drop', p.kind === 'drop' && p.ratio === 0.33, JSON.stringify(p));
    const q = probe(1, 100_001, five, [300_000]);
    check('급락 경계: 1/3 바로 위(100,001) -> normal', q.kind === 'normal', JSON.stringify(q));
  }
  {
    const p = probe(1, 50_000, five, [150_000]);
    check('급락 경계: 편차 정확히 100,000 (150K -> 50K) -> drop', p.kind === 'drop', JSON.stringify(p));
    const q = probe(1, 40_000, five, [120_000]);
    check('급락: 1/3이지만 편차 80K < 10만 -> normal', q.kind === 'normal', JSON.stringify(q));
  }
  {
    const p = probe(1, 300_000, five, [300_000]);
    check('평소 수준(기준선과 같음) -> normal, ratio 1', p.kind === 'normal' && p.ratio === 1, JSON.stringify(p));
  }
  {
    // 판정 가능한 날이 오늘뿐 + 오늘은 급증이 아님: judgedCount 0이어도 오늘은 today-partial이고(기준선 부족이 아님),
    // 어제까지는 insufficient-baseline이다. 뷰가 이 조합을 "기준선 부족"과 구분해 문구를 가른다.
    const t = trendOf([1, 2, 3, 4, 5, 0].map((o) => rec(o, 300_000)));
    const pts = t.anomalies.points;
    const todayPt = pts[pts.length - 1];
    const past = pts.filter((x) => x.date !== TODAY && x.total > 0);
    check('판정 가능일이 오늘뿐: judgedCount 0, 오늘은 today-partial(기준선 5일 보유)', t.anomalies.judgedCount === 0 && todayPt.date === TODAY && todayPt.reason === 'today-partial' && todayPt.baselineDays === 5, JSON.stringify(todayPt));
    check('판정 가능일이 오늘뿐: 어제까지 사용한 날은 모두 insufficient-baseline', past.length === 5 && past.every((x) => x.reason === 'insufficient-baseline'), JSON.stringify(past.map((x) => x.reason)));
  }
  {
    const t = trendOf([rec(1, 500_000)]);
    const p = t.anomalies.points.find((x) => x.date === day(1));
    check('사용 없는 날은 판정하지 않음(zero-day), 사용 있는 날도 기준선 없으면 unjudged', t.anomalies.points[0].reason === 'zero-day' && p.kind === 'unjudged' && p.reason === 'insufficient-baseline', JSON.stringify(p));
    check('데이터 부족: judgedCount 0, anomalies 비어 있음', t.anomalies.judgedCount === 0 && t.anomalies.anomalies.length === 0, JSON.stringify(t.anomalies.judgedCount));
  }
  {
    const p = probe(1, 900_000, [2, 3, 4, 5], [300_000]); // 기준선 4일
    check('기준선 사용일 4일 -> unjudged(insufficient-baseline), baselineDays 4', p.kind === 'unjudged' && p.reason === 'insufficient-baseline' && p.baselineDays === 4, JSON.stringify(p));
    const q = probe(1, 900_000, five, [300_000]); // 5일
    check('기준선 사용일 정확히 5일 -> 판정함', q.kind === 'spike' && q.baselineDays === 5, JSON.stringify(q));
  }
  {
    // target offset 0, lookback 14: offset 14가 포함, 15는 제외. 기준선 4일(1..4) + offset 14 = 5일 -> 판정.
    const inc = probe(0, 900_000, [1, 2, 3, 4, 14], [300_000]);
    check('룩백 경계: 14일 전 사용일은 기준선에 포함', inc.baselineDays === 5 && inc.kind === 'spike', JSON.stringify(inc));
    const exc = probe(0, 900_000, [1, 2, 3, 4, 15], [300_000]);
    check('룩백 경계: 15일 전 사용일은 기준선에서 제외(4일 -> 판정 안 함)', exc.baselineDays === 4 && exc.kind === 'unjudged', JSON.stringify(exc));
  }
  {
    // 사용 없는 날은 기준선에서 빠진다: 5일 사용 + 9일 0이어도 중앙값은 300K.
    const p = probe(0, 900_000, [1, 2, 3, 4, 5], [300_000]);
    check('0인 날은 기준선에서 제외(주말·휴무로 기준선이 0 근처로 내려가지 않음)', p.baselineMedian === 300_000 && p.baselineDays === 5, JSON.stringify(p));
  }
  {
    const p = probe(0, 300_000, [1, 2, 3, 4, 5], [100_000, 100_000, 100_000, 100_000, 5_000_000]);
    check('중앙값: 직전 급증(5M)이 있어도 기준선은 100K 유지 -> 300K는 spike', p.baselineMedian === 100_000 && p.kind === 'spike', JSON.stringify(p));
  }
  {
    const p = probe(0, 2_000_000, [1, 2, 3, 4, 5, 6], [100_000, 200_000, 300_000, 400_000, 500_000, 600_000]);
    check('중앙값: 짝수 개(6일)는 가운데 두 값의 평균 350K', p.baselineMedian === 350_000, JSON.stringify(p));
  }
  {
    // 오늘은 급증만 판정한다. 급락은 하루가 끝나지 않아, 평소 수준은 '기준 이내'를 단정할 수 없어 판정하지 않는다.
    const drop = probe(0, 50_000, five.map((x) => x - 1), [300_000]);
    check('오늘의 급락은 판정하지 않음(today-partial)', drop.kind === 'unjudged' && drop.reason === 'today-partial', JSON.stringify(drop));
    const spike = probe(0, 900_000, five.map((x) => x - 1), [300_000]);
    check('오늘의 급증은 판정함', spike.kind === 'spike', JSON.stringify(spike));
    const normal = probe(0, 300_000, five.map((x) => x - 1), [300_000]);
    check('오늘이 평소 수준이면 normal이 아니라 unjudged(today-partial) — "기준 이내"로 단정하지 않음', normal.kind === 'unjudged' && normal.reason === 'today-partial' && normal.baselineMedian === 300_000, JSON.stringify(normal));
  }
  {
    // 오늘의 normal(=today-partial)은 judgedCount(판정한 날 수)에 들어가지 않는다.
    const base = [1, 2, 3, 4, 5, 6, 7].map((o) => rec(o, 300_000));
    const without = trendOf(base).anomalies;
    const withToday = trendOf([...base, rec(0, 300_000)]).anomalies;
    const withTodaySpike = trendOf([...base, rec(0, 900_000)]).anomalies;
    check('judgedCount: 오늘이 평소 수준이면 판정한 날 수가 늘지 않음', withToday.judgedCount === without.judgedCount && without.judgedCount > 0, `${without.judgedCount} vs ${withToday.judgedCount}`);
    check('judgedCount: 오늘 급증은 판정한 날 수에 들어감', withTodaySpike.judgedCount === without.judgedCount + 1 && withTodaySpike.anomalies[0]?.date === TODAY, `${without.judgedCount} vs ${withTodaySpike.judgedCount}`);
  }
  {
    const t = trendOf([rec(1, 900_000), ...five.map((o) => rec(o, 300_000)), rec(0, 50_000)]);
    check('anomalies 목록: spike/drop만, 최근 날짜 먼저', t.anomalies.anomalies.length >= 1 && t.anomalies.anomalies.every((p) => p.kind === 'spike' || p.kind === 'drop') && t.anomalies.anomalies[0].date >= t.anomalies.anomalies[t.anomalies.anomalies.length - 1].date, JSON.stringify(t.anomalies.anomalies.map((p) => p.date)));
    check('points는 series와 같은 길이·순서', t.anomalies.points.length === t.series.length && t.anomalies.points.every((p, i) => p.date === t.series[i].date), '');
  }
  {
    const t = trendOf([]);
    check('빈 입력: 시계열 30일, 이상치 없음, judgedCount 0, 유한수', t.series.length === 30 && t.anomalies.anomalies.length === 0 && t.anomalies.judgedCount === 0 && allFinite(t), '');
  }
  {
    // 무작위에 가까운 고정 패턴으로 전체 결과에 NaN/Infinity가 없는지(스모크).
    const items = [];
    for (let i = 0; i < 30; i++) if (i % 3 !== 0) items.push(rec(i, ((i * 7919) % 900_000) + (i % 5 === 0 ? 0 : 1000), { cacheReadTokens: (i * 31) % 1000 }));
    const t = trendOf(items);
    check('스모크: 30일 임의 패턴 결과에 NaN/Infinity 없음', allFinite(t), 'NaN/Infinity 존재');
  }

  return { total, failures, rows };
}
