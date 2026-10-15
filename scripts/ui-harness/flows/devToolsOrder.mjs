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

// ---------------------------------------------------------------------------------------------
// R14 P2-9 — 선행 도구(Node.js) 설치 확인 뒤 이어서 설치.
//
// 픽스처는 "백엔드가 실제로 만들 수 있는 응답 모양"만 쓴다(근거는 각 함수 주석):
//  - 미설치 분기 query.rs:34-60 — path:null, actionKind = resolve_install_plan 결과
//    (install_resolver.rs:223-249: 러너를 못 찾으면 Manual, 찾으면 Run)
//  - 설치됨 분기 query.rs:82-101 — path:Some, installed = version.is_some(), actionKind = resolve_plan(업데이트 표)
//  - Claude/pnpm/Wrangler는 npm(또는 pnpm) 러너가 있어야 Run(install_resolver.rs:159-188, 233-235)이고
//    npm은 Node가 있어야 해석된다 → Node 미설치 머신에서는 manual, Node 설치 후 재조회에서 run.
//  - Windows Node 설치는 winget 후보(install_resolver.rs:195-199)이고 winget 미리보기는 불신뢰
//    (query.rs:115-117, 232-239) → previewReliable=false. npm/pnpm 설치는 preview_args:None이라
//    previewReliable=true, affected 1개(query.rs:338-349, plan_table.rs RUN_NPM_*·RUN_PNPM_GLOBAL_ADD).
//  - macOS Node는 구조적 manual(install_resolver.rs:125-128 주석, mac에서 Runner::Winget은 항상 None).
// ---------------------------------------------------------------------------------------------
function backendTool(id, name, o) {
  return {
    id,
    name,
    installed: o.installed,
    version: o.installed ? (o.version ?? '1.0.0') : null,
    path: o.path ?? null,
    installMethod: o.installMethod ?? null,
    actionKind: o.actionKind,
    manualHint: o.manualHint ?? null,
    required: o.required ?? false,
    installDependsOn: o.installDependsOn ?? [],
  };
}

const WIN_NO_NPM_HINT = 'Claude Code가 설치되어 있지 않습니다. npm을 찾을 수 없다면 Node.js를 먼저 설치해주세요(winget install OpenJS.NodeJS.LTS).';
const MAC_NODE_HINT = 'Node.js가 설치되어 있지 않습니다. 이미 nvm/fnm/volta 등으로 관리 중이라면 그쪽에서 설치해주세요. 그렇지 않고 Homebrew가 있다면 아래 명령으로 설치할 수 있습니다.';
const MAC_NO_RUNNER_HINT = '필요한 실행 도구(brew/npm)를 찾을 수 없어 앱이 자동으로 실행하지 않습니다.';

// platform: 'win' | 'mac'.  phase: 'beforeNode'(Node 없음) | 'nodeUnverified'(Node 파일은 있으나 --version 무응답) | 'afterNode'(Node 설치 확인됨)
function chainList(platform, phase) {
  const win = platform === 'win';
  const nodePath = win ? 'C:\\Program Files\\nodejs\\node.exe' : '/opt/homebrew/bin/node';
  const hint = win ? WIN_NO_NPM_HINT : MAC_NO_RUNNER_HINT;
  const depRun = phase === 'afterNode' || phase === 'nodeUnverified' ? 'run' : 'manual';
  const dep = (id, name, extra = {}) =>
    backendTool(id, name, { installed: false, actionKind: depRun, manualHint: depRun === 'manual' ? hint : null, installDependsOn: ['node'], ...extra });
  let node;
  if (phase === 'beforeNode') {
    // 미설치 분기: Windows는 winget 후보가 있어 run, macOS는 구조적 manual.
    node = backendTool('node', 'Node.js', { installed: false, actionKind: win ? 'run' : 'manual', manualHint: win ? null : MAC_NODE_HINT, required: true });
  } else if (phase === 'nodeUnverified') {
    // 설치됨 분기이지만 version=None: installed=false, path는 채워짐(query.rs:82-101). actionKind는 업데이트 표를 따른다:
    // Windows MSI 설치본은 manual, macOS Homebrew 설치본은 run(plan_table.rs:661-665 HomebrewFormula→Run).
    // run이면 실제로 도는 것은 설치가 아니라 업데이트다(actions.rs:237-239: 경로가 있으면 install→update 위임).
    node = backendTool('node', 'Node.js', { installed: false, path: nodePath, installMethod: win ? 'MSI 설치본' : 'Homebrew', actionKind: win ? 'manual' : 'run', manualHint: win ? '업데이트는 앱이 대신 실행하지 않습니다.' : null, required: true });
  } else {
    node = backendTool('node', 'Node.js', { installed: true, version: '22.11.0', path: nodePath, installMethod: win ? 'MSI 설치본' : 'Homebrew', actionKind: win ? 'manual' : 'run', manualHint: win ? '업데이트는 앱이 대신 실행하지 않습니다.' : null, required: true });
  }
  return [
    dep('claude', 'Claude Code', { required: true }),
    node,
    backendTool('gh', 'GitHub CLI', { installed: true, path: win ? 'C:\\Program Files\\GitHub CLI\\gh.exe' : '/opt/homebrew/bin/gh', installMethod: 'winget/Homebrew', actionKind: 'run' }),
    backendTool('git', 'Git', { installed: true, path: win ? 'C:\\Program Files\\Git\\cmd\\git.exe' : '/opt/homebrew/bin/git', installMethod: 'Homebrew', actionKind: 'manual', manualHint: '업데이트는 앱이 대신 실행하지 않습니다.', required: true }),
    dep('pnpm', 'pnpm'),
    dep('wrangler', 'Wrangler'),
  ];
}

