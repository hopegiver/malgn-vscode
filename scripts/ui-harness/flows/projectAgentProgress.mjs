// 흐름 — 프로젝트별 에이전트 진행 상황(docs/design/project-agent-progress.md)
// (A) 프로젝트 카드 `실행 중 N` (B) 프로젝트 상세 "세션" 섹션 (C) 홈 요약.
// 세션↔프로젝트 조인은 백엔드가 붙인 projectPath 문자열 동등 비교뿐이므로, 하위
// 디렉터리 귀속 자체는 Rust 테스트 몫이고 여기서는 cwd가 하위 경로인 스텁 행이
// projectPath 기준으로 올바른 카드에 붙는지만 본다.
export const flowId = 'projectAgentProgress';
export const startHash = '#/projects';

const ALPHA = '/Users/hopegiver/workspace/ha-alpha';
const BETA = '/Users/hopegiver/workspace/ha-beta';
const GAMMA = '/Users/hopegiver/workspace/ha-gamma';

function project(name, p) {
  return { name, path: p, hasStatus: true, archiveStatus: 'active', sections: null, updatedAt: Date.now() };
}

function session(id, projectPath, cwd, title, { running = false, minAgo = 5 } = {}) {
  const t = Date.now() - minAgo * 60_000;
  return { sessionId: id, cwd, projectPath, title, name: `${id}-n`, version: '2.1.0', kind: 'cli', startedAt: t - 60_000, updatedAt: t, running };
}

const PROJECTS = { projects: [project('ha-alpha', ALPHA), project('ha-beta', BETA), project('ha-gamma', GAMMA)], skipped: [] };

const SESSIONS_RUNNING = [
  session('aaaaaaaa-0001', ALPHA, ALPHA, '알파 실행 세션 하나', { running: true, minAgo: 2 }),
  // cwd가 하위 디렉터리여도 projectPath가 ALPHA면 알파 카드에 붙는다.
  session('aaaaaaaa-0002', ALPHA, `${ALPHA}/src-tauri`, '알파 하위 디렉터리 실행 세션', { running: true, minAgo: 1 }),
  session('aaaaaaaa-0003', ALPHA, ALPHA, '알파 끝난 세션', { minAgo: 90 }),
  session('bbbbbbbb-0001', BETA, BETA, '베타 끝난 세션', { minAgo: 30 }),
];

const SESSIONS_NONE_RUNNING = SESSIONS_RUNNING.map((s) => ({ ...s, running: false }));

const common = (sessions) => ({ list_workspace_projects: PROJECTS, list_claude_sessions: sessions, list_project_tree: [] });

async function cardText(page, name) {
  const card = page.locator('.project-card', { hasText: name });
  return (await card.count()) === 0 ? null : card.first().textContent();
}

