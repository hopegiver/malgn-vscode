// "일별 사용량" 날짜 행을 클릭했을 때 펼치는 상세 데이터 소스 — 해당 날짜 하루만
// 대상으로 ~/.claude/projects/**/*.jsonl을 세션/서브에이전트/툴 단위로 집계해
// 반환한다(usageApi.ts의 fetchDailyUsage()와 달리 60일 전체 스캔이 아니다).
import { invoke } from '@tauri-apps/api/core';

export interface ToolUsage {
  readonly toolName: string;
  readonly count: number;
}

export interface AgentUsage {
  readonly agentType: string; // 세션 자신의 턴은 "main", 그 외는 서브에이전트 타입명
  readonly turns: number;
  readonly totalTokens: number;
  readonly costUsd: number;
}

export interface SessionDetail {
  readonly sessionId: string;
  readonly projectKey: string;
  readonly title: string;
  readonly totalTokens: number;
  readonly costUsd: number;
  readonly agents: readonly AgentUsage[]; // totalTokens desc 정렬되어 옴
  readonly tools: readonly ToolUsage[]; // count desc 정렬되어 옴
}

export interface DailyDetailReport {
  readonly date: string;
  readonly sessions: readonly SessionDetail[]; // totalTokens desc 정렬되어 옴
  readonly totalTokens: number;
  readonly costUsd: number;
}

export async function fetchDailyDetail(date: string): Promise<DailyDetailReport> {
  return invoke<DailyDetailReport>('get_daily_detail', { date });
}
