// src/otelLinkText.ts 단위 테스트 — 화면 토스트·입력 가능 여부가 실제 백엔드 결과와 어긋나지 않는지 고정한다(R17 RV-02·RV-06).
import test from 'node:test';
import assert from 'node:assert/strict';

import { disconnectToast, keyManagedByHub, legacyIntervalHint, reissueToast } from '../../src/otelLinkText.ts';

const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const base = {
  state: 'unlinked', masked: null, failure: null, issueSupported: true, receiptCheckSupported: true, telemetryEnabled: true, endpoint: HUB_EP,
  collectionReady: true, blockers: [], hubConnected: true, confirmRequired: false, confirmDeclined: false, revokedByHub: false, waitingFirstData: false,
  firstDataTimedOut: false, blockedReason: null, retryAt: null, notice: null, serverEnvChanged: [], serverEnvRejected: [],
};
const st = (over = {}) => ({ ...base, ...over });

test('RV-02: 허브에 연결되어 앱이 관리할 수 있을 때만 수동 입력을 막는다', () => {
  assert.equal(keyManagedByHub(st()), true);
  assert.equal(keyManagedByHub(st({ hubConnected: false })), false, '허브 미연결 — 수동 입력이 유일한 경로');
  assert.equal(keyManagedByHub(st({ issueSupported: false })), false, '자동 발급을 못 하는 환경');
  assert.equal(keyManagedByHub(null), false, '상태를 못 읽었으면 막지 않는다(백엔드가 최종 판단)');
});

const fail = (cause, message) => ({ cause, message });

// ---- R20 RT-4: 재발급 토스트는 백엔드가 돌려준 결과(outcome) 하나와 1:1로 대응한다 ----
// (이전에는 확인창 전의 masked와 응답 상태를 비교해 성공·실패·대기를 역산했다.)

test('r20: 토스트는 백엔드 결과만으로 정해진다 — "다시 발급해 적용했습니다"는 minted일 때만 나온다', () => {
  const minted = reissueToast({ kind: 'minted', masked: 'mhk_AbCd…' });
  assert.equal(minted.kind, 'applied');
  assert.match(minted.text, /수집키를 다시 발급해 적용했습니다\. 새로 시작하는 Claude Code 세션부터 적용됩니다/);
  const others = [
    { kind: 'declined' },
    { kind: 'notStarted', reason: 'busy' },
    { kind: 'notStarted', reason: 'notPending' },
    { kind: 'noConnection' },
    { kind: 'notApplied' },
    { kind: 'failed', failure: fail('unreachable', '허브에 연결하지 못했습니다.'), transient: true },
    { kind: 'failed', failure: fail('blocked', '막힘'), transient: false },
    { kind: 'failed', failure: fail('reconnectRequired', '연결 해제'), transient: false },
  ];
  for (const o of others) {
    const t = reissueToast(o);
    assert.notEqual(t.kind, 'applied', JSON.stringify(o));
    assert.ok(t.text === null || !t.text.includes('적용했습니다'), `${JSON.stringify(o)}: ${t.text}`);
  }
  // 화면 상태를 읽지 않는다 — 인자는 결과 하나뿐이다.
  assert.equal(reissueToast.length, 1);
});

test('r20: 결과 종류마다 서로 다른 문구가 대응한다(거절만 토스트가 없다)', () => {
  assert.deepEqual(reissueToast({ kind: 'declined' }), { kind: 'declined', text: null });
  const texts = [
    reissueToast({ kind: 'minted', masked: 'mhk_AbCd…' }).text,
    reissueToast({ kind: 'notStarted', reason: 'busy' }).text,
    reissueToast({ kind: 'notStarted', reason: 'notPending' }).text,
    reissueToast({ kind: 'noConnection' }).text,
    reissueToast({ kind: 'notApplied' }).text,
    reissueToast({ kind: 'failed', failure: fail('unreachable', 'A'), transient: true }).text,
  ];
  assert.equal(new Set(texts).size, texts.length, texts.join('\n'));
  assert.match(reissueToast({ kind: 'notStarted', reason: 'busy' }).text, /다른 점검이 진행 중이라 재발급을 시작하지 못했습니다/);
  assert.equal(reissueToast({ kind: 'notStarted', reason: 'busy' }).kind, 'pending');
  assert.equal(reissueToast({ kind: 'noConnection' }).kind, 'failed');
});

test('r20: 실패 토스트는 백엔드의 고정 원인 문구를 그대로 싣고, 일시 원인에는 [다시 시도] 길을 남긴다', () => {
  const transient = reissueToast({ kind: 'failed', failure: fail('unreachable', '허브에 연결하지 못했습니다. 잠시 후 다시 시도합니다.'), transient: true });
  assert.equal(transient.kind, 'failed');
  assert.ok(transient.text.includes('허브에 연결하지 못했습니다. 잠시 후 다시 시도합니다.'));
  assert.match(transient.text, /\[다시 시도\]로 다시 발급할 수 있습니다/);
  const permanent = reissueToast({ kind: 'failed', failure: fail('credentialLimit', '수집키 상한(5개)에 도달했습니다.'), transient: false });
  assert.ok(permanent.text.includes('수집키 상한(5개)에 도달했습니다.'));
  assert.ok(!permanent.text.includes('[다시 시도]'), '다시 시도로 풀리지 않는 원인에 그 버튼을 권하지 않는다');
});

test('r20: 연결이 끊겨 실패한 재발급 토스트는 재발급이 아니라 다시 로그인해 연결하라고 말한다', () => {
  const message = '이 PC의 맑은에이전트 연결이 해제되었습니다. 앱에서 다시 로그인해 연결하세요.';
  const t = reissueToast({ kind: 'failed', failure: fail('reconnectRequired', message), transient: false });
  assert.equal(t.kind, 'failed');
  assert.ok(t.text.includes('다시 로그인해 연결하세요'), t.text);
  assert.ok(!t.text.includes('다시 발급'), `연결이 끊긴 상태에서 "다시 발급"을 행동으로 권하지 않는다: ${t.text}`);
  assert.ok(!t.text.includes('[다시 시도]'), t.text);
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
