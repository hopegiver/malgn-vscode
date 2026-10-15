// 흐름 — "사용량 통계" 3개 서브뷰(daily/projects/models, route.ts UsageTab) —
// src/views/usage.ts, src/sidebar.ts renderUsageSidebar.
//
// get_usage_summary(30일 요약)는 jsonl 전체 1회 스캔이라 하네스에서 재현할 수
// 없어 usageSummarySample()(fixtures.mjs)을 쓴다 — unpricedTokens>0인 모델 1건을
// 고정으로 섞어 "미산정" 표시 경로가 golden 시나리오에서도 항상 지나가게 한다.
import { usageSummarySample, projectDailyTrendSample } from '../lib/fixtures.mjs';

export const flowId = 'usage';
export const startHash = '#/usage';

// ---- R14 P2-6: 빈 상태 원인 구분 시나리오 공용 ----
// 스텁 모양 근거: get_daily_usage는 데이터 없으면 빈 배열(src-tauri/src/usage_stats/mod.rs:24-28
// unwrap_or_default), get_usage_summary 빈 보고서는 totalTokens=0·빈 배열(summary.rs
// UsageSummaryReport::default), get_usage_source_status는 Option<UsageSourceStatus> camelCase
// (usage_stats/source.rs `UsageSourceStatus`, 홈 디렉터리 불명이면 null — mod.rs get_usage_source_status).
function emptyFixtures(base, sourceSpec) {
  return {
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
    get_usage_source_status: sourceSpec,
  };
}

async function mainText(page) {
  return (await page.locator('main').textContent().catch(() => '')) ?? '';
}

// kind: data-usage-empty 기대값, mustInclude/mustExclude: 본문 문구 검사.
async function checkEmptyCause(page, bugs, { kind, reason, mustInclude, mustExclude, tab }) {
  const where = `src/views/usage.ts:renderUsageEmptyState(${tab})`;
  const block = page.locator('[data-usage-empty]').first();
  const got = await block.getAttribute('data-usage-empty').catch(() => null);
  if (got !== kind) {
    bugs.push({ severity: 'Major', symptom: `${tab} 탭 빈 상태 원인이 ${kind}여야 하는데 ${got}`, file: where });
  }
  if (reason) {
    const r = await block.getAttribute('data-usage-empty-reason').catch(() => null);
    if (r !== reason) bugs.push({ severity: 'Major', symptom: `${tab} 탭 unknown 사유가 ${reason}여야 하는데 ${r}`, file: where });
  }
  const text = await mainText(page);
  for (const t of mustInclude) {
    if (!text.includes(t)) bugs.push({ severity: 'Major', symptom: `${tab} 탭 빈 상태에 "${t}" 문구가 없음(실제: ${text.slice(0, 140)})`, file: where });
  }
  for (const t of mustExclude) {
    if (text.includes(t)) bugs.push({ severity: 'Major', symptom: `${tab} 탭 빈 상태에 있어선 안 되는 "${t}" 문구가 있음(단정/혼동)`, file: where });
  }
  if ((await page.locator('main .alert').count()) !== 0) {
    bugs.push({ severity: 'Major', symptom: `${tab} 탭 빈 상태가 오류 배너(.alert)와 섞여 그려짐`, file: where });
  }
}

