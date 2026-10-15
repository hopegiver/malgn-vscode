// src/otelLinkText.ts 단위 테스트 — 화면 토스트·입력 가능 여부가 실제 백엔드 결과와 어긋나지 않는지 고정한다(R17 RV-02·RV-06).
import test from 'node:test';
import assert from 'node:assert/strict';

import { disconnectToast, keyManagedByHub, reissueOutcome } from '../../src/otelLinkText.ts';

const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const base = {
  state: 'unlinked', masked: null, failure: null, issueSupported: true, receiptCheckSupported: true, telemetryEnabled: true, endpoint: HUB_EP,
  collectionReady: true, blockers: [], hubConnected: true, confirmRequired: false, confirmDeclined: false, waitingFirstData: false,
  firstDataTimedOut: false, blockedReason: null, retryAt: null, notice: null,
};
const st = (over = {}) => ({ ...base, ...over });
const fail = (cause, message) => ({ cause, message });

test('RV-02: 허브에 연결되어 앱이 관리할 수 있을 때만 수동 입력을 막는다', () => {
  assert.equal(keyManagedByHub(st()), true);
  assert.equal(keyManagedByHub(st({ hubConnected: false })), false, '허브 미연결 — 수동 입력이 유일한 경로');
  assert.equal(keyManagedByHub(st({ issueSupported: false })), false, '자동 발급을 못 하는 환경');
  assert.equal(keyManagedByHub(null), false, '상태를 못 읽었으면 막지 않는다(백엔드가 최종 판단)');
});

test('RV-06: 재발급 성공 토스트는 응답이 실제로 적용됨일 때만 나온다', () => {
  const ok = reissueOutcome(st({ state: 'applied', masked: 'mhk_AbCd…', waitingFirstData: true }), true);
  assert.equal(ok.kind, 'applied');
  assert.match(ok.text, /다시 발급해 적용했습니다/);
  assert.equal(reissueOutcome(st({ state: 'confirmed', masked: 'mhk_AbCd…' }), true).kind, 'applied');
  // 일시 보조 안내(notice)가 붙어도 적용 사실은 성공이다.
  assert.equal(reissueOutcome(st({ state: 'applied', masked: 'mhk_AbCd…', notice: fail('unreachable', 'x') }), true).kind, 'applied');
});

test('RV-06: 동의했지만 결과가 적용이 아니면 성공이라 말하지 않고 원인을 보여 준다', () => {
  for (const cause of ['notReady', 'endpointNotHub', 'credentialLimit', 'unreachable', 'blocked']) {
    const o = reissueOutcome(st({ state: 'failed', failure: fail(cause, `고정 문구:${cause}`) }), true);
    assert.equal(o.kind, 'failed', cause);
    assert.ok(!o.text.includes('적용했습니다'), o.text);
    assert.ok(o.text.includes(`고정 문구:${cause}`), o.text);
  }
  // 다른 점검이 진행 중이라 백엔드가 아무것도 하지 않은 경우: 여전히 확인 대기(또는 파일에 폐기된 키가 남은 applied+confirmRequired).
  for (const status of [st({ confirmRequired: true }), st({ state: 'applied', masked: 'mhk_AbCd…', confirmRequired: true }), st({ confirmDeclined: true })]) {
    const o = reissueOutcome(status, true);
    assert.equal(o.kind, 'pending');
    assert.ok(!o.text.includes('적용했습니다'), o.text);
  }
  assert.equal(reissueOutcome(st({ state: 'issuing' }), true).kind, 'pending');
  // 키가 없고 실패 원인도 없다(예: 허브 연결 없음) — 성공이 아니다.
  const none = reissueOutcome(st({ hubConnected: false }), true);
  assert.equal(none.kind, 'failed');
  assert.ok(!none.text.includes('적용했습니다'));
});

test('RV-06: 거절은 토스트가 없다', () => {
  assert.deepEqual(reissueOutcome(st({ confirmDeclined: true }), false), { kind: 'declined', text: null });
});

test('RV-06: 해제 토스트는 이 PC의 수집키를 실제로 지웠을 때만 "지웠습니다"라고 말한다', () => {
  assert.match(disconnectToast(null, 'removed'), /허브 연결을 해제했습니다\. 이 PC의 설정 파일에서 수집키도 지웠습니다/);
  for (const removal of ['notPresent', 'failed', undefined]) {
    const t = disconnectToast(null, removal);
    assert.ok(!t.includes('지웠습니다'), `${removal}: ${t}`);
  }
  assert.match(disconnectToast(null, 'notPresent'), /설정 파일은 바꾸지 않았습니다/);
  assert.match(disconnectToast(null, 'failed'), /지우지 못했습니다.*직접 삭제/);
  // 서버 폐기 실패는 그 사실과 함께, 키 처리 결과는 따로 말한다.
  const both = disconnectToast('SERVER_REVOKE_FAILED', 'removed');
  assert.match(both, /서버 폐기에 실패했습니다/);
  assert.match(both, /수집키도 지웠습니다/);
  assert.equal(disconnectToast(null, undefined), '허브 연결을 해제했습니다.');
});
