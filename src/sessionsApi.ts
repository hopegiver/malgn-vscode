// 이 앱에서 유일하게 하드코딩된 샘플이 아니라 실제 로컬 파일시스템/프로세스 값을
// 보여주는 기능 중 하나. Rust 커맨드 list_claude_sessions()의 정본 데이터 소스는
// registry(지금 실행 중인 프로세스) → jsonl 대화 이력이다(대화 전문 자체는 상세
// 화면에서 read_session_transcript()로 따로 읽는다). 각 행에는 반드시 sessionId·
// cwd·title·startedAt·updatedAt·running(지금 실행 중인지)이 있고, version·pid·
// name·kind·entrypoint·bridgeSessionId는 있을 수도 없을 수도 있다. 백엔드가 최근
// 30일/최대 100건으로 windowing하므로 그 범위 밖 세션은 이 목록에 나타나지 않는다.
//
// ~/.claude/sessions/ 를 앱 실행 내내 파일시스템 이벤트로 감시하다가(폴링 아님)
// 변경되면 "claude-sessions-changed" 이벤트를 쏜다 — 프론트는 그 신호를 받으면
// list_claude_sessions()를 다시 호출해 최신 목록을 반영한다.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { TerminalLaunchResult } from './integrationsApi';

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
  /** 이 세션에 지금 진행 중인 턴이 있으면 그 turnId(재진입 재부착용, M3). 없으면 null */
  readonly activeTurnId: string | null;
}

export interface SendStarted {
  readonly turnId: string;
}

// ---------------- 프로젝트 카드 "새 세션" — draft 상태에서 첫 메시지를 보낼 때만
// 실제로 spawn한다(src-tauri/src/session_chat.rs start_new_session_message).
// 백엔드가 요청 시점에 project_path를 재검증하므로, 존재하지 않거나 워크스페이스
// 밖의 경로는 여기서 에러로 돌아온다.

export interface NewSessionStarted {
  readonly sessionId: string;
  readonly turnId: string;
}

export async function startNewSessionMessage(projectPath: string, text: string): Promise<NewSessionStarted> {
  return invoke<NewSessionStarted>('start_new_session_message', { projectPath, text });
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
  /** hub 이슈 01m2wm4e9k822fk73yahrrnnce: `error`가 claude CLI 미인증으로 인한
   * 실패인지(백엔드 turn.rs `parse_result_event`가 판별) — true면 UI가 원문
   * 에러와 함께 "터미널에서 claude login" 안내를 보여준다. */
  readonly authError: boolean;
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

// 인증 실패로 턴이 끝났을 때(SessionChatDone.authError) "로그인" 버튼이 호출한다.
// gh/wrangler 연동과 같은 패턴 — 앱이 대신 로그인하지 않고 사용자가 직접 보는
// 터미널 창을 연다(src-tauri/src/cli_launcher.rs).
export async function openClaudeLoginTerminal(): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('open_claude_login_terminal');
}

export async function onSessionChatDelta(cb: (d: SessionChatDelta) => void): Promise<UnlistenFn> {
  return listen<SessionChatDelta>('session-chat-delta', (e) => cb(e.payload));
}

export async function onSessionChatDone(cb: (d: SessionChatDone) => void): Promise<UnlistenFn> {
  return listen<SessionChatDone>('session-chat-done', (e) => cb(e.payload));
}

// ---------------- 앱 안 claude CLI 로그인(src-tauri/src/claude_auth.rs) ----------------
// hub 이슈 01m2wm4e9k822fk73yahrrnnce의 후속 — 지금까지는 "터미널 열기"만
// 있었다(openClaudeLoginTerminal, 위). 이 그룹은 그 옆에 나란히 두는 새
// 경로다: 앱이 `claude auth login --claudeai`를 자식 프로세스로 띄우고,
// 진행 상황을 이벤트로 스트리밍한다. 완료 판정은 종료코드가 아니라
// `claude auth status --json`을 다시 물어본 결과다(ClaudeAuthStatus).

