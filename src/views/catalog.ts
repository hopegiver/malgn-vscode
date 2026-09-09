// 카탈로그 — 설치된 플러그인은 실제 로컬 데이터(~/.claude/plugins/installed_plugins.json,
// scope "user"만 + 각 installPath 실물 agents/skills/knowledge). 버전 관리 단위는
// 플러그인이다 — 개별 에이전트/스킬/지식 항목은 읽기 전용 목록이고 자체 버전이
// 없다. "최신 버전"은 네트워크 조회가 필요해 다루지 않는다(설치된 버전만 표시).
//
// "업데이트" 버튼은 사용자가 명시적으로 승인해 실제로 `claude plugin update`를
// 실행한다 — 인자는 항상 이미 읽어둔 신뢰할 수 있는 plugin id만 쓴다(자유 입력
// 필드 없음). 성공해도 Claude Code 재시작 전까지는 반영되지 않는다 — 반드시
// 안내한다. "자동 업데이트" 토글만 순수 목업(로컬 UI 상태).
import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import { fetchInstalledPlugins, fetchKnownMarketplaces, updatePlugin } from '../catalogApi';
import type { InstalledPlugin, CatalogEntryItem, CommandResult } from '../catalogApi';

// 성공 결과 노트는 확인 후 후속 조치가 없으므로 잠시 보여준 뒤 자동으로 치운다.
// 실패 노트는 메시지를 계속 봐야 하니 그대로 남긴다.
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
    state.catalog.lastResult[id] = null;
    notifyChange();
  }, RESULT_AUTO_CLEAR_MS);
}

export async function loadCatalog(): Promise<void> {
  state.catalog.loading = true;
  state.catalog.error = null;
  notifyChange();
  try {
    const plugins = await fetchInstalledPlugins();
    state.catalog.plugins = plugins;
    state.catalog.loaded = true;
    for (const p of plugins) {
      if (!(p.id in state.catalog.autoUpdate)) state.catalog.autoUpdate[p.id] = true;
    }
  } catch (err) {
    state.catalog.error = err instanceof Error ? err.message : '카탈로그를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.catalog.loading = false;
    notifyChange();
  }
}

export async function loadMarketplaces(): Promise<void> {
  state.marketplaces.loading = true;
  state.marketplaces.error = null;
  notifyChange();
  try {
    state.marketplaces.items = await fetchKnownMarketplaces();
    state.marketplaces.loaded = true;
  } catch (err) {
    state.marketplaces.error = err instanceof Error ? err.message : '마켓플레이스 정보를 불러오지 못했습니다.';
  } finally {
    state.marketplaces.loading = false;
    notifyChange();
  }
}

