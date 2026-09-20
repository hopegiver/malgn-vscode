// 앱 전역 상태 — 단일 상태 객체 + 아주 단순한 구독자 목록(pub/sub)만 제공한다.
// 순환 import를 피하려고 render() 자체는 여기 두지 않는다: main.ts가 `onStateChange(render)`로
// 한 번 구독하고, 다른 모듈들은 상태를 바꾼 뒤 `notifyChange()`만 호출해 재렌더를 요청한다.
import type { ClaudeSessionRecord, SessionTranscript } from './sessionsApi';
import type { WorkspaceProject, WorkspaceSkippedEntry, ProjectTreeNode, FilePreview } from './workspaceApi';
import type { DevToolStatus, DevToolPreview, DevToolActionResult } from './devToolsApi';
import type { InstalledPlugin, MarketplaceInfo, CommandResult, GlobalCatalog } from './catalogApi';
import type { DailyUsage } from './usageApi';
import type { DailyDetailReport } from './dailyDetailApi';
import type { GithubStatus, CloudflareStatus } from './integrationsApi';
import type { AutonomyRunStatus, AutonomyScheduleMode } from './autonomyApi';
import type { McpServerSummary, McpCatalogEntry } from './mcpApi';
import type { OtelSettings } from './otelApi';
import type { MalgnAgentConfigStatus } from './configApi';
import type { AppLinksStatus } from './appLinksApi';

export type ArchiveStatus = 'active' | 'archived' | 'unknown';
export type DashboardFilter = 'all' | 'active' | 'archived';
export type DashboardSort = 'updated' | 'name';
export type SettingsTab = 'otel' | 'github' | 'cloudflare' | 'marketplace' | 'mcp' | 'applinks' | 'devtools';
export type CatalogTab = 'plugins' | 'global';

