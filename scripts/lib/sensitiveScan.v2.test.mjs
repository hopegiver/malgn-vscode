// security-plan.md §12.3 "수정 완료" 인정 조건 — 7축 회귀 테스트.
// 미검출 32건(§12.1 X-1~X-11) 중 C-1~C-6 각 부류가 최소 1건씩 실제로 검출되는지 고정한다.
//
// fixture는 전부 합성값이지만 **조각 결합 회피 기법을 쓰지 않는다**(§12.3 명시) — 검증하려는
// 코드 경로(원문 스캔)를 우회하기 때문이다. 대신 이 파일 자신을 `compat/sensitive-classes.json`
// `pathExemptions`에 등재해 검사 ⑨가 이 파일의 의도된 fixture를 위반으로 잡지 않게 한다
// (§12.3 "pathExemptions에 등재한 뒤 정면으로 넣으십시오").

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadClassesConfig, scanText } from './sensitiveScan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const config = loadClassesConfig(readFileSync(join(ROOT, 'compat', 'sensitive-classes.json'), 'utf8'));

function violates(filePath, text, classId) {
  return scanText(filePath, text, config).some((v) => v.classId === classId);
}

// --- 파일 형식(X-3) — scanScopes.default: "wholeText" ---
describe('파일 형식(X-3) — 코드 확장자만 quotedOnly, 그 밖은 전문 스캔', () => {
  const domain = 'malgnsoft-internal-fixture.systems'; // .systems는 recognizedTlds에 있다(X-4)

  it('md 산문(인용부호 밖)도 검출한다 — 이전엔 인용부호 프리필터가 전면 통과시켰다', () => {
    expect(violates('docs-like/note.md', `이 시스템은 ${domain} 에서 운영됩니다.`, 'network-authority-domain')).toBe(true);
  });

  it('md 표 셀도 검출한다', () => {
    expect(violates('docs-like/table.md', `| 항목 | 값 |\n|---|---|\n| host | ${domain} |\n`, 'network-authority-domain')).toBe(true);
  });

  it('md 링크(괄호 안)도 검출한다', () => {
    expect(violates('docs-like/link.md', `[문서](https://${domain}/path)`, 'network-authority-domain')).toBe(true);
  });

  it('md 백틱은 이전에도 잡혔고 지금도 잡힌다(회귀 없음)', () => {
    expect(violates('docs-like/code.md', `설정: \`${domain}\``, 'network-authority-domain')).toBe(true);
  });

  it('yaml 무인용 스칼라도 검출한다', () => {
    expect(violates('config-like/settings.yml', `host: ${domain}\n`, 'network-authority-domain')).toBe(true);
  });

  it('yaml 인용 스칼라도 검출한다', () => {
    expect(violates('config-like/settings.yml', `host: "${domain}"\n`, 'network-authority-domain')).toBe(true);
  });

  it('ts 문자열(인용부호 안)은 여전히 검출한다', () => {
    expect(violates('src-like/x.ts', `const host = "${domain}";`, 'network-authority-domain')).toBe(true);
  });

  it('ts 식별자 체인(인용부호 밖)은 오탐하지 않는다 — quotedOnly가 코드 파일에는 여전히 적용된다', () => {
    expect(violates('src-like/x.ts', 'const v = result.policy.otel.env.OTEL_LOG_USER_PROMPTS;', 'network-authority-domain')).toBe(false);
  });

  it('영문 아포스트로피가 있는 산문(X-3b)에서도 도메인 검출이 여전히 동작한다', () => {
    expect(violates('docs-like/prose.md', `The collector's endpoint is ${domain}, per the vendor's note.`, 'network-authority-domain')).toBe(true);
  });
});

