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
  /**
   * 단가표(pricing.rs)에 없는 모델의 토큰 합 — 0보다 크면 costUsd가 실제
   * 비용보다 낮게 잡혔을 수 있다는 뜻이다(그 모델의 비용은 미산정=0으로 처리됨).
   */
  readonly unpricedTokens: number;
}

export async function fetchDailyDetail(date: string): Promise<DailyDetailReport> {
  return invoke<DailyDetailReport>('get_daily_detail', { date });
}

// 세션 상세 화면의 "에이전트 실행 이력" 섹션 전용 — 날짜가 아니라 세션 하나만
// 골라 그 세션 전체 기간(날짜 필터 없음)의 agentType/toolName 집계를 받는다.
// `get_daily_detail`과 백엔드 집계 로직을 공유하되(usage_stats::detail::
// aggregate_session_agents_and_tools) 날짜로 자르지 않는다는 점만 다르다.
// 세션에 대응하는 jsonl을 못 찾으면(아직 메시지가 없는 draft 세션 등) `null`
// — 호출 자체가 실패한 에러와 "자료 없음"을 프론트가 구분해야 한다.
export async function fetchSessionDetail(sessionId: string): Promise<SessionDetail | null> {
  return invoke<SessionDetail | null>('get_session_detail', { sessionId });
}
