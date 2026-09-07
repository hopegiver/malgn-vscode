import { createHash, generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { canonicalizeUpdateManifest } from '../../update/manifest.js';
import type { UnsignedUpdateManifest } from '../../update/manifest.js';
import { verifyUpdateManifest } from '../../update/manifestVerifier.js';
import type { UpdatePublicKey } from '../../update/publicKeys.js';
import { StagedVersionInstaller } from './stagedVersionInstaller.js';

function sha256Of(buffer: Buffer): string {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

function issueVerifiedManifest(version: string, artifact: Buffer) {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' }) as { x: string };
  const key: UpdatePublicKey = { label: 'test', x: jwk.x };
  const unsigned: UnsignedUpdateManifest = {
    schemaVersion: 1,
    version,
    artifactUrl: `https://updates.example.com/${version}/artifact.bin`,
    artifactSha256: sha256Of(artifact),
    publishedAt: '2026-09-07T00:00:00.000Z',
  };
  const signature = cryptoSign(null, canonicalizeUpdateManifest(unsigned), privateKey).toString('base64');
  return verifyUpdateManifest({ ...unsigned, signature }, [key]);
}

let updatesRoot: string;

beforeEach(async () => {
  updatesRoot = await mkdtemp(join(tmpdir(), 'malgn-staged-updates-'));
});

afterEach(async () => {
  await rm(updatesRoot, { recursive: true, force: true });
});

describe('StagedVersionInstaller — U-6 승격 없음(사용자 디렉터리 스테이징)', () => {
  it('아티팩트·매니페스트를 버전 디렉터리에 쓰고 pending-version.json 포인터를 남긴다', async () => {
    const artifact = Buffer.from('payload-bytes');
    const manifest = issueVerifiedManifest('1.4.0', artifact);
    const installer = new StagedVersionInstaller({ updatesRoot, now: () => '2026-09-07T01:00:00.000Z' });

    await installer.installVerifiedUpdate(artifact, manifest);

    const artifactPath = join(updatesRoot, '1.4.0', 'artifact.bin');
    expect(await readFile(artifactPath)).toEqual(artifact);

    const manifestJson = JSON.parse(await readFile(join(updatesRoot, '1.4.0', 'manifest.json'), 'utf8'));
    expect(manifestJson).toMatchObject({ version: '1.4.0', artifactSha256: manifest.artifactSha256 });

    const pointer = JSON.parse(await readFile(join(updatesRoot, 'pending-version.json'), 'utf8'));
    expect(pointer).toEqual({ version: '1.4.0', stagedAt: '2026-09-07T01:00:00.000Z', artifactPath });
  });

  it('아티팩트 파일 모드는 0600이다', async () => {
    const artifact = Buffer.from('payload-bytes');
    const manifest = issueVerifiedManifest('1.4.0', artifact);
    const installer = new StagedVersionInstaller({ updatesRoot, now: () => '2026-09-07T01:00:00.000Z' });
    await installer.installVerifiedUpdate(artifact, manifest);
    const st = await stat(join(updatesRoot, '1.4.0', 'artifact.bin'));
    expect(st.mode & 0o777).toBe(0o600);
  });

  it('두 번째 설치가 첫 번째 버전 디렉터리를 지우지 않는다(side-by-side)', async () => {
    const artifactA = Buffer.from('a');
    const manifestA = issueVerifiedManifest('1.4.0', artifactA);
    const installer = new StagedVersionInstaller({ updatesRoot, now: () => 't1' });
    await installer.installVerifiedUpdate(artifactA, manifestA);

    const artifactB = Buffer.from('b');
    const manifestB = issueVerifiedManifest('1.5.0', artifactB);
    await installer.installVerifiedUpdate(artifactB, manifestB);

    expect(await readFile(join(updatesRoot, '1.4.0', 'artifact.bin'))).toEqual(artifactA);
    expect(await readFile(join(updatesRoot, '1.5.0', 'artifact.bin'))).toEqual(artifactB);
    const pointer = JSON.parse(await readFile(join(updatesRoot, 'pending-version.json'), 'utf8'));
    expect(pointer.version).toBe('1.5.0');
  });
});
