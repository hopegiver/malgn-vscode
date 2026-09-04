import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { deriveInstallEnvSet } from './installEnvSet.js';

describe('deriveInstallEnvSet — R-19 "적용 집합"의 유일한 정본(loadCodeConstants().installEnv)', () => {
  it('compat/install-env.json(policy-contract.md §3 fixture)과 정확히 같은 키/값을 반환한다', () => {
    const fixture = JSON.parse(readFileSync(new URL('../../../compat/install-env.json', import.meta.url), 'utf8'));
    expect(deriveInstallEnvSet()).toEqual(fixture);
  });

  it('§3이 요구하는 5개 HOMEBREW_* 가드가 전부 "1"이다', () => {
    const envSet = deriveInstallEnvSet();
    for (const key of [
      'HOMEBREW_NO_INSTALL_UPGRADE',
      'HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK',
      'HOMEBREW_NO_INSTALL_CLEANUP',
      'HOMEBREW_NO_ENV_HINTS',
      'HOMEBREW_NO_ANALYTICS',
    ]) {
      expect(envSet[key]).toBe('1');
    }
  });

  it('SUDO_ASKPASS·HOMEBREW_NO_AUTO_UPDATE는 이 집합에 없다(§3 — 상속 금지/의도적 미설정)', () => {
    const envSet = deriveInstallEnvSet();
    expect(envSet).not.toHaveProperty('SUDO_ASKPASS');
    expect(envSet).not.toHaveProperty('HOMEBREW_NO_AUTO_UPDATE');
  });
});
