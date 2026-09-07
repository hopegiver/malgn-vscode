// AT-U1~U5 — architecture.md §3.6.1 "아키텍처 테스트 규격 (A-30 · 자동업데이트 코드를
// 쓰기 *전에* 존재해야 한다)". 두 계층으로 검증한다:
//  ① 실제 저장소 대조 — W-N4-b가 `src/update/**`를 실제로 채운 지금, 그 실물 코드가
//     각 규칙을 위반하지 않는지 확인한다("대상 없음"으로 건너뛰던 이전 상태의 결과가
//     아니라 실제 스캔 결과가 위반 0건임을 본다 — AT-U1·U2·U3은 이제 `skippedNoTarget:
//     false`가 정상이고, AT-U4는 `updateTrigger.ts` 한 파일이 대상이 되어 역시
//     `skippedNoTarget: false`로 위반 0건이 정상이다. AT-U5는 정의 지점이 1개가 된
//     지금도 위반이 아니다 — 위반은 "2개 이상"부터다).
//  ② fixture 위반 주입 — 임시 디렉터리에 각 규칙을 실제로 어기는 최소 코드를 만들어
//     각 검사 함수가 그 위반을 실제로 잡는지, 그리고 정상 형태는 잡지 않는지 확인한다
//     (완료판정 #4 "위반을 주입해 확인"을 실제 소스 트리를 건드리지 않고 재현한다).

import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  UPDATE_AUTHORITY_IDENTIFIER,
  checkAuthoritySingleDefinition,
  checkForbiddenIdentifiers,
  checkModuleBoundaryBidirectional,
  checkTriggerInputEnumeration,
  checkTypeReachability,
} from './updateChannelBoundary.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_SRC_ROOT = join(HERE, '..');
const REAL_UPDATE_DIR = join(REAL_SRC_ROOT, 'update');
const REAL_POLICY_DIR = join(REAL_SRC_ROOT, 'core', 'policy');

describe('실제 저장소 대조 — W-N4-b가 채운 src/update/** 실물 코드', () => {
  it('AT-U1: 실제 update 모듈은 정책 모듈 디렉터리를 import하지 않고, 그 역도 성립한다', () => {
    const result = checkModuleBoundaryBidirectional(REAL_UPDATE_DIR, REAL_POLICY_DIR);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations).toEqual([]);
  });

  it('AT-U2: 실제 update 모듈 어디에도 정책 파생 타입 식별자가 등장하지 않는다', () => {
    const result = checkTypeReachability(REAL_UPDATE_DIR);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations).toEqual([]);
  });

  it('AT-U3: 실제 update 모듈 전체(주석 포함)에 금지 식별자가 0회 등장한다', () => {
    const result = checkForbiddenIdentifiers(REAL_UPDATE_DIR);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations).toEqual([]);
  });

  it('AT-U4: 실제 트리거 유니온(updateTrigger.ts)의 리터럴 태그가 허용된 3종을 벗어나지 않는다', () => {
    const result = checkTriggerInputEnumeration(REAL_UPDATE_DIR);
    expect(result.skippedNoTarget).toBe(false);
    expect(result.violations).toEqual([]);
  });

  it('AT-U5: authority 정의 지점이 저장소 전체에서 정확히 1개다(updateServerAuthority.ts) — 위반은 2개부터다', () => {
    const result = checkAuthoritySingleDefinition(REAL_SRC_ROOT);
    expect(result.violations).toEqual([]);
  });
});

