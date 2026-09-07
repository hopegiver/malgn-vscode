import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { buildDefaultOtelEnv, buildOtelResourceAttributesValue, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';
import type { OtelDetectDeps } from './detect.js';
import { MV_OTEL_OK, MV_OTEL_VERIFY_FAILED } from './errors.js';
import { verifyOtel } from './verify.js';

const constants = loadCodeConstants();

function baseDeps(overrides: Partial<OtelDetectDeps> = {}): OtelDetectDeps {
  return {
    claudeHomeDir: '/home/runner/.claude',
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

describe('verifyOtel — detectOtel 재사용(검증 로직 이중 정의 금지)', () => {
  it('desired와 일치하면 ok', async () => {
    const desired = buildDefaultOtelEnv(constants);
    const env = { ...desired.env, [OTEL_RESOURCE_ATTRIBUTES_KEY]: buildOtelResourceAttributesValue('tester') };
    const deps = baseDeps({ readTextFile: async () => JSON.stringify({ env, otelHeadersHelper: '/x/otel-headers.sh' }) });
    const result = await verifyOtel(deps, {} as never);
    expect(result.status).toBe('ok');
    expect(result.code).toBe(MV_OTEL_OK);
  });

  it('desired와 다르면 drift + MV_OTEL_VERIFY_FAILED', async () => {
    const deps = baseDeps({ readTextFile: async () => JSON.stringify({ env: {}, otelHeadersHelper: '/x/otel-headers.sh' }) });
    const result = await verifyOtel(deps, {} as never);
    expect(result.status).toBe('drift');
    expect(result.code).toBe(MV_OTEL_VERIFY_FAILED);
  });

  it('헬퍼가 없으면 blocked를 그대로 전달한다', async () => {
    const deps = baseDeps({ readTextFile: async () => JSON.stringify({ env: {}, otelHeadersHelper: null }) });
    const result = await verifyOtel(deps, {} as never);
    expect(result.status).toBe('blocked');
  });
});
