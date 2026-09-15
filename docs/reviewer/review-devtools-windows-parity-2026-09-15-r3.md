# 개발 환경 화면 Windows 완전 지원 재검토 보고서 (3차, 모드: 축소)

target_id: `devtools-windows-parity`
직전 리뷰: `docs/reviewer/review-devtools-windows-parity-2026-09-15-r2.md` (판정: 🟡 Amber / 신규 Major 3건 N1·N2·N3)
이번 리뷰 대상: 커밋 `3e25317` 1개(4파일 +719/−286). 브랜치 전체 맥락은 `git diff main..HEAD`(9커밋)로 대조.
리스크 범주: **로컬 실행 경계 / 설치 프로그램 구동** — 1·2차와 동일(불변 확인).
작업 등급: Sensitive (노출 범위 축소 미적용 — 위임서에 축소 주장 없음)
리뷰 일자: 2026-09-15
리뷰 페르소나 패널: 4명 전원 재사용 / 신규 0 (2차 6명에서 2명 축소 — 사유는 아래 표)

**이번 라운드의 단 하나의 질문에 대한 답: 🟠 부분적으로 닫혔다. "닫힘"이 아니다.**

**종합 판정: 🟡 Amber** — Critical 0 / Major 2(전부 신규) / Minor 5 / Nit 1.
**머지 차단 사유: 없음.** (2차의 차단 3건은 해소 또는 수용 가능한 부분 해소. 신규 Major 2건은 실기 1호 PC 배포 전 필수이나 머지를 막을 성질이 아니다 — 근거는 "PM에게 권고" 절)

---

## 요약 (2분 규칙)

**개별 3건은 실제로 고쳐졌다.** N1은 14개 Windows 후보를 수정 후 코드로 손 추적해 macOS 문구 잔존 0을 확인했고, N2는 값이 뒤집혔으며(gh가 "전체 업데이트" 배치에서 빠진다 — 2차 blast-radius 지적이 함께 닫힌다), N3의 처방 ②는 집합·순서까지 고정하는 형태로 들어왔다. **구현자가 내 2차 보고를 정정한 것도 옳다** — Unknown은 4건이 아니라 3건이고, 틀린 쪽은 나다.

**그런데 클래스는 닫히지 않았다.** 이유는 가드의 품질이 아니라 **가드가 붙은 위치**다. 신규 가드는 `compute_action_for_platform`의 반환값을 검사하는데, **사용자가 실제로 보는 문구는 그보다 두 계층 아래에서 확정된다.** 그 아래에 같은 클래스가 두 개 살아 있다:

- **`resolve_plan`의 NoRunner 강등**(`install_resolver.rs:59`) — `MANUAL_NO_RUNNER`의 문구가 `"필요한 실행 도구(brew/npm/winget)를 찾을 수 없어…"`다. Windows 사용자는 `brew`를, macOS 사용자는 `winget`을 본다. **양방향으로 새고, 양방향 가드 둘 다 이 계층을 보지 않는다**(2차의 m4 — 이제 "가드 밖"이라는 사실이 추가됐다).
- **PATH 힌트 패널**(`diagnostics.rs:244-260` → `devTools.ts:576-585`) — Windows에서 `path_hint`가 *명령 한 줄*이 아니라 *한국어 조언 문장*인데, 프런트는 그것을 `"아래 줄을 PATH 설정에 추가하세요"`라는 **고정 제목 + 명령 블록 + 복사 버튼**으로 렌더한다. 사용자는 "새 터미널을 열거나 로그아웃 후 다시 로그인하면 PATH가 갱신됩니다."를 PATH 설정에 붙여넣으라는 지시를 받는다. **문구·프리뷰가 아닌 제3의 축이고, 어떤 가드도 이 경로에 없다**(신규 **T1**).

그리고 가드 자체에도 구멍이 두 개 있다. 구현자가 "install/update 양쪽을 하나의 정본으로 묶었다"고 적은 주장은 **사실이 아니며**(설치 경로는 여전히 `installer_label == "winget"` 문자열 비교, 호출부 전수 grep으로 확인), N2 구조 가드의 핵심 단언은 **항진명제**라 어떤 회귀도 잡지 못한다(신규 **T2**).

---

## 모드 판정 근거

**축소(Reduction) 모드.** 착수 전 두 관문을 순서대로 대조했다.

**① 풀패널 강제 승격 조건 (Skill `common-task-grading-and-verification-depth`) — 미해당**

| 승격 조건 | 대조 결과 |
|---|---|
| 직전 Critical/Major 미해결 존재 | 위임 시점 기준 N1·N2·N3 모두 수정 커밋(`3e25317`) 존재 — 미해결로 넘어온 것이 아니다 |
| 직전에 없던 새 실행경로·리스크 표면 **다수** 등장 | **0개.** 이번 커밋은 새 프로세스 스폰·새 argv·새 권한 경계를 하나도 만들지 않는다(추가된 것: 순수함수 1개, 정책함수 1개, Manual 문구 상수 3개, 테스트 4개). 2차의 `-EncodedCommand`처럼 셀 표면 자체가 없다 → 증분이 아니라 축소 |
| 다른 Sensitive 하위도메인 진입 | 없음 — 로컬 실행 경계 그대로 |
| 재검토 4차 이상 누적 | 3차 |
| PM·사용자의 명시적 풀패널 요청 | 없음(위임서가 "모드 판정은 네가 해라"로 위임) |

**② 동일 대상 인정 4조건 — 전부 충족**(PM 판단을 그대로 받지 않고 직접 대조)

| 조건 | 확인방법 | 결과 |
|---|---|---|
| 동일 target_id | 위임서 + 직전 보고서 3행 | `devtools-windows-parity` 일치 |
| 직전 리뷰로부터 7일 이내 | `git log --date=iso-local` — 직전 커밋군 17:28·17:45, 이번 `3e25317` 18:20 | 같은 날 |
| 리스크 범주 불변 | 직전 보고서 6행 vs 위임서 | 로컬 실행 경계/설치 구동 — 동일 |
| 리뷰 대상 파일 실질 중첩 | `git show --stat 3e25317` | 4파일 중 4개 전부 직전 리뷰 대상(`classify_tests.rs`·`plan_table.rs`·`query.rs` + 그 테스트 분리본) |

→ **축소 모드.** 축소한 것은 **패널 인원(6→4)** 하나뿐이고, 위임서가 지정한 단일 질문("클래스가 닫혔는가")에 대해서는 **범위를 오히려 넓혔다** — 가드가 덮는다고 주장하지 않은 계층(`resolve_plan`·`diagnostics.rs`·프런트 렌더)까지 수색했고, 신규 Major 1건이 거기서 나왔다.

---

## 페르소나 재사용 판정 (산출물 게이트)

