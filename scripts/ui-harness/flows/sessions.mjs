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
      fixtures: { ...base, list_claude_sessions: [], list_workspace_projects: [] },
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
        if (base.list_workspace_projects.length === 0) {
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
