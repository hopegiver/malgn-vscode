// scripts/lib/sensitiveScan.mjs 검증 — 완료판정 #5 "검사 ⑨가 실제 민감값을 주입하면
// 검출하는지" 근거. 실제 sensitive-classes.json(공개면, 패턴만)을 그대로 쓴다.
//
// 아래 fixture는 합성(가짜) 값만 쓴다 — 이번 사고의 실제 노출 값(수집기 IP·배포
// 호스트 등)을 여기 리터럴로 넣으면 이 파일 자체가 추적 트리에 그 값을 다시 심는
// 꼴이 되어 검사 ⑨가 이 테스트 파일을 위반으로 잡는다(실제로 한 번 재현·확인했다).
// 실제 노출 값으로의 검출 확인은 완료판정 #5가 요구하는 대로 "주입 후 원복"하는
// 1회성 수동 검증으로 별도 수행했다(추적 파일에 영구히 남기지 않는다).

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadClassesConfig, scanText } from './sensitiveScan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const config = loadClassesConfig(readFileSync(join(ROOT, 'compat', 'sensitive-classes.json'), 'utf8'));

// 아래 세 값은 조각을 코드에서 이어 붙여 만든다(문자열 리터럴 하나로 온전한 형태가
// 소스에 등장하지 않는다) — 그렇지 않으면 이 테스트 파일 자체가 추적 파일이라 검사
// ⑨(전체 스캔)가 "합성이지만 비면제인 값"인 이 fixture를 위반으로 잡는다(실측 확인).
// scanText는 런타임에 완성된 문자열을 받으므로 탐지 로직 자체는 그대로 검증된다.
const SYNTHETIC_NON_EXEMPT_IP = ['198', '18', '5', '5'].join('.');
const SYNTHETIC_NON_EXEMPT_DOMAIN = 'fakecorp-internal' + '.io';

describe('scanText — 민감값 모양(합성 값)을 주입하면 검출한다(완료판정 #5의 메커니즘 증명)', () => {
  it('예약 대역 밖 IPv4 리터럴(합성 값)을 검출한다', () => {
    const violations = scanText('fake.ts', `const x = "${SYNTHETIC_NON_EXEMPT_IP}:18443";`, config);
    expect(violations.some((v) => v.classId === 'ipv4-literal' && v.match === SYNTHETIC_NON_EXEMPT_IP)).toBe(true);
  });

  it('publicAllowlist·예약 네임스페이스 밖 도메인(합성 값)을 검출한다', () => {
    const violations = scanText('fake.ts', `const y = "https://download.${SYNTHETIC_NON_EXEMPT_DOMAIN}/x.vsix";`, config);
    expect(violations.some((v) => v.classId === 'network-authority-domain' && v.match === `download.${SYNTHETIC_NON_EXEMPT_DOMAIN}`)).toBe(true);
  });

  it('와일드카드 접미사의 접미사 부분도 검출한다', () => {
    const violations = scanText('fake.ts', `const z = "*.${SYNTHETIC_NON_EXEMPT_DOMAIN}";`, config);
    expect(violations.some((v) => v.classId === 'network-authority-domain' && v.match === SYNTHETIC_NON_EXEMPT_DOMAIN)).toBe(true);
  });
});

describe('scanText — 예약 네임스페이스는 면제된다(오탐 없음)', () => {
  it('RFC 2606 example.com은 검출하지 않는다', () => {
    const violations = scanText('fake.ts', 'const x = "https://download.example.com/x.vsix";', config);
    expect(violations).toEqual([]);
  });

  it('RFC 5737 TEST-NET-3(203.0.113.0/24)는 검출하지 않는다', () => {
    const violations = scanText('fake.ts', 'const x = "203.0.113.10:4318";', config);
    expect(violations).toEqual([]);
  });

  it('127.0.0.1(루프백)은 검출하지 않는다', () => {
    const violations = scanText('fake.ts', 'const x = "127.0.0.1:3000";', config);
    expect(violations).toEqual([]);
  });
});

describe('scanText — 코드 식별자 체인은 오탐하지 않는다(인용부호 밖)', () => {
  it('result.policy.otel.env 같은 프로퍼티 체인은 도메인으로 오인하지 않는다', () => {
    const violations = scanText('fake.ts', 'const v = result.policy.otel.env.OTEL_LOG_USER_PROMPTS;', config);
    expect(violations).toEqual([]);
  });

  it('semver 문자열(1.8.24)은 IPv4로도 도메인으로도 오인하지 않는다', () => {
    const violations = scanText('fake.ts', "const r = '>=1.8.24 <2.0.0';", config);
    expect(violations).toEqual([]);
  });
});
