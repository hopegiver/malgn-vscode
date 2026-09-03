# malgn-vscode — 개발 가이드 (W1 — 코어 + Provider 레지스트리)

`docs/architecture.md`(정본) · `docs/tech-stack.md`(스택 근거)와 짝을 이룬다. 이 문서는 이
슬라이스(W1)를 기준으로 최소한만 담는다 — 온보딩 UI·정책 로더 등은 이후 작업 단위가
추가되며 갱신된다.

## 환경

- Node.js: VS Code 확장 호스트 내장 Node를 그대로 쓴다(별도 지정 없음). 로컬 개발은
  현재 검증된 버전(Node 22)을 쓰면 된다.
- 패키지 매니저: **pnpm 전용**. `npm`/`yarn` 사용 금지(`package-lock.json`·`yarn.lock`을
  생성하지 않는다).

## 설치

```bash
pnpm install --frozen-lockfile
```

## 빌드

```bash
pnpm run build        # check-types(tsc --noEmit) + compile(esbuild) 순서로 실행
pnpm run check-types   # 타입 검사만 (tsc --noEmit) — apply()의 ConsentToken 타입 계약
                        # 위반 여부도 여기서 잡힌다 (src/providers/__typetests__/)
pnpm run compile        # dist/extension.cjs 생성 (esbuild, CommonJS, external: vscode)
pnpm run watch           # esbuild watch 모드
```

## 테스트

```bash
pnpm test          # vitest run — 단위 테스트 (VS Code 불필요)
pnpm run test:watch
```

VS Code API가 필요한 통합 테스트(`@vscode/test-cli`/`@vscode/test-electron`)는 아직
설정하지 않았다 — activate()에 검증할 만한 로직이 아직 없어서다(W2~W5 이후 재평가).

## 디렉터리 구조 (§1.1, 이 슬라이스에서 실재하는 부분만)

```
src/
├─ extension.ts                     진입점. activate/deactivate만
├─ core/
│  ├─ reconciler/
│  │  ├─ engine.ts                  detect() 오케스트레이션 — 격리 + 5초 타임아웃 (§2.2 ⑤)
│  │  └─ diffHash.ts                Plan.diffHash 계산 (PR-3)
│  └─ consent/
│     └─ types.ts                   ConsentToken 타입 (게이트 로직 본체는 W3)
└─ providers/
   ├─ types.ts                      Provider 균일 인터페이스 (§1.2)
   ├─ registry.ts                   등록/조회 + dependsOn 위상 정렬
   └─ __typetests__/                apply()의 ConsentToken 타입 계약 컴파일 실패 증거
```

`core/policy`·`core/journal`·`core/diagnostics`와 `providers/install|agent|otel|github|
cloudflare|mcp`는 아직 없다 — 각각 W2·W5·W7~W10에서 추가된다. 빈 디렉터리를 미리 만들지
않았다(가짜로 채워진 스텁을 남기지 않기 위해서).

## 핵심 안전 성질 — 다음 작업 단위가 이어받는 지점

- `Provider.apply(plan, consent: ConsentToken, ctx)` — **consent 없이는 컴파일되지 않는다.**
  증거: `src/providers/__typetests__/provider-apply-consent-contract.tscheck.ts`
  (`pnpm run check-types`가 통과하는 것 자체가 증거).
- `gate.assertValid(plan, consent)`(런타임 재검증: providerId·diffHash·extensionVersion·
  만료·nonce)는 **아직 없다** — W3가 `core/consent/`에 구현한다.
- `detectAll()`은 `Promise.allSettled` + provider별 5초 타임아웃 + try/catch로 격리돼
  있다(PR-8). `apply()` 오케스트레이션(engine을 통한 이중 assertValid 검사, N-5
  `reconcile.lock`)은 W3(동의 게이트)·W5(저널)가 갖춰진 뒤 engine.ts에 추가된다.
- `ProviderRegistry`/`topologicalOrder()`는 등록·의존순서만 담당한다. 실제 provider
  인스턴스는 W7~W10에서 `register()`된다.

## 환경변수

이 슬라이스에는 없다. 정책 파일 3순위 로드(체크아웃 → 설치본 → 번들 내장, §3.7.3)와
관련 경로 설정은 W2에서 도입된다.
