// 시나리오별 픽스처 조립 — 실 로컬 데이터(real-data.mjs)를 기반으로 "정상"
// 픽스처를 만들고, 엣지케이스는 그 위에 델타를 얹어 파생시킨다.
import {
  loadInstalledPlugins,
  loadKnownMarketplaces,
  loadGlobalCatalog,
  loadWorkspaceProjects,
  loadMalgnAgentConfig,
  loadAutonomyGroups,
  loadSessionSamples,
  notes,
} from './real-data.mjs';

export { notes };

function runtimeFor(projectPath, taskId, overrides = {}) {
  return {
    projectPath,
    taskId,
    running: false,
    lastStartedAt: null,
    lastFinishedAt: null,
    nextRunAt: null,
    status: null,
    summary: null,
    durationMs: null,
    logPath: null,
    ...overrides,
  };
}

function historyEntries(n, opts = {}) {
  const out = [];
  const base = Date.now() - 60_000;
  for (let i = 0; i < n; i++) {
    const started = base - i * 3600_000;
    out.push({
      startedAt: new Date(started).toISOString(),
      finishedAt: new Date(started + 45_000).toISOString(),
      durationMs: 45_000 + i * 1000,
      result: opts.results ? opts.results[i % opts.results.length] : i % 5 === 0 ? 'failed' : 'success',
      logPath: `/Users/hopegiver/workspace/dummy/.claude/logs/autonomy/2026-09-19/run-${i}.log`,
    });
  }
  return out;
}

// mcp_list는 `claude mcp list`가 네트워크 검증 때문에 수 초~수십 초 걸려
// 하네스에서 실사용하기엔 너무 느리고 불안정했다(실측: 20초 타임아웃 후에도
// 미응답) — 조직 표준 malgnai-hub 고정행 + 대표적인 stdio/http 서버 조합을
// 합성 데이터로 구성한다(그 사실을 note에 남긴다).
notes.push('mcp_list: `claude mcp list` CLI가 네트워크 검증으로 수십 초 이상 걸려 하네스에서 실사용 불가 — 합성 데이터로 대체');
function mcpServersSample() {
  return [
    { name: 'plugin:malgn-agent:malgnai-hub', target: 'https://malgnai-hub.malgnsoft.com/mcp', transport: 'http', connected: true, statusLabel: '✔ Connected' },
    { name: 'github', target: 'https://api.githubcopilot.com/mcp/', transport: 'http', connected: false, statusLabel: '⏸ Pending approval' },
    { name: 'filesystem-local', target: 'npx -y @modelcontextprotocol/server-filesystem', transport: 'stdio', connected: true, statusLabel: '✔ Connected' },
  ];
}

function mcpCatalogSample(installedNames) {
  const all = [
    { id: 'gmail', label: 'Gmail', transport: 'http', target: 'https://gmail-mcp.example.com' },
    { id: 'google-drive', label: 'Google Drive', transport: 'http', target: 'https://drive-mcp.example.com' },
    { id: 'google-calendar', label: 'Google Calendar', transport: 'http', target: 'https://calendar-mcp.example.com' },
    { id: 'atlassian', label: 'Atlassian (Jira/Confluence)', transport: 'sse', target: 'https://atlassian-mcp.example.com' },
  ];
  return all.map((e) => ({ ...e, installed: installedNames?.includes(e.label) ?? false }));
}

function devToolsSample() {
  notes.push('check_dev_tools: 5개 CLI 실제 --version 조회는 Rust 몫이라 재현 불가 — 합성 데이터');
  return [
    { id: 'claude', name: 'Claude Code', installed: true, version: '2.1.0', path: '/Users/hopegiver/.local/bin/claude', installMethod: 'native', actionKind: 'manual', manualHint: null },
    { id: 'node', name: 'Node.js', installed: true, version: '22.9.0', path: '/usr/local/bin/node', installMethod: 'brew', actionKind: 'manual', manualHint: null },
    { id: 'gh', name: 'GitHub CLI', installed: false, version: null, path: null, installMethod: null, actionKind: 'none', manualHint: null },
  ];
}

function dailyUsageSample() {
  notes.push('get_daily_usage: jsonl 전체 재계산은 Rust 몫이라 재현 불가 — 합성 데이터(최근 3일)');
  const out = [];
  for (let i = 0; i < 3; i++) {
    const d = new Date(Date.now() - i * 86400_000).toISOString().slice(0, 10);
    out.push({ date: d, inputTokens: 12000 + i * 500, outputTokens: 3400 + i * 100, cacheCreationTokens: 800, cacheReadTokens: 52000 });
  }
  return out;
}

