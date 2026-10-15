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
import type { IngestLinkFailureCause, IngestLinkStatus, ReadinessBlocker } from './otelApi';

export type StepKind = 'required-tools' | 'claude-install' | 'claude-login' | 'otel-link';

/**
 * done/todo는 확정, loading/error는 판정 불가, blocked는 선행 단계 미완료로 아직 진행 불가.
 * partial/failed/warn은 otel-link 전용 — partial은 "수집키는 적용됐고 수집 준비도 돼 있으나 수신은 확인 못 함"(완료가 아니다),
 * warn은 "수집키는 적용됐지만 수집 설정(텔레메트리·exporter·프로토콜·엔드포인트) 문제로 수집되지 않을 수 있음",
 * failed는 "연동 경로가 실패 원인과 함께 막힘"(판정 불가가 아니라 확정된 실패).
 */
export type StepState = 'done' | 'todo' | 'loading' | 'error' | 'blocked' | 'partial' | 'failed' | 'warn';

/**
 * otel-link 행의 버튼이 하는 일. 문구는 home.ts가 상태와 함께 고른다.
 * open-settings: 설정 > OTel 열기 / retry: [다시 시도] / connect-hub: 허브 (재)연결 화면 열기 /
 * confirm-reissue: 재발급 확인창 열기.
 */
export type StepAction = 'open-settings' | 'retry' | 'connect-hub' | 'confirm-reissue';

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
  /**
   * 필수 3단계가 모두 끝난 홈에서 체크리스트 박스 대신 한 줄(.onboard-slim)로 보여 줄 문구. 있는 항목만 그렇게
   * 노출한다(미적용·실패·수집 불가 경고). 적용·수집 준비 완료, 로딩, 조회 실패는 없다 — 방해하지 않는다.
   */
  readonly slim?: string;
  /** otel-link 행의 버튼 동작. 없으면 버튼이 없다. */
  readonly action?: StepAction;
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

const SESSION_NOTE = '새로 시작하는 Claude Code 세션부터 적용됩니다.';

// 앱이 실제로 아는 것은 "settings.json에 …이 없음"까지다 — 셸 환경변수로 켠 사용자는 틀린 단정을 보지 않도록
// 문구는 파일 기준임을 밝힌다(N-6).
const BLOCKER_TEXT: Readonly<Record<ReadinessBlocker, string>> = {
  telemetryOff: '설정 파일에 텔레메트리 켜기 설정이 없음',
  exporterNotOtlp: '설정 파일에서 metrics·logs 내보내기가 otlp로 지정되어 있지 않음',
  protocolMismatch: '설정 파일의 프로토콜이 http/protobuf가 아님',
  endpointMissing: '설정 파일에 수집 엔드포인트가 없음',
  endpointNotHub: '수집 엔드포인트가 허브가 아님',
  endpointForeign: '설정 파일에 허브 밖 엔드포인트가 함께 있어 수집키가 그쪽으로도 전송됨',
  endpointInsecure: '수집 엔드포인트가 암호화되지 않은 http임',
};

/** 수집이 안 될 수 있는 원인 목록 문구 — 백엔드가 준 식별자만으로 만든다(알 수 없는 값은 일반 문구). */
function blockerTexts(blockers: readonly ReadinessBlocker[]): string[] {
  return blockers.map((b) => BLOCKER_TEXT[b] ?? '수집 설정을 확인해야 함');
}

