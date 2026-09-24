// 흐름 — "사용량 통계" 3개 서브뷰(daily/projects/models, route.ts UsageTab) —
// src/views/usage.ts, src/sidebar.ts renderUsageSidebar.
//
// get_usage_summary(30일 요약)는 jsonl 전체 1회 스캔이라 하네스에서 재현할 수
// 없어 usageSummarySample()(fixtures.mjs)을 쓴다 — unpricedTokens>0인 모델 1건을
// 고정으로 섞어 "미산정" 표시 경로가 golden 시나리오에서도 항상 지나가게 한다.
import { usageSummarySample, projectDailyTrendSample } from '../lib/fixtures.mjs';

export const flowId = 'usage';
export const startHash = '#/usage';

export function scenarios(base) {
  return [
    {
      id: 'golden',
      fixtures: base,
      async run(page, { shot, bugs }) {
        // ---- daily(토큰 사용량, 기본) ----
        await shot('01-daily');
        const statCount = await page.locator('.stat-row .stat').count();
        if (statCount !== 8) {
          bugs.push({ severity: 'Major', symptom: `daily 탭 통계 타일이 8개(기존 4개+최고 사용일/캐시 절대량/입출력 비율/예상 비용)여야 하는데 ${statCount}개`, file: 'src/views/usage.ts:renderUsageStatCards' });
        }
        // frontend-dev 수정(1280/1180/900폭 값 줄바꿈 정돈, 2026-09-24) — daily
        // 탭 통계 타일을 "핵심 지표"(.stat-row)/"세부 지표"(.stat-row
        // .stat-row-detail) 두 줄로 나눴다. "최고 사용일"/"예상 비용"은 세부
        // 지표 줄에 있어 .first()만 보면 못 찾으므로, 두 .stat-row 텍스트를
        // 모두 합쳐 검사한다.
        const statText = await page
          .locator('.stat-row')
          .allTextContents()
          .then((arr) => arr.join(' '))
          .catch(() => null);
        if (!statText || !statText.includes('최고 사용일') || !statText.includes('예상 비용')) {
          bugs.push({ severity: 'Major', symptom: `daily 탭 통계 타일에 "최고 사용일"/"예상 비용" 라벨이 보이지 않음(실제: ${statText})`, file: 'src/views/usage.ts:renderUsageStatCards' });
        }
        if (!statText || !statText.includes('$')) {
          bugs.push({ severity: 'Major', symptom: '예상 비용 타일에 "$" 표기가 없음(usageSummary.totalCostUsd 미반영 의심)', file: 'src/views/usage.ts:costStat' });
        }

        // ---- 서브뷰 이동: daily -> projects ----
        await page.locator('.sidebar-nav-row', { hasText: '프로젝트별' }).click();
        await page.waitForTimeout(300);
        await shot('02-projects');
        const projectRows = await page.locator('.bar-row-clickable').count();
        if (projectRows !== 5) {
          bugs.push({ severity: 'Major', symptom: `프로젝트별 Top5 랭킹 행이 5개여야 하는데 ${projectRows}개`, file: 'src/views/usage.ts:renderProjectsTab' });
        }
        const projectsBodyText = await page.locator('main').textContent().catch(() => null);
        if (!projectsBodyText || !projectsBodyText.includes('Top 5 외 나머지')) {
          bugs.push({ severity: 'Minor', symptom: 'otherProjectsTokens>0인데 "Top 5 외 나머지" 행이 안 보임', file: 'src/views/usage.ts:renderOtherProjectsRow' });
        }

        // ---- 프로젝트 클릭 → 30일 추이 펼침 ----
        await page.locator('.bar-row-clickable').first().click();
        await page.waitForTimeout(300);
        await shot('03-projects-expanded');
        const trendRows = await page.locator('.daily-detail-panel .bar-row').count();
        if (trendRows !== projectDailyTrendSample().length) {
          bugs.push({ severity: 'Major', symptom: `프로젝트 클릭 후 펼쳐진 30일 추이 행 개수가 픽스처(${projectDailyTrendSample().length}개)와 다름(실제 ${trendRows}개)`, file: 'src/views/usage.ts:renderProjectTrendPanel' });
        }
        const trendInvoke = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_project_daily_trend') ?? []);
        if (trendInvoke.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '프로젝트 랭킹 행 클릭이 get_project_daily_trend IPC를 호출하지 않음', file: 'src/views/usage.ts:toggleProjectTrend' });
        } else if (!trendInvoke[0].args?.projectKey) {
          bugs.push({ severity: 'Major', symptom: `get_project_daily_trend 호출에 projectKey 인자가 없음: ${JSON.stringify(trendInvoke[0].args)}`, file: 'src/views/usage.ts:toggleProjectTrend' });
        }

        // ---- 서브뷰 이동: projects -> models ----
        await page.locator('.sidebar-nav-row', { hasText: '모델·도구' }).click();
        await page.waitForTimeout(300);
        await shot('04-models');
        const modelRows = await page.locator('.thief-table-row').count();
        // 헤더 1행 + 모델 3건(픽스처) = 4행.
        if (modelRows !== 4) {
          bugs.push({ severity: 'Major', symptom: `모델별 표 행 개수가 헤더+3건=4여야 하는데 ${modelRows}`, file: 'src/views/usage.ts:renderModelUsageTable' });
        }
        const modelsText = await page.locator('main').textContent().catch(() => null);
        if (!modelsText || !modelsText.includes('미산정')) {
          bugs.push({ severity: 'Major', symptom: 'pricingMatched=false 모델 행에 "미산정" 표시가 없음(단가 조용히 대체 금지 원칙 위반 의심)', file: 'src/views/usage.ts:renderModelUsageTable' });
        }
        if (!modelsText || !modelsText.includes('토큰은 단가 미등록으로 비용 미산정')) {
          bugs.push({ severity: 'Minor', symptom: 'unpricedTokens>0인데 모델·도구 탭에 정직성 안내 문구가 안 보임', file: 'src/views/usage.ts:unpricedNote' });
        }
        const toolChips = await page.locator('.tool-usage-chip').count();
        if (toolChips !== 5) {
          bugs.push({ severity: 'Major', symptom: `자주 쓴 툴 Top5 칩이 5개여야 하는데 ${toolChips}개`, file: 'src/views/usage.ts:renderModelsTab' });
        }

        // ---- 캐싱: daily/projects/models 세 서브뷰를 오갔지만 get_usage_summary는
        // "사용량 탭 진입" 시점 1회만 호출돼야 한다(서브뷰 전환마다 재스캔 금지). ----
        const summaryInvoke = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_usage_summary') ?? []);
        if (summaryInvoke.length !== 1) {
          bugs.push({ severity: 'Major', symptom: `daily→projects→models 서브뷰 이동 중 get_usage_summary가 ${summaryInvoke.length}회 호출됨(1회여야 함 — 서브뷰 전환마다 재스캔되면 안 됨)`, file: 'src/main.ts:handleNavigation(enteringUsageTab)' });
        }
      },
    },
    {
      id: 'summary-error',
      startHash: '#/usage/projects',
      fixtures: { ...base, get_usage_summary: { __throw: '테스트 강제 에러: get_usage_summary 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-projects-error');
        const alertCount = await page.locator('.alert').count();
        if (alertCount === 0) {
          bugs.push({ severity: 'Critical', symptom: 'get_usage_summary가 throw해도 프로젝트별 탭에 에러 배너가 뜨지 않음', file: 'src/views/usage.ts:renderProjectsTab' });
          return;
        }
        await page.getByRole('button', { name: '다시 시도' }).click();
        await page.waitForTimeout(300);
        await shot('02-projects-retry');
        const invokeLog = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_usage_summary') ?? []);
        if (invokeLog.length < 2) {
          bugs.push({ severity: 'Major', symptom: `"다시 시도" 클릭이 get_usage_summary를 재호출하지 않음(실제 ${invokeLog.length}회)`, file: 'src/views/usage.ts:renderProjectsTab' });
        }
      },
    },
    {
      id: 'summary-slow',
      startHash: '#/usage/models',
      fixtures: { ...base, get_usage_summary: { __delay: 2500, value: usageSummarySample() } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-models-loading');
        const loadingVisible = await page.locator('.state-block-desc:has-text("불러오는 중")').count();
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 동안 모델·도구 탭에 "불러오는 중…" 표시가 뜨지 않음', file: 'src/views/usage.ts:renderModelsTab' });
        }
        await page.waitForTimeout(2400);
        await shot('02-models-loaded');
        const rows = await page.locator('.thief-table-row').count();
        if (rows <= 1) {
          bugs.push({ severity: 'Major', symptom: '지연 응답 완료 후에도 모델 표가 채워지지 않음', file: 'src/views/usage.ts:renderModelsTab' });
        }
      },
    },
    {
      id: 'empty',
      fixtures: {
        ...base,
        get_daily_usage: [],
        get_usage_summary: {
          ...usageSummarySample(),
          totalTokens: 0,
          totalCostUsd: 0,
          unpricedTokens: 0,
          models: [],
          projects: [],
          otherProjectsTokens: 0,
          otherProjectsCostUsd: 0,
          tools: [],
        },
      },
      async run(page, { shot, bugs }) {
        await shot('01-daily-empty');
        const dailyEmptyText = await page.locator('main').textContent().catch(() => null);
        if (!dailyEmptyText || !dailyEmptyText.includes('최근 30일 이내 사용 기록이 없습니다')) {
          bugs.push({ severity: 'Major', symptom: `일별 사용량 0건 안내 문구가 안 보임(실제: ${dailyEmptyText?.slice(0, 80)})`, file: 'src/views/usage.ts:renderDailyUsageSection' });
        }
        const statCount = await page.locator('.stat-row .stat').count();
        if (statCount !== 0) {
          bugs.push({ severity: 'Minor', symptom: `일별 사용량 0건인데 통계 타일이 ${statCount}개 남아있음(빈 상태에서 숨겨야 함)`, file: 'src/views/usage.ts:renderUsageStatCards' });
        }

        await page.locator('.sidebar-nav-row', { hasText: '프로젝트별' }).click();
        await page.waitForTimeout(300);
        await shot('02-projects-empty');
        const projectsEmptyText = await page.locator('main').textContent().catch(() => null);
        if (!projectsEmptyText || !projectsEmptyText.includes('프로젝트별 사용 기록이 없습니다')) {
          bugs.push({ severity: 'Major', symptom: `프로젝트별 0건 안내 문구가 안 보임(실제: ${projectsEmptyText?.slice(0, 80)})`, file: 'src/views/usage.ts:renderProjectsTab' });
        }

        await page.locator('.sidebar-nav-row', { hasText: '모델·도구' }).click();
        await page.waitForTimeout(300);
        await shot('03-models-empty');
        const modelsEmptyText = await page.locator('main').textContent().catch(() => null);
        if (!modelsEmptyText || !modelsEmptyText.includes('모델 사용 기록이 없습니다') || !modelsEmptyText.includes('툴 사용 기록이 없습니다')) {
          bugs.push({ severity: 'Major', symptom: `모델·도구 0건 안내 문구가 안 보임(실제: ${modelsEmptyText?.slice(0, 120)})`, file: 'src/views/usage.ts:renderModelsTab' });
        }
      },
    },
  ];
}
