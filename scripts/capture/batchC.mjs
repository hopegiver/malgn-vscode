#!/usr/bin/env node
// Refactor 배치 C(설정·자율업무·로그인 본문 이관 + 상태줄 버전/업데이트 배지
// 수정) 검증용 캡처 스크립트 — batchB.mjs와 동일한 이유로 harness.mjs 대신
// 이 스크립트를 쓴다(stub.mjs/fixtures.mjs 재사용, normal/empty/error +
// 인터랙션 1장씩 + 업데이트 배지 3상태).
// 실행: node scripts/capture/batchC.mjs --round batchC-1440x900 --viewport 1440x900
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const out = { round: 'batchC', baseUrl: 'http://localhost:1420', viewport: '1440x900' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i];
  }
  return out;
}

const ROUTES = [
  { id: 'settings-otel', hash: '#/settings/otel', note: '설정 · OTel — .box 이관' },
  { id: 'settings-github', hash: '#/settings/github', note: '설정 · GitHub — .box 이관' },
  { id: 'settings-cloudflare', hash: '#/settings/cloudflare', note: '설정 · Cloudflare — .box 이관' },
  { id: 'settings-marketplace', hash: '#/settings/marketplace', note: '설정 · 마켓플레이스 — .box 이관' },
  { id: 'settings-mcp', hash: '#/settings/mcp', note: '설정 · MCP 관리 — .box 이관' },
  { id: 'settings-devtools', hash: '#/settings/devtools', note: '개발 도구 — 제목 중복 수정 확인' },
  { id: 'settings-applinks', hash: '#/settings/applinks', note: '앱 링크 — 제목 중복 수정 + 자체 page-header' },
  { id: 'tasks-list', hash: '#/tasks', note: '자율업무 목록 — .box 이관' },
  { id: 'tasks-board', hash: '#/tasks/board', note: '자율업무 진행상황판 — .box 이관' },
];

