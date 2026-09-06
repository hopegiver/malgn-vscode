// AT-S1 — architecture.md §3.6.1 ⑥ "집행 — 아키텍처 테스트 AT-S1: `sessionAttended`는
// `evaluateStop` 안에서만 읽힌다. 재수렴 루프·provider·UI 어디에도 이 신호를 읽는 지점이
// 없음을 고정한다 — `if (!attended) return;`을 루프 진입부에 쓰는 것이 정확히 금지되는
// 형태다."
//
// W4가 이미 한 번 제거한 "진입부 이른 반환" 패턴(§3.6.1 ⑥ 원문: "되돌리는 비용은 확정된
// 매트릭스 행 + 회귀 테스트 13건 + 이 절 정본이다")의 재발을 코드 트리 자체를 훑어
// 막는다 — `maskSinglePoint.test.ts`(§6.2 "단일 지점" 강제)와 같은 종류의 검증이다.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
// HERE = <repo>/src/core/reconciler — 두 단계 위가 <repo>/src다.
const SRC_ROOT = join(HERE, '..', '..');
const STOPGATE_FILE = join(SRC_ROOT, 'core', 'reconciler', 'stopGate.ts');

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFiles(full));
    } else if (entry.isFile() && (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx'))) {
      out.push(full);
    }
  }
  return out;
}

const ALL_SRC_FILES = listTsFiles(SRC_ROOT);
// 이 파일 자신 — 아래 "패턴 자기 검증" describe가 가짜 위반 문자열을 리터럴로 담고
// 있어(`.sessionAttended`·`if (!sessionAttended) return`), 그 문자열이 실제 위반 스캔에
// 잡히면 이 검사 파일 자신이 자기 자신을 오탐한다. 자기 검증용 리터럴은 이 파일에만
// 있어야 하므로(다른 파일에 있으면 그건 진짜 위반이다) 이 파일만 제외한다.
const SELF_FILE = fileURLToPath(import.meta.url);

function relPath(p: string): string {
  return relative(SRC_ROOT, p);
}

/** `signals.sessionAttended`처럼 프로퍼티로 값을 "읽는" 지점만 잡는다. 객체 리터럴 키로
 * `{ sessionAttended: true }`를 쓰는 것(StopSignals를 조립해 evaluateStop에 통째로
 * 넘기는 것)은 "읽기"가 아니다 — 그래서 이 정규식은 앞에 `.`이 붙은 프로퍼티 접근
 * 형태만 찾는다. */
const PROPERTY_READ_RE = /\.sessionAttended\b/;

/** stopGate.ts의 `export function evaluateStop(` 본문 범위를 중괄호 깊이로 추출한다 —
 * 문자열 검색(다음 구분 주석 등)에 기대면 함수 본문 리팩터링 시 조용히 깨진다. */
function extractEvaluateStopBody(sourceText: string): string {
  const marker = 'export function evaluateStop(';
  const startIdx = sourceText.indexOf(marker);
  if (startIdx === -1) {
    throw new Error('stopGate.ts에서 evaluateStop 정의를 찾을 수 없습니다 — AT-S1이 검사 대상을 잃었습니다');
  }
  const bodyStart = sourceText.indexOf('{', startIdx);
  if (bodyStart === -1) {
    throw new Error('evaluateStop의 함수 본문 시작 중괄호를 찾을 수 없습니다');
  }
  let depth = 0;
  for (let i = bodyStart; i < sourceText.length; i += 1) {
    const ch = sourceText[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return sourceText.slice(bodyStart, i + 1);
    }
  }
  throw new Error('evaluateStop 함수 본문의 닫는 중괄호를 찾지 못했습니다(중괄호 불균형)');
}

