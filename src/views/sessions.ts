// 세션목록 — Rust 커맨드 list_claude_sessions()가 반환한 값을 그대로 쓴다. 정본
// 소스는 registry(지금 실행 중인 프로세스) → jsonl 대화 이력이라 끝난 세션도
// 함께 나열되며, 지금 실행 중인지는 각 행의 running 필드로만 구분한다(최근
// 30일/최대 100건으로 windowing됨). 상세 화면은 실제 jsonl 대화 전문을 읽어
// 보여주고(read_session_transcript, 설계 docs/design/session-chat.md), 하단
// 입력창으로 보낸 메시지는 그 세션에 실제로 이어져(send_session_message) 응답이
// 스트리밍된다.
import { el, liveIndicator, runningDot, createModalOverlay, showToast, boundField, restoreModalFocus } from '../dom';
import { state, notifyChange } from '../state';
import {
  fetchClaudeSessions,
  fetchSessionTranscript,
  sendSessionMessage,
  startNewSessionMessage,
  cancelSessionTurn,
  onSessionChatDelta,
  onSessionChatDone,
  openClaudeLoginTerminal,
  startClaudeAuthLogin,
  cancelClaudeAuthLogin,
  submitClaudeAuthLoginCode,
} from '../sessionsApi';
import type { ClaudeSessionRecord, ChatMessageKind, SessionTranscript } from '../sessionsApi';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { navigate } from '../route';
import { loadProjects, sortedProjectsByRecency } from './projects';
import { describeWorkspaceScanScope, summarizeSkippedProjects } from '../workspaceScanHint';

export function asString(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

export function asNumber(v: unknown): number | null {
  return typeof v === 'number' ? v : null;
}

export function asBoolean(v: unknown): boolean {
  return v === true;
}

export function projectNameFromCwd(cwd: string): string {
  if (!cwd) return '(알 수 없음)';
  const parts = cwd.split('/').filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : cwd;
}

// Rust가 대화 로그(jsonl) 첫 사용자 메시지에서 뽑아낸 title을 우선 쓴다. jsonl을
// 못 찾았거나 텍스트를 못 뽑았으면 세션 메타데이터의 name 필드로 폴백한다(예:
// "malgn-vscode-97" 같은 프로젝트명+임의문자 — 실제 대화 요약이 아니다).
export function sessionTitle(session: ClaudeSessionRecord): string {
  return asString(session.title) || asString(session.name) || '(제목 없음)';
}

export function formatTimestamp(ms: number | null): string {
  if (ms === null) return '-';
  try {
    return new Date(ms).toLocaleString('ko-KR', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
  } catch {
    return String(ms);
  }
}

export async function loadSessions(): Promise<void> {
  state.sessions.loading = true;
  state.sessions.error = null;
  notifyChange();
  try {
    state.sessions.items = await fetchClaudeSessions();
    state.sessions.loaded = true;
  } catch (err) {
    state.sessions.error = err instanceof Error ? err.message : '세션 목록을 불러오지 못했습니다. 잠시 후 다시 시도해도 계속되면 IT/개발팀에 문의하세요.';
  } finally {
    state.sessions.loading = false;
    notifyChange();
  }
}

// 목록 소스가 registry(지금 실행 중인 프로세스) + jsonl 대화 이력 결합으로 바뀌며
// 끝난 세션도 함께 섞인다. 이 화면에서는 "언제 시작했나"보다 "언제 마지막으로
// 말했나"(updatedAt)가 사용자 기대(최근 대화가 위)에 맞으므로 updatedAt 내림차순
// 으로 정렬한다. updatedAt이 없는 행(백엔드 폴백 등)은 startedAt으로 대신한다.
export function sortedSessions(): ClaudeSessionRecord[] {
  const sortKey = (s: ClaudeSessionRecord): number => asNumber(s.updatedAt) ?? asNumber(s.startedAt) ?? 0;
  return [...state.sessions.items].sort((a, b) => sortKey(b) - sortKey(a));
}

// 세션목록 화면(목록 뷰)을 완전히 떠날 때 main.ts에서 호출한다 — "새 세션" 모달이
// 열려 있었다면 window에 남은 ESC 리스너를 정리한다(leaveProjectsListView와 동일한 원칙).
export function leaveSessionsListView(): void {
  newSessionModalOpen = false;
  detachNewSessionModalEscHandler();
}

export function renderSessionsListView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['세션목록']),
      el('div', { className: 'page-subtitle-row' }, [
        el('div', { className: 'page-subtitle' }, ['실행 중·종료된 세션 모두 표시 — 지금 실행 중인 세션만 ● 표시']),
        ...(state.sessions.live ? [liveIndicator()] : []),
      ]),
    ]),
    el('div', { className: 'devtool-header-actions' }, [
      el('button', { className: 'btn btn-primary', onClick: openNewSessionModal }, ['+ 새 세션']),
      el('button', { className: 'btn', onClick: () => void loadSessions(), disabled: state.sessions.loading }, [
        state.sessions.loading ? '불러오는 중…' : '↻ 새로고침',
      ]),
    ]),
  ]);

  const body: HTMLElement[] = [];

  if (state.sessions.loading && !state.sessions.loaded) {
    body.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])]));
  } else if (state.sessions.error) {
    body.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${state.sessions.error}`]),
        el('button', { className: 'btn', onClick: () => void loadSessions() }, ['다시 시도']),
      ])
    );
  } else if (state.sessions.items.length === 0) {
    body.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['세션이 없습니다']),
        el('div', { className: 'state-block-desc' }, ['최근 30일 안에 실행했거나 대화한 세션을 찾지 못했습니다.']),
      ])
    );
  } else {
    body.push(el('div', { className: 'session-list' }, sortedSessions().map(renderSessionRow)));
  }

  const rootChildren: HTMLElement[] = [header, ...body];
  if (newSessionModalOpen) {
    if (!newSessionModalEscHandler) {
      newSessionModalEscHandler = (e) => {
        if (e.key === 'Escape') closeNewSessionModal();
      };
      window.addEventListener('keydown', newSessionModalEscHandler);
    }
    rootChildren.push(renderNewSessionModal());
  } else {
    detachNewSessionModalEscHandler();
  }

  return el('div', {}, rootChildren);
}

function renderSessionRow(session: ClaudeSessionRecord): HTMLElement {
  const sessionId = asString(session.sessionId);
  const title = sessionTitle(session);
  const cwd = asString(session.cwd);
  const name = asString(session.name);
  const version = asString(session.version);
  const running = asBoolean(session.running);

  const goToDetail = (): void => navigate(`#/sessions/${encodeURIComponent(sessionId)}`);

  const row = el(
    'div',
    {
      className: 'session-row',
      onClick: goToDetail,
      onKeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') goToDetail();
      },
    },
    [
      el('div', { className: 'session-row-main' }, [
        el('div', { className: 'session-row-name' }, [title]),
        el('div', { className: 'session-row-cwd' }, [name ? `${projectNameFromCwd(cwd)} · ${name}` : projectNameFromCwd(cwd)]),
      ]),
      el('div', { className: 'session-row-meta' }, [
        el('span', { className: 'badge badge-unknown' }, [version || '버전 없음']),
        ...(running ? [runningDot()] : []),
      ]),
      el('div', { className: 'session-row-time' }, [
        el('div', {}, [`시작 ${formatTimestamp(asNumber(session.startedAt))}`]),
        el('div', {}, [`갱신 ${formatTimestamp(asNumber(session.updatedAt))}`]),
      ]),
    ]
  );
  row.setAttribute('role', 'button');
  row.setAttribute('tabindex', '0');
  return row;
}

