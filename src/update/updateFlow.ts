// docs/architecture.md §3.6.2 — U-1~U-4·U-6의 실행 순서 정본. 이 파일이 각 축의
// 검증 함수를 순서대로 엮는다: 서명 검증(U-1 매니페스트 축) → 다운그레이드 거부
// (U-2) → 해시 대조(U-1 아티팩트 축) → 설치 위임(U-6) → 전환 기록(U-4 저널 축).
//
// [U-6 — 승격 없음] 이 파일은 파일시스템에 쓰지 않고 프로세스를 실행하지 않는다
// (`node:child_process`를 import하지 않는다 — `updateFlow.test.ts`가 소스 텍스트로
// 이것을 직접 확인한다). 실제 설치는 `UpdateInstaller`(주입받는 인터페이스)의 몫이고
// 그 구현체는 이 저장소 밖(호스트 어댑터, W-N1)에 있다 — "설치 위치를 사용자
// 디렉터리로 잡아 승격을 회피하고, 회피 불가면 검증된 설치 컴포넌트에 위임한다"는
// architecture.md 원문의 실행 지점이 그 구현체다.
//
// [트리거 무차별 원칙 — AT-U4] 이 함수는 트리거가 ⓐ타이머 ⓑ사용자 명령 ⓒ서명 검증
// 통과 매니페스트(트리거 자체에 매니페스트를 실어 보내는 경우) 중 무엇이든 같은
// 절차를 밟는다 — 트리거 종류에 따라 검증 단계를 생략하는 분기는 두지 않는다.

import { assertArtifactMatchesVerifiedManifest } from './artifactVerifier.js';
import { assertNotDowngrade } from './downgradeGuard.js';
import { verifyUpdateManifest } from './manifestVerifier.js';
import type { VerifiedUpdateManifest } from './manifest.js';
import type { UpdatePublicKey } from './publicKeys.js';
import type { UpdateTrigger } from './updateTrigger.js';
import { recordVersionTransition } from './versionTransitionRecord.js';
import type { VersionTransitionSink } from './versionTransitionRecord.js';

export interface UpdateTransport {
  fetchManifest(authority: string): Promise<unknown>;
  fetchArtifact(artifactUrl: string): Promise<Buffer>;
}

export interface UpdateInstaller {
  /** 검증을 통과한 아티팩트를 실제로 설치한다. 구현체는 이 모듈 밖(호스트 어댑터)에
   * 있다 — U-6: 이 코어는 여기서 파일시스템에 쓰거나 프로세스를 exec하지 않는다.
   * 설치 위치·승격 회피·설치 프레임워크로의 위임은 구현체의 책임이다. */
  installVerifiedUpdate(artifact: Buffer, manifest: VerifiedUpdateManifest): Promise<void>;
}

/** 채널(운영/개발)에 종속된 세 상수를 한 덩어리로 묶는다. 이 코어는 "어느 채널인가"를
 * 스스로 판단하지 않는다 — composition root(호스트 어댑터, W-N1)가
 * `updateServerAuthority.ts`/`publicKeys.ts` 조합이나 `devUpdateChannel.ts` 조합 중
 * 하나를 골라 이 형태로 조립해 주입한다. */
export interface UpdateChannelConfig {
  readonly authority: string;
  readonly publicKeys: readonly UpdatePublicKey[];
  readonly bundleIdentifier: string;
}

export interface RunUpdateFlowDeps {
  readonly transport: UpdateTransport;
  readonly installer: UpdateInstaller;
  readonly transitions: VersionTransitionSink;
  readonly channel: UpdateChannelConfig;
  readonly currentVersion: string;
  readonly now: () => string;
}

export interface UpdateFlowUpToDateResult {
  readonly outcome: 'up-to-date';
}
export interface UpdateFlowInstalledResult {
  readonly outcome: 'installed';
  readonly fromVersion: string;
  readonly toVersion: string;
}
export type UpdateFlowResult = UpdateFlowUpToDateResult | UpdateFlowInstalledResult;

/**
 * U-1~U-4·U-6 실행 순서 정본. `deps.channel.authority`는 항상 호출자가 코드 상수
 * (`UPDATE_SERVER_AUTHORITY` 또는 `DEV_UPDATE_HOST`)에서 조립해 주입한 값이다 — 이
 * 함수 자신은 `process.env`·`process.argv`를 읽지 않는다(U-3).
 */
export async function runUpdateFlow(trigger: UpdateTrigger, deps: RunUpdateFlowDeps): Promise<UpdateFlowResult> {
  const rawManifest = await deps.transport.fetchManifest(deps.channel.authority);
  // 키가 0개면 verifyUpdateManifest 내부에서 던진다(fail-closed) — 여기서 별도로
  // 삼키지 않는다.
  const manifest: VerifiedUpdateManifest = verifyUpdateManifest(rawManifest, deps.channel.publicKeys);

  if (manifest.version === deps.currentVersion) {
    return { outcome: 'up-to-date' };
  }
  assertNotDowngrade(deps.currentVersion, manifest.version);

  const artifact = await deps.transport.fetchArtifact(manifest.artifactUrl);
  assertArtifactMatchesVerifiedManifest(artifact, manifest);

  await deps.installer.installVerifiedUpdate(artifact, manifest);

  await recordVersionTransition(deps.transitions, {
    ts: deps.now(),
    fromVersion: deps.currentVersion,
    toVersion: manifest.version,
    trigger: trigger.kind,
  });

  return { outcome: 'installed', fromVersion: deps.currentVersion, toVersion: manifest.version };
}
