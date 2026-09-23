#!/usr/bin/env node
// Refactor 배치 A(탭별 맥락 사이드바) 검증용 캡처 스크립트 — shell-before-after.mjs와
// 동일한 이유로 harness.mjs 대신 이 스크립트를 쓴다: harness.mjs의 로그인
// loading/error 시나리오에는 알려진 타임아웃 flake가 있고(shell-before-after.mjs
// 상단 주석 참고), 이번 검증 범위는 "9개 탭 사이드바 + 프로젝트/세션 목록·상세·
// 채팅"의 normal 시나리오 시각 확인이라 그 범위만 stub.mjs/fixtures.mjs를 그대로
// 재사용해 빠르게 캡처한다.
// 실행: node scripts/capture/batchA.mjs --round batchA-1440 --viewport 1440x900
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';
import { IDS } from './fixtures.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const enc = encodeURIComponent;

function parseArgs(argv) {
  const out = { round: 'batchA', baseUrl: 'http://localhost:1420', viewport: '1440x900' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--round') out.round = argv[++i];
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--viewport') out.viewport = argv[++i];
  }
  return out;
}

// 9개 탭 전부(사이드바가 보이는 대표 하위탭 1개씩) + 프로젝트/세션의 목록·
// 상세·채팅(draft 포함) — terminus-shell-ia.md §4-1~§4-9 대응.
const ROUTES = [
  { id: 'home', hash: '#/', note: '탭1 홈 — 워크스페이스 사이드바' },
  { id: 'projects-list', hash: '#/projects', note: '탭2 프로젝트 목록 — 워크스페이스 사이드바' },
  { id: 'projects-detail', hash: `#/project/${enc(IDS.PROJECT_PATH)}`, note: '탭2 프로젝트 상세 — 사이드바 현재 항목 강조' },
  { id: 'sessions-list', hash: '#/sessions', note: '탭3 세션 목록 — 세션 사이드바' },
  { id: 'sessions-detail', hash: `#/sessions/${enc(IDS.SESSION_ID)}`, note: '탭3 세션 상세(채팅) — 사이드바 현재 세션 강조' },
  { id: 'sessions-draft', hash: `#/sessions/new/${enc(IDS.PROJECT_PATH)}`, note: '탭3 새 세션 draft(채팅 입력)' },
  { id: 'usage', hash: '#/usage', note: '탭4 사용량 — 사이드바 단일 항목(배치 B로 기간 토글/최근 활동일 폐기, scripts/capture/batchB.mjs 참고)' },
  { id: 'settings-devtools', hash: '#/settings/devtools', note: '탭5 개발 도구 — 필수/선택 그룹 사이드바' },
  { id: 'tasks-list', hash: '#/tasks', note: '탭6 자율 작업 — 뷰 전환+작업 큐 사이드바' },
  { id: 'catalog', hash: '#/catalog', note: '탭7 카탈로그 — nav 사이드바' },
  { id: 'settings-applinks', hash: '#/settings/applinks', note: '탭8 앱 링크 — 퀵오픈 사이드바' },
  { id: 'settings-otel', hash: '#/settings/otel', note: '탭9 설정 — nav 5종 사이드바' },
];

async function main() {
  const { round, baseUrl, viewport } = parseArgs(process.argv.slice(2));
  const [width, height] = viewport.split('x').map(Number);
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const res = await fetch(baseUrl).catch(() => null);
  if (!res) throw new Error(`[batchA-capture] ${baseUrl} 에 연결할 수 없습니다 — 먼저 vite dev 서버를 띄우세요.`);

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width, height }, baseURL: baseUrl });
  const page = await context.newPage();
  let pageErrors = 0;
  page.on('pageerror', (err) => {
    pageErrors++;
    console.error(`[batchA-capture] pageerror: ${err.stack ?? err.message}`);
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      pageErrors++;
      console.error(`[batchA-capture] console.error: ${msg.text()}`);
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
    console.log(`[batchA-capture] OK ${route.id} (${route.note}) -> ${file}`);
  }

  await context.close();
  await browser.close();
  console.log(`[batchA-capture] 완료 — pageErrors=${pageErrors}`);
  if (pageErrors > 0) process.exitCode = 1;
}

main().catch((err) => {
  console.error('[batchA-capture] 실패:', err);
  process.exit(1);
});