// ---------------- 상세 — 실제 대화 + 이어쓰기 ----------------
// 설계: docs/design/session-chat.md. 화면은 메시지 목록 + 입력창 하나뿐이다
// (말풍선 아바타·모델 선택 등 장식·복잡한 UI를 얹지 않는다). 메시지 텍스트는
// 항상 el()의 textContent 경로로만 들어간다 — innerHTML·마크다운 렌더러 금지
// (대화 로그에 임의의 HTML 문자열이 그대로 들어있을 수 있다, S12).

let unlistenChatDelta: UnlistenFn | null = null;
let unlistenChatDone: UnlistenFn | null = null;
// 직전 턴이 취소로 끝났는지 — §7 "중단 후 다음 전송" 안내에만 쓰는 로컬 UI 상태.
// IPC 계약(state.sessionChat)에는 없는 값이라 여기 모듈 스코프에 따로 둔다.
let lastTurnCanceled = false;
// 방금 보낸 사용자 메시지 — done 후 전체 재조회 전까지는 jsonl에 아직 없을 수
// 있어(스트리밍 중) 화면에 즉시 보이도록 낙관적으로 echo한다. done 시 지운다
// (재조회한 transcript.messages가 그 시점부터 정본이다).
let pendingUserText: string | null = null;
// draft(아직 session_id 없음) 상태에서 start_new_session_message() invoke가
// 왕복하는 구간을 잠그는 플래그(RV-002). chat.turnId는 그 invoke 응답이 돌아온
// "뒤"에야 설정되므로 turnId만으로는 왕복 구간 자체의 연타(중복 세션 생성)를
// 막지 못한다 — 이 플래그가 전송 시작~종료(성공/실패/이탈 모두 finally)까지를
// 커버한다.
let draftSending = false;
// send_session_message() IPC 응답(turnId)보다 session-chat-done 이벤트가 먼저
// 도착하는 레이스 대비(M2) — Rust는 스레드를 스폰한 뒤 반환하므로 즉시 실패
// 경로의 done이 invoke 응답보다 먼저 올 수 있다. 그 turnId를 여기 기억해뒀다가
// 뒤늦게 도착한 invoke 응답이 "이미 끝난 턴"을 다시 잠그지 않게 한다.
const earlyDoneTurnIds = new Set<string>();
// 세션 상세 화면을 listen() 왕복보다 빠르게 연속 전환할 때, 아직 등록 중이던
// 이전 화면의 리스너가 모듈 변수를 뒤늦게 덮어써 해제 불가능해지는 것을 막는
// 세대 카운터(m4). enterSessionChatView/leaveSessionChatView가 항상 올린다.
let chatViewGeneration = 0;

// 자동 하단 스크롤(m2) — 실제 스크롤 컨테이너(`.chat-thread`)는 매 렌더마다
// 새로 만들어지는 요소다(델타마다 전체 재렌더는 그대로 둔다, m3 불변). 그래서
// "사용자가 하단 근처였는지"는 새로 생긴 요소가 아니라 이 모듈변수로 렌더를
// 넘나들며 기억한다 — 사용자가 위로 스크롤해 읽고 있으면 다음 렌더에서도
// 낚아채지 않는다.
let chatNearBottom = true;

// 스크롤 컨테이너는 이제 `.content` 전체가 아니라 이 화면 안의 `.chat-thread`다
// (헤더/입력창은 flexbox로 고정되고 메시지 목록만 자체 스크롤한다, styles.css
// `.content.chat-route`/`.chat-thread`). "사용자가 하단 근처였는지" 판단
// 로직(chatNearBottom 계산식) 자체는 그대로 두고, 조회 대상 엘리먼트만 바꾼다.
function scheduleChatAutoScroll(): void {
  requestAnimationFrame(() => {
    const threadEl = document.querySelector<HTMLElement>('.chat-thread');
    if (!threadEl) return;
    if (chatNearBottom) threadEl.scrollTop = threadEl.scrollHeight;
    threadEl.addEventListener(
      'scroll',
      () => {
        chatNearBottom = threadEl.scrollHeight - threadEl.scrollTop - threadEl.clientHeight < 80;
      },
      { passive: true }
    );
  });
}

// 세션 메타데이터 모달 — chat-header의 버튼으로 열고 배경 클릭/ESC/닫기 버튼으로
// 닫는다. 화면을 넘나드는 값이 아니라 이 화면 안에서만 쓰는 로컬 UI 상태라
// chatNearBottom과 같은 모듈 스코프 변수로 둔다(state.sessionChat IPC 계약과 무관).
let metaModalOpen = false;
let metaModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachMetaModalEscHandler(): void {
  if (metaModalEscHandler) {
    window.removeEventListener('keydown', metaModalEscHandler);
    metaModalEscHandler = null;
  }
}

// "새 세션" 프로젝트 선택 모달 — 세션목록 화면 우측 상단 버튼으로 연다. 프로젝트
// 화면의 카드 "새 세션" 버튼과 최종 목적지(라우트)는 같지만, 이 화면에서는
// 먼저 어느 프로젝트인지 골라야 하므로 프로젝트 목록을 모달로 띄운다(metaModal과
// 동일하게 배경클릭/ESC/닫기 버튼 3경로로 닫는 로컬 UI 상태).
let newSessionModalOpen = false;
let newSessionModalEscHandler: ((e: KeyboardEvent) => void) | null = null;

function detachNewSessionModalEscHandler(): void {
  if (newSessionModalEscHandler) {
    window.removeEventListener('keydown', newSessionModalEscHandler);
    newSessionModalEscHandler = null;
  }
}

function openNewSessionModal(): void {
  newSessionModalOpen = true;
  notifyChange();
  // 세션목록 화면만 먼저 열었을 수도 있어 프로젝트 목록이 비어 있을 수 있다 —
  // 아직 조회한 적이 없으면(로딩 중도 아니면) 여기서 처음으로 불러온다.
  if (!state.dashboard.loaded && !state.dashboard.loading) void loadProjects();
}

function closeNewSessionModal(): void {
  newSessionModalOpen = false;
  detachNewSessionModalEscHandler();
  restoreModalFocus(closeNewSessionModal); // C3: 열기 직전 포커스로 복원
  notifyChange();
}

function chooseProjectForNewSession(path: string): void {
  closeNewSessionModal();
  navigate(`#/sessions/new/${encodeURIComponent(path)}`);
}

