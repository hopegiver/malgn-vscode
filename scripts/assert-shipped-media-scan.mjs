#!/usr/bin/env node
// NT-R21 (docs/release-gates.md §7.6.6) — 출하되는 매체(추적 트리 밖 빌드 산출물)에
// 대한 민감값 스캔의 **fail-closed 집행 지점**. `src/compat-check/checks/
// check9-sensitiveValueScan.ts`의 매체 스캔 확장이 `pnpm compat:check` 안에서는
// best-effort(대상이 없으면 skip)인 것과 달리, 이 스크립트는 **패키징 파이프라인**
// (`pnpm run package`, 빌드 다음)에 배선되어 있어 그 시점에는 아티팩트가 반드시
// 있어야 한다 — 없으면 그 자체가 실패다(§7.5.1 M-40과 같은 원칙: 게이트를 라이프
// 사이클 훅 이름이 아니라 "산출물이 실제로 나가는 자리"에 건다).

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadClassesConfig, scanShippedMedium } from './lib/sensitiveScan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

/** 오늘 출하되는 매체는 이 하나뿐이다(`pnpm run compile`의 유일한 산출물). 네이티브
 * 패키징(Electron 등)이 도입되면 이 목록에 그 실행 파일들을 추가한다 — 목록이
 * 실제 출하물과 어긋나지 않는지는 이 스크립트가 패키징 파이프라인 안에 있다는
 * 사실 자체가 강제한다(빠뜨리면 그 아티팩트가 서명되지 않은 채 나가는 것과 같은
 * 종류의 "이름에 의존하는" 실수이므로, 새 패키징 산출물을 추가할 때 이 배열도
 * 함께 갱신해야 한다는 점을 여기 명시해 둔다). */
export function getShippedMediaRelPaths() {
  return [join('dist', 'extension.cjs')];
}

export function assertShippedMediaScan(root) {
  const sensitiveClassesJsonPath = join(root, 'compat', 'sensitive-classes.json');
  const config = loadClassesConfig(readFileSync(sensitiveClassesJsonPath, 'utf8'));

  const violations = [];
  for (const relPath of getShippedMediaRelPaths()) {
    const fullPath = join(root, relPath);
    if (!existsSync(fullPath)) {
      // best-effort가 아니라 진짜 fail-closed다 — 패키징 시점에는 이 아티팩트가
      // 반드시 있어야 한다. 없다는 것은 빌드 순서가 깨졌거나 이 목록이 실제
      // 산출물과 어긋났다는 뜻이고, 둘 다 조용히 넘길 이유가 없다.
      violations.push(`출하 매체가 없습니다(빌드 순서 확인 필요): ${relPath}`);
      continue;
    }
    const { violations: mediaViolations } = scanShippedMedium(relPath, readFileSync(fullPath), config);
    violations.push(...mediaViolations.map((v) => `${v.file} — ${v.classId} 부류에 걸리는 값: ${v.match}`));
  }
  return violations;
}

export function main(rootOverride) {
  const root = rootOverride ?? process.env.MALGN_GEN_SITE_ROOT ?? DEFAULT_ROOT;
  const violations = assertShippedMediaScan(root);
  if (violations.length > 0) {
    throw new Error(`NT-R21: 출하 매체 민감값 스캔 실패(fail-closed) —\n${violations.map((v) => `  - ${v}`).join('\n')}`);
  }
  return { ok: true, scanned: getShippedMediaRelPaths() };
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  try {
    const result = main();
    console.log(`[assert-shipped-media-scan] OK — ${result.scanned.join(', ')}`);
  } catch (error) {
    console.error(`[assert-shipped-media-scan] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
