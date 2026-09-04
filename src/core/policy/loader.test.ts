import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from './codeConstants.js';
import {
  MV_AGENT_TARGET_DENIED,
  MV_COMPAT_WIDENING_REJECTED,
  MV_GITHUB_SCOPE_DENIED,
  MV_POLICY_AUTHORITY_DENIED,
  MV_POLICY_MALFORMED,
  MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED,
  MV_POLICY_OTEL_ENV_KEY_DENIED,
  MV_POLICY_OTEL_HEADERS_KIND_INVALID,
  MV_POLICY_SCHEMA_UNSUPPORTED,
  MV_POLICY_SECRET_FIELD_DETECTED,
  MV_POLICY_SIZE_EXCEEDED,
} from './errors.js';
import { POLICY_MAX_BYTES, loadPolicyFromText } from './loader.js';
import type { PolicyLoadResult } from './types.js';

const constants = loadCodeConstants();
const CURRENT_VERSION = '0.1.0';

function load(policy: unknown): PolicyLoadResult {
  return loadPolicyFromText(JSON.stringify(policy), constants, { currentExtensionVersion: CURRENT_VERSION });
}

const VALID_POLICY = {
  schemaVersion: 1,
  generatedAt: '2026-09-02T00:00:00Z',
  extension: { latestVersion: '9.9.9', downloadHint: 'https://download.example.com/malgn-vscode.vsix' },
  killSwitch: {
    minExtensionVersion: null,
    maxExtensionVersion: null,
    disableProviders: [],
    message: null,
    upgradeHint: null,
  },
  rollout: [{ provider: 'cloudflare', percent: 20 }],
  compat: { malgnAgent: '>=1.8.30 <2.0.0', claudeCode: '>=2.1.240' },
  agent: { marketplace: 'malgnsoft/claude-plugins', plugin: 'malgn-agent@malgnsoft-plugins', scope: 'user', channel: 'stable' },
  otel: {
    env: {
      CLAUDE_CODE_ENABLE_TELEMETRY: '1',
      OTEL_METRICS_EXPORTER: 'otlp',
      OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://203.0.113.10:4318/v1/metrics',
      OTEL_LOG_USER_PROMPTS: '0',
      OTEL_LOG_TOOL_CONTENT: '0',
      OTEL_LOG_TOOL_DETAILS: '0',
      OTEL_LOG_RAW_API_BODIES: '0',
    },
    headersHelper: { kind: 'keychain-basic', service: 'example-service', account: 'example-service' },
  },
  install: { mode: 'assisted' },
  github: { requiredScopes: ['repo', 'read:org', 'workflow'] },
  cloudflare: { loginMode: 'wrangler-oauth' },
};

describe('loadPolicyFromText — happy path', () => {
  it('accepts a fully-valid policy and produces an EffectivePolicy', () => {
    const result = load(VALID_POLICY);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.schemaVersion).toBe(1);
    expect(result.policy.agent).toEqual({
      blocked: false,
      marketplace: 'malgnsoft/claude-plugins',
      plugin: 'malgn-agent@malgnsoft-plugins',
      scope: 'user',
      channel: 'stable',
    });
    expect(result.policy.otel.blocked).toBe(false);
    expect(result.policy.github.blocked).toBe(false);
    expect(result.policy.github.requiredScopes).toEqual(['repo', 'read:org', 'workflow']);
    expect(result.issues).toEqual([]);
  });
});

describe('loadPolicyFromText — 파일 전체 거부(사이즈/파싱)', () => {
  it('64KB를 넘는 파일은 크기 초과로 거부된다', () => {
    const huge = { ...VALID_POLICY, killSwitch: { ...VALID_POLICY.killSwitch, message: 'x'.repeat(POLICY_MAX_BYTES) } };
    const result = load(huge);
    expect(result.status).toBe('rejected');
    if (result.status !== 'rejected') return;
    expect(result.code).toBe(MV_POLICY_SIZE_EXCEEDED);
  });

  it('JSON 파싱 불가 텍스트는 거부된다', () => {
    const result = loadPolicyFromText('{not valid json', constants, { currentExtensionVersion: CURRENT_VERSION });
    expect(result.status).toBe('rejected');
    if (result.status !== 'rejected') return;
    expect(result.code).toBe(MV_POLICY_MALFORMED);
  });
});

