// 앱 엔트리 — 해시 라우팅으로 모든 화면을 연결한다.
//
// ⚠️ 이 앱은 대부분 UI/UX 검증용 목업이다(하드코딩된 샘플 데이터, mockData.ts).
// 다음은 실제 로컬 데이터를 Rust 커맨드로 읽어온다(전부 읽기 전용):
//   - "프로젝트"(views/projects.ts): ~/workspace 스캔 + 폴더 구조/파일 미리보기 — workspaceApi.ts
//   - "세션목록"(views/sessions.ts): ~/.claude/sessions/*.json + 대화 로그 첫 줄 제목 — sessionsApi.ts
//   - "개발 환경"(views/devTools.ts): claude/node/gh/git/pnpm/wrangler --version + 설치방식
//     판별 — devToolsApi.ts. actionKind가 "run"인 도구는 dry-run 미리보기 확인 후 실제
//     설치/업데이트 명령도 실행한다(사용자 명시 승인).
//   - "카탈로그"(views/catalog.ts): installed_plugins.json(user scope) + 실물 agents/skills/knowledge — catalogApi.ts
//   - "마켓플레이스 설정": known_marketplaces.json — catalogApi.ts
//   - "OTel 설정": ~/.claude/settings.json의 env.OTEL_* — otelApi.ts
//   - "사용량 통계"의 일별 사용량과 날짜별 상세: ~/.claude/projects/**/*.jsonl 집계 — usageApi.ts/dailyDetailApi.ts
//   - "GitHub/Cloudflare 설정": 이 앱은 토큰을 취급하지 않는다. 상태는 `gh`/`wrangler`
//     CLI를 읽기 전용으로 조회하고, 연결/해제는 사용자가 조작할 터미널 창을 여는
//     것뿐이다 — integrationsApi.ts.
//   - "자율업무"(views/autonomousTasks.ts): 프로젝트별 자율업무 설정(프롬프트·주기·
//     서브에이전트 등) — autonomyApi.ts. 실제 스케줄 실행 엔진은 Rust 쪽에 있고,
//     이 화면은 조회/추가/수정/삭제/on-off만 한다.
//   - "MCP 관리"(views/settings.ts): `claude mcp` CLI를 위임 실행해 등록된 MCP
//     서버 목록/연결 상태를 조회하고 추가/삭제한다 — mcpApi.ts. 모델을 호출하지
//     않는 순수 헬스체크라 빠르고 무료다. 홈 대시보드의 malgnai-hub 위젯도 이
//     목록에서 이름으로 필터링해 보여준다.
// 카탈로그의 "업데이트"·마켓플레이스의 "새로고침" 버튼은 사용자가 명시적으로
// 승인해 실제로 `claude plugin update`/`claude plugin marketplace update`를
// 실행한다(catalogApi.ts 참고).
//
// 로그인은 더 이상 목업이 아니다 — 실제 Google OAuth(PKCE, malgnsoft.com Google
// Workspace 계정만 허용)다(authApi.ts/views/login.ts 참고). Client ID가 아직
// 설정되지 않은 동안은 로그인 버튼을 눌러도 브라우저가 열리지 않고 명확한 에러만
// 뜬다 — src-tauri/src/lib.rs의 GOOGLE_OAUTH_CLIENT_ID를 채워야 실제로 동작한다.
//
// 세션목록은 파일시스템 이벤트로도 갱신된다(폴링 아님) — initLiveWatchers()가
// 앱 시작 시 한 번 구독한다. 사용량 통계는 실시간 감시를 하지 않는다 — 이
// 컴퓨터처럼 여러 프로젝트에서 계속 쓰고 있으면 재계산이 끝나기도 전에 다음
// 이벤트가 쌓여 오히려 계속 느려졌다. 대신 메뉴 클릭 시점에만 새로 불러온다
// (sidebar.ts).
import { el } from './dom';
import { state, onStateChange, notifyChange, applyAuthenticatedIdentity } from './state';
import { parseRoute } from './route';
import { renderSidebar } from './sidebar';
import { renderLoginView } from './views/login';
import { renderHomeView } from './views/home';
import { renderProjectsListView, renderProjectsDetailView, loadProjects, loadProjectTree, leaveProjectsListView } from './views/projects';
import { renderCatalogView, loadCatalog, loadGlobalCatalog, loadMarketplaces } from './views/catalog';
import { loadDevTools } from './views/devTools';
import {
  renderSettingsView,
  loadOtelEnv,
  loadGithubStatus,
  loadCloudflareStatus,
  loadMcp,
  loadMcpCatalog,
  ensureOtelAutoConfigured,
} from './views/settings';
import { renderUsageView, loadDailyUsage } from './views/usage';
import {
  renderSessionsListView,
  renderSessionDetailView,
  renderSessionDraftView,
  loadSessions,
  enterSessionChatView,
  enterSessionDraftView,
  leaveSessionChatView,
  leaveSessionsListView,
} from './views/sessions';
import { onSessionsChanged } from './sessionsApi';
import {
  renderAutonomousTasksListView,
  renderAutonomousTaskBoardView,
  renderAutonomousTaskDetailView,
  loadAutonomousTasks,
  leaveAutonomousTasksListView,
} from './views/autonomousTasks';
import { loadAppLinks } from './views/appLinks';
import { tryDevAutoLogin } from './authApi';
import { initUpdateCheck } from './updateApi';

