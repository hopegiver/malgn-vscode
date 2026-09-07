// NT-R21(docs/release-gates.md §7.6.6) — "검사 ⑨의 대상 정의를 출하되는 모든 매체로
// 확장" 메커니즘 증명. 완료판정 #4 "실제 민감값이 든 바이너리를 검출하는지 확인"의
// 상시 회귀 버전이다.
//
// fixture는 조각을 런타임에 이어 붙여 만든다(sensitiveScan.test.mjs와 같은 이유 —
// 이 테스트 파일 자신이 추적 트리에 있어 검사 ⑨ 전체 스캔 대상이다. 완성된 리터럴을
// 그대로 쓰면 이 파일 자체가 위반으로 잡힌다).

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { extractPrintableStrings, loadClassesConfig, looksBinary, scanShippedMedium } from './sensitiveScan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const config = loadClassesConfig(readFileSync(join(ROOT, 'compat', 'sensitive-classes.json'), 'utf8'));

const SYNTHETIC_NON_EXEMPT_IP = ['198', '18', '5', '5'].join('.');
const SYNTHETIC_NON_EXEMPT_DOMAIN = 'fakecorp-internal' + '.io';
const SYNTHETIC_TOKEN = 'gh' + 'p_' + 'A'.repeat(40); // secret-token-prefix 형태(합성값)

function bytes(str) {
  return Buffer.from(str, 'latin1');
}

describe('looksBinary', () => {
  it('NUL 바이트가 있으면 바이너리로 판정한다', () => {
    expect(looksBinary(Buffer.from([0x41, 0x00, 0x42]))).toBe(true);
  });

  it('유효한 UTF-8 텍스트는 바이너리가 아니다', () => {
    expect(looksBinary(Buffer.from('const x = 1;\n', 'utf8'))).toBe(false);
  });
});

describe('extractPrintableStrings', () => {
  it('연속된 출력 가능 문자 4자 이상만 뽑는다', () => {
    const buf = Buffer.concat([Buffer.from([0x00, 0x01]), Buffer.from('abcd', 'ascii'), Buffer.from([0x02]), Buffer.from('ef', 'ascii')]);
    expect(extractPrintableStrings(buf)).toEqual(['abcd']); // 'ef'는 2자라 기본 minLength(4) 미만
  });

  it('minLength를 낮추면 짧은 문자열도 뽑는다', () => {
    const buf = Buffer.concat([Buffer.from([0x00]), Buffer.from('ef', 'ascii'), Buffer.from([0x00])]);
    expect(extractPrintableStrings(buf, 2)).toEqual(['ef']);
  });
});

describe('scanShippedMedium — 합성 "바이너리"(NUL 패딩 + 민감값 모양) 검출 (완료판정 #4 상시 회귀)', () => {
  it('바이너리 안에 박힌 예약대역 밖 IPv4를 strings 추출 경로로 검출한다', () => {
    const buf = Buffer.concat([bytes('\x00\x00\x00'), bytes(`host=${SYNTHETIC_NON_EXEMPT_IP}\x00\x00`), bytes('\x00padding\x00')]);
    expect(looksBinary(buf)).toBe(true);
    const { violations, binary } = scanShippedMedium('dist/fake.bin', buf, config);
    expect(binary).toBe(true);
    expect(violations.some((v) => v.classId === 'ipv4-literal')).toBe(true);
  });

  it('바이너리 안에 박힌 비허용 도메인을 검출한다', () => {
    const buf = Buffer.concat([bytes('\x00\x00'), bytes(`https://download.${SYNTHETIC_NON_EXEMPT_DOMAIN}/x`), bytes('\x00\x00')]);
    const { violations } = scanShippedMedium('dist/fake.bin', buf, config);
    expect(violations.some((v) => v.classId === 'network-authority-domain')).toBe(true);
  });

  it('바이너리 안에 박힌 시크릿 토큰 접두 형태를 검출하고 마스킹한다', () => {
    const buf = Buffer.concat([bytes('\x00\x00'), bytes(SYNTHETIC_TOKEN), bytes('\x00\x00')]);
    const { violations } = scanShippedMedium('dist/fake.bin', buf, config);
    const hit = violations.find((v) => v.classId === 'secret-token-prefix');
    expect(hit).toBeDefined();
    expect(hit.match).toBe('[REDACTED]'); // redactMatch:true — 원문을 절대 실어 나르지 않는다
  });

  it('PUB-C(publicAllowlist) 값은 바이너리 안에서도 예외로 남는다(면제 유지)', () => {
    const buf = Buffer.concat([bytes('\x00\x00host='), bytes('example-service'), bytes('\x00\x00')]);
    const { violations } = scanShippedMedium('dist/fake.bin', buf, config);
    expect(violations.some((v) => v.match === 'example-service')).toBe(false);
  });

  it('텍스트로 디코딩되는 매체(esbuild 번들 같은)는 strings 추출 없이 전문 스캔한다', () => {
    const text = `const authority = "download.${SYNTHETIC_NON_EXEMPT_DOMAIN}";\n`;
    const { violations, binary } = scanShippedMedium('dist/extension.cjs', Buffer.from(text, 'utf8'), config);
    expect(binary).toBe(false);
    expect(violations.some((v) => v.classId === 'network-authority-domain')).toBe(true);
  });

  it('민감값이 전혀 없는 매체는 위반이 없다(오탐 없음)', () => {
    const buf = Buffer.concat([bytes('\x00\x00'), bytes('hello world this is fine'), bytes('\x00\x00')]);
    const { violations } = scanShippedMedium('dist/fake.bin', buf, config);
    expect(violations).toEqual([]);
  });
});
