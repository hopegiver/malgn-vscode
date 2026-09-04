import { describe, expect, it } from 'vitest';
import { evaluateStop } from '../reconciler/stopGate.js';
import type { EffectiveKillSwitch } from '../policy/types.js';
import { MASK_MARKER } from './mask.js';
import { buildDiagnosticReport, renderDiagnosticReportForClipboard, toLastInstallAttempt } from './report.js';
import type { InstallJournalEntry } from '../journal/types.js';

const NO_KILL_SWITCH: EffectiveKillSwitch = {
  minExtensionVersion: null,
  maxExtensionVersion: null,
  disableProviders: [],
  message: null,
  upgradeHint: null,
};

function okVerdict() {
  return evaluateStop('provider.apply', 'agent', {
    killSwitch: NO_KILL_SWITCH,
    currentExtensionVersion: '0.1.0',
    compatGateBelowMinimum: false,
    workspaceTrusted: true,
    policyCheckoutStale: false,
    hrs4ReconsentRequired: false,
  });
}

const SAMPLE_INSTALL_ENTRY: InstallJournalEntry = {
  kind: 'install',
  ts: '2026-09-04T00:00:00.000Z',
  tool: 'claude',
  strategy: 'PkgManagerStrategy',
  argv: ['/opt/homebrew/bin/brew', 'install', '--cask', 'claude-code'],
  managerPath: '/opt/homebrew/bin/brew',
  managerRealPath: '/opt/homebrew/bin/brew',
  managerOwner: 'root:admin',
  envSet: { HOMEBREW_NO_INSTALL_UPGRADE: '1' },
  targetVersion: '2.1.236',
  compatFloor: '2.1.237',
  exitCode: null,
  timedOut: true,
  timeoutKind: 'idle',
  resolvedPath: '/opt/homebrew/bin/claude',
  installOrigin: 'brew',
  signature: 'unchecked',
};

describe('toLastInstallAttempt — policy-contract.md §4 리포트 표 부분집합 투영', () => {
  it('저널에만 있는 필드(argv·managerRealPath·managerOwner·envSet·installOrigin)를 리포트에 옮기지 않는다', () => {
    const attempt = toLastInstallAttempt(SAMPLE_INSTALL_ENTRY);
    expect(attempt).toEqual({
      tool: 'claude',
      strategy: 'PkgManagerStrategy',
      exitCode: null,
      timedOut: true,
      timeoutKind: 'idle',
      managerPath: '/opt/homebrew/bin/brew',
      resolvedPath: '/opt/homebrew/bin/claude',
      targetVersion: '2.1.236',
      compatFloor: '2.1.237',
      signature: 'unchecked',
      ts: '2026-09-04T00:00:00.000Z',
    });
    expect(attempt).not.toHaveProperty('argv');
    expect(attempt).not.toHaveProperty('managerRealPath');
    expect(attempt).not.toHaveProperty('managerOwner');
    expect(attempt).not.toHaveProperty('envSet');
    expect(attempt).not.toHaveProperty('installOrigin');
  });

  it('MV_INSTALL_TIMEOUT 시나리오(timedOut:true)의 필드가 그대로 보존된다 — 완료판정 4', () => {
    const attempt = toLastInstallAttempt(SAMPLE_INSTALL_ENTRY);
    expect(attempt.timedOut).toBe(true);
    expect(attempt.timeoutKind).toBe('idle');
  });
});

describe('buildDiagnosticReport — policy-contract.md §4 "항상" + "install 관여 시" 필드', () => {
  it('install이 관여하지 않으면 lastInstallAttempt 필드 자체가 없다', () => {
    const report = buildDiagnosticReport({
      extensionVersion: '0.1.0',
      claudeVersion: '2.1.252',
      installOrigin: 'native',
      agentVersion: '1.8.27',
      policySource: '마켓플레이스',
      verdict: okVerdict(),
    });
    expect(report).not.toHaveProperty('lastInstallAttempt');
  });

  it('install이 관여했으면 lastInstallAttempt가 실린다', () => {
    const report = buildDiagnosticReport({
      extensionVersion: '0.1.0',
      claudeVersion: null,
      installOrigin: 'brew',
      agentVersion: null,
      policySource: '설치본',
      verdict: okVerdict(),
      lastInstallAttempt: toLastInstallAttempt(SAMPLE_INSTALL_ENTRY),
    });
    expect(report.lastInstallAttempt).toMatchObject({ timedOut: true, timeoutKind: 'idle' });
  });

  it('verdict는 StopReason.message를 새로 짓지 않고 그대로 재사용한다(N-2 원문 문구, W4 이어받기)', () => {
    const stopped = evaluateStop('provider.apply', 'agent', {
      killSwitch: NO_KILL_SWITCH,
      currentExtensionVersion: '0.1.0',
      compatGateBelowMinimum: false,
      workspaceTrusted: true,
      policyCheckoutStale: true,
      hrs4ReconsentRequired: false,
    });
    const report = buildDiagnosticReport({
      extensionVersion: '0.1.0',
      claudeVersion: null,
      installOrigin: null,
      agentVersion: null,
      policySource: null,
      verdict: stopped,
    });
    expect(report.verdict.stopped).toBe(true);
    // architecture.md §3.7.3 표의 N-2 표시 문구 원문과 정확히 일치해야 한다.
    expect(report.verdict.reasons[0]?.message).toBe(
      '오래된 정책으로 PC를 바꾸지 않습니다 — 진단과 되돌리기는 계속 쓸 수 있습니다'
    );
  });
});

describe('renderDiagnosticReportForClipboard — §6.2 "같은 필터를 통과한 뒤에만 클립보드로 나간다"', () => {
  it('정상 리포트는 사람이 읽을 수 있는 JSON 문자열을 반환한다', () => {
    const report = buildDiagnosticReport({
      extensionVersion: '0.1.0',
      claudeVersion: '2.1.252',
      installOrigin: 'native',
      agentVersion: '1.8.27',
      policySource: '마켓플레이스',
      verdict: okVerdict(),
    });
    const rendered = renderDiagnosticReportForClipboard(report);
    expect(JSON.parse(rendered)).toMatchObject({ extensionVersion: '0.1.0' });
  });

  it('[방어심층] policySource 같은 문자열 필드에 자격증명 형태 값이 섞여 들어와도 클립보드 출력엔 마스킹된다', () => {
    const leaked = `Basic ${'W'.repeat(40)}`;
    const report = buildDiagnosticReport({
      extensionVersion: '0.1.0',
      claudeVersion: null,
      installOrigin: null,
      agentVersion: null,
      // 설계상 policySource는 "마켓플레이스"/"설치본"/"내장 기본값" 같은 고정 문구만
      // 들어와야 하지만, 이 테스트는 그 불변량이 깨지는 미래 버그를 가정한
      // 방어심층 검증이다.
      policySource: leaked,
      verdict: okVerdict(),
    });
    const rendered = renderDiagnosticReportForClipboard(report);
    expect(rendered).not.toContain(leaked);
    expect(rendered).toContain(MASK_MARKER);
  });
});
