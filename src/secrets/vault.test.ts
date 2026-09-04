import { describe, expect, it } from 'vitest';
import { MASK_MARKER } from '../core/diagnostics/mask.js';
import { Secret, wrapSecret } from './vault.js';

describe('Secret — architecture.md §6.2 "read 시점에 Secret<string>으로 감싸 toString()이 ***를 반환"', () => {
  it('toString()이 원문 대신 MASK_MARKER를 반환한다', () => {
    const raw = `sk-${'A'.repeat(40)}`;
    const secret = wrapSecret(raw);
    expect(secret.toString()).toBe(MASK_MARKER);
    expect(secret.toString()).not.toContain(raw);
  });

  it('문자열 보간(템플릿 리터럴)이 원문을 노출하지 않는다', () => {
    const raw = `token-${'B'.repeat(40)}`;
    const secret = wrapSecret(raw);
    expect(`value=${secret}`).toBe(`value=${MASK_MARKER}`);
  });

  it('JSON.stringify가 toJSON()을 타서 원문을 노출하지 않는다(진단 리포트 직렬화 경로)', () => {
    const raw = `pw-${'C'.repeat(40)}`;
    const secret = wrapSecret(raw);
    const serialized = JSON.stringify({ apiKey: secret, other: 'x' });
    expect(serialized).toBe(`{"apiKey":"${MASK_MARKER}","other":"x"}`);
    expect(serialized).not.toContain(raw);
  });

  it('Object.keys/구조 순회로도 원문 필드에 도달할 수 없다 — 진짜 private 필드다', () => {
    const raw = `zz-${'D'.repeat(40)}`;
    const secret = wrapSecret(raw);
    // ECMAScript 클래스 private 필드(#value)는 Object.keys/for-in/JSON 구조 순회에
    // 애초에 열거되지 않는다 — 이 테스트는 그 성질이 실제로 성립하는지 확인한다.
    expect(Object.keys(secret)).toHaveLength(0);
    expect(JSON.stringify(Object.getOwnPropertyNames(secret))).not.toContain(raw);
  });

  it('reveal()만 원문을 반환한다 — 유일한 의도된 통로', () => {
    const raw = `reveal-${'E'.repeat(20)}`;
    const secret = wrapSecret(raw);
    expect(secret.reveal()).toBe(raw);
  });

  it('Secret 인스턴스임을 타입으로 확인할 수 있다', () => {
    const secret = wrapSecret('x');
    expect(secret).toBeInstanceOf(Secret);
  });
});
