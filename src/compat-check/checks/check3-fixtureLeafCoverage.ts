// 검사 ③ — 모든 리프 필드가 §0 싱크 분류와 상단 전수 검증표의 행을 갖는지 (PR-11 ①)
// (policy-contract.md §6 "검사 3중" (a)③, §0 원문: "정책 fixture의 모든 리프 경로를
// 열거해 이 문서 상단 전수 검증표의 행 집합과 대조한다. 행 없는 필드가 하나라도
// 있으면 CI 실패다")
//
// check2와 같은 정책 fixture(§1 JSONC 예시)를 재사용해 **모든 리프 경로**를 기계적으로
// 열거하고, 그 경로가 전수 검증표(policy-contract.md 상단 "필드별 전수 검증표")의 27개
// 리프 항목 중 하나로 대응되는지 확인한다. loader.ts의 fieldCoverage.test.ts가 이미
// "표에 있는 필드를 코드가 실제로 검증하는가"(반대 방향)를 증명하므로, 이 검사는 그
// 짝인 "fixture에 표에 없는 필드가 있는가"(PR-11①이 문자 그대로 요구하는 방향)를
// 담당한다 — 둘은 서로 다른 실패 모드를 잡는다.

import { readFileSync } from 'node:fs';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';
import { extractFencedBlockContaining, stripJsonComments } from '../lib/textUtils.js';

/**
 * policy-contract.md 상단 "필드별 전수 검증표"(17행)를 리프 단위로 분해한 27개 항목.
 * loader.ts의 fieldCoverage.test.ts가 표에서 독립적으로 분해한 27개 항목과 값이
 * 같아야 하지만(둘 다 같은 표에서 나온다), 파일을 공유하지 않는다 — 표가 바뀌었는데
 * 한쪽만 갱신되는 사고를 두 파일이 서로 다른 각도(검증기 반응 vs fixture 경로)로
 * 잡기 위해서다.
 */
export const DOC_TABLE_LEAF_FIELDS = [
  'schemaVersion',
  'generatedAt',
  'extension.latestVersion',
  'extension.downloadHint',
  'killSwitch.minExtensionVersion',
  'killSwitch.maxExtensionVersion',
  'killSwitch.disableProviders[]',
  'rollout[].provider',
  'rollout[].percent',
  'killSwitch.message',
  'killSwitch.upgradeHint',
  'compat.malgnAgent',
  'compat.claudeCode',
  'agent.marketplace',
  'agent.plugin',
  'agent.scope',
  'agent.channel',
  'otel.env(키 이름 화이트리스트)',
  'otel.env(OTEL_EXPORTER_OTLP_*_ENDPOINT)',
  'otel.env(프라이버시 4키)',
  'otel.env(OTEL_RESOURCE_ATTRIBUTES 금지)',
  'otel.headersHelper.kind',
  'otel.headersHelper.service/.account',
  'github.requiredScopes[]',
  'cloudflare.loginMode',
  'install.mode',
  '문서 전체(PR-5 시크릿 키 이름 금지)',
] as const;

/** fixture 경로 → 전수 검증표 항목 이름 치환(그룹화된 표 행 대응). 나머지는 항등 매핑. */
const PATH_OVERRIDES: Readonly<Record<string, string>> = {
  'otel.env(*)': 'otel.env(키 이름 화이트리스트)',
  'otel.headersHelper.service': 'otel.headersHelper.service/.account',
  'otel.headersHelper.account': 'otel.headersHelper.service/.account',
};

/** 정책 fixture 객체를 재귀적으로 걸어 표 명명 규약(`foo[]`, `a.b`)과 같은 형태의
 * 리프 경로 집합을 만든다. `otel.env`는 동적 키를 가진 맵이라 개별 키로 내려가지
 * 않고 통짜 버킷(`otel.env(*)`)으로 취급한다(그 내부 4갈래 분류는 loader.ts의
 * 몫이지 fixture 경로 열거의 몫이 아니다). */
function walkLeafPaths(value: unknown, path: string, out: Set<string>): void {
  if (path === 'otel.env') {
    out.add('otel.env(*)');
    return;
  }
  if (Array.isArray(value)) {
    const arrPath = `${path}[]`;
    if (value.length === 0) {
      out.add(arrPath);
      return;
    }
    for (const el of value) walkLeafPaths(el, arrPath, out);
    return;
  }
  if (value !== null && typeof value === 'object') {
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      walkLeafPaths(child, path === '' ? key : `${path}.${key}`, out);
    }
    return;
  }
  out.add(path);
}

export interface Check3Input {
  readonly policyContractMdPath: string;
}

export function checkFixtureLeafCoverage(input: Check3Input): CheckResult {
  const id = '③';
  const label = '정책 fixture의 모든 리프 경로가 전수 검증표의 행을 갖는지 (PR-11①)';
  const violations: { ref: string; message: string }[] = [];

  const doc = readFileSync(input.policyContractMdPath, 'utf8');
  const block = extractFencedBlockContaining(doc, '"schemaVersion": 1, "generatedAt"');
  if (block === null) {
    return fail(id, label, [{ ref: '§1', message: 'policy-contract.md §1의 정책 스키마 JSONC 예시 블록을 찾을 수 없습니다' }]);
  }

  let fixture: unknown;
  try {
    fixture = JSON.parse(stripJsonComments(block));
  } catch (error) {
    return fail(id, label, [
      { ref: '§1', message: `정책 스키마 예시 블록이 유효한 JSONC가 아닙니다: ${error instanceof Error ? error.message : String(error)}` },
    ]);
  }

  const rawPaths = new Set<string>();
  walkLeafPaths(fixture, '', rawPaths);

  const knownRows = new Set<string>(DOC_TABLE_LEAF_FIELDS);
  // PR-5(문서 전체) 행은 특정 리프 경로에 대응하지 않는 전역 규칙이라 열거 대상이 아니다.

  for (const path of rawPaths) {
    const canonical = PATH_OVERRIDES[path] ?? path;
    if (!knownRows.has(canonical)) {
      violations.push({
        ref: 'PR-11①',
        message: `정책 fixture 리프 경로 "${path}"가 전수 검증표에 없습니다`,
      });
    }
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
