// src/onboarding.ts OTEL 연동 참고 항목 단위 테스트 — 상태별 판정, 버튼 동작, "확인하지 않은 것을 주장하지 않는다"는 불변식.
// 러너: node:test + 타입 스트리핑(새 의존성 없음). onboarding.ts는 type import뿐이라 번들러 없이 불러온다.
import test from 'node:test';
import assert from 'node:assert/strict';

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  AUTO_LINK_CHANGES_NOTE, AUTO_LINK_SERVER_KEYS, computeOnboarding, DEVICE_KEY_MANAGED_NOTE, fileKeyLiveness, fileKeyNote, REISSUE_CONFIRM_MESSAGE, withoutAutoLinkNote,
} from '../../src/onboarding.ts';

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
  revokedByHub: false,
  waitingFirstData: false,
  firstDataTimedOut: false,
  blockedReason: null,
  retryAt: null,
  notice: null,
  serverEnvChanged: [],
  serverEnvRejected: [],
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
  assert.match(timedOut, /다음 점검\(앱 시작 때·1시간 간격\)에서 다시 확인합니다/);
  assert.ok(!timedOut.includes('매시간'), '매시간 15분 재폴링처럼 읽히는 문구는 쓰지 않는다');
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
  assert.match(slim(failed('reconnectRequired')), /앱에서 다시 로그인해 연결하세요/);
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
  for (const cause of ['settingsUnreadable', 'headerUnrecognized', 'endpointNotHub', 'notReady']) assert.equal(action(failed(cause)), 'open-settings', cause);
  assert.equal(action(failed('featureUnavailable')), undefined, 'N-4: 설정에서 할 일이 없는 상태에 설정 열기 버튼을 두지 않는다');
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
  // RV-02: 허브에 연결하면 붙여넣은 키는 앱이 발급한 키로 교체된다 — 수동 입력은 '연결하지 않을 때'의 경로로만 안내한다.
  assert.match(offline, /직접 받은 수집키를 쓰려면 허브에는 연결하지 말고 설정 > OTel에서 붙여넣으세요/);
  assert.match(offline, /허브에 연결하면 앱이 발급한 키로 교체됩니다/);
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

// ======================= R17 리뷰 RV-01: 웹에서 폐기된 키가 파일에 남은 경우 =======================

const KEY = 'mhk_AbCd…';
const MATRIX = [
  ['applied + confirmRequired', st('applied', { masked: KEY, confirmRequired: true })],
  ['confirmed + confirmRequired', st('confirmed', { masked: KEY, confirmRequired: true })],
  ['applied(수집 준비 안 됨) + confirmRequired', st('applied', { masked: KEY, ...OFF, confirmRequired: true })],
  ['applied + confirmDeclined', st('applied', { masked: KEY, confirmDeclined: true })],
  ['confirmed + confirmDeclined', st('confirmed', { masked: KEY, confirmDeclined: true })],
];

test('RV-01: 폐기된 키가 파일에 남아 있어도 "적용됨"/"수신 확인됨"으로 보이지 않는다', () => {
  for (const [label, status] of MATRIX) {
    const s = step(status);
    assert.equal(s.state, 'todo', `${label}: 완료(done)·부분 완료(partial)·경고(warn)가 아니라 할 일이다`);
    assert.match(s.detail, /폐기/, label);
    assert.ok(s.detail.includes(KEY), `${label}: 파일에 남은 키는 마스킹으로 밝힌다: ${s.detail}`);
    for (const claim of [...CLAIM_CONFIRMED, '수집키가 적용되었습니다', '수집키는 적용되었지만', '적용됨']) {
      assert.ok(!s.detail.includes(claim), `${label}: "${claim}" in ${s.detail}`);
    }
    for (const b of BANNED_IMPLIED) assert.ok(!s.detail.includes(b), `${label}: "${b}"`);
    assert.match(s.tag, /^폐기됨/, `${label}: 설정 패널 배지가 '적용됨/수신 확인됨'이 아니다`);
    assert.ok(s.slim && /폐기/.test(s.slim), `${label}: 필수 완료 후 홈에도 한 줄로 알린다`);
  }
});

