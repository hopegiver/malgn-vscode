// 캡처 하네스 픽스처 — 45개 Tauri 커맨드에 대해 3가지 데이터 시나리오
// (normal/empty/error)의 응답값을 정의한다. 값은 src/*Api.ts에 선언된 TS
// 인터페이스를 정본으로 삼아 타입에 맞게 작성했고, 한국어 사내 개발 환경에서
// 실제로 나올 법한 값(프로젝트명 malgn-agent/malgnai-hub, 한글 세션 제목,
// 현실적인 토큰 수·날짜)만 쓴다. lorem ipsum·test1/test2류는 쓰지 않는다.
//
// 이 파일은 하네스 전용 산출물이다 — src/를 참조하지 않고 순수 데이터만 담는다.

export const IDS = {
  PROJECT_PATH: '/Users/dev/workspace/malgn-agent',
  PROJECT_PATH_2: '/Users/dev/workspace/malgnai-hub',
  SESSION_ID: 'a3f9d21c-8b7e-4c1a-9e2f-6d5b8a7c4e10',
  SESSION_ID_2: '7e2c9b41-1a3d-4f6e-8c9a-2b5d7e1f9a30',
  TASK_ID: 'daily-usage-report',
  TASK_ID_2: 'status-md-lint',
};

export const USER = { email: 'dev@malgnsoft.com', name: '하근호', hd: 'malgnsoft.com' };

export const LOGIN_ERROR_MESSAGE =
  'malgnsoft.com 조직 계정이 아닙니다 (hd=gmail.com). 회사 Google Workspace 계정으로 다시 로그인하세요.';

function iso(offsetMs) {
  return new Date(Date.now() + offsetMs).toISOString();
}

const MIN = 60 * 1000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

// ---------------- list_workspace_projects ----------------
const PROJECTS_NORMAL = [
  {
    name: 'malgn-agent',
    path: IDS.PROJECT_PATH,
    hasStatus: true,
    archiveStatus: 'active',
    sections: {
      parsed: true,
      current: 'STATUS.md 재작성 규칙 정비 및 캡처 하네스 구축',
      recentDone: '세션목록 정본을 jsonl 대화 이력으로 전환',
      inProgress: 'lib.rs 도메인별 모듈 분할 마무리',
      blocked: null,
      raw: '## 현재\nSTATUS.md 재작성 규칙 정비 및 캡처 하네스 구축\n\n## 최근 완료\n세션목록 정본을 jsonl 대화 이력으로 전환',
    },
    updatedAt: Date.now() - 20 * MIN,
  },
  {
    name: 'malgnai-hub',
    path: IDS.PROJECT_PATH_2,
    hasStatus: true,
    archiveStatus: 'active',
    sections: {
      parsed: true,
      current: 'MCP 서버 응답속도 개선',
      recentDone: 'work_record 인덱싱 최적화',
      inProgress: 'project_search_history 캐시 레이어 추가',
      blocked: 'Cloudflare D1 리전 지연 이슈 대기 중',
      raw: '## 현재\nMCP 서버 응답속도 개선',
    },
    updatedAt: Date.now() - 3 * HOUR,
  },
  {
    name: 'legacy-intranet-portal-maintenance',
    path: '/Users/dev/workspace/legacy-intranet-portal-maintenance',
    hasStatus: true,
    archiveStatus: 'archived',
    sections: { parsed: true, current: null, recentDone: '유지보수 계약 종료', inProgress: null, blocked: null, raw: 'archived' },
    updatedAt: Date.now() - 40 * DAY,
  },
  {
    name: 'poc-realtime-notification',
    path: '/Users/dev/workspace/poc-realtime-notification',
    hasStatus: false,
    archiveStatus: 'unknown',
    sections: null,
    updatedAt: Date.now() - 5 * HOUR,
  },
];

// ---------------- list_project_tree (PROJECT_PATH 기준) ----------------
const PROJECT_TREE_NORMAL = [
  { name: 'CLAUDE.md', relativePath: 'CLAUDE.md', isDirectory: false, children: null, truncated: false },
  { name: 'STATUS.md', relativePath: 'STATUS.md', isDirectory: false, children: null, truncated: false },
  {
    name: 'src',
    relativePath: 'src',
    isDirectory: true,
    truncated: false,
    children: [
      { name: 'main.ts', relativePath: 'src/main.ts', isDirectory: false, children: null, truncated: false },
      { name: 'views', relativePath: 'src/views', isDirectory: true, children: [], truncated: true },
    ],
  },
  { name: 'src-tauri', relativePath: 'src-tauri', isDirectory: true, children: [], truncated: true },
  { name: 'docs', relativePath: 'docs', isDirectory: true, children: [], truncated: false },
];

