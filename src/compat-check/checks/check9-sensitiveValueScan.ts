// 검사 ⑨ — 민감값 스캔 (docs/policy-contract.md §8.3 신설 3종 / §8.6 3차선)
//
// **추적 트리 전체**(`git ls-files`, 폴더 무관)를 오프라인 정규식으로 스캔한다. 패턴·
// allowlist는 `compat/sensitive-classes.json`(공개면 — 패턴만, 값 없음)에서 온다.
//
// 정본 스캔 로직은 `scripts/lib/sensitiveScan.mjs`에 있다 — 이 검사(CI)와 `.githooks/
// pre-push`(로컬)가 **같은 모듈**을 import해 "규칙 이중 정의"를 구조적으로 막는다(작업
// 지시 §8.6 2차선 원문 그대로).
//
// [NT-R21 확장 — docs/release-gates.md §7.6.6] 대상을 "추적 트리"에서 "추적 트리 +
// 출하되는 모든 매체"로 넓힌다. `shippedMediaPaths`(오늘은 `dist/extension.cjs` 하나)를
// 추가로 스캔한다 — **best-effort**: 이 검사가 `pnpm compat:check`(문서화된 파이프라인
// 순서상 `compile` 이전에 돈다)에서 실행될 때 그 경로가 아직 없으면 조용히 건너뛰지
// 않고 명시적으로 skip을 로그에 남긴다(console.warn). **진짜 fail-closed 집행 지점은
// 여기가 아니라 `scripts/assert-shipped-media-scan.mjs`다** — 그 스크립트는 패키징
// 파이프라인(`pnpm run package`, 빌드 *다음*)에 배선되어 있어 그 시점엔 산출물이
// 반드시 존재하고, 없으면 그 자체를 실패로 취급한다(§7.5.1 M-40과 같은 "이름에
// 의존하지 않는" 원칙 — 이 검사는 로컬 전 상태 점검용 보조선일 뿐이다).

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
// scripts/lib/sensitiveScan.mjs는 일반 JS(.mjs) 모듈이다 — TS 프로젝트 밖(scripts/)에
// 있다. 타입은 sensitiveScan.d.mts가 제공한다.
import { getEnforcedCoverageGaps, loadClassesConfig, scanShippedMedium, scanText } from '../../../scripts/lib/sensitiveScan.mjs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check9Input {
  readonly repoRoot: string;
  readonly sensitiveClassesJsonPath: string;
  /** NT-R21 — 추적 트리 밖에서 함께 스캔할 출하 매체의 절대경로 목록. 존재하지 않는
   * 경로는 위반이 아니라 명시적 skip으로 처리한다(§7.5.1과 같은 "조용한 통과" 방지
   * 원칙 — skip 자체는 console.warn으로 드러난다). */
  readonly shippedMediaPaths?: readonly string[];
}

export function checkSensitiveValueScan(input: Check9Input): CheckResult {
  const id = '⑨';
  const label = '민감값 스캔 — 추적 트리 전체 + 출하 매체(NT-R21), 오프라인 패턴 + allowlist';

  const config = loadClassesConfig(readFileSync(input.sensitiveClassesJsonPath, 'utf8'));

  const lsFilesOut = execFileSync('git', ['ls-files', '-z'], { cwd: input.repoRoot, encoding: 'utf8' });
  const files = lsFilesOut.split('\0').filter((p) => p.length > 0);

  const violations: { file: string; classId: string; match: string }[] = [];
  for (const relPath of files) {
    let text: string;
    try {
      text = readFileSync(`${input.repoRoot}/${relPath}`, 'utf8');
    } catch {
      continue; // 바이너리·읽기 불가 파일은 건너뛴다(텍스트 스캔 대상이 아니다)
    }
    violations.push(...scanText(relPath, text, config));
  }

  for (const mediaPath of input.shippedMediaPaths ?? []) {
    if (!existsSync(mediaPath)) {
      // NT-R21 best-effort 한계(정직 표기) — 진짜 fail-closed 집행은
      // scripts/assert-shipped-media-scan.mjs(패키징 시점)의 몫이다.
      console.warn(`[검사⑨/NT-R21] 출하 매체 대상 없음(빌드 전 상태로 추정) — 건너뜀: ${mediaPath}`);
      continue;
    }
    const buffer = readFileSync(mediaPath);
    const { violations: mediaViolations } = scanShippedMedium(mediaPath, buffer, config);
    violations.push(...mediaViolations);
  }

  // B4(security-plan.md §12.4) — enforcedClasses의 각 부류를 커버하는 활성 class가
  // 최소 1개 있어야 한다. 오늘 상태(PUB-X·PUB-A 패턴 0개)는 이 규칙 하나로 즉시
  // 실패해야 한다 — 그게 이 장치를 넣는 실질 이유다.
  for (const gap of getEnforcedCoverageGaps(config)) {
    violations.push({
      file: 'compat/sensitive-classes.json',
      classId: 'enforced-coverage-gap',
      match: `enforcedClasses의 ${gap} 부류를 커버하는 활성 class가 없습니다`,
    });
  }

  if (violations.length === 0) return ok(id, label);
  return fail(
    id,
    label,
    violations.map((v) => ({ ref: '§8.3 ⑨', message: `${v.file} — ${v.classId} 부류에 걸리는 값: ${v.match}` }))
  );
}
