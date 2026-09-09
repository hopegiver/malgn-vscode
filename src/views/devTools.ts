// 개발 환경 — "설정"과는 성격이 다르다(설정은 외부 서비스 연동 자격증명, 이건 로컬
// 도구 상태). 설치 여부·버전·설치방식은 Rust가 실제로 조회한 실데이터다
// (devToolsApi.ts). actionKind가 도구별로 무엇을 할 수 있는지 정한다:
//   - "run"    : 이 앱이 실제로 설치/업데이트 명령을 실행할 수 있다. 개별 실행은
//                preview_dev_tool_update로 무엇이 바뀔지 화면에 보여주고 사용자
//                확인을 받은 뒤(plan_id 일치 확인)에만 그 계획으로 실제 명령을
//                실행한다. "전체 업데이트"는 버튼 클릭 자체를 일괄 동의로 보고
//                각 도구의 미리보기를 화면 노출·개별 확인 없이 순차 실행하되,
//                미리보기를 신뢰할 수 없거나(previewReliable === false) 영향
//                대상이 1개보다 많은 항목은 배치에서 제외하고 개별 확인 대기로
//                남긴다(handleUpdateAll).
//   - "manual" : 이 앱이 별도 프로세스로 실행하지 않는다(예: git은 macOS 시스템
//                도구). "안내 보기"로 안내문과 복사 가능한 명령을 보여주며,
//                "터미널에서 실행"은 어떤 명령이 실행될지 먼저 보여주고 확인을
//                받은 뒤에만 터미널 창에서 그 명령을 실행한다(M2).
//   - "none"   : 설치 경로 자체를 찾지 못해 아무 것도 할 수 없다. 버튼은 비활성이다.
// 실행 결과는 성공/실패 2상태가 아니라 updated/alreadyLatest/unknownAfter/failed/
// timedOut/notSupported 3+ 상태로 구분해 보여준다 — exit code만으로 "성공"을
// 주장하지 않는다(verified=false는 명령이 성공을 보고했지만 버전 재조회로 확인하지
// 못했다는 뜻이며, 성공으로 표시하지 않는다).
import { el, showToast } from '../dom';
import { state, notifyChange } from '../state';
import { fetchDevTools, previewDevToolUpdate, updateDevTool, installDevTool, openManualInstruction } from '../devToolsApi';
import type { DevToolStatus, DevToolPreview, DevToolActionResult, TerminalLaunchResult } from '../devToolsApi';

export async function loadDevTools(): Promise<void> {
  state.devTools.loading = true;
  state.devTools.error = null;
  notifyChange();
  try {
    state.devTools.items = await fetchDevTools();
    state.devTools.loaded = true;
  } catch (err) {
    state.devTools.error = err instanceof Error ? err.message : '개발 환경 정보를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.devTools.loading = false;
    notifyChange();
  }
}

// ---------------- 실행 경과 시간 타이머 ----------------
// 상태(state.ts)에는 초 단위 값만 두고, 인터벌 핸들 자체는 모듈 스코프에 둔다
// (직렬화할 필요도 없고, 재렌더마다 새로 만들 이유도 없다).
const elapsedTimers: Record<string, ReturnType<typeof setInterval>> = {};

function startElapsedTimer(id: string): void {
  state.devTools.elapsedSec[id] = 0;
  stopElapsedTimer(id);
  elapsedTimers[id] = setInterval(() => {
    state.devTools.elapsedSec[id] = (state.devTools.elapsedSec[id] ?? 0) + 1;
    notifyChange();
  }, 1000);
}

function stopElapsedTimer(id: string): void {
  const timer = elapsedTimers[id];
  if (timer) {
    clearInterval(timer);
    delete elapsedTimers[id];
  }
}

// 성공(updated/alreadyLatest) 결과 패널은 확인 후 액션이 필요 없으므로 잠시
// 보여준 뒤 자동으로 치운다. 실패/timedOut/unknownAfter는 로그 확인 등 후속
// 조치가 필요할 수 있어 그대로 남긴다.
const resultClearTimers: Record<string, ReturnType<typeof setTimeout>> = {};
const RESULT_AUTO_CLEAR_MS = 5000;

