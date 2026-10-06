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
const WIN = 'C:\\Users\\x\\workspace\\ha-win';

function project(name, p, archiveStatus = 'active') {
  return { name, path: p, hasStatus: true, archiveStatus, sections: null, updatedAt: Date.now() };
}

function session(id, projectPath, cwd, title, { running = false, minAgo = 5 } = {}) {
  const t = Date.now() - minAgo * 60_000;
  return { sessionId: id, cwd, projectPath, title, name: `${id}-n`, version: '2.1.0', kind: 'cli', startedAt: t - 60_000, updatedAt: t, running };
}

const PROJECTS = { projects: [project('ha-alpha', ALPHA), project('ha-beta', BETA), project('ha-gamma', GAMMA, 'archived')], skipped: [] };

const SESSIONS_RUNNING = [
  session('aaaaaaaa-0001', ALPHA, ALPHA, '알파 실행 세션 하나', { running: true, minAgo: 2 }),
  // cwd가 하위 디렉터리여도 projectPath가 ALPHA면 알파 카드에 붙는다.
  session('aaaaaaaa-0002', ALPHA, `${ALPHA}/src-tauri`, '알파 하위 디렉터리 실행 세션', { running: true, minAgo: 1 }),
  session('aaaaaaaa-0003', ALPHA, ALPHA, '알파 끝난 세션', { minAgo: 90 }),
  session('bbbbbbbb-0001', BETA, BETA, '베타 끝난 세션', { minAgo: 30 }),
];

// Windows 경로 projectPath — 구분자가 \\여도 이름은 마지막 조각(ha-win)이어야 한다.
const SESSIONS_WIN = [session('cccccccc-0001', WIN, `${WIN}\\src`, '윈도우 하위 디렉터리 세션', { running: true, minAgo: 1 })];

const SESSIONS_NONE_RUNNING = SESSIONS_RUNNING.map((s) => ({ ...s, running: false }));

const common = (sessions) => ({ list_workspace_projects: PROJECTS, list_claude_sessions: sessions, list_project_tree: [] });

// 사이드바 프로젝트 행(점 색 + meta) — 초록 점은 "실행 중" 한 가지 의미여야 한다(M5).
async function sidebarRows(page) {
  return page.$$eval('.sidebar .ws-row', (rows) =>
    rows.map((r) => ({
      name: r.querySelector('.ws-row-name')?.textContent ?? '',
      meta: r.querySelector('.ws-row-meta')?.textContent ?? '',
      good: !!r.querySelector('.dot.good'),
    })),
  );
}

