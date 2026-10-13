# R7 온보딩·설치 순서·효율 가이드 리뷰 보고서

리뷰 대상: 커밋 범위 `34ef952..1a6ab07` (b637289 P2-2 / b920d62 P2-1 / 1a6ab07 P2-7), target_id `r7-onboarding-install-order-efficiency-guide`
리뷰 페르소나 패널: `persona-first-run-newcomer.md`, `persona-screen-state-honesty.md`, `persona-claimed-vs-verified.md`, `persona-zero-base-redesigner.md`(발산형) — 모두 재사용, 신규 0
리스크 범주: UI 표시·클라이언트 배치 순서·계산값 기반 문구. 인증·capabilities·설치 실행 경로·새 수집 경로 변경 없음(diff로 확인)
등급/모드: Standard, 최초 리뷰(직전 리뷰 없음)
리뷰 일자: 2026-10-13
종합 판정: **🟡 Amber** (Critical 0 / Major 2 / Minor 3 / Nit 4 / Rethink 3)

## 요약 (2분 규칙)
세 항목 모두 요구 조건을 코드상으로는 지켰다. 동의 모델은 바뀌지 않았고, 이미 설치된 도구의 업데이트도 의존 판정에 막히지 않는다. 새 커맨드나 새 수집 경로도 없고, 인과를 단정하는 문구나 비용 계산도 없다. 다만 두 가지 문제가 있다. (1) P2-2에서 Claude를 `required=true`로 바꾼 결과 P2-1 체크리스트의 1단계("필수 도구")와 2단계("Claude Code 설치")가 Claude 미설치 사용자에게 **같은 항목을 중복으로 미완료 처리**한다. 하네스 픽스처가 P2-2 이전 값(claude required=false)이라 이 상태를 놓쳤다. (2) P2-2의 "Node 먼저 → 의존 도구 건너뜀" 경로는 **실제 백엔드가 낼 수 있는 상태로는 신규 머신에서 거의 도달하지 않는다**. mac은 Node가 manual이고, Windows는 Node winget 미리보기가 항상 불신뢰이며, npm이 없으면 Claude·pnpm·Wrangler가 처음부터 manual이라 배치에 들어오지 않는다. 해가 되는 코드는 아니지만 "일괄 설치가 의존 순서로 한 번에 된다"고 보고하면 안 된다.