async function handleUpdatePlugin(plugin: InstalledPlugin): Promise<void> {
  state.catalog.updating[plugin.id] = true;
  state.catalog.lastResult[plugin.id] = null;
  clearResultClearTimer(plugin.id);
  notifyChange();
  try {
    const result = await updatePlugin(plugin.id);
    state.catalog.lastResult[plugin.id] = result;
    showToast(
      result.success
        ? `${plugin.displayName ?? plugin.name} 업데이트 완료 — 적용하려면 Claude Code를 재시작하세요`
        : `${plugin.displayName ?? plugin.name} 업데이트 실패: ${result.message}`
    );
    if (result.success) {
      scheduleResultClear(plugin.id);
      await loadCatalog();
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    state.catalog.lastResult[plugin.id] = { success: false, message };
    showToast(`${plugin.displayName ?? plugin.name} 업데이트 실패: ${message}`);
  } finally {
    state.catalog.updating[plugin.id] = false;
    notifyChange();
  }
}

async function handleUpdateAll(): Promise<void> {
  const targets = state.catalog.plugins;
  if (targets.length === 0) return;

  state.catalog.updatingAll = true;
  for (const p of targets) state.catalog.updating[p.id] = true;
  notifyChange();

  let successCount = 0;
  for (const p of targets) {
    try {
      const result = await updatePlugin(p.id);
      state.catalog.lastResult[p.id] = result;
      if (result.success) {
        successCount += 1;
        scheduleResultClear(p.id);
      }
    } catch (err) {
      state.catalog.lastResult[p.id] = { success: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      state.catalog.updating[p.id] = false;
      notifyChange();
    }
  }

  if (successCount > 0) await loadCatalog();
  state.catalog.updatingAll = false;
  showToast(`${successCount}/${targets.length}개 플러그인 업데이트 완료 — 적용하려면 Claude Code를 재시작하세요`);
  notifyChange();
}

export function renderCatalogView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['카탈로그']),
      el('div', { className: 'page-subtitle' }, [
        state.catalog.loaded ? `설치된 플러그인 ${state.catalog.plugins.length}개 (user scope) — 버전 관리 단위는 플러그인입니다` : '실제 로컬 데이터',
      ]),
    ]),
    el('div', { className: 'devtool-header-actions' }, [
      ...(state.catalog.plugins.length > 1
        ? [
            el(
              'button',
              { className: 'btn btn-primary', onClick: () => void handleUpdateAll(), disabled: state.catalog.updatingAll },
              [state.catalog.updatingAll ? '모두 업데이트 중…' : `모두 업데이트 (${state.catalog.plugins.length})`]
            ),
          ]
        : []),
      el('button', { className: 'btn', onClick: () => void loadCatalog(), disabled: state.catalog.loading }, [
        state.catalog.loading ? '새로고침 중…' : '↻ 새로고침',
      ]),
    ]),
  ]);

  const body: HTMLElement[] = [];

  if (state.catalog.loading && !state.catalog.loaded) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]));
  } else if (state.catalog.error) {
    body.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${state.catalog.error}`]),
        el('button', { className: 'btn', onClick: () => void loadCatalog() }, ['다시 시도']),
      ])
    );
  } else if (state.catalog.plugins.length === 0) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['설치된 플러그인이 없습니다']),
        el('div', { className: 'state-block-desc' }, ['~/.claude/plugins/installed_plugins.json에 user scope 항목이 없습니다.']),
      ])
    );
  } else {
    body.push(el('div', { className: 'plugin-list' }, state.catalog.plugins.map(renderPluginCard)));
  }

  return el('div', {}, [header, ...body]);
}

function renderPluginCard(plugin: InstalledPlugin): HTMLElement {
  const autoUpdate = state.catalog.autoUpdate[plugin.id] ?? true;
  const updating = state.catalog.updating[plugin.id] ?? false;
  const lastResult = state.catalog.lastResult[plugin.id];

  const updateBtn = el(
    'button',
    {
      className: 'btn btn-primary',
      disabled: updating,
      onClick: () => void handleUpdatePlugin(plugin),
    },
    [updating ? '업데이트 중…' : '업데이트']
  );

  const head = el('div', { className: 'plugin-card-head' }, [
    el('div', {}, [
      el('div', { className: 'plugin-card-name' }, [plugin.displayName ?? plugin.name]),
      el('div', { className: 'plugin-card-version' }, [`v${plugin.version}`]),
      ...(plugin.description ? [el('div', { className: 'plugin-card-desc' }, [plugin.description])] : []),
    ]),
    el('div', { className: 'plugin-card-actions' }, [
      el('span', { className: 'auto-update-label' }, ['자동 업데이트']),
      toggleSwitch(autoUpdate, () => {
        state.catalog.autoUpdate[plugin.id] = !autoUpdate;
        notifyChange();
      }),
      updateBtn,
    ]),
  ]);

  const sections = el('div', { className: 'plugin-card-sections' }, [
    pluginSection('에이전트', plugin.agents),
    pluginSection('스킬', plugin.skills),
    pluginSection('지식(Knowledge)', plugin.knowledge),
  ]);

  const children = [head, ...(lastResult ? [renderUpdateResultNote(lastResult)] : []), sections];

  return el('div', { className: 'plugin-card' }, children);
}

function renderUpdateResultNote(result: CommandResult): HTMLElement {
  return el('div', { className: `plugin-update-note ${result.success ? 'ok' : 'fail'}` }, [
    result.success ? `✓ 업데이트 완료 — 적용하려면 Claude Code를 재시작하세요. (${result.message})` : `⚠ 업데이트 실패: ${result.message}`,
  ]);
}

function pluginSection(label: string, items: readonly CatalogEntryItem[]): HTMLElement {
  return el('div', { className: 'plugin-section' }, [
    el('div', { className: 'plugin-section-label' }, [`${label} ${items.length}개`]),
    el(
      'div',
      { className: 'plugin-entry-list' },
      items.map((item) =>
        el('div', { className: 'plugin-entry-row' }, [
          el('span', { className: 'plugin-entry-name' }, [item.name]),
          ...(item.description ? [el('span', { className: 'plugin-entry-desc' }, [item.description])] : []),
        ])
      )
    ),
  ]);
}
