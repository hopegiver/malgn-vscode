// src/onboarding.ts OTEL 연동 참고 항목 단위 테스트 — 상태별 판정과 "미구현 기능을 주장하지 않는다"는 불변식.
// 러너: node:test + 타입 스트리핑(새 의존성 없음). onboarding.ts는 type import뿐이라 번들러 없이 불러온다.
import test from 'node:test';
import assert from 'node:assert/strict';

import { computeOnboarding } from '../../src/onboarding.ts';

const tool = (id, installed, required = true) => ({ id, name: id, installed, version: installed ? '1.0.0' : null, path: installed ? `/bin/${id}` : null, required });
const base = {
  devTools: { items: [tool('node', true), tool('git', true), tool('claude', true)], loaded: true, loading: false, error: null },
  claudeAuth: { status: { loggedIn: false }, loaded: true, loading: false, error: null },
};
const OFF = { issueSupported: false, receiptCheckSupported: false };
const link = (status, extra = {}) => ({ ingestLink: { status, loaded: true, loading: false, error: null, ...extra } });
const otel = (model) => model.steps.find((s) => s.kind === 'otel-link');
const BANNED = ['자동 연동', '연동 완료', '연동되었습니다', '수신이 확인', '수신 확인됨'];

test('ingestLink 입력이 없으면 항목이 없고 기존 3단계 판정은 그대로다', () => {
  const m = computeOnboarding(base);
  assert.equal(m.steps.length, 3);
  assert.equal(m.coreTotal, 3);
  assert.equal(otel(m), undefined);
});

test('상태별로 서로 다른 step.state가 나온다', () => {
  const cases = [
    [{ state: 'unlinked', masked: null, failure: null, ...OFF }, 'todo'],
    [{ state: 'issuing', masked: null, failure: null, ...OFF }, 'loading'],
    [{ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF }, 'partial'],
    [{ state: 'confirmed', masked: 'mhk_AbCd…', failure: null, ...OFF }, 'done'],
    [{ state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: '고정 문구' }, ...OFF }, 'failed'],
  ];
  for (const [status, expected] of cases) assert.equal(otel(computeOnboarding({ ...base, ...link(status) })).state, expected, status.state);
});

test('도달 가능한 상태(unlinked/applied/failed)는 자동 연동·연동 완료·수신 확인을 주장하지 않는다', () => {
  for (const status of [
    { state: 'unlinked', masked: null, failure: null, ...OFF },
    { state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF },
    { state: 'failed', masked: null, failure: { cause: 'notSupported', message: '허브 자동 발급과 수신 확인은 아직 지원되지 않습니다.' }, ...OFF },
  ]) {
    const detail = otel(computeOnboarding({ ...base, ...link(status) })).detail;
    for (const b of BANNED) assert.ok(!detail.includes(b), `${status.state}: "${b}" in ${detail}`);
  }
  const applied = otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF }) })).detail;
  assert.match(applied, /수신은 아직 확인하지 않았습니다/);
});

test('unlinked 안내는 수동 붙여넣기 경로와 허브 발급 화면 부재를 말하고, 자동 발급은 미지원으로만 언급한다', () => {
  const detail = otel(computeOnboarding({ ...base, ...link({ state: 'unlinked', masked: null, failure: null, ...OFF }) })).detail;
  assert.match(detail, /설정 > OTel/);
  assert.match(detail, /직접 붙여넣어/);
  assert.match(detail, /발급 화면이 아직 없어/);
  assert.match(detail, /자동 발급은 아직 지원되지 않습니다/);
});

test('masked가 마스킹 형식이 아니면(원문 모양) 화면 문구에 넣지 않는다', () => {
  const raw = `mhk_${'Z'.repeat(43)}`;
  const detail = otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: raw, failure: null, ...OFF }) })).detail;
  assert.ok(!detail.includes('ZZZZ'), detail);
  const failed = otel(computeOnboarding({ ...base, ...link({ state: 'failed', masked: raw, failure: { cause: 'applyFailed', message: '적용 실패' }, ...OFF }) })).detail;
  assert.ok(!failed.includes('ZZZZ'), failed);
});

test('조회 실패/로딩은 error/loading으로 구분되고 단정하지 않는다', () => {
  assert.equal(otel(computeOnboarding({ ...base, ...link(null, { loaded: false }) })).state, 'loading');
  const err = otel(computeOnboarding({ ...base, ...link(null, { loaded: false, error: '조회 실패' }) }));
  assert.equal(err.state, 'error');
  assert.match(err.detail, /확인하지 못했습니다/);
});

test('참고 항목은 진행률·완료·todo/unknown 판정에 영향을 주지 않는다', () => {
  const doneCore = { ...base, claudeAuth: { status: { loggedIn: true, email: 'a@b.c' }, loaded: true, loading: false, error: null } };
  const m = computeOnboarding({ ...doneCore, ...link({ state: 'unlinked', masked: null, failure: null, ...OFF }) });
  assert.equal(m.allDone, true, '필수 완료 + 수집키 미적용이어도 allDone');
  assert.equal(m.hasTodo, false);
  assert.equal(m.doneCount, 3);
  assert.equal(m.coreTotal, 3);
  const err = computeOnboarding({ ...doneCore, ...link(null, { loaded: false, error: 'x' }) });
  assert.equal(err.hasError, false);
  assert.equal(err.hasUnknown, false);
  const open = computeOnboarding({ ...base, ...link({ state: 'unlinked', masked: null, failure: null, ...OFF }) });
  assert.equal(open.currentKind, 'claude-login', 'currentKind는 필수 단계에서만 정해진다');
  assert.equal(open.doneCount, 2);
});