function renderNewSessionModal(): HTMLElement {
  const bodyChildren: HTMLElement[] = [
    el('div', { className: 'settings-form-hint' }, ['새 세션을 시작할 프로젝트를 선택하세요.']),
  ];

  if (state.dashboard.loading && !state.dashboard.loaded) {
    bodyChildren.push(el('div', { className: 'state-block-desc' }, ['불러오는 중…']));
  } else if (state.dashboard.error) {
    bodyChildren.push(
      el('div', { className: 'alert' }, [
        el('span', {}, [`⚠ ${state.dashboard.error}`]),
        el('button', { className: 'btn', onClick: () => void loadProjects() }, ['다시 시도']),
      ])
    );
  } else if (state.dashboard.projects.length === 0) {
    const emptyStateChildren: HTMLElement[] = [
      el('div', { className: 'state-block-title' }, ['먼저 프로젝트를 추가하세요']),
      el('div', { className: 'state-block-desc' }, [describeWorkspaceScanScope()]),
    ];
    const skipSummary = summarizeSkippedProjects(state.dashboard.skipped);
    if (skipSummary) {
      emptyStateChildren.push(el('div', { className: 'state-block-desc' }, [skipSummary]));
    }
    bodyChildren.push(el('div', { className: 'state-block' }, emptyStateChildren));
  } else {
    const list = el('div', { className: 'session-list' });
    for (const p of sortedProjectsByRecency()) {
      list.appendChild(
        el(
          'div',
          {
            className: 'session-row',
            onClick: () => chooseProjectForNewSession(p.path),
            onKeydown: (e) => {
              if (e.key === 'Enter' || e.key === ' ') chooseProjectForNewSession(p.path);
            },
          },
          [
            el('div', { className: 'session-row-main' }, [
              el('div', { className: 'session-row-name' }, [p.name]),
              el('div', { className: 'session-row-cwd' }, [p.path]),
            ]),
          ]
        )
      );
      const row = list.lastElementChild as HTMLElement;
      row.setAttribute('role', 'button');
      row.setAttribute('tabindex', '0');
    }
    bodyChildren.push(list);
  }

  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['새 세션 — 프로젝트 선택']),
      el('button', { className: 'modal-close-btn', onClick: closeNewSessionModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, bodyChildren),
  ]);
  return createModalOverlay(modalBox, closeNewSessionModal);
}

function openMetaModal(): void {
  metaModalOpen = true;
  notifyChange();
}

function closeMetaModal(): void {
  metaModalOpen = false;
  detachMetaModalEscHandler();
  restoreModalFocus(closeMetaModal); // C3: 열기 직전 포커스로 복원
  notifyChange();
}

// preserveError: 직전에 session-chat-done이 이미 채워놓은 턴 실패 에러(원문 +
// authError)를 이 조회가 지우지 않게 한다. 발견 경위(하네스 auth-error
// 시나리오 디버깅) — done 핸들러는 성공/실패 관계없이 "done 후 전체 재조회"
// 원칙(§2)에 따라 항상 loadSessionTranscript를 호출하는데, 이 함수가 조회
// 성공 시 무조건 error를 null로 지워 인증 실패 등 방금 뜬 배너가 거의 즉시
// (transcript 조회가 보통 매우 빠르므로) 사라지는 회귀가 있었다. 화면 진입 시
// 최초 로드(§ enterSessionChatView)나 "다시 시도" 버튼은 여전히 기본값(false)
// 그대로 지운다 — 그쪽은 chat.error가 "이 조회 자체의 실패"를 뜻하므로 자기
// 성공 시 지우는 게 맞다.
async function loadSessionTranscript(sessionId: string, showLoading: boolean, preserveError = false): Promise<void> {
  const chat = state.sessionChat;
  if (chat.sessionId !== sessionId) return; // 그 사이 화면을 떠났으면 버린다
  if (showLoading) {
    chat.loading = true;
    notifyChange();
  }
  try {
    const transcript = await fetchSessionTranscript(sessionId);
    if (state.sessionChat.sessionId !== sessionId) return; // 응답 도착 전에 이탈
    state.sessionChat.transcript = transcript;
    if (!preserveError) state.sessionChat.error = null;
    // 재진입 시 이미 진행 중인 턴에 재부착(M3) — 이 화면 인스턴스가 스스로 보낸
    // 턴을 이미 들고 있으면 덮어쓰지 않는다. 과거 델타는 못 따라잡지만 이후
    // 델타/done은 정상 수신되고, 입력창은 전송 중 상태(취소 버튼)로 전환된다.
    if (state.sessionChat.turnId === null && transcript.activeTurnId) {
      state.sessionChat.turnId = transcript.activeTurnId;
    }
  } catch (err) {
    if (state.sessionChat.sessionId !== sessionId) return;
    state.sessionChat.error = err instanceof Error ? err.message : String(err);
  } finally {
    if (state.sessionChat.sessionId === sessionId) state.sessionChat.loading = false;
    notifyChange();
  }
}

// 이 화면 인스턴스가 진입할 때마다 공통으로 초기화하는 로컬 UI 상태(m4/§7 등
// state.sessionChat IPC 계약 밖의 값들) — 세션 상세/draft 진입 양쪽에서 쓴다.
function resetChatLocalUiState(): void {
  lastTurnCanceled = false;
  pendingUserText = null;
  earlyDoneTurnIds.clear();
  chatNearBottom = true; // 새로 진입하면 항상 하단(최신)에서 시작한다
  metaModalOpen = false;
  detachMetaModalEscHandler();
  draftSending = false; // 이전 draft 화면에서 남았을 수 있는 잠금을 새 진입 시 초기화
}

