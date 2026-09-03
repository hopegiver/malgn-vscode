import { describe, expect, it } from 'vitest';
import { SiteProfileNotSiteError, assertSiteProfileForApply, assertSiteProfileForPackaging } from './siteProfileGuard.js';

describe('assertSiteProfileForApply — fail-closed 3종의 셋째(런타임)', () => {
  it("siteProfile==='site'면 통과한다(던지지 않는다)", () => {
    expect(() => assertSiteProfileForApply('site')).not.toThrow();
  });

  it("siteProfile==='example'이면 apply를 거부한다", () => {
    expect(() => assertSiteProfileForApply('example')).toThrow(SiteProfileNotSiteError);
    try {
      assertSiteProfileForApply('example');
    } catch (error) {
      expect(error).toBeInstanceOf(SiteProfileNotSiteError);
      if (error instanceof SiteProfileNotSiteError) expect(error.action).toBe('apply');
    }
  });
});

describe('assertSiteProfileForPackaging — fail-closed 3종의 셋째(패키징)', () => {
  it("siteProfile==='site'면 통과한다", () => {
    expect(() => assertSiteProfileForPackaging('site')).not.toThrow();
  });

  it("siteProfile==='example'이면 패키징을 실패시킨다", () => {
    expect(() => assertSiteProfileForPackaging('example')).toThrow(SiteProfileNotSiteError);
  });
});