test('RV-01: confirmRequired는 확인창 버튼, 거절 뒤에는 재발급을 시작할 [다시 시도]가 남는다', () => {
  assert.equal(step(MATRIX[0][1]).action, 'confirm-reissue');
  assert.equal(step(MATRIX[1][1]).action, 'confirm-reissue');
  for (const [label, status] of MATRIX.slice(3)) {
    const s = step(status);
    assert.equal(s.action, 'retry', `${label}: 거절 뒤에도 사용자가 다시 시작할 경로`);
    assert.match(s.detail, /거절했습니다/);
    assert.match(s.detail, /\[다시 시도\]/);
  }
});

test('RV-01: 파일에 키가 없는 경우의 기존 문구·배지는 그대로다', () => {
  const required = step(st('unlinked', { confirmRequired: true }));
  assert.match(required.detail, /웹에서 이 기기 수집키가 폐기되었습니다\. 다시 발급할까요\?/);
  assert.equal(required.tag, undefined);
  assert.equal(step(st('unlinked', { confirmDeclined: true })).action, 'retry');
});

test('RV-01: 허브 연결이 없으면(만료 등) 폐기 판정을 근거로 삼지 않는다', () => {
  const s = step(st('applied', { masked: KEY, confirmRequired: true, hubConnected: false }));
  assert.equal(s.state, 'partial', '플래그가 남아 있어도 연결이 없으면 파일 사실(적용됨)로 돌아간다');
});

// ======================= R17 리뷰 RV-09: 폐기 판정 뒤 일시 오류 점검이 폐기 표시를 지우지 않는다 =======================

test('RV-09: 폐기 판정을 받은 키가 남은 채 점검이 일시 오류로 끝나도 "적용됨"으로 돌아가지 않고 재확인 경로를 준다', () => {
  const notice = { cause: 'unreachable', message: '허브에 연결하지 못했습니다. 네트워크 상태를 확인하세요. 잠시 후 다시 시도합니다.' };
  for (const [label, state] of [['applied', 'applied'], ['confirmed', 'confirmed']]) {
    // 백엔드가 만드는 모양: 파일 상태 + revokedByHub(confirm 플래그는 꺼짐) + 일시 원인 notice.
    const s = step(st(state, { masked: KEY, revokedByHub: true, notice }));
    assert.equal(s.state, 'todo', label);
    assert.match(s.tag, /^폐기됨/, label);
    assert.equal(s.action, 'retry', `${label}: 확인창이 아니라 [다시 시도]로 다시 확인한다`);
    assert.ok(s.detail.includes(KEY), s.detail);
    assert.ok(s.detail.includes(`마지막 점검: ${notice.message}`), '일시 원인은 덧붙여 알린다');
    assert.ok(!s.detail.includes('거절'), `${label}: 사람이 거절한 것이 아니다: ${s.detail}`);
    for (const claim of [...CLAIM_CONFIRMED, '수집키가 적용되었습니다', '수집키는 적용되었지만']) {
      assert.ok(!s.detail.includes(claim), `${label}: "${claim}" in ${s.detail}`);
    }
    assert.ok(slim(st(state, { masked: KEY, revokedByHub: true, notice }))?.includes('폐기'), `${label}: 홈 한 줄`);
  }
  // 허브 연결이 없으면 판정을 근거로 삼지 않는다(RV-01과 같은 규칙).
  assert.equal(step(st('applied', { masked: KEY, revokedByHub: true, hubConnected: false })).state, 'partial');
});

// ======================= R17 리뷰 RV-07: 일시 원인은 적용·수신 확인을 지우지 않는다 =======================

test('RV-07: 일시 원인(notice)이 있어도 적용됨/수신 확인됨 상태와 문구가 유지되고 보조 안내가 붙는다', () => {
  const notice = { cause: 'unreachable', message: '허브에 연결하지 못했습니다. 네트워크 상태를 확인하세요. 잠시 후 다시 시도합니다.' };
  const applied = step(st('applied', { masked: KEY, notice }));
  assert.equal(applied.state, 'partial', '"실패"로 덮지 않는다');
  assert.match(applied.detail, /수집키가 적용되었습니다/);
  assert.ok(applied.detail.includes(`마지막 점검: ${notice.message}`), applied.detail);
  const confirmed = step(st('confirmed', { masked: KEY, notice }));
  assert.equal(confirmed.state, 'done', '확인된 수신을 지우지 않는다');
  assert.match(confirmed.detail, /데이터 수신이 확인되었습니다/);
  assert.ok(confirmed.detail.includes('마지막 점검:'));
  // 체크리스트 마커·홈 한 줄: 정상 수집 중인 PC를 '!'/방해 문구로 바꾸지 않는다.
  assert.equal(slim(st('confirmed', { masked: KEY, notice })), undefined);
  assert.equal(slim(st('applied', { masked: KEY, notice })), undefined);
  // notice가 없으면 보조 안내도 없다.
  assert.ok(!step(st('confirmed', { masked: KEY })).detail.includes('마지막 점검'));
});

