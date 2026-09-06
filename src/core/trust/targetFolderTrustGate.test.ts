// C-11 재현 방지 — "원장 조회" 형태의 회귀 테스트.
//
// stopGate.test.ts는 `targetFolderTrusted`를 리터럴 불리언으로 **주입**해 evaluateStop()의
// 순수 번역 로직만 검증한다. 그 방식만 남기면 신뢰 원장을 통째로 지우거나
// `TrustLedger.get()`을 `return 'trusted'`로 상수화해도 이 테스트들은 계속 통과한다 —
// "함수는 옳게 동작하고 아무도 실제 신호를 생산하지 않는" C-11의 실질이 그대로
// 재현된다는 뜻이다.
//
// 이 파일은 다르다: 실제 `TrustLedger`를 임시 디렉터리에 만들고, **grant()를 한 번도
// 호출하지 않은 상태**에서 원장을 조회해 얻은 값을 `evaluateStop()`에 그대로 먹인다.
// 리터럴 `false`를 손으로 쓰지 않는다 — 신호가 "생산되는 경로" 전체(원장 조회 →
// computeTargetFolderTrusted 변환 → StopSignals → evaluateStop)를 한 번에 통과시킨다.
// `TrustLedger.get()`을 상수 `'trusted'`로 바꾸거나 원장 클래스 자체를 지우면(호출부를
// `true` 리터럴로 때우면) 이 파일의 "최초 실행은 정지된다" 테스트가 즉시 실패한다 —
// 완료판정 #3이 요구하는 실측을 이 파일로 재현했다(반환문에 원복 결과를 기록한다).

import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  MV_STOP_TARGET_FOLDER_UNTRUSTED,
  evaluateStop,
  type StopSignals,
} from '../reconciler/stopGate.js';
import type { EffectiveKillSwitch } from '../policy/types.js';
import { TrustLedger, computeTargetFolderTrusted } from './ledger.js';

let baseDir: string;
let targetFolder: string;

beforeEach(async () => {
  baseDir = await mkdtemp(join(tmpdir(), 'malgn-trust-ledger-gate-'));
  targetFolder = await mkdtemp(join(tmpdir(), 'malgn-target-folder-gate-'));
});

afterEach(async () => {
  await rm(baseDir, { recursive: true, force: true });
  await rm(targetFolder, { recursive: true, force: true });
});

const NO_KILL_SWITCH: EffectiveKillSwitch = {
  minAppVersion: null,
  maxAppVersion: null,
  disableProviders: [],
  message: null,
  upgradeHint: null,
};

function otherSignalsAllClear(targetFolderTrusted: boolean): StopSignals {
  return {
    killSwitch: NO_KILL_SWITCH,
    currentExtensionVersion: '0.1.0',
    compatGateBelowMinimum: false,
    targetFolderTrusted,
    policyCheckoutStale: false,
    hrs4ReconsentRequired: false,
    sessionAttended: true,
  };
}

describe('C-11 재현 방지 — 신뢰 원장 조회 결과가 실제로 evaluateStop을 정지시킨다', () => {
  it('최초 실행(grant를 한 번도 호출하지 않음) — 원장 조회 → 신호 변환 → evaluateStop 전 경로를 실제로 통과시키면 provider.apply·consent.issue가 정지된다', async () => {
    const ledger = new TrustLedger({ baseDir });

    // 리터럴 false를 쓰지 않는다 — 원장을 실제로 조회한 결과를 그대로 신호로 쓴다.
    const trustState = await ledger.get(targetFolder);
    const targetFolderTrusted = computeTargetFolderTrusted(trustState);

    const signals = otherSignalsAllClear(targetFolderTrusted);
    const applyDecision = evaluateStop('provider.apply', 'agent', signals);
    const issueDecision = evaluateStop('consent.issue', 'agent', signals);

    expect(applyDecision.stopped).toBe(true);
    expect(applyDecision.reasons.map((r) => r.code)).toContain(MV_STOP_TARGET_FOLDER_UNTRUSTED);
    expect(issueDecision.stopped).toBe(true);
    expect(issueDecision.reasons.map((r) => r.code)).toContain(MV_STOP_TARGET_FOLDER_UNTRUSTED);
  });

  it('install도 예외 없이 정지된다(§3.6.1 판정 A-1) — 원장 조회 경로로도 동일하게 성립한다', async () => {
    const ledger = new TrustLedger({ baseDir });
    const targetFolderTrusted = computeTargetFolderTrusted(await ledger.get(targetFolder));
    const signals = otherSignalsAllClear(targetFolderTrusted);

    expect(evaluateStop('provider.apply', 'install', signals).stopped).toBe(true);
  });

  it('L2 동의로 grant()한 뒤에는 같은 경로가 정지를 만들지 않는다', async () => {
    const ledger = new TrustLedger({ baseDir });
    await ledger.grant(targetFolder);

    const targetFolderTrusted = computeTargetFolderTrusted(await ledger.get(targetFolder));
    const signals = otherSignalsAllClear(targetFolderTrusted);

    const decision = evaluateStop('provider.apply', 'agent', signals);
    expect(decision.reasons.map((r) => r.code)).not.toContain(MV_STOP_TARGET_FOLDER_UNTRUSTED);
  });

  it('다른 폴더에 grant해도 이 폴더는 여전히 정지된다(머신 단위 원장이지 전역 허용이 아니다)', async () => {
    const ledger = new TrustLedger({ baseDir });
    const otherFolder = await mkdtemp(join(tmpdir(), 'malgn-other-granted-'));
    try {
      await ledger.grant(otherFolder);
      const targetFolderTrusted = computeTargetFolderTrusted(await ledger.get(targetFolder));
      const signals = otherSignalsAllClear(targetFolderTrusted);
      expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(true);
    } finally {
      await rm(otherFolder, { recursive: true, force: true });
    }
  });
});
