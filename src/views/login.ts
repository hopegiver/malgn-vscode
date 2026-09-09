// 로그인 화면 — 실제 Google OAuth(PKCE, malgnsoft.com Google Workspace 계정만
// 허용)로 전환됐다. 버튼을 누르면 Rust가 시스템 브라우저를 열고 RFC 8252 데스크톱
// 앱 플로우 전체를 처리한다(authApi.ts/src-tauri/src/lib.rs 참고). Client ID가
// 아직 비어있는 동안은 브라우저를 열지 않고 명확한 에러만 보여준다.
import { el } from '../dom';
import { state, notifyChange } from '../state';
import { loginWithGoogle } from '../authApi';

async function handleGoogleLogin(): Promise<void> {
  state.auth.loading = true;
  state.auth.error = null;
  notifyChange();
  try {
    const result = await loginWithGoogle();
    state.auth.userEmail = result.email;
    state.auth.userName = result.name;
    state.authenticated = true;
    window.location.hash = '#/';
  } catch (err) {
    state.auth.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.auth.loading = false;
    notifyChange();
  }
}

export function renderLoginView(): HTMLElement {
  const card = el('div', { className: 'login-card' }, [
    el('div', { className: 'login-brand' }, [el('span', { className: 'sidebar-brand-mark' }, ['M']), '맑은에이전트']),
    el('h1', { className: 'login-title' }, ['워크스테이션 프로비저닝']),
    el('div', { className: 'login-desc' }, ['Google Workspace 계정으로 로그인합니다.']),
    ...(state.auth.error ? [el('div', { className: 'login-error' }, [`⚠ ${state.auth.error}`])] : []),
    el(
      'button',
      { className: 'btn btn-primary login-btn', disabled: state.auth.loading, onClick: () => void handleGoogleLogin() },
      [state.auth.loading ? '브라우저에서 로그인 대기 중…' : 'Google 계정으로 로그인']
    ),
    el('div', { className: 'login-note' }, ['malgnsoft.com 조직 계정만 허용됩니다.']),
  ]);
  return el('div', { className: 'login-screen' }, [card]);
}
