// `pnpm gen:site`(scripts/gen-site.mjs) 검증 — docs/policy-contract.md §8.4 fail-closed 2종
// (사이트면 부재 / 구멍 잔존)이 실제로 빌드를 실패시키는지를 순수 함수 레벨과, 실제
// 하위 프로세스(node scripts/gen-site.mjs)를 스폰하는 end-to-end 레벨 둘 다로 증명한다.
// 완료판정 #4 "fail-closed 3종이 실제로 동작함을 테스트로 보이십시오"의 근거.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { generateSiteModule, hasRemainingHole, resolveSiteHoles } from './gen-site.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_GEN_SITE_SCRIPT = join(HERE, 'gen-site.mjs');
const REAL_COMPATIBILITY_JSON = JSON.parse(readFileSync(join(HERE, '..', 'compat', 'compatibility.json'), 'utf8'));
const REAL_SITE_EXAMPLE_JSON = readFileSync(join(HERE, '..', 'compat', 'site.example.json'), 'utf8');

describe('resolveSiteHoles / hasRemainingHole — 순수 로직', () => {
  it('중첩된 $site 구멍을 사이트면 값으로 치환한다', () => {
    const tree = { a: { $site: 'x' }, b: [{ $site: 'y' }, 'literal'] };
    const errors = [];
    const resolved = resolveSiteHoles(tree, { x: 'X', y: ['Y1', 'Y2'] }, errors);
    expect(errors).toEqual([]);
    expect(resolved).toEqual({ a: 'X', b: [['Y1', 'Y2'], 'literal'] });
    expect(hasRemainingHole(resolved)).toBe(false);
  });

  it('사이트면에 키가 없으면 구멍을 그대로 남기고 errors에 기록한다', () => {
    const tree = { a: { $site: 'missing' } };
    const errors = [];
    const resolved = resolveSiteHoles(tree, {}, errors);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toMatch(/missing/);
    expect(hasRemainingHole(resolved)).toBe(true);
  });
});

describe('generateSiteModule — fail-closed', () => {
  it('두 사이트면이 모두 없으면 던진다(①)', () => {
    expect(() =>
      generateSiteModule({
        compatibilityJson: { allowedAuthorities: { $site: 'authorities' } },
        siteJsonPath: '/nonexistent/site.json',
        siteExampleJsonPath: '/nonexistent/site.example.json',
        existsFn: () => false,
      })
    ).toThrow(/사이트면 부재/);
  });

  it('구멍에 대응하는 키가 사이트면에 없으면 던진다(②)', () => {
    expect(() =>
      generateSiteModule({
        compatibilityJson: { allowedAuthorities: { $site: 'authorities' }, allowedKeychainItems: { $site: 'keychainItems' } },
        siteJsonPath: '/nonexistent/site.json',
        siteExampleJsonPath: '/example/site.example.json',
        existsFn: (p) => p === '/example/site.example.json',
        readFileFn: () => JSON.stringify({ authorities: { otel: ['203.0.113.10:4318'] } /* keychainItems 없음 */ }),
      })
    ).toThrow(/구멍 잔존/);
  });

  it('site/site.json이 있으면 그것을 쓰고 siteProfile은 site다', () => {
    const result = generateSiteModule({
      compatibilityJson: { allowedAuthorities: { $site: 'authorities' } },
      siteJsonPath: '/real/site.json',
      siteExampleJsonPath: '/example/site.example.json',
      existsFn: (p) => p === '/real/site.json',
      readFileFn: () => JSON.stringify({ authorities: { otel: ['203.0.113.10:4318'] } }),
    });
    expect(result.siteProfile).toBe('site');
    expect(result.siteConstants).toEqual({ allowedAuthorities: { otel: ['203.0.113.10:4318'] } });
  });

  it('site/site.json이 없으면 compat/site.example.json으로 폴백하고 siteProfile은 example다', () => {
    const result = generateSiteModule({
      compatibilityJson: { allowedAuthorities: { $site: 'authorities' } },
      siteJsonPath: '/nonexistent/site.json',
      siteExampleJsonPath: '/example/site.example.json',
      existsFn: (p) => p === '/example/site.example.json',
      readFileFn: () => JSON.stringify({ authorities: { otel: ['203.0.113.10:4318'] } }),
    });
    expect(result.siteProfile).toBe('example');
  });
});

// --- end-to-end: 실제 하위 프로세스로 scripts/gen-site.mjs 자체를 spawn한다 ---
describe('scripts/gen-site.mjs — 실제 CLI 실행(임시 저장소, MALGN_GEN_SITE_ROOT 오버라이드)', () => {
  const tempDirs = [];
  afterEach(() => {
    for (const dir of tempDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
  });

  function makeTempRepo() {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-gen-site-'));
    tempDirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    writeFileSync(join(dir, 'compat', 'compatibility.json'), JSON.stringify(REAL_COMPATIBILITY_JSON), 'utf8');
    return dir;
  }

  it('두 사이트면이 모두 없으면 0이 아닌 종료 코드로 실패한다(빌드 실패로 이어짐)', () => {
    const repo = makeTempRepo();
    expect(() =>
      execFileSync('node', [REAL_GEN_SITE_SCRIPT], { cwd: repo, env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });

  it('example.json에 구멍이 남으면(필수 키 누락) 0이 아닌 종료 코드로 실패한다', () => {
    const repo = makeTempRepo();
    // keychainItems 키를 일부러 뺀 example 사이트면 — allowedKeychainItems 구멍이 못 채워진다
    writeFileSync(join(repo, 'compat', 'site.example.json'), JSON.stringify({ authorities: { otel: ['203.0.113.10:4318'], extension: ['*.example.com'], identity: ['a.example.com', 'b.example.com'], hub: ['b.example.com'], mcp: ['b.example.com'] } }), 'utf8');
    expect(() =>
      execFileSync('node', [REAL_GEN_SITE_SCRIPT], { cwd: repo, env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });

  it('정상 example 사이트면만 있으면 성공하고 siteConstants.ts를 emit한다(siteProfile=example)', () => {
    const repo = makeTempRepo();
    writeFileSync(join(repo, 'compat', 'site.example.json'), REAL_SITE_EXAMPLE_JSON, 'utf8');
    execFileSync('node', [REAL_GEN_SITE_SCRIPT], { cwd: repo, env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' });
    const generated = readFileSync(join(repo, 'src', 'generated', 'siteConstants.ts'), 'utf8');
    expect(generated).toMatch(/siteProfile: 'site' \| 'example' = "example"/);
    expect(generated).toContain('example.com');
  });

  it('site/site.json이 있으면 example보다 우선하고 siteProfile=site로 emit한다', () => {
    const repo = makeTempRepo();
    writeFileSync(join(repo, 'compat', 'site.example.json'), REAL_SITE_EXAMPLE_JSON, 'utf8');
    mkdirSync(join(repo, 'site'), { recursive: true });
    writeFileSync(
      join(repo, 'site', 'site.json'),
      JSON.stringify({
        authorities: { otel: ['198.51.100.5:4318'], extension: ['*.example.net'], identity: ['a.example.net', 'b.example.net'], hub: ['b.example.net'], mcp: ['b.example.net'] },
        keychainItems: ['temp-service'],
      }),
      'utf8'
    );
    execFileSync('node', [REAL_GEN_SITE_SCRIPT], { cwd: repo, env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' });
    const generated = readFileSync(join(repo, 'src', 'generated', 'siteConstants.ts'), 'utf8');
    expect(generated).toMatch(/siteProfile: 'site' \| 'example' = "site"/);
    expect(generated).toContain('example.net');
  });
});
