// 해시 기반 클라이언트 사이드 라우팅. 서버 없이 window.location.hash만으로 화면을 전환한다.
//
// 라우트 지도:
//   '' / '#/'            -> home (로그인 직후 첫 화면 — 전체 요약 대시보드)
//   '#/projects'         -> projects-list (로컬 프로젝트 목록, 옛 "대시보드")
//   '#/project/<path>'   -> projects-detail
//   '#/catalog[/<tab>]'  -> catalog (탭: plugins 기본값 | global)
//   '#/settings[/<tab>]' -> settings (탭 devtools = 로컬 CLI 도구 버전 — 실제 조회)
//   '#/usage[/<tab>]'    -> usage (탭: daily 기본값 | projects | models)
//   '#/sessions'         -> sessions-list (실제 ~/.claude/sessions/*.json)
//   '#/sessions/new/<projectPath>' -> sessions-draft (프로젝트 카드 "새 세션" — session_id 배정 전)
//   '#/sessions/<id>'    -> sessions-detail
//   '#/tasks'            -> tasks-list (자율업무 목록 탭, 목업)
//   '#/tasks/board'      -> tasks-board (자율업무 진행상황판 탭)
//   '#/tasks/item/<id>'  -> tasks-detail
import type { SettingsTab, CatalogTab, UsageTab } from './state';

// 세션 상세의 "← 세션" 뒤로가기 목적지. 세션 상세로 들어오는 진입점이
// 세션 목록/홈/프로젝트 상세 3군데라 뒤로가기도 그 진입점으로 되돌아가야 한다.
// 해시 쿼리스트링(`?back=home` | `?back=project:<encodedPath>`)으로 인코딩하며,
// 생략 시 기본값은 'sessions-list'(기존 동작과 동일, 하위호환).
export type SessionBackTarget = { readonly kind: 'sessions-list' } | { readonly kind: 'home' } | { readonly kind: 'projects-detail'; readonly path: string };

export type Route =
  | { readonly kind: 'home' }
  | { readonly kind: 'projects-list' }
  | { readonly kind: 'projects-detail'; readonly path: string }
  | { readonly kind: 'catalog'; readonly tab: CatalogTab }
  | { readonly kind: 'settings'; readonly tab: SettingsTab }
  | { readonly kind: 'usage'; readonly tab: UsageTab }
  | { readonly kind: 'sessions-list' }
  | { readonly kind: 'sessions-draft'; readonly projectPath: string }
  | { readonly kind: 'sessions-detail'; readonly sessionId: string; readonly backTo: SessionBackTarget }
  | { readonly kind: 'tasks-list' }
  | { readonly kind: 'tasks-board' }
  | { readonly kind: 'tasks-detail'; readonly taskId: string };

const SETTINGS_TABS: readonly SettingsTab[] = ['otel', 'github', 'cloudflare', 'marketplace', 'mcp', 'applinks', 'devtools'];
const CATALOG_TABS: readonly CatalogTab[] = ['plugins', 'global'];
const USAGE_TABS: readonly UsageTab[] = ['daily', 'projects', 'models'];

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
    const seg = hash.split('/')[2] as CatalogTab | undefined;
    const tab = seg && CATALOG_TABS.includes(seg) ? seg : 'plugins';
    return { kind: 'catalog', tab };
  }

  if (hash.startsWith('#/settings')) {
    const seg = hash.split('/')[2] as SettingsTab | undefined;
    const tab = seg && SETTINGS_TABS.includes(seg) ? seg : 'otel';
    return { kind: 'settings', tab };
  }

  if (hash.startsWith('#/usage')) {
    const seg = hash.split('/')[2] as UsageTab | undefined;
    const tab = seg && USAGE_TABS.includes(seg) ? seg : 'daily';
    return { kind: 'usage', tab };
  }

  if (hash.startsWith('#/sessions/new/')) {
    try {
      return { kind: 'sessions-draft', projectPath: decodeURIComponent(hash.slice('#/sessions/new/'.length)) };
    } catch {
      return { kind: 'sessions-list' };
    }
  }
  if (hash.startsWith('#/sessions/')) {
    try {
      const rest = hash.slice('#/sessions/'.length);
      const qIdx = rest.indexOf('?');
      const idPart = qIdx === -1 ? rest : rest.slice(0, qIdx);
      const queryPart = qIdx === -1 ? '' : rest.slice(qIdx + 1);
      return { kind: 'sessions-detail', sessionId: decodeURIComponent(idPart), backTo: parseSessionBackTarget(queryPart) };
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

// '?back=home' | '?back=project:<encodedPath>' 형태의 쿼리스트링(선행 '?' 제외,
// 빈 문자열 포함)을 파싱한다. 모르는 kind·빈 path 등 잘못된 입력은 전부
// 'sessions-list' 기본값으로 안전하게 폴백한다 — 여기서 에러를 던지지 않는다.
function parseSessionBackTarget(queryPart: string): SessionBackTarget {
  if (!queryPart) return { kind: 'sessions-list' };
  const eqIdx = queryPart.indexOf('=');
  if (eqIdx === -1) return { kind: 'sessions-list' };
  const key = queryPart.slice(0, eqIdx);
  const value = queryPart.slice(eqIdx + 1);
  if (key !== 'back') return { kind: 'sessions-list' };
  if (value === 'home') return { kind: 'home' };
  if (value.startsWith('project:')) {
    const encodedPath = value.slice('project:'.length);
    if (!encodedPath) return { kind: 'sessions-list' };
    try {
      const path = decodeURIComponent(encodedPath);
      return path ? { kind: 'projects-detail', path } : { kind: 'sessions-list' };
    } catch {
      return { kind: 'sessions-list' };
    }
  }
  return { kind: 'sessions-list' };
}

// 세션 상세 뒤로가기 목적지를 해시 쿼리스트링으로 인코딩한다. 기본값
// (sessions-list, 또는 미지정)이면 쿼리스트링을 붙이지 않아 기존 동작과 동일하다.
export function encodeSessionBackTarget(backTo?: SessionBackTarget): string {
  if (!backTo || backTo.kind === 'sessions-list') return '';
  if (backTo.kind === 'home') return '?back=home';
  return `?back=project:${encodeURIComponent(backTo.path)}`;
}

export function navigate(hash: string): void {
  window.location.hash = hash;
}
