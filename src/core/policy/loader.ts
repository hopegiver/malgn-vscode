// 정책 로더 — docs/policy-contract.md §1(전수 검증표)의 리프 필드 전량을 검증하고
// §2.3(PR-9 좁히기 전용 규칙)에 따라 코드 상수와 병합해 `EffectivePolicy`를 만든다.
//
// 순수 함수다: 네트워크·fs 접근이 없다(그 책임은 이 슬라이스 밖 — 작업 지시 "금지 범위"
// 참조). 호출자가 이미 손에 쥔 정책 원문 텍스트 하나를 넘기면, 검증을 통과한
// `EffectivePolicy` 또는 파일 전체 거부 사유를 돌려준다. 여러 출처(체크아웃→설치본→번들
// 내장)를 순회하는 폴백 로직은 이 함수를 반복 호출하는 상위 계층(§3.7.3, 이 슬라이스가
// 아님)의 책임이다.

import type { ProviderId } from '../../providers/types.js';
import {
  MV_AGENT_TARGET_DENIED,
  MV_COMPAT_WIDENING_REJECTED,
  MV_GITHUB_SCOPE_DENIED,
  MV_POLICY_AUTHORITY_DENIED,
  MV_POLICY_CLOUDFLARE_LOGIN_MODE_INVALID,
  MV_POLICY_EXTENSION_DOWNGRADE_REJECTED,
  MV_POLICY_GENERATED_AT_INVALID,
  MV_POLICY_INSTALL_MODE_INVALID,
  MV_POLICY_KILLSWITCH_INSTALL_DENIED,
  MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN,
  MV_POLICY_KILLSWITCH_VERSION_INVALID,
  MV_POLICY_MALFORMED,
  MV_POLICY_MESSAGE_TRUNCATED,
  MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED,
  MV_POLICY_OTEL_ENV_KEY_DENIED,
  MV_POLICY_OTEL_HEADERS_KIND_INVALID,
  MV_POLICY_OTEL_KEYCHAIN_ITEM_DENIED,
  MV_POLICY_OTEL_PRIVACY_KEY_DENIED,
  MV_POLICY_OTEL_RESOURCE_ATTR_DENIED,
  MV_POLICY_ROLLOUT_PERCENT_INVALID,
  MV_POLICY_SCHEMA_UNSUPPORTED,
  MV_POLICY_SECRET_FIELD_DETECTED,
  MV_POLICY_SIZE_EXCEEDED,
} from './errors.js';
import { authorityAllowed, extractHttpsAuthority, findSecretLikeKeyPath, hasLeadingDash, hasShellMetacharacters, isValidCredentialLookupKey } from './sinkGuards.js';
import { compareVersions, isValidSemver, narrowRange, parseVersion } from './semver.js';
import type {
  CodeConstants,
  EffectiveAgent,
  EffectiveCloudflare,
  EffectiveCompat,
  EffectiveGithub,
  EffectiveInstall,
  EffectiveKillSwitch,
  EffectiveOtel,
  EffectiveRolloutEntry,
  PolicyIssue,
  PolicyLoadResult,
} from './types.js';

/** policy-contract.md §1 — "상한 64KB" */
export const POLICY_MAX_BYTES = 64 * 1024;

/** killSwitch.disableProviders[] / rollout[].provider 허용 enum. `install`은 제외돼 있다
 * — 근거는 policy-contract.md §2.5(v1.2-stopmatrix, B-4): ① 킬스위치는 자기 해제 경로를
 * 끊을 수 없다(§3.5.3이 유일한 해소 경로로 지정한 install을 정책 한 장이 정지시키면
 * 그 순환을 끊을 수단이 없다) ② 정책이 install을 통제하는 정본 채널은 `install.mode`
 * (정지가 아니라 격하)뿐이다 ③ rollout이 나눌 대상(코호트별 목표값)이 install에는
 * 없다(§4.8.3 부트스트랩 순환 + PR-10으로 argv 전량이 코드 상수). `install`이 이 배열에
 * 들어오면 미지 값으로 조용히 버리지 않고 **명시 거부**한다(§2.5 ④ — "정책이 지정했는데
 * 엔진이 조용히 무시한다"는 불일치를 만들지 않는다. 아래 validateDisableProviders/
 * validateRollout의 MV_POLICY_KILLSWITCH_INSTALL_DENIED 분기가 그 명시 거부다). */
