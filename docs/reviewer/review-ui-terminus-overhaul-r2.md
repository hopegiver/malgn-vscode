# Terminus UI 전면 개편 리뷰 보고서 — 2차(재검토)

리뷰 대상: target_id `ui-terminus-overhaul` — 브랜치 `feat/ui-terminus` HEAD `6cc6487ce6fcec0f1bfebc27b26a590f363b3e72` (직전 리뷰 기준 `ce8251f`, 이번 검토 범위 `git diff ce8251f..6cc6487`)
종합 판정: **Amber (조건부 통과)** — Critical 0 / Major 1(M3 잔존) / Minor 2(신규) / Nit 5 / Rethink 1

리뷰 페르소나 패널(4인, 모두 재사용): `docs/reviewer/personas/persona-design-contract-auditor.md`, `persona-window-resize-and-post-interaction.md`, `persona-daily-power-operator.md`, `persona-zero-base-redesigner.md`(발산형)
리스크 범주: 전 화면 UI·내비게이션 구조 변경(배포되면 전 사용자에게 자동 업데이트로 나감). 직전 라운드와 같음
등급/모드: Refactor, 2차 증분 재검토(아래 "모드 판정" 참고)
리뷰 일자: 2026-09-24

## 요약 (2분 규칙)
직전 Major 3건 중 **M1(재로그인 첫 클릭 소실)과 M2(900px 상태줄 잘림)는 해소됐습니다.** 둘 다 독립적으로 재현해 확인했습니다. 구현자의 회귀 흐름 5개를 수정 전 트리(`ce8251f`)에서 돌리면 버그 6건으로 실패하고, HEAD에서는 0건입니다. 전체 하네스 79개 시나리오도 HEAD에서 bugs 0으로 다시 확인했습니다. 걱정했던 `setTimeout(…,0)` 지연 등록도 문제가 없었습니다. window 리스너 등록 집합을 정확히 추적하며 열기와 닫기를 20회 반복했고, 같은 태스크 안에서 열기→Esc→열기를 이어 붙이는 경합과 키보드 로그아웃까지 돌렸지만 누수는 0이었습니다.
**M3(폰트 방침)는 일부만 해소됐습니다.** 16개 셀렉터는 고쳐졌지만 한글이 모노 폰트로 렌더되는 곳이 2곳 남아 있습니다. 사용량 탭 통계 타일의 "3일"(실측)과 개발 도구 실행 전 확인 패널의 "다음 명령을 실행합니다: …"(코드 확인)입니다. 두 곳 모두 두 줄 안팎의 수정으로 끝납니다. PM이 R1 대신 택한 "상태줄 좌우 그룹 분리"는 **타당하다고 판정합니다**(근거는 아래 "R1 미채택 판정" 절).

## 모드 판정 (재검토 3요소·동일 대상 조건)
- 재검토 3요소: target_id `ui-terminus-overhaul`, 직전 리뷰 `docs/reviewer/review-ui-terminus-overhaul.md`, 리스크 범주. 셋 다 위임 메시지에 있습니다.
- 풀패널 강제 승격 조건(`common-task-grading-and-verification-depth`)을 대조했습니다. "직전 Major 미해결"은 이번 라운드가 바로 그 해소를 검증하는 라운드라 해당하지 않는다고 봤습니다. 대신 **직전 패널 4인을 모두 그대로 투입**해 풀패널과 같은 관점 폭을 유지했습니다. 새 실행경로는 `setTimeout` 지연 등록 하나뿐이고 기존 리스크 표면(조작 이후 상태) 안에 있습니다. 다른 Sensitive 하위도메인으로 들어가지 않았고, 이번이 2차입니다.
- 동일 대상 4조건을 모두 충족합니다. target_id가 같고, 직전 리뷰와 같은 날이며, 리스크 범주가 같고, 파일도 `src/sidebar.ts`·`src/styles.css`·views가 실질적으로 겹칩니다.
- 판정: **증분(B)**. 새 리스크 표면인 지연 등록 리스너는 직접 프로브로 검증했습니다.