## 페르소나 재사용 판정 (산출물 게이트)
INDEX.md의 역할개념 열을 먼저 대조했다. 4명 모두 기존 역할개념과 일치해 재사용했고, 신규 파일은 만들지 않았다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-first-run-newcomer.md | 재사용 | P2-1 온보딩은 "신입이 첫 실행에서 다음 행동을 찾는가"의 과녁 그 자체 |
| persona-screen-state-honesty.md | 재사용 | 로딩/에러 미단정(P2-1), 표본 부족 ≠ 문제 없음(P2-7) |
| persona-claimed-vs-verified.md | 재사용 | 하네스 스텁이 실제 도달 가능한 상태인지, 주석·커밋 주장과 도달 경로가 맞는지 |
| persona-zero-base-redesigner.md (발산) | 재사용 | 발산형 필수, 역할개념 유효 |

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-001 | 🟠 Major | 신입·정직 | `src/onboarding.ts:84`, `src-tauri/src/dev_tools/mod.rs:127`(Claude `required: true`), `scripts/ui-harness/flows/home.mjs:12-20`, `docs/screenshots/r7-2026-10-13/p2-1-claude-missing.png` | 코드 Read로 판정 경로를 추적하고 스크린샷을 열람 | P2-2(b637289)가 Claude를 필수로 바꿨는데, 뒤에 들어온 P2-1(b920d62)의 `requiredToolsStep`은 `required && !installed`로 필터해서 **Claude가 1단계에도 포함된다**. 실제 백엔드에서 Node·Git은 있고 Claude만 없는 신규 사용자는 1단계 "Claude Code 설치가 필요합니다"(todo·현재 단계)와 2단계 "claude CLI가 설치되어 있지 않습니다"(todo)를 동시에 보고, 진행도는 **0/3**이 된다. 1단계는 2단계가 끝나야만 끝나므로 단계 구분이 무의미하다. 증거로 낸 스크린샷은 "1/3 완료 · 필수 도구가 모두 설치되어 있습니다"인데, home.mjs 픽스처가 "node·git만 true"라는 P2-2 이전 사실로 claude를 `required=false`로 실었기 때문이다. 실제 백엔드는 이 상태를 내지 않는다. | `requiredToolsStep`에서 별도 단계가 있는 Claude를 제외한다(`t.required && !t.installed && t.id !== 'claude'` — `claudeInstallState`가 이미 id `'claude'`에 의존하므로 새 하드코딩 표면이 아니다). home.mjs 픽스처는 `required=true`로 고쳐 실제 계약과 맞춘다. 수정 비용은 작다(2줄 + 픽스처 1줄, 근거: 판정 함수가 순수 함수 하나에 모여 있음). |
| RV-002 | 🟠 Major | 주장≠검증 | `src-tauri/src/dev_tools/install_resolver.rs:159-163`(Claude=npm 후보 하나), `:195-202`(Node=winget 후보 하나, mac은 구조적으로 Manual — 같은 파일 125-128 주석), `src-tauri/src/dev_tools/query.rs:42-46`(미설치 actionKind = resolve_install_plan), `query.rs:115`(winget 미리보기 = 불신뢰), `src/views/devTools.ts:341`(배치 대상 = `actionKind==='run'`만), `scripts/ui-harness/flows/devToolsOrder.mjs:27`(모든 스텁 도구 `actionKind:'run'`) | 백엔드 판정 경로 Read, 하네스 픽스처 Read | P2-2의 핵심 경로(Node 미설치 → 배치에서 Node 먼저 설치 → 확인되면 Claude 설치, 실패하면 건너뜀)는 실제로 도달하지 않는다. **mac**: Node는 항상 manual이라 배치에 오지 않는다. **Windows**: Node는 winget이라 `previewReliable=false` → 배치에서 항상 "개별 확인 대기"로 빠진다(398행). **양쪽 공통**: npm이 해석되지 않으면(=Node 없음) Claude·pnpm·Wrangler는 백엔드가 `manual`로 내려 보내 `runnable`에 들어가지 않는다. 따라서 하네스의 `node-installed-in-batch`·`node-failed-skip` 장면(스텁이 Node 미설치인데 Claude를 `run`으로 실음)은 백엔드가 만들 수 없는 상태를 검증한 것이다. 남는 유일한 도달 경로는 "Node 경로는 있는데 `--version`이 10초 안에 응답하지 않음(installed=false) + npm 해석됨"뿐인데, 이 경우에는 Node가 실제로 있으므로 Claude 건너뜀이 오히려 오판이 된다. 회귀나 해는 없다(방어 코드로서는 안전하다). 그러나 신규 머신 사용자는 여전히 Node 설치(개별/수동) → 재조회 → "전체 업데이트" 재실행의 두 단계를 거쳐야 한다. | 코드 수정은 필수가 아니다. **PM은 완료 보고·WBS·릴리스 노트에 "신규 머신 일괄 설치가 한 번에 된다"고 쓰지 않는다.** "배치 내 순서·건너뜀 가드 추가(현 백엔드에서는 방어 경로)"로 정정한다. 실효를 원하면 후속 항목으로 RT-1(선행 확인 뒤 재조회·재평가)을 등록한다. 하네스에는 "Node 미설치 → Claude가 manual로 내려와 배치에서 빠지는" 실제 모양의 시나리오를 추가한다. |
| RV-003 | 🟡 Minor | 정직 | `src/onboarding.ts:68`, `src/views/home.ts:520`, `src-tauri/src/dev_tools/process.rs:483-505` | Read | `claudeInstallState`는 `installed===false`만 보고 'missing'으로 확정한다. 그런데 백엔드는 경로가 있어도 `claude --version`이 10초 타임아웃이거나 출력이 없으면 `installed=false`(path는 non-null)를 낸다. 이때 홈 위젯은 로그인 버튼을 지우고 "설치되어 있지 않아 로그인할 수 없습니다"라고 단정한다. 요구사항("미설치가 확정일 때만")보다 넓다. 첫 실행이 느린 Windows(미검증 플랫폼)에서 생기기 쉬운 경우다. | `!claude.installed && claude.path == null`일 때만 'missing', 경로가 있으면 'unknown'(기존 로그인 동작 유지). 1줄 수정이다. |
| RV-004 | 🟡 Minor | 신입 | `src/efficiencyGuide.ts:139` vs `:162`, `docs/screenshots/r7-2026-10-13/p2-7-triggered.png` | Read + 스크린샷 열람 | 두 카드가 동시에 뜨면(스크린샷 실례) 조언이 서로 엇갈린다. 캐시 카드는 "같은 세션에서 이어가고… /compact·/clear를 자주 쓰면 캐시 재사용이 줄어들 수 있다"고 하고, 쏠림 카드는 "작업 단위로 세션을 나누거나 중간에 /compact로 정리"하라고 한다. 둘 다 표현을 누그러뜨렸지만, 한 화면에서 정반대 행동을 권하면 가이드 전체의 신뢰가 떨어진다. | 쏠림 카드의 행동을 "그 날짜의 세션별 사용량을 먼저 확인" 하나로 줄인다. 아니면 두 카드가 함께 뜰 때 "/compact는 캐시 재사용과 맞바꾸는 선택"이라는 트레이드오프 문장 하나로 합친다. |
| RV-005 | 🟡 Minor | 정직·신입 | `src/views/devTools.ts:184`, `:518` | Read | 건너뜀 패널은 해당 도구 자신이 설치되거나 다음 배치가 시작될 때만 사라진다. 사용자가 Node를 개별로 설치해도 Claude 항목에는 "선행 도구(Node.js) 설치가 확인되지 않아 건너뜀"이 그대로 남는다. 지금은 거짓이 된 사유다. | 렌더 시 `installDependsOn`이 현재 `state.devTools.items`에서 모두 installed면 패널을 숨긴다. 또는 문구를 "지난 일괄 실행에서 건너뜀"처럼 과거형으로 바꾼다. |
| RV-006 | ⚪ Nit | 주장≠검증 | `src/efficiencyGuide.ts:24-29` | `/Users/hopegiver/.claude/plugins/cache/malgnsoft-plugins/malgn-agent/1.8.52/bin/analyze-usage.mjs:762`(`cacheHitRate < 0.5`) 실물 대조 | 주석의 근거("CLI 진단과 같은 값")는 실재한다. 다만 앱은 반올림한 정수로 비교하므로 49.5~49.99%는 CLI만 경고하고 앱은 경고하지 않는다. 주석이 이 차이를 이미 의도한 것으로 밝혀 두었으므로 정직성 문제는 아니다. | "같은 기준값(경계 0.5%p는 정수 반올림 차이)"으로 한 단어만 보강한다. |
| RV-007 | ⚪ Nit | 신입 | `src/efficiencyGuide.ts:114`, `:185` | Read | 대형 모델 판정은 이름에 `opus`가 들어가는지만 본다. 오분류 방향은 false positive가 아니라 false negative다(이름에 opus가 없는 대형 모델은 잡지 못함). Opus를 기본 모델로 쓰는 조직에서는 이 카드가 상시로 뜨는 잡음이 될 수 있다. | 1차 범위로는 수용한다. 상시 노출 여부는 RT-3에서 다룬다. |
| RV-008 | ⚪ Nit | 시각 | `src/styles.css`(P2-1 블록 `.onboard-detail { margin-top: 2px }`) | Read | 토큰을 쓰지 않은 하드코딩 값이 1건 있다(나머지는 모두 정의된 Terminus 토큰을 쓰며, 토큰 12종 정의를 grep으로 확인). | `--space-0-5` 등 토큰이 있으면 그것으로 바꾼다. |
| RV-009 | ⚪ Nit | 신입 | `docs/screenshots/r7-2026-10-13/p2-2-node-failed-skip.png` | 스크린샷 열람 | 배치 종료 시 토스트 4개가 겹쳐 본문 결과 패널을 가린다. 기존 토스트 동작이라 이번 변경이 원인은 아니고, 건너뜀 요약 토스트가 1개 늘었을 뿐이다. | 범위 밖. 참고만. |

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 정직 | 이미 설치된 도구의 업데이트가 의존 판정으로 막힌다 | 기각 | `devTools.ts:375` 게이트가 `!tool.installed`에만 걸린다. `p2-2-node-failed-skip.png`에서도 Node 실패 시 gh·git 업데이트는 진행됐다. |
| 주장≠검증 | Wrangler를 Node 의존에 넣은 판단은 과하다 | 기각 | 후보가 pnpm→npm이고(install_resolver.rs:168-179) 둘 다 Node 런타임이 필요하다. 정본 일치는 테스트 `tools_installed_via_npm_or_pnpm_declare_node_as_install_dependency`가 고정한다(실행 통과 확인). |
| 시각 | "설치 필요" 배지에 `badge-archived` 클래스를 오용했다 | 기각 | `devTools.ts:495` 주석상 "기존 두 배지 클래스만 재사용"이 의도된 규약이다. 홈의 "로그인 필요"(home.ts:537)도 같은 패턴이다. |
| 정직 | 동의 모델이 바뀌었다(설치 요약 확인 목록에 건너뛸 도구가 포함됨) | 기각 | 확인 대화상자와 previewReliable/다중 영향 제외 분기는 diff에서 변경이 없다. 건너뛰는 것은 동의 범위를 줄이는 쪽이라 동의 위반이 아니다. |
| 정직 | `projectAgentProgress/running` 하네스 실패(bugs=2)가 이번 변경의 회귀다 | 기각(범위 밖 기존 결함) | 직접 재실행해 bugs=2를 확인했다(사이드바 meta "진행/보관" 구분 누락, `src/sidebar.ts:wsRow`). diff는 `src/sidebar.ts`와 해당 flow를 건드리지 않는다(`git diff --stat` 확인). 3→2 감소는 `fixtures.mjs`에 `hub_auth_status` 스텁을 추가해 UNSTUBBED 오탐이 사라진 결과로 보인다(추정). |

