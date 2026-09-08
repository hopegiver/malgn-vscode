import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { classifyArchiveStatus, scanWorkspaceProjects, type ScanWorkspaceProjectsDeps } from './projectScanner.js';

const WORKSPACE_ROOT = '/home/runner/workspace';

function fakeDeps(overrides: {
  readonly dirs: readonly string[];
  readonly claudeMdDirs: readonly string[];
  readonly statusMdByDir?: Readonly<Record<string, string>>;
}): ScanWorkspaceProjectsDeps {
  const statusMap = overrides.statusMdByDir ?? {};
  return {
    workspaceRoot: WORKSPACE_ROOT,
    listDirectories: async () => overrides.dirs,
    fileExists: async (path) => {
      if (path.endsWith('CLAUDE.md')) {
        const dir = path.slice(0, -'/CLAUDE.md'.length);
        return overrides.claudeMdDirs.includes(dir);
      }
      if (path.endsWith('STATUS.md')) {
        const dir = path.slice(0, -'/STATUS.md'.length);
        return dir in statusMap;
      }
      return false;
    },
    readTextFile: async (path) => {
      const dir = path.slice(0, -'/STATUS.md'.length);
      const content = statusMap[dir];
      if (content === undefined) throw new Error(`no fixture for ${path}`);
      return content;
    },
  };
}

describe('scanWorkspaceProjects', () => {
  it('CLAUDE.md가 있는 디렉터리만 malgn-agent 프로젝트로 취급한다', async () => {
    const withClaude = join(WORKSPACE_ROOT, 'proj-a');
    const withoutClaude = join(WORKSPACE_ROOT, 'not-a-project');
    const deps = fakeDeps({ dirs: [withClaude, withoutClaude], claudeMdDirs: [withClaude] });

    const result = await scanWorkspaceProjects(deps);

    expect(result).toHaveLength(1);
    expect(result[0]?.name).toBe('proj-a');
    expect(result[0]?.path).toBe(withClaude);
  });

  it('STATUS.md가 없으면 hasStatus:false, archiveStatus:"unknown"이다(판별 기준 아님)', async () => {
    const dir = join(WORKSPACE_ROOT, 'proj-no-status');
    const deps = fakeDeps({ dirs: [dir], claudeMdDirs: [dir] });

    const result = await scanWorkspaceProjects(deps);

    expect(result[0]?.hasStatus).toBe(false);
    expect(result[0]?.archiveStatus).toBe('unknown');
  });

  it('STATUS.md의 🟢 현재 상태 본문에 보관 키워드가 있으면 archived로 분류한다', async () => {
    const dir = join(WORKSPACE_ROOT, 'proj-archived');
    const deps = fakeDeps({
      dirs: [dir],
      claudeMdDirs: [dir],
      statusMdByDir: { [dir]: '## 🟢 현재 상태\n이 프로젝트는 보관 처리됐습니다.' },
    });

    const result = await scanWorkspaceProjects(deps);

    expect(result[0]?.hasStatus).toBe(true);
    expect(result[0]?.archiveStatus).toBe('archived');
  });

  it('STATUS.md가 있고 보관 키워드가 없으면 active로 분류한다', async () => {
    const dir = join(WORKSPACE_ROOT, 'proj-active');
    const deps = fakeDeps({
      dirs: [dir],
      claudeMdDirs: [dir],
      statusMdByDir: { [dir]: '## 🟢 현재 상태\n한창 개발 중.' },
    });

    const result = await scanWorkspaceProjects(deps);

    expect(result[0]?.archiveStatus).toBe('active');
  });

  it('결과를 이름순으로 정렬한다', async () => {
    const b = join(WORKSPACE_ROOT, 'b-proj');
    const a = join(WORKSPACE_ROOT, 'a-proj');
    const deps = fakeDeps({ dirs: [b, a], claudeMdDirs: [b, a] });

    const result = await scanWorkspaceProjects(deps);

    expect(result.map((p) => p.name)).toEqual(['a-proj', 'b-proj']);
  });

  it('숨김 디렉터리는 방어적으로 제외한다', async () => {
    const hidden = join(WORKSPACE_ROOT, '.hidden');
    const deps = fakeDeps({ dirs: [hidden], claudeMdDirs: [hidden] });

    const result = await scanWorkspaceProjects(deps);

    expect(result).toEqual([]);
  });

  it('디렉터리가 없으면 빈 배열을 반환한다(빈 상태 UI가 이 값으로 판정한다)', async () => {
    const deps = fakeDeps({ dirs: [], claudeMdDirs: [] });
    expect(await scanWorkspaceProjects(deps)).toEqual([]);
  });
});

describe('classifyArchiveStatus', () => {
  it('hasStatus:false면 항상 unknown이다', () => {
    expect(classifyArchiveStatus(false, '보관')).toBe('unknown');
  });

  it('currentSectionBody가 null이면(파싱 실패) unknown이다', () => {
    expect(classifyArchiveStatus(true, null)).toBe('unknown');
  });

  it('대소문자 무관하게 영문 키워드도 인식한다', () => {
    expect(classifyArchiveStatus(true, 'This project is DEPRECATED.')).toBe('archived');
  });
});