## 페르소나 재사용 판정
착수 전 `docs/reviewer/personas/INDEX.md`(15행)의 역할개념 열을 대조했습니다. 신규 페르소나는 0명입니다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-design-contract-auditor | 재사용 | M3·m4·m5는 조항 단위 대조라 역할개념 그대로임 |
| persona-window-resize-and-post-interaction | 재사용 | M1(조작 이후 리스너)·M2(900px)와 새 `setTimeout` 경합이 이 페르소나의 두 축에 해당함 |
| persona-daily-power-operator | 재사용 | M1-b·m1·m3·m6은 매일 쓰는 동선과 키보드 경로 문제임 |
| persona-zero-base-redesigner (발산형) | 재사용 | §3 발산형 게이트. 이번 라운드에서는 R1 미채택 결정을 판정함 |

INDEX.md의 "최근 재사용" 열과 4개 파일의 "적용 이력"에 이번 라운드를 덧붙였습니다.

## 근거 수집 방법
- **빌드**: HEAD에서 `pnpm build`(tsc + vite build)가 통과했습니다. `git diff --stat ce8251f..6cc6487 -- src-tauri 'src/*Api.ts'`는 0줄입니다. Rust 변경이 없으므로 cargo는 다시 돌리지 않았습니다(구현자의 "cargo 543"은 이 커밋과 무관).
- **하네스 전체**: HEAD에서 `pnpm dev`를 띄우고 `node scripts/ui-harness/run.mjs`를 돌렸습니다. **79개 시나리오 bugs=0, consoleErrors=0**으로 구현자 주장과 일치합니다(이 실행으로 미추적 파일 `scripts/ui-harness/last-run-report.json`이 갱신됨).
- **수정 전 실패 재현**: `git archive ce8251f`로 수정 전 트리를 scratchpad에 풀고, 거기에 새 흐름 `accountMenuAndStatusline.mjs`만 얹어 1430 포트에서 실행했습니다. 5개 시나리오에서 **bugs 6건**(M1 Critical 1, M1-b Major 1, m6 Minor 1, M2 Major 3)이 나왔습니다. HEAD에서는 0건입니다. "수정 전 실패 → 수정 후 통과" 주장을 확인했습니다.
- **독립 프로브**(scratchpad 전용 흐름, 저장소에 넣지 않음): `window.add/removeEventListener`를 **함수 참조 Set**으로 추적했습니다. 하네스 카운터는 등록되지 않은 핸들러를 제거해도 수가 줄어 누수를 가릴 수 있어서 따로 만들었습니다(n6).
- **캡처**: `docs/screenshots/ui-terminus-review-r2-2026-09-24/r2-*.png` 8장. 모두 Read로 직접 열어 확인했습니다.
- **폰트 실측**: 9탭과 진행상황판 등 10개 라우트에서 "한글이 들어간 텍스트 노드의 부모 `getComputedStyle().fontFamily`가 JetBrains Mono인 것"을 TreeWalker로 전수 스캔했습니다. 정적 grep과 달리 실제 렌더 결과를 봅니다.

## 직전 지적 해소 현황

