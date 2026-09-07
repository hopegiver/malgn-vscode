// 통합 테스트 — U-1~U-4·U-6이 실제로 한 흐름 안에서 맞물리는지 확인한다. 개별 축의
// 상세 테스트는 각 모듈의 *.test.ts(ed25519Verify·manifestVerifier·artifactVerifier·
// downgradeGuard)에 있다. 이 파일은 transport/installer/전환 기록을 fake로 주입해
// "실제 네트워크 요청을 코드에 넣지 마십시오"라는 작업 지시를 그대로 지킨다.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { canonicalizeUpdateManifest } from './manifest.js';
import type { SignedUpdateManifest, UnsignedUpdateManifest, VerifiedUpdateManifest } from './manifest.js';
import { runUpdateFlow } from './updateFlow.js';
import type { RunUpdateFlowDeps, UpdateChannelConfig, UpdateInstaller, UpdateTransport } from './updateFlow.js';
import type { VersionTransitionRecord, VersionTransitionSink } from './versionTransitionRecord.js';
import { NoUpdatePublicKeysConfiguredError } from './ed25519Verify.js';
import { UpdateManifestSignatureInvalidError } from './manifestVerifier.js';
import { UpdateArtifactHashMismatchError } from './artifactVerifier.js';
import { UpdateDowngradeRejectedError } from './downgradeGuard.js';
import type { UpdatePublicKey } from './publicKeys.js';

const HERE = dirname(fileURLToPath(import.meta.url));

function sha256Of(buffer: Buffer): string {
  return `sha256:${createHash('sha256').update(buffer).digest('hex')}`;
}

function issueTestKeyPair(): { publicKey: UpdatePublicKey; sign: (data: Buffer) => string } {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' }) as { x: string };
  return {
    publicKey: { label: 'test', x: jwk.x },
    sign: (data: Buffer) => cryptoSign(null, data, privateKey).toString('base64'),
  };
}

function buildFixture() {
  const key = issueTestKeyPair();
  const artifact = Buffer.from('this is the v1.4.0 artifact payload');
  const unsigned: UnsignedUpdateManifest = {
    schemaVersion: 1,
    version: '1.4.0',
    artifactUrl: 'https://updates.example.com/v1.4.0/artifact.bin',
    artifactSha256: sha256Of(artifact),
    publishedAt: '2026-09-07T00:00:00.000Z',
  };
  const signature = key.sign(canonicalizeUpdateManifest(unsigned));
  const signed: SignedUpdateManifest = { ...unsigned, signature };
  return { key, artifact, signed };
}

class RecordingTransport implements UpdateTransport {
  fetchedAuthorities: string[] = [];
  constructor(private readonly manifest: unknown, private readonly artifact: Buffer) {}
  async fetchManifest(authority: string): Promise<unknown> {
    this.fetchedAuthorities.push(authority);
    return this.manifest;
  }
  async fetchArtifact(): Promise<Buffer> {
    return this.artifact;
  }
}

class RecordingInstaller implements UpdateInstaller {
  installedManifests: VerifiedUpdateManifest[] = [];
  async installVerifiedUpdate(_artifact: Buffer, manifest: VerifiedUpdateManifest): Promise<void> {
    this.installedManifests.push(manifest);
  }
}

class RecordingSink implements VersionTransitionSink {
  entries: VersionTransitionRecord[] = [];
  async record(entry: VersionTransitionRecord): Promise<void> {
    this.entries.push(entry);
  }
}

function makeDeps(overrides: {
  transport: UpdateTransport;
  installer: UpdateInstaller;
  transitions: VersionTransitionSink;
  channel: UpdateChannelConfig;
  currentVersion: string;
}): RunUpdateFlowDeps {
  return { ...overrides, now: () => '2026-09-07T01:00:00.000Z' };
}

