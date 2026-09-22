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
        // 절대 금지 경로: 되돌린 헤드리스 로그인(start_claude_auth_login)을
        // 호출하면 안 된다 — 이번 클릭으로 그 커맨드가 불렸는지도 함께 확인한다.
        const forbiddenCalls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'start_claude_auth_login') ?? []);
        if (forbiddenCalls.length > 0) {
          bugs.push({ severity: 'Critical', symptom: '금지된 헤드리스 로그인 경로(start_claude_auth_login)가 호출됨', file: 'src/views/home.ts:handleOpenClaudeLoginTerminalFromHome' });
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
  ];
}