// "자율업무" 화면 전용 표시 타입 — autonomyApi.ts의 두 조회를 (projectPath, id)
// 키로 병합한 결과다. id~enabled까지는 설정(autonomy.json, upsert 대상 그대로),
// running~logPath는 메모리 런타임 상태(autonomy_runtime_status, 읽기 전용 — 저장
// 대상이 아니다). scheduleLabel/lastRunLabel/nextRunLabel은 그 둘에서 프론트가
// 계산해 채운다(views/autonomousTasks.ts).
export interface AutonomousTask {
  id: string;
  projectPath: string;
  projectName: string;
  name: string;
  prompt: string;
  subagent: string | null;
  interval: number; // 분 — 이전 실행이 "끝난 뒤" 대기하는 시간(scheduleMode='interval'일 때만 사용)
  scheduleMode: AutonomyScheduleMode;
  atTime: string | null; // 'HH:MM' 로컬 벽시계. autonomyApi.ts의 옵셔널 필드를 여기서 명시 정규화해 비-옵셔널로 둔다(scheduleMode='interval'이면 null).
  days: number[]; // 0=일…6=토. 빈 배열 = 매일(scheduleMode='fixedTime'일 때만 사용).
  hourlyMinute: number | null; // 0~59. autonomyApi.ts의 옵셔널 필드를 여기서 `?? null`로 정규화한다(scheduleMode='hourly'일 때만 사용).
  cron: string | null; // 표준 5필드 표현식. 위와 동일하게 정규화한다(scheduleMode='cron'일 때만 사용).
  enabled: boolean;
  timeout: number | null; // 분. null이면 전역 기본값 사용.
  running: boolean;
  lastStartedAt: string | null;
  lastFinishedAt: string | null;
  nextRunAt: string | null;
  status: AutonomyRunStatus | null;
  summary: string | null;
  durationMs: number | null;
  logPath: string | null;
  scheduleLabel: string;
  lastRunLabel: string;
  nextRunLabel: string;
}

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
    // 후보였지만 프로젝트로 인정되지 않고 제외된 폴더(CLAUDE.md 없음 등,
    // 사유별로 이름 붙여 온다) — projects와 동시에 채워진다(loadProjects).
    skipped: readonly WorkspaceSkippedEntry[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    filter: DashboardFilter;
    sort: DashboardSort;
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
    catalogExpanded: boolean;
    appLinksExpanded: boolean;
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
  // 세션 상세 = 실제 대화 + 이어쓰기 (docs/design/session-chat.md §4-4). 세션
  // 상세 화면에 들어갈 때만 채워지고 나가면 초기화된다(views/sessions.ts의
  // enterSessionChatView/leaveSessionChatView).
  sessionChat: {
    sessionId: string | null;
    /** draft 상태(프로젝트 카드 "새 세션") — session_id가 아직 없을 때만 채워진다.
     * sessionId가 배정되면(첫 메시지 전송 성공) null로 되돌아간다. */
    draftProjectPath: string | null;
    transcript: SessionTranscript | null;
    loading: boolean;
    error: string | null;
    /** error가 claude CLI 미인증으로 인한 실패인지(SessionChatDone.authError를
     * 그대로 반영). true일 때만 화면이 "터미널에서 claude login" 버튼을 보여준다. */
    authError: boolean;
    /** 전송 중인 턴. null이면 입력 가능 */
    turnId: string | null;
    /** 스트리밍으로 쌓는 임시 assistant 말풍선 */
    streamingText: string;
    /** 스트리밍 중 도착한 도구 한 줄들 */
    streamingTools: string[];
    input: string;
  };
  // 사용량 통계의 "일별 사용량" — 실제 ~/.claude/projects/**/*.jsonl 집계(최근
  // 30일). 로그인 직후 한 번 미리 불러오고(main.ts), 이후 "사용량 통계" 메뉴
  // 진입 시점마다 다시 불러온다(sidebar.ts). 실시간 파일 감시는 하지 않는다.
  // 카드 통계·홈 위젯도 이 items만으로 계산한다.
  dailyUsage: {
    items: DailyUsage[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
  };
  // "일별 사용량" 날짜 행을 클릭했을 때 펼치는 상세 — 실제 로컬 데이터(그 날짜
  // 하루만 세션/서브에이전트/툴 단위로 재집계). dailyDetailApi.ts 주석 참고.
  dailyDetail: {
    selectedDate: string | null;
    report: DailyDetailReport | null;
    loading: boolean;
    error: string | null;
  };
  // 실제 프로젝트별 자율업무 설정(autonomyApi.ts) — 로그인 직후 전체 프로젝트를
  // 훑어 한 번 불러온다. on/off·추가·삭제는 각각 Rust 커맨드 호출이 성공한 뒤에만
  // 이 배열을 갱신한다(낙관적 갱신 금지 — 실행 엔진 상태의 정본은 백엔드).
  autonomousTasks: {
    items: AutonomousTask[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
  };
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
  // 실제 `<tool> --version`/설치방식 조회 결과가 들어간다. 설치/업데이트 버튼도
  // actionKind가 "run"인 도구에 한해 실제로 명령을 실행한다(사용자 명시 승인,
  // 2026-09-09). 실행 전에는 preview에 dry-run 미리보기를 담아 사용자 확인을
  // 받고, 확인된 planId로만 update/install을 호출한다.
  devTools: {
    items: DevToolStatus[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    updating: Record<string, boolean>;
    updatingAll: boolean;
    elapsedSec: Record<string, number>;
    previewLoading: Record<string, boolean>;
    preview: Record<string, DevToolPreview | null>;
    lastResult: Record<string, DevToolActionResult | null>;
    logExpanded: Record<string, boolean>;
    manualOpen: Record<string, boolean>;
  };
  // 카탈로그 — 설치된 플러그인은 실제 로컬 데이터(installed_plugins.json + 각
  // installPath 실물). "업데이트" 버튼은 실제로 `claude plugin update`를
  // 실행한다(사용자 명시 승인, 2026-09-08).
  catalog: {
    plugins: InstalledPlugin[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    updating: Record<string, boolean>;
    updatingAll: boolean;
    lastResult: Record<string, CommandResult | null>;
    installingDefault: boolean;
    installDefaultResult: CommandResult | null;
  };
  // 전역(user-level) 에이전트/스킬 — 플러그인에 안 묶인 개인 항목
  // (~/.claude/agents/*.md, ~/.claude/skills/*/SKILL.md). 조회+상태표시만 —
  // enable/disable·삭제·편집 없음. status가 "invalid"인 항목은 화면에서 경고로
  // 표시한다(list_global_catalog).
  globalCatalog: {
    data: GlobalCatalog | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
  };
  // 마켓플레이스 저장소 목록 — 실제 로컬 데이터(known_marketplaces.json). 새로고침
  // 버튼은 실제로 `claude plugin marketplace update`를 실행하고, adding/removingId는
  // 사용자가 직접 소스를 추가하거나 등록된 항목을 제거하는 진행 중 상태다(둘 다
  // `claude plugin marketplace add|remove`를 실제로 실행한다).
  marketplaces: {
    items: MarketplaceInfo[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    refreshing: boolean;
    adding: boolean;
    removingId: string | null;
  };
  // OTel 설정 — otel_settings_get()으로 ~/.claude/settings.json의 관리대상
  // 14키를 allowlist 기반으로 읽고, otel_settings_save()로 실제로 저장한다(더
  // 이상 목업이 아니다). settings는 그 응답을 그대로 보관 — 폼 초기값(values→
  // defaults→빈값)과 "기본값(저장 안 됨)" 구분 표시는 views/settings.ts가 매번
  // 이 값에서 계산한다.
  otel: {
    settings: OtelSettings | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    saving: boolean;
  };
  // 전역 설정 파일(~/.claude/malgn-agent.json) 상태 — 자율업무 화면(autonomy·
  // logs 필드)과 프로젝트 화면(workspaces 필드)이 이 하나의 slice를 함께
  // 조회·표시한다. 저장 API가 4개 필드를 항상 통째로 덮어쓰는 풀 오버라이트라,
  // 두 화면이 각자 편집하지 않는 필드는 이 status의 현재값을 그대로 실어
  // 보내야 한다. editingAutonomy/editingWorkspaces는 각 화면의 편집 폼 펼침
  // 여부(순수 UI 상태, 화면별로 독립), saving은 저장 요청 진행 중 여부다.
  malgnAgentConfig: {
    status: MalgnAgentConfigStatus | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    editingAutonomy: boolean;
    editingWorkspaces: boolean;
    saving: boolean;
  };
  // MCP 관리 — `claude mcp` CLI를 위임 실행해 등록된 MCP 서버 목록/연결 상태를
  // 읽는다(mcpApi.ts). 모델을 호출하지 않는 순수 헬스체크라 빠르고 무료다.
  // 로그인 직후 한 번 미리 불러온다 — 홈 대시보드의 malgnai-hub 상태 위젯이
  // 바로 값을 보여줘야 한다.
  // loggingInName은 등록된 서버 행의 "인증/재인증" 버튼(mcp_login)을 누른
  // 동안만 해당 행 버튼을 잠그는 용도다 — GitHub/카탈로그 설치와 동일하게, 이
  // 버튼도 터미널 창을 여는 데까지만 관여하고 로그인 완료 여부는 알 수 없다.
  // loggingOutName은 "해제" 버튼(mcp_logout)용 — 터미널 없이 즉시 끝나지만
  // 동일한 이중클릭 방지 패턴을 쓴다.
  mcp: {
    items: McpServerSummary[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    loggingInName: string | null;
    loggingOutName: string | null;
  };
  // MCP 카탈로그 — 잘 알려진 공개 MCP 서버(Gmail 등)를 원클릭 등록하는 목록
  // (mcp_catalog_list). "설치"는 백엔드가 터미널 창을 열어 로그인까지 안내할
  // 뿐이다 — installingId는 그 터미널을 여는 동안만 해당 항목 버튼을 잠그는
  // 용도이고, 로그인 완료 여부는 이 화면이 알 수 없다(사용자가 기존 mcp
  // "새로고침"을 눌러야 반영된다).
  mcpCatalog: {
    items: McpCatalogEntry[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    installingId: string | null;
  };
  // 앱링크 — 사내/외부 웹 앱 바로가기(docs/design-app-links.md). status가 정본
  // 응답 전체(links/warnings/limits 포함)를 담고, 파생값(활성 링크 목록)은
  // 저장하지 않고 렌더 시점에 `status.links.filter(l => l.enabled)`로 계산한다.
  // error는 커맨드 호출 자체가 실패한 경우(Tauri IPC 부재 등)에만 쓰인다 — 파일
  // 손상은 status.ok/status.error로 표현된다.
  appLinks: {
    status: AppLinksStatus | null;
    loading: boolean;
    error: string | null;
    loaded: boolean;
    saving: boolean;
  };
  // 자동 업데이트 — updateApi.ts가 관리한다. 평소엔 available:false로 화면에
  // 아무 변화가 없고, 백그라운드 체크+다운로드가 성공했을 때만 사이드바 하단에
  // 버튼이 나타난다(sidebar.ts). version은 표시용, installing은 버튼 클릭 후
  // install()+relaunch() 진행 중(또는 "다음 실행 자동 적용" 시도 중) 중복 클릭을
  // 막는 용도다. 실제 다운로드된 Update 객체 자체는 상태로 직렬화하지 않고
  // updateApi.ts 모듈 내부 변수로만 들고 있는다(직렬화 불가능한 리소스 핸들이라).
  update: {
    available: boolean;
    version: string | null;
    installing: boolean;
  };
}

// 앱 상태의 초기값 — 로그인 직후 한 번 만드는 `state`와, 로그아웃 시 데이터
// 슬라이스를 되돌리는 resetStateForLogout() 양쪽이 이 팩토리를 공유한다(G1,
// 리뷰 v0.2.5). 두 곳이 이 리터럴을 따로 들고 있으면 필드가 하나 늘 때마다
// 한쪽만 갱신되는 드리프트가 나기 쉽다.
function createInitialState(): AppState {
  return {
    authenticated: false,
    auth: { loading: false, error: null, userEmail: null, userName: null },
    // loading을 처음부터 true로 두면 main.ts의 `!state.dashboard.loading` 트리거
    // 가드와 충돌해 loadProjects()가 영원히 호출되지 않는 교착 상태가 된다(실제
    // 재현된 버그 — 프로젝트 목록이 "새로고침 중…" 스켈레톤에서 멈춰 있었다).
    // loadProjects() 자신이 시작하자마자 loading을 true로 바꾸므로 스켈레톤은
    // 여전히 짧게 보인다.
    dashboard: { projects: [], skipped: [], loading: false, error: null, loaded: false, filter: 'all', sort: 'updated' },
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
  sidebar: { settingsExpanded: false, projectsExpanded: false, sessionsExpanded: false, catalogExpanded: false, appLinksExpanded: false },
  sessions: { items: [], loading: false, error: null, loaded: false, live: false },
  sessionChat: {
    sessionId: null,
    draftProjectPath: null,
    transcript: null,
    loading: false,
    error: null,
    authError: false,
    turnId: null,
    streamingText: '',
    streamingTools: [],
    input: '',
  },
  dailyUsage: { items: [], loading: false, error: null, loaded: false },
  dailyDetail: { selectedDate: null, report: null, loading: false, error: null },
  autonomousTasks: { items: [], loading: false, error: null, loaded: false },
  github: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
  cloudflare: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
  devTools: {
    items: [],
    loading: false,
    error: null,
    loaded: false,
    updating: {},
    updatingAll: false,
    elapsedSec: {},
    previewLoading: {},
    preview: {},
    lastResult: {},
    logExpanded: {},
    manualOpen: {},
  },
  catalog: {
    plugins: [],
    loading: false,
    error: null,
    loaded: false,
    updating: {},
    updatingAll: false,
    lastResult: {},
    installingDefault: false,
    installDefaultResult: null,
  },
  globalCatalog: { data: null, loading: false, error: null, loaded: false },
  marketplaces: { items: [], loading: false, error: null, loaded: false, refreshing: false, adding: false, removingId: null },
  otel: { settings: null, loading: false, error: null, loaded: false, saving: false },
  malgnAgentConfig: { status: null, loading: false, error: null, loaded: false, editingAutonomy: false, editingWorkspaces: false, saving: false },
  mcp: { items: [], loading: false, error: null, loaded: false, loggingInName: null, loggingOutName: null },
  mcpCatalog: { items: [], loading: false, error: null, loaded: false, installingId: null },
  appLinks: { status: null, loading: false, error: null, loaded: false, saving: false },
    update: { available: false, version: null, installing: false },
  };
}

export const state: AppState = createInitialState();

type Listener = () => void;
const listeners: Listener[] = [];

export function onStateChange(fn: Listener): void {
  listeners.push(fn);
}

export function notifyChange(): void {
  for (const fn of listeners) fn();
}

// 실제 인증 성공(Google OAuth `loginWithGoogle()` 또는 로컬 개발 전용
// `tryDevAutoLogin()`, 둘 다 authApi.ts) 시 인증 상태로 전이하는 유일한 지점.
// 두 호출부(views/login.ts, main.ts의 bootstrap())가 각자 `userEmail`/
// `userName`/`authenticated` 3줄을 따로 복제하던 것을 여기로 모았다 — 이 3줄이
// 실제 인증 분기라, 두 곳이 따로 유지되면 한쪽만 고치는 조용한 드리프트(예: 향후
// 필드 추가 시 한 곳만 갱신)가 나기 쉽다.
export function applyAuthenticatedIdentity(email: string, name: string): void {
  state.auth.userEmail = email;
  state.auth.userName = name;
  state.authenticated = true;
}

// 로그아웃 — 인증 필드만 지우던 이전 구현은 나머지 데이터 슬라이스(세션 목록,
// 채팅 전문, 프로젝트, 자율업무 등)를 그대로 남겨뒀다(G1, 리뷰 v0.2.5). 1인
// 1머신 사내 도구라 실질 노출 위험은 낮지만, "로그아웃했는데 이전 사용자의
// 데이터가 화면에 남아 있다"는 것은 수명 계약의 구멍이다 — 다음 로그인
// 사용자에게 이전 세션의 잔상이 보이면 안 된다. createInitialState()로 만든
// 새 값을 최상위 슬라이스에 되돌린다(sidebar.ts 로그아웃 핸들러가 호출한다).
//
// sessionChat만 예외다 — main.ts의 handleNavigation()이 이 호출 직후(해시가
// ''로 바뀌며 발생하는 hashchange) 라우트 이탈 정리를 돌리는데, 그 정리는
// "state.sessionChat.sessionId !== null"을 보고 leaveSessionChatView()(실제
// unlisten 호출 + 상태 초기화 둘 다 책임진다)를 부를지 판단한다. 여기서
// sessionChat을 먼저 비워버리면 그 가드가 항상 거짓이 되어 leaveSessionChatView()
// 가 스킵되고, 정작 G1이 고치려던 스트리밍 리스너 누수가 그대로 남는다 —
// 데이터 초기화보다 리스너 해제가 항상 먼저(같은 함수 안에서) 일어나야 한다.
export function resetStateForLogout(): void {
  const fresh = createInitialState();
  for (const key of Object.keys(fresh) as (keyof AppState)[]) {
    if (key === 'sessionChat') continue;
    (state[key] as unknown) = fresh[key];
  }
}
