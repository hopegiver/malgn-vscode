// 이 앱에서 유일하게 하드코딩된 샘플이 아니라 실제 로컬 파일시스템 값을 보여주는
// 기능 중 하나. Rust 커맨드 list_claude_sessions()가 ~/.claude/sessions/*.json
// 메타데이터만 읽어 반환한다(대화 전문 .jsonl은 Rust 쪽에서부터 절대 건드리지
// 않는다 — 경로가 Rust 코드에 고정돼 있어 여기서 다른 경로를 지정할 방법이 없다).
//
// ~/.claude/sessions/ 를 앱 실행 내내 파일시스템 이벤트로 감시하다가(폴링 아님)
// 변경되면 "claude-sessions-changed" 이벤트를 쏜다 — 프론트는 그 신호를 받으면
// list_claude_sessions()를 다시 호출해 최신 목록을 반영한다.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface ClaudeSessionRecord {
  readonly [key: string]: unknown;
}

export async function fetchClaudeSessions(): Promise<ClaudeSessionRecord[]> {
  return invoke<ClaudeSessionRecord[]>('list_claude_sessions');
}

export async function onSessionsChanged(callback: () => void): Promise<void> {
  await listen('claude-sessions-changed', () => callback());
}

// ---------------- 세션 상세 = 실제 대화 + 이어쓰기 ----------------
// 설계: docs/design/session-chat.md §4-3. Rust가 jsonl 전문을 읽어 접은 결과를
// 그대로 쓴다(이 파일 상단 주석과 달리 대화 전문을 더 이상 건너뛰지 않는다).

export type ChatMessageKind = 'user' | 'assistant' | 'tool';

export interface ChatMessage {
  readonly kind: ChatMessageKind;
  readonly text: string;
  /** kind === 'tool' 일 때 접힌 도구 호출 개수(1 이상). 그 외 0 */
  readonly toolCount: number;
  /** jsonl의 ISO8601 timestamp. 없으면 null */
  readonly at: string | null;
}

export interface SessionTranscript {
  readonly sessionId: string;
  readonly cwd: string;
  readonly transcriptPath: string;
  readonly messages: readonly ChatMessage[];
  /** 400개 상한으로 앞부분이 잘렸는가 */
  readonly truncated: boolean;
  /** 이 세션이 지금 다른 창에서 실행 중인가(경고 배지) */
  readonly live: boolean;
  /** 이 세션에 지금 진행 중인 턴이 있으면 그 turnId(재진입 재부착용, M3). 없으면 null */
  readonly activeTurnId: string | null;
}

export interface SendStarted {
  readonly turnId: string;
}

export interface SessionChatDelta {
  readonly sessionId: string;
  readonly turnId: string;
  readonly kind: 'text' | 'tool';
  readonly text: string;
}

export interface SessionChatDone {
  readonly sessionId: string;
  readonly turnId: string;
  readonly ok: boolean;
  readonly canceled: boolean;
  readonly error: string | null;
}

export async function fetchSessionTranscript(sessionId: string): Promise<SessionTranscript> {
  return invoke<SessionTranscript>('read_session_transcript', { sessionId });
}

export async function sendSessionMessage(sessionId: string, text: string): Promise<SendStarted> {
  return invoke<SendStarted>('send_session_message', { sessionId, text });
}

export async function cancelSessionTurn(turnId: string): Promise<void> {
  return invoke<void>('cancel_session_turn', { turnId });
}

export async function onSessionChatDelta(cb: (d: SessionChatDelta) => void): Promise<UnlistenFn> {
  return listen<SessionChatDelta>('session-chat-delta', (e) => cb(e.payload));
}

export async function onSessionChatDone(cb: (d: SessionChatDone) => void): Promise<UnlistenFn> {
  return listen<SessionChatDone>('session-chat-done', (e) => cb(e.payload));
}
