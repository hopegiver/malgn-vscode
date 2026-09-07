import { describe, expect, it } from 'vitest';
import { DEV_UPDATE_HOST, DEV_UPDATE_PUBLIC_KEYS } from './devUpdateChannel.js';
import { UPDATE_SERVER_AUTHORITY } from './updateServerAuthority.js';
import { UPDATE_PUBLIC_KEYS } from './publicKeys.js';

describe('개발 채널 상수 — production과 별도 키·상수(U-3 dev 분기, 번들 식별자는 W-N1/W14 패키징 소유)', () => {
  it('개발 채널 authority는 production authority와 다른 값이다', () => {
    expect(DEV_UPDATE_HOST).not.toBe(UPDATE_SERVER_AUTHORITY);
  });

  it('개발 채널 공개키도 K-3 미발급 상태를 그대로 반영해 비어 있다(전면 거부 유지)', () => {
    expect(DEV_UPDATE_PUBLIC_KEYS).toEqual([]);
  });

  it('개발 채널과 production 채널의 공개키 배열은 서로 다른 인스턴스다(같은 배열을 공유하지 않는다)', () => {
    expect(DEV_UPDATE_PUBLIC_KEYS).not.toBe(UPDATE_PUBLIC_KEYS);
  });
});