test('RV-07: 키가 없을 때의 featureUnavailable은 "적용되어 있지 않습니다"가 맞는 상태이고 버튼이 없다', () => {
  const s = step(failed('featureUnavailable'));
  assert.equal(s.state, 'todo');
  assert.equal(s.action, undefined);
  assert.match(s.slim, /적용되어 있지 않습니다/);
});

// ---- R19: 201 env 적용 안내 ----

test('R19: 자동 연동 안내는 바뀌는 항목(Authorization + 허브 설정 6개)을 모두 이름으로 말하고 건드리지 않는 항목도 밝힌다', () => {
  assert.equal(AUTO_LINK_SERVER_KEYS.length, 6);
  for (const k of AUTO_LINK_SERVER_KEYS) assert.ok(AUTO_LINK_CHANGES_NOTE.includes(k), k);
  assert.match(AUTO_LINK_CHANGES_NOTE, /Authorization 항목/);
  assert.match(AUTO_LINK_CHANGES_NOTE, /허브 값이 기존 값\(앱 기본값·직접 정한 값\)보다 우선/);
  assert.match(AUTO_LINK_CHANGES_NOTE, /엔드포인트·직원 식별\(OTEL_RESOURCE_ATTRIBUTES\)·프라이버시 설정/);
  // 목록 밖 키(엔드포인트·헤더·프라이버시)는 "바뀌는 항목"으로 나열되지 않는다.
  for (const k of ['OTEL_EXPORTER_OTLP_LOGS_ENDPOINT', 'OTEL_LOG_USER_PROMPTS', 'OTEL_LOG_TOOL_CONTENT']) assert.ok(!AUTO_LINK_CHANGES_NOTE.includes(k), k);
  // 미연동(연결됨/안 됨) 두 안내 모두 같은 문장을 싣는다.
  assert.ok(step(st('unlinked')).detail.includes(AUTO_LINK_CHANGES_NOTE));
  assert.ok(step(st('unlinked', { hubConnected: false })).detail.includes(AUTO_LINK_CHANGES_NOTE));
});

test('R19: 적용됨 문구는 허브 값으로 바뀐 키와 거부한 키의 이름만 알린다', () => {
  const none = step(st('applied', { masked: 'mhk_AbCd…' })).detail;
  assert.doesNotMatch(none, /허브가 보낸/);
  const d = step(st('applied', { masked: 'mhk_AbCd…', serverEnvChanged: ['OTEL_LOGS_EXPORT_INTERVAL'], serverEnvRejected: ['OTEL_EXPORTER_OTLP_HEADERS', 'OTEL_LOG_USER_PROMPTS'] })).detail;
  assert.match(d, /허브가 보낸 값으로 바뀐 설정: OTEL_LOGS_EXPORT_INTERVAL\./);
  assert.match(d, /허용되지 않는 항목은 적용하지 않았습니다: OTEL_EXPORTER_OTLP_HEADERS, OTEL_LOG_USER_PROMPTS\./);
  // 수신 확인됨에서도 같은 안내가 붙는다.
  assert.match(step(st('confirmed', { masked: 'mhk_AbCd…', serverEnvChanged: ['OTEL_METRIC_EXPORT_INTERVAL'] })).detail, /바뀐 설정: OTEL_METRIC_EXPORT_INTERVAL/);
});

test('R19: 이름 형식이 아닌 문자열은 화면에 쓰지 않고, 백엔드 자리표시자만 허용한다', () => {
  const d = step(
    st('applied', {
      masked: 'mhk_AbCd…',
      serverEnvRejected: ['Authorization=Bearer mhk_LEAK', '<b>x</b>', 'A B', '(이름 형식 불명)', '(그 밖 3개)', 'OTEL_OK'],
    })
  ).detail;
  assert.doesNotMatch(d, /LEAK|Bearer|<b>|A B/);
  // 형식에 걸려 버린 4개(LEAK·<b>·A B 포함)는 이름 없이 개수로만 남아 거부가 실제보다 적게 보이지 않는다.
  assert.match(d, /\(이름 형식 불명\), \(그 밖 3개\), OTEL_OK, \(표시하지 않은 3개\)\./);
});

