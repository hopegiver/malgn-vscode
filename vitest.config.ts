import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    // 타입 계약 증거 파일(*.tscheck.ts)은 tsc --noEmit 전용이며 실행 대상이 아니다.
    // (파일명에 test/spec을 쓰지 않아 vitest 기본 include 패턴에도 잡히지 않지만,
    //  실수로 잡히는 것을 막기 위해 명시적으로도 제외해 둔다.)
    // src/compat-check/**는 별도 게이트(`pnpm compat:check`, vitest.compat.config.ts)로
    // 실행한다 — `pnpm test`(이 설정)와 CI 파이프라인 단계를 분리하기 위해서다
    // (tech-stack.md §5.4: "… → vitest → compat:check → …").
    exclude: ['node_modules/**', 'dist/**', '**/__typetests__/**', 'src/compat-check/**'],
  },
});
