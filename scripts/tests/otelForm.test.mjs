// src/otelForm.ts 단위 테스트 — 자동 연동이 settings.json에 쓴 서버 권장값을 설정 > OTel 폼이 (1) 보여 주고 (2) 무관한
// 필드 저장으로 되돌려 쓰지 않는지 고정한다(R19 RV-01). 폼 화면 자체는 DOM이 필요해 순수 규칙만 시험한다.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { baselineValues, mergeDraftOnRefresh, valuesToSave, sameFormContent } from '../../src/otelForm.ts';

const SKIP = ['OTEL_RESOURCE_ATTRIBUTES', 'OTEL_EXPORTER_OTLP_HEADERS'];
const MANAGED = [
  'CLAUDE_CODE_ENABLE_TELEMETRY',
  'OTEL_LOGS_EXPORT_INTERVAL',
  'OTEL_METRIC_EXPORT_INTERVAL',
  'OTEL_METRICS_EXPORTER',
  'OTEL_LOG_USER_PROMPTS',
  'OTEL_RESOURCE_ATTRIBUTES',
  'OTEL_EXPORTER_OTLP_HEADERS',
];
const DEFAULTS = { CLAUDE_CODE_ENABLE_TELEMETRY: '1', OTEL_LOGS_EXPORT_INTERVAL: '60000', OTEL_METRIC_EXPORT_INTERVAL: '60000', OTEL_METRICS_EXPORTER: 'otlp', OTEL_LOG_USER_PROMPTS: '0' };
const settings = (values) => ({ managedKeys: MANAGED, values, defaults: DEFAULTS });
const READ_ONLY = ['OTEL_LOG_USER_PROMPTS'];

// 폼이 그려진 때: 파일에 옛 값 5000(앱 옛 기본값)이 있다. 이후 자동 연동이 서버 값 60000을 썼다.
const BEFORE = settings({ CLAUDE_CODE_ENABLE_TELEMETRY: '1', OTEL_LOGS_EXPORT_INTERVAL: '5000', OTEL_METRICS_EXPORTER: 'otlp' });
const AFTER = settings({ CLAUDE_CODE_ENABLE_TELEMETRY: '1', OTEL_LOGS_EXPORT_INTERVAL: '60000', OTEL_METRIC_EXPORT_INTERVAL: '60000', OTEL_METRICS_EXPORTER: 'otlp' });

test('baselineValues: 파일 값 → 기본값 → 빈 값 순서이고 입력칸이 없는 키는 뺀다', () => {
  const b = baselineValues(BEFORE, SKIP);
  assert.equal(b.OTEL_LOGS_EXPORT_INTERVAL, '5000', '파일 값이 기본값보다 먼저');
  assert.equal(b.OTEL_METRIC_EXPORT_INTERVAL, '60000', '파일에 없으면 기본값');
  assert.ok(!('OTEL_RESOURCE_ATTRIBUTES' in b) && !('OTEL_EXPORTER_OTLP_HEADERS' in b));
  assert.equal(baselineValues({ managedKeys: ['X'], values: {}, defaults: {} }, [])['X'], '');
});

test('RV-01 표시: 자동 연동이 파일을 바꾼 뒤 다시 읽으면 고치지 않은 칸은 새 파일 값을 보여 준다', () => {
  const draft = baselineValues(BEFORE, SKIP); // 사용자는 아무것도 고치지 않았다
  const merged = mergeDraftOnRefresh(BEFORE, AFTER, draft, SKIP);
  assert.equal(merged.OTEL_LOGS_EXPORT_INTERVAL, '60000', '옛 5000이 남지 않는다');
  assert.equal(merged.OTEL_METRIC_EXPORT_INTERVAL, '60000');
});

test('RV-01 표시: 사용자가 고치던 칸은 다시 읽어도 보존된다(입력 중 값을 날리지 않는다)', () => {
  const draft = { ...baselineValues(BEFORE, SKIP), OTEL_METRICS_EXPORTER: 'console' }; // 사용자가 이 칸을 고치는 중
  const merged = mergeDraftOnRefresh(BEFORE, AFTER, draft, SKIP);
  assert.equal(merged.OTEL_METRICS_EXPORTER, 'console', '고친 칸 보존');
  assert.equal(merged.OTEL_LOGS_EXPORT_INTERVAL, '60000', '고치지 않은 칸은 새 값');
  // 사용자가 서버가 바꾼 칸 자체를 고치는 중이면 그 입력이 이긴다(명시적 선택).
  const typing = { ...baselineValues(BEFORE, SKIP), OTEL_LOGS_EXPORT_INTERVAL: '30000' };
  assert.equal(mergeDraftOnRefresh(BEFORE, AFTER, typing, SKIP).OTEL_LOGS_EXPORT_INTERVAL, '30000');
});

test('RV-01 표시: 이전 설정이 없으면(첫 초기화) 새 기준값 그대로', () => {
  assert.deepEqual(mergeDraftOnRefresh(null, AFTER, {}, SKIP), baselineValues(AFTER, SKIP));
});