const KILLSWITCH_PROVIDER_IDS: readonly ProviderId[] = ['agent', 'otel', 'github', 'cloudflare', 'mcp'];

const OTEL_PRIVACY_KEYS = new Set([
  'OTEL_LOG_USER_PROMPTS',
  'OTEL_LOG_TOOL_CONTENT',
  'OTEL_LOG_TOOL_DETAILS',
  'OTEL_LOG_RAW_API_BODIES',
]);

const OTEL_ENDPOINT_KEY_RE = /^OTEL_EXPORTER_OTLP_.*_ENDPOINT$/;
const OTEL_ENV_KEY_SHAPE_RE = /^(OTEL_|CLAUDE_CODE_ENABLE_TELEMETRY$)/;

const AGENT_MARKETPLACE_RE = /^[A-Za-z0-9._-]{1,39}\/[A-Za-z0-9._-]{1,100}$/;
const AGENT_PLUGIN_RE = /^[a-z0-9-]{1,64}@[a-z0-9-]{1,64}$/;
const GITHUB_SCOPE_RE = /^[a-z]+(:[a-z]+)?$/;

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function reject(code: string, message: string, issues: readonly PolicyIssue[]): PolicyLoadResult {
  return { status: 'rejected', code, message, issues };
}

function issue(field: string, code: string, severity: PolicyIssue['severity'], message: string): PolicyIssue {
  return { field, code, severity, message };
}

// ---------------------------------------------------------------------------
// 개별 리프 필드 검증기 — 전수 검증표의 각 행에 정확히 하나씩 대응한다.
// (fieldCoverage.test.ts가 이 대응을 표와 자동 대조한다.)
// ---------------------------------------------------------------------------

function validateGeneratedAt(raw: unknown, issues: PolicyIssue[]): string | null {
  if (raw === undefined) return null;
  if (typeof raw === 'string' && !Number.isNaN(Date.parse(raw))) return raw;
  issues.push(issue('generatedAt', MV_POLICY_GENERATED_AT_INVALID, 'info', 'ISO8601 형식이 아니라 폐기했습니다'));
  return null;
}

function validateExtensionLatestVersion(
  raw: unknown,
  currentVersion: string,
  issues: PolicyIssue[]
): string | null {
  if (raw === undefined) return null;
  if (typeof raw !== 'string' || !isValidSemver(raw)) {
    issues.push(issue('extension.latestVersion', MV_POLICY_GENERATED_AT_INVALID, 'info', 'semver 형식이 아니라 폐기했습니다'));
    return null;
  }
  const cmp = compareSemverStrings(raw, currentVersion);
  if (cmp !== null && cmp <= 0) {
    issues.push(
      issue(
        'extension.latestVersion',
        MV_POLICY_EXTENSION_DOWNGRADE_REJECTED,
        cmp < 0 ? 'high' : 'info',
        '현재 버전보다 낮거나 같아 다운그레이드 유도로 간주해 폐기했습니다'
      )
    );
    return null;
  }
  return raw;
}

function compareSemverStrings(a: string, b: string): number | null {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  if (!pa || !pb) return null;
  return compareVersions(pa, pb);
}

