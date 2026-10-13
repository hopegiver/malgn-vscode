# R7 온보딩·설치 순서·효율 가이드 재검토 보고서 (2차, 축소)

리뷰 대상: 커밋 범위 `34ef952..11d0830` (반영 커밋 `11d0830`, 직전 범위 `34ef952..1a6ab07` 위에 얹힘), target_id `r7-onboarding-install-order-efficiency-guide`
직전 리뷰: `docs/reviewer/review-r7-onboarding-install-order-efficiency-guide-2026-10-13.md` (Amber, Major 2 / Minor 3 / Nit 4 / Rethink 3)
리스크 범주: 데스크톱 앱 UI 표시·클라이언트 설치 배치 순서. 접근 경계·인증·capabilities 변경 없음
등급/모드: Standard, 2차 재검토, **축소(C) 모드**
리뷰 일자: 2026-10-13
종합 판정: **Green** (미해소 Critical 0 / Major 0, 신규 Critical 0 / Major 0 / Minor 0 / Nit 2 / Rethink 1)

## 요약 (2분 규칙)
직전 리뷰의 Major 2건과 Minor 3건이 모두 해소됐다. RV-001은 1단계에서 Claude를 제외하고 픽스처를 실제 백엔드 모양(`required=true`, `installDependsOn=['node']`)으로 고쳤다. 그 결과 Claude만 없는 사용자는 이제 "1/3 완료, 2단계가 현재 단계"로 보인다. 하네스 단언과 스크린샷 양쪽에서 이를 확인했다. RV-002의 "후속 이월" 처리는 수용 가능하다. 직전 리뷰가 권고한 처리(코드 유지, 보고 문구 정정, 실제 모양 시나리오 추가, 후속 등록)를 그대로 이행했고, WBS P2-9가 hub에 실재한다. 다만 PM 완료 보고에서 "한 번에 된다"는 주장을 빼는 것이 해소 조건이다(아래 조건 참조). 신규 지적은 Nit 2건(토스트 잔존, 기존 결과 패널 잔존)뿐이다.

## 재검토 모드 판정
- 풀패널 강제 승격 조건: 해당 없음. `git show 11d0830 --stat` 기준 변경 파일은 `src/*.ts`, `src/styles.css`, 하네스 3개다. `src-tauri/`·capabilities·인증·설치 실행 경로는 건드리지 않았다.
- 동일 대상 인정 조건 직접 대조: target_id 동일, 리스크 범주 동일, 변경이 직전 지적의 수정 범위 안에 있음(커밋 메시지 항목과 diff 1:1 대조), 새 커맨드·수집 경로 없음 → 충족.
- 새 리스크 표면: 없음. "경로는 있으나 버전 확인 실패" 상태를 `error`로 표시하는 분기가 하나 추가됐지만, 이는 기존 screen-state-honesty 역할개념 안에 있다 → **축소(C)**.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 없다. 축소 모드라 직전 4인 가운데 지적 소유 관점 2인과 발산형 1인만 동원했다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-claimed-vs-verified.md | 재사용 | RV-001 픽스처 stale 문제와 RV-002 주장≠도달 문제의 소유 관점 |
| persona-screen-state-honesty.md | 재사용 | RV-003(미설치 확정 조건)과 RV-005(stale 사유)의 소유 관점 |
| persona-zero-base-redesigner.md (발산) | 재사용 | 발산형 필수. P2-9 이월의 구조적 타당성을 점검 |
| persona-first-run-newcomer.md | 생략 | RV-001·RV-004 해소 여부를 다른 두 관점과 스크린샷으로 판정할 수 있어 축소 모드에서 생략했다 |

## 직전 지적 해소 여부

