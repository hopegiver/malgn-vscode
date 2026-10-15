// 자율 작업 사전조건 판정 — DOM 없는 순수 함수.
//
// 자율 작업은 로컬 `claude -p`를 실행하므로(src-tauri/src/autonomy/runner.rs) (a) claude CLI가
// 설치돼 있고 (b) 로그인돼 있어야 하고 (c) 조직 표준 플러그인(malgn-agent)이 있어야 서브에이전트·
// 스킬을 쓰는 작업이 돈다. 새 수집 경로를 만들지 않고 앱이 이미 들고 있는 state만 읽는다:
//   - 설치: state.devTools(check_dev_tools) — onboarding.ts의 claudeInstallState를 그대로 재사용
//   - 로그인: state.claudeAuth(check_claude_auth_status)
//   - 플러그인: state.catalog(list_installed_plugins)
//
// 핵심 원칙(onboarding.ts와 동일): 로딩 중이거나 조회가 실패한 상태를 "미설치/미로그인"으로
// 단정하지 않는다. 확정 미충족은 조회가 성공했고 값이 명시적으로 그 상태일 때만 나오고,
// 조회 실패는 별도의 'unknown' 안내로 구분한다. 로딩 중에는 아무것도 내지 않는다.
import { claudeInstallState } from './onboarding';
import type { OnboardingInput } from './onboarding';
import type { InstalledPlugin } from './catalogApi';

export type AutonomyPrereqKind = 'claude-missing' | 'login-required' | 'plugin-missing' | 'check-failed';

export interface AutonomyPrereqNotice {
  readonly kind: AutonomyPrereqKind;
  readonly message: string;
  readonly actionLabel: string;
  /** navigate()에 그대로 넘기는 해시 경로. */
  readonly actionRoute: string;
}

export interface AutonomyPrereqInput extends OnboardingInput {
  readonly catalog: {
    readonly plugins: readonly Pick<InstalledPlugin, 'id'>[];
    readonly loaded: boolean;
    readonly loading: boolean;
    readonly error: string | null;
  };
}

/** pluginId: 사전조건 플러그인 id — 정본은 views/catalog.ts DEFAULT_PLUGIN_ID(순수 모듈이 뷰를 import하지 않도록 호출부가 넘긴다). */
export function computeAutonomyPrereqs(input: AutonomyPrereqInput, pluginId: string): AutonomyPrereqNotice[] {
  const notices: AutonomyPrereqNotice[] = [];
  const install = claudeInstallState(input.devTools);
  const auth = input.claudeAuth;
  const catalog = input.catalog;

  // (a) claude 미설치 — 개발 도구 조회가 성공했고 실행 파일 경로조차 없을 때만 확정.
  if (install === 'missing') {
    notices.push({
      kind: 'claude-missing',
      message: 'Claude Code가 설치되어 있지 않아 자율 작업이 실행되지 않습니다.',
      actionLabel: 'Claude Code 설치하기',
      actionRoute: '#/settings/devtools',
    });
  }

  // (b) 미로그인 — 조회가 성공했고 loggedIn이 명시적으로 false일 때만. 미설치가 확정이면 로그인 이전
  // 문제이므로 중복 안내하지 않는다.
  const loginConfirmedOut = install !== 'missing' && auth.loaded && !auth.error && auth.status !== null && auth.status.loggedIn === false;
  if (loginConfirmedOut) {
    notices.push({
      kind: 'login-required',
      message: 'Claude에 로그인되어 있지 않아 자율 작업이 실행되지 않습니다.',
      actionLabel: '홈에서 로그인',
      actionRoute: '#/',
    });
  }

  // (c) 플러그인 없음 — 목록 조회가 성공했고 그 안에 조직 표준 플러그인이 없을 때만. 단 백엔드
  // list_installed_plugins는 홈 디렉터리 없음·JSON 파싱 실패·스키마 변경도 오류 없이 빈 목록으로
  // 돌려주므로 "설치되어 있지 않다"고 단정하지 않고 "목록에서 찾지 못했다"고만 말한다.
  if (catalog.loaded && !catalog.error && !catalog.plugins.some((p) => p.id === pluginId)) {
    const none = catalog.plugins.length === 0;
    notices.push({
      kind: 'plugin-missing',
      message: '설치된 플러그인 목록에서 malgn-agent 플러그인을 찾지 못했습니다. 서브에이전트·스킬을 쓰는 자율 작업이 실패할 수 있습니다.',
      actionLabel: '플러그인 설치하러 가기',
      // 설치 버튼이 있는 곳: 설치된 플러그인이 전혀 없으면 카탈로그 빈 상태, 그 외에는 MCP 설정의
      // malgnai-hub 안내 행(둘 다 같은 installPlugin 흐름).
      actionRoute: none ? '#/catalog' : '#/settings/mcp',
    });
  }

  // 판정 불가 — 조회 "실패"만 알린다(로딩 중은 침묵). 위 어느 상태도 단정하지 않는다.
  const failed: string[] = [];
  if (input.devTools.error) failed.push('Claude Code 설치 상태');
  if (auth.error && install !== 'missing') failed.push('Claude 로그인 상태');
  if (catalog.error) failed.push('malgn-agent 플러그인');
  if (failed.length > 0) {
    notices.push({
      kind: 'check-failed',
      message: `${failed.join(', ')} 조회에 실패해 자율 작업 실행 가능 여부를 판단할 수 없습니다. 홈 화면에서 다시 확인할 수 있습니다.`,
      actionLabel: '홈에서 확인',
      actionRoute: '#/',
    });
  }

  return notices;
}