test('R19 N-2: "(그 밖 N개)"는 N이 커도(1,200개·u32 끝자리) 꼬리 표시가 사라지지 않는다', () => {
  for (const n of [999, 1200, 4294967295]) {
    const d = step(st('applied', { masked: 'mhk_AbCd…', serverEnvRejected: ['OTEL_A', `(그 밖 ${n}개)`] })).detail;
    assert.ok(d.includes(`OTEL_A, (그 밖 ${n}개).`), `${n}: ${d}`);
    assert.doesNotMatch(d, /표시하지 않은/);
  }
  // 자릿수 상한을 넘는 가짜 자리표시자는 여전히 버린다(개수로만 센다).
  const fake = step(st('applied', { masked: 'mhk_AbCd…', serverEnvRejected: ['(그 밖 12345678901개)'] })).detail;
  assert.doesNotMatch(fake, /12345678901/);
  assert.match(fake, /\(표시하지 않은 1개\)/);
});

test('R19 RV-02: 설정 화면용 문구는 6키 안내를 뺀 연동 상세만 돌려주고 홈 문구는 그대로 한 번 싣는다', () => {
  for (const status of [st('unlinked'), st('unlinked', { hubConnected: false })]) {
    const detail = step(status).detail;
    assert.equal(detail.split(AUTO_LINK_CHANGES_NOTE).length - 1, 1, '홈 체크리스트 문구는 안내를 정확히 1회');
    const forSettings = withoutAutoLinkNote(detail);
    assert.ok(!forSettings.includes('OTEL_LOGS_EXPORT_INTERVAL'), forSettings);
    assert.ok(!forSettings.includes('허브 값이 기존 값'), forSettings);
    assert.ok(forSettings.length > 20 && !forSettings.endsWith(' '), forSettings);
  }
  // 안내가 없는 문구는 그대로.
  const applied = step(st('applied', { masked: 'mhk_AbCd…' })).detail;
  assert.equal(withoutAutoLinkNote(applied), applied);
});

test('R19: 서버 env 필드가 없는 옛 상태 객체에서도 문구가 깨지지 않는다', () => {
  const old = { ...st('applied', { masked: 'mhk_AbCd…' }) };
  delete old.serverEnvChanged;
  delete old.serverEnvRejected;
  assert.match(step(old).detail, /수집키가 적용되었습니다/);
});

// ======================= R20: 웹 폐기(일반/유출 의심)와 앱 화면 문구 정렬 =======================
//
// 허브 웹 /keys의 "맑은에이전트 관리" 키 동작과 같은 사실을 말한다.
//   일반 폐기(manual)   : 그 키 1개만 폐기 → 앱이 확인을 물음(confirm_required) → 앱에서 다시 발급, 다시 로그인 불필요
//   유출 의심 폐기       : 맑은에이전트 연결과 이 기기의 다른 수집키까지 폐기 → 앱에서 다시 로그인해 연결
//   연결이 끊김(401→self 401, ALREADY_RENEWED, blocked/suspected_leak) : 다시 발급이 아니라 다시 로그인해 연결
//   self 200            : 기능 없음 — 재로그인도 재발급도 아니다

const ROOT = join(fileURLToPath(new URL('.', import.meta.url)), '..', '..');

/** 백엔드가 실제로 내려주는 고정 문구를 Rust 소스에서 그대로 읽는다 — 프론트 시험용 사본이 갈라지지 않게 한다. */
function rustMessage(variant) {
  const src = readFileSync(join(ROOT, 'src-tauri', 'src', 'otel_link.rs'), 'utf8');
  const m = new RegExp(`Self::${variant} =>\\s*\\{?\\s*"([^"]+)"`).exec(src);
  assert.ok(m, `${variant} 문구를 otel_link.rs에서 찾지 못했다`);
  return m[1];
}

const RELOGIN_REQUIRED = /다시 로그인해 연결/;
const REISSUE_ACTION = /다시 발급/;

