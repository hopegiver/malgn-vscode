// scripts/lib/sensitiveScan.mjs의 타입 선언 — check9-sensitiveValueScan.ts(TS)가 이 순수
// JS 모듈을 타입 안전하게 import하기 위한 최소 선언. 정본 구현·주석은 .mjs 쪽에 있다.
// v2(security-plan.md §12.2/§12.4) — C-0~C-7 스캐너 + B4 커버리지 게이트.

export interface SensitiveScanScopes {
  readonly quotedOnly?: readonly string[];
  readonly wholeText?: readonly string[];
  readonly default?: 'quotedOnly' | 'wholeText';
}

export interface SensitiveScanClassesConfig {
  readonly version: number;
  readonly classTaxonomy: readonly string[];
  readonly enforcedClasses: readonly string[];
  readonly taxonomyIdsSha256: string | null;
  readonly scanScopes: SensitiveScanScopes;
  readonly classes: readonly unknown[];
  readonly reservedDomainSuffixes: readonly string[];
  readonly publicAllowlist: readonly { readonly value: string; readonly covers: string; readonly reason: string }[];
  readonly pathExemptions: readonly { readonly path: string; readonly classes: readonly string[]; readonly reason: string }[];
}

export interface SensitiveScanViolation {
  readonly file: string;
  readonly classId: string;
  readonly covers: string;
  /** redactMatch(구조적으로 PUB-X/PUB-A는 항상 강제)가 참이면 `'[REDACTED]'`로 이미
   * 마스킹되어 있다 — 소비자가 별도로 마스킹할 필요가 없다. */
  readonly match: string;
  readonly redacted: boolean;
}

export function extractQuotedStrings(text: string): string[];
export function loadClassesConfig(sensitiveClassesJsonText: string): SensitiveScanClassesConfig;
export function scanText(filePath: string, text: string, config: SensitiveScanClassesConfig): SensitiveScanViolation[];
export function scanFiles(
  files: readonly { path: string; text: string }[],
  config: SensitiveScanClassesConfig
): SensitiveScanViolation[];

/** B4(§12.4) — enforcedClasses 중 활성 class로 커버되지 않는 부류 id 목록(빈 배열 = 전부 커버). */
export function getEnforcedCoverageGaps(config: SensitiveScanClassesConfig): string[];
