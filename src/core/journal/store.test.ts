import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { MASK_MARKER } from '../diagnostics/mask.js';
import { JournalStore } from './store.js';

let baseDir: string;

beforeEach(async () => {
  // 임시 디렉터리 — 실제 globalStorage 경로를 흉내내되 하드코딩된 사용자 홈 경로를
  // 소스에 남기지 않는다(compat:check 검사 ⑨ home-directory-path 부류 회피 겸,
  // 테스트 격리 목적).
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-journal-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
});

describe('JournalStore — architecture.md §4.5 step⑥ + §1.1 append-only', () => {
  it('appendChange가 쓴 엔트리를 readAll로 그대로 읽는다', async () => {
    const store = new JournalStore({ baseDir });
    await store.appendChange({
      ts: '2026-09-04T00:00:00.000Z',
      provider: 'agent',
      changes: [
        {
          id: 'agent:register:malgnsoft/claude-plugins',
          target: 'malgnsoft/claude-plugins',
          kind: 'register',
          level: 'L2',
          after: 'installed',
          reversible: true,
          rationale: 'plan()이 결정한 사유',
        },
      ],
      backupPath: null,
      diffHash: 'sha256:deadbeef',
    });

    const entries = await store.readAll();
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ kind: 'change', provider: 'agent', diffHash: 'sha256:deadbeef' });
  });

  it('append-only — 두 번 append하면 두 줄이 순서대로 쌓인다(덮어쓰지 않는다)', async () => {
    const store = new JournalStore({ baseDir });
    await store.appendChange({ ts: 't1', provider: 'agent', changes: [], backupPath: null, diffHash: 'h1' });
    await store.appendChange({ ts: 't2', provider: 'otel', changes: [], backupPath: null, diffHash: 'h2' });

    const entries = await store.readAll();
    expect(entries).toHaveLength(2);
    expect(entries.map((e) => (e as { diffHash?: string }).diffHash)).toEqual(['h1', 'h2']);
  });

  it('파일이 아직 없으면 readAll이 빈 배열을 반환한다(첫 실행 정상 상황)', async () => {
    const store = new JournalStore({ baseDir: join(baseDir, 'never-created') });
    expect(await store.readAll()).toEqual([]);
  });

  it('appendInstall — installEnv 적용 집합(R-19)이 저널에 그대로 남는다', async () => {
    const store = new JournalStore({ baseDir });
    const envSet = { HOMEBREW_NO_INSTALL_UPGRADE: '1', HOMEBREW_NO_AUTO_UPDATE_INTENTIONALLY_UNSET: '' };
    await store.appendInstall({
      ts: '2026-09-04T00:00:00.000Z',
      tool: 'claude',
      strategy: 'PkgManagerStrategy',
      argv: ['/opt/homebrew/bin/brew', 'install', '--cask', 'claude-code'],
      managerPath: '/opt/homebrew/bin/brew',
      managerRealPath: '/opt/homebrew/bin/brew',
      managerOwner: 'root:admin',
      envSet,
      targetVersion: '2.1.236',
      compatFloor: '2.1.237',
      exitCode: 0,
      timedOut: false,
      timeoutKind: null,
      resolvedPath: '/opt/homebrew/bin/claude',
      installOrigin: 'brew',
      signature: 'verified',
    });

    const [entry] = await store.readAll();
    expect(entry).toMatchObject({ kind: 'install', envSet, installOrigin: 'brew' });
  });

  it('appendConsentFailure — MV_CONSENT_INVALID(high) 사고 신호를 저널에 남긴다(§1.2 잔여 요구)', async () => {
    const store = new JournalStore({ baseDir });
    await store.appendConsentFailure({
      ts: '2026-09-04T00:00:00.000Z',
      code: 'MV_CONSENT_INVALID',
      severity: 'high',
      message: 'diffHash 불일치',
      providerId: 'agent',
    });

    const [entry] = await store.readAll();
    expect(entry).toMatchObject({ kind: 'consentFailure', code: 'MV_CONSENT_INVALID', severity: 'high' });
  });

  it('diffHash(sha256 64-hex)는 32자 이상이어도 고엔트로피 마스킹에서 살아남는다', async () => {
    const store = new JournalStore({ baseDir });
    // 실제 diffHash와 같은 형태 — computeDiffHash()가 만드는 값과 동일하게 64자
    // 순수 hex라 마스킹 없이 store.ts에 통과시키면 HIGH_ENTROPY_RE(32자 이상)에
    // 항상 걸린다. 이 테스트가 store.ts의 diffHash 복원 예외를 고정한다.
    const realisticDiffHash = createHash('sha256').update('providerId+changes').digest('hex');
    expect(realisticDiffHash).toHaveLength(64);

    await store.appendChange({
      ts: 't',
      provider: 'agent',
      changes: [],
      backupPath: null,
      diffHash: realisticDiffHash,
    });

    const [entry] = await store.readAll();
    expect(entry).toMatchObject({ diffHash: realisticDiffHash });
  });

  it('[방어심층] 자유 텍스트 필드에 자격증명 형태 값이 섞여도 디스크에 원문이 남지 않는다', async () => {
    const store = new JournalStore({ baseDir });
    const leaked = `Bearer ${'Z'.repeat(40)}`;
    await store.appendChange({
      ts: 't',
      provider: 'agent',
      changes: [
        {
          id: 'agent:exec:1',
          target: 'x',
          kind: 'exec',
          level: 'L2',
          after: 'y',
          reversible: false,
          // rationale은 자유 텍스트다 — 여기에 실수로 값이 섞여도 마스킹돼야 한다
          rationale: `실패 stderr: ${leaked}`,
        },
      ],
      backupPath: null,
      diffHash: 'h',
    });

    // store.readAll()이 아니라 **디스크의 원본 바이트**를 직접 읽어 확인한다 —
    // 파서를 거치지 않고 실제로 파일에 무엇이 저장됐는지가 마스킹 성립의 증거다.
    const raw = await readFile(join(baseDir, 'journal.jsonl'), 'utf8');
    expect(raw).not.toContain(leaked);
    expect(raw).toContain(MASK_MARKER);
  });
});
