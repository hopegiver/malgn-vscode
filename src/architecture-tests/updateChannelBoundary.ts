// NT-R18 / C-13 해소 — architecture.md §3.6.1 "#### [C-13 해소] PR-9 비대칭의 근거를
// 플랫폼에서 코드 경계로 옮긴다" · "아키텍처 테스트 규격 (A-30 · 자동업데이트 코드를
// 쓰기 *전에* 존재해야 한다)" AT-U1~U5 정본 구현(AT-U6은 정책 리프 필드 쪽 검사라
// `core/policy/fieldCoverage.test.ts`를 확장한다 — architecture.md 원문: "fieldCoverage.
// test.ts 확장").
//
// [이 모듈이 존재하는 이유] `src/update/**`(자동업데이트 채널)는 W-N4-b(이 슬라이스
// 범위 밖)에서 만들어진다. "업데이터 구현체보다 먼저" 이 경계 검사가 있어야 하는
// 이유는 §3.6.1 원문 그대로다: "AT-U3·AT-U6이 핵심이다 — AT-U1·U2는 '지금 연결이
// 없다'를 보이지만, U3·U6은 연결을 만들려는 시도 자체가 CI에서 죽게 만든다." 지금
// `src/update/**`가 존재하지 않는 상태에서 이 검사들은 "위반 0건"으로 통과한다 — 그건
// "검사를 건너뛰어서" 통과하는 것과 다르다: 아래 각 함수는 디렉터리 부재를 "검사
// 대상이 없어 위반도 없다"로 명시적으로 처리하고(빈 배열이 아니라 그 사실 자체를
// 반환값에 남긴다), `updateChannelBoundary.test.ts`가 fixture 디렉터리로 각 함수가
// 실제로 위반을 잡는지 별도로 증명한다(완료판정 #4).
//
// [경로 표기 보정] architecture.md §3.6.1 AT-U1은 `src/update/**`↔`src/policy/**`라고
// 쓰지만, 이 저장소의 실제 정책 모듈 경로는 `src/core/policy/**`다(policy-contract.md
// §12.4 등 다른 절은 이 실제 경로를 그대로 쓴다). 이 파일은 `src/policy/**`를 그
// 실제 위치의 축약 표기로 해석한다 — 새 규격을 발명하는 것이 아니라 이미 존재하는
// 경로 하나를 가리키는 방식의 차이일 뿐이며, 이 사실을 반환문에 명시한다.

import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

export interface BoundaryCheckResult {
  readonly violations: readonly string[];
  /** 검사 대상 디렉터리 자체가 없어(아직 만들어지지 않은 `src/update/**`) 검사가
   * "대상 없음"으로 통과했는지 — CI 로그에서 "건너뜀"과 "위반 0건"을 구분하기 위해
   * 명시적으로 남긴다. */
  readonly skippedNoTarget: boolean;
}

function ok(): BoundaryCheckResult {
  return { violations: [], skippedNoTarget: false };
}
function skippedOk(): BoundaryCheckResult {
  return { violations: [], skippedNoTarget: true };
}
function failing(violations: readonly string[]): BoundaryCheckResult {
  return { violations, skippedNoTarget: false };
}

export function listTsFilesRecursive(dir: string): string[] {
  if (!existsSync(dir)) return [];
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFilesRecursive(full));
    } else if (entry.isFile() && (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx'))) {
      out.push(full);
    }
  }
  return out;
}

const IMPORT_SPECIFIER_RE = /(?:from\s+|import\()\s*['"](\.[^'"]+)['"]/g;

/** 상대 import 지정자를 절대경로로 정규화한다(.js 확장자 제거 — NodeNext ESM 규약상
 * 소스는 .ts지만 import 문은 .js를 쓴다). 디렉터리 접두사 비교만 하면 되므로 실제
 * 파일 존재 여부(resolver)까지는 확인하지 않는다 — 이 검사의 목적은 "그 디렉터리
 * 아래를 가리키는 import 문이 있는가"이지 "그 import가 실제로 resolve되는가"가 아니다. */
function resolveImportTarget(fromFile: string, specifier: string): string {
  const withoutExt = specifier.replace(/\.js$/, '');
  return resolve(dirname(fromFile), withoutExt);
}

function extractImportTargets(fileText: string, fromFile: string): string[] {
  const out: string[] = [];
  IMPORT_SPECIFIER_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = IMPORT_SPECIFIER_RE.exec(fileText)) !== null) {
    if (m[1]) out.push(resolveImportTarget(fromFile, m[1]));
  }
  return out;
}

