import { describe, expect, it } from 'vitest';
import { readKnownMarketplaceEntry, readMarketplaceEntryForPlugin } from './marketplaceReader.js';

function fakeReader(files: Record<string, string>) {
  return async (path: string): Promise<string> => {
    const content = files[path];
    if (content === undefined) throw new Error(`ENOENT: ${path}`);
    return content;
  };
}

describe('readKnownMarketplaceEntry', () => {
  it('실측 형태에서 repo·installLocation을 뽑는다', async () => {
    const reader = fakeReader({
      '/home/runner/.claude/plugins/known_marketplaces.json': JSON.stringify({
        'malgnsoft-plugins': {
          source: { source: 'github', repo: 'malgnsoft/claude-plugins' },
          installLocation: '/home/runner/.claude/plugins/marketplaces/malgnsoft-plugins',
          lastUpdated: '2026-09-07T00:00:00Z',
        },
      }),
    });
    const entry = await readKnownMarketplaceEntry('/home/runner/.claude', 'malgnsoft-plugins', reader);
    expect(entry).toEqual({ repo: 'malgnsoft/claude-plugins', installLocation: '/home/runner/.claude/plugins/marketplaces/malgnsoft-plugins' });
  });

  it('파일이 없으면 null', async () => {
    const entry = await readKnownMarketplaceEntry('/home/runner/.claude', 'malgnsoft-plugins', fakeReader({}));
    expect(entry).toBeNull();
  });

  it('항목이 없으면 null', async () => {
    const reader = fakeReader({ '/home/runner/.claude/plugins/known_marketplaces.json': JSON.stringify({}) });
    expect(await readKnownMarketplaceEntry('/home/runner/.claude', 'malgnsoft-plugins', reader)).toBeNull();
  });

  it('JSON 파싱 실패면 null', async () => {
    const reader = fakeReader({ '/home/runner/.claude/plugins/known_marketplaces.json': '{not json' });
    expect(await readKnownMarketplaceEntry('/home/runner/.claude', 'malgnsoft-plugins', reader)).toBeNull();
  });
});

describe('readMarketplaceEntryForPlugin', () => {
  it('실측 형태에서 이름이 일치하는 plugins[] 원소를 찾는다', async () => {
    const reader = fakeReader({
      '/loc/.claude-plugin/marketplace.json': JSON.stringify({
        name: 'malgnsoft-plugins',
        plugins: [{ name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' }],
      }),
    });
    const entry = await readMarketplaceEntryForPlugin('/loc', 'malgn-agent', reader);
    expect(entry).toEqual({ name: 'malgn-agent', source: './malgn-agent', version: '1.8.33' });
  });

  it('이름이 일치하지 않으면 null', async () => {
    const reader = fakeReader({
      '/loc/.claude-plugin/marketplace.json': JSON.stringify({ plugins: [{ name: 'other', source: './x' }] }),
    });
    expect(await readMarketplaceEntryForPlugin('/loc', 'malgn-agent', reader)).toBeNull();
  });

  it('파일 없음/파싱 실패/plugins 부재는 전부 null', async () => {
    expect(await readMarketplaceEntryForPlugin('/loc', 'malgn-agent', fakeReader({}))).toBeNull();
    expect(
      await readMarketplaceEntryForPlugin('/loc', 'malgn-agent', fakeReader({ '/loc/.claude-plugin/marketplace.json': 'nope' }))
    ).toBeNull();
    expect(
      await readMarketplaceEntryForPlugin(
        '/loc',
        'malgn-agent',
        fakeReader({ '/loc/.claude-plugin/marketplace.json': JSON.stringify({ name: 'x' }) })
      )
    ).toBeNull();
  });
});
