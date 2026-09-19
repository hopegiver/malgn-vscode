// 앱링크 설정 — 사내/외부 웹 앱 바로가기를 CRUD하는 설정 탭 패널
// (docs/design-app-links.md §5). 사이드바에 노출할 실제 목록도 이 화면에서
// 관리한 파일 하나(~/.claude/malgn-agent-apps.json)가 정본이다 — sidebar.ts는
// 이 모듈이 내보내는 loadAppLinks()/enabledAppLinks()/openLink()만 가져다 쓴다.
//
// 저장 타이밍: 토글·추가·수정·삭제 각각이 곧바로 전체 목록 치환 저장을 호출한다
// (별도 "저장" 버튼 없음). 낙관적 갱신은 하지 않는다 — 저장이 실패하면
// state.appLinks.status를 건드리지 않으므로(성공했을 때만 교체) 화면은 저절로
// 저장 전 상태로 남는다.
import { el, showToast, loadingBlock, errorBlock, confirmDialog, createModalOverlay, toggleSwitch } from '../dom';
import { state, notifyChange } from '../state';
import { fetchAppLinks, saveAppLinks, openAppLink } from '../appLinksApi';
import type { AppLink } from '../appLinksApi';

export async function loadAppLinks(): Promise<void> {
  state.appLinks.loading = true;
  state.appLinks.error = null;
  notifyChange();
  try {
    state.appLinks.status = await fetchAppLinks();
  } catch (err) {
    state.appLinks.error = err instanceof Error ? err.message : '앱링크 목록을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    // 성공 시에만 loaded=true를 세우면 실패 시 이 값이 영원히 false로 남아
    // 사이드바(renderAppLinksGroup)가 "불러오는 중…"에 무한히 머무는 버그가
    // 있었다 — loaded는 "시도를 마쳤다"는 뜻으로 두고, 성공 여부는 error/status로
    // 구분한다(캡처 하네스 error 시나리오로 실측 발견).
    state.appLinks.loading = false;
    state.appLinks.loaded = true;
    notifyChange();
  }
}

// 사이드바에 보여줄 파생값 — 저장하지 않고 매번 여기서 계산한다(정본은
// state.appLinks.status.links 하나).
export function enabledAppLinks(): readonly AppLink[] {
  return state.appLinks.status?.links.filter((l) => l.enabled) ?? [];
}

// 사이드바 클릭 핸들러 — 실패해도 토스트로만 그친다(모달·배너 없음, §6-4).
export async function openLink(link: AppLink): Promise<void> {
  try {
    await openAppLink(link.id);
  } catch (err) {
    showToast(err instanceof Error ? err.message : `'${link.name}'을(를) 열지 못했습니다`);
  }
}

// 전체 목록 치환 저장 공용 헬퍼 — 성공 시에만 state.appLinks.status를 교체한다.
// 실패 시에는 아무것도 바꾸지 않으므로 화면은 자연히 저장 전 상태로 남는다
// (별도 롤백 로직이 필요 없다).
async function persistLinks(links: readonly AppLink[], successMessage: string): Promise<boolean> {
  state.appLinks.saving = true;
  notifyChange();
  try {
    state.appLinks.status = await saveAppLinks(links);
    showToast(successMessage);
    return true;
  } catch (err) {
    showToast(err instanceof Error ? err.message : '앱링크 저장에 실패했습니다.');
    return false;
  } finally {
    state.appLinks.saving = false;
    notifyChange();
  }
}

async function handleToggleLink(link: AppLink): Promise<void> {
  const status = state.appLinks.status;
  if (!status) return;
  const next = status.links.map((l) => (l.id === link.id ? { ...l, enabled: !l.enabled } : l));
  await persistLinks(next, `'${link.name}' ${!link.enabled ? '활성화' : '비활성화'}됨`);
}

async function handleDeleteLink(link: AppLink): Promise<void> {
  if (!(await confirmDialog(`'${link.name}' 앱링크를 삭제할까요?`, { danger: true }))) return;
  const status = state.appLinks.status;
  if (!status) return;
  const next = status.links.filter((l) => l.id !== link.id);
  await persistLinks(next, `'${link.name}' 앱링크를 삭제했습니다.`);
}

