// 사용량 화면 "효율 가이드"(WBS P2-7 1차) — 규칙 판정 순수 함수.
//
// 원칙(바꾸기 전에 읽을 것):
//   1. 입력은 사용량 화면이 이미 계산해 보여주는 수치(UsageTotals — 캐시 히트율·
//      최고 사용일·총 토큰, UsageSummaryReport.models — 모델별 토큰)뿐이다. 새
//      수집·새 로그 파싱·비용 계산을 하지 않는다. 비용(달러)은 판정에 쓰지 않는다.
//   2. 관측 사실 + 일반적으로 효과 있는 행동 제안만 낸다. "~때문입니다" 같은 인과
//      단정은 하지 않는다(이 코드는 원인을 모른다).
//   3. 표본이 작으면 가이드를 내지 않는다 — 규칙마다 "판단 가능한가"를 먼저 보고,
//      불가능하면 skipped에 사유를 남긴다. 모든 규칙이 보류면 status='insufficient'
//      (= "문제 없음"이 아니라 "판단할 데이터가 부족함")로 "clear"와 구분한다.
//   4. DOM/Tauri에 의존하지 않는다 — node/하네스에서 그대로 import해 경계값을 검증한다.
//   5. 임계값은 이 파일 맨 위 GUIDE_THRESHOLDS 한 곳에만 둔다.

// ---------------- 임계값(한 곳, 근거 주석) ----------------

export const GUIDE_THRESHOLDS = {
  // 표본 하한 — 이 미만이면 비율 규칙을 판정하지 않는다. 활동일 3일 미만이거나
  // 토큰 10만 미만(Claude Code 한두 번의 짧은 대화 수준)이면 비율이 우연에 크게
  // 흔들린다. 값 자체는 경험적 보수치(통계적 유의성 계산이 아님).
  minActiveDays: 3,
  minSampleTokens: 100_000,

  // 캐시 히트율(%) 미만이면 점검 권장. 50%는 malgn-agent 플러그인
  // token-usage-diagnosis(bin/analyze-usage.mjs)의 "캐시 히트율<50%" 규칙과
  // 같은 값이다 — 앱과 CLI 진단이 같은 기준으로 말하도록 맞췄다. 히트율은 화면
  // 타일과 같은 정수(%)이며 이 정수로 비교한다(타일 "50%"인데 "50% 미만" 경고가
  // 뜨는 모순 방지).
  cacheHitLowBelowPct: 50,

  // 최고 사용일이 30일 총 토큰의 이 비율(%) 이상이면 "하루 쏠림" 관측. 활동일 7일
  // 이상일 때만 판정한다(활동일이 적으면 하루 비중이 구조적으로 커진다 — 예:
  // 3일 중 하루는 평균이 이미 33%). 30%는 균등 분포(활동일 N일이면 1/N)보다
  // 확연히 큰 값으로 잡은 경험적 보수치다.
  peakDayShareAtLeastPct: 30,
  peakDayMinActiveDays: 7,

  // Opus 계열 모델이 모델별 토큰 합의 이 비율(%) 이상이면 "대형 모델 편중" 관측.
  // 경험적 보수치 — 조직 표준이 아니다. 판정은 토큰 비중만 쓰며 비용은 쓰지 않는다.
  heavyModelShareAtLeastPct: 70,
} as const;

// ---------------- 입력/출력 타입 ----------------

// UsageTotals(views/usage.ts)의 부분집합 — 구조적 타이핑으로 그대로 넘길 수 있다.
export interface GuideTotals {
  readonly totalTokens: number;
  readonly activeDays: number;
  readonly cacheHitRate: number | null; // 화면 타일과 같은 정수(%), 분모 0이면 null
  readonly inputTokens: number;
  readonly cacheCreationTokens: number;
  readonly cacheReadTokens: number;
  readonly peakDay: { readonly date: string; readonly total: number } | null;
}

// ModelUsageSummary(usageSummaryApi.ts)의 부분집합.
export interface GuideModel {
  readonly modelId: string;
  readonly displayName: string;
  readonly tokens: number;
}

export type GuideRuleId = 'cache-hit-low' | 'peak-day-concentration' | 'heavy-model-share';

export interface GuideEvidence {
  readonly label: string;
  readonly value: string;
  readonly threshold: string;
}

export interface GuideFinding {
  readonly ruleId: GuideRuleId;
  readonly ruleName: string;
  readonly observation: string; // 관측 사실만(원인 단정 금지)
  readonly evidence: readonly GuideEvidence[]; // 근거 수치(값 + 기준 임계값)
  readonly action: string; // 일반적으로 효과 있는 행동 제안
}

