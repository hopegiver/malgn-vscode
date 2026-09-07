// SIGN-R2 CLI 재현 — scripts/verify-codesign-certificate-hash.mjs가 기대/실제 인증서
// 해시를 실제로 대조하고, 불일치·베이스라인 부재 시 0이 아닌 종료 코드로 실패하는지
// 실제 하위 프로세스로 증명한다(docs/release-gates.md §7.6.2).

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import {
  CertificateHashBaselineMissingError,
  CertificateHashMismatchError,
  main,
  parseActualHashArg,
  verifyCertificateHash,
} from './verify-codesign-certificate-hash.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = join(HERE, 'verify-codesign-certificate-hash.mjs');

describe('verifyCertificateHash — 순수 로직', () => {
  it('기대값 null이면 베이스라인 미확립 에러', () => {
    expect(() => verifyCertificateHash(null, 'sha256:actual')).toThrow(CertificateHashBaselineMissingError);
  });

  it('불일치면 CertificateHashMismatchError', () => {
    expect(() => verifyCertificateHash('sha256:expected', 'sha256:actual')).toThrow(CertificateHashMismatchError);
  });

  it('일치하면 통과', () => {
    expect(() => verifyCertificateHash('sha256:same', 'sha256:same')).not.toThrow();
  });
});

describe('parseActualHashArg', () => {
  it('--actual-hash= 인자를 뽑는다', () => {
    expect(parseActualHashArg(['--actual-hash=sha256:abc'])).toBe('sha256:abc');
  });
  it('인자가 없으면 null', () => {
    expect(parseActualHashArg([])).toBeNull();
  });
});

describe('main() — 순수 로직(임시 저장소)', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function repoWith(expectedHash) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-sign-r2-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    writeFileSync(join(dir, 'compat', 'codesign-cert.json'), JSON.stringify({ expectedCertificateHashSha256: expectedHash }), 'utf8');
    return dir;
  }

  it('기대 해시가 null이면(=인증서 미발급) 던진다 — fail-closed', () => {
    const repo = repoWith(null);
    expect(() => main(repo, ['--actual-hash=sha256:zzz'])).toThrow(CertificateHashBaselineMissingError);
  });

  it('기대 해시와 실제 해시가 다르면 던진다', () => {
    const repo = repoWith('sha256:expected');
    expect(() => main(repo, ['--actual-hash=sha256:different'])).toThrow(CertificateHashMismatchError);
  });

  it('기대 해시와 실제 해시가 같으면 통과한다', () => {
    const repo = repoWith('sha256:match');
    expect(main(repo, ['--actual-hash=sha256:match'])).toEqual({ expected: 'sha256:match', actual: 'sha256:match' });
  });

  it('--actual-hash 인자가 없으면 사용법 에러', () => {
    const repo = repoWith('sha256:match');
    expect(() => main(repo, [])).toThrow(/사용법/);
  });
});

describe('실제 CLI 실행 — 서명 파이프라인이 스폰하는 그대로', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function repoWith(expectedHash) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-sign-r2-cli-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    writeFileSync(join(dir, 'compat', 'codesign-cert.json'), JSON.stringify({ expectedCertificateHashSha256: expectedHash }), 'utf8');
    return dir;
  }

  it('베이스라인 부재면 0이 아닌 종료 코드로 실패한다', () => {
    const repo = repoWith(null);
    expect(() =>
      execFileSync('node', [SCRIPT, '--actual-hash=sha256:zzz'], { env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });

  it('불일치면 0이 아닌 종료 코드로 실패한다', () => {
    const repo = repoWith('sha256:expected');
    expect(() =>
      execFileSync('node', [SCRIPT, '--actual-hash=sha256:other'], { env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });

  it('일치하면 0 종료 코드로 통과한다', () => {
    const repo = repoWith('sha256:match');
    const out = execFileSync('node', [SCRIPT, '--actual-hash=sha256:match'], {
      env: { ...process.env, MALGN_GEN_SITE_ROOT: repo },
      stdio: 'pipe',
    }).toString();
    expect(out).toMatch(/OK/);
  });
});