/**
 * 모든 화면 진입 시 main.ts가 즉시 병렬로 호출하는 커맨드들의 "정상" 기본값을
 * 실제 로컬 데이터로 채운 베이스 픽스처. 개별 시나리오는 이 위에 필요한
 * 커맨드만 override해서 쓴다(deepOverride).
 */
export async function buildBaseFixtures() {
  const [plugins, marketplaces, globalCatalog, projects, config] = await Promise.all([
    loadInstalledPlugins(),
    loadKnownMarketplaces(),
    loadGlobalCatalog(),
    loadWorkspaceProjects(),
    loadMalgnAgentConfig(),
  ]);
  const autonomyGroups = await loadAutonomyGroups(projects);
  const sessionSamples = await loadSessionSamples(6);

  const sessions = sessionSamples.map((s, i) => ({
    sessionId: s.sessionId,
    cwd: s.cwd,
    title: s.title,
    name: `${s.cwd.split('/').pop() ?? 'project'}-${i}`,
    version: '2.1.0',
    kind: 'cli',
    startedAt: s.startedAt,
    updatedAt: s.updatedAt,
    running: i === 0, // 최소 1건은 "실행 중" 배지 케이스를 실제로 그려본다
  }));

  const runtimeStatuses = [];
  for (const g of autonomyGroups) {
    for (const t of g.tasks) {
      runtimeStatuses.push(runtimeFor(g.projectPath, t.id, t.enabled ? { nextRunAt: new Date(Date.now() + 3600_000).toISOString() } : {}));
    }
  }

  const appLinksLimits = { maxLinks: 20, maxNameLength: 40, maxUrlLength: 2048, allowedSchemes: ['https', 'http'] };

  return {
    dev_auto_login: { email: 'dev@malgnsoft.com', name: '개발자' },
    'plugin:updater|check': null,
    // sidebar.ts가 렌더 시점에 @tauri-apps/api/app의 getVersion()을 한 번
    // 호출해 사이드바 하단에 앱 버전을 표시한다(내부적으로
    // invoke('plugin:app|version')) — 실제 tauri.conf.json 값과 무관하게
    // 픽스처 문자열이면 충분하다(하네스는 TS/DOM 층만 검증).
    'plugin:app|version': '0.2.8',
    // Rust list_workspace_projects()가 { projects, skipped } 구조로 바뀌었다
    // (제외 사유를 프런트까지 올리는 변경, hub 이슈 01m2wm499xmh3046rnvx4cyn8n) —
    // 베이스 픽스처는 실 데이터라 skipped를 알 수 없어 빈 배열로 둔다. 빈
    // 상태에서 제외 사유가 보이는지는 개별 시나리오가 override해서 확인한다.
    list_workspace_projects: { projects, skipped: [] },
    list_claude_sessions: sessions,
    check_dev_tools: devToolsSample(),
    list_installed_plugins: plugins,
    list_global_catalog: globalCatalog,
    list_known_marketplaces: marketplaces,
    get_daily_usage: dailyUsageSample(),
    autonomy_list: autonomyGroups,
    autonomy_runtime_status: runtimeStatuses,
    // D1③ — 스케줄러 heartbeat. main.ts가 세션당 항상 loadAutonomousTasks()를
    // 부팅 시 먼저 불러오고(홈 위젯 프리로드, route 무관), 그 안에서
    // ensureSchedulerHealthWatcher()가 최초 1회 즉시 조회한다 — 그 시점 라우트가
    // home/tasks-*(isAutonomyRouteActive)면 실제로 이 커맨드가 호출된다.
    // otel_settings_get과 같은 이유로 베이스에 "정상" 기본값을 채워 모든 기존
    // 시나리오에서 UNSTUBBED_COMMAND 오탐과 불필요한 정체 배너를 막는다 — 정체
    // 상태 자체는 이 흐름 전용 시나리오(autonomousTasks.mjs의 scheduler-* 시나리오)가
    // override해서 검증한다.
    autonomy_scheduler_health: { lastTickAt: new Date().toISOString(), now: new Date().toISOString(), tickSeconds: 10 },
    mcp_list: mcpServersSample(),
    mcp_catalog_list: mcpCatalogSample(mcpServersSample().map((s) => s.name)),
    app_links_get: {
      ok: true,
      error: null,
      filePath: '/Users/hopegiver/.claude/malgn-agent-apps.json',
      fileExists: false,
      links: [
        { id: 'link-1', name: '사내 위키', url: 'https://wiki.malgnsoft.internal', enabled: true },
        { id: 'link-2', name: '레거시 사내망(암호화 없음)', url: 'http://legacy.malgnsoft.internal', enabled: false },
      ],
      warnings: [],
      limits: appLinksLimits,
    },
    otel_settings_get: {
      settingsPath: '/Users/hopegiver/.claude/settings.json',
      fileExists: true,
      parseError: null,
      managedKeys: ['CLAUDE_CODE_ENABLE_TELEMETRY'],
      readOnlyKeys: [],
      // main.ts:ensureOtelAutoConfigured()는 로그인 후 세션당 1회 무조건
      // otel_settings_get을 호출하고, 'CLAUDE_CODE_ENABLE_TELEMETRY' 키가
      // values에 없으면 otel_settings_save까지 자동 호출한다(harness가 스텁하지
      // 않은 커맨드라 그대로 두면 매 시나리오마다 UNSTUBBED_COMMAND
      // console.error가 섞여 들어간다) — 이미 설정된 것으로 응답해 그 자동저장
      // 경로를 애초에 타지 않게 한다. 5개 대상 흐름과 무관한 잡음 제거 목적.
      values: { CLAUDE_CODE_ENABLE_TELEMETRY: '0' },
      defaults: {},
      endpointDefaultsInjected: true,
    },
    malgn_agent_config_get: config,
    github_status: { connected: false, username: null },
    cloudflare_status: { connected: false, accountEmail: null },
    // ---- 사용자가 명시적으로 트리거하는 액션 커맨드 (자동 로드되지 않음) ----
    // main.ts가 부팅 시 자동 호출하지 않으므로 "정상" 기본값을 여기 한 곳에
    // 모아두면 각 시나리오가 반복해서 선언하지 않아도 된다 — 실패/지연 경로를
    // 보려는 시나리오만 override한다.
    update_plugin: { success: true, message: '업데이트 완료(테스트 기본값)' },
    install_plugin: { success: true, message: '설치 완료(테스트 기본값)' },
    refresh_marketplaces: { success: true, message: '새로고침 완료(테스트 기본값)' },
    add_marketplace: { success: true, message: '추가 완료(테스트 기본값)' },
    remove_marketplace: { success: true, message: '제거 완료(테스트 기본값)' },
    // invoke<void> 커맨드는 실제로도 JSON null(Rust 유닛 타입)이 내려온다 —
    // 그리고 Playwright의 addInitScript(fn, arg)는 arg를 JSON.stringify로
    // 직렬화하므로 값이 `undefined`인 키는 통째로 사라져(JSON에 undefined가
    // 없다) hasOwnProperty 체크가 "스텁 안 됨"으로 오판한다(실측: mcp_add를
    // undefined로 뒀더니 UNSTUBBED_COMMAND로 잘못 잡히고, 그 결과 모달이 계속
    // 열려 있다가 다음 클릭이 오버레이에 막히는 연쇄 오탐까지 발생했다) — 그래서
    // "값 없음"은 항상 undefined가 아니라 null로 표기한다.
    mcp_add: null,
    mcp_remove: null,
    mcp_login: { opened: true, message: '터미널을 열었습니다(테스트 기본값)' },
    mcp_logout: null,
    mcp_install: { opened: true, message: '터미널을 열었습니다(테스트 기본값)' },
    app_links_save: null, // 시나리오가 필요에 따라 갱신된 AppLinksStatus로 override
    app_links_open: null,
    start_new_session_message: null,
    read_session_transcript: null,
    send_session_message: null,
    cancel_session_turn: null,
    open_claude_login_terminal: { opened: true, message: '터미널을 열었습니다(테스트 기본값)' },
    // claude_auth.rs(앱 안 claude 로그인) — start/cancel은 spawn 확인까지만
    // 하는 invoke<void>라 위 규약대로 null. check_claude_auth_status는 지금
    // 어떤 화면도 직접 호출하지 않지만(보너스 커맨드, UI 미연결) 기본값을
    // 여기 채워둔다 — 향후 대시보드 작업이 호출을 추가해도 이 파일을 먼저
    // 찾아보지 않고 바로 UNSTUBBED_COMMAND에 부딪히지 않게.
    start_claude_auth_login: null,
    cancel_claude_auth_login: null,
    check_claude_auth_status: { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null },
    // @tauri-apps/plugin-opener의 openUrl()이 내는 invoke — 기존 브라우저
    // 오픈은 전부 Rust(`app.opener().open_url()`)에서 직접 호출해 JS 쪽
    // invoke가 없었다(google_oauth.rs 등). 로그인 URL "링크 열기" 폴백
    // 버튼(views/sessions.ts)이 이 저장소 최초로 JS에서 이 플러그인 커맨드를
    // 호출한다.
    'plugin:opener|open_url': null,
    autonomy_save_task: null,
    autonomy_delete_task: null,
    autonomy_set_enabled: null,
    autonomy_run_now: null,
    autonomy_task_history: [],
  };
}

