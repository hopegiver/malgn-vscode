import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { resolveCheckoutPolicyPath, resolveInstalledPolicyPath } from './checkoutAndInstalledPaths.js';

describe('resolveCheckoutPolicyPath', () => {
  it('known_marketplaces.json에서 installLocation을 찾아 malgn-agent/workstation-profile.json 경로를 만든다', async () => {
    const fakeRead = async (path: string) => {
      expect(path).toBe(join('/opt/fixture-home/.claude', 'plugins', 'known_marketplaces.json'));
      return JSON.stringify({ 'malgnsoft-plugins': { installLocation: '/opt/fixture-home/.claude/marketplaces/malgnsoft-plugins' } });
    };
    const result = await resolveCheckoutPolicyPath('/opt/fixture-home/.claude', fakeRead);
    expect(result).toBe(join('/opt/fixture-home/.claude/marketplaces/malgnsoft-plugins', 'malgn-agent', 'workstation-profile.json'));
  });

  it('파일이 없으면(읽기 실패) null이다', async () => {
    const fakeRead = async () => {
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    };
    expect(await resolveCheckoutPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });

  it('JSON 파싱 실패면 null이다', async () => {
    const fakeRead = async () => 'not json';
    expect(await resolveCheckoutPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });

  it('마켓플레이스 항목이 없으면 null이다', async () => {
    const fakeRead = async () => JSON.stringify({ 'other-marketplace': { installLocation: '/x' } });
    expect(await resolveCheckoutPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });

  it('installLocation이 문자열이 아니면 null이다', async () => {
    const fakeRead = async () => JSON.stringify({ 'malgnsoft-plugins': { installLocation: 123 } });
    expect(await resolveCheckoutPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });
});

describe('resolveInstalledPolicyPath', () => {
  it('installed_plugins.json에서 user scope installPath를 찾아 workstation-profile.json 경로를 만든다', async () => {
    const fakeRead = async (path: string) => {
      expect(path).toBe(join('/opt/fixture-home/.claude', 'plugins', 'installed_plugins.json'));
      return JSON.stringify({
        'malgn-agent@malgnsoft-plugins': { user: { installPath: '/opt/fixture-home/.claude/plugins/malgn-agent', version: '1.8.24' } },
      });
    };
    const result = await resolveInstalledPolicyPath('/opt/fixture-home/.claude', fakeRead);
    expect(result).toBe(join('/opt/fixture-home/.claude/plugins/malgn-agent', 'workstation-profile.json'));
  });

  it('파일이 없으면 null이다', async () => {
    const fakeRead = async () => {
      throw new Error('ENOENT');
    };
    expect(await resolveInstalledPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });

  it('플러그인 식별자가 없으면 null이다', async () => {
    const fakeRead = async () => JSON.stringify({});
    expect(await resolveInstalledPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });

  it('user scope가 없으면 null이다', async () => {
    const fakeRead = async () => JSON.stringify({ 'malgn-agent@malgnsoft-plugins': {} });
    expect(await resolveInstalledPolicyPath('/opt/fixture-home/.claude', fakeRead)).toBeNull();
  });
});