function isUnder(candidate: string, dir: string): boolean {
  const normalizedDir = resolve(dir);
  const normalizedCandidate = resolve(candidate);
  return normalizedCandidate === normalizedDir || normalizedCandidate.startsWith(`${normalizedDir}/`);
}

/**
 * AT-U1 — 모듈 경계 양방향 금지: `updateDir` 아래 어떤 파일도 `policyDir`를 import하지
 * 않고, 그 역도 성립한다.
 */
export function checkModuleBoundaryBidirectional(updateDir: string, policyDir: string): BoundaryCheckResult {
  const updateFiles = listTsFilesRecursive(updateDir);
  const policyFiles = listTsFilesRecursive(policyDir);
  if (updateFiles.length === 0 && !existsSync(updateDir)) {
    return skippedOk();
  }

  const violations: string[] = [];
  for (const file of updateFiles) {
    const text = readFileSync(file, 'utf8');
    for (const target of extractImportTargets(text, file)) {
      if (isUnder(target, policyDir)) {
        violations.push(`AT-U1: ${relative(updateDir, file)}가 정책 모듈(${relative(updateDir, target)})을 import합니다`);
      }
    }
  }
  for (const file of policyFiles) {
    const text = readFileSync(file, 'utf8');
    for (const target of extractImportTargets(text, file)) {
      if (isUnder(target, updateDir)) {
        violations.push(`AT-U1: ${relative(policyDir, file)}가 업데이트 모듈(${relative(policyDir, target)})을 import합니다`);
      }
    }
  }
  return failing(violations);
}

/** AT-U2가 감시하는 정책 파생 타입 식별자 — architecture.md §3.6.1 AT-U2 원문 그대로. */
export const FORBIDDEN_POLICY_TYPE_IDENTIFIERS = ['EffectivePolicy', 'EffectiveKillSwitch', 'EffectiveRolloutEntry'] as const;

/**
 * AT-U2 — 타입 도달 불가: `updateDir` 아래 어떤 파일의 텍스트에도 정책 파생 타입
 * 식별자가 나타나지 않는다. [한계] 이 구현은 리터럴 식별자 등장 여부를 텍스트로
 * 검사한다 — TypeScript 컴파일러 API로 "구조적으로 같은 형태의 별도 타입"까지
 * 추적하지는 않는다(그 수준의 검증은 이 슬라이스 범위 밖이며 반환문에 명시한다).
 * 직접 이름을 쓰는 가장 흔하고 직접적인 연결 시도는 이것으로 잡힌다.
 */
export function checkTypeReachability(updateDir: string): BoundaryCheckResult {
  const files = listTsFilesRecursive(updateDir);
  if (files.length === 0 && !existsSync(updateDir)) return skippedOk();

  const violations: string[] = [];
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    for (const identifier of FORBIDDEN_POLICY_TYPE_IDENTIFIERS) {
      if (new RegExp(`\\b${identifier}\\b`).test(text)) {
        violations.push(`AT-U2: ${relative(updateDir, file)}에 금지된 정책 타입 식별자 "${identifier}"가 등장합니다`);
      }
    }
  }
  return failing(violations);
}

/** AT-U3가 감시하는 식별자 — architecture.md §3.6.1 AT-U3 원문 4종(`policy`·`killSwitch`·
 * `rollout`·`minExtensionVersion`·`maxExtensionVersion`) + [추가] W-N2(M-9)가 그 두
 * 필드를 `minAppVersion`/`maxAppVersion`으로 개명했으므로 신어휘도 함께 금지한다 —
 * 원문 목록의 상위집합이라 규격을 좁히지 않는다(반환문에 이 추가를 명시한다). */
