// 아키텍처 테스트 — architecture.md §6.2 "마스킹 필터는 로그 싱크에 단일 지점으로
// 건다"가 실제로 단일 지점인지, 그리고 그 지점을 우회하는 경로가 코드베이스에 없는지를
// grep으로 고정한다. `src/core/reconciler/stopGate.ts`가 STOPPABLE_SURFACES에 대해
// 쓰는 것과 같은 종류의 검증(코드가 아니라 "코드 전체에 걸친 성질"은 단위 테스트로
// 못 잡으므로 소스 트리 자체를 검사 대상으로 삼는다)이다.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
// HERE = <repo>/src/core/diagnostics — 두 단계 위가 <repo>/src다.
const SRC_ROOT = join(HERE, '..', '..');

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFiles(full));
    } else if (entry.isFile() && (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx'))) {
      out.push(full);
    }
  }
  return out;
}

const ALL_SRC_FILES = listTsFiles(SRC_ROOT);
const MASK_DEFINITION_FILE = join(SRC_ROOT, 'core', 'diagnostics', 'mask.ts');

function relPath(p: string): string {
  return relative(SRC_ROOT, p);
}

describe('마스킹 정규식은 core/diagnostics/mask.ts 한 곳에만 정의된다', () => {
  // doc-exact 네 패턴의 소스 텍스트 조각 — 정의 파일 밖 어디에도 다시 등장하면 안 된다
  // (복붙되면 두 구현이 갈릴 위험, 이 프로젝트가 검사 ⑨에서 겪은 실패 패턴과 동형).
  const PATTERN_FRAGMENTS = ['gh[pousr]_[A-Za-z0-9]{20,}', 'Bearer\\s+\\S+', 'Basic\\s+\\S+', '[A-Za-z0-9_-]{32,}'];

  it.each(PATTERN_FRAGMENTS)('패턴 조각 "%s"는 mask.ts 밖에서 재정의되지 않는다', (fragment) => {
    const offenders = ALL_SRC_FILES.filter((file) => {
      if (file === MASK_DEFINITION_FILE) return false;
      if (file.endsWith('.test.ts')) return false; // 이 테스트 파일 자신의 주석/문자열 제외
      const text = readFileSync(file, 'utf8');
      return text.includes(fragment);
    });
    expect(offenders.map(relPath)).toEqual([]);
  });
});

describe('마스킹 함수(maskSensitive/maskDeepValues) 호출부는 정해진 파일 집합 밖으로 늘어나지 않는다', () => {
  function importersOf(identifier: string): string[] {
    return ALL_SRC_FILES.filter((file) => {
      if (file === MASK_DEFINITION_FILE) return false;
      if (file.endsWith('.test.ts')) return false; // 단위 테스트가 함수 자체를 검증하려고 import하는 것은 허용
      const text = readFileSync(file, 'utf8');
      // 상대 경로 표기가 파일 위치에 따라 달라지므로(같은 디렉터리면 './mask.js',
      // 다른 디렉터리면 '../diagnostics/mask.js') 경로 문자열이 아니라 import된
      // 식별자로 판정한다.
      const re = new RegExp(`import\\s*\\{[^}]*\\b${identifier}\\b[^}]*\\}\\s*from`);
      return re.test(text);
    }).map(relPath);
  }

  it('maskSensitive(자유 텍스트용)를 import하는 production 파일은 정확히 ui/log.ts 하나뿐이다', () => {
    expect(importersOf('maskSensitive').sort()).toEqual([join('ui', 'log.ts')]);
  });

  it('maskDeepValues(구조화 값용)를 import하는 production 파일은 정확히 store.ts·report.ts 둘뿐이다', () => {
    expect(importersOf('maskDeepValues').sort()).toEqual(
      [join('core', 'journal', 'store.ts'), join('core', 'diagnostics', 'report.ts')].sort()
    );
  });
});

describe('로그 싱크(appendLine) 호출은 ui/log.ts 한 곳에서만 일어난다', () => {
  it('.appendLine( 호출 지점은 정확히 ui/log.ts 하나다', () => {
    const callers = ALL_SRC_FILES.filter((file) => {
      if (file.endsWith('.test.ts')) return false; // 테스트의 fake sink는 정의(구현)일 뿐 호출이 아니다
      const text = readFileSync(file, 'utf8');
      return /\.appendLine\(/.test(text);
    });
    expect(callers.map(relPath)).toEqual([join('ui', 'log.ts')]);
  });
});

describe('저널 파일에 바이트를 쓰는 호출은 core/journal/store.ts 한 곳에서만 일어난다', () => {
  it('appendFile( 호출 지점은 정확히 core/journal/store.ts 하나다(diagnostics·secrets·ui 범위 내)', () => {
    const scopedRoots = ['core/journal', 'core/diagnostics', 'secrets', 'ui'].map((p) => join(SRC_ROOT, ...p.split('/')));
    const scopedFiles = ALL_SRC_FILES.filter((file) => scopedRoots.some((root) => file.startsWith(root)));
    const callers = scopedFiles.filter((file) => {
      if (file.endsWith('.test.ts')) return false;
      const text = readFileSync(file, 'utf8');
      return /\bappendFile\(/.test(text);
    });
    expect(callers.map(relPath)).toEqual([join('core', 'journal', 'store.ts')]);
  });
});
