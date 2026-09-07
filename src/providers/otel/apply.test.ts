import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { Plan } from '../../providers/types.js';
import { applyOtel, type OtelApplyDeps } from './apply.js';
import { MV_OTEL_APPLY_FAILED, MV_OTEL_OK } from './errors.js';

describe('applyOtel', () => {
  let dir: string;
  let deps: OtelApplyDeps;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'malgn-otel-apply-'));
    deps = {
      claudeHomeDir: dir,
      readTextFile: (p) => readFile(p, 'utf8'),
      backupsDir: join(dir, 'backups'),
      now: () => new Date('2026-09-07T00:00:00.000Z'),
    };
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('changes가 비어 있으면 아무것도 쓰지 않고 ok를 반환한다', async () => {
    const plan: Plan = { providerId: 'otel', changes: [], diffHash: 'x' };
    const result = await applyOtel(deps, plan, {} as never);
    expect(result.status).toBe('ok');
    expect(result.code).toBe(MV_OTEL_OK);
    expect(result.appliedChangeIds).toEqual([]);
  });

  it('env.<KEY> 형태 Change를 settings.json에 실제로 쓴다', async () => {
    const plan: Plan = {
      providerId: 'otel',
      changes: [
        { id: 'c1', target: 'env.CLAUDE_CODE_ENABLE_TELEMETRY', kind: 'add', level: 'L1', after: '1', reversible: true, rationale: 'x' },
      ],
      diffHash: 'x',
    };
    const result = await applyOtel(deps, plan, {} as never);
    expect(result.status).toBe('ok');
    expect(result.appliedChangeIds).toEqual(['c1']);

    const written = JSON.parse(await readFile(join(dir, 'settings.json'), 'utf8'));
    expect(written.env.CLAUDE_CODE_ENABLE_TELEMETRY).toBe('1');
  });

  it('알 수 없는 target 형태면 blocked + MV_OTEL_APPLY_FAILED', async () => {
    const plan: Plan = {
      providerId: 'otel',
      changes: [{ id: 'c1', target: 'not-an-env-target', kind: 'add', level: 'L1', after: '1', reversible: true, rationale: 'x' }],
      diffHash: 'x',
    };
    const result = await applyOtel(deps, plan, {} as never);
    expect(result.status).toBe('blocked');
    expect(result.code).toBe(MV_OTEL_APPLY_FAILED);
  });

  it('기존 파일이 있으면 쓰기 전 백업을 남긴다', async () => {
    await writeFile(join(dir, 'settings.json'), JSON.stringify({ env: { EXISTING: 'x' } }), 'utf8');
    const plan: Plan = {
      providerId: 'otel',
      changes: [{ id: 'c1', target: 'env.NEW', kind: 'add', level: 'L1', after: 'v', reversible: true, rationale: 'x' }],
      diffHash: 'x',
    };
    await applyOtel(deps, plan, {} as never);
    const { readdirSync } = await import('node:fs');
    const backups = readdirSync(join(dir, 'backups'));
    expect(backups.length).toBe(1);
  });
});
