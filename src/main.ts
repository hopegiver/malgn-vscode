// 앱 엔트리 — 해시 라우팅으로 모든 화면을 연결한다.
//
// ⚠️ 이 앱은 대부분 UI/UX 검증용 목업이다(하드코딩된 샘플 데이터, mockData.ts).
// 다음은 실제 로컬 데이터를 Rust 커맨드로 읽어온다(전부 읽기 전용):
//   - "프로젝트"(views/projects.ts): ~/workspace 스캔 + 폴더 구조/파일 미리보기 — workspaceApi.ts
//   - "세션목록"(views/sessions.ts): ~/.claude/sessions/*.json + 대화 로그 첫 줄 제목 — sessionsApi.ts
//   - "개발 환경"(views/devTools.ts): claude/node/gh/git/pnpm/wrangler/docker --version — devToolsApi.ts
//   - "카탈로그"(views/catalog.ts): installed_plugins.json(user scope) + 실물 agents/skills/knowledge — catalogApi.ts
//   - "마켓플레이스 설정": known_marketplaces.json — catalogApi.ts
//   - "OTel 설정": ~/.claude/settings.json의 env.OTEL_* — otelApi.ts
//   - "사용량 통계"의 일별 사용량과 날짜별 상세: ~/.claude/projects/**/*.jsonl 집계 — usageApi.ts/dailyDetailApi.ts
//   - "GitHub/Cloudflare 설정": 이 앱은 토큰을 취급하지 않는다. 상태는 `gh`/`wrangler`
//     CLI를 읽기 전용으로 조회하고, 연결/해제는 사용자가 조작할 터미널 창을 여는
//     것뿐이다 — integrationsApi.ts.
//   - "Jira 설정": 사이트 URL·이메일·API 토큰을 실제로 검증(/rest/api/3/myself)한
//     뒤 macOS 키체인에 저장한다 — integrationsApi.ts.
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
import { state, onStateChange, notifyChange } from './state';
import { parseRoute } from './route';
import { renderSidebar } from './sidebar';
import { renderLoginView } from './views/login';
import { renderHomeView } from './views/home';
import { renderProjectsListView, renderProjectsDetailView, loadProjects, loadProjectTree } from './views/projects';
import { renderCatalogView, loadCatalog, loadMarketplaces } from './views/catalog';
import { renderDevToolsView, loadDevTools } from './views/devTools';
import { renderSettingsView, loadOtelEnv, loadGithubStatus, loadCloudflareStatus, loadJiraStatus } from './views/settings';
import { renderUsageView, loadDailyUsage } from './views/usage';
import { renderSessionsListView, renderSessionDetailView, loadSessions } from './views/sessions';
import { onSessionsChanged } from './sessionsApi';
import { renderAutonomousTasksListView, renderAutonomousTaskBoardView, renderAutonomousTaskDetailView } from './views/autonomousTasks';

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
  const banner = el('div', { className: 'mock-banner' }, [
    'UI/UX 검증용 목업 — 로그인(Google OAuth)·프로젝트·세션목록·개발 환경·카탈로그·마켓플레이스·OTel/GitHub/Cloudflare/Jira 설정·사용량 통계(일별 사용량/날짜별 상세)만 실제이고 나머지는 하드코딩된 샘플입니다',
  ]);

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
      content = renderCatalogView();
      break;
    case 'dev-tools':
      content = renderDevToolsView();
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

  const main = el('main', { className: 'content' }, [banner, content]);
  root.appendChild(renderSidebar(route));
  root.appendChild(main);
}

// 네비게이션 진입점 — 렌더 후에 필요한 데이터 로딩을 "따로" 트리거한다(렌더 함수
// 자체 안에서 하면 로딩 콜백이 재귀적으로 renderApp()을 다시 부르는 동안 바깥
// renderApp()이 아직 실행 중인 상태와 겹쳐서 DOM이 꼬일 수 있다).
//
// 프로젝트·세션·개발환경·카탈로그·마켓플레이스는 로그인 직후 한 번씩 미리
// 불러온다 — 홈 대시보드 위젯과 사이드바 펼침 목록이 실제 값을 바로 보여줘야
// 하기 때문이다(로딩/실패 상태는 각 화면이 loaded/loading 플래그로 알아서 처리).
function handleNavigation(): void {
  renderApp();
  if (!state.authenticated) return;

  if (state.dashboard.projects.length === 0 && !state.dashboard.loading) void loadProjects();
  if (!state.sessions.loaded && !state.sessions.loading) void loadSessions();
  if (!state.devTools.loaded && !state.devTools.loading) void loadDevTools();
  if (!state.catalog.loaded && !state.catalog.loading) void loadCatalog();
  if (!state.marketplaces.loaded && !state.marketplaces.loading) void loadMarketplaces();

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
  if (route.kind === 'settings' && route.tab === 'jira' && !state.jira.loaded && !state.jira.loading) {
    void loadJiraStatus();
  }
  if (route.kind === 'projects-detail' && state.projectTree.projectPath !== route.path && !state.projectTree.loading) {
    void loadProjectTree(route.path);
  }
  if (route.kind === 'usage') {
    // 실시간 감시 대신 메뉴 클릭(=이 라우트 진입) 시점마다 새로 불러온다 — 이미
    // 불러오는 중이면 겹쳐 쌓이지 않게 건너뛴다.
    if (!state.dailyUsage.loading) void loadDailyUsage();
  }
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

onStateChange(renderApp);
window.addEventListener('hashchange', handleNavigation);
window.addEventListener('DOMContentLoaded', () => {
  handleNavigation();
  initLiveWatchers();
});
