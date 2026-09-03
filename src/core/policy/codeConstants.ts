// 코드 상수 로더 — docs/policy-contract.md §2(compat/compatibility.json)의 정본을 읽어
// 타입이 확인된 `CodeConstants`로 만든다. 정책은 이 값을 절대 덮어쓰지 못하고
// 좁히기만 한다(PR-9) — loader.ts가 그 좁히기 연산의 유일한 입력으로 이 모듈의 결과를 쓴다.
//
// $ref 표기(§2 JSONC의 "allowedInstallTargets": {"$ref": "./install-targets.json"})는 이
// 슬라이스에서 범용 JSON $ref 리졸버로 구현하지 않는다 — 참조 대상 파일 3개(install-targets.json/
// manager-paths.json/install-env.json)가 빌드 시점에 고정돼 있으므로 TS import로 직접
// 결합한다. 일반 $ref 해석기를 새로 만드는 것은 이 계약이 요구하지 않는 범위다.

import compatibilityRaw from '../../../compat/compatibility.json';
import installTargetsRaw from '../../../compat/install-targets.json';
import managerPathsRaw from '../../../compat/manager-paths.json';
import installEnvRaw from '../../../compat/install-env.json';
// `src/generated/siteConstants.ts`는 `pnpm gen:site`의 빌드 산출물이라 git에 없다
// (src/generated/.gitignore). `pnpm test`/`pnpm compat:check`/`pnpm build`는 모두 이보다
// 먼저 `gen:site`를 강제한다(package.json의 pretest/precompat:check/prebuild + CI 명시 단계,
// docs/policy-contract.md §8.7 S2). 이 import가 실패한다는 것 자체가 "gen:site를 건너뛰고
// 빌드를 시도했다"는 신호이며, 그 실패가 곧 fail-closed 3종 중 첫째·둘째의 실제 강제 지점이다.
import { siteConstants, siteProfile } from '../../generated/siteConstants.js';
import { MV_INSTALL_TARGET_UNVERIFIED } from './errors.js';
import type { CodeConstants, InstallTargetRow, RejectedInstallTargetRow } from './types.js';

/**
 * policy-contract.md §2.1 / PR-11③ — allowedInstallTargets 격자의 **필수 열 전량**.
 * `artifactKind`·`expectedSigner`는 "미확인"이 정당한 상태(null)이므로 필수 열이 아니다 —
 * 표가 명시한 필수 열은 정확히 이 7개뿐이다.
 */
const REQUIRED_INSTALL_TARGET_COLUMNS = [
  'tool',
  'platform',
  'manager',
  'subcommand',
  'packageId',
  'strategy',
  'verified',
] as const;

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

/**
 * PR-11③ 정본 구현: "행 필수키가 없는 식별자는 존재할 수 없다". 열 하나라도 없거나
 * `verified`가 불리언 리터럴이 아니면 그 행은 **격자 밖으로 취급**한다 — 통과시키고
 * 나중에 판단을 미루지 않는다(부재는 차단). 이 검사는 표↔fixture "동치" 비교가 아니라
 * 고정된 열 스펙과의 대조다(§6 "검사 3중" ⑥의 이유: 동치 비교는 양쪽에 똑같이 없는
 * 키를 통과시켜 대칭적 누락을 잡지 못한다).
 */