| # | 직전 심각도 | 상태 | 근거(원본 라인 인용/확인방법) |
|---|---|---|---|
| RV-001 | 🟠 Major | **해소** | `src/onboarding.ts:90` `t.required && !t.installed && t.id !== 'claude'`. `scripts/ui-harness/flows/home.mjs` 픽스처가 `tool('claude', …, true, …, { installDependsOn: ['node'] })`로 실제 백엔드와 일치한다. 새 단언(필수 단계 done, 카운트 `1/3`, 상세에 Claude 미포함)이 있고 재실행 결과 home 19 시나리오 bugs=0이다. `p2-1-claude-missing.png` 열람 결과 "1/3 완료 · ✓ 필수 도구 설치 · ❯ Claude Code 설치 [개발 도구에서 설치] · ○ claude 로그인(설치 뒤 가능)"으로 중복이 없다. |
| RV-002 | 🟠 Major | **해소(조건부)** — 아래 판정 참조 | real-shape 시나리오(`devToolsOrder.mjs` `realShapeNoNodeList`)가 `query.rs:34-58` 미설치 분기(`path: None`, `action_kind = resolve_install_plan`)와 같은 모양이다. 재실행 결과 bugs=0이고, `p2-2-real-shape-no-node.png`에서 Claude·Node·pnpm·Wrangler는 "안내 보기"(manual)로 남고 gh 업데이트만 실행되며 건너뜀 패널은 없다. hub `wbs_list`에서 P2-9(`01m4swdapc1ax9p5b8njxy3vkj`, planned, "R7 RV-002 후속")가 실재함을 확인했다. |
| RV-003 | 🟡 Minor | **해소** | `onboarding.ts:70-72` `if (claude.installed) return 'installed'; return claude.path == null ? 'missing' : 'unknown';`. 백엔드 `query.rs:53`이 미발견일 때만 `path: None`이라 판정 근거가 맞다. `home.ts:520`의 위젯 분기는 `=== 'missing'`일 때만 설치 버튼을 내므로, unknown이면 로그인 버튼이 유지된다. `p2-1-claude-path-present-version-failed.png` 열람 결과 "? Claude Code 설치 — claude 실행 파일은 있지만 버전을 확인하지 못했습니다 [다시 확인]"이고 로그인 단계는 blocked가 아닌 todo다. 하네스가 위젯의 "앱에서 로그인" 존재와 "설치하기" 부재를 단언한다. `onboarding.ts:109-112`의 `if (claude)` 분기는 앞선 `devToolsUnknown` 반환 뒤라 path가 non-null일 때만 도달한다(추적 확인). |
| RV-004 | 🟡 Minor | **해소** | `efficiencyGuide.ts:139` "/compact·/clear는 컨텍스트를 줄이는 대신 이후 캐시 재사용이 줄어들 수 있으니 필요할 때만", `:162` "서로 무관한 작업끼리만 세션을 나누고… /compact는… 캐시 재사용이 줄어들 수 있는 선택". 두 카드의 권고 방향이 같아졌다. 규칙·임계값은 바뀌지 않았다. 경계 테스트(`guideBoundary.mjs`)에 비모순 단언이 추가됐고 재실행 결과 34/34 PASS다. |
| RV-005 | 🟡 Minor | **해소** | `src/views/devTools.ts:520-527` 렌더 시 `unmetInstallDependencies(tool, installedNow)`로 재계산해, 선행 도구가 모두 설치됐으면 패널을 숨긴다. 남은 선행 도구는 현재 목록 기준 이름으로 표시한다. `skip-reason-clears-after-node-installed` 재실행 결과 bugs=0이고, `p2-2-skip-cleared-after-node-installed.png` 열람 결과 Node.js 1.0.0 설치 상태에서 Claude 행에 건너뜀 패널이 없다. |
| RV-006 | ⚪ Nit | 미반영(수용) | 직전 리뷰에서 정직성 문제가 아니라고 판정했다. 백로그 |
| RV-007 | ⚪ Nit | 미반영(수용) | 1차 범위로 수용. RT-3 소관 |
| RV-008 | ⚪ Nit | **해소** | `src/styles.css:1445` `margin-top: var(--space-05)`. 토큰 정의는 `styles.css:74` `--space-05: 2px`로, 값 동일 |
| RV-009 | ⚪ Nit | 범위 밖 유지 | 기존 토스트 동작 |

