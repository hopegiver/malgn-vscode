// 흐름 (8) 대시보드 claude CLI 로그인 상태 위젯 — src/views/home.ts claudeAuthWidget
//
// 검증 목표: "확인 중 / 로그인됨 / 로그인 안 됨·확인 실패" 세 상태가 서로 다른
// 신호(loading/loggedIn===true/그 외)로만 구분되고, "없음"(에러가 없다 등)이
// 아니라 "있음"(그 상태를 나타내는 요소가 실제로 화면에 보인다)으로 판정한다
// (직전 v0.2.11 회귀 — "대기 중"과 "정상"이 같은 신호를 냈던 사고의 재발 방지).
export const flowId = 'home';
export const startHash = '#/';


// ---- P2-1 온보딩 체크리스트 시나리오 공용 픽스처 ----
// check_dev_tools 응답은 백엔드 DEV_TOOLS 테이블의 실제 모양을 그대로 싣는다 — required는 node·git·
// claude가 true(Claude는 P2-2에서 필수로 바뀜), claude의 installDependsOn은 ['node']. 하네스 베이스
// 샘플에는 required/git이 없어 이 시나리오들이 직접 만든다.
// claudePathWhenMissing: installed=false인데 path가 있는 상태(경로는 찾았으나 `--version` 조회가
// 타임아웃·빈 출력으로 실패) — 백엔드 query.rs가 낼 수 있는 모양이다.
function devToolsFixture({ claudeInstalled = true, nodeInstalled = true, gitInstalled = true, claudePathWhenMissing = null } = {}) {
  const tool = (id, name, installed, required, version, extra = {}) => ({
    id, name, installed, version: installed ? version : null, path: installed ? `/usr/local/bin/${id}` : null,
    installMethod: installed ? 'brew' : null, actionKind: installed ? 'manual' : 'run', manualHint: null, required,
    installDependsOn: [], ...extra,
  });
  return [
    tool('claude', 'Claude Code', claudeInstalled, true, '2.1.0', {
      installDependsOn: ['node'],
      ...(!claudeInstalled && claudePathWhenMissing ? { path: claudePathWhenMissing } : {}),
    }),
    tool('node', 'Node.js', nodeInstalled, true, '22.9.0'),
    tool('git', 'Git', gitInstalled, true, '2.45.0'),
    tool('gh', 'GitHub CLI', false, false, null),
  ];
}
const AUTH_OUT = { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null };
const AUTH_IN = { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' };
const CHECKLIST = 'src/views/home.ts:renderOnboarding / src/onboarding.ts';

async function stepState(page, kind) {
  const row = page.locator(`.onboard-row[data-step="${kind}"]`);
  if ((await row.count()) === 0) return null;
  return { state: await row.getAttribute('data-state'), current: (await row.getAttribute('class')).includes('is-current') };
}

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
    // ⑨ 홈 "최근 세션" 박스에서 행 클릭 → 세션 상세 → 뒤로가기 = 홈.
    // src/sessionRow.ts의 sessionBlistRow()가 항상 '#/sessions'로만 돌아가던
    // 라우팅 버그의 세 번째 진입점(사용자는 projects-detail만 보고했지만 같은
    // 컴포넌트를 쓰는 홈도 동일 결함이 있었다) — 고정 회귀 시나리오.
    {
      id: 'recent-session-back-nav',
      fixtures: base,
      async run(page, { shot, bugs }) {
        const row = page.locator('.blist-row--sessions').first();
        if ((await row.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'recent-session-back-nav 시나리오: 홈 "최근 세션" 박스에 행이 없음(기본 픽스처 확인 필요)', file: 'scripts/ui-harness/flows/home.mjs' });
          return;
        }
        await shot('01-home-recent-sessions');
        await row.click();
        await page.waitForTimeout(150);
        const hash = await page.evaluate(() => window.location.hash);
        if (!hash.startsWith('#/sessions/')) {
          bugs.push({ severity: 'Major', symptom: `홈 최근 세션 행 클릭이 세션 상세로 이동하지 않음(hash=${hash})`, file: 'src/sessionRow.ts' });
        }
        if (!hash.includes('back=home')) {
          bugs.push({ severity: 'Critical', symptom: `홈에서 세션 클릭 시 뒤로가기 목적지(back=home)가 해시에 인코딩되지 않음(실제: ${hash})`, file: 'src/sessionRow.ts / src/views/home.ts' });
        }
        await page.waitForSelector('.chat-page');
        await shot('02-session-detail-from-home');
        await page.locator('.back-link', { hasText: '← 세션' }).click();
        await page.waitForTimeout(150);
        const backHash = await page.evaluate(() => window.location.hash);
        if (backHash !== '#/' && backHash !== '') {
          bugs.push({ severity: 'Critical', symptom: `홈→최근 세션→상세→뒤로가기가 홈(#/)으로 가지 않음(실제: ${backHash})`, file: 'src/views/sessions.ts:renderSessionDetailView' });
        }
        await page.waitForSelector('.greet-sub');
        await shot('03-back-to-home');
      },
    },
    // ===== P2-1 온보딩 체크리스트 =====
    // (a) claude 미설치(개발 도구 조회가 installed:false로 확정) — 로그인 위젯은 로그인 버튼 대신
    // 설치 버튼, 체크리스트는 설치 단계가 todo(현재)이고 로그인 단계는 blocked.
    {
      id: 'onboarding-claude-missing-shows-install-not-login',
      fixtures: { ...base, check_dev_tools: devToolsFixture({ claudeInstalled: false }), check_claude_auth_status: { __throw: 'claude: command not found' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-claude-missing');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        if ((await widget.getByRole('button', { name: 'Claude Code 설치하기' }).count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'claude 미설치 확정인데 로그인 위젯에 "Claude Code 설치하기" 버튼이 없음', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if ((await widget.getByRole('button', { name: '앱에서 로그인' }).count()) > 0 || (await widget.getByRole('button', { name: /claude login/ }).count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: 'claude 미설치 확정인데 "앱에서 로그인"/"터미널 열기" 버튼이 노출됨', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const install = await stepState(page, 'claude-install');
        const login = await stepState(page, 'claude-login');
        if (!install || install.state !== 'todo' || !install.current) {
          bugs.push({ severity: 'Critical', symptom: `claude 설치 단계가 todo+현재 단계가 아님(실제: ${JSON.stringify(install)})`, file: CHECKLIST });
        }
        // RV-001: Claude는 required=true이지만 별도 2단계가 전담한다 — 1단계(필수 도구)는 done이고
        // 진행 카운트는 1/3(Node·Git 설치됨)이어야 한다(Claude가 1·2단계에 중복 집계되면 0/3).
        const reqStep = await stepState(page, 'required-tools');
        if (!reqStep || reqStep.state !== 'done') {
          bugs.push({ severity: 'Critical', symptom: `Claude만 없는데 필수 도구 단계가 done이 아님(실제: ${JSON.stringify(reqStep)}) — Claude가 1단계에 중복 집계됨`, file: 'src/onboarding.ts:requiredToolsStep' });
        }
        const missingCount = await page.locator('.onboard-box .box-head .count').textContent().catch(() => null);
        if (!missingCount || !missingCount.includes('1/3')) {
          bugs.push({ severity: 'Major', symptom: `Claude만 없는 상태의 진행 카운트가 1/3이 아님(실제: ${missingCount})`, file: CHECKLIST });
        }
        const reqDetail = await page.locator('.onboard-row[data-step="required-tools"] .onboard-detail').textContent().catch(() => '');
        if (reqDetail && reqDetail.includes('Claude')) {
          bugs.push({ severity: 'Major', symptom: `필수 도구 단계 설명에 Claude가 중복 표기됨(실제: ${reqDetail})`, file: 'src/onboarding.ts:requiredToolsStep' });
        }
        if (!login || login.state !== 'blocked') {
          bugs.push({ severity: 'Critical', symptom: `미설치인데 로그인 단계가 blocked가 아님(실제: ${JSON.stringify(login)}) — 조회 에러를 해석해 단정함`, file: CHECKLIST });
        }
        if ((await page.locator('.onboard-row[data-step="claude-login"]').getByRole('button').count()) > 0) {
          bugs.push({ severity: 'Major', symptom: '미설치 상태에서 체크리스트 로그인 단계에 시작 버튼이 노출됨', file: CHECKLIST });
        }
        // 설치 버튼은 새 설치 실행이 아니라 개발 도구 화면으로 보낼 뿐이다.
        await widget.getByRole('button', { name: 'Claude Code 설치하기' }).click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        if (hash !== '#/settings/devtools') {
          bugs.push({ severity: 'Critical', symptom: `설치 버튼이 개발 도구 화면으로 이동하지 않음(hash=${hash})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const forbidden = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => ['install_dev_tool', 'update_dev_tool', 'open_manual_instruction', 'start_claude_auth_login'].includes(e.cmd)));
        if (forbidden.length > 0) {
          bugs.push({ severity: 'Critical', symptom: `설치 버튼 클릭이 실행 커맨드를 호출함: ${JSON.stringify(forbidden)}`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        await shot('02-after-install-button-devtools');
      },
    },
    // (a') RV-003: claude 경로는 있는데 버전 조회가 실패해 installed=false — 설치 "확정" 미달이므로
    // 기존 로그인 버튼을 유지하고 설치 버튼으로 바꾸지 않는다. 설치 단계는 단정 없이 확인 불가(error).
    {
      id: 'onboarding-claude-path-present-version-failed-keeps-login',
      fixtures: { ...base, check_dev_tools: devToolsFixture({ claudeInstalled: false, claudePathWhenMissing: '/Users/dev/.local/bin/claude' }), check_claude_auth_status: AUTH_OUT },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-claude-path-present-version-failed');
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        if ((await widget.getByRole('button', { name: 'Claude Code 설치하기' }).count()) > 0) {
          bugs.push({ severity: 'Major', symptom: 'claude 경로가 있는데(버전 조회만 실패) 설치 안내 버튼으로 미설치를 단정함', file: 'src/onboarding.ts:claudeInstallState' });
        }
        if ((await widget.getByRole('button', { name: '앱에서 로그인' }).count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '미설치 확정이 아닌데 기존 "앱에서 로그인" 버튼이 사라짐', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const text = (await widget.textContent()) ?? '';
        if (text.includes('설치되어 있지 않아')) {
          bugs.push({ severity: 'Major', symptom: `경로가 있는데 "설치되어 있지 않아…" 단정 문구가 보임(실제: ${text})`, file: 'src/views/home.ts:claudeAuthWidget' });
        }
        const inst = await stepState(page, 'claude-install');
        const login = await stepState(page, 'claude-login');
        if (!inst || inst.state !== 'error') {
          bugs.push({ severity: 'Major', symptom: `경로는 있고 버전 조회 실패인데 설치 단계가 error(확인 불가)가 아님(실제: ${JSON.stringify(inst)})`, file: 'src/onboarding.ts:claudeInstallStep' });
        }
        if (login && login.state === 'blocked') {
          bugs.push({ severity: 'Major', symptom: '설치 미확정인데 로그인 단계가 blocked로 단정됨', file: 'src/onboarding.ts:claudeLoginStep' });
        }
        const instDetail = await page.locator('.onboard-row[data-step="claude-install"] .onboard-detail').textContent().catch(() => '');
        if (instDetail && instDetail.includes('설치되어 있지 않습니다')) {
          bugs.push({ severity: 'Major', symptom: `설치 단계 설명이 미설치로 단정함(실제: ${instDetail})`, file: 'src/onboarding.ts:claudeInstallStep' });
        }
      },
    },
    // (b) 설치됨 + 미로그인 — 설치 두 단계 done, 로그인 단계가 현재 단계, 위젯은 기존 로그인 버튼 유지.
    {
      id: 'onboarding-installed-not-logged-in-login-is-current',
      fixtures: { ...base, check_dev_tools: devToolsFixture(), check_claude_auth_status: AUTH_OUT },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        await shot('01-installed-not-logged-in');
        const req = await stepState(page, 'required-tools');
        const inst = await stepState(page, 'claude-install');
        const login = await stepState(page, 'claude-login');
        if (!req || req.state !== 'done') bugs.push({ severity: 'Critical', symptom: `필수 도구 단계가 done이 아님(실제: ${JSON.stringify(req)})`, file: CHECKLIST });
        if (!inst || inst.state !== 'done') bugs.push({ severity: 'Critical', symptom: `Claude Code 설치 단계가 done이 아님(실제: ${JSON.stringify(inst)})`, file: CHECKLIST });
        if (!login || login.state !== 'todo' || !login.current) {
          bugs.push({ severity: 'Critical', symptom: `로그인 단계가 todo+현재 단계가 아님(실제: ${JSON.stringify(login)})`, file: CHECKLIST });
        }
        const head = await page.locator('.onboard-box .box-head .count').textContent().catch(() => null);
        if (!head || !head.includes('2/3')) bugs.push({ severity: 'Major', symptom: `진행 카운트가 2/3이 아님(실제: ${head})`, file: CHECKLIST });
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        if ((await widget.getByRole('button', { name: '앱에서 로그인' }).count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '설치됨 상태인데 위젯의 "앱에서 로그인" 버튼이 사라짐(기존 동작 훼손)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if ((await widget.getByRole('button', { name: 'Claude Code 설치하기' }).count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '설치됨 상태인데 설치 버튼이 노출됨', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        // 체크리스트 "로그인하기"는 위젯과 같은 시작 경로(start_claude_auth_login)를 쓴다.
        const startBtn = page.locator('.onboard-row[data-step="claude-login"]').getByRole('button', { name: '로그인하기' });
        if ((await startBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '현재 단계(로그인)에 "로그인하기" 버튼이 없음', file: CHECKLIST });
        } else {
          await startBtn.click();
          await page.waitForTimeout(150);
          const started = await page.evaluate(() => (window.__invokeLog ?? []).some((e) => e.cmd === 'start_claude_auth_login'));
          if (!started) bugs.push({ severity: 'Major', symptom: '"로그인하기" 클릭이 start_claude_auth_login을 호출하지 않음', file: CHECKLIST });
          await shot('02-login-started-from-checklist');
          const cancelBtn = page.getByRole('button', { name: '로그인 취소' });
          if ((await cancelBtn.count()) > 0) {
            await cancelBtn.click();
            await page.waitForTimeout(100);
            await emitEvent('claude-auth-login-finished', { ok: false, canceled: true, error: null, status: null });
            await page.waitForTimeout(150);
          }
        }
      },
    },
    // (b') 필수 도구(node) 미설치 — 첫 단계가 현재 단계, 로그인 단계는 현재가 아니며 시작 버튼이 없다.
    {
      id: 'onboarding-required-tool-missing-first-step-current',
      fixtures: { ...base, check_dev_tools: devToolsFixture({ nodeInstalled: false }), check_claude_auth_status: AUTH_OUT },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-required-missing');
        const req = await stepState(page, 'required-tools');
        const login = await stepState(page, 'claude-login');
        if (!req || req.state !== 'todo' || !req.current) bugs.push({ severity: 'Critical', symptom: `필수 도구 단계가 todo+현재가 아님(실제: ${JSON.stringify(req)})`, file: CHECKLIST });
        if (login && login.current) bugs.push({ severity: 'Critical', symptom: '선행 설치 단계가 미완료인데 로그인 단계가 현재 단계로 표시됨', file: CHECKLIST });
        const detail = await page.locator('.onboard-row[data-step="required-tools"] .onboard-detail').textContent().catch(() => null);
        if (!detail || !detail.includes('Node.js')) bugs.push({ severity: 'Minor', symptom: `누락된 도구명이 설명에 없음(실제: ${detail})`, file: CHECKLIST });
      },
    },
    // (c) 전부 완료 — 체크리스트가 DOM에서 사라진다.
    {
      id: 'onboarding-all-done-hides-checklist',
      // OTEL 수집키도 적용·수집 준비 완료 — "모든 것이 끝났을 때 아무것도 안 남는다"를 본다(미적용이면 OTEL 한 줄이 남는 것은 otel-link-core-done-* 시나리오 몫).
      fixtures: { ...base, check_dev_tools: devToolsFixture(), check_claude_auth_status: AUTH_IN, otel_ingest_link_status: { state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_READY } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-all-done');
        const n = (await page.locator('.onboard-box').count()) + (await page.locator('.onboard-slim').count());
        if (n > 0) bugs.push({ severity: 'Critical', symptom: '모든 단계가 완료인데 체크리스트(또는 안내 줄)가 남아 있음', file: CHECKLIST });
        if ((await page.locator('.home-widget', { hasText: '로그인됨' }).count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '완료 상태에서 기존 로그인됨 위젯이 보이지 않음', file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
    // (d) 로딩 — 응답 전에는 완료/미완료로 단정하지 않는다(얇은 "확인 중" 줄만).
    {
      id: 'onboarding-loading-does-not-assert',
      fixtures: { ...base, check_dev_tools: { __delay: 2500, value: devToolsFixture() }, check_claude_auth_status: { __delay: 2500, value: AUTH_IN }, otel_ingest_link_status: { state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_READY } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-loading');
        const slim = page.locator('.onboard-slim');
        if ((await slim.count()) === 0 || (await slim.getAttribute('data-state')) !== 'loading') {
          bugs.push({ severity: 'Critical', symptom: '로딩 중인데 "확인 중" 안내 줄이 없음', file: CHECKLIST });
        }
        if ((await page.locator('.onboard-box').count()) > 0 || (await page.locator('.onboard-row.is-done, .onboard-row.is-todo').count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '로딩 중인데 단계가 완료/미완료로 단정되어 그려짐', file: CHECKLIST });
        }
        await page.waitForTimeout(2800);
        await shot('02-after-resolve-all-done');
        if ((await page.locator('.onboard-box').count()) + (await page.locator('.onboard-slim').count()) > 0) {
          bugs.push({ severity: 'Major', symptom: '로딩이 끝나 전부 완료로 확정됐는데 체크리스트가 사라지지 않음', file: CHECKLIST });
        }
      },
    },
    // (d') 에러 — 개발 도구·로그인 조회가 모두 실패하면 단정하지 않고 "확인하지 못했습니다" +
    // 다시 확인. 위젯은 설치 여부를 모르므로 기존 로그인 버튼을 유지한다(미설치로 단정 금지).
    {
      id: 'onboarding-error-does-not-assert',
      fixtures: { ...base, check_dev_tools: { __throw: '테스트 강제 에러: check_dev_tools 실패' }, check_claude_auth_status: { __throw: '테스트 강제 에러: claude auth status 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-both-error');
        const slim = page.locator('.onboard-slim');
        if ((await slim.count()) === 0 || (await slim.getAttribute('data-state')) !== 'error') {
          bugs.push({ severity: 'Critical', symptom: '조회가 모두 실패했는데 "확인하지 못했습니다" 안내 줄이 없음', file: CHECKLIST });
        } else {
          const text = await slim.textContent();
          if (!text.includes('확인하지 못했습니다')) bugs.push({ severity: 'Major', symptom: `에러 안내 문구 불일치(실제: ${text})`, file: CHECKLIST });
          if ((await slim.getByRole('button', { name: '다시 확인' }).count()) === 0) bugs.push({ severity: 'Major', symptom: '에러 안내 줄에 "다시 확인" 버튼이 없음', file: CHECKLIST });
        }
        if ((await page.locator('.onboard-row.is-done, .onboard-row.is-todo').count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '조회 실패 상태에서 단계가 완료/미완료로 단정됨', file: CHECKLIST });
        }
        const widget = page.locator('.home-widget', { hasText: 'claude CLI 로그인' });
        if ((await widget.getByRole('button', { name: '앱에서 로그인' }).count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '설치 여부를 모르는 에러 상태에서 기존 "앱에서 로그인" 버튼이 사라짐(미설치로 단정)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
        if ((await widget.getByRole('button', { name: 'Claude Code 설치하기' }).count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '개발 도구 조회 실패 상태에서 설치 버튼이 노출됨(미설치로 단정)', file: 'src/views/home.ts:claudeAuthWidget' });
        }
      },
    },
    // (d'') 혼합 — 개발 도구만 실패, 로그인은 미로그인으로 확정: 박스는 열리되 설치 두 단계는
    // 확인 불가(error)이고 로그인 단계는 선행 단계가 확정되지 않았으므로 현재 단계가 아니다.
    {
      id: 'onboarding-devtools-error-login-known',
      fixtures: { ...base, check_dev_tools: { __throw: '테스트 강제 에러: check_dev_tools 실패' }, check_claude_auth_status: AUTH_OUT },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-devtools-error-login-known');
        const req = await stepState(page, 'required-tools');
        const inst = await stepState(page, 'claude-install');
        const login = await stepState(page, 'claude-login');
        if (!req || req.state !== 'error' || !inst || inst.state !== 'error') {
          bugs.push({ severity: 'Critical', symptom: `개발 도구 조회 실패인데 설치 단계가 error가 아님(실제: ${JSON.stringify([req, inst])})`, file: CHECKLIST });
        }
        if (!login || login.state !== 'todo' || login.current) {
          bugs.push({ severity: 'Critical', symptom: `로그인 단계가 todo(비현재)가 아님(실제: ${JSON.stringify(login)})`, file: CHECKLIST });
        }
        if ((await page.locator('.onboard-row[data-step="claude-login"]').getByRole('button', { name: '로그인하기' }).count()) > 0) {
          bugs.push({ severity: 'Major', symptom: '선행 단계가 확정되지 않았는데 로그인 시작 버튼이 노출됨', file: CHECKLIST });
        }
      },
    },
    // ---- R16 OTEL 연동 참고 항목(src/onboarding.ts:otelLinkStep) ----
    // 불변식: ① 상태별로 다른 문구·data-state ② 자동 연동/연동 완료/수신 확인 주장 없음 ③ mhd_/mhk_ 원문은 화면 어디에도 없음
    // ④ 진행 카운트(n/3)와 박스 노출 여부는 이 항목의 영향을 받지 않는다(참고 항목).
    ...otelLinkScenarios(base),
  ];
}

// 연동 상태 픽스처 — Rust IngestLinkStatus 직렬화와 같은 모양(수집 준비도 필드 포함).
const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const NO_EP = { targetsHub: false, insecure: false, foreignHost: false };
// 미연동/기본: 수집 준비 안 됨. READY: 텔레메트리 켜짐 + 허브 엔드포인트.
const LINK_OFF = { issueSupported: false, receiptCheckSupported: false, telemetryEnabled: false, endpoint: NO_EP, collectionReady: false };
const LINK_READY = { issueSupported: false, receiptCheckSupported: false, telemetryEnabled: true, endpoint: HUB_EP, collectionReady: true };
const FAKE_DEVICE_TOKEN = `mhd_${'x'.repeat(43)}`;
const FAKE_INGEST_KEY = `mhk_${'Z'.repeat(43)}`;
// 미구현 기능 주장 + 시도하지 않은 자동 동작을 암시하는 표현(RV-002)
const BANNED_CLAIMS = [
  '자동 연동', '연동 완료', '연동되었습니다', '수신 확인됨', '수신이 확인되었습니다', '수신을 확인했습니다',
  '자동으로 바꾸', '자동으로 교체', '자동으로 변경', '자동으로 삭제', '자동으로 갱신', '자동 교체', '바꾸지 않았습니다',
];
const HEADER_MSG = '설정 파일에 허브 수집키 형식이 아닌 Authorization 헤더가 있습니다. 허브로 보내려면 설정 > OTel에서 수집키로 교체하세요.';
const ENDPOINT_MSG = '수집 엔드포인트가 허브가 아니거나 암호화되지 않은(http) 주소입니다. 수집키가 그 주소로 전송되지 않도록 발급받지 않았습니다. 설정 > OTel에서 엔드포인트를 확인하세요.';

async function otelRow(page) {
  const row = page.locator('.onboard-row[data-step="otel-link"]');
  if ((await row.count()) === 0) return null;
  return {
    state: await row.getAttribute('data-state'),
    text: (await row.textContent()) ?? '',
    buttons: await row.locator('button').allTextContents(),
    marker: (await row.locator('.onboard-marker').textContent()) ?? '',
  };
}

// 필수 3단계 완료 후 홈에 한 줄로 나오는 OTEL 알림(.onboard-slim[data-step=otel-link]).
async function otelSlim(page) {
  const slim = page.locator('.onboard-slim[data-step="otel-link"]');
  if ((await slim.count()) === 0) return null;
  return {
    state: await slim.getAttribute('data-state'),
    text: (await slim.textContent()) ?? '',
    buttons: await slim.locator('button').allTextContents(),
    marker: (await slim.locator('.onboard-marker').textContent()) ?? '',
  };
}

async function checkNoSecrets(page, bugs, forbidSecrets = []) {
  const dump = await page.evaluate(() => document.documentElement.outerHTML + '\n' + document.body.innerText);
  for (const secret of [FAKE_DEVICE_TOKEN, FAKE_INGEST_KEY, ...forbidSecrets]) {
    if (dump.includes(secret) || dump.includes(secret.slice(0, 16))) {
      bugs.push({ severity: 'Critical', symptom: `DOM에 토큰/키 원문(앞 16자 포함)이 남아 있음: ${secret.slice(0, 8)}…`, file: CHECKLIST });
    }
  }
}

function checkBanned(bugs, text) {
  for (const phrase of BANNED_CLAIMS) {
    if (text.includes(phrase)) bugs.push({ severity: 'Critical', symptom: `미구현/시도하지 않은 동작을 주장하는 문구 "${phrase}" 노출`, file: CHECKLIST });
  }
}

async function checkOtelCommon(page, bugs, row, { expectState, mustInclude, mustExclude = [], forbidSecrets = [] }) {
  if (!row) {
    bugs.push({ severity: 'Critical', symptom: 'OTEL 연동 행이 렌더되지 않음', file: CHECKLIST });
    return;
  }
  if (row.state !== expectState) bugs.push({ severity: 'Critical', symptom: `OTEL 행 data-state가 ${expectState}가 아님(실제: ${row.state})`, file: CHECKLIST });
  for (const phrase of mustInclude) {
    if (!row.text.includes(phrase)) bugs.push({ severity: 'Major', symptom: `OTEL 행에 "${phrase}" 문구가 없음(실제: ${row.text})`, file: CHECKLIST });
  }
  for (const phrase of mustExclude) {
    if (row.text.includes(phrase)) bugs.push({ severity: 'Major', symptom: `OTEL 행에 있으면 안 되는 "${phrase}" 문구가 있음(실제: ${row.text})`, file: CHECKLIST });
  }
  checkBanned(bugs, row.text);
  await checkNoSecrets(page, bugs, forbidSecrets);
  const count = await page.locator('.onboard-box .box-head .count').textContent().catch(() => null);
  if (!count || !count.includes('2/3')) bugs.push({ severity: 'Major', symptom: `OTEL 항목이 진행 카운트에 영향을 줌(기대 2/3, 실제: ${count})`, file: CHECKLIST });
}

// 필수 3단계 완료 홈 — 박스는 열리지 않고, 노출 대상 상태일 때만 OTEL 한 줄이 있다.
async function checkCoreDoneSlim(page, bugs, { expectState, mustInclude, mustExclude = [], button }) {
  if ((await page.locator('.onboard-box').count()) > 0) {
    bugs.push({ severity: 'Critical', symptom: '필수 단계 완료 후 OTEL만으로 체크리스트 박스가 다시 열림(기존 사용자 회귀)', file: CHECKLIST });
  }
  if ((await page.locator('.onboard-slim').count()) !== 1) {
    bugs.push({ severity: 'Critical', symptom: `필수 완료 홈의 .onboard-slim이 정확히 1개가 아님(실제: ${await page.locator('.onboard-slim').count()})`, file: CHECKLIST });
  }
  const slim = await otelSlim(page);
  if (!slim) {
    bugs.push({ severity: 'Critical', symptom: '필수 완료 + OTEL 알림 대상 상태인데 OTEL 한 줄이 없음(RV-001: 실제 사용자가 볼 수 없음)', file: CHECKLIST });
    return null;
  }
  if (slim.state !== expectState) bugs.push({ severity: 'Critical', symptom: `OTEL 한 줄 data-state가 ${expectState}가 아님(실제: ${slim.state})`, file: CHECKLIST });
  for (const phrase of mustInclude) {
    if (!slim.text.includes(phrase)) bugs.push({ severity: 'Major', symptom: `OTEL 한 줄에 "${phrase}" 문구가 없음(실제: ${slim.text})`, file: CHECKLIST });
  }
  for (const phrase of mustExclude) {
    if (slim.text.includes(phrase)) bugs.push({ severity: 'Major', symptom: `OTEL 한 줄에 있으면 안 되는 "${phrase}"가 있음(실제: ${slim.text})`, file: CHECKLIST });
  }
  if (!slim.buttons.includes(button)) bugs.push({ severity: 'Major', symptom: `OTEL 한 줄에 "${button}" 버튼이 없음(실제: ${JSON.stringify(slim.buttons)})`, file: CHECKLIST });
  if (slim.marker.trim() === '✓') bugs.push({ severity: 'Critical', symptom: 'OTEL 한 줄이 완료 체크(✓)로 표시됨', file: CHECKLIST });
  checkBanned(bugs, slim.text);
  await checkNoSecrets(page, bugs);
  return slim;
}

async function checkNothingShown(page, bugs, why) {
  const n = await page.locator('.onboard-box, .onboard-slim').count();
  if (n > 0) bugs.push({ severity: 'Critical', symptom: `${why}인데 체크리스트/한 줄이 노출됨(${n}개)`, file: CHECKLIST });
  await checkNoSecrets(page, bugs);
}

async function clickToOtelSettings(page, bugs, locator, name) {
  await locator.getByRole('button', { name }).click();
  await page.waitForTimeout(200);
  if (!(await page.evaluate(() => location.hash)).startsWith('#/settings/otel')) bugs.push({ severity: 'Major', symptom: `"${name}" 버튼이 #/settings/otel로 이동하지 않음`, file: CHECKLIST });
}

function otelLinkScenarios(base) {
  // 필수 3단계 중 2개 완료(설치 완료·미로그인) — 체크리스트 박스가 열려 있는 상태에서 OTEL 행을 본다.
  const open = { ...base, check_claude_auth_status: AUTH_OUT };
  // 필수 3단계 모두 완료 — 박스는 숨고, OTEL이 알림 대상일 때만 한 줄이 남는다.
  const done = { ...base, check_claude_auth_status: AUTH_IN };
  const link = (extra) => ({ otel_ingest_link_status: extra });
  return [
    {
      id: 'otel-link-unlinked',
      fixtures: { ...open, ...link({ state: 'unlinked', masked: null, failure: null, ...LINK_OFF }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-unlinked');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, {
          expectState: 'todo',
          mustInclude: ['설정 > OTel', '붙여넣으세요', '발급 화면이 아직 없고', '자동 발급도 지원되지 않으며', '기기 토큰은 수집키가 아닙니다'],
        });
        if (row && !row.buttons.includes('수집키 입력하기')) bugs.push({ severity: 'Major', symptom: `unlinked인데 "수집키 입력하기" 버튼이 없음(실제: ${JSON.stringify(row.buttons)})`, file: CHECKLIST });
        if (row) await clickToOtelSettings(page, bugs, page.locator('.onboard-row[data-step="otel-link"]'), '수집키 입력하기');
      },
    },
    {
      id: 'otel-link-applied',
      // masked에 원문이 섞여 와도(방어 확인) 형식이 다르면 표시하지 않는다는 점은 별도 시나리오가 본다.
      fixtures: { ...open, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_READY }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-applied');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, {
          expectState: 'partial',
          mustInclude: ['mhk_AbCd…', '수신은 아직 확인하지 않았습니다', '새로 시작하는 Claude Code 세션부터 적용됩니다'],
          mustExclude: ['수집되지 않을 수 있습니다'],
        });
        if (row && row.marker.trim() === '✓') bugs.push({ severity: 'Critical', symptom: 'applied(수신 미확인)가 완료 체크(✓)로 표시됨', file: CHECKLIST });
      },
    },
    {
      // RV-003: 적용됐지만 텔레메트리가 꺼져 있어 수집되지 않을 수 있음 — "새로 시작하는 세션부터 적용"을 말하지 않는다.
      id: 'otel-link-applied-not-ready-telemetry-off',
      fixtures: { ...open, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_OFF, endpoint: HUB_EP }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-applied-not-ready');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, {
          expectState: 'warn',
          mustInclude: ['mhk_AbCd…', '수집되지 않을 수 있습니다', '텔레메트리가 꺼져 있음', '수신은 확인하지 않았습니다'],
          mustExclude: ['새로 시작하는', '허브가 아님'],
        });
        if (row && row.marker.trim() === '✓') bugs.push({ severity: 'Critical', symptom: '수집 불가 경고가 완료 체크(✓)로 표시됨', file: CHECKLIST });
        if (row && !row.buttons.includes('OTel 설정 열기')) bugs.push({ severity: 'Major', symptom: `수집 불가 경고에 "OTel 설정 열기" 버튼이 없음(실제: ${JSON.stringify(row.buttons)})`, file: CHECKLIST });
        if (row) await clickToOtelSettings(page, bugs, page.locator('.onboard-row[data-step="otel-link"]'), 'OTel 설정 열기');
      },
    },
    {
      // RV-003: 텔레메트리는 켜져 있지만 엔드포인트가 허브가 아님 — 원인이 위 시나리오와 구분돼야 한다.
      id: 'otel-link-applied-not-ready-endpoint-not-hub',
      fixtures: { ...open, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_OFF, telemetryEnabled: true, endpoint: { targetsHub: false, insecure: false, foreignHost: true } }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-applied-not-ready-endpoint');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, {
          expectState: 'warn',
          mustInclude: ['수집되지 않을 수 있습니다', '수집 엔드포인트가 허브가 아님', '허브 밖 주소로도 전송됨'],
          mustExclude: ['텔레메트리가 꺼져', '새로 시작하는'],
        });
      },
    },
    {
      id: 'otel-link-failed-header-unrecognized',
      fixtures: {
        ...open,
        ...link({ state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: HEADER_MSG }, ...LINK_OFF }),
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-failed');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, { expectState: 'failed', mustInclude: ['허브 수집키 형식이 아닌 Authorization 헤더가 있습니다', '수집키로 교체하세요'] });
        if (row && !row.buttons.includes('OTel 설정 열기')) bugs.push({ severity: 'Major', symptom: `failed인데 "OTel 설정 열기" 버튼이 없음(실제: ${JSON.stringify(row.buttons)})`, file: CHECKLIST });
      },
    },
    {
      id: 'otel-link-failed-endpoint-not-hub',
      fixtures: {
        ...open,
        ...link({ state: 'failed', masked: null, failure: { cause: 'endpointNotHub', message: ENDPOINT_MSG }, ...LINK_OFF }),
      },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-failed-endpoint');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, { expectState: 'failed', mustInclude: ['수집 엔드포인트가 허브가 아니거나', '엔드포인트를 확인하세요'] });
      },
    },
    {
      // 조회 실패 — 에러 문구에 기기 토큰 원문이 섞여 와도 errorMessage 경로가 마스킹해 화면에 원문이 없다.
      id: 'otel-link-fetch-error',
      fixtures: { ...open, otel_ingest_link_status: { __throw: `상태 조회 실패: 헤더 Bearer ${FAKE_DEVICE_TOKEN} 거부` } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-fetch-error');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, { expectState: 'error', mustInclude: ['수집키 적용 상태를 확인하지 못했습니다', '***'] });
        if (row && !row.buttons.includes('다시 확인')) bugs.push({ severity: 'Major', symptom: '조회 실패인데 "다시 확인" 버튼이 없음', file: CHECKLIST });
      },
    },
    {
      // 방어: 백엔드가 masked 자리에 원문 모양 값을 실수로 주더라도 화면에 내지 않는다.
      id: 'otel-link-applied-unsafe-masked-not-shown',
      fixtures: { ...open, ...link({ state: 'applied', masked: FAKE_INGEST_KEY, failure: null, ...LINK_READY }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-otel-applied-unsafe-masked');
        const row = await otelRow(page);
        await checkOtelCommon(page, bugs, row, { expectState: 'partial', mustInclude: ['수집키가 적용되었습니다', '수신은 아직 확인하지 않았습니다'] });
      },
    },
    {
      // RV-001: 필수 3단계가 끝나도 박스는 다시 열리지 않지만, 미적용이면 홈에 한 줄이 남는다.
      id: 'otel-link-core-done-slim-unlinked',
      fixtures: { ...done, ...link({ state: 'unlinked', masked: null, failure: null, ...LINK_OFF }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-core-done-slim-unlinked');
        const slim = await checkCoreDoneSlim(page, bugs, { expectState: 'todo', mustInclude: ['사용량 수집키가 적용되어 있지 않습니다'], button: '수집키 입력하기' });
        if (slim) await clickToOtelSettings(page, bugs, page.locator('.onboard-slim[data-step="otel-link"]'), '수집키 입력하기');
      },
    },
    {
      id: 'otel-link-core-done-slim-failed',
      fixtures: { ...done, ...link({ state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: HEADER_MSG }, ...LINK_OFF }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-core-done-slim-failed');
        await checkCoreDoneSlim(page, bugs, { expectState: 'failed', mustInclude: ['연동에 문제가 있습니다'], button: 'OTel 설정 열기' });
      },
    },
    {
      id: 'otel-link-core-done-slim-applied-not-ready',
      fixtures: { ...done, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_OFF, endpoint: HUB_EP }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-core-done-slim-warn');
        await checkCoreDoneSlim(page, bugs, {
          expectState: 'warn',
          mustInclude: ['수집키는 적용됐지만', '수집되지 않을 수 있습니다', '텔레메트리가 꺼져 있음'],
          mustExclude: ['새로 시작하는'],
          button: 'OTel 설정 열기',
        });
      },
    },
    {
      // 적용 + 수집 준비 완료 → 알릴 것이 없다. 박스도 한 줄도 없다.
      id: 'otel-link-core-done-ready-hidden',
      fixtures: { ...done, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...LINK_READY }) },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-core-done-ready-hidden');
        await checkNothingShown(page, bugs, '필수 완료 + 수집키 적용·수집 준비 완료');
      },
    },
    {
      // 조회 실패는 방해하지 않는다 — 필수 완료 홈에서 OTEL 조회 실패만으로 한 줄이 뜨지 않는다.
      id: 'otel-link-core-done-fetch-error-hidden',
      fixtures: { ...done, otel_ingest_link_status: { __throw: '상태 조회 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-core-done-fetch-error-hidden');
        await checkNothingShown(page, bugs, '필수 완료 + OTEL 조회 실패');
      },
    },
  ];
}
