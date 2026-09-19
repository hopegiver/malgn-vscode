import { el, showToast, loadingBlock, errorBlock, confirmDialog, createModalOverlay } from '../dom';
import { state, notifyChange } from '../state';
import type { SettingsTab } from '../state';
import { fetchOtelSettings, saveOtelSettings } from '../otelApi';
import type { OtelSettings } from '../otelApi';
import { loadCatalog, loadMarketplaces, DEFAULT_PLUGIN_ID } from './catalog';
import { renderAppLinksPanel } from './appLinks';
import { refreshMarketplaces, installPlugin, addMarketplace, removeMarketplace } from '../catalogApi';
import type { MarketplaceInfo } from '../catalogApi';
import {
  fetchGithubStatus,
  connectGithub,
  disconnectGithub,
  fetchCloudflareStatus,
  connectCloudflare,
  disconnectCloudflare,
} from '../integrationsApi';
import { fetchMcpServers, addMcpServer, removeMcpServer, loginMcpServer, logoutMcpServer, fetchMcpCatalog, installMcpCatalogEntry } from '../mcpApi';
import type { McpTransport, McpServerSummary, McpCatalogEntry } from '../mcpApi';
import { navigate } from '../route';
import { renderDevToolsView } from './devTools';

const TAB_META: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  { key: 'github', label: 'GitHub 설정' },
  { key: 'cloudflare', label: 'Cloudflare 설정' },
  { key: 'marketplace', label: '마켓플레이스 설정' },
  { key: 'mcp', label: 'MCP 관리' },
  { key: 'applinks', label: '앱링크 설정' },
  { key: 'devtools', label: '개발 환경' },
];

// 탭 전환 자체는 사이드바 하위메뉴가 담당한다(U-15) — 여기서는 동일한 8항목을
// 상단에 칩 탭으로 다시 그리지 않는다(두 벌 내비게이션 + 이중 하이라이트 제거).
// TAB_META는 현재 탭 이름을 페이지 부제로 보여주는 용도로만 남는다.
export function renderSettingsView(tab: SettingsTab): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [el('h1', { className: 'page-title' }, ['설정']), el('div', { className: 'page-subtitle' }, [TAB_META.find((t) => t.key === tab)?.label ?? ''])]),
  ]);

  let body: HTMLElement;
  if (tab === 'otel') body = renderOtelPanel();
  else if (tab === 'github') body = renderGithubPanel();
  else if (tab === 'cloudflare') body = renderCloudflarePanel();
  else if (tab === 'marketplace') body = renderMarketplacePanel();
  else if (tab === 'applinks') body = renderAppLinksPanel();
  else if (tab === 'devtools') body = renderDevToolsView();
  else body = renderMcpPanel();

  return el('div', {}, [header, body]);
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
// readOnlyKeys(프라이버시 4키)는 이 수동 저장 폼에서는 입력을 비활성화하고
// payload에도 포함하지 않는다(백엔드는 "0"/빈 값 외에는 Err를 던진다 —
// ensureOtelAutoConfigured()가 미설정 시 "0" 기본값을 채우는 것은 예외).
// OTEL_RESOURCE_ATTRIBUTES는 employee.* 조립이 전적으로 Rust 책임이라(§8)
// 편집 불가한 읽기 전용 안내로만 보여준다.

