#!/usr/bin/env node
// UI 검증 하네스 — 실행: `node scripts/ui-harness/run.mjs [flowId] [scenarioId]`
//
// 전제: `pnpm tauri dev` 또는 `pnpm dev`로 http://localhost:1420 Vite 서버가
// 떠 있어야 한다(이 스크립트가 서버를 직접 띄우지 않는다).
//
// 검증 범위: TS/DOM/CSS 층만. window.__TAURI_INTERNALS__.invoke를 스텁으로
// 갈아끼워 실제 앱 번들을 Chromium에서 그대로 구동한다 — Rust 커맨드 자체의
// 동작(파일 읽기/쓰기, 프로세스 실행 등)은 이 하네스로 검증되지 않는다.
//
// 인자 없이 실행하면 모든 흐름 x 모든 시나리오를 순차 실행한다. 첫 인자로
// 흐름 id(autonomousTasks/sessions/catalog/settingsMcp/appLinks)를, 두 번째
// 인자로 시나리오 id(golden/empty/large/error/slow 등)를 주면 그 조합만 돈다.
import { chromium } from 'playwright';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { installListenerProbe, installTauriBridge, emitTauriEvent } from './lib/bridge.mjs';
import { buildBaseFixtures, notes } from './lib/fixtures.mjs';

import * as autonomousTasks from './flows/autonomousTasks.mjs';
import * as sessions from './flows/sessions.mjs';
import * as catalog from './flows/catalog.mjs';
import * as settingsMcp from './flows/settingsMcp.mjs';
import * as appLinks from './flows/appLinks.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const BASE_URL = process.env.UI_HARNESS_BASE_URL || 'http://localhost:1420';
const SHOTS_DIR =
  process.env.UI_HARNESS_SHOTS_DIR ||
  '/private/tmp/claude-501/-Users-hopegiver-workspace-malgn-vscode/9682b635-b50a-4778-8416-a05a9cb13e8a/scratchpad/shots';
const REPORT_PATH = path.join(__dirname, 'last-run-report.json');

const FLOWS = [autonomousTasks, sessions, catalog, settingsMcp, appLinks];

const [, , flowFilter, scenarioFilter] = process.argv;

function log(...args) {
  // 지시서 학습자료(feedback_long_running_task_checkins): 조용한 장시간 실행을
  // 피하고 단계마다 짧게 말한다.
  console.log(...args);
}

async function ensureDir(dir) {
  await mkdir(dir, { recursive: true });
}

async function runScenario(browser, flow, scenario, baseFixtures) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 820 } });
  const page = await context.newPage();

  const errors = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`CONSOLE: ${msg.text()}`);
  });
  page.on('pageerror', (err) => errors.push(`PAGEERROR: ${err.message}`));

  await page.addInitScript(installListenerProbe);
  await page.addInitScript(installTauriBridge, scenario.fixtures);

  const shotDir = path.join(SHOTS_DIR, flow.flowId);
  await ensureDir(shotDir);
  let shotSeq = 0;
  const shotPaths = [];
  const shot = async (name) => {
    shotSeq += 1;
    const file = path.join(shotDir, `${scenario.id}-${String(shotSeq).padStart(2, '0')}-${name}.png`);
    await page.screenshot({ path: file, fullPage: true });
    shotPaths.push(file);
  };
  const getListenerCount = async (type) => page.evaluate((t) => window.__listenerCounts?.[t] ?? 0, type);
  // 백엔드가 실제로 쏘는 session-chat-delta/-done 같은 스트리밍 이벤트를 시나리오가
  // 흉내 낼 때 쓴다(session_chat/turn.rs가 emit하는 것과 동일한 event 이름 + payload).
  const emitEvent = async (name, payload) => page.evaluate(emitTauriEvent, { name, payload });

  const bugs = [];
  const hash = scenario.startHash ?? flow.startHash;

  try {
    await page.goto(`${BASE_URL}/${hash}`, { waitUntil: 'networkidle', timeout: 15000 });
    // dev_auto_login → handleNavigation → 각 loadXxx()가 병렬로 도는 초기 렌더
    // 안정화 대기. 명시적 완료 신호가 없는 SPA라 고정 대기를 쓴다(느린 시나리오는
    // 각자 추가로 기다린다).
    await page.waitForTimeout(500);
  } catch (err) {
    bugs.push({ severity: 'Critical', symptom: `페이지 로드 실패: ${err.message}`, file: 'harness', repro: `${BASE_URL}/${hash} 접속` });
  }

  try {
    await scenario.run(page, { shot, bugs, errors, getListenerCount, emitEvent });
  } catch (err) {
    bugs.push({ severity: 'Critical', symptom: `시나리오 실행 중 예외: ${err.message}`, file: 'harness', stack: err.stack });
  }

  const unstubbed = await page.evaluate(() => window.__unstubbedCommands ?? []).catch(() => []);
  const seenUnstubbed = new Set();
  for (const u of unstubbed) {
    if (seenUnstubbed.has(u.cmd)) continue;
    seenUnstubbed.add(u.cmd);
    bugs.push({
      severity: 'Major',
      symptom: `하네스가 스텁하지 않은 커맨드가 호출됨: ${u.cmd} (args=${JSON.stringify(u.args)}) — 앱이 실제로 이 커맨드를 호출한다는 뜻이므로 fixtures에 추가 필요`,
      file: 'scripts/ui-harness/lib/fixtures.mjs',
    });
  }

  await context.close();

  return {
    flow: flow.flowId,
    scenario: scenario.id,
    hash,
    bugs,
    consoleErrors: errors,
    screenshots: shotPaths,
  };
}

async function main() {
  await ensureDir(SHOTS_DIR);
  log('[harness] 베이스 픽스처(실 로컬 데이터) 조립 중...');
  const base = await buildBaseFixtures();
  for (const n of notes) log(`[harness][real-data] ${n}`);

  const browser = await chromium.launch();
  const results = [];

  for (const flow of FLOWS) {
    if (flowFilter && flow.flowId !== flowFilter) continue;
    const scenarioList = flow.scenarios(base);
    for (const scenario of scenarioList) {
      if (scenarioFilter && scenario.id !== scenarioFilter) continue;
      log(`[harness] ▶ ${flow.flowId} / ${scenario.id}`);
      const result = await runScenario(browser, flow, scenario, base);
      const bugCount = result.bugs.length;
      const errCount = result.consoleErrors.length;
      log(`[harness]   완료 — bugs=${bugCount} consoleErrors=${errCount} shots=${result.screenshots.length}`);
      results.push(result);
    }
  }

  await browser.close();

  const report = {
    generatedAt: new Date().toISOString(),
    baseUrl: BASE_URL,
    shotsDir: SHOTS_DIR,
    realDataNotes: notes,
    results,
  };
  await writeFile(REPORT_PATH, JSON.stringify(report, null, 2), 'utf-8');
  log(`[harness] 리포트 저장: ${REPORT_PATH}`);

  const totalBugs = results.reduce((n, r) => n + r.bugs.length, 0);
  const totalErrors = results.reduce((n, r) => n + r.consoleErrors.length, 0);
  log(`[harness] 총 ${results.length}개 시나리오 실행 — bugs=${totalBugs} consoleErrors=${totalErrors}`);
}

main().catch((err) => {
  console.error('[harness] 치명적 오류:', err);
  process.exit(1);
});
