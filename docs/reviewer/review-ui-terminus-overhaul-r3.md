# Terminus UI 전면 개편 리뷰 보고서 — 3차(축소 재검토)

리뷰 대상: target_id `ui-terminus-overhaul` — 브랜치 `feat/ui-terminus` HEAD `1556b5d` (검토 범위 `git diff a30064f..1556b5d`, 사이 커밋 `a30064f`은 리뷰 문서뿐)
종합 판정: **Amber (조건부 통과)** — Critical 0 / Major 1(M4, 이번 커밋이 새로 만든 회귀) / Minor 0 / Nit 3 / Rethink 1
직전 미해결 Major M3-r: **해소**

리뷰 페르소나 패널(4인, 모두 재사용): `docs/reviewer/personas/persona-design-contract-auditor.md`, `persona-daily-power-operator.md`, `persona-window-resize-and-post-interaction.md`, `persona-zero-base-redesigner.md`(발산형)
리스크 범주: 전 화면 UI·내비게이션 구조 변경(배포되면 전 사용자에게 자동 업데이트로 나감). 직전 라운드와 같음
등급/모드: Refactor, 3차 축소 재검토(C). 모드 판정은 아래에 적었습니다.
리뷰 일자: 2026-09-24

## 요약
**M3-r(한글이 모노 폰트로 렌더되는 2곳)은 해소됐습니다.** 구현자가 추가한 스캔 흐름을 수정 전 트리(`a30064f`)에서 돌리면 bugs=2가 나오고, HEAD에서는 0입니다. 전체 80개 시나리오는 bugs=0이고 cargo는 543 passed입니다. 세 수치 모두 제가 직접 다시 돌려 구현자 주장과 일치함을 확인했습니다. PM이 판정을 요청한 **접두 불일치 폴백은 방침을 어기지 않습니다.** 렌더 쪽 폴백은 실제로는 도달할 수 없는 코드입니다(근거는 아래 "폴백 판정" 절).

**다만 m10 수정이 새 회귀를 하나 만들었습니다(M4).** 이제 계정 메뉴를 열면 포커스가 곧바로 "로그아웃" 항목으로 갑니다. 그래서 계정 칩에서 Enter나 Space를 **길게 누르면** 키 반복으로 들어오는 두 번째 keydown이 로그아웃을 실행합니다. 그러면 앱 상태 전체(세션 채팅 제외)가 초기화되고 로그인 화면으로 돌아갑니다. 수정 전 트리에서는 같은 조작을 해도 로그아웃되지 않는 것을 확인했습니다. 한 줄이면 고칠 수 있습니다.

## 모드 판정 (재검토 3요소·동일 대상 조건)
- 재검토 3요소(target_id, 직전 리뷰 `review-ui-terminus-overhaul-r2.md`, 리스크 범주)가 위임 메시지에 모두 있습니다.
- 풀패널 강제 승격 조건을 대조했습니다. 직전 미해결 Major(M3-r)는 이번 라운드가 해소를 확인하는 대상 자체입니다. 수정 내용은 CSS, 뷰 문구, 하네스, 포커스 호출 몇 줄이라 새 Sensitive 하위도메인으로 들어가지 않습니다. `src-tauri/**` diff도 0입니다(`git diff --stat a30064f..1556b5d -- src-tauri` 출력 없음).
- 동일 대상 4조건(target_id 같음, 같은 날, 리스크 범주 같음, `src/sidebar.ts`·`src/styles.css`·views 파일 겹침)을 모두 충족합니다.
- 새 리스크 표면은 m10의 프로그래매틱 `focus()` 하나뿐이고, 기존 "조작 이후 상태·키보드 경로" 표면 안에 있습니다. 그래서 **축소(C)**로 판정했습니다. 다만 그 지점은 직접 프로브를 돌려 확인했고, 그 과정에서 M4를 찾았습니다.

