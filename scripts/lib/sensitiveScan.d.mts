// scripts/lib/sensitiveScan.mjs의 타입 선언 — check9-sensitiveValueScan.ts(TS)가 이 순수
// JS 모듈을 타입 안전하게 import하기 위한 최소 선언. 정본 구현·주석은 .mjs 쪽에 있다.

export interface SensitiveScanClassesConfig {
  readonly ipv4ExemptCidrs: readonly string[];
  readonly recognizedTlds: ReadonlySet<string>;
  readonly publicAllowlist: readonly string[];
  readonly reservedDomainSuffixes: readonly string[];
}

export interface SensitiveScanViolation {
  readonly file: string;
  readonly classId: string;
  readonly match: string;
}

export function extractQuotedStrings(text: string): string[];
export function loadClassesConfig(sensitiveClassesJsonText: string): SensitiveScanClassesConfig;
export function scanText(filePath: string, text: string, config: SensitiveScanClassesConfig): SensitiveScanViolation[];
export function scanFiles(
  files: readonly { path: string; text: string }[],
  config: SensitiveScanClassesConfig
): SensitiveScanViolation[];