| # | 직전 심각도 | 상태 | 확인방법 · 근거 |
|---|---|---|---|
| M1 | Major | **해소** | `src/sidebar.ts:172-182` logoutItem에 `{ stopPropagation: true }`를 추가했습니다. `clickable()`(`src/dom.ts:35-49`)은 click과 keydown 양쪽에서 전파를 막으므로 **키보드 Enter 로그아웃 경로도 막힙니다**(프로브: 키보드 로그아웃 후 리스너가 기준선으로 돌아오고 로그인 1회 클릭으로 셸에 진입). 수정 전 트리에서는 `logout-then-relogin-single-click`이 실패하고 HEAD에서는 통과합니다. |
| M1-b | (M1 병합) | **해소** | 바깥 클릭 감지를 `mousedown`에서 `click`으로 옮겨 재렌더가 mouseup 전에 일어나지 않습니다. 프로브에서 메뉴를 연 채 탭을 5회 클릭했고, 매번 이동한 뒤 메뉴가 닫혔습니다(최종 hash `#/projects`). |
| M1 `setTimeout` 경합·누수 | (이번 요청 초점) | **문제없음** | 아래 "setTimeout 지연 등록 검증" 절 |
| M2 | Major | **해소** | `styles.css:387-394`(`.statusline-left` flex:1 1 auto·min-width:0·overflow:hidden / `.statusline-right` flex:none). 최악 조합(900px, 업데이트 배지, 긴 한글 프로젝트명, 프로젝트 상세)을 실측했습니다: right 그룹 527~890, 배지 588~702, 새 버전 확인 729~805, 시계 832~890이며 **모두 900 안**입니다. 캡처는 `r2-900x600-update-badge-longname-project-detail.png`. |
| M3 | Major | **부분 해소 → 잔존 2곳(아래 M3-r)** | 16개 셀렉터를 `--font-body`로 바꿨습니다. 캡처 `r2-tasks-1440x900.png`의 스케줄 줄, `r2-home-*`의 시간 열이 산세리프로 나옵니다. |
| m1 | Minor | 해소 | `home.ts` `isQueuedTask`로 타일과 큐 헤더의 필터가 같아졌습니다(캡처에서 타일 "1"·헤더 "1건"). 배지는 "실행 중" + green입니다. 세션 행의 "실행중" 표기만 남았습니다(n5). |
| m2 | Minor | 해소 | 캡처 `r2-home-1440x900.png`에서 세션 ID "#a591d9dc…", 도구명 "Claude Code"·"GitHub CLI"가 잘리지 않습니다. |
| m3 | Minor | 대부분 해소 | 탭·제목·버튼이 "세션/개발 도구/자율 작업"으로 통일됐습니다. 잔존 2곳은 n5에 적었습니다. |
| m4 | Minor | 해소 | `appLinks.ts` 문구가 "이 탭 사이드바"로 바뀌었습니다(캡처 `r2-applinks-1440x900.png`). |
| m5 | Minor | 해소 | IA §2-3(220/175px)·§4-6(전체 작업) 갱신, 0건 문구, `state.ts`·`main.ts` 주석을 고친 것을 diff로 확인했습니다. |
| m6 | Minor | 해소 | `role=tablist/tab` + `aria-selected`(`sidebar.ts:86-91, 230-232`), Escape 닫기가 들어갔습니다. 키보드 포커스 유실은 별도 항목 m10으로 적었습니다(이번 수정으로 생긴 회귀가 아니라 기존 결함). |
| m7 | Minor | 해소 | `.home-widget` 토큰 교체, `.box-head > *:last-child:not(.box-title)`, `.mcp-list` max-width 제거. 새 셀렉터의 영향 범위는 `boxHead()` 호출처 전수(home·settings·appLinks·usage·tasks·devTools·catalog)를 대조했는데, 모두 "제목 + 오른쪽 슬롯 1개" 구조라 의도하지 않은 이동은 없습니다. |
| m8 | Minor | 해소 | 캡처 `r2-home-900x600.png`에서 통계 타일 4개가 한 줄에 들어갑니다. |
| n1·n2 | Nit | 미처리 | 백로그 유지(760px 죽은 규칙, ┌─ 기준선) |

