import { el, showToast, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import type { SettingsTab } from '../state';
import { fetchOtelEnv } from '../otelApi';
import { loadCatalog, loadMarketplaces } from './catalog';
import { refreshMarketplaces } from '../catalogApi';
import { navigate } from '../route';

interface FieldSpec {
  readonly id: string;
  readonly label: string;
  readonly placeholder: string;
  readonly type?: 'text' | 'password';
  readonly value?: string;
}

const GITHUB_FIELDS: readonly FieldSpec[] = [
  { id: 'gh-org', label: '조직/계정', placeholder: 'malgnsoft', value: 'malgnsoft' },
  { id: 'gh-repo', label: '기본 레포지토리', placeholder: 'malgn-vscode', value: 'malgn-vscode' },
  { id: 'gh-token', label: 'Personal Access Token', placeholder: 'ghp_****', type: 'password' },
  { id: 'gh-branch', label: '기본 브랜치', placeholder: 'main', value: 'main' },
];

const CLOUDFLARE_FIELDS: readonly FieldSpec[] = [
  { id: 'cf-account', label: '계정 ID', placeholder: 'a1b2c3d4e5f6...' },
  { id: 'cf-token', label: 'API 토큰', placeholder: '****', type: 'password' },
  { id: 'cf-zone', label: 'Zone ID (선택)', placeholder: 'zone id (옵션)' },
];

const JIRA_FIELDS: readonly FieldSpec[] = [
  { id: 'jira-site', label: 'Jira 사이트 URL', placeholder: 'https://malgnsoft.atlassian.net', value: 'https://malgnsoft.atlassian.net' },
  { id: 'jira-email', label: '계정 이메일', placeholder: 'dev@malgnsoft.com' },
  { id: 'jira-token', label: 'API 토큰', placeholder: '****', type: 'password' },
];

interface FormTabSpec {
  readonly key: SettingsTab;
  readonly label: string;
  readonly fields: readonly FieldSpec[];
  readonly hint: string;
}

// 일반 폼 패턴(입력 필드 + 저장 버튼, 전부 목업 값)을 쓰는 탭들. "otel"·"google"·
// "marketplace"는 각자 성격이 달라 별도 패널로 그린다 — TAB_META에는 같이 들어가지만
// FORM_TABS에는 넣지 않는다.
const FORM_TABS: readonly FormTabSpec[] = [
  { key: 'github', label: 'GitHub 설정', fields: GITHUB_FIELDS, hint: '조직 리포지토리 접근에 사용할 자격 증명을 설정합니다.' },
  { key: 'cloudflare', label: 'Cloudflare 설정', fields: CLOUDFLARE_FIELDS, hint: '배포·DNS 자동화에 사용할 Cloudflare 자격 증명을 설정합니다.' },
  { key: 'jira', label: 'Jira 설정', fields: JIRA_FIELDS, hint: '자율업무·이슈 연동에 사용할 Jira 자격 증명을 설정합니다.' },
];

const TAB_META: readonly { readonly key: SettingsTab; readonly label: string }[] = [
  { key: 'otel', label: 'OTel 설정' },
  ...FORM_TABS.map((t) => ({ key: t.key, label: t.label })),
  { key: 'google', label: 'Google Workspace 설정' },
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
  else if (tab === 'google') body = renderGoogleWorkspacePanel();
  else if (tab === 'marketplace') body = renderMarketplacePanel();
  else body = renderFormPanel(tab);

  return el('div', {}, [header, tabsRow, body]);
}

function renderFormPanel(tab: SettingsTab): HTMLElement {
  const current = FORM_TABS.find((t) => t.key === tab) ?? FORM_TABS[0];

  const form = el('form', { className: 'settings-form' }, [el('div', { className: 'settings-form-hint' }, [current.hint]), ...current.fields.map(renderField)]);
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    showToast(`${current.label} 저장됨 (목업 — 실제로 저장되지 않습니다)`);
  });

  const saveBtn = el('button', { className: 'btn btn-primary' }, ['저장']);
  saveBtn.type = 'submit';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn]));

  return el('div', { className: 'settings-card' }, [form]);
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

// ---------------- Google Workspace (OAuth 스타일 목업) ----------------
// 실제 OAuth 플로우는 없다 — 버튼을 누르면 그냥 "연결됨" 상태로 바뀐다.

function renderGoogleWorkspacePanel(): HTMLElement {
  if (!state.google.connected) {
    return el('div', { className: 'settings-card google-panel' }, [
      el('div', { className: 'settings-form-hint' }, [
        'Gmail(업무 이메일 가져오기/처리)과 Google Drive(자료 저장) 연동에 사용할 Google 계정을 연결합니다.',
      ]),
      el(
        'button',
        {
          className: 'btn btn-primary',
          onClick: () => {
            state.google.connected = true;
            state.google.email = state.google.email ?? 'dev@malgnsoft.com';
            showToast('Google 계정이 연결되었습니다 (목업 — 실제 OAuth 연동 없음)');
            notifyChange();
          },
        },
        ['Google 계정으로 연결하기']
      ),
    ]);
  }

  const account = el('div', { className: 'google-account-row' }, [
    el('div', { className: 'google-account-avatar' }, ['G']),
    el('div', { className: 'google-account-info' }, [
      el('div', { className: 'google-account-email' }, [state.google.email ?? '']),
      el('div', { className: 'google-account-status' }, ['연결됨']),
    ]),
    el(
      'button',
      {
        className: 'btn',
        onClick: () => {
          state.google.connected = false;
          showToast('Google 계정 연결이 해제되었습니다');
          notifyChange();
        },
      },
      ['연결 해제']
    ),
  ]);

  const gmailRow = googleServiceRow('Gmail 연동', '업무 이메일을 가져와 처리합니다.', state.google.gmailEnabled, () => {
    state.google.gmailEnabled = !state.google.gmailEnabled;
    notifyChange();
  });
  const driveRow = googleServiceRow('Drive 연동', '자료를 Google Drive에 저장합니다.', state.google.driveEnabled, () => {
    state.google.driveEnabled = !state.google.driveEnabled;
    notifyChange();
  });

  return el('div', { className: 'settings-card google-panel' }, [account, gmailRow, driveRow]);
}

function googleServiceRow(name: string, desc: string, enabled: boolean, onToggle: () => void): HTMLElement {
  return el('div', { className: 'google-service-row' }, [
    el('div', {}, [el('div', { className: 'google-service-name' }, [name]), el('div', { className: 'google-service-desc' }, [desc])]),
    toggleSwitch(enabled, onToggle),
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
