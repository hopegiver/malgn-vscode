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

// 토글 스위치 — 카탈로그 "자동 업데이트" on/off 등에 사용하는 작은 목업 컴포넌트.
export function toggleSwitch(checked: boolean, onChange: () => void): HTMLElement {
  const track = el('span', { className: `toggle-track${checked ? ' on' : ''}`, onClick: onChange }, [
    el('span', { className: 'toggle-thumb' }, []),
  ]);
  track.setAttribute('role', 'switch');
  track.setAttribute('aria-checked', String(checked));
  track.setAttribute('tabindex', '0');
  return track;
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
