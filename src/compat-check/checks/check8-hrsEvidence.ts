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
import type { CodeConstants, InstallTargetRow } from '../../core/policy/types.js';
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
 * `verified:true` 행의 tool → compatibility.json 하한(semver range 하한)을 결정한다.
 * **알려진 갭**: compatibility.json은 `claude`(=claudeCode)의 하한만 코드 상수로 갖고
 * `gh`·`wrangler` 같은 다른 tool의 버전 하한은 이 계약 어디에도 정의돼 있지 않다
 * (§2가 정의하는 것은 `requires.claudeCode`/`requires.malgnAgent`뿐이다). 정의되지
 * 않은 tool은 (c) 하한 비교를 **건너뛴다**(다른 3개 조건 (a)(b)(d)는 그대로 적용) —
 * "미확인은 미확인으로 표기, 추정으로 메우지 않는다"(architecture.md §4.8.6 원칙)를
 * 따른 것이며 임의로 claudeCode 하한을 다른 tool에 전용하지 않는다. 반환문에 별도
 * 설계 갭으로 보고한다.
 */
function resolveVersionFloor(tool: string, constants: Pick<CodeConstants, 'requires'>): string | null {
  if (tool === 'claude') return constants.requires.claudeCode;
  return null;
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

/** 순수 검사 로직. runner.ts와 회귀 테스트가 공유한다. */
export function evaluateHrsEvidence(input: EvaluateHrsEvidenceInput): readonly HrsEvidenceViolation[] {
  const violations: HrsEvidenceViolation[] = [];

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

  return violations;
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

  const violations = evaluateHrsEvidence({
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

  return violations.length === 0 ? ok(id, label) : fail(id, label, violations);
}
