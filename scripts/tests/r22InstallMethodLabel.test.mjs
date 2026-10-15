// src/devToolsText.ts installMethodLabel 단위 테스트 — 앱이 사용자 공간에 설치한 Node(R22 P2-3c)가
// 별도 설치방식("맑은에이전트 설치")으로 보이고, 그 외 코드는 원문 그대로 보이는지 고정한다.
import test from 'node:test';
import assert from 'node:assert/strict';

import { installMethodLabel } from '../../src/devToolsText.ts';

test('r22 P2-3c: 앱이 설치한 Node는 "맑은에이전트 설치"로 표시한다(코드 단독·버전 접미 모두)', () => {
  assert.equal(installMethodLabel('appManagedNode'), '맑은에이전트 설치');
  assert.equal(installMethodLabel('appManagedNode(v24.22.0)'), '맑은에이전트 설치');
});

test('r22 P2-3c: 다른 설치방식 코드는 지어내 번역하지 않고 원문 그대로 보인다', () => {
  for (const raw of ['homebrewFormula(node)', 'npmGlobal(pnpm)', 'wingetPackage', 'claudeNative', 'unknown(/x/y)', 'appManagedNodeX']) {
    assert.equal(installMethodLabel(raw), raw);
  }
});

test('r22 P2-3c: 값이 없으면 null(행에 표시하지 않음)', () => {
  assert.equal(installMethodLabel(null), null);
  assert.equal(installMethodLabel(undefined), null);
  assert.equal(installMethodLabel(''), null);
});
