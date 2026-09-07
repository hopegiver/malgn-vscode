import { describe, expect, it } from 'vitest';
import { DEV_APP_BUNDLE_ID, DEV_APP_PRODUCT_NAME } from './package-dev-app.mjs';

describe('package-dev-app 상수', () => {
  it('productName이 "Dev"/"Local Only"를 포함해 배포 불가임을 이름 자체로 드러낸다', () => {
    expect(DEV_APP_PRODUCT_NAME).toMatch(/Dev/);
    expect(DEV_APP_PRODUCT_NAME).toMatch(/Local Only/);
  });

  it('bundle id가 실 릴리스 식별자와 겹치지 않는다(devlocal 접미사)', () => {
    expect(DEV_APP_BUNDLE_ID).toMatch(/devlocal$/);
  });
});
