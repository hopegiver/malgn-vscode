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
const ISSUE_URL = 'https://malgnai-hub.apiserver.kr/keys';

function otelSettings({ ingestKey, telemetry = '1' }) {
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
  };
}

const UNSET = { state: 'unset', masked: null, issueUrl: ISSUE_URL };
const SET_A = { state: 'set', masked: 'mhk_AAAA…', issueUrl: ISSUE_URL };
const SET_B = { state: 'set', masked: 'mhk_BBBB…', issueUrl: ISSUE_URL };
const UNRECOGNIZED = { state: 'unrecognized', masked: null, issueUrl: ISSUE_URL };

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
  return [
    // 텔레메트리 켜짐 + 키 없음 → "허브에 수집되지 않음" 경고, 삭제 버튼 없음.
    {
      id: 'unset-telemetry-on',
      fixtures: { ...base, otel_settings_get: otelSettings({ ingestKey: UNSET }) },
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
        if ((await page.getByRole('button', { name: /허브에서 수집키 발급/ }).count()) !== 1) {
          bugs.push({ severity: 'Major', symptom: '허브 키 발급 페이지 안내 버튼이 없음', file: 'src/views/settings.ts:renderIngestKeyBlock' });
        }
      },
    },
    // 텔레메트리 꺼짐 + 키 없음 → 경고 없음.
    {
      id: 'unset-telemetry-off',
      fixtures: { ...base, otel_settings_get: otelSettings({ ingestKey: UNSET, telemetry: '0' }) },
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
        ...base,
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
        await shot('03-after-save-masked-b');
      },
    },
    // 삭제 흐름 — 확인 다이얼로그 → clear payload, 다른 입력 드래프트는 보존.
    {
      id: 'clear-flow',
      fixtures: {
        ...base,
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
        await shot('02-after-clear');
      },
    },
    // 형식이 다른 Authorization → 원문 없이 "형식 불명" + 경고.
    {
      id: 'unrecognized',
      fixtures: { ...base, otel_settings_get: otelSettings({ ingestKey: UNRECOGNIZED }) },
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
        ...base,
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
  ];
}
