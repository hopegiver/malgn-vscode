// 홈 온보딩 체크리스트 판정 로직 — DOM 없는 순수 함수 모음.
//
// 새 수집 경로를 만들지 않는다. 이미 앱 state에 있는 값(state.devTools: 개발 도구
// 설치 상태 `check_dev_tools`, state.claudeAuth: `check_claude_auth_status`)만 읽어
// "설치(필수 도구) → 설치(Claude Code) → claude 로그인" 세 단계를 판정한다.
//
// 핵심 원칙: 데이터가 없거나(로딩) 조회가 실패(에러)한 상태를 "완료"나 "미완료"로
// 단정하지 않는다. 그 경우 단계 상태는 'loading' / 'error'로 따로 구분되고, 화면은
// 이를 "확인 중…" / "확인 불가"로 표시한다. 'todo'(미완료)와 'done'(완료)는 오직
// 조회가 성공했고 값이 명시적으로 그 상태일 때만 나온다.
import type { DevToolStatus } from './devToolsApi';
import type { ClaudeAuthStatus } from './sessionsApi';

export type StepKind = 'required-tools' | 'claude-install' | 'claude-login';

/** done/todo는 확정, loading/error는 판정 불가, blocked는 선행 단계 미완료로 아직 진행 불가. */
export type StepState = 'done' | 'todo' | 'loading' | 'error' | 'blocked';

export interface OnboardingStep {
  readonly kind: StepKind;
  readonly title: string;
  readonly state: StepState;
  /** 상태 한 줄 설명(미완료 사유, 확인 불가 사유 등). */
  readonly detail: string;
}

export interface OnboardingModel {
  readonly steps: readonly OnboardingStep[];
  readonly doneCount: number;
  /** 선행 단계가 모두 완료된 상태에서 처음으로 만나는 미완료(todo) 단계. 없으면 null. */
  readonly currentKind: StepKind | null;
  readonly allDone: boolean;
  /** 확정된 미완료(todo) 단계가 하나라도 있는가 — 있어야 전체 체크리스트를 펼쳐 보여준다. */
  readonly hasTodo: boolean;
  /** 판정 불가(loading/error) 단계가 하나라도 있는가. */
  readonly hasUnknown: boolean;
  /** 판정 불가 단계 중 에러가 있는가(없으면 단순 로딩 중). */
  readonly hasError: boolean;
}

/** 입력으로 쓰는 state의 최소 모양 — 테스트/하네스에서 구조적으로 만족시키기 쉽도록 좁게 잡았다. */
export interface OnboardingInput {
  readonly devTools: {
    readonly items: readonly DevToolStatus[];
    readonly loaded: boolean;
    readonly loading: boolean;
    readonly error: string | null;
  };
  readonly claudeAuth: {
    readonly status: ClaudeAuthStatus | null;
    readonly loaded: boolean;
    readonly loading: boolean;
    readonly error: string | null;
  };
}

export type ClaudeInstallState = 'installed' | 'missing' | 'unknown';

/**
 * Claude Code(claude CLI) 설치 여부. 'missing'은 개발 도구 조회가 성공했고 그 결과에
 * claude 항목이 있으며 installed===false이면서 실행 파일 경로(path) 자체가 없을 때만
 * 나온다. 경로는 있는데 버전 조회만 실패(타임아웃·빈 출력)해 installed=false인 경우는
 * 실제로 설치돼 있을 수 있으므로 'unknown'이다. 로딩·에러·항목 부재도 'unknown'이라
 * 호출 측이 기존 동작(로그인 버튼 유지)을 유지해야 한다.
 */
export function claudeInstallState(devTools: OnboardingInput['devTools']): ClaudeInstallState {
  if (devTools.error || !devTools.loaded) return 'unknown';
  const claude = devTools.items.find((t) => t.id === 'claude');
  if (!claude) return 'unknown';
  if (claude.installed) return 'installed';
  // path는 백엔드가 파일을 찾았을 때만 채운다(query.rs: 미발견 None). 경로가 있으면 확정 불가.
  return claude.path == null ? 'missing' : 'unknown';
}

function devToolsUnknown(devTools: OnboardingInput['devTools']): { state: 'loading' | 'error'; detail: string } | null {
  if (devTools.error) return { state: 'error', detail: '개발 도구 설치 상태를 확인하지 못했습니다.' };
  if (!devTools.loaded) return { state: 'loading', detail: '설치 상태 확인 중…' };
  return null;
}

