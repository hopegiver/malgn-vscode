// 앱 전역 상태 — 단일 상태 객체 + 아주 단순한 구독자 목록(pub/sub)만 제공한다.
// 순환 import를 피하려고 render() 자체는 여기 두지 않는다: main.ts가 `onStateChange(render)`로
// 한 번 구독하고, 다른 모듈들은 상태를 바꾼 뒤 `notifyChange()`만 호출해 재렌더를 요청한다.
import { MOCK_AUTONOMOUS_TASKS } from './mockData';
import type { AutonomousTask } from './mockData';
import type { ClaudeSessionRecord } from './sessionsApi';
import type { WorkspaceProject, ProjectTreeNode, FilePreview } from './workspaceApi';
import type { DevToolStatus } from './devToolsApi';
import type { InstalledPlugin, MarketplaceInfo, CommandResult } from './catalogApi';
import type { DailyUsage } from './usageApi';
import type { DailyDetailReport } from './dailyDetailApi';
import type { GithubStatus, CloudflareStatus, JiraStatus } from './integrationsApi';

export type ArchiveStatus = 'active' | 'archived' | 'unknown';
export type DashboardFilter = 'all' | 'active' | 'archived';
export type SettingsTab = 'otel' | 'github' | 'cloudflare' | 'jira' | 'marketplace';

