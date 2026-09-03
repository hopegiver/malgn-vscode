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

`check-types`/`test`/`compat:check`/`build`는 모두 실행 전에 `gen:site`를 먼저 돌린다
(pnpm의 `pre<script>` 훅 — 아래 "민감값 분리" 참고). 수동으로 먼저 돌리고 싶으면
`pnpm run gen:site`.

## 민감값 분리(compat/ 공개면 · site/ 사이트면) — docs/policy-contract.md §8.4

`compat/compatibility.json`의 사내 인프라 값(수집기 authority·keychain 항목명 등)은
`{"$site":"..."}` 구멍이다. 실값은 빌드 시 `pnpm gen:site`가 채운다:

- **로컬에 `site/site.json`이 있으면** 그 값(영구 비추적 — `.gitignore`)으로 채우고
  `src/generated/siteConstants.ts`의 `siteProfile`을 `'site'`로 emit한다.
- **없으면** `compat/site.example.json`(예약 네임스페이스 값만, 추적됨)으로 채우고
  `siteProfile`은 `'example'`이 된다 — CI·신선 클론(`docs/` 없는 상태 포함)은 항상 이
  경로다.
- 두 파일이 모두 없거나 구멍이 하나라도 안 채워지면 `gen:site`가 0이 아닌 종료 코드로
  실패한다(fail-closed — 빈 화이트리스트로 조용히 폴백하지 않는다).
- `siteProfile !== 'site'`인 산출물은 어떤 provider도 `apply()`할 수 없고(
  `src/core/policy/siteProfileGuard.ts`) 패키징도 실패한다(
  `scripts/assert-site-profile-for-packaging.mjs`, `pnpm run prepackage`).

실제 사내 값을 쓰려면 `site/site.json`을 로컬에 만든다(형태는 `compat/site.example.json`
참고 — git에 절대 커밋하지 않는다).

## 계약 검증(`pnpm compat:check`)

```bash
pnpm compat:check   # docs/policy-contract.md §6/§8 검사 ①~⑪
```

검사 ①B·⑤C·⑦은 `docs/`가 로컬에 있을 때만 돈다(문서 실지 스캔 — 모드는 `docs/` 실재로
결정되며 플래그가 아니다). `docs/`가 없으면(CI 등) 그 셋은 건너뛰고 나머지는 그대로
차단성으로 강제된다 — 정본이 `docs/`가 아니라 `compat/`에 있기 때문이다(§8.2 정본 반전).

`pnpm run contract:snapshot`은 `docs/`가 있을 때만(로컬) `compat/contract-snapshot.json`
(이름·형태·해시만 — 값 0)을 재생성한다. 계약(`compat/*.json`의 `allowed*` 키)을 바꾸면
이 명령을 다시 실행해야 검사 ⑤A가 통과한다.

## 민감값 유출 방지 — pre-push 훅(선택, 기본 비활성)

```bash
git config core.hooksPath .githooks   # 사람이 직접 활성화한다(사람의 몫)
```

활성화하면 push 직전 `.githooks/pre-push`가 추적 트리 전체를 검사 ⑨와 같은 패턴 파일
(`compat/sensitive-classes.json`)·같은 로직(`scripts/lib/sensitiveScan.mjs`)으로 스캔한다.

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