## setTimeout 지연 등록 검증 (구현자 주석 `sidebar.ts:96-116`)
**판정: 경합·누수 없음. 설계도 타당함.**
- **논리 검토**: 핸들러 참조를 모듈 변수에 먼저 저장하고, 타이머 콜백은 `accountMenuOutsideClickHandler === handler`일 때만 등록합니다(`:139-142`). 그래서 열기→(0ms 안에)닫기라면 `closeAccountMenu`가 변수를 null로 비워 등록되지 않습니다. 열기→닫기→다시 열기라면 첫 타이머는 참조가 달라 건너뛰고 두 번째만 등록합니다. 구현자가 주석에 적은 "같은 dispatch 중에 window에 추가한 리스너가 같은 이벤트로 발화한다"는 설명은 DOM 사양(리스너 목록은 해당 노드 차례에 읽힘)과 맞습니다.
- **실측(함수 참조 Set 추적, 기준선 대비 증감)**:
  ```
  baseline                       click=1 keydown=0
  10x chip open/close            click=1 keydown=0 open=false
  5x open/Escape                 click=1 keydown=0 open=false
  5x open/탭 클릭                 click=1 keydown=0 open=false hash=#/projects
  동일 태스크 open→Esc→open        click=2 keydown=1 open=true   ← 정확히 1세트만 등록
  바깥 클릭                        click=1 keydown=0 open=false
  키보드 Enter 로그아웃             click=1 keydown=0 (로그인 화면) → 로그인 1회 클릭으로 셸 진입
  ```
- 남는 창은 등록 전 0ms 매크로태스크 1개뿐입니다. 사람의 두 번째 클릭은 그 안에 도달할 수 없어서 실질 위험이 없습니다.
- 대안(참고, 필수 아님): 캡처 단계 등록이나 `e.composedPath()` 비교로 "재렌더로 떨어져 나간 구 노드를 바깥으로 오판"하는 문제를 타이머 없이 풀 수도 있습니다. 지금 방식에서 결함은 찾지 못했습니다.