## 페르소나 재사용 판정
착수 전에 `docs/reviewer/personas/INDEX.md`(15행)의 역할개념 열을 대조했습니다. 신규 페르소나는 0명입니다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-design-contract-auditor | 재사용 | M3-r 해소와 design-system §1 표 개정을 조항 단위로 대조하는 일이라 역할개념 그대로입니다 |
| persona-daily-power-operator | 재사용 | m10 포커스 관리가 키보드 경로를 바꿨습니다(M4 발견) |
| persona-window-resize-and-post-interaction | 재사용 | n3 mask 페이드(창 폭)와 메뉴 조작 뒤의 포커스 상태가 이 페르소나의 두 축입니다 |
| persona-zero-base-redesigner (발산형) | 재사용 | 발산형 게이트입니다. 한국어 문장 접두를 문자열 계약으로 쓰는 구조를 판정합니다(R6) |

INDEX.md의 "최근 재사용" 열과 4개 파일의 "적용 이력"에 이번 라운드를 덧붙였습니다.

## 근거 수집 방법
- **빌드**: HEAD에서 `pnpm build`(`tsc && vite build`)가 통과했습니다.
- **하네스 전체**: HEAD에서 `pnpm dev`(1420 포트)를 띄우고 `node scripts/ui-harness/run.mjs`를 돌렸습니다. **80개 시나리오, bugs=0, consoleErrors=0**으로 구현자 주장과 같습니다. 이 실행으로 `scripts/ui-harness/last-run-report.json`이 다시 써졌습니다(gitignore 대상이라 git status에는 나타나지 않음).
- **수정 전 실패 재현**: `git archive a30064f`로 푼 트리에 HEAD 하네스를 얹고 1430 포트에서 `fontPolicyKoreanMonoScan`만 돌렸습니다. 결과는 **bugs=2**입니다. `#/usage` → `stat-value :: 3일`, 확인 패널 → `devtool-panel-command :: 다음 명령을 실행합니다: brew install --cask claude`. HEAD에서는 0입니다.
- **cargo**: `cargo test` 결과 `543 passed; 0 failed; 17 ignored`로 구현자 주장과 같습니다. Rust diff가 0이라 이번 커밋의 영향을 검증하는 의미는 없고, 수치만 확인한 것입니다.
- **독립 프로브**: 스크래치패드에만 둔 흐름 `probeR3`입니다(저장소에 넣지 않음). 다음을 HEAD(1420)와 수정 전 트리(1430) 양쪽에서 실측했습니다.
  - 키보드로 열기 → Escape → 다시 열기 → 탭 Enter
  - 마우스로 열기 → 바깥 클릭
  - 칩에서 Enter·Space 길게 누르기
  - 사용량 타일과 확인 패널의 computed font
  - 상태줄 1440px과 900px 최악 조합
- **캡처**: `docs/screenshots/ui-terminus-review-r3-2026-09-24/r3-*.png` 7장. 모두 Read로 직접 열어 확인했습니다.

## 직전 지적 해소 현황

