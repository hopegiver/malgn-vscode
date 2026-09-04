// 검사 ⑧ — HRS-E 증거 바인딩 (v1.1-gates 신설)
// (policy-contract.md §6 "검사 3중" (a)⑧, 완전한 규격은 architecture.md §4.8.6)
//
// `allowedInstallTargets`의 **`verified === true`인 모든 행**에 대해:
//   (a) `compat/verification-evidence/<tool>-<platform>-<manager>.json`이 존재
//   (b) 파일 안의 tool·platform·manager가 행과 정확히 일치
//   (c) resolvedVersion이 compatibility.json 하한 이상
//   (d) finishedAt이 현재 커밋 시각 이전
// 하나라도 어긋나면 머지 차단. §7.3.1 HRS-E: "사람이 지던 유일한 비기계 판정(사실
// 주장)"을 대신하는 지점이라 이 슬라이스에서 가장 등급이 높다.
//
// 핵심 로직은 **순수 함수**(evaluateHrsEvidence)로 분리한다 — 파일 존재·git 커밋
// 시각처럼 실제 I/O가 필요한 부분은 호출자가 주입한다. 이렇게 해야 회귀 테스트
// 4종(증거 없음/키 불일치/하한 미달/미래 타임스탬프)이 실제 `compat/verification-evidence/`
// 디렉터리에 파일을 만들지 않고도(작업 지시 금지 사항) 완전히 인메모리로 동작한다.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { compareVersions, parseRange, parseVersion } from '../../core/policy/semver.js';
import type { CodeConstants, CompatRequires, InstallTargetRow } from '../../core/policy/types.js';
import type { CheckResult } from '../types.js';
import { fail, ok } from '../types.js';

/** architecture.md §4.8.6 증거 산출물 스키마 그대로. */
export interface VerificationEvidence {
  readonly tool: string;
  readonly platform: string;
  readonly manager: string;
  readonly managerPath: string;
  readonly resolvedVersion: string;
  readonly codesignOutput: string | null;
  readonly installOrigin: string;
  readonly exitCodes: readonly number[];
  readonly startedAt: string;
  readonly finishedAt: string;
  readonly extensionCommit: string;
}

function evidenceFileName(row: Pick<InstallTargetRow, 'tool' | 'platform' | 'manager'>): string {
  return `${row.tool}-${row.platform}-${row.manager}.json`;
}

/**
 * policy-contract.md §6 ⓐ — tool → compatibility.json 하한 필드 매핑표. **이 표가 정본**
 * (B-3 판정): 술어를 `tool === 'claude'`로 하드코딩하지 않고 이 표를 조회한다(ⓓ) —
 * 그래야 나중에 `gh`·`wrangler` 하한이 정의되는 날 이 표에 한 줄만 추가하면 (c)가
 * 자동으로 적용되고 검사 코드(`resolveVersionFloor`/`evaluateHrsEvidence`)를 고칠 필요가
 * 없다. 이 표에 없는 tool은 (c) 하한 비교를 **건너뛴다**(다른 3개 조건 (a)(b)(d)는
 * 그대로 적용) — "미확인은 미확인으로 표기, 추정으로 메우지 않는다"(architecture.md
 * §4.8.6 원칙)를 따른 것이며 임의로 claudeCode 하한을 다른 tool에 전용하지 않는다.
 * fail-closed(하한 없는 tool의 verified:true를 전부 차단)로 가지 않는 이유도 같다 —
 * 그러면 `gh` 승격의 유일한 해법이 "근거 없는 숫자를 지어내는 것"이 된다.
 */
const VERSION_FLOOR_FIELD_BY_TOOL: Readonly<Record<string, keyof Pick<CompatRequires, 'claudeCode' | 'malgnAgent'>>> = {
  claude: 'claudeCode',
};

