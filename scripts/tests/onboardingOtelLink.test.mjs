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
// 수집 준비도 필드까지 백엔드 직렬화와 같은 모양. OFF=미연동/기본(수집 준비 안 됨), READY=텔레메트리 켜짐+허브 엔드포인트.
const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const NO_EP = { targetsHub: false, insecure: false, foreignHost: false };
const OFF = { issueSupported: false, receiptCheckSupported: false, telemetryEnabled: false, endpoint: NO_EP, collectionReady: false };
const READY = { issueSupported: false, receiptCheckSupported: false, telemetryEnabled: true, endpoint: HUB_EP, collectionReady: true };
const link = (status, extra = {}) => ({ ingestLink: { status, loaded: true, loading: false, error: null, ...extra } });
const otel = (model) => model.steps.find((s) => s.kind === 'otel-link');
// 미구현 기능 주장 + 시도하지 않은 자동 동작을 암시하는 표현(자동으로 바꾸지/교체/변경 …)
const BANNED = ['자동 연동', '연동 완료', '연동되었습니다', '수신이 확인', '수신 확인됨', '자동으로 바꾸', '자동으로 교체', '자동으로 변경', '자동으로 삭제', '자동으로 갱신', '자동 교체', '바꾸지 않았습니다'];
const doneCore = { ...base, claudeAuth: { status: { loggedIn: true, email: 'a@b.c' }, loaded: true, loading: false, error: null } };
const slim = (status, extra) => otel(computeOnboarding({ ...doneCore, ...link(status, extra) })).slim;

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
    [{ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY }, 'partial'],
    [{ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF }, 'warn'],
    [{ state: 'confirmed', masked: 'mhk_AbCd…', failure: null, ...READY }, 'done'],
    [{ state: 'confirmed', masked: 'mhk_AbCd…', failure: null, ...OFF }, 'warn'],
    [{ state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: '고정 문구' }, ...OFF }, 'failed'],
    [{ state: 'failed', masked: null, failure: { cause: 'endpointNotHub', message: '고정 문구' }, ...OFF }, 'failed'],
  ];
  for (const [status, expected] of cases) assert.equal(otel(computeOnboarding({ ...base, ...link(status) })).state, expected, `${status.state}/${status.collectionReady}`);
});

test('도달 가능한 상태(unlinked/applied/failed)는 자동 연동·연동 완료·수신 확인을 주장하지 않는다', () => {
  for (const status of [
    { state: 'unlinked', masked: null, failure: null, ...OFF },
    { state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY },
    { state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF },
    { state: 'failed', masked: null, failure: { cause: 'notSupported', message: '허브 자동 발급과 수신 확인은 아직 지원되지 않습니다.' }, ...OFF },
    { state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: '설정 파일에 허브 수집키 형식이 아닌 Authorization 헤더가 있습니다. 허브로 보내려면 설정 > OTel에서 수집키로 교체하세요.' }, ...OFF },
  ]) {
    const detail = otel(computeOnboarding({ ...base, ...link(status) })).detail;
    for (const b of BANNED) assert.ok(!detail.includes(b), `${status.state}: "${b}" in ${detail}`);
    const s = slim(status);
    if (s) for (const b of BANNED) assert.ok(!s.includes(b), `slim ${status.state}: "${b}" in ${s}`);
  }
  const applied = otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY }) })).detail;
  assert.match(applied, /수신은 아직 확인하지 않았습니다/);
});

test('applied + collectionReady=true일 때만 "새로 시작하는 세션부터 적용" 안내가 나온다', () => {
  const ready = otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY }) }));
  assert.match(ready.detail, /새로 시작하는 Claude Code 세션부터 적용됩니다/);
  assert.equal(ready.slim, undefined);
  const notReady = otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF }) }));
  assert.ok(!notReady.detail.includes('새로 시작하는'), notReady.detail);
  assert.match(notReady.detail, /수집되지 않을 수 있습니다/);
  assert.match(notReady.detail, /수신은 확인하지 않았습니다/);
});