// 델타/완료 스트리밍 이벤트 구독 — sessionId를 캡처하지 않고 매번
// `state.sessionChat.sessionId`를 동적으로 비교한다. draft 상태(아직 session_id가
// 없음)에서도 미리 구독을 걸어둘 수 있게 하기 위해서다 — 첫 메시지 전송이
// 성공해 sessionId가 배정되는 순간부터 그 이후 도착하는 이벤트가 자연히
// 필터를 통과한다(sendDraftMessage 참고).
async function attachChatListeners(myGeneration: number): Promise<void> {
  try {
    const unlistenDelta = await onSessionChatDelta((d) => {
      if (myGeneration !== chatViewGeneration) return;
      // draft 구간(RV-003)에서는 승격 전이라 state.sessionChat.sessionId가 아직
      // null이다 — null과 실제 sessionId를 비교하면 항상 불일치라 통과가
      // 불가능해진다. sessionId가 배정된 뒤에만 정확히 일치를 요구한다.
      if (state.sessionChat.sessionId !== null && d.sessionId !== state.sessionChat.sessionId) return;
      // turnId가 아직 배정 전(M2와 같은 레이스)이면 sessionId만으로 이 세션의
      // 진행 중인 턴으로 간주해 버리지 않는다. 배정 후에는 정확히 일치해야 한다.
      if (state.sessionChat.turnId !== null && d.turnId !== state.sessionChat.turnId) return;
      if (d.kind === 'text') state.sessionChat.streamingText += d.text;
      else state.sessionChat.streamingTools = [...state.sessionChat.streamingTools, d.text];
      notifyChange();
    });
    if (myGeneration !== chatViewGeneration) {
      unlistenDelta(); // 이미 다른 화면으로 넘어갔다 — 모듈 변수는 건드리지 않고 바로 정리
    } else {
      unlistenChatDelta = unlistenDelta;
    }

    const unlistenDone = await onSessionChatDone((d) => {
      if (myGeneration !== chatViewGeneration) return;
      // RV-003: draft 구간에서는 state.sessionChat.sessionId가 아직 null이라
      // sessionId 필터가 항상 걸려 이 아래 earlyDoneTurnIds.add()가 도달 불가였다
      // (sendDraftMessage의 doneAlready 체크가 늘 false가 되어 turnId가 영원히
      // 해제되지 않는 원인). sessionId가 배정된 뒤에만 정확히 일치를 요구한다.
      if (state.sessionChat.sessionId !== null && d.sessionId !== state.sessionChat.sessionId) return;
      if (state.sessionChat.turnId !== null && d.turnId !== state.sessionChat.turnId) return;
      // turnId 배정 전에 done이 먼저 온 경우(M2) — sendChatMessage/sendDraftMessage의
      // invoke 응답이 뒤늦게 이 turnId로 다시 잠그지 않도록 기억해둔다.
      if (state.sessionChat.turnId === null) earlyDoneTurnIds.add(d.turnId);
      const sid = state.sessionChat.sessionId;
      state.sessionChat.turnId = null;
      state.sessionChat.streamingText = '';
      state.sessionChat.streamingTools = [];
      lastTurnCanceled = d.canceled;
      pendingUserText = null;
      // 원문 에러는 항상 그대로 보존한다(home.ts widgetErrorShell 선례) — authError는
      // 그 원문을 대체하지 않고, 화면이 로그인 안내를 추가로 보여줄지만 결정한다.
      if (!d.ok && d.error) {
        state.sessionChat.error = d.error;
        state.sessionChat.authError = d.authError;
      }
      notifyChange();
      // "done 후 전체 재조회" — 화면에 남는 최종 상태는 항상 파일(jsonl)
      // 하나에서만 만든다(중단·다른 창의 동시 기록도 자동 반영됨). 방금 이
      // 턴이 에러로 끝났다면(d.error) 그 에러를 이 재조회가 지우지 않게
      // preserveError=true를 넘긴다 — 위 loadSessionTranscript 주석 참고.
      if (sid) void loadSessionTranscript(sid, false, !d.ok && !!d.error);
    });
    if (myGeneration !== chatViewGeneration) {
      unlistenDone();
    } else {
      unlistenChatDone = unlistenDone;
    }
  } catch {
    // Tauri IPC 브리지가 없는 환경(플레인 브라우저) — 스트리밍 구독만 실패하고
    // 조회는 계속 동작한다.
  }
}

// 세션 상세 화면 진입 — 트랜스크립트를 불러오고 스트리밍 이벤트를 구독한다.
// main.ts의 handleNavigation()이 라우트가 바뀔 때 호출한다.
export async function enterSessionChatView(sessionId: string): Promise<void> {
  const myGeneration = ++chatViewGeneration; // m4: 이 진입 콜의 세대를 고정
  resetChatLocalUiState();
  state.sessionChat = {
    sessionId,
    draftProjectPath: null,
    transcript: null,
    loading: false,
    error: null,
    authError: false,
    turnId: null,
    streamingText: '',
    streamingTools: [],
    input: '',
  };
  notifyChange();
  void loadSessionTranscript(sessionId, true);
  await attachChatListeners(myGeneration);
}

// "새 세션" draft 화면 진입 — 프로젝트 카드에서 시작한다. session_id가 아직
// 없으므로 조회할 트랜스크립트도 없다 — 사용자가 첫 메시지를 보낼 때
// sendDraftMessage()가 비로소 start_new_session_message()를 호출해 세션을
// 발급받고 이 상태를 정상 세션으로 승격시킨다.
export async function enterSessionDraftView(projectPath: string): Promise<void> {
  const myGeneration = ++chatViewGeneration;
  resetChatLocalUiState();
  state.sessionChat = {
    sessionId: null,
    draftProjectPath: projectPath,
    transcript: null,
    loading: false,
    error: null,
    authError: false,
    turnId: null,
    streamingText: '',
    streamingTools: [],
    input: '',
  };
  notifyChange();
  await attachChatListeners(myGeneration);
}

// 세션 상세/draft 화면 이탈 — 리스너를 반드시 해제하고 상태를 초기화한다.
export function leaveSessionChatView(): void {
  chatViewGeneration++; // m4: 아직 listen() 대기 중이던 진입 콜을 무효화
  metaModalOpen = false;
  detachMetaModalEscHandler();
  if (unlistenChatDelta) {
    unlistenChatDelta();
    unlistenChatDelta = null;
  }
  if (unlistenChatDone) {
    unlistenChatDone();
    unlistenChatDone = null;
  }
  state.sessionChat = {
    sessionId: null,
    draftProjectPath: null,
    transcript: null,
    loading: false,
    error: null,
    authError: false,
    turnId: null,
    streamingText: '',
    streamingTools: [],
    input: '',
  };
  pendingUserText = null;
  earlyDoneTurnIds.clear();
}

export async function sendChatMessage(sessionId: string, text: string): Promise<void> {
  const chat = state.sessionChat;
  if (chat.turnId) return; // 세션당 1개 — 전송 중에는 재전송하지 않는다
  const trimmed = text.trim();
  if (!trimmed) {
    chat.error = '보낼 내용을 입력하세요.';
    notifyChange();
    return;
  }
  chat.error = null;
  chat.authError = false;
  chat.input = '';
  pendingUserText = trimmed;
  notifyChange();
  try {
    const started = await sendSessionMessage(sessionId, trimmed);
    if (state.sessionChat.sessionId !== sessionId) return;
    if (earlyDoneTurnIds.delete(started.turnId)) {
      // session-chat-done이 이 invoke 응답보다 먼저 도착해 이미 처리됐다(M2).
      // 화면은 그 done 처리 시점에 이미 정리·재조회됐으니 다시 잠그지 않는다.
      return;
    }
    state.sessionChat.turnId = started.turnId;
    state.sessionChat.streamingText = '';
    state.sessionChat.streamingTools = [];
    lastTurnCanceled = false;
  } catch (err) {
    if (state.sessionChat.sessionId !== sessionId) {
      pendingUserText = null;
      return;
    }
    state.sessionChat.error = err instanceof Error ? err.message : String(err);
    state.sessionChat.input = trimmed; // 실패했으니 재전송할 수 있게 되돌려준다
    pendingUserText = null;
  } finally {
    notifyChange();
  }
}

