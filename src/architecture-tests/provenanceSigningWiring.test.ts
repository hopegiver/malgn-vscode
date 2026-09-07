// NT-R22 서명 배선 구조 검사 테스트 — docs/release-gates.md §7.6.6.
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { checkProvenanceSigningWiring } from './provenanceSigningWiring.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_REPO_ROOT = join(HERE, '..', '..');

describe('실제 저장소 대조 — K-3(업데이트 서명 키)가 아직 없는 이 슬라이스 상태', () => {
  it('신호가 0개라 "대상 없음"으로 통과한다(오늘의 정상 상태)', () => {
    const result = checkProvenanceSigningWiring(REAL_REPO_ROOT);
    expect(result.skippedNoTarget).toBe(true);
    expect(result.violations).toEqual([]);
  });
});

describe('fixture 위반 주입 — K-3가 생겼다고 가정', () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-provenance-wiring-'));
    await mkdir(join(root, 'src', 'core', 'provenance'), { recursive: true });
    await mkdir(join(root, 'scripts'), { recursive: true });
    await writeFile(join(root, 'package.json'), JSON.stringify({ scripts: {} }), 'utf8');
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('서명 라이브러리 의존성만 있어도 신호로 잡아 배선을 요구한다', async () => {
    await writeFile(join(root, 'package.json'), JSON.stringify({ dependencies: { tweetnacl: '^1.0.3' }, scripts: {} }), 'utf8');
    await writeFile(join(root, 'src', 'core', 'provenance', 'buildProvenance.ts'), `export const signature = null;\n`, 'utf8');
    const result = checkProvenanceSigningWiring(root);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations.some((v) => v.includes('tweetnacl'))).toBe(true);
  });

  it('K-3 서명 호출 흔적이 코드에 등장하는데 buildProvenance.ts가 여전히 null이면 위반을 잡는다', async () => {
    await writeFile(
      join(root, 'scripts', 'sign-provenance.mjs'),
      `export function signProvenance(record, privateKey) { return record; }\n`,
      'utf8'
    );
    await writeFile(join(root, 'src', 'core', 'provenance', 'buildProvenance.ts'), `export const signature = null;\n`, 'utf8');
    const result = checkProvenanceSigningWiring(root);
    expect(result.violations.some((v) => v.includes('NT-R22'))).toBe(true);
  });

  it('신호가 있고 buildProvenance.ts가 null이 아닌 signature를 만들면(배선됨) 위반이 없다', async () => {
    await writeFile(
      join(root, 'scripts', 'sign-provenance.mjs'),
      `export function signProvenance(record, privateKey) { return record; }\n`,
      'utf8'
    );
    await writeFile(
      join(root, 'src', 'core', 'provenance', 'buildProvenance.ts'),
      `export function build(sig) { return { signature: sig }; }\n`,
      'utf8'
    );
    const result = checkProvenanceSigningWiring(root);
    expect(result.violations).toEqual([]);
  });

  it('신호가 전혀 없으면 "대상 없음"으로 통과한다', async () => {
    await writeFile(join(root, 'src', 'core', 'provenance', 'buildProvenance.ts'), `export const signature = null;\n`, 'utf8');
    const result = checkProvenanceSigningWiring(root);
    expect(result.skippedNoTarget).toBe(true);
    expect(result.violations).toEqual([]);
  });
});