function clearResultClearTimer(id: string): void {
  const timer = resultClearTimers[id];
  if (timer) {
    clearTimeout(timer);
    delete resultClearTimers[id];
  }
}

function scheduleResultClear(id: string): void {
  clearResultClearTimer(id);
  resultClearTimers[id] = setTimeout(() => {
    delete resultClearTimers[id];
    state.devTools.lastResult[id] = null;
    notifyChange();
  }, RESULT_AUTO_CLEAR_MS);
}

// ---------------- 액션 처리 ----------------

async function copyToClipboard(text: string, label: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    showToast(`${label}을(를) 복사했습니다`);
  } catch {
    showToast('복사에 실패했습니다 — 직접 선택해 복사해주세요');
  }
}

function notifyOutcome(tool: DevToolStatus, result: DevToolActionResult): void {
  switch (result.outcome) {
    case 'updated':
      showToast(`${tool.name}: v${result.normalizedAfter ?? result.versionAfter ?? '?'}(으)로 업데이트되었습니다`);
      break;
    case 'alreadyLatest':
      showToast(`${tool.name}: 이미 최신입니다`);
      break;
    case 'unknownAfter':
      showToast(`${tool.name}: 명령은 성공했다고 보고했으나 실제 버전을 확인하지 못했습니다`);
      break;
    case 'timedOut':
      showToast(`${tool.name}: 상태 불명 — 다시 확인이 필요합니다`);
      break;
    case 'failed':
      showToast(`${tool.name}: 실행 실패 — ${result.message}`);
      break;
    case 'notSupported':
      showToast(`${tool.name}: 이 방식으로는 실행할 수 없습니다`);
      break;
  }
}

