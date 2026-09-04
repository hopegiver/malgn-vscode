// 검사 ⑧(HRS-E) 회귀 테스트 4종 — policy-contract.md §6 완료 조건: "회귀 테스트 4종
// (증거 없음 / 키 불일치 / 하한 미달 / 미래 타임스탬프) 동반이 완료 조건이다."
//
// 전부 인메모리로 동작한다 — `compat/verification-evidence/`에 실제 파일을 만들지
// 않는다(작업 지시 금지 사항: "증거 파일을 실제로 만들지 마십시오"). `evaluateHrsEvidence`가
// `loadEvidence`를 주입받는 순수 함수라 가능하다.
//
// `src/compat-check/**`는 메인 `vitest.config.ts`의 exclude 대상이라(compat:check
// 단계와 vitest 단계를 CI 파이프라인에서 분리하기 위해서 — tech-stack.md §5.4 원문
// "install → lint → tsc → vitest → compat:check → …") 이 파일은 `pnpm test`가 아니라
// `pnpm compat:check`(`vitest.compat.config.ts`)로 실행된다.

import { describe, expect, it } from 'vitest';
import type { InstallTargetRow } from '../../core/policy/types.js';
import { evaluateHrsEvidence, type VerificationEvidence } from './check8-hrsEvidence.js';

const VERIFIED_ROW: InstallTargetRow = {
  tool: 'claude',
  platform: 'darwin',
  manager: 'brew',
  subcommand: 'install --cask',
  packageId: 'claude-code',
  artifactKind: 'Binary',
  expectedSigner: 'Anthropic PBC',
  verified: true,
  strategy: 'PkgManagerStrategy',
};

const CONSTANTS = { requires: { claudeCode: '>=2.1.237', malgnAgent: '>=1.8.24 <2.0.0', manifestSchema: { min: 1, max: 1 } } };
const COMMIT_TIMESTAMP = new Date('2026-09-03T00:00:00Z');

function validEvidence(overrides: Partial<VerificationEvidence> = {}): VerificationEvidence {
  return {
    tool: 'claude',
    platform: 'darwin',
    manager: 'brew',
    managerPath: '/opt/homebrew/bin/brew',
    resolvedVersion: '2.1.240',
    codesignOutput: 'valid; Anthropic PBC',
    installOrigin: 'brew',
    exitCodes: [0],
    startedAt: '2026-09-01T00:00:00Z',
    finishedAt: '2026-09-01T00:05:00Z',
    extensionCommit: 'abc123',
    ...overrides,
  };
}

const GH_ROW: InstallTargetRow = {
  tool: 'gh',
  platform: 'darwin',
  manager: 'brew',
  subcommand: 'install',
  packageId: 'gh',
  artifactKind: null,
  expectedSigner: null,
  verified: true,
  strategy: 'PkgManagerStrategy',
};

describe('evaluateHrsEvidence — 정상 경로', () => {
  it('증거가 모든 조건을 만족하면 위반이 없다', () => {
    const { violations, skipped } = evaluateHrsEvidence({
      verifiedRows: [VERIFIED_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence(),
    });
    expect(violations).toEqual([]);
    expect(skipped).toEqual([]); // claude는 하한이 정의돼 있어 (c)를 건너뛰지 않는다
  });

  it('verified:false 행은 애초에 검사 대상이 아니다(빈 집합에서 실패하지 않는다)', () => {
    const { violations, skipped } = evaluateHrsEvidence({
      verifiedRows: [],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => {
        throw new Error('호출되면 안 됩니다');
      },
    });
    expect(violations).toEqual([]);
    expect(skipped).toEqual([]);
  });
});

describe('evaluateHrsEvidence — 회귀 테스트 5종 (§6 완료 조건, B-3로 4→5종 확장)', () => {
  it('1. 증거 없음 — verified:true인데 대응 파일이 없으면 (a) 위반', () => {
    const { violations } = evaluateHrsEvidence({
      verifiedRows: [VERIFIED_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => undefined,
    });
    expect(violations).toHaveLength(1);
    expect(violations[0]?.ref).toBe('HRS-E (a)');
  });

  it('2. 키 불일치 — 증거 파일의 tool/platform/manager가 행과 다르면 (b) 위반', () => {
    const { violations } = evaluateHrsEvidence({
      verifiedRows: [VERIFIED_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence({ manager: 'winget' }), // 행은 brew인데 증거는 winget
    });
    expect(violations).toHaveLength(1);
    expect(violations[0]?.ref).toBe('HRS-E (b)');
  });

  it('3. 하한 미달 — resolvedVersion이 compatibility.json 하한보다 낮으면 (c) 위반', () => {
    const { violations } = evaluateHrsEvidence({
      verifiedRows: [VERIFIED_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence({ resolvedVersion: '2.1.200' }), // 하한 >=2.1.237 미달
    });
    expect(violations).toHaveLength(1);
    expect(violations[0]?.ref).toBe('HRS-E (c)');
  });

  it('4. 미래 타임스탬프 — finishedAt이 커밋 시각 이전이 아니면 (d) 위반', () => {
    const { violations } = evaluateHrsEvidence({
      verifiedRows: [VERIFIED_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence({ finishedAt: '2099-01-01T00:00:00Z' }),
    });
    expect(violations).toHaveLength(1);
    expect(violations[0]?.ref).toBe('HRS-E (d)');
  });

  it('5. (c′) 하한 미정의 tool + 빈 resolvedVersion — (c)를 건너뛰어도 파싱 불가 버전은 위반이다', () => {
    const { violations, skipped } = evaluateHrsEvidence({
      verifiedRows: [GH_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence({ tool: 'gh', manager: 'brew', resolvedVersion: '' }),
    });
    expect(violations).toHaveLength(1);
    expect(violations[0]?.ref).toBe('HRS-E (c′)');
    expect(skipped).toEqual([{ tool: 'gh', reason: '(c) — gh 하한 미정의' }]);
  });
});

describe('evaluateHrsEvidence — tool별 하한 정의 갭(B-3, 매핑표 조회)', () => {
  it('compatibility.json에 하한이 없는 tool(gh)은 (c)를 건너뛰고 나머지 조건만 적용하며 건너뛴 사실을 skipped에 남긴다', () => {
    const { violations, skipped } = evaluateHrsEvidence({
      verifiedRows: [GH_ROW],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: () => validEvidence({ tool: 'gh', manager: 'brew', resolvedVersion: '0.0.1' }),
    });
    expect(violations).toEqual([]);
    expect(skipped).toEqual([{ tool: 'gh', reason: '(c) — gh 하한 미정의' }]);
  });

  it('같은 tool의 여러 verified:true 행이 있어도 skipped는 tool당 1회만 기록된다', () => {
    const ghWinget: InstallTargetRow = { ...GH_ROW, platform: 'win32', manager: 'winget', packageId: 'GitHub.cli' };
    const { skipped } = evaluateHrsEvidence({
      verifiedRows: [GH_ROW, ghWinget],
      constants: CONSTANTS,
      commitTimestamp: COMMIT_TIMESTAMP,
      loadEvidence: (fileName) =>
        validEvidence({
          tool: 'gh',
          manager: fileName.includes('winget') ? 'winget' : 'brew',
          platform: fileName.includes('winget') ? 'win32' : 'darwin',
          resolvedVersion: '2.50.0',
        }),
    });
    expect(skipped).toEqual([{ tool: 'gh', reason: '(c) — gh 하한 미정의' }]);
  });
});
