// .githooks/pre-push가 호출하는 scripts/pre-push-sensitive-scan.mjs 검증 — 검사 ⑨와
// 같은 모듈·같은 패턴 파일을 쓰는지, 그리고 위반이 있으면 0이 아닌 코드를 내는지.

import { describe, expect, it } from 'vitest';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { main, scanRepo } from './pre-push-sensitive-scan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');

describe('scanRepo — 실제 저장소 추적 트리 전체', () => {
  it('현재 추적 트리에서 위반 0건이다', () => {
    expect(scanRepo(ROOT)).toEqual([]);
  });
});

describe('main — 위반 유무에 따른 종료 코드', () => {
  it('위반이 없으면 0을 반환한다', () => {
    expect(main(ROOT)).toBe(0);
  });
});
