import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import type { SettingsTab } from '../state';
import { fetchOtelEnv } from '../otelApi';
import { loadCatalog, loadMarketplaces } from './catalog';
import { refreshMarketplaces } from '../catalogApi';
import {
  fetchGithubStatus,
  connectGithub,
  disconnectGithub,
  fetchCloudflareStatus,
  connectCloudflare,
  disconnectCloudflare,
  fetchJiraStatus,
  connectJira,
  disconnectJira,
} from '../integrationsApi';
import { navigate } from '../route';

interface FieldSpec {
  readonly id: string;
  readonly label: string;
  readonly placeholder: string;
  readonly type?: 'text' | 'password';
  readonly value?: string;
}

const TAB_META: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  { key: 'github', label: 'GitHub 설정' },
  { key: 'cloudflare', label: 'Cloudflare 설정' },
  { key: 'jira', label: 'Jira 설정' },
  { key: 'marketplace', label: '마켓플레이스 설정' },
];

export function renderSettingsView(tab: SettingsTab): HTMLElement {
  const tabsRow = el(
    'div',
    { className: 'filter-group' },
    TAB_META.map((t) => el('button', { className: `filter-btn${t.key === tab ? ' active' : ''}`, onClick: () => navigate(`#/settings/${t.key}`) }, [t.label]))
  );

  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['설정']), el('div', { className: 'page-subtitle' }, [TAB_META.find((t) => t.key === tab)?.label ?? ''])]),
  ]);

  let body: HTMLElement;
  if (tab === 'otel') body = renderOtelPanel();
  else if (tab === 'github') body = renderGithubPanel();
  else if (tab === 'cloudflare') body = renderCloudflarePanel();
  else if (tab === 'jira') body = renderJiraPanel();
  else body = renderMarketplacePanel();

  return el('div', {}, [header, tabsRow, body]);
}

function renderField(spec: FieldSpec): HTMLElement {
  const input = document.createElement('input');
  input.id = spec.id;
  input.name = spec.id;
  input.type = spec.type ?? 'text';
  input.placeholder = spec.placeholder;
  if (spec.value) input.value = spec.value;
  input.className = 'settings-input';
  input.autocomplete = 'off';
  return el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, [spec.label]), input]);
}

// ---------------- OTel 설정 (읽기 전용 실데이터 + 저장은 목업) ----------------
// ~/.claude/settings.json의 env.OTEL_* 값을 실제로 읽어 필드를 채운다. "저장"은
// 여전히 목업이다 — 이 화면에서 실제 설정 파일을 덮어쓰지 않는다(Claude Code
// 자체 전역 설정이라 잘못 건드리면 동작에 영향을 준다).

export async function loadOtelEnv(): Promise<void> {
  state.otel.loading = true;
  state.otel.error = null;
  notifyChange();
  try {
    state.otel.env = await fetchOtelEnv();
    state.otel.loaded = true;
  } catch (err) {
    state.otel.error = err instanceof Error ? err.message : 'OTel 설정을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.otel.loading = false;
    notifyChange();
  }
}

function renderOtelPanel(): HTMLElement {
  if (state.otel.loading && !state.otel.loaded) {
    return el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]);
  }
  if (state.otel.error) {
    return el('div', { className: 'alert' }, [
      el('span', {}, [`⚠ ${state.otel.error}`]),
      el('button', { className: 'btn', onClick: () => void loadOtelEnv() }, ['다시 시도']),
    ]);
  }

  const keys = Object.keys(state.otel.env).sort();

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, [
      '~/.claude/settings.json의 env.OTEL_* 값을 실제로 읽어와 표시합니다. 저장은 목업입니다 — 이 화면에서 저장해도 실제 설정 파일은 바뀌지 않습니다.',
    ]),
    ...(keys.length > 0
      ? keys.map((key) => renderField({ id: `otel-${key}`, label: key, placeholder: key, value: state.otel.env[key] }))
      : [el('div', { className: 'state-block-desc' }, ['~/.claude/settings.json에 OTEL_ 로 시작하는 값이 없습니다.'])]),
  ]);
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    showToast('OTel 설정 저장됨 (목업 — ~/.claude/settings.json은 건드리지 않습니다)');
  });

  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  return el('div', { className: 'settings-card' }, [form]);
}

