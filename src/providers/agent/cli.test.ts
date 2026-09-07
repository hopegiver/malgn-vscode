import { describe, expect, it } from 'vitest';
import {
  buildMarketplaceAddArgv,
  buildPluginEnableArgv,
  buildPluginInstallArgv,
  buildPluginUpdateArgv,
  marketplaceNameFromPluginId,
  parseMarketplaceListJson,
  parsePluginListJson,
  pluginNameOnly,
} from './cli.js';

describe('pluginNameOnly / marketplaceNameFromPluginId', () => {
  it('id@marketplace를 분해한다', () => {
    expect(pluginNameOnly('malgn-agent@malgnsoft-plugins')).toBe('malgn-agent');
    expect(marketplaceNameFromPluginId('malgn-agent@malgnsoft-plugins')).toBe('malgnsoft-plugins');
  });

  it('@가 없으면 원문을 그대로 돌려주거나 null(추측하지 않는다)', () => {
    expect(pluginNameOnly('malgn-agent')).toBe('malgn-agent');
    expect(marketplaceNameFromPluginId('malgn-agent')).toBeNull();
  });
});

describe('argv 빌더 — 전량 상수, 사용자 입력 도달 불가', () => {
  it('install은 acceptDeclaredCommand일 때만 -y를 붙인다', () => {
    expect(buildPluginInstallArgv('a@b', 'user', false).args).not.toContain('-y');
    expect(buildPluginInstallArgv('a@b', 'user', true).args).toContain('-y');
  });

  it('update도 동일하다', () => {
    expect(buildPluginUpdateArgv('a@b', 'user', false).args).not.toContain('-y');
    expect(buildPluginUpdateArgv('a@b', 'user', true).args).toContain('-y');
  });

  it('enable/marketplace add는 -y 개념이 없다(플래그 없음)', () => {
    expect(buildPluginEnableArgv('a@b').args).toEqual(['plugin', 'enable', 'a@b']);
    expect(buildMarketplaceAddArgv('org/repo').args).toEqual(['plugin', 'marketplace', 'add', 'org/repo']);
  });
});

describe('parsePluginListJson — 실측(2.1.252) 형태', () => {
  it('실측 배열을 파싱한다', () => {
    const stdout = JSON.stringify([
      { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: true, installPath: '/x' },
    ]);
    expect(parsePluginListJson(stdout)).toEqual([
      { id: 'malgn-agent@malgnsoft-plugins', version: '1.8.33', scope: 'user', enabled: true, gitCommitSha: undefined },
    ]);
  });

  it('JSON이 아니거나 배열이 아니면 빈 배열(추측하지 않는다)', () => {
    expect(parsePluginListJson('not json')).toEqual([]);
    expect(parsePluginListJson('{}')).toEqual([]);
  });

  it('필수 필드가 없는 원소는 건너뛴다', () => {
    expect(parsePluginListJson(JSON.stringify([{ id: 'x' }]))).toEqual([]);
  });
});

describe('parseMarketplaceListJson — 실측 형태', () => {
  it('실측 배열을 파싱한다', () => {
    const stdout = JSON.stringify([
      { name: 'malgnsoft-plugins', source: 'github', repo: 'malgnsoft/claude-plugins', installLocation: '/loc' },
    ]);
    expect(parseMarketplaceListJson(stdout)).toEqual([
      { name: 'malgnsoft-plugins', source: 'github', repo: 'malgnsoft/claude-plugins', installLocation: '/loc' },
    ]);
  });

  it('파싱 불가는 빈 배열', () => {
    expect(parseMarketplaceListJson('nope')).toEqual([]);
  });
});
