// src/errorMessage.ts 마스킹 단위 테스트 — 허브 기기 토큰(mhd_)과 OTLP 수집키(mhk_)가 둘 다 가려지는지.
//
// 러너: Node 내장 `node:test` + 타입 스트리핑(`--experimental-strip-types`)이라 새 의존성이 없다.
// 실행: `pnpm test:ts` (CI frontend-check 잡도 같은 명령을 돌린다).
// errorMessage.ts는 import가 없는 순수 모듈이라 번들러 없이 .ts를 그대로 불러온다.
//
// 가짜 값은 repeat()/조립으로 만든다 — 리터럴로 쓰면 pre-push 시크릿 스캔(mhk_/mhd_ + 43자)에 걸린다.
import test from 'node:test';
import assert from 'node:assert/strict';

import {
  errorMessage,
  errorMessagePersistent,
  maskLogText,
  maskSecrets,
  sanitizeBackendText,
} from '../../src/errorMessage.ts';

const BODY_LEN = 43;
const fakeDeviceToken = (body) => `mhd_${body.repeat(Math.ceil(BODY_LEN / body.length)).slice(0, BODY_LEN)}`;
const fakeIngestKey = (body) => `mhk_${body.repeat(Math.ceil(BODY_LEN / body.length)).slice(0, BODY_LEN)}`;

// 본문 모양 변형 — 소문자만인 키는 "대소문자·숫자 혼합 40자+" 일반 불투명 토큰 규칙으로는 가려지지
// 않으므로, 접두어 전용 규칙이 있어야만 통과한다(가장 중요한 케이스).
const BODIES = {
  lowercaseOnly: 'a',
  mixedCase: 'aB3',
  withHyphenUnderscore: 'a_-Z09',
  digitsOnly: '7',
};

for (const [prefixName, make] of [
  ['mhd_', fakeDeviceToken],
  ['mhk_', fakeIngestKey],
]) {
  for (const [shape, body] of Object.entries(BODIES)) {
    const token = make(body);
    const tokenBody = token.slice(4);

    test(`maskSecrets masks ${prefixName} token (${shape}) and keeps no body`, () => {
      const out = maskSecrets(`oops ${token} tail`);
      assert.ok(!out.includes(token), out);
      assert.ok(!out.includes(tokenBody.slice(0, 12)), `본문 앞부분 잔존: ${out}`);
      assert.ok(out.includes(`${prefixName}***`), out);
      assert.ok(out.includes('oops') && out.includes('tail'), '주변 문맥은 유지');
    });
  }

  test(`maskSecrets masks ${prefixName} token inside JSON, URL query and quoted text`, () => {
    const token = make('aB3');
    const body = token.slice(4, 20);
    for (const text of [
      `{"value":"${token}"}`,
      `https://example.test/cb?x=${token}&y=1`,
      `'${token}'`,
      `line1\n${token}\nline3`,
    ]) {
      const out = maskSecrets(text);
      assert.ok(!out.includes(body), `${text} -> ${out}`);
    }
  });
}

test('maskSecrets masks the mhk_ key in an Authorization header and in the settings env line', () => {
  const key = fakeIngestKey('a');
  const body = key.slice(4, 20);
  for (const text of [
    `Authorization=Bearer ${key}`,
    `Authorization: Bearer ${key}`,
    `OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer ${key}`,
    `"OTEL_EXPORTER_OTLP_HEADERS": "Authorization=Bearer ${key}"`,
  ]) {
    const out = maskSecrets(text);
    assert.ok(!out.includes(body), `${text} -> ${out}`);
  }
});

test('maskSecrets masks both token kinds when they appear in the same text', () => {
  const device = fakeDeviceToken('a');
  const key = fakeIngestKey('b');
  const out = maskSecrets(`device=${device} ingest=${key}`);
  assert.ok(!out.includes(device.slice(4, 20)), out);
  assert.ok(!out.includes(key.slice(4, 20)), out);
  assert.ok(out.includes('mhd_***') && out.includes('mhk_***'), out);
});

test('maskSecrets keeps the hub-style masked display value of an ingest key (mhk_ + 4 chars)', () => {
  // 허브·앱이 화면에 보여주는 마스킹 표시값은 비밀이 아니다 — 지나치게 가리면 상태 화면이 깨진다.
  const shown = 'mhk_AbCd…';
  assert.equal(maskSecrets(`수집키 ${shown} 적용됨`), `수집키 ${shown} 적용됨`);
});

test('maskSecrets does not mask look-alike words that are not tokens', () => {
  assert.equal(maskSecrets('mhk_ 접두어와 mhd_ 접두어는 서로 다른 자격이다'), 'mhk_ 접두어와 mhd_ 접두어는 서로 다른 자격이다');
});

test('maskLogText masks mhd_ and mhk_ tokens, also when URLs are kept', () => {
  const device = fakeDeviceToken('a');
  const key = fakeIngestKey('a');
  for (const keepUrls of [false, true]) {
    const out = maskLogText(`header ${device}\nother ${key}\nurl https://example.test/?k=${key}`, keepUrls);
    assert.ok(!out.includes(device.slice(4, 20)), `keepUrls=${keepUrls}: ${out}`);
    assert.ok(!out.includes(key.slice(4, 20)), `keepUrls=${keepUrls}: ${out}`);
  }
});

test('sanitizeBackendText and errorMessage mask both token kinds from backend errors', () => {
  const device = fakeDeviceToken('a');
  const key = fakeIngestKey('a');
  const raw = `허브 응답 오류 ${device} / ${key}`;
  for (const out of [
    sanitizeBackendText(raw),
    errorMessage(raw),
    errorMessage(new Error(raw)),
    errorMessage({ message: raw }),
    errorMessagePersistent(raw),
  ]) {
    assert.ok(!out.includes(device.slice(4, 20)), out);
    assert.ok(!out.includes(key.slice(4, 20)), out);
    assert.ok(out.includes('허브 응답 오류'), '원인 문맥은 유지');
  }
});
