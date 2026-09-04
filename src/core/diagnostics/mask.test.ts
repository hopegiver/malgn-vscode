import { describe, expect, it } from 'vitest';
import { MASK_MARKER, maskSensitive } from './mask.js';

// 테스트 값은 전부 `.repeat()`/템플릿 결합으로 만든다 — 소스 텍스트에 자격증명 형태
// 리터럴 하나가 그대로 등장하면 compat:check 검사 ⑨(민감값 스캔)가 그 자체를 위반으로
// 잡는다(secret-token-prefix). 이 방식은 이 저장소가 이미 쓰는 관례다
// (scripts/lib/sensitiveScan.v2.test.mjs 참고).
//
// 변수 이름도 PR-5 시크릿-키 이름 정규식(compat/sensitive-classes.json "secret-bearing-key")에
// 걸리는 단어를 피한다 — 그 이름을 지역 변수로 선언하고 아무 문자열 리터럴이라도 대입하면
// 값의 실제 엔트로피와 무관하게 "이름이 secret 형태 + 리터럴 대입" 그 자체로 검사 ⑨를
// 위반한다(이 슬라이스에서 신선 클론 재현 중 실제로 재현·수정한 사례, 반환문에 명시 — 이
// 문단 자체도 예시 코드 조각을 직접 쓰지 않는 이유가 이것이다: 자기참조 오탐을 피한다).

describe('maskSensitive — architecture.md §6.2 4개 정규식', () => {
  it('gh 토큰 접두사(gh[pousr]_...)를 마스킹한다', () => {
    const sample = `ghp_${'A'.repeat(36)}`;
    const line = `leaked value: ${sample} end`;
    const masked = maskSensitive(line);
    expect(masked).not.toContain(sample);
    expect(masked).toBe(`leaked value: ${MASK_MARKER} end`);
  });

  it('gh 토큰 5종 접두사(ghp/gho/ghu/ghs/ghr) 전부를 마스킹한다', () => {
    for (const prefix of ['ghp', 'gho', 'ghu', 'ghs', 'ghr']) {
      const sample = `${prefix}_${'B'.repeat(24)}`;
      expect(maskSensitive(sample)).toBe(MASK_MARKER);
    }
  });

  it('Bearer 값을 마스킹하되 접두어는 남긴다', () => {
    const sample = 'x'.repeat(40);
    const line = `Authorization: Bearer ${sample}`;
    const masked = maskSensitive(line);
    expect(masked).not.toContain(sample);
    expect(masked).toBe(`Authorization: Bearer ${MASK_MARKER}`);
  });

  it('Basic 값을 마스킹하되 접두어는 남긴다', () => {
    const sample = 'y'.repeat(40);
    const line = `Authorization: Basic ${sample}`;
    const masked = maskSensitive(line);
    expect(masked).not.toContain(sample);
    expect(masked).toBe(`Authorization: Basic ${MASK_MARKER}`);
  });

  it('접두사 없는 고엔트로피 값(32자 이상)을 마스킹한다', () => {
    const sample = 'z'.repeat(32);
    expect(maskSensitive(`value=${sample}`)).toBe(`value=${MASK_MARKER}`);
  });

  it('31자 이하 문자열은 마스킹하지 않는다 — 과잉 마스킹은 진단을 무용하게 만든다', () => {
    const short = 'a'.repeat(31);
    expect(maskSensitive(`id=${short}`)).toBe(`id=${short}`);
  });

  it('비민감 메타데이터(exitCode·timeoutKind 같은 짧은 값)는 그대로 남는다', () => {
    const text = JSON.stringify({ exitCode: 0, timedOut: false, timeoutKind: null, signature: 'verified' });
    expect(maskSensitive(text)).toBe(text);
  });

  it('여러 패턴이 한 텍스트에 섞여 있어도 전부 마스킹한다', () => {
    const ghLikeValue = `gho_${'C'.repeat(30)}`;
    const bearerLikeValue = 'd'.repeat(40);
    const line = `gh=${ghLikeValue} auth=Bearer ${bearerLikeValue}`;
    const masked = maskSensitive(line);
    expect(masked).not.toContain(ghLikeValue);
    expect(masked).not.toContain(bearerLikeValue);
  });
});