// ---------------- 공용 로딩/에러 블록 ----------------
// otel 패널이 먼저 쓰던 패턴(loading/error/loaded 3상태)을 github/cloudflare/jira
// 패널도 동일하게 따른다.

function renderLoadingBlock(): HTMLElement {
  return el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]);
}

function renderErrorBlock(message: string, onRetry: () => void): HTMLElement {
  return el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${message}`]), el('button', { className: 'btn', onClick: onRetry }, ['다시 시도'])]);
}

// ---------------- GitHub 설정 (CLI 위임, 이 앱은 토큰을 취급하지 않는다) ----------------
// 상태는 `gh` CLI를 읽기 전용으로 조회한 실물이다. "연결하기"/"연결 해제"는 앱이
// 대신 로그인/로그아웃을 완료하지 않는다 — 사용자가 조작할 터미널 창을 여는
// 데까지만 관여한다. 그래서 버튼을 누른 직후 낙관적으로 "연결됨"으로 바꾸지
// 않고, 터미널에서 절차를 마친 뒤 "상태 새로고침"으로 실제 상태를 다시 읽는다.

export async function loadGithubStatus(): Promise<void> {
  state.github.loading = true;
  state.github.error = null;
  notifyChange();
  try {
    state.github.status = await fetchGithubStatus();
    state.github.loaded = true;
  } catch (err) {
    state.github.error = err instanceof Error ? err.message : 'GitHub 연동 상태를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.github.loading = false;
    notifyChange();
  }
}

async function handleGithubConnect(): Promise<void> {
  state.github.connecting = true;
  notifyChange();
  try {
    const result = await connectGithub();
    showToast(result.message);
  } catch (err) {
    showToast(`GitHub 연결을 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.github.connecting = false;
    notifyChange();
  }
}