export function validateInstallTargetsGrid(rows: readonly unknown[]): {
  readonly valid: readonly InstallTargetRow[];
  readonly rejected: readonly RejectedInstallTargetRow[];
} {
  const valid: InstallTargetRow[] = [];
  const rejected: RejectedInstallTargetRow[] = [];

  for (const row of rows) {
    if (row === null || typeof row !== 'object' || Array.isArray(row)) {
      rejected.push({ row, reason: '행이 객체가 아닙니다' });
      continue;
    }
    const record = row as Record<string, unknown>;
    const missing = REQUIRED_INSTALL_TARGET_COLUMNS.filter((col) => !(col in record));
    if (missing.length > 0) {
      rejected.push({ row, reason: `필수 열 누락: ${missing.join(', ')}` });
      continue;
    }
    if (typeof record.verified !== 'boolean') {
      // "false·키 부재·null·문자열 "true"·타입 불일치는 전부 격하" — 여기서는 키가
      // 있더라도 타입이 불리언이 아니면(예: "true" 문자열) 격자 밖으로 취급한다.
      rejected.push({ row, reason: `verified가 불리언 리터럴이 아닙니다: ${JSON.stringify(record.verified)}` });
      continue;
    }
    if (
      !isNonEmptyString(record.tool) ||
      !isNonEmptyString(record.platform) ||
      !isNonEmptyString(record.manager) ||
      !isNonEmptyString(record.subcommand) ||
      !isNonEmptyString(record.packageId) ||
      !isNonEmptyString(record.strategy)
    ) {
      rejected.push({ row, reason: '필수 열 값이 비어 있거나 문자열이 아닙니다' });
      continue;
    }
    valid.push({
      tool: record.tool,
      platform: record.platform,
      manager: record.manager,
      subcommand: record.subcommand,
      packageId: record.packageId,
      artifactKind: typeof record.artifactKind === 'string' ? record.artifactKind : null,
      expectedSigner: typeof record.expectedSigner === 'string' ? record.expectedSigner : null,
      verified: record.verified,
      strategy: record.strategy,
    });
  }

  return { valid, rejected };
}

let cached: CodeConstants | undefined;

/**
 * compat/*.json 4개를 결합해 `CodeConstants`를 만든다. 순수 함수(부작용 0, 네트워크·fs
 * 접근 없음 — 4개 fixture는 빌드에 정적으로 번들된다). 매 호출마다 격자를 재검증하는
 * 비용을 피하기 위해 모듈 레벨로 1회 캐시한다(입력이 빌드 시 고정이라 안전하다).
 */
export function loadCodeConstants(): CodeConstants {
  if (cached) return cached;

  const { valid } = validateInstallTargetsGrid(installTargetsRaw as readonly unknown[]);

  cached = {
    schemaVersion: compatibilityRaw.schemaVersion,
    extensionVersion: compatibilityRaw.extensionVersion,
    requires: compatibilityRaw.requires,
    known: compatibilityRaw.known as CodeConstants['known'],
    onUnknownNewer: compatibilityRaw.onUnknownNewer as 'warn',
    // 민감 슬롯(§8.4) — compatibility.json에는 {"$site":"..."} 구멍만 있고, 실값은
    // gen:site가 만든 생성 모듈에서 온다.
    siteProfile,
    allowedAuthorities: siteConstants.allowedAuthorities,
    allowedMarketplaces: compatibilityRaw.allowedMarketplaces,
    allowedPlugins: compatibilityRaw.allowedPlugins,
    allowedInstallScopes: compatibilityRaw.allowedInstallScopes,
    allowedKeychainItems: siteConstants.allowedKeychainItems,
    allowedGithubScopes: compatibilityRaw.allowedGithubScopes,
    allowedInstallTargets: valid,
    allowedManagerPaths: managerPathsRaw,
    installEnv: installEnvRaw,
  };
  return cached;
}

/** 테스트 전용 — 모듈 캐시를 지워 다른 fixture로 재로드할 수 있게 한다 */
export function resetCodeConstantsCacheForTests(): void {
  cached = undefined;
}

export type InstallTargetResolution =
  | { readonly status: 'usable'; readonly row: InstallTargetRow }
  | { readonly status: 'unverified'; readonly row: InstallTargetRow; readonly code: string }
  | { readonly status: 'not-in-grid' };

/**
 * `verified === true`(불리언 리터럴)인 행만 usable이다(§2.1 "격하 술어는 긍정
 * 화이트리스트다" — v0.6.1 정정). 격자에 없는 조합은 'not-in-grid'로, 격자에는 있지만
 * verified가 아닌 조합은 'unverified'로 **구분**한다 — 둘 다 install Change를 만들 수
 * 없다는 결과는 같지만(W7~W10의 책임), 구분 자체는 이 계약이 요구하는 정보다.
 */
export function resolveInstallTarget(
  constants: CodeConstants,
  tool: string,
  platform: string
): InstallTargetResolution {
  const row = constants.allowedInstallTargets.find(
    (r) => r.tool === tool && (r.platform === platform || r.platform === '*')
  );
  if (!row) return { status: 'not-in-grid' };
  if (row.verified !== true) {
    return { status: 'unverified', row, code: MV_INSTALL_TARGET_UNVERIFIED };
  }
  return { status: 'usable', row };
}