const FILE_PREVIEW_NORMAL = {
  kind: 'text',
  content: '# malgn-agent\n\n사내 개발자 워크스테이션 프로비저닝 Tauri 데스크톱 앱("맑은에이전트"). malgn-agent의 GUI 프론트엔드.\n',
};

// ---------------- list_claude_sessions / read_session_transcript ----------------
// sessions.ts의 formatTimestamp(ms: number|null)는 epoch 밀리초 숫자를 기대한다
// (asNumber()가 문자열을 걸러낸다) — ISO 문자열을 넣으면 항상 "-"로 표시되는
// 픽스처 버그가 났던 자리라 숫자로만 채운다.
const SESSIONS_NORMAL = [
  {
    sessionId: IDS.SESSION_ID,
    cwd: IDS.PROJECT_PATH,
    title: '세션목록 정본을 jsonl 대화 이력으로 전환',
    startedAt: Date.now() - 90 * MIN,
    updatedAt: Date.now() - 3 * MIN,
    running: true,
    pid: 48213,
    version: '2.0.68',
  },
  {
    sessionId: IDS.SESSION_ID_2,
    cwd: IDS.PROJECT_PATH_2,
    title: 'malgnai-hub work_record 인덱싱 최적화 검토 및 회귀 테스트 결과 정리',
    startedAt: Date.now() - 6 * HOUR,
    updatedAt: Date.now() - 5 * HOUR,
    running: false,
  },
];

const TRANSCRIPT_NORMAL = {
  sessionId: IDS.SESSION_ID,
  cwd: IDS.PROJECT_PATH,
  transcriptPath: `${IDS.PROJECT_PATH}/.claude/sessions/${IDS.SESSION_ID}.jsonl`,
  truncated: false,
  activeTurnId: null,
  messages: [
    { kind: 'user', text: '세션목록 정본을 registry에서 jsonl 대화 이력으로 바꿔줘', toolCount: 0, at: iso(-90 * MIN) },
    { kind: 'assistant', text: '네, sessionsApi.ts의 list_claude_sessions 구현을 jsonl 기반으로 바꾸겠습니다.', toolCount: 0, at: iso(-89 * MIN) },
    { kind: 'tool', text: 'Read src-tauri/src/plugins/session_chat/mod.rs 외 2건', toolCount: 3, at: iso(-85 * MIN) },
    { kind: 'assistant', text: '수정 완료했습니다. 이제 registry가 아니라 jsonl 파일을 정본으로 읽습니다.', toolCount: 0, at: iso(-70 * MIN) },
    { kind: 'user', text: '테스트도 같이 부탁해', toolCount: 0, at: iso(-10 * MIN) },
    { kind: 'assistant', text: '테스트 3건을 추가하고 전부 통과 확인했습니다.', toolCount: 0, at: iso(-3 * MIN) },
  ],
};

const TRANSCRIPT_EMPTY = {
  sessionId: IDS.SESSION_ID,
  cwd: IDS.PROJECT_PATH,
  transcriptPath: `${IDS.PROJECT_PATH}/.claude/sessions/${IDS.SESSION_ID}.jsonl`,
  truncated: false,
  activeTurnId: null,
  messages: [],
};

// ---------------- check_dev_tools ----------------
const DEV_TOOLS_NORMAL = [
  { id: 'claude', name: 'Claude Code', installed: true, version: '2.0.68', path: '/opt/homebrew/bin/claude', installMethod: 'homebrew', actionKind: 'run' },
  { id: 'node', name: 'Node.js', installed: true, version: '22.11.0', path: '/opt/homebrew/bin/node', installMethod: 'homebrew', actionKind: 'run' },
  { id: 'gh', name: 'GitHub CLI', installed: true, version: '2.63.2', path: '/opt/homebrew/bin/gh', installMethod: 'homebrew', actionKind: 'run' },
  {
    id: 'git',
    name: 'Git',
    installed: true,
    version: '2.47.0',
    path: '/usr/bin/git',
    installMethod: 'system',
    actionKind: 'manual',
    manualHint: 'macOS 시스템 git은 이 앱이 대신 업데이트할 수 없습니다. Xcode Command Line Tools를 업데이트하세요.',
  },
  { id: 'pnpm', name: 'pnpm', installed: true, version: '9.12.3', path: '/opt/homebrew/bin/pnpm', installMethod: 'homebrew', actionKind: 'run' },
  {
    id: 'wrangler',
    name: 'Wrangler',
    installed: false,
    version: null,
    path: null,
    installMethod: null,
    actionKind: 'manual',
    manualHint: 'wrangler는 프로젝트별 devDependency로 설치하세요 (pnpm add -D wrangler).',
  },
];

