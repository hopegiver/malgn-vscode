// fail-closed 3종의 셋째(패키징) — scripts/assert-site-profile-for-packaging.mjs가
// siteProfile !== 'site'일 때 실제로 0이 아닌 종료 코드를 내는지 실제 하위 프로세스로 증명한다.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { extractSiteProfile, main } from './assert-site-profile-for-packaging.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = join(HERE, 'assert-site-profile-for-packaging.mjs');

function moduleText(profile) {
  return `export const siteProfile: 'site' | 'example' = "${profile}";\nexport const siteConstants = {} as const;\n`;
}

describe('extractSiteProfile', () => {
  it('생성 모듈 텍스트에서 siteProfile 값을 뽑는다', () => {
    expect(extractSiteProfile(moduleText('example'))).toBe('example');
    expect(extractSiteProfile(moduleText('site'))).toBe('site');
  });

  it('패턴이 없으면 던진다', () => {
    expect(() => extractSiteProfile('garbage')).toThrow();
  });
});

describe('main() — 순수 로직', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function repoWith(profile) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-pkg-guard-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'src', 'generated'), { recursive: true });
    writeFileSync(join(dir, 'src', 'generated', 'siteConstants.ts'), moduleText(profile), 'utf8');
    return dir;
  }

  it("siteProfile==='site'면 'site'를 반환한다(통과)", () => {
    expect(main(repoWith('site'))).toBe('site');
  });

  it("siteProfile==='example'이면 던진다", () => {
    expect(() => main(repoWith('example'))).toThrow(/패키징을 실패/);
  });
});

describe('실제 CLI 실행 — 패키징 파이프라인이 스폰하는 그대로', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function repoWith(profile) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-pkg-guard-cli-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'src', 'generated'), { recursive: true });
    writeFileSync(join(dir, 'src', 'generated', 'siteConstants.ts'), moduleText(profile), 'utf8');
    return dir;
  }

  it('siteProfile=example이면 0이 아닌 종료 코드로 실패한다(빌드/패키징 실패로 이어짐)', () => {
    const repo = repoWith('example');
    expect(() =>
      execFileSync('node', [SCRIPT], { env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });

  it('siteProfile=site이면 0 종료 코드로 통과한다', () => {
    const repo = repoWith('site');
    const out = execFileSync('node', [SCRIPT], { env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' }).toString();
    expect(out).toMatch(/OK/);
  });
});
