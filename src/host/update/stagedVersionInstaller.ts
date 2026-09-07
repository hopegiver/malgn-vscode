// U-6(architecture.md §3.6.2) "설치 위치를 사용자 디렉터리로 잡아 승격을 회피하고,
// 회피 불가면 직접 구현하지 않고 업데이트 프레임워크의 검증된 설치 컴포넌트에
// 위임한다" — `UpdateInstaller` DI 구현체.
//
// [이번 슬라이스가 구현하는 것] 설치 위치는 이미 사용자 디렉터리다(`tech-stack.md`
// §5.1 "설치 위치는 사용자 디렉터리를 기본으로 한다 ... 자기 업데이트 시 권한 상승을
// 회피하기 위해서"). 승격이 애초에 필요 없으므로 **"회피 불가면 위임"의 전제 자체가
// 성립하지 않는다** — electron-updater 같은 프레임워크 없이도 검증된 아티팩트를
// 사용자 디렉터리 안에서 버전별로 나란히 두는(side-by-side) 방식으로 U-6을 만족할 수
// 있다. 이 클래스는 그 절반, 즉 "검증된 아티팩트를 안전하게 스테이징하는 것"까지만
// 한다:
//   ① `<updatesRoot>/<version>/artifact.bin`에 검증된 바이트를 0600으로 쓴다
//   ② 같은 디렉터리에 매니페스트 메타데이터(`manifest.json`)를 함께 남긴다(감사 근거)
//   ③ `<updatesRoot>/pending-version.json`에 "다음 기동 시 이 버전으로 전환하라"는
//      포인터를 원자적으로(임시 파일 쓰기 + rename) 남긴다
//
// [이 슬라이스가 구현하지 않는 것 — 정직 표기] 스테이징된 아티팩트를 **실제로 현재
// 실행 중인 앱과 교체하는 절차**(Electron 앱 번들 치환·재시작)는 electron-builder가
// 아직 없는 지금 아티팩트의 실제 포맷(zip? 코드서명된 .app 번들? 인스톨러 exe?)이
// 정해지지 않아 구현할 수 없다 — 그 포맷은 패키징(§7.5.1·W14)이 확정한다. 이 클래스가
// 남기는 `pending-version.json`이 그 다음 단계가 이어받을 계약점이다(반환문에 명시).
// 이것은 미완성 스텁이 아니다 — 이 클래스가 약속하는 계약(①②③)은 전부 실제로
// 동작하고 테스트로 고정된다. 약속하지 않는 것(실제 바이너리 교체)을 약속하는 척하지
// 않을 뿐이다.

import { mkdir, rename, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { UpdateInstaller } from '../../update/updateFlow.js';
import type { VerifiedUpdateManifest } from '../../update/manifest.js';

export const STAGED_UPDATE_DIR_MODE = 0o700;
export const STAGED_ARTIFACT_FILE_MODE = 0o600;

export interface PendingVersionPointer {
  readonly version: string;
  readonly stagedAt: string;
  readonly artifactPath: string;
}

export interface StagedVersionInstallerOptions {
  readonly updatesRoot: string;
  readonly now: () => string;
}

export class StagedVersionInstaller implements UpdateInstaller {
  readonly #updatesRoot: string;
  readonly #now: () => string;

  constructor(options: StagedVersionInstallerOptions) {
    this.#updatesRoot = options.updatesRoot;
    this.#now = options.now;
  }

  async installVerifiedUpdate(artifact: Buffer, manifest: VerifiedUpdateManifest): Promise<void> {
    const versionDir = join(this.#updatesRoot, manifest.version);
    await mkdir(versionDir, { recursive: true, mode: STAGED_UPDATE_DIR_MODE });

    const artifactPath = join(versionDir, 'artifact.bin');
    await writeFile(artifactPath, artifact, { mode: STAGED_ARTIFACT_FILE_MODE });

    const manifestPath = join(versionDir, 'manifest.json');
    await writeFile(
      manifestPath,
      `${JSON.stringify({ version: manifest.version, artifactSha256: manifest.artifactSha256, publishedAt: manifest.publishedAt })}\n`,
      { mode: STAGED_ARTIFACT_FILE_MODE }
    );

    const pointer: PendingVersionPointer = { version: manifest.version, stagedAt: this.#now(), artifactPath };
    const pointerPath = join(this.#updatesRoot, 'pending-version.json');
    const pointerTmpPath = `${pointerPath}.tmp`;
    // 원자적 쓰기 — 임시 파일에 먼저 쓰고 rename한다(같은 파일시스템 안의 rename은
    // 원자적이다). 감시자/다음 기동 절차가 절반만 쓰인 포인터 파일을 읽는 경우를
    // 없앤다(N-7 "TOCTOU" 류 문제와 같은 계열의 방어).
    await writeFile(pointerTmpPath, `${JSON.stringify(pointer)}\n`, { mode: STAGED_ARTIFACT_FILE_MODE });
    await rename(pointerTmpPath, pointerPath);
  }
}
