// 앱링크 설정 — 사내/외부 웹 앱 바로가기를 CRUD하는 설정 탭 패널
// (docs/design-app-links.md §5). 사이드바에 노출할 실제 목록도 이 화면에서
// 관리한 파일 하나(~/.claude/malgn-agent-apps.json)가 정본이다 — sidebar.ts는
// 이 모듈이 내보내는 loadAppLinks()/enabledAppLinks()/openLink()만 가져다 쓴다.
//
// 저장 타이밍: 토글·추가·수정·삭제 각각이 곧바로 전체 목록 치환 저장을 호출한다
// (별도 "저장" 버튼 없음). 낙관적 갱신은 하지 않는다 — 저장이 실패하면
// state.appLinks.status를 건드리지 않으므로(성공했을 때만 교체) 화면은 저절로
// 저장 전 상태로 남는다.
import { el, showToast, loadingBlock, errorBlock, confirmDialog, createModalOverlay, toggleSwitch, boundField, restoreModalFocus } from '../dom';
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
// ESC 리스너 패턴. draft는 모달이 열릴 때(openLinkFormModal) editingLink를
// 기반으로 한 번만 초기화되고, 닫힐 때 버려진다 — 배경 이벤트로 인한 재렌더가
// 일어나도 입력 중이던 값이 사라지지 않게 하는 공용 방식이다(dom.ts의
// boundField 참고, hub 이슈 01m2zwcx7etvk9zh617tk7bayq).
interface LinkFormDraft {
  name: string;
  url: string;
}
let linkFormModal: { readonly editingLink: AppLink | null; readonly draft: LinkFormDraft } | null = null;
let linkFormModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachLinkFormModalEscHandler(): void {
  if (linkFormModalEscHandler) {
    window.removeEventListener('keydown', linkFormModalEscHandler);
    linkFormModalEscHandler = null;
  }
}

function openLinkFormModal(editingLink: AppLink | null): void {
  linkFormModal = { editingLink, draft: { name: editingLink?.name ?? '', url: editingLink?.url ?? '' } };
  notifyChange();
}

function closeLinkFormModal(): void {
  linkFormModal = null;
  detachLinkFormModalEscHandler();
  restoreModalFocus(closeLinkFormModal); // C3: 열기 직전 포커스로 복원
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
  return renderLinkFormModal(linkFormModal.editingLink, linkFormModal.draft);
}

// 앱링크 설정 탭을 벗어날 때(다른 탭으로 이동하거나 라우트를 완전히 떠날 때)
// main.ts에서 호출한다 — 열려 있던 추가/수정 모달이 있었다면 window에 남은
// ESC 리스너를 정리한다(leaveProjectsListView/leaveAutonomousTasksListView와
// 동일한 원칙).
export function leaveAppLinksView(): void {
  linkFormModal = null;
  detachLinkFormModalEscHandler();
}

// 프론트는 UX용 사전 체크만 한다(빈 값 차단 + maxlength 힌트) — 정본 검증은
// Rust 한 곳(app_links/store.rs)이고, 거부되면 에러 원문을 그대로 보여준다(§4-1).
// limits는 하드코딩하지 않고 항상 status.limits에서 읽는다(§3-2).
function renderLinkForm(editingLink: AppLink | null, draft: LinkFormDraft): HTMLElement {
  const limits = state.appLinks.status?.limits;
  const maxNameLength = limits?.maxNameLength ?? 40;
  const maxUrlLength = limits?.maxUrlLength ?? 2048;

  const nameInput = boundField(
    document.createElement('input'),
    () => draft.name,
    (v) => {
      draft.name = v;
    }
  );
  nameInput.className = 'settings-input';
  nameInput.type = 'text';
  nameInput.autocomplete = 'off';
  nameInput.maxLength = maxNameLength;

  const urlInput = boundField(
    document.createElement('input'),
    () => draft.url,
    (v) => {
      draft.url = v;
    }
  );
  urlInput.className = 'settings-input';
  urlInput.type = 'text';
  urlInput.autocomplete = 'off';
  urlInput.maxLength = maxUrlLength;
  urlInput.placeholder = 'https://example.internal';

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
function renderLinkFormModal(editingLink: AppLink | null, draft: LinkFormDraft): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, [editingLink ? '앱링크 수정' : '새 앱링크 추가']),
      el('button', { className: 'modal-close-btn', onClick: closeLinkFormModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [renderLinkForm(editingLink, draft)]),
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

  // 긴 이름은 ellipsis로 잘리므로(styles.css .mcp-row-name) title 속성으로
  // 전체 텍스트를 hover 시 볼 수 있게 한다(views/sessions.ts의 chat-title과 동일 패턴).
  const nameEl = el('span', { className: 'mcp-row-name' }, [link.name]);
  nameEl.title = link.name;

  return el('div', { className: 'mcp-row' }, [
    toggleSwitch(link.enabled, () => {
      if (!saving) void handleToggleLink(link);
    }),
    el('div', { className: 'mcp-row-main' }, [
      el('div', { className: 'mcp-row-top' }, [
        nameEl,
        ...(insecure ? [el('span', { className: 'mcp-row-status-label' }, ['암호화되지 않음'])] : []),
      ]),
      el('div', { className: 'mcp-row-target' }, [link.url]),
    ]),
    el('div', { className: 'task-row-actions' }, [editBtn, deleteBtn]),
  ]);
}