test('applied && !collectionReady는 불리언별로 원인을 구분해 보여 준다', () => {
  const detailOf = (over) => otel(computeOnboarding({ ...base, ...link({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY, collectionReady: false, ...over }) })).detail;
  const tele = detailOf({ telemetryEnabled: false });
  assert.match(tele, /텔레메트리가 꺼져 있음/);
  assert.ok(!tele.includes('허브가 아님'), tele);
  const notHub = detailOf({ endpoint: NO_EP });
  assert.match(notHub, /수집 엔드포인트가 허브가 아님/);
  assert.ok(!notHub.includes('텔레메트리가 꺼져'), notHub);
  const foreign = detailOf({ endpoint: { targetsHub: true, insecure: false, foreignHost: true } });
  assert.match(foreign, /허브 밖 주소로도 전송됨/);
  const insecure = detailOf({ endpoint: { targetsHub: true, insecure: true, foreignHost: false } });
  assert.match(insecure, /암호화되지 않은 http/);
  const both = detailOf({ telemetryEnabled: false, endpoint: NO_EP });
  assert.match(both, /텔레메트리가 꺼져 있음/);
  assert.match(both, /허브가 아님/);
});

test('collectionReady는 applied/confirmed에서만 경고 근거다 — unlinked/failed에서는 무시한다', () => {
  const unlinked = otel(computeOnboarding({ ...base, ...link({ state: 'unlinked', masked: null, failure: null, ...OFF }) }));
  assert.equal(unlinked.state, 'todo');
  assert.ok(!unlinked.detail.includes('수집되지 않을 수 있습니다'));
  const failed = otel(computeOnboarding({ ...base, ...link({ state: 'failed', masked: null, failure: { cause: 'headerUnrecognized', message: 'm' }, ...READY, collectionReady: false }) }));
  assert.equal(failed.state, 'failed');
  assert.ok(!failed.detail.includes('텔레메트리가 꺼져'));
});

test('slim: 필수 완료 후 홈에 한 줄로 노출되는 상태는 미적용/실패/수집 불가 경고뿐이다', () => {
  assert.match(slim({ state: 'unlinked', masked: null, failure: null, ...OFF }), /사용량 수집키가 적용되어 있지 않습니다/);
  assert.match(slim({ state: 'failed', masked: null, failure: { cause: 'endpointNotHub', message: 'm' }, ...OFF }), /문제가 있습니다/);
  assert.match(slim({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...OFF }), /수집되지 않을 수 있습니다 — 텔레메트리가 꺼져 있음/);
  assert.match(slim({ state: 'confirmed', masked: 'mhk_AbCd…', failure: null, ...OFF }), /수집되지 않을 수 있습니다/);
  // 노출하지 않는 상태
  assert.equal(slim({ state: 'applied', masked: 'mhk_AbCd…', failure: null, ...READY }), undefined, '적용·수집 준비 완료');
  assert.equal(slim({ state: 'confirmed', masked: 'mhk_AbCd…', failure: null, ...READY }), undefined);
  assert.equal(slim({ state: 'issuing', masked: null, failure: null, ...OFF }), undefined);
  assert.equal(slim(null, { loaded: false }), undefined, '로딩');
  assert.equal(slim(null, { loaded: false, error: '조회 실패' }), undefined, '조회 실패');
});

test('slim 문구에도 마스킹 원문이 섞이지 않는다', () => {
  const raw = `mhk_${'Z'.repeat(43)}`;
  const s = slim({ state: 'applied', masked: raw, failure: null, ...OFF });
  assert.ok(!s.includes('ZZZZ'), s);
});

test('unlinked 안내는 짧고 사실관계(발급 화면 부재·기기 토큰≠수집키·자동 발급 미지원)를 유지한다', () => {
  const detail = otel(computeOnboarding({ ...base, ...link({ state: 'unlinked', masked: null, failure: null, ...OFF }) })).detail;
  assert.match(detail, /설정 > OTel/);
  assert.match(detail, /붙여넣으세요/);
  assert.match(detail, /발급 화면이 아직 없고/);
  assert.match(detail, /자동 발급도 지원되지 않으며/);
  assert.match(detail, /기기 토큰은 수집키가 아닙니다/);
  assert.ok(detail.length <= 130, `미연동 안내가 길다(${detail.length}자): ${detail}`);
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
