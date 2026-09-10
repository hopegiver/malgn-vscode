import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import type { SettingsTab } from '../state';
import { fetchOtelSettings, saveOtelSettings } from '../otelApi';
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
import { fetchMcpServers, addMcpServer, removeMcpServer, loginMcpServer, fetchMcpCatalog, installMcpCatalogEntry } from '../mcpApi';
import type { McpTransport, McpServerSummary, McpCatalogEntry } from '../mcpApi';
import { navigate } from '../route';

const TAB_META: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  { key: 'github', label: 'GitHub 설정' },
  { key: 'cloudflare', label: 'Cloudflare 설정' },
  { key: 'jira', label: 'Jira 설정' },
  { key: 'marketplace', label: '마켓플레이스 설정' },
  { key: 'mcp', label: 'MCP 관리' },
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
  else if (tab === 'marketplace') body = renderMarketplacePanel();
  else body = renderMcpPanel();

  return el('div', {}, [header, tabsRow, body]);
}

// ---------------- OTel 설정 (실제 저장) ----------------
// ~/.claude/settings.json의 관리대상(allowlist) 14키를 otel_settings_get()으로
// 읽고 otel_settings_save()로 실제 파일에 기록한다. 화면 진입만으로는 절대
// 저장을 호출하지 않는다(저장 버튼을 눌렀을 때만).
//
// 폼 초기값 우선순위: values(실제 저장값) → defaults(기본값) → 빈 값. 이미
// 저장된 값과 "아직 저장 안 된 기본값"을 구분해 보여준다(기본값만 있는 필드에
// "기본값(저장 안 됨)" 힌트를 붙인다) — 그래야 사용자가 "저장을 눌러야 이 값이
// 실제로 기록된다"는 사실을 오해하지 않는다.
//
// readOnlyKeys(프라이버시 4키)는 입력을 비활성화하고 저장 payload에도 포함하지
// 않는다(백엔드가 Err를 던진다). OTEL_RESOURCE_ATTRIBUTES는 employee.* 조립이
// 전적으로 Rust 책임이라(§8) 편집 불가한 읽기 전용 안내로만 보여준다.

export async function loadOtelEnv(): Promise<void> {
  state.otel.loading = true;
  state.otel.error = null;
  notifyChange();
  try {
    state.otel.settings = await fetchOtelSettings();
    state.otel.loaded = true;
  } catch (err) {
    state.otel.error = err instanceof Error ? err.message : 'OTel 설정을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.otel.loading = false;
    notifyChange();
  }
}

