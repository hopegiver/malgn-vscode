import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { ConsentGateError } from '../consent/gate.js';
import { MV_CONSENT_EXPIRED, MV_CONSENT_INVALID } from '../consent/errors.js';
import { createMaskedLogger, type LineSink } from '../../ui/log.js';
import { JournalStore } from './store.js';
import { recordConsentGateFailure } from './consentFailureRecorder.js';

let baseDir: string;

beforeEach(async () => {
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-consent-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
});

function fakeLogger(): { sink: LineSink; lines: string[] } {
  const lines: string[] = [];
  return { sink: { appendLine: (v: string) => lines.push(v) }, lines };
}

describe('recordConsentGateFailure — architecture.md §1.2 잔여 요구 이행 (W3 leftover)', () => {
  it('MV_CONSENT_EXPIRED(info)는 로그만 남기고 저널에는 남기지 않는다(정상 상황)', async () => {
    const { sink, lines } = fakeLogger();
    const logger = createMaskedLogger(sink);
    const journal = new JournalStore({ baseDir });
    const error = new ConsentGateError(MV_CONSENT_EXPIRED, 'info', '동의 만료: expiresAt=t now=t2');

    await recordConsentGateFailure(error, { logger, journal });

    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain('[info]');
    expect(await journal.readAll()).toEqual([]);
  });

  it('MV_CONSENT_INVALID(high)는 로그 + 저널 둘 다에 남긴다 — "조용히 재요청하지 않는다"', async () => {
    const { sink, lines } = fakeLogger();
    const logger = createMaskedLogger(sink);
    const journal = new JournalStore({ baseDir });
    const error = new ConsentGateError(MV_CONSENT_INVALID, 'high', 'diffHash 불일치 — plan이 동의 이후 변경됨');

    await recordConsentGateFailure(error, { logger, journal }, 'agent');

    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain('[error]');

    const entries = await journal.readAll();
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      kind: 'consentFailure',
      code: MV_CONSENT_INVALID,
      severity: 'high',
      providerId: 'agent',
    });
  });

  it('providerId를 생략하면 null로 기록된다(ConsentGateError 자체에는 그 정보가 없다)', async () => {
    const { sink } = fakeLogger();
    const logger = createMaskedLogger(sink);
    const journal = new JournalStore({ baseDir });
    const error = new ConsentGateError(MV_CONSENT_INVALID, 'high', 'nonce 재사용 시도');

    await recordConsentGateFailure(error, { logger, journal });

    const [entry] = await journal.readAll();
    expect(entry).toMatchObject({ providerId: null });
  });

  it('[방어심층] 에러 메시지에 자격증명 형태 값이 섞여도 로그·저널 어느 쪽에도 원문이 남지 않는다', async () => {
    const { sink, lines } = fakeLogger();
    const logger = createMaskedLogger(sink);
    const journal = new JournalStore({ baseDir });
    const leaked = `ghp_${'Q'.repeat(36)}`;
    const error = new ConsentGateError(MV_CONSENT_INVALID, 'high', `사고 재현 값 포함: ${leaked}`);

    await recordConsentGateFailure(error, { logger, journal });

    expect(lines[0]).not.toContain(leaked);
    const raw = JSON.stringify(await journal.readAll());
    expect(raw).not.toContain(leaked);
  });
});