export function withOverrides(base, overrides) {
  return { ...base, ...overrides };
}

export { historyEntries, mcpCatalogSample, mcpServersSample };

// ---------------- 대량(100+) 합성 데이터 생성기 ----------------
export function manyAutonomyGroups(n) {
  const groups = [];
  const runtimes = [];
  for (let i = 0; i < n; i++) {
    const projectPath = `/Users/hopegiver/workspace/synthetic-project-${i}`;
    const taskId = `synthetic-task-${i}`;
    groups.push({
      projectPath,
      projectName: `synthetic-project-${i}`,
      tasks: [
        {
          id: taskId,
          name: `대량테스트 자율업무 #${i} — 한글/특수문자 테스트 <script>alert(1)</script> 🚀`,
          prompt: `이것은 대량 렌더 성능/레이아웃 검증용 프롬프트입니다. 매우 긴 문자열이 레이아웃을 깨뜨리는지 확인합니다. `.repeat(3),
          subagent: i % 3 === 0 ? 'malgn-agent:qa-engineer' : null,
          interval: 60,
          scheduleMode: 'interval',
          atTime: null,
          days: [],
          hourlyMinute: null,
          cron: null,
          enabled: i % 2 === 0,
          timeout: null,
        },
      ],
    });
    runtimes.push(runtimeFor(projectPath, taskId, i % 7 === 0 ? { running: true, lastStartedAt: new Date().toISOString() } : {}));
  }
  return { groups, runtimes };
}

