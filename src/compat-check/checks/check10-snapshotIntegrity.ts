// 검사 ⑩ — 스냅샷 무결성·신선도 (docs/policy-contract.md §8.3 신설 3종 / §8.5 D4)
//
// (a) self-digest 재계산 일치
// (b) generatorVersion == 코드에 컴파일된 생성기 버전(scripts/gen-contract-snapshot.mjs의
//     GENERATOR_VERSION)
// (c) extensionVersion == package.json.version
// (d) 스냅샷에 값형 필드가 없음(이름·개수·해시만 — siteShape이 문자열 authority/keychain
//     항목명을 담고 있지 않은지 구조적으로 확인한다)

import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    const keys = Object.keys(record).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(record[k])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function sha256(text: string): string {
  return `sha256:${createHash('sha256').update(text, 'utf8').digest('hex')}`;
}

interface SourceRegion {
  readonly doc?: unknown;
  readonly region?: unknown;
  readonly sha256?: unknown;
}

interface Snapshot {
  readonly snapshotVersion?: unknown;
  readonly generatorVersion?: unknown;
  readonly extensionVersion?: unknown;
  readonly docIdentifiers?: unknown;
  readonly leafRows?: unknown;
  readonly siteShape?: unknown;
  readonly sourceRegions?: readonly SourceRegion[];
  readonly digest?: unknown;
}

/** siteShape이 이름·개수만 담고 실제 authority 값(문자열 호스트/IP/keychain 항목명)을
 * 담지 않는지 구조적으로 확인한다. 허용 형태: `{ authorities: { keys: string[], counts:
 * Record<string, number> }, keychainItems: { count: number } }` — `keys`(부류 이름)는
 * 값이 아니라 스키마이므로 허용하고, 그 밖의 모든 리프는 숫자여야 한다. */
function hasValueLikeField(siteShape: unknown): boolean {
  if (siteShape === null || typeof siteShape !== 'object') return false;
  const shape = siteShape as Record<string, unknown>;
  const authorities = shape.authorities as { keys?: unknown; counts?: unknown } | undefined;
  if (authorities) {
    if (authorities.keys !== undefined && !Array.isArray(authorities.keys)) return true;
    if (Array.isArray(authorities.keys) && authorities.keys.some((k) => typeof k !== 'string')) return true;
    if (authorities.counts !== undefined) {
      if (typeof authorities.counts !== 'object' || authorities.counts === null) return true;
      for (const v of Object.values(authorities.counts as Record<string, unknown>)) {
        if (typeof v !== 'number') return true;
      }
    }
  }
  const keychainItems = shape.keychainItems as { count?: unknown } | undefined;
  if (keychainItems && typeof keychainItems.count !== 'number') return true;
  return false;
}

export interface Check10Input {
  readonly contractSnapshotJsonPath: string;
  readonly packageJsonPath: string;
  readonly currentGeneratorVersion: string;
  /** B3(§12.4) — `sensitive-classes.json.taxonomyIdsSha256`를 스냅샷의
   * `sourceRegions["security-plan.md"/"§11.5 부류 id"]`와 대조한다. */
  readonly sensitiveClassesJsonPath: string;
}

export function checkSnapshotIntegrity(input: Check10Input): CheckResult {
  const id = '⑩';
  const label = '스냅샷 무결성·신선도 (§8.5 D4)';
  const violations: { ref: string; message: string }[] = [];

  let snapshot: Snapshot;
  try {
    snapshot = JSON.parse(readFileSync(input.contractSnapshotJsonPath, 'utf8')) as Snapshot;
  } catch (error) {
    return fail(id, label, [
      { ref: '§8.5 D4', message: `${input.contractSnapshotJsonPath}를 읽거나 파싱할 수 없습니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }

  const { digest, ...withoutDigest } = snapshot;
  const recomputed = sha256(canonicalJson(withoutDigest));
  if (digest !== recomputed) {
    violations.push({ ref: '§8.5 D4(a)', message: `self-digest 불일치: 저장된 값(${String(digest)}) !== 재계산 값(${recomputed})` });
  }

  if (snapshot.generatorVersion !== input.currentGeneratorVersion) {
    violations.push({
      ref: '§8.5 D4(b)',
      message: `generatorVersion(${String(snapshot.generatorVersion)}) !== 코드 생성기 버전(${input.currentGeneratorVersion})`,
    });
  }

  const pkg = JSON.parse(readFileSync(input.packageJsonPath, 'utf8')) as { version?: unknown };
  if (snapshot.extensionVersion !== pkg.version) {
    violations.push({
      ref: '§8.5 D4(c)',
      message: `extensionVersion(${String(snapshot.extensionVersion)}) !== package.json.version(${String(pkg.version)})`,
    });
  }

  if (hasValueLikeField(snapshot.siteShape)) {
    violations.push({ ref: '§8.5 D4(d)', message: 'siteShape에 이름·개수 이외의 값형 필드가 있습니다' });
  }

  // (e) B3(security-plan.md §12.4) — sensitive-classes.json.taxonomyIdsSha256이
  // 스냅샷의 security-plan.md §11.5 부류 id 해시와 일치해야 한다. 어긋나면 표(부류
  // 정의)와 JSON(강제 규칙)이 서로 다른 부류 집합을 말하고 있다는 뜻이다 — 손으로
  // 고치지 말고 `pnpm contract:snapshot`을 다시 돌려 이 값을 채운다(로컬 full, D1).
  let sensitiveClasses: { taxonomyIdsSha256?: unknown };
  try {
    sensitiveClasses = JSON.parse(readFileSync(input.sensitiveClassesJsonPath, 'utf8')) as { taxonomyIdsSha256?: unknown };
  } catch (error) {
    return fail(id, label, [
      ...violations,
      { ref: '§12.4 B3', message: `${input.sensitiveClassesJsonPath}를 읽거나 파싱할 수 없습니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }
  const idRegion = (snapshot.sourceRegions ?? []).find((r) => r.doc === 'security-plan.md' && r.region === '§11.5 부류 id');
  if (!idRegion) {
    violations.push({ ref: '§12.4 B3', message: '스냅샷에 security-plan.md §11.5 부류 id region이 없습니다(`pnpm contract:snapshot`을 다시 실행하십시오)' });
  } else if (sensitiveClasses.taxonomyIdsSha256 !== idRegion.sha256) {
    violations.push({
      ref: '§12.4 B3',
      message: `sensitive-classes.json.taxonomyIdsSha256(${String(sensitiveClasses.taxonomyIdsSha256)}) !== 스냅샷 §11.5 부류 id 해시(${String(idRegion.sha256)})`,
    });
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
