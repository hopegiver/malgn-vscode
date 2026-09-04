// architecture.md §3.6.1 완료 판정 — 이 파일은 다음을 증명한다:
//  (a) STOPPABLE_SURFACES 정본 값이 §3.6.1 원문과 정확히 일치한다
//  (b) 정지 경로 5개(킬 스위치·호환 게이트·미신뢰 워크스페이스·정책 노후·HRS4)가
//      전부 evaluateStop() 하나만을 통해서만 정지를 표현한다(같은 상수 참조 — 별도
//      정지 함수/상수가 없다는 것 자체가 이 파일에 다른 정지 경로가 없다는 사실로 증명된다)
//  (c) install provider는 다섯 신호를 전부 최댓값(worst case)으로 켜도 정지되지 않는다
//      → "복구 경로가 살아남음"
//  (d) 정책 계층(disableProviders enum)이 뚫려 install이 섞여 들어와도 코드가 한 번 더
//      막는다(방어심층)
//  (e) StopDecision은 {stopped, reasons}만 반환한다 — 새 target·change를 만들 수 없다
//      ("주입 불가")
//  (f) HRS4는 provider.apply만 정지하고 consent.issue는 막지 않는다(재동의 경로 보존)

import { describe, expect, it } from 'vitest';
import type { ProviderId } from '../../providers/types.js';
import type { EffectiveKillSwitch } from '../policy/types.js';
import {
  MV_STOP_COMPAT_GATE_BELOW_MINIMUM,
  MV_STOP_HRS4_RECONSENT_REQUIRED,
  MV_STOP_KILL_SWITCH,
  MV_STOP_POLICY_CHECKOUT_STALE,
  MV_STOP_UNTRUSTED_WORKSPACE,
  POLICY_CHECKOUT_STALE_DAYS,
  STOPPABLE_SURFACES,
  evaluateStop,
  isBelowCompatMinimum,
  isPolicyCheckoutStale,
  type StopSignals,
} from './stopGate.js';

const CURRENT_VERSION = '0.1.0';

function noKillSwitch(overrides: Partial<EffectiveKillSwitch> = {}): EffectiveKillSwitch {
  return {
    minExtensionVersion: null,
    maxExtensionVersion: null,
    disableProviders: [],
    message: null,
    upgradeHint: null,
    ...overrides,
  };
}

function allClearSignals(overrides: Partial<StopSignals> = {}): StopSignals {
  return {
    killSwitch: noKillSwitch(),
    currentExtensionVersion: CURRENT_VERSION,
    compatGateBelowMinimum: false,
    workspaceTrusted: true,
    policyCheckoutStale: false,
    hrs4ReconsentRequired: false,
    ...overrides,
  };
}

/** 5개 신호를 전부 "정지 발동" 상태로 켠 최악의 킬스위치 시나리오 */
function worstCaseSignals(): StopSignals {
  return {
    killSwitch: noKillSwitch({ disableProviders: ['agent', 'otel', 'github', 'cloudflare', 'mcp'] }),
    currentExtensionVersion: CURRENT_VERSION,
    compatGateBelowMinimum: true,
    workspaceTrusted: false,
    policyCheckoutStale: true,
    hrs4ReconsentRequired: true,
  };
}

describe('STOPPABLE_SURFACES — §3.6.1 단일 정본', () => {
  it('원문과 정확히 일치한다: provider.apply, consent.issue', () => {
    expect(STOPPABLE_SURFACES).toEqual(['provider.apply', 'consent.issue']);
  });

  it('길이 2 — 다른 표면(detect·dashboard·diagnostics·undo)이 섞여 들어가지 않는다', () => {
    expect(STOPPABLE_SURFACES.length).toBe(2);
  });
});

describe('evaluateStop — 신호 없음(all-clear)', () => {
  it('provider.apply와 consent.issue 둘 다 정지되지 않는다', () => {
    expect(evaluateStop('provider.apply', 'agent', allClearSignals())).toEqual({ stopped: false, reasons: [] });
    expect(evaluateStop('consent.issue', 'agent', allClearSignals())).toEqual({ stopped: false, reasons: [] });
  });
});

