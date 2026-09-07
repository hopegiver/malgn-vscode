// TOCTOU 대조 — architecture.md §3.2.2 ④ "`update` → 읽기 → `install`을 연속 실행하고
// 읽기 시점의 `marketplaceHeadSha`를 install 후 `gitCommitSha`와 대조한다. 불일치는
// `MV_AGENT_SHA_MISMATCH`(high). sha의 역할은 여기 하나뿐이다."
//
// [설계 이탈 — 반환문에 명시] 원문은 "install 후 gitCommitSha"를 `claude plugin list
// --json`의 필드로 상정하지만 실측(2.1.252)에는 그 필드가 없었다(`cli.ts` 주석 참고).
// 대신 마켓플레이스 체크아웃(`installLocation`)이 실제 git 저장소라는 실측 사실
// (`known_marketplaces.json`이 `github` source를 명시)을 이용해 **같은 디렉터리의 git
// HEAD를 읽기 전/후 두 번 직접 조회**한다 — TOCTOU가 잡으려는 사고("읽고 나서 install
// 사이에 체크아웃이 바뀜")를 원문과 동일한 지점(읽기 직후·install 직후)에서 감지하는
// 것은 같고, 신호 출처만 CLI 필드 대신 git 자체로 바꿨다. `git`이 없거나 `installLocation`이
// git 저장소가 아니면(예: 향후 마켓플레이스 소스 종류 변경) **검사를 건너뛴다**(실패로
// 취급하지 않는다) — 이 검사는 탐지지 예방이 아니므로 "확인 못 함"과 "불일치"를 구분해야
// 한다.

import { runExec, type ExecFileFn } from '../../platform/exec.js';

export const GIT_HEAD_TIMEOUT_MS = 5000;

export type ShaProbe = { readonly kind: 'sha'; readonly sha: string } | { readonly kind: 'unavailable' };

/** `git -C <installLocation> rev-parse HEAD` — 읽기 전용, 인자 전량 상수+경로 하나뿐 */
export async function readMarketplaceCheckoutHead(
  installLocation: string,
  execFileFn: ExecFileFn,
  env: Readonly<Record<string, string | undefined>>
): Promise<ShaProbe> {
  const outcome = await runExec({
    execFileFn,
    file: 'git',
    args: ['-C', installLocation, 'rev-parse', 'HEAD'],
    env,
    timeoutMs: GIT_HEAD_TIMEOUT_MS,
  });
  if (outcome.kind !== 'ok') return { kind: 'unavailable' };
  const sha = outcome.stdout.trim();
  if (!/^[0-9a-f]{7,40}$/i.test(sha)) return { kind: 'unavailable' };
  return { kind: 'sha', sha };
}

/** 둘 다 확인 가능하고 다르면만 불일치다 — 한쪽이라도 `unavailable`이면 "확인 못 함"
 * 취급이라 불일치로 판정하지 않는다(정직한 미확인, PR-8과 같은 정신). */
export function shaMismatch(before: ShaProbe, after: ShaProbe): boolean {
  if (before.kind !== 'sha' || after.kind !== 'sha') return false;
  return before.sha !== after.sha;
}
