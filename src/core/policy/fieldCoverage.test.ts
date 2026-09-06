// 완료판정 #2 — "policy-contract.md 전수 검증표에 있는 모든 리프 필드를 검증한다.
// 표에 있는데 코드에 없는 필드가 0건임을 보인다."
//
// 대조 방법: docs/policy-contract.md "필드별 전수 검증표"(§표, 17개 표 행)를 리프
// 경로 단위로 손으로 분해해 DOC_TABLE_FIELDS에 옮겼다(예: "killSwitch.disableProviders[],
// rollout[].provider" 한 행은 두 개의 리프 경로로 분해된다). 그리고 각 리프 경로마다
// 그 필드를 **실제로 위반하는 최소 정책**을 만들어 loadPolicyFromText에 넣고, 검증기가
// 정말로 반응하는지(issue가 나거나 파일 전체가 거부되는지) 런타임으로 확인한다 —
// "필드 이름이 소스에 문자열로 등장하는지" 같은 텍스트 grep이 아니라 **동작 증거**로
// 대조한다. 이 파일의 테스트 개수가 DOC_TABLE_FIELDS 리프 개수와 정확히 일치하는지도
// 별도로 assert해 "표에는 있는데 테스트가 없는 행"이 생기지 않게 한다.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from './codeConstants.js';
import { loadPolicyFromText } from './loader.js';
import type { PolicyLoadResult } from './types.js';

const constants = loadCodeConstants();
const CURRENT_VERSION = '0.1.0';

function load(policy: unknown): PolicyLoadResult {
  return loadPolicyFromText(JSON.stringify(policy), constants, { currentExtensionVersion: CURRENT_VERSION });
}

const BASE = {
  schemaVersion: 1,
  generatedAt: '2026-09-02T00:00:00Z',
  extension: { latestVersion: '9.9.9', downloadHint: 'https://download.example.com/x.vsix' },
  killSwitch: { minAppVersion: null, maxAppVersion: null, disableProviders: [], message: null, upgradeHint: null },
  rollout: [{ provider: 'cloudflare', percent: 20 }],
  compat: { malgnAgent: '>=1.8.30 <2.0.0', claudeCode: '>=2.1.240' },
  agent: { marketplace: 'malgnsoft/claude-plugins', plugin: 'malgn-agent@malgnsoft-plugins', scope: 'user', channel: 'stable' },
  otel: {
    env: {
      CLAUDE_CODE_ENABLE_TELEMETRY: '1',
      OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://203.0.113.10:4318/v1/metrics',
      OTEL_LOG_USER_PROMPTS: '0',
      OTEL_LOG_TOOL_CONTENT: '0',
      OTEL_LOG_TOOL_DETAILS: '0',
      OTEL_LOG_RAW_API_BODIES: '0',
    },
    headersHelper: { kind: 'keychain-basic', service: 'example-service', account: 'example-service' },
  },
  install: { mode: 'assisted' },
  github: { requiredScopes: ['repo'] },
  cloudflare: { loginMode: 'wrangler-oauth' },
};

/**
 * docs/policy-contract.md "필드별 전수 검증표"를 리프 경로 단위로 분해한 정본 목록.
 * 이 배열의 길이(= 아래 `it.each` 케이스 수)가 표의 리프 개수와 같아야 "표에는
 * 있는데 테스트가 없는 행"이 없다고 주장할 수 있다.
 */
// `export`: 검사 ③C(src/compat-check/checks/check3c-fieldTableCrossSync.ts)가 이 배열을
// check3-fixtureLeafCoverage.ts의 독립 사본과 코드 대 코드로 대조한다
// (docs/policy-contract.md §8.3 "③C 신설").
export const DOC_TABLE_LEAF_FIELDS = [
  'schemaVersion',
  'generatedAt',
  'extension.latestVersion',
  'extension.downloadHint',
  'killSwitch.minAppVersion',
  'killSwitch.maxAppVersion',
  'killSwitch.disableProviders[]',
  'rollout[].provider',
  'rollout[].percent',
  'killSwitch.message',
  'killSwitch.upgradeHint',
  'compat.malgnAgent',
  'compat.claudeCode',
  'agent.marketplace',
  'agent.plugin',
  'agent.scope',
  'agent.channel',
  'otel.env(키 이름 화이트리스트)',
  'otel.env(OTEL_EXPORTER_OTLP_*_ENDPOINT)',
  'otel.env(프라이버시 4키)',
  'otel.env(OTEL_RESOURCE_ATTRIBUTES 금지)',
  'otel.headersHelper.kind',
  'otel.headersHelper.service/.account',
  'github.requiredScopes[]',
  'cloudflare.loginMode',
  'install.mode',
  '문서 전체(PR-5 시크릿 키 이름 금지)',
] as const;

