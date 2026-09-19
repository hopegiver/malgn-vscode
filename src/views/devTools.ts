// 개발 환경 — "설정"과는 성격이 다르다(설정은 외부 서비스 연동 자격증명, 이건 로컬
// 도구 상태). 설치 여부·버전·설치방식은 Rust가 실제로 조회한 실데이터다
// (devToolsApi.ts). actionKind가 도구별로 무엇을 할 수 있는지 정한다:
//   - "run"    : 이 앱이 실제로 설치/업데이트 명령을 실행할 수 있다. 개별 실행은
//                preview_dev_tool_update로 무엇이 바뀔지 화면에 보여주고 사용자
//                확인을 받은 뒤(plan_id 일치 확인)에만 그 계획으로 실제 명령을
//                실행한다. "전체 업데이트"는 이미 설치된 도구(installed===true)만
//                버튼 클릭 자체를 일괄 동의로 보고 각 도구의 미리보기를 화면
//                노출·개별 확인 없이 순차 실행하되, 미리보기를 신뢰할 수
//                없거나(previewReliable === false) 영향 대상이 1개보다 많은
//                항목은 배치에서 제외하고 개별 확인 대기로 남긴다. 아직 설치되지
//                않은 도구는 "전체 업데이트"의 동의 범위 밖이라 "N개를 설치합니다"
//                요약 확인 1회를 받은 경우에만 같은 배치에 포함된다(M1,
//                handleUpdateAll).
//   - "manual" : 이 앱이 별도 프로세스로 실행하지 않는 경우(예: macOS의 Git처럼
//                시스템이 소유해 이 앱이 대신 설치/업데이트할 수 없는 도구, 또는
//                필요한 실행기(winget 등)를 이 머신에서 찾지 못한 경우). "안내
//                보기"로 안내문과 복사 가능한 명령을 보여주며, "터미널에서 실행"은
//                어떤 명령이 실행될지 먼저 보여주고 확인을 받은 뒤에만 터미널
//                창에서 그 명령을 실행한다(M2). 같은 도구도 install/update 경로가
//                서로 다른 actionKind를 낼 수 있다(예: Windows의 Node/Git은
//                미설치 상태에서는 winget으로 "run", 이미 설치돼 있으면 업데이트는
//                여전히 "manual" — hub decisionId 01m2wse823xcszvn7km3vap0qs).
//   - "none"   : 설치 경로 자체를 찾지 못해 아무 것도 할 수 없다. 버튼은 비활성이다.
// 실행 결과는 성공/실패 2상태가 아니라 updated/alreadyLatest/unknownAfter/failed/
// timedOut/notSupported 3+ 상태로 구분해 보여준다 — exit code만으로 "성공"을
// 주장하지 않는다(verified=false는 명령이 성공을 보고했지만 버전 재조회로 확인하지
// 못했다는 뜻이며, 성공으로 표시하지 않는다).
// required(DevToolStatus.required)는 이 도구 없이는 앱/업무가 돌아가지 않는
// 전제조건인지를 나타낸다(현재 Node.js/Git) — 정본은 백엔드 하나뿐이고(위
// devToolsApi.ts 주석 참고), 이 화면은 그 값을 "필수" 배지로만 반영한다.
import { el, showToast, confirmDialog } from '../dom';
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
    state.devTools.error = err instanceof Error ? err.message : '개발 환경 정보를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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
    case 'updated': {
      // R3-03: 패널(renderResultPanel)과 같은 규칙(versionBefore 유무)으로
      // 신규 설치/업데이트를 구분한다 — 이전엔 outcome만 보고 무조건
      // "업데이트되었습니다"라고 말해 신규 설치 성공 때도 동사가 틀렸다.
      const fresh = !result.versionBefore;
      showToast(`${tool.name}: v${result.normalizedAfter ?? result.versionAfter ?? '?'}(으)로 ${fresh ? '설치' : '업데이트'}되었습니다`);
      break;
    }
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

