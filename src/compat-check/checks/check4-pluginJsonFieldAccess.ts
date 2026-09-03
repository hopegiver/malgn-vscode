// 검사 ④ — `plugin.json` 접근 경로가 spec `fields` 안인지
// (policy-contract.md §6 "검사 3중" (a)④, 정본 `compat/agent-interface.spec.json`)
//
// v1.0 시점에 `.claude-plugin/plugin.json`을 실제로 읽는 코드는 아직 없다(agent
// provider는 W7 — 이 슬라이스 범위 밖). 그래서 이 검사는 "지금 당장 위반이 있는가"가
// 아니라 "향후 그 파일을 읽는 코드가 등장했을 때 자동으로 걸리는 구조가 이미 있는가"를
// 만든다: 관례는 — plugin.json을 읽는 모듈은 자신이 실제로 접근하는 필드 경로를
// `export const ACCESSED_PLUGIN_JSON_FIELDS: readonly string[]`로 자기 선언해야 하고,
// 이 검사가 그 배열을 spec.fields와 대조한다. 지금은 그런 모듈이 0개이므로 **빈
// 집합에서 통과**한다(검사 ⑧과 같은 원칙 — 작업 지시 완료판정 #6).

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

export interface Check4Input {
  readonly srcDir: string;
  readonly agentInterfaceSpecPath: string;
}

const DECLARATION_RE = /export const ACCESSED_PLUGIN_JSON_FIELDS[^=]*=\s*\[([\s\S]*?)\]/g;
const STRING_LITERAL_RE = /['"]([^'"]+)['"]/g;

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      out.push(...listTsFiles(full));
    } else if (entry.endsWith('.ts') && !entry.endsWith('.test.ts') && !entry.endsWith('.tscheck.ts')) {
      out.push(full);
    }
  }
  return out;
}

export function checkPluginJsonFieldAccess(input: Check4Input): CheckResult {
  const id = '④';
  const label = 'plugin.json 접근 경로가 agent-interface.spec.json의 fields 안인지';
  const violations: { ref: string; message: string }[] = [];

  const spec = JSON.parse(readFileSync(input.agentInterfaceSpecPath, 'utf8')) as { fields?: unknown };
  if (!Array.isArray(spec.fields) || spec.fields.some((f) => typeof f !== 'string')) {
    return fail(id, label, [{ ref: '§6', message: 'agent-interface.spec.json.fields가 문자열 배열이 아닙니다' }]);
  }
  const allowedFields = new Set<string>(spec.fields as string[]);

  let declarationCount = 0;
  for (const file of listTsFiles(input.srcDir)) {
    const text = readFileSync(file, 'utf8');
    DECLARATION_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = DECLARATION_RE.exec(text)) !== null) {
      declarationCount += 1;
      const body = m[1] ?? '';
      STRING_LITERAL_RE.lastIndex = 0;
      let sm: RegExpExecArray | null;
      while ((sm = STRING_LITERAL_RE.exec(body)) !== null) {
        const field = sm[1];
        if (field !== undefined && !allowedFields.has(field)) {
          violations.push({
            ref: '§6 fields',
            message: `${file}가 spec에 없는 plugin.json 필드에 접근합니다: ${field}`,
          });
        }
      }
    }
  }

  // declarationCount === 0(아직 plugin.json을 읽는 모듈이 없음)은 정상 상태다 — 실패로
  // 취급하지 않는다.
  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