// --- PR-5(정책 무비밀) ---
describe('loadPolicyFromText — PR-5 정책 무비밀', () => {
  it('token/secret/password/authorization 형태의 키 이름이 있으면 파일 전체가 거부된다', () => {
    const withSecret = { ...VALID_POLICY, otel: { ...VALID_POLICY.otel, headersHelper: { kind: 'keychain-basic', service: 'example-service', account: 'example-service', apiToken: 'xxx' } } };
    const result = load(withSecret);
    expect(result.status).toBe('rejected');
    if (result.status !== 'rejected') return;
    expect(result.code).toBe(MV_POLICY_SECRET_FIELD_DETECTED);
  });
});

// --- 완료판정 #4: 부재가 차단임을 보이는 테스트(fail-open이 아니다) ---
describe('부재는 차단 (PR-11 ①) — 필수 키가 빠진 정책이 통과하지 않는다', () => {
  it('schemaVersion이 아예 없으면 정책 전체가 거부된다 (기본값으로 조용히 통과하지 않는다)', () => {
    const { schemaVersion, ...withoutSchemaVersion } = VALID_POLICY;
    const result = load(withoutSchemaVersion);
    expect(result.status).toBe('rejected');
    if (result.status !== 'rejected') return;
    expect(result.code).toBe(MV_POLICY_SCHEMA_UNSUPPORTED);
  });

  it('agent 필드가 아예 없으면 agent 전체가 blocked다 (허용 목록이 우연히 원소 1개라고 기본값을 고르지 않는다)', () => {
    const { agent, ...withoutAgent } = VALID_POLICY;
    const result = load(withoutAgent);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.agent).toEqual({ blocked: true, reason: expect.any(String) });
    expect(result.issues.some((i) => i.code === MV_AGENT_TARGET_DENIED)).toBe(true);
  });

  it('otel.headersHelper.kind가 없으면 정책 파일 전체가 거부된다(otel만 부분 무효화되지 않는다)', () => {
    const withoutKind = {
      ...VALID_POLICY,
      otel: { ...VALID_POLICY.otel, headersHelper: { service: 'example-service', account: 'example-service' } },
    };
    const result = load(withoutKind);
    expect(result.status).toBe('rejected');
    if (result.status !== 'rejected') return;
    expect(result.code).toBe(MV_POLICY_OTEL_HEADERS_KIND_INVALID);
  });
});

// --- 완료판정 #3: 좁히기만 허용됨을 보이는 테스트 ---
describe('좁히기만 허용된다 (PR-9) — 넓히려는 시도는 폐기되고 지정된 에러 코드가 나온다', () => {
  it('버전 하한을 낮추려는 시도는 폐기되고 MV_COMPAT_WIDENING_REJECTED가 난다', () => {
    const widened = { ...VALID_POLICY, compat: { ...VALID_POLICY.compat, claudeCode: '>=1.0.0' } };
    const result = load(widened);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    // 번들 하한(>=2.1.237)이 그대로 유지된다 — 정책이 제안한 >=1.0.0이 반영되지 않는다
    expect(result.policy.compat.claudeCode).toBe(constants.requires.claudeCode);
    expect(result.issues.some((i) => i.code === MV_COMPAT_WIDENING_REJECTED && i.field === 'compat.claudeCode')).toBe(true);
  });

  it('허용 목적지를 추가(=화이트리스트 밖 목적지 사용)하려는 시도는 폐기되고 MV_POLICY_AUTHORITY_DENIED가 난다', () => {
    const untrustedHost = { ...VALID_POLICY, extension: { ...VALID_POLICY.extension, downloadHint: 'https://evil.invalid/malgn-vscode.vsix' } };
    const result = load(untrustedHost);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.extension.downloadHint).toBeNull(); // 필드 폐기 — 허용되지 않은 목적지가 채택되지 않는다
    expect(result.issues.some((i) => i.code === MV_POLICY_AUTHORITY_DENIED && i.field === 'extension.downloadHint')).toBe(true);
  });

  it('권능을 넓히려는 시도(allowedGithubScopes 밖의 scope 요구)는 그 원소만 폐기되고 MV_GITHUB_SCOPE_DENIED가 난다', () => {
    const overreaching = { ...VALID_POLICY, github: { requiredScopes: ['repo', 'admin:org', 'delete_repo'] } };
    const result = load(overreaching);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.github.requiredScopes).toEqual(['repo']); // admin:org, delete_repo 폐기
    expect(result.issues.filter((i) => i.code === MV_GITHUB_SCOPE_DENIED)).toHaveLength(2);
  });

  it('agent.marketplace를 허용 목록 밖 값으로 넓히려는 시도는 agent 전체를 blocked로 만들고 MV_AGENT_TARGET_DENIED가 난다', () => {
    const untrustedMarketplace = { ...VALID_POLICY, agent: { ...VALID_POLICY.agent, marketplace: 'attacker/plugins' } };
    const result = load(untrustedMarketplace);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.agent).toEqual({ blocked: true, reason: expect.any(String) });
    expect(result.issues.some((i) => i.code === MV_AGENT_TARGET_DENIED)).toBe(true);
  });
});

