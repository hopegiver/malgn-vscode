// "사용량 통계" 화면의 일별 사용량 실제 데이터 소스. Rust 커맨드 get_daily_usage()가
// ~/.claude/projects/**/*.jsonl 전체(최근 30일)에서 "type":"assistant" 줄의
// message.usage(토큰 수)와 timestamp만 읽어 날짜별로 합산한다 — 대화 내용은
// 읽지 않는다. ~/.claude/projects/를 앱 실행 내내 재귀 감시하다가 변경되면
// "claude-usage-changed" 이벤트를 쏜다(폴링 아님, 디바운스 적용).
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface DailyUsage {
  readonly date: string;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly cacheCreationTokens: number;
  readonly cacheReadTokens: number;
}

export async function fetchDailyUsage(): Promise<DailyUsage[]> {
  return invoke<DailyUsage[]>('get_daily_usage');
}

export async function onUsageChanged(callback: () => void): Promise<void> {
  await listen('claude-usage-changed', () => callback());
}