| # | 직전 심각도 | 상태 | 확인방법 · 근거 |
|---|---|---|---|
| M3-r ① | Major | **해소** | `usage.ts:265` `{ value: String(t.activeDays), unit: '일' }` + `statTile`이 `<small>`로 분리합니다(`usage.ts:36`). `styles.css:1280` `.stat-value small { font-family: var(--font-body) }`. 실측 computed font: 값은 JetBrains Mono, `small`은 -apple-system입니다. 캡처 `r3-1280x820-usage-stat-tiles.png`에서 "3 일"로 보이고, 단위는 작은 산세리프입니다. |
| M3-r ② | Major | **해소** | `devTools.ts:582-588`. 접두는 `.devtool-panel-label`(body)로, 명령은 `.devtool-panel-command`(mono)로 나눴습니다. 실측: `label="다음 명령을 실행합니다:"` -apple-system, `command="brew install --cask claude"` JetBrains Mono. 캡처 `r3-1280x820-devtool-manual-confirm-panel.png`. |
| 재발 방지 | (권고) | **반영** | `scripts/ui-harness/flows/fontPolicyKoreanMonoScan.mjs`. 10개 라우트와 **조작 뒤에만 나타나는 확인 패널까지** 스캔합니다. 직전 라운드에서 제가 코드로만 확인했던 지점을 실제 렌더로 덮었습니다. 수정 전 트리에서 2건을 잡아내므로 결함 검출력도 확인됐습니다. |
| m9 | Minor | **해소** | `styles.css:1001` `.devtool-binary`를 body로 바꿨고, design-system §1 표에서 "경로(한글 폴더명 가능)"를 body 행으로 옮겼습니다. 경로 규칙이 하나로 통일됐습니다. |
| m10 | Minor | **해소, 단 부작용 M4** | `sidebar.ts:133-147, 170-172, 93-96`. 실측 결과(HEAD): 키보드로 열면 activeElement가 로그아웃 항목이고, Esc를 누르면 칩으로 돌아가며, 탭에서 Enter를 누르면 메뉴가 닫히고 `#/projects`로 이동합니다. 수정 전에는 모두 BODY였고, 탭 Enter 뒤에도 메뉴가 남았습니다. 캡처 `r3-1280x820-kbd-focus-01-kbd-open.png`에 포커스 링이 보입니다. |
| n3 | Nit | **해소** | `styles.css:396-397` mask 페이드. 900px 최악 조합에서 프로젝트명 끝이 페이드됩니다(`r3-900x600-statusline-worst-mask.png`). 1440px처럼 넘치지 않을 때는 `.statusline-left`가 flex:1이라 끝 24px이 빈 공간에 걸려 텍스트에 영향이 없습니다(실측: 마지막 세그먼트 right=558, 컨테이너 right=1195, `r3-1440x900-statusline-mask-no-overflow.png`). |
| n4 | Nit | 미처리 | 주석과 CSS 불일치(`margin-left:auto` 문구). 이번 커밋에 해당 변경이 없어 백로그로 유지합니다. |
| n5 | Nit | **해소** | `home.ts:156` "실행 중", `projects.ts:211` "자율 작업", `devTools.ts:70` "개발 도구". diff로 확인했습니다. |
| n6 | Nit | **해소** | `bridge.mjs` 리스너 카운터가 `Map<type, Set<fn>>`으로 바뀌었습니다. 등록되지 않은 핸들러를 제거해도 수가 줄지 않습니다. 이 변경 뒤에도 `accountMenuAndStatusline` 5개 시나리오가 bugs=0입니다. |
| n7 | Nit | 미처리 | 드롭다운 내부 이메일 라벨을 클릭하면 메뉴가 닫힙니다. 백로그로 유지합니다. |