## 페르소나별 관점

### first-run-newcomer — 🟠
- Claude 미설치 신규 사용자에게 체크리스트가 같은 일을 두 줄로 말하고 0/3으로 시작한다(RV-001). 이 기능의 1순위 대상 장면이다.
- 미설치가 확정됐을 때 홈 위젯이 로그인 버튼 대신 "Claude Code 설치하기"로 바뀌는 분기는 막다른 골목을 없앤 좋은 변화다.
- 효율 가이드는 근거 수치와 "기준 N"이 같은 줄에 있어 읽힌다. 다만 카드 사이 조언이 엇갈린다(RV-004).

### screen-state-honesty — 🟡
- 로딩/에러를 'loading'/'error'로 분리하고, 확정 미완료가 없으면 얇은 한 줄 안내로 낮추는 설계가 요구를 정확히 지킨다(`p2-1-loading.png` 열람: "확인 중…" 한 줄, 위젯도 "확인 중…").
- `insufficient`와 `clear`를 문구로 구분했다("문제가 없다는 뜻이 아니라…").
- 'missing' 확정 조건이 path 존재를 무시한다(RV-003). 건너뜀 사유가 stale로 남는다(RV-005).

### claimed-vs-verified — 🟠
- 하네스 스텁 두 곳이 실제 백엔드 계약과 어긋난다. home.mjs의 claude required=false(RV-001), devToolsOrder.mjs의 "Node 미설치인데 Claude run"(RV-002). 하네스는 TS/DOM 층만 본다는 한계가 이번에는 **픽스처 값 자체**를 통해 결론을 바꿨다.
- 임계값 50% 근거 주석은 플러그인 `analyze-usage.mjs:762`로 실물 확인했다. 나머지 임계값은 "경험적 보수치, 통계적 유의성 아님"이라고 스스로 밝혀 정직하다.
- 문구 경계 테스트("때문" 없음, `$` 없음)가 하네스에 자동화돼 있다(33/33 PASS 재실행).

