// SIGN-R2 구조적 배선 검사 — docs/release-gates.md §7.6.2.
//
// [이 파일이 존재하는 이유] 서명 파이프라인(electron-builder의 macOS 서명 단계, 또는
// 그 후처리로 `codesign`을 직접 호출하는 스크립트)은 **아직 이 저장소에 없다**(작업
// 지시 — Electron·electron-builder를 이번에 설치하지 않는다). 그래서 SIGN-R2의 실제
// 대조 로직(`codesignCertificateGuard.ts`)은 오늘 호출자가 0이다. 이 상태를 "검사할
// 대상이 없다"로 조용히 넘기면, **다음에 서명 파이프라인이 실제로 생겼을 때 이 배선을
// 빠뜨려도 아무도 모른다** — 정확히 release-gates.md §7.6.2가 경고하는 "무심코 재발급해도
// 아무 검사도 실패하지 않는다"는 실패 모드다.
//
// 그래서 이 검사는 "codesign 호출/서명 패키저가 코드베이스에 등장하는가"를 구조적으로
// 스캔하고, 등장했는데 `verify-codesign-certificate-hash` 배선이 없으면 **CI를
// 실패시킨다.** 오늘은 신호가 0개라 `skippedNoTarget: true`로 통과한다 — updateChannel
// Boundary.ts의 AT-U1~U4와 같은 패턴(대상 없음과 위반 0건을 구분해서 반환한다).

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

export interface SigningWiringCheckResult {
  readonly violations: readonly string[];
  readonly skippedNoTarget: boolean;
}

/** 이 검사 자신과 SIGN-R2 정본 구현·테스트 파일은 스캔 대상에서 뺀다(자기참조 오탐
 * 방지 — sensitive-classes.json pathExemptions와 같은 종류의 문제). */
const SELF_FILES = new Set([
  'scripts/verify-codesign-certificate-hash.mjs',
  'scripts/verify-codesign-certificate-hash.test.mjs',
  'src/core/policy/codesignCertificateGuard.ts',
  'src/core/policy/codesignCertificateGuard.test.ts',
  'src/architecture-tests/codesignSigningWiring.ts',
  'src/architecture-tests/codesignSigningWiring.test.ts',
]);

/** 실제 codesign 서명 패키저 — 설치되는 순간 그 자체가 신호다(도입 여부와 무관하게
 * 이 목록에 오르는 것이 이 프로젝트가 "electron-builder를 설치하지 않는다" 제약을
 * 어겼는지 감지하는 부수 효과도 겸한다). */
const CODESIGN_PACKAGER_DEPENDENCIES = ['electron-builder', '@electron/osx-sign', 'electron-osx-sign', '@electron/notarize'];

/** `codesign` 셸 명령 자체를 문자열/따옴표 형태로 호출하는 패턴 — 주석 산문에서
 * "codesign"이라는 단어만 언급하는 경우(이 파일들 자신처럼)는 매칭하지 않도록 명령
 * 형태(따옴표로 감싼 토큰이거나 플래그가 뒤따르는 형태)만 잡는다. */
const CODESIGN_INVOCATION_RE = /(['"`])codesign\1|\bcodesign\s+(-{1,2}\S)/;

const WIRING_MARKER = 'verify-codesign-certificate-hash';
const SCAN_EXTENSIONS = new Set(['.mjs', '.cjs', '.js', '.sh']);

function listFilesRecursive(dir: string): string[] {
  if (!existsSync(dir)) return [];
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listFilesRecursive(full));
    } else if (entry.isFile() && SCAN_EXTENSIONS.has(entry.name.slice(entry.name.lastIndexOf('.')))) {
      out.push(full);
    }
  }
  return out;
}

interface PackageJsonShape {
  readonly scripts?: Readonly<Record<string, string>>;
  readonly dependencies?: Readonly<Record<string, string>>;
  readonly devDependencies?: Readonly<Record<string, string>>;
}

/**
 * `repoRoot`를 스캔해 "서명 단계가 코드베이스에 등장했는데 SIGN-R2 배선이 없는" 상태를
 * 잡는다. 등장 신호가 전혀 없으면 `skippedNoTarget: true`(오늘의 정상 상태).
 */
export function checkCodesignSigningWiring(repoRoot: string): SigningWiringCheckResult {
  let foundSignal = false;
  let foundWiring = false;
  const signalDetails: string[] = [];

  const pkgPath = join(repoRoot, 'package.json');
  if (existsSync(pkgPath)) {
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf8')) as PackageJsonShape;
    for (const [name, cmd] of Object.entries(pkg.scripts ?? {})) {
      if (CODESIGN_INVOCATION_RE.test(cmd)) {
        foundSignal = true;
        signalDetails.push(`package.json scripts.${name}`);
      }
      if (cmd.includes(WIRING_MARKER)) foundWiring = true;
    }
    const deps = { ...(pkg.dependencies ?? {}), ...(pkg.devDependencies ?? {}) };
    for (const dep of CODESIGN_PACKAGER_DEPENDENCIES) {
      if (Object.prototype.hasOwnProperty.call(deps, dep)) {
        foundSignal = true;
        signalDetails.push(`package.json dependency ${dep}`);
      }
    }
  }

  for (const file of listFilesRecursive(join(repoRoot, 'scripts'))) {
    const rel = relative(repoRoot, file).split(sep).join('/');
    if (SELF_FILES.has(rel)) continue;
    const text = readFileSync(file, 'utf8');
    if (CODESIGN_INVOCATION_RE.test(text)) {
      foundSignal = true;
      signalDetails.push(rel);
    }
    if (text.includes(WIRING_MARKER)) foundWiring = true;
  }

  if (!foundSignal) return { violations: [], skippedNoTarget: true };

  if (!foundWiring) {
    return {
      skippedNoTarget: false,
      violations: [
        `SIGN-R2: codesign 호출/서명 패키저 의존성이 감지됐지만(${signalDetails.join(', ')}) ` +
          `'${WIRING_MARKER}' 배선이 어디에도 없습니다 — 서명 단계가 생겼는데 해시 대조가 없으면 ` +
          '릴리스가 조용히 나갑니다(docs/release-gates.md §7.6.2).',
      ],
    };
  }

  return { violations: [], skippedNoTarget: false };
}
