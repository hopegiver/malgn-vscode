// NT-R22 서명 배선 구조 검사 — docs/release-gates.md §7.6.6 / §7.6 K-3.
//
// [이 파일이 존재하는 이유] 빌드 프로베넌스 레코드(`generate-build-provenance.mjs`)의
// `signature` 필드는 지금 항상 `null`이다 — 업데이트 서명 키(K-3, Ed25519)가 아직
// 존재하지 않는다. 이 상태 자체는 정상이다(§7.6.6 정직 표기). 위험한 것은 **K-3가
// 실제로 생겼는데(코드베이스 어딘가에 서명 키 자료·서명 호출이 등장했는데)
// `buildProvenance.ts`/`generate-build-provenance.mjs`가 여전히 `signature: null`을
// 반환하는 상태**다 — 조용히 "미서명 프로베넌스"가 "서명된 것처럼 보이는 산출물"
// 사이 어딘가에 방치된다. 이 검사는 그 상태를 구조적으로 잡는다.
//
// 오늘은 K-3 관련 신호가 코드베이스에 전혀 없다(키 발급이 이 작업의 범위 밖) —
// `skippedNoTarget: true`로 통과한다(updateChannelBoundary.ts AT-U 패턴과 동일).

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

export interface SigningWiringCheckResult {
  readonly violations: readonly string[];
  readonly skippedNoTarget: boolean;
}

const SELF_FILES = new Set([
  'src/core/provenance/buildProvenance.ts',
  'src/core/provenance/buildProvenance.test.ts',
  'scripts/generate-build-provenance.mjs',
  'scripts/generate-build-provenance.test.mjs',
  'src/architecture-tests/provenanceSigningWiring.ts',
  'src/architecture-tests/provenanceSigningWiring.test.ts',
]);

/** K-3(Ed25519 업데이트 서명 키) 자료·호출이 등장했다고 볼 만한 신호. 라이브러리
 * 이름은 이 생태계에서 Ed25519 서명에 흔히 쓰이는 것들이다 — 하나라도 등장하면
 * "서명 인프라가 생겼다"는 신호로 본다. */
const SIGNING_LIBRARY_DEPENDENCIES = ['tweetnacl', '@noble/ed25519', '@noble/curves', 'libsodium-wrappers', 'sodium-native'];
const SIGNING_KEY_IDENTIFIER_RE = /\bK[-_]?3\b.*(?:key|Key)|UPDATE_SIGNING_KEY|updateSigningKey|ed25519.*sign|signProvenance/;
const SCAN_EXTENSIONS = new Set(['.mjs', '.cjs', '.js', '.ts']);
const WIRING_MARKER_RE = /signature:\s*(?!null\b)/; // buildProvenanceRecord가 signature에 null이 아닌 값을 대입하는 흔적

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
  readonly dependencies?: Readonly<Record<string, string>>;
  readonly devDependencies?: Readonly<Record<string, string>>;
}

export function checkProvenanceSigningWiring(repoRoot: string): SigningWiringCheckResult {
  let foundSignal = false;
  let foundWiring = false;
  const signalDetails: string[] = [];

  const pkgPath = join(repoRoot, 'package.json');
  if (existsSync(pkgPath)) {
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf8')) as PackageJsonShape;
    const deps = { ...(pkg.dependencies ?? {}), ...(pkg.devDependencies ?? {}) };
    for (const dep of SIGNING_LIBRARY_DEPENDENCIES) {
      if (Object.prototype.hasOwnProperty.call(deps, dep)) {
        foundSignal = true;
        signalDetails.push(`package.json dependency ${dep}`);
      }
    }
  }

  for (const dir of ['src', 'scripts']) {
    for (const file of listFilesRecursive(join(repoRoot, dir))) {
      const rel = relative(repoRoot, file).split(sep).join('/');
      if (SELF_FILES.has(rel)) continue;
      const text = readFileSync(file, 'utf8');
      if (SIGNING_KEY_IDENTIFIER_RE.test(text)) {
        foundSignal = true;
        signalDetails.push(rel);
      }
    }
  }

  // 배선 여부는 정본 파일(buildProvenance.ts)만 본다 — 정본이 아닌 곳에서
  // "signature: '...'" 흉내를 내도 실제로 반영되지 않기 때문이다.
  const buildProvenancePath = join(repoRoot, 'src', 'core', 'provenance', 'buildProvenance.ts');
  if (existsSync(buildProvenancePath) && WIRING_MARKER_RE.test(readFileSync(buildProvenancePath, 'utf8'))) {
    foundWiring = true;
  }

  if (!foundSignal) return { violations: [], skippedNoTarget: true };

  if (!foundWiring) {
    return {
      skippedNoTarget: false,
      violations: [
        `NT-R22: K-3(업데이트 서명 키) 관련 신호가 감지됐지만(${signalDetails.join(', ')}) ` +
          "buildProvenance.ts는 여전히 signature를 null로만 만듭니다 — 서명 키가 생겼는데 " +
          '프로베넌스에 반영되지 않으면 "서명됐다"는 착각을 만듭니다(docs/release-gates.md §7.6.6).',
      ],
    };
  }

  return { violations: [], skippedNoTarget: false };
}