## 폴백 판정 (PM 요청 — `devTools.ts:578-591`)
**판정: 방침 위반 없음. 렌더 쪽 폴백은 도달할 수 없는 코드입니다.**
- `manualPendingCommand[tool.id]`는 `handleRequestManualConfirm`에서 **`preview.message.startsWith(MANUAL_PREVIEW_PREFIX)`일 때만** 값이 들어갑니다(`devTools.ts:255-256`). 접두가 다르면 확인 패널을 만들지 않고 `showToast`로 보냅니다(`:259`). 토스트는 body 폰트입니다. 그래서 `renderManualPanel`이 받는 `pending`은 언제나 같은 접두로 시작하고, `:584`의 `: pending.message` 분기는 실행되지 않습니다.
- Rust 문구가 바뀌어 접두가 어긋나도 한글이 모노로 렌더되지 않습니다. 대신 기능이 조용히 바뀝니다. 확인 패널과 [실행] 버튼이 사라지고 "다음 명령을…" 토스트만 뜹니다. 이 결합은 이번 커밋이 만든 것이 아니라 기존부터 있던 것입니다(`:255`는 2차 이전부터 존재). Rust 쪽에 이 문구를 고정하는 테스트는 없습니다(`grep -rn "다음 명령을 실행합니다" src-tauri/src`의 적중이 `actions.rs:431` 1건뿐). → R6, n8
- 명령 부분의 원천도 확인했습니다. `copyable_command`는 `&'static str` ASCII 리터럴입니다(`plan_table.rs:519-630`). Run 경로의 `command_display`는 `build_command_display`가 러너 **파일명만** 씁니다(`install_resolver.rs:293-301`, `file_name()`). 그래서 Windows 한글 사용자명이 경로로 섞여 들어오지 않습니다. 명령 박스를 모노로 두는 것은 방침의 취지(한글이 섞이지 않는 데이터만 모노)에 맞습니다.
- 문서 쪽 빈틈이 하나 있습니다(n9). 개정된 §1 표의 모노 행은 "숫자·버전·해시/ID·시계"만 열거하고 "명령(ASCII 고정 문자열)"은 없습니다. 구현은 취지에 맞지만 표의 문언과는 어긋납니다.

## 지적 사항 (통합, 이번 라운드)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| M4 | **Major(신규 회귀)** | 파워유저·조작 이후 | `src/sidebar.ts:172`(열자마자 `.tabstrip-account-dropdown-item` = 로그아웃 항목에 `focus()`) + `src/dom.ts:42-47`(`clickable` keydown이 `e.repeat`를 구분하지 않음) + `src/sidebar.ts:194-203`(로그아웃은 확인 없이 `resetStateForLogout()`, `state.ts:517-523` sessionChat 외 전 상태 초기화) | 프로브 실측(Playwright `keyboard.down` 두 번 = OS 키 반복의 `repeat=true` keydown). HEAD: 칩에서 Enter를 길게 누르면 `loginScreen=true`, Space도 `loginScreen=true`. **수정 전 트리(`a30064f`)**: 둘 다 `loginScreen=false, open=true`. 캡처 `r3-1280x820-hold-enter-on-chip-logged-out.png` | 키보드로 칩을 열 때 키를 조금만 길게 눌러도(OS 키 반복 지연을 넘기면) 첫 keydown이 메뉴를 열고 포커스를 로그아웃으로 옮깁니다. 이어지는 반복 keydown이 그대로 로그아웃을 실행합니다. 마우스로 열 때도 포커스가 로그아웃에 놓이는데, **포커스 링이 보이지 않습니다**(`r3-1280x820-kbd-focus-02-mouse-open-focus.png`, `:focus-visible` 휴리스틱). 그래서 사용자가 모르는 상태에서 Enter나 Space를 누르면 로그아웃됩니다. 로그아웃하면 편집 중이던 폼 등 앱 상태가 초기화되고 Google OAuth를 다시 거쳐야 합니다. 이번 커밋 전에는 없던 동작이고 전 사용자에게 자동 업데이트로 나갑니다. 발생 빈도는 낮지만 **새 회귀이고, 위험(`danger`) 항목이 의도 없이 실행되며, 고치는 비용이 한 줄**이라 Major로 둡니다. 편집 중인 폼이 실제로 사라지는지는 코드로만 추정했고 실측하지 않았습니다. | (필수) `clickable`의 keydown에 `if (e.repeat) return;`을 넣습니다. 모든 clickable의 키 반복 토글을 막는 범용 방어라 부작용이 작습니다. (권장) 열었을 때 포커스를 위험 항목이 아니라 드롭다운 컨테이너(`tabindex=-1`, `role=menu`)나 이메일 라벨에 두고, Tab이나 화살표로 로그아웃에 도달하게 합니다. 회귀 흐름에는 "칩에서 Enter·Space 길게 누름 → 로그인 화면이 아님"을 추가합니다(위 프로브를 그대로 옮기면 됩니다). |
| n8 | Nit | 설계 계약 | `src/views/devTools.ts:582-584` | Read | 도달할 수 없는 폴백입니다. 게다가 만약 도달하면 라벨에 접두가 나오고, 명령 박스(mono)에 접두를 포함한 원문이 **중복으로** 들어갑니다. 지금은 해가 없지만 `:255` 가드가 바뀌면 곧바로 M3 부류가 재발합니다. | 폴백일 때는 라벨 없이 원문 전체를 body 요소(`devtool-panel-notes` 등)에 두거나, 가드와 렌더가 같은 파싱 함수 하나를 쓰도록 합칩니다. |
| n9 | Nit | 문서 드리프트 | `docs/design/terminus-design-system.md` §1 표(모노 행) vs `styles.css:1024-1026`(`.devtool-panel-command` mono), `:1028`(`.devtool-affected-list` mono) | Read | 모노 허용 목록에 "명령(ASCII 고정 문자열)·패키지명"이 없어 표의 문언과 구현이 맞지 않습니다. 이 두 셀렉터가 다룰 데이터는 한글이 섞이지 않는 원천이라 실제 위반은 없습니다. | 표의 모노 행 예시에 "앱이 조합한 명령(러너 파일명 + 검증된 인자)·패키지 식별자"를 추가하거나, R5 결론에 맞춰 정리합니다. |
| n10 | Nit(기존, 이번 회귀 아님) | 파워유저 | `src/sidebar.ts:93-96` → `navigate()` 재렌더 | 프로브. HEAD와 수정 전 모두 탭 Enter 뒤 `activeElement=BODY` | 키보드로 탭을 활성화하면 재렌더로 포커스가 body로 떨어집니다. m10과 같은 부류지만 탭 경로라 이번 수정 범위 밖입니다. | 백로그입니다. 렌더 뒤 활성 탭(`.tab.active`)으로 포커스를 복원합니다. |

