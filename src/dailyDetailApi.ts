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

// 서브에이전트 위임 1건(사용자가 "에이전트들이 실제로 어떤 작업을 했는지
// 목록"을 원해서 추가됨) — AgentUsage(agentType별 턴/토큰/비용 합계)와 달리
// 위임 호출 1건당 1개 항목이다. 메인 에이전트 자신의 턴은 포함되지 않는다
// (description 같은 짧은 라벨이 원래 없다).
export interface AgentWorkItem {
  readonly description: string;
  readonly agentType: string;
  readonly startedAt: string; // RFC3339 — 위임 tool_use가 기록된 시각
  // 매칭되는 서브에이전트 트랜스크립트를 찾았을 때만 채워진다. 못 찾으면
  // 억지로 만들지 않고 null("정직하게 계산 가능한 지표만" 원칙) — 세
  // 필드가 함께 null/non-null이다(같은 보강 단계에서 함께 채워짐).
  readonly durationSeconds: number | null;
  readonly totalTokens: number | null;
  readonly costUsd: number | null;
}

export interface SessionDetail {
  readonly sessionId: string;
  readonly projectKey: string;
  readonly title: string;
  readonly totalTokens: number;
  readonly costUsd: number;
  readonly agents: readonly AgentUsage[]; // totalTokens desc 정렬되어 옴
  readonly tools: readonly ToolUsage[]; // count desc 정렬되어 옴
  // startedAt desc(최신 위임 먼저) 정렬되어 옴. get_session_detail(세션 상세
  // "에이전트 실행 이력")에서만 채워진다 — get_daily_detail(날짜별 상세)의
  // sessions 배열에서는 항상 빈 배열이다(그 화면은 이 필드를 쓰지 않는다).
  readonly workItems: readonly AgentWorkItem[];
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
