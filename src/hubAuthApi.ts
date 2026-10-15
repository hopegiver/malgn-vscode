// 허브 연결(기기 토큰) — Rust 커맨드 hub_auth_* 래퍼 (설계 docs/design/device-token-auth.md §7).
//
// 이 파일의 어떤 타입에도 토큰 필드가 없다. 토큰 원문은 Rust(키체인)에서만 다루고 WebView로
// 넘어오지 않는다 — 원격 데이터를 렌더하는 WebView가 XSS를 당해도 `invoke`로 토큰을 얻을
// 수 없게 하는 계약이다(§7.1). 커맨드가 돌려주는 것은 상태 메타데이터뿐이다.
// 새 필드를 추가할 때 토큰이나 그 일부를 담는 필드를 만들지 말 것.
import { invoke } from '@tauri-apps/api/core';

// Rust state.rs HubState (SCREAMING_SNAKE_CASE).
export type HubAuthState =
  | 'NOT_CONNECTED'
  | 'CONNECTED'
  | 'RENEW_DUE'
  | 'RENEW_RETRYING'
  | 'SAVE_FAILED'
  | 'EXPIRED'
  | 'REVOKED'
  | 'KEYCHAIN_DENIED';

export interface HubAuthStatus {
  readonly connected: boolean;
  readonly state: HubAuthState;
  readonly device_name: string | null;
  readonly expires_at: string | null; // ISO 8601
  readonly renew_after: string | null; // ISO 8601
  // 사유·안내 코드. 문구는 hubAuthReasonText()가 코드로 고른다(서버 본문 텍스트는 오지 않는다).
  readonly reason: string | null;
}

export function fetchHubAuthStatus(): Promise<HubAuthStatus> {
  return invoke<HubAuthStatus>('hub_auth_status');
}

// 브라우저가 열리고 사용자가 허용할 때까지(최대 5분) 기다린다.
export function connectHub(): Promise<HubAuthStatus> {
  return invoke<HubAuthStatus>('hub_auth_connect');
}

// 연결 해제 때 이 PC settings.json의 수집키(Authorization 항목)를 어떻게 했는가 — 해제 토스트가 실제 결과를 말한다.
// removed: 앱이 적용한 키를 지웠다 / notPresent: 지울 키가 없어 파일을 바꾸지 않았다 / failed: 지우려 했지만 쓰지 못했다.
export type IngestKeyRemoval = 'removed' | 'notPresent' | 'failed';

export interface HubDisconnectResult extends HubAuthStatus {
  readonly ingest_key?: IngestKeyRemoval;
}

export function disconnectHub(): Promise<HubDisconnectResult> {
  return invoke<HubDisconnectResult>('hub_auth_disconnect');
}

// 갱신 창과 무관하게 서버에 물어본다(서버가 시기를 교정한다). 키체인 재시도에도 쓴다.
export function renewHubNow(): Promise<HubAuthStatus> {
  return invoke<HubAuthStatus>('hub_auth_renew_now');
}

const REASON_TEXT: Readonly<Record<string, string>> = {
  ALREADY_RENEWED: '보안상 연결이 해제되었습니다. 다시 연결하세요. 이 안내가 반복되면 관리자에게 알리세요.',
  UNAUTHORIZED: '허브에서 이 기기의 연결이 해제되었거나 계정 상태가 바뀌었습니다. 다시 연결하세요.',
  NOT_RENEWABLE: '이 연결은 자동 갱신할 수 없습니다. 다시 연결하세요.',
  EXPIRED: '허브 토큰이 만료되었습니다. 다시 연결하세요.',
  CORRUPT: '저장된 연결 정보가 손상되었습니다. 다시 연결하세요.',
  DENIED: '키체인 접근이 거부되었습니다. 키체인 잠금을 풀고 [다시 시도]하세요. 계속되면 앱을 재시작하고 접근 허용 대화상자에서 "항상 허용"을 선택하세요.',
  KEYCHAIN_ERROR: '키체인 오류가 발생했습니다. [다시 시도]해도 계속되면 IT/개발팀에 문의하세요.',
  SERVER_REVOKE_FAILED: '이 PC의 연결은 해제했지만 허브 서버에서 폐기하지 못했습니다. 웹 /keys 화면에서 해당 기기를 폐기하세요.',
};

export function daysUntil(iso: string | null, now: Date = new Date()): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return Math.max(0, Math.ceil((t - now.getTime()) / 86_400_000));
}

export function formatHubDate(iso: string | null): string {
  if (!iso) return '-';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '-' : d.toLocaleDateString('ko-KR');
}

export interface HubAuthView {
  readonly label: string; // 배지 문구
  readonly tone: 'active' | 'archived' | 'unknown';
  readonly detail: string; // 한 줄 설명
}

// 상태 → 화면 문구(설계 §7.2 표). 순수 함수라 테스트하기 쉽다.
export function describeHubAuth(status: HubAuthStatus | null, now: Date = new Date()): HubAuthView {
  if (!status) return { label: '확인 중', tone: 'unknown', detail: '허브 연결 상태를 확인하는 중…' };
  const reason = status.reason ? REASON_TEXT[status.reason] : undefined;
  const expiry = status.expires_at ? `만료일 ${formatHubDate(status.expires_at)}` : '';
  const device = status.device_name ? `${status.device_name} · ` : '';
  switch (status.state) {
    case 'CONNECTED':
    case 'RENEW_DUE':
      return { label: '연결됨', tone: 'active', detail: `${device}${expiry}` };
    case 'RENEW_RETRYING': {
      const d = daysUntil(status.expires_at, now);
      return { label: '갱신 대기 중', tone: 'unknown', detail: d === null ? '갱신 대기 중' : `갱신 대기 중(만료 D-${d})` };
    }
    case 'SAVE_FAILED':
      return {
        label: '저장 실패',
        tone: 'archived',
        detail: '키체인 저장에 실패했습니다. 앱을 끄거나 다시 시작하면 연결이 해제되어 다시 연결해야 합니다.',
      };
    case 'EXPIRED':
      return { label: '만료됨', tone: 'archived', detail: reason ?? REASON_TEXT.EXPIRED };
    case 'REVOKED':
      return { label: '연결 해제됨', tone: 'archived', detail: reason ?? '다시 연결하세요.' };
    case 'KEYCHAIN_DENIED':
      return { label: '키체인 거부', tone: 'archived', detail: reason ?? REASON_TEXT.DENIED };
    case 'NOT_CONNECTED':
    default:
      return {
        label: '미연결',
        tone: 'archived',
        detail: reason ?? '맑은에이전트 앱에서 허브에 연결하면 Claude Code 세션마다 다시 로그인하지 않아도 됩니다.',
      };
  }
}
