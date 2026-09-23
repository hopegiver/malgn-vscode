// DOM 헬퍼 — 프레임워크 없는 바닐라 TS 전용. 어떤 값도 innerHTML로 넣지 않고 항상
// createElement + textContent/appendChild로만 DOM에 삽입한다(XSS 방어 원칙).

export interface ElOptions {
  readonly className?: string;
  readonly onClick?: () => void;
  readonly onKeydown?: (e: KeyboardEvent) => void;
  readonly disabled?: boolean;
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  opts: ElOptions = {},
  children: readonly (Node | string)[] = []
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (opts.className) node.className = opts.className;
  if (opts.onClick) node.addEventListener('click', opts.onClick);
  if (opts.onKeydown) node.addEventListener('keydown', opts.onKeydown as EventListener);
  if (opts.disabled && 'disabled' in node) (node as unknown as { disabled: boolean }).disabled = true;
  for (const child of children) {
    node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
  }
  return node;
}

// 클릭+키보드 활성화를 한 번에 붙이는 헬퍼 — role="button" + tabindex="0" +
// click/keydown(Enter·Space, preventDefault로 스페이스의 페이지 스크롤 방지)을
// 한 곳에서 관리한다. 카드형 위젯·트리 행·목록 행처럼 "전체가 클릭 가능한 블록"에
// 새로 붙일 때 쓴다. 이미 자체적으로 role/tabindex/keydown을 갖추고 정상 동작하는
// 곳(session-row, task-row-main 등)까지 굳이 재작성하지는 않았다.
// opts.stopPropagation: 이 요소가 클릭 가능한 부모 안에 중첩된 별도의 조작
// 대상(예: 행 안의 펼침 캐럿)일 때 true로 준다 — 없으면 Enter/Space가 이
// 핸들러를 실행한 뒤 부모의 keydown까지 버블링해 두 동작이 동시에 발동한다.
export function clickable<T extends HTMLElement>(node: T, onActivate: () => void, opts?: { readonly stopPropagation?: boolean }): T {
  node.setAttribute('role', 'button');
  node.setAttribute('tabindex', '0');
  node.addEventListener('click', (e) => {
    if (opts?.stopPropagation) e.stopPropagation();
    onActivate();
  });
  node.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      // Space의 페이지 스크롤 방지는 반복 keydown에서도 계속 막아야 하므로
      // preventDefault는 항상 먼저 실행한다.
      e.preventDefault();
      // M4(리뷰 r3) — OS 키 반복으로 들어오는 두 번째 이후 keydown(e.repeat)은
      // onActivate()를 실행하지 않는다. 이게 없으면 role="button" 요소에서
      // Enter/Space를 길게 눌러 키만 떼지 않아도 활성화가 반복 실행된다 —
      // 특히 계정 메뉴 드롭다운처럼 열자마자 위험 항목(로그아웃)에 포커스가 갈
      // 수 있는 곳에서는 첫 keydown이 메뉴를 열고 두 번째(반복) keydown이 그대로
      // 그 항목을 실행해 확인 없이 로그아웃되는 결과로 이어졌다. 전 clickable()
      // 사용처에 적용되는 범용 방어라 개별 화면을 고칠 필요가 없다.
      if (e.repeat) return;
      if (opts?.stopPropagation) e.stopPropagation();
      onActivate();
    }
  });
  return node;
}

// 토글 스위치 — 자율업무 활성/비활성 등에 쓰는 공용 컴포넌트. role="switch" +
// tabindex라 키보드로 포커스는 가지만, 클릭
// 핸들러만 있으면 Enter/Space로는 조작할 수 없다 — keydown에서 두 키 모두
// 받아 onChange를 호출한다(공용 컴포넌트라 이 한 곳만 고치면 전 화면에 적용된다).
export function toggleSwitch(checked: boolean, onChange: () => void): HTMLElement {
  const track = el(
    'span',
    {
      className: `toggle-track${checked ? ' on' : ''}`,
      onClick: onChange,
      onKeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onChange();
        }
      },
    },
    [el('span', { className: 'toggle-thumb' }, [])]
  );
  track.setAttribute('role', 'switch');
  track.setAttribute('aria-checked', String(checked));
  track.setAttribute('tabindex', '0');
  return track;
}

