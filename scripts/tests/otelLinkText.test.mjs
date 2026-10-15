// src/otelLinkText.ts 단위 테스트 — 화면 토스트·입력 가능 여부가 실제 백엔드 결과와 어긋나지 않는지 고정한다(R17 RV-02·RV-06).
import test from 'node:test';
import assert from 'node:assert/strict';

import { disconnectToast, keyManagedByHub, legacyIntervalHint, reissueOutcome } from '../../src/otelLinkText.ts';

const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const base = {
  state: 'unlinked', masked: null, failure: null, issueSupported: true, receiptCheckSupported: true, telemetryEnabled: true, endpoint: HUB_EP,
  collectionReady: true, blockers: [], hubConnected: true, confirmRequired: false, confirmDeclined: false, revokedByHub: false, waitingFirstData: false,
  firstDataTimedOut: false, blockedReason: null, retryAt: null, notice: null, serverEnvChanged: [], serverEnvRejected: [],
};
const st = (over = {}) => ({ ...base, ...over });
const fail = (cause, message) => ({ cause, message });

test('RV-02: 허브에 연결되어 앱이 관리할 수 있을 때만 수동 입력을 막는다', () => {
  assert.equal(keyManagedByHub(st()), true);
  assert.equal(keyManagedByHub(st({ hubConnected: false })), false, '허브 미연결 — 수동 입력이 유일한 경로');
  assert.equal(keyManagedByHub(st({ issueSupported: false })), false, '자동 발급을 못 하는 환경');
  assert.equal(keyManagedByHub(null), false, '상태를 못 읽었으면 막지 않는다(백엔드가 최종 판단)');
});

const OLD = 'mhk_OLDK…';
const NEW = 'mhk_AbCd…';

test('RV-06: 재발급 성공 토스트는 다른 키가 적용됐다는 관측이 있을 때만 나온다', () => {
  const ok = reissueOutcome(st({ state: 'applied', masked: NEW, waitingFirstData: true }), true, OLD);
  assert.equal(ok.kind, 'applied');
  assert.match(ok.text, /다시 발급해 적용했습니다/);
  assert.equal(reissueOutcome(st({ state: 'confirmed', masked: NEW }), true, OLD).kind, 'applied');
  // 키가 없던 상태(null)에서 시작해 키가 생겼다.
  assert.equal(reissueOutcome(st({ state: 'applied', masked: NEW }), true, null).kind, 'applied');
  // 일시 보조 안내(notice)가 붙어도 새 키가 적용된 사실은 성공이다.
  assert.equal(reissueOutcome(st({ state: 'applied', masked: NEW, notice: fail('unreachable', 'x') }), true, OLD).kind, 'applied');
});

test('RV-09: 폐기된 키가 그대로 남은 채 발급 요청이 일시 오류로 끝나면 성공이라 말하지 않는다', () => {
  const notice = fail('unreachable', '허브에 연결하지 못했습니다. 잠시 후 다시 시도합니다.');
  // 백엔드가 이 경우 내는 모양: applied(이전 키) + revokedByHub(confirm 플래그 꺼짐) + notice.
  for (const status of [
    st({ state: 'applied', masked: OLD, revokedByHub: true, notice }),
    st({ state: 'confirmed', masked: OLD, revokedByHub: true, notice }),
    st({ state: 'applied', masked: OLD, revokedByHub: true, notice: fail('rateLimited', '요청이 많아 잠시 후 다시 시도합니다.') }),
  ]) {
    const o = reissueOutcome(status, true, OLD);
    assert.equal(o.kind, 'failed', JSON.stringify(status));
    assert.ok(!o.text.includes('적용했습니다'), o.text);
    assert.ok(o.text.includes(status.notice.message), '일시 원인을 알린다');
    assert.match(o.text, /폐기된 수집키가 아직 이 PC에 남아 있습니다/);
  }
  // 폐기 표시가 없더라도 키가 그대로면(같은 마스킹) 새 키가 적용됐다는 관측이 없다.
  assert.notEqual(reissueOutcome(st({ state: 'applied', masked: OLD }), true, OLD).kind, 'applied');
  assert.notEqual(reissueOutcome(st({ state: 'applied', masked: OLD, notice: fail('unreachable', 'x') }), true, OLD).kind, 'applied');
  // 답하기 전 상태를 몰랐다면 관측할 수 없다.
  assert.notEqual(reissueOutcome(st({ state: 'applied', masked: NEW }), true, undefined).kind, 'applied');
  // 새 키가 적용됐더라도 폐기 판정이 이어지는 상태라면(다른 점검이 새 판정을 준 경우 등) 성공이라 말하지 않는다.
  assert.notEqual(reissueOutcome(st({ state: 'applied', masked: NEW, revokedByHub: true }), true, OLD).kind, 'applied');
});