### zero-base-redesigner (발산) — 🔵 아래 Rethink 참조

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-1 | 배치 시작 시점의 정적 `targets`를 의존 순서로 정렬 | **선행 확인 후 재평가 배치**: 선행 도구가 확인되면 `loadDevTools()`로 재조회하고, 그 결과 새로 `run`이 된 의존 도구를 이어서 실행한다. 동의는 첫 요약 대화상자에 "Node.js 설치 후 진행: Claude Code, pnpm"처럼 조건부로 미리 받는다. | RV-002의 "도달하지 않는 경로"를 실제 신규 머신 1회 셋업으로 바꾼다. 의존 정본(`installDependsOn`)은 그대로 재사용한다. | 중간(미확인 추정). 동의 문구 변경이 동의 모델에 닿으므로 Standard 이상 재리뷰가 필요하다. Windows Node는 여전히 개별 확인이 필요하다. |
| RT-2 | 홈 체크리스트는 개발 도구 화면으로 보내는 안내판이고, 실제 설치는 "전체 업데이트" 버튼(이름은 업데이트)이 겸한다 | 체크리스트 단계를 **실행 진입점**으로 만든다. 각 단계 버튼이 그 도구의 미리보기(기존 preview/plan_id)를 바로 열고, "전체 업데이트"는 이름 그대로 업데이트 전용으로 되돌린다. | 신규 사용자의 목표("셋업 끝내기")와 화면 동선이 일치한다. "업데이트" 버튼이 설치를 겸하는 의미 혼선(M1 이후 남은 것)이 사라진다. | 중간. 미리보기 패널 컴포넌트를 홈에서 재사용해야 한다. |
| RT-3 | 고정 임계값(50/30/70%)으로 "점검 권장" | 같은 30일 데이터 안에서 **본인 기준선 대비 변화**(최근 7일 vs 앞 23일 비중)로 바꾸고, 고정 임계값은 첫 사용자용 보조로만 둔다 | Opus가 기본인 조직처럼 구조적으로 늘 넘는 사용자에게 상시 경고 잡음이 생기지 않는다. 새 수집 경로 없이 DailyUsage만으로 가능하다(모델 축은 요약 API에 일별 분해가 없어 불가 — 확인 필요). | 낮음~중간(미확인 추정). |

