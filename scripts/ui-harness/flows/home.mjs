// 흐름 (8) 대시보드 claude CLI 로그인 상태 위젯 — src/views/home.ts claudeAuthWidget
//
// 검증 목표: "확인 중 / 로그인됨 / 로그인 안 됨·확인 실패" 세 상태가 서로 다른
// 신호(loading/loggedIn===true/그 외)로만 구분되고, "없음"(에러가 없다 등)이
// 아니라 "있음"(그 상태를 나타내는 요소가 실제로 화면에 보인다)으로 판정한다
// (직전 v0.2.11 회귀 — "대기 중"과 "정상"이 같은 신호를 냈던 사고의 재발 방지).
export const flowId = 'home';
export const startHash = '#/';

export function scenarios(base) {
  return [
    // ① 확인 중 — check_claude_auth_status가 아직 응답하기 전.
    {
      id: 'checking',
      fixtures: { ...base, check_claude_auth_status: { __delay: 3000, value: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' } } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-checking');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        if ((await widget.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '대시보드에 "claude CLI 로그인" 위젯 자체가 렌더되지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
          return;
        }
        const text = await widget.textContent();
        if (!text || !text.includes('확인 중')) {
          bugs.push({ severity: 'Major', symptom: `응답 지연 중인데 "확인 중…" 문구가 보이지 않음(실제: ${text})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if (text && text.includes('로그인됨')) {
          bugs.push({ severity: 'Critical', symptom: '아직 응답이 오지 않았는데(확인 중) "로그인됨"이 표시됨 — 근거 없이 정상으로 보이는 표시(v0.2.11 회귀 재발)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const loginBtnDuringCheck = await widget.getByRole('button', { name: /claude login/ }).count();
        if (loginBtnDuringCheck > 0) {
          bugs.push({ severity: 'Major', symptom: '확인 중 상태인데 이미 "터미널 열기" 로그인 버튼이 보임(아직 미확인 상태를 로그인 필요로 단정)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        await page.waitForTimeout(3200);
        await shot('02-after-resolve-logged-in');
      },
    },
    // ② 로그인됨 — loggedIn: true라는 명시적 긍정 신호가 있을 때만.
    {
      id: 'logged-in',
      fixtures: { ...base, check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-logged-in');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const text = await widget.textContent().catch(() => null);
        if (!text || !text.includes('로그인됨')) {
          bugs.push({ severity: 'Critical', symptom: `loggedIn:true 응답인데 "로그인됨" 표시가 없음(실제: ${text})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if (!text || !text.includes('dev@malgnsoft.com')) {
          bugs.push({ severity: 'Minor', symptom: '로그인 상태에 email이 함께 표시되지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const loginBtn = await widget.getByRole('button', { name: /claude login/ }).count();
        if (loginBtn > 0) {
          bugs.push({ severity: 'Major', symptom: '이미 로그인된 상태인데도 "터미널 열기" 로그인 버튼이 노출됨(불필요한 액션 노출)', file: 'src/views/home.ts:claudeAuthWidget' });
        }

        // ---- 로그아웃 버튼: 위젯 안에 있고, 라벨이 사이드바의 앱 자체
        // "로그아웃"(sidebar-logout span)과 다르다. 사이드바 요소는 모든
        // 화면에 함께 렌더되므로 페이지 전역 getByRole('로그아웃')로 확인하면
        // 두 요소가 동시에 걸려 어느 쪽인지 구분되지 않는다 — 그래서 위젯
        // 스코프(widget.getByRole)로만 조회한다(작업 지시 경고와 동일한 함정
        // 회피).
        const logoutBtn = widget.getByRole('button', { name: 'Anthropic 계정 로그아웃' });
        if ((await logoutBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '로그인된 상태인데 claude CLI 로그아웃 버튼이 없음', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        // 사이드바 앱 자체 로그아웃과 정확히 같은 텍스트("로그아웃")를 쓰는
        // 버튼이 없어야 한다 — 있으면 두 로그아웃이 라벨로 구분되지 않는다.
        const sameLabelAsSidebar = widget.getByRole('button', { name: '로그아웃', exact: true });
        if ((await sameLabelAsSidebar.count()) > 0) {
          bugs.push({
            severity: 'Critical',
            symptom: '위젯의 claude CLI 로그아웃 버튼이 사이드바 앱 로그아웃과 정확히 같은 라벨("로그아웃")을 씀 — 둘을 구분할 수 없음',
            file: 'src/views/home.ts:claudeAuthWidget',
          });
        }
      },
    },
    // ⑦ claude CLI 로그아웃 — 확인 다이얼로그를 취소하면 아무 일도 일어나지
    // 않는다(logout_claude_auth 미호출, 배지도 그대로).
    {
      id: 'logout-cancelled-does-not-log-out',
      fixtures: {
        ...base,
        check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        logout_claude_auth: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const logoutBtn = widget.getByRole('button', { name: 'Anthropic 계정 로그아웃' });
        if ((await logoutBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '로그인된 상태인데 claude CLI 로그아웃 버튼이 없음', file: 'src/views/home.ts:claudeAuthWidget' });
          return;
        }
        await logoutBtn.click();
        await page.waitForTimeout(150);
        const overlay = page.locator('.modal-overlay');
        if ((await overlay.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'claude CLI 로그아웃 버튼을 눌러도 확인 다이얼로그가 뜨지 않음', file: 'src/views/home.ts:handleClaudeAuthLogout' });
          return;
        }
        await shot('01-logout-confirm-dialog');
        await overlay.getByRole('button', { name: '취소' }).click();
        await page.waitForTimeout(150);
        const invoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'logout_claude_auth'));
        if (invoked) {
          bugs.push({ severity: 'Critical', symptom: '확인 다이얼로그에서 "취소"를 눌렀는데도 logout_claude_auth가 호출됨', file: 'src/views/home.ts:handleClaudeAuthLogout' });
        }
        const stillLoggedIn = await widget.textContent().catch(() => null);
        if (!stillLoggedIn || !stillLoggedIn.includes('로그인됨')) {
          bugs.push({ severity: 'Critical', symptom: `로그아웃을 취소했는데 배지가 바뀜(실제: ${stillLoggedIn})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
    // ⑧ claude CLI 로그아웃 확정 — 완료 후 배지가 즉시 "로그인 필요"로
    // 바뀐다(로그인 완료 시 즉시 갱신과 대칭인 요구사항). check_claude_auth_status
    // 픽스처는 의도적으로 낡은 "로그인됨" 값으로 고정해둔다 — logout_claude_auth의
    // 응답만으로(재조회 없이) 갱신되는지가 이 시나리오의 핵심(⑥과 동일한 검증
    // 기법).
    {
      id: 'logout-confirmed-updates-dashboard-badge-immediately',
      fixtures: {
        ...base,
        check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        logout_claude_auth: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        await widget.getByRole('button', { name: 'Anthropic 계정 로그아웃' }).click();
        await page.waitForTimeout(150);
        const overlay = page.locator('.modal-overlay');
        await overlay.getByRole('button', { name: '로그아웃 확인' }).click();
        await page.waitForTimeout(250);
        await shot('01-logout-confirmed-badge-updated');

        const invoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'logout_claude_auth'));
        if (!invoked) {
          bugs.push({ severity: 'Critical', symptom: '확인 다이얼로그에서 확정해도 logout_claude_auth가 호출되지 않음', file: 'src/views/home.ts:handleClaudeAuthLogout' });
        }
        const text = await widget.textContent().catch(() => null);
        if (!text || text.includes('로그인됨')) {
          bugs.push({
            severity: 'Critical',
            symptom: `로그아웃 완료 직후에도 배지가 "로그인 필요"로 바뀌지 않음(실제: ${text}) — check_claude_auth_status 재조회에만 의존하는 회귀`,
            file: 'src/views/home.ts:handleClaudeAuthLogout',
          });
        }
        const loginBtnBack = widget.getByRole('button', { name: '앱에서 로그인' });
        if ((await loginBtnBack.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '로그아웃 완료 후 "앱에서 로그인" 버튼이 다시 나타나지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
    // ⑨ 로그아웃 재확인(status --json) 자체가 실패하면(확인 불가) "로그아웃
    // 됨"으로 단정하지 않는다 — claude_auth.rs 상단 경계와 동일한 원칙을
    // 프론트에서도 지킨다. 배지는 로그인됨 그대로 남아야 한다.
    {
      id: 'logout-verify-failure-does-not-assume-logged-out',
      fixtures: {
        ...base,
        check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        logout_claude_auth: { __throw: '테스트 강제 에러: claude auth status --json 재확인 실패' },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        await widget.getByRole('button', { name: 'Anthropic 계정 로그아웃' }).click();
        await page.waitForTimeout(150);
        const overlay = page.locator('.modal-overlay');
        await overlay.getByRole('button', { name: '로그아웃 확인' }).click();
        await page.waitForTimeout(250);
        await shot('01-logout-verify-failed-badge-unchanged');

        const text = await widget.textContent().catch(() => null);
        if (!text || !text.includes('로그인됨')) {
          bugs.push({
            severity: 'Critical',
            symptom: `재확인 조회 실패(확인 불가)인데도 배지가 "로그인됨"에서 바뀜(실제: ${text}) — 확인 불가를 로그아웃됨으로 단정함`,
            file: 'src/views/home.ts:handleClaudeAuthLogout',
          });
        }
        const toastCount = await page.locator('.toast', { hasText: '확인하지 못했습니다' }).count();
        if (toastCount === 0) {
          bugs.push({ severity: 'Minor', symptom: '재확인 실패 토스트가 뜨지 않음', file: 'src/views/home.ts:handleClaudeAuthLogout' });
        }
      },
    },
    // ⑩ 이중 클릭 방지 — 왕복 중 버튼이 비활성화되고 라벨이 "로그아웃 중…"로
    // 바뀐다. __delay로 응답을 늦춰 그 사이 창을 관찰한다.
    {
      id: 'logout-in-flight-disables-button',
      fixtures: {
        ...base,
        check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        logout_claude_auth: { __delay: 600, value: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null } },
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        await widget.getByRole('button', { name: 'Anthropic 계정 로그아웃' }).click();
        await page.waitForTimeout(150);
        const overlay = page.locator('.modal-overlay');
        await overlay.getByRole('button', { name: '로그아웃 확인' }).click();
        await page.waitForTimeout(150);
        await shot('01-logout-in-flight');

        const inFlightBtn = widget.getByRole('button', { name: '로그아웃 중…' });
        if ((await inFlightBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '로그아웃 요청 왕복 중 버튼 라벨이 "로그아웃 중…"으로 바뀌지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
        } else if (!(await inFlightBtn.isDisabled())) {
          bugs.push({ severity: 'Critical', symptom: '로그아웃 요청이 진행 중인데도 버튼이 비활성화되지 않음(이중 클릭 가능)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        await page.waitForTimeout(700);
        const calls = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'logout_claude_auth'));
        if (calls.length !== 1) {
          bugs.push({ severity: 'Critical', symptom: `logout_claude_auth 호출 횟수가 1이 아님(실제: ${calls.length}) — 이중 클릭 방지 실패`, file: 'src/views/home.ts:handleClaudeAuthLogout' });
        }
      },
    },
    // ③ 로그인 안 됨 — loggedIn: false(명시적 응답은 받았지만 부정).
    {
      id: 'logged-out',
      fixtures: { ...base, check_claude_auth_status: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-logged-out');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const text = await widget.textContent().catch(() => null);
        if (!text || text.includes('로그인됨')) {
          bugs.push({ severity: 'Critical', symptom: `loggedIn:false 응답인데 "로그인됨"으로 보임(실제: ${text})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        // 완료 판정 4: "로그인 안 됨" 상태에서 터미널 열기 버튼이 실제로 보여야 한다.
        const loginBtn = widget.getByRole('button', { name: /claude login/ });
        if ((await loginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '로그인 안 됨 상태인데 "터미널 열기 (claude login)" 버튼이 보이지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
          return;
        }
        await loginBtn.click();
        await page.waitForTimeout(300);
        await shot('02-after-terminal-open-click');
        const calls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'open_claude_login_terminal') ?? []);
        if (calls.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '"터미널 열기" 클릭이 open_claude_login_terminal IPC를 호출하지 않음', file: 'src/views/home.ts:handleOpenClaudeLoginTerminalFromHome' });
        }
        // "터미널 열기" 버튼 클릭 단독으로는 헤드리스 로그인(start_claude_auth_login)이
        // 호출되면 안 된다 — 그 경로는 이제 별도의 "앱에서 로그인" 버튼에서만
        // 열린다(요구사항 2, home.mjs 'app-login-global-panel' 시나리오가 그
        // 버튼 쪽을 검증한다). 두 버튼이 서로의 커맨드를 침범하지 않는지 여기서
        // 함께 확인한다.
        const forbiddenCalls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'start_claude_auth_login') ?? []);
        if (forbiddenCalls.length > 0) {
          bugs.push({ severity: 'Critical', symptom: '"터미널 열기" 버튼 클릭만으로 start_claude_auth_login이 호출됨(두 버튼의 동작이 뒤섞임)', file: 'src/views/home.ts:handleOpenClaudeLoginTerminalFromHome' });
        }
      },
    },
    // ④ 확인 실패 — IPC 자체가 throw. "로그인 안 됨"과 같은 버킷(터미널 열기
    // 버튼 노출)으로 취급하되, 문구로는 "확인하지 못했습니다"를 구분해 보여준다.
    {
      id: 'check-error',
      fixtures: { ...base, check_claude_auth_status: { __throw: '테스트 강제 에러: claude auth status --json 실행 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-check-error');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const text = await widget.textContent().catch(() => null);
        if (!text || text.includes('로그인됨')) {
          bugs.push({ severity: 'Critical', symptom: `조회 자체가 실패했는데 "로그인됨"으로 보임(실제: ${text}) — "모름"과 "인증됨"이 구분되지 않음`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if (!text || !text.includes('확인하지 못했습니다')) {
          bugs.push({ severity: 'Minor', symptom: `조회 실패 사유가 "로그인 안 됨"과 구분되는 문구로 보이지 않음(실제: ${text})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const loginBtn = widget.getByRole('button', { name: /claude login/ });
        if ((await loginBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '조회 실패 상태에서도 "터미널 열기" 버튼이 보여야 하는데 없음(안전한 기본 액션 제공 실패)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
    // ⑤ 대시보드에서 시작한 "앱에서 로그인"이 화면 이동과 무관하게 항상 보인다
    // (malgn-vscode 로그인 버튼 통일 작업 요구사항 1·2·4) — 이전엔 로그인 진행
    // 패널(코드 입력창·취소 버튼)이 세션 상세/draft 화면의 bottomFixed에만
    // 있어서, 그 자리 자체가 없는 대시보드에서 로그인을 시작하면 입력창을 볼
    // 방법이 없었다(대시보드는 이전엔 터미널 열기 경로만 썼다). main.ts가
    // 이제 앱 셸(#app) 최상위에 라우트와 무관하게 모달로 그리는지(v0.2.13
    // 후속, 요구사항 2) 대시보드→세션목록→대시보드 왕복으로 확인한다.
    {
      id: 'app-login-global-panel',
      fixtures: { ...base, check_claude_auth_status: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null } },
      async run(page, { shot, bugs, emitEvent }) {
        const codeInputPlaceholder = 'claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)';
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const appLoginBtn = widget.getByRole('button', { name: '앱에서 로그인' });
        if ((await appLoginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '대시보드 로그인 위젯에 "앱에서 로그인" 버튼이 없음(요구사항 2 미구현)', file: 'src/views/home.ts:claudeAuthWidget' });
          return;
        }
        await appLoginBtn.click();
        await page.waitForTimeout(150);

        const startInvoked = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'start_claude_auth_login'));
        if (!startInvoked) {
          bugs.push({ severity: 'Critical', symptom: '대시보드 "앱에서 로그인" 버튼을 눌러도 start_claude_auth_login 커맨드가 호출되지 않음', file: 'src/views/home.ts:claudeAuthWidget / src/views/sessions.ts:handleClaudeAuthLoginStart' });
        }

        // ---- 1) 대시보드에서 코드 입력창이 보인다 ----
        const codeInputOnDashboard = page.getByPlaceholder(codeInputPlaceholder);
        if ((await codeInputOnDashboard.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '대시보드에서 로그인을 시작해도 코드 입력창이 화면에 없음(bottomFixed가 없는 화면이라 갇힘)', file: 'src/main.ts:renderApp' });
        }
        await shot('01-login-active-on-dashboard');

        // ---- 2) 세션목록으로 이동해도 입력창이 그대로 있다 ----
        await page.evaluate(() => { window.location.hash = '#/sessions'; });
        await page.waitForTimeout(200);
        await shot('02-login-active-on-sessions-list');
        const codeInputOnList = page.getByPlaceholder(codeInputPlaceholder);
        if ((await codeInputOnList.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '세션목록 화면으로 이동하면 로그인 코드 입력창이 사라짐', file: 'src/main.ts:renderApp' });
        }

        // ---- 3) 대시보드로 복귀해도 입력창이 다시 있다 ----
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForTimeout(200);
        await shot('03-login-active-back-on-dashboard');
        const codeInputBack = page.getByPlaceholder(codeInputPlaceholder);
        if ((await codeInputBack.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '대시보드로 복귀해도 로그인 코드 입력창이 다시 보이지 않음', file: 'src/main.ts:renderApp' });
        }

        // ---- 정리: 취소로 idle 복귀(다른 시나리오 오염 방지) ----
        const cancelBtn = page.getByRole('button', { name: '로그인 취소' });
        if ((await cancelBtn.count()) > 0) {
          await cancelBtn.click();
          await page.waitForTimeout(100);
          await emitEvent('claude-auth-login-finished', { ok: false, canceled: true, error: null, status: null });
          await page.waitForTimeout(150);
        }
      },
    },
    // ⑥ 실사용자 보고 1번(v0.2.13) 핵심 회귀 고정 — "인증은 성공했는데 대시보드는
    // 계속 '연결 안 됨'". 원인은 main.ts:handleNavigation()의
    // `!state.claudeAuth.loaded` 가드가 이미 loaded===true인 뒤로는 다시 불리지
    // 않아, 로그인 완료 후에도 앱을 재시작하기 전까지 예전 값이 남아있던 것이다
    // (claude_auth.rs 확인 불가/실패 판정 회귀와는 별개 지점). check_claude_auth_status
    // 픽스처를 의도적으로 "미로그인"(오래된 값)으로 고정해두고, 로그인 완료
    // 이벤트가 최신 status를 직접 실어 보내면 그 낡은 픽스처를 다시 조회하지
    // 않고도(재확인 호출 없이) 위젯이 즉시 "로그인됨"으로 바뀌는지 확인한다 —
    // 재조회에 의존했다면 이 시나리오는 여전히 "로그인 필요"로 남아 실패한다.
    {
      id: 'login-finished-updates-dashboard-badge-immediately',
      fixtures: { ...base, check_claude_auth_status: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null } },
      async run(page, { shot, bugs, emitEvent }) {
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        const appLoginBtn = widget.getByRole('button', { name: '앱에서 로그인' });
        if ((await appLoginBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '대시보드 로그인 위젯에 "앱에서 로그인" 버튼이 없음', file: 'src/views/home.ts:claudeAuthWidget' });
          return;
        }
        await appLoginBtn.click();
        await page.waitForTimeout(150);
        await shot('01-login-started-from-dashboard');

        // check_claude_auth_status 픽스처는 여전히 loggedIn:false(낡은 값) —
        // 이 이벤트가 실어 보내는 status만으로 갱신되는지가 이 시나리오의 핵심.
        await emitEvent('claude-auth-login-finished', {
          ok: true,
          canceled: false,
          error: null,
          status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' },
        });
        await page.waitForTimeout(200);
        await shot('02-badge-after-finished-event');

        const text = await widget.textContent().catch(() => null);
        if (!text || !text.includes('로그인됨')) {
          bugs.push({
            severity: 'Critical',
            symptom: `로그인 완료 이벤트 직후에도 대시보드 배지가 "로그인됨"으로 바뀌지 않음(실제: ${text}) — 앱 재시작 전까지 옛 상태가 남는 실사용자 보고 1번 회귀`,
            file: 'src/main.ts:initClaudeAuthLoginWatcher',
          });
        }
        const stillLoginNeeded = widget.getByRole('button', { name: '앱에서 로그인' });
        if ((await stillLoginNeeded.count()) > 0) {
          bugs.push({ severity: 'Major', symptom: '로그인 완료 후에도 "앱에서 로그인" 버튼이 남아있음(성공이 반영되지 않음)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
  ];
}