// ---------------- list_installed_plugins / list_global_catalog / list_known_marketplaces ----------------
const PLUGINS_NORMAL = [
  {
    id: 'malgn-agent',
    name: 'malgn-agent',
    displayName: '맑은에이전트',
    version: '1.8.39',
    description: '사내 개발 표준 에이전트·스킬·지식 번들',
    installPath: '/Users/dev/.claude/plugins/malgnsoft-plugins/malgn-agent/1.8.39',
    agents: [
      { id: 'qa-engineer', name: 'qa-engineer', description: '구현된 코드를 실제로 테스트하고 버그를 수정' },
      { id: 'frontend-dev', name: 'frontend-dev', description: '프론트엔드 화면 구현' },
      { id: 'pm', name: 'pm', description: '프로젝트 관리 및 위임' },
    ],
    skills: [
      { id: 'common-screen-verification-and-capture', name: '화면 검증 및 캡처', description: '표준화된 화면 캡처와 검증 체계' },
      { id: 'domain-software-test-design-techniques', name: '테스트 설계 기법', description: '경계값분석·동등분할 등 테스트 설계' },
    ],
    knowledge: [{ id: 'e2e-testing-guide', name: 'E2E 테스트 가이드', description: 'Playwright 기반 E2E 표준' }],
  },
];

const GLOBAL_CATALOG_NORMAL = {
  agents: [{ name: 'personal-notes-agent', description: '개인 메모 정리 보조', path: '/Users/dev/.claude/agents/personal-notes-agent.md', status: 'valid' }],
  skills: [{ name: 'broken-skill', description: '', path: '/Users/dev/.claude/skills/broken-skill', status: 'invalid' }],
};

const MARKETPLACES_NORMAL = [{ id: 'malgnsoft-plugins', repo: 'malgnsoft/malgn-agent-marketplace', lastUpdated: iso(-20 * HOUR) }];

// ---------------- get_daily_usage / get_daily_detail ----------------
function buildDailyUsage() {
  const items = [];
  for (let i = 13; i >= 0; i--) {
    const d = new Date(Date.now() - i * DAY);
    // 앱은 프런트(views/usage.ts localDateKey)·백엔드(usage_stats/daily.rs
    // local_date_key) 모두 로컬 날짜를 쓴다 — 여기서 UTC(toISOString)를 쓰면
    // KST 00~09시 캡처에서 하루 어긋난 키가 생겨 "오늘" 데이터가 사라져 보인다.
    const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    const base = 200_000 + Math.round(Math.sin(i) * 80_000) + (i === 0 ? 400_000 : 0);
    items.push({
      date,
      inputTokens: Math.round(base * 0.3),
      outputTokens: Math.round(base * 0.15),
      cacheCreationTokens: Math.round(base * 0.1),
      cacheReadTokens: Math.round(base * 0.45),
    });
  }
  return items;
}
const DAILY_USAGE_NORMAL = buildDailyUsage();

const DAILY_DETAIL_NORMAL = {
  date: DAILY_USAGE_NORMAL[DAILY_USAGE_NORMAL.length - 1].date,
  sessions: [
    {
      sessionId: IDS.SESSION_ID,
      projectKey: 'malgn-agent',
      title: '세션목록 정본을 jsonl 대화 이력으로 전환',
      totalTokens: 842_000,
      costUsd: 3.15,
      agents: [
        { agentType: 'main', turns: 12, totalTokens: 600_000, costUsd: 2.1 },
        { agentType: 'qa-engineer', turns: 3, totalTokens: 242_000, costUsd: 1.05 },
      ],
      tools: [
        { toolName: 'Read', count: 22 },
        { toolName: 'Edit', count: 9 },
        { toolName: 'Bash', count: 14 },
      ],
    },
  ],
  totalTokens: 842_000,
  costUsd: 3.15,
};