describe('runUpdateFlow — 통합', () => {
  it('양성 대조군 — 정상 서명 + 정상 아티팩트는 검증을 통과하고 설치·기록이 정확히 한 번씩 일어난다', async () => {
    const { key, artifact, signed } = buildFixture();
    const transport = new RecordingTransport(signed, artifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    const result = await runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.3.0' }));

    expect(result).toEqual({ outcome: 'installed', fromVersion: '1.3.0', toVersion: '1.4.0' });
    expect(installer.installedManifests).toHaveLength(1);
    expect(transitions.entries).toEqual([{ ts: '2026-09-07T01:00:00.000Z', fromVersion: '1.3.0', toVersion: '1.4.0', trigger: 'timer' }]);
    expect(transport.fetchedAuthorities).toEqual([channel.authority]);
  });

  it('변조 매니페스트 거부 — 전송 중 매니페스트 필드가 바뀌면 설치·기록 없이 예외로 끝난다', async () => {
    const { key, artifact, signed } = buildFixture();
    const tampered: SignedUpdateManifest = { ...signed, version: '9.9.9' };
    const transport = new RecordingTransport(tampered, artifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    await expect(
      runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.3.0' }))
    ).rejects.toThrow(UpdateManifestSignatureInvalidError);
    expect(installer.installedManifests).toHaveLength(0);
    expect(transitions.entries).toHaveLength(0);
  });

  it('변조 아티팩트 거부 — 매니페스트는 진짜인데 받은 바이트가 다르면 설치·기록 없이 예외로 끝난다', async () => {
    const { key, signed } = buildFixture();
    const swappedArtifact = Buffer.from('a completely different payload');
    const transport = new RecordingTransport(signed, swappedArtifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    await expect(
      runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.3.0' }))
    ).rejects.toThrow(UpdateArtifactHashMismatchError);
    expect(installer.installedManifests).toHaveLength(0);
    expect(transitions.entries).toHaveLength(0);
  });

  it('다운그레이드 거부 — 서명은 유효해도 현재 버전보다 낮으면 거부한다', async () => {
    const { key, artifact, signed } = buildFixture(); // signed.version === '1.4.0'
    const transport = new RecordingTransport(signed, artifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    await expect(
      runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '2.0.0' }))
    ).rejects.toThrow(UpdateDowngradeRejectedError);
    expect(installer.installedManifests).toHaveLength(0);
  });

  it('키 부재 시 전면 거부 — 채널에 공개키가 0개면(K-3 미발급) 서명이 형식상 그럴듯해도 전부 거부한다', async () => {
    const { artifact, signed } = buildFixture();
    const transport = new RecordingTransport(signed, artifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    await expect(
      runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.3.0' }))
    ).rejects.toThrow(NoUpdatePublicKeysConfiguredError);
    expect(installer.installedManifests).toHaveLength(0);
  });

  it('현재 버전과 동일한 매니페스트는 설치·기록 없이 up-to-date로 끝난다', async () => {
    const { key, artifact, signed } = buildFixture();
    const transport = new RecordingTransport(signed, artifact);
    const installer = new RecordingInstaller();
    const transitions = new RecordingSink();
    const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

    const result = await runUpdateFlow({ kind: 'user-command' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.4.0' }));

    expect(result).toEqual({ outcome: 'up-to-date' });
    expect(installer.installedManifests).toHaveLength(0);
    expect(transitions.entries).toHaveLength(0);
  });

  describe('환경변수는 authority 선택에 어떤 영향도 주지 않는다(U-3 — "코드 상수 밖 authority 거부")', () => {
    beforeEach(() => {
      process.env.UPDATE_AUTHORITY_OVERRIDE = 'attacker.example.com';
    });
    afterEach(() => {
      delete process.env.UPDATE_AUTHORITY_OVERRIDE;
    });

    it('환경변수가 설정돼 있어도 transport는 channel.authority(코드 상수에서 조립된 값)로만 호출된다', async () => {
      const { key, artifact, signed } = buildFixture();
      const transport = new RecordingTransport(signed, artifact);
      const installer = new RecordingInstaller();
      const transitions = new RecordingSink();
      const channel: UpdateChannelConfig = { authority: 'updates.example.com', publicKeys: [key.publicKey], bundleIdentifier: 'com.malgnsoft.malgn-vscode' };

      await runUpdateFlow({ kind: 'timer' }, makeDeps({ transport, installer, transitions, channel, currentVersion: '1.3.0' }));

      expect(transport.fetchedAuthorities).toEqual(['updates.example.com']);
      expect(transport.fetchedAuthorities).not.toContain('attacker.example.com');
    });
  });
});

describe('U-6 승격 없음 — 구조 확인', () => {
  it('updateFlow.ts는 자식 프로세스 실행 모듈을 import하지 않는다(설치는 항상 주입된 installer에 위임한다)', () => {
    const source = readFileSync(join(HERE, 'updateFlow.ts'), 'utf8');
    // 주석에서 "이 모듈을 안 쓴다"고 설명하는 것과 실제로 import하는 것은 다르다 —
    // 실제 import/require 구문 형태만 검사한다(설명용 문장까지 걸리면 이 검사
    // 자신이 오탐한다).
    expect(source).not.toMatch(/from\s+['"]node:child_process['"]/);
    expect(source).not.toMatch(/require\(\s*['"]node:child_process['"]\s*\)/);
    expect(source).not.toMatch(/\bexecSync\s*\(|\bspawnSync?\s*\(/);
  });
});
