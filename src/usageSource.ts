// 사용량이 비어 있을 때 "왜 비었는가"를 가르는 순수 판정 + 데이터 소스(R14 P2-6).
// Rust 커맨드 get_usage_source_status()가 사실 신호 3개만 주고(src-tauri/src/
// usage_stats/source.rs에 신호별 한계), 어떤 원인으로 보여 줄지는 여기서 정한다.
// 신호가 서로 모순되거나 조회가 실패하면 단정하지 않고 'unknown'으로 둔다 —
// "미설치"는 세 신호가 모두 "없음"일 때만 낸다(후보 경로 밖에 설치된 Claude를
// 미설치로 오판하지 않기 위해).
import { invoke } from '@tauri-apps/api/core';

export interface UsageSourceStatus {
  /** 개발 도구 화면과 같은 탐지(후보 경로 + PATH 폴백)로 claude 실행 파일을 찾았는가. */
  readonly claudeBinaryFound: boolean;
  /** ~/.claude 폴더가 있는가(첫 실행·로그인 때 생김, 삭제 후에도 남을 수 있는 약한 신호). */
  readonly claudeDirExists: boolean;
  /** ~/.claude/projects 아래에 프로젝트 폴더가 하나라도 있는가(기간 무관 — 과거 기록 유무). */
  readonly projectsDirHasEntries: boolean;
}

/** 홈 디렉터리를 못 찾으면 백엔드가 null을 준다 — 판정 불가로 다룬다. */
export async function fetchUsageSourceStatus(): Promise<UsageSourceStatus | null> {
  return invoke<UsageSourceStatus | null>('get_usage_source_status');
}

export type UsageEmptyCause =
  /** 실행 파일도 ~/.claude도 없다 — 미설치로 보인다(후보 경로 밖 설치는 구분 못 함). */
  | { readonly kind: 'not-installed' }
  /** 실행 파일은 있고 projects 기록이 한 번도 없다 — 첫 세션 전. */
  | { readonly kind: 'installed-no-history' }
  /** 실행 파일이 있고 과거 기록은 있지만 최근 30일엔 없다. */
  | { readonly kind: 'installed-stale' }
  /** 신호가 모순되거나 조회 불가 — 원인을 단정하지 않는다. */
  | { readonly kind: 'unknown'; readonly reason: 'status-unavailable' | 'binary-not-found-but-traces' };

export function classifyUsageEmpty(s: UsageSourceStatus | null): UsageEmptyCause {
  if (!s) return { kind: 'unknown', reason: 'status-unavailable' };
  if (s.claudeBinaryFound) {
    return s.projectsDirHasEntries ? { kind: 'installed-stale' } : { kind: 'installed-no-history' };
  }
  if (!s.claudeDirExists && !s.projectsDirHasEntries) return { kind: 'not-installed' };
  return { kind: 'unknown', reason: 'binary-not-found-but-traces' };
}