// ---------------- 추가/수정 겸용 모달 ----------------
// autonomousTasks.ts의 renderTaskFormModal과 동일한 모듈 로컬 열림상태 +
// ESC 리스너 패턴.
let linkFormModal: { readonly editingLink: AppLink | null } | null = null;
let linkFormModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachLinkFormModalEscHandler(): void {
  if (linkFormModalEscHandler) {
    window.removeEventListener('keydown', linkFormModalEscHandler);
    linkFormModalEscHandler = null;
  }
}

function openLinkFormModal(editingLink: AppLink | null): void {
  linkFormModal = { editingLink };
  notifyChange();
}

function closeLinkFormModal(): void {
  linkFormModal = null;
  detachLinkFormModalEscHandler();
  notifyChange();
}

function renderLinkFormModalIfOpen(): HTMLElement | null {
  if (!linkFormModal) {
    detachLinkFormModalEscHandler();
    return null;
  }
  if (!linkFormModalEscHandler) {
    linkFormModalEscHandler = (e) => {
      if (e.key === 'Escape') closeLinkFormModal();
    };
    window.addEventListener('keydown', linkFormModalEscHandler);
  }
  return renderLinkFormModal(linkFormModal.editingLink);
}

// 프론트는 UX용 사전 체크만 한다(빈 값 차단 + maxlength 힌트) — 정본 검증은
// Rust 한 곳(app_links/store.rs)이고, 거부되면 에러 원문을 그대로 보여준다(§4-1).
// limits는 하드코딩하지 않고 항상 status.limits에서 읽는다(§3-2).
function renderLinkForm(editingLink: AppLink | null): HTMLElement {
  const limits = state.appLinks.status?.limits;
  const maxNameLength = limits?.maxNameLength ?? 40;
  const maxUrlLength = limits?.maxUrlLength ?? 2048;

  const nameInput = document.createElement('input');
  nameInput.className = 'settings-input';
  nameInput.type = 'text';
  nameInput.autocomplete = 'off';
  nameInput.maxLength = maxNameLength;
  nameInput.value = editingLink?.name ?? '';

  const urlInput = document.createElement('input');
  urlInput.className = 'settings-input';
  urlInput.type = 'text';
  urlInput.autocomplete = 'off';
  urlInput.maxLength = maxUrlLength;
  urlInput.placeholder = 'https://example.internal';
  urlInput.value = editingLink?.url ?? '';

  const form = el('form', { className: 'settings-form' }, [
    el('div', { className: 'settings-form-hint' }, ['https:// 또는 http:// 로 시작하는 주소를 입력하세요.']),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['이름']), nameInput]),
    el('label', { className: 'settings-field' }, [el('span', { className: 'settings-field-label' }, ['주소']), urlInput]),
  ]);

  const saveBtn = el('button', { className: 'btn btn-primary', disabled: state.appLinks.saving }, [
    state.appLinks.saving ? '저장 중…' : editingLink ? '저장' : '추가',
  ]);
  saveBtn.type = 'submit';
  const cancelBtn = el('button', { className: 'btn', onClick: closeLinkFormModal }, ['취소']);
  cancelBtn.type = 'button';
  form.appendChild(el('div', { className: 'settings-form-actions' }, [saveBtn, cancelBtn]));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = nameInput.value.trim();
    const url = urlInput.value.trim();
    if (!name || !url) {
      showToast('이름과 주소를 모두 입력하세요.');
      return;
    }

    const status = state.appLinks.status;
    if (!status) return;

    const link: AppLink = {
      id: editingLink ? editingLink.id : crypto.randomUUID(),
      name,
      url,
      enabled: editingLink ? editingLink.enabled : true,
    };
    const next = editingLink ? status.links.map((l) => (l.id === editingLink.id ? link : l)) : [...status.links, link];

    void (async () => {
      const ok = await persistLinks(next, editingLink ? `'${name}' 앱링크를 수정했습니다.` : `'${name}' 앱링크를 추가했습니다.`);
      if (ok) closeLinkFormModal();
    })();
  });

  return form;
}