export async function loadOtelEnv(): Promise<void> {
  state.otel.loading = true;
  state.otel.error = null;
  notifyChange();
  try {
    state.otel.settings = await fetchOtelSettings();
    state.otel.loaded = true;
  } catch (err) {
    state.otel.error = err instanceof Error ? err.message : 'OTel 설정을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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

// ---------------- OTel 자동 세팅 (설정이 한 번도 없었을 때만, 세션당 1회) ----------------
// 화면 진입만으로는 저장을 호출하지 않는다는 위 원칙(51~53행)은 "설정 화면에
// 들어갔을 때"에 대한 것이고, 이 함수는 그 화면에 아예 들어가지 않은 사용자를
// 위한 별도 경로다 — 아래 3개 조건을 전부 만족할 때만, 딱 한 번 자동 저장한다.
// 호출 시점(세션당 1회 트리거)은 main.ts의 handleNavigation()이 담당한다.
export async function ensureOtelAutoConfigured(): Promise<void> {
  let settings: OtelSettings;
  try {
    // state.otel을 오염시키지 않으려고 이 체크 전용 지역 변수로만 받는다 —
    // 사용자가 마침 설정 > OTel 탭에 있다면 화면과 결과가 다를 수 있는 드문
    // 경합은 감수한다.
    settings = await fetchOtelSettings();
  } catch {
    return; // Tauri IPC 브리지가 없는 환경 등 — 조용히 건너뛴다.
  }

  // 조건 1: settings.json 자체가 정상 파싱됨(깨진 파일은 절대 건드리지 않는다).
  if (settings.parseError !== null) return;
  // 조건 2: 사내 collector 주소가 이 빌드에 실제로 주입돼 있음(포크·시크릿 없는
  // CI 빌드에서는 빈 엔드포인트로 텔레메트리를 켜봐야 아무 데도 못 보낸다).
  if (!settings.endpointDefaultsInjected) return;
  // 조건 3: 이 키가 파일에 아예 없음 = 한 번도 설정한 적 없다는 뜻. 값이 이미
  // 있으면(0이든 1이든) 사용자의 명시적 선택이므로 절대 덮지 않는다.
  if ('CLAUDE_CODE_ENABLE_TELEMETRY' in settings.values) return;

  const values: Record<string, string> = {};
  for (const key of settings.managedKeys) {
    if (key === 'OTEL_RESOURCE_ATTRIBUTES') continue;
    if (settings.readOnlyKeys.includes(key)) {
      // 프라이버시 4키는 아직 한 번도 설정되지 않았을 때만(=파일에 키 자체가
      // 없을 때만) 기본값(0)을 채운다. 이미 값이 있으면(예: 수동 편집으로 1)
      // 다시 보내지 않는다 — 백엔드는 readOnly 키를 0/빈 값으로만 받아주므로,
      // 기존 값을 그대로 재전송하면 1인 경우 전체 저장이 거부된다.
      if (key in settings.values) continue;
      const value = settings.defaults[key];
      if (value === undefined) continue;
      values[key] = value;
      continue;
    }
    const value = key in settings.values ? settings.values[key] : settings.defaults[key];
    if (value === undefined) continue;
    values[key] = value;
  }

  // 로그인 전이어도 텔레메트리 기본 키는 켤 수 있다 — resource attributes만
  // 나중에 로그인 후 사용자가 설정 화면에서 저장할 때 자연히 붙는다.
  const identity = state.auth.userEmail ? { email: state.auth.userEmail, name: state.auth.userName } : null;

  try {
    const result = await saveOtelSettings({ values, identity });
    showToast('OTel 텔레메트리를 기본값으로 자동 설정했습니다. 설정 > OTel 설정에서 확인·변경할 수 있습니다.');
    // 사용자가 마침 설정 > OTel 탭을 보고 있었다면 화면도 최신 상태로 갱신한다.
    if (state.otel.settings) {
      state.otel.settings = result;
      notifyChange();
    }
  } catch (err) {
    // 자동화 실패로 사용자를 방해하지 않는다 — 토스트 없이 콘솔에만 남긴다.
    console.error('OTel 자동 설정 실패:', err);
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
    state.github.error = err instanceof Error ? err.message : 'GitHub 연동 상태를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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
  if (state.github.loading && !state.github.loaded) return loadingBlock();
  if (state.github.error) return errorBlock(state.github.error, () => void loadGithubStatus());

  const status = state.github.status;
  // loading은 최초 로드뿐 아니라 이 "상태 새로고침" 버튼을 눌렀을 때도 true가
  // 된다(loadGithubStatus) — loaded=true 이후에는 전체 패널을 loadingBlock()으로
  // 갈아치우지 않으므로, 버튼 자체에 진행 중 피드백을 준다(마켓플레이스 새로고침과
  // 동일 패턴).
  const refreshBtn = el(
    'button',
    { className: 'btn', disabled: state.github.loading, onClick: () => void loadGithubStatus() },
    [state.github.loading ? '새로고침 중…' : '상태 새로고침']
  );

  if (!status || !status.installed) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        'GitHub CLI(gh)가 설치되어 있지 않습니다 → 개발 환경 화면에서 설치 상태를 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el('button', { className: 'btn btn-primary', onClick: () => navigate('#/settings/devtools') }, ['개발 환경 화면으로 이동']),
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
    state.cloudflare.error = err instanceof Error ? err.message : 'Cloudflare 연동 상태를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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
  if (state.cloudflare.loading && !state.cloudflare.loaded) return loadingBlock();
  if (state.cloudflare.error) return errorBlock(state.cloudflare.error, () => void loadCloudflareStatus());

  const status = state.cloudflare.status;
  // GitHub 패널과 동일한 이유로 loading 중에는 버튼 자체가 진행 중임을 알린다.
  const refreshBtn = el(
    'button',
    { className: 'btn', disabled: state.cloudflare.loading, onClick: () => void loadCloudflareStatus() },
    [state.cloudflare.loading ? '새로고침 중…' : '상태 새로고침']
  );

  if (!status || !status.installed) {
    return el('div', { className: 'settings-card integration-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        'Wrangler CLI가 설치되어 있지 않습니다 → 개발 환경 화면에서 설치 상태를 확인하세요.',
      ]),
      el('div', { className: 'settings-form-actions' }, [
        el('button', { className: 'btn btn-primary', onClick: () => navigate('#/settings/devtools') }, ['개발 환경 화면으로 이동']),
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

// malgn-agent 플러그인이 배포되는 회사 마켓플레이스 — catalog.ts의
// DEFAULT_PLUGIN_ID("malgn-agent@malgnsoft-plugins")에서 "@" 뒤쪽 이름만 뽑는다.
// 45명 사용자에게 이 URL을 직접 타이핑시키지 않고 추천 블록(원클릭 추가) 노출
// 여부를 판단하는 용도로만 쓴다 — 이전 커밋의 PINNED_MARKETPLACE_ID처럼 제거를
// 막거나 "필수" 배지를 붙이는 특별 취급은 하지 않는다(사용자 결정: 추가 후에도
// 다른 마켓플레이스와 동등하게 제거 가능해야 한다). 백엔드(marketplace.rs)도
// 더는 이 이름을 특별 취급하지 않는다.
const COMPANY_MARKETPLACE_ID = DEFAULT_PLUGIN_ID.split('@')[1] ?? 'malgnsoft-plugins';
// public 저장소 주소라 시크릿이 아니다(README에도 같은 값이 실릴 수 있는 수준) —
// 소스코드에 리터럴로 두어도 된다.
const COMPANY_MARKETPLACE_URL = 'https://github.com/malgnsoft/claude-plugins';

// U-05 계열(빈 값·공백만·개행 거부)을 프런트에서 먼저 막는다 — 백엔드
// (marketplace.rs:validate_marketplace_field)가 동일 규칙을 다시 검증하므로
// 프런트 검증을 우회해도(devtools invoke 직접 호출 등) 최종적으로 거부된다.
function validateMarketplaceInput(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  if (/[\r\n]/.test(trimmed)) return null;
  return trimmed;
}

async function handleAddMarketplace(source: string): Promise<void> {
  state.marketplaces.adding = true;
  notifyChange();
  try {
    const result = await addMarketplace(source);
    if (result.success) {
      showToast(`마켓플레이스를 추가했습니다: ${result.message}`);
      await loadMarketplaces();
      await loadCatalog();
    } else {
      // 실패 원인 원문을 그대로 보여준다(home.ts의 widgetErrorShell 선례) —
      // "실패했습니다"로만 뭉개면 잘못된 URL인지 네트워크 문제인지 알 수 없다.
      showToast(`마켓플레이스 추가 실패: ${result.message}`);
    }
  } catch (err) {
    showToast(`마켓플레이스 추가 실패: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.marketplaces.adding = false;
    notifyChange();
  }
}

async function handleRemoveMarketplace(marketplace: MarketplaceInfo): Promise<void> {
  // 파괴적 동작이라 confirmDialog로 확인을 받는다(views/settings.ts의
  // handleRemoveMcp와 동일 패턴) — 잘못 추가한 소스를 되돌릴 수 있어야 하지만,
  // 실수로 지우는 것은 막는다.
  if (
    !(await confirmDialog(`"${marketplace.id}" 마켓플레이스를 제거할까요? 이 마켓플레이스에서 설치한 플러그인은 더 이상 업데이트되지 않습니다.`, {
      danger: true,
    }))
  ) {
    return;
  }
  state.marketplaces.removingId = marketplace.id;
  notifyChange();
  try {
    const result = await removeMarketplace(marketplace.id);
    if (result.success) {
      showToast(`"${marketplace.id}" 마켓플레이스를 제거했습니다.`);
      await loadMarketplaces();
      await loadCatalog();
    } else {
      showToast(`마켓플레이스 제거 실패: ${result.message}`);
    }
  } catch (err) {
    showToast(`마켓플레이스 제거 실패: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.marketplaces.removingId = null;
    notifyChange();
  }
}

// 인라인 추가 폼 — MCP 관리 탭의 "새 MCP 서버" 모달과 달리 필드가 하나뿐이라
// 모달 없이 카드 상단에 항상 노출한다. 제출 성공 시 loadMarketplaces()로 목록을
// 다시 읽어오면 이 함수가 통째로 재렌더되어 입력값도 자연히 비워진다.
function renderMarketplaceAddForm(): HTMLElement {
  const input = document.createElement('input');
  input.className = 'settings-input';
  input.type = 'text';
  input.placeholder = '예: https://github.com/acme/plugins, ./local/marketplace';
  input.autocomplete = 'off';
  input.disabled = state.marketplaces.adding;

  const addBtn = el(
    'button',
    { className: 'btn btn-primary', disabled: state.marketplaces.adding },
    [state.marketplaces.adding ? '추가 중…' : '+ 추가']
  );
  addBtn.type = 'submit';

  const form = el('form', { className: 'settings-form marketplace-add-form' }, [
    el('label', { className: 'settings-field' }, [
      el('span', { className: 'settings-field-label' }, ['새 마켓플레이스 소스 (URL·경로·GitHub repo)']),
      input,
    ]),
    el('div', { className: 'settings-form-actions' }, [addBtn]),
  ]);

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const source = validateMarketplaceInput(input.value);
    if (!source) {
      showToast(input.value.trim() ? '마켓플레이스 소스에 개행 문자를 포함할 수 없습니다.' : '마켓플레이스 소스를 입력하세요.');
      return;
    }
    void handleAddMarketplace(source);
  });

  return form;
}

// 45명 사용자가 회사 마켓플레이스 URL을 직접 타이핑하지 않도록, 아직 등록되지
// 않았을 때만 원클릭 추가 추천을 보여준다. 이미 등록돼 있으면(대부분의 경우 —
// malgn-agent 자체가 이 마켓플레이스에서 온다) null을 돌려줘 잡음을 만들지
// 않는다. 클릭 시 renderMarketplaceAddForm의 입력 폼을 거치지 않고
// handleAddMarketplace(URL)을 바로 호출한다 — addMarketplace(catalogApi.ts)와
// 백엔드 검증(marketplace.rs)을 그대로 재사용하는 동일 경로다.
function renderMarketplaceRecommendBlock(): HTMLElement | null {
  const alreadyAdded = state.marketplaces.items.some((m) => m.id === COMPANY_MARKETPLACE_ID);
  if (alreadyAdded) return null;

  const addBtn = el(
    'button',
    {
      className: 'btn btn-primary',
      disabled: state.marketplaces.adding,
      onClick: () => void handleAddMarketplace(COMPANY_MARKETPLACE_URL),
    },
    // 버튼 라벨은 짧게(맑은소프트라는 사실은 옆 문구가 이미 설명한다) — .alert는
    // 좁은 flex 행이라 긴 라벨이 여러 줄로 쪼개져 읽기 어려워진다(캡처로 실측).
    [state.marketplaces.adding ? '추가 중…' : '+ 마켓플레이스 추가']
  );
  // styles.css는 수정 범위 밖이라, 버튼이 글자 단위로 줄바꿈되던 문제(캡처로
  // 실측)는 다른 el() 호출부(removeBtn.style.color 등)와 같은 인라인 스타일로
  // 고친다 — flex item 기본 min-width:auto 때문에 텍스트가 줄바꿈 가능한
  // 상태에서는 버튼이 라벨 폭보다 더 줄어들 수 있다. nowrap을 주면 버튼은
  // 라벨 전체 폭을 최소 폭으로 확보하고, 대신 옆 span(줄바꿈 가능한 문단)이
  // 줄어든다.
  addBtn.style.whiteSpace = 'nowrap';
  addBtn.style.flexShrink = '0';

  return el('div', { className: 'alert' }, [
    el('span', {}, [
      `맑은소프트 마켓플레이스를 추가하시겠습니까? 이 저장소(${COMPANY_MARKETPLACE_URL.replace('https://', '')})는 지금 쓰고 있는 맑은에이전트(malgn-agent) 플러그인의 출처입니다. 등록해 두면 URL을 직접 입력하지 않고도 사내 플러그인을 마켓플레이스에서 찾아 설치·업데이트할 수 있습니다.`,
    ]),
    addBtn,
  ]);
}

function renderMarketplacePanel(): HTMLElement {
  if (state.marketplaces.loading && !state.marketplaces.loaded) return loadingBlock();
  if (state.marketplaces.error) return errorBlock(state.marketplaces.error, () => void loadMarketplaces());

  const repoRows =
    state.marketplaces.items.length > 0
      ? state.marketplaces.items.map((m) => {
          const removing = state.marketplaces.removingId === m.id;
          // 회사 마켓플레이스(COMPANY_MARKETPLACE_ID)도 다른 소스와 동등하게
          // 취급한다 — "필수" 배지·제거 버튼 숨김을 두지 않는다(사용자 결정:
          // 추가 후에도 원치 않으면 제거할 수 있어야 한다). 백엔드도 더는 이
          // 이름을 특별 취급해 제거를 거부하지 않는다(marketplace.rs 참고).
          const removeBtn = el(
            'button',
            { className: 'btn', disabled: removing, onClick: () => void handleRemoveMarketplace(m) },
            [removing ? '제거 중…' : '제거']
          );
          removeBtn.style.color = 'var(--color-danger)';
          const actions: HTMLElement[] = [el('span', { className: 'badge badge-active' }, ['연결됨']), removeBtn];
          return el('div', { className: 'marketplace-repo-row' }, [
            el('div', {}, [
              el('div', { className: 'marketplace-repo-name' }, [m.id]),
              el('div', { className: 'marketplace-repo-path' }, [m.repo ? `github.com/${m.repo}` : '저장소 정보 없음']),
              ...(m.lastUpdated ? [el('div', { className: 'marketplace-repo-updated' }, [`마지막 업데이트: ${formatIsoDate(m.lastUpdated)}`])] : []),
            ]),
            el('div', { className: 'mcp-row-actions' }, actions),
          ]);
        })
      : [el('div', { className: 'state-block-desc' }, ['등록된 마켓플레이스가 없습니다.'])];

  const refreshBtn = el(
    'button',
    { className: 'btn', disabled: state.marketplaces.refreshing, onClick: () => void handleRefreshMarketplaces() },
    [state.marketplaces.refreshing ? '새로고침 중…' : '↻ 마켓플레이스 새로고침']
  );

  // 자동 업데이트 토글·저장 버튼은 저장되지 않는 로컬 UI 상태였던 목업이라
  // 제거했다 — 이 목록은 설치된 플러그인 이름·버전만 보여주는 조회 전용이다.
  const description = el('div', { className: 'settings-form-hint' }, [
    '카탈로그가 플러그인을 받아오는 저장소를 관리합니다. 아래에서 새 저장소(사내 자체 저장소 등)를 추가하거나 등록된 저장소를 제거할 수 있습니다.',
  ]);

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
          ])
        )
      : [el('div', { className: 'state-block-desc' }, ['설치된 플러그인이 없습니다.'])]
  );

  const recommendBlock = renderMarketplaceRecommendBlock();

  return el('div', { className: 'settings-card marketplace-panel' }, [
    description,
    ...(recommendBlock ? [recommendBlock] : []),
    el('div', { className: 'marketplace-repo-list' }, repoRows),
    el('div', { className: 'marketplace-actions' }, [refreshBtn]),
    renderMarketplaceAddForm(),
    el('div', { className: 'settings-field-label marketplace-list-label' }, ['설치된 플러그인']),
    pluginList,
  ]);
}

// ---------------- MCP 관리 (claude mcp CLI 위임, 읽기+추가/삭제) ----------------
// 목록/상세는 `claude mcp` CLI를 실제로 위임 실행한 결과다(mcpApi.ts). 모델을
// 호출하지 않는 순수 헬스체크라 빠르고 무료다. 삭제는 실제로 서버 등록을
// 지우므로 확인(confirmDialog) 후에만 실행한다. 추가 폼 펼침 상태는 다른
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
    state.mcp.error = err instanceof Error ? err.message : 'MCP 서버 목록을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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
    state.mcpCatalog.error = err instanceof Error ? err.message : 'MCP 카탈로그를 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
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

// 이미 등록된 경우(state.mcp.items에 정규화된 target이 있음)에는
// buildInstallableMcpRows가 이 행 자체를 만들지 않는다(V-05: 끝 슬래시·대소문자
// 차이는 normalizeMcpTarget이 흡수한다) — 그래서 "이미 등록됨" 분기가 필요 없다.
function renderGithubMcpQuickstartRow(): HTMLElement {
  const actionEl = el(
    'button',
    {
      className: 'btn btn-primary',
      onClick: () => openMcpAddModal({ name: 'GitHub', transport: 'http', target: GITHUB_MCP_TARGET }),
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

// ---------------- Telegram MCP 빠른시작 (stdio, npx 실행) ----------------
// mcp-telegram-agent는 공식 벤더가 호스팅하는 원격 서버가 없어 카탈로그
// (원클릭 OAuth 설치)에는 넣지 않는다 — GitHub 퀵스타트와 같은 패턴으로 수동
// 추가 폼을 이름/transport/target/args/env까지 미리 채워서 열어주고, 사용자는
// 본인의 봇 토큰·챗 ID만 입력해 저장하면 된다.
const TELEGRAM_MCP_DISPLAY_TARGET = 'npx -y mcp-telegram-agent';

// stdio 서버는 target이 CLI가 재작성한 실행 경로로 바뀔 수 있어(예: npx가
// 절대경로로 해석됨) URL 문자열 매칭이 신뢰할 수 없다 — 이름으로 이미
// 등록됐는지 판단한다(buildInstallableMcpRows 참고).
function renderTelegramMcpQuickstartRow(): HTMLElement {
  const actionEl = el(
    'button',
    {
      className: 'btn btn-primary',
      onClick: () =>
        openMcpAddModal({
          name: 'Telegram',
          transport: 'stdio',
          target: 'npx',
          args: '-y mcp-telegram-agent',
          env: 'BOT_TELEGRAM_TOKEN=\nBOT_TELEGRAM_CHAT_ID=',
        }),
    },
    ['설치']
  );

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, ['Telegram']),
        el('span', { className: 'badge badge-unknown' }, ['stdio']),
      ]),
      el('div', { className: 'mcp-row-target' }, [TELEGRAM_MCP_DISPLAY_TARGET]),
    ]),
    el('div', { className: 'mcp-row-actions' }, [actionEl]),
  ]);
}

