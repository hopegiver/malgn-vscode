// src/onboarding.ts OTEL 연동 참고 항목 단위 테스트 — 상태별 판정, 버튼 동작, "확인하지 않은 것을 주장하지 않는다"는 불변식.
// 러너: node:test + 타입 스트리핑(새 의존성 없음). onboarding.ts는 type import뿐이라 번들러 없이 불러온다.
import test from 'node:test';
import assert from 'node:assert/strict';

import { computeOnboarding } from '../../src/onboarding.ts';

const tool = (id, installed, required = true) => ({ id, name: id, installed, version: installed ? '1.0.0' : null, path: installed ? `/bin/${id}` : null, required });
const base = {
  devTools: { items: [tool('node', true), tool('git', true), tool('claude', true)], loaded: true, loading: false, error: null },
  claudeAuth: { status: { loggedIn: false }, loaded: true, loading: false, error: null },
};
// 백엔드 직렬화와 같은 모양. OFF=미연동/기본(수집 준비 안 됨), READY=수집 준비 완료.
const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const NO_EP = { targetsHub: false, insecure: false, foreignHost: false };
const EXTRA = {
  hubConnected: true,
  confirmRequired: false,
  confirmDeclined: false,
  waitingFirstData: false,
  firstDataTimedOut: false,
  blockedReason: null,
  retryAt: null,
  issueSupported: true,
  receiptCheckSupported: true,
};
const OFF = { ...EXTRA, telemetryEnabled: false, endpoint: NO_EP, collectionReady: false, blockers: ['telemetryOff', 'exporterNotOtlp', 'protocolMismatch', 'endpointMissing'] };
const READY = { ...EXTRA, telemetryEnabled: true, endpoint: HUB_EP, collectionReady: true, blockers: [] };
const st = (state, over = {}) => ({ state, masked: null, failure: null, ...READY, ...over });
const failed = (cause, over = {}) => st('failed', { failure: { cause, message: `고정 문구:${cause}` }, ...over });
const link = (status, extra = {}) => ({ ingestLink: { status, loaded: true, loading: false, error: null, ...extra } });
const otel = (model) => model.steps.find((s) => s.kind === 'otel-link');
const doneCore = { ...base, claudeAuth: { status: { loggedIn: true, email: 'a@b.c' }, loaded: true, loading: false, error: null } };
const step = (status, extra) => otel(computeOnboarding({ ...base, ...link(status, extra) }));
const slim = (status, extra) => otel(computeOnboarding({ ...doneCore, ...link(status, extra) })).slim;

// 확인하지 않은 것을 주장하는 표현 — 해당 상태가 아닐 때 나오면 안 된다.
const CLAIM_CONFIRMED = ['수신이 확인되었습니다', '수신 확인됨', '데이터 수신이 확인'];
// 시도하지 않은 자동 동작을 암시하는 표현.
const BANNED_IMPLIED = ['자동으로 바꾸', '자동으로 교체', '자동으로 변경', '자동으로 삭제', '자동으로 갱신', '자동 교체', '바꾸지 않았습니다'];

test('ingestLink 입력이 없으면 항목이 없고 기존 3단계 판정은 그대로다', () => {
  const m = computeOnboarding(base);
  assert.equal(m.steps.length, 3);
  assert.equal(m.coreTotal, 3);
  assert.equal(otel(m), undefined);
});

