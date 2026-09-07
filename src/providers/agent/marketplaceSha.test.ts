import { describe, expect, it } from 'vitest';
import { readMarketplaceCheckoutHead, shaMismatch } from './marketplaceSha.js';
import type { ExecFileFn } from '../../platform/exec.js';

function fakeExec(stdout: string, error: { code?: number } | null = null): ExecFileFn {
  return (_file, _args, _options, callback) => {
    callback(error as never, stdout, '');
  };
}

describe('readMarketplaceCheckoutHead', () => {
  it('40자 hex를 sha로 인식한다', async () => {
    const sha = 'a'.repeat(40);
    const probe = await readMarketplaceCheckoutHead('/loc', fakeExec(`${sha}\n`), {});
    expect(probe).toEqual({ kind: 'sha', sha });
  });

  it('git 실행 실패면 unavailable', async () => {
    const probe = await readMarketplaceCheckoutHead('/loc', fakeExec('', { code: 128 }), {});
    expect(probe).toEqual({ kind: 'unavailable' });
  });

  it('출력이 sha 형태가 아니면 unavailable', async () => {
    const probe = await readMarketplaceCheckoutHead('/loc', fakeExec('not a sha\n'), {});
    expect(probe).toEqual({ kind: 'unavailable' });
  });
});

describe('shaMismatch', () => {
  it('둘 다 sha이고 다르면 true', () => {
    expect(shaMismatch({ kind: 'sha', sha: 'a' }, { kind: 'sha', sha: 'b' })).toBe(true);
  });

  it('둘 다 sha이고 같으면 false', () => {
    expect(shaMismatch({ kind: 'sha', sha: 'a' }, { kind: 'sha', sha: 'a' })).toBe(false);
  });

  it('한쪽이라도 unavailable이면 false(확인 못 함 ≠ 불일치)', () => {
    expect(shaMismatch({ kind: 'unavailable' }, { kind: 'sha', sha: 'a' })).toBe(false);
    expect(shaMismatch({ kind: 'sha', sha: 'a' }, { kind: 'unavailable' })).toBe(false);
  });
});
