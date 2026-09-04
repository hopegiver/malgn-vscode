// A-33(F-5, security-report.md §2) — findSecretLikeKeyPath(SECRET_KEY_RE) 회귀.
// 원래 정규식(/token|secret|password|authorization/i)은 apiKey·apiToken은 `token`
// 부분 매칭으로 잡았지만 credential·privateKey·passwd·pwd·bearer·passphrase는 놓쳤다
// — PR-5(정책 무비밀)의 유일한 파일 전체 거부급 검사인데 그릇 이름만 바꾸면 통과했다.

import { describe, expect, it } from 'vitest';
import { findSecretLikeKeyPath } from './sinkGuards.js';

describe('findSecretLikeKeyPath — A-33(F-5) 확장분 검출', () => {
  it.each([
    ['credential', { otel: { credential: 'CHANGEME' } }],
    ['privateKey', { otel: { privateKey: 'CHANGEME' } }],
    ['private_key', { otel: { private_key: 'CHANGEME' } }],
    ['passwd', { db: { passwd: 'CHANGEME' } }],
    ['pwd', { db: { pwd: 'CHANGEME' } }],
    ['bearer', { auth: { bearer: 'CHANGEME' } }],
    ['passphrase', { pgp: { passphrase: 'CHANGEME' } }],
  ])('키 이름 "%s"가 있으면 경로를 반환한다(파일 전체 거부 신호)', (_label, obj) => {
    expect(findSecretLikeKeyPath(obj)).not.toBeNull();
  });

  it('기존에 이미 잡던 형태(token/secret/password/authorization)는 계속 잡는다(회귀 방지)', () => {
    expect(findSecretLikeKeyPath({ apiToken: 'CHANGEME' })).not.toBeNull();
    expect(findSecretLikeKeyPath({ secret: 'CHANGEME' })).not.toBeNull();
    expect(findSecretLikeKeyPath({ password: 'CHANGEME' })).not.toBeNull();
    expect(findSecretLikeKeyPath({ authorization: 'CHANGEME' })).not.toBeNull();
  });

  it('비밀 형태의 키 이름이 전혀 없으면 null이다(오탐 없음)', () => {
    expect(
      findSecretLikeKeyPath({
        schemaVersion: 1,
        agent: { marketplace: 'malgnsoft/claude-plugins', plugin: 'malgn-agent@malgnsoft-plugins' },
        otel: { headersHelper: { kind: 'keychain-basic', service: 'example-service', account: 'example-service' } },
      })
    ).toBeNull();
  });
});
