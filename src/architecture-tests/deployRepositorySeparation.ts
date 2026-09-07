// NT-R23 (docs/release-gates.md §7.6.6) — "배포 저장소를 소스 저장소와 분리 + 가시성
// 변경에 실패하는 검사."
//
// [지금 실제로 되는 것과 조건부 대기인 것 — 정직 표기]
// - **실제로 동작(오늘)**: `checkNoReleaseArtifactsTracked` — 이 소스 저장소 자신이
//   배포 표면으로 새는 것을 막는다. 릴리스 아티팩트(`dist/**`·`*.vsix`·`site/**`·
//   `build-provenance.json`·서명 키 파일 확장자)가 **추적 트리에 커밋되면** 그 자체가
//   "분리"의 실패이므로(소스 저장소가 곧 배포 매체가 되어 버린다) 지금 이 저장소
//   상태로 즉시 실행·검증 가능하다.
// - **조건부 대기(fail-closed)**: `checkDeployRepoWiringConsistency` — 별도 private
//   배포 저장소는 아직 없다(`compat/deploy-repository.json`이 전부 null). "가시성
//   변경에 실패하는 검사"의 실물(GitHub API로 저장소 visibility를 폴링하는 워크플로)은
//   **배포 저장소가 실재해야 의미가 있다** — 없는 저장소의 가시성을 물을 수 없다.
//   그래서 지금은 "설정(owner/name/expectedVisibility)과 감시 워크플로 파일의 존재가
//   서로 일치하는가"만 구조적으로 강제한다: 배포 저장소 설정이 채워졌는데 감시
//   워크플로가 없으면(=분리는 했는데 가시성 변경 감지가 안 걸림) 실패하고, 반대로
//   워크플로만 있고 설정이 비어 있으면(=가리키는 대상이 없는 고아 워크플로) 그것도
//   실패한다. 오늘은 둘 다 없어 `skippedNoTarget: true`.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export interface DeployRepoSeparationResult {
  readonly violations: readonly string[];
}

export interface DeployRepoWiringResult {
  readonly violations: readonly string[];
  readonly skippedNoTarget: boolean;
}

/** 릴리스 아티팩트로 볼 만한 경로 패턴 — 추적 트리에 등장하면 그 자체가 NT-R23
 * 위반이다(소스 저장소가 배포 매체를 겸하게 된다). 서명 키 파일 확장자는 K-1/K-3
 * 자산이 실수로라도 커밋되는 것을 잡기 위해 포함한다(release-gates.md §7.6 K-1/K-3 —
 * 이 파일들은 애초에 저장소에 있으면 안 된다). */
const RELEASE_ARTIFACT_PATTERNS: readonly RegExp[] = [
  /^dist\//,
  /^site\//,
  /\.vsix$/,
  /build-provenance\.json$/,
  /\.p12$/,
  /\.pfx$/,
  /\.pem$/,
  /\.keystore$/,
];

/**
 * `git ls-files`(추적 트리)에 릴리스 아티팩트로 볼 만한 경로가 있는지 확인한다.
 * 하나라도 있으면 위반 — 소스 저장소와 배포 매체가 분리되지 않았다는 뜻이다.
 */
export function checkNoReleaseArtifactsTracked(repoRoot: string): DeployRepoSeparationResult {
  const lsFilesOut = execFileSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf8' });
  const files = lsFilesOut.split('\0').filter((p) => p.length > 0);

  const violations = files
    .filter((f) => RELEASE_ARTIFACT_PATTERNS.some((re) => re.test(f)))
    .map((f) => `NT-R23: 릴리스 아티팩트로 보이는 파일이 추적 트리에 있습니다 — ${f} (소스 저장소와 배포 매체가 분리되지 않았습니다)`);

  return { violations };
}

interface DeployRepoConfig {
  readonly owner: string | null;
  readonly name: string | null;
  readonly expectedVisibility: string | null;
}

/**
 * `compat/deploy-repository.json`(배포 저장소 설정)과 가시성 감시 워크플로 파일의
 * 존재가 서로 일치하는지 확인한다. 배포 저장소가 아직 없어 설정이 전부 null이고
 * 워크플로도 없으면 "대상 없음"으로 통과한다(오늘의 정상 상태) — 어느 한쪽만
 * 있으면 불일치이므로 실패한다.
 */
export function checkDeployRepoWiringConsistency(input: {
  readonly deployRepoConfigPath: string;
  readonly visibilityWorkflowPath: string;
}): DeployRepoWiringResult {
  const config = JSON.parse(readFileSync(input.deployRepoConfigPath, 'utf8')) as DeployRepoConfig;
  const configConfigured = config.owner !== null || config.name !== null || config.expectedVisibility !== null;
  const workflowExists = existsSync(input.visibilityWorkflowPath);

  if (!configConfigured && !workflowExists) {
    return { violations: [], skippedNoTarget: true };
  }

  const violations: string[] = [];
  if (configConfigured && !workflowExists) {
    violations.push(
      'NT-R23: compat/deploy-repository.json에 배포 저장소 설정이 있는데 가시성 감시 워크플로가 없습니다 — ' +
        '배포 저장소를 분리했다면 가시성 변경에 실패하는 검사도 함께 있어야 합니다(docs/release-gates.md §7.6.6).'
    );
  }
  if (!configConfigured && workflowExists) {
    violations.push(
      'NT-R23: 가시성 감시 워크플로는 있는데 compat/deploy-repository.json 설정이 비어 있습니다 — ' +
        '가리키는 대상이 없는 고아 워크플로입니다.'
    );
  }
  if (configConfigured && (config.owner === null || config.name === null || config.expectedVisibility === null)) {
    violations.push(
      `NT-R23: compat/deploy-repository.json 설정이 일부만 채워졌습니다(owner=${String(config.owner)}, name=${String(
        config.name
      )}, expectedVisibility=${String(config.expectedVisibility)}) — 셋 다 채우거나 전부 null로 두어야 합니다.`
    );
  }

  return { violations, skippedNoTarget: false };
}

export function defaultDeployRepoConfigPath(repoRoot: string): string {
  return join(repoRoot, 'compat', 'deploy-repository.json');
}

export function defaultVisibilityWorkflowPath(repoRoot: string): string {
  return join(repoRoot, '.github', 'workflows', 'deploy-repo-visibility-check.yml');
}