// dry-run 프리뷰를 가져온다. "run" actionKind 도구는 설치든 업데이트든 실제 명령
// 실행 전에 반드시 이 단계를 거친다(부록 A). 미리보기 전용 커맨드가
// preview_dev_tool_update 하나뿐이라 설치 흐름에도 그대로 재사용한다.
async function handleRequestPreview(tool: DevToolStatus): Promise<void> {
  state.devTools.previewLoading[tool.id] = true;
  notifyChange();
  try {
    state.devTools.preview[tool.id] = await previewDevToolUpdate(tool.id);
  } catch (err) {
    showToast(`${tool.name}: 미리보기를 가져오지 못했습니다 — ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.devTools.previewLoading[tool.id] = false;
    notifyChange();
  }
}

function cancelPreview(toolId: string): void {
  state.devTools.preview[toolId] = null;
  notifyChange();
}

async function runPlan(tool: DevToolStatus, preview: DevToolPreview): Promise<void> {
  state.devTools.updating[tool.id] = true;
  state.devTools.lastResult[tool.id] = null;
  clearResultClearTimer(tool.id);
  startElapsedTimer(tool.id);
  notifyChange();
  try {
    const result = tool.installed ? await updateDevTool(tool.id, preview.planId) : await installDevTool(tool.id, preview.planId);
    state.devTools.lastResult[tool.id] = result;
    notifyOutcome(tool, result);
    if (result.outcome === 'updated' || result.outcome === 'alreadyLatest') {
      scheduleResultClear(tool.id);
      await loadDevTools();
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    showToast(`${tool.name}: 실행 중 오류가 발생했습니다 — ${message}`);
  } finally {
    stopElapsedTimer(tool.id);
    state.devTools.updating[tool.id] = false;
    notifyChange();
  }
}

async function handleConfirmRun(tool: DevToolStatus): Promise<void> {
  const preview = state.devTools.preview[tool.id];
  if (!preview) return;
  state.devTools.preview[tool.id] = null;
  await runPlan(tool, preview);
}

function toggleManual(toolId: string): void {
  state.devTools.manualOpen[toolId] = !state.devTools.manualOpen[toolId];
  if (!state.devTools.manualOpen[toolId]) {
    // 안내 패널을 닫으면 대기 중이던 확인 단계도 함께 정리한다 — 다시 열었을 때
    // 지난 확인 대상이 그대로 남아있지 않도록.
    manualPendingCommand[toolId] = null;
  }
  notifyChange();
}

// ---------------- Manual "터미널에서 실행" 확인 절차 ----------------
// M2: 이전에는 이 버튼을 누르는 즉시 `brew install …` 등을 실행했다(동의 없는
// 실행). Run 경로(preview → 확인 → 실행)와 동일한 2단계로 맞추되, 전역
// state(state.ts)와 프론트-백엔드 타입 계약(DevToolStatus/DevToolActionResult/
// DevToolPreview)은 바꾸지 않는다는 제약 때문에, 이 화면 전용 휘발성 상태를
// elapsedTimers와 같은 방식으로 모듈 스코프에 둔다. 백엔드
// open_manual_instruction은 execute:false일 때 아무 것도 실행하지 않고 어떤
// 명령이 실행될지만 반환한다(기존 TerminalLaunchResult 타입 그대로 재사용).
const manualConfirmLoading: Record<string, boolean> = {};
const manualPendingCommand: Record<string, TerminalLaunchResult | null> = {};
// 백엔드가 만드는 두 메시지를 구분하는 표식. "실행할 명령이 없는" 경우(예:
// copyable_command가 없는 도구)에는 확인할 것이 없으므로 바로 안내만 띄운다.
const MANUAL_PREVIEW_PREFIX = '다음 명령을 실행합니다: ';

async function handleRequestManualConfirm(tool: DevToolStatus): Promise<void> {
  manualConfirmLoading[tool.id] = true;
  notifyChange();
  try {
    const preview = await openManualInstruction(tool.id, false);
    if (preview.message.startsWith(MANUAL_PREVIEW_PREFIX)) {
      manualPendingCommand[tool.id] = preview;
    } else {
      // 실행할 명령이 없다 — 확인 단계 없이 안내 문구만 보여준다(기존 동작 유지).
      showToast(`${tool.name}: ${preview.message}`);
    }
  } catch (err) {
    showToast(`${tool.name}: 실행 준비 중 오류가 발생했습니다 — ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    manualConfirmLoading[tool.id] = false;
    notifyChange();
  }
}

function cancelManualConfirm(toolId: string): void {
  manualPendingCommand[toolId] = null;
  notifyChange();
}

async function handleConfirmManualExecute(tool: DevToolStatus): Promise<void> {
  manualPendingCommand[tool.id] = null;
  notifyChange();
  try {
    const result = await openManualInstruction(tool.id, true);
    showToast(result.opened ? `${tool.name}: ${result.message}` : `${tool.name}: 터미널을 열지 못했습니다 — ${result.message}`);
  } catch (err) {
    showToast(`${tool.name}: 실행 중 오류가 발생했습니다 — ${err instanceof Error ? err.message : String(err)}`);
  }
}

function toggleLog(toolId: string): void {
  state.devTools.logExpanded[toolId] = !state.devTools.logExpanded[toolId];
  notifyChange();
}

// "전체 업데이트" — actionKind가 "run"인 도구만 순차(await 직렬)로 처리한다(백엔드가
// 뮤텍스로 1건씩만 받으므로 동시 호출하지 않는다). 각 도구도 개별 실행과 동일하게
// 먼저 미리보기를 받는다. 미리보기 결과 영향 대상이 1개보다 많으면(brew가 의존성까지
// 올리는 경우) 자동으로 실행하지 않고 그 항목만 확인 대기 상태로 남겨 사용자가
// 개별적으로 검토·확인하게 한다.
//
// fail-open 방지(과제 4): brew dry-run 미리보기가 spawn 실패하거나 타임아웃하면
// 백엔드는 안전을 위해 affected를 길이 1(`[def.label]`)로 채워 반환하지만, 이는
// 실제로 확인된 값이 아니다 — `affected.length > 1` 검사만으로는 이 경우를 걸러내지
// 못해 무엇이 바뀔지 모르는 채로 배치가 자동 실행될 수 있었다. 그래서
// previewReliable === false인 항목도 영향 대상이 여럿인 경우와 동일하게 배치에서
// 제외하고 개별 확인 대기로 남긴다.
async function handleUpdateAll(): Promise<void> {
  const targets = state.devTools.items.filter((t) => t.actionKind === 'run');
  if (targets.length === 0) return;

  state.devTools.updatingAll = true;
  notifyChange();

  for (const tool of targets) {
    state.devTools.previewLoading[tool.id] = true;
    notifyChange();
    let preview: DevToolPreview | null = null;
    try {
      preview = await previewDevToolUpdate(tool.id);
    } catch (err) {
      showToast(`${tool.name}: 미리보기를 가져오지 못했습니다 — ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      state.devTools.previewLoading[tool.id] = false;
    }
    if (!preview) continue;

    if (!preview.previewReliable) {
      state.devTools.preview[tool.id] = preview;
      showToast(`${tool.name}: 미리보기를 확인하지 못했습니다(실패/시간 초과) — 개별 확인이 필요합니다`);
      notifyChange();
      continue;
    }

    if (preview.affected.length > 1) {
      state.devTools.preview[tool.id] = preview;
      showToast(`${tool.name}: ${preview.affected.length}개 항목이 함께 바뀝니다 — 개별 확인이 필요합니다`);
      notifyChange();
      continue;
    }

    await runPlan(tool, preview);
  }

  state.devTools.updatingAll = false;
  notifyChange();
}

// ---------------- 렌더 ----------------

export function renderDevToolsView(): HTMLElement {
  const runnableCount = state.devTools.items.filter((t) => t.actionKind === 'run').length;

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['개발 환경']),
      el('div', { className: 'page-subtitle' }, ['로컬에 설치된 CLI 도구 — 상태·버전은 실제 조회값이며, 설치/업데이트도 실제로 실행됩니다']),
    ]),
    el('div', { className: 'devtool-header-actions' }, [
      ...(runnableCount > 0
        ? [
            el(
              'button',
              { className: 'btn btn-primary', onClick: () => void handleUpdateAll(), disabled: state.devTools.updatingAll },
              [state.devTools.updatingAll ? '전체 업데이트 중…' : '전체 업데이트']
            ),
          ]
        : []),
      el('button', { className: 'btn', onClick: () => void loadDevTools(), disabled: state.devTools.loading }, [
        state.devTools.loading ? '확인 중…' : '↻ 다시 확인',
      ]),
    ]),
  ]);

  const body: HTMLElement[] = [];

  if (state.devTools.loading && !state.devTools.loaded) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['확인 중…'])]));
  } else if (state.devTools.error) {
    body.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${state.devTools.error}`]),
        el('button', { className: 'btn', onClick: () => void loadDevTools() }, ['다시 시도']),
      ])
    );
  } else {
    body.push(el('div', { className: 'devtool-list' }, state.devTools.items.map(renderDevToolItem)));
  }

  return el('div', {}, [header, ...body]);
}

