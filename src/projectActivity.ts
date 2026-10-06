// 프로젝트별 에이전트 진행 상황 파생 함수(docs/design/project-agent-progress.md §3).
// 세션↔프로젝트 판정은 백엔드(list_claude_sessions)가 각 행에 붙여 주는
// projectPath(프로젝트 path 원문)로만 한다 — cwd 접두사 매칭을 여기서 재구현하지 않는다.
import { asBoolean, asNumber, asString } from './views/sessions';
import type { ClaudeSessionRecord } from './sessionsApi';

export interface ProjectActivity {
  readonly running: ClaudeSessionRecord[];
  readonly recent: ClaudeSessionRecord[];
}

function recencyKey(s: ClaudeSessionRecord): number {
  return asNumber(s.updatedAt) ?? asNumber(s.startedAt) ?? 0;
}

/** projectPath → 실행 중 세션 / 끝난 세션(recent, updatedAt ?? startedAt 내림차순). projectPath 없는 행은 건너뛴다. */
export function groupSessionsByProject(items: readonly ClaudeSessionRecord[]): Map<string, ProjectActivity> {
  const map = new Map<string, { running: ClaudeSessionRecord[]; recent: ClaudeSessionRecord[] }>();
  for (const s of items) {
    const projectPath = asString(s.projectPath);
    if (!projectPath) continue;
    let entry = map.get(projectPath);
    if (!entry) {
      entry = { running: [], recent: [] };
      map.set(projectPath, entry);
    }
    (asBoolean(s.running) ? entry.running : entry.recent).push(s);
  }
  for (const entry of map.values()) {
    entry.running.sort((a, b) => recencyKey(b) - recencyKey(a));
    entry.recent.sort((a, b) => recencyKey(b) - recencyKey(a));
  }
  return map;
}

/** 활동 중 프로젝트 수(실행 중 세션이 있는 고유 projectPath)와 실행 중 세션 수. */
export function portfolioSummary(items: readonly ClaudeSessionRecord[]): { activeProjects: number; runningSessions: number } {
  const active = new Set<string>();
  let runningSessions = 0;
  for (const s of items) {
    if (!asBoolean(s.running)) continue;
    runningSessions += 1;
    const projectPath = asString(s.projectPath);
    if (projectPath) active.add(projectPath);
  }
  return { activeProjects: active.size, runningSessions };
}