async function main() {
  const { round, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  const [width, height] = viewport.split('x').map(Number);
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const res = await fetch(baseUrl).catch(() => null);
  if (!res) throw new Error(`[batchC-capture] ${baseUrl} 에 연결할 수 없습니다 — 먼저 vite dev 서버를 띄우세요.`);

  const browser = await chromium.launch();

  async function newStubbedPage(kind, updaterPreset) {
    const context = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
    const page = await context.newPage();
    let pageErrors = 0;
    page.on('pageerror', (err) => {
      pageErrors++;
      console.error(`[batchC-capture] pageerror: ${err.stack ?? err.message}`);
    });
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        pageErrors++;
        console.error(`[batchC-capture] console.error: ${msg.text()}`);
      }
    });
    await page.addInitScript(installTauriStub, buildScenarioConfig(kind, updaterPreset));
    await page.goto(baseUrl, { waitUntil: 'load' });
    return { context, page, getErrors: () => pageErrors };
  }

  // ---- 1) 설정/자율업무 본문 이관 확인 (normal) ----
  {
    const { context, page, getErrors } = await newStubbedPage('normal');
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });

    for (const route of ROUTES) {
      await page.evaluate((h) => { window.location.hash = h; }, route.hash);
      await page.waitForTimeout(600);
      const file = join(outDir, `${round}__${route.id}.png`);
      await page.screenshot({ path: file, fullPage: true });
      console.log(`[batchC-capture] OK ${route.id} (${route.note}) -> ${file}`);
    }

    // ---- 인터랙션 1: MCP 관리 "+ 새 MCP 서버" 모달 ----
    await page.evaluate(() => { window.location.hash = '#/settings/mcp'; });
    await page.waitForTimeout(400);
    await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
    await page.waitForSelector('.modal-overlay');
    await page.waitForTimeout(200);
    await page.screenshot({ path: join(outDir, `${round}__mcp-add-modal.png`), fullPage: true });
    console.log('[batchC-capture] OK mcp-add-modal');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);

    // ---- 인터랙션 2: 자율업무 "+ 새 자율업무" 모달 ----
    await page.evaluate(() => { window.location.hash = '#/tasks'; });
    await page.waitForTimeout(400);
    await page.getByRole('button', { name: '+ 새 자율업무' }).click();
    await page.waitForSelector('.modal-overlay');
    await page.waitForTimeout(200);
    await page.screenshot({ path: join(outDir, `${round}__tasks-add-modal.png`), fullPage: true });
    console.log('[batchC-capture] OK tasks-add-modal');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);

    // ---- 인터랙션 3: 자율업무 상세 ----
    await page.evaluate(() => { window.location.hash = '#/tasks/item/daily-usage-report'; });
    await page.waitForTimeout(500);
    await page.screenshot({ path: join(outDir, `${round}__tasks-detail.png`), fullPage: true });
    console.log('[batchC-capture] OK tasks-detail');

    const n = getErrors();
    if (n > 0) console.error(`[batchC-capture] normal 컨텍스트 pageErrors=${n}`);
    await context.close();
  }

  // ---- 2) empty/error 상태 ----
  for (const kind of ['empty', 'error']) {
    const { context, page } = await newStubbedPage(kind);
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
    for (const id of ['#/settings/mcp', '#/settings/marketplace', '#/tasks']) {
      await page.evaluate((h) => { window.location.hash = h; }, id);
      await page.waitForTimeout(500);
      const slug = id.replace(/[#/]/g, '_');
      await page.screenshot({ path: join(outDir, `${round}__${kind}${slug}.png`), fullPage: true });
      console.log(`[batchC-capture] OK ${kind}${slug}`);
    }
    await context.close();
  }

  // ---- 3) 로그인 화면(정상/에러) — 셸 없이 단독 화면 ----
  {
    const { context, page } = await newStubbedPage('login-normal');
    await page.waitForSelector('.login-card');
    await page.screenshot({ path: join(outDir, `${round}__login-normal.png`), fullPage: true });
    console.log('[batchC-capture] OK login-normal');
    await context.close();
  }
  {
    const { context, page } = await newStubbedPage('login-error');
    await page.waitForSelector('.login-card');
    await page.click('.login-btn');
    await page.waitForSelector('.login-error', { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(200);
    await page.screenshot({ path: join(outDir, `${round}__login-error.png`), fullPage: true });
    console.log('[batchC-capture] OK login-error');
    await context.close();
  }

  // ---- 4) 상태줄 버전/업데이트 배지 3상태 ----
  // 수정 3 — 근본 원인은 stub.mjs가 'plugin:app|version'/'plugin:updater|check'를
  // 정의하지 않아 항상 reject되던 것(앱 코드는 정상, ensureAppVersionLoaded의
  // .catch가 조용히 흡수). 이제 PLUGIN_COMMAND_DEFAULTS로 기본 버전이 항상
  // 뜨고, updaterPreset으로 감지/확인중/설치중 3상태를 재현한다.
  {
    const { context, page } = await newStubbedPage('normal', 'available');
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
    await page.waitForSelector('.statusline-update', { timeout: 5000 });
    await page.waitForTimeout(200);
    await page.screenshot({ path: join(outDir, `${round}__statusline-update-available.png`), fullPage: true });
    console.log('[batchC-capture] OK statusline-update-available');
    await context.close();
  }
  {
    // 부팅 직후 initUpdateCheck()가 자동으로 1회 체크를 이미 시작해 두므로
    // (isChecking 재진입 가드), 그 자동 체크가 먼저 끝나기를 기다린 뒤(4000ms
    // 프리셋 지연 + 여유) "새 버전 확인" 버튼을 수동으로 눌러야 진짜 "확인
        // 중…" 라벨을 붙잡을 수 있다 — 안 그러면 재진입 가드 토스트만 찍힌다.
    const { context, page } = await newStubbedPage('normal', 'checking');
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
    await page.waitForSelector('.sidebar-version-check-btn', { timeout: 5000 });
    await page.waitForTimeout(4300);
    await page.click('.sidebar-version-check-btn');
    await page.waitForTimeout(300);
    await page.screenshot({ path: join(outDir, `${round}__statusline-update-checking.png`), fullPage: true });
    console.log('[batchC-capture] OK statusline-update-checking');
    await context.close();
  }
  {
    const { context, page } = await newStubbedPage('normal', 'installing');
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
    await page.waitForSelector('.statusline-update', { timeout: 5000 });
    await page.click('.statusline-update');
    await page.waitForTimeout(300);
    await page.screenshot({ path: join(outDir, `${round}__statusline-update-installing.png`), fullPage: true });
    console.log('[batchC-capture] OK statusline-update-installing');
    await context.close();
  }

  await browser.close();
  console.log('[batchC-capture] 완료');
}

main().catch((err) => {
  console.error('[batchC-capture] 실패:', err);
  process.exit(1);
});