function renderDevToolItem(tool: DevToolStatus): HTMLElement {
  const updating = state.devTools.updating[tool.id] ?? false;
  const previewLoading = state.devTools.previewLoading[tool.id] ?? false;
  const pendingPreview = state.devTools.preview[tool.id] ?? null;
  const lastResult = state.devTools.lastResult[tool.id] ?? null;

  const statusBadge = tool.installed
    ? el('span', { className: 'badge badge-active' }, [tool.version ?? '설치됨'])
    : el('span', { className: 'badge badge-archived' }, ['설치 안 됨']);

  const metaParts = [tool.id, tool.installMethod, tool.path].filter((v): v is string => !!v);
  const metaLine = el('div', { className: 'devtool-binary' }, [metaParts.join(' · ')]);

  const row = el('div', { className: 'devtool-row' }, [
    el('div', { className: 'devtool-main' }, [el('div', { className: 'devtool-name' }, [tool.name]), metaLine]),
    statusBadge,
    renderActionArea(tool, updating, previewLoading, !!pendingPreview),
  ]);

  const panels: HTMLElement[] = [];
  if (updating) panels.push(renderRunningPanel(tool));
  if (pendingPreview) panels.push(renderPreviewPanel(tool, pendingPreview));
  if (!updating && !pendingPreview && lastResult) panels.push(renderResultPanel(tool, lastResult));
  if (tool.actionKind === 'manual' && state.devTools.manualOpen[tool.id]) panels.push(renderManualPanel(tool));

  return el('div', { className: 'devtool-item' }, [row, ...panels]);
}

