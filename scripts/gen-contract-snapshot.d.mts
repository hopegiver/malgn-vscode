// scripts/gen-contract-snapshot.mjs의 타입 선언 — compatCheck.gate.test.ts(TS)와
// check10-snapshotIntegrity.ts가 이 순수 함수/상수를 재사용하기 위한 최소 선언.

export const GENERATOR_VERSION: string;
export const SNAPSHOT_VERSION: number;

export interface ContractSnapshot {
  readonly snapshotVersion: number;
  readonly generatorVersion: string;
  readonly extensionVersion: string;
  readonly docIdentifiers: readonly string[];
  readonly leafRows: readonly string[];
  readonly siteShape: {
    readonly authorities: { readonly keys: readonly string[]; readonly counts: Readonly<Record<string, number>> };
    readonly keychainItems: { readonly count: number };
  };
  readonly sourceRegions: readonly { readonly doc: string; readonly region: string; readonly sha256: string }[];
  readonly digest: string;
}

export function generateContractSnapshot(args: {
  readonly policyContractMdText: string;
  readonly architectureMdText: string;
  readonly securityPlanMdText?: string;
  readonly extensionVersion: string;
}): ContractSnapshot;

export function main(rootOverride?: string): ContractSnapshot;

/** security-plan.md §11.5 표 첫 열에서 부류 id(PUB-X 등)만 뽑는다 (§12.4 B1/B2). */
export function extractPubClassIds(securityPlanMdText: string): readonly string[];

/** security-plan.md §11.5 표 전문(헤더+데이터 행 원문)을 이어붙인다 (§12.4 B5, 로컬 full 전용). */
export function extractPubClassTableFullText(securityPlanMdText: string): string;
