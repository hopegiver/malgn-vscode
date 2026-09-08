// 메인 창 IPC 계약 — 채널 이름과 페이로드 타입의 단일 정본. `electron`을 import하지
// 않는 순수 타입/상수 파일이라 main(등록 지점)·preload(브리지)·renderer(호출부) 세
// 곳이 전부 이 파일 하나만 보고 형태를 맞춘다(형태가 세 곳에서 따로 어긋나는 것을
// 막는다).

import type { ProjectArchiveStatus } from './projectScanner.js';
import type { ParsedStatusSections } from './statusParser.js';

export type { ProjectArchiveStatus } from './projectScanner.js';
export type { ParsedStatusSections } from './statusParser.js';

export const IPC_CHANNEL = {
  listProjects: 'malgn:projects:list',
  getProjectDetail: 'malgn:projects:detail',
} as const;

export interface ProjectListItem {
  readonly name: string;
  readonly path: string;
  readonly hasStatus: boolean;
  readonly archiveStatus: ProjectArchiveStatus;
}

export type ProjectDetailResult =
  | { readonly ok: true; readonly name: string; readonly path: string; readonly hasStatus: boolean; readonly sections: ParsedStatusSections | null }
  | { readonly ok: false; readonly reason: 'invalid-path' | 'not-a-project' | 'read-error' };

/** 렌더러(`contextBridge`로 노출되는 전역)가 보는 타입 — preload.ts 구현과 이 인터페이스가
 * 일치해야 한다. */
export interface MalgnRendererApi {
  readonly listProjects: () => Promise<readonly ProjectListItem[]>;
  readonly getProjectDetail: (projectPath: string) => Promise<ProjectDetailResult>;
}
