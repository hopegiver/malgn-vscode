// 개발 환경 — "설정"과는 성격이 다르다(설정은 외부 서비스 연동 자격증명, 이건 로컬
// 도구 상태). 설치 여부·버전은 Rust가 실제로 `<tool> --version`을 실행해 조회한
// 실데이터다(devToolsApi.ts). 설치/업데이트 버튼은 순수 목업 — 실제로 아무것도
// 설치하지 않는다. 목업 업데이트가 "성공"했을 때 화면에 반영할 버전은
// state.devTools.mockUpdatedVersion에만 기록한다(실제로 조회된 tool.version은
// 건드리지 않는다 — "다시 확인"을 누르면 실제 값으로 돌아온다).
import { el, showToast } from '../dom';
import { state, notifyChange } from '../state';
import { fetchDevTools } from '../devToolsApi';
import type { DevToolStatus } from '../devToolsApi';
import { MOCK_DEV_TOOL_META } from '../mockData';

export async function loadDevTools(): Promise<void> {
  state.devTools.loading = true;
  state.devTools.error = null;
  state.devTools.mockUpdatedVersion = {};
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

// 실제 설치됐지만(readonly) 목업 업데이트가 "적용된 척" 보여줄 항목 — 최신
// 버전으로 이미 올라간 것으로 판정한다.
function isUpdateAvailable(tool: DevToolStatus): boolean {
  if (!tool.installed) return false;
  if (state.devTools.mockUpdatedVersion[tool.id]) return false;
  return MOCK_DEV_TOOL_META[tool.id]?.updateAvailable ?? false;
}

function displayVersion(tool: DevToolStatus): string | null {
  return state.devTools.mockUpdatedVersion[tool.id] ?? tool.version;
}

export function renderDevToolsView(): HTMLElement {
  const updatableCount = state.devTools.items.filter(isUpdateAvailable).length;

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['개발 환경']),
      el('div', { className: 'page-subtitle' }, ['로컬에 설치된 CLI 도구 — 버전은 실제 조회값입니다']),
    ]),
    el('div', { className: 'devtool-header-actions' }, [
      ...(updatableCount > 0
        ? [
            el(
              'button',
              { className: 'btn btn-primary', onClick: () => void mockUpdateAll(), disabled: state.devTools.updatingAll },
              [state.devTools.updatingAll ? '모두 업데이트 중…' : `모두 업데이트 (${updatableCount})`]
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
    body.push(el('div', { className: 'devtool-list' }, state.devTools.items.map(renderDevToolRow)));
  }

  return el('div', {}, [header, ...body]);
}

function renderDevToolRow(tool: DevToolStatus): HTMLElement {
  const meta = MOCK_DEV_TOOL_META[tool.id];
  const updating = state.devTools.updating[tool.id] ?? false;
  const updateAvailable = isUpdateAvailable(tool);

  let actionBtn: HTMLElement;
  if (updating) {
    actionBtn = el('button', { className: 'btn', disabled: true }, [tool.installed ? '업데이트 중…' : '설치 중…']);
  } else if (!tool.installed) {
    actionBtn = el('button', { className: 'btn btn-primary', onClick: () => mockInstallOrUpdate(tool, true) }, ['설치 (목업)']);
  } else if (updateAvailable) {
    actionBtn = el('button', { className: 'btn btn-primary', onClick: () => mockInstallOrUpdate(tool, false) }, [`업데이트 → v${meta?.latestVersion} (목업)`]);
  } else {
    actionBtn = el('button', { className: 'btn', disabled: true }, ['최신 버전']);
  }

  const statusBadge = tool.installed
    ? el('span', { className: 'badge badge-active' }, [displayVersion(tool) ?? '설치됨'])
    : el('span', { className: 'badge badge-archived' }, ['설치 안 됨']);

  return el('div', { className: 'devtool-row' }, [
    el('div', { className: 'devtool-main' }, [el('div', { className: 'devtool-name' }, [tool.name]), el('div', { className: 'devtool-binary' }, [tool.id])]),
    statusBadge,
    actionBtn,
  ]);
}

// 목업 — 실제로 설치/업데이트 명령을 실행하지 않는다. 로딩 흉내만 낸다.
function mockInstallOrUpdate(tool: DevToolStatus, isInstall: boolean): void {
  state.devTools.updating[tool.id] = true;
  notifyChange();
  setTimeout(() => {
    state.devTools.updating[tool.id] = false;
    const meta = MOCK_DEV_TOOL_META[tool.id];
    if (meta) state.devTools.mockUpdatedVersion[tool.id] = meta.latestVersion;
    showToast(`${tool.name} ${isInstall ? '설치' : '업데이트'} 완료 (목업 — 실제로 변경되지 않았습니다)`);
    notifyChange();
  }, 900);
}

// "모두 업데이트" — 업데이트 가능한(설치돼 있고 구버전인) 항목만 한 번에 처리한다.
async function mockUpdateAll(): Promise<void> {
  const targets = state.devTools.items.filter(isUpdateAvailable);
  if (targets.length === 0) return;

  state.devTools.updatingAll = true;
  for (const t of targets) state.devTools.updating[t.id] = true;
  notifyChange();

  await new Promise((resolve) => setTimeout(resolve, 900));

  for (const t of targets) {
    const meta = MOCK_DEV_TOOL_META[t.id];
    if (meta) state.devTools.mockUpdatedVersion[t.id] = meta.latestVersion;
    state.devTools.updating[t.id] = false;
  }
  state.devTools.updatingAll = false;
  showToast(`${targets.length}개 도구 업데이트 완료 (목업 — 실제로 변경되지 않았습니다)`);
  notifyChange();
}
