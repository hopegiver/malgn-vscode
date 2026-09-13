#!/usr/bin/env node
// 라운드2 전용 캡처 — harness.mjs가 다루지 않은 두 표면만 찍는다.
//   (1) 좁은 창 폭: 데스크톱 앱이라 사용자가 창을 줄인다. styles.css에 @media가
//       0개이므로 1440px 밖의 폭은 전부 미검증 영역이었다.
//   (2) 상호작용 이후 상태: 버튼을 누른 뒤 열리는 패널/폼/모달/토스트.
// harness.mjs의 stub/fixtures를 그대로 재사용하고 src/는 건드리지 않는다.
//
// 실행: node scripts/capture/round2.mjs [--round r2] [--base-url http://localhost:5173]
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildScenarioConfig, installTauriStub } from './stub.mjs';
import { IDS } from './fixtures.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const enc = encodeURIComponent;

function parseArgs(argv) {
  const out = { round: 'r2', baseUrl: 'http://localhost:5173' };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--round') out.round = argv[++i];
    else if (argv[i] === '--base-url') out.baseUrl = argv[++i];
  }
  return out;
}

// 좁은 폭에서 볼 라우트 — 고정폭 그리드/테이블을 가진 화면 위주로 고른다.
const NARROW_ROUTES = [
  { id: 'home', hash: '#/' },
  { id: 'usage', hash: '#/usage' },
  { id: 'sessions-detail', hash: `#/sessions/${enc(IDS.SESSION_ID)}` },
  { id: 'projects-detail', hash: `#/project/${enc(IDS.PROJECT_PATH)}` },
  { id: 'tasks-board', hash: '#/tasks/board' },
  { id: 'settings-mcp', hash: '#/settings/mcp' },
  { id: 'dev-tools', hash: '#/dev-tools' },
  { id: 'catalog', hash: '#/catalog' },
  { id: 'sessions-list', hash: '#/sessions' },
  { id: 'settings-otel', hash: '#/settings/otel' },
];
const NARROW_WIDTHS = [1120, 900, 760];