## 트레이드오프 (페르소나 간 충돌)
- **RV-002의 처리**: 주장≠검증 관점은 "도달하지 않는 코드는 지우거나 실효화하라"고 하고, 정직 관점은 "Node 버전 조회 타임아웃 같은 예외 경로에서 방어 가드로 남겨 두는 편이 안전하다"고 본다. → 권고: 코드는 유지하고 **보고 문구만 정정**한다. 실효화는 RT-1로 별도 결정한다. 단, 타임아웃 예외 경로에서는 건너뜀이 오판이 된다는 점을 RT-1 설계에 반영한다.

## 잘 된 점
- 의존 관계의 정본을 백엔드 `DevTool.install_depends_on` 하나로 두고 Rust 테스트 3종(비순환·Node 선행·러너 일치)으로 고정했다. 프론트에는 id 목록이 없다. 직접 실행해 모두 통과를 확인했다.
- `devToolsOrder.ts`·`onboarding.ts`·`efficiencyGuide.ts`를 DOM 없는 순수 함수로 분리해 하네스가 그대로 import해 경계값을 검증한다.
- 업데이트에는 의존 판정을 걸지 않는다는 회귀 회피 판단과 그 이유(네이티브 설치본 Claude)를 주석에 남겼다.
- 효율 가이드에 근거 값·기준값·판단 보류 사유를 매 카드에 표시했고, "원인을 판정하지 않으며 비용은 쓰지 않음" 고지를 두었다. 임계값을 한 곳에 모았다.
- 데스크톱 네이티브 원칙 준수: `@media`나 반응형 패턴이 추가되지 않았고, 신규 CSS는 정의된 Terminus 토큰만 쓴다(RV-008 1건 제외).

