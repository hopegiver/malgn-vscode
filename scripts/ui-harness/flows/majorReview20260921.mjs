// 흐름 (7) 리뷰 v0.2.5(docs/reviewer/review-v0.2.5-whole-app-2026-09-21.md)가
// 지목한 Major 5건(A2/A3/B1/C3/G1)의 회귀 방지 시나리오. A1(Critical, 폼 입력
// 유실)은 formBackgroundRerender.mjs가 이미 덮는다 — 여기서는 그 위에 얹은
// 포커스·캐럿·접근성·수명주기 결함만 다룬다.
export const flowId = 'majorReview20260921';
export const startHash = '#/';

const BACKGROUND_EVENT = 'claude-sessions-changed';

export function scenarios(base) {
  return [
    // ---------------- A2: 채팅 입력창 포커스·캐럿 복원 ----------------
    {
      id: 'a2-chat-caret-survives-background-rerender',
      startHash: base.list_claude_sessions[0] ? `#/sessions/${encodeURIComponent(base.list_claude_sessions[0].sessionId)}` : '#/sessions',
      fixtures: {
        ...base,
        read_session_transcript: {
          sessionId: base.list_claude_sessions[0]?.sessionId ?? 'none',
          cwd: base.list_claude_sessions[0]?.cwd ?? '/Users/hopegiver/workspace/malgn-vscode',
          transcriptPath: '/dev/null',
          messages: [],
          truncated: false,
          activeTurnId: null,
        },
      },
      async run(page, { shot, bugs }) {
        if (base.list_claude_sessions.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'a2 시나리오용 세션 데이터가 없음(실제 jsonl에서 세션을 못 찾음)', file: 'scripts/ui-harness/flows/majorReview20260921.mjs' });
          return;
        }
        const textarea = page.locator('.chat-input-textarea');
        await textarea.waitFor({ state: 'visible' });
        await textarea.click();
        await textarea.fill('hello world caret test');
        // 캐럿을 문장 중간(5)으로 옮긴다 — 리뷰 실측과 동일하게 "끝이 아닌 위치".
        await page.keyboard.press('Home');
        for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
        const before = await page.evaluate(() => {
          const el = document.activeElement;
          return { tag: el?.tagName, cls: el?.className, start: el?.selectionStart, end: el?.selectionEnd, value: el?.value };
        });
        await shot('01-caret-at-5');

        // 배경 이벤트 1회 — main.ts의 onSessionsChanged 구독이 세션당 1회 살아
        // 있어(라우트 무관) loadSessions() → notifyChange() → root.replaceChildren()
        // 전체 재빌드를 일으킨다(A1이 고친 값 보존과 별개로, A2가 고치기 전엔
        // 포커스가 body로, 캐럿이 끝으로 날아갔다).
        await page.evaluate((name) => {
          const ids = window.__eventCallbackIdsByName?.[name] ?? [];
          for (const id of ids) {
            const cb = window['_cb' + id];
            if (typeof cb === 'function') cb({ event: name, id: 0, payload: null });
          }
        }, BACKGROUND_EVENT);
        await page.waitForTimeout(400);
        await shot('02-after-background-event');

        const after = await page.evaluate(() => {
          const el = document.activeElement;
          const textarea = document.querySelector('.chat-input-textarea');
          return {
            tag: el?.tagName,
            cls: el?.className,
            start: el?.selectionStart,
            end: el?.selectionEnd,
            // 값 보존은 포커스와 독립적으로 확인한다 — 포커스가 body로 날아간
            // 뒤 el.value를 읽으면(body에는 value가 없다) "undefined"라는 잘못된
            // 값 유실 신호가 나온다(포커스 버그와 값 버그를 혼동하면 안 된다).
            textareaValue: textarea?.value,
          };
        });

        if (after.tag !== 'TEXTAREA' || !String(after.cls).includes('chat-input-textarea')) {
          bugs.push({
            severity: 'Major',
            symptom: `배경 이벤트 후 채팅 입력창 포커스가 유지되지 않음(이전: ${before.tag}.${before.cls}, 이후: ${after.tag}.${after.cls})`,
            file: 'src/views/sessions.ts:renderChatInputArea',
            repro: '세션 상세 → 채팅 입력창 클릭 → 캐럿 중간 이동 → claude-sessions-changed 발화 → activeElement 확인',
          });
        }
        if (after.textareaValue !== before.value) {
          bugs.push({
            severity: 'Critical',
            symptom: `배경 이벤트 후 채팅 입력값이 변경됨(이전: "${before.value}", 이후: "${after.textareaValue}")`,
            file: 'src/views/sessions.ts:renderChatInputArea',
          });
        }
        if (after.start !== 5 || after.end !== 5) {
          bugs.push({
            severity: 'Major',
            symptom: `배경 이벤트 후 캐럿 위치가 복원되지 않음(기대: start=5,end=5, 실제: start=${after.start},end=${after.end})`,
            file: 'src/dom.ts:boundField/attachFocusCaretTracking',
          });
        }
      },
    },

    // ---------------- A3: devtools 1초 타이머가 무관한 화면을 재렌더하지 않음 ----------------
    {
      id: 'a3-devtools-timer-does-not-rerender-unrelated-route',
      startHash: '#/settings/devtools',
      fixtures: {
        ...base,
        check_dev_tools: [{ id: 'node', name: 'Node.js', installed: true, version: '20.0.0', path: '/usr/local/bin/node', installMethod: 'brew', actionKind: 'run', manualHint: null, required: true }],
        preview_dev_tool_update: { id: 'node', planId: 'harness-plan-1', willRun: true, commandDisplay: 'brew upgrade node', affected: ['node'], notes: '', previewReliable: true },
        update_dev_tool: {
          __delay: 3200,
          value: { id: 'node', outcome: 'updated', verified: true, installMethod: 'brew', versionBefore: '20.0.0', versionAfter: '20.1.0', durationMs: 3200, message: 'ok', logTail: '', pathVisible: true },
        },
      },
      async run(page, { shot, bugs }) {
        await page.getByRole('button', { name: '업데이트 확인' }).click();
        await page.waitForSelector('.devtool-panel-actions');
        await page.getByRole('button', { name: '실행' }).click();
        await page.waitForSelector('.devtool-panel-running');
        await shot('01-devtools-running');

        // 실행 중(경과 타이머 도는 상태)에 완전히 무관한 화면(OTel 설정)으로
        // 이동한다 — 리뷰 시나리오("설치를 걸어두고 그 사이 다른 설정을
        // 입력한다")의 재현.
        await page.evaluate(() => {
          window.location.hash = '#/settings/otel';
        });
        await page.waitForTimeout(150);
        const endpointInput = page.locator('#otel-CLAUDE_CODE_ENABLE_TELEMETRY');
        await endpointInput.waitFor({ state: 'visible' });
        // 이 특정 DOM 노드 참조를 들고 있다가, 시간이 지난 뒤에도 "같은 노드"인지
        // 확인한다 — root.replaceChildren()이 한 번이라도 돌면 이 노드는 완전히
        // 새 인스턴스로 교체된다(A2의 포커스 복원과 무관하게, 노드 정체성 자체가
        // 재생성 여부의 증거다).
        const handle = await endpointInput.elementHandle();
        await shot('02-on-otel-screen');

        await page.waitForTimeout(2200); // 이 사이 1초 타이머가 게이트 없으면 2회 이상 notifyChange()를 불렀을 시간
        const stillSameNode = await page.evaluate((el) => document.contains(el) && el === document.querySelector('#otel-CLAUDE_CODE_ENABLE_TELEMETRY'), handle);
        await shot('03-after-2s-on-otel-screen');
        if (!stillSameNode) {
          bugs.push({
            severity: 'Major',
            symptom: '개발 환경 화면의 1초 경과 타이머가 무관한 화면(OTel 설정)에 있는 동안에도 전체 DOM을 재빌드함(라우트 가드 미적용)',
            file: 'src/views/devTools.ts:startElapsedTimer',
            repro: '#/settings/devtools → 업데이트 실행 시작 → #/settings/otel로 이동 → 2초 대기 → 동일 DOM 노드 참조 유지 여부 확인',
          });
        }

        // 게이트가 "화면 자체는 계속 최신"이라는 정직성 요구까지 깨지 않았는지도
        // 확인한다 — devtools 탭으로 돌아오면 경과 초가 계속 흘러 있어야 한다
        // (데이터는 항상 갱신되고, 재렌더만 걸렀어야 한다).
        await page.evaluate(() => {
          window.location.hash = '#/settings/devtools';
        });
        await page.waitForTimeout(200);
        const runningText = await page.locator('.devtool-panel-running').textContent().catch(() => null);
        await shot('04-back-on-devtools-screen');
        if (!runningText || /\(0초 경과\)/.test(runningText)) {
          bugs.push({
            severity: 'Major',
            symptom: `devtools 탭으로 복귀했는데 경과 시간이 흐르지 않음(데이터 자체가 멈춘 것으로 보임, 실제: ${runningText})`,
            file: 'src/views/devTools.ts:startElapsedTimer',
          });
        }
      },
    },

    // ---------------- A3: 자율업무 런타임 push가 무관한 화면을 재렌더하지 않음 ----------------
    {
      id: 'a3-autonomy-runtime-event-does-not-rerender-unrelated-route',
      startHash: '#/settings/otel',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        const task = base.autonomy_list?.[0]?.tasks?.[0];
        const projectPath = base.autonomy_list?.[0]?.projectPath;
        if (!task || !projectPath) {
          bugs.push({ severity: 'Minor', symptom: 'a3-autonomy 시나리오용 자율업무 데이터가 없음(스킵)', file: 'scripts/ui-harness/flows/majorReview20260921.mjs' });
          return;
        }
        // 자율업무 데이터 로딩(ensureRuntimeWatcher 지연 초기화)이 되도록 홈을
        // 먼저 거친다 — main.ts가 로그인 직후 loadAutonomousTasks()를 이미
        // 부르므로 초기 홈 렌더 시점에 구독이 걸린다.
        await page.waitForTimeout(300);
        await page.evaluate(() => {
          window.location.hash = '#/settings/otel';
        });
        await page.waitForTimeout(200);
        const endpointInput = page.locator('#otel-CLAUDE_CODE_ENABLE_TELEMETRY');
        await endpointInput.waitFor({ state: 'visible' });
        const handle = await endpointInput.elementHandle();
        await shot('01-on-otel-screen');

        await page.evaluate(
          ({ name, payload }) => {
            const ids = window.__eventCallbackIdsByName?.[name] ?? [];
            for (const id of ids) {
              const cb = window['_cb' + id];
              if (typeof cb === 'function') cb({ event: name, id: 0, payload });
            }
          },
          {
            name: 'autonomy-task-updated',
            payload: { projectPath, taskId: task.id, running: true, lastStartedAt: new Date().toISOString(), lastFinishedAt: null, nextRunAt: null, status: null, summary: null, durationMs: null, logPath: null },
          }
        );
        await page.waitForTimeout(300);
        const stillSameNode = await page.evaluate((el) => document.contains(el) && el === document.querySelector('#otel-CLAUDE_CODE_ENABLE_TELEMETRY'), handle);
        await shot('02-after-runtime-event');
        if (!stillSameNode) {
          bugs.push({
            severity: 'Major',
            symptom: '무관한 화면(OTel 설정)에 있는 동안 자율업무 런타임 push 이벤트가 전체 DOM을 재빌드함(라우트 가드 미적용)',
            file: 'src/views/autonomousTasks.ts:applyRuntimeUpdate',
            repro: '#/settings/otel → autonomy-task-updated 이벤트 발화 → 동일 DOM 노드 참조 유지 여부 확인',
          });
        }
      },
    },

    // ---------------- B1: 사이드바 "불러오는 중…" vs 빈 상태 ----------------
    {
      id: 'b1-sidebar-empty-state-after-load',
      startHash: '#/',
      fixtures: { ...base, list_workspace_projects: { projects: [], skipped: [] }, list_claude_sessions: [] },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400); // 로드 완료 대기(loaded:true로 전이할 시간)

        const projectsGroup = page.locator('.sidebar-nav-group', { hasText: '프로젝트' }).first();
        await projectsGroup.locator('.sidebar-nav-chevron-btn').click();
        await page.waitForTimeout(100);
        const projectsEmptyText = await page.locator('.sidebar-nav-group', { hasText: '프로젝트' }).first().locator('.sidebar-subnav-empty').textContent().catch(() => null);

        const sessionsGroup = page.locator('.sidebar-nav-group', { hasText: '세션목록' }).first();
        await sessionsGroup.locator('.sidebar-nav-chevron-btn').click();
        await page.waitForTimeout(100);
        const sessionsEmptyText = await page.locator('.sidebar-nav-group', { hasText: '세션목록' }).first().locator('.sidebar-subnav-empty').textContent().catch(() => null);

        await shot('01-sidebar-expanded-empty');

        if (projectsEmptyText && projectsEmptyText.includes('불러오는 중')) {
          bugs.push({
            severity: 'Major',
            symptom: `프로젝트 0건 + 로드 완료 상태인데 사이드바가 "불러오는 중…"을 영구 표시함(실제: ${projectsEmptyText})`,
            file: 'src/sidebar.ts:renderProjectsGroup',
            repro: 'list_workspace_projects=[] → 로드 완료 대기 → 사이드바 "프로젝트" 펼침 → 문구 확인',
          });
        }
        if (sessionsEmptyText && sessionsEmptyText.includes('불러오는 중')) {
          bugs.push({
            severity: 'Major',
            symptom: `세션 0건 + 로드 완료 상태인데 사이드바가 "불러오는 중…"을 영구 표시함(실제: ${sessionsEmptyText})`,
            file: 'src/sidebar.ts:renderSessionsGroup',
            repro: 'list_claude_sessions=[] → 로드 완료 대기 → 사이드바 "세션목록" 펼침 → 문구 확인',
          });
        }
      },
    },

    // ---------------- C3: 모달 포커스 트랩 ----------------
    {
      id: 'c3-mcp-modal-focus-trap',
      startHash: '#/settings/mcp',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        const openBtn = page.getByRole('button', { name: '+ 새 MCP 서버' });
        await openBtn.click();
        await page.waitForSelector('.modal-overlay');
        await page.waitForTimeout(50); // createModalOverlay의 queueMicrotask 포커스 예약 반영 대기
        await shot('01-modal-opened');

        const initialFocusInModal = await page.evaluate(() => {
          const box = document.querySelector('.modal-box');
          return !!box && box.contains(document.activeElement) && document.activeElement !== document.body;
        });
        if (!initialFocusInModal) {
          bugs.push({ severity: 'Major', symptom: '모달을 열어도 포커스가 모달 안으로 이동하지 않음(activeElement가 body에 남음)', file: 'src/dom.ts:createModalOverlay' });
        }

        // Tab을 넉넉히(8회) 눌러도 모달 밖(예: 사이드바 캐럿)으로 빠져나가지
        // 않아야 한다 — 리뷰 실측(probe-p4)은 Tab 5회 후 .sidebar-nav-chevron-btn
        // 으로 탈출했었다.
        for (let i = 0; i < 8; i++) {
          await page.keyboard.press('Tab');
        }
        const stillTrapped = await page.evaluate(() => {
          const box = document.querySelector('.modal-box');
          return !!box && box.contains(document.activeElement);
        });
        await shot('02-after-8-tabs');
        if (!stillTrapped) {
          const escapedTo = await page.evaluate(() => `${document.activeElement?.tagName}.${document.activeElement?.className}`);
          bugs.push({
            severity: 'Major',
            symptom: `Tab 8회 후 포커스가 모달 밖으로 탈출함(실제 위치: ${escapedTo})`,
            file: 'src/dom.ts:createModalOverlay',
            repro: '#/settings/mcp → "+ 새 MCP 서버" → Tab 8회 → activeElement 확인',
          });
        }

        // 닫은 뒤 포커스가 열기 버튼으로 복원되는지.
        await page.keyboard.press('Escape');
        await page.waitForTimeout(50);
        const restoredToOpener = await page.evaluate(() => document.activeElement?.textContent?.includes('새 MCP 서버'));
        await shot('03-after-close-focus-restored');
        if (!restoredToOpener) {
          bugs.push({
            severity: 'Minor',
            symptom: '모달을 닫아도(ESC) 포커스가 열기 버튼으로 복원되지 않음',
            file: 'src/dom.ts:restoreModalFocus / src/views/settings.ts:closeMcpAddModal',
          });
        }
      },
    },

    // ---------------- G1: 로그아웃 시 라우트 이탈 정리 ----------------
    {
      id: 'g1-logout-unlistens-chat-streaming',
      startHash: base.list_claude_sessions[0] ? `#/sessions/${encodeURIComponent(base.list_claude_sessions[0].sessionId)}` : '#/sessions',
      fixtures: {
        ...base,
        read_session_transcript: {
          sessionId: base.list_claude_sessions[0]?.sessionId ?? 'none',
          cwd: base.list_claude_sessions[0]?.cwd ?? '/Users/hopegiver/workspace/malgn-vscode',
          transcriptPath: '/dev/null',
          messages: [{ kind: 'user', text: '로그아웃 회귀 테스트 메시지', toolCount: 0, at: new Date().toISOString() }],
          truncated: false,
          activeTurnId: null,
        },
      },
      async run(page, { shot, bugs, getListenerCount }) {
        if (base.list_claude_sessions.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'g1 시나리오용 세션 데이터가 없음', file: 'scripts/ui-harness/flows/majorReview20260921.mjs' });
          return;
        }
        await page.waitForSelector('.chat-page');
        // 메타데이터 모달도 열어둔 채로 로그아웃한다 — window에는 이미
        // updateApi.ts가 앱 수명 내내(로그인 여부 무관) 유지하는 keydown
        // 리스너 1개가 항상 있으므로, 그 기준선 위에 모달 ESC 리스너를 하나
        // 더 얹어야 "정리됐다"를 정확히 관측할 수 있다(기준선만 보면 로그아웃
        // 전후 값이 우연히 같아 보여 리스너 누수를 놓친다).
        const metaBtn = page.getByRole('button', { name: /메타데이터/ });
        if ((await metaBtn.count()) > 0) {
          await metaBtn.click();
          await page.waitForSelector('.modal-overlay');
        }
        const unlistenBefore = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'plugin:event|unlisten').length);
        const keydownBefore = await getListenerCount('keydown');
        await shot('01-session-detail-before-logout');

        // 모달이 열려 있으면 오버레이가 position:fixed로 뷰포트 전체를 덮어
        // 사이드바를 시각적으로도 가린다(실제 사용자도 이 배치에서는 배경을
        // 먼저 클릭하면 모달이 닫혀버려 "모달을 열어둔 채 로그아웃 버튼을
        // 클릭"할 도리가 없다). Playwright의 마우스 좌표 클릭은 { force: true }
        // 를 줘도 브라우저 히트테스트가 오버레이로 라우팅해 실제로는 모달만
        // 닫힌다 — 그래서 좌표 기반 클릭 대신 DOM 노드에 직접 .click()을
        // 호출해 addEventListener('click', ...) 핸들러를 호출한다. 이 시나리오가
        // 검증하려는 것은 "떠날 때 정리가 항상 도는가"라는 핸들러 계약 자체이지
        // 오버레이의 시각적 클릭 차단(별개 UX 사안)이 아니다.
        await page.evaluate(() => {
          const btn = document.querySelector('.sidebar-logout');
          if (btn instanceof HTMLElement) btn.click();
        });
        await page.waitForTimeout(400); // resetStateForLogout() → hashchange → handleNavigation()의 leaveSessionChatView() 반영 대기
        await shot('02-after-logout');

        const onLoginScreen = (await page.locator('.login-screen').count()) > 0;
        if (!onLoginScreen) {
          bugs.push({ severity: 'Critical', symptom: '로그아웃 후 로그인 화면으로 전환되지 않음', file: 'src/sidebar.ts / src/main.ts' });
        }

        const unlistenAfter = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'plugin:event|unlisten').length);
        if (unlistenAfter <= unlistenBefore) {
          bugs.push({
            severity: 'Major',
            symptom: `세션 상세(채팅)에서 로그아웃해도 plugin:event|unlisten이 호출되지 않음(이전: ${unlistenBefore}, 이후: ${unlistenAfter}) — 스트리밍 리스너 누수`,
            file: 'src/main.ts:handleNavigation / src/views/sessions.ts:leaveSessionChatView',
            repro: '세션 상세 진입 → 로그아웃 → plugin:event|unlisten invoke 횟수 확인',
          });
        }

        const keydownAfter = await getListenerCount('keydown');
        if (keydownAfter >= keydownBefore && keydownBefore > 0) {
          bugs.push({
            severity: 'Minor',
            symptom: `로그아웃 후에도 window keydown 리스너가 줄지 않음(이전: ${keydownBefore}, 이후: ${keydownAfter})`,
            file: 'src/main.ts:handleNavigation',
          });
        }
      },
    },
  ];
}