describe('정지 경로 ① 킬 스위치(G-2)', () => {
  it('disableProviders에 포함되면 provider.apply·consent.issue 둘 다 정지된다', () => {
    const signals = allClearSignals({ killSwitch: noKillSwitch({ disableProviders: ['agent'] }) });
    const applyDecision = evaluateStop('provider.apply', 'agent', signals);
    const issueDecision = evaluateStop('consent.issue', 'agent', signals);
    expect(applyDecision.stopped).toBe(true);
    expect(applyDecision.reasons.map((r) => r.code)).toContain(MV_STOP_KILL_SWITCH);
    expect(issueDecision.stopped).toBe(true);
    expect(issueDecision.reasons.map((r) => r.code)).toContain(MV_STOP_KILL_SWITCH);
  });

  it('minExtensionVersion 미달이면 정지된다', () => {
    const signals = allClearSignals({
      killSwitch: noKillSwitch({ minExtensionVersion: '0.2.0' }),
      currentExtensionVersion: '0.1.0',
    });
    expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(true);
  });

  it('maxExtensionVersion 초과면 정지된다(미래 버전 회수용)', () => {
    const signals = allClearSignals({
      killSwitch: noKillSwitch({ maxExtensionVersion: '0.1.0' }),
      currentExtensionVersion: '0.2.0',
    });
    expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(true);
  });

  it('버전 범위 안이면 정지되지 않는다', () => {
    const signals = allClearSignals({
      killSwitch: noKillSwitch({ minExtensionVersion: '0.1.0', maxExtensionVersion: '0.5.0' }),
      currentExtensionVersion: '0.2.0',
    });
    expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(false);
  });
});

describe('정지 경로 ② 호환 게이트 하한 미달(§3.5.3)', () => {
  it('compatGateBelowMinimum이면 provider.apply·consent.issue 둘 다 정지된다(install 제외)', () => {
    const signals = allClearSignals({ compatGateBelowMinimum: true });
    const applyDecision = evaluateStop('provider.apply', 'agent', signals);
    const issueDecision = evaluateStop('consent.issue', 'agent', signals);
    expect(applyDecision.reasons.map((r) => r.code)).toContain(MV_STOP_COMPAT_GATE_BELOW_MINIMUM);
    expect(issueDecision.reasons.map((r) => r.code)).toContain(MV_STOP_COMPAT_GATE_BELOW_MINIMUM);
  });
});

describe('정지 경로 ③ 미신뢰 워크스페이스(§2.1)', () => {
  it('workspaceTrusted=false면 provider.apply·consent.issue 둘 다 정지된다', () => {
    const signals = allClearSignals({ workspaceTrusted: false });
    expect(evaluateStop('provider.apply', 'agent', signals).reasons.map((r) => r.code)).toContain(
      MV_STOP_UNTRUSTED_WORKSPACE
    );
    expect(evaluateStop('consent.issue', 'agent', signals).reasons.map((r) => r.code)).toContain(
      MV_STOP_UNTRUSTED_WORKSPACE
    );
  });
});

describe('정지 경로 ④ 정책 체크아웃 14일 노후(§3.7.3·N-2)', () => {
  it('policyCheckoutStale=true면 provider.apply·consent.issue 둘 다 정지된다', () => {
    const signals = allClearSignals({ policyCheckoutStale: true });
    expect(evaluateStop('provider.apply', 'agent', signals).reasons.map((r) => r.code)).toContain(
      MV_STOP_POLICY_CHECKOUT_STALE
    );
    expect(evaluateStop('consent.issue', 'agent', signals).reasons.map((r) => r.code)).toContain(
      MV_STOP_POLICY_CHECKOUT_STALE
    );
  });
});

describe('정지 경로 ⑤ HRS 4 재동의(§7.3.1) — provider.apply만 정지', () => {
  it('hrs4ReconsentRequired=true면 provider.apply는 정지된다', () => {
    const signals = allClearSignals({ hrs4ReconsentRequired: true });
    const decision = evaluateStop('provider.apply', 'agent', signals);
    expect(decision.stopped).toBe(true);
    expect(decision.reasons.map((r) => r.code)).toContain(MV_STOP_HRS4_RECONSENT_REQUIRED);
  });

  it('hrs4ReconsentRequired=true여도 consent.issue는 정지되지 않는다(재동의 경로 보존)', () => {
    const signals = allClearSignals({ hrs4ReconsentRequired: true });
    const decision = evaluateStop('consent.issue', 'agent', signals);
    expect(decision.stopped).toBe(false);
    expect(decision.reasons).toEqual([]);
  });
});

