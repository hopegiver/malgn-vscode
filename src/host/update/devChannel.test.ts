import { describe, expect, it } from 'vitest';
import { DEV_UPDATE_HOST, DEV_UPDATE_PUBLIC_KEYS } from '../../update/devUpdateChannel.js';
import { BundleIdentifierNotConfiguredError } from './prodChannel.js';
import { DEV_BUNDLE_IDENTIFIER_ENV_VAR, assembleDevUpdateChannelConfig } from './devChannel.js';

describe('assembleDevUpdateChannelConfig', () => {
  it('환경변수가 없으면 명시적으로 던진다', () => {
    expect(() => assembleDevUpdateChannelConfig({})).toThrow(BundleIdentifierNotConfiguredError);
  });

  it('개발 채널 상수(devUpdateChannel.ts)를 쓴다 — 운영 상수와 다른 식별자', () => {
    const config = assembleDevUpdateChannelConfig({ [DEV_BUNDLE_IDENTIFIER_ENV_VAR]: 'dev-bundle-id' });
    expect(config).toEqual({ authority: DEV_UPDATE_HOST, publicKeys: DEV_UPDATE_PUBLIC_KEYS, bundleIdentifier: 'dev-bundle-id' });
  });
});
