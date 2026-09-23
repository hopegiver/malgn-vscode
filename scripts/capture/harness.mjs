#!/usr/bin/env node
// 재사용 캡처 하네스 — 앱의 모든 라우트 x 시나리오(정상/빈 상태/에러/로딩)
// 조합을 PNG로 남기고 매니페스트를 생성한다. 사용법은 scripts/capture/README.md.
//
// 실행: node scripts/capture/harness.mjs [--round r1] [--label before] [--base-url http://localhost:1420]
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';
import { AUTH_ROUTES } from './routes.mjs';
import { LOGIN_ERROR_MESSAGE } from './fixtures.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, '..', '..');
let VIEWPORT = { width: 1440, height: 900 };

function parseArgs(argv) {
  const out = { round: 'r1', label: null, baseUrl: 'http://localhost:1420', viewport: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--label') out.label = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i]; // "WxH", 예: 900x600(창 최소 크기 검증용)
  }
  return out;
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function ensureDevServer(baseUrl) {
  try {
    const res = await fetch(baseUrl);
    if (res.status < 500) {
      console.log(`[capture] 기존 dev 서버 재사용: ${baseUrl}`);
      return null;
    }
  } catch {
    /* 아래에서 새로 띄운다 */
  }
  console.log(`[capture] ${baseUrl} 응답 없음 — vite dev 서버를 새로 띄웁니다…`);
  const { spawn } = await import('node:child_process');
  const port = new URL(baseUrl).port || '1420';
  const proc = spawn('pnpm', ['exec', 'vite', '--port', port, '--strictPort'], {
    cwd: PROJECT_ROOT,
    stdio: 'ignore',
    detached: true,
  });
  proc.unref();
  for (let i = 0; i < 30; i++) {
    await sleep(500);
    try {
      const res = await fetch(baseUrl);
      if (res.status < 500) {
        console.log('[capture] dev 서버 기동 확인');
        return proc;
      }
    } catch {
      /* 계속 폴링 */
    }
  }
  throw new Error(`[capture] ${baseUrl} 에서 vite dev 서버를 기동하지 못했습니다 (15초 초과).`);
}

async function loginViaUI(page, timeoutMs = 8000) {
  await page.click('.login-btn');
  await page.locator('.sidebar').waitFor({ state: 'visible', timeout: timeoutMs });
}

function fileFor(outDir, round, routeId, scenarioLabel) {
  return join(outDir, `${round}__${routeId}__${scenarioLabel}.png`);
}

async function main() {
  const { round, label, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  if (viewport) {
    const [w, h] = viewport.split('x').map(Number);
    if (Number.isFinite(w) && Number.isFinite(h)) VIEWPORT = { width: w, height: h };
  }
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const startedServer = await ensureDevServer(baseUrl);

  const browser = await chromium.launch();
  const manifest = [];
  const errorLog = [];
  let successCount = 0;
  let failureCount = 0;

  function record(entry) {
    manifest.push(entry);
    if (entry.ok) successCount++;
    else failureCount++;
    const mark = entry.ok ? 'OK  ' : 'FAIL';
    const suffix = entry.ok ? (entry.quirk ? `(quirk: ${entry.note})` : '') : entry.note ?? '';
    console.log(`[capture] ${mark} ${entry.scenario.padEnd(8)} ${entry.routeId.padEnd(20)} ${suffix}`);
  }

  async function captureOne(page, route, scenarioLabel, waitMs, currentTagSetter) {
    currentTagSetter(`${scenarioLabel}/${route.id}`);
    if (route.hash) {
      await page.evaluate((h) => {
        window.location.hash = h;
      }, route.hash);
    }
    await page.waitForTimeout(waitMs);
    const result = await route.verify(page, scenarioLabel);
    const file = fileFor(outDir, round, route.id, scenarioLabel);
    await page.screenshot({ path: file, fullPage: true });
    record({
      routeId: route.id,
      label: route.label,
      scenario: scenarioLabel,
      file,
      ok: result.ok,
      note: result.note ?? null,
      quirk: !!result.quirk,
    });
  }

  // ---------------- 1) 로그인 화면 (normal / loading / error) ----------------
  {
    const context = await browser.newContext({ viewport: VIEWPORT, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = 'login/normal';
    page.on('pageerror', (err) => errorLog.push({ tag, message: err.message }));
    await page.addInitScript(installTauriStub, buildScenarioConfig('login-normal'));
    await page.goto(baseUrl, { waitUntil: 'load' });
    await page.locator('.login-card').waitFor({ state: 'visible', timeout: 8000 });
    await page.waitForTimeout(200);
    const okNormal = (await page.locator('.login-card').innerText()).includes('Google 계정으로 로그인');
    const fileNormal = fileFor(outDir, round, 'login', 'normal');
    await page.screenshot({ path: fileNormal, fullPage: true });
    record({ routeId: 'login', label: '로그인', scenario: 'normal', file: fileNormal, ok: okNormal, note: okNormal ? null : '로그인 버튼 텍스트 미검출' });
    await context.close();
  }
  {
    const context = await browser.newContext({ viewport: VIEWPORT, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = 'login/loading';
    page.on('pageerror', (err) => errorLog.push({ tag, message: err.message }));
    await page.addInitScript(installTauriStub, buildScenarioConfig('login-loading'));
    await page.goto(baseUrl, { waitUntil: 'load' });
    await page.click('.login-btn');
    await page.waitForTimeout(400);
    const text = await page.locator('.login-card').innerText();
    const okLoading = text.includes('브라우저에서 로그인 대기 중');
    const fileLoading = fileFor(outDir, round, 'login', 'loading');
    await page.screenshot({ path: fileLoading, fullPage: true });
    record({ routeId: 'login', label: '로그인', scenario: 'loading', file: fileLoading, ok: okLoading, note: okLoading ? null : '로그인 대기 문구 미검출' });
    await context.close();
  }
  {
    const context = await browser.newContext({ viewport: VIEWPORT, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = 'login/error';
    page.on('pageerror', (err) => errorLog.push({ tag, message: err.message }));
    await page.addInitScript(installTauriStub, buildScenarioConfig('login-error'));
    await page.goto(baseUrl, { waitUntil: 'load' });
    await page.click('.login-btn');
    await page.locator('.login-error').waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
    const text = await page.locator('.login-card').innerText();
    const okError = text.includes(LOGIN_ERROR_MESSAGE) || text.includes('malgnsoft.com 조직 계정');
    const fileError = fileFor(outDir, round, 'login', 'error');
    await page.screenshot({ path: fileError, fullPage: true });
    record({ routeId: 'login', label: '로그인', scenario: 'error', file: fileError, ok: okError, note: okError ? null : '로그인 에러 문구 미검출' });
    await context.close();
  }

  // ---------------- 2) 인증 후 라우트 18개 x (normal / empty / error) ----------------
  for (const scenarioLabel of ['normal', 'empty', 'error']) {
    const context = await browser.newContext({ viewport: VIEWPORT, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = `${scenarioLabel}/(login)`;
    page.on('pageerror', (err) => errorLog.push({ tag, message: err.message }));
    await page.addInitScript(installTauriStub, buildScenarioConfig(scenarioLabel));
    await page.goto(baseUrl, { waitUntil: 'load' });
    await loginViaUI(page);

    for (const route of AUTH_ROUTES) {
      await captureOne(page, route, scenarioLabel, 500, (v) => (tag = v));
    }
    await context.close();
  }

  // ---------------- 3) 인증 후 라우트 18개 x loading ----------------
  {
    const context = await browser.newContext({ viewport: VIEWPORT, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = 'loading/(login)';
    page.on('pageerror', (err) => errorLog.push({ tag, message: err.message }));
    await page.addInitScript(installTauriStub, buildScenarioConfig('app-loading'));
    await page.goto(baseUrl, { waitUntil: 'load' });
    await loginViaUI(page);

    for (const route of AUTH_ROUTES) {
      await captureOne(page, route, 'loading', 350, (v) => (tag = v));
    }
    await context.close();
  }

  await browser.close();
  if (startedServer) {
    try {
      process.kill(-startedServer.pid);
    } catch {
      /* 이미 종료됐으면 무시 */
    }
  }

  const manifestPath = join(outDir, 'manifest.json');
  writeFileSync(
    manifestPath,
    JSON.stringify(
      {
        round,
        label,
        baseUrl,
        capturedAt: new Date().toISOString(),
        total: manifest.length,
        success: successCount,
        failure: failureCount,
        pageErrors: errorLog,
        entries: manifest,
      },
      null,
      2
    )
  );

  console.log('----------------------------------------');
  console.log(`[capture] 캡처 ${successCount}장 성공 / ${failureCount}장 실패 (총 ${manifest.length}장)`);
  console.log(`[capture] pageerror 총 ${errorLog.length}건`);
  console.log(`[capture] 출력 디렉터리: ${outDir}`);
  console.log(`[capture] 매니페스트: ${manifestPath}`);

  if (failureCount > 0 || errorLog.length > 0) {
    process.exitCode = 1;
  }
}

main().catch((err) => {
  console.error('[capture] 치명적 오류:', err);
  process.exitCode = 1;
});
