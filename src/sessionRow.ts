// 세션 요약 행(.blist-row--sessions) 공용 렌더러 — 홈 "최근 세션" 박스와 프로젝트
// 상세 "세션" 섹션이 같은 마크업을 쓰도록 한 곳에 둔다(사본 금지). 프로젝트 상세는
// 이미 한 프로젝트 안이라 프로젝트명 열이 중복이므로 showProject:false로 뺀다.
import { el, clickable } from './dom';
import { encodeSessionBackTarget, navigate } from './route';
import type { SessionBackTarget } from './route';
import { asBoolean, asNumber, asString, sessionProjectName, sessionTitle } from './views/sessions';
import type { ClaudeSessionRecord } from './sessionsApi';

// m2 수정(리뷰 2026-09-24) — 세션/자율 작업 id를 같은 "#앞8자…" 표기로 줄인다.
export function shortId(id: string): string {
  return id.length <= 8 ? `#${id}` : `#${id.slice(0, 8)}…`;
}

export function formatCompactElapsed(ms: number): string {
  const diffMin = Math.floor((Date.now() - ms) / 60000);
  if (diffMin < 1) return '방금';
  if (diffMin < 60) return `${diffMin}m`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) {
    const remMin = diffMin % 60;
    return remMin > 0 ? `${diffHour}h ${String(remMin).padStart(2, '0')}m` : `${diffHour}h`;
  }
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay === 1) return '어제';
  return `${diffDay}일 전`;
}

function accentEl(s: ClaudeSessionRecord): HTMLElement {
  const e = el('span', { className: 'blist-accent' }, [sessionProjectName(s)]);
  const cwd = asString(s.cwd);
  if (cwd) e.setAttribute('title', cwd); // 하위 디렉터리 세션의 실제 cwd 확인용
  return e;
}

// backTo 생략 시 기존 동작과 동일하게 뒤로가기 목적지가 세션 목록(기본값)이 된다
// — home 박스/projects-detail 세션 섹션은 각각 { kind: 'home' } /
// { kind: 'projects-detail', path }를 넘겨 그 화면으로 되돌아가게 한다.
export function sessionBlistRow(s: ClaudeSessionRecord, opts: { readonly showProject?: boolean; readonly backTo?: SessionBackTarget } = {}): HTMLElement {
  const showProject = opts.showProject !== false;
  const sessionId = asString(s.sessionId);
  const running = asBoolean(s.running);
  const updatedAt = asNumber(s.updatedAt) ?? asNumber(s.startedAt);
  const row = el('div', { className: `blist-row blist-row--sessions${showProject ? '' : ' blist-row--sessions-noproj'}` }, [
    el('span', { className: 'blist-id' }, [shortId(sessionId)]),
    ...(showProject ? [accentEl(s)] : []),
    el('span', { className: 'blist-body' }, [sessionTitle(s)]),
    el('span', { className: `blist-status ${running ? 'status-run' : 'status-done'}` }, [el('span', { className: 'dot' }, []), running ? '실행 중' : '완료']),
    el('span', { className: 'blist-time' }, [updatedAt !== null ? formatCompactElapsed(updatedAt) : '-']),
  ]);
  return clickable(row, () => navigate(`#/sessions/${encodeURIComponent(sessionId)}${encodeSessionBackTarget(opts.backTo)}`));
}