/**
 * `verified:true` 행의 tool → compatibility.json 하한(semver range 하한)을 결정한다.
 * 매핑표(`VERSION_FLOOR_FIELD_BY_TOOL`) 조회일 뿐 하드코딩된 분기가 아니다(ⓓ).
 */
function resolveVersionFloor(tool: string, constants: Pick<CodeConstants, 'requires'>): string | null {
  const field = VERSION_FLOOR_FIELD_BY_TOOL[tool];
  if (field === undefined) return null;
  return constants.requires[field];
}

export interface EvaluateHrsEvidenceInput {
  readonly verifiedRows: readonly InstallTargetRow[];
  readonly loadEvidence: (fileName: string) => VerificationEvidence | undefined;
  readonly constants: Pick<CodeConstants, 'requires'>;
  readonly commitTimestamp: Date;
}

export interface HrsEvidenceViolation {
  readonly ref: string;
  readonly message: string;
}

/** §6 ⓒ — (c)를 건너뛴 사실의 출력 단위. "조용한 통과 금지"를 만족하기 위해
 * `evaluateHrsEvidence`가 데이터로 반환하고, 이를 소비하는 `checkHrsEvidence`(CLI 진입점,
 * 아래)가 실제로 stdout에 출력한다 — 순수 함수 자신은 IO를 하지 않는다. */
export interface HrsEvidenceSkip {
  readonly tool: string;
  /** 형식은 §6 ⓒ 원문 그대로: "(c) — <tool> 하한 미정의" */
  readonly reason: string;
}

export interface EvaluateHrsEvidenceResult {
  readonly violations: readonly HrsEvidenceViolation[];
  readonly skipped: readonly HrsEvidenceSkip[];
}

/** 순수 검사 로직. runner.ts와 회귀 테스트가 공유한다. */
export function evaluateHrsEvidence(input: EvaluateHrsEvidenceInput): EvaluateHrsEvidenceResult {
  const violations: HrsEvidenceViolation[] = [];
  const skipped: HrsEvidenceSkip[] = [];
  const skippedTools = new Set<string>();

  for (const row of input.verifiedRows) {
    if (row.verified !== true) continue; // 방어적 재확인(호출자가 이미 필터했어도)
    const fileName = evidenceFileName(row);
    const evidence = input.loadEvidence(fileName);

    if (evidence === undefined) {
      violations.push({
        ref: 'HRS-E (a)',
        message: `verified:true 행(${row.tool}/${row.platform}/${row.manager})에 대응하는 증거 파일이 없습니다: compat/verification-evidence/${fileName}`,
      });
      continue;
    }

    if (evidence.tool !== row.tool || evidence.platform !== row.platform || evidence.manager !== row.manager) {
      violations.push({
        ref: 'HRS-E (b)',
        message: `증거 파일 ${fileName}의 tool/platform/manager(${evidence.tool}/${evidence.platform}/${evidence.manager})가 행(${row.tool}/${row.platform}/${row.manager})과 일치하지 않습니다`,
      });
      continue;
    }

    const floorRange = resolveVersionFloor(row.tool, input.constants);
    if (floorRange !== null) {
      const parsedRange = parseRange(floorRange);
      const resolvedVersion = parseVersion(evidence.resolvedVersion);
      if (parsedRange === null || resolvedVersion === null) {
        violations.push({
          ref: 'HRS-E (c)',
          message: `증거 파일 ${fileName}의 resolvedVersion(${evidence.resolvedVersion}) 또는 하한(${floorRange})을 파싱할 수 없습니다`,
        });
      } else if (compareVersions(resolvedVersion, parsedRange.lower.version) < 0) {
        violations.push({
          ref: 'HRS-E (c)',
          message: `증거 파일 ${fileName}의 resolvedVersion(${evidence.resolvedVersion})이 compatibility.json 하한(${floorRange}) 미만입니다`,
        });
      }
    } else {
      // §6 ⓒ — (c)를 건너뛴 사실을 출력 대상으로 남긴다(도구당 1회만 — 같은 tool의
      // 여러 행이 중복 출력을 만들지 않는다).
      if (!skippedTools.has(row.tool)) {
        skippedTools.add(row.tool);
        skipped.push({ tool: row.tool, reason: `(c) — ${row.tool} 하한 미정의` });
      }
      // §6 ⓑ(c′) — (c)를 건너뛰어도 resolvedVersion은 "비어 있지 않은 파싱 가능한
      // 버전 문자열"이어야 한다. 하한이 없다는 것이 "버전을 기록하지 않아도 된다"는
      // 뜻은 아니다 — 이것이 없으면 resolvedVersion이 빈 문자열이어도 검사가 통과한다.
      if (parseVersion(evidence.resolvedVersion) === null) {
        violations.push({
          ref: 'HRS-E (c′)',
          message: `증거 파일 ${fileName}의 resolvedVersion이 비어 있거나 파싱 가능한 버전 문자열이 아닙니다: "${evidence.resolvedVersion}"`,
        });
      }
    }

    const finishedAt = new Date(evidence.finishedAt);
    if (Number.isNaN(finishedAt.getTime())) {
      violations.push({ ref: 'HRS-E (d)', message: `증거 파일 ${fileName}의 finishedAt이 유효한 시각이 아닙니다: ${evidence.finishedAt}` });
    } else if (finishedAt.getTime() >= input.commitTimestamp.getTime()) {
      violations.push({
        ref: 'HRS-E (d)',
        message: `증거 파일 ${fileName}의 finishedAt(${evidence.finishedAt})이 현재 커밋 시각(${input.commitTimestamp.toISOString()}) 이전이 아닙니다`,
      });
    }
  }

  return { violations, skipped };
}