test('r20: 일반 폐기 — 웹에서 폐기됨 → 앱에서 다시 발급, 다시 로그인을 요구하지 않는다(확인 대기·거절·재확인, 키가 남은 경우와 없는 경우)', () => {
  const cases = [
    ['확인 대기', st('unlinked', { confirmRequired: true })],
    ['확인 대기 + 파일 키', st('applied', { masked: KEY, confirmRequired: true })],
    ['거절', st('unlinked', { confirmDeclined: true })],
    ['거절 + 파일 키', st('applied', { masked: KEY, confirmDeclined: true })],
    ['재확인 필요 + 파일 키', st('applied', { masked: KEY, revokedByHub: true, notice: { cause: 'unreachable', message: '허브에 연결하지 못했습니다.' } })],
  ];
  for (const [label, status] of cases) {
    const s = step(status);
    assert.match(s.detail, /웹에서[^.]*폐기/, `${label}: 웹에서 폐기됐음을 말한다: ${s.detail}`);
    assert.match(s.detail, /다시 발급|\[다시 시도\]/, `${label}: 앱에서 다시 발급하는 길을 준다: ${s.detail}`);
    assert.match(s.detail, /다시 로그인할 필요는 없습니다/, `${label}: ${s.detail}`);
    assert.ok(!RELOGIN_REQUIRED.test(s.detail), `${label}: 재로그인·재연결을 요구하지 않는다: ${s.detail}`);
    assert.notEqual(s.action, 'connect-hub', `${label}: 허브 연결 화면으로 보내지 않는다`);
    assert.ok(['confirm-reissue', 'retry'].includes(s.action), `${label}: ${s.action}`);
  }
  // 확인창 본문과 홈 한 줄도 같은 사실이다.
  assert.match(REISSUE_CONFIRM_MESSAGE, /웹에서 이 기기 수집키가 폐기되었습니다\. 다시 발급할까요\?/);
  assert.match(REISSUE_CONFIRM_MESSAGE, /다시 로그인할 필요는 없습니다/);
  assert.ok(!RELOGIN_REQUIRED.test(REISSUE_CONFIRM_MESSAGE));
  assert.match(slim(st('applied', { masked: KEY, confirmRequired: true })), /다시 로그인 불필요/);
});

test('r20: 연결이 끊김 — 맑은에이전트 연결 해제를 말하고 다시 로그인해 연결하라고 하며 "다시 발급" 행동을 권하지 않는다', () => {
  const reconnect = rustMessage('ReconnectRequired');
  const blocked = rustMessage('Blocked');
  // 백엔드 문구 자체가 사실을 말한다: 연결 해제 + 유출 의심 폐기의 파급 + 재로그인.
  assert.match(reconnect, /이 PC의 맑은에이전트 연결이 해제되었습니다/);
  assert.match(reconnect, /유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기/);
  assert.match(reconnect, RELOGIN_REQUIRED);
  assert.ok(!REISSUE_ACTION.test(reconnect), reconnect);
  assert.match(blocked, RELOGIN_REQUIRED);
  assert.ok(!REISSUE_ACTION.test(blocked), blocked);

  const cases = [
    ['401→self 401 / ALREADY_RENEWED (reconnectRequired)', failed('reconnectRequired', { failure: { cause: 'reconnectRequired', message: reconnect } })],
    ['reconnectRequired + 파일 키', failed('reconnectRequired', { masked: KEY, failure: { cause: 'reconnectRequired', message: reconnect } })],
    ['blocked/suspected_leak', failed('blocked', { blockedReason: 'suspectedLeak', failure: { cause: 'blocked', message: blocked } })],
    ['blocked/suspected_leak + 파일 키', failed('blocked', { masked: KEY, blockedReason: 'suspectedLeak', failure: { cause: 'blocked', message: blocked } })],
  ];
  for (const [label, status] of cases) {
    const s = step(status);
    assert.equal(s.action, 'connect-hub', `${label}: 다시 로그인해 연결하는 화면으로 간다`);
    assert.match(s.detail, RELOGIN_REQUIRED, `${label}: ${s.detail}`);
    assert.ok(!REISSUE_ACTION.test(s.detail), `${label}: "다시 발급"을 행동으로 권하지 않는다: ${s.detail}`);
    assert.ok(!/\[다시 시도\]/.test(s.detail), `${label}: ${s.detail}`);
    assert.ok(s.slim && RELOGIN_REQUIRED.test(s.slim) && !REISSUE_ACTION.test(s.slim), `${label}: 홈 한 줄: ${s.slim}`);
    assert.ok(!(s.tag ?? '').includes('폐기됨 · 재발급'), `${label}: 재발급 배지를 달지 않는다`);
  }
  // 유출 의심 차단은 GET 200 — 연결은 살아 있다. 이 기기에 폐기된 키가 있어 발급이 막혔다는 사실만 말한다(R20 RV-002).
  const leak = step(cases[2][1]).detail;
  assert.match(leak, /사유: 유출 의심/);
  assert.match(leak, /이 기기에 유출 의심으로 폐기된 수집키가 있어 이 연결로는 발급이 막혔습니다/);
  assert.ok(!/연결이 해제되었습니다|연결과 이 기기의 다른 수집키도 함께 폐기/.test(leak), leak);
});

