// 흐름 (2) 세션목록 "새세션" → 프로젝트 선택 모달 → 새세션 — src/views/sessions.ts
import { manySessions } from '../lib/fixtures.mjs';

export const flowId = 'sessions';
export const startHash = '#/sessions';

export function scenarios(base) {
  return [
    {
      id: 'golden',
      fixtures: {
        ...base,
        start_new_session_message: { sessionId: 'harness-new-session-1', turnId: 'harness-turn-1' },
        read_session_transcript: { sessionId: base.list_claude_sessions[0]?.sessionId ?? 'none', cwd: '/Users/hopegiver/workspace/malgn-vscode', transcriptPath: '/dev/null', messages: [{ kind: 'user', text: '테스트 메시지입니다', toolCount: 0, at: new Date().toISOString() }], truncated: false, activeTurnId: null },
      },
      async run(page, { shot, bugs, getListenerCount }) {
        await shot('01-list');
        if (base.list_claude_sessions.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'golden 시나리오 데이터가 비어 있음(실제 jsonl에서 세션을 못 찾음 — 데이터 이슈)', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }

        // ---- 새 세션 모달: 열기 → ESC ----
        const before = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-new-session-modal');
        await page.keyboard.press('Escape');
        const closed = await page.locator('.modal-overlay').count();
        if (closed > 0) {
          bugs.push({ severity: 'Critical', symptom: 'ESC를 눌러도 "새 세션" 프로젝트 선택 모달이 닫히지 않음', file: 'src/views/sessions.ts', repro: '#/sessions → "+ 새 세션" → ESC' });
        }
        const afterEsc = await getListenerCount('keydown');
        if (afterEsc > before) {
          bugs.push({ severity: 'Minor', symptom: `새 세션 모달을 ESC로 닫은 뒤 keydown 리스너 잔존(before=${before}, after=${afterEsc})`, file: 'src/views/sessions.ts:detachNewSessionModalEscHandler' });
        }

        // ---- 모달 연 채로 라우트 이탈 → leaveSessionsListView 리스너 정리 확인 ----
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        const beforeLeave = await getListenerCount('keydown');
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForTimeout(150);
        const afterLeave = await getListenerCount('keydown');
        if (afterLeave >= beforeLeave && beforeLeave > 0) {
          bugs.push({ severity: 'Major', symptom: `새 세션 모달을 연 채로 라우트를 이탈해도 keydown 리스너가 정리되지 않음(누수) — before=${beforeLeave}, after=${afterLeave}`, file: 'src/main.ts / src/views/sessions.ts:leaveSessionsListView', repro: '#/sessions → "+ 새 세션" 열기(닫지 않음) → hash를 "#/"로 변경' });
        }
        await page.evaluate(() => { window.location.hash = '#/sessions'; });
        await page.waitForTimeout(150);

        // ---- 새 세션 모달: 프로젝트 선택 → draft 화면 → 메시지 전송 ----
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        const projectRow = page.locator('.modal-body .session-row').first();
        if ((await projectRow.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '새 세션 모달에 선택 가능한 프로젝트가 없음(list_workspace_projects 데이터 확인 필요)', file: 'src/views/sessions.ts:renderNewSessionModal' });
          await page.keyboard.press('Escape');
        } else {
          await projectRow.click();
          await page.waitForSelector('.chat-page');
          await shot('03-draft-empty');
          const textarea = page.locator('.chat-input-textarea');
          await textarea.fill('안녕하세요, 테스트 메시지입니다 <script>alert(1)</script>');
          await textarea.press('Enter');
          await page.waitForTimeout(300);
          await shot('04-draft-sent-optimistic-echo');
          const echoed = await page.locator('.chat-message.user').count();
          if (echoed === 0) {
            bugs.push({ severity: 'Major', symptom: '메시지 전송 직후 낙관적 echo(사용자 말풍선)가 보이지 않음', file: 'src/views/sessions.ts:sendDraftMessage / renderSessionDraftView' });
          }
          // XSS 방어 확인: <script> 문자열이 textContent로만 들어가 실제 스크립트로
          // 실행되지 않았는지(el()이 innerHTML을 쓰지 않는다는 주석의 실증).
          const bubbleText = await page.locator('.chat-message.user .chat-message-text').first().textContent().catch(() => null);
          if (bubbleText && !bubbleText.includes('<script>')) {
            bugs.push({ severity: 'Critical', symptom: '사용자 입력의 <script> 태그가 텍스트로 남지 않음 — innerHTML 경로로 샜을 가능성(XSS)', file: 'src/views/sessions.ts:chatMessage' });
          }
        }

        // ---- 세션 상세: 메타데이터 모달 ----
        await page.evaluate(() => { window.location.hash = '#/sessions'; });
        await page.waitForTimeout(150);
        await page.locator('.session-row').first().click();
        await page.waitForSelector('.chat-page');
        await shot('05-session-detail');
        const metaBtn = page.getByRole('button', { name: /메타데이터/ });
        if ((await metaBtn.count()) > 0) {
          await metaBtn.click();
          await page.waitForSelector('.modal-overlay');
          await shot('06-meta-modal');
          await page.locator('.modal-overlay').click({ position: { x: 5, y: 5 } });
          const stillOpen = await page.locator('.modal-overlay').count();
          if (stillOpen > 0) {
            bugs.push({ severity: 'Major', symptom: '배경 클릭으로 세션 메타데이터 모달이 닫히지 않음', file: 'src/views/sessions.ts:renderMetaModal' });
          }
        }
      },
    },
    {
      id: 'empty',
      fixtures: { ...base, list_claude_sessions: [], list_workspace_projects: { projects: [], skipped: [] } },
      async run(page, { shot, bugs }) {
        await shot('01-empty');
        const emptyTitle = await page.locator('.state-block-title').first().textContent().catch(() => null);
        if (!emptyTitle || !emptyTitle.includes('없습니다')) {
          bugs.push({ severity: 'Major', symptom: `세션 0건인데 "세션이 없습니다" 안내가 없음(실제: ${emptyTitle})`, file: 'src/views/sessions.ts:renderSessionsListView' });
        }
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-empty-new-session-modal-no-projects');
        const noProjectHint = await page.locator('.modal-body .state-block-title').first().textContent().catch(() => null);
        if (!noProjectHint || !noProjectHint.includes('프로젝트')) {
          bugs.push({ severity: 'Minor', symptom: `프로젝트가 0개인데 "먼저 프로젝트를 추가하세요" 안내가 없음(실제: ${noProjectHint})`, file: 'src/views/sessions.ts:renderNewSessionModal' });
        }
      },
    },
    {
      id: 'single',
      fixtures: { ...base, list_claude_sessions: [manySessions(1)[0]] },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.session-row').count();
        if (count !== 1) bugs.push({ severity: 'Major', symptom: `1건 픽스처인데 행이 ${count}개 렌더됨`, file: 'src/views/sessions.ts' });
        await shot('01-single');
      },
    },
    {
      id: 'large',
      fixtures: { ...base, list_claude_sessions: manySessions(150) },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.session-row').count();
        if (count !== 150) bugs.push({ severity: 'Minor', symptom: `150건 픽스처인데 ${count}건만 렌더됨`, file: 'src/views/sessions.ts:renderSessionsListView' });
        await shot('01-large');
        const box = await page.locator('.session-row').first().boundingBox();
        const vw = page.viewportSize()?.width ?? 1180;
        if (box && box.width > vw + 5) {
          bugs.push({ severity: 'Minor', symptom: `긴 제목(한글 반복) 세션 행 폭(${Math.round(box.width)}px)이 뷰포트(${vw}px) 초과`, file: 'src/views/sessions.ts:renderSessionRow' });
        }
      },
    },
    {
      id: 'error',
      fixtures: { ...base, list_claude_sessions: { __throw: '테스트 강제 에러: 세션 파일을 읽을 수 없습니다' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const alertVisible = await page.locator('.alert').count();
        if (alertVisible === 0) {
          bugs.push({ severity: 'Critical', symptom: 'list_claude_sessions invoke가 throw해도 에러 배너가 뜨지 않음', file: 'src/views/sessions.ts:loadSessions' });
        }
        await shot('01-error');
      },
    },
    {
      // hub 이슈 01m2wm4e9k822fk73yahrrnnce 재발 방지 — claude CLI 미인증
      // 상태에서 턴이 끝났을 때(session_chat/turn.rs `parse_result_event`가
      // authError:true로 분류) 사용자가 "무엇을 해야 하는지" 알 수 있는 UI가
      // 실제로 뜨는지 확인한다. 백엔드는 이 이벤트 모양 그대로 emit한다(PM이
      // 격리 환경에서 실측한 원문 JSON — turn.rs 단위 테스트의 픽스처와 동일).
      id: 'auth-error',
      fixtures: {
        ...base,
        start_new_session_message: { sessionId: 'harness-auth-session-1', turnId: 'harness-auth-turn-1' },
        send_session_message: { turnId: 'harness-auth-turn-2' },
        read_session_transcript: {
          sessionId: 'harness-auth-session-1',
          cwd: '/Users/hopegiver/workspace/malgn-vscode',
          transcriptPath: '/dev/null',
          messages: [],
          truncated: false,
          activeTurnId: null,
        },
        open_claude_login_terminal: { opened: true, message: '터미널 창에서 claude login 절차를 진행한 뒤 다시 시도해주세요.' },
      },
      async run(page, { shot, bugs, emitEvent }) {
        if (base.list_workspace_projects.projects.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'auth-error 시나리오에 쓸 프로젝트가 없음(list_workspace_projects 비어있음)', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        const projectRow = page.locator('.modal-body .session-row').first();
        if ((await projectRow.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'auth-error 시나리오: 새 세션 모달에 선택 가능한 프로젝트가 없음', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await projectRow.click();
        await page.waitForSelector('.chat-page');
        await page.locator('.chat-input-textarea').fill('인증 실패 재현용 메시지');
        await page.locator('.chat-input-textarea').press('Enter');
        await page.waitForTimeout(200);

        // 미인증 상태의 실제 claude -p 출력 그대로(PM 실측, stderr는 비고
        // stdout stream-json 마지막 줄만 이렇다) — turn.rs가 이 JSON을 파싱해
        // authError:true로 emit한 것을 흉내낸다.
        await emitEvent('session-chat-done', {
          sessionId: 'harness-auth-session-1',
          turnId: 'harness-auth-turn-1',
          ok: false,
          canceled: false,
          error: 'success: Not logged in · Please run /login',
          authError: true,
        });
        await page.waitForTimeout(200);
        await shot('01-auth-error-banner');

        const alertText = await page.locator('.alert').first().textContent().catch(() => null);
        if (!alertText || !alertText.includes('로그인')) {
          bugs.push({ severity: 'Critical', symptom: `인증 실패(authError) 턴 종료 후 "로그인이 필요합니다" 안내가 뜨지 않음(실제: ${alertText})`, file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }
        if (!alertText || !alertText.includes('Not logged in')) {
          bugs.push({ severity: 'Major', symptom: '인증 실패 안내에 원문 에러(Not logged in)가 보존되지 않음', file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }

        const loginBtn = page.getByRole('button', { name: /claude login/ });
        if ((await loginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '인증 실패 안내에 "터미널 열기(claude login)" 버튼이 없음', file: 'src/views/sessions.ts:renderChatErrorBlock' });
        } else {
          await loginBtn.click();
          await page.waitForTimeout(150);
          await shot('02-after-login-button-click');
          const toastText = await page.locator('.toast').first().textContent().catch(() => null);
          if (!toastText) {
            bugs.push({ severity: 'Major', symptom: '"터미널 열기" 버튼을 눌러도 결과 토스트가 뜨지 않음', file: 'src/views/sessions.ts:handleClaudeLogin' });
          }
          const loginInvoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'open_claude_login_terminal'));
          if (!loginInvoked) {
            bugs.push({ severity: 'Critical', symptom: '"터미널 열기" 버튼을 눌러도 open_claude_login_terminal 커맨드가 호출되지 않음', file: 'src/views/sessions.ts:handleClaudeLogin' });
          }
        }

        // 회귀 방지: authError:false인 일반 에러는 기존처럼 원문 한 줄만 뜨고
        // 로그인 버튼은 보이지 않아야 한다(모든 에러를 인증 UI로 오탐하면 안 됨).
        const textarea = page.locator('.chat-input-textarea');
        await textarea.fill('일반 에러 재현용 메시지');
        await textarea.press('Enter');
        await page.waitForTimeout(200);
        await emitEvent('session-chat-done', {
          sessionId: 'harness-auth-session-1',
          turnId: 'harness-auth-turn-2',
          ok: false,
          canceled: false,
          error: 'error_max_turns: 도구 실행 한도를 초과했습니다',
          authError: false,
        });
        await page.waitForTimeout(200);
        await shot('03-non-auth-error-banner');
        const secondAlertText = await page.locator('.alert').first().textContent().catch(() => null);
        if (secondAlertText && secondAlertText.includes('로그인')) {
          bugs.push({ severity: 'Critical', symptom: `authError:false인 일반 에러인데도 로그인 안내가 표시됨(오탐, 실제: ${secondAlertText})`, file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }
        if ((await page.getByRole('button', { name: /claude login/ }).count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '일반 에러(authError:false)에도 "claude login" 버튼이 남아있음', file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }
      },
    },
    {
      // 앱 안 claude CLI 로그인(claude_auth.rs, v0.2.11 사고의 근본 수정판) —
      // 인증 실패 배너의 "앱에서 로그인" 버튼. 터미널을 여는 기존 경로(위
      // auth-error 시나리오)와 나란히 검증한다.
      //
      // 핵심 회귀 방지 포인트: 코드 입력창은 "특정 문구를 감지해야" 뜨는 게
      // 아니라 로그인이 진행 중인 동안(login.active) 항상 있어야 한다 —
      // 그래서 이 시나리오는 URL 이벤트조차 오기 전(start 직후)에 입력창부터
      // 확인한다. v0.2.11은 감지 분기가 틀려 입력 경로 자체가 없었다 — 이
      // 시나리오가 그 감지 분기가 되살아나지 않았음을 고정한다.
      id: 'app-login',
      fixtures: {
        ...base,
        start_new_session_message: { sessionId: 'harness-app-login-session-1', turnId: 'harness-app-login-turn-1' },
        read_session_transcript: {
          sessionId: 'harness-app-login-session-1',
          cwd: '/Users/hopegiver/workspace/malgn-vscode',
          transcriptPath: '/dev/null',
          messages: [],
          truncated: false,
          activeTurnId: null,
        },
      },
      async run(page, { shot, bugs, emitEvent }) {
        if (base.list_workspace_projects.projects.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'app-login 시나리오에 쓸 프로젝트가 없음(list_workspace_projects 비어있음)', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        const projectRow = page.locator('.modal-body .session-row').first();
        if ((await projectRow.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'app-login 시나리오: 새 세션 모달에 선택 가능한 프로젝트가 없음', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await projectRow.click();
        await page.waitForSelector('.chat-page');
        await page.locator('.chat-input-textarea').fill('인증 실패 재현용 메시지');
        await page.locator('.chat-input-textarea').press('Enter');
        await page.waitForTimeout(200);
        await emitEvent('session-chat-done', {
          sessionId: 'harness-app-login-session-1',
          turnId: 'harness-app-login-turn-1',
          ok: false,
          canceled: false,
          error: 'success: Not logged in · Please run /login',
          authError: true,
        });
        await page.waitForTimeout(200);

        // ---- 1) "앱에서 로그인" 버튼이 "터미널 열기"와 나란히 뜬다 ----
        const appLoginBtn = page.getByRole('button', { name: '앱에서 로그인' });
        const terminalBtn = page.getByRole('button', { name: /claude login/ });
        if ((await appLoginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '인증 실패 안내에 "앱에서 로그인" 버튼이 없음', file: 'src/views/sessions.ts:renderChatErrorBlock' });
          return;
        }
        if ((await terminalBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '"앱에서 로그인" 버튼이 추가되며 기존 "터미널 열기" 폴백이 사라짐(막다른 길 위험)', file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }
        await shot('01-both-login-buttons');

        // ---- 2) 시작 → start_claude_auth_login 호출 확인, URL 이벤트조차
        //         오기 전에 코드 입력창이 이미 있어야 한다(분기 없음 설계) ----
        await appLoginBtn.click();
        await page.waitForTimeout(150);
        const startInvoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'start_claude_auth_login'));
        if (!startInvoked) {
          bugs.push({ severity: 'Critical', symptom: '"앱에서 로그인" 버튼을 눌러도 start_claude_auth_login 커맨드가 호출되지 않음', file: 'src/views/sessions.ts:handleClaudeAuthLoginStart' });
        }
        // 로그인 패널은 이제 인증 실패 안내와 별도의 .alert(.chat-auth-login-panel)로
        // 그려진다(renderClaudeAuthLoginBlock — chat.error/authError와 무관하게
        // login.active만 본다) — 그래서 전체 .alert 중 첫 번째가 아니라 이 클래스로
        // 콕 집어 확인한다.
        const progressText = await page.locator('.chat-auth-login-panel').first().textContent().catch(() => null);
        if (!progressText || !progressText.includes('시작하는 중')) {
          bugs.push({ severity: 'Major', symptom: `로그인 시작 직후 진행 중 표시가 뜨지 않음(실제: ${progressText})`, file: 'src/views/sessions.ts:renderClaudeAuthLoginBlock' });
        }
        const codeInputEarly = page.getByPlaceholder('claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)');
        if ((await codeInputEarly.count()) === 0) {
          bugs.push({
            severity: 'Critical',
            symptom: 'URL 이벤트가 오기 전인데도 코드 입력창이 없음 — 입력창이 특정 문구 감지에 의존하는 분기로 되돌아간 회귀(v0.2.11과 동일한 함정)',
            file: 'src/views/sessions.ts:renderClaudeAuthLoginActivePanel',
          });
        }
        const cancelBtnEarly = page.getByRole('button', { name: '로그인 취소' });
        if ((await cancelBtnEarly.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'URL 이벤트 전에 "로그인 취소" 버튼이 없음(영구 대기처럼 보일 위험)', file: 'src/views/sessions.ts:renderClaudeAuthLoginActivePanel' });
        }
        await shot('02-login-starting-code-input-already-present');

        // ---- 3) URL 이벤트 → 링크 열기 버튼 노출(자동 오픈 실패 폴백) ----
        await emitEvent('claude-auth-login-url', { url: 'https://claude.com/cai/oauth/authorize?state=harness-test' });
        await page.waitForTimeout(150);
        const openLinkBtn = page.getByRole('button', { name: '로그인 링크 열기' });
        if ((await openLinkBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'URL 이벤트 수신 후 "로그인 링크 열기" 버튼이 뜨지 않음', file: 'src/views/sessions.ts:renderClaudeAuthLoginActivePanel' });
        } else {
          await openLinkBtn.click();
          await page.waitForTimeout(100);
          const openUrlInvoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'plugin:opener|open_url'));
          if (!openUrlInvoked) {
            bugs.push({ severity: 'Major', symptom: '"로그인 링크 열기" 버튼을 눌러도 opener 플러그인이 호출되지 않음', file: 'src/views/sessions.ts:handleOpenClaudeAuthLoginUrl' });
          }
        }
        await shot('03-login-url-ready');

        // ---- 4) 개행 없는 프롬프트 원문이 stdout 이벤트로 오면 출력 로그에
        //         그대로 나타난다(claude_auth.rs가 바이트 단위로 읽어 보내는
        //         텍스트 — 이 하네스는 TS/DOM 층만 검증하므로 실제 자식
        //         프로세스는 없다. 이벤트 자체는 emitEvent로 흉내낸다) ----
        await emitEvent('claude-auth-login-output', { text: 'Paste code here if prompted > ' });
        await page.waitForTimeout(100);
        const outputLog = await page.locator('.chat-auth-login-output').first().textContent().catch(() => null);
        if (!outputLog || !outputLog.includes('Paste code here if prompted')) {
          bugs.push({ severity: 'Major', symptom: `claude-auth-login-output 이벤트 원문이 로그 영역에 반영되지 않음(실제: ${outputLog})`, file: 'src/main.ts:initClaudeAuthLoginWatcher' });
        }
        await shot('04-raw-output-shown');

        // ---- 5) 코드 입력 → 제출 → submit_claude_auth_login_code 호출,
        //         입력값이 그대로 전달됐는지 확인 ----
        const codeInput = page.getByPlaceholder('claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)');
        await codeInput.fill('HARNESS-CODE-999');
        await page.getByRole('button', { name: '코드 제출' }).click();
        await page.waitForTimeout(150);
        const submitCall = await page.evaluate(() => (window.__invokeLog ?? []).find((e) => e.cmd === 'submit_claude_auth_login_code'));
        if (!submitCall) {
          bugs.push({ severity: 'Critical', symptom: '"코드 제출" 버튼을 눌러도 submit_claude_auth_login_code 커맨드가 호출되지 않음', file: 'src/views/sessions.ts:handleClaudeAuthLoginSubmitCode' });
        } else if (submitCall.args?.code !== 'HARNESS-CODE-999') {
          bugs.push({ severity: 'Critical', symptom: `제출된 코드 값이 입력창 내용과 다름(실제: ${JSON.stringify(submitCall.args)})`, file: 'src/views/sessions.ts:handleClaudeAuthLoginSubmitCode' });
        }
        const codeInputAfterSubmit = await codeInput.inputValue().catch(() => null);
        if (codeInputAfterSubmit !== '') {
          bugs.push({ severity: 'Minor', symptom: `코드 제출 후 입력창이 비워지지 않음(실제: "${codeInputAfterSubmit}")`, file: 'src/views/sessions.ts:handleClaudeAuthLoginSubmitCode' });
        }
        await shot('05-code-submitted');

        // ---- 6) 완료(ok) → 배너 자체가 닫히고 같은 화면에서 이어갈 수 있다 ----
        await emitEvent('claude-auth-login-finished', {
          ok: true,
          canceled: false,
          error: null,
          status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        });
        await page.waitForTimeout(150);
        const alertGone = await page.locator('.alert').count();
        if (alertGone > 0) {
          bugs.push({ severity: 'Critical', symptom: '로그인 완료 후에도 인증 실패 배너가 그대로 남아 있음(또 실패로 오인함)', file: 'src/views/sessions.ts / src/main.ts:initClaudeAuthLoginWatcher' });
        }
        // 토스트는 스택으로 쌓이고(dom.ts showToast) 2.2초 뒤에야 사라진다 —
        // 바로 앞 단계(5)의 "코드를 전달했습니다." 토스트가 아직 남아있는
        // 동안 이 이벤트가 오므로 `.first()`가 아니라 "완료" 문구를 포함하는
        // 토스트가 하나라도 있는지로 확인한다(스택 순서에 의존하지 않는다).
        const doneToastCount = await page.locator('.toast', { hasText: '완료' }).count();
        if (doneToastCount === 0) {
          bugs.push({ severity: 'Minor', symptom: '로그인 완료 토스트("...완료되었습니다")가 뜨지 않음', file: 'src/main.ts:initClaudeAuthLoginWatcher' });
        }
        await shot('06-login-finished-banner-cleared');

        // ---- 7) 재현 후 취소 → 진행 컨트롤이 idle로 돌아온다 ----
        await page.locator('.chat-input-textarea').fill('두 번째 인증 실패 재현용 메시지');
        await page.locator('.chat-input-textarea').press('Enter');
        await page.waitForTimeout(200);
        await emitEvent('session-chat-done', {
          sessionId: 'harness-app-login-session-1',
          turnId: 'harness-app-login-turn-1',
          ok: false,
          canceled: false,
          error: 'success: Not logged in · Please run /login',
          authError: true,
        });
        await page.waitForTimeout(200);
        await page.getByRole('button', { name: '앱에서 로그인' }).click();
        await page.waitForTimeout(150);
        const cancelBtn = page.getByRole('button', { name: '로그인 취소' });
        if ((await cancelBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '로그인 진행 중에 "로그인 취소" 버튼이 뜨지 않음(영구 대기처럼 보일 위험)', file: 'src/views/sessions.ts:renderClaudeAuthLoginActivePanel' });
        } else {
          await cancelBtn.click();
          await page.waitForTimeout(100);
          const cancelInvoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'cancel_claude_auth_login'));
          if (!cancelInvoked) {
            bugs.push({ severity: 'Critical', symptom: '"로그인 취소" 버튼을 눌러도 cancel_claude_auth_login 커맨드가 호출되지 않음', file: 'src/views/sessions.ts:handleClaudeAuthLoginCancel' });
          }
          await emitEvent('claude-auth-login-finished', { ok: false, canceled: true, error: null, status: null });
          await page.waitForTimeout(150);
          const idleAppLoginBtn = await page.getByRole('button', { name: '앱에서 로그인' }).count();
          if (idleAppLoginBtn === 0) {
            bugs.push({ severity: 'Major', symptom: '로그인 취소 후 "앱에서 로그인" 버튼이 돌아오지 않음(재시도 불가)', file: 'src/views/sessions.ts:renderChatErrorBlock' });
          }
        }
        await shot('07-login-canceled-idle-again');
      },
    },
    {
      // 이번 수정(hub 이슈 01m33qe0zhn55mczhcdgec2b61 후속) 핵심 가드 — 로그인
      // 진행 중 다른 화면에 갔다 돌아와도 코드 입력창이 남아있는지 확인한다.
      // leaveSessionChatView()는 state.sessionChat(error/authError 포함)을
      // 화면을 뜰 때마다 통째로 비우지만, 그 사이에도 claude_auth.rs 자식
      // 프로세스는 계속 코드를 기다린다(state.claudeAuthLogin.active는 그대로
      // true). 코드 입력창/취소 버튼이 chat.authError에 얹혀 있으면 화면
      // 이동만으로 사라진다 — v0.2.11과 같은 종류의 결함이라 별도 시나리오로
      // 고정한다.
      id: 'app-login-nav-away',
      fixtures: {
        ...base,
        start_new_session_message: { sessionId: 'harness-nav-login-session-1', turnId: 'harness-nav-login-turn-1' },
        read_session_transcript: {
          sessionId: 'harness-nav-login-session-1',
          cwd: '/Users/hopegiver/workspace/malgn-vscode',
          transcriptPath: '/dev/null',
          messages: [],
          truncated: false,
          activeTurnId: null,
        },
      },
      async run(page, { shot, bugs, emitEvent }) {
        if (base.list_workspace_projects.projects.length === 0) {
          bugs.push({ severity: 'Major', symptom: 'app-login-nav-away 시나리오에 쓸 프로젝트가 없음(list_workspace_projects 비어있음)', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await page.getByRole('button', { name: '+ 새 세션' }).click();
        await page.waitForSelector('.modal-overlay');
        const projectRow = page.locator('.modal-body .session-row').first();
        if ((await projectRow.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'app-login-nav-away 시나리오: 새 세션 모달에 선택 가능한 프로젝트가 없음', file: 'scripts/ui-harness/flows/sessions.mjs' });
          return;
        }
        await projectRow.click();
        await page.waitForSelector('.chat-page');
        await page.locator('.chat-input-textarea').fill('로그인 후 화면 이동 재현용 메시지');
        await page.locator('.chat-input-textarea').press('Enter');
        await page.waitForTimeout(200);
        await emitEvent('session-chat-done', {
          sessionId: 'harness-nav-login-session-1',
          turnId: 'harness-nav-login-turn-1',
          ok: false,
          canceled: false,
          error: 'success: Not logged in · Please run /login',
          authError: true,
        });
        await page.waitForTimeout(200);

        // ---- 1) 로그인 시작 → URL 이벤트조차 오기 전에 코드 입력창부터 뜬다 ----
        const appLoginBtn = page.getByRole('button', { name: '앱에서 로그인' });
        if ((await appLoginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'app-login-nav-away 시나리오: 인증 실패 안내에 "앱에서 로그인" 버튼이 없음', file: 'src/views/sessions.ts:renderChatErrorBlock' });
          return;
        }
        await appLoginBtn.click();
        await page.waitForTimeout(150);
        const codeInputBefore = page.getByPlaceholder('claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)');
        if ((await codeInputBefore.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '로그인 시작 직후 코드 입력창이 뜨지 않음', file: 'src/views/sessions.ts:renderClaudeAuthLoginBlock' });
        }
        await shot('01-login-active-before-nav');

        // ---- 2) 핵심 회귀 확인: 세션목록으로 이동(leaveSessionChatView 발동) ----
        await page.evaluate(() => { window.location.hash = '#/sessions'; });
        await page.waitForTimeout(200);
        await shot('02-navigated-away-to-list');
        const codeInputOnListScreen = page.getByPlaceholder('claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)');
        if ((await codeInputOnListScreen.count()) > 0) {
          bugs.push({ severity: 'Minor', symptom: '세션목록 화면에도 로그인 코드 입력창이 새어나옴(범위는 세션 상세/draft 화면으로 한정돼야 함)', file: 'src/views/sessions.ts:renderClaudeAuthLoginBlock' });
        }

        // ---- 3) 같은 세션 화면으로 복귀 → 코드 입력창/취소 버튼이 다시 있어야 한다 ----
        await page.evaluate(() => { window.location.hash = '#/sessions/harness-nav-login-session-1'; });
        await page.waitForSelector('.chat-page');
        await page.waitForTimeout(250);
        await shot('03-returned-to-chat-code-input-present');

        const codeInputAfterReturn = page.getByPlaceholder('claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)');
        if ((await codeInputAfterReturn.count()) === 0) {
          bugs.push({
            severity: 'Critical',
            symptom: '로그인 진행 중 다른 화면에 갔다 돌아오면 코드 입력창이 사라짐(leaveSessionChatView가 chat.error/authError를 비우면서 로그인 패널까지 함께 지워지는 v0.2.11류 회귀)',
            file: 'src/views/sessions.ts:leaveSessionChatView / renderChatErrorBlock',
          });
        }
        const cancelBtnAfterReturn = page.getByRole('button', { name: '로그인 취소' });
        if ((await cancelBtnAfterReturn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '화면 이동 후 복귀 시 "로그인 취소" 버튼이 사라짐', file: 'src/views/sessions.ts:renderClaudeAuthLoginBlock' });
        }

        // ---- 4) 중복 렌더/빈 껍데기 방지: 이미 리셋된 인증 실패 안내 문구가
        //         되살아나지 않아야 하고, 내용 없는 alert가 남지 않아야 한다 ----
        const alertTexts = await page.locator('.alert').allTextContents();
        if (alertTexts.some((t) => t.includes('claude CLI에 로그인이 필요합니다'))) {
          bugs.push({ severity: 'Minor', symptom: '화면 복귀 후 이미 리셋된 인증 실패 안내 문구가 다시 나타남(중복 렌더 의심)', file: 'src/views/sessions.ts:renderChatErrorBlock' });
        }
        if (alertTexts.some((t) => t.trim() === '')) {
          bugs.push({ severity: 'Major', symptom: '빈 alert 껍데기가 렌더됨', file: 'src/views/sessions.ts:renderClaudeAuthLoginBlock' });
        }

        // ---- 5) 마무리: 취소로 idle 복귀(다른 시나리오 오염 방지) ----
        if ((await cancelBtnAfterReturn.count()) > 0) {
          await cancelBtnAfterReturn.click();
          await page.waitForTimeout(100);
          await emitEvent('claude-auth-login-finished', { ok: false, canceled: true, error: null, status: null });
          await page.waitForTimeout(150);
        }
      },
    },
    {
      id: 'slow',
      fixtures: { ...base, list_claude_sessions: { __delay: 2500, value: base.list_claude_sessions } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const loadingVisible = await page.locator('.state-block-title:has-text("불러오는 중")').count();
        await shot('01-slow-loading');
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 동안 "불러오는 중…" 표시가 뜨지 않음', file: 'src/views/sessions.ts:renderSessionsListView' });
        }
        await page.waitForTimeout(2500);
        await shot('02-slow-loaded');
      },
    },
  ];
}
