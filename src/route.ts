// 해시 기반 클라이언트 사이드 라우팅. 서버 없이 window.location.hash만으로 화면을 전환한다.
//
// 라우트 지도:
//   '' / '#/'            -> home (로그인 직후 첫 화면 — 전체 요약 대시보드)
//   '#/projects'         -> projects-list (로컬 프로젝트 목록, 옛 "대시보드")
//   '#/project/<path>'   -> projects-detail
//   '#/catalog'          -> catalog (플러그인 단위 카드 목록 — 탭 없음)
//   '#/dev-tools'        -> dev-tools (로컬 CLI 도구 버전 — 실제 조회)
//   '#/settings[/<tab>]' -> settings
//   '#/usage'            -> usage
//   '#/sessions'         -> sessions-list (실제 ~/.claude/sessions/*.json)
//   '#/sessions/<id>'    -> sessions-detail
//   '#/tasks'            -> tasks-list (자율업무 목록 탭, 목업)
//   '#/tasks/board'      -> tasks-board (자율업무 진행상황판 탭)
//   '#/tasks/item/<id>'  -> tasks-detail
import type { SettingsTab } from './state';

export type Route =
  | { readonly kind: 'home' }
  | { readonly kind: 'projects-list' }
  | { readonly kind: 'projects-detail'; readonly path: string }
  | { readonly kind: 'catalog' }
  | { readonly kind: 'dev-tools' }
  | { readonly kind: 'settings'; readonly tab: SettingsTab }
  | { readonly kind: 'usage' }
  | { readonly kind: 'sessions-list' }
  | { readonly kind: 'sessions-detail'; readonly sessionId: string }
  | { readonly kind: 'tasks-list' }
  | { readonly kind: 'tasks-board' }
  | { readonly kind: 'tasks-detail'; readonly taskId: string };

const SETTINGS_TABS: readonly SettingsTab[] = ['otel', 'github', 'cloudflare', 'jira', 'marketplace'];

export function parseRoute(): Route {
  const hash = window.location.hash;

  if (hash.startsWith('#/project/')) {
    try {
      return { kind: 'projects-detail', path: decodeURIComponent(hash.slice('#/project/'.length)) };
    } catch {
      return { kind: 'projects-list' };
    }
  }
  if (hash.startsWith('#/projects')) {
    return { kind: 'projects-list' };
  }

  if (hash.startsWith('#/catalog')) {
    return { kind: 'catalog' };
  }

  if (hash.startsWith('#/dev-tools')) {
    return { kind: 'dev-tools' };
  }

  if (hash.startsWith('#/settings')) {
    const seg = hash.split('/')[2] as SettingsTab | undefined;
    const tab = seg && SETTINGS_TABS.includes(seg) ? seg : 'otel';
    return { kind: 'settings', tab };
  }

  if (hash.startsWith('#/usage')) {
    return { kind: 'usage' };
  }

  if (hash.startsWith('#/sessions/')) {
    try {
      return { kind: 'sessions-detail', sessionId: decodeURIComponent(hash.slice('#/sessions/'.length)) };
    } catch {
      return { kind: 'sessions-list' };
    }
  }
  if (hash.startsWith('#/sessions')) {
    return { kind: 'sessions-list' };
  }

  if (hash.startsWith('#/tasks/item/')) {
    try {
      return { kind: 'tasks-detail', taskId: decodeURIComponent(hash.slice('#/tasks/item/'.length)) };
    } catch {
      return { kind: 'tasks-list' };
    }
  }
  if (hash.startsWith('#/tasks/board')) {
    return { kind: 'tasks-board' };
  }
  if (hash.startsWith('#/tasks')) {
    return { kind: 'tasks-list' };
  }

  return { kind: 'home' };
}

export function navigate(hash: string): void {
  window.location.hash = hash;
}