const NOT_INSTALLED_TITLE = 'Claude Code가 설치돼 있지 않은 것으로 보입니다';
const NO_HISTORY_TITLE = 'Claude Code는 설치돼 있지만 아직 사용 기록이 없습니다';

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
          bugs.push({ severity: 'Major', symptom: `daily 탭 통계 타일이 8개(기존 4개+최고 사용일/캐시 절대량/입출력 비율/추정 비용)여야 하는데 ${statCount}개`, file: 'src/views/usage.ts:renderUsageStatCards' });
        }
        // frontend-dev 수정(1280/1180/900폭 값 줄바꿈 정돈, 2026-09-24) — daily
        // 탭 통계 타일을 "핵심 지표"(.stat-row)/"세부 지표"(.stat-row
        // .stat-row-detail) 두 줄로 나눴다. "최고 사용일"/"추정 비용"은 세부
        // 지표 줄에 있어 .first()만 보면 못 찾으므로, 두 .stat-row 텍스트를
        // 모두 합쳐 검사한다.
        const statText = await page
          .locator('.stat-row')
          .allTextContents()
          .then((arr) => arr.join(' '))
          .catch(() => null);
        if (!statText || !statText.includes('최고 사용일') || !statText.includes('추정 비용')) {
          bugs.push({ severity: 'Major', symptom: `daily 탭 통계 타일에 "최고 사용일"/"추정 비용" 라벨이 보이지 않음(실제: ${statText})`, file: 'src/views/usage.ts:renderUsageStatCards' });
        }
        if (!statText || !statText.includes('$')) {
          bugs.push({ severity: 'Major', symptom: '추정 비용 타일에 "$" 표기가 없음(usageSummary.totalCostUsd 미반영 의심)', file: 'src/views/usage.ts:costStat' });
        }
        // 요약 리포트에 단가 미산정 토큰(픽스처 300K)이 있으면 카드도 주간 표·차트와 같은 "≥"로 과소 표기를 드러낸다.
        const costTile = await page.locator('.stat-row .stat').filter({ hasText: '최근 30일 추정 비용' }).first().innerText().catch(() => '');
        if (!costTile.includes('≥$128.42')) {
          bugs.push({ severity: 'Major', symptom: `미산정 토큰이 있는데 "최근 30일 추정 비용" 카드 값이 ≥$128.42가 아님(실제: ${costTile.replace(/\n/g, ' | ')})`, file: 'src/views/usage.ts:costStat' });
        }
        const dailyBoxTitle = await page.locator('main').textContent().catch(() => '');
        if (!dailyBoxTitle || !dailyBoxTitle.includes('일별 사용량 (최근 30일)')) {
          bugs.push({ severity: 'Minor', symptom: '일별 목록 박스 제목이 "일별 사용량 (최근 30일)"이 아님', file: 'src/views/usage.ts:renderDailyUsageSection' });
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
        // R14 P2-6: 데이터가 있는 정상 경로는 빈 상태 원인 신호를 묻지 않고 빈 상태 블록도 그리지 않는다.
        const sourceInvoke = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_usage_source_status') ?? []);
        if (sourceInvoke.length !== 0) {
          bugs.push({ severity: 'Major', symptom: `일별 데이터가 있는데 get_usage_source_status가 ${sourceInvoke.length}회 호출됨(비었을 때만 호출해야 함)`, file: 'src/views/usage.ts:loadDailyUsage' });
        }
        if ((await page.locator('[data-usage-empty]').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '데이터가 있는 정상 경로에 빈 상태 블록(data-usage-empty)이 그려짐', file: 'src/views/usage.ts:renderUsageEmptyState' });
        }
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
    {
      id: 'empty-not-installed',
      fixtures: emptyFixtures(base, { claudeBinaryFound: false, claudeDirExists: false, projectsDirHasEntries: false }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-not-installed');
        await checkEmptyCause(page, bugs, {
          tab: 'daily',
          kind: 'not-installed',
          mustInclude: [NOT_INSTALLED_TITLE, '다른 위치에 설치했다면 이 판단이 틀릴 수 있습니다', '개발 도구에서 설치'],
          mustExclude: ['아직 사용 기록이 없습니다', '첫 세션을 시작하면'],
        });
        const calls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_usage_source_status') ?? []);
        if (calls.length !== 1) bugs.push({ severity: 'Major', symptom: `get_usage_source_status 호출이 ${calls.length}회(1회여야 함)`, file: 'src/views/usage.ts:loadDailyUsage' });
        // 다음 행동: 개발 도구 화면으로 이동
        await page.getByRole('button', { name: '개발 도구에서 설치' }).click();
        await page.waitForTimeout(300);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/settings/devtools') bugs.push({ severity: 'Major', symptom: `"개발 도구에서 설치" 클릭 후 해시가 #/settings/devtools가 아님(실제 ${hash})`, file: 'src/views/usage.ts:renderUsageEmptyState' });
        await page.evaluate(() => { window.location.hash = '#/usage/projects'; });
        await page.waitForTimeout(300);
        await shot('02-projects-not-installed');
        await checkEmptyCause(page, bugs, { tab: 'projects', kind: 'not-installed', mustInclude: [NOT_INSTALLED_TITLE], mustExclude: ['아직 사용 기록이 없습니다'] });
        await page.locator('.sidebar-nav-row', { hasText: '모델·도구' }).click();
        await page.waitForTimeout(300);
        await shot('03-models-not-installed');
        await checkEmptyCause(page, bugs, { tab: 'models', kind: 'not-installed', mustInclude: [NOT_INSTALLED_TITLE, '모델 사용 기록이 없습니다'], mustExclude: [] });
      },
    },
    {
      id: 'empty-installed-no-history',
      fixtures: emptyFixtures(base, { claudeBinaryFound: true, claudeDirExists: true, projectsDirHasEntries: false }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-installed-no-history');
        await checkEmptyCause(page, bugs, {
          tab: 'daily',
          kind: 'installed-no-history',
          mustInclude: [NO_HISTORY_TITLE, '첫 세션을 시작하면 여기에 쌓입니다'],
          mustExclude: ['설치돼 있지 않은', '다른 위치에 설치했다면'],
        });
        await page.locator('.sidebar-nav-row', { hasText: '프로젝트별' }).click();
        await page.waitForTimeout(300);
        await shot('02-projects-installed-no-history');
        await checkEmptyCause(page, bugs, { tab: 'projects', kind: 'installed-no-history', mustInclude: [NO_HISTORY_TITLE], mustExclude: ['설치돼 있지 않은'] });
      },
    },
    {
      id: 'empty-unknown-binary-missing-traces',
      fixtures: emptyFixtures(base, { claudeBinaryFound: false, claudeDirExists: true, projectsDirHasEntries: true }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-unknown-traces');
        await checkEmptyCause(page, bugs, {
          tab: 'daily',
          kind: 'unknown',
          reason: 'binary-not-found-but-traces',
          mustInclude: ['최근 30일 이내 사용 기록이 없습니다', '원인은 단정할 수 없습니다'],
          mustExclude: ['설치돼 있지 않은', NO_HISTORY_TITLE],
        });
      },
    },
    {
      id: 'empty-unknown-status-null',
      fixtures: emptyFixtures(base, null),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-unknown-null');
        await checkEmptyCause(page, bugs, {
          tab: 'daily',
          kind: 'unknown',
          reason: 'status-unavailable',
          mustInclude: ['최근 30일 이내 사용 기록이 없습니다', '원인(Claude Code 미설치인지, 아직 사용 전인지)은 확인하지 못했습니다'],
          mustExclude: ['설치돼 있지 않은', NO_HISTORY_TITLE],
        });
      },
    },
    {
      id: 'empty-unknown-status-throw',
      fixtures: emptyFixtures(base, { __throw: '테스트 강제 에러: get_usage_source_status 실패' }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-unknown-throw');
        // 원인 조회 실패는 사용량 오류가 아니다 — 오류 배너 없이 "판정 불가"로 내려간다.
        await checkEmptyCause(page, bugs, {
          tab: 'daily',
          kind: 'unknown',
          reason: 'status-unavailable',
          mustInclude: ['최근 30일 이내 사용 기록이 없습니다'],
          mustExclude: ['설치돼 있지 않은', NO_HISTORY_TITLE, '불러오지 못했습니다'],
        });
      },
    },
    {
      id: 'daily-error-not-empty-state',
      fixtures: { ...base, get_daily_usage: { __throw: '테스트 강제 에러: get_daily_usage 실패' }, get_usage_source_status: { claudeBinaryFound: false, claudeDirExists: false, projectsDirHasEntries: false } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-daily-error');
        if ((await page.locator('main .alert').count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'get_daily_usage가 throw해도 오류 배너가 뜨지 않음', file: 'src/views/usage.ts:renderDailyUsageSection' });
        }
        if ((await page.locator('[data-usage-empty]').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '조회 실패(오류)인데 "미설치/사용 기록 없음" 빈 상태 블록이 같이 그려짐 — 오류와 빈 상태가 섞임', file: 'src/views/usage.ts:renderUsageEmptyState' });
        }
        const text = await mainText(page);
        if (text.includes('설치돼 있지 않은')) bugs.push({ severity: 'Major', symptom: '조회 실패 화면에 미설치 단정 문구가 보임', file: 'src/views/usage.ts' });
        const calls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'get_usage_source_status') ?? []);
        if (calls.length !== 0) bugs.push({ severity: 'Minor', symptom: `조회 실패인데 원인 신호를 ${calls.length}회 물음`, file: 'src/views/usage.ts:loadDailyUsage' });
      },
    },
  ];
}