test('r20 RV-002: blocked/suspectedLeak(연결 생존)과 reconnectRequired(연결 끊김)의 홈 한 줄이 사실대로 갈린다', () => {
  const leakSlim = slim(failed('blocked', { blockedReason: 'suspectedLeak', failure: { cause: 'blocked', message: rustMessage('Blocked') } }));
  // 연결이 살아 있으므로 "해제되었습니다"가 아니다. 할 일은 "허브 연결을 해제한 뒤 다시 로그인해 연결".
  assert.ok(!/해제되었습니다/.test(leakSlim), leakSlim);
  assert.match(leakSlim, /유출 의심 폐기 이력 때문에 이 연결로는 수집키를 발급할 수 없습니다/);
  assert.match(leakSlim, /허브 연결을 해제한 뒤 다시 로그인해 연결하세요/);
  assert.ok(!REISSUE_ACTION.test(leakSlim), leakSlim);
  // 401→self 401 경로는 연결이 실제로 끊겼다 — 문구 그대로 "연결 해제됨 → 다시 로그인".
  const lostSlim = slim(failed('reconnectRequired', { failure: { cause: 'reconnectRequired', message: rustMessage('ReconnectRequired') } }));
  assert.match(lostSlim, /맑은에이전트 연결이 해제되어/);
  assert.match(lostSlim, /앱에서 다시 로그인해 연결하세요/);
  assert.notEqual(leakSlim, lostSlim);
  // 사용량 한도 차단은 유출 의심 문구를 섞지 않고 기존 안내를 쓴다.
  const quotaSlim = slim(failed('blocked', { blockedReason: 'quota', failure: { cause: 'blocked', message: rustMessage('Blocked') } }));
  assert.ok(!/유출 의심/.test(quotaSlim), quotaSlim);
});

test('r20 RV-001: 재연결 필요·차단·데스크톱 토큰 아님에서 파일에 남은 키를 "적용된 수집키"라고 말하지 않고, 폐기됐을 수 있다고 말한다', () => {
  const cases = [
    ['reconnectRequired', failed('reconnectRequired', { masked: KEY })],
    ['blocked/suspectedLeak', failed('blocked', { masked: KEY, blockedReason: 'suspectedLeak' })],
    ['notDesktopToken', failed('notDesktopToken', { masked: KEY })],
  ];
  for (const [label, status] of cases) {
    const d = step(status).detail;
    assert.ok(!/적용된 수집키/.test(d), `${label}: ${d}`);
    assert.ok(d.includes(`설정 파일에 남아 있는 수집키(${KEY})는 허브에서 함께 폐기됐을 수 있어 수집되지 않을 수 있습니다.`), `${label}: ${d}`);
  }
  // 사용량 한도 차단은 현행 키까지 폐기되므로 단정해도 된다.
  const quota = step(failed('blocked', { masked: KEY, blockedReason: 'quota' })).detail;
  assert.ok(quota.includes(`설정 파일에 남아 있는 수집키(${KEY})는 허브에서 폐기되어 수집되지 않습니다.`), quota);
  assert.ok(!/적용된 수집키|폐기됐을 수/.test(quota), quota);
  // 키가 파일에 없으면 있는 것처럼 말하지 않는다.
  for (const [label, status] of cases) assert.ok(!/설정 파일에 남아 있는 수집키/.test(step({ ...status, masked: null }).detail), label);
  // 폐기를 의심할 신호가 없는 실패는 파일 사실 그대로 말한다(기존 동작).
  assert.ok(step(failed('hubRejected', { masked: KEY })).detail.endsWith(`적용된 수집키 (${KEY}).`));
});