function validateExtensionDownloadHint(
  raw: unknown,
  constants: CodeConstants,
  issues: PolicyIssue[]
): string | null {
  if (raw === undefined) return null;
  if (typeof raw !== 'string') {
    issues.push(issue('extension.downloadHint', MV_POLICY_AUTHORITY_DENIED, 'high', '문자열이 아닙니다'));
    return null;
  }
  const authority = extractHttpsAuthority(raw);
  if (!authority || !authorityAllowed(authority, constants.allowedAuthorities.extension)) {
    issues.push(issue('extension.downloadHint', MV_POLICY_AUTHORITY_DENIED, 'high', `허용되지 않은 목적지: ${raw}`));
    return null;
  }
  return raw;
}

function validateKillSwitchVersion(raw: unknown, field: string, issues: PolicyIssue[]): string | null {
  if (raw === undefined || raw === null) return null;
  if (typeof raw === 'string' && isValidSemver(raw)) return raw;
  issues.push(issue(field, MV_POLICY_KILLSWITCH_VERSION_INVALID, 'info', 'semver 또는 null이 아니라 폐기했습니다'));
  return null;
}

function validateDisableProviders(raw: unknown, issues: PolicyIssue[]): readonly ProviderId[] {
  if (raw === undefined) return [];
  if (!Array.isArray(raw)) {
    issues.push(issue('killSwitch.disableProviders', MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN, 'info', '배열이 아닙니다'));
    return [];
  }
  const result: ProviderId[] = [];
  for (const entry of raw) {
    if (entry === 'install') {
      // policy-contract.md §2.5 전수 검증표 원문 — 미지 값 폐기가 아니라 명시 거부(severity high).
      issues.push(
        issue(
          'killSwitch.disableProviders[]',
          MV_POLICY_KILLSWITCH_INSTALL_DENIED,
          'high',
          'install은 킬스위치 대상이 아닙니다 — install.mode: guided-only를 쓰십시오'
        )
      );
      continue;
    }
    if (typeof entry === 'string' && (KILLSWITCH_PROVIDER_IDS as readonly string[]).includes(entry)) {
      result.push(entry as ProviderId);
    } else {
      issues.push(issue('killSwitch.disableProviders[]', MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN, 'info', `알 수 없는 provider id: ${String(entry)}`));
    }
  }
  return result;
}

function validateMessageLike(raw: unknown, field: string, issues: PolicyIssue[]): string | null {
  if (raw === undefined || raw === null) return null;
  if (typeof raw !== 'string') return null;
  // 제어문자(C0 + DEL) 제거 -- "길이 <=512, 제어문자 제거"(S6 공통 통제)
  let stripped = '';
  for (const ch of raw) {
    const code = ch.codePointAt(0) ?? 0;
    if (code <= 0x1f || code === 0x7f) continue;
    stripped += ch;
  }
  if (stripped.length <= 512) return stripped;
  issues.push(issue(field, MV_POLICY_MESSAGE_TRUNCATED, 'info', '512자를 넘어 절삭했습니다'));
  return stripped.slice(0, 512);
}

function validateRollout(raw: unknown, issues: PolicyIssue[]): readonly EffectiveRolloutEntry[] {
  if (raw === undefined) return [];
  if (!Array.isArray(raw)) {
    issues.push(issue('rollout', MV_POLICY_ROLLOUT_PERCENT_INVALID, 'info', '배열이 아닙니다'));
    return [];
  }
  const result: EffectiveRolloutEntry[] = [];
  for (const entry of raw) {
    if (!isPlainObject(entry)) {
      issues.push(issue('rollout[]', MV_POLICY_ROLLOUT_PERCENT_INVALID, 'info', '항목이 객체가 아닙니다'));
      continue;
    }
    const provider = entry.provider;
    const percent = entry.percent;
    if (provider === 'install') {
      // policy-contract.md §2.5 전수 검증표 원문 — 미지 값 폐기가 아니라 명시 거부(severity high).
      issues.push(
        issue(
          'rollout[].provider',
          MV_POLICY_KILLSWITCH_INSTALL_DENIED,
          'high',
          'install은 킬스위치 대상이 아닙니다 — install.mode: guided-only를 쓰십시오'
        )
      );
      continue;
    }
    const providerValid = typeof provider === 'string' && (KILLSWITCH_PROVIDER_IDS as readonly string[]).includes(provider);
    const percentValid = typeof percent === 'number' && Number.isInteger(percent) && percent >= 0 && percent <= 100;
    if (!providerValid) {
      issues.push(issue('rollout[].provider', MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN, 'info', `알 수 없는 provider id: ${String(provider)}`));
      continue;
    }
    if (!percentValid) {
      issues.push(issue('rollout[].percent', MV_POLICY_ROLLOUT_PERCENT_INVALID, 'info', `0~100 정수가 아닙니다: ${String(percent)}`));
      continue;
    }
    result.push({ provider: provider as ProviderId, percent });
  }
  return result;
}