describe('전수 검증표 리프 필드 개수 자기 점검', () => {
  it('DOC_TABLE_LEAF_FIELDS는 27개(표 17행을 리프 단위로 분해)이며, 아래 it.each가 그 개수만큼 있다', () => {
    expect(DOC_TABLE_LEAF_FIELDS).toHaveLength(27);
  });
});

interface FieldCase {
  readonly field: (typeof DOC_TABLE_LEAF_FIELDS)[number];
  readonly run: () => void;
}

const cases: FieldCase[] = [
  {
    field: 'schemaVersion',
    run: () => {
      const { schemaVersion, ...rest } = BASE;
      const result = load(rest);
      expect(result.status).toBe('rejected');
    },
  },
  {
    field: 'generatedAt',
    run: () => {
      const result = load({ ...BASE, generatedAt: 'not-a-date' });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.generatedAt).toBeNull();
    },
  },
  {
    field: 'extension.latestVersion',
    run: () => {
      const result = load({ ...BASE, extension: { ...BASE.extension, latestVersion: '0.0.1' } }); // 다운그레이드 시도
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.extension.latestVersion).toBeNull();
    },
  },
  {
    field: 'extension.downloadHint',
    run: () => {
      const result = load({ ...BASE, extension: { ...BASE.extension, downloadHint: 'https://evil.invalid/x.vsix' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.extension.downloadHint).toBeNull();
    },
  },
  {
    field: 'killSwitch.minAppVersion',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, minAppVersion: 'not-semver' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.killSwitch.minAppVersion).toBeNull();
    },
  },
  {
    field: 'killSwitch.maxAppVersion',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, maxAppVersion: 'not-semver' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.killSwitch.maxAppVersion).toBeNull();
    },
  },
  {
    field: 'killSwitch.disableProviders[]',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, disableProviders: ['not-a-provider'] } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.killSwitch.disableProviders).toEqual([]);
    },
  },
  {
    field: 'rollout[].provider',
    run: () => {
      const result = load({ ...BASE, rollout: [{ provider: 'not-a-provider', percent: 10 }] });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.rollout).toEqual([]);
    },
  },
  {
    field: 'rollout[].percent',
    run: () => {
      const result = load({ ...BASE, rollout: [{ provider: 'cloudflare', percent: 999 }] });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.rollout).toEqual([]);
    },
  },
  {
    field: 'killSwitch.message',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, message: 'x'.repeat(600) } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.killSwitch.message).toHaveLength(512);
    },
  },
  {
    field: 'killSwitch.upgradeHint',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, upgradeHint: 'x'.repeat(600) } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.killSwitch.upgradeHint).toHaveLength(512);
    },
  },
  {
    field: 'compat.malgnAgent',
    run: () => {
      const result = load({ ...BASE, compat: { ...BASE.compat, malgnAgent: '>=1.0.0' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.compat.malgnAgent).toBe(constants.requires.malgnAgent);
    },
  },
  {
    field: 'compat.claudeCode',
    run: () => {
      const result = load({ ...BASE, compat: { ...BASE.compat, claudeCode: '>=1.0.0' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.compat.claudeCode).toBe(constants.requires.claudeCode);
    },
  },
  {
    field: 'agent.marketplace',
    run: () => {
      const result = load({ ...BASE, agent: { ...BASE.agent, marketplace: 'attacker/plugins' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.agent.blocked).toBe(true);
    },
  },
  {
    field: 'agent.plugin',
    run: () => {
      const result = load({ ...BASE, agent: { ...BASE.agent, plugin: 'evil-plugin@attacker-plugins' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.agent.blocked).toBe(true);
    },
  },
  {
    field: 'agent.scope',
    run: () => {
      const result = load({ ...BASE, agent: { ...BASE.agent, scope: 'system' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.agent.blocked).toBe(true);
    },
  },
  {
    field: 'agent.channel',
    run: () => {
      const result = load({ ...BASE, agent: { ...BASE.agent, channel: 'nightly' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok' && !result.policy.agent.blocked) expect(result.policy.agent.channel).toBe('stable');
    },
  },
  {
    field: 'otel.env(키 이름 화이트리스트)',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, env: { ...BASE.otel.env, OTEL_NOT_A_KNOWN_KEY: 'x' } } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.otel.env.OTEL_NOT_A_KNOWN_KEY).toBeUndefined();
    },
  },
  {
    field: 'otel.env(OTEL_EXPORTER_OTLP_*_ENDPOINT)',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, env: { ...BASE.otel.env, OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://evil.invalid/v1/metrics' } } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.otel.blocked).toBe(true);
    },
  },
  {
    field: 'otel.env(프라이버시 4키)',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, env: { ...BASE.otel.env, OTEL_LOG_USER_PROMPTS: '1' } } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.otel.env.OTEL_LOG_USER_PROMPTS).toBeUndefined();
    },
  },
  {
    field: 'otel.env(OTEL_RESOURCE_ATTRIBUTES 금지)',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, env: { ...BASE.otel.env, OTEL_RESOURCE_ATTRIBUTES: 'employee.id=1' } } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.otel.blocked).toBe(true);
    },
  },
  {
    field: 'otel.headersHelper.kind',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, headersHelper: { ...BASE.otel.headersHelper, kind: 'shell-exec' } } });
      expect(result.status).toBe('rejected'); // 파일 전체 거부
    },
  },
  {
    field: 'otel.headersHelper.service/.account',
    run: () => {
      const result = load({ ...BASE, otel: { ...BASE.otel, headersHelper: { kind: 'keychain-basic', service: 'not-allowed-item', account: 'example-service' } } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.otel.blocked).toBe(true);
    },
  },
  {
    field: 'github.requiredScopes[]',
    run: () => {
      const result = load({ ...BASE, github: { requiredScopes: ['admin:org'] } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') {
        expect(result.policy.github.requiredScopes).toEqual([]);
        expect(result.policy.github.blocked).toBe(true);
      }
    },
  },
  {
    field: 'cloudflare.loginMode',
    run: () => {
      const result = load({ ...BASE, cloudflare: { loginMode: 'sso-magic' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.cloudflare.loginMode).toBe('wrangler-oauth');
    },
  },
  {
    field: 'install.mode',
    run: () => {
      const result = load({ ...BASE, install: { mode: 'auto-yolo' } });
      expect(result.status).toBe('ok');
      if (result.status === 'ok') expect(result.policy.install.mode).toBe('assisted');
    },
  },
  {
    field: '문서 전체(PR-5 시크릿 키 이름 금지)',
    run: () => {
      const result = load({ ...BASE, killSwitch: { ...BASE.killSwitch, secretToken: 'xxx' } });
      expect(result.status).toBe('rejected');
    },
  },
];

describe('전수 검증표 리프 필드별 동작 증거 (표 ↔ 코드 대조)', () => {
  it('cases 배열이 DOC_TABLE_LEAF_FIELDS와 원소 단위로 정확히 일치한다(누락·중복 0건)', () => {
    const caseFields = cases.map((c) => c.field).sort();
    const docFields = [...DOC_TABLE_LEAF_FIELDS].sort();
    expect(caseFields).toEqual(docFields);
  });

  it.each(cases)('$field — 검증기가 실제로 반응한다', ({ run }) => {
    run();
  });
});

// --- AT-U6 (architecture.md §3.6.1) — "정책의 어떤 리프 필드도 싱크 `update`를
// 배정받지 않는다(PR-11① 전수 검증표에 `update` 싱크가 존재하지 않는다) |
// fieldCoverage.test.ts 확장" — 원문 지시대로 이 파일을 확장한다.
describe('AT-U6 — 정책 리프 필드는 어떤 형태로도 update 싱크로 라우팅되지 않는다', () => {
  const HERE = dirname(fileURLToPath(import.meta.url));
  const TYPES_SOURCE = readFileSync(join(HERE, 'types.ts'), 'utf8');
  const LOADER_SOURCE = readFileSync(join(HERE, 'loader.ts'), 'utf8');

  it('EffectivePolicy 인터페이스에 update라는 이름의 리프 필드가 없다', () => {
    const start = TYPES_SOURCE.indexOf('export interface EffectivePolicy');
    expect(start, 'types.ts에서 EffectivePolicy 정의를 찾을 수 없습니다').toBeGreaterThanOrEqual(0);
    const end = TYPES_SOURCE.indexOf('\n}', start);
    const body = TYPES_SOURCE.slice(start, end);
    // §0 싱크 분류(S1~S6) 중 어느 것도 이름이 "update"가 아니다 — 이 인터페이스가
    // 그 6종 밖의 새 싱크(예: `update`)를 리프 필드로 들이지 않는지 직접 확인한다.
    expect(body).not.toMatch(/readonly\s+update\s*[:?]/);
  });

  it('DOC_TABLE_LEAF_FIELDS(전수 검증표) 27개 항목 중 어느 것도 "update"가 아니다', () => {
    for (const field of DOC_TABLE_LEAF_FIELDS) {
      expect(field.toLowerCase()).not.toBe('update');
    }
  });

  it('loader.ts는 update 모듈을 import하지 않는다(AT-U1과 같은 경계를 정책 쪽에서도 재확인)', () => {
    expect(LOADER_SOURCE).not.toMatch(/from\s+['"][^'"]*\/update\//);
  });

  it('[위반 주입 확인] EffectivePolicy에 update 리프가 있다고 가정한 가짜 소스는 이 검사가 실제로 잡는다', () => {
    const fakeTypesSource = `export interface EffectivePolicy {\n  readonly update: { readonly channel: string };\n}\n`;
    const start = fakeTypesSource.indexOf('export interface EffectivePolicy');
    const end = fakeTypesSource.indexOf('\n}', start);
    const body = fakeTypesSource.slice(start, end);
    expect(body).toMatch(/readonly\s+update\s*[:?]/);
  });
});
