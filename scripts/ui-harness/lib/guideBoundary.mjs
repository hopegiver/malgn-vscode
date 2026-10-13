// 효율 가이드(src/efficiencyGuide.ts) 규칙 판정 순수 함수의 경계값 검사.
//
// runGuideBoundaryChecks(mod)는 DOM/클로저 없이 mod(efficiencyGuide 모듈)만 받는다.
//   1) 하네스 flows/usageGuide.mjs boundary 시나리오: Vite dev 서버가 서빙하는 실제
//      모듈(/src/efficiencyGuide.ts 변환 결과)을 받아 Node에서 로드해 실행
//   2) node 단독(Node 22.6+):
//        node --experimental-strip-types -e "
//          const m = await import('./src/efficiencyGuide.ts');
//          const { runGuideBoundaryChecks } = await import('./scripts/ui-harness/lib/guideBoundary.mjs');
//          const r = runGuideBoundaryChecks(m); console.log(r.rows.join('\n'), r.failures.length);"
// 반환: { total, failures: string[], rows: string[] } (rows는 케이스별 PASS/FAIL 한 줄)
export function runGuideBoundaryChecks(mod) {
  const { evaluateEfficiencyGuide, GUIDE_THRESHOLDS: T } = mod;
  const rows = [];
  const failures = [];
  let total = 0;
  const check = (name, cond, detail) => {
    total += 1;
    const line = `${cond ? 'PASS' : 'FAIL'} ${name}${cond ? '' : ` — ${detail}`}`;
    rows.push(line);
    if (!cond) failures.push(line);
  };
  const ids = (r) => r.findings.map((f) => f.ruleId).join(',');
  const skippedIds = (r) => r.skipped.map((s) => s.ruleId).join(',');

  // 모든 규칙이 "판정 가능 + 기준 이내"인 기준 입력. 케이스마다 필요한 필드만 덮어쓴다.
  // 캐시 분모 sample = input+creation+read = 1,000,000 (≥ minSampleTokens).
  const okTotals = {
    totalTokens: 1_000_000,
    activeDays: 10,
    cacheHitRate: 80,
    inputTokens: 100_000,
    cacheCreationTokens: 100_000,
    cacheReadTokens: 800_000,
    peakDay: { date: '2026-10-01', total: 100_000 }, // 10%
  };
  const okModels = [
    { modelId: 'claude-sonnet-5', displayName: 'Sonnet 5', tokens: 900_000 },
    { modelId: 'claude-opus-5-5', displayName: 'Opus 5.5', tokens: 100_000 }, // 10%
  ];
  const run = (over = {}, models = okModels) => evaluateEfficiencyGuide({ ...okTotals, ...over }, models);

  // 기준선
  const base = run();
  check('baseline: 전 규칙 판정 + 기준 이내 → clear', base.status === 'clear' && base.checkedCount === 3 && base.findings.length === 0, `status=${base.status} checked=${base.checkedCount} findings=${ids(base)}`);

  // ---- 캐시 히트율: 기준 미만(<50) ----
  check('cache 49% → finding', ids(run({ cacheHitRate: T.cacheHitLowBelowPct - 1 })).includes('cache-hit-low'), ids(run({ cacheHitRate: 49 })));
  check('cache 50% (기준 정확히) → 경고 없음', !ids(run({ cacheHitRate: T.cacheHitLowBelowPct })).includes('cache-hit-low'), 'fired at 50');
  check('cache 51% → 경고 없음', !ids(run({ cacheHitRate: T.cacheHitLowBelowPct + 1 })).includes('cache-hit-low'), 'fired at 51');
  check('cache 0% (표본 충분) → finding', ids(run({ cacheHitRate: 0 })).includes('cache-hit-low'), ids(run({ cacheHitRate: 0 })));
  // 표본 하한(분모 = input+creation+read)
  const sampleAt = (n) => run({ cacheHitRate: 10, inputTokens: n, cacheCreationTokens: 0, cacheReadTokens: 0 });
  check(`cache 표본 ${T.minSampleTokens - 1} → 보류`, skippedIds(sampleAt(T.minSampleTokens - 1)).includes('cache-hit-low') && !ids(sampleAt(T.minSampleTokens - 1)).includes('cache-hit-low'), skippedIds(sampleAt(T.minSampleTokens - 1)));
  check(`cache 표본 ${T.minSampleTokens} → 판정`, ids(sampleAt(T.minSampleTokens)).includes('cache-hit-low'), ids(sampleAt(T.minSampleTokens)));
  // 활동일 하한
  check(`cache 활동일 ${T.minActiveDays - 1} → 보류`, skippedIds(run({ cacheHitRate: 10, activeDays: T.minActiveDays - 1 })).includes('cache-hit-low'), '');
  check(`cache 활동일 ${T.minActiveDays} → 판정`, ids(run({ cacheHitRate: 10, activeDays: T.minActiveDays })).includes('cache-hit-low'), '');
  // 0으로 나누기: 히트율 null
  check('cache 히트율 null → 보류(0 나누기 안전)', skippedIds(run({ cacheHitRate: null })).includes('cache-hit-low'), '');

  // ---- 하루 쏠림: 최고 사용일 비중 ≥ 30% (totalTokens 1,000,000 기준) ----
  const peakAt = (total, over = {}) => run({ peakDay: { date: '2026-10-01', total }, ...over });
  check('peak 29% (294,000) → 경고 없음', !ids(peakAt(294_000)).includes('peak-day'), ids(peakAt(294_000)));
  check('peak 30% (300,000, 기준 정확히) → finding', ids(peakAt(300_000)).includes('peak-day-concentration'), ids(peakAt(300_000)));
  check('peak 31% → finding', ids(peakAt(310_000)).includes('peak-day-concentration'), '');
  check(`peak 활동일 ${T.peakDayMinActiveDays - 1} → 보류`, skippedIds(peakAt(500_000, { activeDays: T.peakDayMinActiveDays - 1 })).includes('peak-day-concentration'), '');
  check(`peak 활동일 ${T.peakDayMinActiveDays} → 판정`, ids(peakAt(500_000, { activeDays: T.peakDayMinActiveDays })).includes('peak-day-concentration'), '');
  check(`peak 총토큰 ${T.minSampleTokens - 1} → 보류`, skippedIds(peakAt(60_000, { totalTokens: T.minSampleTokens - 1 })).includes('peak-day-concentration'), '');
  check(`peak 총토큰 ${T.minSampleTokens} → 판정`, ids(peakAt(60_000, { totalTokens: T.minSampleTokens })).includes('peak-day-concentration'), '');
  check('peak peakDay null → 보류', skippedIds(run({ peakDay: null })).includes('peak-day-concentration'), '');
  check('peak 총토큰 0 → 보류(0 나누기 안전)', skippedIds(run({ totalTokens: 0, peakDay: { date: '2026-10-01', total: 0 } })).includes('peak-day-concentration'), '');

  // ---- 대형 모델 비중: Opus 계열 ≥ 70% (모델 합 1,000,000 기준) ----
  const mix = (opusTokens, rest = 1_000_000 - opusTokens) => [
    { modelId: 'claude-opus-5-5', displayName: 'Opus 5.5', tokens: opusTokens },
    { modelId: 'claude-sonnet-5', displayName: 'Sonnet 5', tokens: rest },
  ];
  check('model Opus 69% → 경고 없음', !ids(run({}, mix(690_000))).includes('heavy-model'), '');
  check('model Opus 70% (기준 정확히) → finding', ids(run({}, mix(700_000))).includes('heavy-model-share'), '');
  check('model Opus 71% → finding', ids(run({}, mix(710_000))).includes('heavy-model-share'), '');
  check(`model 표본 ${T.minSampleTokens - 1} → 보류`, skippedIds(run({}, mix(99_000, 999))).includes('heavy-model-share'), '');
  check(`model 표본 ${T.minSampleTokens} → 판정`, ids(run({}, mix(100_000, 0))).includes('heavy-model-share'), '');
  check('model 요약 null(로딩/실패) → 보류, 다른 규칙은 판정', skippedIds(run({}, null)) === 'heavy-model-share' && run({}, null).checkedCount === 2, `${skippedIds(run({}, null))}/${run({}, null).checkedCount}`);
  check('model 빈 배열 → 보류(0 나누기 안전)', skippedIds(run({}, [])).includes('heavy-model-share'), '');
  check('model Opus 없음(Sonnet 100%) → 경고 없음', !ids(run({}, [{ modelId: 'claude-sonnet-5', displayName: 'Sonnet 5', tokens: 1_000_000 }])).includes('heavy-model'), '');
  check('model Opus 여러 버전 합산(40%+35%=75%) → finding', ids(run({}, [
    { modelId: 'claude-opus-5-5', displayName: 'Opus 5.5', tokens: 400_000 },
    { modelId: 'claude-opus-5', displayName: 'Opus 5', tokens: 350_000 },
    { modelId: 'claude-sonnet-5', displayName: 'Sonnet 5', tokens: 250_000 },
  ])).includes('heavy-model-share'), '');

  // ---- 전체 상태 구분(insufficient ≠ clear) ----
  const empty = evaluateEfficiencyGuide(
    { totalTokens: 0, activeDays: 0, cacheHitRate: null, inputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0, peakDay: null },
    []
  );
  check('빈 데이터(전부 0/null) → insufficient, 경고 0, 보류 3', empty.status === 'insufficient' && empty.findings.length === 0 && empty.skipped.length === 3 && empty.checkedCount === 0, `${empty.status}/${empty.skipped.length}`);
  const tiny = run({ totalTokens: 5_000, activeDays: 2, cacheHitRate: 5, inputTokens: 4_000, cacheCreationTokens: 500, cacheReadTokens: 500, peakDay: { date: '2026-10-01', total: 4_000 } }, null);
  check('소표본(활동 2일·5K 토큰·히트율 5%) → insufficient(경고 만들지 않음)', tiny.status === 'insufficient' && tiny.findings.length === 0, `${tiny.status}/${ids(tiny)}`);
  const all = run({ cacheHitRate: 10, peakDay: { date: '2026-10-01', total: 500_000 } }, mix(800_000));
  check('3규칙 모두 초과 → findings 3건, 각 finding에 근거 수치+기준(threshold)과 action', all.status === 'findings' && all.findings.length === 3 && all.findings.every((f) => f.evidence.length > 0 && f.evidence.every((e) => e.value && e.threshold) && f.action && f.observation), `${all.status}/${all.findings.length}`);
  check('finding 문구에 인과 단정("때문") 없음', all.findings.every((f) => !/때문/.test(f.observation + f.action)), '');
  // RV-004: 캐시 카드와 쏠림 카드가 함께 뜰 때 /compact·세션 분할 조언이 서로 반대로 읽히지 않는다 —
  // /compact를 권하는 문장("/compact로 정리")이 어느 카드에도 없고, 두 카드 모두 /compact를
  // 캐시 재사용과 맞바꾸는 선택으로만 언급한다.
  const cacheAction = all.findings.find((f) => f.ruleId === 'cache-hit-low')?.action ?? '';
  const peakAction = all.findings.find((f) => f.ruleId === 'peak-day-concentration')?.action ?? '';
  check('캐시·쏠림 카드가 함께 떠도 /compact 조언이 모순되지 않음', !/\/compact로 (정리|압축)/.test(cacheAction + peakAction) && /캐시 재사용이 줄어들/.test(cacheAction) && /캐시 재사용이 줄어들/.test(peakAction) && /무관한 작업/.test(cacheAction) && /무관한 작업/.test(peakAction), `${cacheAction} | ${peakAction}`);
  check('finding 문구에 달러($) 없음(비용 미사용)', all.findings.every((f) => !/\$/.test(f.observation + f.action + f.evidence.map((e) => e.value).join(''))), '');

  return { total, failures, rows };
}