test('상태별로 서로 다른 step.state가 나온다', () => {
  const cases = [
    [st('unlinked', { ...OFF }), 'todo'],
    [st('unlinked', { hubConnected: false }), 'todo'],
    [st('issuing'), 'loading'],
    [st('applied', { masked: 'mhk_AbCd…' }), 'partial'],
    [st('applied', { masked: 'mhk_AbCd…', ...OFF }), 'warn'],
    [st('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }), 'partial'],
    [st('confirmed', { masked: 'mhk_AbCd…' }), 'done'],
    [st('confirmed', { masked: 'mhk_AbCd…', ...OFF }), 'warn'],
    [failed('headerUnrecognized', OFF), 'failed'],
    [failed('endpointNotHub', OFF), 'failed'],
    [failed('unreachable'), 'failed'],
    [failed('blocked', { blockedReason: 'quota' }), 'failed'],
    [failed('featureUnavailable'), 'todo'],
    [failed('notReady', OFF), 'todo'],
  ];
  for (const [status, expected] of cases) assert.equal(step(status).state, expected, `${status.state}/${status.failure?.cause}/${status.collectionReady}`);
});

test('확인(수신 확인됨)은 confirmed 상태일 때만 말한다', () => {
  for (const status of [
    st('unlinked'),
    st('issuing'),
    st('applied', { masked: 'mhk_AbCd…' }),
    st('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }),
    st('applied', { masked: 'mhk_AbCd…', firstDataTimedOut: true }),
    st('applied', { masked: 'mhk_AbCd…', ...OFF }),
    failed('applyFailed', { masked: 'mhk_AbCd…' }),
  ]) {
    const detail = step(status).detail;
    for (const c of CLAIM_CONFIRMED) assert.ok(!detail.includes(c), `${status.state}: "${c}" in ${detail}`);
    const s = slim(status);
    if (s) for (const c of CLAIM_CONFIRMED) assert.ok(!s.includes(c), `slim ${status.state}: "${c}" in ${s}`);
  }
  assert.match(step(st('confirmed', { masked: 'mhk_AbCd…' })).detail, /허브에서 데이터 수신이 확인되었습니다/);
});

test('어떤 상태의 문구도 시도하지 않은 자동 동작을 암시하지 않는다', () => {
  const statuses = [
    st('unlinked'), st('unlinked', { hubConnected: false }), st('unlinked', { confirmRequired: true }), st('unlinked', { confirmDeclined: true }),
    st('applied', { masked: 'mhk_AbCd…' }), st('applied', { masked: 'mhk_AbCd…', ...OFF }),
    failed('headerUnrecognized', OFF), failed('notReady', OFF), failed('unreachable'), failed('rotateBudgetExceeded', { retryAt: '2026-10-16T01:02:03.456Z' }),
  ];
  for (const status of statuses) {
    const detail = step(status).detail;
    for (const b of BANNED_IMPLIED) assert.ok(!detail.includes(b), `${status.state}: "${b}" in ${detail}`);
    const s = slim(status);
    if (s) for (const b of BANNED_IMPLIED) assert.ok(!s.includes(b), `slim ${status.state}: "${b}" in ${s}`);
  }
});

test('applied: 수신 대기 / 15분 경과 / 기본 문구가 구분된다', () => {
  const waiting = step(st('applied', { masked: 'mhk_AbCd…', waitingFirstData: true })).detail;
  assert.match(waiting, /첫 데이터 수신을 확인하는 중입니다\(최대 15분\)/);
  assert.match(waiting, /새로 시작하는 Claude Code 세션부터 적용됩니다/);
  const timedOut = step(st('applied', { masked: 'mhk_AbCd…', firstDataTimedOut: true })).detail;
  assert.match(timedOut, /15분 안에 수신이 확인되지 않았습니다/);
  assert.match(timedOut, /새 Claude Code 세션을 열면 수집이 시작됩니다/);
  assert.match(timedOut, /다음 앱 시작 때 다시 확인합니다/);
  const plain = step(st('applied', { masked: 'mhk_AbCd…' })).detail;
  assert.match(plain, /수신은 아직 확인하지 않았습니다/);
});

test('applied + collectionReady=true일 때만 "새로 시작하는 세션부터 적용" 안내가 나온다', () => {
  const ready = step(st('applied', { masked: 'mhk_AbCd…' }));
  assert.match(ready.detail, /새로 시작하는 Claude Code 세션부터 적용됩니다/);
  assert.equal(ready.slim, undefined);
  const notReady = step(st('applied', { masked: 'mhk_AbCd…', ...OFF }));
  assert.ok(!notReady.detail.includes('새로 시작하는'), notReady.detail);
  assert.match(notReady.detail, /수집되지 않을 수 있습니다/);
  assert.match(notReady.detail, /수신은 확인하지 않았습니다/);
});

test('applied && !collectionReady는 blockers별로 원인을 구분하고 settings.json 기준임을 밝힌다(N-6)', () => {
  const detailOf = (blockers) => step(st('applied', { masked: 'mhk_AbCd…', collectionReady: false, blockers })).detail;
  const tele = detailOf(['telemetryOff']);
  assert.match(tele, /설정 파일에 텔레메트리 켜기 설정이 없음/);
  assert.ok(!tele.includes('꺼져 있음'), `파일에 없다는 사실보다 강한 단정이다: ${tele}`);
  assert.match(detailOf(['exporterNotOtlp']), /metrics·logs 내보내기가 otlp로 지정되어 있지 않음/);
  assert.match(detailOf(['protocolMismatch']), /프로토콜이 http\/protobuf가 아님/);
  assert.match(detailOf(['endpointMissing']), /수집 엔드포인트가 없음/);
  assert.match(detailOf(['endpointNotHub']), /수집 엔드포인트가 허브가 아님/);
  assert.match(detailOf(['endpointForeign']), /허브 밖 엔드포인트가 함께 있어/);
  assert.match(detailOf(['endpointInsecure']), /암호화되지 않은 http/);
  const both = detailOf(['telemetryOff', 'endpointMissing']);
  assert.match(both, /텔레메트리 켜기 설정이 없음/);
  assert.match(both, /수집 엔드포인트가 없음/);
  // 알 수 없는 식별자나 빈 목록은 일반 문구로 — 원문을 만들어 내지 않는다.
  assert.match(detailOf([]), /수집 설정을 확인해야 함/);
  assert.match(detailOf(['somethingNew']), /수집 설정을 확인해야 함/);
});

test('collectionReady·blockers는 applied/confirmed에서만 경고 근거다 — unlinked/failed에서는 무시한다', () => {
  const unlinked = step(st('unlinked', { ...OFF }));
  assert.equal(unlinked.state, 'todo');
  assert.ok(!unlinked.detail.includes('수집되지 않을 수 있습니다'));
  const f = step(failed('headerUnrecognized', { ...READY, collectionReady: false, blockers: ['telemetryOff'] }));
  assert.equal(f.state, 'failed');
  assert.ok(!f.detail.includes('텔레메트리'));
});

test('slim: 필수 완료 후 홈에 한 줄로 노출되는 상태는 미적용/실패/수집 불가 경고뿐이다', () => {
  assert.match(slim(st('unlinked')), /사용량 수집키가 아직 적용되어 있지 않습니다/);
  assert.match(slim(st('unlinked', { hubConnected: false })), /허브에 연결하면 사용량 수집키가 자동으로 적용됩니다/);
  assert.match(slim(failed('endpointNotHub', OFF)), /수집 엔드포인트가 허브가 아니어서 수집키를 발급받지 않았습니다/);
  assert.match(slim(st('applied', { masked: 'mhk_AbCd…', ...OFF })), /수집되지 않을 수 있습니다 — 설정 파일에 텔레메트리 켜기 설정이 없음/);
  assert.match(slim(st('confirmed', { masked: 'mhk_AbCd…', ...OFF })), /수집되지 않을 수 있습니다/);
  assert.match(slim(failed('reconnectRequired')), /허브에 다시 연결하세요/);
  assert.match(slim(failed('credentialLimit')), /\/keys/);
  assert.match(slim(st('unlinked', { confirmRequired: true })), /다시 발급할지 확인이 필요합니다/);
  assert.match(slim(failed('featureUnavailable')), /적용되어 있지 않습니다/);
  // 노출하지 않는 상태
  assert.equal(slim(st('applied', { masked: 'mhk_AbCd…' })), undefined, '적용·수집 준비 완료');
  assert.equal(slim(st('applied', { masked: 'mhk_AbCd…', waitingFirstData: true })), undefined, '수신 대기');
  assert.equal(slim(st('confirmed', { masked: 'mhk_AbCd…' })), undefined);
  assert.equal(slim(st('issuing')), undefined);
  assert.equal(slim(failed('unreachable')), undefined, '자동 재시도 중인 일시 오류는 홈을 방해하지 않는다');
  assert.equal(slim(failed('rateLimited')), undefined);
  assert.equal(slim(null, { loaded: false }), undefined, '로딩');
  assert.equal(slim(null, { loaded: false, error: '조회 실패' }), undefined, '조회 실패');
});

test('slim 문구에도 마스킹 원문이 섞이지 않는다', () => {
  const raw = `mhk_${'Z'.repeat(43)}`;
  const s = slim(st('applied', { masked: raw, ...OFF }));
  assert.ok(!s.includes('ZZZZ'), s);
});

test('버튼 동작: 상태가 정한다(다시 시도 / 허브 연결 열기 / 확인하기 / OTel 설정 열기 / 없음)', () => {
  const action = (status) => step(status).action;
  assert.equal(action(st('unlinked')), 'retry');
  assert.equal(action(st('unlinked', { confirmDeclined: true })), 'retry');
  assert.equal(action(st('unlinked', { hubConnected: false })), 'connect-hub');
  assert.equal(action(st('unlinked', { confirmRequired: true })), 'confirm-reissue');
  assert.equal(action(st('issuing')), undefined);
  assert.equal(action(st('applied', { masked: 'mhk_AbCd…' })), undefined);
  assert.equal(action(st('applied', { masked: 'mhk_AbCd…', ...OFF })), 'open-settings');
  assert.equal(action(st('confirmed', { masked: 'mhk_AbCd…' })), undefined);
  for (const cause of ['unreachable', 'rateLimited', 'hubRejected', 'invalidKeyReturned', 'applyFailed', 'credentialLimit', 'rotationLost']) assert.equal(action(failed(cause)), 'retry', cause);
  for (const cause of ['reconnectRequired', 'notDesktopToken', 'blocked']) assert.equal(action(failed(cause)), 'connect-hub', cause);
  for (const cause of ['settingsUnreadable', 'headerUnrecognized', 'endpointNotHub', 'notReady', 'featureUnavailable']) assert.equal(action(failed(cause)), 'open-settings', cause);
  assert.equal(action(failed('rotateBudgetExceeded', { retryAt: '2026-10-16T01:02:03.456Z' })), undefined, '서버가 정한 시각까지 다시 시도는 소용없다');
});

test('실패 문구: blocked는 사유를, 회전 예산은 다시 가능한 시각을 덧붙이고, 백엔드 고정 문구를 그대로 쓴다', () => {
  const blockedQuota = step(failed('blocked', { blockedReason: 'quota' })).detail;
  assert.match(blockedQuota, /고정 문구:blocked/);
  assert.match(blockedQuota, /사유: 사용량 한도 초과/);
  assert.match(step(failed('blocked', { blockedReason: 'suspectedLeak' })).detail, /사유: 유출 의심/);
  assert.ok(!step(failed('blocked')).detail.includes('사유'), '사유를 모르면 지어내지 않는다');
  const budget = step(failed('rotateBudgetExceeded', { retryAt: '2026-10-16T01:02:03.456Z' })).detail;
  assert.match(budget, /고정 문구:rotateBudgetExceeded/);
  assert.match(budget, /다시 교체할 수 있는 시각/);
  assert.ok(!step(failed('rotateBudgetExceeded', { retryAt: '시각아님' })).detail.includes('시각:'), '해석 못 하는 시각은 쓰지 않는다');
});

test('notReady는 오류가 아니라 할 일이고, 확인할 항목을 blockers로 알린다', () => {
  const s = step(failed('notReady', { ...OFF, blockers: ['telemetryOff', 'endpointMissing'] }));
  assert.equal(s.state, 'todo');
  assert.match(s.detail, /확인할 항목: .*텔레메트리 켜기 설정이 없음.*수집 엔드포인트가 없음/);
});

test('unlinked 안내: 허브 연결 여부·확인 대기·거절 상태별로 한 가지씩 말한다', () => {
  const connected = step(st('unlinked')).detail;
  assert.match(connected, /허브에 연결되어 있습니다/);
  assert.match(connected, /자동으로 발급·적용합니다/);
  assert.match(connected, /\[다시 시도\]/);
  const offline = step(st('unlinked', { hubConnected: false })).detail;
  assert.match(offline, /허브에 연결하면 사용량 수집키가 자동으로 발급·적용됩니다/);
  assert.match(offline, /설정 > OTel에서 붙여넣을 수도 있습니다/);
  const confirm = step(st('unlinked', { confirmRequired: true })).detail;
  assert.match(confirm, /웹에서 이 기기 수집키가 폐기되었습니다\. 다시 발급할까요\?/);
  const declined = step(st('unlinked', { confirmDeclined: true })).detail;
  assert.match(declined, /거절했습니다/);
  assert.match(declined, /이번 앱 실행 동안은 다시 묻지 않습니다/);
});

test('masked가 마스킹 형식이 아니면(원문 모양) 화면 문구에 넣지 않는다', () => {
  const raw = `mhk_${'Z'.repeat(43)}`;
  const detail = step(st('applied', { masked: raw })).detail;
  assert.ok(!detail.includes('ZZZZ'), detail);
  const f = step(failed('applyFailed', { masked: raw })).detail;
  assert.ok(!f.includes('ZZZZ'), f);
});

test('조회 실패/로딩은 error/loading으로 구분되고 단정하지 않는다', () => {
  assert.equal(step(null, { loaded: false }).state, 'loading');
  const err = step(null, { loaded: false, error: '조회 실패' });
  assert.equal(err.state, 'error');
  assert.match(err.detail, /확인하지 못했습니다/);
});

test('참고 항목은 진행률·완료·todo/unknown 판정에 영향을 주지 않는다', () => {
  const m = computeOnboarding({ ...doneCore, ...link(st('unlinked')) });
  assert.equal(m.allDone, true, '필수 완료 + 수집키 미적용이어도 allDone');
  assert.equal(m.hasTodo, false);
  assert.equal(m.doneCount, 3);
  assert.equal(m.coreTotal, 3);
  const e = computeOnboarding({ ...doneCore, ...link(null, { loaded: false, error: 'x' }) });
  assert.equal(e.hasError, false);
  assert.equal(e.hasUnknown, false);
  const open = computeOnboarding({ ...base, ...link(st('unlinked')) });
  assert.equal(open.currentKind, 'claude-login', 'currentKind는 필수 단계에서만 정해진다');
  assert.equal(open.doneCount, 2);
});
