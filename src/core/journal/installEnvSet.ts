// R-19 — "옵션 이름이 바뀌면 env 가드가 조용히 무효가 된다... env 집합을 저널에
// 기록해 사후 대조하고 가드 이름을 한 곳에 둔다"(architecture.md R-19 표 · policy-contract.md
// §3 "잔여 위험"). `installEnv` 값 자체의 정본은 이미 `loadCodeConstants().installEnv`
// (compat/install-env.json, W2/W6이 배선)다 — 이 파일은 그 정본을 저널이 기록할 형태로
// 다시 노출하는 통로일 뿐이고, 값을 다시 정의하지 않는다("가드 이름을 한 곳에 둔다"의
// 실행).

import { loadCodeConstants } from '../policy/codeConstants.js';

/**
 * "이번 install 실행에 실제로 적용한 env 집합"을 `InstallJournalEntry.envSet`에 넣을
 * 형태로 반환한다. W10(install provider)이 아직 없어 "실제로 자식 프로세스에 건 env"를
 * 관측할 exec 경로가 없다 — 지금 이 함수가 반환하는 것은 "적용해야 할 집합"(코드 상수
 * 정본)이며, W10은 실제 실행 직전 이 값을 그대로 `exec()`의 env로 넘기고 그 **동일한
 * 참조**를 저널에 기록해야 한다(값을 다시 계산하면 "실제로 적용한 것"과 "기록한 것"이
 * 갈릴 수 있다 — 저널의 목적 자체가 무너진다).
 */
export function deriveInstallEnvSet(): Readonly<Record<string, string>> {
  return loadCodeConstants().installEnv;
}
