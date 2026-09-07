import { describe, expect, it } from 'vitest';
import {
  REMOTE_SCOPE_BANNER_TEXT,
  buildRemoteEnvWarningMessage,
  detectRemoteEnvSignals,
  isAnyRemoteEnvDetected,
} from './remoteEnvDetection.js';

describe('detectRemoteEnvSignals', () => {
  it('아무 신호도 없으면 둘 다 false다', () => {
    expect(detectRemoteEnvSignals({})).toEqual({ ssh: false, wsl: false });
  });

  it('SSH_CONNECTION이 있으면 ssh=true다', () => {
    // RFC 5737 TEST-NET-1/TEST-NET-2 예약 대역(문서·예시 전용) — 실제 인프라 IP가 아니다.
    expect(detectRemoteEnvSignals({ SSH_CONNECTION: '192.0.2.10 1 198.51.100.10 22' }).ssh).toBe(true);
  });

  it('SSH_TTY만 있어도 ssh=true다', () => {
    expect(detectRemoteEnvSignals({ SSH_TTY: '/dev/ttys001' }).ssh).toBe(true);
  });

  it('WSL_DISTRO_NAME이 있으면 wsl=true다', () => {
    expect(detectRemoteEnvSignals({ WSL_DISTRO_NAME: 'Ubuntu' }).wsl).toBe(true);
  });

  it('WSL_INTEROP만 있어도 wsl=true다', () => {
    expect(detectRemoteEnvSignals({ WSL_INTEROP: '/run/WSL/1_interop' }).wsl).toBe(true);
  });
});

describe('isAnyRemoteEnvDetected / buildRemoteEnvWarningMessage', () => {
  it('신호가 전혀 없으면 경고 메시지가 null이다', () => {
    expect(isAnyRemoteEnvDetected({ ssh: false, wsl: false })).toBe(false);
    expect(buildRemoteEnvWarningMessage({ ssh: false, wsl: false })).toBeNull();
  });

  it('ssh 신호가 있으면 경고 메시지에 SSH가 언급되고 배너 문구를 포함한다', () => {
    const msg = buildRemoteEnvWarningMessage({ ssh: true, wsl: false });
    expect(msg).toContain('SSH');
    expect(msg).toContain(REMOTE_SCOPE_BANNER_TEXT);
  });

  it('wsl 신호가 있으면 경고 메시지에 WSL이 언급된다', () => {
    const msg = buildRemoteEnvWarningMessage({ ssh: false, wsl: true });
    expect(msg).toContain('WSL');
  });

  it('둘 다 있으면 둘 다 언급된다', () => {
    const msg = buildRemoteEnvWarningMessage({ ssh: true, wsl: true });
    expect(msg).toContain('SSH');
    expect(msg).toContain('WSL');
  });
});
