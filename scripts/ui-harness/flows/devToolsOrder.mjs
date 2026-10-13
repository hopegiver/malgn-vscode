// 흐름 (P2-2) 개발 도구 "전체 업데이트" 의존 순서 — src/views/devTools.ts handleUpdateAll
// + src/devToolsOrder.ts.
//
// 검증 목표: (a) 선행 도구(Node.js)가 설치 확인되지 않으면 그에 의존하는 미설치
// 도구(Claude Code/pnpm/Wrangler)는 미리보기·설치 호출 없이 건너뛰고 사유가 항목에
// 보인다 (b) 배치 실행 순서가 Node.js 먼저(백엔드 목록에서 Claude가 앞에 있어도)
// (c) Claude Code에 "필수" 배지 (d) 선행 도구가 이미 설치돼 있으면 정상 진행.
//
// 검증 범위: TS/DOM 층만(스텁 브리지). 실제 설치·업데이트 명령은 절대 실행되지
// 않는다 — install_dev_tool/update_dev_tool/preview_dev_tool_update는 전부 아래
// 인라인 스텁이 응답한다.
export const flowId = 'devToolsOrder';
export const startHash = '#/settings/devtools';

const FILE = 'src/views/devTools.ts:handleUpdateAll';

// 백엔드 DEV_TOOLS 테이블 순서(Claude가 Node보다 앞) 그대로 — 정렬이 없으면 Claude가 먼저
// 실행되는 상황을 재현한다. installDependsOn/required는 백엔드 정본 값과 같은 모양.
function tool(id, name, { installed, required = false, installDependsOn = [], actionKind = 'run' }) {
  return {
    id,
    name,
    installed,
    version: installed ? '1.0.0' : null,
    path: installed ? `/opt/homebrew/bin/${id}` : null,
    installMethod: installed ? 'brew' : null,
    actionKind,
    manualHint: null,
    required,
    installDependsOn,
  };
}

// 실제 백엔드가 내는 "신규 머신" 모양: Node가 없으면 npm이 해석되지 않아 Claude/pnpm/Wrangler가
// 처음부터 actionKind=manual이고(query.rs: resolve_install_plan → Manual), mac에서는 Node도 manual이다.
// 그래서 배치의 runnable에는 이미 설치된 gh/git 업데이트만 남는다.
function realShapeNoNodeList() {
  return [
    tool('claude', 'Claude Code', { installed: false, required: true, installDependsOn: ['node'], actionKind: 'manual' }),
    tool('node', 'Node.js', { installed: false, required: true, actionKind: 'manual' }),
    tool('gh', 'GitHub CLI', { installed: true }),
    tool('git', 'Git', { installed: true, required: true, actionKind: 'manual' }),
    tool('pnpm', 'pnpm', { installed: false, installDependsOn: ['node'], actionKind: 'manual' }),
    tool('wrangler', 'Wrangler', { installed: false, installDependsOn: ['node'], actionKind: 'manual' }),
  ];
}

function toolList({ nodeInstalled }) {
  return [
    tool('claude', 'Claude Code', { installed: false, required: true, installDependsOn: ['node'] }),
    tool('node', 'Node.js', { installed: nodeInstalled, required: true }),
    tool('gh', 'GitHub CLI', { installed: true }),
    tool('git', 'Git', { installed: true, required: true }),
    tool('pnpm', 'pnpm', { installed: false, installDependsOn: ['node'] }),
    tool('wrangler', 'Wrangler', { installed: false, installDependsOn: ['node'] }),
  ];
}