// ---------------- 등록 폼 모달 ----------------
// sessions.ts의 renderMetaModal / appLinks.ts의 renderLinkFormModal과 동일한
// "모듈 로컬 열림상태 + ESC 리스너 attach/detach" 패턴. 등록 폼은 이 화면에
// 유일한 인라인 편집 폼이었고(수정 폼은 없음), 이번에 모달로 전환한다 —
// GitHub 공식 MCP 빠른시작(renderGithubMcpQuickstartRow)·Telegram 빠른시작
// (renderTelegramMcpQuickstartRow)이 prefill을 채워 여는 진입점도 함께 옮긴다.
let mcpAddModalOpen = false;
let mcpAddPrefill: McpAddPrefill | null = null;
let mcpAddModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachMcpAddModalEscHandler(): void {
  if (mcpAddModalEscHandler) {
    window.removeEventListener('keydown', mcpAddModalEscHandler);
    mcpAddModalEscHandler = null;
  }
}

function openMcpAddModal(prefill: McpAddPrefill | null): void {
  mcpAddPrefill = prefill;
  mcpAddModalOpen = true;
  notifyChange();
}

function closeMcpAddModal(): void {
  mcpAddModalOpen = false;
  mcpAddPrefill = null;
  detachMcpAddModalEscHandler();
  notifyChange();
}