// sessions.ts/autonomousTasks.ts와 동일한 모달 구조(modal-overlay/modal-box/
// modal-header+modal-close-btn/modal-body) — 배경 클릭·ESC·닫기 버튼·취소 버튼
// 4가지 경로로 닫힌다.
function renderLinkFormModal(editingLink: AppLink | null): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, [editingLink ? '앱링크 수정' : '새 앱링크 추가']),
      el('button', { className: 'modal-close-btn', onClick: closeLinkFormModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderLinkForm(editingLink)]),
  ]);
  return createModalOverlay(modalBox, closeLinkFormModal);
}

// ---------------- 목록 행 ----------------
// mcp-row/mcp-row-main/mcp-row-top/mcp-row-name/mcp-row-target(설정 화면의 MCP
// 관리 패널)과 task-row-actions(자율업무 목록의 토글+수정+삭제 액션 그룹)를
// 그대로 재사용한다 — 이 화면 전용 새 레이아웃 클래스를 만들지 않는다.
function renderAppLinkRow(link: AppLink): HTMLElement {
  const saving = state.appLinks.saving;
  const insecure = link.url.toLowerCase().startsWith('http://');

  const editBtn = el('button', { className: 'btn', disabled: saving, onClick: () => openLinkFormModal(link) }, ['수정']);
  const deleteBtn = el('button', { className: 'btn', disabled: saving, onClick: () => void handleDeleteLink(link) }, ['삭제']);
  deleteBtn.style.color = 'var(--color-danger)';

  return el('div', { className: 'mcp-row' }, [
    toggleSwitch(link.enabled, () => {
      if (!saving) void handleToggleLink(link);
    }),
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        el('span', { className: 'mcp-row-name' }, [link.name]),
        ...(insecure ? [el('span', { className: 'mcp-row-status-label' }, ['암호화되지 않음'])] : []),
      ]),
      el('div', { className: 'mcp-row-target' }, [link.url]),
    ]),
    el('div', { className: 'task-row-actions' }, [editBtn, deleteBtn]),
  ]);
}

// ---------------- 패널 본체 ----------------

export function renderAppLinksPanel(): HTMLElement {
  if (state.appLinks.loading && !state.appLinks.loaded) return loadingBlock();
  if (state.appLinks.error) return errorBlock(state.appLinks.error, () => void loadAppLinks());

  const status = state.appLinks.status;
  if (!status) return loadingBlock();

  const body: HTMLElement[] = [];

  // 손으로 편집된 파일에서 거른 항목이 있으면 경고 배너로 알린다(§2-2) —
  // 이 상태에서 저장하면 화면에 안 보이는 항목은 파일에서 사라진다(알려진 귀결).
  if (status.warnings.length > 0) {
    body.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${status.warnings.join(' / ')}`])]));
  }

  // 설정 파일 자체가 손상된 경우 — 목록·추가 버튼을 모두 비활성화한다(§5-2).
  const blocked = !status.ok;
  if (blocked) {
    body.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${status.error ?? '앱링크 설정 파일을 읽지 못했습니다.'}`]),
        el('button', { className: 'btn', onClick: () => void loadAppLinks() }, ['다시 시도']),
      ])
    );
  }

  const disabled = blocked || state.appLinks.saving;
  const addBtn = el('button', { className: 'btn btn-primary', disabled, onClick: () => openLinkFormModal(null) }, ['+ 링크 추가']);
  const countLabel = el('span', { className: 'applink-count-label' }, [
    `사이드바에 노출할 링크를 켜세요. (${status.links.length} / ${status.limits.maxLinks})`,
  ]);
  body.push(el('div', { className: 'filter-row' }, [countLabel, addBtn]));

  if (status.links.length === 0) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['아직 등록된 앱링크가 없습니다']),
        el('div', { className: 'state-block-desc' }, [
          '자주 쓰는 사내 시스템·SaaS 주소를 추가하면 사이드바에서 바로 열 수 있습니다.',
        ]),
      ])
    );
  } else {
    body.push(el('div', { className: 'mcp-list' }, status.links.map(renderAppLinkRow)));
  }

  body.push(el('div', { className: 'applink-filepath' }, [`저장 위치: ${status.filePath}`]));

  const modalEl = renderLinkFormModalIfOpen();
  return el('div', {}, modalEl ? [...body, modalEl] : body);
}