## 지적 사항 (통합, 이번 라운드)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| M3-r | **Major(잔존)** | 설계 계약 | ① `src/views/usage.ts:260` (`value: \`${t.activeDays}일\``) + `src/styles.css:1263-1264`(`.stat-value` font-numeric) ② `src/views/devTools.ts:580`(`pending.message`) + `src-tauri/src/dev_tools/actions.rs:431`(`"다음 명령을 실행합니다: {cmd}"`) + `src/styles.css:1013-1014`(`.devtool-panel-command` font-numeric) | ① DOM 전수 스캔(10개 라우트 중 유일한 적중: `stat-value :: 3일`) + 캡처 `r2-1280x820-usage-stat-value-mono-hangul.png` ② 코드 Read. 확인 패널은 조작 뒤에만 나타나 스캔 대상에서 빠졌음 | 확정 방침(`terminus-design-system.md` §1 표, "예외 없음")을 아직 어깁니다. ①은 사용량 탭에 들어갈 때마다 보이는 타일에서 "일"이 모노 폴백으로 렌더됩니다. ②는 한글 안내문이 명령 박스 안에서 모노로 나옵니다. 구현자의 grep이 **CSS 셀렉터 기준**이라, 공용 클래스(`.stat-value`는 홈 4개와 사용량 4개 타일이 공유)에 한글 값이 흘러드는 경우를 놓쳤습니다. 이 방침은 두 번 반려 끝에 확정된 사항이라 Major를 유지합니다. | ① 이미 있는 단위 라벨 패턴 `.stat-value small`(`styles.css:1267`, "통계 값 옆 단위 라벨")을 써서 값을 `3` + `<small>일</small>`로 나누고, `small`에 `font-family: var(--font-body)`를 줍니다. 또는 라벨을 "활동일수(일)"로 바꾸고 값은 숫자만 둡니다. ② 프론트에서 `MANUAL_PREVIEW_PREFIX`(`devTools.ts:248`)를 떼어 라벨 div(body)로 두고, 명령만 `.devtool-panel-command`에 넣습니다. 재발 방지로, 이번에 쓴 "한글 텍스트 노드 × 부모 computed font" TreeWalker 스캔을 하네스 흐름에 넣길 권합니다(직전 권고의 grep 가드보다 정확합니다. 스캔 코드는 아래 부록). |
| m9 | Minor(신규) | 설계 계약 | `src/styles.css:990`(`.devtool-binary` mono, `devTools.ts:436`에 `tool.path` 포함) vs `src/styles.css:554`(`.detail-path`를 "한글 폴더명 가능"을 이유로 body로 변경) · `terminus-design-system.md` §1 표("경로"는 numeric leaf로 분류) | Read | 이번 수정이 "경로는 한글일 수 있다"는 이유로 `.detail-path`·`.applink-filepath`는 산세리프로 바꾸고 `.devtool-binary`는 모노로 둬서, 경로에 대한 규칙이 둘로 갈렸습니다. 게다가 설계 정본 표에는 여전히 "경로 = numeric"으로 적혀 있어 문서와 구현이 어긋납니다. Windows 한글 사용자명(`C:\Users\홍길동\…`)이면 실제로 모노 폴백이 납니다(실기 미확인, 추정). | design-system §1 표의 "경로"를 "한글 가능 → body"로 옮기고, `.devtool-binary`도 같은 규칙을 따릅니다. |
| m10 | Minor(기존, 이번 회귀 아님) | 파워유저(키보드) | `src/sidebar.ts:118-151`(`openAccountMenu`→`notifyChange()` 재렌더), `:159-165` | 프로브(칩에 포커스 → Enter → `document.activeElement.className === ''`, 즉 body) + 캡처 `r2-1280x820-keyboard-open-account-menu.png` | 키보드로 메뉴를 열면 재렌더로 칩 노드가 바뀌어 포커스가 body로 떨어집니다. 로그아웃 항목까지 가려면 문서 처음부터 Tab을 다시 눌러야 합니다. 키보드로 다른 탭을 활성화(Enter는 keydown이라 click이 아님)해도 메뉴가 닫히지 않고 새 화면 위에 남습니다(Escape로는 닫힘). | 열린 뒤 `.tabstrip-account-dropdown-item`에 `focus()`, 닫은 뒤 칩으로 포커스를 복귀시킵니다. 탭의 onClick 경로(navigate)에서 `accountMenuOpen`이면 닫습니다. |
| n3 | Nit | 창 크기 | `src/styles.css:387-390` + 캡처 `r2-900x600-update-badge-longname-project-detail.png` | 캡처 | 왼쪽 그룹이 말줄임 없이 **글자 중간에서 잘립니다**("…이름을" 뒤가 뚝 끊기고 구분자 없이 오른쪽 그룹 `v0.2.8`이 이어짐). 기능 손실은 없습니다. | 프로젝트명 세그먼트에만 `min-width:0; overflow:hidden; text-overflow:ellipsis; flex:0 1 auto`를 주거나, `.statusline-left`에 `mask-image` 페이드를 줍니다. |
| n4 | Nit | 문서 드리프트 | `src/sidebar.ts:702-708` 주석("margin-left:auto로") vs `src/styles.css:393`(`margin-left: var(--space-3)`) | Read | 주석과 실제 CSS가 다릅니다(동작은 left가 flex:1이라 같음). | 주석을 고칩니다. |
| n5 | Nit | 용어 | `src/views/home.ts:156`('실행중'), `src/views/projects.ts:211`('자율업무'), `src/views/devTools.ts:70`('개발 환경') | grep(주석 제외) | m1·m3 용어 통일에서 빠진 곳이 3곳 남았습니다. | "실행 중"·"자율 작업"·"개발 도구"로 바꿉니다. |
| n6 | Nit | 테스트 신뢰성 | `scripts/ui-harness/lib/bridge.mjs:25-27` | Read | `removeEventListener`가 등록되지 않은 핸들러에도 카운트를 줄입니다. 이번 지연 등록 구조에서는 "열기→0ms 안에 닫기"가 등록 없이 제거를 부르므로, 누수가 있어도 카운트가 상쇄돼 M1 단언(`accountMenuAndStatusline.mjs:107-114`)이 통과할 수 있습니다. 현재 코드에서 누수는 없습니다(Set 프로브로 확인). | 카운터를 `Map<type, Set<fn>>`으로 바꿉니다. |
| n7 | Nit | 파워유저 | `src/sidebar.ts:184-188`(드롭다운이 칩 clickable의 자식) | 프로브(이메일 라벨 클릭 → open=false) | 드롭다운 안의 이메일 라벨을 클릭하면 칩 토글로 버블링돼 메뉴가 닫힙니다. 기존 동작이고 해도 없지만 의도한 것은 아닐 것입니다. | 드롭다운 컨테이너에 click `stopPropagation`을 둡니다. |