function renderActionArea(tool: DevToolStatus, updating: boolean, previewLoading: boolean, hasPendingPreview: boolean): HTMLElement {
  if (updating) {
    return el('button', { className: 'btn', disabled: true }, ['실행 중…']);
  }
  if (hasPendingPreview) {
    return el('button', { className: 'btn', disabled: true }, ['확인 대기 중']);
  }
  if (previewLoading) {
    return el('button', { className: 'btn', disabled: true }, ['미리보기 확인 중…']);
  }

  if (tool.actionKind === 'run') {
    const label = tool.installed ? '업데이트' : '설치';
    return el('button', { className: 'btn btn-primary', onClick: () => void handleRequestPreview(tool) }, [label]);
  }
  if (tool.actionKind === 'manual') {
    return el('button', { className: 'btn', onClick: () => toggleManual(tool.id) }, [
      state.devTools.manualOpen[tool.id] ? '안내 닫기' : '안내 보기',
    ]);
  }
  // actionKind === 'none'
  return el('button', { className: 'btn', disabled: true }, ['실행 불가']);
}

function renderRunningPanel(tool: DevToolStatus): HTMLElement {
  const sec = state.devTools.elapsedSec[tool.id] ?? 0;
  return el('div', { className: 'devtool-panel devtool-panel-running' }, [
    el('span', { className: 'devtool-spinner' }, []),
    el('span', {}, [`${tool.installed ? '업데이트' : '설치'} 실행 중… (${sec}초 경과)`]),
    el('span', { className: 'devtool-panel-hint' }, ['실행 중에는 창을 닫지 마세요']),
  ]);
}

function renderPreviewPanel(tool: DevToolStatus, preview: DevToolPreview): HTMLElement {
  const warnMultiple = preview.affected.length !== 1;
  const children: HTMLElement[] = [
    el('div', { className: 'devtool-panel-title' }, ['실행 전 확인']),
    el('div', { className: 'devtool-panel-command' }, [preview.commandDisplay]),
  ];

  if (preview.affected.length > 0) {
    children.push(
      el('div', { className: 'devtool-panel-label' }, ['영향받는 항목']),
      el(
        'ul',
        { className: 'devtool-affected-list' },
        preview.affected.map((a) => el('li', {}, [a]))
      )
    );
  }

  if (warnMultiple) {
    children.push(
      el('div', { className: 'devtool-panel devtool-panel-warn' }, [
        `⚠ ${tool.name} 외 ${Math.max(preview.affected.length - 1, 0)}개 항목이 함께 바뀔 수 있습니다(의존성 연쇄 업그레이드). 신중히 확인 후 실행하세요.`,
      ])
    );
  }

  if (!preview.previewReliable) {
    children.push(
      el('div', { className: 'devtool-panel devtool-panel-warn' }, [
        '⚠ 미리보기 확인에 실패했거나 시간이 초과되어 위 "영향받는 항목"이 실제 범위를 반영하지 못할 수 있습니다. 신중히 확인 후 실행하세요.',
      ])
    );
  }

  if (preview.notes) {
    children.push(el('div', { className: 'devtool-panel-notes' }, [preview.notes]));
  }

  if (!preview.willRun) {
    children.push(el('div', { className: 'devtool-panel devtool-panel-warn' }, ['이 계획은 실행 대상이 없습니다(willRun: false). 실행해도 변화가 없을 수 있습니다.']));
  }

  children.push(
    el('div', { className: 'devtool-panel-actions' }, [
      el('button', { className: 'btn btn-primary', onClick: () => void handleConfirmRun(tool) }, ['실행']),
      el('button', { className: 'btn', onClick: () => cancelPreview(tool.id) }, ['취소']),
    ])
  );

  return el('div', { className: 'devtool-panel' }, children);
}

