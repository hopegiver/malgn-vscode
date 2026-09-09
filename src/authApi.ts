// 로그인 — 이 앱에서 유일하게 "쓰기/실행"에 해당하는 보안 기능이다. Rust 커맨드
// google_oauth_login()이 PKCE + 로컬 루프백 리스너로 RFC 8252 데스크톱 앱 OAuth
// 플로우 전체(브라우저 열기 → 콜백 수신 → 토큰 교환 → Google JWKS로 서명 검증 →
// hd/aud/iss/exp/email_verified 검증)를 수행하고, malgnsoft.com Google Workspace
// 계정이 아니면 거부한다 — src-tauri/src/lib.rs 주석 참고. Client ID가 아직
// 비어있는(TODO) 동안은 브라우저를 열지 않고 즉시 에러를 반환한다.
import { invoke } from '@tauri-apps/api/core';

export interface GoogleLoginResult {
  readonly email: string;
  readonly name: string;
  readonly hd: string;
}

export async function loginWithGoogle(): Promise<GoogleLoginResult> {
  return invoke<GoogleLoginResult>('google_oauth_login');
}
