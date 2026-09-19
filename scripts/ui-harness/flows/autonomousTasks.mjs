// 흐름 (1) 자율업무 상세화면/이력목록 — src/views/autonomousTasks.ts (1649줄, 가장 크고 위험)
import { historyEntries, manyAutonomyGroups } from '../lib/fixtures.mjs';

export const flowId = 'autonomousTasks';
export const startHash = '#/tasks';

function runtimeFor(projectPath, taskId, overrides = {}) {
  return { projectPath, taskId, running: false, lastStartedAt: null, lastFinishedAt: null, nextRunAt: null, status: null, summary: null, durationMs: null, logPath: null, ...overrides };
}

export function scenarios(base) {
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
          bugs.push({ severity: 'Major', symptom: 'golden 시나리오인데 자율업무 목록이 비어 있음(실제 .claude/autonomy.json에 활성 task가 없을 수 있음 — 데이터 이슈, 코드 버그 아닐 수 있음)', file: 'scripts/ui-harness/flows/autonomousTasks.mjs', repro: '#/tasks 진입' });
          return;
        }

        // ---- 추가 모달: ESC로 닫기 ----
        const before = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 새 자율업무' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-add-modal');
        await page.keyboard.press('Escape');
        await page.waitForSelector('.modal-overlay', { state: 'detached' }).catch(() => {
          bugs.push({ severity: 'Critical', symptom: 'ESC를 눌러도 "새 자율업무" 추가 모달이 닫히지 않음', file: 'src/views/autonomousTasks.ts', repro: '#/tasks → "+ 새 자율업무" 클릭 → ESC' });
        });
        const afterEsc = await getListenerCount('keydown');
        if (afterEsc > before) {
          bugs.push({ severity: 'Minor', symptom: `ESC로 모달을 닫은 뒤에도 keydown 리스너가 정리되지 않음(before=${before}, after=${afterEsc})`, file: 'src/views/autonomousTasks.ts:detachTaskFormModalEscHandler', repro: '+ 새 자율업무 열기 → ESC → 리스너 카운트 확인' });
        }

        // ---- 배경 클릭으로 닫기 ----
        await page.getByRole('button', { name: '+ 새 자율업무' }).click();
        await page.waitForSelector('.modal-overlay');
        await page.locator('.modal-overlay').click({ position: { x: 5, y: 5 } });
        const stillOpen = await page.locator('.modal-overlay').count();
        if (stillOpen > 0) {
          bugs.push({ severity: 'Major', symptom: '배경(오버레이) 클릭으로 "새 자율업무" 모달이 닫히지 않음', file: 'src/dom.ts:createModalOverlay', repro: '+ 새 자율업무 열기 → 모달 바깥 클릭' });
          await page.keyboard.press('Escape'); // 다음 단계 진행을 위해 강제로 닫는다
        }

        // ---- 모달 연 채로 라우트 이탈 → leaveAutonomousTasksListView가 리스너 정리하는지 ----
        await page.getByRole('button', { name: '+ 새 자율업무' }).click();
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
            repro: '#/tasks → "+ 새 자율업무" 열기(닫지 않음) → location.hash를 "#/"로 변경 → keydown 리스너 카운트 확인',
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
          bugs.push({ severity: 'Major', symptom: `빈 목록인데 "등록된 자율업무가 없습니다" 안내가 보이지 않음(실제: ${emptyTitle})`, file: 'src/views/autonomousTasks.ts:renderAutonomousTasksListView', repro: 'autonomy_list=[] 로 #/tasks 진입' });
        }
      },
    },
    {
      id: 'single',
      fixtures: {
        ...base,
        autonomy_list: [{ projectPath: '/Users/hopegiver/workspace/single-project', projectName: 'single-project', tasks: [{ id: 'single-task', name: '단일 자율업무', prompt: '단일 케이스 프롬프트', subagent: null, interval: 60, scheduleMode: 'interval', atTime: null, days: [], hourlyMinute: null, cron: null, enabled: true, timeout: null }] }],
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
  ];
}