export interface GuideSkipped {
  readonly ruleId: GuideRuleId;
  readonly ruleName: string;
  readonly reason: string; // 왜 판정을 보류했는가(표본 부족 등)
}

export interface GuideResult {
  // insufficient: 판정 가능한 규칙이 하나도 없음("문제 없음"이 아니다)
  // clear: 판정 가능한 규칙이 있었고 모두 기준 이내
  // findings: 한 개 이상 기준 초과
  readonly status: 'insufficient' | 'clear' | 'findings';
  readonly findings: readonly GuideFinding[];
  readonly skipped: readonly GuideSkipped[];
  readonly checkedCount: number; // 판정이 실제로 수행된 규칙 수
}

export type TokenFormatter = (n: number) => string;

const defaultFormat: TokenFormatter = (n) => n.toLocaleString('ko-KR');

const RULE_NAMES: Record<GuideRuleId, string> = {
  'cache-hit-low': '캐시 히트율',
  'peak-day-concentration': '하루 사용량 쏠림',
  'heavy-model-share': '대형 모델 비중',
};

function pct(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 100) : 0;
}

function monthDay(dateKey: string): string {
  const parts = dateKey.split('-');
  return parts.length === 3 ? `${parts[1]}/${parts[2]}` : dateKey;
}

function isOpus(m: GuideModel): boolean {
  return /opus/i.test(m.modelId) || /opus/i.test(m.displayName);
}

type RuleOutcome = { readonly kind: 'skipped'; readonly reason: string } | { readonly kind: 'ok' } | { readonly kind: 'finding'; readonly finding: GuideFinding };

// ---------------- 규칙 ----------------

function ruleCacheHitLow(t: GuideTotals, fmt: TokenFormatter): RuleOutcome {
  const T = GUIDE_THRESHOLDS;
  const sample = t.inputTokens + t.cacheCreationTokens + t.cacheReadTokens; // 캐시 히트율 분모와 동일 정의
  if (t.cacheHitRate === null) return { kind: 'skipped', reason: '캐시 히트율을 계산할 토큰이 없습니다' };
  if (t.activeDays < T.minActiveDays) return { kind: 'skipped', reason: `활동일 ${t.activeDays}일 — ${T.minActiveDays}일 이상부터 판단합니다` };
  if (sample < T.minSampleTokens) return { kind: 'skipped', reason: `표본 ${fmt(sample)} 토큰 — ${fmt(T.minSampleTokens)} 토큰 이상부터 판단합니다` };
  if (t.cacheHitRate >= T.cacheHitLowBelowPct) return { kind: 'ok' };
  return {
    kind: 'finding',
    finding: {
      ruleId: 'cache-hit-low',
      ruleName: RULE_NAMES['cache-hit-low'],
      observation: `최근 30일 캐시 히트율이 ${t.cacheHitRate}%입니다 (입력 토큰 중 캐시에서 재사용된 비율).`,
      evidence: [
        { label: '캐시 히트율', value: `${t.cacheHitRate}%`, threshold: `${T.cacheHitLowBelowPct}% 미만` },
        { label: '표본', value: `${fmt(sample)} 토큰 · 활동 ${t.activeDays}일`, threshold: `${fmt(T.minSampleTokens)} 토큰 · ${T.minActiveDays}일 이상` },
      ],
      action: '관련된 작업은 새 세션을 여는 대신 같은 세션에서 이어가고, 서로 무관한 작업일 때만 세션을 나누세요. /compact·/clear를 자주 쓰면 캐시 재사용이 줄어들 수 있습니다.',
    },
  };
}

