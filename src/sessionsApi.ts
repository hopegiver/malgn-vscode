// 이 앱에서 유일하게 하드코딩된 샘플이 아니라 실제 로컬 파일시스템 값을 보여주는
// 기능 중 하나. Rust 커맨드 list_claude_sessions()가 ~/.claude/sessions/*.json
// 메타데이터만 읽어 반환한다(대화 전문 .jsonl은 Rust 쪽에서부터 절대 건드리지
// 않는다 — 경로가 Rust 코드에 고정돼 있어 여기서 다른 경로를 지정할 방법이 없다).
//
// ~/.claude/sessions/ 를 앱 실행 내내 파일시스템 이벤트로 감시하다가(폴링 아님)
// 변경되면 "claude-sessions-changed" 이벤트를 쏜다 — 프론트는 그 신호를 받으면
// list_claude_sessions()를 다시 호출해 최신 목록을 반영한다.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface ClaudeSessionRecord {
  readonly [key: string]: unknown;
}

export async function fetchClaudeSessions(): Promise<ClaudeSessionRecord[]> {
  return invoke<ClaudeSessionRecord[]>('list_claude_sessions');
}

export async function onSessionsChanged(callback: () => void): Promise<void> {
  await listen('claude-sessions-changed', () => callback());
}