### RV-002 "후속 이월" 처리 판정 — 수용
- 직전 Major의 실체는 **코드 결함이 아니라 주장과 도달 경로의 불일치**였다. 직전 리뷰도 "코드 수정은 필수가 아니다"라고 했고, 권고는 ①보고 문구 정정 ②실제 모양 시나리오 추가 ③실효화는 후속 등록 세 가지였다. ②③은 이행이 확인됐다. ①은 PM 위임문에 "완료 보고에서 주장하지 않는다"로 명시돼 있다.
- 실제 모양 시나리오는 "오동작 없음"(manual 도구 미호출, 동의창 미표시, 건너뜀 패널 미생성, 설치된 도구 업데이트 정상)을 단언한다. 즉 지금 코드가 해가 되지 않는다는 점이 관측으로 고정됐다.
- **해소 조건**(PM 책임): P2-2 WBS 완료 기록·릴리스 노트·완료 보고에 "신규 머신 일괄 설치가 한 번에 된다"를 쓰지 않는다. "배치 내 의존 순서·건너뜀 가드(현 백엔드에서는 방어 경로), 연쇄 설치는 P2-9"로 적는다. 이 보고서 작성 시점에는 그 문구가 아직 작성되지 않아 확인하지 못했다.

## 신규 지적

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-010 | ⚪ Nit | 정직 | `src/views/devTools.ts:418`, `docs/screenshots/r7-2026-10-13/p2-2-skip-cleared-after-node-installed.png` | 스크린샷 열람 + 코드 Read | 재조회로 건너뜀 패널이 사라진 뒤에도, 배치 종료 시 띄운 토스트 "3개 도구는… 건너뛰었습니다 — 각 항목에서 사유를 확인하세요"가 화면에 남아 있다. 토스트가 가리키는 사유는 이미 사라졌다. 토스트는 일시적이고, 하네스가 재조회를 수 초 안에 실행해서 생긴 장면이라 실사용에서 겹칠 가능성은 낮다. | 조치 불요. 거슬리면 재조회 시 해당 토스트를 닫는다. |
| RV-011 | ⚪ Nit | 정직(범위 밖, 기존 동작) | 같은 스크린샷의 Node.js 행 | 스크린샷 열람 | Node가 "1.0.0 설치됨"으로 재조회됐는데도 직전 배치의 "✕ 실행 실패 · 확인되지 않음(unverified)" 결과 패널이 남아 있다. RV-005와 같은 성격의 stale 상태지만, `lastResult` 패널의 기존 수명 규칙이라 이번 diff가 원인은 아니다. | 백로그 참고. 재조회 시 installed로 바뀐 도구의 실패 결과 패널을 "이후 설치 확인됨"으로 표시하거나 숨긴다. |

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 정직 | RV-005 렌더 재계산이 배치 진행 중의 `confirmedInstalled`가 아니라 현재 목록을 써서, 배치 중 판정과 표시가 어긋날 수 있다 | 기각 | 배치 중에는 `updating`/`pendingPreview` 가드(`devTools.ts:520`) 때문에 패널을 그리지 않는다. 배치 후 현재 목록 기준 표시가 오히려 "지금 사실"에 맞다. 선행 도구가 unknownAfter였다가 재조회에서 installed가 된 경우 패널을 숨기는 것도 사실과 맞다. |
| 주장≠검증 | `path == null` 비교가 `undefined`(필드 누락)를 missing으로 확정한다 | 기각 | 백엔드 `DevToolStatus.path`는 `Option`이라 직렬화하면 항상 `null` 또는 문자열이다(`query.rs:53`·`:71`·`:95`). 필드 누락은 계약상 생기지 않는다. |
| 정직 | path 있음 + 버전 실패 상태에서 로그인 단계가 todo인데 현재 단계 표시가 없어 다음 행동이 불명확하다 | 기각 | 앞 단계가 error면 현재 단계를 지정하지 않는다(`onboarding.ts:136-143`)는 기존 설계 규칙 그대로다. 위젯의 로그인 버튼과 "다시 확인" 액션이 모두 남아 있어 막다른 골목이 아니다(스크린샷 확인). |

## 페르소나별 관점

### claimed-vs-verified — Green
- 픽스처 두 곳이 모두 실제 백엔드 계약으로 정정됐다. 특히 real-shape 픽스처 주석이 `query.rs: resolve_install_plan → Manual`이라는 출처를 밝혔고, 실물과 대조해 일치함을 확인했다.
- RV-002는 "코드를 고쳤다"가 아니라 "해가 없음을 관측으로 고정 + 실효화 후속 등록"으로 처리됐다. 정직한 처리다. 주장 정정은 PM 보고 시점에 확인해야 한다.

