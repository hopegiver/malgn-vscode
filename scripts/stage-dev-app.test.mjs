import { describe, expect, it } from 'vitest';
import { buildStagedPackageJson } from './stage-dev-app.mjs';

describe('buildStagedPackageJson', () => {
  it('루트 package.json의 version을 그대로 옮기고 main을 지정한다', () => {
    const staged = buildStagedPackageJson({ version: '0.1.0' }, 'entry.dev.cjs');
    expect(staged).toEqual({ name: 'malgn-dev-app', version: '0.1.0', private: true, main: 'entry.dev.cjs' });
  });

  it('entryFile을 바꾸면 main도 바뀐다(entry.prod.cjs 스테이징용)', () => {
    const staged = buildStagedPackageJson({ version: '0.1.0' }, 'entry.prod.cjs');
    expect(staged.main).toBe('entry.prod.cjs');
  });
});