function validateCompatField(
  fieldName: 'malgnAgent' | 'claudeCode',
  raw: unknown,
  bundled: string,
  issues: PolicyIssue[]
): string {
  if (raw === undefined || typeof raw !== 'string') return bundled;
  const result = narrowRange(bundled, raw);
  if (!result) return bundled; // 정책 값 파싱 불가 = 형식 오류(폭 시도 아님) → 조용히 번들 유지
  if (result.widened) {
    issues.push(
      issue(
        `compat.${fieldName}`,
        MV_COMPAT_WIDENING_REJECTED,
        'high',
        `정책이 호환 범위를 넓히려 시도해 번들 값을 유지했습니다 (정책 값: ${raw}, 번들 값: ${bundled})`
      )
    );
  }
  return result.effective;
}

function validateAgent(raw: unknown, constants: CodeConstants, issues: PolicyIssue[]): EffectiveAgent {
  const deny = (field: string, reason: string): EffectiveAgent => {
    issues.push(issue(field, MV_AGENT_TARGET_DENIED, 'high', reason));
    return { blocked: true, reason };
  };

  if (raw === undefined) {
    // PR-6(파괴적 동작 fail-closed): 정책이 agent 대상을 아예 지정하지 않으면 추측해
    // 기본값을 고르지 않고 차단한다. (allowedMarketplaces/Plugins/InstallScopes가
    // 지금은 우연히 원소 1개씩이지만, 그 사실에 의존해 "유일한 값이니 기본으로 쓴다"는
    // 암묵적 규칙을 만들지 않는다 — 목록이 늘어나면 그 규칙이 조용히 깨진다.)
    return deny('agent', '정책에 agent 대상이 지정되지 않았습니다');
  }
  if (!isPlainObject(raw)) return deny('agent', 'agent 필드가 객체가 아닙니다');

  const marketplace = raw.marketplace;
  const plugin = raw.plugin;
  const scope = raw.scope;
  const channelRaw = raw.channel;

  if (typeof marketplace !== 'string' || typeof plugin !== 'string' || typeof scope !== 'string') {
    return deny('agent', 'agent.marketplace/plugin/scope 중 필수 필드가 없습니다');
  }
  if (!AGENT_MARKETPLACE_RE.test(marketplace) || hasShellMetacharacters(marketplace) || hasLeadingDash(marketplace)) {
    return deny('agent.marketplace', `agent.marketplace 형식 위반: ${marketplace}`);
  }
  if (!constants.allowedMarketplaces.includes(marketplace)) {
    return deny('agent.marketplace', `allowedMarketplaces 밖의 값: ${marketplace}`);
  }
  if (!AGENT_PLUGIN_RE.test(plugin) || hasShellMetacharacters(plugin) || hasLeadingDash(plugin)) {
    return deny('agent.plugin', `agent.plugin 형식 위반: ${plugin}`);
  }
  if (!constants.allowedPlugins.includes(plugin)) {
    return deny('agent.plugin', `allowedPlugins 밖의 값: ${plugin}`);
  }
  if (!constants.allowedInstallScopes.includes(scope)) {
    return deny('agent.scope', `allowedInstallScopes 밖의 값: ${scope}`);
  }
  const channel = channelRaw === 'beta' ? 'beta' : 'stable';
  if (channelRaw !== undefined && channelRaw !== 'stable' && channelRaw !== 'beta') {
    issues.push(issue('agent.channel', MV_AGENT_TARGET_DENIED, 'info', `stable|beta가 아니라 stable을 사용합니다: ${String(channelRaw)}`));
  }

  return { blocked: false, marketplace, plugin, scope, channel };
}

