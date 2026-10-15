// 흐름 — OTel 설정 탭의 허브 수집키(OTEL_EXPORTER_OTLP_HEADERS) 입력/표시 UI
// (src/views/settings.ts:renderIngestKeyBlock). 검증 범위는 TS/DOM 층뿐이다 —
// Rust 쪽 원문 비노출·0600·형식 검증은 `cargo test`(otel_settings.rs·otel_ingest_key.rs)가 맡는다.
//
// 핵심 불변식(원문 비노출): 어떤 상태에서도 가짜 키 원문이 화면 DOM(HTML·텍스트·속성)과
// 스크린샷에 남지 않는다. 입력칸은 password 타입이라 스크린샷에는 점으로만 보인다.
// 가짜 키는 테스트 전용 값(`mhk_` + 'A'×43 / 'B'×43)만 쓴다 — 실제 키가 아니다.
export const flowId = 'otelIngestKey';
export const startHash = '#/settings/otel';

const FAKE_KEY_A = `mhk_${'A'.repeat(43)}`;
const FAKE_KEY_B = `mhk_${'B'.repeat(43)}`;
const HUB_EP = { targetsHub: true, insecure: false, foreignHost: false };
// 사내 https collector(허브 아님) / 평문 http collector.
const INTERNAL_EP = { targetsHub: false, insecure: false, foreignHost: true };
const HTTP_EP = { targetsHub: false, insecure: true, foreignHost: true };

function otelSettings({ ingestKey, telemetry = '1', ingestEndpoint = HUB_EP }) {
  return {
    settingsPath: '/Users/dev/.claude/settings.json',
    fileExists: true,
    parseError: null,
    // 실제 Rust 응답처럼 managedKeys에는 헤더 키가 있지만 values에는 없다.
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
    values: {
      CLAUDE_CODE_ENABLE_TELEMETRY: telemetry,
      OTEL_EXPORTER_OTLP_PROTOCOL: 'http/protobuf',
      OTEL_METRICS_EXPORTER: 'otlp',
      OTEL_LOGS_EXPORTER: 'otlp',
      OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: 'https://malgnai-hub.apiserver.kr/v1/metrics',
      OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: 'https://malgnai-hub.apiserver.kr/v1/logs',
    },
    defaults: {},
    endpointDefaultsInjected: true,
    ingestKey,
    ingestEndpoint,
  };
}

const UNSET = { state: 'unset', masked: null };
const SET_A = { state: 'set', masked: 'mhk_AAAA…' };
const SET_B = { state: 'set', masked: 'mhk_BBBB…' };
const UNRECOGNIZED = { state: 'unrecognized', masked: null };