// --- IPv6(X-5) ---
describe('IPv6(X-5) — reservedNamespaceAllowlist.ipv6Cidrs를 실제로 읽는다', () => {
  it('완전형을 검출한다', () => {
    expect(violates('fixture.ts', '"2606:4700:4700:0000:0000:0000:0000:1111"', 'ipv6-literal')).toBe(true);
  });

  it('압축형을 검출한다', () => {
    expect(violates('fixture.ts', '"2606:4700::1111"', 'ipv6-literal')).toBe(true);
  });

  it('IPv4-mapped를 검출한다', () => {
    expect(violates('fixture.ts', '"::ffff:198.18.5.5"', 'ipv6-literal')).toBe(true);
  });

  it('링크로컬(fe80)을 검출한다 — 면제하지 않는다', () => {
    expect(violates('fixture.ts', '"fe80::1"', 'ipv6-literal')).toBe(true);
  });

  it('ULA(fd00 계열, fc00::/7)를 검출한다 — 면제하지 않는다', () => {
    expect(violates('fixture.ts', '"fd12:3456:789a::1"', 'ipv6-literal')).toBe(true);
  });

  it('NAT64(64:ff9b::/96)를 검출한다', () => {
    expect(violates('fixture.ts', '"64:ff9b::198.18.5.5"', 'ipv6-literal')).toBe(true);
  });

  it('대괄호+포트 형태에서도 주소를 검출한다', () => {
    expect(violates('fixture.ts', '"[2001:4860:4860::8888]:8443"', 'ipv6-literal')).toBe(true);
  });

  it('대소문자 혼용도 검출한다', () => {
    expect(violates('fixture.ts', '"FD00::AbCd"', 'ipv6-literal')).toBe(true);
  });

  it('2001:db8::/32(예약)는 면제한다', () => {
    expect(violates('fixture.ts', '"2001:db8::1234"', 'ipv6-literal')).toBe(false);
  });

  it('::1(루프백)은 면제한다', () => {
    expect(violates('fixture.ts', '"::1"', 'ipv6-literal')).toBe(false);
  });
});

// --- PUB-X(X-1) — secret-token-prefix 7종 + secret-bearing-key ---
describe('PUB-X(X-1) — 발급자 형태 접두 7종을 검출한다(합성값, 실제 발급 토큰 아님)', () => {
  it.each([
    ['ghp_', `ghp_${'A'.repeat(36)}`],
    ['github_pat_', `github_pat_${'1'.repeat(22)}`],
    ['PEM 개인키', '-----BEGIN RSA PRIVATE KEY-----'],
    ['AKIA', `AKIA${'A'.repeat(16)}`],
    ['xox', `xoxb-${'1'.repeat(10)}`],
    ['sk-', `sk-${'A'.repeat(20)}`],
    ['JWT', `eyJ${'A'.repeat(10)}.${'B'.repeat(10)}.${'C'.repeat(10)}`],
  ])('%s 형태를 검출한다', (_label, value) => {
    expect(violates('fixture.ts', `const x = "${value}";`, 'secret-token-prefix')).toBe(true);
  });

  it('secret-bearing-key — password 등 비밀 키 이름 + 비-플레이스홀더 값을 검출한다', () => {
    const violations = scanText('fixture.json', JSON.stringify({ db: { password: 'hunter2xyz-not-real' } }), config);
    expect(violations.some((v) => v.classId === 'secret-bearing-key')).toBe(true);
  });

  it.each(['', '$site', 'REPLACE_ME', 'CHANGEME', 'example'])('플레이스홀더 값("%s")은 통과한다(오탐 0)', (placeholder) => {
    const violations = scanText('fixture.json', JSON.stringify({ db: { password: placeholder } }), config);
    expect(violations.some((v) => v.classId === 'secret-bearing-key')).toBe(false);
  });

  it('redactMatch — PUB-X 위반의 match에 원문 값이 남지 않는다(§12.3 redactMatch 회귀)', () => {
    const value = `ghp_${'Z'.repeat(36)}`;
    const violations = scanText('fixture.ts', `const x = "${value}";`, config);
    const hit = violations.find((v) => v.classId === 'secret-token-prefix');
    expect(hit).toBeDefined();
    expect(hit.match).not.toContain(value);
    expect(hit.match).toBe('[REDACTED]');
    expect(hit.redacted).toBe(true);
  });
});

// --- PUB-A(X-2) — 홈경로 3종 + 이메일 ---
describe('PUB-A(X-2) — 개발자 로컬 절대경로 + 이메일을 검출한다', () => {
  it('macOS 홈경로를 검출한다', () => {
    expect(violates('fixture.md', '경로: /Users/testuser/project/file.ts', 'home-directory-path')).toBe(true);
  });

  it('Linux 홈경로를 검출한다', () => {
    expect(violates('fixture.md', '경로: /home/testuser/project/file.ts', 'home-directory-path')).toBe(true);
  });

  it('Windows 홈경로를 검출한다', () => {
    expect(violates('fixture.md', String.raw`경로: C:\Users\testuser\project\file.ts`, 'home-directory-path')).toBe(true);
  });

  it('/home/runner/(GitHub Actions)는 면제한다 — 없으면 CI 로그에서 상시 오탐이 난다', () => {
    expect(violates('fixture.md', '경로: /home/runner/work/repo/repo/file.ts', 'home-directory-path')).toBe(false);
  });

  it('이메일 리터럴을 검출한다', () => {
    expect(violates('fixture.md', '문의: someone@fakecorp-internal-fixture.io', 'email-literal')).toBe(true);
  });

  it('@example.com 이메일은 면제한다', () => {
    expect(violates('fixture.md', '문의: someone@example.com', 'email-literal')).toBe(false);
  });

  it('noreply@ 이메일은 면제한다', () => {
    expect(violates('fixture.md', '발신: noreply@github.com', 'email-literal')).toBe(false);
  });
});