function renderMcpAddModalIfOpen(): HTMLElement | null {
  if (!mcpAddModalOpen) {
    detachMcpAddModalEscHandler();
    return null;
  }
  if (!mcpAddModalEscHandler) {
    mcpAddModalEscHandler = (e) => {
      if (e.key === 'Escape') closeMcpAddModal();
    };
    window.addEventListener('keydown', mcpAddModalEscHandler);
  }
  return renderMcpAddModal(mcpAddPrefill ?? undefined);
}

// 설정 화면에서 MCP 관리 탭을 벗어날 때(다른 탭으로 이동하거나 라우트를 완전히
// 떠날 때) main.ts에서 호출한다 — 열려 있던 "새 MCP 서버 등록" 모달이 있었다면
// window에 남은 ESC 리스너를 정리한다(leaveProjectsListView/
// leaveAutonomousTasksListView와 동일한 원칙).
export function leaveMcpSettingsView(): void {
  mcpAddModalOpen = false;
  mcpAddPrefill = null;
  detachMcpAddModalEscHandler();
}

// createModalOverlay로 배경 클릭·ESC·닫기 버튼 3가지 경로로 닫힌다(다른
// 화면들과 동일한 modal-overlay/modal-box/modal-header+modal-close-btn/
// modal-body 구조).
function renderMcpAddModal(prefill?: McpAddPrefill): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['새 MCP 서버 등록']),
      el('button', { className: 'modal-close-btn', onClick: closeMcpAddModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderMcpAddForm(prefill)]),
  ]);
  return createModalOverlay(modalBox, closeMcpAddModal);
}