// 화면(DOM 전체: HTML 직렬화 + 보이는 텍스트 + input.value)에 원문이 없는지 점검한다.
async function assertNoRawKey(page, bugs, where, secrets) {
  const dump = await page.evaluate(() => {
    const values = [...document.querySelectorAll('input,textarea')].map((n) => n.value).join('\n');
    return { html: document.documentElement.outerHTML, text: document.body.innerText, values };
  });
  for (const secret of secrets) {
    // 'AAAA'/'BBBB'(마스킹이 허용하는 4자)를 넘는 연속 반복이 있으면 원문(일부) 노출이다.
    const tail = secret.slice(0, 12); // mhk_ + 8자
    if (dump.html.includes(tail) || dump.text.includes(tail)) {
      bugs.push({ severity: 'Critical', symptom: `${where}: 수집키 원문(앞 12자 이상)이 DOM/텍스트에 노출됨`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
    }
  }
  return dump;
}

async function saveCalls(page) {
  return page.evaluate(() => window.__invokeLog.filter((e) => e.cmd === 'otel_settings_save').map((e) => e.args));
}

export function scenarios(base) {
  // 이 흐름의 기본은 '수동 입력' 경로다 — 허브에 연결되어 있으면(base 기본값) 앱이 수집키를 관리하므로 입력칸이 잠긴다(RV-02).
  // 그래서 허브 미연결 상태를 기본으로 두고, 연결된 경우는 아래 managed-* 시나리오가 따로 본다.
  // 연동 상태를 돌려주는 커맨드(조회·재시도·재확인·확인 답) 전부가 같은 연결 상태를 말해야 저장 뒤 재조회에서도 상태가 유지된다.
  const withHub = (hubConnected) => {
    const out = { ...base };
    for (const cmd of ['otel_ingest_link_status', 'otel_link_retry', 'otel_link_recheck', 'otel_link_answer_confirm']) {
      out[cmd] = { ...base[cmd], hubConnected, issueSupported: true };
    }
    return out;
  };
  const manualBase = withHub(false);
  const managedBase = withHub(true);
  return [
    // 텔레메트리 켜짐 + 키 없음 → "허브에 수집되지 않음" 경고, 삭제 버튼 없음.
    {
      id: 'unset-telemetry-on',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: UNSET }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await shot('01-unset-warning');
        if ((await page.locator('#otel-ingest-key-warning').count()) !== 1) {
          bugs.push({ severity: 'Major', symptom: '텔레메트리 켜짐+수집키 미설정인데 "허브에 수집되지 않음" 경고가 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if (!(await page.locator('#otel-ingest-key-status').innerText()).includes('미설정')) {
          bugs.push({ severity: 'Major', symptom: '미설정 상태 배지가 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-clear').count()) !== 0) {
          bugs.push({ severity: 'Minor', symptom: '미설정인데 "수집키 삭제" 버튼이 보임', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-EXPORTER_OTLP_HEADERS, #otel-OTEL_EXPORTER_OTLP_HEADERS').count()) !== 0) {
          bugs.push({ severity: 'Critical', symptom: 'OTEL_EXPORTER_OTLP_HEADERS가 일반 텍스트 필드로도 그려짐', file: 'src/views/settings.ts:renderOtelPanel' });
        }
        if ((await page.locator('#otel-ingest-key').getAttribute('type')) !== 'password') {
          bugs.push({ severity: 'Critical', symptom: '수집키 입력칸이 password 타입이 아님', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        const bodyText = await page.locator('#otel-ingest-key-block').innerText();
        if (!bodyText.includes('대화·이슈·채팅에 붙여 넣지 마세요')) {
          bugs.push({ severity: 'Major', symptom: '허브 NOTICE 취지의 주의 문구가 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        // RV-001: 허브에 수집키 셀프서비스 화면이 없으므로 /keys(기기 토큰 화면)로 보내는 버튼이 없어야 한다.
        if ((await page.getByRole('button', { name: /발급/ }).count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '수집키 발급 버튼이 있음(허브에 수집키 발급 화면이 없어 기기 토큰 화면으로 유도됨)', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-block a[href]').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '수집키 블록에 외부 링크가 있음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        // RV-006(R17 갱신): 허브에 연결되어 있으면 앱이 수집키를 자동 발급한다 — "관리자에게 발급 요청"이라 쓰면 안 되고
        // (개인 수집키는 본인 기기 토큰으로만 발급된다), 수동 입력은 "직접 받은 키가 있을 때"의 대체 경로로 안내한다.
        // 폐기는 관리자 요청이 사실이다.
        if (
          !/허브에 연결되어 있으면 앱이 수집키\(mhk_…\)를 자동으로 발급해 적용/.test(bodyText) ||
          !/직접 받은 수집키가 있으면 아래에 붙여넣고 저장할 수도 있습니다\. 단, 그 뒤 허브에 연결하면 앱이 발급한 키로 교체됩니다/.test(bodyText) ||
          /관리자에게 수집키 발급을 요청/.test(bodyText) ||
          /발급 방법은 허브 관리자에게 문의/.test(bodyText) ||
          !/관리자에게 해당 수집키의 즉시 폐기를 요청/.test(bodyText)
        ) {
          bugs.push({ severity: 'Major', symptom: '발급은 "허브 연결 시 앱이 자동 발급·직접 받은 키는 수동 입력"으로, 폐기는 "관리자에게 요청"으로 안내하지 않음', file: 'src/views/settings.ts:INGEST_KEY_COPY' });
        }
        if (!/mhd_/.test(bodyText) || /\/keys/.test(bodyText) || /웹에서 발급받아/.test(bodyText)) {
          bugs.push({ severity: 'Major', symptom: '기기 토큰(mhd_)은 수집키가 아니라는 안내가 없거나 /keys 발급을 유도하는 문구가 있음', file: 'src/views/settings.ts:INGEST_KEY_COPY' });
        }
        // RV-003: 새 세션부터 적용 안내.
        if (!(await page.locator('#otel-ingest-key-scope-hint').innerText()).includes('새로 시작하는 Claude Code 세션부터')) {
          bugs.push({ severity: 'Minor', symptom: '"새로 시작하는 세션부터 적용" 안내가 없음', file: 'src/views/settings.ts:INGEST_KEY_COPY' });
        }
        await assertNoRawKey(page, bugs, '미설정 상태', [FAKE_KEY_A, FAKE_KEY_B]);
      },
    },
    // RV-002: 사내 collector(허브 아님) → 키 미설정이어도 "허브가 거부" 오경고가 없어야 한다.
    {
      id: 'unset-internal-collector-no-hub-warning',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: UNSET, ingestEndpoint: INTERNAL_EP }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        if ((await page.locator('#otel-ingest-key-warning').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '허브가 아닌 엔드포인트인데 "허브가 거부" 경고가 뜸', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if (/허브가 인증 실패로 거부/.test(await page.locator('#otel-ingest-key-block').innerText())) {
          bugs.push({ severity: 'Major', symptom: '허브가 아닌 엔드포인트인데 허브 거부 문구가 노출됨', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        await shot('01-internal-collector-no-warning');
      },
    },
    // RV-002: 키 설정됨 + 엔드포인트가 http:// → 평문 전송 경고(+ 허브 밖 경고). 저장은 막지 않는다.
    {
      id: 'set-insecure-http-endpoint',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: SET_A, ingestEndpoint: HTTP_EP }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        const insecure = page.locator('#otel-ingest-key-insecure-warning');
        if ((await insecure.count()) !== 1 || !(await insecure.innerText()).includes('평문으로 전송')) {
          bugs.push({ severity: 'Major', symptom: '키 설정됨 + http:// 엔드포인트인데 평문 전송 경고가 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-foreign-warning').count()) !== 1) {
          bugs.push({ severity: 'Minor', symptom: '허브가 아닌 호스트로 키가 전송되는데 경고가 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-warning').count()) !== 0) {
          bugs.push({ severity: 'Major', symptom: '키가 설정돼 있는데 "허브가 거부" 경고가 뜸', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        await shot('01-insecure-warning');
      },
    },
    // RV-002 대조군: 허브(https) + 키 설정됨 → 전송 경고 없음.
    {
      id: 'set-hub-https-no-transit-warning',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: SET_A }) },
      async run(page, { bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        for (const id of ['otel-ingest-key-insecure-warning', 'otel-ingest-key-foreign-warning', 'otel-ingest-key-warning']) {
          if ((await page.locator(`#${id}`).count()) !== 0) {
            bugs.push({ severity: 'Minor', symptom: `허브 https + 키 설정됨인데 경고(#${id})가 뜸`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
          }
        }
      },
    },
    // 텔레메트리 꺼짐 + 키 없음 → 경고 없음.
    {
      id: 'unset-telemetry-off',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: UNSET, telemetry: '0' }) },
      async run(page, { bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        if ((await page.locator('#otel-ingest-key-warning').count()) !== 0) {
          bugs.push({ severity: 'Minor', symptom: '텔레메트리가 꺼져 있는데 미수집 경고가 뜸', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
      },
    },
    // 설정됨(가짜 키) → 마스킹 + 설정됨 배지, 원문 없음, 입력칸은 비어 있음. 새 키 입력 후 저장.
    {
      id: 'set-masked',
      fixtures: {
        ...manualBase,
        otel_settings_get: otelSettings({ ingestKey: SET_A }),
        otel_settings_save: otelSettings({ ingestKey: SET_B }),
      },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await shot('01-set-masked');
        const status = await page.locator('#otel-ingest-key-status').innerText();
        if (!status.includes('설정됨') || !status.includes('mhk_AAAA…')) {
          bugs.push({ severity: 'Major', symptom: `설정됨 배지/마스킹 표시가 없음: ${JSON.stringify(status)}`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-warning').count()) !== 0) {
          bugs.push({ severity: 'Minor', symptom: '키가 설정돼 있는데 미수집 경고가 뜸', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        const initialValue = await page.locator('#otel-ingest-key').inputValue();
        if (initialValue !== '') {
          bugs.push({ severity: 'Critical', symptom: `입력칸이 기존 키/마스킹으로 미리 채워져 있음: ${JSON.stringify(initialValue)}`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        await assertNoRawKey(page, bugs, '설정됨 상태', [FAKE_KEY_A]);

        // (1) 새 키를 입력하지 않고 저장 → keep(마스킹 값이 payload에 실리지 않는다).
        await page.getByRole('button', { name: '저장', exact: true }).click();
        await page.waitForTimeout(300);
        let calls = await saveCalls(page);
        const keepCall = calls[0]?.payload;
        if (!keepCall || keepCall.ingestKey?.action !== 'keep') {
          bugs.push({ severity: 'Critical', symptom: `새 키 없이 저장했는데 ingestKey가 keep이 아님: ${JSON.stringify(keepCall?.ingestKey)}`, file: 'src/views/settings.ts:renderOtelPanel' });
        }
        if (JSON.stringify(calls).includes('mhk_')) {
          bugs.push({ severity: 'Critical', symptom: 'keep 저장 payload에 mhk_ 문자열(마스킹 포함)이 실림', file: 'src/views/settings.ts:renderOtelPanel' });
        }
        if ('OTEL_EXPORTER_OTLP_HEADERS' in (keepCall?.values ?? {})) {
          bugs.push({ severity: 'Critical', symptom: 'values에 OTEL_EXPORTER_OTLP_HEADERS가 실림', file: 'src/views/settings.ts:renderOtelPanel' });
        }

        // (2) 새 키 입력 → 화면에는 password 점만, 저장 시 set payload.
        await page.locator('#otel-ingest-key').fill(FAKE_KEY_B);
        await shot('02-typed-new-key-password-dots');
        await assertNoRawKey(page, bugs, '새 키 입력 중', [FAKE_KEY_B]);
        await page.getByRole('button', { name: '저장', exact: true }).click();
        await page.waitForTimeout(300);
        calls = await saveCalls(page);
        const setCall = calls[1]?.payload;
        if (!setCall || setCall.ingestKey?.action !== 'set' || setCall.ingestKey?.key !== FAKE_KEY_B) {
          bugs.push({ severity: 'Critical', symptom: `새 키 입력 후 저장 payload가 set이 아님: ${JSON.stringify(setCall?.ingestKey)}`, file: 'src/views/settings.ts:renderOtelPanel' });
        }
        // 저장 성공 후 입력칸이 비워지고, 새 마스킹이 보이며, 원문이 DOM에 없다.
        const afterValue = await page.locator('#otel-ingest-key').inputValue();
        if (afterValue !== '') {
          bugs.push({ severity: 'Critical', symptom: '저장 성공 후에도 입력칸에 키 원문이 남아 있음', file: 'src/views/settings.ts:renderOtelPanel' });
        }
        const afterStatus = await page.locator('#otel-ingest-key-status').innerText();
        if (!afterStatus.includes('mhk_BBBB…')) {
          bugs.push({ severity: 'Major', symptom: `저장 후 새 마스킹이 표시되지 않음: ${JSON.stringify(afterStatus)}`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        await assertNoRawKey(page, bugs, '저장 후', [FAKE_KEY_A, FAKE_KEY_B]);
        const savedToast = (await page.locator('.toast').allInnerTexts()).join('\n');
        if (!savedToast.includes('새로 시작하는 Claude Code 세션부터 적용')) {
          bugs.push({ severity: 'Minor', symptom: `저장 토스트에 "새 세션부터 적용" 안내가 없음: ${JSON.stringify(savedToast)}`, file: 'src/views/settings.ts:INGEST_KEY_COPY.savedToast' });
        }
        await shot('03-after-save-masked-b');
      },
    },
    // 삭제 흐름 — 확인 다이얼로그 → clear payload, 다른 입력 드래프트는 보존.
    {
      id: 'clear-flow',
      fixtures: {
        ...manualBase,
        otel_settings_get: otelSettings({ ingestKey: SET_A }),
        otel_settings_save: otelSettings({ ingestKey: UNSET }),
      },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key-clear');
        // 저장 전 다른 필드 입력(드래프트) — 삭제 후에도 남아야 한다.
        await page.locator('#otel-OTEL_METRICS_EXPORTER').fill('console');
        await page.locator('#otel-ingest-key-clear').click();
        await page.waitForSelector('.modal-overlay');
        await shot('01-clear-confirm');
        // RV-004: 로컬 삭제는 허브 쪽 폐기가 아니다 + 백업까지 지운다 + 새 세션부터 적용.
        const confirmText = await page.locator('.modal-overlay .modal-body').innerText();
        if (!confirmText.includes('허브에 등록된 키는 폐기되지 않') || !confirmText.includes('허브 관리자에게 폐기를 요청')) {
          bugs.push({ severity: 'Major', symptom: `삭제 확인 문구에 "로컬 삭제는 허브 폐기가 아니다" 안내가 없음: ${JSON.stringify(confirmText)}`, file: 'src/views/settings.ts:INGEST_KEY_COPY.clearConfirm' });
        }
        if (!confirmText.includes('백업 포함') || !confirmText.includes('새로 시작하는 Claude Code 세션부터')) {
          bugs.push({ severity: 'Minor', symptom: `삭제 확인 문구에 백업/새 세션 안내가 없음: ${JSON.stringify(confirmText)}`, file: 'src/views/settings.ts:INGEST_KEY_COPY.clearConfirm' });
        }
        await page.getByRole('button', { name: '삭제', exact: true }).last().click();
        await page.waitForTimeout(300);
        const calls = await saveCalls(page);
        const call = calls[0]?.payload;
        if (!call || call.ingestKey?.action !== 'clear' || Object.keys(call.values).length !== 0) {
          bugs.push({ severity: 'Critical', symptom: `삭제 payload가 clear 단독이 아님: ${JSON.stringify(call)}`, file: 'src/views/settings.ts:handleClearIngestKey' });
        }
        const status = await page.locator('#otel-ingest-key-status').innerText();
        if (!status.includes('미설정')) {
          bugs.push({ severity: 'Major', symptom: `삭제 후 미설정 배지가 아님: ${JSON.stringify(status)}`, file: 'src/views/settings.ts:handleClearIngestKey' });
        }
        const kept = await page.locator('#otel-OTEL_METRICS_EXPORTER').inputValue();
        if (kept !== 'console') {
          bugs.push({ severity: 'Major', symptom: `삭제 후 저장 전 다른 입력 드래프트가 유실됨: ${JSON.stringify(kept)}`, file: 'src/views/settings.ts:handleClearIngestKey' });
        }
        const clearedToast = await page.locator('.toast').allInnerTexts().catch(() => []);
        if (!clearedToast.join('\n').includes('폐기되지 않았습니다')) {
          bugs.push({ severity: 'Minor', symptom: `삭제 토스트에 "허브 키는 폐기되지 않음" 안내가 없음: ${JSON.stringify(clearedToast)}`, file: 'src/views/settings.ts:INGEST_KEY_COPY.clearedToast' });
        }
        await shot('02-after-clear');
      },
    },
    // 형식이 다른 Authorization → 원문 없이 "형식 불명" + 경고.
    {
      id: 'unrecognized',
      fixtures: { ...manualBase, otel_settings_get: otelSettings({ ingestKey: UNRECOGNIZED }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await shot('01-unrecognized');
        const status = await page.locator('#otel-ingest-key-status').innerText();
        if (!status.includes('형식 불명')) {
          bugs.push({ severity: 'Major', symptom: `형식 불명 배지가 없음: ${JSON.stringify(status)}`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        if ((await page.locator('#otel-ingest-key-clear').count()) !== 1) {
          bugs.push({ severity: 'Minor', symptom: '형식 불명 상태에서 삭제 버튼이 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
      },
    },
    // 저장 실패(형식 오류 등) — 백엔드 에러 문구(원문 미인용)만 토스트로 보이고, 입력 원문은 DOM에 없다.
    {
      id: 'save-error-no-echo',
      fixtures: {
        ...manualBase,
        otel_settings_get: otelSettings({ ingestKey: UNSET }),
        otel_settings_save: { __throw: '수집키 형식이 올바르지 않습니다. 허브에서 발급받은 mhk_로 시작하는 키 전체(47자)를 붙여넣으세요.' },
      },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await page.locator('#otel-ingest-key').fill(FAKE_KEY_B.slice(0, 40));
        await page.getByRole('button', { name: '저장', exact: true }).click();
        await page.waitForTimeout(400);
        await shot('01-save-error');
        await assertNoRawKey(page, bugs, '저장 실패 후', [FAKE_KEY_B]);
      },
    },
    // ---- R17 RV-02: 허브에 연결되어 앱이 수집키를 관리하는 동안에는 수동 입력을 받지 않는다 ----
    {
      id: 'managed-by-hub-input-locked',
      fixtures: { ...managedBase, otel_settings_get: otelSettings({ ingestKey: UNSET }), otel_settings_save: otelSettings({ ingestKey: UNSET }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await page.waitForTimeout(300);
        await shot('01-managed-unset');
        const input = page.locator('#otel-ingest-key');
        if (!(await input.isDisabled())) {
          bugs.push({ severity: 'Major', symptom: '허브에 연결되어 앱이 수집키를 관리하는데 수동 입력칸이 열려 있음(붙여넣으면 다음 점검에서 조용히 교체된다)', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        const placeholder = (await input.getAttribute('placeholder')) ?? '';
        if (!placeholder.includes('앱이 수집키를 자동 관리')) {
          bugs.push({ severity: 'Minor', symptom: `잠긴 입력칸 안내가 없음: ${JSON.stringify(placeholder)}`, file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        const hint = await page.locator('#otel-ingest-key-issue-hint').innerText();
        for (const t of ['허브에 연결되어 있는 동안은 앱이 수집키(mhk_…)를 자동으로 발급·교체', '입력을 막았습니다', '허브 연결을 해제하세요']) {
          if (!hint.includes(t)) bugs.push({ severity: 'Major', symptom: `잠금 안내에 "${t}"가 없음(실제: ${hint})`, file: 'src/views/settings.ts:INGEST_KEY_COPY.issueManaged' });
        }
        if (hint.includes('아래에 붙여넣고 저장할 수도 있습니다')) {
          bugs.push({ severity: 'Major', symptom: '입력이 잠겼는데 "붙여넣고 저장할 수도 있습니다"라고 안내함', file: 'src/views/settings.ts:INGEST_KEY_COPY.issueManaged' });
        }
        const warn = await page.locator('#otel-ingest-key-warning').innerText();
        if (warn.includes('아래에 등록할 수도 있습니다')) {
          bugs.push({ severity: 'Major', symptom: '입력이 잠겼는데 경고가 "아래에 등록할 수도 있습니다"라고 안내함', file: 'src/views/settings.ts:INGEST_KEY_COPY.warnUnsetHubManaged' });
        }
        // 저장은 수집키 없이(keep) 나간다 — 다른 설정 저장은 막지 않는다.
        await page.getByRole('button', { name: '저장', exact: true }).click();
        await page.waitForTimeout(300);
        const calls = await saveCalls(page);
        if (calls.length !== 1 || calls[0]?.payload?.ingestKey?.action !== 'keep') {
          bugs.push({ severity: 'Critical', symptom: `잠긴 상태의 저장이 ingestKey keep이 아님(${JSON.stringify(calls.map((c) => c?.payload?.ingestKey))})`, file: 'src/views/settings.ts:renderOtelPanel' });
        }
        await assertNoRawKey(page, bugs, '허브 관리 중 미설정', [FAKE_KEY_A, FAKE_KEY_B]);
      },
    },
    {
      id: 'managed-by-hub-set-locked-delete-allowed',
      fixtures: { ...managedBase, otel_settings_get: otelSettings({ ingestKey: SET_A }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await page.waitForTimeout(300);
        await shot('01-managed-set');
        if (!(await page.locator('#otel-ingest-key').isDisabled())) {
          bugs.push({ severity: 'Major', symptom: '허브 관리 중 키가 설정된 상태에서 입력칸이 열려 있음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        // 삭제는 막지 않는다(앱이 만든 키 정리) — 확인 문구가 다시 발급·교체 횟수 사용 가능성을 밝힌다.
        if ((await page.locator('#otel-ingest-key-clear').count()) !== 1) {
          bugs.push({ severity: 'Major', symptom: '허브 관리 중에도 수집키 삭제 버튼은 있어야 함', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
        await page.locator('#otel-ingest-key-clear').click();
        await page.waitForTimeout(200);
        const confirmText = (await page.locator('.modal-box').textContent()) ?? '';
        if (!confirmText.includes('키 교체 횟수(24시간 6회)')) {
          bugs.push({ severity: 'Major', symptom: `삭제 확인 문구에 키 교체 횟수 사용 가능성 안내가 없음: ${JSON.stringify(confirmText)}`, file: 'src/views/settings.ts:INGEST_KEY_COPY.clearConfirm' });
        }
        await page.getByRole('button', { name: '취소' }).click();
        if ((await saveCalls(page)).length !== 0) bugs.push({ severity: 'Critical', symptom: '취소했는데 저장이 호출됨', file: 'src/views/settings.ts:handleClearIngestKey' });
      },
    },
    {
      id: 'managed-by-hub-unrecognized-points-to-delete',
      fixtures: { ...managedBase, otel_settings_get: otelSettings({ ingestKey: UNRECOGNIZED }) },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('#otel-ingest-key');
        await page.waitForTimeout(300);
        await shot('01-managed-unrecognized');
        const warn = await page.locator('#otel-ingest-key-warning').innerText();
        for (const t of ['앱은 이 값을 바꾸지 않습니다', '[수집키 삭제]', '[다시 시도]']) {
          if (!warn.includes(t)) bugs.push({ severity: 'Major', symptom: `관리 중 형식 불명 경고에 "${t}"가 없음(실제: ${warn})`, file: 'src/views/settings.ts:INGEST_KEY_COPY.warnUnrecognizedHubManaged' });
        }
        if (warn.includes('새 수집키를 입력하고 저장하면 교체됩니다')) {
          bugs.push({ severity: 'Major', symptom: '입력이 잠겼는데 "입력하고 저장하면 교체됩니다"라고 안내함', file: 'src/views/settings.ts:INGEST_KEY_COPY.warnUnrecognizedHubManaged' });
        }
      },
    },
  ];
}
