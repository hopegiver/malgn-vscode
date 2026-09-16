// 앱 전역 상태 — 단일 상태 객체 + 아주 단순한 구독자 목록(pub/sub)만 제공한다.
// 순환 import를 피하려고 render() 자체는 여기 두지 않는다: main.ts가 `onStateChange(render)`로
// 한 번 구독하고, 다른 모듈들은 상태를 바꾼 뒤 `notifyChange()`만 호출해 재렌더를 요청한다.
import type { ClaudeSessionRecord, SessionTranscript } from './sessionsApi';
import type { WorkspaceProject, ProjectTreeNode, FilePreview } from './workspaceApi';
import type { DevToolStatus, DevToolPreview, DevToolActionResult } from './devToolsApi';
import type { InstalledPlugin, MarketplaceInfo, CommandResult, GlobalCatalog } from './catalogApi';
import type { DailyUsage } from './usageApi';
import type { DailyDetailReport } from './dailyDetailApi';
import type { GithubStatus, CloudflareStatus, JiraStatus } from './integrationsApi';
import type { AutonomyRunStatus } from './autonomyApi';
import type { McpServerSummary, McpCatalogEntry } from './mcpApi';
import type { OtelSettings } from './otelApi';
import type { MalgnAgentConfigStatus } from './configApi';
import type { AppLinksStatus } from './appLinksApi';

export type ArchiveStatus = 'active' | 'archived' | 'unknown';
export type DashboardFilter = 'all' | 'active' | 'archived';
export type DashboardSort = 'updated' | 'name';
export type SettingsTab = 'otel' | 'github' | 'cloudflare' | 'jira' | 'marketplace' | 'mcp' | 'applinks';
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
  interval: number; // 분 — 이전 실행이 "끝난 뒤" 대기하는 시간
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
  // 버튼은 실제로 `claude plugin marketplace update`를 실행한다.
  marketplaces: {
    items: MarketplaceInfo[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    refreshing: boolean;
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
  // loggingInName은 등록된 서버 행의 "로그인" 버튼(mcp_login)을 누른 동안만
  // 해당 행 버튼을 잠그는 용도다 — GitHub/카탈로그 설치와 동일하게, 이 버튼도
  // 터미널 창을 여는 데까지만 관여하고 로그인 완료 여부는 알 수 없다.
  mcp: {
    items: McpServerSummary[];
    loading: boolean;
    error: string | null;
    loaded: boolean;
    loggingInName: string | null;
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
}

export const state: AppState = {
  authenticated: false,
  auth: { loading: false, error: null, userEmail: null, userName: null },
  // loading을 처음부터 true로 두면 main.ts의 `!state.dashboard.loading` 트리거
  // 가드와 충돌해 loadProjects()가 영원히 호출되지 않는 교착 상태가 된다(실제
  // 재현된 버그 — 프로젝트 목록이 "새로고침 중…" 스켈레톤에서 멈춰 있었다).
  // loadProjects() 자신이 시작하자마자 loading을 true로 바꾸므로 스켈레톤은
  // 여전히 짧게 보인다.
  dashboard: { projects: [], loading: false, error: null, loaded: false, filter: 'all', sort: 'updated' },
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
  sidebar: { settingsExpanded: false, projectsExpanded: false, sessionsExpanded: false, catalogExpanded: false, appLinksExpanded: true },
  sessions: { items: [], loading: false, error: null, loaded: false, live: false },
  sessionChat: {
    sessionId: null,
    draftProjectPath: null,
    transcript: null,
    loading: false,
    error: null,
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
  jira: { status: null, loading: false, error: null, loaded: false, connecting: false, disconnecting: false },
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
  marketplaces: { items: [], loading: false, error: null, loaded: false, refreshing: false },
  otel: { settings: null, loading: false, error: null, loaded: false, saving: false },
  malgnAgentConfig: { status: null, loading: false, error: null, loaded: false, editingAutonomy: false, editingWorkspaces: false, saving: false },
  mcp: { items: [], loading: false, error: null, loaded: false, loggingInName: null },
  mcpCatalog: { items: [], loading: false, error: null, loaded: false, installingId: null },
  appLinks: { status: null, loading: false, error: null, loaded: false, saving: false },
};

type Listener = () => void;
const listeners: Listener[] = [];

export function onStateChange(fn: Listener): void {
  listeners.push(fn);
}

export function notifyChange(): void {
  for (const fn of listeners) fn();
}
