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
import type { IngestLinkStatus } from './otelApi';

export type StepKind = 'required-tools' | 'claude-install' | 'claude-login' | 'otel-link';

/**
 * done/todo는 확정, loading/error는 판정 불가, blocked는 선행 단계 미완료로 아직 진행 불가.
 * partial/failed는 otel-link 전용 — partial은 "수집키는 적용됐지만 수신은 확인 못 함"(완료가 아니다),
 * failed는 "연동 경로가 실패 원인과 함께 막힘"(판정 불가가 아니라 확정된 실패).
 */
export type StepState = 'done' | 'todo' | 'loading' | 'error' | 'blocked' | 'partial' | 'failed';

export interface OnboardingStep {
  readonly kind: StepKind;
  readonly title: string;
  readonly state: StepState;
  /** 상태 한 줄 설명(미완료 사유, 확인 불가 사유 등). */
  readonly detail: string;
  /**
   * true면 참고용 항목 — 진행률(doneCount/coreTotal)·allDone·currentKind·hasTodo·hasUnknown 판정에
   * 들어가지 않는다. 이 단계가 todo여도 이미 설치·로그인을 끝낸 사용자의 홈에 체크리스트가 다시 뜨지 않는다.
   */
  readonly advisory?: boolean;
}

export interface OnboardingModel {
  /** 화면에 그릴 행 전체(필수 3단계 + 있으면 참고 항목). */
  readonly steps: readonly OnboardingStep[];
  /** 진행률 분모 — advisory 항목을 뺀 필수 단계 수. */
  readonly coreTotal: number;
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
  /** 있으면 OTEL 연동 참고 항목을 만든다. 생략하면 항목 자체가 없다(autonomyPrereq 등 기존 입력 호환). */
  readonly ingestLink?: {
    readonly status: IngestLinkStatus | null;
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

// Rust가 내려주는 마스킹 표시값(`mhk_`+최대 4자+…)만 화면에 허용한다. 형식이 다르면(원문일 가능성 포함)
// 아예 표시하지 않는다.
const MASKED_KEY_SHAPE = /^mhk_[A-Za-z0-9_-]{1,4}…$/;
function safeMasked(masked: string | null): string | null {
  return masked !== null && MASKED_KEY_SHAPE.test(masked) ? masked : null;
}

/**
 * OTEL 연동 참고 항목. 자동 발급·수신 확인은 아직 구현되지 않았으므로(issueSupported /
 * receiptCheckSupported = false) "자동 연동"·"연동 완료"를 말하지 않는다. 지금 실제로 도달하는 상태는
 * unlinked / applied / failed 뿐이며 issuing / confirmed는 타입상으로만 처리한다.
 */
function otelLinkStep(link: NonNullable<OnboardingInput['ingestLink']>): OnboardingStep {
  const base = { kind: 'otel-link' as const, title: 'OTEL 연동(사용량 수집키)', advisory: true };
  if (link.error) {
    return { ...base, state: 'error', detail: `수집키 적용 상태를 확인하지 못했습니다: ${link.error}` };
  }
  const status = link.status;
  if (!link.loaded || !status) return { ...base, state: 'loading', detail: '수집키 적용 상태 확인 중…' };
  const masked = safeMasked(status.masked);
  const keyTag = masked ? ` (${masked})` : '';
  switch (status.state) {
    case 'unlinked':
      return {
        ...base,
        state: 'todo',
        detail:
          '수집키가 적용되어 있지 않습니다. 설정 > OTel에서 허브 관리자에게 받은 수집키를 직접 붙여넣어 저장하세요. 허브 웹에 수집키 발급 화면이 아직 없어 발급 방법은 허브 관리자에게 문의해야 하며, 허브 연결의 기기 토큰은 수집키가 아닙니다.' +
          (status.issueSupported ? '' : ' 자동 발급은 아직 지원되지 않습니다.'),
      };
    case 'issuing':
      return { ...base, state: 'loading', detail: '수집키를 발급받아 적용하는 중…' };
    case 'applied':
      return {
        ...base,
        state: 'partial',
        detail: `수집키가 적용되었습니다${keyTag}. 수신은 아직 확인하지 않았습니다. 새로 시작하는 Claude Code 세션부터 적용됩니다.`,
      };
    case 'confirmed':
      return { ...base, state: 'done', detail: `수집키가 적용되었고 허브에서 수신이 확인되었습니다${keyTag}.` };
    case 'failed': {
      const msg = status.failure?.message ?? '수집키 적용 상태에 문제가 있습니다.';
      return { ...base, state: 'failed', detail: masked ? `${msg} 적용된 수집키${keyTag}.` : msg };
    }
  }
}

export function computeOnboarding(input: OnboardingInput): OnboardingModel {
  const installState = claudeInstallState(input.devTools);
  const core: OnboardingStep[] = [requiredToolsStep(input.devTools), claudeInstallStep(input.devTools), claudeLoginStep(input, installState)];
  const steps: OnboardingStep[] = input.ingestLink ? [...core, otelLinkStep(input.ingestLink)] : core;

  // 현재 단계 = 앞선 단계가 모두 done인 상태에서 처음 만나는 todo. 앞에 판정 불가/블록이
  // 끼어 있으면 순서를 보장할 수 없으므로 현재 단계를 지정하지 않는다.
  let currentKind: StepKind | null = null;
  for (const s of core) {
    if (s.state === 'done') continue;
    if (s.state === 'todo') currentKind = s.kind;
    break;
  }

  // 진행률·전체 판정은 필수 단계(core)만 본다 — advisory 항목은 여기에 영향을 주지 않는다.
  const doneCount = core.filter((s) => s.state === 'done').length;
  return {
    steps,
    coreTotal: core.length,
    doneCount,
    currentKind,
    allDone: doneCount === core.length,
    hasTodo: core.some((s) => s.state === 'todo'),
    hasUnknown: core.some((s) => s.state === 'loading' || s.state === 'error'),
    hasError: core.some((s) => s.state === 'error'),
  };
}