export function manySessions(n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push({
      sessionId: `synthetic-session-${i}`,
      cwd: `/Users/hopegiver/workspace/synthetic-project-${i % 10}`,
      title: i === 0 ? '매우 긴 제목 테스트: ' + '가나다라마바사아자차카타파하 '.repeat(10) : `합성 세션 #${i}`,
      name: `synthetic-${i}`,
      version: '2.1.0',
      kind: 'cli',
      startedAt: Date.now() - i * 60_000,
      updatedAt: Date.now() - i * 30_000,
      running: i < 3,
    });
  }
  return out;
}

export function manyMcpServers(n) {
  const out = [
    { name: 'plugin:malgn-agent:malgnai-hub', target: 'https://malgnai-hub.malgnsoft.com/mcp', transport: 'http', connected: true, statusLabel: '✔ Connected' },
  ];
  for (let i = 0; i < n - 1; i++) {
    out.push({ name: `synthetic-mcp-${i}`, target: `https://mcp-${i}.example.com`, transport: i % 2 === 0 ? 'http' : 'stdio', connected: i % 3 !== 0, statusLabel: i % 3 !== 0 ? '✔ Connected' : '⏸ Pending approval' });
  }
  return out;
}

export function manyAppLinks(n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push({ id: `link-${i}`, name: `링크 ${i}`, url: i % 5 === 0 ? `http://insecure-${i}.example.com` : `https://link-${i}.example.com`, enabled: i % 2 === 0 });
  }
  return out;
}

export function manyPlugins(n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push({
      id: `synthetic-plugin-${i}@dummy-marketplace`,
      name: `synthetic-plugin-${i}`,
      displayName: i === 0 ? '긴 이름 테스트 플러그인 — ' + '가나다라'.repeat(10) : null,
      version: `0.${i}.0`,
      description: `합성 플러그인 #${i} 설명`,
      installPath: `/nonexistent/synthetic-${i}`,
      agents: [{ id: 'a1', name: 'agent-one', description: '' }],
      skills: [],
      knowledge: [],
    });
  }
  return out;
}
