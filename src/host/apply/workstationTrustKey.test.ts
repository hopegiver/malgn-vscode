// `wireApplyMenu.ts`는 Electron `Tray`/`Menu`/`powerMonitor`를 직접 다루는 얇은
// 어댑터라(`trayAdapter.ts`·`trustDialogAdapter.ts`와 같은 부류) vitest 단위 테스트
// 대상이 아니다(그 파일들의 기존 관례와 동일 — 실제 검증은 Electron 실행으로 한다).
// 순수 함수는 `workstationTrustKey.ts`로 분리해 여기서 독립적으로 검증한다(그 파일을
// import하면 `wireApplyMenu.ts`가 끌고 오는 `electron` 모듈까지 함께 로드되어 vitest
// 환경에서 깨질 위험이 생긴다 — `Tray`/`Menu`는 vitest에서 실행되는 순수 Node 환경에
// 없다).

import { describe, expect, it } from 'vitest';
import { resolveWorkstationTrustKey } from './workstationTrustKey.js';

describe('resolveWorkstationTrustKey', () => {
  it('홈 디렉터리 경로를 그대로 키로 쓴다(이 PC를 가리키는 안정적인 단일 항목)', () => {
    expect(resolveWorkstationTrustKey('/Users/dev')).toBe('/Users/dev');
  });
});