async function handleSaveOtelSettings(values: Record<string, string>): Promise<void> {
  state.otel.saving = true;
  notifyChange();
  try {
    // 비로그인이면 identity: null — 백엔드가 이 경우 OTEL_RESOURCE_ATTRIBUTES를
    // 손대지 않는다(기존 귀속 정보를 지우는 것이 가장 나쁜 결과라서).
    const identity = state.auth.userEmail ? { email: state.auth.userEmail, name: state.auth.userName } : null;
    state.otel.settings = await saveOtelSettings({ values, identity });
    showToast('OTel 설정을 저장했습니다.');
  } catch (err) {
    showToast(`OTel 설정 저장에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.otel.saving = false;
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

  const settings = state.otel.settings;
  if (!settings) {
    return el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['OTel 설정을 불러오지 못했습니다'])]);
  }

  // settings.json 자체가 파싱 불가면 폼 전체를 잠그고 오류만 보여준다 — 파싱
  // 못 하는 파일을 저장 시도로 덮어쓰는 사고를 막는다(백엔드도 이 경우 저장을
  // 거부하지만, 프론트도 폼 자체를 아예 그리지 않는다).
  if (settings.parseError) {
    return el('div', { className: 'settings-card' }, [
      el('div', { className: 'alert' }, [`⚠ ${settings.settingsPath} 파싱 실패: ${settings.parseError}`]),
      el('div', { className: 'settings-form-hint' }, ['파일을 직접 열어 문법 오류를 고친 뒤 "다시 시도"를 누르세요.']),
      el('button', { className: 'btn', onClick: () => void loadOtelEnv() }, ['다시 시도']),
    ]);
  }

  const inputs = new Map<string, HTMLInputElement>();
  const fields: HTMLElement[] = [];

  for (const key of settings.managedKeys) {
    if (key === 'OTEL_RESOURCE_ATTRIBUTES') {
      const hasValue = key in settings.values;
      const currentValue = hasValue ? settings.values[key] : key in settings.defaults ? settings.defaults[key] : '(없음)';
      fields.push(
        el('div', { className: 'settings-field' }, [
          el('span', { className: 'settings-field-label' }, [key]),
          el('div', { className: 'settings-form-hint' }, [
            `${currentValue} — 로그인 계정 정보(이메일/이름)에서 저장 시 자동으로 조립됩니다. 여기서 직접 입력할 수 없습니다.`,
          ]),
        ])
      );
      continue;
    }

    const readOnly = settings.readOnlyKeys.includes(key);
    const hasValue = key in settings.values;
    const hasDefault = key in settings.defaults;
    const initialValue = hasValue ? settings.values[key] : hasDefault ? settings.defaults[key] : '';
    const isDefaultOnly = !hasValue && hasDefault;
    const isEndpoint = key.endsWith('_ENDPOINT');

    const input = document.createElement('input');
    input.id = `otel-${key}`;
    input.name = key;
    input.type = 'text';
    input.value = initialValue;
    input.className = 'settings-input';
    input.autocomplete = 'off';
    input.disabled = readOnly;
    if (isEndpoint && !settings.endpointDefaultsInjected) {
      input.placeholder = 'https://collector.example.com:4318';
    }
    inputs.set(key, input);

    const hints: HTMLElement[] = [];
    if (readOnly) {
      hints.push(
        el('div', { className: 'settings-form-hint' }, ['프라이버시 보호를 위해 이 값은 여기서 바꿀 수 없습니다. 필요하면 ~/.claude/settings.json을 직접 편집하세요.'])
      );
    } else if (isDefaultOnly) {
      hints.push(
        el('div', { className: 'settings-form-hint' }, ['기본값(저장 안 됨) — 지금 표시된 값은 아직 settings.json에 기록되지 않았습니다. 저장을 누르면 이 값 그대로 기록됩니다.'])
      );
    }
    if (isEndpoint && !settings.endpointDefaultsInjected) {
      hints.push(el('div', { className: 'settings-form-hint' }, ['사내 collector 주소는 배포 빌드에 주입됩니다. 비어 있으면 직접 입력하세요.']));
    }

    fields.push(el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, [key]), input, ...hints]));
  }

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, [
      '~/.claude/settings.json의 관리대상 OTel 키를 실제로 읽고 저장합니다. 저장 시 파일 형식이 정규화됩니다(키가 알파벳순으로 재정렬되고, 저장 직전 원본이 settings.json.malgn-bak으로 백업됩니다).',
    ]),
    ...fields,
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const values: Record<string, string> = {};
    for (const [key, input] of inputs) {
      if (settings.readOnlyKeys.includes(key)) continue;
      values[key] = input.value;
    }
    void handleSaveOtelSettings(values);
  });

  const saveBtn = el('button', { className: 'btn btn-primary', disabled: state.otel.saving }, [state.otel.saving ? '저장 중…' : '저장']);
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

// ---------------- MCP 관리 (claude mcp CLI 위임, 읽기+추가/삭제) ----------------
// 목록/상세는 `claude mcp` CLI를 실제로 위임 실행한 결과다(mcpApi.ts). 모델을
// 호출하지 않는 순수 헬스체크라 빠르고 무료다. 삭제는 실제로 서버 등록을
// 지우므로 확인(window.confirm) 후에만 실행한다. 추가 폼 펼침 상태는 다른
// 화면 탭 전환과 무관한 순수 UI 상태라 자율업무 화면의 showAddForm과 같은
// 방식으로 모듈 스코프 변수에 둔다.

export async function loadMcp(): Promise<void> {
  state.mcp.loading = true;
  state.mcp.error = null;
  notifyChange();
  try {
    state.mcp.items = await fetchMcpServers();
    state.mcp.loaded = true;
  } catch (err) {
    state.mcp.error = err instanceof Error ? err.message : 'MCP 서버 목록을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.mcp.loading = false;
    notifyChange();
  }
}

// ---------------- MCP 카탈로그 (잘 알려진 공개 MCP 서버 원클릭 설치) ----------------
// 넷 다 OAuth 로그인이 필요해서 이 앱이 로그인을 대신 처리하지 않는다 — GitHub/
// Cloudflare 연동과 동일한 정책으로, "설치" 버튼은 백엔드가 `claude mcp add`+
// `claude mcp login`을 이어서 실행할 터미널 창을 여는 데까지만 관여한다. 그래서
// 버튼을 누른 직후 낙관적으로 "연결됨"으로 바꾸지 않고, 안내 토스트만 띄운 뒤
// 사용자가 터미널에서 로그인을 마치고 기존 "새로고침"(mcp_list 재조회)을 눌러야
// 실제 연결 여부가 반영된다.

export async function loadMcpCatalog(): Promise<void> {
  state.mcpCatalog.loading = true;
  state.mcpCatalog.error = null;
  notifyChange();
  try {
    state.mcpCatalog.items = await fetchMcpCatalog();
    state.mcpCatalog.loaded = true;
  } catch (err) {
    state.mcpCatalog.error = err instanceof Error ? err.message : 'MCP 카탈로그를 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.mcpCatalog.loading = false;
    notifyChange();
  }
}

async function handleInstallMcpCatalogEntry(entry: McpCatalogEntry): Promise<void> {
  state.mcpCatalog.installingId = entry.id;
  notifyChange();
  try {
    const result = await installMcpCatalogEntry(entry.id);
    showToast(result.message);
  } catch (err) {
    showToast(`"${entry.label}" 설치를 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.mcpCatalog.installingId = null;
    notifyChange();
  }
}

// 카탈로그 행은 이미 설치된 항목을 걸러낸 뒤(buildMcpRows) 전달받으므로 여기서는
// "설치" 액션 하나만 그린다.
function renderMcpCatalogRow(entry: McpCatalogEntry): HTMLElement {
  const installing = state.mcpCatalog.installingId === entry.id;

  const actionEl = el(
    'button',
    { className: 'btn btn-primary', disabled: installing, onClick: () => void handleInstallMcpCatalogEntry(entry) },
    [installing ? '터미널 여는 중…' : '설치']
  );

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, [entry.label]),
        el('span', { className: 'badge badge-unknown' }, [entry.transport]),
      ]),
      el('div', { className: 'mcp-row-target' }, [entry.target]),
    ]),
    el('div', { className: 'mcp-row-actions' }, [actionEl]),
  ]);
}