// 상호작용 시나리오 — 각 항목의 act(page)가 클릭/입력을 수행하고, 그 직후 화면을 찍는다.
const INTERACTIONS = [
  {
    id: 'usage-daily-detail',
    hash: '#/usage',
    note: '일별 막대 행 클릭 → 그날의 세션·에이전트·툴 상세 패널',
    act: async (page) => {
      await page.locator('.bar-row-clickable').first().click();
      await page.waitForTimeout(500);
    },
    expect: '에이전트',
  },
  {
    id: 'devtools-preview',
    hash: '#/dev-tools',
    note: '"업데이트 확인" 클릭 → dry-run 미리보기 패널',
    act: async (page) => {
      await page.locator('.devtool-row .btn').first().click();
      await page.waitForTimeout(600);
    },
    expect: '실행 전 확인',
  },
  {
    id: 'mcp-add-form-http',
    hash: '#/settings/mcp',
    note: '"+ 새 MCP 서버" 클릭 → 추가 폼(기본 transport=stdio)',
    act: async (page) => {
      await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
      await page.waitForTimeout(400);
    },
    expect: '실행 인자',
  },
  {
    id: 'mcp-add-form-oauth',
    hash: '#/settings/mcp',
    note: '추가 폼에서 transport=http 선택 → OAuth 필드 3개 노출',
    act: async (page) => {
      await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
      await page.waitForTimeout(300);
      await page.locator('.mcp-add-form select.settings-input').selectOption('http');
      await page.waitForTimeout(300);
    },
    expect: 'OAuth Client Secret',
  },
  {
    id: 'mcp-github-prefill',
    hash: '#/settings/mcp',
    note: 'GitHub 퀵스타트 행의 "설치" 클릭 → 프리필된 추가 폼',
    act: async (page) => {
      const rows = page.locator('.mcp-row');
      const n = await rows.count();
      for (let i = 0; i < n; i++) {
        const row = rows.nth(i);
        if ((await row.innerText()).includes('api.githubcopilot.com')) {
          await row.getByRole('button', { name: '설치' }).click();
          break;
        }
      }
      await page.waitForTimeout(400);
    },
    expect: 'api.githubcopilot.com',
  },
  {
    id: 'mcp-install-toast',
    hash: '#/settings/mcp',
    note: '카탈로그 행 "설치" 클릭 → 토스트',
    act: async (page) => {
      const rows = page.locator('.mcp-row');
      const n = await rows.count();
      for (let i = 0; i < n; i++) {
        const row = rows.nth(i);
        const t = await row.innerText();
        if (t.includes('설치') && !t.includes('api.githubcopilot.com')) {
          const btn = row.getByRole('button', { name: '설치' });
          if ((await btn.count()) > 0) {
            await btn.first().click();
            break;
          }
        }
      }
      await page.waitForTimeout(600);
    },
    expect: '터미널',
  },
  {
    id: 'task-add-form',
    hash: '#/tasks',
    note: '"+ 새 자율업무" 클릭 → 추가 폼',
    act: async (page) => {
      await page.getByRole('button', { name: '+ 새 자율업무' }).click();
      await page.waitForTimeout(400);
    },
    expect: '프롬프트',
  },
  {
    id: 'task-add-form-validation',
    hash: '#/tasks',
    note: '추가 폼을 비운 채 저장 → 검증 토스트',
    act: async (page) => {
      await page.getByRole('button', { name: '+ 새 자율업무' }).click();
      await page.waitForTimeout(300);
      await page.locator('.task-add-form button[type="submit"], .task-add-form .btn-primary').first().click();
      await page.waitForTimeout(500);
    },
    expect: '입력하세요',
  },
  {
    id: 'session-meta-modal',
    hash: `#/sessions/${enc(IDS.SESSION_ID)}`,
    note: '"ⓘ 메타데이터" 클릭 → 모달',
    act: async (page) => {
      await page.getByRole('button', { name: 'ⓘ 메타데이터' }).click();
      await page.waitForTimeout(400);
    },
    expect: '세션 메타데이터',
  },
  {
    id: 'project-tree-file',
    hash: `#/project/${enc(IDS.PROJECT_PATH)}`,
    note: '트리에서 파일 행 클릭 → 파일 내용 패널',
    act: async (page) => {
      const rows = page.locator('.tree-name');
      const n = await rows.count();
      for (let i = 0; i < n; i++) {
        if ((await rows.nth(i).innerText()).includes('CLAUDE.md')) {
          await rows.nth(i).click();
          break;
        }
      }
      await page.waitForTimeout(600);
    },
    expect: 'CLAUDE.md',
  },
  {
    id: 'chat-composer-typed',
    hash: `#/sessions/new/${enc(IDS.PROJECT_PATH)}`,
    note: '새 세션 draft에 긴 메시지 입력 → 컴포저 확장 상태',
    act: async (page) => {
      const ta = page.locator('textarea').first();
      await ta.click();
      await ta.fill(
        'src/views/settings.ts의 MCP 패널에서 등록 서버와 카탈로그 항목을 섹션으로 나누고, 각 섹션에 개수를 붙여줘. 그리고 기존 동작은 바꾸지 마.'
      );
      await page.waitForTimeout(400);
    },
    expect: '새 대화를 시작하세요',
  },
  {
    id: 'catalog-plugin-open',
    hash: '#/catalog',
    note: '카탈로그 첫 플러그인 카드의 접기/펼치기 등 기본 상호작용',
    act: async (page) => {
      const btn = page.locator('.plugin-card button').first();
      if ((await btn.count()) > 0) await btn.click();
      await page.waitForTimeout(600);
    },
    expect: null,
  },
];

// 상호작용 결과를 좁은 폭에서도 한 번씩 본다(가장 넓은 표를 가진 두 곳).
const NARROW_INTERACTIONS = [
  { base: 'usage-daily-detail', width: 900 },
  { base: 'devtools-preview', width: 900 },
  { base: 'session-meta-modal', width: 760 },
  { base: 'mcp-add-form-oauth', width: 900 },
];

async function loginViaUI(page) {
  await page.click('.login-btn');
  await page.locator('.sidebar').waitFor({ state: 'visible', timeout: 8000 });
}

// 미리보기 패널이 실제 운영에서 보여줄 만한 내용(영향 항목·경고)을 담도록 보강한다.
function richConfig() {
  const cfg = buildScenarioConfig('normal');
  cfg.commands.preview_dev_tool_update = {
    value: {
      id: 'claude',
      planId: 'plan-7f3a91',
      willRun: true,
      commandDisplay: 'npm install -g @anthropic-ai/claude-code@latest',
      affected: [
        '/Users/dev/.nvm/versions/node/v22.14.0/lib/node_modules/@anthropic-ai/claude-code',
        '/Users/dev/.nvm/versions/node/v22.14.0/bin/claude',
      ],
      notes: '전역 npm 패키지를 교체합니다. 실행 중인 claude 프로세스가 있으면 종료 후 다시 시작하세요.',
      previewReliable: true,
    },
    delayMs: 120,
  };
  return cfg;
}

