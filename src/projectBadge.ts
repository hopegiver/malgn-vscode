// 프로젝트 상태 배지 판정 — 카드 목록과 상세 헤더(그리고 진행/보관 필터)가 같은 규칙을
// 쓰도록 한 곳에 모은 순수 함수.
//
// 배지가 담는 의미 = "STATUS.md에서 실제로 읽어낸 진행/보관 판정". STATUS.md가 없거나
// 읽기에 실패했거나(sections 없음) 이모지 섹션을 하나도 못 읽었으면(parsed:false)
// 판정할 근거가 없으므로 archiveStatus 값과 무관하게 'unknown'(상태 없음)으로 본다.
// 근거 없는 "진행"을 보여주는 것보다 "상태 없음"이 사실에 맞다.
//
// 카드 목록과 상세는 같은 list_workspace_projects 응답(state.dashboard.projects)을
// 공유하므로 두 화면 모두 hasStatus·sections를 가지고 있다 — 이 함수 하나로 충분하다.
import type { ArchiveStatus } from './state';
import type { WorkspaceProject } from './workspaceApi';

export function projectBadgeStatus(project: Pick<WorkspaceProject, 'hasStatus' | 'archiveStatus' | 'sections'>): ArchiveStatus {
  if (!project.hasStatus || !project.sections?.parsed) return 'unknown';
  return project.archiveStatus;
}
