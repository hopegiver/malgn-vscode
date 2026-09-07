import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { buildDefaultOtelEnv, buildDesiredOtelEnv, buildOtelResourceAttributesValue, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';

const constants = loadCodeConstants();

describe('buildDefaultOtelEnv — 사이트면 코드 상수 기본값', () => {
  it('otelDefaultEnv + allowedAuthorities.otel[0]로 엔드포인트 2개 + 프라이버시 4키를 조립한다', () => {
    const result = buildDefaultOtelEnv(constants);
    expect(result.blocked).toBe(false);
    const authority = constants.allowedAuthorities.otel[0];
    expect(result.env.OTEL_EXPORTER_OTLP_METRICS_ENDPOINT).toBe(`https://${authority}/v1/metrics`);
    expect(result.env.OTEL_EXPORTER_OTLP_LOGS_ENDPOINT).toBe(`https://${authority}/v1/logs`);
    expect(result.env.OTEL_LOG_USER_PROMPTS).toBe('0');
    expect(result.env.OTEL_LOG_TOOL_CONTENT).toBe('0');
    expect(result.env.OTEL_LOG_TOOL_DETAILS).toBe('0');
    expect(result.env.OTEL_LOG_RAW_API_BODIES).toBe('0');
    for (const key of Object.keys(constants.otelDefaultEnv)) {
      expect(result.env[key]).toBe(constants.otelDefaultEnv[key]);
    }
  });

  it('결과 키는 전부 allowedOtelEnvKeys(9개) 부분집합이다(§2.4 화이트리스트 재사용)', () => {
    const result = buildDefaultOtelEnv(constants);
    const known = new Set(constants.allowedOtelEnvKeys);
    for (const key of Object.keys(result.env)) {
      expect(known.has(key)).toBe(true);
    }
  });

  it('OTEL_RESOURCE_ATTRIBUTES는 절대 포함하지 않는다(§2.4 별도 금지)', () => {
    const result = buildDefaultOtelEnv(constants);
    expect(result.env[OTEL_RESOURCE_ATTRIBUTES_KEY]).toBeUndefined();
  });

  it('allowedAuthorities.otel가 비어 있으면 blocked=true(구성 오류를 추측하지 않는다)', () => {
    const emptyConstants = { ...constants, allowedAuthorities: { ...constants.allowedAuthorities, otel: [] } };
    const result = buildDefaultOtelEnv(emptyConstants);
    expect(result.blocked).toBe(true);
    expect(result.env).toEqual({});
  });

  it('허용 목적지 밖 엔드포인트가 섞여도(가상의 otelDefaultEnv 오염) 그 키만 폐기된다', () => {
    const poisoned = { ...constants, otelDefaultEnv: { ...constants.otelDefaultEnv, OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://evil.invalid/v1/metrics' } };
    const result = buildDefaultOtelEnv(poisoned);
    // 실제 authority 기반 엔드포인트가 뒤에서 다시 덮어써 여전히 허용값이어야 한다
    expect(result.env.OTEL_EXPORTER_OTLP_METRICS_ENDPOINT).toBe(`https://${constants.allowedAuthorities.otel[0]}/v1/metrics`);
  });
});

describe('buildDesiredOtelEnv — 정책 우선, 없으면 사이트면 기본값', () => {
  it('policyEnv가 비어있지 않으면 그대로 채택한다(이미 loader.ts 검증을 통과한 값)', () => {
    const policyEnv = { CLAUDE_CODE_ENABLE_TELEMETRY: '1' };
    const result = buildDesiredOtelEnv(policyEnv, constants);
    expect(result.env).toEqual(policyEnv);
  });

  it('policyEnv가 null이면 사이트면 기본값으로 떨어진다', () => {
    const result = buildDesiredOtelEnv(null, constants);
    expect(result.env.OTEL_EXPORTER_OTLP_METRICS_ENDPOINT).toContain(constants.allowedAuthorities.otel[0]);
  });

  it('policyEnv가 빈 객체({})면 사이트면 기본값으로 떨어진다', () => {
    const result = buildDesiredOtelEnv({}, constants);
    expect(Object.keys(result.env).length).toBeGreaterThan(0);
  });
});

describe('buildOtelResourceAttributesValue — employee.id만 담는다(사람 승인 확정, employee.name 금지)', () => {
  it('employee.id=<osUsername> 형태를 만든다', () => {
    expect(buildOtelResourceAttributesValue('sample-user')).toBe('employee.id=sample-user');
  });

  it('사용자명에 employee.name이 들어갈 여지가 없다(함수 시그니처가 단일 인자)', () => {
    expect(buildOtelResourceAttributesValue.length).toBe(1);
  });

  it('URL 인코딩이 필요한 문자를 인코딩한다', () => {
    expect(buildOtelResourceAttributesValue('a b')).toBe('employee.id=a%20b');
  });
});
