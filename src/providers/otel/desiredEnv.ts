// otel provider의 desired env 조립 — architecture.md §4.2 "정본은 사내 관측 프로파일
// (정책의 otel 절)" + 작업 지시 "기존 정책 로더의 allowedOtelEnvKeys·policy.otel.env
// 검증 경로를 그대로 통과해야 합니다".
//
// 우선순위: ① `EffectivePolicy.otel.env`(이미 `core/policy/loader.ts`의 `validateOtel`을
// 통과해 화이트리스트·authority 검증이 끝난 값)가 있으면 그대로 쓴다 ② 없거나 비어
// 있으면(§4.2 "정책을 못 읽어도 최소 안전 상태로 뜬다") 사이트면 코드 상수
// (`otelDefaultEnv` + `allowedAuthorities.otel`)로 이 provider가 스스로 기본값을
// 만들되, **loader.ts와 같은 정규식·같은 화이트리스트·같은 authority 검사 함수**를
// import해 재사용한다(다시 정의하지 않는다 — 두 곳에서 같은 규칙을 각자 구현하면
// 한쪽만 바뀌는 사고가 이 프로젝트의 반복된 실패 패턴이었다, sinkGuards.ts 도입 사유와
// 동형).
//
// `OTEL_RESOURCE_ATTRIBUTES`는 이 두 경로 어디에도 없다 — policy-contract.md §2.4가
// "포함 금지"로 못박은 필드라 정책도 사이트면도 이 값을 제안할 수 없고, **런타임
// OS 로그인 사용자명**에서만 나온다(사람 승인 확정 사항: `employee.name`은 절대
// 포함하지 않는다).

import { OTEL_ENDPOINT_KEY_RE, OTEL_ENV_KEY_SHAPE_RE, OTEL_PRIVACY_KEYS } from '../../core/policy/loader.js';
import { authorityAllowed, extractHttpsAuthority } from '../../core/policy/sinkGuards.js';
import type { CodeConstants } from '../../core/policy/types.js';

/** policy-contract.md §2.4 — 프라이버시 4키는 코드 상한(O-6)이라 사이트면·정책 어느
 * 쪽에서도 오지 않는다. 정책 경로(①)는 loader.ts가 이미 "0" 아니면 폐기했으므로
 * 이 상수는 오직 기본값 경로(②)에서만 쓰인다. */
const FIXED_PRIVACY_ENV: Readonly<Record<string, string>> = {
  OTEL_LOG_USER_PROMPTS: '0',
  OTEL_LOG_TOOL_CONTENT: '0',
  OTEL_LOG_TOOL_DETAILS: '0',
  OTEL_LOG_RAW_API_BODIES: '0',
};

export interface DesiredOtelEnvResult {
  /** `allowedOtelEnvKeys` 화이트리스트를 통과한 관리 대상 env만 담는다. */
  readonly env: Readonly<Record<string, string>>;
  readonly blocked: boolean;
  readonly blockedReason?: string;
}

function otelEndpointsFromAuthority(authority: string): Record<string, string> {
  return {
    OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: `https://${authority}/v1/metrics`,
    OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: `https://${authority}/v1/logs`,
  };
}

/** 후보 env를 loader.ts와 동일한 3단 검사(형태 정규식 + 화이트리스트 → 프라이버시
 * "0" 고정 → 엔드포인트 authority)로 걸러 관리 대상만 남긴다. */
function filterByOtelPolicy(candidate: Readonly<Record<string, string>>, constants: CodeConstants): Record<string, string> {
  const known = new Set(constants.allowedOtelEnvKeys);
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(candidate)) {
    if (!OTEL_ENV_KEY_SHAPE_RE.test(key) || !known.has(key)) continue;
    if (OTEL_PRIVACY_KEYS.has(key)) {
      if (value === '0') out[key] = value;
      continue;
    }
    if (OTEL_ENDPOINT_KEY_RE.test(key)) {
      const authority = extractHttpsAuthority(value);
      if (!authority || !authorityAllowed(authority, constants.allowedAuthorities.otel)) continue;
    }
    out[key] = value;
  }
  return out;
}

/** ② 사이트면 코드 상수만으로 조립하는 기본값 — 정책이 없을 때도 OTel provider가
 * 완전히 무력화되지 않게 한다(architecture.md §4.2 "정책을 못 읽어도... 최소 안전
 * 상태로 뜬다"를 otel provider에 적용한 것). */
export function buildDefaultOtelEnv(constants: CodeConstants): DesiredOtelEnvResult {
  const authority = constants.allowedAuthorities.otel[0];
  if (!authority) {
    return { env: {}, blocked: true, blockedReason: 'allowedAuthorities.otel가 비어 있어 수집기 목적지를 만들 수 없습니다' };
  }
  const candidate: Record<string, string> = {
    ...constants.otelDefaultEnv,
    ...otelEndpointsFromAuthority(authority),
    ...FIXED_PRIVACY_ENV,
  };
  const env = filterByOtelPolicy(candidate, constants);
  return { env, blocked: false };
}

/**
 * 진입점 — `policyEnv`(EffectivePolicy.otel.env, 이미 검증 끝난 값)가 비어 있지 않으면
 * 그대로 채택하고(①), 아니면 사이트면 기본값(②)으로 떨어진다.
 */
export function buildDesiredOtelEnv(
  policyEnv: Readonly<Record<string, string>> | null,
  constants: CodeConstants
): DesiredOtelEnvResult {
  if (policyEnv && Object.keys(policyEnv).length > 0) {
    return { env: policyEnv, blocked: false };
  }
  return buildDefaultOtelEnv(constants);
}

/** `OTEL_RESOURCE_ATTRIBUTES` 값 — `employee.id`만 담는다(§4.2 "(b)안", `employee.name`
 * 절대 포함 금지). OTel 리소스 속성 문법(`key=value`, 콤마 구분)을 따른다. */
export function buildOtelResourceAttributesValue(osUsername: string): string {
  return `employee.id=${encodeURIComponent(osUsername)}`;
}

export const OTEL_RESOURCE_ATTRIBUTES_KEY = 'OTEL_RESOURCE_ATTRIBUTES';