// 공용 로딩/에러 블록 — 원래 views/settings.ts의 otel 패널이 쓰던 패턴
// (loading/error/loaded 3상태)을 github/cloudflare/mcp 패널도 동일하게
// 따라 쓰면서 여러 곳에 중복되어 있던 것을 승격했다. 메시지와 재시도 콜백은 항상
// 호출부가 그대로 넘긴다 — 여기서 공용 문구로 바꿔치기하지 않는다(원인 경로가
// 담긴 원본 에러 메시지를 그대로 보여주는 것이 이 앱의 강점이다).
export function loadingBlock(): HTMLElement {
  return el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]);
}

export function errorBlock(message: string, onRetry: () => void): HTMLElement {
  return el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${message}`]), el('button', { className: 'btn', onClick: onRetry }, ['다시 시도'])]);
}

// 실시간 감시 인디케이터 — 파일시스템 이벤트 기반 자동 갱신이 실제로 구독되어
// 있을 때만 켠다(세션목록·사용량 통계 일별 사용량). Tauri IPC 브리지가 없는
// 환경(플레인 브라우저)에서는 구독 자체가 실패하므로 자연스럽게 꺼진 채로 남는다
// — "실시간"이라고 거짓으로 표시하지 않는다.
export function liveIndicator(label = '실시간'): HTMLElement {
  return el('span', { className: 'live-indicator' }, [el('span', { className: 'live-dot' }, []), label]);
}

// 세션 행 단위 "지금 실행 중" 표시 — liveIndicator()와 의미가 다르다. liveIndicator는
// 전역 파일시스템 워처 구독 여부(state.sessions.live)를 나타내고, 이 함수는 세션
// 레코드 하나하나의 running 필드(그 세션이 지금 실행 중인 프로세스인지)를 나타낸다.
// 경고 배너가 아니라 점 하나 수준의 조용한 표시로 그친다 — 끝난 세션이 기본
// 상태이므로 running === false일 때는 호출하지 말 것(아무것도 표시하지 않는다).
export function runningDot(): HTMLElement {
  const dot = el('span', { className: 'session-running-dot' }, []);
  dot.setAttribute('title', '실행 중');
  dot.setAttribute('aria-label', '실행 중');
  return dot;
}

// 모달 오버레이 — 배경 클릭 시 닫히되, "모달 박스 안(예: 제목 텍스트)에서
// mousedown으로 드래그를 시작해 오버레이 위에서 mouseup"하는 경우에는 닫히지
// 않아야 한다. el()의 onClick은 단순 'click' 리스너라 이 케이스를 못 막는다 —
// click 이벤트의 target이 오버레이 자신이 되어(버블링이 아니라 직접 발생)
// modalBox의 stopPropagation과 무관하게 오버레이 핸들러가 실행되기 때문이다.
// 그래서 mousedown 시점에 "오버레이 자기 자신에서 시작했는가"를 기록해뒀다가
// click 시점에 그 기록과 현재 target을 함께 확인한다. role="dialog"/aria-modal도
// 여기서 함께 설정한다. (기존 4곳의 modal-overlay가 공통으로 겪던 버그를 승격.)
// 포커스 가능 요소 selector — confirmDialog()가 이미 쓰던 기준(버튼/링크/입력/
// select/textarea + tabindex 음수 아닌 것)을 그대로 승격했다.
const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (n) => n.offsetParent !== null || n === document.activeElement
  );
}

// 모달별 "열기 직전 포커스" 기억소 — onClose 함수 참조(모듈 스코프의
// closeXxxModal, 모달이 열려 있는 동안 매 재렌더에도 항상 같은 참조)를 키로
// 쓴다. 이 앱은 배경 이벤트로 root.replaceChildren() 전체 재빌드가 일어나면
// 모달도 매번 새 DOM 노드로 다시 만들어지므로(모달이 열려 있다는 사실은
// state 플래그로만 남는다), "모달을 새로 열었다"와 "열려 있던 모달이
// 재렌더로 다시 그려졌다"를 이 맵의 존재 여부로 구분한다 — 재렌더마다
// previouslyFocused를 <body>(재빌드 직후 activeElement)로 덮어쓰지 않기
// 위함이다.
const modalFocusMemory = new WeakMap<() => void, HTMLElement | null>();

export function createModalOverlay(modalBox: HTMLElement, onClose: () => void): HTMLElement {
  modalBox.addEventListener('click', (e) => e.stopPropagation());

  let downOnOverlay = false;
  const overlay = el('div', { className: 'modal-overlay' }, [modalBox]);
  overlay.addEventListener('mousedown', (e) => {
    downOnOverlay = e.target === overlay;
  });
  overlay.addEventListener('click', (e) => {
    if (downOnOverlay && e.target === overlay) onClose();
    downOnOverlay = false;
  });
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');

  // 포커스 트랩(C3, 리뷰 v0.2.5) — role="dialog"/aria-modal="true"가 선언만
  // 하고 실제로는 안 지켜지던 부분. confirmDialog()가 이미 쓰던 "열기 직전
  // activeElement 기억 → 열 때 모달 안 첫 포커스 가능 요소로 이동 → Tab을
  // 가로채 모달 내부로 순환 → 닫을 때 원래 요소로 복원" 패턴을 여기 승격해
  // 호출부 7곳(sessions.ts 메타모달/새세션, projects.ts, autonomousTasks.ts
  // 2곳, settings.ts, appLinks.ts)에 한 번에 적용한다. "닫을 때 복원"은
  // 이 함수만으로는 닫힘을 감지할 수 없어(각 호출부의 close*Modal()이 실제
  // 소유자다) restoreModalFocus(onClose)를 별도로 내보낸다 — 호출부가 이미
  // 갖고 있는 close*Modal() 끝에 한 줄만 추가하면 된다(detach*EscHandler()와
  // 같은 자리에 두는 기존 관례를 그대로 따른다).
  const isFreshOpen = !modalFocusMemory.has(onClose);
  if (isFreshOpen) {
    modalFocusMemory.set(onClose, document.activeElement instanceof HTMLElement ? document.activeElement : null);
  }

  // 오버레이가 문서에 실제로 붙는 시점(다음 마이크로태스크)까지 기다렸다가
  // 포커스를 옮긴다 — 호출부가 반환값을 아직 DOM에 appendChild하지 않았을
  // 수 있다. boundField가 이미 이번 렌더에서 포커스 복원을 예약해뒀다면
  // (pendingFocus) 그쪽이 우선한다 — 재렌더로 모달이 다시 그려질 때 안의
  // 입력 필드에 포커스가 있었다면 첫 요소로 되돌리지 않는다.
  queueMicrotask(() => {
    if (!overlay.isConnected) return;
    if (hasPendingFocus()) return;
    const active = document.activeElement;
    if (active && active !== document.body && modalBox.contains(active)) return;
    const focusable = focusableElements(modalBox);
    (focusable[0] ?? modalBox).focus();
  });

  overlay.addEventListener('keydown', (e) => {
    if (e.key !== 'Tab') return;
    const focusable = focusableElements(modalBox);
    if (focusable.length === 0) {
      e.preventDefault();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !modalBox.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (active === last || !modalBox.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    }
  });

  return overlay;
}

// createModalOverlay와 짝을 이루는 "닫을 때 포커스 복원" — 각 화면의
// close*Modal() 끝에서 자기 자신의 참조를 넘겨 호출한다(예:
// `restoreModalFocus(closeMetaModal)`). 모달을 열 때 기억해둔 요소가 아직
// 문서에 남아 있으면 그리로 포커스를 되돌리고, 기억소는 비운다(다음에 같은
// onClose로 모달을 다시 열면 그 시점의 activeElement를 새로 기억해야 하므로).
export function restoreModalFocus(onClose: () => void): void {
  const target = modalFocusMemory.get(onClose);
  modalFocusMemory.delete(onClose);
  if (target && target.isConnected) target.focus();
}

// 확인 다이얼로그 — window.confirm()을 대체한다. 이 앱의 Tauri v2 WKWebView
// (macOS)에서는 @tauri-apps/plugin-dialog를 설치하지 않았고 네이티브 dialog
// delegate도 연결돼 있지 않아 window.confirm()이 실제로는 다이얼로그를 띄우지
// 못하고 조용히 false를 반환한다 — "삭제" 버튼을 눌러도 아무 반응이 없는 것처럼
// 보이는 원인이었다. showToast처럼 #app 트리 바깥(document.body)에 직접 붙여서
// 메인 render() 사이클(전체 재빌드)의 영향을 받지 않게 하고, createModalOverlay로
// 배경 드래그 안전 닫힘을 재사용한다. ESC와 오버레이 바깥 클릭·닫기 버튼은 모두
// 취소로 resolve(false)한다.
export function confirmDialog(
  message: string,
  opts?: { readonly title?: string; readonly confirmLabel?: string; readonly cancelLabel?: string; readonly danger?: boolean }
): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: boolean): void => {
      if (settled) return;
      settled = true;
      window.removeEventListener('keydown', onKeydown);
      overlay.remove();
      resolve(result);
    };

    const onKeydown = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') finish(false);
    };

    const confirmBtn = el('button', { className: 'btn btn-primary', onClick: () => finish(true) }, [opts?.confirmLabel ?? '확인']);
    confirmBtn.type = 'button';
    if (opts?.danger) {
      confirmBtn.style.background = 'var(--color-danger)';
      confirmBtn.style.borderColor = 'var(--color-danger)';
    }
    const cancelBtn = el('button', { className: 'btn', onClick: () => finish(false) }, [opts?.cancelLabel ?? '취소']);
    cancelBtn.type = 'button';

    const modalBox = el('div', { className: 'modal-box' }, [
      el('div', { className: 'modal-header' }, [
        el('h2', { className: 'modal-title' }, [opts?.title ?? '확인']),
        el('button', { className: 'modal-close-btn', onClick: () => finish(false) }, ['✕']),
      ]),
      el('div', { className: 'modal-body' }, [el('p', {}, [message]), el('div', { className: 'settings-form-actions' }, [confirmBtn, cancelBtn])]),
    ]);

    const overlay = createModalOverlay(modalBox, () => finish(false));
    window.addEventListener('keydown', onKeydown);
    document.body.appendChild(overlay);
    confirmBtn.focus();
  });
}

// 폼 값 바인딩 — 배경 이벤트(파일시스템 워처·폴링·타이머)로 인한 전체 재렌더
// (main.ts renderApp()의 root.replaceChildren())가 일어나도 입력 중이던 값이
// 사라지지 않게 하는 공용 헬퍼(hub 이슈 01m2zwcx7etvk9zh617tk7bayq). 이 앱은
// 매 렌더마다 폼 함수를 처음부터 다시 실행해 <input>/<textarea>/<select>를
// 새로 만든다 — 값을 그 DOM 노드에만 담으면 노드 자체가 버려지는 순간 값도
// 함께 사라진다.
//
// 해법: 값을 DOM이 아니라 호출부가 준비한 "드래프트 저장소"(get/set)에 둔다.
// 이벤트가 날 때마다 set()으로 저장소에 쓰고(렌더를 트리거하지 않는다 — 세션
// 채팅 입력창 views/sessions.ts의 renderChatInputArea가 먼저 쓰던 패턴을
// 승격했다), 렌더 시점에는 get()으로 그 저장소에서 값을 읽어 채운다. 저장소가
// 렌더 함수 "안"의 로컬 변수가 아니라 그 바깥(모달 오픈 시 한 번만 초기화되고
// 닫힐 때 버려지는 모듈 스코프 draft 객체, 또는 앱 생애주기 내내 의미 있는
// 값이면 전역 state)에 있으면 재렌더가 일어나도 값이 복원된다. 새 폼을 추가할
// 때는 `document.createElement('input')` 뒤에 `.value = x`를 직접 대입하지
// 말고 이 함수로 감싸라 — 그래야 다음 폼에서 같은 버그가 재발하지 않는다.
//
// 커서 위치(selection)·포커스 복원(A2, 리뷰 v0.2.5) — fieldId를 주면(선택)
// 이 필드가 재렌더 직전에 포커스를 갖고 있었는지 계속 추적해뒀다가, 노드가
// 통째로 교체된 뒤에도 새 노드로 포커스·캐럿을 이어받는다. 실측(리뷰
// probe-p1): 배경 워처 이벤트 1회로 채팅 입력창 포커스가 `focused:true,
// start:7` → `focused:false,start:13`(body)로 날아갔다 — 값은 위 드래프트
// 저장소 덕에 남았지만 커서 자리는 잃었다.
//
// 메커니즘: focusedFieldId/focusedSelectionStart/End는 "지금 어떤 필드가
// 포커스를 갖고 있는가"를 실시간으로 따라간다(focus/blur/keyup/click/select).
// main.ts의 renderApp()이 root.replaceChildren() 직전에
// captureActiveFocusForRerender()를 불러 그 시점 값을 renderTarget*으로
// 스냅샷한다 — replaceChildren() 자체가 포커스된 옛 노드를 지우며 즉시
// blur를 일으켜 focusedFieldId를 지우므로, 스냅을 먼저 떠 둬야 그 blur에
// 스냅이 지워지지 않는다. 이후 이번 렌더에서 boundField가 fieldId가 일치하는
// 새 노드를 만들면 pendingFocus로 예약해두고, main.ts가 새 트리를 문서에
// 붙인 직후 flushPendingFieldFocus()를 불러 실제로 focus()+
// setSelectionRange()를 적용한다(문서에 붙기 전에는 focus()가 먹지 않는다).
// fieldId를 생략하면 기존과 동일하게 값만 보존된다(포커스·캐럿 복원 없음) —
// 매 폼에 강제하지 않는다.
let focusedFieldId: string | null = null;
let focusedSelectionStart: number | null = null;
let focusedSelectionEnd: number | null = null;

let renderTargetFieldId: string | null = null;
let renderTargetSelectionStart: number | null = null;
let renderTargetSelectionEnd: number | null = null;

let pendingFocus: {
  readonly node: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;
  readonly start: number | null;
  readonly end: number | null;
} | null = null;

// main.ts의 renderApp()이 root.replaceChildren() 직전에 정확히 한 번 호출한다.
export function captureActiveFocusForRerender(): void {
  renderTargetFieldId = focusedFieldId;
  renderTargetSelectionStart = focusedSelectionStart;
  renderTargetSelectionEnd = focusedSelectionEnd;
  pendingFocus = null;
}

// main.ts의 renderApp()이 새 트리를 root에 appendChild한 직후 정확히 한 번
// 호출한다. pendingFocus가 없으면(이번 렌더에 포커스 복원 대상 필드가 없었다면)
// 아무 일도 하지 않는다.
//
// 버그 수정 메모: node.focus()는 'focus' 이벤트를 동기 발생시키고, 그 리스너
// (attachFocusCaretTracking)가 이 시점의 selectionStart를 즉시 캡처한다 — 그런데
// 이 시점은 아직 setSelectionRange()를 부르기 "전"이라 브라우저 기본 커서
// 위치(보통 문자열 끝)가 잘못 캡처된다. 연쇄 배경 이벤트로 렌더가 한 프레임 안에
// 두 번 일어나면(예: loadSessions()의 loading=true/loaded=true 각각이 렌더를
// 하나씩 냄), 두 번째 렌더가 이 잘못된 값을 스냅샷해 캐럿이 다시 끝으로
// 밀리는 재발이 있었다(실측). setSelectionRange() 호출 "직후" 여기서 직접
// focusedSelectionStart/End를 덮어써 다음 렌더가 항상 정확한 값을 스냅샷하게
// 한다 — 'select' 이벤트 발생 여부(브라우저마다 다를 수 있음)에 기대지 않는다.
export function flushPendingFieldFocus(): void {
  if (!pendingFocus) return;
  const { node, start, end } = pendingFocus;
  pendingFocus = null;
  node.focus();
  if (start !== null && 'setSelectionRange' in node) {
    try {
      (node as HTMLInputElement | HTMLTextAreaElement).setSelectionRange(start, end ?? start);
      focusedSelectionStart = start;
      focusedSelectionEnd = end ?? start;
    } catch {
      /* type=email/number 등 setSelectionRange를 지원하지 않는 입력 — 무시 */
    }
  }
}

// createModalOverlay가 "모달 첫 요소로 기본 포커스"보다 이 예약을 우선하기
// 위해 참조한다(모달 안의 boundField 입력이 재렌더로 다시 만들어지는 경우,
// 모달의 기본 포커스 로직이 그 복원을 가로채지 않아야 한다).
export function hasPendingFocus(): boolean {
  return pendingFocus !== null;
}

function attachFocusCaretTracking(node: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, fieldId: string): void {
  const captureSelection = (): void => {
    if ('selectionStart' in node) {
      try {
        focusedSelectionStart = (node as HTMLInputElement | HTMLTextAreaElement).selectionStart;
        focusedSelectionEnd = (node as HTMLInputElement | HTMLTextAreaElement).selectionEnd;
      } catch {
        // type="email"/"number" 등 일부 input은 selectionStart 접근 자체가 예외를 던진다.
        focusedSelectionStart = null;
        focusedSelectionEnd = null;
      }
    }
  };
  node.addEventListener('focus', () => {
    focusedFieldId = fieldId;
    captureSelection();
  });
  node.addEventListener('blur', () => {
    if (focusedFieldId === fieldId) focusedFieldId = null;
  });
  node.addEventListener('keyup', () => {
    if (focusedFieldId === fieldId) captureSelection();
  });
  node.addEventListener('click', () => {
    if (focusedFieldId === fieldId) captureSelection();
  });
  node.addEventListener('select', () => {
    if (focusedFieldId === fieldId) captureSelection();
  });
  // 이 노드가 만들어지는 시점이 이번 렌더의 스냅샷과 일치하면(=재렌더 직전에
  // 이 필드가 포커스를 갖고 있었다면) 새 노드로 포커스를 이어받도록 예약한다.
  if (renderTargetFieldId === fieldId) {
    pendingFocus = { node, start: renderTargetSelectionStart, end: renderTargetSelectionEnd };
  }
}

export function boundField<E extends HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(
  node: E,
  get: () => string,
  set: (value: string) => void,
  eventName: 'input' | 'change' = 'input',
  fieldId?: string
): E {
  node.value = get();
  node.addEventListener(eventName, () => set(node.value));
  if (fieldId) attachFocusCaretTracking(node, fieldId);
  return node;
}

// 체크박스/라디오 전용 — boundField와 같은 이유로 checked 상태도 드래프트에
// 보관한다. fieldId를 주면 위와 동일하게 포커스를 복원한다(체크박스는 캐럿
// 개념이 없어 start/end는 항상 null로 무시된다).
export function boundChecked(node: HTMLInputElement, get: () => boolean, set: (checked: boolean) => void, fieldId?: string): HTMLInputElement {
  node.checked = get();
  node.addEventListener('change', () => set(node.checked));
  if (fieldId) attachFocusCaretTracking(node, fieldId);
  return node;
}

// 토스트 — 설정 저장 등 "실제로는 아무것도 안 하지만 사용자에게 반응은 보여줘야 하는"
// 목업 액션의 피드백 채널. #app 트리 바깥(document.body 직속)에 붙여서 메인
// render() 사이클(전체 재빌드)의 영향을 받지 않게 한다.
let toastRoot: HTMLElement | null = null;

export function showToast(message: string): void {
  if (!toastRoot) {
    toastRoot = document.createElement('div');
    toastRoot.className = 'toast-root';
    document.body.appendChild(toastRoot);
  }
  const toast = document.createElement('div');
  toast.className = 'toast';
  toast.textContent = message;
  toastRoot.appendChild(toast);
  requestAnimationFrame(() => toast.classList.add('show'));
  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 250);
  }, 2200);
}