export function scenarios(base) {
  return [
    {
      id: 'running',
      fixtures: { ...base, ...common(SESSIONS_RUNNING) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.project-card');
        await shot('01-card-indicator');
        const alpha = await cardText(page, 'ha-alpha');
        if (!alpha || !alpha.includes('실행 중 2')) bugs.push({ severity: 'Critical', symptom: `알파 카드에 "실행 중 2"가 없음(실제: ${alpha})`, file: 'src/views/projects.ts:renderProjectCard' });
        for (const n of ['ha-beta', 'ha-gamma']) {
          const t = await cardText(page, n);
          if (t && t.includes('실행 중')) bugs.push({ severity: 'Critical', symptom: `${n} 카드에 실행 중 표시가 잘못 붙음(실제: ${t})`, file: 'src/views/projects.ts' });
        }
        const order = await page.locator('.project-card-name').allTextContents();
        if (order.length !== 3) bugs.push({ severity: 'Major', symptom: `카드 수 이상(${order.length})`, file: 'src/views/projects.ts' });

        // (B) 상세
        await page.locator('.project-card', { hasText: 'ha-alpha' }).getByRole('button', { name: /상세 보기/ }).click();
        await page.waitForSelector('.project-sessions-box');
        await shot('02-detail-sessions');
        const box = page.locator('.project-sessions-box');
        const rows = box.locator('.blist-row--sessions');
        if ((await rows.count()) !== 3) bugs.push({ severity: 'Critical', symptom: `알파 상세 세션 행 수 3 기대, 실제 ${await rows.count()}`, file: 'src/views/projects.ts:renderProjectSessionsBox' });
        if ((await box.locator('.blist-accent').count()) !== 0) bugs.push({ severity: 'Major', symptom: '상세 세션 행에 프로젝트명 열이 남아 있음(showProject:false 미적용)', file: 'src/sessionRow.ts' });
        const statuses = await box.locator('.blist-status').allTextContents();
        if (!(statuses[0]?.includes('실행 중') && statuses[1]?.includes('실행 중') && statuses[2]?.includes('완료'))) bugs.push({ severity: 'Critical', symptom: `실행 중 → 완료 순서가 아님: ${JSON.stringify(statuses)}`, file: 'src/projectActivity.ts' });
        const head = await box.locator('.box-head').textContent();
        if (!head || !head.includes('실행 중 2') || !head.includes('최근 1')) bugs.push({ severity: 'Major', symptom: `헤더 카운트 불일치(실제: ${head})`, file: 'src/views/projects.ts' });
        await rows.first().click();
        await page.waitForTimeout(150);
        const hash = await page.evaluate(() => window.location.hash);
        if (!hash.startsWith('#/sessions/')) bugs.push({ severity: 'Major', symptom: `행 클릭이 세션 상세로 이동하지 않음(hash=${hash})`, file: 'src/sessionRow.ts' });

        // (C) 홈
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForSelector('.greet-sub');
        await page.waitForTimeout(200);
        await shot('03-home-tile');
        const sub = await page.locator('.greet-sub').textContent();
        if (!sub || !sub.includes('활동 중 1개 프로젝트 · 실행 중 세션 2개')) bugs.push({ severity: 'Critical', symptom: `홈 인사말 요약 불일치(실제: ${sub})`, file: 'src/views/home.ts:renderGreeting' });
        const tip = await page.locator('.greet-sub [title]').getAttribute('title');
        if (!tip || !tip.includes('워크스페이스')) bugs.push({ severity: 'Minor', symptom: '"워크스페이스 기준" 툴팁 없음', file: 'src/views/home.ts' });
        const tile = page.locator('.stat', { hasText: '실행 중 세션' });
        const tt = await tile.textContent();
        if (!tt || !tt.includes('2') || !tt.includes('1개 프로젝트')) bugs.push({ severity: 'Critical', symptom: `홈 타일 값/트렌드 불일치(실제: ${tt})`, file: 'src/views/home.ts:renderStatRow' });
        if ((await page.locator('.stat', { hasText: '실행 중 세션' }).count()) !== 1) bugs.push({ severity: 'Major', symptom: '실행 중 세션 타일 중복', file: 'src/views/home.ts' });
      },
    },
    {
      id: 'none-running-empty',
      fixtures: { ...base, ...common(SESSIONS_NONE_RUNNING) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.project-card');
        await shot('01-cards-no-indicator');
        if ((await page.locator('.project-card-running').count()) !== 0) bugs.push({ severity: 'Critical', symptom: '실행 중 0건인데 카드 인디케이터가 표시됨', file: 'src/views/projects.ts' });
        // 세션 0건 프로젝트 상세 → 빈 상태 + 새 세션 버튼
        await page.locator('.project-card', { hasText: 'ha-gamma' }).getByRole('button', { name: /상세 보기/ }).click();
        await page.waitForSelector('.project-sessions-box');
        await shot('02-detail-empty');
        const box = page.locator('.project-sessions-box');
        const t = await box.textContent();
        if (!t || !t.includes('최근 세션이 없습니다')) bugs.push({ severity: 'Critical', symptom: `빈 상태 문구 없음(실제: ${t})`, file: 'src/views/projects.ts' });
        if ((await box.getByRole('button', { name: '+ 새 세션' }).count()) !== 1) bugs.push({ severity: 'Major', symptom: '빈 상태에 + 새 세션 버튼 없음', file: 'src/views/projects.ts' });
        await box.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForTimeout(150);
        const hash = await page.evaluate(() => window.location.hash);
        if (!hash.startsWith('#/sessions/new/')) bugs.push({ severity: 'Major', symptom: `새 세션 버튼 이동 실패(hash=${hash})`, file: 'src/views/projects.ts' });
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForSelector('.greet-sub');
        await page.waitForTimeout(200);
        await shot('03-home-zero');
        const tile = await page.locator('.stat', { hasText: '실행 중 세션' }).textContent();
        if (tile && tile.includes('개 프로젝트')) bugs.push({ severity: 'Major', symptom: `0건인데 타일 trend가 표시됨(실제: ${tile})`, file: 'src/views/home.ts' });
        const sub = await page.locator('.greet-sub').textContent();
        if (!sub || !sub.includes('활동 중 0개 프로젝트 · 실행 중 세션 0개')) bugs.push({ severity: 'Major', symptom: `0건 인사말 불일치(실제: ${sub})`, file: 'src/views/home.ts' });
      },
    },
    {
      id: 'sessions-error',
      fixtures: { ...base, ...common([]), list_claude_sessions: { __throw: '테스트 강제 에러: 세션 목록 로드 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.project-card');
        await shot('01-cards-error');
        if ((await page.locator('.project-card-running').count()) !== 0) bugs.push({ severity: 'Critical', symptom: '세션 로드 실패인데 카드 인디케이터 표시(거짓 값)', file: 'src/views/projects.ts' });
        await page.locator('.project-card', { hasText: 'ha-alpha' }).getByRole('button', { name: /상세 보기/ }).click();
        await page.waitForSelector('.project-sessions-box');
        await shot('02-detail-error');
        const t = await page.locator('.project-sessions-box').textContent();
        if (!t || !t.includes('테스트 강제 에러')) bugs.push({ severity: 'Critical', symptom: `상세 세션 섹션에 에러 블록이 없음(실제: ${t})`, file: 'src/views/projects.ts' });
        if (t && t.includes('최근 세션이 없습니다')) bugs.push({ severity: 'Critical', symptom: '로드 실패를 "세션 없음"(거짓 0)으로 표시', file: 'src/views/projects.ts' });
      },
    },
  ];
}