// 페이지 안에서 invoke를 감싸 도구별로 응답한다(정적 픽스처는 같은 커맨드를 도구별로
// 다르게 답할 수 없다). 인자로 { nodeOutcome, nodePreviewReliable }를 받는다.
function installStub({ nodeOutcome, nodePreviewReliable }) {
  const orig = window.__TAURI_INTERNALS__.invoke;
  const result = (id, outcome, update) => ({
    id,
    outcome,
    verified: outcome === 'updated' || outcome === 'alreadyLatest',
    versionBefore: update ? '1.0.0' : null,
    versionAfter: outcome === 'failed' || outcome === 'unknownAfter' || outcome === 'timedOut' ? null : '2.0.0',
    normalizedAfter: null,
    installMethod: 'brew',
    durationMs: 10,
    message: outcome === 'failed' ? 'harness: 설치 실패(스텁)' : 'ok',
    logTail: '',
    pathVisible: true,
  });
  window.__installedAfter = new Set(['gh', 'git']);
  window.__invokeOrder = [];
  window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
    if (cmd === 'preview_dev_tool_update') {
      window.__invokeOrder.push(`preview:${args.toolId}`);
      const reliable = args.toolId === 'node' ? nodePreviewReliable : true;
      return { id: args.toolId, planId: `plan-${args.toolId}`, willRun: true, commandDisplay: `stub install ${args.toolId}`, affected: [args.toolId], notes: '', previewReliable: reliable };
    }
    if (cmd === 'install_dev_tool' || cmd === 'update_dev_tool') {
      window.__invokeOrder.push(`${cmd === 'install_dev_tool' ? 'install' : 'update'}:${args.toolId}`);
      const outcome = args.toolId === 'node' ? nodeOutcome : 'updated';
      if (outcome === 'updated') window.__installedAfter.add(args.toolId);
      return result(args.toolId, outcome, cmd === 'update_dev_tool');
    }
    return orig(cmd, args);
  };
}

async function runBatch(page) {
  await page.getByRole('button', { name: '전체 업데이트' }).click();
  // 미설치 N개 요약 확인(기존 동의 모델 — 그대로 유지돼야 한다).
  await page.locator('.modal-body').getByRole('button', { name: '확인' }).click();
  await page.waitForFunction(() => !document.querySelector('.devtool-header-actions .btn-primary')?.textContent?.includes('중…'), null, { timeout: 8000 });
  await page.waitForTimeout(150);
}

// 의존 순서와 무관한 도구(gh/git 업데이트)는 걸러 node 계열 호출만 본다 — 그 둘의 위치는
// 입력 순서 유지(안정 정렬) 결과일 뿐 이 흐름의 검증 대상이 아니다.
const ORDER_TOOLS = ['node', 'claude', 'pnpm', 'wrangler'];
async function invokeOrder(page) {
  const all = await page.evaluate(() => window.__invokeOrder ?? []);
  return all.filter((entry) => ORDER_TOOLS.includes(entry.split(':')[1]));
}

async function skipPanelFor(page, toolId) {
  const row = page.locator(`#devtool-row-${toolId}`);
  // 행과 같은 .devtool-item 안의 건너뜀 패널
  const panel = row.locator('xpath=ancestor::div[contains(@class,"devtool-item")]').locator('.devtool-panel-skipped');
  return (await panel.count()) > 0 ? (await panel.first().textContent()) ?? '' : null;
}