## 구조적 제언 (Rethink) — 발산형 페르소나

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R6 | Rust가 사람이 읽을 한국어 문장("다음 명령을 실행합니다: {cmd}")을 만들고, 프론트가 그 문장의 **접두 문자열**로 ① 확인 단계로 갈지 판정하고(`:255`) ② 라벨과 명령을 쪼갭니다(`:582`). 문구를 한 글자만 바꿔도 기능 분기와 폰트 분리가 함께 조용히 깨집니다. | 반환에 구조화 필드를 둡니다(예: `TerminalLaunchResult { opened, message, command: Option<String> }`). 프론트는 `command`가 있으면 확인 패널, 없으면 토스트로 분기하고, 라벨 문구는 프론트가 소유합니다. | 문장이 계약에서 빠지므로 번역이나 문구 수정이 기능을 깨지 않습니다. 폰트 분리도 파싱 없이 "명령 필드 = mono"로 정해집니다. | 낮음. 필드 1개 추가(`Option`이라 하위 호환)와 프론트 분기 2곳입니다. 다만 CLAUDE.md의 "`*Api.ts`·Rust 계약 불변" 제약이 있는 이번 PR 범위 밖이라 백로그로 둡니다(미구현 추정치). |

R5(폰트 규칙 단위, 로그 예외)는 PM이 "데이터 종류 기준"으로 결정해 design-system §1 표에 반영했습니다. 그래서 규칙 단위 문제는 종결로 봅니다. 로그 영역 예외(`--font-code`)는 채택하지 않았고, `.devtool-log`는 body 그대로입니다. 결정대로 된 것을 확인했습니다.

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 설계 계약 | 확인 패널의 명령 박스가 여전히 모노라 "숫자·버전·해시/ID·시계만" 결정을 어긴다 | 결함에서 기각하고 문서 드리프트(n9)로 강등 | 원천이 ASCII 고정 리터럴과 러너 파일명뿐이라 한글이 들어올 경로가 없습니다(`plan_table.rs`, `install_resolver.rs:293-301`). 결정의 목적(한글 모노 폴백 방지)은 충족됩니다. |
| 설계 계약 | Run 경로 `command_display`에 Windows 한글 사용자 경로가 섞여 모노로 렌더된다 | 기각 | `build_command_display`가 `Path::file_name()`만 씁니다. 인자는 `validate_argv_token`을 통과한 토큰입니다. |
| 창 탄력성 | `.statusline-left` mask가 넘치지 않을 때도 마지막 세그먼트를 흐리게 한다 | 기각 | 컨테이너가 flex:1이라 끝 24px은 빈 공간입니다. 1440px 실측에서 마지막 세그먼트 right=558, 컨테이너 right=1195입니다. |
| 파워유저 | 탭 `onActivate`에서 `closeAccountMenu()`를 먼저 부르면 마우스 클릭 때 window 바깥클릭 핸들러와 이중으로 닫힌다 | 기각 | `closeAccountMenu`가 dispatch 도중 window 리스너를 제거하므로 같은 이벤트에서 발화하지 않습니다(DOM 사양). 하네스 `menu-open-tab-click-not-swallowed`가 bugs=0입니다. |
| 테스트 신뢰성 | 스캔 흐름이 스텁 데이터(ASCII 위주)만 보므로 실제 한글 데이터가 흘러드는 필드를 놓친다 | 결함 아님(한계로 "생략함"에 기재) | 스캔 방식 자체는 맞습니다. 데이터 의존 누락은 셀렉터 원천 확인(위 폴백 판정 절)으로 보완했습니다. |