// 스텁: 모든 invoke를 window.__calls에 `cmd:toolId`로 기록한다. check_dev_tools는 window.__list를 돌려주고,
// 설치 성공이면 그 도구를 installed로 바꾼다(Node 성공 시에는 afterList로 통째 교체 — 재조회 후 의존 도구가 run이 된 응답).
function installChainStub({ afterList, nodeOutcome, nodePreviewReliable = false, nodeCommand = 'winget install --id OpenJS.NodeJS.LTS -e' }) {
  const orig = window.__TAURI_INTERNALS__.invoke;
  window.__calls = [];
  window.__afterList = afterList;
  window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
    const id = args && args.toolId;
    if (['check_dev_tools', 'preview_dev_tool_update', 'install_dev_tool', 'update_dev_tool', 'open_manual_instruction'].includes(cmd)) {
      window.__calls.push(id ? `${cmd}:${id}${cmd === 'open_manual_instruction' ? `:${args.execute}` : ''}` : cmd);
    }
    if (cmd === 'check_dev_tools') return window.__list;
    if (cmd === 'preview_dev_tool_update') {
      return { id, planId: `plan-${id}`, willRun: true, commandDisplay: id === 'node' ? nodeCommand : `npm install -g ${id}`, affected: [id], notes: id === 'node' && !nodePreviewReliable ? 'winget은 사전 시뮬레이션을 제공하지 않아 함께 변경될 항목을 미리 확인할 수 없습니다.' : '', previewReliable: id === 'node' ? nodePreviewReliable : true };
    }
    if (cmd === 'install_dev_tool' || cmd === 'update_dev_tool') {
      const outcome = id === 'node' ? nodeOutcome : 'updated';
      if (outcome === 'updated') {
        if (id === 'node') window.__list = window.__afterList;
        else window.__list = window.__list.map((t) => (t.id === id ? { ...t, installed: true, version: '2.0.0', path: `/stub/${id}`, actionKind: 'run' } : t));
      }
      return {
        id,
        outcome,
        verified: outcome === 'updated',
        versionBefore: null,
        versionAfter: outcome === 'updated' ? '2.0.0' : null,
        normalizedAfter: null,
        installMethod: 'stub',
        durationMs: 10,
        message: outcome === 'failed' ? 'harness: 설치 실패(스텁)' : 'ok',
        logTail: '',
        pathVisible: true,
      };
    }
    if (cmd === 'open_manual_instruction') {
      return { opened: !!args.execute, message: args.execute ? '터미널을 열었습니다(스텁)' : '다음 명령을 실행합니다: brew install node' };
    }
    return orig(cmd, args);
  };
}

const EXEC_CMDS = ['install_dev_tool', 'update_dev_tool', 'open_manual_instruction'];
const DEPS = ['claude', 'pnpm', 'wrangler'];
const callsOf = (page) => page.evaluate(() => window.__calls ?? []);
const execOn = (calls, ids) => calls.filter((c) => EXEC_CMDS.some((e) => c.startsWith(`${e}:`)) && ids.includes(c.split(':')[1]));
const anyOn = (calls, ids) => calls.filter((c) => ids.includes(c.split(':')[1]));

async function waitIdle(page) {
  await page.waitForFunction(() => !/중…/.test(document.querySelector('.devtool-header-actions')?.textContent ?? '') && !document.querySelector('.devtool-spinner'), null, { timeout: 8000 });
  await page.waitForTimeout(200);
}

async function modalText(page) {
  return (await page.locator('.modal-body').count()) > 0 ? ((await page.locator('.modal-body').first().textContent()) ?? '') : null;
}

