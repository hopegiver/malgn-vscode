#!/usr/bin/env node
// Refactor 배치 B(사용량 사이드바 단순화 + 4개 화면 본문 이관) 검증용 캡처
// 스크립트 — batchA.mjs와 동일한 이유로 harness.mjs 대신 이 스크립트를 쓴다
// (stub.mjs/fixtures.mjs 재사용, normal/empty/error + 인터랙션 1장씩).
// 실행: node scripts/capture/batchB.mjs --round batchB-1440x900 --viewport 1440x900
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const out = { round: 'batchB', baseUrl: 'http://localhost:1420', viewport: '1440x900' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i];
  }
  return out;
}

const ROUTES = [
  { id: 'usage', hash: '#/usage', note: '탭4 사용량 — 사이드바 단일 항목 + 본문 .box/.stat 이관' },
  { id: 'devtools', hash: '#/settings/devtools', note: '탭5 개발 도구 — 본문 전체 .box 이관' },
  { id: 'catalog-plugins', hash: '#/catalog/plugins', note: '탭7 카탈로그(플러그인) — .plugin-card ┌─ 헤더' },
  { id: 'catalog-global', hash: '#/catalog/global', note: '탭7 카탈로그(전역) — .box 이관' },
  { id: 'applinks', hash: '#/settings/applinks', note: '탭8 앱 링크 — 본문 .box 이관' },
];

async function main() {
  const { round, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  const [width, height] = viewport.split('x').map(Number);
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const res = await fetch(baseUrl).catch(() => null);
  if (!res) throw new Error(`[batchB-capture] ${baseUrl} 에 연결할 수 없습니다 — 먼저 vite dev 서버를 띄우세요.`);

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
  const page = await context.newPage();
  let pageErrors = 0;
  page.on('pageerror', (err) => {
    pageErrors++;
    console.error(`[batchB-capture] pageerror: ${err.stack ?? err.message}`);
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      pageErrors++;
      console.error(`[batchB-capture] console.error: ${msg.text()}`);
    }
  });

  await page.addInitScript(installTauriStub, buildScenarioConfig('normal'));
  await page.goto(baseUrl, { waitUntil: 'load' });
  await page.click('.login-btn');
  await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });

  for (const route of ROUTES) {
    await page.evaluate((h) => { window.location.hash = h; }, route.hash);
    await page.waitForTimeout(600);
    const file = join(outDir, `${round}__${route.id}.png`);
    await page.screenshot({ path: file, fullPage: true });
    console.log(`[batchB-capture] OK ${route.id} (${route.note}) -> ${file}`);
  }

  // ---- 인터랙션 1: 앱 링크 "+ 링크 추가" 모달 ----
  await page.evaluate(() => { window.location.hash = '#/settings/applinks'; });
  await page.waitForTimeout(400);
  await page.getByRole('button', { name: '+ 링크 추가' }).click();
  await page.waitForSelector('.modal-overlay');
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(outDir, `${round}__applinks-add-modal.png`), fullPage: true });
  console.log('[batchB-capture] OK applinks-add-modal');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);

  // ---- 인터랙션 2: 개발 도구 — 첫 "run" 도구의 미리보기 패널 펼침 ----
  await page.evaluate(() => { window.location.hash = '#/settings/devtools'; });
  await page.waitForTimeout(400);
  const previewBtn = page.locator('.devtool-row button:not([disabled])').first();
  if ((await previewBtn.count()) > 0) {
    await previewBtn.click();
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(outDir, `${round}__devtools-preview-panel.png`), fullPage: true });
    console.log('[batchB-capture] OK devtools-preview-panel');
  } else {
    console.log('[batchB-capture] SKIP devtools-preview-panel (no enabled action button in fixture)');
  }

  await context.close();
  await browser.close();
  console.log(`[batchB-capture] 완료 — pageErrors=${pageErrors}`);
  if (pageErrors > 0) process.exitCode = 1;
}

main().catch((err) => {
  console.error('[batchB-capture] 실패:', err);
  process.exit(1);
});