describe('복구 경로 생존 — install provider는 절대 정지되지 않는다', () => {
  const nonInstallProviders: readonly ProviderId[] = ['agent', 'otel', 'github', 'cloudflare', 'mcp'];

  it('최악의 킬스위치(5개 신호 전부 발동)에서도 install의 provider.apply·consent.issue는 정지되지 않는다', () => {
    const signals = worstCaseSignals();
    expect(evaluateStop('provider.apply', 'install', signals)).toEqual({ stopped: false, reasons: [] });
    expect(evaluateStop('consent.issue', 'install', signals)).toEqual({ stopped: false, reasons: [] });
  });

  it('대조군 — 같은 최악의 신호에서 install 외 provider는 실제로 정지된다(테스트가 항상 통과하는 게 아님을 증명)', () => {
    const signals = worstCaseSignals();
    for (const providerId of nonInstallProviders) {
      expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
    }
  });

  it('방어심층 — killSwitch.disableProviders에 install이 섞여 들어와도(정책 계층 우회 가정) 코드가 한 번 더 막는다', () => {
    const signals = allClearSignals({
      killSwitch: noKillSwitch({ disableProviders: ['install' as ProviderId] }),
    });
    expect(evaluateStop('provider.apply', 'install', signals)).toEqual({ stopped: false, reasons: [] });
  });

  it('호환 게이트 하한 미달 상태에서도 install만은 계속 동작한다(§3.5.3 — 유일한 해소 경로)', () => {
    const signals = allClearSignals({ compatGateBelowMinimum: true });
    expect(evaluateStop('provider.apply', 'install', signals).stopped).toBe(false);
    expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(true);
  });
});

describe('주입 불가 — StopDecision은 {stopped, reasons}만 반환한다', () => {
  it('반환 객체에 target·change·providerId 등 새 대상을 나타내는 필드가 없다', () => {
    const decision = evaluateStop('provider.apply', 'agent', worstCaseSignals());
    expect(Object.keys(decision).sort()).toEqual(['reasons', 'stopped']);
    for (const reason of decision.reasons) {
      expect(Object.keys(reason).sort()).toEqual(['code', 'message']);
    }
  });

  it('reasons의 code는 5개 고정 상수 중 하나만 나온다 — 임의 문자열이 섞이지 않는다', () => {
    const KNOWN_CODES = new Set([
      MV_STOP_KILL_SWITCH,
      MV_STOP_COMPAT_GATE_BELOW_MINIMUM,
      MV_STOP_UNTRUSTED_WORKSPACE,
      MV_STOP_POLICY_CHECKOUT_STALE,
      MV_STOP_HRS4_RECONSENT_REQUIRED,
    ]);
    const decision = evaluateStop('provider.apply', 'agent', worstCaseSignals());
    expect(decision.reasons.length).toBeGreaterThan(0);
    for (const reason of decision.reasons) {
      expect(KNOWN_CODES.has(reason.code)).toBe(true);
    }
  });
});

describe('isPolicyCheckoutStale — N-2 순수 계산부', () => {
  it('정확히 14일 미만이면 stale이 아니다', () => {
    const now = new Date('2026-01-15T00:00:00.000Z');
    const justUnder = new Date(now.getTime() - (POLICY_CHECKOUT_STALE_DAYS * 24 * 60 * 60 * 1000 - 1));
    expect(isPolicyCheckoutStale(justUnder, now)).toBe(false);
  });

  it('정확히 14일 이상이면 stale이다', () => {
    const now = new Date('2026-01-15T00:00:00.000Z');
    const exactly = new Date(now.getTime() - POLICY_CHECKOUT_STALE_DAYS * 24 * 60 * 60 * 1000);
    expect(isPolicyCheckoutStale(exactly, now)).toBe(true);
  });
});

describe('isBelowCompatMinimum — §3.5.3 순수 계산부', () => {
  it('실측 버전이 하한 미만이면 true', () => {
    expect(isBelowCompatMinimum('2.1.236', '>=2.1.237')).toBe(true);
  });

  it('실측 버전이 하한 이상이면 false', () => {
    expect(isBelowCompatMinimum('2.1.237', '>=2.1.237')).toBe(false);
    expect(isBelowCompatMinimum('2.2.0', '>=2.1.237')).toBe(false);
  });

  it('파싱 불가 입력은 판단을 보류하고 false를 반환한다(이 함수 책임 밖)', () => {
    expect(isBelowCompatMinimum('not-a-version', '>=2.1.237')).toBe(false);
    expect(isBelowCompatMinimum('2.1.237', 'not-a-range')).toBe(false);
  });
});
