// 흐름 — 사용량 화면 "효율 가이드"(WBS P2-7) — src/efficiencyGuide.ts(순수 규칙 판정) +
// src/views/usage.ts renderEfficiencyGuide(daily 탭).
//
// 검증 범위: TS/DOM/CSS 층만(하네스 공통 한계). 합성 픽스처로 규칙 충족/미충족/
// 표본 부족/요약 실패를 각각 만들고 화면에 무엇이 나오는지 본다. 순수 함수 경계값은
// lib/guideBoundary.mjs를 Vite가 서빙하는 실제 모듈에 대해 실행한다(boundary 시나리오).
import { usageSummarySample } from '../lib/fixtures.mjs';
import { runGuideBoundaryChecks } from '../lib/guideBoundary.mjs';

export const flowId = 'usageGuide';
export const startHash = '#/usage';

function dayKey(offset) {
  return new Date(Date.now() - offset * 86400_000).toISOString().slice(0, 10);
}

// items: [{ input, output, creation, read }] — 0번째가 가장 최근.
function daily(items) {
  return items.map((v, i) => ({ date: dayKey(i), inputTokens: v.input, outputTokens: v.output, cacheCreationTokens: v.creation, cacheReadTokens: v.read, costUsd: 0, unpricedTokens: 0, toolCalls: 0, sessionCount: 0 }));
}

function summaryWithModels(models) {
  const total = models.reduce((s, m) => s + m.tokens, 0);
  return { ...usageSummarySample(), totalTokens: total, unpricedTokens: 0, models };
}

const SONNET = (tokens) => ({ modelId: 'claude-sonnet-5', displayName: 'Sonnet 5', tokens, costUsd: 1, pricingMatched: true });
const OPUS = (tokens) => ({ modelId: 'claude-opus-5-5', displayName: 'Opus 5.5', tokens, costUsd: 1, pricingMatched: true });

// 3규칙 전부 초과: 10일, 최고 사용일 1일(520K)+나머지 9일(각 75K) → 하루 비중 44%,
// 캐시 히트율 25%(read 280K / 1.13M), Opus 80%.
function triggeredDaily() {
  const peak = { input: 300_000, output: 20_000, creation: 100_000, read: 100_000 };
  const rest = { input: 30_000, output: 5_000, creation: 20_000, read: 20_000 };
  return daily([peak, ...Array.from({ length: 9 }, () => rest)]);
}

// 전부 기준 이내: 10일 균등(히트율 84%, 하루 비중 10%), Opus 20%.
function clearDaily() {
  const d = { input: 5_000, output: 5_000, creation: 10_000, read: 80_000 };
  return daily(Array.from({ length: 10 }, () => d));
}

// 표본 부족: 2일·합계 약 8K 토큰, 모델 합 5K.
function tinyDaily() {
  const d = { input: 1_000, output: 500, creation: 1_000, read: 1_500 };
  return daily([d, d]);
}

async function text(page, selector) {
  return page.locator(selector).first().textContent().catch(() => null);
}