test('r20 RV-001: fileKeyLiveness — 허브가 폐기를 알린 경우와 폐기를 의심하는 경우를 가르고, 신호가 없으면 건드리지 않는다', () => {
  // 허브가 이미 폐기했다고 알림: 웹 폐기 확인 대기·거절·재확인 필요(applied/confirmed/unlinked), 사용량 한도 차단.
  for (const over of [{ confirmRequired: true }, { confirmDeclined: true }, { revokedByHub: true }]) {
    for (const state of ['unlinked', 'applied', 'confirmed']) assert.equal(fileKeyLiveness(st(state, { masked: KEY, ...over })), 'revoked', `${state} ${JSON.stringify(over)}`);
  }
  assert.equal(fileKeyLiveness(failed('blocked', { blockedReason: 'quota' })), 'revoked');
  // 의심: 연결이 끊김 / 데스크톱 토큰 아님 / 유출 의심 차단(GET 200이라 단정 못 함).
  assert.equal(fileKeyLiveness(failed('reconnectRequired')), 'maybeRevoked');
  assert.equal(fileKeyLiveness(failed('notDesktopToken')), 'maybeRevoked');
  assert.equal(fileKeyLiveness(failed('blocked', { blockedReason: 'suspectedLeak' })), 'maybeRevoked');
  // 신호 없음: 정상 적용·수신 확인·그 밖의 실패·상태 미확인. 허브 연결이 없으면 웹 폐기 판정도 쓰지 않는다(홈 화면과 같은 규칙).
  for (const s of [st('applied', { masked: KEY }), st('confirmed', { masked: KEY }), failed('hubRejected'), failed('unreachable'), failed('featureUnavailable'), null]) {
    assert.equal(fileKeyLiveness(s), 'noSignal');
  }
  assert.equal(fileKeyLiveness(st('applied', { masked: KEY, revokedByHub: true, hubConnected: false })), 'noSignal');
  // 문구: 마스킹을 붙이거나 생략하고, 신호가 없으면 문구가 없다.
  assert.match(fileKeyNote('revoked', KEY), /폐기되어 수집되지 않습니다\.$/);
  assert.match(fileKeyNote('maybeRevoked', null), /^설정 파일에 남아 있는 수집키는 허브에서 함께 폐기됐을 수 있어/);
  assert.equal(fileKeyNote('noSignal', KEY), null);
});

test('r20 RV-001: 웹 폐기 확인 대기에서도 파일의 키를 "적용"이라 하지 않는다(홈 문구)', () => {
  const d = step(st('applied', { masked: KEY, confirmRequired: true })).detail;
  assert.ok(!/적용된|적용되었습니다/.test(d), d);
  assert.ok(d.includes(`설정 파일에 남아 있는 키(${KEY})로는 수집되지 않습니다`), d);
});

test('r20: 연결이 끊긴 상태에서는 이전의 폐기 확인 표시가 화면을 가리지 않는다(failed 상태가 revokedByHub보다 먼저 보인다)', () => {
  // 백엔드가 폐기 표시를 비우지만, 남아 있더라도 failed 상태는 재발급 화면이 아니라 재연결 화면이다.
  const s = step(failed('reconnectRequired', { masked: KEY, revokedByHub: true, failure: { cause: 'reconnectRequired', message: rustMessage('ReconnectRequired') } }));
  assert.equal(s.action, 'connect-hub');
  assert.ok(!/폐기됨 · 재발급/.test(s.tag ?? ''));
});

test('r20: blocked/quota는 기존 안내를 유지한다 — 사유를 밝히고 연결을 다시 만든다(유출 의심 안내를 섞지 않는다)', () => {
  const blocked = rustMessage('Blocked');
  const s = step(failed('blocked', { blockedReason: 'quota', failure: { cause: 'blocked', message: blocked } }));
  assert.match(s.detail, /사유: 사용량 한도 초과/);
  assert.match(s.detail, RELOGIN_REQUIRED);
  assert.ok(!s.detail.includes('유출 의심'), s.detail);
  assert.equal(s.action, 'connect-hub');
});

