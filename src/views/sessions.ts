// 세션목록 — 이 앱에서 유일하게 실제 로컬 파일(~/.claude/sessions/*.json)을 읽어
// 보여주는 화면 중 하나. Rust 커맨드 list_claude_sessions()가 반환한 값을 그대로
// 쓴다. 상세 화면은 첫 사용자 메시지(title, 실제 데이터)만 실제로 갖고 있고 —
// 대화 전문(jsonl)은 읽지 않는다는 기존 결정을 유지한다 — 나머지 대화 흐름은
// "그 세션이 어떤 느낌이었는지" 보여주는 샘플 말풍선으로 채운다(명시적으로 표시).
import { el, liveIndicator } from '../dom';
import { state, notifyChange } from '../state';
import { fetchClaudeSessions } from '../sessionsApi';
import type { ClaudeSessionRecord } from '../sessionsApi';
import { navigate } from '../route';

export function asString(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

export function asNumber(v: unknown): number | null {
  return typeof v === 'number' ? v : null;
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
    state.sessions.error = err instanceof Error ? err.message : '세션 목록을 불러오지 못했습니다. Tauri 앱(pnpm tauri dev)에서 실행 중인지 확인하세요.';
  } finally {
    state.sessions.loading = false;
    notifyChange();
  }
}

export function sortedSessions(): ClaudeSessionRecord[] {
  return [...state.sessions.items].sort((a, b) => (asNumber(b.updatedAt) ?? 0) - (asNumber(a.updatedAt) ?? 0));
}

export function renderSessionsListView(): HTMLElement {
  const header = el('div', { className: 'page-header' }, [
    el('div', {}, [
      el('h1', { className: 'page-title' }, ['세션목록']),
      el('div', { className: 'page-subtitle-row' }, [
        el('div', { className: 'page-subtitle' }, ['~/.claude/sessions/*.json — 실제 로컬 파일을 읽습니다']),
        ...(state.sessions.live ? [liveIndicator()] : []),
      ]),
    ]),
    el('button', { className: 'btn', onClick: () => void loadSessions(), disabled: state.sessions.loading }, [
      state.sessions.loading ? '불러오는 중…' : '↻ 새로고침',
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
        el('div', { className: 'state-block-desc' }, ['~/.claude/sessions/ 아래 *.json 파일을 찾지 못했습니다.']),
      ])
    );
  } else {
    body.push(el('div', { className: 'session-list' }, sortedSessions().map(renderSessionRow)));
  }

  return el('div', {}, [header, ...body]);
}

function renderSessionRow(session: ClaudeSessionRecord): HTMLElement {
  const sessionId = asString(session.sessionId);
  const title = sessionTitle(session);
  const cwd = asString(session.cwd);
  const name = asString(session.name);
  const version = asString(session.version);
  const kind = asString(session.kind);
  const entrypoint = asString(session.entrypoint);

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
        el('span', { className: 'badge badge-active' }, [kind || entrypoint || '-']),
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

// ---------------- 상세 — Claude.ai/ChatGPT 웹 화면 스타일 채팅 인터페이스 ----------------
// 첫 사용자 메시지(title)만 실제 데이터다. 나머지 턴은 "이 세션이 어떤 느낌이었는지"
// 보여주는 샘플이며, 화면에 그 사실을 명시한다 — 실제 대화 전문(jsonl)은 여전히
// 읽지 않는다(기존 범위 결정 유지).
const SAMPLE_FOLLOWUP_TURNS: readonly { readonly role: 'assistant' | 'user'; readonly text: string }[] = [
  { role: 'assistant', text: '네, 확인했습니다. 관련 파일들을 먼저 살펴본 뒤 진행 계획을 정리하겠습니다.' },
  { role: 'user', text: '네, 진행해 주세요.' },
  { role: 'assistant', text: '작업을 마쳤습니다. 변경한 파일과 확인한 내용을 정리해서 알려드릴게요.' },
];

export function renderSessionDetailView(sessionId: string): HTMLElement {
  const back = el('a', { className: 'back-link', onClick: () => navigate('#/sessions') }, ['← 세션목록']);
  const session = state.sessions.items.find((s) => asString(s.sessionId) === sessionId);

  if (!session) {
    return el('div', {}, [back, el('div', { className: 'state-block' }, [el('div', { className: 'state-block-title' }, ['세션을 찾을 수 없습니다'])])]);
  }

  const title = sessionTitle(session);
  const cwd = asString(session.cwd);
  const version = asString(session.version);
  const kind = asString(session.kind);
  const startedAt = formatTimestamp(asNumber(session.startedAt));
  const updatedAt = formatTimestamp(asNumber(session.updatedAt));

  const header = el('div', { className: 'chat-header' }, [
    back,
    el('h1', { className: 'chat-title' }, [title]),
    el('div', { className: 'chat-meta-row' }, [
      el('span', {}, [projectNameFromCwd(cwd)]),
      el('span', {}, ['·']),
      el('span', {}, [version ? `v${version}` : '버전 정보 없음']),
      el('span', {}, ['·']),
      el('span', {}, [kind || '-']),
      el('span', {}, ['·']),
      el('span', {}, [`시작 ${startedAt}`]),
      el('span', {}, ['·']),
      el('span', {}, [`갱신 ${updatedAt}`]),
    ]),
  ]);

  const thread: HTMLElement[] = [chatMessage('user', title)];
  for (const turn of SAMPLE_FOLLOWUP_TURNS) thread.push(chatMessage(turn.role, turn.text));
  thread.push(el('div', { className: 'chat-sample-note' }, ['첫 메시지만 실제 기록이고, 이후 대화는 세션 분위기를 보여주기 위한 샘플입니다 — 대화 전문은 표시하지 않습니다.']));

  const metaPanel = el('div', { className: 'chat-meta-panel' }, [
    el('div', { className: 'chat-meta-panel-title' }, ['세션 메타데이터 (실제 데이터)']),
    el('div', { className: 'session-detail-fields' }, Object.entries(session).map(([key, value]) => renderFieldRow(key, value))),
  ]);

  return el('div', { className: 'chat-page' }, [header, el('div', { className: 'chat-thread' }, thread), metaPanel]);
}

function chatMessage(role: 'user' | 'assistant', text: string): HTMLElement {
  return el('div', { className: `chat-message ${role}` }, [
    el('div', { className: 'chat-avatar' }, [role === 'user' ? '나' : 'AI']),
    el('div', { className: 'chat-bubble' }, [el('div', { className: 'chat-message-text' }, [text])]),
  ]);
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
