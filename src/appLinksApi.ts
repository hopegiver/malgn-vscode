// 앱링크 — 사내/외부 웹 앱 바로가기를 로컬 파일(~/.claude/malgn-agent-apps.json)에
// 저장하고, 사이드바에서 OS 기본 브라우저로 연다(docs/design-app-links.md §3-4).
//
// 검증 정본은 Rust 한 곳(app_links/store.rs)이다 — 프론트는 limits를 하드코딩하지
// 않고 이 응답의 `limits` 필드에서만 읽어 입력 힌트에 쓴다. 열기(app_links_open)는
// URL이 아니라 id만 넘긴다 — 열리는 주소는 항상 저장·검증을 통과한 목록 안의
// 값이어야 한다(§8 보안 검토).
import { invoke } from '@tauri-apps/api/core';

export interface AppLink {
  readonly id: string;
  readonly name: string;
  readonly url: string;
  readonly enabled: boolean;
}

export interface AppLinksLimits {
  readonly maxLinks: number;
  readonly maxNameLength: number;
  readonly maxUrlLength: number;
  readonly allowedSchemes: readonly string[];
}

export interface AppLinksStatus {
  readonly ok: boolean;
  readonly error: string | null;
  readonly filePath: string;
  readonly fileExists: boolean;
  readonly links: readonly AppLink[];
  readonly warnings: readonly string[];
  readonly limits: AppLinksLimits;
}

/** 항상 성공한다(손상 파일도 ok:false를 담은 정상 응답으로 내려온다). */
export async function fetchAppLinks(): Promise<AppLinksStatus> {
  return invoke<AppLinksStatus>('app_links_get');
}

/** 전체 목록 치환 저장 — 추가·수정·삭제·토글·순서변경 전부 이 하나로 처리한다.
 * 검증 실패 시 reject된다 — 에러 메시지는 백엔드 원문을 그대로 사용자에게 보여준다. */
export async function saveAppLinks(links: readonly AppLink[]): Promise<AppLinksStatus> {
  return invoke<AppLinksStatus>('app_links_save', { links });
}

/** URL이 아니라 id만 넘긴다(§6 보안 — 저장·검증을 통과한 값만 열리도록). */
export async function openAppLink(id: string): Promise<void> {
  return invoke<void>('app_links_open', { id });
}