// draft 화면에서 첫 메시지를 보낸다 — 이 시점에 비로소 백엔드가 실제로 세션을
// 생성하고 spawn한다(project-cards 4-b). 성공하면 이 상태를 그 session_id의
// 정상 세션으로 승격시키고 라우트도 `#/sessions/<id>`로 바꾼다(4-c: 사이드바에
// 낙관적 항목을 넣지 않고, 화면 전환만으로 사용자 기대를 충족한다).
export async function sendDraftMessage(projectPath: string, text: string): Promise<void> {
  const chat = state.sessionChat;
  // RV-002: chat.turnId는 아래 invoke 응답이 돌아온 뒤에야 설정되므로 그것만으로는
  // invoke 왕복 구간(연타) 자체를 막지 못한다. draftSending이 그 구간 전체를 잠근다.
  if (chat.turnId || draftSending) return;
  const trimmed = text.trim();
  if (!trimmed) {
    chat.error = '보낼 내용을 입력하세요.';
    notifyChange();
    return;
  }
  chat.error = null;
  chat.authError = false;
  chat.input = '';
  pendingUserText = trimmed;
  draftSending = true;
  notifyChange();
  try {
    const started = await startNewSessionMessage(projectPath, trimmed);
    if (state.sessionChat.draftProjectPath !== projectPath) return; // 그 사이 이 draft 화면을 떠났다
    // session-chat-done이 이 invoke 응답보다 먼저 도착해 이미 처리됐을 수 있다(M2와
    // 같은 레이스, RV-003로 필터를 고쳐 이제 정상적으로 기록된다) — 그래도 세션
    // 자체는 생성됐으니 승격은 그대로 진행한다.
    const doneAlready = earlyDoneTurnIds.delete(started.turnId);
    state.sessionChat.sessionId = started.sessionId;
    state.sessionChat.draftProjectPath = null;
    if (!doneAlready) {
      state.sessionChat.turnId = started.turnId;
      state.sessionChat.streamingText = '';
      state.sessionChat.streamingTools = [];
      lastTurnCanceled = false;
    }
    // 라우트를 먼저 실제 session_id로 바꾼 뒤에 알린다 — notifyChange()가 그
    // 시점의 URL 해시를 기준으로 다시 그리므로, 순서가 바뀌면 화면이 draft
    // 라우트에 머문 채 이미 배정된 sessionId 상태로 한 프레임 어긋나게 그려진다.
    navigate(`#/sessions/${encodeURIComponent(started.sessionId)}`);
    // RV-001: 승격 직후 바로 조회하면 `claude -p --session-id`가 아직 jsonl을
    // 만들지 않아 실패해 화면이 "대화 기록을 불러오지 못했습니다" 오류로 덮인다.
    // 진행 중인 일반 경로(!doneAlready)는 session-chat-done 리스너(RV-003로 고친
    // 필터 덕에 이제 이 세션에도 정상 도달)가 완료 시점에 재조회하도록 맡긴다.
    // doneAlready(턴이 이미 끝난 뒤 승격된 드문 레이스)만 예외 — 그 경우 프로세스가
    // 이미 jsonl을 다 썼을 것이므로 즉시(로딩 상태로) 조회해도 안전하다.
    if (doneAlready) void loadSessionTranscript(started.sessionId, true);
  } catch (err) {
    if (state.sessionChat.draftProjectPath !== projectPath) {
      pendingUserText = null;
      return;
    }
    state.sessionChat.error = err instanceof Error ? err.message : String(err);
    state.sessionChat.input = trimmed; // 실패했으니 재전송할 수 있게 되돌려준다
    pendingUserText = null;
  } finally {
    draftSending = false;
    notifyChange();
  }
}

