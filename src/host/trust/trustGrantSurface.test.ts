import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TrustLedger } from '../../core/trust/ledger.js';
import { requestTargetFolderTrust } from './trustGrantSurface.js';

let baseDir: string;
let targetFolder: string;

beforeEach(async () => {
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-trust-surface-appdata-'));
  targetFolder = await mkdtemp(join(tmpdir(), 'malgn-trust-surface-target-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
  await rm(targetFolder, { recursive: true, force: true });
});

describe('requestTargetFolderTrust — trust.grant 표면', () => {
  it('사용자가 승인하면 원장에 trusted로 기록되고 outcome도 trusted를 반환한다', async () => {
    const ledger = new TrustLedger({ baseDir });
    const showConsentDialog = vi.fn().mockResolvedValue(true);
    const outcome = await requestTargetFolderTrust(targetFolder, { ledger, showConsentDialog });
    expect(outcome).toEqual({ state: 'trusted', userApproved: true });
    expect(await ledger.get(targetFolder)).toBe('trusted');
  });

  it('절대경로 전문을 다이얼로그에 그대로 넘긴다(§3.6.1 ③ "절대경로 전문 표시")', async () => {
    const ledger = new TrustLedger({ baseDir });
    const showConsentDialog = vi.fn().mockResolvedValue(true);
    await requestTargetFolderTrust(targetFolder, { ledger, showConsentDialog });
    expect(showConsentDialog).toHaveBeenCalledWith(targetFolder);
  });

  it('사용자가 거부하면 원장을 건드리지 않는다(기본값 untrusted 유지)', async () => {
    const ledger = new TrustLedger({ baseDir });
    const showConsentDialog = vi.fn().mockResolvedValue(false);
    const outcome = await requestTargetFolderTrust(targetFolder, { ledger, showConsentDialog });
    expect(outcome).toEqual({ state: 'untrusted', userApproved: false });
    expect(await ledger.get(targetFolder)).toBe('untrusted');
  });
});

describe('trust.grant는 정지 판정을 거치지 않는다 — 소스 검사', () => {
  it('trustGrantSurface.ts는 stopGate.ts를 import하지 않고 evaluateStop/STOPPABLE_SURFACES를 참조하지 않는다', async () => {
    const { readFileSync } = await import('node:fs');
    const { dirname } = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const here = dirname(fileURLToPath(import.meta.url));
    const source = readFileSync(join(here, 'trustGrantSurface.ts'), 'utf8');
    // import 지정자 자체가 없는지를 본다 — 설명 주석에서 `evaluateStop`·
    // `STOPPABLE_SURFACES`를 산문으로 언급하는 것(왜 안 쓰는지 설명)과 실제로 그
    // 식별자를 import해 호출하는 것은 다르다. 이 검사는 후자(실제 배선)만 잡는다.
    expect(source).not.toMatch(/from\s+['"].*stopGate\.js['"]/);
    expect(source).not.toMatch(/evaluateStop\(/);
  });
});