// 순수 렌더 — 상태를 바꾸지 않는다. onStateChange(renderApp)로 구독되어 있어
// notifyChange() 한 번으로 항상 최신 상태가 반영된다.
function renderApp(): void {
  const root = document.getElementById('app');
  if (!root) return;
  root.replaceChildren();

  if (!state.authenticated) {
    root.appendChild(renderLoginView());
    return;
  }

  const route = parseRoute();

  let content: HTMLElement;
  switch (route.kind) {
    case 'home':
      content = renderHomeView();
      break;
    case 'projects-list':
      content = renderProjectsListView();
      break;
    case 'projects-detail':
      content = renderProjectsDetailView(route.path);
      break;
    case 'catalog':
      content = renderCatalogView(route.tab);
      break;
    case 'settings':
      content = renderSettingsView(route.tab);
      break;
    case 'usage':
      content = renderUsageView();
      break;
    case 'sessions-list':
      content = renderSessionsListView();
      break;
    case 'sessions-detail':
      content = renderSessionDetailView(route.sessionId);
      break;
    case 'sessions-draft':
      content = renderSessionDraftView(route.projectPath);
      break;
    case 'tasks-list':
      content = renderAutonomousTasksListView();
      break;
    case 'tasks-board':
      content = renderAutonomousTaskBoardView();
      break;
    case 'tasks-detail':
      content = renderAutonomousTaskDetailView(route.taskId);
      break;
  }

  // 세션 상세(채팅) 화면만 보조 클래스를 얹어 헤더/입력창 고정 + 메시지 목록
  // 자체 스크롤 레이아웃을 쓴다(styles.css `.content.chat-route`). 다른 라우트는
  // 기존 전체 페이지 스크롤(`.content` 단독)을 그대로 쓴다.
  const contentClassName = route.kind === 'sessions-detail' || route.kind === 'sessions-draft' ? 'content chat-route' : 'content';
  const main = el('main', { className: contentClassName }, [content]);
  root.appendChild(renderSidebar(route));
  root.appendChild(main);
}

// 네비게이션 진입점 — 렌더 후에 필요한 데이터 로딩을 "따로" 트리거한다(렌더 함수
// 자체 안에서 하면 로딩 콜백이 재귀적으로 renderApp()을 다시 부르는 동안 바깥
// renderApp()이 아직 실행 중인 상태와 겹쳐서 DOM이 꼬일 수 있다).
//
// 프로젝트·세션·개발환경·카탈로그·마켓플레이스·사용량 통계는 로그인 직후 한
// 번씩 미리 불러온다 — 홈 대시보드 위젯과 사이드바 펼침 목록이 실제 값을 바로
// 보여줘야 하기 때문이다(로딩/실패 상태는 각 화면이 loaded/loading 플래그로 알아서 처리).
// OTel 자동 세팅 시도 여부 — 앱 세션당 정확히 한 번만 ensureOtelAutoConfigured()를
// 호출한다(runtimeWatcherInitialized/liveWatchersInitialized와 같은 1회성 플래그
// 패턴).
let otelAutoSetupChecked = false;
// 자동 업데이트 체크 — 인증된 세션에서만 시작해야 한다(비로그인 상태에서 캐리오버
// 자동설치 경로가 로그인 화면에서 발동해 사용자가 영문도 모른 채 앱이 재시작되는
// 사고를 막기 위함). handleNavigation()은 맨 위에서 `!state.authenticated`면 이미
// return하므로, 아래 이 플래그 분기에 도달하는 시점은 항상 로그인 성공 후다.
// Google OAuth(views/login.ts) 성공 시의 해시 변경과 dev 자동 로그인(bootstrap())
// 직후 호출 양쪽 다 결국 이 handleNavigation()을 거치므로 두 로그인 경로 모두
// 커버되고, 세션당 정확히 한 번만 실행되도록 플래그로 막는다.
let updateCheckStarted = false;

