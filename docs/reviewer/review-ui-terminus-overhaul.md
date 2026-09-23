# Terminus UI 전면 개편 리뷰 보고서

리뷰 대상: target_id `ui-terminus-overhaul` — 브랜치 `feat/ui-terminus` HEAD `ce8251f667c2a67ac78ba6b79dba5b6fbd850b26` (base `main` = `5375d8cb7fe546ec42edc8454032a4e904289713`, `git diff main...feat/ui-terminus`, 34파일 +5140/−1323)
종합 판정: **Amber (조건부 통과)** — Critical 0 / Major 3 / Minor 8 / Nit 2 / Rethink 4

리뷰 페르소나 패널(4인, 전원 재사용): `docs/reviewer/personas/persona-design-contract-auditor.md`, `persona-window-resize-and-post-interaction.md`, `persona-daily-power-operator.md`, `persona-zero-base-redesigner.md`(발산형)
리스크 범주: 전 화면 UI·내비게이션 구조 변경(배포되면 전 사용자에게 자동 업데이트로 나간다)
등급/모드: Refactor, 최초 리뷰(풀패널). 약식 축소 없음.
리뷰 일자: 2026-09-24

## 요약 (2분 규칙)
`src/*Api.ts`·`src-tauri/**` diff가 0인 것을 실측으로 확인했고, 9탭·탭별 사이드바·상태줄 구조와 확정 사항 대부분이 설계 정본대로 구현됐습니다. 다만 출시 전에 고쳐야 할 결함이 셋 있습니다. (1) 계정 메뉴에서 로그아웃하면 window 리스너가 남아, 다시 로그인할 때 로그인 버튼의 첫 클릭이 먹힙니다(재현 확인). (2) 최소 창 900px에서는 상태줄이 오른쪽 세그먼트를 잘라내, 앱에서 업데이트를 적용하는 유일한 버튼이 안 보입니다(실측). (3) 사용자가 확정한 폰트 방침("한글 섞일 가능성 있는 텍스트는 산세리프, 예외 없음")을 셀렉터 6~7곳이 어기고 있습니다.

## 페르소나 재사용 판정
착수 전 `docs/reviewer/personas/INDEX.md`(15행)의 역할개념 열을 대조했습니다. 4인 모두 기존 역할개념과 겹쳐 신규 페르소나는 0명입니다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-design-contract-auditor | 재사용 | 설계 정본 3종과 사용자 확정 조항을 조항 단위로 대조하는 것이 이 페르소나의 역할개념 그대로임 |
| persona-window-resize-and-post-interaction | 재사용 | 고정 높이 탭스트립·상태줄의 900px 잘림, 로그아웃·드롭다운 조작 이후 상태가 새 표면의 핵심 |
| persona-daily-power-operator | 재사용 | 좌측 내비 → 9탭 전환이 매일 반복하는 동선과 키보드 경로를 바꿈 |
| persona-zero-base-redesigner (발산형) | 재사용 | §3 발산형 게이트. 확정 방향은 뒤집지 않고 그 안의 구조만 봄 |

INDEX.md의 "최근 재사용" 열과 각 파일의 "적용 이력"에 이번 라운드를 추가했습니다.