/**
 * otel.headersHelper.kind 위반은 "필드 폐기"가 아니라 **파일 전체 거부**다(전수 검증표
 * 명시). 이 신호를 문자열 비교가 아니라 명시적 유니온으로 표현해 다음 함수에 넘긴다.
 */
type OtelValidation = { readonly fileRejected: true } | { readonly fileRejected: false; readonly otel: EffectiveOtel };

function validateOtel(raw: unknown, constants: CodeConstants, issues: PolicyIssue[]): OtelValidation {
  if (raw === undefined) {
    return { fileRejected: false, otel: { blocked: false, env: {}, headersHelper: null } };
  }
  if (!isPlainObject(raw)) {
    return { fileRejected: false, otel: { blocked: true, blockedReason: 'otel 필드가 객체가 아닙니다', env: {}, headersHelper: null } };
  }

  // policy-contract.md §2.4 — 값 정본은 compat/compatibility.json.allowedOtelEnvKeys(9개).
  // 코드 상수 로더(codeConstants.ts)가 실은 값을 그대로 쓴다 — 이 함수 안에서 목록을
  // 다시 정의하지 않는다(PR-7 단일 정본 값).
  const knownOtelEnvKeys = new Set(constants.allowedOtelEnvKeys);

  let blocked = false;
  let blockedReason: string | undefined;
  const env: Record<string, string> = {};

  const envRaw = raw.env;
  if (envRaw !== undefined) {
    if (!isPlainObject(envRaw)) {
      issues.push(issue('otel.env', MV_POLICY_OTEL_ENV_KEY_DENIED, 'info', 'env가 객체가 아니라 무시했습니다'));
    } else {
      for (const [key, value] of Object.entries(envRaw)) {
        if (key === 'OTEL_RESOURCE_ATTRIBUTES') {
          issues.push(issue(`otel.env.${key}`, MV_POLICY_OTEL_RESOURCE_ATTR_DENIED, 'high', 'PII 확장 우회 경로라 포함을 금지합니다'));
          blocked = true;
          blockedReason = blockedReason ?? 'OTEL_RESOURCE_ATTRIBUTES 포함 시도';
          continue;
        }
        if (typeof value !== 'string') {
          issues.push(issue(`otel.env.${key}`, MV_POLICY_OTEL_ENV_KEY_DENIED, 'info', '값이 문자열이 아니라 키를 폐기했습니다'));
          continue;
        }
        if (OTEL_PRIVACY_KEYS.has(key)) {
          if (value !== '0') {
            issues.push(issue(`otel.env.${key}`, MV_POLICY_OTEL_PRIVACY_KEY_DENIED, 'high', `프라이버시 키는 "0"만 허용됩니다: ${value}`));
            continue;
          }
          env[key] = value;
          continue;
        }
        // A-31(F-4, security-report.md) — 순서 교정: 닫힌 화이트리스트를 먼저 확인한다.
        // 이전에는 엔드포인트 분기가 이 검사보다 먼저 돌아 화이트리스트에 없는 임의의
        // `OTEL_EXPORTER_OTLP_*_ENDPOINT` 키(예: TRACES_ENDPOINT, 또는 임의 접미 키)가
        // authority만 통과하면 `env[key] = value; continue;`로 닫힌 목록을 건너뛰고
        // effective env에 실렸다(실측 재현, F-4). 대조군인 `..._HEADERS`는 이미 닫힌
        // 목록에서 폐기되고 있었다 — 그 정상 동작과 같은 경로를 엔드포인트 키에도
        // 적용한다. 순서를 바꾸면: 화이트리스트 밖의 엔드포인트 키는 authority 검사에
        // 도달하지 않고 그대로 폐기된다(info, blocked 아님) — HEADERS와 동일한 "폐기됨"
        // 취급이다. METRICS_ENDPOINT/LOGS_ENDPOINT(화이트리스트 안)는 그대로 authority
        // 검사를 받는다.
        if (!OTEL_ENV_KEY_SHAPE_RE.test(key) || !knownOtelEnvKeys.has(key)) {
          issues.push(issue(`otel.env.${key}`, MV_POLICY_OTEL_ENV_KEY_DENIED, 'info', `알려진 키 화이트리스트 밖이라 폐기했습니다: ${key}`));
          continue;
        }
        if (OTEL_ENDPOINT_KEY_RE.test(key)) {
          const authority = extractHttpsAuthority(value);
          if (!authority || !authorityAllowed(authority, constants.allowedAuthorities.otel)) {
            issues.push(issue(`otel.env.${key}`, MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED, 'high', `허용되지 않은 수집기 목적지: ${value}`));
            blocked = true;
            blockedReason = blockedReason ?? `${key} 목적지 거부`;
            continue;
          }
        }
        env[key] = value;
      }
    }
  }

  const headersHelperRaw = raw.headersHelper;
  let headersHelper: EffectiveOtel['headersHelper'] = null;
  if (headersHelperRaw !== undefined) {
    if (!isPlainObject(headersHelperRaw) || (headersHelperRaw.kind !== 'keychain-basic' && headersHelperRaw.kind !== 'none')) {
      // "임의 스크립트 경로 지정 불가" — kind가 없거나 enum 밖이면 파일 전체 거부
      issues.push(issue('otel.headersHelper.kind', MV_POLICY_OTEL_HEADERS_KIND_INVALID, 'high', 'kind가 keychain-basic|none이 아닙니다'));
      return { fileRejected: true };
    }
    const kind = headersHelperRaw.kind;
    let service: string | null = null;
    let account: string | null = null;
    if (kind === 'keychain-basic') {
      const serviceRaw = headersHelperRaw.service;
      const accountRaw = headersHelperRaw.account;
      if (
        typeof serviceRaw === 'string' &&
        isValidCredentialLookupKey(serviceRaw) &&
        constants.allowedKeychainItems.includes(serviceRaw)
      ) {
        service = serviceRaw;
      } else {
        issues.push(issue('otel.headersHelper.service', MV_POLICY_OTEL_KEYCHAIN_ITEM_DENIED, 'high', `허용되지 않은 keychain 항목: ${String(serviceRaw)}`));
        blocked = true;
        blockedReason = blockedReason ?? 'headersHelper.service 거부';
      }
      if (
        typeof accountRaw === 'string' &&
        isValidCredentialLookupKey(accountRaw) &&
        constants.allowedKeychainItems.includes(accountRaw)
      ) {
        account = accountRaw;
      } else {
        issues.push(issue('otel.headersHelper.account', MV_POLICY_OTEL_KEYCHAIN_ITEM_DENIED, 'high', `허용되지 않은 keychain 항목: ${String(accountRaw)}`));
        blocked = true;
        blockedReason = blockedReason ?? 'headersHelper.account 거부';
      }
    }
    headersHelper = { kind, service, account };
  }

  return { fileRejected: false, otel: { blocked, blockedReason, env, headersHelper } };
}