// hub 이슈 01m2wm4e9k822fk73yahrrnnce: 인증 실패 알림의 "터미널에서 로그인"
// 버튼 — gh/wrangler 연동(views/settings.ts handleLoginMcp 등)과 같은 패턴으로
// 앱이 대신 로그인하지 않고 터미널 창을 여는 데까지만 관여한다. 로그인 완료
// 여부는 이 화면이 알 수 없으므로, 결과는 토스트로만 알리고 입력창은 그대로
// 둔다 — 사용자가 로그인 후 직접 재전송하면 된다.
async function handleClaudeLogin(): Promise<void> {
  try {
    const result = await openClaudeLoginTerminal();
    showToast(result.message);
  } catch (err) {
    showToast(`터미널을 여는 데 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// hub 이슈 01m2wm4e9k822fk73yahrrnnce 후속, v0.2.11 사고(hub 이슈
// 01m33qe0zhn55mczhcdgec2b61)의 근본 수정판 — 터미널을 여는 대신 앱 안에서
// `claude auth login --claudeai`를 시작한다(claude_auth.rs). 여기서는 spawn
// 성공만 반영하고, 그 다음 진행 상황(로그인 URL·원문 출력·완료/실패/취소)은
// main.ts가 앱 시작 시 구독한 이벤트가 state.claudeAuthLogin을 갱신하며
// 채운다 — 이 화면은 그 상태를 읽기만 한다(아래 renderChatErrorBlock).
async function handleClaudeAuthLoginStart(): Promise<void> {
  state.claudeAuthLogin.active = true;
  state.claudeAuthLogin.url = null;
  state.claudeAuthLogin.error = null;
  state.claudeAuthLogin.output = '';
  state.claudeAuthLogin.codeInput = '';
  notifyChange();
  try {
    await startClaudeAuthLogin();
  } catch (err) {
    state.claudeAuthLogin.active = false;
    state.claudeAuthLogin.error = err instanceof Error ? err.message : String(err);
    notifyChange();
  }
}

// 진행 중인 로그인이 없어도(이미 끝난 뒤 늦게 눌렀다 등) 백엔드가 no-op으로
// 처리한다 — 여기서 별도 방어 분기를 두지 않는다.
async function handleClaudeAuthLoginCancel(): Promise<void> {
  try {
    await cancelClaudeAuthLogin();
  } catch (err) {
    showToast(`취소 요청에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// CLI가 스스로 여는 브라우저 창과 별개로, 그게 조용히 실패했을 때(패키징된
// 앱의 다른 PATH/환경 등) 쓰는 수동 폴백 — 자동 오픈이 이미 성공한
// 경우에는 사용자가 이 버튼을 누르지 않으므로 탭이 중복되지 않는다.
function handleOpenClaudeAuthLoginUrl(url: string): void {
  void openUrl(url).catch(() => {
    showToast('브라우저를 여는 데 실패했습니다. 주소를 직접 복사해 열어주세요.');
  });
}

// 근본 수정 핵심(claude_auth.rs 상단 주석): claude가 실제로 코드를 요구하는지
// 이 앱은 판단하지 않는다 — "특정 문구를 감지해야 입력창이 뜬다"는 분기
// 자체를 없앴다. 로그인이 진행 중인 동안(renderChatErrorBlock의 login.active)
// 이 함수를 부르는 입력창+버튼이 항상 존재하고, claude가 코드를 요구하지
// 않으면 그냥 아무도 안 쓸 뿐이다.
async function handleClaudeAuthLoginSubmitCode(): Promise<void> {
  const code = state.claudeAuthLogin.codeInput.trim();
  if (!code) {
    showToast('코드를 입력해주세요.');
    return;
  }
  state.claudeAuthLogin.submitting = true;
  notifyChange();
  try {
    await submitClaudeAuthLoginCode(code);
    state.claudeAuthLogin.codeInput = '';
    showToast('코드를 전달했습니다.');
  } catch (err) {
    showToast(`코드 전달에 실패했습니다: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    state.claudeAuthLogin.submitting = false;
    notifyChange();
  }
}

// 로그인이 진행 중일 때(login.active) 보여줄 패널 — 상태 안내, URL 링크 열기
// 폴백, 상시 코드 입력창(분기 없음), 원문 출력 로그, 취소 버튼 순서.
function renderClaudeAuthLoginActivePanel(login: typeof state.claudeAuthLogin): HTMLElement[] {
  const children: HTMLElement[] = [
    el('div', { className: 'chat-sample-note' }, [
      login.url
        ? '브라우저에서 로그인을 진행해주세요. 자동으로 열리지 않았다면 아래 링크를 클릭하세요.'
        : '로그인을 시작하는 중…',
    ]),
  ];

  if (login.url) {
    const url = login.url;
    children.push(el('button', { className: 'btn', onClick: () => handleOpenClaudeAuthLoginUrl(url) }, ['로그인 링크 열기']));
  }

  const codeInput = boundField(
    document.createElement('input'),
    () => login.codeInput,
    (v) => {
      state.claudeAuthLogin.codeInput = v; // 렌더를 트리거하지 않는다(키 입력마다 포커스가 끊기지 않도록)
    },
    'input',
    'claude-auth-login-code'
  );
  codeInput.className = 'settings-input';
  codeInput.type = 'text';
  codeInput.autocomplete = 'off';
  codeInput.placeholder = 'claude가 코드를 요구하면 여기에 붙여넣으세요 (필요할 때만)';
  codeInput.disabled = login.submitting;
  codeInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (!login.submitting) void handleClaudeAuthLoginSubmitCode();
    }
  });
  const submitBtn = el(
    'button',
    { className: 'btn', disabled: login.submitting, onClick: () => void handleClaudeAuthLoginSubmitCode() },
    [login.submitting ? '전달하는 중…' : '코드 제출']
  );
  children.push(el('div', { className: 'chat-auth-code-row' }, [codeInput, submitBtn]));

  if (login.output) {
    children.push(el('pre', { className: 'chat-auth-login-output' }, [login.output]));
  }

  children.push(el('button', { className: 'btn', onClick: () => void handleClaudeAuthLoginCancel() }, ['로그인 취소']));
  return children;
}

// 로그인 진행 패널 — chat.error/authError와 무관하게 state.claudeAuthLogin.active
// 하나만으로 그린다(hub 이슈, v0.2.11류 회귀 재발 방지). leaveSessionChatView가
// 화면을 뜰 때마다 state.sessionChat(error·authError 포함)을 통째로 비우므로,
// 로그인 패널을 authError에 얹어두면 로그인을 시작한 뒤 다른 화면에 갔다
// 돌아왔을 때(그 사이에도 백엔드 자식 프로세스는 계속 코드를 기다린다) 입력창이
// 통째로 사라진다 — active 하나만 보고 그리면 화면 이동과 무관하게 항상 뜬다.
// 세션 상세/draft 두 화면에서만 로그인을 시작할 수 있어(handleClaudeAuthLoginStart
// 호출부가 이 파일뿐) 두 화면의 bottomFixed에만 이 블록을 얹는다 — 다른 화면
// (홈·세션목록 등)까지 전역으로 넓히는 건 이번 범위를 넘는다.
function renderClaudeAuthLoginBlock(): HTMLElement[] {
  const login = state.claudeAuthLogin;
  if (!login.active) return [];
  return [el('div', { className: 'alert chat-auth-login-panel' }, renderClaudeAuthLoginActivePanel(login))];
}

// 세션 상세/draft 화면이 공유하는 하단 에러 표시. 일반 에러는 기존 그대로
// "⚠ {원문}" 한 줄이지만, 인증 실패(chat.authError)일 때는 원문 에러를 버리지
// 않으면서(home.ts widgetErrorShell 선례) 무엇을 해야 하는지를 버튼으로 함께
// 보여준다 — 이 갭이 실사용자 보고(claude CLI 인증 체크/처리 로직도 화면도
// 없다)의 핵심이었다. "앱에서 로그인"(새 경로)이 실패하거나 이 환경에서
// 아예 쓸 수 없을 때 막다른 길에 몰리지 않도록 "터미널 열기"(기존 경로)를
// 항상 나란히 남겨둔다. 로그인이 진행 중일 때의 입력창·취소 버튼 자체는
// renderClaudeAuthLoginBlock()이 authError와 무관하게 따로 그리므로, 여기서는
// 중복 렌더하지 않도록 login.active가 아닐 때만 시작 버튼들을 붙인다.
function renderChatErrorBlock(): HTMLElement[] {
  const chat = state.sessionChat;
  if (!chat.error) return [];
  if (chat.authError) {
    const login = state.claudeAuthLogin;
    const children: HTMLElement[] = [
      el('div', {}, [
        el('div', {}, ['claude CLI에 로그인이 필요합니다.']),
        el('div', { className: 'chat-sample-note' }, [`원본 에러: ${chat.error}`]),
      ]),
    ];

    if (!login.active) {
      if (login.error) {
        children.push(el('div', { className: 'chat-sample-note' }, [`⚠ ${login.error}`]));
      }
      children.push(
        el('button', { className: 'btn btn-primary', onClick: () => void handleClaudeAuthLoginStart() }, ['앱에서 로그인']),
        el('button', { className: 'btn', onClick: () => void handleClaudeLogin() }, ['터미널 열기 (claude login)'])
      );
    }

    return [el('div', { className: 'alert' }, children)];
  }
  return [el('div', { className: 'alert' }, [el('span', {}, [`⚠ ${chat.error}`])])];
}

export async function cancelChatTurn(): Promise<void> {
  const turnId = state.sessionChat.turnId;
  if (!turnId) return;
  try {
    await cancelSessionTurn(turnId);
  } catch (err) {
    state.sessionChat.error = err instanceof Error ? err.message : String(err);
    notifyChange();
  }
}

// 목록 레코드가 없을 때(P2) 트랜스크립트의 첫 사용자 메시지에서 대신 제목을
// 유도한다. 원인은 두 가지다 — (1) 다른 창이 닫혀 registry에서 실행 중 항목이
// 사라진 경우, (2) 백엔드가 목록에 최근 30일/최대 100건으로 windowing하므로 그
// 범위 밖 세션을 직접 링크(URL)로 열었을 경우. 어느 쪽이든 jsonl은 여전히
// 정상적으로 읽히므로 화면 자체는 이 정보만으로도 설 수 있다.
function deriveTitleFromTranscript(transcript: SessionTranscript | null): string | null {
  if (!transcript) return null;
  const firstUser = transcript.messages.find((m) => m.kind === 'user');
  const text = firstUser?.text.trim();
  if (!text) return null;
  return text.length > 60 ? `${text.slice(0, 60)}…` : text;
}

// U-20: 세션 레코드가 없을 때(kind/startedAt/updatedAt이 전부 빈 값) 각 세그먼트를
// 무조건 '-'로 채우면 "· - · 시작 - · 갱신 -"처럼 '-'가 겹쳐 나온다. 빈 세그먼트는
// filter로 걷어내고 '·' 구분자는 남은 세그먼트 사이에만 join으로 생성한다.
function renderChatMetaRow(cwd: string, version: string, kind: string, startedAt: string, updatedAt: string): HTMLElement {
  const parts = [
    projectNameFromCwd(cwd),
    version ? `v${version}` : '버전 정보 없음',
    kind,
    startedAt !== '-' ? `시작 ${startedAt}` : '',
    updatedAt !== '-' ? `갱신 ${updatedAt}` : '',
  ].filter((part) => part !== '');

  const children: HTMLElement[] = [];
  parts.forEach((part, i) => {
    if (i > 0) children.push(el('span', {}, ['·']));
    children.push(el('span', {}, [part]));
  });
  return el('div', { className: 'chat-meta-row' }, children);
}

export function renderSessionDetailView(sessionId: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/sessions') }, ['← 세션목록']);
  const session = state.sessions.items.find((s) => asString(s.sessionId) === sessionId);
  const chat = state.sessionChat;
  const transcript = chat.sessionId === sessionId ? chat.transcript : null;

  // P2: 목록 레코드는 있으면 쓰는 부가 메타 정보로 격하한다 — 없어도(다른
  // 창이 닫혀 registry에서 사라져도) 트랜스크립트만으로 화면이 선다.
  const title = session ? sessionTitle(session) : (deriveTitleFromTranscript(transcript) ?? '(제목 없음)');
  const cwd = session ? asString(session.cwd) : (transcript?.cwd ?? '');
  const version = session ? asString(session.version) : '';
  const kind = session ? asString(session.kind) : '';
  const startedAt = session ? formatTimestamp(asNumber(session.startedAt)) : '-';
  const updatedAt = session ? formatTimestamp(asNumber(session.updatedAt)) : '-';

  // 긴 제목은 ellipsis로 잘리므로(styles.css .chat-header-row .chat-title)
  // title 속성으로 전체 텍스트를 hover 시 볼 수 있게 한다.
  const titleEl = el('h1', { className: 'chat-title' }, [title]);
  titleEl.title = title;

  const header = el('div', { className: 'chat-header' }, [
    back,
    el('div', { className: 'chat-header-row' }, [
      titleEl,
      ...(session ? [el('button', { className: 'btn chat-meta-btn', onClick: openMetaModal }, ['ⓘ 메타데이터'])] : []),
    ]),
    renderChatMetaRow(cwd, version, kind, startedAt, updatedAt),
  ]);

  const body: HTMLElement[] = [header];

  // RV-001: !chat.transcript는 두 가지 서로 다른 상황을 가리킬 수 있다 —
  // (a) 진짜 조회 실패(예: 존재하지 않는 세션을 직접 URL로 연 경우, turnId 없음)
  // (b) draft 승격 직후처럼 세션은 막 시작됐지만 `claude -p --session-id`가 아직
  // jsonl을 안 만들어 조회할 게 없을 뿐인 "정상" 상태(turnId 있음). (b)를 (a)로
  // 오판해 오류 블록을 그리면 그 아래 있던 스트리밍/입력창까지 통째로 가려진다
  // (분기 자체가 다른 return 경로였기 때문) — 그래서 turnId 유무로 두 상황을
  // 가른다.
  if (chat.sessionId !== sessionId || (chat.loading && !chat.transcript)) {
    body.push(el('div', { className: 'chat-thread' }, [el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['불러오는 중…'])])]));
  } else if (!chat.transcript && !chat.turnId) {
    body.push(
      el('div', { className: 'chat-thread' }, [
        el('div', { className: 'state-block' }, [
          el('div', { className: 'state-block-title' }, ['대화 기록을 불러오지 못했습니다']),
          el('div', { className: 'state-block-desc' }, [chat.error ?? '이 세션의 대화 기록 파일을 찾을 수 없습니다.']),
          el('button', { className: 'btn', onClick: () => void loadSessionTranscript(sessionId, true) }, ['다시 시도']),
        ]),
      ])
    );
  } else {
    // chat.transcript가 아직 null일 수 있다(위 (b) 상황) — 그 경우 빈 목록/false로
    // 취급하고, 아래 streaming/입력창은 그대로 정상 렌더한다.
    const thread: HTMLElement[] = [];
    if (chat.transcript?.truncated) {
      thread.push(el('div', { className: 'chat-sample-note' }, ['이전 대화 일부는 표시하지 않습니다.']));
    }
    for (const msg of chat.transcript?.messages ?? []) thread.push(chatMessage(msg.kind, msg.text));

    // 방금 보낸 메시지 — done 후 재조회 전까지 낙관적으로 미리 보여준다(§2 흐름).
    if (pendingUserText !== null) thread.push(chatMessage('user', pendingUserText));

    if (chat.turnId) {
      for (const toolLine of chat.streamingTools) thread.push(chatMessage('tool', toolLine));
      thread.push(chatMessage('assistant', chat.streamingText || '…'));
    }

    if (thread.length === 0) {
      thread.push(el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['표시할 대화가 없습니다'])]));
    }

    body.push(el('div', { className: 'chat-thread' }, thread));
    // claude.ai처럼 화면 진입/최초 로드 완료 시에도 항상 맨 아래(최신)부터
    // 보이게 한다(§2) — 스트리밍 중이 아니어도 매 렌더마다 호출한다. 사용자가
    // 위로 스크롤해 과거를 읽고 있으면 chatNearBottom 판정이 그대로 막아준다
    // (이 판단 로직 자체는 건드리지 않았다).
    scheduleChatAutoScroll();

    const bottomFixed: HTMLElement[] = [...renderChatErrorBlock(), ...renderClaudeAuthLoginBlock()];
    bottomFixed.push(renderChatInputArea(cwd, (text) => void sendChatMessage(sessionId, text)));
    body.push(el('div', { className: 'chat-bottom-fixed' }, bottomFixed));
  }

  if (metaModalOpen && session) {
    if (!metaModalEscHandler) {
      metaModalEscHandler = (e) => {
        if (e.key === 'Escape') closeMetaModal();
      };
      window.addEventListener('keydown', metaModalEscHandler);
    }
    body.push(renderMetaModal(session));
  } else {
    detachMetaModalEscHandler();
  }

  return el('div', { className: 'chat-page' }, body);
}

// "새 세션" draft 화면 — 프로젝트 카드 "새 세션" 버튼으로 진입한다(4-b). 아직
// session_id가 없으므로 조회할 트랜스크립트도, 메타데이터 모달도 없다. 사용자가
// 첫 메시지를 보내면 sendDraftMessage()가 실제 세션을 발급받아 이 화면을
// `#/sessions/<id>` 정상 세션 화면으로 승격시킨다.
export function renderSessionDraftView(projectPath: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/projects') }, ['← 프로젝트']);
  const chat = state.sessionChat;

  const header = el('div', { className: 'chat-header' }, [
    back,
    el('div', { className: 'chat-header-row' }, [el('h1', { className: 'chat-title' }, ['새 세션'])]),
    el('div', { className: 'chat-meta-row' }, [
      el('span', {}, [projectNameFromCwd(projectPath)]),
      el('span', {}, ['·']),
      el('span', {}, [projectPath]),
    ]),
  ]);

  const thread: HTMLElement[] = [];
  if (pendingUserText !== null) thread.push(chatMessage('user', pendingUserText));
  if (chat.turnId) {
    for (const toolLine of chat.streamingTools) thread.push(chatMessage('tool', toolLine));
    thread.push(chatMessage('assistant', chat.streamingText || '…'));
  }
  if (thread.length === 0) {
    thread.push(
      el('div', { className: 'state-block' }, [
        el('div', { className: 'state-block-title' }, ['새 대화를 시작하세요']),
        el('div', { className: 'state-block-desc' }, [`이 프로젝트에서 새 Claude 세션을 시작합니다 — 실행 폴더: ${projectPath}`]),
      ])
    );
  }
  scheduleChatAutoScroll();

  const bottomFixed: HTMLElement[] = [...renderChatErrorBlock(), ...renderClaudeAuthLoginBlock()];
  bottomFixed.push(renderChatInputArea(projectPath, (text) => void sendDraftMessage(projectPath, text), draftSending));

  return el('div', { className: 'chat-page' }, [
    header,
    el('div', { className: 'chat-thread' }, thread),
    el('div', { className: 'chat-bottom-fixed' }, bottomFixed),
  ]);
}

function renderMetaModal(session: ClaudeSessionRecord): HTMLElement {
  const modalBox = el('div', { className: 'modal-box' }, [
    el('div', { className: 'modal-header' }, [
      el('h2', { className: 'modal-title' }, ['세션 메타데이터']),
      el('button', { className: 'modal-close-btn', onClick: closeMetaModal }, ['✕']),
    ]),
    el('div', { className: 'modal-body' }, [
      el('div', { className: 'session-detail-fields' }, Object.entries(session).map(([key, value]) => renderFieldRow(key, value))),
    ]),
  ]);
  // 오버레이 생성(배경 클릭 시 닫힘, 드래그로 바깥에 나가도 안 닫힘)은 dom.ts의
  // createModalOverlay로 공용화했다.
  return createModalOverlay(modalBox, closeMetaModal);
}

// sessionId(세션 상세)와 projectPath(draft — 아직 세션이 없음) 양쪽에서
// 공유한다. 실제 전송 동작은 onSend 콜백으로 주입받는다(sendChatMessage 또는
// sendDraftMessage). extraSending은 draft 화면에서 draftSending(RV-002, 아직
// chat.turnId가 배정되지 않은 invoke 왕복 구간)까지 입력창에 반영하기 위한 것.
function renderChatInputArea(cwd: string, onSend: (text: string) => void, extraSending = false): HTMLElement {
  const chat = state.sessionChat;
  const sending = chat.turnId !== null || extraSending;
  // claude.ai 스타일 단순 입력창 — 버튼 없이 Enter로만 전송.
  // A2(리뷰 v0.2.5) — 값은 state.sessionChat.input 덕에 배경 재렌더에도
  // 살아남지만(이 패턴이 dom.ts의 boundField로 승격된 원본이다), 노드 자체가
  // 매 렌더 새로 만들어져 포커스가 <body>로, 캐럿이 문장 끝으로 날아갔다
  // (실측: `focused:true,start:7` → `focused:false,start:13`). boundField에
  // fieldId('chat-input')를 주면 재렌더 직전 포커스·캐럿을 기억했다가 새
  // 노드에 그대로 이어준다(dom.ts 참고) — 개별 폼 땜질 대신 이미 있는 공용
  // 헬퍼를 확장해 처리한다.
  const textarea = boundField(
    document.createElement('textarea'),
    () => state.sessionChat.input,
    (v) => {
      state.sessionChat.input = v; // 렌더를 트리거하지 않는다(키 입력마다 포커스가 끊기지 않도록)
    },
    'input',
    'chat-input'
  );
  textarea.className = 'settings-input chat-input-textarea';
  textarea.placeholder = sending ? '응답을 기다리는 중…' : '메시지를 입력하세요 (Enter 전송 / Shift+Enter 줄바꿈)';
  textarea.rows = 2;
  textarea.disabled = sending;
  textarea.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!sending) onSend(textarea.value);
    }
  });

  const row = el('div', { className: 'chat-input-row' }, [textarea]);
  if (sending) {
    row.appendChild(el('button', { className: 'btn', onClick: () => void cancelChatTurn() }, ['취소 ■']));
  }

  const children: HTMLElement[] = [row];
  if (!sending) {
    children.push(
      el('div', { className: 'chat-sample-note' }, [`⚠ 전송한 메시지는 실제로 파일을 변경하거나 명령을 실행할 수 있습니다 (실행 폴더: ${cwd || '(알 수 없음)'})`])
    );
  }
  if (!sending && lastTurnCanceled) {
    children.push(el('div', { className: 'chat-sample-note' }, ['직전에 중단한 요청이 다음 응답에 함께 반영될 수 있습니다']));
  }
  return el('div', {}, children);
}

// claude.ai 웹 스타일: 사용자 메시지는 말풍선(배경 있는 박스, 오른쪽 정렬)으로
// 감싸고, 어시스턴트 메시지는 말풍선 없이 일반 본문 텍스트(왼쪽 정렬, 전체 폭)로
// 그대로 렌더한다 — 감싸는 박스 자체를 두지 않는다(styles.css `.chat-message`).
function chatMessage(kind: ChatMessageKind, text: string): HTMLElement {
  if (kind === 'tool') {
    return el('div', { className: 'chat-tool-line' }, [text]);
  }
  const textEl = el('div', { className: 'chat-message-text' }, [text]);
  if (kind === 'user') {
    return el('div', { className: 'chat-message user' }, [el('div', { className: 'chat-bubble' }, [textEl])]);
  }
  return el('div', { className: 'chat-message assistant' }, [textEl]);
}

function renderFieldRow(key: string, value: unknown): HTMLElement {
  let display: string;
  if (Array.isArray(value)) {
    display = value.length > 0 ? value.map((v) => String(v)).join(', ') : '(빈 배열)';
  } else if (value === null || value === undefined) {
    display = '-';
  } else if (typeof value === 'object') {
    display = JSON.stringify(value);
  } else {
    display = String(value);
  }
  return el('div', { className: 'session-field-row' }, [el('div', { className: 'session-field-key' }, [key]), el('div', { className: 'session-field-value' }, [display])]);
}