export interface ClaudeAuthStatus {
  readonly loggedIn: boolean;
  readonly authMethod: string | null;
  readonly email: string | null;
  readonly orgName: string | null;
  readonly subscriptionType: string | null;
}

/** 대시보드의 claude CLI 로그인 상태 위젯(views/home.ts loadClaudeAuthStatus)이
 * 부팅 시 미리 호출한다 — 조회 전용, 자격증명을 읽거나 쓰지 않는다. */
export async function checkClaudeAuthStatus(): Promise<ClaudeAuthStatus> {
  return invoke<ClaudeAuthStatus>('check_claude_auth_status');
}

// 이 앱 자체 계정(Google 로그인, sidebar.ts의 사이드바 "로그아웃")과 무관하다 —
// 이 함수는 claude CLI 자신의 Anthropic 계정 로그인만 끊는다
// (`claude auth logout` 위임 실행, claude_auth.rs logout_claude_auth). 완료
// 판정은 종료코드가 아니라 백엔드가 다시 물은 `claude auth status --json`
// 결과다 — 그 재확인 자체가 실패하면(확인 불가) 이 호출도 그대로 reject되고,
// 호출부는 이를 "로그아웃 성공"으로 단정하지 않는다.
export async function logoutClaudeAuth(): Promise<ClaudeAuthStatus> {
  return invoke<ClaudeAuthStatus>('logout_claude_auth');
}

/** 즉시 반환한다(spawn 확인까지만) — 실제 진행 상황은
 * onClaudeAuthLoginUrl/onClaudeAuthLoginFinished 이벤트로 온다. */
export async function startClaudeAuthLogin(): Promise<void> {
  return invoke<void>('start_claude_auth_login');
}

/** 진행 중인 로그인이 없으면 아무 일도 하지 않는다(정상 케이스, 백엔드가
 * no-op으로 처리). */
export async function cancelClaudeAuthLogin(): Promise<void> {
  return invoke<void>('cancel_claude_auth_login');
}

// 근본 수정(claude_auth.rs 상단 주석): 자식 stdin을 파이프로 바꾸고, 로그인이
// 진행 중인 동안 이 커맨드로 아무 때나 코드를 전달할 수 있게 했다 — claude가
// 실제로 코드를 요구하는지는 이 앱이 판단하지 않는다("감지해서 분기"가
// v0.2.11 사고의 원인이었다).
export async function submitClaudeAuthLoginCode(code: string): Promise<void> {
  return invoke<void>('submit_claude_auth_login_code', { code });
}

export interface ClaudeAuthLoginUrlEvent {
  readonly url: string;
}

export interface ClaudeAuthLoginFinishedEvent {
  readonly ok: boolean;
  readonly canceled: boolean;
  readonly error: string | null;
  readonly status: ClaudeAuthStatus | null;
}

/** 자식 stdout 원문 청크(줄 경계 없음, claude_auth.rs `pump_stdout`이 읽는
 * 그대로) — 사용자가 코드 입력창 옆에서 claude가 지금 무엇을 묻는지 원문으로
 * 볼 수 있게 하는 용도다. 이 이벤트를 보고 입력창 노출 여부를 정하지 않는다
 * (입력창은 로그인이 진행 중인 동안 항상 있다). */
export interface ClaudeAuthLoginOutputEvent {
  readonly text: string;
}

export async function onClaudeAuthLoginUrl(cb: (d: ClaudeAuthLoginUrlEvent) => void): Promise<UnlistenFn> {
  return listen<ClaudeAuthLoginUrlEvent>('claude-auth-login-url', (e) => cb(e.payload));
}

export async function onClaudeAuthLoginOutput(cb: (d: ClaudeAuthLoginOutputEvent) => void): Promise<UnlistenFn> {
  return listen<ClaudeAuthLoginOutputEvent>('claude-auth-login-output', (e) => cb(e.payload));
}

export async function onClaudeAuthLoginFinished(cb: (d: ClaudeAuthLoginFinishedEvent) => void): Promise<UnlistenFn> {
  return listen<ClaudeAuthLoginFinishedEvent>('claude-auth-login-finished', (e) => cb(e.payload));
}