test('RV-06: 동의했지만 결과가 적용이 아니면 성공이라 말하지 않고 원인을 보여 준다', () => {
  for (const cause of ['notReady', 'endpointNotHub', 'credentialLimit', 'unreachable', 'blocked']) {
    const o = reissueOutcome(st({ state: 'failed', failure: fail(cause, `고정 문구:${cause}`) }), true, OLD);
    assert.equal(o.kind, 'failed', cause);
    assert.ok(!o.text.includes('적용했습니다'), o.text);
    assert.ok(o.text.includes(`고정 문구:${cause}`), o.text);
  }
  // 다른 점검이 진행 중이라 백엔드가 아무것도 하지 않은 경우: 여전히 확인 대기(또는 파일에 폐기된 키가 남은 applied+confirmRequired).
  for (const status of [st({ confirmRequired: true }), st({ state: 'applied', masked: 'mhk_AbCd…', confirmRequired: true }), st({ confirmDeclined: true })]) {
    const o = reissueOutcome(status, true, 'mhk_AbCd…');
    assert.equal(o.kind, 'pending');
    assert.ok(!o.text.includes('적용했습니다'), o.text);
  }
  assert.equal(reissueOutcome(st({ state: 'issuing' }), true, null).kind, 'pending');
  // 키가 없고 실패 원인도 없다(예: 허브 연결 없음) — 성공이 아니다.
  const none = reissueOutcome(st({ hubConnected: false }), true, null);
  assert.equal(none.kind, 'failed');
  assert.ok(!none.text.includes('적용했습니다'));
});

test('RV-06: 거절은 토스트가 없다', () => {
  assert.deepEqual(reissueOutcome(st({ confirmDeclined: true }), false, null), { kind: 'declined', text: null });
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

test('R19: 옛 기본값 5000만 권장값 안내를 받고, 안내는 파일을 바꾼다고 말하지 않는다', () => {
  const hint = legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', '5000', '60000');
  assert.ok(hint, '5000이면 안내가 있다');
  assert.match(hint, /이전 앱 버전의 기본값/);
  assert.match(hint, /권장값은 60000/);
  assert.match(hint, /직접 정한 값이면 그대로 두어도 됩니다/);
  // 201 적용 때 서버 값이 덮는다는 사실을 숨기지 않는다.
  assert.match(hint, /허브가 보낸 값이 이 값을 덮어씁니다/);
  assert.match(hint, /값을 고쳐 저장하세요/);
});

test('R19: 사용자가 정한 다른 값·다른 키·파일에 값이 없는 경우·권장값과 같은 경우에는 안내가 없다', () => {
  assert.equal(legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', '30000', '60000'), null);
  assert.equal(legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', '60000', '60000'), null);
  assert.equal(legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', undefined, '60000'), null);
  assert.equal(legacyIntervalHint('OTEL_METRIC_EXPORT_INTERVAL', '5000', '60000'), null);
  assert.equal(legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', '5000', undefined), null);
  assert.equal(legacyIntervalHint('OTEL_LOGS_EXPORT_INTERVAL', '5000', '5000'), null);
});