export interface AppState {
  authenticated: boolean;
  // 로그인 — 실제 Google OAuth(PKCE, malgnsoft.com 조직 계정 제한). authApi.ts 참고.
  auth: {
    loading: boolean;
    error: string | null;
    userEmail: string | null;
    userName: string | null;
  };
  // 목업이 아니라 실제 ~/workspace 아래를 스캔해온 값이 들어간다(workspaceApi.ts).
  dashboard: {
    projects: readonly WorkspaceProject[];
    loading: boolean;
    error: string | null;
    filter: DashboardFilter;
  };
  // 프로젝트 상세의 폴더 구조 + 파일 미리보기 — 실제 로컬 파일(읽기 전용).
  projectTree: {
    projectPath: string | null;
    nodes: ProjectTreeNode[];
    loading: boolean;
    error: string | null;
    expanded: Record<string, boolean>;
    selectedPath: string | null;
    preview: FilePreview | null;
    previewLoading: boolean;
    previewError: string | null;
  };
  sidebar: {
    settingsExpanded: boolean;
    projectsExpanded: boolean;
    sessionsExpanded: boolean;
  };
  // 목업이 아니라 실제 ~/.claude/sessions/*.json을 읽어온 값이 들어간다. live는
  // 파일시스템 워처 이벤트 구독이 실제로 성공했을 때만 true — 브라우저 폴백
  // 환경에서는 자연히 false로 남는다("실시간"이라고 거짓 표시하지 않는다).
  sessions: {
    items: ClaudeSessionRecord[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    live: boolean;
  };
  // 사용량 통계의 "일별 사용량" — 실제 ~/.claude/projects/**/*.jsonl 집계(최근
  // 30일). 5시간/주간 패널은 로컬 데이터 소스가 없어 여전히 mockData.ts 샘플이다.
  dailyUsage: {
    items: DailyUsage[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    live: boolean;
  };
  // "일별 사용량" 날짜 행을 클릭했을 때 펼치는 상세 — 실제 로컬 데이터(그 날짜
  // 하루만 세션/서브에이전트/툴 단위로 재집계). dailyDetailApi.ts 주석 참고.
  dailyDetail: {
    selectedDate: string | null;
    report: DailyDetailReport | null;
    loading: boolean;
    error: string | null;
  };
  // 순수 목업 — 실제 스케줄 실행 엔진 없음. on/off·추가는 이 배열만 바꾼다.
  autonomousTasks: AutonomousTask[];
  // GitHub 연동 — 이 앱은 토큰을 취급하지 않는다. 상태는 `gh` CLI를 읽기 전용으로
  // 조회한 실물이고, 연결/해제는 터미널 창을 여는 것뿐이다(integrationsApi.ts).
  github: {
    status: GithubStatus | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    connecting: boolean;
    disconnecting: boolean;
  };
  // Cloudflare 연동 — wrangler CLI 위임. wrangler 미설치(installed:false)는
  // 이 개발 머신의 정상 상태이지 에러가 아니다.
  cloudflare: {
    status: CloudflareStatus | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    connecting: boolean;
    disconnecting: boolean;
  };
  // Jira 연동 — 위임할 CLI가 없어 사이트 URL·이메일·API 토큰을 직접 받아 검증 후
  // macOS 키체인에 저장한다. 토큰 원문은 절대 이 state에 보관하지 않는다 — 제출
  // 시점에만 함수 인자로 넘어가고 그 뒤로는 참조가 남지 않는다.
  jira: {
    status: JiraStatus | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    connecting: boolean;
    disconnecting: boolean;
  };
  // 목업이 아니라 실제 `<tool> --version` 조회 결과가 들어간다. 설치/업데이트
  // 버튼(updating)은 순수 목업 — 실제로 아무것도 설치하지 않는다(사용자가 이건
  // 승인하지 않았다).
  devTools: {
    items: DevToolStatus[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    updating: Record<string, boolean>;
    updatingAll: boolean;
    mockUpdatedVersion: Record<string, string>;
  };
  // 카탈로그 — 설치된 플러그인은 실제 로컬 데이터(installed_plugins.json + 각
  // installPath 실물). "자동 업데이트" 토글만 순수 목업(로컬 UI 상태). "업데이트"
  // 버튼은 실제로 `claude plugin update`를 실행한다(사용자 명시 승인, 2026-09-08).
  catalog: {
    plugins: InstalledPlugin[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    autoUpdate: Record<string, boolean>;
    updating: Record<string, boolean>;
    updatingAll: boolean;
    lastResult: Record<string, CommandResult | null>;
  };
  // 마켓플레이스 저장소 목록 — 실제 로컬 데이터(known_marketplaces.json). 새로고침
  // 버튼은 실제로 `claude plugin marketplace update`를 실행한다.
  marketplaces: {
    items: MarketplaceInfo[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    refreshing: boolean;
  };
  // OTel 설정 — ~/.claude/settings.json의 env.OTEL_* 만 읽기 전용으로 채운다.
  // "저장"은 여전히 목업 — 이 값을 실제 설정 파일에 다시 쓰지 않는다.
  otel: {
    env: Record<string, string>;
    loading: boolean;
    error: string | null;
    loaded: boolean;
  };
}

export const state: AppState = {
  authenticated: false,
  auth: { loading: false, error: null, userEmail: null, userName: null },
  // loading을 처음부터 true로 두면 main.ts의 `!state.dashboard.loading` 트리거
  // 가드와 충돌해 loadProjects()가 영원히 호출되지 않는 교착 상태가 된다(실제
  // 재현된 버그 — 프로젝트 목록이 "새로고침 중…" 스켈레톤에서 멈춰 있었다).
  // loadProjects() 자신이 시작하자마자 loading을 true로 바꾸므로 스켈레톤은
  // 여전히 짧게 보인다.
  dashboard: { projects: [], loading: false, error: null, filter: 'all' },
  projectTree: {
    projectPath: null,
    nodes: [],
    loading: false,
    error: null,
    expanded: {},
    selectedPath: null,
    preview: null,
    previewLoading: false,
    previewError: null,
  },
  sidebar: { settingsExpanded: false, projectsExpanded: false, sessionsExpanded: false },
  sessions: { items: [], loading: false, error: null, loaded: false, live: false },
  dailyUsage: { items: [], loading: false, error: null, loaded: false, live: false },
  dailyDetail: { selectedDate: null, report: null, loading: false, error: null },
  autonomousTasks: MOCK_AUTONOMOUS_TASKS.map((t) => ({ ...t, history: t.history ? [...t.history] : undefined })),
  github: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
  cloudflare: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
  jira: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
  devTools: { items: [], loading: false, error: null, loaded: false, updating: {}, updatingAll: false, mockUpdatedVersion: {} },
  catalog: { plugins: [], loading: false, error: null, loaded: false, autoUpdate: {}, updating: {}, updatingAll: false, lastResult: {} },
  marketplaces: { items: [], loading: false, error: null, loaded: false, refreshing: false },
  otel: { env: {}, loading: false, error: null, loaded: false },
};

type Listener = () => void;
const listeners: Listener[] = [];

export function onStateChange(fn: Listener): void {
  listeners.push(fn);
}

export function notifyChange(): void {
  for (const fn of listeners) fn();
}