function validateGithub(raw: unknown, constants: CodeConstants, issues: PolicyIssue[]): EffectiveGithub {
  const scopesRaw = isPlainObject(raw) ? raw.requiredScopes : undefined;
  if (scopesRaw === undefined) {
    return { blocked: true, requiredScopes: [] };
  }
  if (!Array.isArray(scopesRaw)) {
    issues.push(issue('github.requiredScopes', MV_GITHUB_SCOPE_DENIED, 'high', '배열이 아니라 무시했습니다'));
    return { blocked: true, requiredScopes: [] };
  }
  const limited = scopesRaw.slice(0, 10);
  const result: string[] = [];
  for (const scope of limited) {
    if (
      typeof scope === 'string' &&
      scope.length <= 32 &&
      GITHUB_SCOPE_RE.test(scope) &&
      !hasShellMetacharacters(scope) &&
      constants.allowedGithubScopes.includes(scope)
    ) {
      result.push(scope);
    } else {
      issues.push(issue('github.requiredScopes[]', MV_GITHUB_SCOPE_DENIED, 'high', `허용되지 않은 scope: ${String(scope)}`));
    }
  }
  return { blocked: result.length === 0, requiredScopes: result };
}

function validateCloudflare(raw: unknown, issues: PolicyIssue[]): EffectiveCloudflare {
  const loginModeRaw = isPlainObject(raw) ? raw.loginMode : undefined;
  if (loginModeRaw === 'wrangler-oauth' || loginModeRaw === 'manual-token') {
    return { loginMode: loginModeRaw };
  }
  if (loginModeRaw !== undefined) {
    issues.push(issue('cloudflare.loginMode', MV_POLICY_CLOUDFLARE_LOGIN_MODE_INVALID, 'info', `enum 밖이라 기본값을 사용합니다: ${String(loginModeRaw)}`));
  }
  return { loginMode: 'wrangler-oauth' };
}

