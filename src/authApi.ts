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

// 로컬 개발 전용 자동 로그인 — src-tauri/src/dev_auto_login.rs 참고. 디버그 빌드 +
// src-tauri/.env의 DEV_AUTO_LOGIN_EMAIL이 `@malgnsoft.com` 도메인으로 설정된
// 경우에만 값을 반환한다(그 외 도메인은 Rust 쪽에서 거부되어 None). 릴리스
// 빌드에서는 Rust 커맨드 자체가 컴파일 타임에 항상 None만 반환하도록 고정되어
// 있으므로(어떤 입력으로도 우회 불가) 여기서 빌드 프로필을 따로 분기하지 않아도
// 안전하다. Tauri IPC 브리지가 없는 환경(플레인 브라우저 미리보기 등)에서는
// invoke 자체가 실패할 수 있으므로 그 경우도 조용히 null로 처리해 기존 로그인
// 화면으로 자연스럽게 이어지게 한다. 호출부(main.ts)가 `import.meta.env.DEV`
// 가드로 감싸므로, 이 함수는 프로덕션 번들에서는 아예 호출되지 않는다(F7).
export interface DevAutoLoginResult {
  readonly email: string;
  readonly name: string;
}

export async function tryDevAutoLogin(): Promise<DevAutoLoginResult | null> {
  try {
    return await invoke<DevAutoLoginResult | null>('dev_auto_login');
  } catch (e) {
    // fail-closed 동작은 그대로 유지한다(모든 실패를 null로 흡수해 기존 로그인
    // 화면으로 자연스럽게 이어지게 한다) — 다만 왜 자동 로그인이 안 됐는지 알
    // 신호가 전혀 없으면 디버깅이 막히므로 디버그 콘솔에만 남긴다.
    console.debug('tryDevAutoLogin failed, falling back to normal login', e);
    return null;
  }
}
