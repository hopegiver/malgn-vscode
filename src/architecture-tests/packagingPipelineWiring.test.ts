// NT-R21 패키징 파이프라인 배선 검사 테스트.
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { checkPackagingPipelineWiring } from './packagingPipelineWiring.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_REPO_ROOT = join(HERE, '..', '..');

describe('실제 저장소 대조 — package.json이 실제로 배선되어 있는지', () => {
  it('scripts.package가 assert-shipped-media-scan.mjs를 호출한다(위반 0건)', () => {
    const result = checkPackagingPipelineWiring(REAL_REPO_ROOT);
    expect(result.violations).toEqual([]);
  });
});

describe('fixture 위반 주입 — 배선이 지워졌다고 가정', () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-pkg-wiring-'));
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('package 스크립트가 build만 하고 media-scan을 부르지 않으면 위반을 잡는다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: { package: 'pnpm run build' } }), 'utf8');
    const result = checkPackagingPipelineWiring(root);
    expect(result.violations.some((v) => v.includes('NT-R21'))).toBe(true);
  });

  it('package 스크립트 자체가 없으면 위반을 잡는다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: {} }), 'utf8');
    const result = checkPackagingPipelineWiring(root);
    expect(result.violations.some((v) => v.includes('NT-R21'))).toBe(true);
  });

  it('package 스크립트가 media-scan을 호출하면 위반이 없다', async () => {
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({ scripts: { package: 'pnpm run build && node scripts/assert-shipped-media-scan.mjs' } }),
      'utf8'
    );
    const result = checkPackagingPipelineWiring(root);
    expect(result.violations).toEqual([]);
  });
});