test('r20: self 200(서버에 기능 없음) — 재로그인도 재발급도 아니다. 연결은 정상이고 버튼이 없다', () => {
  const msg = rustMessage('FeatureUnavailable');
  assert.match(msg, /허브 연결은 정상/);
  assert.ok(!RELOGIN_REQUIRED.test(msg) && !REISSUE_ACTION.test(msg), msg);
  const none = step(failed('featureUnavailable', { failure: { cause: 'featureUnavailable', message: msg } }));
  assert.equal(none.action, undefined);
  assert.ok(!RELOGIN_REQUIRED.test(none.detail) && !REISSUE_ACTION.test(none.detail), none.detail);
  // 키가 적용돼 있으면 notice로만 붙고 적용·확인 사실을 지우지 않는다.
  const withKey = step(st('applied', { masked: KEY, notice: { cause: 'featureUnavailable', message: msg } }));
  assert.equal(withKey.state, 'partial');
  assert.equal(withKey.action, undefined);
  assert.ok(!RELOGIN_REQUIRED.test(withKey.detail) && !REISSUE_ACTION.test(withKey.detail), withKey.detail);
});

test('r20 N-11: 파일에 폐기된 키가 없으면 "남아 있는 키"라고 말하지 않는다(재확인 필요·거절 분기)', () => {
  const notice = { cause: 'unreachable', message: '허브에 연결하지 못했습니다.' };
  // 키가 없는 채 재확인 대기 — 프론트만 보면 가능한 모양(백엔드는 이 모양을 내지 않지만 문구가 거짓을 말해선 안 된다).
  const noKey = step(st('unlinked', { revokedByHub: true, notice }));
  assert.match(noKey.tag, /폐기됨 · 재확인 필요/);
  assert.ok(!noKey.detail.includes('남아 있는'), noKey.detail);
  assert.ok(!noKey.slim.includes('남아 있어'), noKey.slim);
  assert.match(noKey.detail, /웹에서 이 기기 수집키가 폐기되었습니다/);
  const noKeyDeclined = step(st('unlinked', { confirmDeclined: true }));
  assert.ok(!noKeyDeclined.detail.includes('남아 있는'), noKeyDeclined.detail);
  // 키가 있으면 마스킹과 함께 남아 있음을 말한다.
  const onFile = step(st('applied', { masked: KEY, revokedByHub: true, notice }));
  assert.ok(onFile.detail.includes(`남아 있는 키(${KEY})`), onFile.detail);
  assert.match(onFile.slim, /남아 있어/);
});

test('r20: 설정의 수집키 영역 안내 — 앱이 관리하고, 웹 /keys에서는 회전할 수 없고 회전은 앱이 하며, 폐기 뒤 길이 다르다', () => {
  const n = DEVICE_KEY_MANAGED_NOTE;
  assert.match(n, /이 PC의 수집키는 맑은에이전트가 관리합니다/);
  assert.match(n, /\/keys/);
  assert.match(n, /웹에서는 회전할 수 없고, 회전은 앱이 합니다/);
  assert.match(n, /웹에서 폐기하면[^.]*앱에서 다시 발급할 수 있습니다\(다시 로그인할 필요는 없습니다\)/);
  assert.match(n, /웹에서 유출 의심으로 폐기하면 이 PC의 맑은에이전트 연결과 이 기기의 다른 수집키도 함께 폐기되므로 앱에서 다시 로그인해 연결해야 합니다/);
});

test('r20: 앱 안(프론트·Rust 사용자 문구)에 "웹에서 회전하라"는 반대 취지 문구가 없다', () => {
  const files = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name);
      if (statSync(p).isDirectory()) walk(p);
      else if (/\.(ts|rs)$/.test(name) && name !== 'tests.rs' && name !== 'testkit.rs') files.push(p);
    }
  };
  walk(join(ROOT, 'src'));
  walk(join(ROOT, 'src-tauri', 'src'));
  assert.ok(files.length > 50, '소스를 실제로 훑었다');
  // 웹(허브 /keys)에서 회전·교체하라고 시키는 표현. "웹에서는 회전할 수 없고"는 부정이라 걸리지 않는다.
  const opposite = /(?:웹|허브|\/keys)[^.\n]{0,15}(?:에서|으로)[^.\n]{0,10}(?:회전|교체)(?:하세요|해 주세요|하십시오|하면|해야|하거나)/;
  for (const f of files) {
    const lines = readFileSync(f, 'utf8').split('\n');
    lines.forEach((line, i) => {
      if (line.trim().startsWith('//')) return;
      assert.ok(!opposite.test(line), `${f}:${i + 1}: ${line.trim()}`);
    });
  }
});