function handleNavigation(): void {
  renderApp();
  if (!state.authenticated) return;

  if (!updateCheckStarted) {
    updateCheckStarted = true;
    // fire-and-forget — 메인 렌더/네비게이션을 절대 기다리게 하지 않아야
    // 오프라인·느린 네트워크에서도 부팅이 멈춘 것처럼 보이지 않는다.
    initUpdateCheck();
  }

  if (!state.dashboard.loaded && !state.dashboard.loading) void loadProjects();
  if (!state.sessions.loaded && !state.sessions.loading) void loadSessions();
  if (!state.devTools.loaded && !state.devTools.loading) void loadDevTools();
  if (!state.catalog.loaded && !state.catalog.loading) void loadCatalog();
  if (!state.globalCatalog.loaded && !state.globalCatalog.loading) void loadGlobalCatalog();
  if (!state.marketplaces.loaded && !state.marketplaces.loading) void loadMarketplaces();
  if (!state.dailyUsage.loaded && !state.dailyUsage.loading) void loadDailyUsage();
  if (!state.autonomousTasks.loaded && !state.autonomousTasks.loading) void loadAutonomousTasks();
  if (!state.mcp.loaded && !state.mcp.loading) void loadMcp();
  if (!state.mcpCatalog.loaded && !state.mcpCatalog.loading) void loadMcpCatalog();
  // 사이드바가 전 화면에 상시 렌더되므로 탭 진입을 기다리지 않고 미리 불러온다
  // (docs/design-app-links.md §6-3).
  if (!state.appLinks.loaded && !state.appLinks.loading) void loadAppLinks();
  // OTel 자동 세팅 — 설정 화면에 한 번도 안 들어간 사용자를 위해 세션당 정확히
  // 한 번만 조건 확인 후 시도한다(조건 3개는 views/settings.ts의
  // ensureOtelAutoConfigured 참고).
  if (!otelAutoSetupChecked) {
    otelAutoSetupChecked = true;
    void ensureOtelAutoConfigured();
  }

  const route = parseRoute();
  if (route.kind === 'settings' && route.tab === 'otel' && !state.otel.loaded && !state.otel.loading) {
    void loadOtelEnv();
  }
  if (route.kind === 'settings' && route.tab === 'github' && !state.github.loaded && !state.github.loading) {
    void loadGithubStatus();
  }
  if (route.kind === 'settings' && route.tab === 'cloudflare' && !state.cloudflare.loaded && !state.cloudflare.loading) {
    void loadCloudflareStatus();
  }
  if (route.kind === 'projects-detail' && state.projectTree.projectPath !== route.path && !state.projectTree.loading) {
    void loadProjectTree(route.path);
  }
  // 세션 상세 = 실제 대화 + 이어쓰기 (docs/design/session-chat.md). 다른 세션으로
  // 옮기거나 이 라우트를 완전히 떠나면 항상 먼저 리스너를 해제한다(UnlistenFn 누수 방지).
  if (route.kind === 'sessions-detail') {
    if (state.sessionChat.sessionId !== route.sessionId) {
      leaveSessionChatView();
      void enterSessionChatView(route.sessionId);
    }
  } else if (route.kind === 'sessions-draft') {
    if (state.sessionChat.draftProjectPath !== route.projectPath || state.sessionChat.sessionId !== null) {
      leaveSessionChatView();
      void enterSessionDraftView(route.projectPath);
    }
  } else if (state.sessionChat.sessionId !== null || state.sessionChat.draftProjectPath !== null) {
    leaveSessionChatView();
  }
  if (route.kind === 'usage') {
    // 실시간 감시 대신 메뉴 클릭(=이 라우트 진입) 시점마다 새로 불러온다 — 이미
    // 불러오는 중이면 겹쳐 쌓이지 않게 건너뛴다.
    if (!state.dailyUsage.loading) void loadDailyUsage();
  }
  // 자율업무/프로젝트 화면의 설정 모달은 각 화면의 목록 뷰에서만 렌더된다 —
  // 그 라우트를 완전히 벗어나면 열려 있던 모달의 ESC 리스너가 window에 남지
  // 않도록 매번 정리한다(sessionChat 리스너 해제와 같은 원칙). 모달이 닫혀
  // 있던 경우엔 두 함수 모두 아무 일도 하지 않는다.
  if (route.kind !== 'tasks-list') leaveAutonomousTasksListView();
  if (route.kind !== 'projects-list') leaveProjectsListView();
  if (route.kind !== 'sessions-list') leaveSessionsListView();
}