function formatRetryAt(iso: string | null): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return new Date(t).toLocaleString('ko-KR', { month: 'numeric', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

const BLOCKED_REASON_TEXT = { suspectedLeak: '유출 의심', quota: '사용량 한도 초과' } as const;

type OtelLink = NonNullable<OnboardingInput['ingestLink']>;

/** 허브에 연결되어 있지 않다 — 키가 없고 자동 발급도 시작할 수 없다. */
function notConnectedStep(base: StepBase): OnboardingStep {
  return {
    ...base,
    state: 'todo',
    action: 'connect-hub',
    detail:
      '허브에 연결하면 사용량 수집키가 자동으로 발급·적용됩니다. 지금은 허브에 연결되어 있지 않습니다. 직접 받은 수집키가 있으면 설정 > OTel에서 붙여넣을 수도 있습니다.',
    slim: '선택 · 허브에 연결하면 사용량 수집키가 자동으로 적용됩니다.',
  };
}

type StepBase = { readonly kind: 'otel-link'; readonly title: string; readonly advisory: true };

function unlinkedStep(base: StepBase, status: IngestLinkStatus): OnboardingStep {
  if (!status.hubConnected) return notConnectedStep(base);
  if (status.confirmRequired) {
    return {
      ...base,
      state: 'todo',
      action: 'confirm-reissue',
      detail: '웹에서 이 기기 수집키가 폐기되었습니다. 다시 발급할까요? 확인하면 새 수집키를 발급해 적용합니다.',
      slim: '선택 · 웹에서 폐기된 이 기기 수집키를 다시 발급할지 확인이 필요합니다.',
    };
  }
  if (status.confirmDeclined) {
    return {
      ...base,
      state: 'todo',
      action: 'retry',
      detail: '수집키 재발급을 거절했습니다. 이번 앱 실행 동안은 다시 묻지 않습니다. 다시 발급하려면 [다시 시도]를 누르세요.',
      slim: '선택 · 사용량 수집키가 적용되어 있지 않습니다(재발급 거절).',
    };
  }
  return {
    ...base,
    state: 'todo',
    action: 'retry',
    detail:
      '허브에 연결되어 있습니다. 앱이 사용량 수집키를 자동으로 발급·적용합니다(앱 시작 때와 1시간 간격으로 점검). 바로 하려면 [다시 시도]를 누르세요.',
    slim: '선택 · 사용량 수집키가 아직 적용되어 있지 않습니다.',
  };
}

function appliedStep(base: StepBase, status: IngestLinkStatus): OnboardingStep {
  const masked = safeMasked(status.masked);
  const keyTag = masked ? ` (${masked})` : '';
  // 수집 준비가 안 됐으면 "적용됨"만으로 수집될 거라고 믿게 하지 않는다. 수신 확인 여부는 state 그대로 따른다.
  if (!status.collectionReady) {
    const texts = blockerTexts(status.blockers);
    const why = texts.length > 0 ? texts.join(', ') : '수집 설정을 확인해야 함';
    return {
      ...base,
      state: 'warn',
      action: 'open-settings',
      detail: `수집키는 적용되었지만${keyTag} 사용량이 수집되지 않을 수 있습니다: ${why}.${status.state === 'confirmed' ? '' : ' 수신은 확인하지 않았습니다.'}`,
      slim: `선택 · 수집키는 적용됐지만 사용량이 수집되지 않을 수 있습니다 — ${why}.`,
    };
  }
  if (status.state === 'confirmed') {
    return { ...base, state: 'done', detail: `수집키가 적용되었고 허브에서 데이터 수신이 확인되었습니다${keyTag}.` };
  }
  if (status.waitingFirstData) {
    return { ...base, state: 'partial', detail: `수집키가 적용되었습니다${keyTag}. 첫 데이터 수신을 확인하는 중입니다(최대 15분). ${SESSION_NOTE}` };
  }
  if (status.firstDataTimedOut) {
    return {
      ...base,
      state: 'partial',
      detail: `수집키가 적용되었습니다${keyTag}. 15분 안에 수신이 확인되지 않았습니다. 새 Claude Code 세션을 열면 수집이 시작됩니다. 다음 앱 시작 때 다시 확인합니다.`,
    };
  }
  return { ...base, state: 'partial', detail: `수집키가 적용되었습니다${keyTag}. 수신은 아직 확인하지 않았습니다. ${SESSION_NOTE}` };
}

// 원인별 버튼. rotateBudgetExceeded는 서버가 정한 시각까지 소용이 없어 버튼을 두지 않는다.
const FAILURE_ACTION: Readonly<Record<IngestLinkFailureCause, StepAction | null>> = {
  settingsUnreadable: 'open-settings',
  headerUnrecognized: 'open-settings',
  endpointNotHub: 'open-settings',
  notReady: 'open-settings',
  featureUnavailable: 'open-settings',
  reconnectRequired: 'connect-hub',
  notDesktopToken: 'connect-hub',
  blocked: 'connect-hub',
  hubRejected: 'retry',
  unreachable: 'retry',
  rateLimited: 'retry',
  invalidKeyReturned: 'retry',
  applyFailed: 'retry',
  credentialLimit: 'retry',
  rotationLost: 'retry',
  rotateBudgetExceeded: null,
};

// 홈 상단 한 줄(필수 단계를 끝낸 사용자) 문구. 없으면 그 원인으로는 방해하지 않는다 — 자동 재시도 중인 일시 오류.
const FAILURE_SLIM: Readonly<Partial<Record<IngestLinkFailureCause, string>>> = {
  reconnectRequired: '선택 · 허브 연결이 끊겨 사용량 수집키를 받을 수 없습니다. 허브에 다시 연결하세요.',
  notDesktopToken: '선택 · 이 허브 연결로는 사용량 수집키를 받을 수 없습니다. 허브 연결을 다시 만드세요.',
  blocked: '선택 · 이 허브 연결로는 사용량 수집키를 발급할 수 없습니다. 연결을 해제한 뒤 다시 연결하세요.',
  credentialLimit: '선택 · 수집키 상한(5개)에 도달해 새로 발급하지 못했습니다. 허브 웹 /keys에서 쓰지 않는 키를 폐기하세요.',
  rotateBudgetExceeded: '선택 · 이 기기의 수집키가 너무 자주 바뀌어 잠시 교체를 멈췄습니다.',
  rotationLost: '선택 · 허브 응답이 반복해서 끊겨 수집키 자동 교체를 멈췄습니다.',
  applyFailed: '선택 · 발급된 수집키를 설정 파일에 쓰지 못했습니다. 다시 시도하세요.',
  hubRejected: '선택 · 허브가 사용량 수집키 요청을 거부했습니다.',
  invalidKeyReturned: '선택 · 허브가 돌려준 값이 수집키 형식이 아니어 적용하지 않았습니다.',
  settingsUnreadable: '선택 · 사용량 수집키 연동에 문제가 있습니다. 설정 > OTel에서 확인하세요.',
  headerUnrecognized: '선택 · 사용량 수집키 연동에 문제가 있습니다. 설정 > OTel에서 확인하세요.',
  endpointNotHub: '선택 · 수집 엔드포인트가 허브가 아니어서 수집키를 발급받지 않았습니다. 설정 > OTel에서 확인하세요.',
};

function failedStep(base: StepBase, status: IngestLinkStatus): OnboardingStep {
  const f = status.failure;
  const cause = f?.cause;
  const masked = safeMasked(status.masked);
  let msg = f?.message ?? '수집키 적용 상태에 문제가 있습니다.';
  if (cause === 'blocked' && status.blockedReason) msg += ` (사유: ${BLOCKED_REASON_TEXT[status.blockedReason] ?? '알 수 없음'})`;
  if (cause === 'rotateBudgetExceeded') {
    const at = formatRetryAt(status.retryAt);
    if (at) msg += ` 다시 교체할 수 있는 시각: ${at}.`;
  }
  const action = cause ? FAILURE_ACTION[cause] : 'open-settings';
  const detail = masked ? `${msg} 적용된 수집키 (${masked}).` : msg;
  const withAction = action ? { action } : {};

  // 서버에 기능이 아직 없다 — 오류가 아니라 "아직 적용 안 됨"이다(연결은 정상).
  if (cause === 'featureUnavailable') {
    return { ...base, state: 'todo', ...withAction, detail, slim: '선택 · 사용량 수집키가 적용되어 있지 않습니다.' };
  }
  // 설정 파일에 수집이 준비되어 있지 않아 발급을 시작하지 않았다 — 사용자가 설정을 마치면 이어진다.
  if (cause === 'notReady') {
    const texts = blockerTexts(status.blockers);
    return {
      ...base,
      state: 'todo',
      ...withAction,
      detail: texts.length > 0 ? `${msg} 확인할 항목: ${texts.join(', ')}.` : msg,
      slim: '선택 · 설정 파일에서 수집이 켜져 있지 않아 사용량 수집키를 발급하지 않았습니다. 설정 > OTel에서 확인하세요.',
    };
  }
  const slim = cause ? FAILURE_SLIM[cause] : '선택 · 사용량 수집키 연동에 문제가 있습니다. 설정 > OTel에서 확인하세요.';
  return { ...base, state: 'failed', ...withAction, detail, ...(slim ? { slim } : {}) };
}

/**
 * OTEL 연동 참고 항목. 앱이 허브 연결로 수집키를 자동 발급·적용하고 첫 데이터까지 확인한다. 백엔드가 알려 준
 * 상태와 플래그만으로 문구를 고른다 — 확인하지 않은 것("연동 완료", "수신 확인됨")은 그 상태일 때만 말한다.
 * 시도하지 않은 자동 동작("자동으로 바꾸지 않았습니다" 등)을 암시하는 문구도 쓰지 않는다.
 */
function otelLinkStep(link: OtelLink): OnboardingStep {
  const base: StepBase = { kind: 'otel-link', title: 'OTEL 연동(사용량 수집키)', advisory: true };
  if (link.error) {
    return { ...base, state: 'error', detail: `수집키 적용 상태를 확인하지 못했습니다: ${link.error}` };
  }
  const status = link.status;
  if (!link.loaded || !status) return { ...base, state: 'loading', detail: '수집키 적용 상태 확인 중…' };
  switch (status.state) {
    case 'unlinked':
      return unlinkedStep(base, status);
    case 'issuing':
      return { ...base, state: 'loading', detail: '수집키를 발급받아 적용하는 중…' };
    case 'applied':
    case 'confirmed':
      return appliedStep(base, status);
    case 'failed':
      return failedStep(base, status);
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