async function handleRemoveMcp(server: McpServerSummary): Promise<void> {
  if (!(await confirmDialog(`"${server.name}" MCP 서버를 삭제할까요? 이 작업은 되돌릴 수 없습니다.`, { danger: true }))) return;
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

// claude.ai 커넥터(Gmail/Drive/Calendar/Atlassian Rovo 등)처럼 계정 단위로
// 연결돼 로컬 PC와 무관한 서버를 평소엔 해제해두고 필요할 때만 연결하려는
// 사용자를 위한 버튼 — 서버 등록은 그대로 두고 저장된 OAuth 자격증명만
// 지운다(mcp_logout). 터미널을 열지 않는 즉시 완료 동작이라 성공/실패를
// 바로 토스트로 알리고 목록을 새로고침한다.
async function handleLogoutMcp(server: McpServerSummary): Promise<void> {
  state.mcp.loggingOutName = server.name;
  notifyChange();
  try {
    await logoutMcpServer(server.name);
    showToast(`"${server.name}" 연결을 해제했습니다.`);
    await loadMcp();
  } catch (err) {
    showToast(`"${server.name}" 연결 해제에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.mcp.loggingOutName = null;
    notifyChange();
  }
}

// malgn-agent 플러그인 설치 시 자동 등록되는 필수 MCP 서버 — malgn-agent.md
// "역할 경계" 지시에 따라 고정 표시(삭제 불가)한다.
const MALGNAI_HUB_MCP_NAME = 'plugin:malgn-agent:malgnai-hub';

function renderMcpRow(server: McpServerSummary, opts?: { readonly locked?: boolean }): HTMLElement {
  const locked = opts?.locked ?? false;
  const deleteBtn = el('button', { className: 'btn', onClick: () => void handleRemoveMcp(server) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';

  const actions: HTMLElement[] = [];
  // stdio 서버는 OAuth 로그인 개념이 없다 — http/sse에만 노출한다. 이미 연결된
  // 서버든 아니든(재로그인 필요할 수 있다) 항상 보여준다.
  if (server.transport !== 'stdio') {
    const loggingIn = state.mcp.loggingInName === server.name;
    // 라벨만 연결 여부에 따라 "인증"/"재인증"으로 구분한다 — 동작(claude mcp
    // login 재실행)은 동일하다. "연결됨"은 CLI 헬스체크일 뿐 토큰 유효성을
    // 보장하지 않으므로, 연결된 행에서도 버튼 자체는 항상 노출한다.
    const loginLabel = server.connected ? '재인증' : '인증';
    const loginBtn = el(
      'button',
      { className: 'btn', disabled: loggingIn, onClick: () => void handleLoginMcp(server) },
      [loggingIn ? '터미널 여는 중…' : loginLabel]
    );
    actions.push(loginBtn);

    // claude.ai 커넥터처럼 계정 단위로 연결돼 이 PC와 무관한 서버는 평소엔
    // 해제해두고 필요할 때만 연결하는 게 자연스럽다 — 연결된 행에만 "해제"를
    // 추가로 노출한다(서버 등록은 유지, 자격증명만 지운다).
    if (server.connected) {
      const loggingOut = state.mcp.loggingOutName === server.name;
      const logoutBtn = el(
        'button',
        { className: 'btn', disabled: loggingOut, onClick: () => void handleLogoutMcp(server) },
        [loggingOut ? '해제 중…' : '해제']
      );
      actions.push(logoutBtn);
    }
  }
  // 필수 고정 서버(malgnai-hub)는 삭제 버튼 자체를 숨긴다 — 조직 표준 MCP라
  // 이 화면에서 등록을 해제할 수 없다(플러그인 삭제로만 함께 사라진다).
  if (!locked) actions.push(deleteBtn);

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, [server.name]),
        el('span', { className: 'badge badge-unknown' }, [server.transport]),
        el('span', { className: `badge ${server.connected ? 'badge-active' : 'badge-archived'}` }, [server.connected ? '연결됨' : '미연결']),
        ...(locked ? [el('span', { className: 'badge badge-active' }, ['필수'])] : []),
      ]),
      el('div', { className: 'mcp-row-target' }, [server.target]),
      el('div', { className: 'mcp-row-status-label' }, [server.statusLabel]),
    ]),
    el('div', { className: 'mcp-row-actions' }, actions),
  ]);
}

// malgn-agent 플러그인이 아직 설치되지 않아 malgnai-hub 서버가 mcp 목록에
// 없을 때, 등록된 서버 목록 최상단(실제 행이 있었을 자리)에 대신 보여주는
// 안내 행 — "플러그인 설치"를 누르면 catalog.ts와 동일한 설치 흐름
// (installPlugin(DEFAULT_PLUGIN_ID))을 재사용해 실행한다.
function renderMalgnaiHubMissingRow(): HTMLElement {
  const installing = state.catalog.installingDefault;
  const installBtn = el(
    'button',
    { className: 'btn btn-primary', disabled: installing, onClick: () => void handleInstallMalgnAgentPluginForMcp() },
    [installing ? '설치 중…' : '플러그인 설치']
  );

  return el('div', { className: 'mcp-row' }, [
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, ['malgnai-hub']),
        el('span', { className: 'badge badge-active' }, ['필수']),
      ]),
      el('div', { className: 'mcp-row-target' }, ['malgn-agent 플러그인을 설치하면 자동으로 등록되고 인증할 수 있습니다.']),
    ]),
    el('div', { className: 'mcp-row-actions' }, [installBtn]),
  ]);
}

// catalog.ts의 handleInstallDefaultPlugin과 동일한 상태(state.catalog.installingDefault/
// installDefaultResult)를 공유해 같은 설치 실행 흐름을 재사용한다 — 새 백엔드
// 커맨드나 새 로딩 상태를 만들지 않는다. 성공 시 카탈로그와 MCP 목록을 함께
// 새로고침해야 이 화면에서 malgnai-hub 행이 곧바로 실제 서버 행으로 바뀐다.
async function handleInstallMalgnAgentPluginForMcp(): Promise<void> {
  state.catalog.installingDefault = true;
  state.catalog.installDefaultResult = null;
  notifyChange();
  try {
    const result = await installPlugin(DEFAULT_PLUGIN_ID);
    state.catalog.installDefaultResult = result;
    showToast(
      result.success
        ? 'malgn-agent 설치 완료 — 적용하려면 Claude Code를 재시작하세요'
        : `malgn-agent 설치 실패: ${result.message}`
    );
    if (result.success) {
      await loadCatalog();
      await loadMcp();
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    state.catalog.installDefaultResult = { success: false, message };
    showToast(`malgn-agent 설치 실패: ${message}`);
  } finally {
    state.catalog.installingDefault = false;
    notifyChange();
  }
}

// ---------------- 목록 (등록된 서버 / 설치 가능한 서버 두 섹션) ----------------
// U-16: 등록된 서버·미설치 카탈로그·GitHub 퀵스타트를 한 목록에 섞어 두면
// "github"(등록됨)와 "GitHub"(카탈로그)가 나란히 떠 중복처럼 보인다. catalog.ts가
// 이미 쓰는 2섹션 패턴(설치된 것 / 설치 가능한 것)을 그대로 복제해 분리한다.
// malgn-agent 관련 항목(malgnai-hub 등)은 각 섹션 안에서 맨 위로 올린다
// (Array.sort는 ES2019+ 스펙상 안정 정렬이라 동순위 항목의 상대 순서가 보존된다).

function isMalgnAgentEntry(name: string): boolean {
  return name.includes('malgn-agent');
}

function sortMalgnAgentFirst<T extends { name: string }>(rows: T[]): T[] {
  return [...rows].sort((a, b) => {
    const aTop = isMalgnAgentEntry(a.name);
    const bTop = isMalgnAgentEntry(b.name);
    if (aTop === bTop) return 0;
    return aTop ? -1 : 1;
  });
}

// V-05: 끝 슬래시·대소문자 차이만으로 정확일치 비교가 실패해 이미 등록된 GitHub
// MCP 서버와 퀵스타트 행이 중복으로 뜨는 문제 — 비교 전 정규화한다.
function normalizeMcpTarget(url: string): string {
  return url.replace(/\/+$/, '').toLowerCase();
}

// malgnai-hub는 sortMalgnAgentFirst의 일반 규칙(이름에 "malgn-agent" 포함 시
// 맨 위)에 맡기지 않고 항상 명시적으로 최상단에 고정한다 — 설치돼 있으면
// 잠긴(삭제 불가) 실제 행을, 없으면 같은 위치에 설치 안내 행을 보여준다.
function buildRegisteredMcpRows(): HTMLElement[] {
  const hubServer = state.mcp.items.find((s) => s.name === MALGNAI_HUB_MCP_NAME);
  const otherServers = state.mcp.items.filter((s) => s.name !== MALGNAI_HUB_MCP_NAME);
  const otherRows = sortMalgnAgentFirst(otherServers.map((server) => ({ name: server.name, el: renderMcpRow(server) }))).map((r) => r.el);
  const pinnedRow = hubServer ? renderMcpRow(hubServer, { locked: true }) : renderMalgnaiHubMissingRow();
  return [pinnedRow, ...otherRows];
}

function buildInstallableMcpRows(): HTMLElement[] {
  const rows: { name: string; el: HTMLElement }[] = [];

  for (const entry of state.mcpCatalog.items) {
    if (!entry.installed) rows.push({ name: entry.label, el: renderMcpCatalogRow(entry) });
  }

  const githubAlreadyRegistered = state.mcp.items.some(
    (item) => normalizeMcpTarget(item.target) === normalizeMcpTarget(GITHUB_MCP_TARGET)
  );
  if (!githubAlreadyRegistered) {
    rows.push({ name: 'GitHub', el: renderGithubMcpQuickstartRow() });
  }

  // stdio 서버는 target이 CLI 실행 경로로 재작성될 수 있어 URL 매칭이
  // 신뢰할 수 없다 — 이름으로 이미 등록됐는지 판단한다.
  const telegramAlreadyRegistered = state.mcp.items.some((item) => item.name.toLowerCase() === 'telegram');
  if (!telegramAlreadyRegistered) {
    rows.push({ name: 'Telegram', el: renderTelegramMcpQuickstartRow() });
  }

  return sortMalgnAgentFirst(rows).map((r) => r.el);
}

interface McpAddPrefill {
  readonly name: string;
  readonly transport: McpTransport;
  readonly target: string;
  readonly args?: string;
  readonly env?: string;
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
  if (prefill?.args) argsInput.value = prefill.args;
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
  if (prefill?.env) envInput.value = prefill.env;
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
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['전송 방식 (transport)']), transportSelect]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['대상 (target)']), targetInput]),
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
        closeMcpAddModal();
        await loadMcp();
      } catch (err) {
        showToast(`추가에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
        saveBtn.disabled = false;
        notifyChange();
      }
    })();
  });

  return form;
}

