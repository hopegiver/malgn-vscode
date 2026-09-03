// docs/policy-contract.md §8.4 — fail-closed 3종의 셋째: `siteProfile !== 'site'`이면
// apply를 거부하고(런타임) 패키징도 실패시킨다(빌드/패키징 시점).
//
// apply() 오케스트레이션 본체는 아직 없다(engine.ts 주석 참고 — W3/W7+ 범위). 이 모듈은
// 그 오케스트레이션이 반드시 통과해야 할 게이트를 순수 함수로 미리 고정해 둔다 —
// "언젠가 apply()가 생기면 이 함수를 통과시킨다"가 아니라 "이 함수가 없으면 apply 경로
// 자체가 컴파일되지 않는다"가 되도록, W7+가 이 가드를 호출하는 것이 계약이다.

import type { SiteProfile } from './types.js';

export class SiteProfileNotSiteError extends Error {
  constructor(public readonly siteProfile: SiteProfile, public readonly action: 'apply' | 'package') {
    super(
      `siteProfile이 'site'가 아니라(${siteProfile}) ${action === 'apply' ? 'apply를 거부합니다' : '패키징을 실패시킵니다'} (docs/policy-contract.md §8.4)`
    );
    this.name = 'SiteProfileNotSiteError';
  }
}

/**
 * apply 직전 호출 지점 — `siteProfile !== 'site'`면 던진다(fail-closed). 예시면으로
 * 빌드된 확장이 실제로 어떤 변경도 적용하지 못하게 하는 마지막 방어선이다.
 */
export function assertSiteProfileForApply(siteProfile: SiteProfile): void {
  if (siteProfile !== 'site') {
    throw new SiteProfileNotSiteError(siteProfile, 'apply');
  }
}

/**
 * 패키징(.vsix) 직전 호출 지점 — `scripts/assert-site-profile-for-packaging.mjs`가 이
 * 로직과 동일한 판정을 CLI 종료 코드로 재현한다(그 스크립트는 패키징 파이프라인이
 * TypeScript를 실행하지 못하는 자리에서 쓰인다 — 판정 자체는 여기 한 곳이 정본이다).
 */
export function assertSiteProfileForPackaging(siteProfile: SiteProfile): void {
  if (siteProfile !== 'site') {
    throw new SiteProfileNotSiteError(siteProfile, 'package');
  }
}