function rulePeakDay(t: GuideTotals, fmt: TokenFormatter): RuleOutcome {
  const T = GUIDE_THRESHOLDS;
  if (!t.peakDay) return { kind: 'skipped', reason: '최고 사용일 데이터가 없습니다' };
  if (t.activeDays < T.peakDayMinActiveDays) return { kind: 'skipped', reason: `활동일 ${t.activeDays}일 — ${T.peakDayMinActiveDays}일 이상부터 판단합니다` };
  if (t.totalTokens < T.minSampleTokens) return { kind: 'skipped', reason: `표본 ${fmt(t.totalTokens)} 토큰 — ${fmt(T.minSampleTokens)} 토큰 이상부터 판단합니다` };
  const share = pct(t.peakDay.total, t.totalTokens);
  if (share < T.peakDayShareAtLeastPct) return { kind: 'ok' };
  const day = monthDay(t.peakDay.date);
  return {
    kind: 'finding',
    finding: {
      ruleId: 'peak-day-concentration',
      ruleName: RULE_NAMES['peak-day-concentration'],
      observation: `최고 사용일(${day}) 하루가 최근 30일 총 토큰의 ${share}%를 차지합니다.`,
      evidence: [
        { label: `최고 사용일 ${day} 비중`, value: `${share}% (${fmt(t.peakDay.total)} / ${fmt(t.totalTokens)} 토큰)`, threshold: `${T.peakDayShareAtLeastPct}% 이상` },
        { label: '활동일', value: `${t.activeDays}일`, threshold: `${T.peakDayMinActiveDays}일 이상` },
      ],
      action: '아래 일별 목록에서 그 날짜를 눌러 세션별 사용량을 확인해 보세요. 한 세션에 몰려 있다면 작업 단위로 세션을 나누거나 중간에 /compact로 정리하는 것이 일반적으로 도움이 됩니다.',
    },
  };
}

function ruleHeavyModel(models: readonly GuideModel[] | null, fmt: TokenFormatter): RuleOutcome {
  const T = GUIDE_THRESHOLDS;
  if (!models) return { kind: 'skipped', reason: '모델별 30일 요약을 불러오지 못했거나 아직 불러오는 중입니다' };
  const total = models.reduce((sum, m) => sum + m.tokens, 0);
  if (total < T.minSampleTokens) return { kind: 'skipped', reason: `모델별 표본 ${fmt(total)} 토큰 — ${fmt(T.minSampleTokens)} 토큰 이상부터 판단합니다` };
  const opus = models.filter(isOpus);
  const opusTokens = opus.reduce((sum, m) => sum + m.tokens, 0);
  const share = pct(opusTokens, total);
  if (share < T.heavyModelShareAtLeastPct) return { kind: 'ok' };
  return {
    kind: 'finding',
    finding: {
      ruleId: 'heavy-model-share',
      ruleName: RULE_NAMES['heavy-model-share'],
      observation: `최근 30일 모델별 토큰의 ${share}%가 Opus 계열(${opus.map((m) => m.displayName).join(', ')})입니다.`,
      evidence: [
        { label: 'Opus 계열 토큰 비중', value: `${share}% (${fmt(opusTokens)} / ${fmt(total)} 토큰)`, threshold: `${T.heavyModelShareAtLeastPct}% 이상` },
      ],
      action: '파일 검색·단순 수정·포맷 변경처럼 가벼운 작업은 /model로 더 가벼운 모델로 바꿔 쓰는 것을 고려해 보세요. 어려운 설계·디버깅에는 지금처럼 큰 모델이 맞을 수 있습니다.',
    },
  };
}

// ---------------- 진입점 ----------------

/**
 * @param totals  computeUsageTotals() 결과(화면 통계 타일과 동일한 값). 활동 기록이
 *                없으면 호출하지 않는 것이 보통이나, 호출돼도 전 규칙이 보류된다.
 * @param models  UsageSummaryReport.models. 요약이 없거나 로딩 중이면 null — 모델
 *                규칙만 보류되고 나머지 규칙은 그대로 판정한다.
 * @param fmt     토큰 수 표기 함수(화면의 formatTokenCount를 주입해 표기를 맞춘다).
 */
export function evaluateEfficiencyGuide(totals: GuideTotals, models: readonly GuideModel[] | null, fmt: TokenFormatter = defaultFormat): GuideResult {
  const outcomes: ReadonlyArray<readonly [GuideRuleId, RuleOutcome]> = [
    ['cache-hit-low', ruleCacheHitLow(totals, fmt)],
    ['peak-day-concentration', rulePeakDay(totals, fmt)],
    ['heavy-model-share', ruleHeavyModel(models, fmt)],
  ];
  const findings: GuideFinding[] = [];
  const skipped: GuideSkipped[] = [];
  let checkedCount = 0;
  for (const [ruleId, o] of outcomes) {
    if (o.kind === 'skipped') {
      skipped.push({ ruleId, ruleName: RULE_NAMES[ruleId], reason: o.reason });
      continue;
    }
    checkedCount += 1;
    if (o.kind === 'finding') findings.push(o.finding);
  }
  const status = checkedCount === 0 ? 'insufficient' : findings.length > 0 ? 'findings' : 'clear';
  return { status, findings, skipped, checkedCount };
}
