import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { loadEffectivePolicyWithFallback, resolveCheckoutMtimeIfApplicable } from './loadWithFallback.js';

const CURRENT_VERSION = '0.1.0';
const CLAUDE_HOME = '/opt/fixture-home/.claude';
const CHECKOUT_INSTALL_LOCATION = '/opt/fixture-home/.claude/marketplaces/malgnsoft-plugins';
const CHECKOUT_POLICY_PATH = join(CHECKOUT_INSTALL_LOCATION, 'malgn-agent', 'workstation-profile.json');
const INSTALLED_PATH = '/opt/fixture-home/.claude/plugins/malgn-agent';
const INSTALLED_POLICY_PATH = join(INSTALLED_PATH, 'workstation-profile.json');
const KNOWN_MARKETPLACES_PATH = join(CLAUDE_HOME, 'plugins', 'known_marketplaces.json');
const INSTALLED_PLUGINS_PATH = join(CLAUDE_HOME, 'plugins', 'installed_plugins.json');

function makeFileMap(files: Record<string, string>) {
  return async (path: string): Promise<string> => {
    if (path in files) return files[path]!;
    const err = new Error('ENOENT') as NodeJS.ErrnoException;
    err.code = 'ENOENT';
    throw err;
  };
}

describe('loadEffectivePolicyWithFallback — §3.7.3 3순위 폴백', () => {
  it('체크아웃이 유효하면 체크아웃을 채택한다', async () => {
    const readTextFile = makeFileMap({
      [KNOWN_MARKETPLACES_PATH]: JSON.stringify({ 'malgnsoft-plugins': { installLocation: CHECKOUT_INSTALL_LOCATION } }),
      [CHECKOUT_POLICY_PATH]: JSON.stringify({ schemaVersion: 1 }),
    });
    const outcome = await loadEffectivePolicyWithFallback({ claudeHomeDir: CLAUDE_HOME, readTextFile, currentExtensionVersion: CURRENT_VERSION });
    expect(outcome.source).toBe('checkout');
    expect(outcome.sourcePath).toBe(CHECKOUT_POLICY_PATH);
    expect(outcome.result.status).toBe('ok');
    expect(outcome.skippedSources).toEqual([]);
  });

  it('체크아웃이 없고 설치본이 유효하면 설치본을 채택하고 체크아웃 실패를 skippedSources에 남긴다', async () => {
    const readTextFile = makeFileMap({
      [INSTALLED_PLUGINS_PATH]: JSON.stringify({ 'malgn-agent@malgnsoft-plugins': { user: { installPath: INSTALLED_PATH } } }),
      [INSTALLED_POLICY_PATH]: JSON.stringify({ schemaVersion: 1 }),
    });
    const outcome = await loadEffectivePolicyWithFallback({ claudeHomeDir: CLAUDE_HOME, readTextFile, currentExtensionVersion: CURRENT_VERSION });
    expect(outcome.source).toBe('installed');
    expect(outcome.sourcePath).toBe(INSTALLED_POLICY_PATH);
    expect(outcome.skippedSources).toHaveLength(1);
    expect(outcome.skippedSources[0]!.source).toBe('checkout');
  });

  it('체크아웃 경로는 있지만 정책 파일 자체가 없으면 다음 순위로 폴백한다', async () => {
    const readTextFile = makeFileMap({
      [KNOWN_MARKETPLACES_PATH]: JSON.stringify({ 'malgnsoft-plugins': { installLocation: CHECKOUT_INSTALL_LOCATION } }),
      // CHECKOUT_POLICY_PATH 자체는 없음
      [INSTALLED_PLUGINS_PATH]: JSON.stringify({ 'malgn-agent@malgnsoft-plugins': { user: { installPath: INSTALLED_PATH } } }),
      [INSTALLED_POLICY_PATH]: JSON.stringify({ schemaVersion: 1 }),
    });
    const outcome = await loadEffectivePolicyWithFallback({ claudeHomeDir: CLAUDE_HOME, readTextFile, currentExtensionVersion: CURRENT_VERSION });
    expect(outcome.source).toBe('installed');
  });

  it('아무 파일도 없으면 번들 내장(3순위)로 떨어지고, 이 순위는 항상 성공한다', async () => {
    const readTextFile = makeFileMap({});
    const outcome = await loadEffectivePolicyWithFallback({ claudeHomeDir: CLAUDE_HOME, readTextFile, currentExtensionVersion: CURRENT_VERSION });
    expect(outcome.source).toBe('bundled');
    expect(outcome.sourcePath).toBeNull();
    expect(outcome.result.status).toBe('ok');
    expect(outcome.skippedSources).toHaveLength(2);
  });

  it('체크아웃 정책 파일이 스키마 위반(파싱 실패)이면 부분 적용 없이 다음 순위로 넘어간다', async () => {
    const readTextFile = makeFileMap({
      [KNOWN_MARKETPLACES_PATH]: JSON.stringify({ 'malgnsoft-plugins': { installLocation: CHECKOUT_INSTALL_LOCATION } }),
      [CHECKOUT_POLICY_PATH]: 'not json',
      [INSTALLED_PLUGINS_PATH]: JSON.stringify({ 'malgn-agent@malgnsoft-plugins': { user: { installPath: INSTALLED_PATH } } }),
      [INSTALLED_POLICY_PATH]: JSON.stringify({ schemaVersion: 1 }),
    });
    const outcome = await loadEffectivePolicyWithFallback({ claudeHomeDir: CLAUDE_HOME, readTextFile, currentExtensionVersion: CURRENT_VERSION });
    expect(outcome.source).toBe('installed');
    expect(outcome.skippedSources[0]!.reason).toMatch(/MV_POLICY_MALFORMED/);
  });
});

describe('resolveCheckoutMtimeIfApplicable — N-2는 체크아웃일 때만 의미가 있다', () => {
  it('source가 checkout이 아니면 statMtime을 호출하지 않고 null을 반환한다', async () => {
    let called = false;
    const statMtime = async () => {
      called = true;
      return new Date();
    };
    const result = await resolveCheckoutMtimeIfApplicable({ source: 'bundled', sourcePath: null }, statMtime);
    expect(result).toBeNull();
    expect(called).toBe(false);
  });

  it('source가 checkout이면 sourcePath로 statMtime을 호출한다', async () => {
    const statMtime = async (path: string) => {
      expect(path).toBe(CHECKOUT_POLICY_PATH);
      return new Date('2026-01-01T00:00:00.000Z');
    };
    const result = await resolveCheckoutMtimeIfApplicable({ source: 'checkout', sourcePath: CHECKOUT_POLICY_PATH }, statMtime);
    expect(result?.toISOString()).toBe('2026-01-01T00:00:00.000Z');
  });
});
