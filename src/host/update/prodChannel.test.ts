import { describe, expect, it } from 'vitest';
import { UPDATE_PUBLIC_KEYS } from '../../update/publicKeys.js';
import { UPDATE_SERVER_AUTHORITY } from '../../update/updateServerAuthority.js';
import { BundleIdentifierNotConfiguredError, PROD_BUNDLE_IDENTIFIER_ENV_VAR, assembleProdUpdateChannelConfig } from './prodChannel.js';

describe('assembleProdUpdateChannelConfig', () => {
  it('환경변수가 없으면 조용히 빈 값으로 넘어가지 않고 명시적으로 던진다', () => {
    expect(() => assembleProdUpdateChannelConfig({})).toThrow(BundleIdentifierNotConfiguredError);
  });

  it('환경변수가 있으면 코드 상수(authority·publicKeys) + 주입된 bundleIdentifier로 조립한다', () => {
    const config = assembleProdUpdateChannelConfig({ [PROD_BUNDLE_IDENTIFIER_ENV_VAR]: 'injected-by-packaging' });
    expect(config).toEqual({ authority: UPDATE_SERVER_AUTHORITY, publicKeys: UPDATE_PUBLIC_KEYS, bundleIdentifier: 'injected-by-packaging' });
  });
});