async function main() {
  const { round, baseUrl } = parseArgs(process.argv.slice(2));
  const outDir = join(__dirname, 'output', round);
  mkdirSync(outDir, { recursive: true });

  const browser = await chromium.launch();
  const manifest = [];
  const errorLog = [];
  let ok = 0;
  let fail = 0;

  function record(entry) {
    manifest.push(entry);
    if (entry.ok) ok++;
    else fail++;
    console.log(`[r2] ${entry.ok ? 'OK  ' : 'FAIL'} ${entry.id.padEnd(30)} ${entry.note ?? ''}`);
  }

  // ---------------- 1) 좁은 폭 ----------------
  for (const width of NARROW_WIDTHS) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = `w${width}/(login)`;
    page.on('pageerror', (e) => errorLog.push({ tag, message: e.message }));
    await page.addInitScript(installTauriStub, richConfig());
    await page.goto(baseUrl, { waitUntil: 'load' });
    await loginViaUI(page);

    for (const route of NARROW_ROUTES) {
      tag = `w${width}/${route.id}`;
      await page.evaluate((h) => {
        window.location.hash = h;
      }, route.hash);
      await page.waitForTimeout(500);
      // 가로 스크롤(= 폭 넘침)이 생겼는지 함께 관측한다.
      const overflow = await page.evaluate(() => {
        const c = document.querySelector('.content');
        return {
          docOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
          contentOverflow: c ? c.scrollWidth - c.clientWidth : null,
        };
      });
      const file = join(outDir, `${round}__w${width}__${route.id}.png`);
      await page.screenshot({ path: file, fullPage: true });
      record({
        id: `w${width}/${route.id}`,
        kind: 'narrow',
        width,
        routeId: route.id,
        file,
        ok: true,
        note: `docOverflow=${overflow.docOverflow}px contentOverflow=${overflow.contentOverflow}px`,
        overflow,
      });
    }
    await context.close();
  }

  // ---------------- 2) 상호작용 (1440px) ----------------
  for (const spec of INTERACTIONS) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = `act/${spec.id}`;
    page.on('pageerror', (e) => errorLog.push({ tag, message: e.message }));
    page.on('dialog', (d) => void d.accept());
    await page.addInitScript(installTauriStub, richConfig());
    await page.goto(baseUrl, { waitUntil: 'load' });
    await loginViaUI(page);
    await page.evaluate((h) => {
      window.location.hash = h;
    }, spec.hash);
    await page.waitForTimeout(700);

    let actErr = null;
    try {
      await spec.act(page);
    } catch (e) {
      actErr = e.message;
    }
    const text = await page.locator('body').innerText();
    const matched = spec.expect ? text.includes(spec.expect) : true;
    const file = join(outDir, `${round}__act__${spec.id}.png`);
    await page.screenshot({ path: file, fullPage: true });
    record({
      id: `act/${spec.id}`,
      kind: 'interaction',
      width: 1440,
      file,
      ok: !actErr && matched,
      note: actErr ? `조작 실패: ${actErr}` : matched ? spec.note : `기대 문구 미검출: ${spec.expect}`,
    });
    await context.close();
  }

  // ---------------- 3) 상호작용 x 좁은 폭 ----------------
  for (const ni of NARROW_INTERACTIONS) {
    const spec = INTERACTIONS.find((s) => s.id === ni.base);
    const context = await browser.newContext({ viewport: { width: ni.width, height: 900 }, baseURL: baseUrl });
    const page = await context.newPage();
    let tag = `act-w${ni.width}/${spec.id}`;
    page.on('pageerror', (e) => errorLog.push({ tag, message: e.message }));
    page.on('dialog', (d) => void d.accept());
    await page.addInitScript(installTauriStub, richConfig());
    await page.goto(baseUrl, { waitUntil: 'load' });
    await loginViaUI(page);
    await page.evaluate((h) => {
      window.location.hash = h;
    }, spec.hash);
    await page.waitForTimeout(700);
    let actErr = null;
    try {
      await spec.act(page);
    } catch (e) {
      actErr = e.message;
    }
    const overflow = await page.evaluate(() => {
      const c = document.querySelector('.content');
      return {
        docOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        contentOverflow: c ? c.scrollWidth - c.clientWidth : null,
      };
    });
    const file = join(outDir, `${round}__actw${ni.width}__${spec.id}.png`);
    await page.screenshot({ path: file, fullPage: true });
    record({
      id: `act-w${ni.width}/${spec.id}`,
      kind: 'interaction-narrow',
      width: ni.width,
      file,
      ok: !actErr,
      note: actErr ? `조작 실패: ${actErr}` : `docOverflow=${overflow.docOverflow}px contentOverflow=${overflow.contentOverflow}px`,
      overflow,
    });
    await context.close();
  }

  await browser.close();

  const manifestPath = join(outDir, 'manifest.json');
  writeFileSync(
    manifestPath,
    JSON.stringify({ round, baseUrl, capturedAt: new Date().toISOString(), total: manifest.length, success: ok, failure: fail, pageErrors: errorLog, entries: manifest }, null, 2)
  );
  console.log('----------------------------------------');
  console.log(`[r2] ${ok} OK / ${fail} FAIL (총 ${manifest.length}장), pageerror ${errorLog.length}건`);
  console.log(`[r2] ${manifestPath}`);
}

main().catch((e) => {
  console.error('[r2] 치명적 오류:', e);
  process.exitCode = 1;
});
