// NT-R22 — scripts/generate-build-provenance.mjs 검증. 실제 파일시스템(임시 디렉터리)을
// 통해 fail-closed 3종(아티팩트 부재·siteConstants 부재·git 부재)과 정상 경로를 확인한다.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { computeArtifactSha256, generateProvenance, getCommitSha, main } from './generate-build-provenance.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

function moduleText(profile) {
  return `export const siteProfile: 'site' | 'example' = "${profile}";\nexport const siteConstants = {} as const;\n`;
}

describe('computeArtifactSha256', () => {
  it('sha256: 접두로 hex 다이제스트를 낸다', () => {
    expect(computeArtifactSha256(Buffer.from('x', 'utf8'))).toMatch(/^sha256:[0-9a-f]{64}$/);
  });
});

describe('getCommitSha', () => {
  it('execFileSync가 실패하면(예: .git 없음) fail-closed 에러를 던진다', () => {
    const throwingExec = () => {
      throw new Error('not a git repository');
    };
    expect(() => getCommitSha('/nonexistent', throwingExec)).toThrow(/NT-R22/);
  });

  it('execFileSync가 SHA를 돌려주면 trim해서 반환한다', () => {
    const fakeExec = () => 'abc1234\n';
    expect(getCommitSha('/repo', fakeExec)).toBe('abc1234');
  });
});

describe('generateProvenance — fail-closed 3종 + 정상 경로', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function makeRoot({ withArtifact = true, withSiteConstants = true } = {}) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-provenance-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'dist'), { recursive: true });
    mkdirSync(join(dir, 'src', 'generated'), { recursive: true });
    if (withArtifact) writeFileSync(join(dir, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    if (withSiteConstants) writeFileSync(join(dir, 'src', 'generated', 'siteConstants.ts'), moduleText('example'), 'utf8');
    return dir;
  }

  it('아티팩트가 없으면 던진다', () => {
    const root = makeRoot({ withArtifact: false });
    expect(() => generateProvenance({ root, execFileSyncFn: () => 'aaa\n' })).toThrow(/아티팩트가 없습니다/);
  });

  it('siteConstants.ts가 없으면 던진다', () => {
    const root = makeRoot({ withSiteConstants: false });
    expect(() => generateProvenance({ root, execFileSyncFn: () => 'aaa\n' })).toThrow(/siteConstants/);
  });

  it('git이 없으면(execFileSync 실패) 던진다', () => {
    const root = makeRoot();
    const throwingExec = () => {
      throw new Error('fatal: not a git repository');
    };
    expect(() => generateProvenance({ root, execFileSyncFn: throwingExec })).toThrow(/NT-R22/);
  });

  it('전부 갖춰지면 레코드를 만든다 — siteProfile/commitSha/artifactSha256/buildTimestamp/signature:null', () => {
    const root = makeRoot();
    const fixedNow = () => new Date('2026-09-07T00:00:00.000Z');
    const record = generateProvenance({ root, execFileSyncFn: () => 'deadbeef1234\n', now: fixedNow });
    expect(record.recordVersion).toBe(1);
    expect(record.siteProfile).toBe('example');
    expect(record.commitSha).toBe('deadbeef1234');
    expect(record.artifactPath).toBe('dist/extension.cjs');
    expect(record.artifactSha256).toMatch(/^sha256:[0-9a-f]{64}$/);
    expect(record.buildTimestamp).toBe('2026-09-07T00:00:00.000Z');
    expect(record.signature).toBeNull();
  });

  it('아티팩트 내용이 다르면 해시도 달라진다(레코드가 실제 산출물을 반영)', () => {
    const rootA = makeRoot();
    writeFileSync(join(rootA, 'dist', 'extension.cjs'), 'console.log("v1");', 'utf8');
    const recordA = generateProvenance({ root: rootA, execFileSyncFn: () => 'a\n' });

    const rootB = makeRoot();
    writeFileSync(join(rootB, 'dist', 'extension.cjs'), 'console.log("v2 — different content");', 'utf8');
    const recordB = generateProvenance({ root: rootB, execFileSyncFn: () => 'a\n' });

    expect(recordA.artifactSha256).not.toBe(recordB.artifactSha256);
  });
});

describe('main() — 파일 쓰기까지 end-to-end', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  it('.git이 없으면 main()도 fail-closed로 던진다', () => {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-provenance-main-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'dist'), { recursive: true });
    mkdirSync(join(dir, 'src', 'generated'), { recursive: true });
    writeFileSync(join(dir, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    writeFileSync(join(dir, 'src', 'generated', 'siteConstants.ts'), moduleText('site'), 'utf8');
    // main()은 기본적으로 실제 execFileSync(git)를 쓴다 — 이 임시 디렉터리엔 .git이
    // 없으므로 실제로 fail-closed 되는지까지 함께 확인한다.
    expect(() => main(dir)).toThrow(/NT-R22/);
  });

  it('.git이 있으면(진짜 git repo) 실제로 dist/build-provenance.json을 쓴다', () => {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-provenance-main-git-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'dist'), { recursive: true });
    mkdirSync(join(dir, 'src', 'generated'), { recursive: true });
    writeFileSync(join(dir, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    writeFileSync(join(dir, 'src', 'generated', 'siteConstants.ts'), moduleText('site'), 'utf8');
    // 이 저장소의 실제 git 설정은 건드리지 않는다 — 이 임시 디렉터리 하나에 한정된
    // 로컬 sandbox repo이고 -c로 커밋 1건에만 적용되는 identity를 준다.
    execFileSync('git', ['init', '-q'], { cwd: dir });
    writeFileSync(join(dir, 'README.md'), 'sandbox\n', 'utf8');
    execFileSync('git', ['add', 'README.md'], { cwd: dir });
    // user.email은 RFC 2606 예약 도메인(example.com)을 쓴다 — sensitive-classes.json의
    // email-literal exempt 패턴과 일치시켜 검사 ⑨ 자기 스캔에서 합성값을 실제 개인 이메일로
    // 오탐하지 않게 한다.
    execFileSync('git', ['-c', 'user.email=sandbox@example.com', '-c', 'user.name=sandbox', 'commit', '-q', '-m', 'sandbox'], { cwd: dir });

    const record = main(dir);
    expect(record.commitSha).toMatch(/^[0-9a-f]{40}$/);
    const written = JSON.parse(readFileSync(join(dir, 'dist', 'build-provenance.json'), 'utf8'));
    expect(written.artifactSha256).toBe(record.artifactSha256);
    expect(written.signature).toBeNull();
  });
});
