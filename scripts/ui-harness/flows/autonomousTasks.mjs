// 흐름 (1) 자율 작업 상세화면/이력목록 — src/views/autonomousTasks.ts (1649줄, 가장 크고 위험)
import { historyEntries, manyAutonomyGroups } from '../lib/fixtures.mjs';

export const flowId = 'autonomousTasks';
export const startHash = '#/tasks';

function runtimeFor(projectPath, taskId, overrides = {}) {
  return { projectPath, taskId, running: false, lastStartedAt: null, lastFinishedAt: null, nextRunAt: null, status: null, summary: null, durationMs: null, logPath: null, ...overrides };
}

// ---- R14 P2-4 사전조건 배너용 스텁 — 백엔드가 실제로 내는 모양을 따른다 ----
// check_dev_tools: src-tauri/src/dev_tools/contract.rs:8-25 DevToolStatus(camelCase).
//   claude 미설치는 query.rs:33-56의 `None` 분기와 같다(installed=false, version/path=null,
//   actionKind='run', required=true).
// check_claude_auth_status: src-tauri/src/claude_auth.rs:320-327 ClaudeAuthStatus. 미설치·타임아웃·
//   파싱 실패는 Err(String)이라 invoke가 reject된다(claude_auth.rs:371-398) → __throw로 흉내낸다.
// list_installed_plugins: src-tauri/src/plugins/installed.rs:22-34 InstalledPlugin(id는 "name@marketplace").
const PREREQ_TOOLS_OK = [
  { id: 'claude', name: 'Claude Code', installed: true, version: '2.1.0', path: '/Users/hopegiver/.local/bin/claude', installMethod: 'native', actionKind: 'manual', manualHint: null, required: true, installDependsOn: [] },
  { id: 'node', name: 'Node.js', installed: true, version: '22.9.0', path: '/usr/local/bin/node', installMethod: 'brew', actionKind: 'manual', manualHint: null, required: true, installDependsOn: [] },
];
const PREREQ_TOOLS_NO_CLAUDE = [
  { id: 'claude', name: 'Claude Code', installed: false, version: null, path: null, installMethod: null, actionKind: 'run', manualHint: null, required: true, installDependsOn: [] },
  PREREQ_TOOLS_OK[1],
];
const PREREQ_AUTH_IN = { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' };
const PREREQ_AUTH_OUT = { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null };
const prereqPlugin = (id, name) => ({ id, name, displayName: null, version: '1.8.52', description: `${name} 플러그인`, installPath: `/Users/hopegiver/.claude/plugins/cache/${name}`, agents: [], skills: [], knowledge: [] });
const PREREQ_PLUGINS_OK = [prereqPlugin('malgn-agent@malgnsoft-plugins', 'malgn-agent')];

export function scenarios(rawBase) {
  // 기존 시나리오 전체가 "사전조건 충족" 상태에서 돌도록 베이스를 고정한다(공용 fixtures.mjs의
  // 기본 auth는 loggedIn=false라 그대로 두면 모든 시나리오에 로그인 배너가 섞인다).
  const base = { ...rawBase, check_dev_tools: PREREQ_TOOLS_OK, check_claude_auth_status: PREREQ_AUTH_IN, list_installed_plugins: PREREQ_PLUGINS_OK };
  const prereqKinds = async (page) => page.locator('.autonomy-prereq-banner').evaluateAll((els) => els.map((e) => e.getAttribute('data-prereq')));
  const sameSet = (a, b) => a.length === b.length && [...a].sort().join(',') === [...b].sort().join(',');
  const expectKinds = async (page, bugs, expected, repro) => {
    const got = await prereqKinds(page);
    if (!sameSet(got, expected)) {
      bugs.push({ severity: 'Major', symptom: `사전조건 배너 종류가 기대와 다름(기대: [${expected}] / 실제: [${got}])`, file: 'src/autonomyPrereq.ts:computeAutonomyPrereqs / src/views/autonomousTasks.ts:renderPrereqBanners', repro });
    }
    return got;
  };
  const expectRowsStillRender = async (page, bugs, repro) => {
    const rows = await page.locator('.task-row').count();
    if (rows === 0) bugs.push({ severity: 'Critical', symptom: '사전조건 배너가 떠 있는 동안 자율 작업 목록이 렌더되지 않음', file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView', repro });
    // 재진입 렌더(리뷰 2026-09-17 M1)는 #app 안에 같은 사이드바/본문이 2벌 쌓인다 — 자식 수는 토스트
    // 컨테이너 등 때문에 고정값이 아니므로 "같은 태그+클래스 자식의 중복"으로 판정한다.
    const dup = await page.evaluate(() => {
      const seen = new Map();
      for (const c of document.getElementById('app')?.children ?? []) {
        if (c.id === 'toast' || c.className.includes?.('toast')) continue;
        const k = `${c.tagName}.${c.className}`;
        seen.set(k, (seen.get(k) ?? 0) + 1);
      }
      return [...seen.entries()].filter(([, n]) => n > 1).map(([k]) => k);
    });
    if (dup.length > 0) bugs.push({ severity: 'Major', symptom: `#app에 같은 자식이 중복됨(${dup}) — 재진입 렌더 의심`, file: 'src/views/autonomousTasks.ts', repro });
  };
  return [
    {
      id: 'golden',
      fixtures: {
        ...base,
        // 이력 조회는 목록/런타임과 별도 IPC — 실제 task id 유무와 무관하게
        // 고정된 샘플 이력을 반환한다(bridge.mjs는 커맨드당 인자 무관 고정값).
        autonomy_task_history: historyEntries(5),
      },
      async run(page, { shot, bugs, getListenerCount }) {
        await shot('01-list');
        const rows = page.locator('.task-row');
        const rowCount = await rows.count();
        if (rowCount === 0) {
          bugs.push({ severity: 'Major', symptom: 'golden 시나리오인데 자율 작업 목록이 비어 있음(실제 .claude/autonomy.json에 활성 task가 없을 수 있음 — 데이터 이슈, 코드 버그 아닐 수 있음)', file: 'scripts/ui-harness/flows/autonomousTasks.mjs', repro: '#/tasks 진입' });
          return;
        }

        // ---- 추가 모달: ESC로 닫기 ----
        const before = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 새 자율 작업' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-add-modal');
        await page.keyboard.press('Escape');
        await page.waitForSelector('.modal-overlay', { state: 'detached' }).catch(() => {
          bugs.push({ severity: 'Critical', symptom: 'ESC를 눌러도 "새 자율 작업" 추가 모달이 닫히지 않음', file: 'src/views/autonomousTasks.ts', repro: '#/tasks → "+ 새 자율 작업" 클릭 → ESC' });
        });
        const afterEsc = await getListenerCount('keydown');
        if (afterEsc > before) {
          bugs.push({ severity: 'Minor', symptom: `ESC로 모달을 닫은 뒤에도 keydown 리스너가 정리되지 않음(before=${before}, after=${afterEsc})`, file: 'src/views/autonomousTasks.ts:detachTaskFormModalEscHandler', repro: '+ 새 자율 작업 열기 → ESC → 리스너 카운트 확인' });
        }

        // ---- 배경 클릭으로 닫기 ----
        await page.getByRole('button', { name: '+ 새 자율 작업' }).click();
        await page.waitForSelector('.modal-overlay');
        await page.locator('.modal-overlay').click({ position: { x: 5, y: 5 } });
        const stillOpen = await page.locator('.modal-overlay').count();
        if (stillOpen > 0) {
          bugs.push({ severity: 'Major', symptom: '배경(오버레이) 클릭으로 "새 자율 작업" 모달이 닫히지 않음', file: 'src/dom.ts:createModalOverlay', repro: '+ 새 자율 작업 열기 → 모달 바깥 클릭' });
          await page.keyboard.press('Escape'); // 다음 단계 진행을 위해 강제로 닫는다
        }

        // ---- 모달 연 채로 라우트 이탈 → leaveAutonomousTasksListView가 리스너 정리하는지 ----
        await page.getByRole('button', { name: '+ 새 자율 작업' }).click();
        await page.waitForSelector('.modal-overlay');
        const beforeLeave = await getListenerCount('keydown');
        await page.evaluate(() => {
          window.location.hash = '#/';
        });
        await page.waitForTimeout(150);
        const afterLeave = await getListenerCount('keydown');
        if (afterLeave >= beforeLeave && beforeLeave > 0) {
          bugs.push({
            severity: 'Major',
            symptom: `모달을 연 채로 다른 라우트로 이동해도 keydown 리스너가 정리되지 않음(누수) — before=${beforeLeave}, after=${afterLeave}`,
            file: 'src/main.ts:leaveAutonomousTasksListView 호출부 / src/views/autonomousTasks.ts:leaveAutonomousTasksListView',
            repro: '#/tasks → "+ 새 자율 작업" 열기(닫지 않음) → location.hash를 "#/"로 변경 → keydown 리스너 카운트 확인',
          });
        }
        await page.evaluate(() => {
          window.location.hash = '#/tasks';
        });
        await page.waitForTimeout(150);

        // ---- 상세 화면 진입 + 이력 펼치기 ----
        await page.locator('.task-row-main').first().click();
        await page.waitForSelector('.task-detail-grid');
        await shot('03-detail');
        const toggleBtn = page.locator('.run-history-row button.btn-sm').first();
        if ((await toggleBtn.count()) > 0) {
          await toggleBtn.click();
          await shot('04-detail-history-expanded');
        } else {
          const hasHistoryEmptyState = await page.locator('.run-history-card .state-block-title').count();
          if (hasHistoryEmptyState === 0) {
            bugs.push({ severity: 'Minor', symptom: '이력 카드가 로딩/빈 상태/목록 어느 것도 렌더하지 않음', file: 'src/views/autonomousTasks.ts:renderRunHistoryCard', repro: '#/tasks/item/<id> 진입' });
          }
        }

        // ---- 진행상황판 탭 ----
        await page.getByRole('button', { name: '진행상황판' }).click().catch(async () => {
          await page.evaluate(() => { window.location.hash = '#/tasks/board'; });
        });
        await page.waitForTimeout(200);
        await shot('05-board');
      },
    },
    {
      id: 'empty',
      fixtures: { ...base, autonomy_list: [], autonomy_runtime_status: [] },
      async run(page, { shot, bugs }) {
        await shot('01-empty');
        const emptyTitle = await page.locator('.state-block-title').first().textContent().catch(() => null);
        if (!emptyTitle || !emptyTitle.includes('없습니다')) {
          bugs.push({ severity: 'Major', symptom: `빈 목록인데 "등록된 자율 작업이 없습니다" 안내가 보이지 않음(실제: ${emptyTitle})`, file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView', repro: 'autonomy_list=[] 로 #/tasks 진입' });
        }
      },
    },
    {
      // hub 이슈 01m2wm499xmh3046rnvx4cyn8n 재발 방지 — 프로젝트 폴더가 실제로
      // 있는데도 CLAUDE.md가 없어(또는 workspace 루트를 읽지 못해) 조용히
      // 제외되던 문제. list_workspace_projects가 0건 + skipped 몇 건을 반환할
      // 때, "새 자율 작업" 폼의 빈 프로젝트 드롭다운에 스캔 조건 설명과 제외
      // 사유 요약이 실제로 뜨는지 못박는다(기존 'empty' 시나리오의 판정 로직은
      // 건드리지 않고 이 시나리오만 추가한다).
      id: 'empty-workspace-scan',
      fixtures: {
        ...base,
        list_workspace_projects: {
          projects: [],
          skipped: [
            { name: 'no-claude-project', path: '/Users/hopegiver/workspace/no-claude-project', reason: 'noClaudeMd' },
            { name: 'legacy-tool', path: '/Users/hopegiver/workspace/legacy-tool', reason: 'noClaudeMd' },
            { name: 'D-workspace', path: 'D:\\workspace', reason: 'rootUnreadable' },
          ],
        },
      },
      async run(page, { shot, bugs }) {
        await page.getByRole('button', { name: '+ 새 자율 작업' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('01-empty-project-select-hint');

        const projectField = page
          .locator('.modal-body .settings-field', { has: page.locator('.settings-field-label', { hasText: '프로젝트' }) })
          .first();
        const hints = await projectField.locator('.settings-form-hint').allTextContents();
        const combined = hints.join(' / ');

        if (!hints.some((h) => h.includes('CLAUDE.md'))) {
          bugs.push({
            severity: 'Major',
            symptom: `프로젝트 0건인데 스캔 조건 안내(CLAUDE.md 언급)가 보이지 않음(실제: ${combined || '(없음)'})`,
            file: 'src/views/autonomousTasks.ts:renderTaskForm',
            repro: 'list_workspace_projects=[]로 #/tasks → "+ 새 자율 작업" 클릭',
          });
        }
        if (!hints.some((h) => h.includes('CLAUDE.md가 없어 제외'))) {
          bugs.push({
            severity: 'Major',
            symptom: `제외된 폴더(skipped)가 있는데 제외 사유 요약이 보이지 않음(실제: ${combined || '(없음)'})`,
            file: 'src/views/autonomousTasks.ts:renderTaskForm',
          });
        }
        if (!hints.some((h) => h.includes('no-claude-project'))) {
          bugs.push({
            severity: 'Minor',
            symptom: `제외 사유 요약에 실제 폴더 이름이 포함되지 않음(실제: ${combined || '(없음)'})`,
            file: 'src/workspaceScanHint.ts:summarizeSkippedProjects',
          });
        }
        if (!hints.some((h) => h.includes('읽을 수 없습니다'))) {
          bugs.push({
            severity: 'Minor',
            symptom: `workspace 루트를 읽지 못한 경우(rootUnreadable) 안내가 보이지 않음(실제: ${combined || '(없음)'})`,
            file: 'src/workspaceScanHint.ts:summarizeSkippedProjects',
          });
        }

        const select = projectField.locator('select');
        if (await select.isEnabled()) {
          bugs.push({
            severity: 'Major',
            symptom: '프로젝트가 0건인데 프로젝트 선택 드롭다운이 활성화되어 있음',
            file: 'src/views/autonomousTasks.ts:renderTaskForm',
          });
        }
      },
    },
    {
      id: 'single',
      fixtures: {
        ...base,
        autonomy_list: [{ projectPath: '/Users/hopegiver/workspace/single-project', projectName: 'single-project', tasks: [{ id: 'single-task', name: '단일 자율 작업', prompt: '단일 케이스 프롬프트', subagent: null, interval: 60, scheduleMode: 'interval', atTime: null, days: [], hourlyMinute: null, cron: null, enabled: true, timeout: null }] }],
        autonomy_runtime_status: [runtimeFor('/Users/hopegiver/workspace/single-project', 'single-task', { nextRunAt: new Date(Date.now() + 3600_000).toISOString() })],
        autonomy_task_history: [],
      },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.task-row').count();
        if (count !== 1) bugs.push({ severity: 'Major', symptom: `1건 픽스처인데 행이 ${count}개 렌더됨`, file: 'src/views/autonomousTasks.ts', repro: 'autonomy_list에 task 1건' });
        await shot('01-single');
        await page.locator('.task-row-main').first().click();
        await page.waitForSelector('.task-detail-grid');
        await shot('02-single-detail-empty-history');
      },
    },
    {
      id: 'large',
      fixtures: (() => {
        const { groups, runtimes } = manyAutonomyGroups(120);
        return { ...base, autonomy_list: groups, autonomy_runtime_status: runtimes, autonomy_task_history: historyEntries(3) };
      })(),
      async run(page, { shot, bugs }) {
        const count = await page.locator('.task-row').count();
        if (count !== 120) bugs.push({ severity: 'Minor', symptom: `120건 픽스처인데 ${count}건만 렌더됨(가상화/페이지네이션 없음이 의도인지 확인 필요)`, file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView' });
        await shot('01-large-list');
        // 긴 한글/특수문자/스크립트 문자열이 레이아웃을 깨뜨리는지 — 첫 행의 폭이
        // 뷰포트를 벗어나는지로 판정한다.
        const box = await page.locator('.task-row').first().boundingBox();
        const vw = page.viewportSize()?.width ?? 1180;
        if (box && box.width > vw + 5) {
          bugs.push({ severity: 'Minor', symptom: `긴 이름/특수문자 task 행의 폭(${Math.round(box.width)}px)이 뷰포트(${vw}px)를 초과 — 레이아웃 오버플로 의심`, file: 'src/views/autonomousTasks.ts:renderTaskRow' });
        }
        await page.locator('.task-board-column').first().waitFor().catch(() => {});
        await page.evaluate(() => { window.location.hash = '#/tasks/board'; });
        await page.waitForTimeout(300);
        await shot('02-large-board');
      },
    },
    {
      id: 'error',
      fixtures: { ...base, autonomy_list: { __throw: '전역 설정(malgn-agent.json)이 손상되었습니다: 테스트 강제 에러' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const alertVisible = await page.locator('.alert').count();
        if (alertVisible === 0) {
          bugs.push({ severity: 'Critical', symptom: 'autonomy_list invoke가 throw해도 에러 배너(.alert)가 뜨지 않음 — 무한 로딩/빈 화면 가능성', file: 'src/views/autonomousTasks.ts:loadAutonomousTasks', repro: 'autonomy_list fixture를 __throw로 설정하고 #/tasks 진입' });
        }
        await shot('01-error');
        const retryBtn = page.getByRole('button', { name: '다시 시도' });
        if ((await retryBtn.count()) === 0) {
          bugs.push({ severity: 'Minor', symptom: '에러 배너에 "다시 시도" 버튼이 없음', file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView' });
        }
      },
    },
    {
      id: 'slow',
      fixtures: { ...base, autonomy_list: { __delay: 2500, value: base.autonomy_list }, autonomy_runtime_status: { __delay: 2500, value: base.autonomy_runtime_status } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const loadingVisible = await page.locator('.state-block-title:has-text("불러오는 중")').count();
        await shot('01-slow-loading');
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 응답 동안 "불러오는 중…" 로딩 표시가 뜨지 않음', file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView', repro: 'autonomy_list를 2500ms 지연시키고 #/tasks 진입 직후 400ms 시점 확인' });
        }
        await page.waitForTimeout(2500);
        await shot('02-slow-loaded');
        const rowsAfter = await page.locator('.task-row').count();
        if (rowsAfter === 0 && base.autonomy_list.length > 0) {
          bugs.push({ severity: 'Critical', symptom: '지연 응답이 도착한 뒤에도 목록이 렌더되지 않음(무한 로딩)', file: 'src/views/autonomousTasks.ts:loadAutonomousTasks' });
        }
      },
    },
    // ---- 스케줄러 heartbeat 정체 배너 (리뷰 D1 권고 ③, PM 지시서) ----
    // autonomy_scheduler_health의 lastTickAt/now는 chrono DateTime<Utc>가
    // RFC3339 문자열로 직렬화된 값이다(src-tauri/src/autonomy/mod.rs:337-343
    // 확인) — 픽스처도 동일하게 ISO 문자열로 준다.
    {
      id: 'scheduler-stall',
      fixtures: (() => {
        const now = new Date();
        // 임계값(STALL_THRESHOLD_MULTIPLIER=6 × tickSeconds=10초 = 60초)을
        // 넉넉히 초과하는 120초 전 tick — 확실한 정체 케이스.
        const stale = new Date(now.getTime() - 120_000);
        return { ...base, autonomy_scheduler_health: { lastTickAt: stale.toISOString(), now: now.toISOString(), tickSeconds: 10 } };
      })(),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const banner = page.locator('.alert', { hasText: '예약된 자율 작업이 실행되지 않고 있습니다' });
        if ((await banner.count()) === 0) {
          bugs.push({
            severity: 'Critical',
            symptom: '스케줄러 heartbeat가 임계값(60초)을 훨씬 넘겨 정체됐는데도 정체 배너가 뜨지 않음',
            file: 'src/views/autonomousTasks.ts:renderSchedulerStallBanner',
            repro: 'autonomy_scheduler_health.lastTickAt을 120초 전으로 픽스처 주입하고 #/tasks 진입',
          });
        }
        await shot('01-scheduler-stall-banner');
      },
    },
    {
      id: 'scheduler-healthy',
      fixtures: (() => {
        const now = new Date();
        const recent = new Date(now.getTime() - 5_000); // 임계값(60초) 안쪽의 정상 지연
        return { ...base, autonomy_scheduler_health: { lastTickAt: recent.toISOString(), now: now.toISOString(), tickSeconds: 10 } };
      })(),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const banner = page.locator('.alert', { hasText: '예약된 자율 작업이 실행되지 않고 있습니다' });
        if ((await banner.count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: '스케줄러 heartbeat가 정상(5초 전)인데도 정체 배너가 표시됨(오탐)',
            file: 'src/views/autonomousTasks.ts:renderSchedulerStallBanner',
            repro: 'autonomy_scheduler_health.lastTickAt을 5초 전으로 픽스처 주입하고 #/tasks 진입',
          });
        }
        await shot('01-scheduler-healthy-no-banner');
      },
    },
    {
      id: 'scheduler-not-ticked-yet',
      fixtures: { ...base, autonomy_scheduler_health: { lastTickAt: null, now: new Date().toISOString(), tickSeconds: 10 } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const banner = page.locator('.alert', { hasText: '예약된 자율 작업이 실행되지 않고 있습니다' });
        if ((await banner.count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: 'lastTickAt이 아직 없는(앱 기동 직후, 첫 tick 전) 정상 상태인데도 정체 배너가 표시됨(오판)',
            file: 'src/views/autonomousTasks.ts:isSchedulerStalled',
            repro: 'autonomy_scheduler_health.lastTickAt을 null로 픽스처 주입하고 #/tasks 진입',
          });
        }
        await shot('01-scheduler-not-ticked-yet-no-banner');
      },
    },
    {
      // A3 가드(리뷰 v0.2.5) 실측 — 이 화면(home/tasks-*)에 있을 때만 스케줄러
      // heartbeat를 폴링하고, 벗어나면 멈추는지를 실제 IPC 호출 로그
      // (window.__invokeLog)로 확인한다. 폴링 주기가 30초라 두 구간(온/오프
      // 라우트)을 real-time으로 기다린다 — 이 시나리오만 따로 돌릴 때는 넉넉한
      // 타임아웃이 필요하다(`node scripts/ui-harness/run.mjs autonomousTasks scheduler-poll-gate`).
      id: 'scheduler-poll-gate',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        const countCalls = async () =>
          page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'autonomy_scheduler_health').length);

        const initialCount = await countCalls();
        if (initialCount === 0) {
          bugs.push({
            severity: 'Major',
            symptom: '#/tasks 진입 직후 최초 스케줄러 heartbeat 조회(autonomy_scheduler_health)가 호출되지 않음',
            file: 'src/views/autonomousTasks.ts:ensureSchedulerHealthWatcher',
          });
        }

        // ---- 화면 안(자율 작업 라우트): 폴링이 실제로 도는지 ----
        await page.waitForTimeout(31000);
        const afterOnRoute = await countCalls();
        if (afterOnRoute <= initialCount) {
          bugs.push({
            severity: 'Major',
            symptom: `자율 작업 화면에 머무는 동안(31초 대기) 스케줄러 heartbeat 폴링이 추가로 호출되지 않음(before=${initialCount}, after=${afterOnRoute})`,
            file: 'src/views/autonomousTasks.ts:ensureSchedulerHealthWatcher',
          });
        }
        await shot('01-poll-on-route');

        // ---- 화면 밖(무관한 라우트): 폴링이 멈추는지(A3 가드) ----
        await page.evaluate(() => {
          window.location.hash = '#/catalog';
        });
        await page.waitForTimeout(300);
        const beforeOffRoute = await countCalls();
        await page.waitForTimeout(31000);
        const afterOffRoute = await countCalls();
        if (afterOffRoute > beforeOffRoute) {
          bugs.push({
            severity: 'Major',
            symptom: `자율 작업 화면을 벗어난 동안(31초 대기)에도 스케줄러 heartbeat가 계속 폴링됨(A3 가드 미적용, before=${beforeOffRoute}, after=${afterOffRoute})`,
            file: 'src/views/autonomousTasks.ts:checkSchedulerHealth',
          });
        }
        await shot('02-poll-off-route');
      },
    },
    // ---- R14 P2-4 사전조건 배너 ----
    {
      id: 'prereq-all-ok',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await expectKinds(page, bugs, [], '#/tasks (claude 설치·로그인·malgn-agent 플러그인 모두 충족)');
        await expectRowsStillRender(page, bugs, '#/tasks, 사전조건 충족');
        await shot('01-prereq-all-ok-no-banner');
      },
    },
    {
      id: 'prereq-claude-missing',
      fixtures: {
        ...base,
        check_dev_tools: PREREQ_TOOLS_NO_CLAUDE,
        // claude가 없으면 실제로는 resolve_claude()가 Err를 낸다(claude_auth.rs:371-373).
        check_claude_auth_status: { __throw: 'claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).' },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        // 미설치가 확정이면 로그인/조회 실패 배너를 중복해 내지 않는다.
        await expectKinds(page, bugs, ['claude-missing'], 'check_dev_tools: claude installed=false,path=null');
        await expectRowsStillRender(page, bugs, '#/tasks, claude 미설치');
        await shot('01-prereq-claude-missing');
        await page.evaluate(() => { window.location.hash = '#/tasks/board'; });
        await page.waitForTimeout(300);
        await expectKinds(page, bugs, ['claude-missing'], '#/tasks/board, claude 미설치');
        await shot('02-prereq-claude-missing-board');
        await page.evaluate(() => { window.location.hash = '#/tasks'; });
        await page.waitForTimeout(300);
        await page.locator('[data-prereq="claude-missing"] button').click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/settings/devtools') bugs.push({ severity: 'Major', symptom: `"Claude Code 설치하기"가 개발 환경 탭으로 이동하지 않음(실제 hash: ${hash})`, file: 'src/autonomyPrereq.ts', repro: '배너 버튼 클릭' });
      },
    },
    {
      id: 'prereq-login-required',
      fixtures: { ...base, check_claude_auth_status: PREREQ_AUTH_OUT },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await expectKinds(page, bugs, ['login-required'], 'check_claude_auth_status: loggedIn=false');
        await expectRowsStillRender(page, bugs, '#/tasks, 미로그인');
        await shot('01-prereq-login-required');
        await page.locator('[data-prereq="login-required"] button').click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/' && hash !== '') bugs.push({ severity: 'Major', symptom: `"홈에서 로그인"이 홈으로 이동하지 않음(실제 hash: ${hash})`, file: 'src/autonomyPrereq.ts', repro: '배너 버튼 클릭' });
      },
    },
    {
      id: 'prereq-plugin-missing',
      // 설치된 플러그인이 전혀 없음 → 카탈로그 빈 상태의 "malgn-agent 설치" 버튼으로 안내.
      fixtures: { ...base, list_installed_plugins: [] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await expectKinds(page, bugs, ['plugin-missing'], 'list_installed_plugins=[]');
        await expectRowsStillRender(page, bugs, '#/tasks, 플러그인 없음');
        await shot('01-prereq-plugin-missing');
        await page.locator('[data-prereq="plugin-missing"] button').click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/catalog') bugs.push({ severity: 'Major', symptom: `플러그인이 0개일 때 배너 버튼이 카탈로그로 이동하지 않음(실제 hash: ${hash})`, file: 'src/autonomyPrereq.ts', repro: '배너 버튼 클릭' });
      },
    },
    {
      id: 'prereq-plugin-missing-others-installed',
      // 다른 플러그인만 있음 → 카탈로그에는 설치 버튼이 없으므로 MCP 설정의 malgnai-hub 안내 행으로.
      fixtures: { ...base, list_installed_plugins: [prereqPlugin('other-tool@somewhere', 'other-tool')] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await expectKinds(page, bugs, ['plugin-missing'], 'list_installed_plugins에 malgn-agent 없음(타 플러그인만)');
        await shot('01-prereq-plugin-missing-others');
        await page.locator('[data-prereq="plugin-missing"] button').click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/settings/mcp') bugs.push({ severity: 'Major', symptom: `타 플러그인만 있을 때 배너 버튼이 MCP 설정으로 이동하지 않음(실제 hash: ${hash})`, file: 'src/autonomyPrereq.ts', repro: '배너 버튼 클릭' });
      },
    },
    {
      id: 'prereq-all-missing',
      fixtures: { ...base, check_dev_tools: PREREQ_TOOLS_NO_CLAUDE, check_claude_auth_status: PREREQ_AUTH_OUT, list_installed_plugins: [] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        // 미설치가 확정이면 로그인 배너는 숨고, 플러그인 배너만 함께 뜬다.
        await expectKinds(page, bugs, ['claude-missing', 'plugin-missing'], 'claude 미설치 + 플러그인 없음');
        await expectRowsStillRender(page, bugs, '#/tasks, 복합 미충족');
        await shot('01-prereq-multiple');
      },
    },
    {
      id: 'prereq-loading-no-verdict',
      // 세 조회 모두 지연 — 응답 전에는 "미설치/미로그인"으로 단정하지 않는다(침묵).
      // 응답 도착 후(전부 충족)에도 배너가 없어야 한다.
      fixtures: {
        ...base,
        check_dev_tools: { __delay: 2500, value: PREREQ_TOOLS_OK },
        check_claude_auth_status: { __delay: 2500, value: PREREQ_AUTH_IN },
        list_installed_plugins: { __delay: 2500, value: PREREQ_PLUGINS_OK },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(600);
        await expectKinds(page, bugs, [], '세 조회가 응답하기 전(로딩 중)');
        await shot('01-prereq-loading-silent');
        await page.waitForTimeout(2800);
        await expectKinds(page, bugs, [], '세 조회가 모두 성공하고 충족');
        await shot('02-prereq-loading-then-ok');
      },
    },
    {
      id: 'prereq-check-failed',
      // 세 조회 모두 실패 — 미설치/미로그인/플러그인 없음으로 단정하지 않고 "확인 불가" 한 건만.
      fixtures: {
        ...base,
        check_dev_tools: { __throw: '개발 도구 조회 실패(테스트 강제 에러)' },
        check_claude_auth_status: { __throw: 'claude auth status 조회가 시간 초과되었습니다.' },
        list_installed_plugins: { __throw: '플러그인 목록 읽기 실패(테스트 강제 에러)' },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(600);
        await expectKinds(page, bugs, ['check-failed'], '세 조회 모두 reject');
        await expectRowsStillRender(page, bugs, '#/tasks, 사전조건 조회 실패');
        await shot('01-prereq-check-failed');
      },
    },
  ];
}