// ---------------- GitHub 공식 MCP 빠른시작 (PAT 방식, OAuth 카탈로그와 별개) ----------------
// GitHub 공식 원격 MCP(https://api.githubcopilot.com/mcp/)는 OAuth가 아니라
// 사용자가 직접 발급한 Personal Access Token을 Authorization 헤더로 넣는
// 방식이다. 그래서 원클릭 설치(mcp_install)가 아니라, 기존 "새 MCP 서버" 폼을
// 미리 채워서 열어주고 사용자는 토큰만 입력해 기존 mcp_add(addMcpServer)로
// 저장하게 한다 — 새 백엔드 커맨드는 필요 없다.

const GITHUB_MCP_TARGET = 'https://api.githubcopilot.com/mcp/';

// 이미 등록된 경우(state.mcp.items에 target이 있음)에는 buildMcpRows가 이 행 자체를
// 만들지 않는다 — 그래서 "이미 등록됨" 분기가 필요 없다.
function renderGithubMcpQuickstartRow(): HTMLElement {
  const actionEl = el(
    'button',
    {
      className: 'btn btn-primary',
      onClick: () => {
        mcpAddPrefill = { name: 'GitHub', transport: 'http', target: GITHUB_MCP_TARGET };
        mcpAddFormOpen = true;
        notifyChange();
      },
    },
    ['설치']
  );

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, ['GitHub']),
        el('span', { className: 'badge badge-unknown' }, ['http']),
      ]),
      el('div', { className: 'mcp-row-target' }, [GITHUB_MCP_TARGET]),
    ]),
    el('div', { className: 'mcp-row-actions' }, [actionEl]),
  ]);
}

let mcpAddFormOpen = false;
let mcpAddPrefill: McpAddPrefill | null = null;

