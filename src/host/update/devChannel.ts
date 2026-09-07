// 개발 채널 `UpdateChannelConfig` 조립 — `prodChannel.ts`와 대칭이다. `src/update/
// devUpdateChannel.ts`의 별도 상수(`DEV_UPDATE_HOST`·`DEV_UPDATE_PUBLIC_KEYS`)를 쓴다 —
// production 상수와 같은 식별자를 조건부로 고르지 않는다(U-3, "한 바이너리 안의
// 분기" 금지).
//
// 번들 식별자 값을 하드코딩하지 않는 이유는 `prodChannel.ts` 헤더 주석과 같다.

import { DEV_UPDATE_HOST, DEV_UPDATE_PUBLIC_KEYS } from '../../update/devUpdateChannel.js';
import type { UpdateChannelConfig } from '../../update/updateFlow.js';
import { BundleIdentifierNotConfiguredError } from './prodChannel.js';

export const DEV_BUNDLE_IDENTIFIER_ENV_VAR = 'MALGN_DEV_BUNDLE_IDENTIFIER';

export function assembleDevUpdateChannelConfig(env: Readonly<Record<string, string | undefined>>): UpdateChannelConfig {
  const bundleIdentifier = env[DEV_BUNDLE_IDENTIFIER_ENV_VAR];
  if (!bundleIdentifier) {
    throw new BundleIdentifierNotConfiguredError(DEV_BUNDLE_IDENTIFIER_ENV_VAR);
  }
  return { authority: DEV_UPDATE_HOST, publicKeys: DEV_UPDATE_PUBLIC_KEYS, bundleIdentifier };
}