// ---------------- autonomy_list / autonomy_runtime_status / malgn_agent_config_get ----------------
const AUTONOMY_LIST_NORMAL = [
  {
    projectPath: IDS.PROJECT_PATH,
    projectName: 'malgn-agent',
    tasks: [
      {
        id: IDS.TASK_ID,
        name: '일일 사용량 리포트 발송',
        prompt: '어제 하루 팀 전체 토큰 사용량을 집계해 malgnai-hub에 기록하고 요약을 알려줘',
        subagent: 'usage-reporter',
        interval: 1440,
        enabled: true,
        timeout: 30,
      },
      {
        id: IDS.TASK_ID_2,
        name: 'STATUS.md 정합성 점검',
        prompt: 'STATUS.md가 3000바이트를 넘는지, 섹션 형식이 깨지지 않았는지 점검해줘',
        subagent: null,
        interval: 360,
        enabled: false,
        timeout: null,
      },
    ],
  },
];

const AUTONOMY_RUNTIME_NORMAL = [
  {
    projectPath: IDS.PROJECT_PATH,
    taskId: IDS.TASK_ID,
    running: true,
    lastStartedAt: iso(-4 * MIN),
    lastFinishedAt: iso(-1 * DAY),
    nextRunAt: null,
    status: null,
    summary: null,
    durationMs: null,
    logPath: null,
  },
  {
    projectPath: IDS.PROJECT_PATH,
    taskId: IDS.TASK_ID_2,
    running: false,
    lastStartedAt: iso(-30 * HOUR),
    lastFinishedAt: iso(-29 * HOUR - 55 * MIN),
    nextRunAt: null,
    status: 'failed',
    summary: 'STATUS.md가 3412바이트로 상한(3000바이트)을 초과했습니다.',
    durationMs: 8200,
    logPath: '/Users/dev/workspace/malgn-agent/.claude/logs/autonomy/2026-09-12/status-md-lint.log',
  },
];

const MALGN_AGENT_CONFIG_NORMAL = {
  ok: true,
  error: null,
  configPath: '/Users/dev/.claude/malgn-agent.json',
  fileExists: true,
  workspaces: [IDS.PROJECT_PATH, IDS.PROJECT_PATH_2],
  warnings: [],
  autonomy: { concurrency: 2, defaultTimeout: 20 },
  logs: { retentionDays: 30 },
  limits: { minInterval: 5, maxInterval: 1440, minTimeout: 1, maxTimeout: 120, maxConcurrency: 4, startupGraceMinutes: 5 },
};

// ---------------- mcp_list / mcp_catalog_list ----------------
const MCP_LIST_NORMAL = [
  { name: 'plugin:malgn-agent:malgnai-hub', target: 'https://malgnai-hub.malgnsoft.workers.dev/mcp', transport: 'http', connected: true, statusLabel: '✔ Connected' },
  { name: 'sequential-thinking', target: 'npx -y @modelcontextprotocol/server-sequential-thinking', transport: 'stdio', connected: true, statusLabel: '✔ Connected' },
  { name: 'github', target: 'https://api.githubcopilot.com/mcp', transport: 'http', connected: false, statusLabel: '⏸ Pending approval' },
];

const MCP_CATALOG_NORMAL = [
  { id: 'gmail', label: 'Gmail', transport: 'http', target: 'https://mcp.gmail.com', installed: false },
  { id: 'google-drive', label: 'Google Drive', transport: 'http', target: 'https://mcp.google-drive.com', installed: false },
  { id: 'google-calendar', label: 'Google Calendar', transport: 'http', target: 'https://mcp.google-calendar.com', installed: false },
  { id: 'atlassian', label: 'Atlassian (Jira/Confluence)', transport: 'sse', target: 'https://mcp.atlassian.com/sse', installed: true },
];