## 근거 수집 방법
- 캡처: 기존 `batchD-*` 세트(00:51)는 이후 커밋 2개(`21cf18b` 00:54, `9a70996` 01:03)보다 먼저 찍혀 HEAD를 반영하지 못합니다. 그래서 HEAD에서 `pnpm dev`(1420)를 띄우고 `scripts/capture/final.mjs`로 다시 찍었습니다. 1440x900과 900x600 각 10장, pageErrors=0입니다. 결과는 `docs/screenshots/ui-terminus-review-2026-09-24/review-*.png`에 있고, 목업 비교용 `mockup-1440x900.png`도 함께 두었습니다. 캡처는 모두 Read로 직접 열어 확인했습니다.
- 동적 재현: 일회성 Playwright 스크립트로 로그아웃·재로그인, 메뉴가 열린 상태의 탭 클릭·Escape, 업데이트 배지가 뜬 상태의 상태줄 폭을 실측했습니다(스크립트는 실행 후 삭제). 증거는 `evidence-*.png` 3장입니다.
- 빌드: `pnpm build`(tsc + vite build)를 HEAD에서 실행해 통과를 확인했습니다.
- 불변식: `git diff --stat main...feat/ui-terminus -- src-tauri 'src/*Api.ts'`의 출력이 0줄이었습니다.

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| M1 | Major | 조작 이후 상태 / 파워유저 | `src/sidebar.ts:114-135`(chip·logoutItem), `:100-110`(openAccountMenu) | Read + Playwright 재현(아래 재현 절) | `logoutItem`이 계정 칩 `clickable` 요소 **안쪽 자식**이고, `clickable()`(`src/dom.ts:35-49`)은 기본적으로 전파를 막지 않습니다. 그래서 로그아웃 클릭이 칩까지 올라가고, 칩 핸들러는 `resetStateForLogout()`으로 새로 만들어진 `accountMenuOpen=false`를 보고 `openAccountMenu()`를 다시 부릅니다. 그 결과 **로그인 화면에 window `mousedown` 리스너 1개가 남습니다**. 다음 로그인 버튼 mousedown에서 `closeAccountMenu()`→`notifyChange()`가 로그인 뷰를 통째로 다시 그리면서 버튼 노드가 바뀌고, **첫 클릭이 먹힙니다**. Enter 키로 로그아웃해도 keydown이 같은 경로로 전파됩니다. | `logoutItem`을 `clickable(..., { stopPropagation: true })`로 바꿉니다(옵션은 이미 있음). 메뉴를 바깥 클릭으로 닫는 처리는 `mousedown` 대신 `click`에서 하고, 가능하면 재렌더 없이 해당 노드만 제거하게 합니다(M1-b도 함께 해소). 회귀 시나리오 "로그아웃 → 로그인 버튼 1회 클릭으로 셸 진입"을 `majorReview20260921.mjs` G1 옆에 추가합니다. 기존 G1은 `.click()`을 DOM에 직접 호출하고 재로그인은 보지 않아 이 결함을 못 잡습니다(`:375-383`). |
| M1-b | (M1에 병합) | 파워유저 | 같은 위치 | Playwright | 계정 메뉴가 열린 채로 "사용량" 탭을 1회 클릭하면 hash가 `#/` 그대로이고 메뉴만 닫힙니다. mousedown 시점의 재렌더가 탭 노드를 바꾸기 때문입니다. Escape로도 메뉴가 닫히지 않습니다. | M1 수정에 포함합니다. Escape 닫기 핸들러를 추가합니다(appLinks/projects의 ESC 등록·해제 관례). |
| M2 | Major | 창 크기 탄력성 | `src/styles.css:366-381`(`.statusline` `overflow:hidden; white-space:nowrap`), `src/sidebar.ts:556-645` | Playwright 실측 + `evidence-statusline-update-900x600.png`, `-1180x760.png` | 업데이트가 있는 상태(`UPDATER_PRESETS.available`)로 프로젝트 상세에 들어가 상태줄 좌표를 쟀습니다. **900px**: `업데이트 적용` 배지 left 883 / right 1004, `새 버전 확인` 1031, 시계 1135로 **셋 다 화면 밖**입니다. **1180px(기본 창)**: 시계가 잘립니다. 업데이트를 적용하고 확인하는 UI 진입점은 상태줄뿐입니다(`applyUpdateFromButton`/`checkForUpdateFromButton` 호출처는 `sidebar.ts:603,620` 두 곳만). 좁은 창을 쓰는 사용자는 부팅 캐리오버 경로 말고는 업데이트를 누를 방법이 없습니다. QA의 `boundary-900x600-no-horizontal-overflow`(`qaShellNavCoverage.mjs:194-216`)는 문서 `scrollWidth`만 보는데, 상태줄은 `overflow:hidden`이라 넘쳐도 수치가 늘지 않아 이 결함을 통과시킵니다. | 오른쪽 그룹을 `flex:none`으로 두고 왼쪽 세그먼트에 `min-width:0` + ellipsis를 줍니다. 공간이 모자라면 우선순위가 낮은 세그먼트(프로젝트명 → `개발 도구 확인 →` → claude/node 버전)부터 숨깁니다. 대안은 R1(업데이트 배지를 탭스트립 오른쪽으로 이동)입니다. 하네스에는 "업데이트 배지 `getBoundingClientRect().right <= innerWidth`" 단언을 900px에서 추가합니다. |
| M3 | Major | 설계 계약 | `src/styles.css:334`(`.ws-row-meta`), `:859`(`.session-row-time`), `:889`(`.task-row-schedule`), `:922`(`.task-run-time`), `:722`(`.applink-filepath`), `:1012`(`.marketplace-repo-updated`), `:539`(`.detail-path`, 한글 폴더 경로 가능) | `grep font-numeric` 전수 + DOM 문자열 대조(`appLinks.ts:337` "저장 위치:", `autonomousTasks.ts:1528` "마지막 실행 …") + 캡처 `review-1440x900__03/05/06/08` | 확정 폰트 방침(`terminus-design-system.md:87-93`: "한글이 섞일 가능성이 있는 모든 UI 텍스트 → `--font-display/body`, 예외 없음")을 어깁니다. 사이드바 메타 "20분 전"·"malgn-agent · 3분 전"·"미설치"·"중지됨", 세션 "시작 2026. 09. 23. 오후 11:55", 자율업무 "이전 실행 완료 후 1일 뒤 재실행 마지막 실행: …"이 JetBrains Mono로 렌더되고, 한글 글리프는 폴백되어 자간이 벌어져 보입니다(캡처 06의 스케줄 줄에서 뚜렷함). 이 방침은 두 번 반려된 끝에 확정된 사항이라(`:492`) 사용자 민감도가 높습니다. Windows에서 한글이 모노 스택(Consolas → 시스템 폴백)을 타면 어떤 글꼴로 나오는지는 **실기로 확인하지 못했습니다(추정)**. | 해당 셀렉터에서 `font-family`를 빼고, 순수 숫자 leaf만 `<span class="num">`으로 감싸 `--font-numeric`을 줍니다. `usage.ts`가 `bar-row-value-num`으로 이미 이렇게 처리한 선례가 있습니다. 대조용 grep 가드로 "`font-numeric` 셀렉터 목록 vs design-system §1 표"를 QA 흐름에 추가합니다. |
| m1 | Minor | 화면 정직성 / 파워유저 | `src/views/home.ts:104-106`, `:180`, `:195-200` | Read + 캡처 `review-1440x900__01-home.png` | 같은 화면이 스스로 모순됩니다. 통계 타일 "대기 중 자율 작업 **0**"은 실행중을 뺀 수이고, 큐 박스 헤더 "**1건 대기**"는 실행중을 포함한 수입니다. 행 배지는 "진행중"(cyan)인데 자율업무 목록 배지는 "실행 중"(green, 캡처 06), 사이드바 메타도 "실행 중"입니다. | 큐 헤더를 "N건(실행 a·대기 b)"로 바꾸거나 타일과 필터를 통일합니다. "실행 중" 한 단어와 한 가지 색으로 맞춥니다. |
| m2 | Minor | 설계 계약 vs 실데이터 | `src/styles.css:1259-1261`(`.blist-row--sessions` 52px, `--devtools` 60px, `--tasks` 62px) | 캡처 `01-home`(1440) | 목업 값(#2214, claude)에 맞춘 고정 열이라 실데이터가 잘립니다. 세션 ID는 "#a3…"(3글자, 식별 불가), 작업 ID는 "dail…", 도구명은 **1440px에서도** "Claude …"·"GitHub …"로 보입니다. | ID 열은 `minmax(…, max-content)`로 두거나 ID를 앞 6자로 잘라 넣고, 도구명 열은 `auto`로 둡니다. |
| m3 | Minor | 파워유저 | `src/sidebar.ts:146-177` vs 각 뷰 page-title | 캡처 전수 | 탭 라벨과 페이지 제목이 서로 다릅니다: 세션/"세션목록", 개발 도구/"개발 환경", 자율 작업/"자율업무"(버튼 "+ 새 자율업무", 홈 "자율 작업 큐"). 사이드바 헤더는 영문(`workspace ~`, `autonomy`, `dev tools`)입니다. 영문 헤더는 목업 어휘라 허용하지만, 한글 명칭 셋은 하나로 맞춰야 합니다. | 용어집을 한 줄로 정해(예: "자율 작업") 탭·제목·버튼에 일괄 적용합니다. |
| m4 | Minor | 설계 계약(기능 불변) | `src/views/appLinks.ts:277,318,329`, `main:src/sidebar.ts:230-258`, `src/main.ts:251` | Read(main 대비) | 앱 링크 퀵오픈이 "전 화면 상시 사이드바"에서 "앱 링크 탭 사이드바"로 좁아졌습니다(IA §4-8의 확정 구조라 구조 자체는 지적 대상이 아님). 그런데 화면 문구("사이드바에서 바로 열 수 있는", "사이드바에 노출할 링크를 켜세요")는 옛 의미 그대로입니다. 지금 토글은 **같은 화면 안의 옆 칸**에 보일지 말지만 정해, 문구가 약속하는 가치와 실제 동작이 다릅니다. | 문구를 새 의미에 맞게 고칩니다. 전역 퀵오픈을 되살리는 방향은 R4를 봅니다. |
| m5 | Minor | 설계 계약(문서·주석 드리프트) | `docs/design/terminus-shell-ia.md:88-89`(190/150px), `:224`(큐 필터 "running∨enabled"), `src/sidebar.ts:451`, `src/state.ts:89-93`, `src/main.ts:251` | Read | IA 문서가 확정된 220/175px와 "큐 = 전체 작업, 비활성은 흐리게"(`sidebar.ts:411-413`)로 갱신되지 않았습니다. 사이드바는 전체 작업을 보여주는데 0건 문구는 "대기 중인 자율 작업이 없습니다"입니다. `state.ts` 주석은 "하위탭은 본문 보조탭으로 이동"이라고 적어 실제 구현(사이드바로 이관)과 반대이고, `main.ts:251`은 "사이드바가 전 화면에 상시 렌더"라고 적혀 있습니다. | IA §2-3·§4-6을 갱신하고, 0건 문구를 "등록된 자율 작업이 없습니다"로 바꾸고, 주석 2곳을 고칩니다. |
| m6 | Minor | 파워유저(접근성) | `src/sidebar.ts:83-85`, `:112-143` | Read + Playwright(Escape) | 탭이 `role=button`이라 `tablist/tab`·`aria-selected`가 없습니다. 활성 탭은 시각 표시만 있고 스크린리더와 보조기술에는 전달되지 않습니다. 계정 메뉴에 Escape가 없습니다(M1-b). | 탭 컨테이너에 `role=tablist`, 탭에 `role=tab` + `aria-selected`를 줍니다(또는 최소 `aria-current="page"`). |
| m7 | Minor | 시각 일관성 | `src/styles.css:816-831`(`.home-widget*`), 캡처 `01-home`, `08-applinks` | 캡처 + design-system §4 매핑표 9번(`.home-widget`→`.card`) | 홈 하단 위젯 3종이 새 어휘(`.card`/`.box`의 ┌─ 헤더)로 옮겨지지 않아 같은 화면에서 두 시각 언어가 섞입니다. 앱 링크 박스 헤더의 "+ 링크 추가" 버튼이 제목에 붙어 헤더 높이를 벗어나고, 행 폭이 박스 폭보다 짧아 오른쪽 끝선이 어긋납니다. | 매핑표대로 `.card`로 옮기고, 헤더 버튼은 `.box-head` 오른쪽(`margin-left:auto`, `btn-sm`)에 둡니다. |
| m8 | Minor | 창 크기 탄력성 | `src/styles.css:1215`(`.stat-row` `minmax(155px,1fr)`) | 캡처 `review-900x600__01-home.png` | 900px에서 통계 타일 4개가 3+1로 줄바꿈되어 한 칸이 떨어져 보입니다. | 컨테이너 쿼리에서 2×2로 고정하거나 `minmax(130px,1fr)`로 조정합니다. |
| n1 | Nit | 시각 일관성 | `src/styles.css:400-401` + `src-tauri/tauri.conf.json:20`(minWidth 900) | Read | `@container (max-width:760px)` 규칙은 창 하한이 900이라 닿을 수 없는 죽은 규칙입니다. | 삭제하거나 주석으로 의도를 명시합니다. |
| n2 | Nit | 시각 일관성 | `src/styles.css:655`, 캡처 `07-catalog` | 캡처 | 플러그인 카드 이름 앞 "┌─" 글리프의 기준선이 제목과 어긋납니다. | `align-items:baseline` 또는 `vertical-align` 보정 |

### M1 재현 절(실측 로그)
Playwright, 스텁 `buildScenarioConfig('normal')`, window `add/removeEventListener('mousedown')` 계수:
```
A logged in; mousedown listeners = 0
B menu open; listeners = 1
C after logout; login screen = 1 listeners = 1   ← 로그인 화면인데 리스너 잔존
D 1st login click; sidebar = 0 listeners = 0      ← 첫 클릭 소실(1.5s 대기 후에도 셸 없음)
E 2nd login click; sidebar = 1                    ← 두 번째 클릭에서야 진입
```
증거 이미지: `docs/screenshots/ui-terminus-review-2026-09-24/evidence-relogin-first-click-swallowed.png`(첫 클릭 후에도 로그인 화면).

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 발산형 | 사용량 사이드바가 클릭 동작 없는 단일 항목이라 IA §2-3 원칙("빈 패널보다 `<aside>` 생략")에 어긋남 | 기각 | 사용자가 확정한 사항("사용량은 일별 사용량 1항목")이라 리뷰에서 뒤집지 않는다는 위임 조건에 해당. 구현은 확정 내용과 일치(`sidebar.ts:362-366`) |
| 파워유저 | 사용량 탭 클릭 시 `loadDailyUsage`가 onClick과 `handleNavigation` 양쪽에서 불려 이중 로드 | 기각 | `loadDailyUsage`가 `loading=true`를 동기로 먼저 세우고(`usage.ts:79-82`) 양쪽 모두 `!loading` 가드가 있음. main의 동작(`main:src/sidebar.ts:107-113`)을 그대로 이어받은 것이라 회귀도 아님 |
| 설계 계약 | 목업 상태줄의 "main ✓ clean"·"pnpm/gh 업데이트 2건"이 빠짐 | 기각 | 데이터 소스가 없어 가짜 값을 넣는 대신 정적 링크로 바꿨다고 명시함(`sidebar.ts:584-587`). 정직성 측면에서 오히려 옳음. 잘 된 점으로 옮김 |
| 설계 계약 | Windows에서 모노 스택에 한글이 섞이면 굴림 계열로 렌더된다 | 독립 지적에서 강등, M3의 "추정" 주석으로 흡수 | Windows 실기가 없어 재현하지 못함. 근거 없는 단정은 올리지 않음 |
| 발산형 | 다크 전용이고 라이트 테마가 없음 | 기각 | Terminus 방향은 사용자 확정 사항이고 스코프 밖 |

## 페르소나별 관점

### 설계 계약 감사관 — 판정: Amber
- 통과: 9탭 활성 조건이 IA §1 표와 1:1로 대응합니다(`sidebar.ts:146-177`). 사이드바 분기는 12개 kind를 모두 다루는 exhaustive switch입니다(`:522-545`). 설정 사이드바에서 devtools·applinks를 뺐고(`:510`), 자율업무 본문의 목록/진행상황판 버튼을 제거했습니다(`autonomousTasks.ts` diff에서 `tabsRow` 삭제). body 14px·사이드바 220px(`styles.css:108-109`), 로그인 화면은 셸 없이 렌더(`main.ts:93-97`), 가짜 타이틀바 미구현도 모두 확정 사항대로입니다.
- 위반: M3(폰트 방침), m2(목업 고정 폭을 실데이터에 그대로 씀), m4·m5(문구·문서 드리프트).
- 불변식 "API·Rust diff 0" 실측 통과.

### 창 크기 탄력성 + 조작 이후 상태 — 판정: Amber
- M2: 900px에서 업데이트 진입점이 사라집니다. 기존 QA 경계 시나리오의 측정 방식에 맹점이 있습니다(`overflow:hidden` 영역은 `scrollWidth`에 잡히지 않음).
- M1: 로그아웃 → 재로그인에서 첫 클릭이 먹힙니다. 드롭다운을 자식으로 품은 clickable에서 이벤트가 전파되는 문제입니다.
- 통과: 탭스트립 드롭다운 잘림 회귀(9a70996) 수정은 `.tabstrip-tabs` 분리로 맞게 됐습니다(`styles.css:220-240`). 900x600 캡처 10장 모두 탭 9개와 계정 칩이 가로 스크롤 없이 들어갑니다.

### 매일 쓰는 파워유저 — 판정: Amber
- 메뉴가 열린 상태에서 탭 클릭이 먹히고 Escape가 없습니다(M1-b). 숫자 모순(m1)과 용어 불일치(m3)는 매일 보는 홈 화면에서 신뢰를 깎습니다.
- 앱 링크 퀵오픈은 이전에도 "그룹 펼침 → 클릭" 2동작이었고 지금은 "탭 → 클릭" 2동작이라 클릭 수 손실은 없습니다. 다만 목적지 화면이 바뀌어 작업 맥락이 끊깁니다(R4).

### 제로베이스 재설계자(발산형) — 판정: 해당 없음(Rethink 전용)
- 아래 R1~R4. 확정 사항은 건드리지 않았습니다.

## 구조적 제언 (Rethink) — 발산형 페르소나

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R1 | 업데이트 적용·새 버전 확인·앱 버전이 하단 상태줄 오른쪽에 있고, 상태줄은 `overflow:hidden`(IA 3필드도 "상태줄 동선 없음, 업데이트 배지만 예외") | 업데이트 배지와 "새 버전 확인"을 **탭스트립 오른쪽, 계정 칩 옆**으로 옮기고 상태줄은 수동 정보 전용으로 둠 | 탭스트립 오른쪽은 이미 `flex:none` 고정 슬롯이라 잘리지 않음. 예외가 없어져 M2가 구조적으로 사라지고, "창 수준 상태는 창 크롬에"라는 데스크톱 관례와도 맞음 | 낮음. `renderStatusline`의 두 블록을 `renderTabstrip`으로 옮기면 됨. ui-harness가 클래스명으로 조회하므로 클래스는 유지 |
| R2 | "개발 도구"·"앱 링크" 탭은 최상위지만 라우트는 `#/settings/devtools`·`#/settings/applinks`. 그래서 설정 탭 활성 판정이 "settings이면서 두 탭은 아님"(`sidebar.ts:175`)이고, 사이드바 분기도 settings 안에서 다시 갈라짐(`:534-537`) | `#/devtools`, `#/applinks`로 라우트를 승격하고 옛 해시는 `parseRoute`에서 리다이렉트 | 시각 IA와 URL IA가 일치함. 설정 탭에 새 하위탭을 추가할 때 "최상위로 승격된 두 예외"를 기억할 필요가 없음. `leaveAppLinksView` 조건(`main.ts`)도 단순해짐 | 낮음~중. 라우트 2개 추가 + 리다이렉트, 하네스 해시 갱신 |
| R3 | 프로젝트 탭에서 사이드바(워크스페이스 목록 4행)와 본문(같은 4개의 카드 그리드)이 **같은 목록을 두 번** 보여줌(캡처 02) | 사이드바를 목록 겸 선택기로 두고 본문은 **선택된 프로젝트 상세**(마스터-디테일, VS Code 탐색기 패턴)로 보여줌. 필터(전체/진행/보관)·정렬은 사이드바 헤더로 이동 | IA가 참고한 "액티비티바 → 사이드바 패널" 패턴을 끝까지 적용하면 목록 화면 자체가 필요 없음. 클릭 한 번 절약, 중복 제거 | 중. `projects-list` 라우트의 본문 역할을 재정의해야 함(편집 모달 진입 경로 유지 필요). IA 확정("리스트 채택")과 충돌하지 않고 본문만 바꾸는 안 |
| R4 | 앱 링크 퀵오픈이 앱 링크 탭 안에서만 가능(m4) | 전역 **명령 팔레트(Cmd/Ctrl+K)**: 9탭 이동, 활성 앱 링크 열기, 최근 세션·프로젝트 점프. "사이드바 노출" 토글은 "팔레트 노출"로 의미를 바꿈 | 터미널 에뮬레이터 어휘(Tabby·VS Code)와 맞고, 전 화면 퀵오픈을 좁은 폭 부담 없이 되살림. 파워유저 키보드 동선이 생김 | 중. 신규 컴포넌트 1개, API 변경 없음. 이번 Refactor 범위 밖이니 차기 버전 후보로 둠 |

## 트레이드오프 (페르소나 간 충돌)
- **목업 픽셀 충실도 vs 실데이터 적합성(m2)**: 설계 계약 관점에서는 목업 열 폭을 그대로 쓰는 것이 "준수"이지만, 파워유저 관점에서는 세션 ID가 3글자로 잘리면 기능을 잃습니다. → 권고: 실데이터 쪽. 목업이 증명한 것은 비율과 어휘이지 px 값이 아닙니다(design-system 자체도 "정확한 px"를 IA 범위 밖으로 둠).
- **터미널 미학 vs 확정 폰트 방침(M3)**: 메타 줄을 모노로 두면 "터미널다움"은 커지지만, 사용자가 두 번 반려한 끝에 확정한 방침과 충돌합니다. → 권고: 방침 쪽. 숫자 leaf만 모노로 감싸도 터미널 인상은 충분히 유지됩니다(홈 통계 타일이 그 증거).

## 잘 된 점
- **불변식 준수**: API·Rust diff 0(실측). 34파일 개편이 뷰·스타일·하네스 안에서 끝났습니다.
- **시계가 전역 재렌더를 일으키지 않음**: 매초 `notifyChange()` 대신 마지막 노드의 텍스트만 바꾸고 인터벌은 1개입니다(`sidebar.ts:54-74`). v0.2.5 리뷰의 "전역 재렌더 계약" 교훈을 적용했습니다.
- **가짜 데이터 거부**: 목업의 "pnpm/gh 업데이트 2건"을 조회할 방법이 없자 정적 링크로 바꾸고 사유를 주석에 남겼습니다(`sidebar.ts:584-587`).
- **하네스 호환 유지**: 이동한 요소의 클래스명(`sidebar-logout`, `sidebar-update-item`, `sidebar-version-check-btn`)을 일부러 보존하고 이유를 적었습니다(`:593-612`).
- **exhaustive switch**와 IA 절 번호가 달린 주석 덕분에 설계 추적성이 높습니다.
- **폰트 번들**: JetBrains Mono latin 서브셋 2종 + OFL.txt를 로컬 번들해 CSP `default-src 'self'`를 건드리지 않았습니다.
- **회귀 수정의 원인 기록**: 탭스트립 overflow-x가 overflow-y를 auto로 끌어올리는 CSS 스펙 함정을 주석으로 남겼습니다(`styles.css:229-232`).
- 캡처 20장 모두 pageErrors 0, `pnpm build` 통과.

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| `src/*Api.ts`·`src-tauri/**` diff 0 | 설계 계약 | 필수 | 충족 | 실측 0줄 |
| 9탭 활성 조건 = IA §1 | 설계 계약 | 필수 | 충족 | `sidebar.ts:146-177` |
| 탭별 맥락 사이드바 = IA §4 | 설계 계약 | 필수 | 대부분 충족 | 큐 필터는 PM 결정으로 변경됐는데 문서 미갱신(m5) |
| 폰트 방침(한글 → 산세리프) | 설계 계약 | 필수 | **미충족** | M3 |
| body 14px / 사이드바 220px | 설계 계약 | 필수 | 충족 | `styles.css:108-109` |
| 900px에서 모든 조작 도달 가능 | 창 탄력성 | 필수 | **미충족** | M2 |
| 조작 이후 상태 정상(로그아웃 → 재로그인) | 조작 이후 | 필수 | **미충족** | M1 |
| 키보드 도달성 | 파워유저 | 권장 | 부분 | clickable로 Tab·Enter 가능, Escape와 ARIA 역할 부족(m6) |
| 동일 화면 수치 일관성 | 파워유저 | 권장 | 미충족 | m1 |

## 생략함 / 확인하지 못한 것
- **실기(실제 Tauri 창, Windows·macOS)**: 확인하지 못했습니다. 모든 화면 근거는 Vite dev + Tauri 스텁(Chromium)입니다. WebView2의 폰트 폴백, DPI 스케일링 시 상태줄 폭은 실기 확인이 필요합니다.
- **캡처 범위**: 9탭 기본 라우트, 로그인, 증거 3장만 찍었습니다. 세션 상세(채팅)·draft·자율업무 상세·진행상황판·각종 모달·에러/빈/로딩 시나리오는 이번에 따로 캡처하지 않았습니다. QA 하네스 74시나리오 결과(`scripts/ui-harness/last-run-report.json`)를 참고했을 뿐 재실행하지는 않았습니다.
- **색 대비 재측정**: 하지 않았습니다(design-system의 AA 수치 주장을 검증 없이 그대로 두었음).
- 캡처 원본은 `scripts/capture/output/review-1440x900/`, `review-900x600/`에도 남아 있습니다.

## 실행 액션
코드 수정·커밋·push·배포는 하지 않았습니다. 새로 만들었거나 고친 파일은 이 보고서, `docs/screenshots/ui-terminus-review-2026-09-24/`(캡처 24장), 페르소나 4개 파일의 적용 이력 append, `docs/reviewer/personas/INDEX.md` 갱신뿐이며, 모두 미커밋 상태입니다.

## PM에게 권고
1. **출시 전 필수(Major 3)**: M1(전파 차단 한 줄 + Escape + 회귀 시나리오), M2(상태줄 우선순위 레이아웃, 또는 R1으로 배지를 탭스트립으로 이동. R1이 근본책), M3(셀렉터 6~7곳 폰트 정리). 세 건 모두 frontend-dev가 처리할 수 있고 API 변경은 없습니다.
2. 고친 뒤에는 **축소 재검토**(target_id `ui-terminus-overhaul`, 2차)로 M1~M3의 해소만 확인하면 됩니다. 이때 재현 스크립트 세 개(재로그인 첫 클릭, 900px 배지 좌표, font-numeric grep)를 QA 하네스에 넣어 두면 재검토 비용이 거의 들지 않습니다.
3. **Windows 실기 1회**: 전 사용자 자동 업데이트 대상이므로, 머지 전에 Windows에서 M3 폰트 렌더와 상태줄 폭을 최소 1회 눈으로 확인하길 권합니다(release 전 사람 확인 항목).
4. Minor(m1~m8)는 같은 PR에 넣어도 되고 백로그로 넘겨도 됩니다. m1·m3(홈 숫자 모순, 용어 통일)은 비용이 작으니(문자열·필터 몇 줄, 코드 확인함) 같은 PR을 권합니다.
5. R1은 M2의 근본 해결이므로 이번에 채택을 검토하길 권합니다. R2~R4는 차기 방향을 정할 때 판단하면 됩니다.