// --- 포트(X-6) — 호스트가 면제여도 포트는 별도 판정 ---
describe('포트(X-6) — <예약대역IP>:<비표준포트> 형태를 검출한다', () => {
  it('예약대역 IP + 비표준 포트는 검출된다(호스트 자체는 exemptCidrs로 면제여도)', () => {
    expect(violates('fixture.ts', '"203.0.113.10:9999"', 'nonstandard-authority-port')).toBe(true);
  });

  it('예약대역 IP + 공개 문서화 포트(4318, OTLP 기본값)는 면제한다', () => {
    expect(violates('fixture.ts', '"203.0.113.10:4318"', 'nonstandard-authority-port')).toBe(false);
  });

  it('템플릿 보간 잔여 형태(`${VAR}:포트`)도 검출한다 — X-6의 실제 잔존 형태', () => {
    expect(violates('fixture.ts', '`${HOST}:9999`', 'nonstandard-authority-port')).toBe(true);
  });

  it('file.ts:29류 소스 줄 참조는 오탐하지 않는다(도메인 후보의 TLD가 recognizedTlds 밖)', () => {
    expect(violates('fixture.md', '자세한 내용은 file.ts:29를 보십시오.', 'nonstandard-authority-port')).toBe(false);
  });
});

// --- TLD(X-4) — recognizedTlds가 default-deny를 뒤집던 문제의 정정 ---
describe('TLD(X-4) — 이전에 미등재였던 TLD를 검출한다 + api.localhost 오탐 0(X-9 회귀)', () => {
  it.each(['internal-fixture.systems', 'metrics-fixture.cc', 'admin-fixture.local'])('%s를 검출한다', (host) => {
    expect(violates('fixture.md', `내부 대시보드: ${host}`, 'network-authority-domain')).toBe(true);
  });

  it('api.localhost는 오탐하지 않는다(X-9 회귀 — reservedNamespaceAllowlist에 localhost 추가)', () => {
    expect(violates('fixture.md', '개발 서버: api.localhost', 'network-authority-domain')).toBe(false);
  });
});

// --- 구조(C-6) — site-hole-discipline: 어휘가 아니라 키 경로로 판정 ---
describe('구조(C-6) — site-hole-discipline은 키 경로로 판정한다(어휘로는 판정 불가능했다)', () => {
  it('$site 구멍은 통과한다', () => {
    const text = JSON.stringify({ allowedAuthorities: { $site: 'authorities' } });
    expect(violates('compat/fixture-structural.json', text, 'site-hole-discipline')).toBe(false);
  });

  it('publicAllowlist에 등재된 값(example-service)은 통과한다', () => {
    const text = JSON.stringify({ keychainItems: ['example-service'] });
    expect(violates('compat/fixture-structural.json', text, 'site-hole-discipline')).toBe(false);
  });

  it('임의 문자열(어휘적으로 claude-otel과 구별 불가능한 값)은 검출한다', () => {
    const text = JSON.stringify({ keychainItems: ['some-unlisted-item-name'] });
    expect(violates('compat/fixture-structural.json', text, 'site-hole-discipline')).toBe(true);
  });

  it('예약 네임스페이스 도메인(authorities 값)은 통과한다', () => {
    const text = JSON.stringify({ authorities: { otel: ['collector.example.com'] } });
    expect(violates('compat/fixture-structural.json', text, 'site-hole-discipline')).toBe(false);
  });

  it('compat/*.json 밖의 파일에는 적용되지 않는다(filesGlob 범위)', () => {
    const text = JSON.stringify({ keychainItems: ['some-unlisted-item-name'] });
    expect(violates('src/fixture-not-compat.json', text, 'site-hole-discipline')).toBe(false);
  });
});