export const FORBIDDEN_UPDATE_IDENTIFIERS = [
  'policy',
  'killSwitch',
  'rollout',
  'minExtensionVersion',
  'maxExtensionVersion',
  'minAppVersion',
  'maxAppVersion',
] as const;

/**
 * AT-U3 — 식별자 부재: `updateDir` 전체(주석 포함)에 위 식별자가 0회 등장한다. 대소문자
 * 구분 없이 검사한다(`Policy`·`EffectivePolicy`의 부분 문자열 등 변형도 잡기 위해) —
 * "연결을 만들려는 시도 자체를 죽인다"는 원문 취지상 과탐이 과소탐보다 안전한 방향이다.
 */
export function checkForbiddenIdentifiers(updateDir: string): BoundaryCheckResult {
  const files = listTsFilesRecursive(updateDir);
  if (files.length === 0 && !existsSync(updateDir)) return skippedOk();

  const violations: string[] = [];
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    for (const identifier of FORBIDDEN_UPDATE_IDENTIFIERS) {
      if (new RegExp(identifier, 'i').test(text)) {
        violations.push(`AT-U3: ${relative(updateDir, file)}에 금지된 식별자 "${identifier}"가 등장합니다`);
      }
    }
  }
  return failing(violations);
}

/** AT-U4가 허용하는 업데이트 트리거 입력의 전수 — architecture.md §3.6.1 AT-U4 원문:
 * "ⓐ 타이머 ⓑ 사용자 명령 ⓒ 코드 상수 authority에서 받은 서명 검증 통과 매니페스트
 * 셋뿐이다". 코드 상 리터럴 태그 이름은 이 슬라이스가 처음 정하는 것이라 합리적인
 * kebab/camel 표기를 골랐다 — W-N4-b가 실제 트리거 타입을 만들 때 이 세 값을
 * 그대로 재사용해야 한다(새 이름을 짓지 않는 것이 계약이다). */
export const ALLOWED_UPDATE_TRIGGER_KINDS = ['timer', 'user-command', 'verified-manifest'] as const;

/**
 * AT-U4 — 트리거 입력 전수 열거: `updateDir` 안에 "트리거"로 보이는 타입/유니온이
 * 있다면(`Trigger`가 이름에 들어간 파일 또는 식별자), 그 유니온의 문자열 리터럴 태그가
 * `ALLOWED_UPDATE_TRIGGER_KINDS`의 부분집합이어야 한다. 아직 어떤 트리거 타입도
 * 존재하지 않으면(W-N4-b 이전) 검사 대상이 없어 통과한다 — 이는 "무엇이든 허용"이
 * 아니라 "아직 그 타입이 없다"는 사실의 정직한 표현이다.
 */
export function checkTriggerInputEnumeration(updateDir: string): BoundaryCheckResult {
  const files = listTsFilesRecursive(updateDir).filter((f) => /trigger/i.test(f));
  if (files.length === 0) return skippedOk();

  const violations: string[] = [];
  const literalRe = /'([a-z][a-z0-9-]*)'/g;
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    // "Trigger" 식별자가 등장하는 타입 선언 주변부만 본다는 정밀도는 이 슬라이스
    // 범위를 넘는다 — 파일 전체에서 문자열 리터럴 태그 후보를 뽑아 허용 집합
        // 밖의 것이 있으면 위반으로 본다(과탐이 과소탐보다 안전한 방향).
    if (!/Trigger/.test(text)) continue;
    literalRe.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = literalRe.exec(text)) !== null) {
      const literal = m[1];
      if (literal && !(ALLOWED_UPDATE_TRIGGER_KINDS as readonly string[]).includes(literal)) {
        violations.push(`AT-U4: ${relative(updateDir, file)}의 트리거 유니온에 허용되지 않은 입력 "${literal}"이 있습니다`);
      }
    }
  }
  return failing(violations);
}