describe('otel.env — S4 프라이버시 4키/S2 목적지/PII 확장 우회 차단', () => {
  it('프라이버시 키가 "0"이 아니면 그 키만 폐기된다', () => {
    const leaky = { ...VALID_POLICY, otel: { ...VALID_POLICY.otel, env: { ...VALID_POLICY.otel.env, OTEL_LOG_USER_PROMPTS: '1' } } };
    const result = load(leaky);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.env.OTEL_LOG_USER_PROMPTS).toBeUndefined();
  });

  it('OTEL_RESOURCE_ATTRIBUTES가 있으면 OTel 전체가 blocked다', () => {
    const withPii = { ...VALID_POLICY, otel: { ...VALID_POLICY.otel, env: { ...VALID_POLICY.otel.env, OTEL_RESOURCE_ATTRIBUTES: 'employee.id=1' } } };
    const result = load(withPii);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.blocked).toBe(true);
    expect(result.policy.otel.env.OTEL_RESOURCE_ATTRIBUTES).toBeUndefined();
  });

  it('허용되지 않은 수집기 목적지는 OTel 전체를 blocked로 만든다', () => {
    const wrongCollector = {
      ...VALID_POLICY,
      otel: { ...VALID_POLICY.otel, env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://evil.invalid/v1/metrics' } },
    };
    const result = load(wrongCollector);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.blocked).toBe(true);
  });
});