## 트레이드오프 (페르소나 간 충돌)
- **키보드 접근성(m10: 열면 첫 항목에 포커스) vs 위험 조작 분리(M4)**: WAI-ARIA 메뉴 패턴은 "열면 첫 항목에 포커스"가 표준입니다. 하지만 이 메뉴에서 조작 가능한 항목은 로그아웃(danger) 하나뿐이라 "첫 항목"이 곧 "위험 항목"입니다. → 권고: 키 반복 방어(`e.repeat`)는 필수로 하고, 포커스 대상은 컨테이너로 옮기는 것을 권장합니다. 접근성 이득(포커스 유실 해소)은 그대로 유지됩니다.

## 잘 된 점
- **스캔 흐름이 직전 라운드의 사각지대를 메웠습니다**: 확인 패널은 조작한 뒤에만 나타나서 제가 직전 라운드에 코드로만 확인했던 지점입니다. 이번 흐름은 실제로 패널을 열고 스캔합니다(`fontPolicyKoreanMonoScan.mjs:77-106`). 패널이 열리지 않으면 "스캔 대상 없음"을 버그로 올리는 가드도 있어, 거짓 통과를 막습니다.
- **검출력을 증명했습니다**: 수정 전 트리에서 2건 실패, HEAD에서 0건. 제가 독립적으로 재현했습니다.
- **n6 수정이 정확합니다**: 카운터가 등록된 함수 집합의 크기라, 직전에 지적한 "등록 없이 remove하면 상쇄되는" 거짓 통과가 구조적으로 사라졌습니다.
- **변경 범위를 지켰습니다**: Rust·API diff 0, 기존 79개 시나리오 무결함 유지. 방침 개정을 design-system 정본 표에 먼저 반영했습니다.
- **m10 포커스 복귀 조건이 정교합니다**: "포커스가 칩 안에 있었을 때만 되돌린다"(`sidebar.ts:135`)는 조건이라 마우스로 바깥을 클릭해 닫을 때는 포커스를 빼앗지 않습니다(실측: 바깥 클릭 후 `BODY`, Esc 후 칩).

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| `src/*Api.ts`·`src-tauri/**` diff 0 | 설계 계약 | 필수 | 충족 | 실측 0줄 |
| 폰트 방침(한글 → 산세리프) | 설계 계약 | 필수 | **충족** | 스캔 0건, 수정 전 2건 |
| 900px에서 모든 조작 도달 가능 | 창 탄력성 | 필수 | 충족 | 최악 조합에서 right 그룹 527~890 |
| 조작 이후 상태 정상 | 조작 이후 | 필수 | **미충족** | M4(키 반복 → 의도하지 않은 로그아웃) |
| 키보드 도달성 | 파워유저 | 권장 | 대부분 충족 | m10 해소, n10 잔존 |

