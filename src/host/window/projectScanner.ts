// `~/workspace` 아래 malgn-agent 프로젝트 스캔 — PM 확정 판별 기준(실측 근거 있음):
// **`CLAUDE.md`가 있으면 malgn-agent 프로젝트**다. `STATUS.md`는 판별 기준이 아니라
// 선택적 상태 소스(gitignore 대상이라 없을 수 있다) — 없으면 `archiveStatus:'unknown'` +
// `hasStatus:false`로 접는다(추측해 분류하지 않는다, PR-6과 같은 정신).
//
// [순수 함수 + DI] 이 파일은 `node:fs`를 직접 쓰지 않는다 — 호출자(host/window/
// registerProjectIpc.ts, Electron 계층)가 실제 fs 함수를 주입한다. 그래서 이 로직은
// Electron 없이도 vitest로 전량 검증 가능하다(같은 이유로 다른 provider들이 실행을
// `platform/exec.ts` DI로 격리하는 것과 같은 패턴).

import { basename, join } from 'node:path';
import { parseStatusMarkdown } from './statusParser.js';

export interface ScanWorkspaceProjectsDeps {
  readonly workspaceRoot: string;
  /** `workspaceRoot` 바로 아래 디렉터리들의 절대경로 목록을 반환한다(숨김 디렉터리는
   * 호출자가 걸러도 되고, 이 함수도 방어적으로 다시 거른다). */
  readonly listDirectories: (root: string) => Promise<readonly string[]>;
  readonly fileExists: (path: string) => Promise<boolean>;
  readonly readTextFile: (path: string) => Promise<string>;
}

export type ProjectArchiveStatus = 'active' | 'archived' | 'unknown';

export interface ProjectSummary {
  readonly name: string;
  readonly path: string;
  readonly hasStatus: boolean;
  readonly archiveStatus: ProjectArchiveStatus;
}

// STATUS.md "🟢 현재 상태" 본문에 이 **구(phrase)**가 있으면 보관 상당으로 분류한다
// (휴리스틱 — 구조화된 상태 필드가 없는 자유 텍스트에서 뽑아내는 최선의 근사치다.
// 오분류돼도 "전체" 필터에는 항상 나타나므로 데이터가 숨겨지지는 않는다).
//
// [단어 하나가 아니라 구를 쓰는 이유 — 실측 오탐 수정] 이 저장소 실제 워크스페이스로
// 검증하는 중 `'보관'`·`'종료'`·`'중단'` 같은 한 단어 키워드가 "스크린샷 원본 보관
// 중"(claude-code-guide) · "자동종료 안 됨"(malgnai, `종료`가 `자동종료`에 포함되어
// 매칭) 같은 **프로젝트 자체와 무관한 문장에서 오탐**했다(한국어는 띄어쓰기 없는
// 복합어가 흔해 부분 문자열 매칭이 특히 위험하다). "이 프로젝트가 보관/중단됐다"는
// 의도가 분명한 구만 남긴다 — recall은 낮아지지만(현재 실제 워크스페이스에는 이
// 구로 걸리는 프로젝트가 0개다) precision을 우선한다(오탐이 사람을 오도하는 게, 놓친
// 분류가 "전체" 아래 남는 것보다 나쁘다).
const ARCHIVE_KEYWORDS = ['프로젝트 보관', '보관 처리', '보관 상태', '개발 중단', '서비스 종료', '운영 종료', 'archived', 'deprecated'];

export function classifyArchiveStatus(hasStatus: boolean, currentSectionBody: string | null): ProjectArchiveStatus {
  if (!hasStatus || currentSectionBody === null) return 'unknown';
  const normalized = currentSectionBody.toLowerCase();
  return ARCHIVE_KEYWORDS.some((keyword) => normalized.includes(keyword.toLowerCase())) ? 'archived' : 'active';
}

export async function scanWorkspaceProjects(deps: ScanWorkspaceProjectsDeps): Promise<ProjectSummary[]> {
  const dirs = await deps.listDirectories(deps.workspaceRoot);
  const results: ProjectSummary[] = [];

  for (const dir of dirs) {
    const name = basename(dir);
    if (name.startsWith('.')) continue; // 방어적 재확인 — 숨김 디렉터리는 대상 아님

    const isAgentProject = await deps.fileExists(join(dir, 'CLAUDE.md'));
    if (!isAgentProject) continue;

    const statusPath = join(dir, 'STATUS.md');
    const hasStatus = await deps.fileExists(statusPath);
    let archiveStatus: ProjectArchiveStatus = 'unknown';
    if (hasStatus) {
      const content = await deps.readTextFile(statusPath);
      const { current } = parseStatusMarkdown(content);
      archiveStatus = classifyArchiveStatus(true, current);
    }

    results.push({ name, path: dir, hasStatus, archiveStatus });
  }

  return [...results].sort((a, b) => a.name.localeCompare(b.name));
}