async function handleGithubDisconnect(): Promise<void> {
  state.github.disconnecting = true;
  notifyChange();
  try {
    const result = await disconnectGithub();
    showToast(result.message);
  } catch (err) {
    showToast(`GitHub 연결 해제를 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.github.disconnecting = false;
    notifyChange();
  }
}

function renderGithubPanel(): HTMLElement {
  if (state.github.loading && !state.github.loaded) return renderLoadingBlock();
  if (state.github.error) return renderErrorBlock(state.github.error, () => void loadGithubStatus());

  const status = state.github.status;
  const refreshBtn = el('button', { className: 'btn', onClick: () => void loadGithubStatus() }, ['상태 새로고침']);

  if (!status || !status.installed) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        'GitHub CLI(gh)가 설치되어 있지 않습니다 → 개발 환경 화면에서 설치 상태를 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el('button', { className: 'btn btn-primary', onClick: () => navigate('#/dev-tools') }, ['개발 환경 화면으로 이동']),
        refreshBtn,
      ]),
    ]);
  }

  if (!status.connected) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        '조직 리포지토리 접근에 사용할 GitHub 계정을 연결합니다. 버튼을 누르면 터미널 창이 열리고, 그 창에서 로그인 절차를 직접 진행합니다. 로그인을 마친 뒤 "상태 새로고침"으로 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el(
          'button',
          { className: 'btn btn-primary', disabled: state.github.connecting, onClick: () => void handleGithubConnect() },
          [state.github.connecting ? '터미널 여는 중…' : 'GitHub 계정 연결하기']
        ),
        refreshBtn,
      ]),
    ]);
  }

  return el('div', { className: 'settings-card integration-panel' }, [
    el('div', { className: 'integration-account-row' }, [
      el('div', { className: 'integration-account-avatar' }, [(status.login ?? '?').charAt(0).toUpperCase()]),
      el('div', { className: 'integration-account-info' }, [
        el('div', { className: 'integration-account-name' }, [status.name ?? status.login ?? '']),
        el('div', { className: 'integration-account-status' }, [`@${status.login ?? '?'} · 연결됨`]),
      ]),
    ]),
    el('div', { className: 'settings-form-actions' }, [
      el(
        'button',
        { className: 'btn', disabled: state.github.disconnecting, onClick: () => void handleGithubDisconnect() },
        [state.github.disconnecting ? '터미널 여는 중…' : '연결 해제']
      ),
      refreshBtn,
    ]),
  ]);
}

// ---------------- Cloudflare 설정 (CLI 위임, wrangler) ----------------
// wrangler가 설치되어 있지 않은 것(installed:false)은 에러가 아니라 정상 상태다
// — 이 개발 머신에는 실제로 설치되어 있지 않다. 에러 알림이 아니라 "개발 환경
// 화면에서 확인" 안내로 처리한다. 나머지 흐름은 GitHub 패널과 동일하다.

export async function loadCloudflareStatus(): Promise<void> {
  state.cloudflare.loading = true;
  state.cloudflare.error = null;
  notifyChange();
  try {
    state.cloudflare.status = await fetchCloudflareStatus();
    state.cloudflare.loaded = true;
  } catch (err) {
    state.cloudflare.error = err instanceof Error ? err.message : 'Cloudflare 연동 상태를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.cloudflare.loading = false;
    notifyChange();
  }
}

async function handleCloudflareConnect(): Promise<void> {
  state.cloudflare.connecting = true;
  notifyChange();
  try {
    const result = await connectCloudflare();
    showToast(result.message);
  } catch (err) {
    showToast(`Cloudflare 연결을 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.cloudflare.connecting = false;
    notifyChange();
  }
}

async function handleCloudflareDisconnect(): Promise<void> {
  state.cloudflare.disconnecting = true;
  notifyChange();
  try {
    const result = await disconnectCloudflare();
    showToast(result.message);
  } catch (err) {
    showToast(`Cloudflare 연결 해제를 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.cloudflare.disconnecting = false;
    notifyChange();
  }
}

function renderCloudflarePanel(): HTMLElement {
  if (state.cloudflare.loading && !state.cloudflare.loaded) return renderLoadingBlock();
  if (state.cloudflare.error) return renderErrorBlock(state.cloudflare.error, () => void loadCloudflareStatus());

  const status = state.cloudflare.status;
  const refreshBtn = el('button', { className: 'btn', onClick: () => void loadCloudflareStatus() }, ['상태 새로고침']);

  if (!status || !status.installed) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        'Wrangler CLI가 설치되어 있지 않습니다 → 개발 환경 화면에서 설치 상태를 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el('button', { className: 'btn btn-primary', onClick: () => navigate('#/dev-tools') }, ['개발 환경 화면으로 이동']),
        refreshBtn,
      ]),
    ]);
  }

  if (!status.connected) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        '배포·DNS 자동화에 사용할 Cloudflare 계정을 연결합니다. 버튼을 누르면 터미널 창이 열리고, 그 창에서 로그인 절차를 직접 진행합니다. 로그인을 마친 뒤 "상태 새로고침"으로 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el(
          'button',
          { className: 'btn btn-primary', disabled: state.cloudflare.connecting, onClick: () => void handleCloudflareConnect() },
          [state.cloudflare.connecting ? '터미널 여는 중…' : 'Cloudflare 계정 연결하기']
        ),
        refreshBtn,
      ]),
    ]);
  }

  return el('div', { className: 'settings-card integration-panel' }, [
    el('div', { className: 'integration-account-row' }, [
      el('div', { className: 'integration-account-avatar' }, [(status.email ?? '?').charAt(0).toUpperCase()]),
      el('div', { className: 'integration-account-info' }, [
        el('div', { className: 'integration-account-name' }, [status.email ?? '']),
        el('div', { className: 'integration-account-status' }, ['연결됨']),
      ]),
    ]),
    el('div', { className: 'settings-form-actions' }, [
      el(
        'button',
        { className: 'btn', disabled: state.cloudflare.disconnecting, onClick: () => void handleCloudflareDisconnect() },
        [state.cloudflare.disconnecting ? '터미널 여는 중…' : '연결 해제']
      ),
      refreshBtn,
    ]),
  ]);
}

// ---------------- Jira 설정 (개인별 자격증명, 실제 저장) ----------------
// 위임할 CLI가 없어 사이트 URL·이메일·API 토큰을 직접 받는다. "저장"은 실제로
// jira_connect를 호출해 /rest/api/3/myself로 검증한 뒤 macOS 키체인에 담는다.
// 토큰 입력란에는 절대 value를 주지 않는다(이미 저장된 상태에서도 placeholder로만
// 표시) — 빈 값으로 제출하면 "변경 없음"으로 해석해 API를 호출하지 않는다.
// 토큰은 제출 시점에만 읽어 connectJira()에 넘기고 그 뒤로 어떤 변수에도 남기지
// 않는다("눈 아이콘"으로 보이게 하지 않는다 — 화면 공유가 잦은 사내 환경 위험).

export async function loadJiraStatus(): Promise<void> {
  state.jira.loading = true;
  state.jira.error = null;
  notifyChange();
  try {
    state.jira.status = await fetchJiraStatus();
    state.jira.loaded = true;
  } catch (err) {
    state.jira.error = err instanceof Error ? err.message : 'Jira 연동 상태를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.jira.loading = false;
    notifyChange();
  }
}

async function handleJiraDisconnect(): Promise<void> {
  state.jira.disconnecting = true;
  notifyChange();
  try {
    await disconnectJira();
    state.jira.status = { connected: false };
    showToast('Jira 연결이 해제되었습니다.');
  } catch (err) {
    showToast(`Jira 연결 해제에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.jira.disconnecting = false;
    notifyChange();
  }
}

function renderJiraPanel(): HTMLElement {
  if (state.jira.loading && !state.jira.loaded) return renderLoadingBlock();
  if (state.jira.error) return renderErrorBlock(state.jira.error, () => void loadJiraStatus());

  const status = state.jira.status;
  const connected = status?.connected ?? false;

  const siteInput = document.createElement('input');
  siteInput.id = 'jira-site';
  siteInput.name = 'jira-site';
  siteInput.type = 'text';
  siteInput.placeholder = 'https://malgnsoft.atlassian.net';
  siteInput.value = status?.site ?? '';
  siteInput.className = 'settings-input';
  siteInput.autocomplete = 'off';

  const emailInput = document.createElement('input');
  emailInput.id = 'jira-email';
  emailInput.name = 'jira-email';
  emailInput.type = 'text';
  emailInput.placeholder = 'dev@malgnsoft.com';
  emailInput.value = status?.email ?? '';
  emailInput.className = 'settings-input';
  emailInput.autocomplete = 'off';

  // 토큰 입력란 — value를 절대 주지 않는다. 이미 연결된 상태면 placeholder로만
  // "저장되어 있다"는 사실을 알린다.
  const tokenInput = document.createElement('input');
  tokenInput.id = 'jira-token';
  tokenInput.name = 'jira-token';
  tokenInput.type = 'password';
  tokenInput.placeholder = connected ? '변경하려면 새 토큰을 입력하세요' : '****';
  tokenInput.className = 'settings-input';
  tokenInput.autocomplete = 'off';

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, [
      connected
        ? `${status?.displayName ?? status?.email ?? ''} 계정으로 연결되어 있습니다. 토큰을 바꾸려면 새 토큰을 입력한 뒤 저장하세요.`
        : '자율업무·이슈 연동에 사용할 Jira 계정을 연결합니다. API 토큰은 https://id.atlassian.com/manage-profile/security/api-tokens 에서 발급받을 수 있습니다.',
    ]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['Jira 사이트 URL']), siteInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['계정 이메일']), emailInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['API 토큰']), tokenInput]),
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const site = siteInput.value.trim();
    const email = emailInput.value.trim();
    const token = tokenInput.value;

    if (!token) {
      if (connected) {
        showToast('토큰을 입력하지 않아 변경 사항이 없습니다.');
      } else {
        showToast('API 토큰을 입력해주세요.');
      }
      return;
    }
    if (!site || !email) {
      showToast('Jira 사이트 URL과 계정 이메일을 입력해주세요.');
      return;
    }

    void (async () => {
      state.jira.connecting = true;
      notifyChange();
      try {
        state.jira.status = await connectJira(site, email, token);
        showToast('Jira 계정이 연결되었습니다.');
      } catch (err) {
        showToast(`Jira 연결에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        state.jira.connecting = false;
        notifyChange();
      }
    })();
  });

  const saveBtn = el('button', { className: 'btn btn-primary', disabled: state.jira.connecting }, [state.jira.connecting ? '저장 중…' : '저장']);
  saveBtn.type = 'submit';
  const actions: HTMLElement[] = [saveBtn];
  if (connected) {
    const disconnectBtn = el('button', { className: 'btn', disabled: state.jira.disconnecting, onClick: () => void handleJiraDisconnect() }, [
      state.jira.disconnecting ? '해제 중…' : '연결 해제',
    ]);
    // <form> 안의 <button>은 type을 명시하지 않으면 기본값이 "submit"이라 이
    // 버튼을 눌러도 저장 폼이 함께 제출된다 — 명시적으로 "button"으로 막는다.
    disconnectBtn.type = 'button';
    actions.push(disconnectBtn);
  }
  form.appendChild(el('div', { className: 'settings-form-actions' }, actions));

  return el('div', { className: 'settings-card' }, [form]);
}

// ---------------- 마켓플레이스 설정 (실제 로컬 데이터 + 실제 새로고침 실행) ----------------
// 저장소 목록은 known_marketplaces.json 실물, 설치된 플러그인 목록은 카탈로그
// 화면과 같은 state.catalog.plugins를 공유한다. "마켓플레이스 새로고침" 버튼은
// 사용자 승인 아래 실제로 `claude plugin marketplace update`를 실행한다.

function formatIsoDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString('ko-KR', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
  } catch {
    return iso;
  }
}

async function handleRefreshMarketplaces(): Promise<void> {
  state.marketplaces.refreshing = true;
  notifyChange();
  try {
    const result = await refreshMarketplaces();
    showToast(result.success ? `마켓플레이스 새로고침 완료 (${result.message})` : `마켓플레이스 새로고침 실패: ${result.message}`);
    await loadMarketplaces();
    await loadCatalog();
  } catch (err) {
    showToast(`마켓플레이스 새로고침 실패: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.marketplaces.refreshing = false;
    notifyChange();
  }
}

function renderMarketplacePanel(): HTMLElement {
  const repoRows =
    state.marketplaces.items.length > 0
      ? state.marketplaces.items.map((m) =>
          el('div', { className: 'marketplace-repo-row' }, [
            el('div', {}, [
              el('div', { className: 'marketplace-repo-name' }, [m.id]),
              el('div', { className: 'marketplace-repo-path' }, [m.repo ? `github.com/${m.repo}` : '저장소 정보 없음']),
              ...(m.lastUpdated ? [el('div', { className: 'marketplace-repo-updated' }, [`마지막 업데이트: ${formatIsoDate(m.lastUpdated)}`])] : []),
            ]),
            el('span', { className: 'badge badge-active' }, ['연결됨']),
          ])
        )
      : [el('div', { className: 'state-block-desc' }, ['등록된 마켓플레이스가 없습니다.'])];

  const refreshBtn = el(
    'button',
    { className: 'btn', disabled: state.marketplaces.refreshing, onClick: () => void handleRefreshMarketplaces() },
    [state.marketplaces.refreshing ? '새로고침 중…' : '↻ 마켓플레이스 새로고침']
  );

  const pluginList = el(
    'div',
    { className: 'marketplace-plugin-list' },
    state.catalog.plugins.length > 0
      ? state.catalog.plugins.map((p) =>
          el('div', { className: 'marketplace-plugin-row' }, [
            el('div', {}, [
              el('div', { className: 'marketplace-plugin-name' }, [p.displayName ?? p.name]),
              el('div', { className: 'marketplace-plugin-version' }, [`v${p.version}`]),
            ]),
            toggleSwitch(state.catalog.autoUpdate[p.id] ?? true, () => {
              state.catalog.autoUpdate[p.id] = !(state.catalog.autoUpdate[p.id] ?? true);
              notifyChange();
            }),
          ])
        )
      : [el('div', { className: 'state-block-desc' }, ['설치된 플러그인이 없습니다.'])]
  );

  const form = el('form', { className: 'settings-form marketplace-form' }, [
    el('div', { className: 'settings-form-hint' }, ['카탈로그가 플러그인을 받아오는 저장소와 설치된 플러그인의 자동 업데이트 여부를 관리합니다.']),
  ]);
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    showToast('마켓플레이스 설정 저장됨 (목업 — 실제로 저장되지 않습니다)');
  });
  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  return el('div', { className: 'settings-card marketplace-panel' }, [
    el('div', { className: 'marketplace-repo-list' }, repoRows),
    el('div', { className: 'marketplace-actions' }, [refreshBtn]),
    el('div', { className: 'settings-field-label marketplace-list-label' }, ['설치된 플러그인']),
    pluginList,
    form,
  ]);
}