test('RV-01 저장: 무관한 필드 하나만 고쳐 저장해도 서버가 바꾼 값이 옛 값으로 되돌려지지 않는다', () => {
  // 폼은 BEFORE로 그려졌고(기준값), 사용자는 OTEL_METRICS_EXPORTER만 고쳤다. 저장 직전 파일은 AFTER다(자동 연동이 60000을 썼다).
  const baseline = baselineValues(BEFORE, SKIP);
  const draft = { ...baseline, OTEL_METRICS_EXPORTER: 'otlp,console' };
  const values = valuesToSave({ keys: Object.keys(draft), readOnlyKeys: READ_ONLY, draft, baseline, fileValues: AFTER.values });
  assert.deepEqual(values, { OTEL_METRICS_EXPORTER: 'otlp,console' }, '고친 칸만 간다 — 옛 5000이 실리지 않는다');
  assert.ok(!('OTEL_LOGS_EXPORT_INTERVAL' in values));
});

test('RV-01 저장: 아무것도 고치지 않고 저장하면 파일에 이미 있는 값은 하나도 보내지 않는다', () => {
  const baseline = baselineValues(BEFORE, SKIP);
  const values = valuesToSave({ keys: Object.keys(baseline), readOnlyKeys: READ_ONLY, draft: baseline, baseline, fileValues: AFTER.values });
  for (const k of Object.keys(values)) assert.ok(!(k in AFTER.values), `파일에 있는 ${k}를 되돌려 보냄`);
});

test('저장: 파일에 아직 없는 칸(기본값을 보여 주던 칸)은 안내대로 저장하면 기록된다', () => {
  // 처음 저장하는 사용자: 파일이 비어 있다 → 기본값 그대로도 전부 기록돼야 텔레메트리가 켜진다.
  const empty = settings({});
  const baseline = baselineValues(empty, SKIP);
  const values = valuesToSave({ keys: Object.keys(baseline), readOnlyKeys: READ_ONLY, draft: baseline, baseline, fileValues: {} });
  assert.equal(values.CLAUDE_CODE_ENABLE_TELEMETRY, '1');
  assert.equal(values.OTEL_LOGS_EXPORT_INTERVAL, '60000');
  assert.ok(!('OTEL_LOG_USER_PROMPTS' in values), '읽기 전용 키는 보내지 않는다');
});

test('저장: 사용자가 고친 칸은 서버가 같은 칸을 바꿨어도 사용자의 입력이 저장된다', () => {
  const baseline = baselineValues(BEFORE, SKIP);
  const draft = { ...baseline, OTEL_LOGS_EXPORT_INTERVAL: '30000' };
  const values = valuesToSave({ keys: Object.keys(draft), readOnlyKeys: READ_ONLY, draft, baseline, fileValues: AFTER.values });
  assert.equal(values.OTEL_LOGS_EXPORT_INTERVAL, '30000');
});

test('저장: 칸을 비우면(삭제 신호) 고친 것으로 보고 빈 값을 보낸다', () => {
  const baseline = baselineValues(BEFORE, SKIP);
  const draft = { ...baseline, OTEL_METRICS_EXPORTER: '' };
  const values = valuesToSave({ keys: Object.keys(draft), readOnlyKeys: READ_ONLY, draft, baseline, fileValues: BEFORE.values });
  assert.equal(values.OTEL_METRICS_EXPORTER, '');
});

test('sameFormContent: 같은 내용이면 true(다시 그릴 이유 없음), 파일 값이 다르면 false', () => {
  assert.equal(sameFormContent(BEFORE, structuredClone(BEFORE)), true);
  assert.equal(sameFormContent(BEFORE, AFTER), false);
});

// ---- 배선 가드: 연동 상태를 받는 모든 경로가 폼 재조회를 거치고, 설정 화면에 6키 안내가 한 번만 있다 ----
const settingsSrc = readFileSync(new URL('../../src/views/settings.ts', import.meta.url), 'utf8');

test('RV-01 배선: ingestLink.status는 applyIngestStatus 한 곳에서만 대입한다(폼 재조회를 건너뛰는 경로가 없다)', () => {
  const assigns = settingsSrc.split('\n').filter((l) => /state\.ingestLink\.status\s*=[^=]/.test(l));
  assert.equal(assigns.length, 1, assigns.join('\n'));
  assert.match(settingsSrc, /function applyIngestStatus[\s\S]{0,300}refreshOtelFormFromFile\(\)/);
});

test('RV-01 배선: 저장은 저장 직전에 파일을 다시 읽고 valuesToSave로 보낼 칸을 고른다', () => {
  assert.match(settingsSrc, /async function handleSaveOtelSettings[\s\S]{0,1500}fetchOtelSettings\(\)[\s\S]{0,300}valuesToSave\(/);
});

test('RV-02 배선: 6키 안내(AUTO_LINK_CHANGES_NOTE)는 설정 화면 소스에서 import 외에 한 곳에서만 쓴다', () => {
  const uses = settingsSrc.split('\n').filter((l) => l.includes('AUTO_LINK_CHANGES_NOTE') && !l.trim().startsWith('//') && !/^import /.test(l.trim()));
  assert.equal(uses.length, 1, uses.join('\n'));
  assert.match(uses[0], /otel-autolink-changes-note|AUTO_LINK_CHANGES_NOTE\]\)/);
});