## R1 미채택 판정 (발산형 + 창 탄력성 합의)
**판정: PM 결정 타당. R1은 "필수"에서 "선택적 구조 개선"으로 내립니다.**
- 직전 R1의 목적은 "업데이트 진입점이 어느 창 폭에서도 잘리지 않게"였습니다. 현재 구조는 `.statusline-right`를 `flex:none`으로 두어 **오른쪽 그룹이 자기 폭을 무조건 확보**하므로 같은 목적을 이룹니다. 최악 조합 실측에서 오른쪽 그룹 폭은 363px, 900px 창에서 왼쪽에 505px이 남습니다.
- 오른쪽 그룹에 들어가는 요소는 **길이 상한이 있는 고정 문자열뿐**입니다(`sidebar.ts:648-690`: `v{version}`, "업데이트 적용 (vX.Y.Z)"/"업데이트 적용 중…", "새 버전 확인"/"확인 중…", 시계). 사용자 데이터가 들어가지 않으므로 이 보장은 구조적으로 유지됩니다.
- R1보다 나은 점도 있습니다. 탭스트립은 9탭과 계정 칩으로 이미 900px 하한에 가깝습니다(직전 캡처 `review-900x600__*`). 거기에 배지를 더하면 탭스트립 쪽에 새 잘림 위험이 생기는데, 지금 방식은 그 위험이 없습니다.
- 남는 약점은 두 가지입니다. ① 우선순위가 낮은 세그먼트가 말줄임 없이 잘립니다(n3). ② 나중에 누군가 오른쪽 그룹에 가변 길이 요소를 추가하면 보장이 깨집니다. ②는 `statusline-update-and-clock-visible-at-900` 시나리오가 잡아 주지만, 그 시나리오는 홈 라우트만 봅니다. 긴 프로젝트명 + 배지 + 900px 조합은 이번 프로브에서만 확인했으니 시나리오에 추가하길 권합니다.

## 구조적 제언 (Rethink) — 발산형 페르소나

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R5 | 폰트 방침을 **CSS 셀렉터 단위**로 지킵니다. 셀렉터마다 "여기에 한글이 올 수 있나"를 사람이 판단하고, 그 결과 `.devtool-log`(CLI 로그)까지 산세리프가 됐습니다(`styles.css:1024-1028`). 로그와 명령 박스는 원래 모노가 정렬상 맞는 영역입니다. | 규칙의 단위를 **데이터 종류**로 옮깁니다. "숫자·버전·시계만 모노 leaf(`<span class="num">`), 그 외 전부 body"를 기본값으로 두고, 셀렉터의 `font-family: var(--font-numeric)`은 `.num`·`::before` 글리프에만 허용합니다. 로그·명령 박스는 "모노 + 한글 폴백을 Noto Sans KR 등으로 명시한 스택(`--font-code`)"이라는 별도 토큰으로 방침에 예외 조항을 둘지 **사용자에게 확인**합니다. | 셀렉터 전수 판단이 필요 없어져 이번 M3-r 같은 누락이 구조적으로 사라집니다. 모노 스택에 한글 폴백을 명시하면 "한글이 모노 폴백으로 자간이 벌어지는" 원래 불만도 로그 영역에서 해결됩니다. | 낮음~중. CSS 규칙 정리와 하네스 스캔 1개. 예외 조항은 "예외 없음"으로 확정된 방침을 바꾸는 일이라 사람 확인이 필요하므로 이번 PR 범위 밖으로 둡니다. |