// "전체 업데이트" — actionKind가 "run"인 도구를 순차(await 직렬)로 처리한다(백엔드가
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
//
// M1(review-devtools-install-2026-09-10.md): "전체 업데이트"가 동의하는 범위는
// "있는 것들을 최신화"이지 "없는 것을 새로 설치"가 아니다. 이번 라운드 전에는
// actionKind==='run'인 미설치 도구(gh/Claude/Wrangler)까지 targets에 들어가
// 개별 확인 없이 실설치됐다 — preview_args가 없는 설치 프리뷰는 항상
// previewReliable===true + affected.length===1이라 위 두 제외 조건을 둘 다
// 통과했기 때문이다. 그래서 "업데이트"(설치돼 있는 도구)와 "설치"(아직 없는
// 도구)를 분리한다: 업데이트는 기존처럼 개별 확인 없이 배치 처리하고, 설치는
// "N개를 설치합니다" 요약 확인 1회를 받은 뒤에만 같은 배치에 포함시킨다 —
// 개별 확인 0회(이전 버그)와 도구 수만큼의 확인(N회) 사이에서, 신규 머신
// 셋업 편의와 비가역 실행의 동의 범위를 절충한 값이다.
async function handleUpdateAll(): Promise<void> {
  const runnable = state.devTools.items.filter((t) => t.actionKind === 'run');
  const updateTargets = runnable.filter((t) => t.installed);
  const installTargets = runnable.filter((t) => !t.installed);

  let confirmedInstallTargets: DevToolStatus[] = [];
  if (installTargets.length > 0) {
    const names = installTargets.map((t) => t.name).join(', ');
    const proceed = await confirmDialog(`다음 ${installTargets.length}개 도구를 설치합니다: ${names}. 계속할까요?`);
    if (proceed) {
      confirmedInstallTargets = installTargets;
    }
  }

  const targets = [...updateTargets, ...confirmedInstallTargets];
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
      showToast(`${tool.name}: 미리보기로 영향 범위를 확인할 수 없습니다 — 개별 확인이 필요합니다`);
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
      el('div', { className: 'page-subtitle' }, ['로컬에 설치된 CLI 도구 — 설치/업데이트도 실제로 실행됩니다']),
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
  } else if (state.devTools.items.length === 0) {
    // 백엔드는 고정 6종을 항상 돌려주므로(check_dev_tools) 빈 목록은 정상 상태가
    // 아니라 조회 자체가 이상했다는 신호다 — "0/7" 같은 값을 사실처럼 보여주지
    // 않는다.
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['도구 상태를 확인하지 못했습니다']),
        el('div', { className: 'state-block-desc' }, ['"↻ 다시 확인"을 눌러 주세요.']),
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

  // 배지 클래스는 기존 두 개(badge-active/badge-archived)만 재사용한다(styles.css
  // 수정 금지) — 미설치 필수 도구를 눈에 띄게 하는 강조는 appLinks.ts의 기존
  // 패턴(인라인 style.color = 'var(--color-danger)')을 그대로 따른다.
  const badges = [statusBadge];
  if (tool.required) {
    const requiredBadge = el('span', { className: 'badge badge-archived' }, ['필수']);
    if (!tool.installed) requiredBadge.style.color = 'var(--color-danger)';
    badges.push(requiredBadge);
  }

  const row = el('div', { className: 'devtool-row' }, [
    el('div', { className: 'devtool-main' }, [el('div', { className: 'devtool-name' }, [tool.name]), metaLine]),
    ...badges,
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
    // U-17: 이 버튼의 실제 동작은 dry-run 미리보기를 여는 것뿐이다(실행은 미리보기
    // 패널의 "실행" 버튼만 한다). 이미 설치된 도구는 라벨을 "업데이트 확인"으로
    // 바꾸고 클래스를 보조로 낮춰, 진짜 비가역 실행 버튼("실행")만 이 화면에서
    // primary로 남게 한다.
    const label = tool.installed ? '업데이트 확인' : '설치';
    const className = tool.installed ? 'btn' : 'btn btn-primary';
    return el('button', { className, onClick: () => void handleRequestPreview(tool) }, [label]);
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
    // 릴리즈 전 필수 수정(A): winget install/upgrade는 UAC 승인 창을 다른
    // 모니터나 뒤쪽 창으로 띄울 수 있다 — 사용자가 원인을 모른 채 최대
    // 600초를 기다리는 것을 막기 위해 실행 중 패널에도 안내를 남긴다.
    el('span', { className: 'devtool-panel-hint' }, ['관리자 권한 승인 창이 뜨면 승인해주세요']),
  ]);
}

function renderPreviewPanel(tool: DevToolStatus, preview: DevToolPreview): HTMLElement {
  // V-02: affected.length !== 1은 0건(=영향 범위를 전혀 모른다)일 때도 참이 되어
  // "외 0개 항목이 함께 바뀔 수 있습니다"라는, 가장 정보가 없는 상태를 가장
  // 구체적인 것처럼 말하는 문구가 나갔다. "여럿이 함께 바뀐다"와 "범위를 모른다"는
  // 서로 다른 경고이므로 나눈다.
  const warnMultiple = preview.affected.length > 1;
  const warnUnknownAffected = preview.affected.length === 0;
  const children: HTMLElement[] = [
    // V-07: 도구가 6개 늘어선 목록에서 이 패널이 어느 도구 것인지 화면 어디에도
    // 없었다 — 제목에 도구명을 넣는다.
    el('div', { className: 'devtool-panel-title' }, [`실행 전 확인 — ${tool.name}`]),
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
  } else if (warnUnknownAffected) {
    children.push(
      el('div', { className: 'devtool-panel devtool-panel-warn' }, [
        '⚠ 영향 범위를 확인하지 못했습니다 — 무엇이 바뀔지 알 수 없습니다. 신중히 확인 후 실행하세요.',
      ])
    );
  }

  if (!preview.previewReliable) {
    children.push(
      el('div', { className: 'devtool-panel devtool-panel-warn' }, [
        '⚠ 미리보기로 영향 범위를 확인할 수 없어 위 "영향받는 항목"이 실제 범위를 반영하지 못할 수 있습니다. 신중히 확인 후 실행하세요.',
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
    // V-07: 미리보기 패널과 마찬가지로 결과 패널 제목에도 도구명을 넣는다.
    el('div', { className: 'devtool-panel-title' }, [`${tool.name} — ${summary[result.outcome]}`]),
    el('div', { className: 'devtool-panel-verified' }, [result.verified ? '확인됨(verified)' : '확인되지 않음(unverified)']),
  ];

  if (result.versionBefore || result.versionAfter) {
    // R3-04: 신규 설치(versionBefore 없음)에서는 "? → 2.0.68"처럼 물음표를
    // 노출하지 않고 "신규 설치: 2.0.68"로 말한다.
    const notes = !result.versionBefore && result.versionAfter
      ? `신규 설치: ${result.normalizedAfter ?? result.versionAfter}`
      : `${result.versionBefore ?? '?'} → ${result.versionAfter ?? '?'}`;
    children.push(el('div', { className: 'devtool-panel-notes' }, [notes]));
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
