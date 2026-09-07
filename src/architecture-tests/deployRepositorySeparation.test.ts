// NT-R23 테스트 — docs/release-gates.md §7.6.6.
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  checkDeployRepoWiringConsistency,
  checkNoReleaseArtifactsTracked,
  defaultDeployRepoConfigPath,
  defaultVisibilityWorkflowPath,
} from './deployRepositorySeparation.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_REPO_ROOT = join(HERE, '..', '..');

describe('checkNoReleaseArtifactsTracked — 실제 저장소 대조', () => {
  it('오늘 이 저장소에는 추적된 릴리스 아티팩트가 없다(dist/·site/·*.vsix는 모두 .gitignore 대상)', () => {
    const result = checkNoReleaseArtifactsTracked(REAL_REPO_ROOT);
    expect(result.violations).toEqual([]);
  });
});

describe('checkNoReleaseArtifactsTracked — fixture 위반 주입(실제 git 저장소)', () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-deploy-sep-'));
    execFileSync('git', ['init', '-q'], { cwd: root });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('dist/extension.cjs가 커밋되면(잘못된 .gitignore 등으로) 위반을 잡는다', async () => {
    await mkdir(join(root, 'dist'), { recursive: true });
    await writeFile(join(root, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    execFileSync('git', ['add', 'dist/extension.cjs'], { cwd: root });

    const result = checkNoReleaseArtifactsTracked(root);
    expect(result.violations.some((v) => v.includes('dist/extension.cjs'))).toBe(true);
  });

  it('*.vsix 아티팩트가 커밋되면 위반을 잡는다', async () => {
    await writeFile(join(root, 'malgn-0.1.0.vsix'), 'fake vsix content', 'utf8');
    execFileSync('git', ['add', 'malgn-0.1.0.vsix'], { cwd: root });

    const result = checkNoReleaseArtifactsTracked(root);
    expect(result.violations.some((v) => v.includes('.vsix'))).toBe(true);
  });

  it('서명 키 파일(.p12)이 커밋되면 위반을 잡는다', async () => {
    await writeFile(join(root, 'signing-cert.p12'), 'fake key material', 'utf8');
    execFileSync('git', ['add', 'signing-cert.p12'], { cwd: root });

    const result = checkNoReleaseArtifactsTracked(root);
    expect(result.violations.some((v) => v.includes('.p12'))).toBe(true);
  });

  it('무관한 소스 파일만 있으면 위반이 없다', async () => {
    await writeFile(join(root, 'index.ts'), 'export const x = 1;\n', 'utf8');
    execFileSync('git', ['add', 'index.ts'], { cwd: root });

    const result = checkNoReleaseArtifactsTracked(root);
    expect(result.violations).toEqual([]);
  });
});

describe('checkDeployRepoWiringConsistency — 실제 저장소 대조', () => {
  it('오늘은 설정도 워크플로도 없다 — "대상 없음"으로 통과한다', () => {
    const result = checkDeployRepoWiringConsistency({
      deployRepoConfigPath: defaultDeployRepoConfigPath(REAL_REPO_ROOT),
      visibilityWorkflowPath: defaultVisibilityWorkflowPath(REAL_REPO_ROOT),
    });
    expect(result.skippedNoTarget).toBe(true);
    expect(result.violations).toEqual([]);
  });
});

describe('checkDeployRepoWiringConsistency — fixture 위반 주입(배포 저장소가 생겼다고 가정)', () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-deploy-wiring-'));
    await mkdir(join(root, 'compat'), { recursive: true });
    await mkdir(join(root, '.github', 'workflows'), { recursive: true });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  function configPath() {
    return defaultDeployRepoConfigPath(root);
  }
  function workflowPath() {
    return defaultVisibilityWorkflowPath(root);
  }

  it('설정이 채워졌는데 워크플로가 없으면 위반을 잡는다', async () => {
    await writeFile(
      configPath(),
      JSON.stringify({ owner: 'malgnsoft', name: 'malgn-vscode-releases', expectedVisibility: 'private' }),
      'utf8'
    );
    const result = checkDeployRepoWiringConsistency({ deployRepoConfigPath: configPath(), visibilityWorkflowPath: workflowPath() });
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations.some((v) => v.includes('가시성 감시 워크플로가 없습니다'))).toBe(true);
  });

  it('워크플로만 있고 설정이 비어 있으면 위반을 잡는다(고아 워크플로)', async () => {
    await writeFile(configPath(), JSON.stringify({ owner: null, name: null, expectedVisibility: null }), 'utf8');
    await writeFile(workflowPath(), 'name: deploy-repo-visibility-check\n', 'utf8');
    const result = checkDeployRepoWiringConsistency({ deployRepoConfigPath: configPath(), visibilityWorkflowPath: workflowPath() });
    expect(result.violations.some((v) => v.includes('고아 워크플로'))).toBe(true);
  });

  it('설정과 워크플로가 둘 다 있으면 위반이 없다(제대로 배선됨)', async () => {
    await writeFile(
      configPath(),
      JSON.stringify({ owner: 'malgnsoft', name: 'malgn-vscode-releases', expectedVisibility: 'private' }),
      'utf8'
    );
    await writeFile(workflowPath(), 'name: deploy-repo-visibility-check\n', 'utf8');
    const result = checkDeployRepoWiringConsistency({ deployRepoConfigPath: configPath(), visibilityWorkflowPath: workflowPath() });
    expect(result.violations).toEqual([]);
    expect(result.skippedNoTarget).toBe(false);
  });

  it('설정이 일부만 채워지면 위반을 잡는다', async () => {
    await writeFile(configPath(), JSON.stringify({ owner: 'malgnsoft', name: null, expectedVisibility: null }), 'utf8');
    await writeFile(workflowPath(), 'name: deploy-repo-visibility-check\n', 'utf8');
    const result = checkDeployRepoWiringConsistency({ deployRepoConfigPath: configPath(), visibilityWorkflowPath: workflowPath() });
    expect(result.violations.length).toBeGreaterThan(0);
  });
});