// ---------------- otel_settings_get / github_status / cloudflare_status / jira_status ----------------
const OTEL_NORMAL = {
  settingsPath: '/Users/dev/.claude/settings.json',
  fileExists: true,
  parseError: null,
  managedKeys: [
    'CLAUDE_CODE_ENABLE_TELEMETRY',
    'OTEL_METRICS_EXPORTER',
    'OTEL_LOGS_EXPORTER',
    'OTEL_EXPORTER_OTLP_ENDPOINT',
    'OTEL_EXPORTER_OTLP_PROTOCOL',
    'OTEL_RESOURCE_ATTRIBUTES',
  ],
  readOnlyKeys: ['OTEL_SERVICE_NAME'],
  values: {
    CLAUDE_CODE_ENABLE_TELEMETRY: '1',
    OTEL_METRICS_EXPORTER: 'otlp',
    OTEL_LOGS_EXPORTER: 'otlp',
    OTEL_EXPORTER_OTLP_ENDPOINT: 'https://otel-collector.malgnsoft.com:4318',
    OTEL_EXPORTER_OTLP_PROTOCOL: 'http/protobuf',
    OTEL_RESOURCE_ATTRIBUTES: 'employee.email=dev%40malgnsoft.com,employee.name=%ED%95%98%EA%B7%BC%ED%98%B8',
  },
  defaults: { OTEL_EXPORTER_OTLP_ENDPOINT: 'https://otel-collector.malgnsoft.com:4318' },
  endpointDefaultsInjected: true,
};

const OTEL_EMPTY = {
  settingsPath: '/Users/dev/.claude/settings.json',
  fileExists: false,
  parseError: null,
  managedKeys: OTEL_NORMAL.managedKeys,
  readOnlyKeys: ['OTEL_SERVICE_NAME'],
  values: {},
  defaults: {},
  endpointDefaultsInjected: false,
};

const GITHUB_NORMAL = { installed: true, connected: true, login: 'hopegiver', name: '하근호', avatarUrl: 'https://avatars.githubusercontent.com/u/9081726?v=4' };
const GITHUB_EMPTY = { installed: true, connected: false, login: null, name: null, avatarUrl: null };

const CLOUDFLARE_NORMAL = { installed: true, connected: true, email: 'dev@malgnsoft.com' };
const CLOUDFLARE_EMPTY = { installed: false, connected: false, email: null };

const JIRA_NORMAL = {
  connected: true,
  site: 'malgnsoft.atlassian.net',
  host: 'malgnsoft.atlassian.net',
  email: 'dev@malgnsoft.com',
  accountId: '5f8a1c2b3d4e5f6a7b8c9d0e',
  displayName: '하근호',
};
const JIRA_EMPTY = { connected: false };

// ---------------- 시나리오별 값 맵 (읽기 커맨드) ----------------
export const READ_FIXTURES = {
  normal: {
    google_oauth_login: USER,
    list_workspace_projects: PROJECTS_NORMAL,
    list_project_tree: PROJECT_TREE_NORMAL,
    read_project_file: FILE_PREVIEW_NORMAL,
    list_claude_sessions: SESSIONS_NORMAL,
    read_session_transcript: TRANSCRIPT_NORMAL,
    check_dev_tools: DEV_TOOLS_NORMAL,
    list_installed_plugins: PLUGINS_NORMAL,
    list_global_catalog: GLOBAL_CATALOG_NORMAL,
    list_known_marketplaces: MARKETPLACES_NORMAL,
    get_daily_usage: DAILY_USAGE_NORMAL,
    get_daily_detail: DAILY_DETAIL_NORMAL,
    autonomy_list: AUTONOMY_LIST_NORMAL,
    autonomy_runtime_status: AUTONOMY_RUNTIME_NORMAL,
    malgn_agent_config_get: MALGN_AGENT_CONFIG_NORMAL,
    mcp_list: MCP_LIST_NORMAL,
    mcp_catalog_list: MCP_CATALOG_NORMAL,
    otel_settings_get: OTEL_NORMAL,
    github_status: GITHUB_NORMAL,
    cloudflare_status: CLOUDFLARE_NORMAL,
    jira_status: JIRA_NORMAL,
  },
  empty: {
    google_oauth_login: USER,
    list_workspace_projects: [],
    list_project_tree: [],
    read_project_file: { kind: 'notFound' },
    list_claude_sessions: [],
    read_session_transcript: TRANSCRIPT_EMPTY,
    check_dev_tools: [],
    list_installed_plugins: [],
    list_global_catalog: { agents: [], skills: [] },
    list_known_marketplaces: [],
    get_daily_usage: [],
    get_daily_detail: { date: DAILY_DETAIL_NORMAL.date, sessions: [], totalTokens: 0, costUsd: 0 },
    autonomy_list: [],
    autonomy_runtime_status: [],
    malgn_agent_config_get: { ...MALGN_AGENT_CONFIG_NORMAL, workspaces: [] },
    mcp_list: [],
    mcp_catalog_list: MCP_CATALOG_NORMAL.map((e) => ({ ...e, installed: false })),
    otel_settings_get: OTEL_EMPTY,
    github_status: GITHUB_EMPTY,
    cloudflare_status: CLOUDFLARE_EMPTY,
    jira_status: JIRA_EMPTY,
  },
};