function chainScenarios(base) {
  const FILE_C = 'src/views/devTools.ts:offerInstallChain';
  const mk = (id, { platform, phase = 'beforeNode', nodeOutcome = 'updated' }, body) => ({
    id,
    startHash,
    fixtures: { ...base, check_dev_tools: chainList(platform, phase) },
    async run(page, ctx) {
      await page.evaluate((arg) => { window.__list = arg.list; }, { list: chainList(platform, phase) });
      // 이 모듈의 Windows 픽스처는 winget(미리보기 불신뢰), macOS 픽스처는 Homebrew(brew --dry-run 미리보기 신뢰).
      await page.evaluate(installChainStub, { afterList: chainList(platform, 'afterNode'), nodeOutcome, nodePreviewReliable: platform !== 'win', nodeCommand: platform === 'win' ? 'winget install --id OpenJS.NodeJS.LTS -e' : 'brew upgrade node' });
      // 초기 로드는 스텁 설치 전에 끝났으므로 목록을 스텁 기준으로 다시 맞춘다.
      await page.getByRole('button', { name: '↻ 다시 확인' }).click();
      await page.waitForSelector('.devtool-row');
      await page.waitForTimeout(150);
      await page.evaluate(() => { window.__calls = []; });
      await body(page, ctx);
    },
  });

  const installNodeIndividually = async (page) => {
    await page.locator('#devtool-row-node').getByRole('button', { name: '설치' }).click();
    await page.waitForSelector('.devtool-panel-actions');
    await page.locator('.devtool-panel-actions').getByRole('button', { name: '실행' }).click();
  };

  const expectChainOrder = (calls, bugs, label) => {
    const want = ['preview_dev_tool_update:claude', 'install_dev_tool:claude', 'preview_dev_tool_update:pnpm', 'install_dev_tool:pnpm', 'preview_dev_tool_update:wrangler', 'install_dev_tool:wrangler'];
    const got = calls.filter((c) => /^(preview_dev_tool_update|install_dev_tool|update_dev_tool):(claude|pnpm|wrangler)$/.test(c));
    if (!arrEq(got, want)) bugs.push({ severity: 'Critical', symptom: `[${label}] 이어서 설치 호출이 기대와 다름 — 기대 ${JSON.stringify(want)}, 실제 ${JSON.stringify(got)}`, file: FILE_C });
  };

  return [
    // (a) Windows 형태: Node 개별 확인 설치 성공 → 재조회 → 이어서 설치 동의 → 의존 도구 실행.
    mk('chain-win-consent-then-run', { platform: 'win' }, async (page, { shot, bugs }) => {
      // Node 미리보기 패널: 이어서 설치가 "한 번 더 묻는다"고만 말하고 한 번에 된다고 주장하지 않는다.
      await page.locator('#devtool-row-node').getByRole('button', { name: '설치' }).click();
      await page.waitForSelector('.devtool-panel-actions');
      await shot('01-node-preview-with-chain-hint');
      const panel = (await page.locator('.devtool-chain-hint').first().textContent()) ?? '';
      if (!panel.includes('Claude Code') || !panel.includes('한 번 더 묻습니다')) bugs.push({ severity: 'Major', symptom: `[win] Node 미리보기에 이어서 설치 안내가 없거나 부정확함: ${panel}`, file: 'src/views/devTools.ts:renderChainHint' });
      await page.locator('.devtool-panel-actions').getByRole('button', { name: '실행' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      const text = (await modalText(page)) ?? '';
      await shot('02-chain-consent-dialog');
      for (const n of ['Claude Code', 'pnpm', 'Wrangler', 'Node.js 설치가 확인되었습니다']) {
        if (!text.includes(n)) bugs.push({ severity: 'Critical', symptom: `[win] 동의 문구에 "${n}"이 없음: ${text}`, file: FILE_C });
      }
      if (/GitHub CLI|Git\b/.test(text.replace('GitHub', ''))) bugs.push({ severity: 'Critical', symptom: `[win] 동의 문구에 실행 대상이 아닌 도구가 있음: ${text}`, file: FILE_C });
      // 동의 전: 의존 도구에 대한 실행 커맨드 0건(미리보기도 0건).
      const before = await callsOf(page);
      if (anyOn(before, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[win] 동의 전에 의존 도구 커맨드가 호출됨: ${JSON.stringify(anyOn(before, DEPS))}`, file: FILE_C });
      if (execOn(before, ['node']).join() !== 'install_dev_tool:node') bugs.push({ severity: 'Critical', symptom: `[win] Node 개별 설치가 정확히 1회가 아님: ${JSON.stringify(before)}`, file: FILE_C });
      await page.locator('.modal-body').getByRole('button', { name: '이어서 설치' }).click();
      await waitIdle(page);
      await shot('03-after-chain');
      const calls = await callsOf(page);
      expectChainOrder(calls, bugs, 'win');
      const nodeExec = execOn(calls, ['node']);
      if (nodeExec.length !== 1) bugs.push({ severity: 'Critical', symptom: `[win] Node 실행이 ${nodeExec.length}회 (중복 실행)`, file: FILE_C });
      const rows = await page.locator('.devtool-row').evaluateAll((rs) => rs.map((r) => [r.id.replace('devtool-row-', ''), r.textContent ?? '']));
      for (const [id, t] of rows) if (['claude', 'node', 'pnpm', 'wrangler'].includes(id) && t.includes('설치 안 됨')) bugs.push({ severity: 'Major', symptom: `[win] ${id}가 이어서 설치 뒤에도 미설치로 보임`, file: FILE_C });
      if ((await page.locator('.modal-body').count()) !== 0) bugs.push({ severity: 'Major', symptom: '[win] 흐름이 끝났는데 확인창이 남아 있음(연쇄가 끝나지 않음)', file: FILE_C });
    }),

    // (b) 동의 거부: 어떤 의존 도구도 미리보기·실행되지 않고, 항목은 개별 "설치"로 남는다.
    mk('chain-win-declined', { platform: 'win' }, async (page, { shot, bugs }) => {
      await installNodeIndividually(page);
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await page.locator('.modal-body').getByRole('button', { name: '취소' }).click();
      await page.waitForTimeout(400);
      await shot('01-after-decline');
      const calls = await callsOf(page);
      if (anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[declined] 거부했는데 의존 도구 커맨드가 호출됨: ${JSON.stringify(anyOn(calls, DEPS))}`, file: FILE_C });
      for (const id of DEPS) {
        const btn = await page.locator(`#devtool-row-${id}`).getByRole('button', { name: '설치' }).count();
        if (btn !== 1) bugs.push({ severity: 'Major', symptom: `[declined] ${id}에 개별 "설치" 버튼이 남아 있지 않음`, file: FILE_C });
      }
      // 같은 상태에서 다시 확인해도(변화 없음) 다시 묻지 않는다 — 유한성.
      await page.getByRole('button', { name: '↻ 다시 확인' }).click();
      await page.waitForTimeout(400);
      if ((await page.locator('.modal-body').count()) !== 0) bugs.push({ severity: 'Major', symptom: '[declined] 변화 없는 재조회가 이어서 설치를 다시 물음(무한 제안)', file: FILE_C });
    }),

    // (c) Node 설치 실패/unknownAfter/timedOut → 연쇄 없음, 의존 도구 호출 0건.
    ...['failed', 'unknownAfter', 'timedOut'].map((nodeOutcome) =>
      mk(`chain-win-node-${nodeOutcome}`, { platform: 'win', nodeOutcome }, async (page, { shot, bugs }) => {
        await installNodeIndividually(page);
        await page.waitForTimeout(700);
        await shot('01-node-not-confirmed');
        if ((await page.locator('.modal-body').count()) !== 0) bugs.push({ severity: 'Critical', symptom: `[node-${nodeOutcome}] Node 설치가 확인되지 않았는데 이어서 설치 확인창이 뜸`, file: FILE_C });
        const calls = await callsOf(page);
        if (anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[node-${nodeOutcome}] 선행 설치 미확인인데 의존 도구 커맨드가 호출됨: ${JSON.stringify(anyOn(calls, DEPS))}`, file: FILE_C });
        // 실패 사유(결과 패널)는 Node 행에 남는다.
        const item = await page.locator('#devtool-row-node').locator('xpath=ancestor::div[contains(@class,"devtool-item")]').textContent();
        if (!item.includes(nodeOutcome === 'failed' ? '실행 실패' : nodeOutcome === 'timedOut' ? '시간이 초과' : '확인하지 못했습니다')) bugs.push({ severity: 'Major', symptom: `[node-${nodeOutcome}] Node 결과 사유가 화면에 없음: ${item}`, file: 'src/views/devTools.ts:renderResultPanel' });
      })
    ),

    // (d) macOS 형태: Node는 manual. 문구가 한 번에 설치된다고 주장하지 않고, 직접 설치 뒤 다시 확인하면 같은 이어서 설치 흐름.
    mk('chain-mac-manual-node-then-recheck', { platform: 'mac' }, async (page, { shot, bugs }) => {
      await page.locator('#devtool-row-node').getByRole('button', { name: '안내 보기' }).click();
      await page.waitForSelector('.devtool-chain-hint');
      await shot('01-mac-node-manual-guide');
      const hint = (await page.locator('.devtool-chain-hint').first().textContent()) ?? '';
      if (!hint.includes('직접 설치한 뒤') || !hint.includes('Claude Code') || !hint.includes('설치를 대신하지는 않습니다')) bugs.push({ severity: 'Major', symptom: `[mac] Node 안내의 이어서 설치 문구가 부정확함: ${hint}`, file: 'src/views/devTools.ts:renderChainHint' });
      // R14 RV-005: "다시 확인하면 이어서 묻는다"는 약속은 앱이 감지할 수 있는 설치(Homebrew·공식 설치 프로그램)로 한정하고,
      // 버전 관리자(nvm 등) 설치는 감지되지 않으니 직접 설치하라고 사실대로 적는다.
      if (!hint.includes('Homebrew 또는 공식 설치 프로그램') || !hint.includes('nvm') || !hint.includes('감지하지 못하므로')) bugs.push({ severity: 'Major', symptom: `[mac] Node 안내가 앱이 감지하는 설치 방법으로 약속을 한정하지 않음: ${hint}`, file: 'src/views/devTools.ts:renderChainHint' });
      const bodyText = (await page.locator('body').textContent()) ?? '';
      if (/한 번에|한번에|자동으로 (모두|함께) 설치/.test(bodyText)) bugs.push({ severity: 'Major', symptom: '[mac] 화면이 macOS에서 한 번에 설치된다고 주장함', file: 'src/views/devTools.ts' });
      // 전체 업데이트: 설치 대상(run)이 없으므로 확인창이 뜨지 않고 Node/의존 도구에 아무 것도 하지 않는다.
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForTimeout(500);
      if ((await page.locator('.modal-body').count()) !== 0) bugs.push({ severity: 'Major', symptom: '[mac] 설치 대상이 없는데 전체 업데이트가 설치 확인창을 띄움', file: 'src/views/devTools.ts:handleUpdateAll' });
      let calls = await callsOf(page);
      if (anyOn(calls, ['node', ...DEPS]).filter((c) => !c.startsWith('check_dev_tools')).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac] 전체 업데이트가 manual 도구에 커맨드를 호출함: ${JSON.stringify(calls)}`, file: 'src/views/devTools.ts:handleUpdateAll' });
      // 사용자가 터미널에서 Node를 직접 설치했다고 가정: 다음 조회에서 Node 설치됨 + 의존 도구 run.
      await page.evaluate(() => { window.__list = window.__afterList; });
      await page.evaluate(() => { window.__calls = []; });
      await page.getByRole('button', { name: '↻ 다시 확인' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await shot('02-mac-chain-consent-after-manual-install');
      const text = (await modalText(page)) ?? '';
      if (!text.includes('Claude Code') || !text.includes('pnpm') || !text.includes('Wrangler')) bugs.push({ severity: 'Critical', symptom: `[mac] 동의 문구에 대상 도구 이름이 없음: ${text}`, file: FILE_C });
      calls = await callsOf(page);
      if (execOn(calls, ['node', ...DEPS]).length > 0 || anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac] 동의 전에 설치 관련 커맨드가 호출됨: ${JSON.stringify(calls)}`, file: FILE_C });
      await page.locator('.modal-body').getByRole('button', { name: '이어서 설치' }).click();
      await waitIdle(page);
      await shot('03-mac-after-chain');
      calls = await callsOf(page);
      expectChainOrder(calls, bugs, 'mac');
      if (anyOn(calls, ['node']).some((c) => !c.startsWith('check_dev_tools'))) bugs.push({ severity: 'Critical', symptom: `[mac] 앱이 Node.js에 설치/미리보기를 시도함(P2-3 범위 밖): ${JSON.stringify(calls)}`, file: FILE_C });
    }),

    // R7 RV-002 예외 경로: Node 파일은 있는데 --version 무응답(installed=false, path 있음) + npm 해석됨(의존 도구 run).
    // 오판 1) "설치가 확인되지 않아"라고 단정하거나 2) 그 상태로 Claude 등을 설치 시도하면 안 된다.
    mk('chain-node-unverified-timeout', { platform: 'win', phase: 'nodeUnverified' }, async (page, { shot, bugs }) => {
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForTimeout(400);
      // 의존 3종은 이번 배치에서 실행될 수 없으므로 동의창에 이름이 오르지 않는다(동의 == 실행 대상). 설치 대상이 없어 창이 없다.
      if ((await page.locator('.modal-body').count()) !== 0) {
        bugs.push({ severity: 'Critical', symptom: `[unverified] 실행할 수 없는 도구가 설치 동의창에 오름: ${await modalText(page)}`, file: 'src/views/devTools.ts:handleUpdateAll' });
        await page.locator('.modal-body').getByRole('button', { name: '취소' }).click();
      }
      await waitIdle(page);
      await shot('01-unverified-batch-skip-reasons');
      let calls = await callsOf(page);
      if (anyOn(calls, ['node', ...DEPS]).some((c) => !c.startsWith('check_dev_tools'))) bugs.push({ severity: 'Critical', symptom: `[unverified] 버전 미확인 Node 때문에 의존 도구/Node에 커맨드가 호출됨: ${JSON.stringify(calls)}`, file: 'src/views/devTools.ts:handleUpdateAll' });
      for (const id of DEPS) {
        const panel = page.locator(`#devtool-row-${id}`).locator('xpath=ancestor::div[contains(@class,"devtool-item")]').locator('.devtool-panel-skipped');
        const t = (await panel.count()) > 0 ? ((await panel.first().textContent()) ?? '') : null;
        if (t === null) bugs.push({ severity: 'Critical', symptom: `[unverified] ${id}: 건너뜀 사유가 없음`, file: 'src/views/devTools.ts:renderSkipPanel' });
        else {
          if (!t.includes('버전을 확인하지 못해') || !t.includes('파일이 발견됐지만')) bugs.push({ severity: 'Major', symptom: `[unverified] ${id}: 사유가 "파일은 있으나 버전 미확인"을 말하지 않음: ${t}`, file: 'src/devToolsOrder.ts:describeUnmetDependencies' });
          if (t.includes('설치가 확인되지 않아')) bugs.push({ severity: 'Major', symptom: `[unverified] ${id}: 있는 Node를 "설치 안 됨"으로 단정하는 문구: ${t}`, file: 'src/devToolsOrder.ts:describeUnmetDependencies' });
        }
      }
      // 이후 Node가 응답하면(재조회) 같은 이어서 설치 흐름으로 회복된다.
      await page.evaluate(() => { window.__list = window.__afterList; window.__calls = []; });
      await page.getByRole('button', { name: '↻ 다시 확인' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await shot('02-unverified-recovers-consent');
      await page.locator('.modal-body').getByRole('button', { name: '이어서 설치' }).click();
      await waitIdle(page);
      calls = await callsOf(page);
      expectChainOrder(calls, bugs, 'unverified-recover');
      if ((await page.locator('.devtool-panel-skipped').count()) !== 0) bugs.push({ severity: 'Major', symptom: '[unverified] 설치 확인 뒤에도 건너뜀 패널이 남음', file: 'src/views/devTools.ts:renderDevToolItem' });
    }),

    // R14 RV-001: macOS Homebrew Node가 `--version`에 응답하지 않는 모양(installed=false, path 있음, actionKind=run).
    // 배치가 Node를 "설치"로 적거나, 이번 배치에서 확인될 보장이 없는 의존 도구를 "설치합니다" 목록에 올리면 안 된다.
    mk('chain-mac-node-unverified-run', { platform: 'mac', phase: 'nodeUnverified' }, async (page, { shot, bugs }) => {
      const FILE_B = 'src/devToolsOrder.ts:planBatchInstall / src/views/devTools.ts:handleUpdateAll';
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      const text = (await modalText(page)) ?? '';
      await shot('01-mac-unverified-batch-consent');
      if (/개 도구를 설치합니다/.test(text)) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] 설치 대상이 없는데 동의창이 "설치합니다"로 적음: ${text}`, file: FILE_B });
      if (!text.includes('(버전 미확인): Node.js. ') || !text.includes('업데이트를 시도합니다') || !text.includes('파일이 발견됐지만 버전을 확인하지 못해')) {
        bugs.push({ severity: 'Critical', symptom: `[mac-unverified] Node.js가 "업데이트(버전 미확인)"로 적히지 않음: ${text}`, file: FILE_B });
      }
      // 동의 대상(": " 뒤)은 Node.js 하나뿐이고, Claude/pnpm/Wrangler는 "이번에는 설치하지 않으며" 문장에만 나온다.
      const listed = text.split('(버전 미확인): ')[1] ?? '';
      if (!/^Node\.js\. /.test(listed)) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] 동의 대상 목록이 Node.js 하나가 아님: ${listed}`, file: FILE_B });
      if (!text.includes('이번에는 설치하지 않으며')) bugs.push({ severity: 'Major', symptom: `[mac-unverified] 의존 도구를 이번에 설치하지 않는다는 고지가 없음: ${text}`, file: FILE_B });
      let calls = await callsOf(page);
      if (execOn(calls, ['node', ...DEPS]).length > 0 || anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] 동의 전에 커맨드가 호출됨: ${JSON.stringify(calls)}`, file: FILE_B });
      await page.locator('.modal-body').getByRole('button', { name: '확인' }).click();
      // Node 업데이트가 버전 확인(updated)되면 이어서 설치 동의창이 이름을 적어 다시 묻는다. 그 전에 의존 도구 호출은 0건.
      await page.waitForSelector('.modal-body', { timeout: 6000 });
      calls = await callsOf(page);
      const chainText = (await modalText(page)) ?? '';
      await shot('02-mac-unverified-then-chain-consent');
      if (!chainText.includes('Node.js 설치가 확인되었습니다') || !chainText.includes('Claude Code') || !chainText.includes('pnpm') || !chainText.includes('Wrangler')) {
        bugs.push({ severity: 'Critical', symptom: `[mac-unverified] Node 확인 뒤 이어서 설치 동의창이 대상 이름을 적지 않음: ${chainText}`, file: 'src/views/devTools.ts:handleUpdateAll(handled)' });
      }
      if (anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] 배치 중 의존 도구에 커맨드가 호출됨: ${JSON.stringify(anyOn(calls, DEPS))}`, file: FILE_B });
      if (!calls.includes('install_dev_tool:node') || execOn(calls, ['node']).length !== 1) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] Node 실행이 정확히 1회가 아님: ${JSON.stringify(calls)}`, file: FILE_B });
      await page.locator('.modal-body').getByRole('button', { name: '이어서 설치' }).click();
      await waitIdle(page);
      calls = await callsOf(page);
      expectChainOrder(calls, bugs, 'mac-unverified');
      if (execOn(calls, ['node']).length !== 1) bugs.push({ severity: 'Critical', symptom: `[mac-unverified] Node 중복 실행: ${JSON.stringify(execOn(calls, ['node']))}`, file: FILE_B });
    }),

    // 같은 모양에서 업데이트 뒤에도 버전이 확인되지 않으면(unknownAfter) 의존 도구는 끝내 실행되지 않고 사유가 남는다.
    mk('chain-mac-node-unverified-still-unknown', { platform: 'mac', phase: 'nodeUnverified', nodeOutcome: 'unknownAfter' }, async (page, { shot, bugs }) => {
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await page.locator('.modal-body').getByRole('button', { name: '확인' }).click();
      await waitIdle(page);
      await page.waitForTimeout(300);
      await shot('01-mac-unverified-still-unknown');
      if ((await page.locator('.modal-body').count()) !== 0) bugs.push({ severity: 'Critical', symptom: '[mac-unverified-unknown] 버전이 끝내 확인되지 않았는데 이어서 설치 확인창이 뜸', file: 'src/views/devTools.ts:offerInstallChain' });
      const calls = await callsOf(page);
      if (anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac-unverified-unknown] 의존 도구에 커맨드가 호출됨: ${JSON.stringify(anyOn(calls, DEPS))}`, file: 'src/views/devTools.ts:handleUpdateAll' });
      for (const id of DEPS) {
        const panel = page.locator(`#devtool-row-${id}`).locator('xpath=ancestor::div[contains(@class,"devtool-item")]').locator('.devtool-panel-skipped');
        const t = (await panel.count()) > 0 ? ((await panel.first().textContent()) ?? '') : null;
        if (t === null || !t.includes('파일이 발견됐지만')) bugs.push({ severity: 'Major', symptom: `[mac-unverified-unknown] ${id}: 건너뜀 사유가 "파일은 있으나 버전 미확인"이 아님: ${t}`, file: 'src/devToolsOrder.ts:describeUnmetDependencies' });
      }
    }),

    // 같은 모양에서 동의를 거절하면 아무 것도 실행되지 않는다(Node도, 의존 도구도).
    mk('chain-mac-node-unverified-declined', { platform: 'mac', phase: 'nodeUnverified' }, async (page, { bugs }) => {
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await page.locator('.modal-body').getByRole('button', { name: '취소' }).click();
      await waitIdle(page);
      const calls = await callsOf(page);
      if (execOn(calls, ['node', ...DEPS]).length > 0) bugs.push({ severity: 'Critical', symptom: `[mac-unverified-declined] 거절했는데 설치/업데이트가 실행됨: ${JSON.stringify(calls)}`, file: 'src/views/devTools.ts:handleUpdateAll' });
    }),

    // 배치 경로: 전체 업데이트로 Node(개별 확인 대기) → 개별 확인 설치 → 이어서 설치. 배치 동의창은 Node 한 개만 올린다.
    mk('chain-win-batch-then-individual', { platform: 'win' }, async (page, { shot, bugs }) => {
      await page.getByRole('button', { name: '전체 업데이트' }).click();
      await page.waitForSelector('.modal-body');
      const t = (await modalText(page)) ?? '';
      if (!t.includes('1개 도구를 설치합니다: Node.js') || /Claude|pnpm|Wrangler/.test(t)) bugs.push({ severity: 'Critical', symptom: `[batch] 배치 동의창이 실행 대상과 다름: ${t}`, file: 'src/views/devTools.ts:handleUpdateAll' });
      await shot('01-batch-summary-node-only');
      await page.locator('.modal-body').getByRole('button', { name: '확인' }).click();
      await waitIdle(page);
      // Node는 winget이라 previewReliable=false → 개별 확인 대기(배치는 실행하지 않음). 이 시점에 의존 도구 커맨드 0건.
      let calls = await callsOf(page);
      if (execOn(calls, ['node', ...DEPS]).some((c) => c !== 'update_dev_tool:gh') || anyOn(calls, DEPS).length > 0) bugs.push({ severity: 'Critical', symptom: `[batch] 개별 확인 전에 Node/의존 도구가 실행됨: ${JSON.stringify(calls)}`, file: 'src/views/devTools.ts:handleUpdateAll' });
      await page.locator('.devtool-panel-actions').getByRole('button', { name: '실행' }).click();
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await page.locator('.modal-body').getByRole('button', { name: '이어서 설치' }).click();
      await waitIdle(page);
      await shot('02-batch-then-chain-done');
      calls = await callsOf(page);
      expectChainOrder(calls, bugs, 'batch');
      if (execOn(calls, ['node']).length !== 1) bugs.push({ severity: 'Critical', symptom: `[batch] Node 실행 횟수가 1이 아님: ${JSON.stringify(execOn(calls, ['node']))}`, file: FILE_C });
    }),
  ];
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
      // R14 P2-9: 사용자가 Node를 직접 설치한 뒤 재조회하면 이어서 설치 확인창이 뜬다. 이 시나리오의 관심사는
      // 건너뜀 패널 소거이므로 확인창은 거절한다(거절하면 아무것도 실행되지 않는다).
      await page.waitForSelector('.modal-body', { timeout: 4000 });
      await page.locator('.modal-body').getByRole('button', { name: '취소' }).click();
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

    ...chainScenarios(base),

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
            ...(() => {
              const c = (id, o = {}) => ({ id, name: id.toUpperCase(), installed: false, actionKind: 'run', installDependsOn: [], path: null, ...o });
              const before = [c('claude', { installDependsOn: ['node'], actionKind: 'manual' }), c('node'), c('pnpm', { installDependsOn: ['node'], actionKind: 'manual' }), c('gh', { installed: true })];
              const after = [c('claude', { installDependsOn: ['node'] }), c('node', { installed: true }), c('pnpm', { installDependsOn: ['node'] }), c('gh', { installed: true })];
              const ids = (p) => (p ? { causes: p.causes.map((t) => t.id), candidates: p.candidates.map((t) => t.id), stuck: p.stuck.map((t) => t.id) } : null);
              return {
                chainNew: ids(m.planInstallChain(before, after, new Set())),
                chainNoPrev: ids(m.planInstallChain([], after, new Set())),
                chainNoChange: ids(m.planInstallChain(after, after, new Set())),
                chainNodeStillMissing: ids(m.planInstallChain(before, before, new Set())),
                chainHandled: ids(m.planInstallChain(before, after, new Set(['claude', 'pnpm']))),
                chainPartialHandled: ids(m.planInstallChain(before, after, new Set(['claude']))),
                chainStuck: ids(m.planInstallChain(before, [after[0], after[1], { ...after[2], actionKind: 'manual' }, after[3]], new Set())),
                chainTwoDeps: ids(m.planInstallChain(
                  [c('x', { installDependsOn: ['node', 'git'], actionKind: 'manual' }), c('node'), c('git')],
                  [c('x', { installDependsOn: ['node', 'git'] }), c('node', { installed: true }), c('git')], new Set())),
                ...(() => {
                  const mk2 = (id, o = {}) => ({ id, name: id.toUpperCase(), installed: false, actionKind: 'run', installDependsOn: [], path: null, ...o });
                  const shape = (b) => ({ update: b.updateTargets.map((t) => t.id), install: b.installTargets.map((t) => t.id), unverified: b.unverifiedTargets.map((t) => t.id), blocked: b.blocked.map((t) => t.id), deferred: b.deferredByUnverified.map((t) => t.id) });
                  const deps = (node) => [node, mk2('claude', { installDependsOn: ['node'] }), mk2('gh', { installed: true, path: '/gh' })];
                  return {
                    batchUnverifiedRun: shape(m.planBatchInstall(deps(mk2('node', { path: '/opt/homebrew/bin/node' })))),
                    batchUnverifiedManual: shape(m.planBatchInstall(deps(mk2('node', { path: 'C:/node.exe', actionKind: 'manual' })))),
                    batchMissingRun: shape(m.planBatchInstall(deps(mk2('node')))),
                    batchInstalled: shape(m.planBatchInstall(deps(mk2('node', { installed: true, path: '/n' })))),
                    consentInstallOnly: m.describeBatchConsent([{ name: 'A' }, { name: 'B' }], [], []),
                    consentUnverified: m.describeBatchConsent([], [{ name: 'Node.js' }], [{ name: 'Claude Code' }]),
                  };
                })(),
                unverifiedText: m.describeUnmetDependencies(['node'], [{ id: 'node', name: 'Node.js', installed: false, actionKind: 'manual', path: 'C:/node.exe' }]),
                missingText: m.describeUnmetDependencies(['node'], [{ id: 'node', name: 'Node.js', installed: false, actionKind: 'manual', path: null }]),
              };
            })(),
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
        expect('chainNew', out.chainNew, { causes: ['node'], candidates: ['claude', 'pnpm'], stuck: [] });
        expect('chainNoPrev(첫 조회는 제안 없음)', out.chainNoPrev, null);
        expect('chainNoChange', out.chainNoChange, null);
        expect('chainNodeStillMissing', out.chainNodeStillMissing, null);
        expect('chainHandled(이미 다룬 도구는 재제안 안 함)', out.chainHandled, null);
        expect('chainPartialHandled', out.chainPartialHandled, { causes: ['node'], candidates: ['pnpm'], stuck: [] });
        expect('chainStuck(run이 아니면 후보 아님)', out.chainStuck, { causes: ['node'], candidates: ['claude'], stuck: ['pnpm'] });
        expect('chainTwoDeps(다른 선행 도구 미설치면 후보 아님)', out.chainTwoDeps, { causes: ['node'], candidates: [], stuck: ['x'] });
        expect('batchUnverifiedRun(경로 있음+run: Node는 업데이트로 따로, 의존 도구 제외)', out.batchUnverifiedRun, { update: ['gh'], install: [], unverified: ['node'], blocked: ['claude'], deferred: ['claude'] });
        expect('batchUnverifiedManual(Node가 실행 대상 아님: 의존 도구 제외, 연기 아님)', out.batchUnverifiedManual, { update: ['gh'], install: [], unverified: [], blocked: ['claude'], deferred: [] });
        expect('batchMissingRun(경로 없음+run: 순수 설치)', out.batchMissingRun, { update: ['gh'], install: ['node', 'claude'], unverified: [], blocked: [], deferred: [] });
        expect('batchInstalled', out.batchInstalled, { update: ['node', 'gh'], install: ['claude'], unverified: [], blocked: [], deferred: [] });
        expect('consentInstallOnly(기존 문구 유지)', out.consentInstallOnly, '다음 2개 도구를 설치합니다: A, B. 계속할까요?');
        expect('consentUnverified', out.consentUnverified.includes('(버전 미확인): Node.js.') && !out.consentUnverified.includes('설치합니다') && out.consentUnverified.includes('Claude Code은(는)'), true);
        expect('unverifiedText', out.unverifiedText.includes('파일이 발견됐지만') && !out.unverifiedText.includes('설치가 확인되지 않아'), true);
        expect('missingText', out.missingText, '선행 도구(Node.js) 설치가 확인되지 않아 건너뜀');
      },
    },
  ];
}