function arrEq(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Node 실패류 시나리오 공통 검증: Claude/pnpm/Wrangler는 미리보기·설치 호출조차 없고 사유가 보인다.
async function expectDependentsSkipped(page, bugs, label, expectedOrder) {
  const order = await invokeOrder(page);
  if (!arrEq(order, expectedOrder)) {
    bugs.push({ severity: 'Critical', symptom: `[${label}] 호출 순서가 기대와 다름 — 기대 ${JSON.stringify(expectedOrder)}, 실제 ${JSON.stringify(order)}`, file: FILE });
  }
  for (const id of ['claude', 'pnpm', 'wrangler']) {
    const text = await skipPanelFor(page, id);
    if (text === null) {
      bugs.push({ severity: 'Critical', symptom: `[${label}] ${id}: 선행 도구 미확인인데 "건너뜀" 사유 패널이 없음`, file: 'src/views/devTools.ts:renderSkipPanel' });
    } else if (!text.includes('건너뜀') || !text.includes('Node.js') || !text.includes('설치가 확인되지 않아')) {
      bugs.push({ severity: 'Major', symptom: `[${label}] ${id}: 건너뜀 사유 문구가 기대와 다름(실제: ${text})`, file: 'src/views/devTools.ts:renderSkipPanel' });
    }
  }
}

export function scenarios(base) {
  const mk = (id, nodeInstalled, stub, check) => ({
    id,
    startHash,
    fixtures: { ...base, check_dev_tools: toolList({ nodeInstalled }) },
    async run(page, ctx) {
      await page.evaluate(installStub, stub);
      await page.waitForSelector('.devtool-row');
      await check(page, ctx);
    },
  });

  return [
    // (c) Claude "필수" 배지 + 목록은 백엔드 순서 그대로(Claude가 Node보다 위) — 화면 순서는 안 바뀐다.
    mk('required-badge', false, { nodeOutcome: 'updated', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await shot('01-list');
      const claudeBadges = await page.locator('#devtool-row-claude .badge').allTextContents();
      if (!claudeBadges.includes('필수')) {
        bugs.push({ severity: 'Critical', symptom: `Claude Code 행에 "필수" 배지가 없음(실제 배지: ${JSON.stringify(claudeBadges)})`, file: 'src/views/devTools.ts:renderDevToolItem' });
      }
      for (const id of ['pnpm', 'wrangler', 'gh']) {
        const badges = await page.locator(`#devtool-row-${id} .badge`).allTextContents();
        if (badges.includes('필수')) {
          bugs.push({ severity: 'Major', symptom: `${id} 행에 "필수" 배지가 잘못 붙음`, file: 'src/views/devTools.ts:renderDevToolItem' });
        }
      }
      const rowOrder = await page.locator('.devtool-row').evaluateAll((rows) => rows.map((r) => r.id.replace('devtool-row-', '')));
      if (rowOrder[0] !== 'claude') {
        bugs.push({ severity: 'Minor', symptom: `화면 목록 순서가 백엔드 순서와 다름(실제: ${rowOrder.join(',')}) — 정렬은 배치 실행 순서에만 적용돼야 함`, file: 'src/views/devTools.ts:renderDevToolsView' });
      }
    }),

    // (a)+(b) Node 설치 실패 → 의존 3종 건너뜀, 실행은 Node 한 건뿐.
    mk('node-failed', false, { nodeOutcome: 'failed', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-node-failed');
      await expectDependentsSkipped(page, bugs, 'node-failed', ['preview:node', 'install:node']);
    }),

    // unknownAfter(명령은 성공했다지만 버전 재조회로 확인 못함) = "설치 확인 안 됨".
    mk('node-unknown-after', false, { nodeOutcome: 'unknownAfter', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-node-unknown-after');
      await expectDependentsSkipped(page, bugs, 'node-unknown-after', ['preview:node', 'install:node']);
    }),

    mk('node-timed-out', false, { nodeOutcome: 'timedOut', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-node-timed-out');
      await expectDependentsSkipped(page, bugs, 'node-timed-out', ['preview:node', 'install:node']);
    }),

    // 기존 규칙(previewReliable=false → 개별 확인 대기)과의 결합: Node가 실행되지 않으므로 의존 도구도 건너뜀.
    mk('node-preview-unreliable', false, { nodeOutcome: 'updated', nodePreviewReliable: false }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-node-pending-confirm');
      await expectDependentsSkipped(page, bugs, 'node-preview-unreliable', ['preview:node']);
      if ((await page.locator('#devtool-row-node').locator('xpath=ancestor::div[contains(@class,"devtool-item")]').locator('.devtool-panel-actions').count()) === 0) {
        bugs.push({ severity: 'Major', symptom: '[node-preview-unreliable] Node.js가 개별 확인 대기(미리보기 패널)로 남지 않음 — 기존 previewReliable=false 규칙 회귀', file: FILE });
      }
    }),

    // (b)+정상 진행: Node 설치 성공 → Node가 Claude/pnpm/Wrangler보다 먼저, 건너뜀 0건.
    mk('node-installed-in-batch', false, { nodeOutcome: 'updated', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-all-installed');
      const order = await invokeOrder(page);
      // 목록상 Claude가 Node 앞이지만 실행은 Node → Claude → pnpm → Wrangler(입력 순서 유지).
      const expected = ['preview:node', 'install:node', 'preview:claude', 'install:claude', 'preview:pnpm', 'install:pnpm', 'preview:wrangler', 'install:wrangler'];
      if (!arrEq(order, expected)) {
        bugs.push({ severity: 'Critical', symptom: `[node-installed-in-batch] 실행 순서가 의존 순서가 아님 — 기대 ${JSON.stringify(expected)}, 실제 ${JSON.stringify(order)}`, file: FILE });
      }
      const skipped = await page.locator('.devtool-panel-skipped').count();
      if (skipped !== 0) {
        bugs.push({ severity: 'Critical', symptom: `[node-installed-in-batch] Node 설치가 확인됐는데 건너뜀 패널이 ${skipped}개 있음`, file: FILE });
      }
    }),

    // 선행 도구가 이미 설치돼 있으면 정상 진행(Node는 업데이트로 먼저 실행, 이후 설치 3건).
    mk('node-already-installed', true, { nodeOutcome: 'failed', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await shot('01-after-batch-node-already-installed');
      const order = await invokeOrder(page);
      // Node 업데이트가 (스텁상) 실패해도 Node는 이미 설치돼 있으므로 의존 도구는 건너뛰지 않는다.
      // gh/git은 설치됨+run이라 업데이트 대상이지만 입력 순서(목록 순)상 node 다음에 온다.
      const idxNode = order.indexOf('update:node');
      const installs = order.filter((o) => o.startsWith('install:'));
      if (idxNode === -1 || !arrEq(installs, ['install:claude', 'install:pnpm', 'install:wrangler'])) {
        bugs.push({ severity: 'Critical', symptom: `[node-already-installed] 설치된 Node가 있는데 의존 도구가 정상 진행되지 않음 — 실제 ${JSON.stringify(order)}`, file: FILE });
      } else if (order.indexOf('install:claude') < idxNode) {
        bugs.push({ severity: 'Critical', symptom: `[node-already-installed] Claude 설치가 Node 업데이트보다 먼저 실행됨 — ${JSON.stringify(order)}`, file: FILE });
      }
      if ((await page.locator('.devtool-panel-skipped').count()) !== 0) {
        bugs.push({ severity: 'Critical', symptom: '[node-already-installed] Node가 이미 설치돼 있는데 건너뜀 패널이 표시됨', file: FILE });
      }
    }),

    // RV-005: 배치에서 Node 실패로 건너뜀 사유가 붙은 뒤 사용자가 Node를 따로 설치해 재조회하면(지금은
    // 선행 도구가 모두 설치됨) "설치가 확인되지 않아 건너뜀"은 사실이 아니므로 패널이 사라져야 한다.
    mk('skip-reason-clears-after-node-installed', false, { nodeOutcome: 'failed', nodePreviewReliable: true }, async (page, { shot, bugs }) => {
      await runBatch(page);
      await expectDependentsSkipped(page, bugs, 'skip-clears(배치 직후)', ['preview:node', 'install:node']);
      await shot('01-skip-panels-before');
      await page.evaluate((installedNodeList) => {
        const orig = window.__TAURI_INTERNALS__.invoke;
        window.__TAURI_INTERNALS__.invoke = async (cmd, args) => (cmd === 'check_dev_tools' ? installedNodeList : orig(cmd, args));
      }, toolList({ nodeInstalled: true }));
      await page.getByRole('button', { name: '↻ 다시 확인' }).click();
      await page.waitForFunction(() => document.querySelector('#devtool-row-node')?.textContent?.includes('설치됨') || !document.querySelector('.devtool-panel-skipped'), null, { timeout: 4000 }).catch(() => {});
      await page.waitForTimeout(200);
      await shot('02-skip-panels-after-node-installed');
      const left = await page.locator('.devtool-panel-skipped').count();
      if (left !== 0) {
        const texts = await page.locator('.devtool-panel-skipped').allTextContents();
        bugs.push({ severity: 'Major', symptom: `[skip-reason-clears] Node가 설치된 것으로 재조회됐는데 건너뜀 사유 패널이 ${left}개 남음(거짓 문장): ${JSON.stringify(texts)}`, file: 'src/views/devTools.ts:renderDevToolItem' });
      }
    }),

    // RV-002 보완(실제 모양): Node 미설치 신규 머신에서는 Claude/pnpm/Wrangler가 manual로 내려와
    // 배치 runnable에 들어오지 않는다 — 배치는 이들을 미리보기·설치하지 않고(오동작 없음), 건너뜀
    // 패널도 만들지 않으며(실행 대상이 아니었으므로), 설치 요약 동의창도 뜨지 않는다.
    {
      id: 'real-shape-no-node-dependents-manual-not-run',
      startHash,
      fixtures: { ...base, check_dev_tools: realShapeNoNodeList() },
      async run(page, { shot, bugs }) {
        await page.evaluate(installStub, { nodeOutcome: 'failed', nodePreviewReliable: true });
        await page.waitForSelector('.devtool-row');
        await page.getByRole('button', { name: '전체 업데이트' }).click();
        await page.waitForTimeout(300);
        if ((await page.locator('.modal-body').count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: '[real-shape] 설치 대상(run)이 없는데 설치 요약 동의창이 떴음', file: FILE });
        }
        await page.waitForFunction(() => !document.querySelector('.devtool-header-actions .btn-primary')?.textContent?.includes('중…'), null, { timeout: 8000 });
        await page.waitForTimeout(150);
        await shot('01-real-shape-after-batch');
        const all = await page.evaluate(() => window.__invokeOrder ?? []);
        const touched = all.filter((e) => ['claude', 'node', 'pnpm', 'wrangler'].includes(e.split(':')[1]));
        if (touched.length > 0) {
          bugs.push({ severity: 'Critical', symptom: `[real-shape] manual 도구(Claude/Node/pnpm/Wrangler)에 배치가 미리보기·설치를 호출함: ${JSON.stringify(touched)}`, file: FILE });
        }
        if (!all.includes('update:gh')) {
          bugs.push({ severity: 'Major', symptom: `[real-shape] 설치된 gh 업데이트는 정상 진행돼야 함(실제 호출: ${JSON.stringify(all)})`, file: FILE });
        }
        if ((await page.locator('.devtool-panel-skipped').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '[real-shape] 실행 대상이 아니었던 manual 도구에 건너뜀 패널이 표시됨', file: FILE });
        }
      },
    },

    // 순수 함수(devToolsOrder.ts) 단위 검증 — Vite가 모듈을 그대로 서빙하므로 브라우저에서 import한다.
    {
      id: 'pure-order-functions',
      startHash,
      fixtures: { ...base, check_dev_tools: toolList({ nodeInstalled: false }) },
      async run(page, { bugs }) {
        const out = await page.evaluate(async () => {
          const m = await import('/src/devToolsOrder.ts');
          const ids = (xs) => xs.map((x) => x.id);
          const t = (id, dep) => ({ id, installDependsOn: dep });
          return {
            reordered: ids(m.orderByInstallDependency([t('claude', ['node']), t('node', []), t('pnpm', ['node']), t('gh')])),
            noDeps: ids(m.orderByInstallDependency([t('a'), t('b'), t('c')])),
            depOutsideBatch: ids(m.orderByInstallDependency([t('claude', ['node']), t('gh')])),
            chain: ids(m.orderByInstallDependency([t('c', ['b']), t('b', ['a']), t('a')])),
            cycle: ids(m.orderByInstallDependency([t('x', ['y']), t('y', ['x']), t('z')])),
            unmetNone: m.unmetInstallDependencies(t('claude', ['node']), new Set(['node'])),
            unmetOne: m.unmetInstallDependencies(t('claude', ['node']), new Set()),
            unmetMissingField: m.unmetInstallDependencies({ id: 'gh' }, new Set()),
            confirmed: ['updated', 'alreadyLatest'].map(m.isInstallConfirmedOutcome),
            notConfirmed: ['failed', 'unknownAfter', 'timedOut', 'notSupported'].map(m.isInstallConfirmedOutcome),
          };
        });
        const expect = (name, actual, expected) => {
          if (JSON.stringify(actual) !== JSON.stringify(expected)) {
            bugs.push({ severity: 'Critical', symptom: `[pure:${name}] 기대 ${JSON.stringify(expected)}, 실제 ${JSON.stringify(actual)}`, file: 'src/devToolsOrder.ts' });
          }
        };
        expect('reordered', out.reordered, ['node', 'claude', 'pnpm', 'gh']);
        expect('noDeps(입력 순서 유지)', out.noDeps, ['a', 'b', 'c']);
        expect('depOutsideBatch', out.depOutsideBatch, ['claude', 'gh']);
        expect('chain', out.chain, ['a', 'b', 'c']);
        expect('cycle(누락 없이 반환)', out.cycle.slice().sort(), ['x', 'y', 'z']);
        expect('unmetNone', out.unmetNone, []);
        expect('unmetOne', out.unmetOne, ['node']);
        expect('unmetMissingField', out.unmetMissingField, []);
        expect('confirmed', out.confirmed, [true, true]);
        expect('notConfirmed', out.notConfirmed, [false, false, false, false]);
      },
    },
  ];
}
