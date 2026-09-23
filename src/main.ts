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
import { el, captureActiveFocusForRerender, flushPendingFieldFocus, showToast } from './dom';
import { state, onStateChange, notifyChange, applyAuthenticatedIdentity } from './state';
import { parseRoute } from './route';
import { renderTabstrip, renderSidebar, renderStatusline } from './sidebar';
import { renderLoginView } from './views/login';
import { renderHomeView, loadClaudeAuthStatus } from './views/home';
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
  leaveMcpSettingsView,
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
  renderClaudeAuthLoginModal,
  renderClaudeAuthLoginReopenBanner,
} from './views/sessions';
import { onSessionsChanged, onClaudeAuthLoginUrl, onClaudeAuthLoginOutput, onClaudeAuthLoginFinished } from './sessionsApi';
import {
  renderAutonomousTasksListView,
  renderAutonomousTaskBoardView,
  renderAutonomousTaskDetailView,
  loadAutonomousTasks,
  leaveAutonomousTasksListView,
} from './views/autonomousTasks';
import { loadAppLinks, leaveAppLinksView } from './views/appLinks';
import { tryDevAutoLogin } from './authApi';
import { initUpdateCheck } from './updateApi';

// 순수 렌더 — 상태를 바꾸지 않는다. onStateChange(renderApp)로 구독되어 있어
// notifyChange() 한 번으로 항상 최신 상태가 반영된다.
function renderApp(): void {
  const root = document.getElementById('app');
  if (!root) return;
  // A2(리뷰 v0.2.5) — root.replaceChildren() 직전에 "지금 어떤 필드가 포커스를
  // 갖고 있는가"를 스냅샷한다(dom.ts 참고). replaceChildren() 자체가 포커스된
  // 옛 노드를 지우며 즉시 blur를 일으키므로, 그 전에 떠야 한다.
  captureActiveFocusForRerender();
  root.replaceChildren();

  if (!state.authenticated) {
    root.appendChild(renderLoginView());
    flushPendingFieldFocus();
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
  // Terminus 셸: 탭스트립(상단) → shell-body{사이드바+본문}(가운데) → 상태줄
  // (하단)을 세로로 쌓는다(#app이 flex-direction:column, styles.css 참고).
  root.appendChild(renderTabstrip(route));
  root.appendChild(el('div', { className: 'shell-body' }, [renderSidebar(route), main]));
  root.appendChild(renderStatusline(route));
  // 앱 안 claude 로그인 — 라우트와 무관하게 state.claudeAuthLogin(modalOpen/
  // active/error) 하나만으로 앱 셸(#app) 최상위에 한 번만 그린다. 이전엔
  // 전역 고정 패널(position:fixed)이었지만, v0.2.13 후속 작업(요구사항 2)에서
  // 이 저장소의 기존 모달 관례(dom.ts createModalOverlay)를 쓰는 모달로
  // 바꿨다 — 닫아도(모달만 숨김) 백엔드 로그인은 계속 진행되고, 화면 어디서나
  // 보이는 작은 재진입 배너(renderClaudeAuthLoginReopenBanner)가 "계속하기"
  // 통로를 남긴다. 이전엔 세션 상세/draft 화면의 bottomFixed 안에만 있어서,
  // 로그인을 시작할 수 있는 진입점이 대시보드 위젯으로 넓어지자 대시보드·
  // 세션목록처럼 그 자리 자체가 없는 화면으로 이동하면 입력창이 사라지는
  // 갇힘이 재현됐다(937fde9와 같은 종류, 범위만 좁아짐) — 라우트와 무관하게
  // 여기서 그리는 것으로 그 갇힘을 다시 만들지 않는다. views/sessions.ts는
  // 더 이상 이 블록을 자기 bottomFixed에 중복 렌더하지 않는다.
  for (const panel of renderClaudeAuthLoginModal()) root.appendChild(panel);
  for (const panel of renderClaudeAuthLoginReopenBanner()) root.appendChild(panel);
  // 새 트리가 문서에 완전히 붙은 뒤에만 focus()가 먹는다(dom.ts 참고).
  flushPendingFieldFocus();
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

  // G1(리뷰 v0.2.5) — 라우트 이탈 정리는 인증 여부와 무관하게 항상 돈다.
  // 이전엔 이 블록 전체가 아래 `!state.authenticated` 조기 return 뒤에
  // 있어서, 로그아웃(=state.authenticated를 false로 바꾸고 해시를 리셋하는
  // 것도 "라우트 이탈"이다) 시 leaveSessionChatView() 등이 한 번도 실행되지
  // 않았다(실측: 세션 상세에서 로그아웃해도 plugin:event|unlisten 호출 0건).
  // 그 결과 채팅 스트리밍 리스너가 살아남고 state.sessionChat.transcript에
  // 이전 사용자의 대화 전문이 메모리에 남았다. "인증 여부와 무관하게 라우트
  // 이탈 정리는 항상 돈다"로 계약을 바꾸기 위해 이 블록을 조기 return보다
  // 앞으로 옮긴다 — 단, 실제 데이터를 새로 불러오는
  // enterSessionChatView/enterSessionDraftView는 인증된 세션에서만 호출한다
  // (로그인 화면에서 새 IPC 왕복을 열 이유가 없다 — leaveSessionChatView()로
  // 정리만 하고 재진입은 하지 않는다).
  const route = parseRoute();
  if (route.kind === 'sessions-detail') {
    if (state.sessionChat.sessionId !== route.sessionId) {
      leaveSessionChatView();
      if (state.authenticated) void enterSessionChatView(route.sessionId);
    }
  } else if (route.kind === 'sessions-draft') {
    if (state.sessionChat.draftProjectPath !== route.projectPath || state.sessionChat.sessionId !== null) {
      leaveSessionChatView();
      if (state.authenticated) void enterSessionDraftView(route.projectPath);
    }
  } else if (state.sessionChat.sessionId !== null || state.sessionChat.draftProjectPath !== null) {
    leaveSessionChatView();
  }
  // 자율업무/프로젝트 화면의 설정 모달은 각 화면의 목록 뷰에서만 렌더된다 —
  // 그 라우트를 완전히 벗어나면(로그아웃 포함) 열려 있던 모달의 ESC 리스너가
  // window에 남지 않도록 매번 정리한다(sessionChat 리스너 해제와 같은 원칙).
  // 모달이 닫혀 있던 경우엔 아무 일도 하지 않는다.
  if (route.kind !== 'tasks-list') leaveAutonomousTasksListView();
  if (route.kind !== 'projects-list') leaveProjectsListView();
  if (route.kind !== 'sessions-list') leaveSessionsListView();
  // MCP 관리/앱링크 설정은 둘 다 'settings' 라우트의 서로 다른 탭이다
  // (#/settings/mcp, #/settings/applinks) — route.kind만 보면 탭 간 이동
  // (mcp ↔ applinks)에서는 모달이 닫히지 않으므로 탭까지 함께 확인한다.
  if (!(route.kind === 'settings' && route.tab === 'mcp')) leaveMcpSettingsView();
  if (!(route.kind === 'settings' && route.tab === 'applinks')) leaveAppLinksView();

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
  if (!state.claudeAuth.loaded && !state.claudeAuth.loading) void loadClaudeAuthStatus();
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

// 앱 안 claude CLI 로그인(claude_auth.rs) 이벤트 구독 — state.claudeAuthLogin
// 설명 참고: 세션 채팅 화면의 진입/이탈과 무관하게 앱 시작 시 한 번만
// 구독한다. 이 흐름은 백엔드가 세션에 묶이지 않는 전역 단일 슬롯으로
// 관리하므로, sessionChat처럼 화면별로 구독을 걸고 떼면 사용자가 로그인
// 진행 중 다른 화면으로 이동했을 때 완료 이벤트를 영영 놓친다.
//
// v0.2.11(hub 이슈 01m33qe0zhn55mczhcdgec2b61)에서 stdin=/dev/null로 사용자를
// 멈춘 화면에 가두는 사고를 낸 뒤 `03b8c24`로 이 구독 자체를 껐었다 —
// claude_auth.rs의 근본 수정(stdin 파이프 + 상시 코드 입력창, 감지 분기
// 제거)과 함께 되살린다.
let claudeAuthLoginWatcherInitialized = false;
function initClaudeAuthLoginWatcher(): void {
  if (claudeAuthLoginWatcherInitialized) return;
  claudeAuthLoginWatcherInitialized = true;

  onClaudeAuthLoginUrl((d) => {
    state.claudeAuthLogin.url = d.url;
    notifyChange();
  }).catch(() => {
    /* Tauri IPC 브리지가 없는 환경(플레인 브라우저) — 조용히 넘어간다 */
  });

  // 자식 stdout 원문을 누적한다(줄 경계 없음 — claude_auth.rs pump_stdout이
  // 보내는 그대로). 이 값을 보고 입력창을 띄울지 말지 정하지 않는다 — 입력창
  // 자체는 views/sessions.ts가 login.active만으로 항상 그린다.
  onClaudeAuthLoginOutput((d) => {
    state.claudeAuthLogin.output += d.text;
    notifyChange();
  }).catch(() => {
    /* Tauri IPC 브리지가 없는 환경(플레인 브라우저) — 조용히 넘어간다 */
  });

  // v0.2.13 실사용자 보고 후속(2026-09-23) — 이 핸들러가 그 버그(보고 1번)의
  // 진원지였다: 인증은 완전히 성공했는데 대시보드는 "연결 안 됨"으로 남고
  // 채팅 화면엔 옛 에러가 그대로 있었다. 원인 둘:
  // (a) 이 핸들러가 대시보드 위젯이 읽는 state.claudeAuth를 전혀 갱신하지
  //     않았다 — main.ts:handleNavigation()의 `!state.claudeAuth.loaded` 가드는
  //     이미 loaded===true인 이후로는 다시 불리지 않으므로, 로그인이 끝나도
  //     대시보드는 앱을 재시작하기 전까지 예전 값을 계속 보여줬다.
  // (b) 채팅의 authError/error를 지우는 조건이 백엔드가 보낸 `d.ok` 불리언
  //     하나뿐이었다. `d.ok`는 claude_auth.rs의 run_login이 로그인 직후
  //     `claude auth status --json`을 다시 물어본 결과로 정해지는데, 그
  //     재확인 조회 자체가 실패("확인 불가")해도 run_login은 그 경우를
  //     `ok:false`로 뭉뚱그려 보냈다(parse_claude_auth_status가 파싱 실패를
  //     "로그인 안 됨"과 같은 값으로 떨어뜨렸던 것과 같은 종류의 문제,
  //     claude_auth.rs 주석 참고). 그 결과 실제로는 성공한 로그인이 화면에는
  //     실패로 보였다.
  //
  // 수정: `d.ok`를 신뢰하지 않는다 — 대신 이 이벤트가 함께 실어 보내는 최신
  // 조회 결과(`d.status`)만 근거로 삼는다. 이 판단은 백엔드의 ok 산정 로직이
  // 앞으로 바뀌어도 항상 "지금 실제로 로그인됐는가" 하나만 본다는 점에서
  // 구조적으로 더 안전하다.
  onClaudeAuthLoginFinished((d) => {
    state.claudeAuthLogin.active = false;
    state.claudeAuthLogin.url = null;
    state.claudeAuthLogin.codeInput = '';

    if (d.canceled) {
      state.claudeAuthLogin.modalOpen = false;
      state.claudeAuthLogin.error = null;
      state.claudeAuthLogin.outcome = null;
      state.claudeAuthLogin.output = '';
      state.claudeAuthLogin.outputExpanded = false;
      showToast('로그인을 취소했습니다.');
      notifyChange();
      return;
    }

    // 대시보드 위젯(claudeAuth)을 즉시 갱신한다(요구사항: "로그인이 끝나면
    // 대시보드 인증 상태가 즉시 갱신돼야 한다") — status가 함께 왔으면(백엔드
    // 재확인 성공) 그 값을 바로 반영해 다음 렌더에서 곧장 보이게 하고, 없으면
    // (재확인 자체가 실패한 "확인 불가" 케이스) 별도로 한 번 더 조회한다 —
    // 읽기 전용 조회라 재시도해도 부작용이 없다.
    if (d.status) {
      state.claudeAuth.status = d.status;
      state.claudeAuth.loaded = true;
      state.claudeAuth.error = null;
    } else {
      void loadClaudeAuthStatus();
    }

    const confirmedLoggedIn = d.status?.loggedIn === true;

    if (confirmedLoggedIn) {
      state.claudeAuthLogin.modalOpen = false;
      state.claudeAuthLogin.error = null;
      state.claudeAuthLogin.outcome = null;
      state.claudeAuthLogin.output = '';
      state.claudeAuthLogin.outputExpanded = false;
      // 완료 후 같은 화면에서 이어서 작업할 수 있어야 한다 — 인증 실패
      // 배너(있었다면)를 닫는다. 화면이 그대로면 또 실패로 오인한다. 이
      // 판단은 위에서 구한 confirmedLoggedIn(=최신 조회 결과) 근거다.
      state.sessionChat.authError = false;
      state.sessionChat.error = null;
      showToast('claude 로그인이 완료되었습니다.');
    } else {
      // 실패(status 확인됨, loggedIn:false) 또는 확인 불가(status 없음) —
      // "없음"을 "실패"로 단정하지 않고 문구를 구분한다(outcome). 모달은
      // 자동으로 닫지 않는다 — 방금 쌓인 원문 로그가 원인 추적의 유일한
      // 단서일 수 있다(요구사항 3, sessions.ts renderClaudeAuthLoginOutputToggle).
      state.claudeAuthLogin.outcome = d.status ? 'failed' : 'unknown';
      state.claudeAuthLogin.error = d.error ?? (d.status ? '로그인에 실패했습니다.' : '로그인 결과를 확인하지 못했습니다.');
      state.claudeAuthLogin.outputExpanded = true;
    }
    notifyChange();
  }).catch(() => {
    /* Tauri IPC 브리지가 없는 환경(플레인 브라우저) — 조용히 넘어간다 */
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
  initClaudeAuthLoginWatcher();
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
