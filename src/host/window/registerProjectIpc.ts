// 프로젝트 목록/상세 IPC — 렌더러(`contextIsolation:true`, `nodeIntegration:false`)는
// `node:fs`에 직접 접근할 수 없다. 파일 읽기는 **메인 프로세스에서 하고 IPC로 넘긴다**
// (작업 지시 · §2.8과 같은 신뢰 경계). 렌더러가 넘기는 경로는 항상 `pathGuard.ts`로
// `~/workspace` 바로 아래 1단계 자식인지 검증한 뒤에만 읽는다 — 스캔 대상을
// `~/workspace` 아래로 한정한다.

import { ipcMain } from 'electron';
import { access, readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { IPC_CHANNEL, type ProjectDetailResult, type ProjectListItem } from './ipcContract.js';
import { isDirectChildOfWorkspaceRoot } from './pathGuard.js';
import { scanWorkspaceProjects } from './projectScanner.js';
import { parseStatusMarkdown } from './statusParser.js';

async function fileExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

let registered = false;

/**
 * 메인 창이 열릴 때 호출한다. 창을 여러 번 열어도(싱글턴이라 보통 1회지만) 핸들러
 * 중복 등록은 막는다 — `ipcMain.handle`은 같은 채널에 두 번째 핸들러를 등록하면
 * 던진다.
 */
export function registerProjectIpc(workspaceRoot: string): void {
  if (registered) return;
  registered = true;

  ipcMain.handle(IPC_CHANNEL.listProjects, async (): Promise<readonly ProjectListItem[]> => {
    return scanWorkspaceProjects({
      workspaceRoot,
      listDirectories: async (root) => {
        const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
        return entries.filter((entry) => entry.isDirectory() && !entry.name.startsWith('.')).map((entry) => join(root, entry.name));
      },
      fileExists,
      readTextFile: (path) => readFile(path, 'utf8'),
    });
  });

  ipcMain.handle(IPC_CHANNEL.getProjectDetail, async (_event, projectPath: unknown): Promise<ProjectDetailResult> => {
    if (typeof projectPath !== 'string' || !isDirectChildOfWorkspaceRoot(workspaceRoot, projectPath)) {
      return { ok: false, reason: 'invalid-path' };
    }

    const isAgentProject = await fileExists(join(projectPath, 'CLAUDE.md'));
    if (!isAgentProject) {
      return { ok: false, reason: 'not-a-project' };
    }

    const name = projectPath.split(/[/\\]/).filter(Boolean).pop() ?? projectPath;
    const statusPath = join(projectPath, 'STATUS.md');
    const hasStatus = await fileExists(statusPath);
    if (!hasStatus) {
      return { ok: true, name, path: projectPath, hasStatus: false, sections: null };
    }

    try {
      const content = await readFile(statusPath, 'utf8');
      return { ok: true, name, path: projectPath, hasStatus: true, sections: parseStatusMarkdown(content) };
    } catch {
      return { ok: false, reason: 'read-error' };
    }
  });
}

/** 테스트 전용 — 등록 가드를 초기화한다. */
export function __resetProjectIpcRegistrationForTests(): void {
  registered = false;
}
