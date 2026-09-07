import { describe, expect, it } from 'vitest';
import { HEARTBEAT_STALE_THRESHOLD_MS, isHeartbeatStale } from './watchdogCheck.js';

describe('isHeartbeatStale — §2.7 임계(3×주기) 초과 판정', () => {
  const now = new Date('2026-09-07T12:00:00.000Z');

  it('하트비트가 없으면(null) 신선도 미상 — stale로 간주(fail-closed)', () => {
    expect(isHeartbeatStale(null, now)).toBe(true);
  });

  it('임계 미만이면 신선하다', () => {
    const recent = new Date(now.getTime() - (HEARTBEAT_STALE_THRESHOLD_MS - 1000));
    expect(isHeartbeatStale(recent, now)).toBe(false);
  });

  it('임계를 정확히 넘기면 stale이다(경계 포함)', () => {
    const exact = new Date(now.getTime() - HEARTBEAT_STALE_THRESHOLD_MS);
    expect(isHeartbeatStale(exact, now)).toBe(true);
  });

  it('임계를 훌쩍 넘기면 stale이다', () => {
    const old = new Date(now.getTime() - HEARTBEAT_STALE_THRESHOLD_MS * 10);
    expect(isHeartbeatStale(old, now)).toBe(true);
  });

  it('커스텀 임계값을 주입할 수 있다(테스트 전용 오버라이드)', () => {
    const tenMinAgo = new Date(now.getTime() - 10 * 60 * 1000);
    expect(isHeartbeatStale(tenMinAgo, now, 5 * 60 * 1000)).toBe(true);
    expect(isHeartbeatStale(tenMinAgo, now, 20 * 60 * 1000)).toBe(false);
  });
});