R2~R4(라우트 승격, 프로젝트 마스터-디테일, 명령 팔레트)는 직전 보고서 그대로 유효하며 이번 라운드 범위 밖입니다.

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 창 탄력성 | `setTimeout(…,0)`이 등록되기 전 0ms 사이의 클릭은 바깥 클릭으로 처리되지 않는다 | 기각 | 같은 클릭의 dispatch가 끝난 직후의 매크로태스크라 사람 입력이 그 사이에 들어올 수 없습니다. 프로브에서 합성 동기 시퀀스로도 이상이 없었습니다. |
| 설계 계약 | `.box-head > *:last-child:not(.box-title)`가 다른 화면의 헤더 배치를 흔든다 | 기각 | `boxHead()` 호출처를 전수 대조했습니다. 오른쪽 슬롯이 있는 곳은 모두 "제목 + 슬롯 1개"라 의도한 배치와 같고, 캡처 `r2-home-1440x900`·`r2-applinks-1440x900`에서도 정상입니다. |
| 설계 계약 | `.devtool-log`를 산세리프로 바꾼 것은 방침 위반의 반대 방향 오류다 | 결함에서 제외하고 R5(트레이드오프)로 이동 | 방침 문구("한글 섞일 가능성 있는 모든 UI 텍스트, 예외 없음")에 비추면 구현자 판단이 문언상 맞습니다. 로그 가독성 손실은 규칙 설계의 문제라 Rethink로 다룹니다. |
| 파워유저 | 900px에서 "개발 도구 확인 →" 링크가 가려져 진입점이 사라진다 | 기각 | 같은 목적지로 가는 "개발 도구" 탭이 항상 보입니다. 세그먼트 우선순위 설계(`sidebar.ts:608-615`)대로의 의도된 동작입니다. |

## 트레이드오프 (페르소나 간 충돌)
- **폰트 방침의 문언 준수 vs 로그·명령 가독성(R5)**: 설계 계약 관점은 "예외 없음" 준수를 요구합니다. 파워유저 관점에서는 CLI 로그가 산세리프가 되면 정렬과 가독성을 잃습니다. → 권고: 이번 PR은 문언을 따르고(M3-r 수정), 로그·명령 박스 예외 조항 여부는 사용자에게 묻습니다.

## 잘 된 점
- **회귀 테스트가 먼저 실패하는 것을 증명했습니다**: 새 흐름 5개가 수정 전 트리에서 실제로 6건 실패하는 것을 제가 독립적으로 재현했습니다. 테스트가 결함을 잡을 수 있다는 것이 확인됐습니다.
- **2차 회귀를 스스로 발견하고 원인을 기록했습니다**: `click`으로 옮기면서 생긴 "열자마자 닫힘" 회귀의 원인(dispatch 중 추가된 리스너 발화 + 재렌더로 떨어져 나간 target)을 DOM 사양 수준으로 주석에 남겼습니다(`sidebar.ts:103-116`). 식별자 가드로 누수도 막았습니다.
- **상태줄 우선순위를 배열 순서에 묶었습니다**: 우선순위가 코드 구조 자체에 들어 있어(`sidebar.ts:608-640`) 새 세그먼트를 추가하는 사람이 우선순위를 놓칠 가능성이 낮습니다.
- **변경 범위를 지켰습니다**: Rust·API diff 0, 하네스 79개 무결함, 클래스명 보존으로 기존 시나리오 호환 유지.
- **셀렉터별로 근거를 달았습니다**: M3 수정의 16개 셀렉터마다 "어떤 한글이 들어오는지"를 파일·라인과 함께 주석으로 남겨, 다음 사람이 되돌릴 때 판단 근거가 됩니다.

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| `src/*Api.ts`·`src-tauri/**` diff 0 | 설계 계약 | 필수 | 충족 | 실측 0줄 |
| 폰트 방침(한글 → 산세리프) | 설계 계약 | 필수 | **부분** | M3-r 2곳 |
| 900px에서 모든 조작 도달 가능 | 창 탄력성 | 필수 | 충족 | 최악 조합 실측 |
| 조작 이후 상태 정상(로그아웃 → 재로그인, 마우스·키보드) | 조작 이후 | 필수 | 충족 | 수정 전 실패 → 수정 후 통과, 리스너 누수 0 |
| 키보드 도달성 | 파워유저 | 권장 | 부분 | ARIA·Escape 충족, 포커스 관리 미흡(m10) |
| 동일 화면 수치 일관성 | 파워유저 | 권장 | 충족 | m1 해소 |

