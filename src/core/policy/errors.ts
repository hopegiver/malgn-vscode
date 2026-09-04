// 정책 검증 에러 코드 — architecture.md §0.5 공통 규약(`MV_<PROVIDER|AREA>_<REASON>`)을 따른다.
//
// "doc-exact"로 주석된 상수는 docs/policy-contract.md(전수 검증표 또는 §2.1)에 **문자열 그대로**
// 등장한다 — 임의로 바꾸지 않는다(작업 지시 완료판정 #5). "synthesized"로 주석된 상수는 표에
// 문자열이 명시되지 않은 행에 대해 같은 명명 규약을 따라 이번 슬라이스에서 이름 붙인 것이며,
// 완료판정 #5가 요구하는 "설계 문서 정의 문자열과의 일치"는 doc-exact 상수에 대해서만 성립을
// 주장한다. 이 구분을 반환문에도 명시했다.

// --- doc-exact -------------------------------------------------------------

/** 전수 검증표 1행(schemaVersion) — "파일 전체 거부 → 다음 출처" */
export const MV_POLICY_SCHEMA_UNSUPPORTED = 'MV_POLICY_SCHEMA_UNSUPPORTED';

/** 전수 검증표 extension.downloadHint 행 — S2 authority 불일치 */
export const MV_POLICY_AUTHORITY_DENIED = 'MV_POLICY_AUTHORITY_DENIED';

/** 전수 검증표 compat.malgnAgent/.claudeCode 행 + §2.3 — PR-9 위반(넓히기 시도) */
export const MV_COMPAT_WIDENING_REJECTED = 'MV_COMPAT_WIDENING_REJECTED';

/** 전수 검증표 agent.marketplace/plugin/scope 행 — agent 전체 blocked */
export const MV_AGENT_TARGET_DENIED = 'MV_AGENT_TARGET_DENIED';

/** 전수 검증표 github.requiredScopes[] 행 */
export const MV_GITHUB_SCOPE_DENIED = 'MV_GITHUB_SCOPE_DENIED';

/** policy-contract.md §2.1 — allowedInstallTargets 격자에서 verified !== true(키 부재 포함) */
export const MV_INSTALL_TARGET_UNVERIFIED = 'MV_INSTALL_TARGET_UNVERIFIED';

/** 전수 검증표 killSwitch.disableProviders[]/rollout[].provider 행(§2.5) — `"install"`은
 * 조용히 폐기하지 않고 명시 거부한다(severity=high). */
export const MV_POLICY_KILLSWITCH_INSTALL_DENIED = 'MV_POLICY_KILLSWITCH_INSTALL_DENIED';

// --- synthesized (표에 문자열이 없는 행 · MV_POLICY_<REASON> 관례) ----------

export const MV_POLICY_MALFORMED = 'MV_POLICY_MALFORMED';
export const MV_POLICY_SIZE_EXCEEDED = 'MV_POLICY_SIZE_EXCEEDED';
export const MV_POLICY_SECRET_FIELD_DETECTED = 'MV_POLICY_SECRET_FIELD_DETECTED';
export const MV_POLICY_GENERATED_AT_INVALID = 'MV_POLICY_GENERATED_AT_INVALID';
export const MV_POLICY_EXTENSION_DOWNGRADE_REJECTED = 'MV_POLICY_EXTENSION_DOWNGRADE_REJECTED';
export const MV_POLICY_KILLSWITCH_VERSION_INVALID = 'MV_POLICY_KILLSWITCH_VERSION_INVALID';
export const MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN = 'MV_POLICY_KILLSWITCH_PROVIDER_UNKNOWN';
export const MV_POLICY_ROLLOUT_PERCENT_INVALID = 'MV_POLICY_ROLLOUT_PERCENT_INVALID';
export const MV_POLICY_MESSAGE_TRUNCATED = 'MV_POLICY_MESSAGE_TRUNCATED';
export const MV_POLICY_OTEL_ENV_KEY_DENIED = 'MV_POLICY_OTEL_ENV_KEY_DENIED';
export const MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED = 'MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED';
export const MV_POLICY_OTEL_PRIVACY_KEY_DENIED = 'MV_POLICY_OTEL_PRIVACY_KEY_DENIED';
export const MV_POLICY_OTEL_RESOURCE_ATTR_DENIED = 'MV_POLICY_OTEL_RESOURCE_ATTR_DENIED';
export const MV_POLICY_OTEL_HEADERS_KIND_INVALID = 'MV_POLICY_OTEL_HEADERS_KIND_INVALID';
export const MV_POLICY_OTEL_KEYCHAIN_ITEM_DENIED = 'MV_POLICY_OTEL_KEYCHAIN_ITEM_DENIED';
export const MV_POLICY_CLOUDFLARE_LOGIN_MODE_INVALID = 'MV_POLICY_CLOUDFLARE_LOGIN_MODE_INVALID';
export const MV_POLICY_INSTALL_MODE_INVALID = 'MV_POLICY_INSTALL_MODE_INVALID';