// ---------------- 패널 본체 ----------------

// design-system.md §3.1 — 박스 공용 헤더(home.ts boxHead()와 동일 패턴). 이
// 화면은 헤더 우측에 "+ 링크 추가" 버튼을 둔다.
function boxHead(title: string, right: Node | string): HTMLElement {
  return el('div', { className: 'box-head' }, [el('span', { className: 'box-title' }, [title]), right]);
}

// terminus-shell-ia.md §1이 앱 링크를 최상위 탭(#8)으로 승격해뒀다 — 이전에는
// settings.ts의 renderSettingsView()가 "설정" page-header를 씌워줬지만, 이제
// 이 탭은 renderSettingsView()에서 직접 이 함수를 반환하므로(제목 중복 회귀
// 수정, 배치 C) 이 화면 스스로 page-header를 그려야 한다(devTools.ts와 동일
// 원칙 — page-header는 탭 자신의 제목, box-head는 그 안의 콘텐츠 라벨).
function renderAppLinksHeader(): HTMLElement {
  return el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['앱 링크']),
      el('div', { className: 'page-subtitle' }, ['사이드바에서 바로 열 수 있는 사내/외부 웹 앱 바로가기']),
    ]),
  ]);
}

export function renderAppLinksPanel(): HTMLElement {
  if (state.appLinks.loading && !state.appLinks.loaded) return el('div', {}, [renderAppLinksHeader(), loadingBlock()]);
  if (state.appLinks.error) return el('div', {}, [renderAppLinksHeader(), errorBlock(state.appLinks.error, () => void loadAppLinks())]);

  const status = state.appLinks.status;
  if (!status) return el('div', {}, [renderAppLinksHeader(), loadingBlock()]);

  const notices: HTMLElement[] = [];

  // 손으로 편집된 파일에서 거른 항목이 있으면 경고 배너로 알린다(§2-2) —
  // 이 상태에서 저장하면 화면에 안 보이는 항목은 파일에서 사라진다(알려진 귀결).
  if (status.warnings.length > 0) {
    notices.push(el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${status.warnings.join(' / ')}`])]));
  }

  // 설정 파일 자체가 손상된 경우 — 목록·추가 버튼을 모두 비활성화한다(§5-2).
  const blocked = !status.ok;
  if (blocked) {
    notices.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${status.error ?? '앱링크 설정 파일을 읽지 못했습니다.'}`]),
        el('button', { className: 'btn', onClick: () => void loadAppLinks() }, ['다시 시도']),
      ])
    );
  }

  // 한도 도달(>=) 시 추가 버튼을 비활성화한다 — 백엔드도 어차피 거부하지만
  // (app_links/store.rs), 폼을 다 채운 뒤에야 실패를 알게 하지 않기 위해
  // 프론트에서 먼저 막는다. "왜 못 누르는지"는 버튼 title 툴팁 + 카운트
  // 라벨을 danger 색으로 강조하는 두 가지로 함께 알린다(deleteBtn의 인라인
  // style.color 지정과 동일한 패턴, --color-danger 재사용).
  const atCapacity = status.links.length >= status.limits.maxLinks;
  const disabled = blocked || state.appLinks.saving || atCapacity;
  const addBtn = el('button', { className: 'btn btn-primary btn-sm', disabled, onClick: () => openLinkFormModal(null) }, ['+ 링크 추가']);
  if (atCapacity) addBtn.title = `링크 개수 한도(${status.limits.maxLinks}개)에 도달했습니다. 추가하려면 기존 링크를 먼저 삭제하세요.`;
  const countLabel = el('span', { className: 'applink-count-label' }, [
    `사이드바에 노출할 링크를 켜세요. (${status.links.length} / ${status.limits.maxLinks})`,
  ]);
  if (atCapacity) countLabel.style.color = 'var(--color-danger)';

  const boxBody: HTMLElement[] = [countLabel];

  if (status.links.length === 0) {
    boxBody.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['아직 등록된 앱링크가 없습니다']),
        el('div', { className: 'state-block-desc' }, [
          '자주 쓰는 사내 시스템·SaaS 주소를 추가하면 사이드바에서 바로 열 수 있습니다.',
        ]),
      ])
    );
  } else {
    boxBody.push(el('div', { className: 'mcp-list' }, status.links.map(renderAppLinkRow)));
  }

  boxBody.push(el('div', { className: 'applink-filepath' }, [`저장 위치: ${status.filePath}`]));

  const box = el('div', { className: 'box' }, [boxHead('등록된 링크', addBtn), el('div', { className: 'box-body' }, boxBody)]);

  const modalEl = renderLinkFormModalIfOpen();
  const body = [renderAppLinksHeader(), ...notices, box];
  return el('div', {}, modalEl ? [...body, modalEl] : body);
}