// action(write) 커맨드 — 캡처 흐름에서 직접 클릭하지 않지만, 방어적으로 안전한
// 기본 응답을 채워 예기치 못한 호출에도 하네스가 멈추지 않게 한다.
export const ACTION_DEFAULTS = {
  autonomy_delete_task: null,
  autonomy_save_task: null,
  autonomy_set_enabled: null,
  cancel_session_turn: null,
  cloudflare_connect: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  cloudflare_disconnect: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  github_connect: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  github_disconnect: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  install_dev_tool: { id: 'unknown', outcome: 'updated', verified: true, installMethod: 'homebrew', durationMs: 1200, message: '설치 완료 (캡처 하네스 스텁)', logTail: '', pathVisible: true },
  jira_connect: JIRA_NORMAL,
  jira_disconnect: null,
  mcp_add: null,
  mcp_get: { name: 'stub', scope: 'user', status: 'connected', transport: 'http', target: 'https://example.invalid', oauth: null },
  mcp_install: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  mcp_login: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  mcp_remove: null,
  open_manual_instruction: { opened: true, message: '터미널을 열었습니다 (캡처 하네스 스텁).' },
  otel_settings_save: OTEL_NORMAL,
  preview_dev_tool_update: { id: 'unknown', planId: 'plan-stub', willRun: true, commandDisplay: 'echo stub', affected: [], notes: '', previewReliable: true },
  refresh_marketplaces: { success: true, message: '캡처 하네스 스텁' },
  send_session_message: { turnId: 'stub-turn' },
  start_new_session_message: { sessionId: IDS.SESSION_ID, turnId: 'stub-turn' },
  update_dev_tool: { id: 'unknown', outcome: 'updated', verified: true, installMethod: 'homebrew', durationMs: 1200, message: '업데이트 완료 (캡처 하네스 스텁)', logTail: '', pathVisible: true },
  update_plugin: { success: true, message: '캡처 하네스 스텁' },
};

// ---------------- error 시나리오 메시지 ----------------
export const ERROR_MESSAGES = {
  list_workspace_projects: '~/workspace 디렉터리를 읽는 중 권한이 거부되었습니다 (EACCES).',
  list_project_tree: '프로젝트 경로가 워크스페이스 밖에 있거나 존재하지 않습니다.',
  read_project_file: '파일을 읽는 중 알 수 없는 오류가 발생했습니다.',
  list_claude_sessions: '~/.claude/sessions 디렉터리를 찾을 수 없습니다.',
  read_session_transcript: '세션을 찾을 수 없습니다: 대화 기록 파일이 삭제되었거나 이동되었습니다.',
  check_dev_tools: 'CLI 도구 상태를 확인하는 중 프로세스 실행에 실패했습니다.',
  list_installed_plugins: 'installed_plugins.json 파일을 파싱하지 못했습니다 (JSON 문법 오류).',
  list_global_catalog: '~/.claude/agents 디렉터리를 읽지 못했습니다.',
  list_known_marketplaces: 'known_marketplaces.json 파일을 찾을 수 없습니다.',
  get_daily_usage: '~/.claude/projects 아래 jsonl 파일을 집계하는 중 오류가 발생했습니다.',
  get_daily_detail: '해당 날짜의 사용량 데이터를 집계하지 못했습니다.',
  autonomy_list: '전역 설정 파일(malgn-agent.json)이 손상되었습니다: 예상치 못한 토큰 위치 12행.',
  autonomy_runtime_status: 'Tauri IPC 연결이 끊어졌습니다.',
  malgn_agent_config_get: '전역 설정 파일을 읽을 권한이 없습니다.',
  mcp_list: '`claude mcp list` 명령 실행에 실패했습니다 (command not found).',
  mcp_catalog_list: 'MCP 카탈로그 정의를 불러오지 못했습니다.',
  otel_settings_get: '~/.claude/settings.json 파싱에 실패했습니다: 예상치 못한 문자 위치 45행.',
  github_status: '`gh auth status` 실행에 실패했습니다.',
  cloudflare_status: '`wrangler whoami` 실행에 실패했습니다.',
  jira_status: '키체인에서 Jira 자격 증명을 읽지 못했습니다.',
};
