#!/usr/bin/env node
// Refactor 3단계(Terminus 셸 전면 개편) 완료 후 10개 화면 전체를 한 번에 찍는
// 최종 캡처 — QA·리뷰어 풀패널의 공통 근거가 된다. 로그인 화면(셸 없음) +
// 9개 탭(홈/프로젝트/세션/사용량/개발도구/자율업무/카탈로그/앱링크/설정)을
// normal 시나리오로 순서대로 찍는다.
// 실행: node scripts/capture/final.mjs --round final-1440x900 --viewport 1440x900
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const out = { round: 'final', baseUrl: 'http://localhost:1420', viewport: '1440x900' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i];
  }
  return out;
}

// 10개 화면 — terminus-shell-ia.md §1 탭스트립 9종 + 로그인(셸 없음).
const ROUTES = [
  { id: '01-home', hash: '#/' },
  { id: '02-projects', hash: '#/projects' },
  { id: '03-sessions', hash: '#/sessions' },
  { id: '04-usage', hash: '#/usage' },
  { id: '05-devtools', hash: '#/settings/devtools' },
  { id: '06-tasks', hash: '#/tasks' },
  { id: '07-catalog', hash: '#/catalog/plugins' },
  { id: '08-applinks', hash: '#/settings/applinks' },
  { id: '09-settings', hash: '#/settings/otel' },
];

async function main() {
  const { round, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  const [width, height] = viewport.split('x').map(Number);
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const res = await fetch(baseUrl).catch(() => null);
  if (!res) throw new Error(`[final-capture] ${baseUrl} 에 연결할 수 없습니다 — 먼저 vite dev 서버를 띄우세요.`);

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
  const page = await context.newPage();
  let pageErrors = 0;
  page.on('pageerror', (err) => {
    pageErrors++;
    console.error(`[final-capture] pageerror: ${err.stack ?? err.message}`);
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      pageErrors++;
      console.error(`[final-capture] console.error: ${msg.text()}`);
    }
  });

  // 10번째 화면: 로그인(셸 없음, 인증 전 상태 그대로).
  await page.addInitScript(installTauriStub, buildScenarioConfig('login-normal'));
  await page.goto(baseUrl, { waitUntil: 'load' });
  await page.waitForSelector('.login-card');
  await page.screenshot({ path: join(outDir, `${round}__10-login.png`), fullPage: true });
  console.log('[final-capture] OK 10-login');

  // 로그인 이후 9개 탭 — normal 시나리오로 재진입(새 컨텍스트로 스텁 갱신).
  await context.close();
  const context2 = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
  const page2 = await context2.newPage();
  page2.on('pageerror', (err) => {
    pageErrors++;
    console.error(`[final-capture] pageerror: ${err.stack ?? err.message}`);
  });
  page2.on('console', (msg) => {
    if (msg.type() === 'error') {
      pageErrors++;
      console.error(`[final-capture] console.error: ${msg.text()}`);
    }
  });
  await page2.addInitScript(installTauriStub, buildScenarioConfig('normal'));
  await page2.goto(baseUrl, { waitUntil: 'load' });
  await page2.click('.login-btn');
  await page2.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });

  for (const route of ROUTES) {
    await page2.evaluate((h) => { window.location.hash = h; }, route.hash);
    await page2.waitForTimeout(600);
    const file = join(outDir, `${round}__${route.id}.png`);
    await page2.screenshot({ path: file, fullPage: true });
    console.log(`[final-capture] OK ${route.id} -> ${file}`);
  }

  await context2.close();
  await browser.close();
  console.log(`[final-capture] 완료 — pageErrors=${pageErrors}`);
  if (pageErrors > 0) process.exitCode = 1;
}

main().catch((err) => {
  console.error('[final-capture] 실패:', err);
  process.exit(1);
});