function validateInstall(raw: unknown, issues: PolicyIssue[]): EffectiveInstall {
  const modeRaw = isPlainObject(raw) ? raw.mode : undefined;
  if (modeRaw === 'assisted' || modeRaw === 'guided-only') {
    return { mode: modeRaw };
  }
  if (modeRaw !== undefined) {
    issues.push(issue('install.mode', MV_POLICY_INSTALL_MODE_INVALID, 'info', `enum 밖이라 기본값을 사용합니다: ${String(modeRaw)}`));
  }
  return { mode: 'assisted' };
}

// ---------------------------------------------------------------------------
// 진입점
// ---------------------------------------------------------------------------

export interface LoadPolicyOptions {
  readonly currentExtensionVersion: string;
}

export function loadPolicyFromText(rawText: string, constants: CodeConstants, options: LoadPolicyOptions): PolicyLoadResult {
  const issues: PolicyIssue[] = [];

  const byteLength = Buffer.byteLength(rawText, 'utf8');
  if (byteLength > POLICY_MAX_BYTES) {
    return reject(MV_POLICY_SIZE_EXCEEDED, `정책 파일이 ${POLICY_MAX_BYTES}바이트 상한을 넘었습니다 (${byteLength}바이트)`, issues);
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(rawText);
  } catch (error) {
    return reject(MV_POLICY_MALFORMED, `JSON 파싱 실패: ${error instanceof Error ? error.message : String(error)}`, issues);
  }
  if (!isPlainObject(parsed)) {
    return reject(MV_POLICY_MALFORMED, '정책 파일의 최상위 값이 객체가 아닙니다', issues);
  }

  // PR-5(정책 무비밀) — 문서 전체. 다른 무엇보다 먼저 본다: 이 검사를 통과하지 못하면
  // 나머지 필드를 파싱할 이유가 없다(사고 신호이므로 조기 종료가 곧 안전이다).
  const secretPath = findSecretLikeKeyPath(parsed);
  if (secretPath) {
    return reject(MV_POLICY_SECRET_FIELD_DETECTED, `시크릿 형태의 키 이름이 발견됐습니다: ${secretPath}`, issues);
  }

  // schemaVersion — 전수 검증표 1행. PR-11①의 "부재는 차단" 첫 실증 지점: 필드가
  // 아예 없어도(=undefined) 조용히 통과시키지 않고 파일 전체를 거부한다.
  const schemaVersionRaw = parsed.schemaVersion;
  const manifestRange = constants.requires.manifestSchema;
  const schemaVersionValid =
    typeof schemaVersionRaw === 'number' &&
    Number.isInteger(schemaVersionRaw) &&
    schemaVersionRaw >= manifestRange.min &&
    schemaVersionRaw <= manifestRange.max;
  if (!schemaVersionValid) {
    return reject(
      MV_POLICY_SCHEMA_UNSUPPORTED,
      `schemaVersion이 지원 범위(${manifestRange.min}~${manifestRange.max}) 밖이거나 없습니다: ${String(schemaVersionRaw)}`,
      issues
    );
  }
  const schemaVersion = schemaVersionRaw;

  const extensionRaw = parsed.extension;
  const extension = {
    latestVersion: validateExtensionLatestVersion(
      isPlainObject(extensionRaw) ? extensionRaw.latestVersion : undefined,
      options.currentExtensionVersion,
      issues
    ),
    downloadHint: validateExtensionDownloadHint(isPlainObject(extensionRaw) ? extensionRaw.downloadHint : undefined, constants, issues),
  };

  const killSwitchRaw = parsed.killSwitch;
  const killSwitch: EffectiveKillSwitch = {
    minExtensionVersion: validateKillSwitchVersion(
      isPlainObject(killSwitchRaw) ? killSwitchRaw.minExtensionVersion : undefined,
      'killSwitch.minExtensionVersion',
      issues
    ),
    maxExtensionVersion: validateKillSwitchVersion(
      isPlainObject(killSwitchRaw) ? killSwitchRaw.maxExtensionVersion : undefined,
      'killSwitch.maxExtensionVersion',
      issues
    ),
    disableProviders: validateDisableProviders(isPlainObject(killSwitchRaw) ? killSwitchRaw.disableProviders : undefined, issues),
    message: validateMessageLike(isPlainObject(killSwitchRaw) ? killSwitchRaw.message : undefined, 'killSwitch.message', issues),
    upgradeHint: validateMessageLike(isPlainObject(killSwitchRaw) ? killSwitchRaw.upgradeHint : undefined, 'killSwitch.upgradeHint', issues),
  };

  const rollout = validateRollout(parsed.rollout, issues);

  const compatRaw = parsed.compat;
  const compat: EffectiveCompat = {
    malgnAgent: validateCompatField('malgnAgent', isPlainObject(compatRaw) ? compatRaw.malgnAgent : undefined, constants.requires.malgnAgent, issues),
    claudeCode: validateCompatField('claudeCode', isPlainObject(compatRaw) ? compatRaw.claudeCode : undefined, constants.requires.claudeCode, issues),
  };

  const agent = validateAgent(parsed.agent, constants, issues);
  const otelValidation = validateOtel(parsed.otel, constants, issues);
  if (otelValidation.fileRejected) {
    // 여기까지 쌓인 다른 필드 검증 issue들도 함께 반환해 진단에 남긴다.
    return reject(MV_POLICY_OTEL_HEADERS_KIND_INVALID, 'otel.headersHelper.kind가 유효하지 않아 정책 파일 전체를 거부했습니다', issues);
  }
  const otel = otelValidation.otel;

  const install = validateInstall(parsed.install, issues);
  const github = validateGithub(parsed.github, constants, issues);
  const cloudflare = validateCloudflare(parsed.cloudflare, issues);

  const generatedAt = validateGeneratedAt(parsed.generatedAt, issues);

  return {
    status: 'ok',
    policy: {
      schemaVersion,
      generatedAt,
      extension,
      killSwitch,
      rollout,
      compat,
      agent,
      otel,
      install,
      github,
      cloudflare,
    },
    issues,
  };
}