async function handleRemoveMcp(server: McpServerSummary): Promise<void> {
  if (!window.confirm(`"${server.name}" MCP 서버를 삭제할까요? 이 작업은 되돌릴 수 없습니다.`)) return;
  try {
    await removeMcpServer(server.name);
    showToast(`"${server.name}" 서버를 삭제했습니다`);
    await loadMcp();
  } catch (err) {
    showToast(`삭제에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  }
}

async function handleLoginMcp(server: McpServerSummary): Promise<void> {
  state.mcp.loggingInName = server.name;
  notifyChange();
  try {
    const result = await loginMcpServer(server.name);
    showToast(result.message);
  } catch (err) {
    showToast(`"${server.name}" 로그인을 시작하지 못했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.mcp.loggingInName = null;
    notifyChange();
  }
}

function renderMcpRow(server: McpServerSummary): HTMLElement {
  const deleteBtn = el('button', { className: 'btn', onClick: () => void handleRemoveMcp(server) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';

  const actions: HTMLElement[] = [];
  // stdio 서버는 OAuth 로그인 개념이 없다 — http/sse에만 노출한다. 이미 연결된
  // 서버든 아니든(재로그인 필요할 수 있다) 항상 보여준다.
  if (server.transport !== 'stdio') {
    const loggingIn = state.mcp.loggingInName === server.name;
    const loginBtn = el(
      'button',
      { className: 'btn', disabled: loggingIn, onClick: () => void handleLoginMcp(server) },
      [loggingIn ? '터미널 여는 중…' : '인증']
    );
    actions.push(loginBtn);
  }
  actions.push(deleteBtn);

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, [server.name]),
        el('span', { className: 'badge badge-unknown' }, [server.transport]),
        el('span', { className: `badge ${server.connected ? 'badge-active' : 'badge-archived'}` }, [server.connected ? '연결됨' : '미연결']),
      ]),
      el('div', { className: 'mcp-row-target' }, [server.target]),
      el('div', { className: 'mcp-row-status-label' }, [server.statusLabel]),
    ]),
    el('div', { className: 'mcp-row-actions' }, actions),
  ]);
}

// ---------------- 통합 목록 (등록된 서버 + 미설치 카탈로그 + GitHub 퀵스타트) ----------------
// 세 데이터 소스를 하나의 전체폭 .mcp-list로 합친다 — 박스 구분 없이 행 단위로만
// 나열한다. malgn-agent 관련 항목(malgnai-hub 등)은 항상 맨 위로 올리고, 나머지는
// 등록된 서버 → 미설치 카탈로그 → GitHub 퀵스타트 순서를 유지한다(Array.sort는
// ES2019+ 스펙상 안정 정렬이라 동순위 항목의 상대 순서가 보존된다).

function isMalgnAgentEntry(name: string): boolean {
  return name.includes('malgn-agent');
}

function buildMcpRows(): HTMLElement[] {
  const rows: { name: string; el: HTMLElement }[] = [];

  for (const server of state.mcp.items) {
    rows.push({ name: server.name, el: renderMcpRow(server) });
  }

  for (const entry of state.mcpCatalog.items) {
    if (!entry.installed) rows.push({ name: entry.label, el: renderMcpCatalogRow(entry) });
  }

  const githubAlreadyRegistered = state.mcp.items.some((item) => item.target === GITHUB_MCP_TARGET);
  if (!githubAlreadyRegistered) {
    rows.push({ name: 'GitHub', el: renderGithubMcpQuickstartRow() });
  }

  rows.sort((a, b) => {
    const aTop = isMalgnAgentEntry(a.name);
    const bTop = isMalgnAgentEntry(b.name);
    if (aTop === bTop) return 0;
    return aTop ? -1 : 1;
  });

  return rows.map((r) => r.el);
}

interface McpAddPrefill {
  readonly name: string;
  readonly transport: McpTransport;
  readonly target: string;
}