// --- A-31(F-4, security-report.md) — otel.env 검사 순서 회귀 3축 ---
// loader.ts:341-355의 엔드포인트 분기가 KNOWN_OTEL_ENV_KEYS 화이트리스트보다 먼저 돌면
// TRACES_ENDPOINT·임의 접미 키가 authority만 통과해도 effective env에 실린다(실측
// 재현됨). 순서를 뒤집은 뒤에도 (1) 그 두 키는 여전히 폐기되는지 (2) 화이트리스트
// 안의 METRICS/LOGS_ENDPOINT는 여전히 authority 검사를 받는지 (3) 허용 밖 authority가
// 여전히 high로 거부되는지 3축 전부를 고정한다.
describe('otel.env — A-31(F-4) 검사 순서 회귀: 화이트리스트가 엔드포인트 분기보다 먼저 돈다', () => {
  it('[축1] TRACES_ENDPOINT(화이트리스트 밖, 대조군은 KNOWN_OTEL_ENV_KEYS에 METRICS/LOGS만 있음)는 authority가 허용값이어도 폐기되고 blocked를 만들지 않는다', () => {
    const withTraces = {
      ...VALID_POLICY,
      otel: {
        ...VALID_POLICY.otel,
        env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: 'https://203.0.113.10:4318/v1/traces' },
      },
    };
    const result = load(withTraces);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT).toBeUndefined();
    expect(result.policy.otel.blocked).toBe(false);
    expect(
      result.issues.some((i) => i.field === 'otel.env.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT' && i.code === MV_POLICY_OTEL_ENV_KEY_DENIED)
    ).toBe(true);
  });

  it('[축1] 임의 접미 OTEL_EXPORTER_OTLP_*_ENDPOINT 키도 authority가 허용값이어도 폐기된다(TRACES와 동일 취급)', () => {
    const withArbitrary = {
      ...VALID_POLICY,
      otel: {
        ...VALID_POLICY.otel,
        env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_ANYTHINGGOES_ENDPOINT: 'https://203.0.113.10:4318/v1/x' },
      },
    };
    const result = load(withArbitrary);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.env.OTEL_EXPORTER_OTLP_ANYTHINGGOES_ENDPOINT).toBeUndefined();
    expect(result.policy.otel.blocked).toBe(false);
  });

  it('[축2] 화이트리스트 안의 METRICS_ENDPOINT/LOGS_ENDPOINT는 여전히 authority 검사를 받아 허용 목적지면 채택된다', () => {
    const withLogs = {
      ...VALID_POLICY,
      otel: {
        ...VALID_POLICY.otel,
        env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: 'https://203.0.113.10:4318/v1/logs' },
      },
    };
    const result = load(withLogs);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.env.OTEL_EXPORTER_OTLP_METRICS_ENDPOINT).toBe('https://203.0.113.10:4318/v1/metrics');
    expect(result.policy.otel.env.OTEL_EXPORTER_OTLP_LOGS_ENDPOINT).toBe('https://203.0.113.10:4318/v1/logs');
    expect(result.policy.otel.blocked).toBe(false);
  });

  it('[축3] 화이트리스트 안 키(LOGS_ENDPOINT)라도 허용 밖 authority면 여전히 high로 거부되고 OTel 전체가 blocked된다', () => {
    const wrongLogs = {
      ...VALID_POLICY,
      otel: {
        ...VALID_POLICY.otel,
        env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: 'https://evil.invalid/v1/logs' },
      },
    };
    const result = load(wrongLogs);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.blocked).toBe(true);
    expect(
      result.issues.some((i) => i.field === 'otel.env.OTEL_EXPORTER_OTLP_LOGS_ENDPOINT' && i.code === MV_POLICY_OTEL_ENDPOINT_AUTHORITY_DENIED && i.severity === 'high')
    ).toBe(true);
  });

  it('[대조군] OTEL_EXPORTER_OTLP_HEADERS(엔드포인트 형태 아님)는 원래도 지금도 폐기된다', () => {
    const withHeaders = {
      ...VALID_POLICY,
      otel: { ...VALID_POLICY.otel, env: { ...VALID_POLICY.otel.env, OTEL_EXPORTER_OTLP_HEADERS: 'x-api-key=abc' } },
    };
    const result = load(withHeaders);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.otel.env.OTEL_EXPORTER_OTLP_HEADERS).toBeUndefined();
    expect(result.policy.otel.blocked).toBe(false);
  });
});

describe('killSwitch / rollout — enum·범위 검증', () => {
  it('disableProviders의 미지 원소는 폐기되고 나머지는 유지된다', () => {
    const p = { ...VALID_POLICY, killSwitch: { ...VALID_POLICY.killSwitch, disableProviders: ['otel', 'install', 'not-a-provider'] } };
    const result = load(p);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    // 'install'은 enum 밖(§ 표 원문: agent|otel|github|cloudflare|mcp만)이라 폐기된다
    expect(result.policy.killSwitch.disableProviders).toEqual(['otel']);
  });

  it('rollout percent가 0~100 밖이면 그 항목이 폐기된다', () => {
    const p = { ...VALID_POLICY, rollout: [{ provider: 'cloudflare', percent: 150 }] };
    const result = load(p);
    expect(result.status).toBe('ok');
    if (result.status !== 'ok') return;
    expect(result.policy.rollout).toEqual([]);
  });
});
