import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { TrustLedger, computeTargetFolderTrusted } from './ledger.js';

let baseDir: string;
let targetFolder: string;

beforeEach(async () => {
  // 앱 데이터 디렉터리(원장 저장 위치)와 "대상 폴더"(신뢰 대상)를 서로 다른 임시
  // 디렉터리로 둔다 — §3.6.1 ③ "대상 폴더 안에 두지 않는다"를 테스트 구조 자체로도
  // 반영한다(원장 파일이 대상 폴더 트리 밖에 있다는 것을 아래 테스트가 별도로 확인한다).
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-trust-ledger-'));
  targetFolder = await mkdtemp(join(tmpdir(), 'malgn-target-folder-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
  await rm(targetFolder, { recursive: true, force: true });
});

describe('TrustLedger — C-11 해소: 기본값 untrusted', () => {
  it('원장 파일 자체가 없는 최초 실행 상태에서 조회하면 untrusted다', async () => {
    const ledger = new TrustLedger({ baseDir });
    expect(await ledger.get(targetFolder)).toBe('untrusted');
  });

  it('원장에 없는(다른 폴더만 등록된) 폴더를 조회하면 untrusted다', async () => {
    const ledger = new TrustLedger({ baseDir });
    const otherFolder = await mkdtemp(join(tmpdir(), 'malgn-other-folder-'));
    try {
      await ledger.grant(otherFolder);
      expect(await ledger.get(targetFolder)).toBe('untrusted');
      expect(await ledger.get(otherFolder)).toBe('trusted');
    } finally {
      await rm(otherFolder, { recursive: true, force: true });
    }
  });

  it('grant() 이후에만 trusted로 바뀐다 — grant 전에는 untrusted', async () => {
    const ledger = new TrustLedger({ baseDir });
    expect(await ledger.get(targetFolder)).toBe('untrusted');
    await ledger.grant(targetFolder);
    expect(await ledger.get(targetFolder)).toBe('trusted');
  });

  it('revoke() 이후에는 다시 untrusted로 돌아간다(원장에서 제거 — 명시적 false로 남기지 않는다)', async () => {
    const ledger = new TrustLedger({ baseDir });
    await ledger.grant(targetFolder);
    expect(await ledger.get(targetFolder)).toBe('trusted');
    await ledger.revoke(targetFolder);
    expect(await ledger.get(targetFolder)).toBe('untrusted');
  });

  it('상대경로로 grant해도 절대경로 조회와 같은 대상으로 취급된다(resolve 정규화)', async () => {
    const ledger = new TrustLedger({ baseDir });
    const relative = join(targetFolder, '..', targetFolder.split('/').pop()!);
    await ledger.grant(relative);
    expect(await ledger.get(targetFolder)).toBe('trusted');
  });

  it('[손상 원장 fail-closed] 원장 파일이 유효한 JSON이 아니면 조회는 untrusted다(자동 복구·재작성 없음)', async () => {
    const ledger = new TrustLedger({ baseDir });
    await writeFile(join(baseDir, 'trust-ledger.json'), 'not json at all', 'utf8');
    expect(await ledger.get(targetFolder)).toBe('untrusted');
  });

  it('[손상 값 fail-closed] 원장 항목 값이 "trusted"/"untrusted"가 아니면 그 항목은 버려진다(untrusted로 읽힌다)', async () => {
    await writeFile(
      join(baseDir, 'trust-ledger.json'),
      JSON.stringify({ [targetFolder]: true }), // 불리언 true — 문자열 리터럴이 아니다
      'utf8'
    );
    const ledger = new TrustLedger({ baseDir });
    expect(await ledger.get(targetFolder)).toBe('untrusted');
  });

  it('원장 파일은 대상 폴더 밖(baseDir)에 저장된다 — 대상 폴더 트리 안에 신뢰 상태를 스스로 선언하지 않는다', async () => {
    const ledger = new TrustLedger({ baseDir });
    await ledger.grant(targetFolder);
    // baseDir와 targetFolder는 이 테스트에서 애초에 서로 다른 임시 디렉터리다 — grant() 호출이
    // baseDir 밖(targetFolder 하위)에 아무 파일도 만들지 않았음을 별도로 확인한다.
    const { readdir } = await import('node:fs/promises');
    const targetFolderContents = await readdir(targetFolder);
    expect(targetFolderContents).toEqual([]);
  });
});

describe('computeTargetFolderTrusted — StopSignals.targetFolderTrusted로의 단일 변환 지점', () => {
  it("'trusted'만 true로 변환된다", () => {
    expect(computeTargetFolderTrusted('trusted')).toBe(true);
    expect(computeTargetFolderTrusted('untrusted')).toBe(false);
  });
});
