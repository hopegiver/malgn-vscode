// U-2 (docs/architecture.md §3.6.2) — "버전 단조 증가 강제. 서명이 유효한 구버전
// (취약점 있는 과거 릴리스)의 재배포는 서명 검증만으로 안 막힌다. PR-9의 업데이트
// 채널 등가물 — 방향이 있는 값은 한쪽으로만 움직인다." 서명이 완전히 유효해도
// 버전이 후퇴하면 여기서 거부한다(서명 검증과는 다른 코드 경로).

import { compareUpdateVersions, parseUpdateVersion } from './version.js';

export class InvalidUpdateVersionError extends Error {}

export class UpdateDowngradeRejectedError extends Error {
  constructor(public readonly currentVersion: string, public readonly candidateVersion: string) {
    super(
      `다운그레이드 거부(U-2): 현재 ${currentVersion} → 후보 ${candidateVersion}. ` +
        '서명이 유효해도 버전은 단조 증가 방향으로만 이동합니다.'
    );
    this.name = 'UpdateDowngradeRejectedError';
  }
}

/**
 * U-2 — `candidateVersion`이 `currentVersion`보다 **엄격히 커야** 통과한다. 같은
 * 버전(재적용)도 여기서는 진행 대상이 아니다("새 버전"이 아니면 업데이트를 트리거할
 * 이유가 없다 — 호출자(`updateFlow.ts`)는 동일 버전을 별도로 "최신 상태"로 처리하고
 * 이 함수를 아예 부르지 않는다). 형식이 깨진 버전 문자열은 fail-closed로 던진다.
 */
export function assertNotDowngrade(currentVersion: string, candidateVersion: string): void {
  const current = parseUpdateVersion(currentVersion);
  if (!current) throw new InvalidUpdateVersionError(`currentVersion 형식 오류: ${currentVersion}`);
  const candidate = parseUpdateVersion(candidateVersion);
  if (!candidate) throw new InvalidUpdateVersionError(`candidateVersion 형식 오류: ${candidateVersion}`);
  if (compareUpdateVersions(candidate, current) <= 0) {
    throw new UpdateDowngradeRejectedError(currentVersion, candidateVersion);
  }
}
