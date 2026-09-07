import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { detectOtel, type OtelDetectDeps } from './detect.js';
import { MV_OTEL_DRIFT, MV_OTEL_HELPER_MISSING, MV_OTEL_HELPER_UNSUPPORTED_OS, MV_OTEL_OK, MV_OTEL_SETTINGS_UNREADABLE } from './errors.js';
import { buildDefaultOtelEnv, buildOtelResourceAttributesValue, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';

const constants = loadCodeConstants();
const CLAUDE_HOME = '/home/runner/.claude';
const HELPER_PATH = '/home/runner/.claude/otel-headers.sh';

function baseDeps(overrides: Partial<OtelDetectDeps> = {}): OtelDetectDeps {
  return {
    claudeHomeDir: CLAUDE_HOME,
    readTextFile: async () => {
      const err = new Error('ENOENT') as NodeJS.ErrnoException;
      err.code = 'ENOENT';
      throw err;
    },
    pathExecutable: async () => true,
    platform: 'darwin',
    ...overrides,
  };
}

function settingsJson(env: Record<string, string>, otelHeadersHelper: string | null = HELPER_PATH): string {
  return JSON.stringify({ env, otelHeadersHelper });
}

describe('detectOtel', () => {
  it('darwin이 아니면 blocked + MV_OTEL_HELPER_UNSUPPORTED_OS', async () => {
    const deps = baseDeps({ platform: 'win32' });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('blocked');
    expect(observed.code).toBe(MV_OTEL_HELPER_UNSUPPORTED_OS);
  });

  it('settings.json 파싱 실패면 unknown + MV_OTEL_SETTINGS_UNREADABLE', async () => {
    const deps = baseDeps({ readTextFile: async () => '{not json' });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('unknown');
    expect(observed.code).toBe(MV_OTEL_SETTINGS_UNREADABLE);
  });

  it('otelHeadersHelper가 설정돼 있지 않고 조직이 keychain 항목을 요구하면 blocked + MV_OTEL_HELPER_MISSING', async () => {
    const deps = baseDeps({ readTextFile: async () => settingsJson({}, null) });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('blocked');
    expect(observed.code).toBe(MV_OTEL_HELPER_MISSING);
  });

  it('otelHeadersHelper는 있으나 실행 권한이 없으면 blocked + MV_OTEL_HELPER_MISSING', async () => {
    const deps = baseDeps({ readTextFile: async () => settingsJson({}), pathExecutable: async () => false });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('blocked');
    expect(observed.code).toBe(MV_OTEL_HELPER_MISSING);
  });

  it('desired env + resource attributes가 전부 일치하면 ok', async () => {
    const desired = buildDefaultOtelEnv(constants);
    const env = { ...desired.env, [OTEL_RESOURCE_ATTRIBUTES_KEY]: buildOtelResourceAttributesValue('tester') };
    const deps = baseDeps({ readTextFile: async () => settingsJson(env) });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('ok');
    expect(observed.code).toBe(MV_OTEL_OK);
  });

  it('desired 키가 하나라도 없으면 drift', async () => {
    const deps = baseDeps({ readTextFile: async () => settingsJson({}) });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('drift');
    expect(observed.code).toBe(MV_OTEL_DRIFT);
    const detail = observed.detail as { missingKeys: string[] };
    expect(detail.missingKeys.length).toBeGreaterThan(0);
  });

  it('OTEL_RESOURCE_ATTRIBUTES가 없으면 drift(비어 있어도 desired env 자체는 일치할 수 있다)', async () => {
    const desired = buildDefaultOtelEnv(constants);
    const deps = baseDeps({ readTextFile: async () => settingsJson(desired.env) });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('drift');
    const detail = observed.detail as { resourceAttributesConfigured: boolean };
    expect(detail.resourceAttributesConfigured).toBe(false);
  });

  it('Observed.detail은 env 값을 마스킹하지 않지만 missingKeys/mismatchedKeys는 키 이름만 담는다', async () => {
    const deps = baseDeps({ readTextFile: async () => settingsJson({ OTEL_METRICS_EXPORTER: 'wrong' }) });
    const observed = await detectOtel(deps, {} as never);
    const detail = observed.detail as { mismatchedKeys: string[] };
    expect(detail.mismatchedKeys).toContain('OTEL_METRICS_EXPORTER');
    expect(typeof detail.mismatchedKeys[0]).toBe('string');
  });

  it('던지지 않는다(PR-8) — readTextFile이 임의 예외를 던져도 unknown으로 접힌다', async () => {
    const deps = baseDeps({ readTextFile: async () => { throw new Error('boom'); } });
    const observed = await detectOtel(deps, {} as never);
    expect(observed.status).toBe('unknown');
  });
});
