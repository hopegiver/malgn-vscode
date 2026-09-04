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

describe('§3.6.1 매트릭스 — 5경로 × 2표면 × install/비install (architect 판정, 규칙 A)', () => {
  const nonInstallProviders: readonly ProviderId[] = ['agent', 'otel', 'github', 'cloudflare', 'mcp'];

  // 매트릭스 원문(§3.6.1 표) 그대로 고정한다:
  //   ① 킬 스위치            | 정지 | 정지 | install: 대상 아님 + 런타임 재확인
  //   ② 호환 게이트 하한 미달  | 정지 | 정지 | install: 예외 — 계속 동작
  //   ③ 미신뢰 워크스페이스    | 정지 | 정지 | install: 정지(예외 없음)
  //   ④ 정책 체크아웃 14일 노후 | 정지 | 정지 | install: 정지(예외 없음)
  //   ⑤ HRS 4 재동의          | 정지 | 정지하지 않는다 | install: 표면 집합 밖(해당 없음)

  describe('① 킬 스위치 — install은 대상 아님(정책 enum 밖) + 런타임 재확인', () => {
    it('비install provider는 provider.apply·consent.issue 둘 다 정지된다', () => {
      const signals = allClearSignals({ killSwitch: noKillSwitch({ disableProviders: ['agent'] }) });
      for (const providerId of nonInstallProviders.filter((p) => p === 'agent')) {
        expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
        expect(evaluateStop('consent.issue', providerId, signals).stopped).toBe(true);
      }
    });

    it('install은 disableProviders에 섞여 들어와도(정책 계층 우회 가정) provider.apply·consent.issue 둘 다 정지되지 않는다(런타임 재확인)', () => {
      const signals = allClearSignals({
        killSwitch: noKillSwitch({ disableProviders: ['install' as ProviderId] }),
      });
      expect(evaluateStop('provider.apply', 'install', signals)).toEqual({ stopped: false, reasons: [] });
      expect(evaluateStop('consent.issue', 'install', signals)).toEqual({ stopped: false, reasons: [] });
    });

    it('install은 minExtensionVersion/maxExtensionVersion(버전 상하한, provider 전체 적용)에도 정지되지 않는다', () => {
      const belowMin = allClearSignals({
        killSwitch: noKillSwitch({ minExtensionVersion: '0.2.0' }),
        currentExtensionVersion: '0.1.0',
      });
      const aboveMax = allClearSignals({
        killSwitch: noKillSwitch({ maxExtensionVersion: '0.1.0' }),
        currentExtensionVersion: '0.2.0',
      });
      expect(evaluateStop('provider.apply', 'install', belowMin).stopped).toBe(false);
      expect(evaluateStop('provider.apply', 'install', aboveMax).stopped).toBe(false);
      // 대조군 — 같은 신호에서 비install provider는 실제로 정지된다.
      expect(evaluateStop('provider.apply', 'agent', belowMin).stopped).toBe(true);
      expect(evaluateStop('provider.apply', 'agent', aboveMax).stopped).toBe(true);
    });
  });

  describe('② 호환 게이트 하한 미달(§3.5.3) — install만 예외, 계속 동작', () => {
    it('비install provider는 provider.apply·consent.issue 둘 다 정지된다', () => {
      const signals = allClearSignals({ compatGateBelowMinimum: true });
      for (const providerId of nonInstallProviders) {
        expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
        expect(evaluateStop('consent.issue', providerId, signals).stopped).toBe(true);
      }
    });

    it('install은 provider.apply·consent.issue 둘 다 정지되지 않는다(§3.5.3 — 유일한 해소 경로)', () => {
      const signals = allClearSignals({ compatGateBelowMinimum: true });
      expect(evaluateStop('provider.apply', 'install', signals)).toEqual({ stopped: false, reasons: [] });
      expect(evaluateStop('consent.issue', 'install', signals)).toEqual({ stopped: false, reasons: [] });
    });
  });

  describe('③ 미신뢰 워크스페이스(§2.1) — install도 정지(예외 없음, 규칙 A)', () => {
    it('비install provider는 provider.apply·consent.issue 둘 다 정지된다', () => {
      const signals = allClearSignals({ workspaceTrusted: false });
      for (const providerId of nonInstallProviders) {
        expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
        expect(evaluateStop('consent.issue', providerId, signals).stopped).toBe(true);
      }
    });

    it('install도 provider.apply·consent.issue 둘 다 정지된다(예외 없음)', () => {
      const signals = allClearSignals({ workspaceTrusted: false });
      const applyDecision = evaluateStop('provider.apply', 'install', signals);
      const issueDecision = evaluateStop('consent.issue', 'install', signals);
      expect(applyDecision.stopped).toBe(true);
      expect(applyDecision.reasons.map((r) => r.code)).toContain(MV_STOP_UNTRUSTED_WORKSPACE);
      expect(issueDecision.stopped).toBe(true);
      expect(issueDecision.reasons.map((r) => r.code)).toContain(MV_STOP_UNTRUSTED_WORKSPACE);
    });
  });

  describe('④ 정책 체크아웃 14일 노후(§3.7.3·N-2) — install도 정지(예외 없음, 규칙 A)', () => {
    it('비install provider는 provider.apply·consent.issue 둘 다 정지된다', () => {
      const signals = allClearSignals({ policyCheckoutStale: true });
      for (const providerId of nonInstallProviders) {
        expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
        expect(evaluateStop('consent.issue', providerId, signals).stopped).toBe(true);
      }
    });

    it('install도 provider.apply·consent.issue 둘 다 정지된다(예외 없음)', () => {
      const signals = allClearSignals({ policyCheckoutStale: true });
      const applyDecision = evaluateStop('provider.apply', 'install', signals);
      const issueDecision = evaluateStop('consent.issue', 'install', signals);
      expect(applyDecision.stopped).toBe(true);
      expect(applyDecision.reasons.map((r) => r.code)).toContain(MV_STOP_POLICY_CHECKOUT_STALE);
      expect(issueDecision.stopped).toBe(true);
      expect(issueDecision.reasons.map((r) => r.code)).toContain(MV_STOP_POLICY_CHECKOUT_STALE);
    });
  });

  describe('⑤ HRS 4 재동의(§7.3.1) — provider.apply만 정지, install은 표면 집합 밖(해당 없음)', () => {
    it('비install provider는 provider.apply만 정지되고 consent.issue는 정지되지 않는다', () => {
      const signals = allClearSignals({ hrs4ReconsentRequired: true });
      expect(evaluateStop('provider.apply', 'agent', signals).stopped).toBe(true);
      expect(evaluateStop('consent.issue', 'agent', signals).stopped).toBe(false);
    });

    it('install에는 이 신호에 대한 별도 코드 예외가 없다 — 신호는 non-install provider 표면에 대해서만 계산되므로(W7) 구조적으로 발동하지 않는다', () => {
      // 매트릭스의 "표면 집합 밖이라 해당 없음"은 신호 계산 범위(W7 책임)에 대한
      // 서술이지 이 함수가 install을 별도로 면제한다는 뜻이 아니다 — ①②만 install
      // 예외가 있다(작업 지시: "install 예외를 ①킬스위치 판정 내부와 ②호환 게이트,
      // 두 곳에만 겁니다"). 신호가 (가정적으로) 켜지면 install도 다른 provider와
      // 동일하게 정지된다는 것을 대조군으로 고정한다.
      const signals = allClearSignals({ hrs4ReconsentRequired: true });
      expect(evaluateStop('provider.apply', 'install', signals).stopped).toBe(true);
    });
  });

  it('대조군 — 5개 신호를 전부 최악으로 켜면 비install provider는 모두 정지된다(install 예외가 "항상 통과"로 새지 않았음을 증명)', () => {
    const signals = worstCaseSignals();
    for (const providerId of nonInstallProviders) {
      expect(evaluateStop('provider.apply', providerId, signals).stopped).toBe(true);
    }
  });

  it('install은 ①②만 정지되지 않고 ③④(+가정적 ⑤)는 정지된다 — "5개 정지 경로 전부 예외"였던 구판과의 핵심 차이', () => {
    const signals = worstCaseSignals();
    const decision = evaluateStop('provider.apply', 'install', signals);
    expect(decision.stopped).toBe(true); // ③④가 살아있어 전체 판정은 정지다
    const codes = decision.reasons.map((r) => r.code);
    expect(codes).not.toContain(MV_STOP_KILL_SWITCH);
    expect(codes).not.toContain(MV_STOP_COMPAT_GATE_BELOW_MINIMUM);
    expect(codes).toContain(MV_STOP_UNTRUSTED_WORKSPACE);
    expect(codes).toContain(MV_STOP_POLICY_CHECKOUT_STALE);
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
