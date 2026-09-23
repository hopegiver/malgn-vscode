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
// 흐름 id(autonomousTasks/sessions/catalog/settingsMcp/appLinks/
// formBackgroundRerender/majorReview20260921/updateCheck/home/
// tabstripOverflow/qaShellNavCoverage/accountMenuAndStatusline)를, 두 번째
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
import * as formBackgroundRerender from './flows/formBackgroundRerender.mjs';
import * as majorReview20260921 from './flows/majorReview20260921.mjs';
import * as updateCheck from './flows/updateCheck.mjs';
import * as home from './flows/home.mjs';
import * as tabstripOverflow from './flows/tabstripOverflow.mjs';
import * as qaShellNavCoverage from './flows/qaShellNavCoverage.mjs';
import * as accountMenuAndStatusline from './flows/accountMenuAndStatusline.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const BASE_URL = process.env.UI_HARNESS_BASE_URL || 'http://localhost:1420';
const SHOTS_DIR =
  process.env.UI_HARNESS_SHOTS_DIR ||
  '/private/tmp/claude-501/-Users-hopegiver-workspace-malgn-vscode/9682b635-b50a-4778-8416-a05a9cb13e8a/scratchpad/shots';
const REPORT_PATH = path.join(__dirname, 'last-run-report.json');

const FLOWS = [autonomousTasks, sessions, catalog, settingsMcp, appLinks, formBackgroundRerender, majorReview20260921, updateCheck, home, tabstripOverflow, qaShellNavCoverage, accountMenuAndStatusline];

const [, , flowFilter, scenarioFilter] = process.argv;

function log(...args) {
  // 지시서 학습자료(feedback_long_running_task_checkins): 조용한 장시간 실행을
  // 피하고 단계마다 짧게 말한다.
  console.log(...args);
}

async function ensureDir(dir) {
  await mkdir(dir, { recursive: true });
}

// 개발서버(Vite) 미기동을 "제품 버그"로 오집계하지 않기 위한 사전 점검.
// bugs=113 consoleErrors=3 처럼 나왔던 실제 사고 원인이 서버 다운(net::ERR_CONNECTION_REFUSED)이었음.
const DEV_SERVER_HINT = `개발서버(${BASE_URL})에 연결할 수 없습니다. \`pnpm dev\`로 띄운 뒤 다시 실행하세요.`;

async function isDevServerReachable() {
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 3000);
    try {
      await fetch(BASE_URL, { signal: controller.signal });
      return true; // 상태코드 무관 — 응답이 왔다는 것 자체가 서버 기동 신호.
    } finally {
      clearTimeout(timer);
    }
  } catch {
    return false;
  }
}

// 사전 점검은 "시작 시점" 미기동만 잡는다. 시나리오 도중 서버가 죽는 경우(별도
// 확인 사항)는 page.goto가 던지는 net::ERR_CONNECTION_REFUSED를 여기서 식별해
// "제품 버그"가 아니라 "환경 실패"로 구분하고, 나머지 시나리오를 계속 돌려봐야
// 전부 같은 이유로 오염되므로 전체 실행을 즉시 중단한다(부분 분류까지는 하지
// 않음 — 과설계 방지, 아래 반환 텍스트에 한계 명시).
class EnvironmentFailureError extends Error {}
function isConnectionRefusedError(err) {
  return /ERR_CONNECTION_REFUSED/.test(String(err?.message ?? err));
}

async function runScenario(browser, flow, scenario, baseFixtures) {
  // 시나리오가 반응형 검증을 위해 뷰포트를 지정할 수 있게 한다(예: 900×600
  // 하한에서의 탭스트립 오버플로 회귀 검사). 미지정 시 기존 기본값 유지.
  const context = await browser.newContext({
    viewport: scenario.viewport ?? { width: 1280, height: 820 },
    // 경계 상황(prefers-reduced-motion) 검증용 — 시나리오가 지정하지 않으면
    // Playwright 기본값('no-preference')을 그대로 쓴다.
    ...(scenario.reducedMotion ? { reducedMotion: scenario.reducedMotion } : {}),
  });
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
    if (isConnectionRefusedError(err)) {
      // 시작 시점엔 떠 있다가 시나리오 도중 개발서버가 죽은 경우. "제품 버그"로
      // 잘못 집계하지 않도록 별도 에러 타입으로 올려 전체 실행을 중단시킨다.
      await context.close();
      throw new EnvironmentFailureError(
        `${flow.flowId}/${scenario.id} 진행 중 개발서버 연결이 끊겼습니다(${err.message}). ${DEV_SERVER_HINT}`,
      );
    }
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
  // 시나리오 루프 진입 전 필수 전제조건 확인: 개발서버 미기동을 "bugs"로
  // 오집계했던 실제 사고(bugs=113) 재발 방지. 여기서 실패하면 시나리오를 하나도
  // 돌리지 않고 즉시 비정상 종료한다 — last-run-report.json은 건드리지 않는다
  // (직전의 유효한 결과를 지우지 않기 위해 writeFile 이전에 종료).
  log(`[harness] 개발서버(${BASE_URL}) 도달 가능 여부 확인 중...`);
  if (!(await isDevServerReachable())) {
    log(`[harness] 중단: ${DEV_SERVER_HINT}`);
    process.exitCode = 1;
    return;
  }
  log('[harness] 개발서버 응답 확인됨.');

  await ensureDir(SHOTS_DIR);
  log('[harness] 베이스 픽스처(실 로컬 데이터) 조립 중...');
  const base = await buildBaseFixtures();
  for (const n of notes) log(`[harness][real-data] ${n}`);

  const browser = await chromium.launch();
  const results = [];

  try {
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
  } catch (err) {
    await browser.close();
    if (err instanceof EnvironmentFailureError) {
      // 시나리오 도중 서버가 죽은 경우 — 이미 실행한 결과가 있어도 리포트를
      // 쓰지 않는다(부분 결과가 "정상 실행" 리포트로 오독될 수 있음).
      log(`[harness] 중단(환경 실패): ${err.message}`);
      process.exitCode = 1;
      return;
    }
    throw err;
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
