// 흐름 — R17 OTEL 수집키 자동 발급·회전·수신 확인의 화면 상태
// (src/onboarding.ts:otelLinkStep, src/views/home.ts 체크리스트, src/views/settings.ts "허브 수집키 자동 연동" 패널,
//  재발급 확인창 askReissueConfirm, 허브 연결 해제 확인창 HUB_DISCONNECT_CONFIRM).
//
// 검증 범위는 TS/DOM/CSS 층이다 — Rust 쪽 계약 동작(D-1~D-15)은 `cargo test`가 고정한다. 여기서는 백엔드가 줄 수 있는
// 상태(IngestLinkStatus)마다 ① 서로 다른 문구·data-state ② 맞는 버튼(다시 시도 / 허브 연결 열기 / 확인하기 / 없음)
// ③ 확인하지 않은 것을 주장하지 않음 ④ 토큰·키 원문이 DOM에 없음 ⑤ 확인창 문구와 응답 전달을 본다.
//
// 스크린샷은 `docs/reviewer/r17-otel-autolink-*.png`의 근거다(UI_HARNESS_SHOTS_DIR 아래 otelAutolink/ 에 저장).
import { installStatefulOtelSettings } from '../lib/statefulOtelSettings.mjs';

export const flowId = 'otelAutolink';
export const startHash = '#/';

const CHECKLIST = 'src/views/home.ts:renderOnboarding / src/onboarding.ts';
const PANEL = 'src/views/settings.ts:renderIngestLinkPanel';
const FAKE_DEVICE_TOKEN = `mhd_${'x'.repeat(43)}`;
const FAKE_INGEST_KEY = `mhk_${'Z'.repeat(43)}`;

const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
const NO_EP = { targetsHub: false, insecure: false, foreignHost: false };
const EXTRA = {
  issueSupported: true,
  receiptCheckSupported: true,
  hubConnected: true,
  confirmRequired: false,
  confirmDeclined: false,
  waitingFirstData: false,
  firstDataTimedOut: false,
  blockedReason: null,
  retryAt: null,
  notice: null,
  serverEnvChanged: [],
  serverEnvRejected: [],
};
const READY = { ...EXTRA, telemetryEnabled: true, endpoint: HUB_EP, collectionReady: true, blockers: [] };
const OFF = { ...EXTRA, telemetryEnabled: false, endpoint: NO_EP, collectionReady: false, blockers: ['telemetryOff', 'exporterNotOtlp', 'protocolMismatch', 'endpointMissing'] };

// 백엔드(src-tauri/src/otel_link.rs LinkFailureCause::message)가 원인별로 고정한 문구를 그대로 옮겼다.
const MSG = {
  reconnectRequired: '이 PC의 맑은에이전트 연결이 해제되었습니다. 웹에서 유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기됩니다. 앱에서 다시 로그인해 연결하세요.',
  blocked: '이 연결로는 수집키를 발급하거나 바꿀 수 없습니다. 앱의 허브 연결을 해제한 뒤 다시 로그인해 연결하세요.',
  credentialLimit: "수집키 상한(5개)에 도달했습니다. 허브 웹 /keys에서 쓰지 않는 키를 폐기하세요. 이전 연결에서 만든 '맑은에이전트' 키가 남아 있을 수 있습니다.",
  rotateBudgetExceeded:
    '이 기기의 수집키가 24시간 안에 너무 자주 바뀌었습니다. 네트워크가 불안정해 응답이 반복해서 끊기거나, 설정 파일(~/.claude/settings.json)이 다른 PC와 동기화되는 경우에도 생깁니다. 둘 다 아니라면 이 연결(기기 토큰)이 다른 곳에서 쓰이고 있을 수 있으니 연결 해제 후 다시 연결하세요.',
  featureUnavailable: '허브에 수집키 자동 발급 기능이 아직 없습니다. 허브 연결은 정상이며 1시간 뒤 다시 확인합니다.',
  unreachable: '허브에 연결하지 못했습니다. 네트워크 상태를 확인하세요. 잠시 후 다시 시도합니다.',
  rotationLost: '허브 응답이 반복해서 끊겨 수집키 자동 교체를 멈췄습니다(네트워크 오류). 네트워크를 확인한 뒤 [다시 시도]하거나 앱을 다시 시작하면 재개합니다.',
  applyFailed:
    '발급된 수집키를 설정 파일에 적용하지 못했습니다. 키는 앱이 메모리에 보관하고 있어 다시 시도하면 같은 키로 다시 씁니다. 앱을 종료하면 사라지고 다음 실행에서 새 키가 발급됩니다.',
  notReady: '설정 파일에서 수집이 켜져 있지 않아(텔레메트리·exporter·프로토콜·엔드포인트) 수집키를 발급받지 않았습니다. 설정 > OTel에서 저장한 뒤 다시 시도하세요.',
  notDesktopToken: '이 허브 연결은 데스크톱 앱에서 발급된 것이 아니라 수집키를 받을 수 없습니다. 허브 연결을 해제한 뒤 앱에서 다시 연결하세요.',
};

// 설정 > OTel 화면의 파일 상태(otel_settings_get) — 연동 패널의 상태와 모순되지 않게 같은 사실을 담는다.
function settingsFixture({ ingestKey, ready = true }) {
  const hub = 'https://malgnai-hub.apiserver.kr';
  return {
    settingsPath: '/Users/dev/.claude/settings.json',
    fileExists: true,
    parseError: null,
    managedKeys: [
      'CLAUDE_CODE_ENABLE_TELEMETRY',
      'OTEL_EXPORTER_OTLP_PROTOCOL',
      'OTEL_METRICS_EXPORTER',
      'OTEL_LOGS_EXPORTER',
      'OTEL_EXPORTER_OTLP_METRICS_ENDPOINT',
      'OTEL_EXPORTER_OTLP_LOGS_ENDPOINT',
      'OTEL_RESOURCE_ATTRIBUTES',
      'OTEL_EXPORTER_OTLP_HEADERS',
    ],
    readOnlyKeys: [],
    values: ready
      ? {
          CLAUDE_CODE_ENABLE_TELEMETRY: '1',
          OTEL_EXPORTER_OTLP_PROTOCOL: 'http/protobuf',
          OTEL_METRICS_EXPORTER: 'otlp',
          OTEL_LOGS_EXPORTER: 'otlp',
          OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: `${hub}/v1/metrics`,
          OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: `${hub}/v1/logs`,
          OTEL_RESOURCE_ATTRIBUTES: 'employee.id=dev,employee.name=%ED%95%98%EA%B7%BC%ED%98%B8',
        }
      : {},
    defaults: {},
    endpointDefaultsInjected: true,
    ingestKey,
    ingestEndpoint: HUB_EP,
  };
}
const KEY_SET = { state: 'set', masked: 'mhk_AbCd…' };
const KEY_UNSET = { state: 'unset', masked: null };

const status = (state, over = {}) => ({ state, masked: null, failure: null, ...READY, ...over });
// R20: 확인창 답(otel_link_answer_confirm)은 { status, outcome }이다. outcome은 엔진이 직접 알려 주는 재발급 결과이고 토스트가 그 값과 1:1로 대응한다.
const answer = (s, outcome) => ({ status: s, outcome });
const DECLINED = { kind: 'declined' };
const MINTED = { kind: 'minted', masked: 'mhk_AbCd…' };
const failed = (cause, over = {}) => status('failed', { failure: { cause, message: MSG[cause] }, ...over });

// 확인하지 않은 것을 주장하는 표현(수신 확인은 confirmed일 때만) + 시도하지 않은 자동 동작을 암시하는 표현
const CLAIM_CONFIRMED = ['수신 확인됨', '수신이 확인되었습니다', '데이터 수신이 확인'];
const IMPLIED = ['자동으로 바꾸', '자동으로 교체', '자동으로 변경', '자동으로 삭제', '자동으로 갱신', '바꾸지 않았습니다'];

async function noSecrets(page, bugs, file) {
  const dump = await page.evaluate(() => document.documentElement.outerHTML + '\n' + document.body.innerText);
  for (const secret of [FAKE_DEVICE_TOKEN, FAKE_INGEST_KEY]) {
    if (dump.includes(secret) || dump.includes(secret.slice(0, 16))) bugs.push({ severity: 'Critical', symptom: `DOM에 토큰/키 원문이 남아 있음: ${secret.slice(0, 8)}…`, file });
  }
}

function checkClaims(bugs, text, file, { allowConfirmed = false } = {}) {
  if (!allowConfirmed) {
    for (const c of CLAIM_CONFIRMED) if (text.includes(c)) bugs.push({ severity: 'Critical', symptom: `확인하지 않은 수신을 주장하는 문구 "${c}"`, file });
  }
  for (const c of IMPLIED) if (text.includes(c)) bugs.push({ severity: 'Critical', symptom: `시도하지 않은 자동 동작을 암시하는 문구 "${c}"`, file });
}

async function homeRow(page) {
  const row = page.locator('.onboard-row[data-step="otel-link"]');
  if ((await row.count()) === 0) return null;
  return {
    state: await row.getAttribute('data-state'),
    text: (await row.textContent()) ?? '',
    buttons: await row.locator('button').allTextContents(),
    marker: ((await row.locator('.onboard-marker').textContent()) ?? '').trim(),
  };
}