/** git HEAD 커밋 시각을 구한다. git이 없거나 커밋이 0개인 저장소(예: 최초 커밋 전
 * 로컬 작업 상태)에서는 안전한 폴백으로 현재 시각을 쓴다 — 이 경우 "미래 타임스탬프"
 * 판정 기준이 현재 시각이 되어 오히려 더 엄격해지므로 fail-open이 아니다. */
export function getCommitTimestamp(cwd: string): Date {
  try {
    const out = execFileSync('git', ['log', '-1', '--format=%cI'], { cwd, encoding: 'utf8' }).trim();
    if (out.length > 0) {
      const parsed = new Date(out);
      if (!Number.isNaN(parsed.getTime())) return parsed;
    }
  } catch {
    // git 없음 / 커밋 0개 — 폴백
  }
  return new Date();
}

export interface Check8Input {
  readonly installTargets: readonly InstallTargetRow[];
  readonly constants: Pick<CodeConstants, 'requires'>;
  readonly evidenceDir: string;
  readonly commitTimestamp: Date;
}

export function checkHrsEvidence(input: Check8Input): CheckResult {
  const id = '⑧';
  const label = 'HRS-E — verified:true 행 ↔ 검증 증거 파일 바인딩';

  const verifiedRows = input.installTargets.filter((r) => r.verified === true);

  const { violations, skipped } = evaluateHrsEvidence({
    verifiedRows,
    constants: input.constants,
    commitTimestamp: input.commitTimestamp,
    loadEvidence: (fileName) => {
      const path = join(input.evidenceDir, fileName);
      if (!existsSync(path)) return undefined;
      try {
        return JSON.parse(readFileSync(path, 'utf8')) as VerificationEvidence;
      } catch {
        return undefined;
      }
    },
  });

  // §6 ⓒ — 건너뛴 사실을 출력한다(조용한 통과 금지). 이 함수(CLI 진입점)만 IO를 하고
  // `evaluateHrsEvidence`는 순수 함수로 남는다.
  for (const skip of skipped) {
    console.log(`[compat-check] ${id} skipped: ${skip.reason}`);
  }

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