function renderManualPanel(tool: DevToolStatus): HTMLElement {
  // Run 경로의 프리뷰 확인 패널(renderPreviewPanel)과 동일한 구조 —
  // devtool-panel-title / devtool-panel-command / devtool-panel-actions —
  // 를 재사용해 실행 전 무엇이 실행될지 보여주고 [실행]/[취소]를 받는다.
  const pending = manualPendingCommand[tool.id];
  if (pending) {
    return el('div', { className: 'devtool-panel' }, [
      el('div', { className: 'devtool-panel-title' }, ['실행 전 확인']),
      el('div', { className: 'devtool-panel-command' }, [pending.message]),
      el('div', { className: 'devtool-panel-actions' }, [
        el('button', { className: 'btn btn-primary', onClick: () => void handleConfirmManualExecute(tool) }, ['실행']),
        el('button', { className: 'btn', onClick: () => cancelManualConfirm(tool.id) }, ['취소']),
      ]),
    ]);
  }

  const confirmLoading = manualConfirmLoading[tool.id] ?? false;
  const children: HTMLElement[] = [
    el('div', { className: 'devtool-panel-title' }, ['안내']),
    el('div', {}, [tool.manualHint ?? '이 도구는 앱이 대신 실행할 수 없습니다. 터미널에서 직접 실행해주세요.']),
    el('div', { className: 'devtool-panel-actions' }, [
      el(
        'button',
        { className: 'btn', onClick: () => void handleRequestManualConfirm(tool), disabled: confirmLoading },
        [confirmLoading ? '확인 준비 중…' : '터미널에서 실행']
      ),
      ...(tool.manualHint
        ? [el('button', { className: 'btn', onClick: () => void copyToClipboard(tool.manualHint ?? '', '안내 문구') }, ['복사'])]
        : []),
    ]),
  ];
  return el('div', { className: 'devtool-panel' }, children);
}

function renderResultPanel(tool: DevToolStatus, result: DevToolActionResult): HTMLElement {
  const toneClass =
    result.outcome === 'updated' || result.outcome === 'alreadyLatest'
      ? 'devtool-panel-success'
      : result.outcome === 'failed'
        ? 'devtool-panel-danger'
        : 'devtool-panel-warn'; // unknownAfter / timedOut / notSupported

  const installedFresh = !result.versionBefore;
  const summary: Record<DevToolActionResult['outcome'], string> = {
    updated: installedFresh
      ? `✓ v${result.normalizedAfter ?? result.versionAfter ?? '?'}(으)로 설치되었습니다`
      : `✓ v${result.normalizedAfter ?? result.versionAfter ?? '?'}(으)로 업데이트되었습니다`,
    alreadyLatest: '✓ 이미 최신입니다',
    unknownAfter: '⚠ 명령은 성공했다고 보고했으나 실제 버전을 확인하지 못했습니다(확인되지 않음)',
    timedOut: '⏱ 상태 불명 — 시간이 초과되었습니다. 다시 확인해주세요',
    failed: `✕ 실행 실패: ${result.message}`,
    notSupported: '이 방식으로는 실행할 수 없습니다',
  };

  const children: HTMLElement[] = [
    el('div', { className: 'devtool-panel-title' }, [summary[result.outcome]]),
    el('div', { className: 'devtool-panel-verified' }, [result.verified ? '확인됨(verified)' : '확인되지 않음(unverified)']),
  ];

  if (result.versionBefore || result.versionAfter) {
    children.push(el('div', { className: 'devtool-panel-notes' }, [`${result.versionBefore ?? '?'} → ${result.versionAfter ?? '?'}`]));
  }

  if (!result.pathVisible && result.pathHint) {
    children.push(
      el('div', { className: 'devtool-panel devtool-panel-warn' }, [
        el('div', {}, ['설치는 됐지만 터미널에서 바로 쓸 수 없습니다. 아래 줄을 PATH 설정에 추가하세요.']),
        ...(result.pathHintTarget ? [el('div', { className: 'devtool-panel-notes' }, [`대상 파일: ${result.pathHintTarget}`])] : []),
        el('div', { className: 'devtool-panel-command' }, [result.pathHint]),
        el('div', { className: 'devtool-panel-actions' }, [
          el('button', { className: 'btn', onClick: () => void copyToClipboard(result.pathHint ?? '', 'PATH 설정') }, ['복사']),
        ]),
      ])
    );
  }

  if (result.outcome === 'failed' || result.outcome === 'timedOut') {
    const expanded = state.devTools.logExpanded[tool.id] ?? false;
    children.push(
      el('div', {}, [
        el('button', { className: 'btn', onClick: () => toggleLog(tool.id) }, [expanded ? '로그 접기' : '로그 보기']),
        ...(expanded ? [el('pre', { className: 'devtool-log' }, [result.logTail || '(로그 없음)'])] : []),
      ])
    );
  }

  return el('div', { className: `devtool-panel ${toneClass}` }, children);
}
