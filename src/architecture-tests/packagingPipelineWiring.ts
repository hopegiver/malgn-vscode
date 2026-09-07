// NT-R21 패키징 파이프라인 배선 검사 — package.json의 `package` 스크립트가 실제로
// `scripts/assert-shipped-media-scan.mjs`를 부르는지 구조적으로 확인한다.
//
// [이 검사가 존재하는 이유] `scripts/assert-shipped-media-scan.test.mjs`는 그 스크립트
// 자체의 로직(민감값을 실제로 잡는지)을 증명하지만, **그 스크립트가 패키징 경로에서
// 실제로 호출되는지는 증명하지 않는다** — package.json의 한 줄이 실수로 지워지거나
// `pnpm run build`만 남도록 되돌려져도 스크립트 단위 테스트는 계속 초록이다. 이것이
// 정확히 §7.5.1 M-40이 겪은 실패 모양이다("이름에 의존하면 조용히 안 돈다"). 그래서
// 배선 자체를 별도 테스트로 고정한다.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export interface PackagingWiringResult {
  readonly violations: readonly string[];
}

const REQUIRED_INVOCATIONS: readonly { readonly script: string; readonly mustContain: string; readonly ref: string }[] = [
  { script: 'package', mustContain: 'assert-shipped-media-scan.mjs', ref: 'NT-R21' },
];

interface PackageJsonShape {
  readonly scripts?: Readonly<Record<string, string>>;
}

export function checkPackagingPipelineWiring(repoRoot: string): PackagingWiringResult {
  const pkg = JSON.parse(readFileSync(join(repoRoot, 'package.json'), 'utf8')) as PackageJsonShape;
  const scripts = pkg.scripts ?? {};
  const violations: string[] = [];

  for (const { script, mustContain, ref } of REQUIRED_INVOCATIONS) {
    const cmd = scripts[script];
    if (cmd === undefined) {
      violations.push(`${ref}: package.json scripts.${script}가 없습니다 — 배선을 확인할 수 없습니다`);
      continue;
    }
    if (!cmd.includes(mustContain)) {
      violations.push(
        `${ref}: package.json scripts.${script}("${cmd}")가 '${mustContain}'를 호출하지 않습니다 — ` +
          '출하 매체 민감값 스캔이 패키징 경로에서 빠졌습니다(docs/release-gates.md §7.6.6).'
      );
    }
  }

  return { violations };
}