describe('fixture 위반 주입 — 각 AT-U 검사가 실제로 위반을 잡는지', () => {
  let root: string;
  let updateDir: string;
  let policyDir: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'malgn-update-boundary-'));
    updateDir = join(root, 'update');
    policyDir = join(root, 'core', 'policy');
    await mkdir(updateDir, { recursive: true });
    await mkdir(policyDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  describe('AT-U1 — 모듈 경계 양방향 금지', () => {
    it('update가 policy를 import하면 위반을 잡는다', async () => {
      await writeFile(
        join(updateDir, 'trigger.ts'),
        `import { loadCodeConstants } from '../core/policy/codeConstants.js';\nexport function run() { return loadCodeConstants(); }\n`,
        'utf8'
      );
      const result = checkModuleBoundaryBidirectional(updateDir, policyDir);
      expect(result.violations.length).toBeGreaterThan(0);
      expect(result.violations[0]).toMatch(/AT-U1/);
    });

    it('policy가 update를 import하면 위반을 잡는다(역방향)', async () => {
      await writeFile(join(updateDir, 'authority.ts'), `export const X = 1;\n`, 'utf8');
      await writeFile(
        join(policyDir, 'loader.ts'),
        `import { X } from '../../update/authority.js';\nexport const y = X;\n`,
        'utf8'
      );
      const result = checkModuleBoundaryBidirectional(updateDir, policyDir);
      expect(result.violations.some((v) => v.includes('AT-U1'))).toBe(true);
    });

    it('서로 무관한 import만 있으면 위반이 없다(정상 형태)', async () => {
      await writeFile(
        join(updateDir, 'trigger.ts'),
        `import { createHash } from 'node:crypto';\nexport function run() { return createHash('sha256'); }\n`,
        'utf8'
      );
      const result = checkModuleBoundaryBidirectional(updateDir, policyDir);
      expect(result.violations).toEqual([]);
      expect(result.skippedNoTarget).toBe(false);
    });
  });

  describe('AT-U2 — 타입 도달 불가', () => {
    it('EffectivePolicy가 update 파일에 등장하면 위반을 잡는다', async () => {
      await writeFile(
        join(updateDir, 'manifest.ts'),
        `import type { EffectivePolicy } from '../core/policy/types.js';\nexport function apply(p: EffectivePolicy) { return p; }\n`,
        'utf8'
      );
      const result = checkTypeReachability(updateDir);
      expect(result.violations.some((v) => v.includes('EffectivePolicy'))).toBe(true);
    });

    it('정책과 무관한 타입만 쓰면 위반이 없다', async () => {
      await writeFile(join(updateDir, 'manifest.ts'), `export interface UpdateManifest { readonly version: string; }\n`, 'utf8');
      const result = checkTypeReachability(updateDir);
      expect(result.violations).toEqual([]);
    });
  });

  describe('AT-U3 — 식별자 부재(주석 포함)', () => {
    it('코드에 killSwitch 식별자가 등장하면 위반을 잡는다', async () => {
      await writeFile(join(updateDir, 'x.ts'), `export function check(killSwitch: unknown) { return killSwitch; }\n`, 'utf8');
      const result = checkForbiddenIdentifiers(updateDir);
      expect(result.violations.some((v) => v.includes('killSwitch'))).toBe(true);
    });

    it('주석에만 policy가 등장해도 위반을 잡는다(주석 제외하지 않음)', async () => {
      await writeFile(join(updateDir, 'x.ts'), `// TODO: 나중에 policy와 연결할 수도?\nexport const y = 1;\n`, 'utf8');
      const result = checkForbiddenIdentifiers(updateDir);
      expect(result.violations.some((v) => v.includes('policy'))).toBe(true);
    });

    it('금지 식별자가 전혀 없으면 위반이 없다', async () => {
      await writeFile(join(updateDir, 'x.ts'), `export const version = '1.0.0';\n`, 'utf8');
      const result = checkForbiddenIdentifiers(updateDir);
      expect(result.violations).toEqual([]);
    });
  });

  describe('AT-U4 — 트리거 입력 전수 열거', () => {
    it('허용되지 않은 4번째 트리거 종류가 있으면 위반을 잡는다', async () => {
      await writeFile(
        join(updateDir, 'trigger.ts'),
        `export type UpdateTrigger =\n  | { kind: 'timer' }\n  | { kind: 'user-command' }\n  | { kind: 'verified-manifest' }\n  | { kind: 'remote-push' };\n`,
        'utf8'
      );
      const result = checkTriggerInputEnumeration(updateDir);
      expect(result.violations.some((v) => v.includes('remote-push'))).toBe(true);
    });

    it('허용된 3종만 있으면 위반이 없다', async () => {
      await writeFile(
        join(updateDir, 'trigger.ts'),
        `export type UpdateTrigger =\n  | { kind: 'timer' }\n  | { kind: 'user-command' }\n  | { kind: 'verified-manifest' };\n`,
        'utf8'
      );
      const result = checkTriggerInputEnumeration(updateDir);
      expect(result.violations).toEqual([]);
    });

    it('트리거 관련 파일이 전혀 없으면 "대상 없음"으로 통과한다', async () => {
      await writeFile(join(updateDir, 'other.ts'), `export const y = 1;\n`, 'utf8');
      const result = checkTriggerInputEnumeration(updateDir);
      expect(result.skippedNoTarget).toBe(true);
    });
  });

  describe('AT-U5 — authority 단일 정의', () => {
    it('정의가 2곳이면 위반을 잡는다', async () => {
      await writeFile(join(updateDir, 'a.ts'), `export const ${UPDATE_AUTHORITY_IDENTIFIER} = 'updates.example.com';\n`, 'utf8');
      await writeFile(join(updateDir, 'b.ts'), `export const ${UPDATE_AUTHORITY_IDENTIFIER} = 'evil.example.com';\n`, 'utf8');
      const result = checkAuthoritySingleDefinition(root);
      expect(result.violations.some((v) => v.includes('AT-U5'))).toBe(true);
    });

    it('정의가 정확히 1곳이면 위반이 없다', async () => {
      await writeFile(join(updateDir, 'a.ts'), `export const ${UPDATE_AUTHORITY_IDENTIFIER} = 'updates.example.com';\n`, 'utf8');
      const result = checkAuthoritySingleDefinition(root);
      expect(result.violations).toEqual([]);
    });

    it('환경변수 유래로 보이는 패턴이 있으면 위반을 잡는다(개발용 오버라이드 금지)', async () => {
      await writeFile(
        join(updateDir, 'a.ts'),
        `export const ${UPDATE_AUTHORITY_IDENTIFIER} = process.env.UPDATE_AUTHORITY_OVERRIDE ?? 'updates.example.com';\n`,
        'utf8'
      );
      const result = checkAuthoritySingleDefinition(root);
      expect(result.violations.some((v) => v.includes('환경변수'))).toBe(true);
    });

    it('재대입(export const가 아닌 대입)이 있으면 위반을 잡는다', async () => {
      await writeFile(join(updateDir, 'a.ts'), `export const ${UPDATE_AUTHORITY_IDENTIFIER} = 'updates.example.com';\n`, 'utf8');
      await writeFile(join(updateDir, 'b.ts'), `import './a.js';\nlet x = 1;\n// @ts-expect-error 테스트용 재대입 흉내\n${UPDATE_AUTHORITY_IDENTIFIER} = 'attacker.example.com';\n`, 'utf8');
      const result = checkAuthoritySingleDefinition(root);
      expect(result.violations.some((v) => v.includes('재대입'))).toBe(true);
    });
  });
});