function renderMcpAddForm(prefill?: McpAddPrefill): HTMLElement {
  const nameInput = document.createElement('input');
  nameInput.className = 'settings-input';
  nameInput.placeholder = '예: plugin:malgn-agent:malgnai-hub';
  nameInput.autocomplete = 'off';
  if (prefill) nameInput.value = prefill.name;

  const transportSelect = document.createElement('select');
  transportSelect.className = 'settings-input';
  for (const t of ['stdio', 'http', 'sse'] as const) {
    const opt = document.createElement('option');
    opt.value = t;
    opt.textContent = t;
    transportSelect.appendChild(opt);
  }
  if (prefill) transportSelect.value = prefill.transport;

  const targetInput = document.createElement('input');
  targetInput.className = 'settings-input';
  targetInput.autocomplete = 'off';
  if (prefill) targetInput.value = prefill.target;

  const argsInput = document.createElement('input');
  argsInput.className = 'settings-input';
  argsInput.placeholder = '예: run server.js --port 3000 (공백으로 구분해 args 배열로 변환)';
  argsInput.autocomplete = 'off';
  const argsField = el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['실행 인자 (args)']), argsInput]);

  const headerInput = document.createElement('input');
  headerInput.className = 'settings-input';
  headerInput.placeholder = 'Authorization: Bearer xxx (비워두면 헤더 없음)';
  headerInput.autocomplete = 'off';
  const headerField = el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['헤더 (선택)']), headerInput]);

  // 내부(사내) MCP 서버용 OAuth 설정 — http/sse 전용. Client Secret은 실제
  // 인증서버 발급 비밀값이라 반드시 마스킹한다(env textarea와 달리).
  const oauthClientIdInput = document.createElement('input');
  oauthClientIdInput.className = 'settings-input';
  oauthClientIdInput.placeholder = '내부 MCP 서버의 OAuth Client ID';
  oauthClientIdInput.autocomplete = 'off';
  const oauthClientIdField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['OAuth Client ID (선택)']),
    oauthClientIdInput,
  ]);

  const oauthClientSecretInput = document.createElement('input');
  oauthClientSecretInput.className = 'settings-input';
  oauthClientSecretInput.type = 'password';
  oauthClientSecretInput.placeholder = '내부 MCP 서버의 OAuth Client Secret';
  oauthClientSecretInput.autocomplete = 'off';
  const oauthClientSecretField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['OAuth Client Secret (선택)']),
    oauthClientSecretInput,
  ]);

  const oauthCallbackPortInput = document.createElement('input');
  oauthCallbackPortInput.className = 'settings-input';
  oauthCallbackPortInput.type = 'number';
  oauthCallbackPortInput.placeholder = '예: 51000';
  oauthCallbackPortInput.autocomplete = 'off';
  const oauthCallbackPortField = el('label', { className: 'settings-field' }, [
    el('span', { className: 'settings-field-label' }, ['OAuth Callback Port (선택)']),
    oauthCallbackPortInput,
  ]);

  const envInput = document.createElement('textarea');
  envInput.className = 'settings-input';
  envInput.rows = 3;
  envInput.placeholder = '한 줄에 KEY=VALUE 하나씩 입력\n예: GRAFANA_URL=http://localhost:3000\nGRAFANA_SERVICE_ACCOUNT_TOKEN=glsa_xxx';
  envInput.autocomplete = 'off';
  const envField = el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['환경변수 (선택)']), envInput]);

  // stdio/http/sse에 따라 target placeholder와 args/header/env 필드 노출 여부가
  // 달라진다 — select를 바꿀 때마다 전체 재렌더(notifyChange)를 하면 이미
  // 입력한 다른 필드 값이 날아가므로, 이 폼 안에서는 DOM을 직접 갱신한다.
  function syncTransportFields(): void {
    const t = transportSelect.value as McpTransport;
    if (t === 'stdio') {
      targetInput.placeholder = '실행 파일 경로 또는 명령어';
      argsField.style.display = '';
      envField.style.display = '';
      headerField.style.display = 'none';
      oauthClientIdField.style.display = 'none';
      oauthClientSecretField.style.display = 'none';
      oauthCallbackPortField.style.display = 'none';
    } else {
      targetInput.placeholder = 'URL';
      argsField.style.display = 'none';
      envField.style.display = 'none';
      headerField.style.display = '';
      oauthClientIdField.style.display = '';
      oauthClientSecretField.style.display = '';
      oauthCallbackPortField.style.display = '';
    }
  }
  transportSelect.addEventListener('change', syncTransportFields);
  syncTransportFields();

  const form = el('form', { className: 'settings-form mcp-add-form' }, [
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['transport']), transportSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['target']), targetInput]),
    argsField,
    envField,
    headerField,
    oauthClientIdField,
    oauthClientSecretField,
    oauthCallbackPortField,
  ]);

  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = nameInput.value.trim();
    const target = targetInput.value.trim();
    const transport = transportSelect.value as McpTransport;
    if (!name || !target) {
      showToast('이름과 target을 입력하세요.');
      return;
    }
    const args = transport === 'stdio' ? argsInput.value.trim().split(/\s+/).filter(Boolean) : [];
    const header = transport === 'stdio' ? null : headerInput.value.trim() || null;
    const env =
      transport === 'stdio'
        ? envInput.value
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean)
            .map((line) => {
              const idx = line.indexOf('=');
              return idx === -1 ? null : { key: line.slice(0, idx).trim(), value: line.slice(idx + 1).trim() };
            })
            .filter((pair): pair is { key: string; value: string } => pair !== null && pair.key !== '')
        : [];

    const oauthClientId = transport === 'stdio' ? null : oauthClientIdInput.value.trim() || null;
    const oauthClientSecret = transport === 'stdio' ? null : oauthClientSecretInput.value.trim() || null;
    const oauthCallbackPortRaw = transport === 'stdio' ? '' : oauthCallbackPortInput.value.trim();
    const oauthCallbackPort = oauthCallbackPortRaw ? Number(oauthCallbackPortRaw) : null;
    if (oauthCallbackPort !== null && (!Number.isInteger(oauthCallbackPort) || oauthCallbackPort <= 0)) {
      showToast('OAuth Callback Port는 양의 정수로 입력하세요.');
      return;
    }

    saveBtn.disabled = true;
    void (async () => {
      try {
        await addMcpServer({ name, transport, target, args, header, env, oauthClientId, oauthClientSecret, oauthCallbackPort });
        showToast(`"${name}" MCP 서버가 추가되었습니다`);
        mcpAddFormOpen = false;
        mcpAddPrefill = null;
        await loadMcp();
      } catch (err) {
        showToast(`추가에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
        saveBtn.disabled = false;
        notifyChange();
      }
    })();
  });

  return el('div', { className: 'settings-card mcp-add-card' }, [form]);
}

function renderMcpPanel(): HTMLElement {
  if (state.mcp.loading && !state.mcp.loaded) return renderLoadingBlock();
  if (state.mcp.error) return renderErrorBlock(state.mcp.error, () => void loadMcp());

  const refreshBtn = el('button', { className: 'btn', onClick: () => void loadMcp() }, ['↻ 새로고침']);
  const addToggleBtn = el(
    'button',
    {
      className: 'btn btn-primary',
      onClick: () => {
        mcpAddFormOpen = !mcpAddFormOpen;
        if (!mcpAddFormOpen) mcpAddPrefill = null;
        notifyChange();
      },
    },
    [mcpAddFormOpen ? '취소' : '+ 새 MCP 서버']
  );

  const body: HTMLElement[] = [
    el('div', { className: 'settings-form-hint' }, [
      'claude mcp CLI로 연결 상태만 확인합니다 — 모델을 호출하지 않는 순수 헬스체크라 빠르고 비용이 들지 않습니다. 카탈로그 항목은 OAuth 로그인이, GitHub 공식 MCP는 Personal Access Token이 필요합니다. "설치"/"인증"을 누르면 터미널 창이 열리고, 그 창에서 절차를 직접 마친 뒤 "↻ 새로고침"으로 반영하세요.',
    ]),
    el('div', { className: 'mcp-toolbar' }, [refreshBtn, addToggleBtn]),
  ];

  if (mcpAddFormOpen) body.push(renderMcpAddForm(mcpAddPrefill ?? undefined));

  if (state.mcpCatalog.error) {
    body.push(renderErrorBlock(`MCP 카탈로그를 불러오지 못했습니다: ${state.mcpCatalog.error}`, () => void loadMcpCatalog()));
  }

  const rows = buildMcpRows();
  const catalogStillLoading = state.mcpCatalog.loading && !state.mcpCatalog.loaded;

  if (rows.length > 0) {
    body.push(el('div', { className: 'mcp-list' }, rows));
  } else if (!catalogStillLoading) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['등록된 MCP 서버가 없습니다']),
        el('div', { className: 'state-block-desc' }, ['"+ 새 MCP 서버"로 등록하세요.']),
      ])
    );
  }

  // 카탈로그는 등록된 서버와 별도로 로딩된다 — 등록된 서버(및 GitHub 퀵스타트)는
  // 위 mcp-list에 먼저 보이고, 카탈로그 항목은 로딩이 끝나면 같은 목록에 합류한다.
  if (catalogStillLoading) body.push(renderLoadingBlock());

  return el('div', {}, body);
}
