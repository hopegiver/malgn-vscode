// 리뷰 검증 전용 — 프로젝트 파일은 일절 수정하지 않는다.
// 가설: renderRunHistoryCard -> ensureTaskHistoryLoaded -> loadTaskHistory -> notifyChange()가
// renderApp() 실행 도중 동기적으로 재진입 렌더를 일으켜 #app 자식이 2개가 아니라 4개가 된다.
import { chromium } from '/Users/hopegiver/workspace/malgn-vscode-detail-redesign/node_modules/playwright/index.mjs';
import { buildScenarioConfig, installTauriStub } from '/Users/hopegiver/workspace/malgn-vscode-detail-redesign/scripts/capture/stub.mjs';
import { AUTH_ROUTES } from '/Users/hopegiver/workspace/malgn-vscode-detail-redesign/scripts/capture/routes.mjs';

const BASE = process.env.BASE_URL || 'http://localhost:5199';
const detail = AUTH_ROUTES.find((r) => r.id === 'tasks-detail');
console.log('[verify] tasks-detail hash =', detail.hash);

const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();
page.on('pageerror', (e) => console.log('[pageerror]', e.message));

await page.addInitScript(installTauriStub, buildScenarioConfig('normal'));
await page.addInitScript(() => {
  window.__appMutations = [];
  const start = () => {
    const root = document.getElementById('app');
    if (!root) return setTimeout(start, 10);
    new MutationObserver(() => {
      window.__appMutations.push({
        t: Math.round(performance.now()),
        children: root.childElementCount,
        sidebars: root.querySelectorAll(':scope > .sidebar').length,
        mains: root.querySelectorAll(':scope > main').length,
        hash: location.hash,
      });
    }).observe(root, { childList: true });
  };
  start();
});

await page.goto(BASE, { waitUntil: 'load' });
await page.locator('.login-btn').click();
await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 10000 });
await page.waitForTimeout(600);

await page.evaluate(() => { window.__appMutations.length = 0; });
await page.evaluate((h) => { window.location.hash = h; }, detail.hash);
await page.waitForTimeout(1500);

const log = await page.evaluate(() => window.__appMutations);
console.log('\n[verify] tasks-detail 진입 후 #app 자식 변화 기록:');
for (const m of log) console.log(`  t=${m.t}ms children=${m.children} sidebar=${m.sidebars} main=${m.mains}`);
const worst = Math.max(...log.map((m) => m.children), 0);
console.log(`\n[verify] 관측된 최대 #app 자식 수 = ${worst} (정상=2, 재진입 중복 시=4)`);
console.log(`[verify] 최종 상태 =`, log[log.length - 1]);

await page.screenshot({ path: '/private/tmp/claude-501/-Users-hopegiver-workspace-malgn-vscode/3c66d427-3435-4d68-8c84-5c87cfee4d89/scratchpad/final-state.png', fullPage: true });
await browser.close();