## 생략함 / 확인하지 못한 것
- **실기(Tauri 창, Windows·macOS)**: 확인하지 못했습니다. 모든 근거는 Vite dev + 하네스 스텁(Chromium)입니다. 직전 권고 3(머지 전 Windows 실기에서 폰트와 상태줄 폭 1회 육안 확인)은 여전히 유효합니다.
- **개발 도구 실행 전 확인 패널(M3-r ②)**: 렌더 캡처는 없고 코드로만 확인했습니다.
- **n1·n2 해소 여부**: 이번 커밋에 해당 변경이 없어 다시 캡처하지 않았습니다.
- **cargo test**: Rust diff가 0이라 돌리지 않았습니다.
- **색 대비**: 이번에도 재측정하지 않았습니다.

## 실행 액션
코드 수정·커밋·push·배포는 하지 않았습니다. 이번에 만들었거나 고친 파일은 이 보고서, `docs/screenshots/ui-terminus-review-r2-2026-09-24/`(캡처 8장), 페르소나 4개 파일의 적용 이력 append, `docs/reviewer/personas/INDEX.md`이고 모두 미커밋입니다. 부수 효과로, 하네스를 다시 실행하면서 미추적 파일 `scripts/ui-harness/last-run-report.json`이 HEAD 기준 79개 시나리오 결과로 다시 써졌습니다(내용은 bugs 0으로 같은 결론). 프로브, 수정 전 트리, dev 서버는 모두 scratchpad에만 있었고 저장소에는 남기지 않았습니다.

## PM에게 권고
1. **머지 전 필수(Major 1)**: M3-r 2곳. `usage.ts:260`의 단위 분리, `devTools.ts:580`의 접두 문구 분리이며 frontend-dev가 약 2줄씩 고치면 됩니다. 같은 PR에 **한글-모노 DOM 스캔 흐름**(부록)을 하네스에 넣으면 이 부류가 재발하지 않습니다.
2. 수정 뒤에는 **축소 재검토(3차, C 모드)**로 충분합니다. 확인할 것은 스캔 흐름 결과 0건과 캡처 1장입니다. 다른 Major는 모두 해소됐으니 풀패널은 필요 없습니다.
3. Minor m9·m10과 Nit n3~n7은 같은 PR에 넣어도 되고 백로그로 넘겨도 됩니다. m9는 design-system 표 한 줄과 CSS 한 줄이라 비용이 작습니다(코드 확인함). m10(포커스 관리)은 `openAccountMenu`/`closeAccountMenu` 두 곳에 `focus()` 호출을 넣는 정도로 추정합니다(미구현 추정치).
4. R5(폰트 규칙의 단위와 로그 예외)는 확정 방침을 바꾸는 일이라 **사용자 확인 사항**입니다. 차기 라운드에서 판단하면 됩니다.
5. 직전 권고 3(**Windows 실기 1회 육안 확인**)은 전 사용자 자동 업데이트 대상이라 릴리스 전 사람 확인 항목으로 그대로 유지합니다.

## 부록 — 한글-모노 DOM 스캔(하네스 흐름에 그대로 옮길 수 있음)
```js
const hits = await page.evaluate(() => {
  const out = new Set();
  const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let n;
  while ((n = w.nextNode())) {
    if (/[가-힣]/.test(n.textContent)) {
      const p = n.parentElement;
      if (p && /JetBrains/.test(getComputedStyle(p).fontFamily)) out.add(`${p.className} :: ${n.textContent.trim().slice(0, 40)}`);
    }
  }
  return [...out];
});
// 라우트: #/ #/projects #/sessions #/usage #/settings/devtools #/tasks #/tasks/board #/catalog #/settings/applinks #/settings
```
HEAD 실측 결과: `#/usage` → `["stat-value :: 3일"]`, 나머지 9개 라우트 → `[]`.