function checkSidebarGood(bugs, rows, expectedGoodNames, label) {
  const good = rows.filter((r) => r.good).map((r) => r.name).sort();
  const want = [...expectedGoodNames].sort();
  if (JSON.stringify(good) !== JSON.stringify(want)) {
    bugs.push({ severity: 'Major', symptom: `${label}: 사이드바 초록 점 프로젝트 불일치(기대 ${JSON.stringify(want)}, 실제 ${JSON.stringify(good)})`, file: 'src/sidebar.ts:dotClassFor' });
  }
}

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
        // M5: 사이드바 점 — 실행 중인 알파만 초록, 나머지 중립. 진행/보관은 meta로 구분.
        const sb = await sidebarRows(page);
        await shot('01b-sidebar-running');
        checkSidebarGood(bugs, sb, ['ha-alpha'], 'running');
        const sbAlpha = sb.find((r) => r.name === 'ha-alpha');
        const sbGamma = sb.find((r) => r.name === 'ha-gamma');
        if (!sbAlpha || !sbAlpha.meta.startsWith('진행')) bugs.push({ severity: 'Major', symptom: `사이드바 진행 프로젝트 meta에 "진행" 구분이 없음(실제: ${sbAlpha?.meta})`, file: 'src/sidebar.ts:wsRow' });
        if (!sbGamma || !sbGamma.meta.startsWith('보관')) bugs.push({ severity: 'Major', symptom: `사이드바 보관 프로젝트 meta에 "보관" 구분이 없음(실제: ${sbGamma?.meta})`, file: 'src/sidebar.ts:wsRow' });
        if ((await page.locator('.project-sessions-error').count()) !== 0) bugs.push({ severity: 'Major', symptom: '세션 로드 성공인데 목록 에러 신호가 표시됨', file: 'src/views/projects.ts' });
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
        // 뒤로가기 목적지(back=project:<path>)가 해시 쿼리스트링에 인코딩됐는지 확인.
        if (!hash.includes('back=project:')) {
          bugs.push({ severity: 'Critical', symptom: `프로젝트 상세 세션 섹션에서 세션 클릭 시 뒤로가기 목적지(back=project:)가 해시에 인코딩되지 않음(실제: ${hash})`, file: 'src/sessionRow.ts / src/views/projects.ts' });
        }

        // 사용자 보고 버그(가장 중요) — 프로젝트 상세→세션 클릭→상세→뒤로가기는
        // 세션 목록이 아니라 그 프로젝트 상세로 가야 한다.
        await page.waitForSelector('.chat-page');
        await shot('02b-session-detail-from-project');
        await page.locator('.back-link', { hasText: '← 세션' }).click();
        await page.waitForTimeout(150);
        const backHash = await page.evaluate(() => window.location.hash);
        const expectedBackHash = `#/project/${encodeURIComponent(ALPHA)}`;
        if (backHash !== expectedBackHash) {
          bugs.push({ severity: 'Critical', symptom: `프로젝트 상세→세션 클릭→상세→뒤로가기가 그 프로젝트 상세(${expectedBackHash})로 가지 않음(실제: ${backHash}) — 사용자 보고 버그`, file: 'src/views/sessions.ts:renderSessionDetailView' });
        }
        await page.waitForSelector('.project-sessions-box');
        await shot('02c-back-to-project-detail');

        // (C) 홈
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForSelector('.greet-sub');
        await page.waitForTimeout(200);
        await shot('03-home-tile');
        const sub = await page.locator('.greet-sub').textContent();
        if (!sub || !sub.includes('실행 중 세션 2개 · 1개 프로젝트에서')) bugs.push({ severity: 'Critical', symptom: `홈 인사말 요약 불일치(실제: ${sub})`, file: 'src/views/home.ts:renderGreeting' });
        const tip = await page.locator('.greet-sub [title]').getAttribute('title');
        if (!tip || !tip.includes('워크스페이스')) bugs.push({ severity: 'Minor', symptom: '"워크스페이스 기준" 툴팁 없음', file: 'src/views/home.ts' });
        // 귀속 프로젝트명 — 하위 디렉터리(src-tauri) 세션도 홈 최근 세션에서 ha-alpha여야 한다.
        const accents = await page.locator('.blist-row--sessions .blist-accent').allTextContents();
        if (accents.length === 0 || accents.some((a) => a !== 'ha-alpha' && a !== 'ha-beta')) bugs.push({ severity: 'Critical', symptom: `홈 최근 세션 프로젝트명이 귀속 프로젝트가 아님: ${JSON.stringify(accents)}`, file: 'src/sessionRow.ts' });
        const tile = page.locator('.stat', { hasText: '실행 중 세션' });
        const st = await page.locator('.stat', { hasText: '진행 프로젝트' }).count();
        if (st !== 1) bugs.push({ severity: 'Major', symptom: '"진행 프로젝트" 타일 라벨 없음', file: 'src/views/home.ts' });
        const tt = await tile.textContent();
        if (!tt || !tt.includes('2') || !tt.includes('1개 프로젝트에서')) bugs.push({ severity: 'Critical', symptom: `홈 타일 값/트렌드 불일치(실제: ${tt})`, file: 'src/views/home.ts:renderStatRow' });
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
        checkSidebarGood(bugs, await sidebarRows(page), [], 'none-running');
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
        if (tile && tile.includes('개 프로젝트에서')) bugs.push({ severity: 'Major', symptom: `0건인데 타일 trend가 표시됨(실제: ${tile})`, file: 'src/views/home.ts' });
        const sub = await page.locator('.greet-sub').textContent();
        // M3: 0건이면 타일과 같은 규칙으로 "· 0개 프로젝트에서" 꼬리가 없어야 한다.
        if (!sub || !sub.includes('실행 중 세션 0개') || sub.includes('프로젝트에서')) bugs.push({ severity: 'Major', symptom: `0건 인사말에 꼬리가 남았거나 문구 불일치(실제: ${sub})`, file: 'src/views/home.ts' });
      },
    },
    {
      id: 'windows-path',
      fixtures: { ...base, list_workspace_projects: { projects: [project('ha-win', WIN)], skipped: [] }, list_claude_sessions: SESSIONS_WIN, list_project_tree: [] },
      async run(page, { shot, bugs }) {
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForSelector('.blist-row--sessions .blist-accent');
        await shot('01-home-windows');
        const a = await page.locator('.blist-row--sessions .blist-accent').first().textContent();
        if (a !== 'ha-win') bugs.push({ severity: 'Critical', symptom: `Windows projectPath 이름 불일치(실제: ${a})`, file: 'src/views/sessions.ts:projectNameFromCwd' });
      },
    },
    {
      id: 'sessions-error',
      fixtures: { ...base, ...common([]), list_claude_sessions: { __throw: '테스트 강제 에러: 세션 목록 로드 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.project-card');
        await shot('01-cards-error');
        if ((await page.locator('.project-card-running').count()) !== 0) bugs.push({ severity: 'Critical', symptom: '세션 로드 실패인데 카드 인디케이터 표시(거짓 값)', file: 'src/views/projects.ts' });
        // M2: 목록 화면에도 세션 로드 실패 신호 + 다시 시도가 있어야 한다(첫 로드 실패 = 값 없음).
        const errBar = page.locator('.project-sessions-error');
        if ((await errBar.count()) !== 1) bugs.push({ severity: 'Major', symptom: '프로젝트 목록에 세션 로드 실패 신호가 없음(실행 0과 모름 구분 불가)', file: 'src/views/projects.ts' });
        else {
          const et = await errBar.textContent();
          if (!et.includes('세션 상태를 불러오지 못했습니다') || !et.includes('확인할 수 없습니다')) bugs.push({ severity: 'Major', symptom: `첫 로드 실패 문구 불일치(실제: ${et})`, file: 'src/views/projects.ts' });
          if ((await errBar.getByRole('button', { name: '다시 시도' }).count()) !== 1) bugs.push({ severity: 'Major', symptom: '목록 에러 신호에 다시 시도 버튼 없음', file: 'src/views/projects.ts' });
        }
        checkSidebarGood(bugs, await sidebarRows(page), [], 'sessions-error');
        await page.locator('.project-card', { hasText: 'ha-alpha' }).getByRole('button', { name: /상세 보기/ }).click();
        await page.waitForSelector('.project-sessions-box');
        await shot('02-detail-error');
        const t = await page.locator('.project-sessions-box').textContent();
        if (!t || !t.includes('테스트 강제 에러')) bugs.push({ severity: 'Critical', symptom: `상세 세션 섹션에 에러 블록이 없음(실제: ${t})`, file: 'src/views/projects.ts' });
        if (t && t.includes('최근 세션이 없습니다')) bugs.push({ severity: 'Critical', symptom: '로드 실패를 "세션 없음"(거짓 0)으로 표시', file: 'src/views/projects.ts' });
      },
    },
    {
      // M2: 이미 성공한 값이 있는 상태에서 이후 로드가 실패 — 값은 유지, 신호는 표시.
      id: 'sessions-error-stale',
      fixtures: { ...base, ...common(SESSIONS_RUNNING) },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForSelector('.project-card-running');
        await page.evaluate(() => {
          const orig = window.__TAURI_INTERNALS__.invoke;
          window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
            if (cmd === 'list_claude_sessions') throw new Error('테스트 강제 에러: 이후 로드 실패');
            return orig(cmd, args);
          };
        });
        await emitEvent('claude-sessions-changed', null);
        await page.waitForSelector('.project-sessions-error');
        await shot('01-cards-stale-error');
        const alpha = await cardText(page, 'ha-alpha');
        if (!alpha || !alpha.includes('실행 중 2')) bugs.push({ severity: 'Critical', symptom: `실패 후 카드가 마지막 성공값을 잃음(실제: ${alpha})`, file: 'src/views/projects.ts' });
        const et = await page.locator('.project-sessions-error').textContent();
        if (!et.includes('마지막으로 확인된 값')) bugs.push({ severity: 'Major', symptom: `stale 안내 문구 없음(실제: ${et})`, file: 'src/views/projects.ts' });
        checkSidebarGood(bugs, await sidebarRows(page), ['ha-alpha'], 'stale-error');
      },
    },
    {
      // M1: 먼저 시작한 느린 요청(옛 스냅숏=실행 중)이 나중에 시작한 빠른 요청(최신=0건)을 덮어쓰면 안 된다.
      id: 'load-race',
      fixtures: { ...base, ...common(SESSIONS_NONE_RUNNING) },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForSelector('.project-card');
        await page.evaluate(({ stale, fresh }) => {
          const orig = window.__TAURI_INTERNALS__.invoke;
          let n = 0;
          window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
            if (cmd !== 'list_claude_sessions') return orig(cmd, args);
            n += 1;
            if (n === 1) {
              await new Promise((r) => setTimeout(r, 700));
              return stale;
            }
            return fresh;
          };
        }, { stale: SESSIONS_RUNNING, fresh: SESSIONS_NONE_RUNNING });
        await emitEvent('claude-sessions-changed', null); // 느린 1번째(옛 값)
        await page.waitForTimeout(100);
        await emitEvent('claude-sessions-changed', null); // 빠른 2번째(최신 값)
        await page.waitForTimeout(1100); // 느린 응답이 뒤늦게 도착할 시간까지
        await shot('01-after-race');
        if ((await page.locator('.project-card-running').count()) !== 0) bugs.push({ severity: 'Major', symptom: '늦게 도착한 이전 스냅숏이 최신 값을 덮어 "실행 중"이 남음(M1 회귀)', file: 'src/views/sessions.ts:loadSessions' });
        checkSidebarGood(bugs, await sidebarRows(page), [], 'load-race');
        const loadingBtn = await page.getByRole('button', { name: '↻ 새로고침' }).count();
        if (loadingBtn < 1) bugs.push({ severity: 'Minor', symptom: '레이스 후 화면이 정상 상태가 아님', file: 'src/views/projects.ts' });
      },
    },
    {
      // N2: 표시 상한(10)으로 잘리면 헤더가 상한을 드러내야 한다("최근 23"인데 8행만 보이는 모순 금지).
      id: 'many-sessions',
      fixtures: {
        ...base,
        ...common([
          session('mmmmmmmm-r1', ALPHA, ALPHA, '실행 1', { running: true, minAgo: 1 }),
          session('mmmmmmmm-r2', ALPHA, ALPHA, '실행 2', { running: true, minAgo: 2 }),
          ...Array.from({ length: 14 }, (_, i) => session(`mmmmmmmm-f${i}`, ALPHA, ALPHA, `끝난 ${i}`, { minAgo: 10 + i })),
        ]),
      },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.project-card');
        await page.locator('.project-card', { hasText: 'ha-alpha' }).getByRole('button', { name: /상세 보기/ }).click();
        await page.waitForSelector('.project-sessions-box');
        await shot('01-detail-capped');
        const box = page.locator('.project-sessions-box');
        const rows = await box.locator('.blist-row--sessions').count();
        const head = await box.locator('.box-head').textContent();
        if (rows !== 10) bugs.push({ severity: 'Major', symptom: `표시 행 수 10 기대, 실제 ${rows}`, file: 'src/views/projects.ts' });
        if (!head || !head.includes('최근 14') || !head.includes(`${rows}건 표시`)) bugs.push({ severity: 'Major', symptom: `헤더가 표시 상한을 드러내지 않음(실제: ${head})`, file: 'src/views/projects.ts:renderProjectSessionsBox' });
      },
    },
  ];
}
