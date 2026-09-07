// NT-R21 패키징 시점 fail-closed 집행 — scripts/assert-shipped-media-scan.mjs 검증.
// docs/release-gates.md §7.6.6.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { assertShippedMediaScan, getShippedMediaRelPaths, main } from './assert-shipped-media-scan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = join(HERE, 'assert-shipped-media-scan.mjs');
const REAL_SENSITIVE_CLASSES = join(HERE, '..', 'compat', 'sensitive-classes.json');

const SYNTHETIC_NON_EXEMPT_DOMAIN = 'fakecorp-internal' + '.io';

describe('getShippedMediaRelPaths', () => {
  it('오늘은 dist/extension.cjs 하나뿐이다', () => {
    expect(getShippedMediaRelPaths()).toEqual([join('dist', 'extension.cjs')]);
  });
});

describe('assertShippedMediaScan — 순수 로직(임시 저장소)', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function makeRoot() {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-media-assert-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    mkdirSync(join(dir, 'dist'), { recursive: true });
    // 실제 sensitive-classes.json을 그대로 복사해 쓴다 — 규칙 이중 정의를 피하고
    // 실제 클래스 집합으로 검증한다.
    writeFileSync(join(dir, 'compat', 'sensitive-classes.json'), readFileSync(REAL_SENSITIVE_CLASSES, 'utf8'), 'utf8');
    return dir;
  }

  it('아티팩트가 없으면 위반을 낸다(패키징 순서 깨짐을 잡음)', () => {
    const root = makeRoot();
    const violations = assertShippedMediaScan(root);
    expect(violations.some((v) => v.includes('출하 매체가 없습니다'))).toBe(true);
  });

  it('아티팩트에 민감값 모양이 없으면 위반이 없다', () => {
    const root = makeRoot();
    writeFileSync(join(root, 'dist', 'extension.cjs'), 'console.log("clean build");', 'utf8');
    expect(assertShippedMediaScan(root)).toEqual([]);
  });

  it('아티팩트에 민감값 모양(비허용 도메인)이 있으면 검출한다', () => {
    const root = makeRoot();
    writeFileSync(join(root, 'dist', 'extension.cjs'), `const h = "download.${SYNTHETIC_NON_EXEMPT_DOMAIN}";`, 'utf8');
    const violations = assertShippedMediaScan(root);
    expect(violations.some((v) => v.includes('network-authority-domain'))).toBe(true);
  });
});

describe('main() — 종료 코드', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function makeRoot() {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-media-assert-main-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    mkdirSync(join(dir, 'dist'), { recursive: true });
    writeFileSync(join(dir, 'compat', 'sensitive-classes.json'), readFileSync(REAL_SENSITIVE_CLASSES, 'utf8'), 'utf8');
    return dir;
  }

  it('아티팩트 부재면 던진다', () => {
    const root = makeRoot();
    expect(() => main(root)).toThrow(/NT-R21/);
  });

  it('깨끗한 아티팩트면 통과하고 스캔 목록을 반환한다', () => {
    const root = makeRoot();
    writeFileSync(join(root, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    expect(main(root)).toEqual({ ok: true, scanned: [join('dist', 'extension.cjs')] });
  });

  it('실제 CLI 실행 — 민감값이 있으면 0이 아닌 종료 코드로 실패한다', () => {
    const root = makeRoot();
    writeFileSync(join(root, 'dist', 'extension.cjs'), `const h = "download.${SYNTHETIC_NON_EXEMPT_DOMAIN}";`, 'utf8');
    expect(() =>
      execFileSync('node', [SCRIPT], { env: { ...process.env, MALGN_GEN_SITE_ROOT: root }, stdio: 'pipe' })
    ).toThrow();
  });

  it('실제 CLI 실행 — 깨끗하면 0 종료 코드로 통과한다', () => {
    const root = makeRoot();
    writeFileSync(join(root, 'dist', 'extension.cjs'), 'console.log(1);', 'utf8');
    const out = execFileSync('node', [SCRIPT], { env: { ...process.env, MALGN_GEN_SITE_ROOT: root }, stdio: 'pipe' }).toString();
    expect(out).toMatch(/OK/);
  });
});
