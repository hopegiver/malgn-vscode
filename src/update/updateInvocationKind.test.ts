import { describe, expect, it } from 'vitest';
import { UnknownUpdateTriggerKindError, assertKnownUpdateTriggerKind } from './updateTrigger.js';
import type { UpdateTrigger } from './updateTrigger.js';

describe('UpdateTrigger (AT-U4 — 트리거 입력 전수 열거)', () => {
  it('허용된 세 종류는 리터럴로 구성 가능하다', () => {
    const timer: UpdateTrigger = { kind: 'timer' };
    const userCommand: UpdateTrigger = { kind: 'user-command' };
    const verifiedManifest: UpdateTrigger = { kind: 'verified-manifest' };
    expect([timer, userCommand, verifiedManifest].map((t) => t.kind)).toEqual(['timer', 'user-command', 'verified-manifest']);
  });

  it('assertKnownUpdateTriggerKind는 허용된 세 값을 통과시킨다', () => {
    expect(() => assertKnownUpdateTriggerKind('timer')).not.toThrow();
    expect(() => assertKnownUpdateTriggerKind('user-command')).not.toThrow();
    expect(() => assertKnownUpdateTriggerKind('verified-manifest')).not.toThrow();
  });

  it('네 번째 종류를 넣으면 런타임에서 거부한다', () => {
    expect(() => assertKnownUpdateTriggerKind('remote-push')).toThrow(UnknownUpdateTriggerKindError);
  });
});