function requiredToolsStep(devTools: OnboardingInput['devTools']): OnboardingStep {
  const title = '필수 도구 설치';
  const unknown = devToolsUnknown(devTools);
  if (unknown) return { kind: 'required-tools', title, ...unknown };
  if (devTools.items.length === 0) {
    return { kind: 'required-tools', title, state: 'error', detail: '개발 도구 목록이 비어 있어 설치 상태를 판정할 수 없습니다.' };
  }
  // Claude Code는 필수(required)지만 별도의 2단계(claude-install)가 전담한다 — 여기에도
  // 넣으면 같은 미설치가 두 단계에서 중복 미완료로 잡힌다.
  const missing = devTools.items.filter((t) => t.required && !t.installed && t.id !== 'claude');
  if (missing.length > 0) {
    return { kind: 'required-tools', title, state: 'todo', detail: `${missing.map((t) => t.name).join(', ')} 설치가 필요합니다.` };
  }
  return { kind: 'required-tools', title, state: 'done', detail: '필수 도구가 모두 설치되어 있습니다.' };
}

function claudeInstallStep(devTools: OnboardingInput['devTools']): OnboardingStep {
  const title = 'Claude Code 설치';
  const unknown = devToolsUnknown(devTools);
  if (unknown) return { kind: 'claude-install', title, ...unknown };
  const install = claudeInstallState(devTools);
  if (install === 'installed') {
    const claude = devTools.items.find((t) => t.id === 'claude');
    return { kind: 'claude-install', title, state: 'done', detail: claude?.version ? `v${claude.version}` : '설치되어 있습니다.' };
  }
  if (install === 'missing') {
    return { kind: 'claude-install', title, state: 'todo', detail: 'claude CLI가 설치되어 있지 않습니다.' };
  }
  const claude = devTools.items.find((t) => t.id === 'claude');
  if (claude) {
    return { kind: 'claude-install', title, state: 'error', detail: 'claude 실행 파일은 있지만 버전을 확인하지 못했습니다.' };
  }
  return { kind: 'claude-install', title, state: 'error', detail: '개발 도구 목록에서 Claude Code 항목을 찾지 못했습니다.' };
}

function claudeLoginStep(input: OnboardingInput, installState: ClaudeInstallState): OnboardingStep {
  const title = 'claude 로그인';
  const auth = input.claudeAuth;
  // 로그인됨이 명시적으로 확인되면 설치 여부와 무관하게 완료(실제로 인증된 증거가 우선).
  if (auth.loaded && !auth.error && auth.status?.loggedIn === true) {
    return { kind: 'claude-login', title, state: 'done', detail: auth.status.email ?? '로그인되어 있습니다.' };
  }
  // 설치가 확정적으로 안 된 상태에서는 로그인 상태 조회 결과(에러 포함)를 해석하지 않는다.
  if (installState === 'missing') {
    return { kind: 'claude-login', title, state: 'blocked', detail: 'Claude Code를 설치한 뒤 로그인할 수 있습니다.' };
  }
  if (auth.error) return { kind: 'claude-login', title, state: 'error', detail: '로그인 상태를 확인하지 못했습니다.' };
  if (!auth.loaded) return { kind: 'claude-login', title, state: 'loading', detail: '로그인 상태 확인 중…' };
  return { kind: 'claude-login', title, state: 'todo', detail: 'Anthropic 계정으로 로그인하세요.' };
}

export function computeOnboarding(input: OnboardingInput): OnboardingModel {
  const installState = claudeInstallState(input.devTools);
  const steps: OnboardingStep[] = [requiredToolsStep(input.devTools), claudeInstallStep(input.devTools), claudeLoginStep(input, installState)];

  // 현재 단계 = 앞선 단계가 모두 done인 상태에서 처음 만나는 todo. 앞에 판정 불가/블록이
  // 끼어 있으면 순서를 보장할 수 없으므로 현재 단계를 지정하지 않는다.
  let currentKind: StepKind | null = null;
  for (const s of steps) {
    if (s.state === 'done') continue;
    if (s.state === 'todo') currentKind = s.kind;
    break;
  }

  const doneCount = steps.filter((s) => s.state === 'done').length;
  return {
    steps,
    doneCount,
    currentKind,
    allDone: doneCount === steps.length,
    hasTodo: steps.some((s) => s.state === 'todo'),
    hasUnknown: steps.some((s) => s.state === 'loading' || s.state === 'error'),
    hasError: steps.some((s) => s.state === 'error'),
  };
}