## 평가기준 충족 현황

| 요구 | 충족 | 비고 |
|---|---|---|
| P2-1 기존 state 실데이터만, 새 커맨드·수집 없음 | ✅ | `state.devTools`/`state.claudeAuth`만 읽는다 |
| P2-1 로딩/에러 미단정, 전부 완료 시 숨김 | ✅ | |
| P2-1 단계 판정 정확성 | ❌ | RV-001(Claude 중복) |
| P2-1 claude 미설치 "확정"일 때만 설치 안내, 로그인 진행 중 분기 | ⚠️ | `login.active` 분기 ✅, 확정 조건이 넓다(RV-003) |
| P2-2 Node 먼저·건너뜀+사유·필수 표기·정본 단일화 | ✅(코드) / ⚠️(실효) | RV-002 |
| P2-2 동의 모델 불변 | ✅ | |
| P2-7 계산 수치만, 근거·기준 표시, 인과·비용 없음, 표본 부족 보류 | ✅ | 하네스 33/33 재확인 |
| 데스크톱 UI·Terminus 토큰 | ✅ | |

## 검증 수행 내역 (정직 보고)
- `git diff 34ef952..1a6ab07` 전체를 Read했다.
- `pnpm exec tsc --noEmit` 통과.
- `cargo test --lib`로 신규·변경 테스트 4종(`install_dependencies_are_well_formed_and_acyclic`, `node_precedes_the_tools_installed_through_it`, `only_node_git_and_claude_are_marked_required`, `tools_installed_via_npm_or_pnpm_declare_node_as_install_dependency`)이 통과했다. 전체 Rust 스위트는 실행하지 않았다.
- 하네스 재실행: home(18 시나리오, bugs=0), devToolsOrder(8, bugs=0), usageGuide(6, 경계 33/33), projectAgentProgress(7, bugs=2 — 범위 밖). 포트 1420에는 **이미 떠 있던 vite(PID 3467, 이 저장소)**를 그대로 사용했다. 내가 띄운 서버가 아니므로 종료하지 않았다.
- 스크린샷 열람: p2-1-claude-missing, p2-1-loading, p2-2-node-failed-skip, p2-7-triggered(4/10장). 나머지 6장은 열람을 생략했다(같은 흐름의 다른 분기라 코드로 판정했다).
- **미확인**: 실제 Tauri 앱 실기동, 실제 mac/Windows 백엔드 응답(RV-002의 도달 불가 판단은 코드 경로 추적에 근거한 것이며 실기로 관측하지 않았다), Windows 실기.
- 소스 수정·git 커밋·push는 하지 않았다. 산출물은 이 보고서와 페르소나 4개 파일의 적용 이력 append, INDEX.md 갱신뿐이다.

## PM에게 권고
1. **RV-001은 커밋 전에 수정하기를 권한다.** 2줄 + 픽스처 1줄 수정에 증분 재검토(축소)면 충분하다. 수정 후 home 흐름 스크린샷을 다시 찍는다.
2. **RV-002는 보고 문구를 정정한다.** WBS P2-2를 done으로 둘 때 "신규 머신 일괄 설치 실효는 RT-1 후속"을 명시한다. 하네스에 실제 모양의 시나리오 추가를 권장한다.
3. RV-003~005(Minor)는 백로그로 보낸다. RV-003은 1줄이라 RV-001과 함께 처리해도 좋다.
4. RT-1~3은 차기 방향 결정 대상이다(출시를 막지 않음).
