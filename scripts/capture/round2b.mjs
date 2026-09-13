#!/usr/bin/env node
// 라운드2 보충 캡처 — round2.mjs의 1차 관측에서 "코드상 도달 가능해 보이는데
// 기본 픽스처로는 안 찍힌" 분기만 골라 픽스처를 바꿔가며 재현한다.
// 실행: node scripts/capture/round2b.mjs
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROUND = 'r2b';
const BASE = process.env.CAPTURE_BASE_URL ?? 'http://localhost:5173';

function cfgWith(overrides) {
  const cfg = buildScenarioConfig('normal');
  for (const [cmd, entry] of Object.entries(overrides)) cfg.commands[cmd] = { ...entry, delayMs: entry.delayMs ?? 120 };
  return cfg;
}

const SPECS = [
  {
    // renderPreviewPanel의 warnMultiple = affected.length !== 1 → affected가 0건이면
    // "외 0개 항목이 함께 바뀔 수 있습니다"라는 문구가 실제로 렌더되는지 확인한다.
    id: 'devtools-preview-affected0',
    config: cfgWith({
      preview_dev_tool_update: {
        value: { id: 'claude', planId: 'plan-empty', willRun: true, commandDisplay: 'brew upgrade claude', affected: [], notes: '', previewReliable: true },
      },
    }),
    hash: '#/dev-tools',
    act: async (page) => {
      await page.locator('.devtool-row .btn').first().click();
      await page.waitForTimeout(600);
    },
  },
  {
    // affected가 1건이면 경고가 아예 안 뜨는지(대조군)
    id: 'devtools-preview-affected1',
    config: cfgWith({
      preview_dev_tool_update: {
        value: { id: 'claude', planId: 'plan-one', willRun: true, commandDisplay: 'brew upgrade claude', affected: ['/opt/homebrew/bin/claude'], notes: '', previewReliable: true },
      },
    }),
    hash: '#/dev-tools',
    act: async (page) => {
      await page.locator('.devtool-row .btn').first().click();
      await page.waitForTimeout(600);
    },
  },
  {
    // previewReliable=false + willRun=false 동시 → 경고 블록이 겹쳐 쌓이는 모습
    id: 'devtools-preview-unreliable',
    config: cfgWith({
      preview_dev_tool_update: {
        value: { id: 'claude', planId: 'plan-x', willRun: false, commandDisplay: 'brew upgrade claude', affected: ['/opt/homebrew/bin/claude', '/opt/homebrew/Cellar/claude'], notes: '', previewReliable: false },
      },
    }),
    hash: '#/dev-tools',
    act: async (page) => {
      await page.locator('.devtool-row .btn').first().click();
      await page.waitForTimeout(600);
    },
  },
  {
    // GitHub 퀵스타트 행의 "설치" → 프리필된 추가 폼
    id: 'mcp-github-prefill',
    config: cfgWith({}),
    hash: '#/settings/mcp',
    act: async (page) => {
      const rows = page.locator('.mcp-row');
      const n = await rows.count();
      for (let i = 0; i < n; i++) {
        const t = await rows.nth(i).innerText();
        if (t.includes('api.githubcopilot.com/mcp/')) {
          await rows.nth(i).locator('button').first().click();
          break;
        }
      }
      await page.waitForTimeout(500);
    },
  },
  {
    // "전체 업데이트" — 개별 확인 없이 일괄 실행되는 경로의 화면
    id: 'devtools-update-all',
    config: cfgWith({}),
    hash: '#/dev-tools',
    act: async (page) => {
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForTimeout(400);
    },
  },
  {
    // 실행 완료 후 결과 패널
    id: 'devtools-run-result',
    config: cfgWith({}),
    hash: '#/dev-tools',
    act: async (page) => {
      await page.locator('.devtool-row .btn').first().click();
      await page.waitForTimeout(600);
      await page.getByRole('button', { name: '실행', exact: true }).first().click();
      await page.waitForTimeout(1200);
    },
  },
  {
    // 자율업무 목록에서 토글 off → 토스트 + 행 상태
    id: 'task-toggle-off',
    config: cfgWith({}),
    hash: '#/tasks',
    act: async (page) => {
      await page.locator('.toggle-track').first().click();
      await page.waitForTimeout(700);
    },
  },
];

async function main() {
  const outDir = join(__dirname, 'output', ROUND);
  mkdirSync(outDir, { recursive: true });
  const browser = await chromium.launch();
  const manifest = [];
  const errorLog = [];

  for (const spec of SPECS) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, baseURL: BASE });
    const page = await context.newPage();
    page.on('pageerror', (e) => errorLog.push({ tag: spec.id, message: e.message }));
    page.on('dialog', (d) => void d.accept());
    await page.addInitScript(installTauriStub, spec.config);
    await page.goto(BASE, { waitUntil: 'load' });
    await page.click('.login-btn');
    await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
    await page.evaluate((h) => {
      window.location.hash = h;
    }, spec.hash);
    await page.waitForTimeout(700);
    let err = null;
    try {
      await spec.act(page);
    } catch (e) {
      err = e.message;
    }
    const body = await page.locator('body').innerText();
    const file = join(outDir, `${ROUND}__${spec.id}.png`);
    await page.screenshot({ path: file, fullPage: true });
    const warnLine = body.split('\n').find((l) => l.includes('외 ') && l.includes('항목')) ?? null;
    manifest.push({ id: spec.id, file, ok: !err, err, warnLine });
    console.log(`[r2b] ${err ? 'FAIL' : 'OK  '} ${spec.id.padEnd(30)} ${err ?? warnLine ?? ''}`);
    await context.close();
  }

  await browser.close();
  writeFileSync(join(outDir, 'manifest.json'), JSON.stringify({ round: ROUND, capturedAt: new Date().toISOString(), pageErrors: errorLog, entries: manifest }, null, 2));
  console.log(`[r2b] pageerror ${errorLog.length}건 · ${outDir}`);
}

main().catch((e) => {
  console.error('[r2b] 치명적 오류:', e);
  process.exitCode = 1;
});