export function scenarios(base) {
  return [
    {
      id: 'triggered',
      fixtures: { ...base, get_daily_usage: triggeredDaily(), get_usage_summary: summaryWithModels([OPUS(2_800_000), SONNET(700_000)]) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-with-guide');
        const status = await page.locator('.guide-box').getAttribute('data-guide-status').catch(() => null);
        if (status !== 'findings') {
          bugs.push({ severity: 'Critical', symptom: `규칙 3개 충족 픽스처인데 가이드 상태가 findings가 아님(실제: ${status})`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
          return;
        }
        const rules = await page.locator('.guide-card').evaluateAll((els) => els.map((e) => e.getAttribute('data-rule')));
        for (const want of ['cache-hit-low', 'peak-day-concentration', 'heavy-model-share']) {
          if (!rules.includes(want)) bugs.push({ severity: 'Major', symptom: `충족된 규칙 카드 누락: ${want} (실제 ${rules.join(',')})`, file: 'src/efficiencyGuide.ts' });
        }
        // 근거 수치(값 + 기준)가 카드 안에 같이 보이는가 — 값은 화면 타일과 같은 정수여야 한다.
        const tileText = await page.locator('.stat-row').allTextContents().then((a) => a.join(' '));
        const cacheCard = await text(page, '.guide-card[data-rule="cache-hit-low"]');
        const cacheTile = tileText.match(/캐시 히트율\s*(\d+)%/);
        if (!cacheCard || !cacheTile || !cacheCard.includes(`${cacheTile[1]}%`) || !cacheCard.includes('기준 50% 미만')) {
          bugs.push({ severity: 'Major', symptom: `캐시 카드가 타일 값(${cacheTile?.[1]}%)과 기준(기준 50% 미만)을 함께 보여주지 않음: ${cacheCard}`, file: 'src/views/usage.ts:guideFindingCard' });
        }
        const peakCard = await text(page, '.guide-card[data-rule="peak-day-concentration"]');
        if (!peakCard || !/\d+%/.test(peakCard) || !peakCard.includes('기준 30% 이상')) {
          bugs.push({ severity: 'Major', symptom: `하루 쏠림 카드에 비중(%)·기준(30% 이상)이 없음: ${peakCard}`, file: 'src/views/usage.ts:guideFindingCard' });
        }
        const modelCard = await text(page, '.guide-card[data-rule="heavy-model-share"]');
        if (!modelCard || !modelCard.includes('80%') || !modelCard.includes('기준 70% 이상')) {
          bugs.push({ severity: 'Major', symptom: `대형 모델 카드에 비중 80%·기준(70% 이상)이 없음: ${modelCard}`, file: 'src/views/usage.ts:guideFindingCard' });
        }
        // 권장 행동이 있고, 인과 단정/달러 표기가 없는가.
        const guideText = (await text(page, '.guide-box')) ?? '';
        if (!guideText.includes('권장 행동')) bugs.push({ severity: 'Major', symptom: '카드에 "권장 행동"이 없음', file: 'src/views/usage.ts:guideFindingCard' });
        if (/때문/.test(guideText)) bugs.push({ severity: 'Major', symptom: '가이드 문구에 인과 단정("때문")이 있음', file: 'src/efficiencyGuide.ts' });
        if (/\$/.test(guideText)) bugs.push({ severity: 'Major', symptom: '가이드 영역에 달러($) 표기가 있음(비용 미사용 원칙 위반)', file: 'src/efficiencyGuide.ts' });
        if (guideText.includes('판단할 데이터가 부족')) bugs.push({ severity: 'Major', symptom: '규칙 충족 상태에서 "데이터 부족" 문구가 같이 보임', file: 'src/views/usage.ts:renderEfficiencyGuide' });
        // 기존 카운트 계약(.stat-row .stat 8개)을 가이드가 깨지 않는가.
        const statCount = await page.locator('.stat-row .stat').count();
        if (statCount !== 8) bugs.push({ severity: 'Major', symptom: `가이드 추가 후 통계 타일이 8개가 아님(${statCount})`, file: 'src/views/usage.ts' });
        // 외부에서 새 커맨드를 부르지 않았는가(새 수집 경로 금지).
        const cmds = await page.evaluate(() => [...new Set((window.__invokeLog ?? []).map((e) => e.cmd))]);
        const allowedNew = cmds.filter((c) => !['get_daily_usage', 'get_usage_summary'].includes(c) && /usage|guide|efficien/i.test(c));
        if (allowedNew.length > 0) bugs.push({ severity: 'Critical', symptom: `가이드용으로 보이는 신규 커맨드 호출: ${allowedNew.join(',')}`, file: 'src/views/usage.ts' });
      },
    },
    {
      id: 'clear',
      fixtures: { ...base, get_daily_usage: clearDaily(), get_usage_summary: summaryWithModels([SONNET(800_000), OPUS(200_000)]) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-guide-clear');
        const status = await page.locator('.guide-box').getAttribute('data-guide-status').catch(() => null);
        const cards = await page.locator('.guide-card').count();
        if (status !== 'clear' || cards !== 0) {
          bugs.push({ severity: 'Critical', symptom: `기준 이내 픽스처인데 status=${status}, 카드 ${cards}개(clear/0이어야 함)`, file: 'src/efficiencyGuide.ts' });
        }
        const t = (await text(page, '.guide-box')) ?? '';
        if (!t.includes('모두 기준 이내')) bugs.push({ severity: 'Major', symptom: `clear 안내 문구 없음: ${t.slice(0, 120)}`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
        if (t.includes('판단할 데이터가 부족')) bugs.push({ severity: 'Major', symptom: 'clear 상태에 "데이터 부족" 문구가 보임(두 상태 혼동)', file: 'src/views/usage.ts:renderEfficiencyGuide' });
      },
    },
    {
      id: 'insufficient',
      fixtures: { ...base, get_daily_usage: tinyDaily(), get_usage_summary: summaryWithModels([OPUS(4_000), SONNET(1_000)]) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-guide-insufficient');
        const status = await page.locator('.guide-box').getAttribute('data-guide-status').catch(() => null);
        const cards = await page.locator('.guide-card').count();
        if (status !== 'insufficient' || cards !== 0) {
          bugs.push({ severity: 'Critical', symptom: `표본 부족 픽스처(히트율 낮고 Opus 80%)인데 status=${status}, 카드 ${cards}개 — 표본이 작으면 단정하면 안 됨`, file: 'src/efficiencyGuide.ts' });
        }
        const t = (await text(page, '.guide-box')) ?? '';
        if (!t.includes('판단할 데이터가 부족합니다')) bugs.push({ severity: 'Major', symptom: `"판단할 데이터가 부족합니다" 문구 없음: ${t.slice(0, 120)}`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
        if (t.includes('모두 기준 이내')) bugs.push({ severity: 'Major', symptom: '데이터 부족인데 "기준 이내(문제 없음)"로 표시됨', file: 'src/views/usage.ts:renderEfficiencyGuide' });
        const skips = await page.locator('.guide-skipped li').count();
        if (skips !== 3) bugs.push({ severity: 'Minor', symptom: `보류 사유가 3줄이어야 하는데 ${skips}줄`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
      },
    },
    {
      id: 'summary-error',
      fixtures: { ...base, get_daily_usage: triggeredDaily(), get_usage_summary: { __throw: '테스트 강제 에러: get_usage_summary 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-daily-guide-summary-error');
        const rules = await page.locator('.guide-card').evaluateAll((els) => els.map((e) => e.getAttribute('data-rule')));
        if (rules.includes('heavy-model-share')) bugs.push({ severity: 'Critical', symptom: '30일 요약 실패인데 모델 규칙 카드가 나옴', file: 'src/views/usage.ts:renderEfficiencyGuide' });
        if (!rules.includes('cache-hit-low') || !rules.includes('peak-day-concentration')) {
          bugs.push({ severity: 'Major', symptom: `요약 실패여도 일별 데이터 기반 규칙은 판정돼야 함(실제 ${rules.join(',')})`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
        }
        const skipText = (await text(page, '.guide-skipped')) ?? '';
        if (!skipText.includes('대형 모델 비중') || !skipText.includes('판단 보류')) {
          bugs.push({ severity: 'Major', symptom: `모델 규칙 보류 사유가 안 보임: ${skipText}`, file: 'src/views/usage.ts:renderEfficiencyGuide' });
        }
      },
    },
    {
      id: 'empty',
      fixtures: { ...base, get_daily_usage: [] },
      async run(page, { bugs }) {
        await page.waitForTimeout(300);
        const n = await page.locator('.guide-box').count();
        if (n !== 0) bugs.push({ severity: 'Minor', symptom: '사용 기록 0건인데 효율 가이드 박스가 보임(빈 상태는 기존 안내만)', file: 'src/views/usage.ts:renderDailyTab' });
      },
    },
    {
      id: 'boundary',
      fixtures: { ...base, get_daily_usage: clearDaily(), get_usage_summary: summaryWithModels([SONNET(800_000), OPUS(200_000)]) },
      async run(page, { bugs }) {
        // 실제 앱이 쓰는 모듈 그대로 — Vite dev 서버가 TS를 변환해 서빙하는 JS를 받아
        // Node에서 로드한다(페이지는 CSP 때문에 new Function/eval을 쓸 수 없어 브라우저
        // 안에서는 돌리지 않는다. efficiencyGuide.ts는 DOM/Tauri/다른 모듈 import가 없다).
        const baseUrl = process.env.UI_HARNESS_BASE_URL || 'http://localhost:1420';
        const js = await (await fetch(`${baseUrl}/src/efficiencyGuide.ts`)).text();
        if (/^\s*import\s/m.test(js)) bugs.push({ severity: 'Major', symptom: 'efficiencyGuide.ts가 다른 모듈을 import함(순수 함수 계약 위반)', file: 'src/efficiencyGuide.ts' });
        const mod = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
        const result = runGuideBoundaryChecks(mod);
        for (const row of result.rows) console.log(`[usageGuide/boundary] ${row}`);
        console.log(`[usageGuide/boundary] ${result.total - result.failures.length}/${result.total} PASS`);
        for (const f of result.failures) bugs.push({ severity: 'Critical', symptom: `경계값 검사 실패: ${f}`, file: 'src/efficiencyGuide.ts' });
        if (result.total < 30) bugs.push({ severity: 'Major', symptom: `경계값 케이스 수가 너무 적음(${result.total})`, file: 'scripts/ui-harness/lib/guideBoundary.mjs' });
      },
    },
  ];
}
