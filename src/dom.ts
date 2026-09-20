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
      e.preventDefault();
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
  return overlay;
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
// 커서 위치(selection)는 복원하지 않는다 — DOM 노드 자체가 매 렌더 교체되는
// 구조에서 selection까지 복원하려면 매 렌더마다 활성 포커스 노드를 추적해
// 재렌더 직후 같은 자리에 focus()+setSelectionRange()를 호출해야 하는데, 이
// 앱은 배경 이벤트가 잦아(세션 파일 감시·자율업무 폴링·devtools 경과시간
// 타이머) 그 훅을 모든 라우트에 안전하게 얹는 비용이 값 보존이라는 핵심 목표
// 대비 크다. 값이 보존되면 최소한 실질적 데이터 유실 없이 이어서 타이핑할 수
// 있다(포커스가 한 번 끊기는 것은 남지만, 지금까지 입력한 내용은 남는다).
export function boundField<E extends HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(
  node: E,
  get: () => string,
  set: (value: string) => void,
  eventName: 'input' | 'change' = 'input'
): E {
  node.value = get();
  node.addEventListener(eventName, () => set(node.value));
  return node;
}

// 체크박스/라디오 전용 — boundField와 같은 이유로 checked 상태도 드래프트에
// 보관한다.
export function boundChecked(node: HTMLInputElement, get: () => boolean, set: (checked: boolean) => void): HTMLInputElement {
  node.checked = get();
  node.addEventListener('change', () => set(node.checked));
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