/** AT-U5가 찾는 authority 상수의 정식 이름 — U-3(architecture.md §3.6.2) "서버 authority는
 * 코드 상수" 요구의 식별자. W-N4-b가 실제 값을 채울 때 이 이름을 그대로 써야 한다
 * (이름을 새로 짓지 않는 것이 계약). */
export const UPDATE_AUTHORITY_IDENTIFIER = 'UPDATE_SERVER_AUTHORITY';

/**
 * AT-U5 — authority 단일 정의: `UPDATE_SERVER_AUTHORITY`의 **정의 지점**(`export const
 * UPDATE_SERVER_AUTHORITY =`)이 `root` 전체에서 정확히 0개 또는 1개여야 하고(0개 =
 * "아직 W-N4-b가 이 상수를 만들지 않았다", 1개 = "정상"), 2개 이상이면 위반이다.
 * 추가로 그 식별자에 대한 **재대입**(`UPDATE_SERVER_AUTHORITY =` 이되 `export const`가
 * 아닌 형태)이나 `process.env`/`argv`/설정/정책에서 끌어오는 패턴
 * (`process.env` 문자열이 이 식별자와 같은 파일·같은 줄 부근에 등장)이 있으면 위반이다
 * — U-3 원문: "개발용 오버라이드를 만들지 않는다 ... 개발 편의용 오버라이드가 곧
 * 공격 경로다".
 */
/** 블록·라인 주석을 제거한다(문자열 리터럴 내부의 `//`까지 정교하게 가리지는 않는다 —
 * 이 검사 대상은 `src/update/**`의 실제 코드이지 임의 3rd-party 소스가 아니므로 이
 * 단순화된 근사로 충분하다). AT-U5의 재대입·환경변수 유래 판정은 **실행되는 코드**를
 * 대상으로 해야 하므로, 이 상수를 설명하는 주석 문구(예: 이 파일 자신의 JSDoc이
 * "`UPDATE_SERVER_AUTHORITY =`"·"`argv`"를 나란히 언급하는 것) 때문에 이 검사 함수
 * 자신이 자기 자신을 오탐하는 것을 막는다. */
function stripComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
}

export function checkAuthoritySingleDefinition(root: string): BoundaryCheckResult {
  const files = listTsFilesRecursive(root);
  const DEFINITION_RE = new RegExp(`export\\s+const\\s+${UPDATE_AUTHORITY_IDENTIFIER}\\s*=`);
  const REASSIGN_RE = new RegExp(`(?<!export\\s+const\\s+)\\b${UPDATE_AUTHORITY_IDENTIFIER}\\b\\s*=(?!=)`);
  const ENV_DERIVED_RE = new RegExp(`${UPDATE_AUTHORITY_IDENTIFIER}[\\s\\S]{0,80}(process\\.env|argv|policy)`, 'i');

  let definitionCount = 0;
  const violations: string[] = [];

  for (const file of files) {
    const rawText = readFileSync(file, 'utf8');
    const codeText = stripComments(rawText);
    if (DEFINITION_RE.test(codeText)) definitionCount += 1;
    if (REASSIGN_RE.test(codeText) && !DEFINITION_RE.test(codeText)) {
      violations.push(`AT-U5: ${relative(root, file)}가 ${UPDATE_AUTHORITY_IDENTIFIER}를 재대입합니다`);
    }
    if (ENV_DERIVED_RE.test(codeText)) {
      violations.push(`AT-U5: ${relative(root, file)}에서 ${UPDATE_AUTHORITY_IDENTIFIER}가 환경변수/argv/정책 유래로 보입니다`);
    }
  }

  if (definitionCount > 1) {
    violations.push(`AT-U5: ${UPDATE_AUTHORITY_IDENTIFIER} 정의 지점이 ${definitionCount}개입니다(1개여야 합니다)`);
  }

  return failing(violations);
}
