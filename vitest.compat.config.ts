// `pnpm compat:check` 전용 vitest 설정 — docs/policy-contract.md §6의 차단성 검사
// ①~⑧을 실행하는 `src/compat-check/**`만 대상으로 한다. 메인 `vitest.config.ts`는
// 이 디렉터리를 exclude해 `pnpm test`(단위 테스트 단계)와 이 게이트(계약 강제 단계)가
// CI 파이프라인에서 항상 분리 실행되게 한다(tech-stack.md §5.4: "… → vitest →
// compat:check → …" — 두 단계가 서로 다른 실패 신호를 낸다).
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/compat-check/**/*.test.ts'],
    exclude: ['node_modules/**', 'dist/**'],
  },
});
