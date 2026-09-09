// "프로젝트" 화면의 실제 데이터 소스. Rust 커맨드 list_workspace_projects()가
// ~/workspace 바로 아래 1단계 디렉터리를 스캔해 CLAUDE.md가 있는 폴더만 프로젝트로
// 인식하고, STATUS.md가 있으면 이모지 섹션 파싱까지 마쳐서 반환한다(electron 브랜치
// projectScanner.ts/statusParser.ts 알고리즘 포팅 — src-tauri/src/lib.rs 주석 참고).
import { invoke } from '@tauri-apps/api/core';
import type { ArchiveStatus } from './state';

export interface ProjectStatusSections {
  readonly parsed: boolean;
  readonly current: string | null;
  readonly recentDone: string | null;
  readonly inProgress: string | null;
  readonly blocked: string | null;
  readonly raw: string;
}

export interface WorkspaceProject {
  readonly name: string;
  readonly path: string;
  readonly hasStatus: boolean;
  readonly archiveStatus: ArchiveStatus;
  readonly sections: ProjectStatusSections | null;
  // UNIX epoch 밀리초 — STATUS.md/CLAUDE.md 중 더 최근 mtime.
  readonly updatedAt: number;
}

export async function fetchWorkspaceProjects(): Promise<WorkspaceProject[]> {
  return invoke<WorkspaceProject[]>('list_workspace_projects');
}

// ---------------- 프로젝트 폴더 구조 + 파일 미리보기 (읽기 전용) ----------------
// Rust가 경로 검증(워크스페이스 루트 1단계 자식만 허용 + canonicalize 기반 트래버설
// 차단)을 전부 맡는다 — 프론트엔드는 이미 목록으로 받은 project.path만 그대로
// 되돌려 보낸다(자유 입력 경로 필드 없음).

export interface ProjectTreeNode {
  readonly name: string;
  readonly relativePath: string;
  readonly isDirectory: boolean;
  readonly children: readonly ProjectTreeNode[] | null;
  readonly truncated: boolean;
}

export async function fetchProjectTree(projectPath: string): Promise<ProjectTreeNode[]> {
  return invoke<ProjectTreeNode[]>('list_project_tree', { projectPath });
}

export type FilePreview =
  | { readonly kind: 'text'; readonly content: string }
  | { readonly kind: 'tooLarge'; readonly size: number }
  | { readonly kind: 'binary' }
  | { readonly kind: 'notFound' }
  | { readonly kind: 'denied' };

export async function fetchFilePreview(projectPath: string, relativePath: string): Promise<FilePreview> {
  return invoke<FilePreview>('read_project_file', { projectPath, relativePath });
}
