// src/devToolsText.ts 단위 테스트 — 개발 도구 화면의 배지·결과 동사·사유 문구가 실제 상태와 어긋나지 않는지 고정한다(R20: R14 리뷰 RV-002·007·010·011).
import test from 'node:test';
import assert from 'node:assert/strict';

import {
  chainConsentMessage, chainStuckToast, classifyRun, isVersionUnverified, toolStatusKind, toolStatusLabel, updatedSummary, versionChangeNote,
} from '../../src/devToolsText.ts';
import { isVersionUnverified as orderIsVersionUnverified } from '../../src/devToolsOrder.ts';

test('r20 RV-007: 경로는 있는데 버전을 못 읽은 도구는 "설치 안 됨/미설치"가 아니라 "버전 미확인"으로 표시한다', () => {
  const unverified = { installed: false, version: null, path: '/opt/homebrew/bin/node' };
  assert.equal(isVersionUnverified(unverified), true);
  assert.equal(toolStatusLabel(unverified, '설치 안 됨'), '버전 미확인');
  assert.equal(toolStatusLabel(unverified, '미설치'), '버전 미확인');
});

test('r20 RV-007: 경로가 없으면 사실대로 미설치, 설치됨이면 버전(없으면 "설치됨")을 그대로 보인다', () => {
  assert.equal(toolStatusLabel({ installed: false, version: null, path: null }, '설치 안 됨'), '설치 안 됨');
  assert.equal(toolStatusLabel({ installed: false, version: null }, '미설치'), '미설치');
  assert.equal(toolStatusLabel({ installed: true, version: '22.1.0', path: '/x/node' }, '설치 안 됨'), '22.1.0');
  assert.equal(toolStatusLabel({ installed: true, version: null, path: '/x/node' }, '설치 안 됨'), '설치됨');
  assert.equal(isVersionUnverified({ installed: true, path: '/x/node' }), false);
});

test('r20 RV-010: 실행 전 경로가 있던 도구의 이전 버전 미확인 실행은 "신규 설치"가 아니다', () => {
  const kind = classifyRun({ versionBefore: null, wasInstalled: false, hadPathBefore: true });
  assert.equal(kind, 'updateUnverified');
  assert.equal(updatedSummary(kind, '22.2.0'), 'v22.2.0(으)로 업데이트되었습니다(이전 버전 미확인)');
  const note = versionChangeNote(kind, null, '22.2.0', '22.2.0');
  assert.equal(note, '이전 버전 미확인 → 22.2.0');
  assert.doesNotMatch(`${updatedSummary(kind, '22.2.0')} ${note}`, /신규 설치|설치되었습니다/);
});

test('r20 RV-010: 이미 설치로 분류됐는데 이전 버전 재조회만 비면 설치가 아니라 업데이트(이전 버전 미확인)로 말한다', () => {
  assert.equal(classifyRun({ versionBefore: undefined, wasInstalled: true, hadPathBefore: true }), 'updateUnverified');
});

test('r20 RV-010: 경로도 버전도 없던 도구만 신규 설치, 이전 버전을 아는 실행은 업데이트로 말한다(기존 동작 유지)', () => {
  const fresh = classifyRun({ versionBefore: null, wasInstalled: false, hadPathBefore: false });
  assert.equal(fresh, 'install');
  assert.equal(updatedSummary(fresh, '2.0.68'), 'v2.0.68(으)로 설치되었습니다');
  assert.equal(versionChangeNote(fresh, null, '2.0.68', '2.0.68'), '신규 설치: 2.0.68');
  const upd = classifyRun({ versionBefore: '2.0.60', wasInstalled: true, hadPathBefore: true });
  assert.equal(upd, 'update');
  assert.equal(updatedSummary(upd, '2.0.68'), 'v2.0.68(으)로 업데이트되었습니다');
  assert.equal(versionChangeNote(upd, '2.0.60', '2.0.68', '2.0.68'), '2.0.60 → 2.0.68');
  assert.equal(versionChangeNote(upd, null, null, null), null);
});

test('r20 RV-002: 자동 설치가 안 되는 이유를 "설치 경로를 찾지 못했다"로 단정하지 않는다', () => {
  const toast = chainStuckToast('Node.js', 'Claude Code, pnpm');
  assert.match(toast, /Node\.js 설치가 확인되었지만 Claude Code, pnpm은\(는\) 앱이 자동으로 설치할 수 없는 상태입니다/);
  assert.match(toast, /각 항목의 안내에 이유가 있습니다/);
  assert.doesNotMatch(toast, /경로를 찾지 못/);
});

test('r20 RV-002: 후보와 함께 자동 설치 불가 도구가 있으면 확인창에도 한 줄로 알린다', () => {
  const withStuck = chainConsentMessage('Node.js', 'pnpm', 1, 'Wrangler');
  assert.match(withStuck, /다음 1개 도구를 설치합니다: pnpm\. \(자동 설치 불가: Wrangler — 항목 안내 참고\)/);
  assert.match(withStuck, /계속할까요\?$/);
  const without = chainConsentMessage('Node.js', 'pnpm', 1, '');
  assert.doesNotMatch(without, /자동 설치 불가/);
  assert.match(without, /설치합니다: pnpm\. 각 도구는/);
});

test('r20 RV-007: 버전 미확인의 판정은 한 곳(devToolsOrder)에만 있고, 배지·사이드바 톤이 같은 종류를 쓴다', () => {
  assert.equal(isVersionUnverified, orderIsVersionUnverified, '같은 식을 두 곳에 두지 않는다');
  assert.equal(toolStatusKind({ installed: true, path: '/x/node' }), 'installed');
  assert.equal(toolStatusKind({ installed: false, path: '/opt/homebrew/bin/node' }), 'versionUnverified');
  assert.equal(toolStatusKind({ installed: false, path: null }), 'missing');
  assert.equal(toolStatusKind({ installed: false }), 'missing');
});