// 세션목록·사용량 통계 실시간 감시 — 앱이 켜져 있는 동안 딱 한 번만 구독한다.
// 구독 자체가 실패하면(Tauri IPC 브리지가 없는 플레인 브라우저) live 플래그가
// false로 남아 화면에 "실시간" 표시가 뜨지 않는다(거짓 표시 금지).
let liveWatchersInitialized = false;
function initLiveWatchers(): void {
  if (liveWatchersInitialized) return;
  liveWatchersInitialized = true;

  onSessionsChanged(() => {
    if (state.sessions.loaded) void loadSessions();
  })
    .then(() => {
      state.sessions.live = true;
      notifyChange();
    })
    .catch(() => {
      /* Tauri IPC 브리지가 없는 환경 — live 상태를 false로 둔 채 조용히 넘어간다 */
    });
}

// 앱 시작 직후 첫 렌더 전에 로컬 개발 전용 자동 로그인을 한 번 시도한다
// (authApi.ts/src-tauri/src/dev_auto_login.rs 참고). 이 await가 끝나기 전에는
// handleNavigation()을 부르지 않으므로, 자동 로그인이 성공하는 환경에서는 로그인
// 화면이 잠깐이라도 그려지지 않는다. 릴리스 빌드이거나 로컬 설정값이 없으면
// tryDevAutoLogin()이 null을 반환해 아래 분기가 아무 일도 하지 않고, 기존 Google
// 로그인 플로우로 그대로 이어진다.
//
// `import.meta.env.DEV` 가드(F7): Rust 쪽 `#[cfg(debug_assertions)]`(dev_auto_login.rs)
// 와 축을 맞춘 프런트 쪽 게이트다. Vite는 프로덕션 빌드 시 `import.meta.env.DEV`를
// `false` 리터럴로 치환하므로, Rollup이 이 if 블록 전체를 죽은 코드로 트리쇼킹해
// 프로덕션 번들에서 아예 제거한다 — 모든 사용자가 시작 시 겪던 무의미한 IPC 왕복
// 1회(릴리스 빌드에서도 `dev_auto_login` 커맨드는 등록되어 있어 호출 자체는
// 성공하고 항상 None만 돌아온다)가 사라진다.
async function bootstrap(): Promise<void> {
  if (import.meta.env.DEV) {
    const devLogin = await tryDevAutoLogin();
    if (devLogin) {
      applyAuthenticatedIdentity(devLogin.email, devLogin.name);
    }
  }
  handleNavigation();
  initLiveWatchers();
  // 자동 업데이트 체크는 여기서 직접 부르지 않는다 — handleNavigation() 안의
  // 인증 게이트를 거쳐야 하기 때문이다(위 updateCheckStarted 플래그 분기 참고).
  // dev 자동 로그인이 성공한 경우 바로 위 handleNavigation() 호출 시점에 이미
  // 트리거된다.
}

onStateChange(renderApp);
window.addEventListener('hashchange', handleNavigation);
window.addEventListener('DOMContentLoaded', () => {
  void bootstrap();
});
