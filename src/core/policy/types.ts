// 정책 계약 타입 — docs/policy-contract.md §1(정책 스키마)·§2(코드 상수)의 정본을
// TypeScript로 옮긴 것. 이 파일은 "무엇이 유효한 결과 형태인가"만 정의하고 검증 로직은
// loader.ts/codeConstants.ts가 담당한다.

import type { ProviderId } from '../../providers/types.js';

// ---------------------------------------------------------------------------
// 코드 상수 (compat/*.json) — policy-contract.md §2
// ---------------------------------------------------------------------------

export interface ManifestSchemaRange {
  readonly min: number;
  readonly max: number;
}

export interface CompatRequires {
  readonly claudeCode: string;
  readonly malgnAgent: string;
  readonly manifestSchema: ManifestSchemaRange;
}

export interface KnownVersionEntry {
  readonly component: string;
  readonly range: string;
  readonly verdict: 'block-apply';
  readonly reason: string;
  readonly action: string;
}

export interface AllowedAuthorities {
  readonly otel: readonly string[];
  readonly extension: readonly string[];
  readonly identity: readonly string[];
  readonly hub: readonly string[];
  readonly mcp: readonly string[];
}

export type AuthorityClass = keyof AllowedAuthorities;

/** policy-contract.md §2.1 — allowedInstallTargets 격자의 필수 열 전량(PR-11 ③) */
export interface InstallTargetRow {
  readonly tool: string;
  readonly platform: string;
  readonly manager: string;
  readonly subcommand: string;
  readonly packageId: string;
  readonly artifactKind: string | null;
  readonly expectedSigner: string | null;
  readonly verified: boolean;
  readonly strategy: string;
}

export interface CodeConstants {
  readonly schemaVersion: number;
  readonly extensionVersion: string;
  readonly requires: CompatRequires;
  readonly known: readonly KnownVersionEntry[];
  readonly onUnknownNewer: 'warn';
  readonly allowedAuthorities: AllowedAuthorities;
  readonly allowedMarketplaces: readonly string[];
  readonly allowedPlugins: readonly string[];
  readonly allowedInstallScopes: readonly string[];
  readonly allowedKeychainItems: readonly string[];
  readonly allowedGithubScopes: readonly string[];
  /** PR-11③ 검사를 통과한 행만 담긴다 — 필수 열이 빠진 행은 여기 없다(격자 밖 취급) */
  readonly allowedInstallTargets: readonly InstallTargetRow[];
  readonly allowedManagerPaths: Readonly<Record<string, readonly string[]>>;
  readonly installEnv: Readonly<Record<string, string>>;
}

/** allowedInstallTargets 원본 fixture 로드 시 열 스펙 미달로 제외된 행의 진단 정보 */
export interface RejectedInstallTargetRow {
  readonly row: unknown;
  readonly reason: string;
}

// ---------------------------------------------------------------------------
// 정책 파일 (workstation-profile.json) 검증 결과 — policy-contract.md §1
// ---------------------------------------------------------------------------

export type PolicySeverity = 'info' | 'high';

export interface PolicyIssue {
  /** 정책 스키마 상의 점 경로. 예: 'compat.claudeCode', 'agent.marketplace' */
  readonly field: string;
  readonly code: string;
  readonly severity: PolicySeverity;
  readonly message: string;
}

export interface EffectiveExtension {
  readonly latestVersion: string | null;
  readonly downloadHint: string | null;
}

export interface EffectiveKillSwitch {
  readonly minExtensionVersion: string | null;
  readonly maxExtensionVersion: string | null;
  readonly disableProviders: readonly ProviderId[];
  readonly message: string | null;
  readonly upgradeHint: string | null;
}

export interface EffectiveRolloutEntry {
  readonly provider: ProviderId;
  readonly percent: number;
}

export interface EffectiveCompat {
  readonly malgnAgent: string;
  readonly claudeCode: string;
}

export type EffectiveAgent =
  | { readonly blocked: false; readonly marketplace: string; readonly plugin: string; readonly scope: string; readonly channel: 'stable' | 'beta' }
  | { readonly blocked: true; readonly reason: string };

export interface EffectiveHeadersHelper {
  readonly kind: 'keychain-basic' | 'none';
  readonly service: string | null;
  readonly account: string | null;
}

export interface EffectiveOtel {
  readonly blocked: boolean;
  readonly blockedReason?: string;
  readonly env: Readonly<Record<string, string>>;
  readonly headersHelper: EffectiveHeadersHelper | null;
}

export interface EffectiveInstall {
  readonly mode: 'assisted' | 'guided-only';
}

export interface EffectiveGithub {
  readonly blocked: boolean;
  readonly requiredScopes: readonly string[];
}

export interface EffectiveCloudflare {
  readonly loginMode: 'wrangler-oauth' | 'manual-token';
}

/**
 * 검증·좁히기를 모두 통과한 뒤 provider들이 실제로 소비할 형태.
 * §3.7.0에 따라 이 값의 "출처"(체크아웃/설치본/번들 내장, 폴백 순서)는 이 슬라이스의
 * 책임이 아니다 — loader는 이미 손에 쥔 정책 원문 텍스트 하나를 검증·병합할 뿐이다.
 */
export interface EffectivePolicy {
  readonly schemaVersion: number;
  readonly generatedAt: string | null;
  readonly extension: EffectiveExtension;
  readonly killSwitch: EffectiveKillSwitch;
  readonly rollout: readonly EffectiveRolloutEntry[];
  readonly compat: EffectiveCompat;
  readonly agent: EffectiveAgent;
  readonly otel: EffectiveOtel;
  readonly install: EffectiveInstall;
  readonly github: EffectiveGithub;
  readonly cloudflare: EffectiveCloudflare;
}

export type PolicyLoadResult =
  | { readonly status: 'rejected'; readonly code: string; readonly message: string; readonly issues: readonly PolicyIssue[] }
  | { readonly status: 'ok'; readonly policy: EffectivePolicy; readonly issues: readonly PolicyIssue[] };
