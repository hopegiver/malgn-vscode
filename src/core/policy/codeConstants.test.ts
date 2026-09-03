import { describe, expect, it } from 'vitest';
import { loadCodeConstants, resolveInstallTarget, validateInstallTargetsGrid } from './codeConstants.js';
import { MV_INSTALL_TARGET_UNVERIFIED } from './errors.js';

describe('loadCodeConstants', () => {
  it('loads all 4 compat/*.json fixtures into a typed CodeConstants', () => {
    const constants = loadCodeConstants();
    expect(constants.requires.claudeCode).toBe('>=2.1.237');
    expect(constants.requires.malgnAgent).toBe('>=1.8.24 <2.0.0');
    expect(constants.allowedMarketplaces).toEqual(['malgnsoft/claude-plugins']);
    expect(constants.allowedPlugins).toEqual(['malgn-agent@malgnsoft-plugins']);
    expect(constants.allowedInstallScopes).toEqual(['user']);
    expect(constants.allowedGithubScopes).toContain('repo');
    expect(constants.allowedGithubScopes).not.toContain('admin:org');
    expect(constants.allowedGithubScopes).not.toContain('delete_repo');
    // pnpm 키는 의도적으로 정의하지 않는다 (policy-contract.md §2.2 M-14 예외)
    expect(constants.allowedManagerPaths.pnpm).toBeUndefined();
    expect(constants.allowedManagerPaths.brew).toContain('/opt/homebrew/bin/brew');
  });

  it('allowedInstallTargets 격자의 모든 행이 verified:false 리터럴이다 (§4.8.6 현재 상태 직역)', () => {
    const constants = loadCodeConstants();
    expect(constants.allowedInstallTargets.length).toBeGreaterThan(0);
    for (const row of constants.allowedInstallTargets) {
      expect(row.verified).toBe(false);
    }
  });
});

// --- PR-11③(행 필수키가 없는 식별자는 존재할 수 없다) 정본 테스트 ---
describe('validateInstallTargetsGrid — 부재는 차단(fail-open 아님)', () => {
  it('verified 키가 아예 없는 행은 격자 밖으로 취급된다(제외)', () => {
    const rows = [
      { tool: 'claude', platform: 'darwin', manager: 'brew', subcommand: 'install --cask', packageId: 'claude-code', strategy: 'PkgManagerStrategy' },
      // verified 키가 없다 — 예전 결함(PR-11③ reviewer M-17)의 정확한 재현
    ];
    const { valid, rejected } = validateInstallTargetsGrid(rows);
    expect(valid).toHaveLength(0);
    expect(rejected).toHaveLength(1);
    expect(rejected[0]!.reason).toMatch(/verified/);
  });

  it('verified가 불리언이 아닌 값(문자열 "true" 등)이면 격자 밖으로 취급된다', () => {
    const rows = [
      {
        tool: 'claude',
        platform: 'darwin',
        manager: 'brew',
        subcommand: 'install --cask',
        packageId: 'claude-code',
        strategy: 'PkgManagerStrategy',
        verified: 'true',
      },
    ];
    const { valid, rejected } = validateInstallTargetsGrid(rows);
    expect(valid).toHaveLength(0);
    expect(rejected).toHaveLength(1);
  });

  it('다른 필수 열(예: manager)이 없는 행도 격자 밖으로 취급된다', () => {
    const rows = [
      { tool: 'claude', platform: 'darwin', subcommand: 'install --cask', packageId: 'claude-code', strategy: 'PkgManagerStrategy', verified: false },
    ];
    const { valid, rejected } = validateInstallTargetsGrid(rows);
    expect(valid).toHaveLength(0);
    expect(rejected[0]!.reason).toMatch(/manager/);
  });

  it('필수 열이 전량 있고 verified가 불리언 리터럴이면 통과한다', () => {
    const rows = [
      { tool: 'claude', platform: 'darwin', manager: 'brew', subcommand: 'install --cask', packageId: 'claude-code', strategy: 'PkgManagerStrategy', verified: false },
    ];
    const { valid, rejected } = validateInstallTargetsGrid(rows);
    expect(valid).toHaveLength(1);
    expect(rejected).toHaveLength(0);
  });
});

describe('resolveInstallTarget', () => {
  it('격자에 없는 (tool, platform) 조합은 not-in-grid다', () => {
    const constants = loadCodeConstants();
    expect(resolveInstallTarget(constants, 'claude', 'linux').status).toBe('not-in-grid');
  });

  it('격자에는 있지만 verified:false인 조합은 unverified + MV_INSTALL_TARGET_UNVERIFIED다', () => {
    const constants = loadCodeConstants();
    const result = resolveInstallTarget(constants, 'claude', 'darwin');
    expect(result.status).toBe('unverified');
    if (result.status === 'unverified') {
      expect(result.code).toBe(MV_INSTALL_TARGET_UNVERIFIED);
    }
  });
});
