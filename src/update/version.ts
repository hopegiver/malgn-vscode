// [경계 표기 · 의도된 중복] AT-U1(architecture.md §3.6.1 · src/architecture-tests/
// updateChannelBoundary.ts)이 `src/update/**` ↔ 정책 모듈 디렉터리(저장소 실제 경로는
// `codeConstants.ts`가 있는 그곳) 양방향 import를 금지한다. 그 디렉터리에 이미 버전
// 비교기(`semver.ts`)가 있지만 그 파일을 import하면 AT-U1이 CI에서 실패해야 정상이다
// (모듈 경계 위반) — 그래서 이 파일은 그것을 재사용하지 않고 독립적으로 최소 버전
// 비교기를 다시 둔다. 중복을 피하려는 시도가 곧 경계를 넘는 시도이므로, 이 중복은
// 실수가 아니라 경계 준수의 직접적 대가다.

export interface ParsedUpdateVersion {
  readonly major: number;
  readonly minor: number;
  readonly patch: number;
}

const VERSION_RE = /^(\d+)\.(\d+)\.(\d+)$/;

/** "1.2.3" 형태만 인정한다(부분 생략·prerelease·build metadata는 이 채널의 계약
 * 밖이라 거부 대상이다 — 업데이트 매니페스트 버전은 항상 완전한 X.Y.Z여야 한다). */
export function parseUpdateVersion(input: string): ParsedUpdateVersion | null {
  const m = VERSION_RE.exec(input.trim());
  if (!m || m[1] === undefined || m[2] === undefined || m[3] === undefined) return null;
  const major = Number(m[1]);
  const minor = Number(m[2]);
  const patch = Number(m[3]);
  if (!Number.isInteger(major) || !Number.isInteger(minor) || !Number.isInteger(patch)) return null;
  return { major, minor, patch };
}

export function compareUpdateVersions(a: ParsedUpdateVersion, b: ParsedUpdateVersion): -1 | 0 | 1 {
  if (a.major !== b.major) return a.major < b.major ? -1 : 1;
  if (a.minor !== b.minor) return a.minor < b.minor ? -1 : 1;
  if (a.patch !== b.patch) return a.patch < b.patch ? -1 : 1;
  return 0;
}