async function panel(page) {
  const p = page.locator('#otel-link-panel');
  if ((await p.count()) === 0) return null;
  return {
    state: await p.getAttribute('data-state'),
    text: (await p.textContent()) ?? '',
    buttons: await p.locator('button').allTextContents(),
  };
}

// 설정 > OTel의 수집키 영역(상태 배지 + 폐기 가능성 안내). 배지가 초록 "설정됨"이면 폐기됐을 수 있는 파일 키를 사용 중인 키처럼 보이게 한다(RV-001).
async function keyBlockCheck(page, bugs, { badge, note }) {
  const st = page.locator('#otel-ingest-key-status');
  if ((await st.count()) === 0) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 수집키 상태 줄이 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
  const text = (await st.textContent()) ?? '';
  const cls = (await st.locator('.badge').first().getAttribute('class')) ?? '';
  if (!text.includes(badge)) bugs.push({ severity: 'Major', symptom: `수집키 배지에 "${badge}"가 없음(실제: ${text})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
  if (text.includes('설정됨') && !text.includes('폐기')) bugs.push({ severity: 'Critical', symptom: `폐기됐을 수 있는 파일 키에 "설정됨"을 말함(${text})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
  if (cls.includes('badge-active')) bugs.push({ severity: 'Major', symptom: `폐기됐을 수 있는 파일 키 배지가 초록(badge-active)임(${cls})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
  const n = (await page.locator('#otel-ingest-key-liveness-note').textContent().catch(() => null)) ?? '';
  if (!n.includes(note)) bugs.push({ severity: 'Major', symptom: `수집키 영역 안내에 "${note}"가 없음(실제: ${n})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
  if (!text.includes('mhk_AbCd…')) bugs.push({ severity: 'Minor', symptom: `마스킹된 키 표시가 사라짐(${text})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
}

async function checkHome(page, bugs, { state, include = [], exclude = [], buttons, allowConfirmed = false }) {
  const row = await homeRow(page);
  if (!row) {
    bugs.push({ severity: 'Critical', symptom: 'OTEL 연동 행이 렌더되지 않음', file: CHECKLIST });
    return null;
  }
  if (row.state !== state) bugs.push({ severity: 'Critical', symptom: `OTEL 행 data-state가 ${state}가 아님(실제: ${row.state})`, file: CHECKLIST });
  for (const t of include) if (!row.text.includes(t)) bugs.push({ severity: 'Major', symptom: `OTEL 행에 "${t}" 문구가 없음(실제: ${row.text})`, file: CHECKLIST });
  for (const t of exclude) if (row.text.includes(t)) bugs.push({ severity: 'Major', symptom: `OTEL 행에 있으면 안 되는 "${t}"가 있음(실제: ${row.text})`, file: CHECKLIST });
  if (buttons) {
    const got = JSON.stringify(row.buttons);
    if (got !== JSON.stringify(buttons)) bugs.push({ severity: 'Major', symptom: `OTEL 행 버튼이 ${JSON.stringify(buttons)}가 아님(실제: ${got})`, file: CHECKLIST });
  }
  if (row.state !== 'done' && row.marker === '✓') bugs.push({ severity: 'Critical', symptom: '완료되지 않은 상태가 완료 체크(✓)로 표시됨', file: CHECKLIST });
  checkClaims(bugs, row.text, CHECKLIST, { allowConfirmed });
  await noSecrets(page, bugs, CHECKLIST);
  const count = await page.locator('.onboard-box .box-head .count').textContent().catch(() => null);
  if (!count || !count.includes('2/3')) bugs.push({ severity: 'Major', symptom: `OTEL 항목이 진행 카운트에 영향을 줌(기대 2/3, 실제: ${count})`, file: CHECKLIST });
  return row;
}

// ---- R19: 서버 권장값(env) 적용 뒤 설정 > OTel 폼 ----
// 자동 연동이 발급·회전 201의 서버 권장값을 settings.json에 쓰면, 이미 열려 있는 폼이 그 변경을 따라가고 사용자가 고치던 칸은 지키며
// 저장은 고친 칸만 보내야 한다. 저장·이벤트·재진입이 모두 `otel_settings_get`을 다시 읽으므로 상태 있는 스텁을 쓴다
// (lib/statefulOtelSettings.mjs). Rust 쪽 실제 파일 쓰기는 `cargo test`가 맡는다 — 여기는 TS/DOM 층만 본다.
const FORM = 'src/views/settings.ts:refreshOtelFormFromFile / handleSaveOtelSettings';
const FORM_MANAGED = [
  'CLAUDE_CODE_ENABLE_TELEMETRY',
  'OTEL_EXPORTER_OTLP_PROTOCOL',
  'OTEL_METRICS_EXPORTER',
  'OTEL_LOGS_EXPORTER',
  'OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE',
  'OTEL_METRIC_EXPORT_INTERVAL',
  'OTEL_LOGS_EXPORT_INTERVAL',
  'OTEL_LOG_USER_PROMPTS',
  'OTEL_RESOURCE_ATTRIBUTES',
  'OTEL_EXPORTER_OTLP_HEADERS',
];
const FORM_DEFAULTS = {
  CLAUDE_CODE_ENABLE_TELEMETRY: '1',
  OTEL_EXPORTER_OTLP_PROTOCOL: 'http/protobuf',
  OTEL_METRICS_EXPORTER: 'otlp',
  OTEL_LOGS_EXPORTER: 'otlp',
  OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: 'cumulative',
  OTEL_METRIC_EXPORT_INTERVAL: '60000',
  OTEL_LOGS_EXPORT_INTERVAL: '60000',
  OTEL_LOG_USER_PROMPTS: '0',
};
// 서버 권장값이 적용되기 전(옛 5000 간격 하나만 파일에 있는 기기)과 적용된 뒤.
const FORM_FILE_BEFORE = {
  CLAUDE_CODE_ENABLE_TELEMETRY: '1',
  OTEL_EXPORTER_OTLP_PROTOCOL: 'http/protobuf',
  OTEL_METRICS_EXPORTER: 'otlp',
  OTEL_LOGS_EXPORTER: 'otlp',
  OTEL_LOGS_EXPORT_INTERVAL: '5000',
  OTEL_LOG_USER_PROMPTS: '0',
};
const FORM_FILE_AFTER = {
  ...FORM_FILE_BEFORE,
  OTEL_LOGS_EXPORT_INTERVAL: '60000',
  OTEL_METRIC_EXPORT_INTERVAL: '60000',
  OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: 'cumulative',
};
const SERVER_CHANGED = ['OTEL_LOGS_EXPORT_INTERVAL', 'OTEL_METRIC_EXPORT_INTERVAL', 'OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE'];
const TEMPORALITY = 'OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE';

function formSettings(values) {
  return {
    settingsPath: '/Users/dev/.claude/settings.json',
    fileExists: true,
    parseError: null,
    managedKeys: FORM_MANAGED,
    readOnlyKeys: ['OTEL_LOG_USER_PROMPTS'],
    values,
    defaults: FORM_DEFAULTS,
    endpointDefaultsInjected: true,
    ingestKey: KEY_SET,
    ingestEndpoint: HUB_EP,
  };
}

async function formInputs(page) {
  return page.evaluate((keys) => Object.fromEntries(keys.map((k) => [k, document.getElementById(`otel-${k}`)?.value ?? null])), FORM_MANAGED);
}
const legacyHintShown = (page) => page.evaluate(() => !!document.getElementById('otel-legacy-hint-OTEL_LOGS_EXPORT_INTERVAL'));
const stubSaves = (page) => page.evaluate(() => window.__otelStub.saves.map((s) => s.payload.values));
const stubCalls = (page) => page.evaluate(() => [...window.__otelStub.calls]);
const settleForm = async (page) => {
  await page.waitForSelector('#otel-OTEL_LOGS_EXPORT_INTERVAL');
  await page.waitForTimeout(500);
};
// 앱 밖에서 서버 권장값이 파일에 쓰이고 연동 상태가 "바뀐 설정 3키"를 말하게 된 순간을 만든다.
async function applyServerEnv(page) {
  await page.evaluate(
    ({ file, link }) => {
      window.__otelStub.setFile(file);
      window.__otelStub.setLink(link);
    },
    { file: formSettings(FORM_FILE_AFTER), link: status('applied', { masked: 'mhk_AbCd…', serverEnvChanged: SERVER_CHANGED }) }
  );
}
function expectEqual(bugs, label, actual, expected) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) bugs.push({ severity: 'Major', symptom: `${label}: 기대 ${JSON.stringify(expected)}, 실제 ${JSON.stringify(actual)}`, file: FORM });
}

async function invoked(page, cmd) {
  return page.evaluate((c) => (window.__invokeLog ?? []).filter((e) => e.cmd === c), cmd);
}

const AUTH_OUT = { loggedIn: false, authMethod: null, email: null, orgName: null, subscriptionType: null };

export function scenarios(base) {
  // 필수 3단계 중 2개 완료(설치 완료·미로그인) — 체크리스트 박스가 열려 있어 OTEL 행이 보인다.
  const open = { ...base, check_claude_auth_status: AUTH_OUT };
  // hub_auth_renew_now: [허브 연결 열기]가 재연결 필요(reconnectRequired)일 때 시키는 허브 상태 확인(RV-003). 기본은 서버가 연결을 폐기한 응답.
  const REVOKED_HUB = { connected: false, state: 'REVOKED', device_name: null, expires_at: null, renew_after: null, reason: 'UNAUTHORIZED' };
  const withLink = (s, extra = {}) => ({ ...open, otel_ingest_link_status: s, otel_link_retry: s, otel_link_recheck: s, hub_auth_renew_now: REVOKED_HUB, ...extra });
  const homeCase = (id, shotName, s, expect, after) => ({
    id,
    fixtures: withLink(s),
    async run(page, ctx) {
      await page.waitForTimeout(400);
      await ctx.shot(shotName);
      const row = await checkHome(page, ctx.bugs, expect);
      if (after) await after(page, ctx, row);
    },
  });

  return [
    // ---- 미연동 ----
    homeCase('unlinked-hub-connected', '01-unlinked', status('unlinked', { ...OFF }), {
      state: 'todo',
      include: ['허브에 연결되어 있습니다', '자동으로 발급·적용합니다', '[다시 시도]'],
      exclude: ['발급 화면이 아직 없고', '자동 발급도 지원되지 않으며'],
      buttons: ['다시 시도'],
    }, async (page, { bugs }) => {
      await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '다시 시도' }).click();
      await page.waitForTimeout(200);
      if ((await invoked(page, 'otel_link_retry')).length !== 1) bugs.push({ severity: 'Major', symptom: '[다시 시도]가 otel_link_retry를 1회 호출하지 않음', file: CHECKLIST });
    }),
    homeCase('unlinked-hub-not-connected', '01-unlinked-no-hub', status('unlinked', { ...OFF, hubConnected: false }), {
      state: 'todo',
      include: ['허브에 연결하면 사용량 수집키가 자동으로 발급·적용됩니다', '허브에 연결되어 있지 않습니다'],
      buttons: ['허브 연결 열기'],
    }, async (page, { bugs }) => {
      await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '허브 연결 열기' }).click();
      await page.waitForTimeout(200);
      if (!(await page.evaluate(() => location.hash)).startsWith('#/settings/mcp')) bugs.push({ severity: 'Major', symptom: '"허브 연결 열기"가 #/settings/mcp로 이동하지 않음', file: CHECKLIST });
    }),
    // ---- 발급 중 ----
    homeCase('issuing', '01-issuing', status('issuing', { ...OFF }), {
      state: 'loading',
      include: ['수집키를 발급받아 적용하는 중'],
      buttons: [],
    }),
    // ---- 적용됨 ----
    homeCase('applied-waiting-first-data', '01-applied-waiting', status('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }), {
      state: 'partial',
      include: ['mhk_AbCd…', '첫 데이터 수신을 확인하는 중입니다(최대 15분)', '새로 시작하는 Claude Code 세션부터 적용됩니다'],
      exclude: ['수집되지 않을 수 있습니다'],
      buttons: [],
    }),
    homeCase('applied-first-data-timeout', '01-applied-timeout', status('applied', { masked: 'mhk_AbCd…', firstDataTimedOut: true }), {
      state: 'partial',
      include: ['15분 안에 수신이 확인되지 않았습니다', '새 Claude Code 세션을 열면 수집이 시작됩니다', '다음 점검(앱 시작 때·1시간 간격)에서 다시 확인합니다'],
      buttons: [],
    }),
    homeCase('applied-not-ready', '01-applied-not-ready', status('applied', { masked: 'mhk_AbCd…', ...OFF, blockers: ['endpointMissing', 'exporterNotOtlp'], telemetryEnabled: true }), {
      state: 'warn',
      include: ['수집되지 않을 수 있습니다', '설정 파일에 수집 엔드포인트가 없음', 'metrics·logs 내보내기가 otlp'],
      exclude: ['새로 시작하는'],
      buttons: ['OTel 설정 열기'],
    }),
    // ---- 실패/차단/한도 ----
    homeCase('blocked-quota', '01-blocked', failed('blocked', { ...OFF, blockedReason: 'quota' }), {
      state: 'failed',
      include: ['이 연결로는 수집키를 발급하거나 바꿀 수 없습니다', '사유: 사용량 한도 초과', '허브 연결을 해제한 뒤 다시 로그인해 연결하세요'],
      exclude: ['유출'],
      buttons: ['허브 연결 열기'],
    }),
    homeCase('blocked-suspected-leak', '01-blocked-leak', failed('blocked', { ...OFF, blockedReason: 'suspectedLeak' }), {
      state: 'failed',
      include: ['사유: 유출 의심', '이 기기에 유출 의심으로 폐기된 수집키가 있어 이 연결로는 발급이 막혔습니다', '다시 로그인해 연결하세요'],
      exclude: ['다시 발급', '연결이 해제되었습니다', '연결과 이 기기의 다른 수집키도 함께 폐기'],
      buttons: ['허브 연결 열기'],
    }),
    homeCase('rotate-budget-exceeded', '01-rotate-budget', failed('rotateBudgetExceeded', { masked: 'mhk_AbCd…', retryAt: '2099-10-16T01:02:03.456Z' }), {
      state: 'failed',
      include: ['24시간 안에 너무 자주 바뀌었습니다', '네트워크가 불안정해', '설정 파일(~/.claude/settings.json)이 다른 PC와 동기화', '연결 해제 후 다시 연결하세요', '다시 교체할 수 있는 시각', 'mhk_AbCd…'],
      buttons: [],
    }),
    homeCase('credential-limit', '01-credential-limit', failed('credentialLimit', { ...OFF }), {
      state: 'failed',
      include: ['수집키 상한(5개)에 도달했습니다', '/keys', '맑은에이전트'],
      buttons: ['다시 시도'],
    }),
    homeCase('reconnect-required', '01-reconnect', failed('reconnectRequired', { ...OFF }), {
      state: 'failed',
      include: ['이 PC의 맑은에이전트 연결이 해제되었습니다', '유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기', '앱에서 다시 로그인해 연결하세요'],
      exclude: ['다시 발급', '[다시 시도]'],
      buttons: ['허브 연결 열기'],
    }),
    homeCase('not-desktop-token', '01-not-desktop', failed('notDesktopToken', { ...OFF }), {
      state: 'failed',
      include: ['데스크톱 앱에서 발급된 것이 아니라'],
      buttons: ['허브 연결 열기'],
    }),
    homeCase('unreachable', '01-unreachable', failed('unreachable', { ...OFF }), {
      state: 'failed',
      include: ['허브에 연결하지 못했습니다', '잠시 후 다시 시도합니다'],
      buttons: ['다시 시도'],
    }),
    homeCase('rotation-lost', '01-rotation-lost', failed('rotationLost', { masked: 'mhk_AbCd…' }), {
      state: 'failed',
      include: ['수집키 자동 교체를 멈췄습니다'],
      buttons: ['다시 시도'],
    }),
    homeCase('apply-failed', '01-apply-failed', failed('applyFailed', { ...OFF }), {
      state: 'failed',
      include: ['설정 파일에 적용하지 못했습니다', '메모리에 보관'],
      buttons: ['다시 시도'],
    }),
    homeCase('not-ready', '01-not-ready', failed('notReady', { ...OFF }), {
      state: 'todo',
      include: ['수집키를 발급받지 않았습니다', '확인할 항목', '설정 파일에 텔레메트리 켜기 설정이 없음'],
      buttons: ['OTel 설정 열기'],
    }),
    // ---- 필수 3단계 완료 홈의 한 줄 ----
    {
      id: 'core-done-slim-blocked',
      fixtures: { ...withLink(failed('blocked', { ...OFF, blockedReason: 'quota' })), check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-slim-blocked');
        if ((await page.locator('.onboard-box').count()) > 0) bugs.push({ severity: 'Critical', symptom: '필수 완료 후 박스가 다시 열림', file: CHECKLIST });
        const slim = page.locator('.onboard-slim[data-step="otel-link"]');
        if ((await slim.count()) !== 1) bugs.push({ severity: 'Critical', symptom: '차단 상태인데 홈 한 줄이 없음', file: CHECKLIST });
        else if (!((await slim.textContent()) ?? '').includes('발급할 수 없습니다')) bugs.push({ severity: 'Major', symptom: '한 줄에 차단 문구가 없음', file: CHECKLIST });
      },
    },
    {
      id: 'core-done-transient-hidden',
      fixtures: { ...withLink(failed('unreachable', { ...OFF })), check_claude_auth_status: { loggedIn: true, authMethod: 'claude.ai', email: 'dev@malgnsoft.com', orgName: 'malgnsoft', subscriptionType: 'max' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-slim-transient-hidden');
        const n = await page.locator('.onboard-box, .onboard-slim').count();
        if (n > 0) bugs.push({ severity: 'Major', symptom: `자동 재시도 중인 일시 오류가 필수 완료 홈을 방해함(${n}개)`, file: CHECKLIST });
      },
    },
    // ---- 재발급 확인창(confirm_required, 계약 D-1) ----
    {
      id: 'confirm-required-dialog-decline',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('unlinked', { ...OFF, confirmDeclined: true }), DECLINED),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        const dialog = page.locator('.modal-box');
        if ((await dialog.count()) !== 1) {
          bugs.push({ severity: 'Critical', symptom: `confirm_required인데 확인창이 1개가 아님(실제: ${await dialog.count()})`, file: 'src/views/settings.ts:askReissueConfirm' });
          return;
        }
        await shot('01-confirm-required-dialog');
        // RV-08: 앱이 스스로 띄운 창이라 초점은 '발급하지 않음'에 있다 — Enter 한 번으로 동의되지 않는다.
        const focused = await page.evaluate(() => document.activeElement?.textContent?.trim() ?? null);
        if (focused !== '발급하지 않음') bugs.push({ severity: 'Critical', symptom: `확인창 기본 초점이 '발급하지 않음'이 아님(실제: ${focused}) — Enter로 우발 동의될 수 있다`, file: 'src/views/settings.ts:askReissueConfirm' });
        const text = (await dialog.textContent()) ?? '';
        for (const t of ['수집키 재발급', '웹에서 이 기기 수집키가 폐기되었습니다. 다시 발급할까요?', '다시 로그인할 필요는 없습니다', '다시 발급', '발급하지 않음']) {
          if (!text.includes(t)) bugs.push({ severity: 'Major', symptom: `확인창에 "${t}"가 없음(실제: ${text})`, file: 'src/views/settings.ts:askReissueConfirm' });
        }
        await dialog.getByRole('button', { name: '발급하지 않음' }).click();
        await page.waitForTimeout(300);
        const calls = await invoked(page, 'otel_link_answer_confirm');
        if (calls.length !== 1 || calls[0].args?.accept !== false) bugs.push({ severity: 'Critical', symptom: `거절이 accept:false로 전달되지 않음(${JSON.stringify(calls.map((c) => c.args))})`, file: 'src/views/settings.ts:askReissueConfirm' });
        if ((await page.locator('.modal-box').count()) !== 0) bugs.push({ severity: 'Major', symptom: '거절 후에도 확인창이 남아 있음', file: 'src/views/settings.ts:askReissueConfirm' });
        await shot('02-after-decline');
        const row = await homeRow(page);
        if (!row || !row.text.includes('거절했습니다') || !row.text.includes('이번 앱 실행 동안은 다시 묻지 않습니다')) bugs.push({ severity: 'Major', symptom: `거절 뒤 행에 거절 안내가 없음(실제: ${row?.text})`, file: CHECKLIST });
        if (calls.length > 1 || (await page.locator('.modal-box').count()) > 0) bugs.push({ severity: 'Critical', symptom: '거절 뒤 확인창이 다시 뜸', file: 'src/views/settings.ts:askReissueConfirm' });
        await noSecrets(page, bugs, CHECKLIST);
      },
    },
    {
      id: 'confirm-required-dialog-accept',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }), MINTED),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        const dialog = page.locator('.modal-box');
        await dialog.getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(300);
        const calls = await invoked(page, 'otel_link_answer_confirm');
        if (calls.length !== 1 || calls[0].args?.accept !== true) bugs.push({ severity: 'Critical', symptom: `동의가 accept:true로 전달되지 않음(${JSON.stringify(calls.map((c) => c.args))})`, file: 'src/views/settings.ts:askReissueConfirm' });
        await shot('01-after-accept');
        const row = await homeRow(page);
        if (!row || row.state !== 'partial') bugs.push({ severity: 'Major', symptom: `동의 뒤 행이 적용됨(partial)으로 바뀌지 않음(실제: ${row?.state})`, file: CHECKLIST });
      },
    },
    {
      id: 'confirm-required-escape-declines',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('unlinked', { ...OFF, confirmDeclined: true }), DECLINED),
      }),
      async run(page, { bugs }) {
        await page.waitForTimeout(500);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
        const calls = await invoked(page, 'otel_link_answer_confirm');
        if (calls.length !== 1 || calls[0].args?.accept !== false) bugs.push({ severity: 'Critical', symptom: 'Esc(창 닫기)가 거절로 전달되지 않음 — 닫았는데 재발급될 수 있다', file: 'src/views/settings.ts:askReissueConfirm' });
      },
    },
    // ---- 설정 > OTel 패널 ----
    {
      id: 'settings-panel-confirmed',
      startHash: '#/settings/otel',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…' }), { otel_settings_get: settingsFixture({ ingestKey: KEY_SET }) }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await shot('01-settings-confirmed');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (p.state !== 'done') bugs.push({ severity: 'Critical', symptom: `패널 data-state가 done이 아님(실제: ${p.state})`, file: PANEL });
        for (const t of ['허브 수집키 자동 연동', '수신 확인됨', '허브에서 데이터 수신이 확인되었습니다', 'mhk_AbCd…']) if (!p.text.includes(t)) bugs.push({ severity: 'Major', symptom: `패널에 "${t}"가 없음(실제: ${p.text})`, file: PANEL });
        checkClaims(bugs, p.text, PANEL, { allowConfirmed: true });
        await noSecrets(page, bugs, PANEL);
      },
    },
    {
      id: 'settings-panel-applied-waiting',
      startHash: '#/settings/otel',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }), { otel_settings_get: settingsFixture({ ingestKey: KEY_SET }) }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await shot('01-settings-applied-waiting');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (p.state !== 'partial') bugs.push({ severity: 'Critical', symptom: `패널 data-state가 partial이 아님(실제: ${p.state})`, file: PANEL });
        for (const t of ['적용됨', '첫 데이터 수신을 확인하는 중입니다', '새로 시작하는 Claude Code 세션부터 적용됩니다']) if (!p.text.includes(t)) bugs.push({ severity: 'Major', symptom: `패널에 "${t}"가 없음(실제: ${p.text})`, file: PANEL });
        checkClaims(bugs, p.text, PANEL);
        await noSecrets(page, bugs, PANEL);
      },
    },
    {
      id: 'settings-panel-blocked-with-retry',
      startHash: '#/settings/otel',
      fixtures: withLink(failed('unreachable', { ...READY }), { otel_settings_get: settingsFixture({ ingestKey: KEY_UNSET }) }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await shot('01-settings-unreachable');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (JSON.stringify(p.buttons) !== JSON.stringify(['다시 시도'])) bugs.push({ severity: 'Major', symptom: `패널 버튼이 [다시 시도]가 아님(실제: ${JSON.stringify(p.buttons)})`, file: PANEL });
        await page.locator('#otel-link-panel').getByRole('button', { name: '다시 시도' }).click();
        await page.waitForTimeout(200);
        if ((await invoked(page, 'otel_link_retry')).length !== 1) bugs.push({ severity: 'Major', symptom: '패널 [다시 시도]가 otel_link_retry를 호출하지 않음', file: PANEL });
        // 폼 안의 수집키 입력칸은 있다. 허브에 연결되어 앱이 관리하는 동안에는 잠겨 있다(RV-02).
        if ((await page.locator('#otel-ingest-key').count()) !== 1) bugs.push({ severity: 'Major', symptom: '수동 수집키 입력칸이 사라짐', file: PANEL });
        else if (!(await page.locator('#otel-ingest-key').isDisabled())) bugs.push({ severity: 'Major', symptom: '허브 연결 중인데 수동 수집키 입력칸이 열려 있음', file: PANEL });
      },
    },
    // ---- 허브 연결 해제 확인창(계약 D-13) ----
    {
      id: 'hub-disconnect-confirm-dialog',
      startHash: '#/settings/mcp',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…' })),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        const btn = page.getByRole('button', { name: '연결 해제' });
        if ((await btn.count()) === 0) return bugs.push({ severity: 'Critical', symptom: '허브 연결 행에 [연결 해제]가 없음', file: 'src/views/settings.ts:renderHubAuthRow' });
        await btn.first().click();
        await page.waitForTimeout(200);
        const dialog = page.locator('.modal-box');
        await shot('01-disconnect-confirm');
        const text = (await dialog.textContent()) ?? '';
        for (const t of [
          '허브 연결 해제',
          '이 기기의 사용량 수집키가 허브 서버에서 함께 폐기',
          '~/.claude/settings.json에서 수집키(Authorization 항목)가 지워져',
          '사용량 수집이 멈춥니다',
          '다른 OTel 설정은 그대로입니다',
        ]) {
          if (!text.includes(t)) bugs.push({ severity: 'Major', symptom: `해제 확인창에 "${t}"가 없음(실제: ${text})`, file: 'src/views/settings.ts:HUB_DISCONNECT_CONFIRM' });
        }
        await dialog.getByRole('button', { name: '취소' }).click();
        await page.waitForTimeout(200);
        if ((await invoked(page, 'hub_auth_disconnect')).length !== 0) bugs.push({ severity: 'Critical', symptom: '취소했는데 hub_auth_disconnect가 호출됨', file: 'src/views/settings.ts:handleDisconnectHub' });
      },
    },
    // ---- R17 리뷰 RV-01/06/07/08 ----
    // RV-01: 웹에서 폐기된 키가 settings.json에 남아 있다(applied + confirmRequired). 홈이 '적용됨'을 말하면 안 되고,
    // 거절한 뒤에도 다시 시작할 [다시 시도]가 남아야 한다. Enter는 동의가 아니라 거절이다(RV-08).
    {
      id: 'revoked-key-on-file-enter-declines-and-retry-remains',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('applied', { masked: 'mhk_AbCd…', confirmDeclined: true }), DECLINED),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        const dialog = page.locator('.modal-box');
        if ((await dialog.count()) !== 1) return bugs.push({ severity: 'Critical', symptom: '폐기된 키가 파일에 남은 confirm_required에서 확인창이 뜨지 않음', file: 'src/views/settings.ts:askReissueConfirm' });
        await shot('01-revoked-on-file-dialog');
        // 확인창이 떠 있는 동안에도 뒤의 행은 '적용됨/수신 확인됨'을 말하면 안 된다.
        const behind = await homeRow(page);
        if (!behind || behind.state !== 'todo' || behind.text.includes('수집키가 적용되었습니다')) bugs.push({ severity: 'Critical', symptom: `폐기된 키가 파일에 남았는데 행이 적용됨으로 보임(state=${behind?.state}, ${behind?.text})`, file: CHECKLIST });
        await page.keyboard.press('Enter');
        await page.waitForTimeout(300);
        const calls = await invoked(page, 'otel_link_answer_confirm');
        if (calls.length !== 1 || calls[0].args?.accept !== false) bugs.push({ severity: 'Critical', symptom: `Enter가 거절(accept:false)로 전달되지 않음 — 우발 동의 가능(${JSON.stringify(calls.map((c) => c.args))})`, file: 'src/views/settings.ts:askReissueConfirm' });
        await shot('02-declined-key-still-on-file');
        const row = await checkHome(page, bugs, {
          state: 'todo',
          include: ['웹에서 이 기기의 수집키가 폐기되어', 'mhk_AbCd…', '재발급을 거절했습니다', '[다시 시도]'],
          exclude: ['수집키가 적용되었습니다', '수신이 확인', '수집키는 적용되었지만'],
          buttons: ['다시 시도'],
        });
        if (!row) return;
        // 거절 뒤 재발급 경로: [다시 시도] → 점검 → 다시 확인창.
        await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '다시 시도' }).click();
        await page.waitForTimeout(400);
        if ((await invoked(page, 'otel_link_retry')).length !== 1) bugs.push({ severity: 'Major', symptom: '거절 뒤 [다시 시도]가 otel_link_retry를 호출하지 않음', file: CHECKLIST });
        if ((await page.locator('.modal-box').count()) !== 1) bugs.push({ severity: 'Major', symptom: '[다시 시도] 뒤 다시 묻는 확인창이 뜨지 않음', file: 'src/views/settings.ts:retryIngestLinkNow' });
        await shot('03-retry-asks-again');
        await page.keyboard.press('Escape');
      },
    },
    // RV-01(수신 확인되었던 키): 같은 실행에서 확인되었어도 폐기 판정이 우선한다 — 설정 패널 배지는 '수신 확인됨'이 아니다.
    {
      id: 'revoked-confirmed-key-settings-panel',
      startHash: '#/settings/otel',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…', confirmRequired: true }), {
        otel_settings_get: settingsFixture({ ingestKey: KEY_SET }),
        otel_link_answer_confirm: answer(status('confirmed', { masked: 'mhk_AbCd…', confirmDeclined: true }), DECLINED),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        // 설정 > OTel에서도 확인창이 뜬다(홈·설정 화면 한정). 초점은 거절 쪽.
        const focused = await page.evaluate(() => document.activeElement?.textContent?.trim() ?? null);
        if ((await page.locator('.modal-box').count()) !== 1 || focused !== '발급하지 않음') bugs.push({ severity: 'Critical', symptom: `설정 > OTel에서 확인창/초점이 기대와 다름(modal=${await page.locator('.modal-box').count()}, focus=${focused})`, file: 'src/views/settings.ts:maybeAskReissueConfirm' });
        await shot('01-settings-revoked-dialog');
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
        await shot('02-settings-revoked-declined');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (p.state !== 'todo') bugs.push({ severity: 'Critical', symptom: `패널 data-state가 todo가 아님(실제: ${p.state})`, file: PANEL });
        for (const t of ['폐기됨 · 재발급 거절', '웹에서 이 기기의 수집키가 폐기되어', 'mhk_AbCd…']) if (!p.text.includes(t)) bugs.push({ severity: 'Major', symptom: `패널에 "${t}"가 없음(실제: ${p.text})`, file: PANEL });
        for (const t of ['수신 확인됨', '수신이 확인되었습니다', '적용됨']) if (p.text.includes(t)) bugs.push({ severity: 'Critical', symptom: `폐기된 키인데 패널이 "${t}"를 말함(${p.text})`, file: PANEL });
        if (JSON.stringify(p.buttons) !== JSON.stringify(['다시 시도'])) bugs.push({ severity: 'Major', symptom: `거절 뒤 패널 버튼이 [다시 시도]가 아님(${JSON.stringify(p.buttons)})`, file: PANEL });
        await noSecrets(page, bugs, PANEL);
      },
    },
    // RV-08: 다른 화면에서는 자동으로 끼어들지 않고, 홈·설정에 들어오면 묻는다.
    {
      id: 'confirm-dialog-waits-for-home-or-settings',
      startHash: '#/usage',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true })),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(600);
        await shot('01-other-screen-no-dialog');
        if ((await page.locator('.modal-box').count()) !== 0) bugs.push({ severity: 'Major', symptom: '홈·설정이 아닌 화면에서 재발급 확인창이 끼어듦', file: 'src/views/settings.ts:maybeAskReissueConfirm' });
        await page.evaluate(() => {
          location.hash = '#/';
        });
        await page.waitForTimeout(600);
        if ((await page.locator('.modal-box').count()) !== 1) bugs.push({ severity: 'Major', symptom: '홈으로 들어왔는데 대기 중이던 확인창이 뜨지 않음', file: 'src/main.ts:handleNavigation' });
        await shot('02-home-asks');
        await page.keyboard.press('Escape');
      },
    },
    // RV-06: 동의했지만 결과가 적용이 아니면 성공 토스트를 띄우지 않는다.
    {
      id: 'reissue-accepted-but-not-applied-toast',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true }), {
        otel_link_answer_confirm: answer(failed('notReady', { ...OFF }), { kind: 'failed', failure: { cause: 'notReady', message: MSG.notReady }, transient: false }),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.modal-box').getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(400);
        await shot('01-accepted-not-applied');
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        if (toast.includes('다시 발급해 적용했습니다')) bugs.push({ severity: 'Critical', symptom: `적용되지 않았는데 성공 토스트가 뜸: ${toast}`, file: 'src/views/settings.ts:askReissueConfirm' });
        if (!toast.includes('다시 발급하지 못했습니다') || !toast.includes('수집키를 발급받지 않았습니다')) bugs.push({ severity: 'Major', symptom: `실패 원인 토스트가 없음: ${toast}`, file: 'src/views/settings.ts:askReissueConfirm' });
      },
    },
    {
      id: 'reissue-accepted-and-applied-toast',
      fixtures: withLink(status('unlinked', { ...OFF, confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('applied', { masked: 'mhk_AbCd…', waitingFirstData: true }), MINTED),
      }),
      async run(page, { bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.modal-box').getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(400);
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        if (!toast.includes('다시 발급해 적용했습니다')) bugs.push({ severity: 'Major', symptom: `적용됨 응답인데 성공 토스트가 없음: ${toast}`, file: 'src/views/settings.ts:askReissueConfirm' });
      },
    },
    // RV-07: 일시 오류(notice)는 적용·수신 확인 상태를 '실패'로 덮지 않는다.
    {
      id: 'transient-notice-keeps-confirmed-state',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…', notice: { cause: 'unreachable', message: MSG.unreachable } }), {}),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-confirmed-with-notice');
        await checkHome(page, bugs, {
          state: 'done',
          include: ['데이터 수신이 확인되었습니다', 'mhk_AbCd…', '마지막 점검: 허브에 연결하지 못했습니다'],
          exclude: ['적용되어 있지 않습니다'],
          buttons: [],
          allowConfirmed: true,
        });
      },
    },
    {
      id: 'transient-notice-keeps-applied-state-feature-unavailable',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', waitingFirstData: true, notice: { cause: 'featureUnavailable', message: '허브에 수집키 자동 발급 기능이 아직 없습니다. 허브 연결은 정상이며 1시간 뒤 다시 확인합니다.' } }), {}),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        await shot('01-applied-with-feature-notice');
        await checkHome(page, bugs, {
          state: 'partial',
          include: ['수집키가 적용되었습니다', '마지막 점검: 허브에 수집키 자동 발급 기능이 아직 없습니다'],
          exclude: ['사용량 수집키가 적용되어 있지 않습니다'],
          buttons: [],
        });
      },
    },
    // RV-06: 연결 해제 토스트는 이 PC의 수집키를 실제로 지웠을 때만 "지웠습니다"라고 말한다.
    ...[
      ['removed', ['수집키도 지웠습니다'], []],
      ['notPresent', ['설정 파일은 바꾸지 않았습니다'], ['지웠습니다']],
      ['failed', ['지우지 못했습니다'], ['지웠습니다']],
    ].map(([removal, include, exclude]) => ({
      id: `hub-disconnect-toast-${removal}`,
      startHash: '#/settings/mcp',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…' }), {
        hub_auth_disconnect: { connected: false, state: 'NOT_CONNECTED', device_name: null, expires_at: null, renew_after: null, reason: null, ingest_key: removal },
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await page.getByRole('button', { name: '연결 해제' }).first().click();
        await page.locator('.modal-box').getByRole('button', { name: '해제' }).click();
        await page.waitForTimeout(500);
        await shot(`01-disconnect-${removal}`);
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        for (const t of include) if (!toast.includes(t)) bugs.push({ severity: 'Major', symptom: `해제 토스트(${removal})에 "${t}"가 없음: ${toast}`, file: 'src/otelLinkText.ts:disconnectToast' });
        for (const t of exclude) if (toast.includes(t)) bugs.push({ severity: 'Critical', symptom: `해제 토스트(${removal})가 사실과 다르게 "${t}"를 말함: ${toast}`, file: 'src/otelLinkText.ts:disconnectToast' });
      },
    })),

    // ---- R20: 웹 폐기(일반/유출 의심)와 앱 화면 — 허브 웹 /keys의 "맑은에이전트 관리" 키 동작과 같은 사실을 말한다 ----
    // R18 N-13: 폐기 판정을 받은 키가 파일에 남은 채 다음 점검이 일시 오류로 끝난 모양(applied + revokedByHub + notice)의 렌더.
    homeCase('r20-applied-revoked-by-hub-with-notice', 'r20-01-applied-revoked-notice', status('applied', {
      masked: 'mhk_AbCd…', revokedByHub: true, notice: { cause: 'unreachable', message: MSG.unreachable },
    }), {
      state: 'todo',
      include: ['웹에서 이 기기의 수집키가 폐기되어', 'mhk_AbCd…', '재발급 여부를 아직 다시 확인하지 못했습니다', '[다시 시도]', '다시 로그인할 필요는 없습니다', '마지막 점검: 허브에 연결하지 못했습니다'],
      exclude: ['수집키가 적용되었습니다', '거절', '다시 로그인해 연결'],
      buttons: ['다시 시도'],
    }),
    // 같은 상태의 설정 > OTel 패널: 배지는 "적용됨"이 아니라 "폐기됨 · 재확인 필요"다.
    {
      id: 'r20-settings-panel-applied-revoked-by-hub-with-notice',
      startHash: '#/settings/otel',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', revokedByHub: true, notice: { cause: 'unreachable', message: MSG.unreachable } }), {
        otel_settings_get: settingsFixture({ ingestKey: KEY_SET }),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await shot('r20-01b-settings-panel-applied-revoked-notice');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (p.state !== 'todo') bugs.push({ severity: 'Critical', symptom: `패널 data-state가 todo가 아님(실제: ${p.state})`, file: PANEL });
        for (const t of ['폐기됨 · 재확인 필요', '재발급 여부를 아직 다시 확인하지 못했습니다', '마지막 점검: 허브에 연결하지 못했습니다', '다시 로그인할 필요는 없습니다']) {
          if (!p.text.includes(t)) bugs.push({ severity: 'Major', symptom: `패널에 "${t}"가 없음(실제: ${p.text})`, file: PANEL });
        }
        for (const t of ['적용됨', '수신 확인됨', '수집키가 적용되었습니다', '거절']) if (p.text.replace('폐기됨 · 재확인 필요', '').includes(t)) bugs.push({ severity: 'Critical', symptom: `폐기 판정이 이어지는데 패널이 "${t}"를 말함(${p.text})`, file: PANEL });
        if (JSON.stringify(p.buttons) !== JSON.stringify(['다시 시도'])) bugs.push({ severity: 'Major', symptom: `패널 버튼이 [다시 시도]가 아님(${JSON.stringify(p.buttons)})`, file: PANEL });
        await keyBlockCheck(page, bugs, { badge: '폐기됨 · 파일에 남음', note: '허브에서 폐기되어 수집되지 않습니다' });
        await noSecrets(page, bugs, PANEL);
      },
    },
    // 일반 폐기(manual): 확인창이 "다시 발급(다시 로그인 불필요)"을 말하고, 동의하면 새 키가 적용된다 — 토스트는 엔진이 알려 준 결과(minted)와 1:1.
    {
      id: 'r20-manual-revoke-dialog-accept-minted',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', confirmRequired: true }), {
        otel_link_answer_confirm: answer(status('applied', { masked: 'mhk_NnNn…', waitingFirstData: true }), { kind: 'minted', masked: 'mhk_NnNn…' }),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        const dialog = page.locator('.modal-box');
        if ((await dialog.count()) !== 1) return bugs.push({ severity: 'Critical', symptom: '일반 폐기(confirm_required)인데 확인창이 뜨지 않음', file: 'src/views/settings.ts:askReissueConfirm' });
        const text = (await dialog.textContent()) ?? '';
        for (const t of ['웹에서 이 기기 수집키가 폐기되었습니다. 다시 발급할까요?', '앱에서 바로 발급하며 다시 로그인할 필요는 없습니다']) {
          if (!text.includes(t)) bugs.push({ severity: 'Major', symptom: `확인창에 "${t}"가 없음(실제: ${text})`, file: 'src/onboarding.ts:REISSUE_CONFIRM_MESSAGE' });
        }
        if (/다시 로그인해 연결|허브 연결/.test(text)) bugs.push({ severity: 'Major', symptom: `일반 폐기 확인창이 재로그인·재연결을 요구함: ${text}`, file: 'src/onboarding.ts:REISSUE_CONFIRM_MESSAGE' });
        await shot('r20-02-manual-revoke-dialog');
        const behind = await homeRow(page);
        for (const t of ['웹에서 이 기기의 수집키가 폐기되어', 'mhk_AbCd…', '다시 로그인할 필요는 없습니다']) {
          if (!behind?.text.includes(t)) bugs.push({ severity: 'Major', symptom: `확인창 뒤 행에 "${t}"가 없음(실제: ${behind?.text})`, file: CHECKLIST });
        }
        await dialog.getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(400);
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        if (!toast.includes('수집키를 다시 발급해 적용했습니다')) bugs.push({ severity: 'Major', symptom: `minted 결과인데 성공 토스트가 없음: ${toast}`, file: 'src/otelLinkText.ts:reissueToast' });
        await shot('r20-03-manual-revoke-reissued-toast');
        const row = await homeRow(page);
        if (!row || row.state !== 'partial') bugs.push({ severity: 'Major', symptom: `재발급 뒤 행이 적용됨(partial)이 아님(실제: ${row?.state})`, file: CHECKLIST });
      },
    },
    // 토스트는 화면 상태가 아니라 결과(outcome)만 본다: 상태가 "적용됨"을 말해도 결과가 notStarted/failed면 성공을 말하지 않는다.
    ...[
      ['not-started-busy', { kind: 'notStarted', reason: 'busy' }, ['다른 점검이 진행 중이라 재발급을 시작하지 못했습니다'], ['적용했습니다']],
      ['not-applied', { kind: 'notApplied' }, ['새 수집키는 적용하지 않았습니다'], ['적용했습니다']],
      ['transient-failure', { kind: 'failed', failure: { cause: 'unreachable', message: MSG.unreachable }, transient: true }, ['수집키를 다시 발급하지 못했습니다', '허브에 연결하지 못했습니다', '[다시 시도]로 다시 발급할 수 있습니다'], ['적용했습니다']],
    ].map(([name, outcome, include, exclude]) => ({
      id: `r20-toast-follows-outcome-${name}`,
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', confirmRequired: true }), {
        // 일부러 "적용됨 + 새 마스킹"을 돌려준다 — 예전 추론이었다면 성공 토스트가 떴을 모양.
        otel_link_answer_confirm: answer(status('applied', { masked: 'mhk_NnNn…', waitingFirstData: true }), outcome),
      }),
      async run(page, { bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.modal-box').getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(400);
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        for (const t of include) if (!toast.includes(t)) bugs.push({ severity: 'Major', symptom: `결과(${name}) 토스트에 "${t}"가 없음: ${toast}`, file: 'src/otelLinkText.ts:reissueToast' });
        for (const t of exclude) if (toast.includes(t)) bugs.push({ severity: 'Critical', symptom: `결과(${name})와 다르게 "${t}"를 말함: ${toast}`, file: 'src/otelLinkText.ts:reissueToast' });
      },
    })),
    // 동의했는데 그 사이 연결이 폐기된 경우(유출 의심 폐기): 토스트·화면 모두 "다시 로그인해 연결"이고 "다시 발급"을 권하지 않는다.
    {
      id: 'r20-accept-meets-dead-connection',
      fixtures: withLink(status('applied', { masked: 'mhk_AbCd…', confirmRequired: true }), {
        otel_link_answer_confirm: answer(
          failed('reconnectRequired', { masked: 'mhk_AbCd…' }),
          { kind: 'failed', failure: { cause: 'reconnectRequired', message: MSG.reconnectRequired }, transient: false }
        ),
      }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.modal-box').getByRole('button', { name: '다시 발급' }).click();
        await page.waitForTimeout(400);
        const toast = (await page.locator('.toast').allTextContents()).join(' | ');
        if (!toast.includes('앱에서 다시 로그인해 연결하세요')) bugs.push({ severity: 'Major', symptom: `연결 폐기 결과인데 토스트가 재로그인을 말하지 않음: ${toast}`, file: 'src/otelLinkText.ts:reissueToast' });
        for (const t of ['다시 발급해 적용했습니다', '[다시 시도]']) if (toast.includes(t)) bugs.push({ severity: 'Critical', symptom: `연결이 끊겼는데 토스트가 "${t}"를 말함: ${toast}`, file: 'src/otelLinkText.ts:reissueToast' });
        await shot('r20-06-accept-meets-dead-connection');
        const row = await homeRow(page);
        if (!row || row.state !== 'failed' || !row.text.includes('다시 로그인해 연결하세요') || row.buttons.join() !== '허브 연결 열기') {
          bugs.push({ severity: 'Major', symptom: `연결 폐기 뒤 행이 재연결 안내가 아님(state=${row?.state}, buttons=${row?.buttons}, ${row?.text})`, file: CHECKLIST });
        }
      },
    },
    // 유출 의심 폐기: 맑은에이전트 연결과 이 기기의 다른 수집키까지 폐기 — 파일에 키가 남아 있어도 "다시 발급"이 아니라 "다시 로그인해 연결".
    homeCase('r20-leak-reconnect-required-key-on-file', 'r20-04-leak-reconnect-required', failed('reconnectRequired', { masked: 'mhk_AbCd…', collectionReady: true }), {
      state: 'failed',
      include: [
        '이 PC의 맑은에이전트 연결이 해제되었습니다', '유출 의심으로 폐기했다면 이 연결과 이 기기의 다른 수집키도 함께 폐기', '앱에서 다시 로그인해 연결하세요',
        // RV-001: 파일의 키는 "적용된"이 아니라 폐기됐을 수 있는 키로 말한다.
        '설정 파일에 남아 있는 수집키(mhk_AbCd…)는 허브에서 함께 폐기됐을 수 있어 수집되지 않을 수 있습니다',
      ],
      exclude: ['다시 발급', '[다시 시도]', '폐기됨 · 재발급', '적용된 수집키'],
      buttons: ['허브 연결 열기'],
    }, async (page, { bugs }) => {
      await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '허브 연결 열기' }).click();
      await page.waitForTimeout(200);
      if (!(await page.evaluate(() => location.hash)).startsWith('#/settings/mcp')) bugs.push({ severity: 'Major', symptom: '"허브 연결 열기"가 다시 로그인할 #/settings/mcp로 이동하지 않음', file: CHECKLIST });
    }),
    homeCase('r20-leak-blocked-suspected-leak-key-on-file', 'r20-05-leak-blocked-suspected-leak', failed('blocked', { masked: 'mhk_AbCd…', blockedReason: 'suspectedLeak' }), {
      state: 'failed',
      // RV-002: GET 200 = 연결은 살아 있다. "연결이 해제되었습니다"가 아니라 "발급이 막혔다 → 연결을 해제한 뒤 다시 로그인".
      include: ['사유: 유출 의심', '이 기기에 유출 의심으로 폐기된 수집키가 있어 이 연결로는 발급이 막혔습니다', '앱의 허브 연결을 해제한 뒤 다시 로그인해 연결하세요', '폐기됐을 수 있어 수집되지 않을 수 있습니다'],
      exclude: ['다시 발급', '[다시 시도]', '연결이 해제되었습니다', '연결과 이 기기의 다른 수집키도 함께 폐기', '적용된 수집키'],
      buttons: ['허브 연결 열기'],
    }),
    // 설정 > OTel 패널에서도 같다.
    {
      id: 'r20-settings-panel-leak-reconnect',
      startHash: '#/settings/otel',
      fixtures: withLink(failed('reconnectRequired', { masked: 'mhk_AbCd…' }), { otel_settings_get: settingsFixture({ ingestKey: KEY_SET }) }),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await shot('r20-07-settings-panel-leak-reconnect');
        const p = await panel(page);
        if (!p) return bugs.push({ severity: 'Critical', symptom: '설정 > OTel에 자동 연동 패널이 없음', file: PANEL });
        if (p.state !== 'failed') bugs.push({ severity: 'Critical', symptom: `패널 data-state가 failed가 아님(실제: ${p.state})`, file: PANEL });
        for (const t of ['이 PC의 맑은에이전트 연결이 해제되었습니다', '앱에서 다시 로그인해 연결하세요']) if (!p.text.includes(t)) bugs.push({ severity: 'Major', symptom: `패널에 "${t}"가 없음(실제: ${p.text})`, file: PANEL });
        if (p.text.includes('다시 발급')) bugs.push({ severity: 'Critical', symptom: `연결이 끊겼는데 패널이 "다시 발급"을 말함(${p.text})`, file: PANEL });
        if (JSON.stringify(p.buttons) !== JSON.stringify(['허브 연결 열기'])) bugs.push({ severity: 'Major', symptom: `패널 버튼이 [허브 연결 열기]가 아님(${JSON.stringify(p.buttons)})`, file: PANEL });
        if (p.text.includes('적용된 수집키')) bugs.push({ severity: 'Major', symptom: `연결이 끊겼는데 패널이 파일의 키를 "적용된 수집키"라고 말함(${p.text})`, file: PANEL });
        await keyBlockCheck(page, bugs, { badge: '파일에 남음 · 폐기됐을 수 있음', note: '허브에서 함께 폐기됐을 수 있어 수집되지 않을 수 있습니다' });
        await noSecrets(page, bugs, PANEL);
      },
    },
    // RV-001 대조군: 폐기 신호가 없는 정상 적용 상태는 그대로 초록 "설정됨"이다(배지를 일괄로 바꾸지 않았다).
    {
      id: 'r20fix-settings-key-badge-normal-applied-stays-set',
      startHash: '#/settings/otel',
      fixtures: withLink(status('confirmed', { masked: 'mhk_AbCd…' }), { otel_settings_get: settingsFixture({ ingestKey: KEY_SET }) }),
      async run(page, { bugs }) {
        await page.waitForTimeout(500);
        const text = (await page.locator('#otel-ingest-key-status').textContent().catch(() => '')) ?? '';
        const cls = (await page.locator('#otel-ingest-key-status .badge').first().getAttribute('class').catch(() => '')) ?? '';
        if (!text.includes('설정됨') || !cls.includes('badge-active')) bugs.push({ severity: 'Major', symptom: `정상 적용 상태의 수집키 배지가 초록 "설정됨"이 아님(${text} / ${cls})`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        if ((await page.locator('#otel-ingest-key-liveness-note').count()) !== 0) bugs.push({ severity: 'Major', symptom: '정상 적용 상태에 폐기 가능성 안내가 보임', file: 'src/views/settings.ts:renderIngestKeyBlock' });
      },
    },
    // RV-003: OTel 카드가 "연결이 해제되었습니다"(reconnectRequired)라고 말하면, [허브 연결 열기]가 허브 상태 확인을 1회 먼저 시켜
    // 허브 연결 행이 "연결됨"으로 남지 않고 "연결 해제됨 · [다시 연결]"이 된다(라이브 허브 호출 없이: hub_auth_renew_now 스텁이 REVOKED를 돌려준다).
    {
      id: 'r20fix-reconnect-required-opens-hub-row-already-revoked',
      fixtures: withLink(failed('reconnectRequired', { masked: 'mhk_AbCd…', collectionReady: true })),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '허브 연결 열기' }).click();
        await page.waitForTimeout(700);
        if (!(await page.evaluate(() => location.hash)).startsWith('#/settings/mcp')) bugs.push({ severity: 'Major', symptom: '"허브 연결 열기"가 #/settings/mcp로 이동하지 않음', file: CHECKLIST });
        if ((await invoked(page, 'hub_auth_renew_now')).length !== 1) bugs.push({ severity: 'Major', symptom: `재연결 필요인데 허브 상태 확인(hub_auth_renew_now)이 1회가 아님(${(await invoked(page, 'hub_auth_renew_now')).length})`, file: 'src/views/settings.ts:openHubConnection' });
        const hubRow = page.locator('.mcp-row', { hasText: '허브 연결(기기 토큰)' });
        const text = (await hubRow.textContent()) ?? '';
        const buttons = await hubRow.locator('button').allTextContents();
        await shot('01-hub-row-after-open');
        if (!text.includes('연결 해제됨')) bugs.push({ severity: 'Major', symptom: `OTel 카드는 연결 해제라는데 허브 연결 행이 "연결 해제됨"이 아님(${text})`, file: 'src/views/settings.ts:openHubConnection' });
        if (text.includes('연결됨') && !text.includes('연결 해제됨')) bugs.push({ severity: 'Critical', symptom: `허브 연결 행이 "연결됨"을 말함(${text})`, file: 'src/views/settings.ts:openHubConnection' });
        if (JSON.stringify(buttons) !== JSON.stringify(['다시 연결'])) bugs.push({ severity: 'Major', symptom: `허브 연결 행 버튼이 [다시 연결]이 아님(${JSON.stringify(buttons)})`, file: 'src/views/settings.ts:renderHubAuthRow' });
        await noSecrets(page, bugs, 'src/views/settings.ts:openHubConnection');
      },
    },
    // RV-003 대조군: 유출 의심 차단(GET 200)은 연결이 실제로 살아 있다 — 상태 확인(갱신)을 시키지 않고, 행에는 안내대로 [연결 해제]가 있다.
    {
      id: 'r20fix-blocked-suspected-leak-hub-row-stays-connected-with-disconnect',
      fixtures: withLink(failed('blocked', { masked: 'mhk_AbCd…', blockedReason: 'suspectedLeak' })),
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(500);
        await page.locator('.onboard-row[data-step="otel-link"]').getByRole('button', { name: '허브 연결 열기' }).click();
        await page.waitForTimeout(700);
        if ((await invoked(page, 'hub_auth_renew_now')).length !== 0) bugs.push({ severity: 'Major', symptom: '연결이 살아 있는 차단 상태에서 허브 갱신을 시켰음', file: 'src/views/settings.ts:openHubConnection' });
        const hubRow = page.locator('.mcp-row', { hasText: '허브 연결(기기 토큰)' });
        const text = (await hubRow.textContent()) ?? '';
        const buttons = await hubRow.locator('button').allTextContents();
        await shot('01-hub-row-connected-disconnect-available');
        if (!text.includes('연결됨')) bugs.push({ severity: 'Major', symptom: `연결이 살아 있는데 허브 연결 행이 "연결됨"이 아님(${text})`, file: 'src/views/settings.ts:renderHubAuthRow' });
        if (!buttons.includes('연결 해제')) bugs.push({ severity: 'Major', symptom: `안내가 "연결을 해제한 뒤"인데 [연결 해제] 버튼이 없음(${JSON.stringify(buttons)})`, file: 'src/views/settings.ts:renderHubAuthRow' });
      },
    },
    // self 200(서버에 기능 없음): 재로그인도 재발급도 아니다 — 키가 없는 PC의 문구와 버튼 없음.
    homeCase('r20-self-200-feature-unavailable', 'r20-08-self-200-feature-unavailable', failed('featureUnavailable', { ...OFF }), {
      state: 'todo',
      include: ['허브에 수집키 자동 발급 기능이 아직 없습니다', '허브 연결은 정상'],
      exclude: ['다시 로그인', '다시 발급', '다시 연결'],
      buttons: [],
    }),

    // ---- R19: 서버 권장값이 파일에 쓰인 뒤의 설정 > OTel 폼(이벤트 후 갱신·고치던 칸 보존·고친 칸만 저장·탭 재진입) ----
    {
      id: 'server-env-event-refreshes-form-keeps-typing',
      startHash: '#/settings/otel',
      fixtures: { ...open, otel_settings_get: formSettings(FORM_FILE_BEFORE), otel_ingest_link_status: status('applied', { masked: 'mhk_AbCd…' }) },
      initScripts: [{ fn: installStatefulOtelSettings, arg: { settings: formSettings(FORM_FILE_BEFORE), link: status('applied', { masked: 'mhk_AbCd…' }) } }],
      async run(page, { shot, bugs, emitEvent }) {
        await settleForm(page);
        const before = await formInputs(page);
        expectEqual(bugs, '이벤트 전 로그 간격 칸', before.OTEL_LOGS_EXPORT_INTERVAL, '5000');
        if (!(await legacyHintShown(page))) bugs.push({ severity: 'Minor', symptom: '파일이 옛 5000 간격인데 안내가 없음(대조군)', file: FORM });
        // 사용자가 TEMPORALITY 칸을 고치는 중이다.
        await page.fill(`#otel-${TEMPORALITY}`, 'delta');
        // 앱 밖에서 서버 권장값이 파일에 쓰이고 연동 이벤트가 온다.
        await applyServerEnv(page);
        await emitEvent('otel-link-changed', null);
        await page.waitForTimeout(600);
        await shot('01-after-event');
        const after = await formInputs(page);
        expectEqual(bugs, '이벤트 뒤 로그 간격 칸이 서버 값을 따라감', after.OTEL_LOGS_EXPORT_INTERVAL, '60000');
        expectEqual(bugs, '이벤트 뒤 메트릭 간격 칸이 서버 값을 따라감', after.OTEL_METRIC_EXPORT_INTERVAL, '60000');
        expectEqual(bugs, '이벤트 뒤 사용자가 고치던 칸이 보존됨', after[TEMPORALITY], 'delta');
        if (await legacyHintShown(page)) bugs.push({ severity: 'Major', symptom: '파일이 60000으로 바뀌었는데 옛 5000 안내가 남아 있음', file: FORM });
        const detail = (await page.locator('#otel-link-detail').textContent().catch(() => '')) ?? '';
        if (!detail.includes('허브가 보낸 값으로 바뀐 설정') || !detail.includes('OTEL_LOGS_EXPORT_INTERVAL')) {
          bugs.push({ severity: 'Major', symptom: `연동 상세가 서버가 바꾼 설정을 말하지 않음: ${detail}`, file: FORM });
        }
        // 저장: 고친 칸 하나만 보낸다 — 서버가 쓴 값을 옛 화면 값으로 되돌려 보내지 않는다.
        await page.click('form.settings-form button[type=submit]');
        await page.waitForTimeout(600);
        const saves = await stubSaves(page);
        if (saves.length !== 1) bugs.push({ severity: 'Major', symptom: `저장이 1회가 아님(${saves.length})`, file: FORM });
        expectEqual(bugs, '저장 payload의 values(고친 칸만)', saves[0], { [TEMPORALITY]: 'delta' });
        const saved = await formInputs(page);
        expectEqual(bugs, '저장 뒤 로그 간격 칸', saved.OTEL_LOGS_EXPORT_INTERVAL, '60000');
      },
    },
    // 이벤트를 놓쳤어도(폼은 옛 5000을 보여 준다) 무관한 칸만 고쳐 저장하면 서버 값이 되돌려지지 않는다.
    {
      id: 'server-env-silent-file-change-save-does-not-revert',
      startHash: '#/settings/otel',
      fixtures: { ...open, otel_settings_get: formSettings(FORM_FILE_BEFORE), otel_ingest_link_status: status('applied', { masked: 'mhk_AbCd…' }) },
      initScripts: [{ fn: installStatefulOtelSettings, arg: { settings: formSettings(FORM_FILE_BEFORE), link: status('applied', { masked: 'mhk_AbCd…' }) } }],
      async run(page, { shot, bugs }) {
        await settleForm(page);
        await page.fill('#otel-OTEL_METRICS_EXPORTER', 'otlp,console');
        // 이벤트 없이 파일만 바뀐다 — 폼은 아직 옛 값을 보여 준다.
        await applyServerEnv(page);
        expectEqual(bugs, '이벤트 없이는 폼이 옛 값을 보여 줌(대조군)', (await formInputs(page)).OTEL_LOGS_EXPORT_INTERVAL, '5000');
        await page.evaluate(() => (window.__otelStub.calls.length = 0));
        await page.click('form.settings-form button[type=submit]');
        await page.waitForTimeout(600);
        await shot('01-after-save');
        const calls = await stubCalls(page);
        if (calls.indexOf('otel_settings_get') === -1 || calls.indexOf('otel_settings_get') > calls.indexOf('otel_settings_save')) {
          bugs.push({ severity: 'Major', symptom: `저장 직전 파일을 다시 읽지 않음(호출 순서 ${JSON.stringify(calls)})`, file: FORM });
        }
        const saves = await stubSaves(page);
        expectEqual(bugs, '저장 payload의 values(고친 칸만, 5000·파일에 새로 생긴 칸은 싣지 않음)', saves[0], { OTEL_METRICS_EXPORTER: 'otlp,console' });
        const saved = await formInputs(page);
        expectEqual(bugs, '저장 뒤 로그 간격 칸(서버 값 유지)', saved.OTEL_LOGS_EXPORT_INTERVAL, '60000');
      },
    },
    // 다른 탭에 갔다 돌아오면 파일을 다시 읽어 서버 값을 보여 준다.
    {
      id: 'server-env-tab-reentry-refreshes-form',
      startHash: '#/settings/otel',
      fixtures: { ...open, otel_settings_get: formSettings(FORM_FILE_BEFORE), otel_ingest_link_status: status('applied', { masked: 'mhk_AbCd…' }) },
      initScripts: [{ fn: installStatefulOtelSettings, arg: { settings: formSettings(FORM_FILE_BEFORE), link: status('applied', { masked: 'mhk_AbCd…' }) } }],
      async run(page, { shot, bugs }) {
        await settleForm(page);
        await applyServerEnv(page);
        await page.evaluate(() => (location.hash = '#/settings/mcp'));
        await page.waitForTimeout(500);
        await page.evaluate(() => (location.hash = '#/settings/otel'));
        await settleForm(page);
        await shot('01-reentry');
        const after = await formInputs(page);
        expectEqual(bugs, '재진입 뒤 로그 간격 칸', after.OTEL_LOGS_EXPORT_INTERVAL, '60000');
        expectEqual(bugs, '재진입 뒤 메트릭 간격 칸', after.OTEL_METRIC_EXPORT_INTERVAL, '60000');
        if (await legacyHintShown(page)) bugs.push({ severity: 'Major', symptom: '재진입 뒤에도 옛 5000 안내가 남아 있음', file: FORM });
      },
    },
  ];
}
