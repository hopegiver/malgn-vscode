// build-host.mjs 단위 테스트 — 실제 esbuild 실행(느림·부작용)은 여기서 다시 확인하지
// 않는다(빌드 성공 자체는 `pnpm run build` CI가 매번 실행한다). 이 테스트는 경로 계산
// 순수 함수만 확인한다 — 다른 scripts/*.test.mjs와 동일한 "경로/판정 로직만 단위
// 테스트, 실제 I/O는 스모크 수준"의 관례를 따른다.

import { describe, expect, it } from 'vitest';
import { join } from 'node:path';
import { resolveHostBuildPaths } from './build-host.mjs';

describe('resolveHostBuildPaths', () => {
  it('산출물을 dist/dev-app/host 아래에만 둔다(이미 gitignore된 dist/ 안 — 새 규칙 불필요)', () => {
    const paths = resolveHostBuildPaths('/repo');
    expect(paths.outDir).toBe(join('/repo', 'dist', 'dev-app', 'host'));
    expect(paths.resourcesDest).toBe(join('/repo', 'dist', 'dev-app', 'host', 'resources'));
  });

  it('entry.dev.ts/entry.prod.ts를 src/host/에서 찾는다(entry.dev.ts/entry.prod.ts와 동일 소스)', () => {
    const paths = resolveHostBuildPaths('/repo');
    expect(paths.entryDev).toBe(join('/repo', 'src', 'host', 'entry.dev.ts'));
    expect(paths.entryProd).toBe(join('/repo', 'src', 'host', 'entry.prod.ts'));
  });
});