착수 전 `docs/reviewer/personas/INDEX.md`를 Read해 "역할개념" 열을 스크리닝했다. **신규 0명.** 이번 라운드에 새 리스크 표면이 없으므로 신규 페르소나를 만들 근거 자체가 없다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-heterogeneous-machine-operator.md` | **재사용** | 단일 질문("Windows 사용자가 사실이 아닌 것을 보는 클래스")이 이 페르소나의 O1~O3 기준 그 자체. T1 발견 주체. 6대 요소 무수정. |
| `persona-claimed-vs-verified.md` | **재사용** | 위임서가 검증을 명시 요구한 두 주장(Unknown 3건 정정 / 수정 전 실패 재현)이 이 페르소나의 V1·V4 기준. 6대 요소 무수정. |
| `persona-design-contract-auditor.md` | **재사용** | "강제 수단이 생겼다"는 주장의 실질 판정 — T2(항진명제·이중 진실원) 발견 주체. 6대 요소 무수정. |
| `persona-zero-base-redesigner.md` (발산) | **재사용** | 발산형 최소 1명 규칙 담당. R-4 유지 판정 + R-7 신규 제안. 6대 요소 무수정. |
| `persona-process-execution-security.md` | **투입 안 함(생략)** | 이번 커밋에 argv·셸 인용·env·프로세스 수명 변경이 **0줄**이다(`git show 3e25317` 전수 — 새 `Command` 호출·새 `Arg` 슬롯·새 러너 없음). 이 페르소나가 볼 변경분이 없다. 2차 판정(B1/B2/B3·`-EncodedCommand`) 그대로 유효. |
| `persona-delegated-authority-blast-radius.md` | **투입 안 함(생략)** | 권한 경계 이동 없음. 단 2차 최대 지적("gh가 전체 업데이트 배치에 자동 포함 → 예고 없는 UAC")은 N2 수정으로 **해소**됐음을 아래 N2 절에서 대신 확인했다. |

축소 모드라 발산형 생략이 허용되나 **생략하지 않았다** — 이번 라운드의 핵심 처방(R-4·R-7)이 구조 축에 있기 때문이다.

---

## 이번 라운드의 단 하나의 질문: 클래스가 닫혔는가

**판정: 🟠 부분.**

"클래스"를 위임서 정의대로 읽는다 — *"Windows 사용자가 사실이 아닌 것을 보게 되는 결함"*. 그 클래스의 **발현 지점 전수**를 따라가면 사용자 화면에 문구를 내보내는 출구가 다섯 개다. 신규 가드는 그중 **둘**을 덮는다.

| # | 출구(사용자가 문구를 보는 자리) | 확정 위치 | 신규 가드 커버 | 현재 상태 |
|---|---|---|---|---|
| 1 | 설치된 도구의 업데이트 안내 | `compute_action_for_platform` → `ManualPlan` | ✅ `windows_installed_tool_actions_never_reach_macos_only_manual_text` | **깨끗**(14후보 손 추적 확인) |
| 2 | 미설치 도구의 설치 안내 | `install_manual_plan` → mac/win 분기 | ⚠️ 구조 순회 아님 — `install_manual_plan_windows_variant_uses_winget_not_brew`가 **도구 6개를 손으로 나열** | 오늘은 6/6이라 우연히 전수. 7번째 도구가 추가되면 조용히 면제 |
| 3 | 실행기 부재 시 강등 안내 | `resolve_plan` → `MANUAL_NO_RUNNER` (`install_resolver.rs:59`) | ❌ **덮지 않음** | **결함 실재** — `plan_table.rs:466` `"필요한 실행 도구(brew/npm/winget)"`. Windows에서 `brew`, macOS에서 `winget`이 그대로 노출(**T3**) |
| 4 | 설치/업데이트 후 PATH 힌트 패널 | `compute_path_visibility_windows` (`diagnostics.rs:244`) → `devTools.ts:576-585` | ❌ **덮지 않음** | **결함 실재, 신규 발견** — 조언 문장을 "PATH에 추가할 줄"로 제시(**T1**) |
| 5 | git 스텁 직접 분기 | `query.rs:73` — `compute_action`을 **거치지 않고** `MANUAL_XCODE_CLT`를 직접 세팅 | ❌ 덮지 않음 | **오늘은 안전**. `is_git_stub_without_clt`가 `resolved_path == "/usr/bin/git"` 리터럴 비교라(`install_resolver.rs:213`) Windows에서 도달 불가. 단 **우연히 안전한 것이지 가드가 보장하는 것이 아니다** |

**한 문장으로:** 가드는 *결정 계층*(`compute_action`)에 붙었고, 클래스는 *출력 계층*(마지막으로 문자열이 확정되는 자리)에서 발생한다. 그래서 2차에서 내가 지적한 구조("가드가 설치 경로만 덮었다")가 **한 단계 높은 축에서 그대로 반복됐다** — 이번엔 "가드가 결정 계층만 덮는다".

### 위임서 4대 질문에 대한 답

**1. 전수성이 진짜인가 — 업데이트 경로는 진짜, 설치 경로는 손 나열, 그 아래 두 계층은 부재.**
`windows_installed_tool_actions_never_reach_macos_only_manual_text`(`plan_table_tests.rs:341-375`)는 `DEV_TOOLS`(6) × `windows_path_candidates`(3+2+2+3+2+2=**14**)를 하드코딩 없이 순회하고 `assert_eq!(checked, total_candidates)`로 순회 자체를 고정한다. **업데이트 경로 축은 진짜 전수다** — 내가 14개를 손으로 재계산해 `total_candidates == 14`와 일치함을 확인했다. 반면 설치 경로 축(위 표 #2)은 구조 순회가 아니라 손 나열이고, #3·#4 계층은 아예 없다.

**2. 검출 토큰이 충분한가 — 현재 상수 집합에 대해서는 충분, 앞으로는 불충분.**
`MAC_ONLY_TOKENS`(`softwareupdate`/`brew`/`Xcode`/`xcode-select`/`Homebrew`)를 현재 존재하는 모든 `ManualPlan` 문구와 대조했다. `plan_table.rs`의 mac 계열 상수 전수(`MANUAL_XCODE_CLT:375-376`, `MANUAL_CASK_NOT_WRITABLE:429-430`, `MANUAL_NO_RUNNER:466`, `install_manual_plan_mac:577-610`)가 **전부 `brew` 또는 `Xcode`에 걸린다** — 오늘은 구멍이 없다. 다만 새는 표현은 남아 있다: `"캐스크"`(`brew`를 빼면 통과), `/opt/homebrew`·`/usr/local` 같은 경로, `Terminal.app`, `~/.zprofile`·`export PATH=` 같은 POSIX 셸 관용구. **마지막 항목이 실제로 T1의 형태다** — `export PATH="…"`(`diagnostics.rs:278`)는 macOS 전용 관용구인데, 토큰 목록에 없을 뿐 아니라 애초에 가드가 보는 자료구조(`ManualPlan`) 밖에 있다. 역방향 `WIN_ONLY_TOKENS`(`winget`/`.exe`/`Program Files`)도 `PowerShell`·`%LOCALAPPDATA%`·`UAC`·`nvm-windows`를 못 잡는다.

**3. `assert_eq!(checked, total)`이 가드를 지키는가 — 한 축만 지킨다. 세 축이 아직 조용히 면제된다.**

| 축 | `assert_eq!(checked, total)`이 막는가 | 판정 |
|---|---|---|
| 순회가 도중에 멈춤·필터링됨 | ✅ 막는다 | 의도대로 작동 |
| **새 도구가 `windows_path_candidates: &[]`로 추가** | ❌ 못 막는다 — `checked`와 `total` **둘 다** 0을 더하므로 등식이 유지된다 | **조용한 면제 실재**(T4) |
| **새 `MethodKind` 추가** | ⚠️ 절반 — `assert_kind_is_covered`의 exhaustive match가 컴파일 에러를 내지만(`query.rs:570-582`), **실제 순회 소스는 별개 배열 `ALL_METHOD_KINDS: [MethodKind; 10]`**(`query.rs:584-595`)다. match arm만 추가하고 배열에 안 넣으면 컴파일이 통과하고 새 kind는 한 번도 순회되지 않는다 | **부분 면제**(T5) |
| **새 `Runner` 추가** | ❌ 전혀 못 막는다 — `winget_preview_is_reliable(r) = r != Runner::Winget`(`query.rs:108-110`)은 **블랙리스트**다. dry-run 없는 러너(scoop/choco 등)를 추가하면 기본값 `true`(신뢰 가능)를 자동으로 물려받는다 | **면제 실재 — N2의 근본원인이 Runner 축에 그대로 남음**(T2b) |

2차에서 내가 쓴 문장("판정식이라 면제 집합이 조용히 자란다")은 `Unknown` 축에서는 해소됐고, **`Runner` 축과 "빈 후보 배열" 축으로 옮겨갔다.**

**4. 다른 축에 같은 클래스가 남아 있는가 — 남아 있다. "없음"이라고 단정하지 않는다.**
문구·프리뷰 말고 **PATH 힌트 축**이 남아 있다(T1). 수색 범위를 밝힌다: `src-tauri/src/dev_tools/**/*.rs`(테스트 제외)와 `src/views/devTools.ts`·`src/devToolsApi.ts`를 macOS 관용구 토큰(`brew|Homebrew|Xcode|softwareupdate|xcode-select|Terminal.app|/opt/homebrew|캐스크|CLT`)으로 전수 grep한 뒤, 히트한 자리마다 Windows 도달 가능성을 호출부로 역추적했다. 그 결과 **사용자 화면에 닿는 히트는 5개 출구 표로 수렴했고**, 그중 미커버 2건(T1·T3)이 위에 있다. 프런트(`devTools.ts`)의 하드코딩 문구는 T1 한 자리를 제외하면 플랫폼 중립이다(`터미널`, `설치 방식` 등).

---

## 2차 Major 3건 해소 여부

| ID | 해소 판정 | 근거(확인방법 포함) |
|---|---|---|
| **N1** | **✅ 해소** | `plan_table.rs:718-731`에 `compute_action_for_platform`이 신설되고 `SystemManaged && Platform::Win`이 `lookup_action`보다 **먼저** `windows_system_managed_plan`으로 갈라진다(corepack 가드와 같은 최우선 override 위치). 확인방법: 14개 Windows 후보를 HEAD 분류기(`classify.rs:310-405`)로 손 추적해 SystemManaged가 되는 3건(node `C:\Program Files\nodejs\node.exe`, git `C:\Program Files\Git\cmd\git.exe`, git `C:\Program Files (x86)\Git\cmd\git.exe`)을 특정하고, 각각이 `MANUAL_WINDOWS_SYSTEM_MANAGED_{NODE,GIT,GIT}`(`plan_table.rs:385-397`)를 받는 것을 확인. macOS 토큰 0. winget 패키지 id도 `install_manual_plan_windows`와 동일 값(`Git.Git`/`OpenJS.NodeJS.LTS`)으로 맞췄다 — 새 id를 지어내지 않았다. **`MANUAL_XCODE_CLT`는 UPDATE_TABLE(`plan_table.rs:529`)에 macOS 전용으로 남고, Windows에서 그 행에 도달할 방법이 없다.** |
| **N2** | **✅ 해소(값). 단 재발 방지력은 주장보다 약함 → T2** | `query.rs:180-191`의 `None` 폴백이 `winget_preview_is_reliable(plan.runner)`로 갈라지고 `RUN_WINGET_UPGRADE_GH`(`preview_args: None`, `runner: Winget`)는 `preview_reliable: false` + 사유 notes를 낸다. `build_run_preview_marks_winget_update_path_as_unreliable`(`query.rs:504-528`)가 실제 함수를 호출해 이를 관측한다 — 이 테스트는 **수정 전 코드로 되돌리면 실제로 깨진다**(유일하게 성립하는 재현 사례 중 하나). **2차 blast-radius 지적도 함께 닫힌다**: `devToolsApi.ts:12-15`의 배치 포함 조건이 `previewReliable===true && affected.length===1`이므로, gh가 이제 "전체 업데이트" 배치에서 **빠진다** → 예고 없는 머신 스코프 UAC 승격 경로가 사라진다. |
| **N3** | **🟠 부분 해소(처방②만, 처방①은 보류 — 보류는 수용)** | `classify_tests.rs:102-106`의 `KNOWN_UNKNOWN_WINDOWS_CANDIDATES` 리터럴 3행 + `classify_tests.rs:617-626`의 `assert_eq!(actual_unknown, expected_unknown)`. 판정식 → 허용목록 전환이 실효하며, **집합뿐 아니라 순서까지** 고정된다(아래 Nit). 남은 결함 3건은 그대로 살아 있고 `MANUAL_UNKNOWN_METHOD`(`plan_table.rs:456-460`)는 여전히 `copyable_command`·`doc_url` 모두 `None` — **다음 행동 0개**. |

---

## 구현자의 두 주장 검증

### 주장 A — "실제 Unknown은 4건이 아니라 3건이며 `C:\Program Files (x86)\Git\cmd\git.exe`는 이미 SystemManaged다"

**✅ 사실이다. 틀린 쪽은 내 2차 보고다.**

확인방법: `git show HEAD:classify.rs`의 `classify_install_method_windows`(7개 분기)와 `git show HEAD:mod.rs`의 `DEV_TOOLS`(14후보), 그리고 테스트가 주입하는 `EnvRoots`(`classify_tests.rs` / `plan_table_tests.rs:344-352`)를 놓고 14개를 손으로 전부 분류했다. **분류기 코드는 이번 커밋에서 한 줄도 바뀌지 않았다**(`git show 3e25317 --stat` — `classify.rs` 부재, 바뀐 것은 `classify_tests.rs`뿐) → 2차 시점에도 이미 그랬다.

| # | 도구 | 후보 | 분류 | 근거 분기 |
|---|---|---|---|---|
| 1 | claude | `%USERPROFILE%\.local\bin\claude.exe` | ClaudeNative | 4번 |
| 2 | claude | `%APPDATA%\npm\claude.cmd` | NpmGlobal(claude) | 3번 shim |
| 3 | claude | `…\WinGet\Links\claude.exe` | WingetPackage | 1번 |
| 4 | node | `C:\Program Files\nodejs\node.exe` | SystemManaged | 5번 리터럴 |
| 5 | node | `%LOCALAPPDATA%\Programs\nodejs\node.exe` | **Unknown** | 어느 분기도 미적중 |
| 6 | gh | `C:\Program Files\GitHub CLI\gh.exe` | **Unknown** | 〃 |
| 7 | gh | `…\WinGet\Links\gh.exe` | WingetPackage | 1번 |
| 8 | git | `C:\Program Files\Git\cmd\git.exe` | SystemManaged | 5번 리터럴 |
| 9 | git | `C:\Program Files (x86)\Git\cmd\git.exe` | **SystemManaged** | **5번의 두 번째 블록**(`classify.rs:399-404`, `roots.program_files_x86` + `\Git` 접두) |
| 10 | git | `%LOCALAPPDATA%\Programs\Git\cmd\git.exe` | **Unknown** | 어느 분기도 미적중 |
| 11 | pnpm | `%LOCALAPPDATA%\pnpm\pnpm.exe` | PnpmStandalone | 2번(stem=="pnpm") |
| 12 | pnpm | `%APPDATA%\npm\pnpm.cmd` | NpmGlobal(pnpm) | 3번 shim |
| 13 | wrangler | `%LOCALAPPDATA%\pnpm\wrangler.cmd` | PnpmGlobalPackage(wrangler) | 2번 |
| 14 | wrangler | `%APPDATA%\npm\wrangler.cmd` | NpmGlobal(wrangler) | 3번 shim |

→ Unknown은 **정확히 3건**(#5·#6·#10)이고, `DEV_TOOLS` 순회 순서(claude→node→gh→git→pnpm→wrangler)로 수집하면 **`[node, gh, git]`** — 허용목록 리터럴의 순서와 정확히 일치한다. **내 2차 보고가 #9를 Unknown으로 잘못 셌다**(`program_files_x86` 분기를 놓쳤다). 구현자의 실측 정정이 옳고, 이 보고서로 정정한다.

### 주장 B — "각 가드를 수정 전 코드로 되돌려 실제 실패를 재현했다"

**⚠️ 4종 중 1종만 성립한다. 나머지 3종은 "되돌려서 실패시키는 것"이 논리적으로 불가능하다.** 각 가드를 한 줄씩 읽고 "수정 전 코드에서 이 단언이 깨지는가"를 따졌다.

| 가드 | 되돌림 실패 재현 가능? | 근거 |
|---|---|---|
| `windows_installed_tool_actions_never_reach_macos_only_manual_text` | **✅ 성립** | `compute_action_for_platform`의 SystemManaged override(`plan_table.rs:725-730`)만 제거하면 후보 #4·#8·#9가 `lookup_action(_, SystemManaged)` → `UPDATE_TABLE`(`plan_table.rs:529`) → `MANUAL_XCODE_CLT` → `"Xcode"`·`"softwareupdate"` 적중 → **violations 3건**. 독립 추적으로 확인. 이 가드는 진짜다. |
| `windows_system_managed_plan_never_mentions_macos_terms` | **❌ 불가능** | 검사 대상 함수 `windows_system_managed_plan`이 **이 커밋에서 신설**됐다. 되돌리면 함수가 없어 컴파일 에러이지 테스트 실패가 아니다. 실패시키려면 새 함수를 **일부러 오염**시켜야 한다(뮤테이션 테스트). "수정 전 재현"이라는 표현은 이 가드에 대해 성립하지 않는다. |
| `mac_installed_tool_actions_never_reach_windows_only_manual_text` | **❌ 불가능** | 반대 방향 가드다. macOS 경로는 이번 커밋에서 **변경되지 않았고**(`compute_action_for_platform(_, _, Mac)`은 기존 `compute_action`과 동작 동일), 수정 전에도 mac 후보에서 `winget`/`.exe`/`Program Files`가 나오지 않았다 → **되돌려도 통과한다**. 앞으로의 회귀를 막는 값어치는 인정하지만, "실패를 재현했다"의 근거가 될 수 없다. |
| `all_winget_run_plans_report_unreliable_preview_in_both_install_and_update_paths` | **❌ 불가능(항진명제)** | 핵심 단언 `assert!(!winget_preview_is_reliable(plan.runner))`(`query.rs:585`)가 `if plan.runner == Runner::Winget` 블록 **안에** 있다. `winget_preview_is_reliable(r) = r != Runner::Winget`이므로 이 단언은 `!(Winget != Winget)` = `!false` = **항상 참**이다. 설치 경로 쪽(`query.rs:608`)도 같은 형태. 이 가드가 실제로 보호하는 것은 `plan.preview_args.is_none()`(`query.rs:588`) 하나뿐이고, **`build_run_preview`가 정말 그 정책을 참조하는지는 전혀 검사하지 않는다**(그 검사는 하드코딩된 단일 플랜 테스트 `query.rs:504-528`이 담당). |

**정직한 종합:** 구현자가 거짓을 보고했다고 보지 않는다 — 실제로 `cp` 백업 후 원복하고 `cargo test`를 돌렸을 것이고, 그때 **깨지는 테스트는 분명히 있었다**(가드 1 + `build_run_preview_marks_winget_update_path_as_unreliable`). 다만 "**각** 가드를"이라는 문장은 실제보다 넓다. 2차의 M1 검증에서 내가 요구했던 기준("이 테스트가 진짜 잡는 물건인가")을 이번 4종에 그대로 적용하면, **진짜 잡는 물건은 1.5종**이다.

---

## `system_prefixes` 확장(처방 #1) 보류에 대한 판정

**수용 가능. 지금 하지 않은 것이 오히려 옳다.** 단 조건이 하나 붙는다.

수용 근거 세 가지:
1. **#1은 미검증 추측으로 분류기를 넓히는 일이다.** 남은 Unknown 3건 중 실제로 존재하는 경로가 어느 것인지 아무도 모른다(위임서 자신이 "Windows 실기는 아무도 검증 못 했다"고 적었다). 잘못 넓히면 오분류 → **잘못된 러너로 실행**(Manual이 Run으로 승격)이라 피해가 현재보다 **커진다**. 지금의 Unknown → `MANUAL_UNKNOWN_METHOD`는 답답하지만 **fail-closed**다.
2. **#2가 들어와 빚이 코드에 상시 게시된다.** `KNOWN_UNKNOWN_WINDOWS_CANDIDATES` 3행은 grep 한 번으로 남은 결함 수를 세게 한다 — 2차에서 내가 요구한 "다음 라운드에 자동 재부상하는 형태"가 정확히 충족됐다.
3. **#1의 올바른 값은 실기 1호 PC가 결정한다.** 특히 gh는 `C:\Program Files\GitHub CLI\gh.exe`가 후보 1번이라 winget 머신 스코프 설치 시 **여기서 먼저 잡히고**, 설계 §B.2가 run으로 단언한 도구가 다음 행동 0개로 떨어진다 — 실기 체크리스트에서 이 자리가 실재하는지 확인한 뒤 고쳐야 값이 맞는다.

**붙는 조건(저비용, 지금 가능):** `MANUAL_UNKNOWN_METHOD`에 `doc_url`만이라도 넣으면 #1 없이도 "다음 행동 0개"가 해소된다(막다른 골목 → 최소한의 출구). `plan_table.rs:456-460` 한 줄. 분류 규칙을 건드리지 않으므로 오분류 위험이 없다.

---

## 신규 지적

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| **T1** | 🟠 Major | 이기종머신 / 화면 정직성 | `diagnostics.rs:244-260`(Win 분기) ↔ `diagnostics.rs:271-281`(Mac 분기) ↔ `devTools.ts:576-585` | 두 분기 Read + `compute_path_visibility` 디스패치(`diagnostics.rs:294-297`) 확인 + 프런트 렌더 코드 Read + `path_hint`/`path_hint_target` 계약 전수 grep | macOS 분기는 `hint = "export PATH=\"{dir}:$PATH\""`(붙여넣을 **한 줄**) + `hint_target = "~/.zprofile"` 등을 준다. **Windows 분기는 `hint = "새 터미널을 열거나 로그아웃 후 다시 로그인하면 PATH가 갱신됩니다."`(한국어 **조언 문장**) + `hint_target = None`을 준다.** 그런데 프런트는 플랫폼 분기 없이 고정 제목 **"설치는 됐지만 터미널에서 바로 쓸 수 없습니다. 아래 줄을 PATH 설정에 추가하세요."** 를 붙이고, 그 문장을 `devtool-panel-command`(명령 블록 스타일)에 넣고, **"복사" 버튼(라벨 'PATH 설정')** 까지 단다. Windows 사용자는 *"이 한국어 문장을 PATH 설정에 추가하라"* 는 **실행 불가능한 지시**를 받는다. 이것은 문구·프리뷰가 아닌 **제3의 축**이고 신규 가드 어디에도 걸리지 않는다. 게다가 `compute_path_visibility_windows`는 `std::env::var("PATH")`를 직접 읽어 **순수함수가 아니라** 이 머신에서 Windows 동작을 재현할 수 없다(`diagnostics.rs:511-517` 테스트는 "hint 문자열이 이 리터럴인가"만 고정할 뿐 화면에서 어떻게 보이는지는 보지 않는다) | 최소안: 백엔드가 `hint`의 **성격**을 함께 내보낸다 — `path_hint_kind: "command" \| "advice"`. 프런트는 `advice`면 제목을 "곧 해결됩니다"류로 바꾸고 명령 블록·복사 버튼을 렌더하지 않는다. 계약 1필드 추가 + 프런트 조건 1개. 계약 변경을 피하려면 차선: Windows 분기가 `hint: None`을 주고(패널 자체가 안 뜬다) 조언은 `notes`로 내린다 — **정보를 잃지 않으면서 거짓 지시를 없앤다**. 그리고 가드: 이 클래스는 `ManualPlan` 밖에 있으므로 **출력 계층 가드**(R-7)가 필요하다 |
| **T2** | 🟠 Major | 설계조항 / 주장vs관측 | `query.rs:105-110`(정본 함수 + 주석) ↔ `query.rs:215-232`(`no_dry_run_install_notes`) ↔ `query.rs:585, 608`(항진명제) | `git grep -n winget_preview_is_reliable` 호출부 전수(프로덕션 **1곳**: `query.rs:185`) + 두 판정 로직 나란히 Read + 단언 논리 수기 평가 | ①**주석이 사실과 다르다.** 새 정본 함수 doc(`query.rs:105-108`)과 커밋 메시지가 *"`no_dry_run_install_notes`(설치 경로)와 `build_run_preview`의 `None` 폴백(업데이트 경로) 둘 다 이 함수 하나로 판정을 공유"* 한다고 단언하지만, `no_dry_run_install_notes`는 여전히 **`installer_label == "winget"` 문자열 비교**(`query.rs:216`)다. 정본은 하나가 아니라 **둘**이고, 하나는 `Runner` 타입, 하나는 문자열 라벨이다. `install_candidates`의 라벨이 `"winget (machine)"` 식으로 바뀌면 설치 프리뷰가 조용히 `reliable=true`로 되돌아간다 — **N2와 글자 그대로 같은 결함이 재발할 통로**가 그대로 열려 있다. ②**구조 가드의 핵심 단언이 항진명제다**(위 "주장 B" 참조) — `if runner == Winget { assert!(runner != Winget == false) }`. 이 가드는 라벨 드리프트도, `build_run_preview`의 배선 제거도 잡지 못한다 | ①`no_dry_run_install_notes`가 `installer_label` 대신 `plan.runner`(또는 `winget_preview_is_reliable`)를 받도록 시그니처 변경 — 호출부 1곳(`query.rs:334`), 테스트 2개(`query.rs:477,485`) 수정. 이러면 주석의 주장이 **비로소 사실이 된다**. ②항진 단언을 실효 단언으로 교체: 가드 안에서 `build_run_preview`(또는 `build_install_preview`)를 실제로 호출해 반환된 `preview_reliable == false`를 단언한다 — `RUN_WINGET_UPGRADE_GH` 한 플랜만 하던 것을 **모든 winget 플랜**으로 넓히면 구조 가드가 된다 |
| **T3** | 🟡 Minor(2차 m4 승계 + 가드 사각 확인) | 이기종머신 / 설계조항 | `plan_table.rs:461-467`(`MANUAL_NO_RUNNER`) ↔ `install_resolver.rs:59`(강등 지점) | `git grep -n MANUAL_NO_RUNNER` 전수(사용처 1곳) + `resolve_plan` Read + 두 신규 가드의 호출 대상이 `compute_action_for_platform`임을 확인 | 실행기를 못 찾으면 `resolve_plan`이 `MANUAL_NO_RUNNER`로 강등하는데 문구가 `"필요한 실행 도구(brew/npm/winget)를 찾을 수 없어…"` 하나다. **Windows 사용자는 `brew`를, macOS 사용자는 `winget`을 본다** — 양방향으로 새는 유일한 자리다. 거짓 단언은 아니지만(세 후보를 나열할 뿐) 다음 행동이 0개이고, **신규 가드 2종이 이 계층을 보지 않는다**(둘 다 `compute_action_for_platform`의 반환값만 검사하고, 강등은 그 뒤에 일어난다). 심각도를 Minor로 유지하는 이유: 사용자를 틀린 OS로 유도하지는 않는다 | 플랫폼별 분기(`MANUAL_NO_RUNNER_MAC`/`_WIN`) + **가드를 `resolve_plan` 계층으로 내린다**. 후자가 본질 — T1·T3이 같은 처방(R-7)을 공유한다 |
| **T4** | 🟡 Minor | 설계조항 | `plan_table_tests.rs:336-340, 371-374` / `classify_tests.rs:578-582, 613-616` | 단언 논리 수기 평가 | `assert_eq!(checked, total_candidates)`는 새 도구가 `windows_path_candidates: &[]`로 추가될 때 **양변에 0을 더하므로 통과한다** — 그 도구는 가드를 한 번도 거치지 않고 조용히 면제된다. 2차에서 내가 지적한 "면제 집합이 조용히 자란다"가 이 축에 남았다 | `for def in DEV_TOOLS { assert!(!def.windows_path_candidates.is_empty(), "{}에 Windows 후보가 없습니다", def.key) }` 한 줄 추가. 또는 `assert!(total_candidates >= DEV_TOOLS.len())` |
| **T5** | 🟡 Minor | 설계조항 | `query.rs:570-596` | 코드 Read + 컴파일 의미 평가 | `assert_kind_is_covered`의 exhaustive match는 새 `MethodKind`에 **컴파일 에러**를 내지만, 실제 순회 소스는 별개 리터럴 배열 `ALL_METHOD_KINDS: [MethodKind; 10]`이다. 개발자가 컴파일 에러를 보고 match arm만 추가하면 통과하고, 새 kind는 한 번도 순회되지 않는다 — "컴파일타임 전수성 강제"라는 주석의 주장보다 실제 보장이 약하다 | match arm 본문을 비우지 말고 `_ => assert!(ALL_METHOD_KINDS.contains(&kind), "{kind:?}를 ALL_METHOD_KINDS에 추가하세요")` 형태로 — 각 arm이 배열 등재를 **런타임에** 강제한다. 컴파일 에러(arm 추가 강제) + 런타임 실패(배열 등재 강제)가 맞물려 축이 닫힌다 |
| **T6** | 🟡 Minor | 설계조항 | `plan_table_tests.rs:118-148`(`install_manual_plan_windows_variant_uses_winget_not_brew`) | 테스트 Read + `DEV_TOOLS` 개수 대조 | 설치 경로(출구 #2) 가드가 6개 도구를 **손으로 나열**한다 — 오늘은 6/6이라 우연히 전수지만 7번째 도구가 추가되면 조용히 면제된다. 게다가 macOS 토큰 검사가 도구마다 제각각이다(gh는 `copyable_command`에 `brew` 부재만, node는 `message_ko`에 `Homebrew` 부재만, git·pnpm·claude·wrangler는 **문구 검사 없음**) | `windows_installed_tool_actions_never_reach_macos_only_manual_text`와 동형으로 `for def in DEV_TOOLS { install_manual_plan_windows(def.id) }`를 순회하며 `MAC_ONLY_TOKENS` 전체를 검사 — 이미 존재하는 패턴을 한 번 더 적용하는 것이라 비용이 거의 없다 |
| **T7** | ⚪ Nit | 주장vs관측 | `classify_tests.rs:617-626` | 단언 형태 확인 | `assert_eq!(actual_unknown, expected_unknown)`이 `Vec` 비교라 **순서에 의존**한다. 오늘은 `DEV_TOOLS` 순회 순서와 허용목록 순서가 우연히 일치하지만, `DEV_TOOLS` 배열 순서를 바꾸는 무해한 리팩터가 이 가드를 "허용목록이 달라졌습니다"라는 오해 유발 메시지로 깨뜨린다 | `BTreeSet`(또는 양쪽 `sort()`)으로 비교. 실패 메시지의 의미가 "집합이 달라졌다"와 일치하게 된다 |

### 2차 Minor 5건(m4~m8) + n9 현황

**이번 커밋이 건드린 것은 0건이다.** m4는 위 T3로 승계(가드 사각이라는 사실이 추가됐다). m5(`--accept-package-agreements` 비대칭)·m6(`rfind('\\')`)·m7(UAC 거부 종료코드)·m8(UAC 예고 문구)·n9("컴파일 호스트" 표현) 모두 그대로 — 확인방법: 각 위치 `git show HEAD:` Read. **m7의 등급은 내려간다** — N2 수정으로 gh가 배치에서 빠졌으므로 UAC 거부는 개별 실행 국면에서만 발생한다(2차에서 "N2가 격상시킨다"고 했던 조건이 해소).

---

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 이기종머신 | `compute_path_visibility`가 `Path::new(win_path).parent()`를 쓰는데, `classify.rs:253-262`가 경고한 "컴파일 호스트 구분자" 함정에 걸린다 | **기각** | `std::path`의 구분자 규칙은 **타깃** OS로 결정된다(호스트가 아니다). Windows 타깃 빌드에서는 `\`를 올바로 다룬다. `classify.rs`의 경고는 "macOS **타깃**에서 Windows 문자열을 다루는 테스트"에 한정된 것이고, 이 함수는 Windows 타깃에서만 실행된다. 단 그 때문에 **이 머신에서 테스트할 수 없다**는 사실은 T1의 서술에 포함시켰다 |
| 설계조항 | 반대 방향 가드(`mac_installed_tool_actions_never_reach_windows_only_manual_text`)는 실패한 적도 없고 앞으로도 안 깨질 테니 삭제하라 | **기각** | 대칭 가드는 앞으로의 회귀(예: T3를 고치며 `MANUAL_NO_RUNNER`를 win 문구로만 바꾸는 실수)를 막는 값어치가 있다. 다만 "수정 전 실패를 재현했다"는 **주장의 근거로는 쓸 수 없다**는 점만 주장 B 절에 기록했다 |
| 주장vs관측 | `windows_system_managed_plan`의 `_ => GENERIC` 폴백이 `copyable_command: None`이라 다음 행동 0개다 — N3와 같은 결함이다 | **강등 → 지적 철회** | 현재 이 arm에 도달하는 도구가 **없다**(SystemManaged로 분류되는 것은 Git/Node뿐임을 14후보 추적으로 확인). 도달하지 않는 코드의 문구를 결함으로 세면 오탐이다. 주석이 "분류기가 바뀔 경우의 안전망"이라고 목적을 정확히 밝히고 있고, macOS 문구가 새는 것보다 명백히 낫다 |
| 실행보안 | `windows_system_managed_plan`이 `winget upgrade` 명령을 새로 노출하므로 실행 경계가 넓어진다 | **기각** | `ManualPlan.copyable_command`는 사용자가 **직접 복사해 터미널에서 실행**하는 안내 문자열이고, 앱이 spawn하지 않는다(`Action::Manual`은 `resolve_plan`에서 실행 경로로 가지 않는다 — `install_resolver.rs:56`). 실행 경계 불변. 주석이 같은 판단을 이미 명시(`plan_table.rs:383-384`) |
| 이기종머신 | 워킹트리의 `autonomy/runner.rs`·`dev_tools/process.rs`·`mcp_manager/*`·`src/views/*` 미커밋 변경 | **스코프 밖** | 위임서 명시 제외(다른 세션 동시 작업). 모든 인용은 `git show HEAD:` / `git show 3e25317` / `git diff main..HEAD` 기준이고 **워킹트리 파일을 한 번도 읽지 않았다** |

---

## 페르소나별 관점

### [이기종 머신 운영자] — 판정: 🟠 Amber (개선됨)
2차에서 그린 "6개 도구 × Windows 설치 형태 → 화면에 나오는 것" 표를 다시 그렸다.

| 도구 | Windows 설치 형태 | 2차 | 3차 |
|---|---|---|---|
| Git | 공식 설치기(전체 사용자, 64/32비트) | **macOS Xcode CLT 문구 + `softwareupdate --list`** | ✅ `"Git이 시스템 설치 경로(Program Files)에…"` + `winget upgrade --id Git.Git` |
| Node | 공식 설치기 | **동일한 macOS 문구** | ✅ `"Node.js가 시스템 설치 경로(Program Files)에…"` + `winget upgrade --id OpenJS.NodeJS.LTS` |
| gh | winget(Links shim) | 프리뷰 "신뢰 가능" 거짓 + 배치 자동 포함 | ✅ `preview_reliable:false` + 사유 명시 + **배치에서 제외** |
| gh | `C:\Program Files\GitHub CLI\gh.exe` | 다음 행동 0개 | ⚠️ 그대로(허용목록에 등재돼 가시화만 됨) |
| Git | "only for me" 설치 | 다음 행동 0개 | ⚠️ 그대로 |
| Node | `%LOCALAPPDATA%\Programs\nodejs` | 다음 행동 0개 | ⚠️ 그대로 |
| **전 도구** | **설치/업데이트 성공 직후 PATH 미반영** | (보지 않았음) | 🔴→🟠 **한국어 조언 문장을 "PATH에 추가할 줄"로 제시(T1 — 신규)** |
| **전 도구** | **실행기(npm/pnpm) 부재** | m4(Minor) | ⚠️ 그대로 + **가드 밖임이 확인됨(T3)** |

**거짓말의 형태가 또 바뀌었다.** 2차에서 "존재하지 않는 OS의 명령"이었던 것이 3차에서는 "실행 불가능한 지시"다. 다만 **양은 줄었고 방향은 맞다** — 첫 실행 화면(도구 6개 목록 + 업데이트 안내)은 이제 정확하고, 남은 것은 *조작 이후* 화면이다. 2차에서 내가 머지 차단으로 돌아섰던 이유("Windows 지원을 했다는 주장이 첫 실행에서 무너진다")는 **더 이상 성립하지 않는다.**

### [주장 vs 관측] — 판정: 🟠 Amber
이번 라운드에 검증을 요구받은 주장 2건 중 **하나는 옳았고(그리고 내가 틀렸고), 하나는 실제보다 넓다.**
- ✅ Unknown 3건 정정 — 14후보 독립 재계산으로 확인. **내 2차 보고의 오류를 이 보고서로 정정한다.**
- ⚠️ "각 가드를 수정 전 코드로 되돌려 실패 재현" — 4종 중 1종만 논리적으로 성립(주장 B 절).
- ❌ 새 정본 함수 주석의 "두 경로가 판정을 공유한다" — 호출부 전수 grep 결과 프로덕션 호출부는 1곳뿐(T2).
- ⏸ PM의 "355 passed / 0 failed / 2 ignored" — **재측정하지 않았다.** git 상태 변경 금지 범위라 분리 워크트리를 만들 수 없다. 이 보고서의 어떤 판정도 그 수치에 의존하지 않는다 — 전부 소스 정독과 손 추적이다.

2차에 이어 같은 패턴이 반복된다: **테스트를 만드는 성실함은 높은데, 그 테스트가 무엇을 보장하는지에 대한 서술이 항상 실제보다 한 뼘 넓다.** 이것 자체가 "클래스"의 한 종류다 — 코드가 아니라 **주석이** 사실이 아닌 것을 말한다.

### [설계 계약 감사] — 판정: 🟠 Amber
| 설계 조항 | 이번 라운드 상태 |
|---|---|
| §B.3 "winget 프리뷰는 `preview_reliable:false`" | **양쪽 경로 모두 구현 완료** — 2차 N2 해소. 단 강제 수단이 **두 벌의 서로 다른 판정 로직**으로 갈려 있다(T2) |
| §B.2 "run 3 / manual 3 도구 단위 일치" | 유지. gh가 Unknown에 걸리는 조건은 미해소(허용목록으로 가시화) |
| 업데이트 경로 Manual 문구의 플랫폼 분리 | **조항 부재였던 자리에 구현이 먼저 들어왔다** — `windows_system_managed_plan`. 설계서(`docs/design/devtools-windows-parity.md`)에 이 축을 추가하는 것이 다음 순서 |
| **가드의 부착 계층** | **조항 부재** — "어느 계층에 가드를 붙일 것인가"를 설계가 말하지 않아, 라운드마다 가드가 그 라운드의 결함이 있던 자리에만 붙는다. T1·T3이 이 공백의 산물 |

§B.2 → §B.3 → **가드 부착 계층**으로, 세 라운드 연속 "조항은 있는데 강제 수단이 한쪽만 덮는다"의 **한 단계 위 버전**이 나왔다. 이것이 "클래스가 부분적으로만 닫혔다"고 판정하는 계약 관점의 근거다.

---

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| **R-4(2차 제안, 유지)** | `winget_preview_is_reliable(runner) = runner != Runner::Winget` — 정책을 **블랙리스트**로 적었다 | `RunPlan`에 `preview: PreviewCapability` 단일 필드 — `DryRun(&[Arg])` / `NoneButReliable(사유)` / `NotOffered(사유)`. 기본값 없음 | **N2를 만든 원인은 "판단을 적지 않았더니 낙관적 기본값이 대신 판단해 준 것"이고, 그 원인은 이번 수정으로 사라지지 않았다** — `Runner` 축으로 옮겨갔을 뿐이다(T2b). 새 러너를 추가하는 사람은 블랙리스트의 존재를 모른 채 `preview_reliable: true`를 물려받는다. 선언형이면 컴파일러가 묻는다 | 낮음~중간. `RunPlan` 리터럴 9개 수정. **이번 브랜치에는 넣지 말 것** — 실기 검증 전에 타입을 흔들 이유가 없다 |
| **R-7(신규)** | 가드가 **결정 계층**(`compute_action_for_platform`)에 붙어 있다. 사용자가 보는 문자열은 그 아래 두 계층(`resolve_plan` 강등, `DevToolStatus`/`DevToolActionResult` 조립)에서 확정된다 | 가드를 **출력 계층**으로 내린다: `check_dev_tools_blocking()`·`perform_preview()`·`perform_update()`가 반환하는 **계약 구조체(`contract.rs`) 전체를 문자열로 직렬화해** 플랫폼 전용 토큰을 검사하는 가드 1개. 입력은 지금처럼 `DEV_TOOLS × 후보`를 순회하되 **출구에서 잡는다** | **이번 라운드가 증명한 것이 정확히 이것이다.** 가드 4종을 잘 만들었는데도 T1·T3이 살아남은 이유는 품질이 아니라 **위치**다. 출력 계층 가드 하나는 ①`MANUAL_NO_RUNNER` 강등 ②PATH 힌트 ③`describe_install_method` ④앞으로 추가될 어떤 문자열 필드든 자동으로 덮는다. "새 필드가 추가되면 가드가 자동으로 본다"는 성질은 결정 계층 가드가 원리적으로 가질 수 없다 | 중간. `contract.rs` 구조체에 `serde_json::to_string` 한 번(이미 serde 파생이 있다) + 플랫폼 주입을 위해 `compute_path_visibility`·`resolve_plan`을 순수화해야 한다(`platform` 인자 주입 — `compute_action_for_platform`이 이미 증명한 패턴). **실기 검증 후 별도 작업 권장** |
| **R-8(신규)** | 가드마다 `assert_eq!(checked, total)`이라는 "가드의 가드"를 손으로 적는다 — T4가 보여주듯 그 등식도 0=0으로 만족될 수 있다 | 순회 가드를 **헬퍼 한 개**로 통일: `fn for_every_windows_candidate(f: impl Fn(&DevTool, &str, InstallMethod))` — 내부에서 빈 배열 검사·개수 단언·토큰 확장 실패 패닉을 한 번만 구현하고, 각 가드는 검사 로직만 넘긴다 | 지금 같은 순회 코드가 `classify_tests.rs`와 `plan_table_tests.rs`에 **두 벌 복사돼 있다**(`EnvRoots` 리터럴까지 동일). 한쪽만 고치면 조용히 갈린다 — 이번 라운드에 T4가 **두 곳 모두**에 있는 이유가 그것이다 | 매우 낮음(테스트 리팩터). **지금 넣을 수 있다** |

---

## 트레이드오프 (페르소나 간 충돌)

1. **처방 #1(`system_prefixes` 확장) 즉시 반영 vs 보류** — 이기종머신은 gh·Git "only for me"가 사내에서 흔한 형태라 지금 고치길 원하고, 주장vs관측은 **실기 미검증 상태에서 분류기를 넓히는 것은 추측을 코드로 굳히는 일**이라 반대한다.
   → **권고: 보류 유지.** 지금의 Unknown은 fail-closed(답답하지만 안전)이고, 잘못 넓히면 fail-open(틀린 러너 실행)이다. 대신 `MANUAL_UNKNOWN_METHOD`에 `doc_url` 한 줄로 막다른 골목만 없앤다.

2. **T1 최소안(계약 1필드 추가) vs 차선(계약 불변, `hint: None`)** — 설계계약감사는 `contract.rs` diff 0을 이 작업의 모범으로 유지 평가하고, 이기종머신은 조언 정보를 잃지 않기를 원한다.
   → **권고: 차선(계약 불변)을 먼저.** Windows 분기가 `hint: None`을 주고 조언은 다른 자리로 내리면 **거짓 지시가 즉시 사라지고** 계약은 그대로다. 실기에서 "PATH가 안 잡혀 혼란스럽다"는 마찰이 실제로 관측되면 그때 `path_hint_kind`를 추가한다 — 2차 N2에서 썼던 순서(값 먼저, 구조는 관측 후)와 같다.

3. **머지 속도 vs 남은 2건** — 2차에서 이기종머신이 머지 차단으로 돌아섰던 근거가 이번엔 소멸했다(첫 실행 화면이 정확해졌다). 설계계약감사는 T2(주석이 사실이 아님)를 남긴 채 머지하면 **다음 세션이 그 주석을 믿는다**고 우려한다.
   → **권고: 머지하되 T2 ①(시그니처 통일, 호출부 1곳+테스트 2개)은 머지 전에 넣는다.** 코드 수정이 아니라 **거짓 주석을 사실로 만드는** 작업이라 비용이 거의 없고, 방치하면 정확히 N2가 재발하는 통로다.

---

## 잘 된 점 (유지할 패턴)

1. **리뷰어의 사실 오류를 실측으로 정정했다.** 구현자는 내 2차 보고가 Unknown을 4건으로 센 것을 받아들이지 않고 직접 덤프해 3건임을 밝히고, **왜 하나가 빠지는지(`program_files_x86` 분기)까지 주석에 적었다**(`classify_tests.rs:89-101`). 리뷰 지적을 무비판 수용하는 것보다 훨씬 나은 태도이고, 실제로 맞았다.
2. **N1 처방을 문구 교체가 아니라 순수함수 분리로 풀었다.** `compute_action_for_platform`은 "이 머신에서 Windows 분기를 실행 검증할 수 있게 한다"는 설계 §D 패턴을 정확히 한 번 더 적용한 것이고, **덕분에 가드 테스트가 존재할 수 있게 됐다.** 이 리팩터가 없었다면 N1 가드를 macOS에서 돌릴 방법이 없었다.
3. **override 위치를 corepack 가드와 같은 자리로 골랐다.** `lookup_action` 이전에 갈라 `UPDATE_TABLE`의 macOS 행을 **손대지 않고** 우회한다 — 기존 macOS 동작에 회귀가 들어갈 여지를 구조적으로 0으로 만들었다(`mac_installed_tool_actions_…` 가드가 이를 고정).
4. **새 id를 지어내지 않았다.** `windows_system_managed_plan`의 winget id를 `install_manual_plan_windows`와 동일 값으로 맞추고 그 사실을 테스트로 고정했다(`plan_table_tests.rs:300-308`). 같은 사실이 두 곳에 다른 값으로 갈리는 것을 미리 막았다.
5. **가드가 무의미해지는 경우를 스스로 의심했다** — `"winget RunPlan이 하나도 발견되지 않았습니다 — 이 가드가 무의미해집니다"`, `"DEV_TOOLS 순회가 예상과 다르게 실행됐습니다"`. 방향이 정확히 옳다. T4·T5는 이 의심을 **한 칸 더** 밀면 나오는 것들이다.
6. **1,000줄 규율을 세 번째 같은 패턴으로 적용했다** — `plan_table.rs` → `plan_table_tests.rs`(`#[path]` 연결, 정본은 원 파일 유지). `classify`/`platform`과 동형이고, 내용 이동분에 **변경이 섞이지 않았음**을 diff에서 확인했다(이동된 8개 테스트의 본문은 한 글자도 다르지 않다).

---

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| Windows Git·Node가 macOS 문구를 받지 않는가 | 이기종머신 | 필수 | ✅ | N1 해소 — 14후보 추적 |
| winget 프리뷰가 신뢰 가능이라 거짓말하지 않는가 | 설계조항 | 필수 | ✅ | N2 해소 + gh가 배치에서 제외됨 |
| Unknown 면제가 조용히 자라지 않는가 | 이기종머신 | 필수 | ✅ | N3 처방② — 허용목록 + `assert_eq!` |
| **클래스 전체가 닫혔는가** | 전 관점 | 필수 | ❌ | **T1(PATH 힌트 축)·T3(강등 계층) — 출구 5개 중 2개 미커버** |
| 가드가 주장하는 전수성이 사실인가 | 주장vs관측 | 필수 | ⚠️ | 업데이트 경로는 진짜 전수. 설치 경로는 손 나열(T6), 새 Runner·빈 후보 축은 면제(T2b·T4) |
| 구조 가드의 단언이 실효하는가 | 설계조항 | 필수 | ❌ | **T2 — 핵심 단언이 항진명제** |
| 새 정본 함수의 주석이 사실인가 | 주장vs관측 | 필수 | ❌ | **T2 — 설치 경로는 여전히 문자열 비교** |
| Unknown 3건 정정이 맞는가 | 주장vs관측 | 필수 | ✅ | 맞다. 내 2차 보고가 틀렸다 |
| 수정 전 실패 재현 주장이 맞는가 | 주장vs관측 | 필수 | ⚠️ | 4종 중 1종만 논리적으로 성립 |
| `system_prefixes` 보류가 수용 가능한가 | 이기종머신 | 필수 | ✅ | 수용 — fail-closed 유지가 추측 확장보다 안전 |
| macOS 경로 회귀가 없는가 | 이기종머신 | 필수 | ✅ | `compute_action_for_platform(_,_,Mac)`이 기존 `compute_action`과 동작 동일 + 반대 방향 가드 |
| 실행 경계가 넓어지지 않았는가 | 실행보안 | 필수 | ✅ | 새 spawn·새 argv·새 러너 0줄(diff 전수) |
| 미검증 항목이 정직하게 표기됐는가 | 주장vs관측 | 필수 | ⚠️ | 코드 주석은 일관되게 정직. **가드의 보장 범위 서술만 실제보다 넓다** |
| 실패 시 막다른 골목이 아닌가 | 이기종머신 | 권장 | ❌ | Unknown 3건 + `MANUAL_NO_RUNNER` 여전히 다음 행동 0개 |
| `#[cfg(windows)]` 테스트가 컴파일되는가 | 주장vs관측 | 권장 | ⏸ | 2차 N7 그대로. PM이 `winparity-code-final` 빌드 진행 중 — 이 리뷰와 별개 |

---

## PM에게 권고

**머지 차단 사유: 없음.**

판단 근거를 명시한다 — 2차에서 내가 차단을 건 근거는 *"Git·Node 2개 도구에서 macOS 문구가 나오는 상태로 배포하면 Windows 지원이라는 주장 자체가 첫 실행에서 무너진다"* 였다. **그 조건이 소멸했다.** 신규 T1·T3은 ①첫 실행 화면이 아니라 조작 이후 화면이고 ②사용자를 존재하지 않는 OS로 유도하지 않으며 ③막다른 골목을 만들지 않는다(T1의 조언 내용 자체는 맞는 말이다). 현재 상태는 `main`(Windows에서 6개 도구 전부 "설치 안 됨"이라 거짓말)보다 **모든 축에서 낫다.**

**2차의 M3 판정("`#[cfg(windows)]` 미실행은 머지 차단 사유 아님")은 4라운드 이후에도 유지한다.** 이번 커밋은 `#[cfg]` 게이트가 걸린 코드를 한 줄도 추가하지 않았고(신규 코드 전부 플랫폼 무관 `match`/순수함수), macOS `cargo test`가 컴파일을 보증한다. 미실행 범위는 2차와 동일한 2개 테스트 그대로다.

**우선순위 1 (머지 전 — 둘 다 국소)**
- **T2 ①** — `no_dry_run_install_notes`가 `installer_label` 문자열 대신 `Runner`를 받게 한다(호출부 1곳 `query.rs:334` + 테스트 2개). **코드 결함 수정이 아니라 이미 코드에 적혀 있는 주석을 사실로 만드는 작업**이고, 방치하면 다음 세션이 "정본 하나로 묶였다"를 믿고 N2를 재발시킨다.
- **T2 ②** — 항진 단언을 `build_run_preview` 실호출 단언으로 교체. 가드 하나가 진짜가 된다.

**우선순위 2 (실기 1호 PC 배포 전 필수)**
- **T1** — PATH 힌트 패널. 차선안(Windows 분기가 `hint: None`)이면 백엔드 3줄, 계약 불변. **설치 성공 직후에 보이는 화면이라 1호 PC의 첫인상을 정확히 여기가 결정한다.**
- **T3** — `MANUAL_NO_RUNNER` 플랫폼 분기(2차 m4 승계).
- `MANUAL_UNKNOWN_METHOD`에 `doc_url` 한 줄 — Unknown 3건의 막다른 골목만 해소(분류기 무변경).

**우선순위 3 (가드 보강 — 전부 테스트 파일, 비용 매우 낮음)**
- T4(빈 후보 배열 단언 1줄) · T5(match arm에 배열 등재 강제) · T6(설치 경로 가드를 구조 순회로) · T7(`BTreeSet` 비교) · **R-8**(순회 헬퍼 통일 — 지금 순회 코드가 두 벌 복사돼 있어 T4가 양쪽에 동시에 생겼다).

**우선순위 4 (실기 1호 PC 체크리스트 — 2차 목록 유지, 순서만 갱신)**
1. **N6: EDR이 `-EncodedCommand`를 차단하는가** — 차단되면 로그인·mcp·터미널 안내가 전부 죽는다. 여전히 1번.
2. **gh 실제 설치 자리** — `C:\Program Files\GitHub CLI\` 직하인가 `\bin\` 하위인가. **`system_prefixes` 확장(처방 #1)의 올바른 값이 여기서 결정된다.**
3. **Git "only for me" 설치의 실제 경로** — 허용목록 3건 중 사내에서 가장 흔할 후보.
4. PATH 미반영 빈도(T1이 실제로 얼마나 자주 보이는가) · pnpm Windows 실제 레이아웃.
5. M3 잔여(`''''` 이스케이프) · m5 · m7.

**우선순위 5 (백로그)** — R-4, R-5, R-7, m6, m8, n9.

**배포 방식** — 2차 권고 유지: **1~2명 선배포.** 화면이 `installMethod`와 해석된 경로를 노출하므로(`devTools.ts:385`) 허용목록 3건의 실발현 여부와 T1의 실제 노출 빈도를 **스크린샷 두 장**으로 회수할 수 있다.

---

## 정직 보고 — 이 리뷰가 확인하지 못한 것

- **Windows 실기 동작은 이 리뷰도 전혀 검증하지 못했다.** "Windows에서 이렇게 동작한다"고 단정한 곳은 없다. T1·T2·T3·T4~T7은 **코드 내부의 제어 흐름 추적과 표 대조**로 성립하는 지적이라 실기 없이도 확정되지만, 그 경로에 도달하는 **빈도**(사내 Git 설치가 전체 사용자인지 "only for me"인지, PATH가 실제로 얼마나 자주 미반영인지)는 모른다.
- **`cargo test`를 재실행하지 않았다.** git 상태·워킹트리 변경 금지 범위라 분리 워크트리를 만들 수 없다. PM의 355 passed를 받되 **이 보고서의 어떤 판정도 그 수치에 의존하지 않는다.**
- **"수정 전 되돌림"을 실제로 재현하지 않았다.** 파일 수정·git 상태 변경 금지라 `cp` 백업/원복을 할 수 없었다. 주장 B의 판정은 **각 가드의 단언을 읽고 수정 전 코드 경로를 손으로 따라간 논리적 추론**이다 — 가드 1의 "실패한다"와 가드 4의 "항진명제라 실패할 수 없다"는 논리적으로 확정되지만, 구현자가 실제 실행에서 무엇을 보았는지는 관측하지 못했다.
- **화면 캡처 없음.** T1은 백엔드 두 분기와 프런트 렌더 코드를 **읽어서** 도출했고 렌더링 결과를 보지 않았다. macOS에서는 `compute_path_visibility_windows` 분기가 `platform_now()`로 구조적으로 도달 불가능해 재현할 수단이 없다. **코드 기반 추정임을 명시한다** — 다만 두 파일의 값과 렌더 코드가 모두 하드코딩이라 추정의 불확실성은 "어떻게 보이는가"가 아니라 "얼마나 자주 보이는가"에만 있다.
- **워킹트리 파일을 한 번도 읽지 않았다.** 모든 인용은 `git show HEAD:<path>` / `git show 3e25317` / `git diff main..HEAD` / `git grep <pat> HEAD` 기준이다.
- **2차 보고의 사실 오류 1건을 이 보고서로 정정했다** — Unknown 후보를 4건으로 셌으나 실제는 3건이다(`classify.rs:399-404`의 `program_files_x86` 분기를 놓쳤다). 2차 보고서 파일은 수정하지 않았다(이력 보존).
- **실행 액션 없음.** 이 리뷰는 읽기 전용이었다. 소스 파일 수정·git 상태 변경(add/commit/push/checkout/stash/worktree)·병합·배포를 **하지 않았다.** 실행한 것은 `git log/show/diff/grep/ls-tree`(전부 읽기)와, `docs/reviewer/` 아래 이 보고서 1개 작성 + 페르소나 4개의 "적용 이력" append + `INDEX.md`의 "최근 재사용" 열 4행 갱신뿐이다(페르소나 기록은 리뷰 표준 산출물 게이트가 요구하는 항목이며 전부 `docs/reviewer/` 하위다).
