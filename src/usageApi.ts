// "사용량 통계" 화면의 일별 사용량 실제 데이터 소스. Rust 커맨드 get_daily_usage()가
// ~/.claude/projects/**/*.jsonl 전체(최근 30일)에서 "type":"assistant" 줄의
// message.usage(토큰 수)와 timestamp만 읽어 날짜별로 합산한다 — 대화 내용은
// 읽지 않는다. 실시간 파일감시는 안 한다 — 이 컴퓨터처럼 여러 프로젝트에서 계속
// 쓰고 있으면 재계산(수 초 소요)이 끝나기도 전에 다음 이벤트가 쌓여 오히려 계속
// 느려졌다. 대신 "사용량 통계" 메뉴를 클릭하는 시점에만 새로 불러온다.
import { invoke } from '@tauri-apps/api/core';

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