describe('AT-S1 — sessionAttended는 evaluateStop() 안에서만 읽힌다(architecture.md §3.6.1 ⑥)', () => {
  it('stopGate.ts 밖 어떤 소스 파일도 .sessionAttended 프로퍼티에 접근하지 않는다', () => {
    const offenders = ALL_SRC_FILES.filter((file) => {
      if (file === STOPGATE_FILE || file === SELF_FILE) return false;
      const text = readFileSync(file, 'utf8');
      return PROPERTY_READ_RE.test(text);
    });
    expect(offenders.map(relPath)).toEqual([]);
  });

  it('stopGate.ts 안에서도 .sessionAttended 읽기는 evaluateStop() 함수 본문 안에서만 등장한다', () => {
    const fullText = readFileSync(STOPGATE_FILE, 'utf8');
    const bodyText = extractEvaluateStopBody(fullText);

    const totalReads = (fullText.match(new RegExp(PROPERTY_READ_RE, 'g')) ?? []).length;
    const bodyReads = (bodyText.match(new RegExp(PROPERTY_READ_RE, 'g')) ?? []).length;

    expect(totalReads).toBeGreaterThan(0); // 검사 자체가 무력화(0건)되지 않았는지 자기 점검
    expect(bodyReads).toBe(totalReads); // 파일 전체의 읽기 = 함수 본문 안의 읽기(밖에는 0건)
  });

  it('금지 패턴 — "무인이면 즉시 반환"하는 이른 반환이 stopGate.ts 밖 어디에도 없다(재수렴 루프·provider·UI 진입부 포함)', () => {
    // W4가 제거한 진입부 이른 반환의 정확한 형태: `if (!attended) return;` 계열.
    // `attended`/`Attended`(sessionAttended, isAttended 등 파생 식별자 포함)를 부정 조건
    // 안에서 검사한 뒤 곧바로 return(또는 블록 반환)하는 패턴을 넓게 잡는다.
    // stopGate.ts 자신은 제외한다 — `evaluateStop()` 내부의 정당한 구현
    // (`if (!signals.sessionAttended) { reasons.push(...) }`)이 이 모양 자체와 같아서다.
    // "그 판정이 evaluateStop 안에서만 일어나는가"는 위 두 테스트가 이미 별도로 고정한다 —
    // 이 테스트는 "그 판정이 evaluateStop **밖**으로 새어 나가지 않는가"만 본다.
    const EARLY_RETURN_RE = /if\s*\(\s*!\s*[A-Za-z0-9_.]*[Aa]ttended[A-Za-z0-9_]*\s*\)\s*(\{[^}]*\}|return\b)/;
    const offenders = ALL_SRC_FILES.filter((file) => {
      if (file === SELF_FILE || file === STOPGATE_FILE) return false;
      const text = readFileSync(file, 'utf8');
      return EARLY_RETURN_RE.test(text);
    });
    expect(offenders.map(relPath)).toEqual([]);
  });
});

// --- 위반 주입 확인용 고정 사례(완료판정 #4의 재현 방법을 코드로 남긴다) ----------------
//
// 아래 두 케이스는 PROPERTY_READ_RE·EARLY_RETURN_RE가 실제로 반응하는지 그 자체를
// 증명한다 — "패턴이 있다"가 아니라 "패턴이 위반을 잡는다"를 보인다. 실제 소스 파일을
// 건드리지 않고 인메모리 문자열로만 검사해 이 파일 자체가 회귀 재현 도구를 겸한다.
describe('AT-S1 패턴 자기 검증 — 실제로 위반을 잡는지(가짜 소스 문자열로 확인)', () => {
  it('PROPERTY_READ_RE는 다른 파일의 .sessionAttended 접근을 실제로 잡는다', () => {
    const fakeOffendingSource = `
      export function reconcileLoop(signals: { sessionAttended: boolean }) {
        if (!signals.sessionAttended) return; // 금지된 이른 반환 재현
      }
    `;
    expect(PROPERTY_READ_RE.test(fakeOffendingSource)).toBe(true);
  });

  it('EARLY_RETURN_RE는 "if (!xAttended) return" 계열의 이른 반환을 실제로 잡는다', () => {
    const fakeOffendingSource = `
      function tick() {
        if (!sessionAttended) return;
        doWork();
      }
    `;
    expect(EARLY_RETURN_RE_FOR_TEST.test(fakeOffendingSource)).toBe(true);
  });

  it('정상 형태(StopSignals 객체 리터럴로 조립해 evaluateStop에 넘기는 것)는 두 패턴 모두 잡지 않는다', () => {
    const fakeCompliantSource = `
      const signals = { sessionAttended: computeAttended() };
      return evaluateStop('provider.apply', providerId, signals);
    `;
    expect(PROPERTY_READ_RE.test(fakeCompliantSource)).toBe(false);
    expect(EARLY_RETURN_RE_FOR_TEST.test(fakeCompliantSource)).toBe(false);
  });
});

// 위 describe가 쓰는 정규식은 본문의 것과 정확히 같은 리터럴이어야 "패턴이 검사 대상과
// 동일하다"는 주장이 성립한다 — 같은 소스 위치에서 재정의해 복붙 드리프트를 막는다.
const EARLY_RETURN_RE_FOR_TEST = /if\s*\(\s*!\s*[A-Za-z0-9_.]*[Aa]ttended[A-Za-z0-9_]*\s*\)\s*(\{[^}]*\}|return\b)/;