### screen-state-honesty — Green
- 확인 불가 상태가 '?' 표시와 "실행 파일은 있지만 버전을 확인하지 못했습니다"라는 사실 서술로 바뀌었다. 미설치를 단정하지 않는다.
- stale 사유는 렌더 시 재계산으로 제거됐다. 남은 stale 요소 2건(RV-010·011)은 일시적이거나 기존 동작이다.

### zero-base-redesigner (발산) — 아래 Rethink

## 구조적 제언 (Rethink, 발산형)

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-4 | P2-9 제목은 "Node 설치 확인 후 재조회로 Claude/pnpm 이어서 설치" | P2-9 본문에 **선행 조건으로 P2-3(macOS 새 기기 자동설치 경로)을 명시**하거나, 범위를 "Windows 우선 + mac은 P2-3 이후"로 쪼갠다 | mac에서는 Node가 구조적으로 manual이다(직전 RV-002, `install_resolver.rs` 125-128 주석). 그래서 재조회 연쇄만으로는 mac 신규 머신 1회 셋업이 생기지 않는다. 그대로 두면 P2-9 완료 후 같은 주장≠도달 문제가 재발한다. Windows도 Node winget 미리보기가 불신뢰라 개별 확인 1회가 남는다. | 낮음(WBS 메모 수정만). 근거: hub WBS 목록에서 P2-3이 별도 Exploration 항목으로 planned 상태임을 확인함 |

## 잘 된 점
- 지적마다 하네스 단언을 1:1로 추가했다(RV-001 카운트·단계, RV-003 위젯 버튼·단계 상태, RV-004 비모순, RV-005 소거, RV-002 실제 모양). 다음 회귀를 자동으로 잡는다.
- RV-001 수정이 `claudeInstallState`가 이미 쓰는 id `'claude'`를 재사용해 새 하드코딩 표면을 만들지 않았다.
- 문구 수정(RV-004)은 규칙·임계값을 건드리지 않고 action 문자열만 바꿨다. 범위 통제가 좋다.
- 스크린샷 4장이 각 해소 지점을 정확히 보여 준다(4장 모두 직접 열람).

## 검증 수행 내역 (정직 보고)
- `git show 11d0830` 전체를 Read했다. `git diff --stat 34ef952..11d0830`으로 최종 범위를 확인했다(18 파일).
- `pnpm exec tsc --noEmit` 통과.
- 하네스 재실행: home 19 시나리오 bugs=0, devToolsOrder 10 시나리오 bugs=0, usageGuide 6 시나리오 bugs=0(경계 34/34 PASS), usage 4 시나리오 bugs=0. 포트 1420은 기존 vite(PID 3467)를 재사용했고 종료하지 않았다.
- 스크린샷 열람: p2-1-claude-missing, p2-1-claude-path-present-version-failed, p2-2-skip-cleared-after-node-installed, p2-2-real-shape-no-node(갱신·추가분 4장 전부).
- hub `wbs_list`(status=planned)로 P2-9·P2-3이 실재함을 확인했다.
- **미확인**: 실제 Tauri 앱 실기동, 실제 mac/Windows 백엔드 응답, Rust 테스트(이번 커밋은 `src-tauri` 무변경이라 재실행하지 않음), PM 완료 보고 문구(아직 작성 전).
- 소스 수정, git commit/push/stash/checkout/reset은 하지 않았다. 산출물은 이 보고서, 페르소나 3개 파일의 적용 이력 append, INDEX.md "최근 재사용" 3행 덧붙임뿐이다.

## PM에게 권고
1. 출시를 막는 항목은 없다. Green으로 닫아도 된다.
2. **RV-002 해소 조건을 지킨다.** P2-2 완료 기록·완료 보고에 "신규 머신 일괄 설치 한 번에"를 쓰지 않고, 연쇄 설치는 P2-9 후속으로 적는다.
3. P2-9 메모에 RT-4(mac은 P2-3 선행)를 덧붙인다.
4. RV-010·011(Nit)은 백로그 참고용이다.
