// 완료 판정 #3 — "활성화 시퀀스에서 apply()가 호출되지 않음을 테스트로 고정(호출그래프
// 검사 등)". architecture.md §2.2 "여기서 apply()는 절대 호출되지 않는다 — 불변량이다."
//
// 두 계층으로 확인한다:
//  ① 소스 텍스트 검사 — 활성화 시퀀스를 조립하는 파일(`src/host/activation/**`)에
//     `.apply(` 호출 패턴이 등장하지 않는다. `runActivationSequence`는 `Provider.plan`만
//     호출하고 `Provider.apply`·`gate.assertValid`를 import조차 하지 않는다(타입
//     수준에서 호출 불가) — 이 검사는 그 사실을 텍스트로도 이중 확인한다.
//  ② import 그래프 검사 — `src/host/activation/**`의 어떤 파일도 `core/consent/gate.js`
//     (assertValid의 정의 지점)를 import하지 않는다. import하지 않으면 애초에 그
//     함수를 부를 수 없다 — "동의 없는 파괴적 변경 금지"를 이 계층에서도 구조적으로
//     보장한다.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC_ROOT = join(HERE, '..');
const ACTIVATION_DIR = join(SRC_ROOT, 'host', 'activation');

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listTsFiles(full));
    } else if (entry.isFile() && entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) {
      out.push(full);
    }
  }
  return out;
}

describe('활성화 시퀀스는 apply()를 호출하지 않는다(architecture.md §2.2 불변량)', () => {
  const files = listTsFiles(ACTIVATION_DIR);

  it('src/host/activation/** 소스 파일 목록이 비어 있지 않다(검사 대상 존재 자기 점검)', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('src/host/activation/** 어떤 파일에도 `.apply(` 호출 패턴이 없다', () => {
    const offenders = files.filter((file) => /\.apply\(/.test(readFileSync(file, 'utf8')));
    expect(offenders.map((f) => relative(SRC_ROOT, f))).toEqual([]);
  });

  it('src/host/activation/** 어떤 파일도 core/consent/gate.js(assertValid 정의 지점)를 import하지 않는다', () => {
    const offenders = files.filter((file) => /from\s+['"].*consent\/gate\.js['"]/.test(readFileSync(file, 'utf8')));
    expect(offenders.map((f) => relative(SRC_ROOT, f))).toEqual([]);
  });

  it('[자기 검증] `.apply(` 패턴 자체는 실제로 탐지 가능하다(가짜 문자열로 정규식 확인)', () => {
    const fakeOffendingSource = "await provider.apply(plan, consent, ctx);";
    expect(/\.apply\(/.test(fakeOffendingSource)).toBe(true);
  });
});