## 생략함 / 확인하지 못한 것
- **실기(Tauri 창, Windows·macOS)**: 확인하지 못했습니다. 모든 근거는 Vite dev와 하네스 스텁(Chromium)입니다. 특히 M4의 키 반복 지연은 OS 설정에 따라 다르고, 실기 WebView(WKWebView·WebView2)가 `repeat` keydown을 보내는 방식도 실측하지 않았습니다. Playwright의 `repeat=true` keydown은 브라우저 표준 동작을 재현한 것입니다.
- **로그아웃으로 편집 중 폼이 사라지는지**: `resetStateForLogout` 코드로만 추정했습니다.
- **스캔 흐름의 데이터 의존 한계**: 스텁 데이터에 없는 한글이 실데이터로 들어오는 필드는 스캔이 잡지 못합니다. 모노 셀렉터 18개 중 이번 diff와 관련된 것(`.devtool-panel-command`, `.devtool-affected-list`, `.stat-value`)만 원천을 확인했고, 나머지는 직전 라운드 판단을 그대로 따랐습니다.
- **색 대비와 n1·n2**: 이번에도 재측정하지 않았습니다.

## 실행 액션
코드 수정, 커밋, push, 배포는 하지 않았습니다. 이번에 만들었거나 고친 파일은 다음이고, 모두 미커밋입니다.
- 이 보고서
- `docs/screenshots/ui-terminus-review-r3-2026-09-24/`(7장)
- 페르소나 4개 파일의 적용 이력 append
- `docs/reviewer/personas/INDEX.md`

부수 효과로 `scripts/ui-harness/last-run-report.json`이 HEAD 기준 80개 시나리오 결과로 다시 써졌습니다. 프로브, 수정 전 트리, dev 서버(1420·1430)는 스크래치패드에서만 썼고 모두 종료했습니다.
참고로, 작업 트리에는 제가 만들지 않은 미커밋 변경이 있습니다(`package.json`, `src-tauri/Cargo.toml`, `Cargo.lock`, `tauri.conf.json`의 버전 0.2.14→0.3.0). 다른 세션의 작업으로 보이며 건드리지 않았습니다.

## PM에게 권고
1. **머지 전 필수(Major 1)**: M4입니다. `src/dom.ts` clickable keydown에 `if (e.repeat) return;` 한 줄을 넣습니다(필수). 드롭다운을 열 때의 포커스 대상을 컨테이너로 옮기는 것은 권장입니다. 회귀 흐름에 "칩에서 Enter·Space 길게 누름 → 로그아웃되지 않음"을 추가합니다.
2. 수정 뒤에는 **4차 축소 재검토(C)**로 충분합니다. 확인할 것은 키 반복 프로브 2건과 전체 하네스 bugs=0입니다.
3. n8·n9는 같은 PR에 넣어도 되고, 각각 몇 줄 수준입니다(코드와 문서를 확인함). n4·n7·n10은 백로그로 둡니다.
4. R6(Rust 문장 접두 계약을 구조화 필드로 교체)은 계약 불변 제약 때문에 이번 범위 밖이라 백로그로 둡니다.
5. 직전 권고인 **Windows 실기 1회 육안 확인**(폰트, 상태줄 폭, 이번 M4의 키 반복)은 전 사용자 자동 업데이트 대상이라 릴리스 전 사람 확인 항목으로 유지합니다.
