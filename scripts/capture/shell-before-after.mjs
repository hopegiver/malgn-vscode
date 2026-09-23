#!/usr/bin/env node
// Terminus 셸 리팩터 before/after 대조 전용 임시 캡처 스크립트. 전체 18라우트
// x 4시나리오 행렬(harness.mjs)은 이번 대조에 필요한 범위를 넘어서고(로그인
// loading/error 시나리오의 기존 타임아웃 flake까지 함께 타 시간이 오래 걸림),
// 이번 라운드가 요구하는 것은 홈/프로젝트/세션 3화면의 normal 시나리오
// 시각 대조뿐이다 — stub.mjs/fixtures.mjs/routes.mjs를 그대로 재사용해 그
// 범위만 빠르게 캡처한다.
// 실행: node scripts/capture/shell-before-after.mjs --round before-1440x900 --viewport 1440x900 [--base-url http://localhost:1420]
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';
import { IDS } from './fixtures.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const enc = encodeURIComponent;

function parseArgs(argv) {
  const out = { round: 'shell-r1', baseUrl: 'http://localhost:1420', viewport: '1440x900' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i];
  }
  return out;
}

const ROUTES = [
  { id: 'home', hash: '#/' },
  { id: 'projects-list', hash: '#/projects' },
  { id: 'projects-detail', hash: `#/project/${enc(IDS.PROJECT_PATH)}` },
  { id: 'sessions-list', hash: '#/sessions' },
];

async function main() {
  const { round, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  const [width, height] = viewport.split('x').map(Number);
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const res = await fetch(baseUrl).catch(() => null);
  if (!res) throw new Error(`[shell-capture] ${baseUrl} 에 연결할 수 없습니다 — 먼저 vite dev 서버를 띄우세요.`);

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
  const page = await context.newPage();
  page.on('pageerror', (err) => console.error(`[shell-capture] pageerror: ${err.stack ?? err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') console.error(`[shell-capture] console.error: ${msg.text()}`);
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
    console.log(`[shell-capture] OK ${route.id} -> ${file}`);
  }

  await context.close();
  await browser.close();
}

main().catch((err) => {
  console.error('[shell-capture] 실패:', err);
  process.exit(1);
});