function renderMcpPanel(): HTMLElement {
  if (state.mcp.loading && !state.mcp.loaded) return loadingBlock();
  if (state.mcp.error) return errorBlock(state.mcp.error, () => void loadMcp());

  // claude mcp list는 등록된 서버마다 순차 헬스체크를 하므로 서버가 여러 개면
  // 새로고침 한 번에 수 초~수십 초가 걸릴 수 있다 — 최초 로드 이후(loaded=true)에는
  // 전체 패널이 loadingBlock()으로 바뀌지 않으므로, 이 버튼이 disabled+라벨
  // 변경으로 "지금 실제로 재조회 중"임을 알려야 한다(안 그러면 멈춘 것처럼 보임).
  // 카탈로그도 같은 버튼이 함께 누르므로 두 로딩 상태를 함께 본다.
  const mcpRefreshing = state.mcp.loading || state.mcpCatalog.loading;
  const refreshBtn = el(
    'button',
    { className: 'btn', disabled: mcpRefreshing, onClick: () => { void loadMcp(); void loadMcpCatalog(); } },
    [mcpRefreshing ? '새로고침 중…' : '↻ 새로고침']
  );
  const addToggleBtn = el('button', { className: 'btn btn-primary', onClick: () => openMcpAddModal(null) }, ['+ 새 MCP 서버']);

  const body: HTMLElement[] = [
    el('div', { className: 'settings-form-hint' }, [
      'claude mcp CLI로 연결 상태만 확인합니다 — 모델을 호출하지 않는 순수 헬스체크라 빠르고 비용이 들지 않습니다. 카탈로그 항목은 OAuth 로그인이, GitHub 공식 MCP는 Personal Access Token이 필요합니다. "설치"/"인증"을 누르면 터미널 창이 열리고, 그 창에서 절차를 직접 마친 뒤 "↻ 새로고침"으로 반영하세요.',
    ]),
    el('div', { className: 'mcp-toolbar' }, [refreshBtn, addToggleBtn]),
  ];

  if (state.mcpCatalog.error) {
    body.push(errorBlock(`MCP 카탈로그를 불러오지 못했습니다: ${state.mcpCatalog.error}`, () => void loadMcpCatalog()));
  }

  // U-16: 등록된 서버와 설치 가능한 카탈로그를 섹션 헤더 2개로 분리한다
  // (catalog.ts의 "설치된 플러그인" / "전역 에이전트·스킬" 2섹션 패턴과 동일).
  // malgnai-hub는 실제 등록 여부와 무관하게 항상 최상단에 한 행을 차지하므로
  // (설치돼 있으면 잠긴 실제 행, 없으면 설치 안내 행), 섹션 라벨의 개수는
  // registeredRows.length가 아니라 실제 등록된 서버 수(state.mcp.items.length)를
  // 그대로 보여준다.
  const registeredRows = buildRegisteredMcpRows();
  const installableRows = buildInstallableMcpRows();
  const catalogStillLoading = state.mcpCatalog.loading && !state.mcpCatalog.loaded;

  // 새로고침 중에는 목록을 살짝 흐리게 해 "재조회가 실제로 진행 중"임을
  // 버튼 라벨 외에도 목록 자체에서 확인할 수 있게 한다(새 애니메이션 없이
  // opacity만, styles.css .is-refreshing).
  const listClassName = `mcp-list${mcpRefreshing ? ' is-refreshing' : ''}`;

  body.push(el('div', { className: 'plugin-section-label' }, [`등록된 서버 ${state.mcp.items.length}개`]));
  body.push(el('div', { className: listClassName }, registeredRows));

  if (installableRows.length > 0 || catalogStillLoading) {
    body.push(el('div', { className: 'plugin-section-label' }, ['설치 가능한 서버']));
  }
  if (installableRows.length > 0) {
    body.push(el('div', { className: listClassName }, installableRows));
  }
  // 카탈로그는 등록된 서버와 별도로 로딩된다 — 등록된 서버 섹션이 먼저 보이고,
  // 카탈로그 항목은 로딩이 끝나면 설치 가능한 서버 섹션에 합류한다.
  if (catalogStillLoading) body.push(loadingBlock());

  const modalEl = renderMcpAddModalIfOpen();
  return el('div', {}, modalEl ? [...body, modalEl] : body);
}
