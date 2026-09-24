// "사용량 통계" 확장 화면의 30일 요약 데이터 소스. Rust 커맨드 get_usage_summary()가
// ~/.claude/projects/**/*.jsonl 최근 30일치를 "한 번" 스캔해 모델별/프로젝트별/
// 툴별 비중과 예상 비용을 계산한다 — usageApi.ts의 fetchDailyUsage()(날짜별
// 토큰 합만)와 달리 모델/프로젝트/비용까지 포함한다. 최고 사용일·캐시 절대량·
// 입출력 비율처럼 기존 DailyUsage 30일치만으로 계산 가능한 지표는 여기 없다 —
// usageApi.ts의 DailyUsage[]에서 프론트가 직접 계산한다.
import { invoke } from '@tauri-apps/api/core';
import type { DailyUsage } from './usageApi';

export interface ModelUsageSummary {
  /**
   * 정규 모델 ID(날짜 접미사 제거, 예: "claude-opus-5-5"). family(opus/
   * sonnet/haiku) 합산이 아니라 모델 버전 단위로 나온다 — 단가표에 없는
   * 모델은 원본에서 날짜 접미사만 뗀 문자열 그대로.
   */
  readonly modelId: string;
  /** 사람이 읽을 표시명(예: "Opus 5.5", "Sonnet 5"). 단가표에 없으면 modelId와 동일. */
  readonly displayName: string;
  readonly tokens: number;
  readonly costUsd: number;
  /**
   * false면 이 모델은 단가표(pricing.rs)에 없어 costUsd=0으로 비용을
   * 계산하지 않았다 — UI가 이 값이 false인 행의 tokens를 "비용 미산정"으로
   * 정직하게 표시해야 한다(예전처럼 sonnet 단가로 조용히 대체하지 않는다).
   */
  readonly pricingMatched: boolean;
}

export interface ProjectUsageSummary {
  readonly projectKey: string;
  /** ~/workspace/<이름> 관례를 따르면 <이름>, 벗어나면 projectKey 원문(선행 '-'만 제거). */
  readonly displayName: string;
  readonly tokens: number;
  readonly costUsd: number;
}

export interface ToolUsageSummary {
  readonly toolName: string;
  readonly count: number;
}

export interface UsageSummaryReport {
  /** 집계 룩백 일수(현재 30). */
  readonly days: number;
  readonly totalTokens: number;
  readonly totalCostUsd: number;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly cacheCreationTokens: number;
  readonly cacheReadTokens: number;
  /**
   * 단가표에 없는 모델의 토큰 합(=`models`에서 `pricingMatched=false`인
   * 행들의 tokens 합과 항상 같다). 0이면 30일 비용 전액이 실단가 기준이다.
   */
  readonly unpricedTokens: number;
  /** 모델 ID 단위(계열 합산 아님), tokens desc 정렬되어 옴. */
  readonly models: readonly ModelUsageSummary[];
  /** 토큰 기준 상위 5개, tokens desc 정렬되어 옴. */
  readonly projects: readonly ProjectUsageSummary[];
  /** projects 상위 5개 밖으로 빠진 나머지 프로젝트들의 합(투명성 확인용). */
  readonly otherProjectsTokens: number;
  readonly otherProjectsCostUsd: number;
  /** count desc 정렬되어 온 상위 5개. */
  readonly tools: readonly ToolUsageSummary[];
}

export async function fetchUsageSummary(): Promise<UsageSummaryReport> {
  return invoke<UsageSummaryReport>('get_usage_summary');
}

/**
 * `projects` Top5 카드에서 프로젝트 하나를 선택했을 때만 호출한다(파고들기 뷰).
 * 반환 타입은 fetchDailyUsage()와 같은 DailyUsage[]라 같은 차트 렌더링 코드를
 * 재사용할 수 있다.
 */
export async function fetchProjectDailyTrend(projectKey: string): Promise<DailyUsage[]> {
  return invoke<DailyUsage[]>('get_project_daily_trend', { projectKey });
}
