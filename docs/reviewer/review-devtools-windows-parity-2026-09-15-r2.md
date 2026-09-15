# 개발 환경 화면 Windows 완전 지원 재검토 보고서 (2차, 모드: 증분)

target_id: `devtools-windows-parity`
직전 리뷰: `docs/reviewer/review-devtools-windows-parity-2026-09-15.md` (판정: 🟡 Amber / Major 3건)
이번 리뷰 대상: `git diff main..HEAD` — 브랜치 `feat/devtools-windows-parity` (main `8c81e97` 대비 8커밋, 31파일 +4,332/−777). 이번 라운드 신규 커밋은 `3e0e387`(M2) · `25153c2`(M1·M3) · `79162c9`(문서).
리스크 범주: **로컬 실행 경계 / 설치 프로그램 구동** — 직전 리뷰와 동일(불변 확인).
작업 등급: Sensitive (노출 범위 축소 미적용 — 위임서에 축소 주장 없음)
리뷰 일자: 2026-09-15
리뷰 페르소나 패널: 6명 전원 재사용 / 신규 0

**종합 판정: 🟡 Amber** — Critical 0건, **Major 3건(전부 신규)**. 직전 M1·M2·M3은 해소 또는 부분 해소.
**머지 차단 사유: 있음 (N1·N2·N3).**

---

## 요약 (2분 규칙)

직전 3건의 처방은 **실제로 작동한다**. 특히 M1의 교차 불변식 테스트는 내가 `DEV_TOOLS` 6개 도구 × 14개 후보를 수정 전 분류기로 손수 추적해 **정확히 2건이 위반으로 잡힘을 독립 확인**했다 — 구현자 주장은 사실이고, 통과하도록 맞춘 테스트가 아니다.

그런데 그 테스트를 만들면서 드러난 것이 더 크다. **이 테스트는 지금 "거짓 문구"를 통과시킨다.** 위반 판정식이 `reason == UnknownMethod` 하나라, Windows Git·Node가 macOS 전용 문구(`"이 도구는 macOS 명령어 도구(Xcode CLT) 등 시스템이 제공합니다"` + `softwareupdate --list`)로 라우팅되는 것을 정상으로 인정한다(**N1**). 같은 축에서 프리뷰도 반대 방향으로 거짓말한다 — winget gh **업데이트** 프리뷰는 `preview_reliable: true`를 내보내(설계 §B.3 위반) "전체 업데이트" 배치에 자동 포함되고, 개별 확인 없이 UAC 승격을 띄운다(**N2**). M2가 "정상인데 실패라고 말함"이었다면 N2는 **"확인 못 했는데 확인했다고 말함"**이고, 실행 결과까지 따라온다.

`-EncodedCommand`(위임서 제1질문)는 **설계 의도를 훼손하지 않는다** — `-Command`도 애초에 명령을 창에 에코하지 않기 때문이다. 훼손되는 것은 사후 감사 가시성이고, 그건 디코딩되는 스크립트 앞에 에코 한 줄을 붙이면 오히려 macOS보다 나아진다(N5).

---

## 모드 판정 근거

**증분(Incremental) 모드.** 착수 전 두 관문을 순서대로 대조했다.

**① 풀패널 강제 승격 조건 (Skill `common-task-grading-and-verification-depth`) — 미해당**

| 승격 조건 | 대조 결과 |
|---|---|
| 직전 Critical/Major 미해결 존재 | 위임 시점 기준 3건 모두 수정 커밋이 존재(`3e0e387`·`25153c2`) — 미해결 상태로 넘어온 것이 아니다 |
| 직전에 없던 새 실행경로·리스크 표면 **다수** 등장 | **1개**(`-EncodedCommand`). "다수"에 미달 → 증분 트리거 |
| 다른 Sensitive 하위도메인 진입 | 없음 — 로컬 실행 경계 그대로(인증·배포·결제·개인정보 아님) |
| 재검토 4차 이상 누적 | 2차 |
| PM·사용자의 명시적 풀패널 요청 | 없음(위임서가 "모드 판정은 네가 해라"로 위임) |

**② 동일 대상 인정 4조건 — 전부 충족**(PM 판단을 그대로 받지 않고 직접 대조)

| 조건 | 확인방법 | 결과 |
|---|---|---|
| 동일 target_id | 위임서 + 직전 보고서 10행 | `devtools-windows-parity` 일치 |
| 직전 리뷰로부터 7일 이내 | 커밋 타임스탬프(`git log --date=iso-local`) | 같은 날(직전 17:24, 이번 커밋 17:28·17:45) |
| 리스크 범주 불변 | 직전 보고서 12행 vs 위임서 | 로컬 실행 경계/설치 구동 — 동일 |
| 리뷰 대상 파일 실질 중첩 | `git show --stat 25153c2 3e0e387` | 6파일 중 5개가 직전 리뷰 대상(`classify.rs`·`plan_table.rs`·`platform.rs`·`platform_tests.rs`·`cli_launcher.rs`) |

→ 증분 모드. **단 위임서 지시대로 `-EncodedCommand` 신규 실행경로는 최초 리뷰 수준으로 다뤘다**(호출부 전수 grep, 인코딩 함수 정독, 두 `#[cfg(windows)]` 테스트 정독, 사용자 가시성·EDR 표면까지 — N5·N6·N7).

---

## 페르소나 재사용 판정 (산출물 게이트)

착수 전 `docs/reviewer/personas/INDEX.md`를 Read해 "역할개념" 열을 스크리닝했다. **신규 0명** — 새 리스크 표면(`-EncodedCommand`)조차 기존 역할개념에 정확히 대응하는 페르소나가 이미 있었다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-process-execution-security.md` | **재사용** | `-EncodedCommand`가 argv·셸 인용 축의 변경이라 그대로 과녁. M3 판정 담당. 6대 요소 무수정. |
| `persona-claimed-vs-verified.md` | **재사용** | 위임서 최대 질문("테스트가 진짜 M1을 잡는가, 통과하도록 맞춘 것인가")이 이 페르소나의 기준 자체. CI 사실관계 정정(N7)도 이 축. 6대 요소 무수정. |
| `persona-heterogeneous-machine-operator.md` | **재사용** | N1·N3(Windows 실사용자가 실제로 보게 되는 문구·막다른 골목)이 전부 이 축. 6대 요소 무수정. |
| `persona-design-contract-auditor.md` | **재사용** | N2(설계 §B.3 "winget 프리뷰는 preview_reliable:false"가 install 경로에만 구현됨) 발견의 주체. 6대 요소 무수정. |
| `persona-delegated-authority-blast-radius.md` | **재사용 (이번 라운드 신규 투입)** | INDEX.md 15행 역할개념("앱이 자식 프로세스에 넘긴 권한의 크기를 사용자가 **사전에 알고 사후에 확인**할 수 있는가")이 새 표면과 정확히 일치 — 관심사 2(사전 고지)=N5, 3(사후 감사 표면)=`-EncodedCommand`, 1(권한의 크기)=N2의 배치 UAC. **신규 작성 대신 재사용으로 처리**(회전문 페르소나 안티패턴 회피). 6대 요소 무수정. |
| `persona-zero-base-redesigner.md` (발산) | **재사용** | 발산형 최소 1명 규칙 담당. R-4·R-5 제언. 6대 요소 무수정. |

신규 0건이므로 INDEX.md 행 추가 없음 — "최근 재사용" 열만 6행 갱신했다(`persona-delegated-authority-blast-radius.md` 포함).

---

## 직전 Major 3건 해소 여부

| ID | 직전 지적 | 해소 판정 | 근거(원본 라인 vs 신규 라인) |
|---|---|---|---|
| **M1** | Wrangler Windows 후보(`%LOCALAPPDATA%\pnpm\wrangler.cmd`, `\bin\` 없음)와 분류기(`\bin\` 유무로 판별)가 모순 → 조용히 manual 강등 | **✅ 해소 (독립 검증 완료)** | 아래 "M1 심층 검증" 참조 |
| **M2** | `preview_reliable=false`를 프런트가 "실패/시간 초과"로 거짓 서술 | **✅ 해소 (문구). 단 같은 클래스 결함이 반대 방향으로 잔존 → N2** | `devTools.ts:303`(토스트)·`484`(배너) 둘 다 원인 중립으로 교체 확인. `renderPreviewPanel`에서 `notes`가 배너 **바로 아래** 같은 패널에 렌더됨을 확인(`devTools.ts:490-492`) — 정확한 사유가 실제로 화면에 함께 나온다. `contract.rs` diff 0 재확인. |
| **M3** | Windows 인용 검증이 구현식 재작성(동어반복), 3계층 중 앞 2계층 무검증 | **🟠 부분 해소** | 아래 "M3 심층 검증" 참조 |

### M1 심층 검증 — "이 테스트가 진짜 M1을 잡는 물건인가"

**결론: 잡는다. 구현자 주장(수정 전 2건 실패)을 독립 재현으로 확인했다.**

검증 방법: 워킹트리를 건드리지 않기 위해 `git show 833cb20:classify.rs`(수정 전)와 `git show HEAD:mod.rs`(후보 표)를 각각 읽어, `DEV_TOOLS` 6개 도구 × `windows_path_candidates` 14개 전부를 **수정 전 분류기 규칙으로 손수 추적**하고 `UPDATE_TABLE`(`plan_table.rs:426-499`) 행 매칭까지 따라갔다.

| # | 후보 | 수정 전 분류 | `lookup_action` 결과 | 위반? |
|---|---|---|---|---|
| 3 | Claude `%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe` | `WingetPackage` | (Claude, WingetPackage) 행 없음 → 폴백 `MANUAL_UNKNOWN_METHOD` | **위반 1** |
| 13 | Wrangler `%LOCALAPPDATA%\pnpm\wrangler.cmd` | `PnpmStandalone` | (Wrangler, PnpmStandalone) 행 없음 → 폴백 `MANUAL_UNKNOWN_METHOD` | **위반 2** |
| 나머지 12 | — | Run 매칭 6건 / `Unknown`(테스트 제외 대상) 4건 / SystemManaged 2건 | — | 없음 |

→ **정확히 2건.** 구현자가 보고한 "Wrangler + 부수 발견 Claude/WingetPackage"와 일치한다. 테스트는 통과하도록 맞춘 물건이 아니다.

**회귀 여부(위임서 특별질의 1) — 없음.** `git diff 833cb20 HEAD -- classify.rs`의 **프로덕션 변경분은 Windows pnpm 분기 하나뿐**이다(나머지 483줄 삭제는 전부 테스트 모듈을 `classify_tests.rs`로 이관한 것). macOS 정본 `classify_install_method`와 npm shim 분기(`classify.rs:355-381`)는 **한 글자도 바뀌지 않았다**. 확인방법: diff에서 `^-` 줄을 추출해 프로덕션 코드 라인을 전수 확인 — 제거된 프로덕션 라인은 구 pnpm 분기 6줄이 전부.

부수 효과로 **잠복 버그 1건이 함께 닫혔다**: 구 코드는 확장자를 제거하지 않아 `\bin\` 레이아웃에서 `package: "wrangler.cmd"`를 만들었다(`pnpm update -g wrangler.cmd` → 확정 실패). 신규 코드의 `stem` 처리가 이를 제거한다.

**기존 테스트 1개 재작성 판정: 더 강하다.**
- 이전: 입력이 `…\pnpm\bin\wrangler.cmd` **하나**, 기대값 `package: "wrangler.cmd"`. 이 앱이 절대 만들지 않는 경로 + 틀린 기대값을 고정하고 있었다.
- 이후(`classify_tests.rs:374-421`): ①앱이 실제 선언한 후보(`\bin\` 없음) ②가상의 `\bin\` 레이아웃 ③`pnpm.exe` 자기 자신 — 3입력으로 넓히고, 기대값을 `"wrangler"`로 교정한 뒤 **`lookup_action(Wrangler, PnpmGlobalPackage)`가 `Run`인지까지 이어서 단언**한다. 분류만 맞고 라우팅이 없으면 여전히 통과하던 구멍을 닫았다.

**Claude/WingetPackage 처리 판정: 타당.** `MANUAL_CLAUDE_WINGET_UNSUPPORTED`(`plan_table.rs:398-413`)는 ①사실과 다른 문구("확인할 수 없습니다")를 사실 그대로("winget으로 설치된 것으로 보이지만 이 방식의 자동 업데이트는 지원하지 않습니다")로 바꾸고 ②`copyable_command: Some("claude update")` + `doc_url`을 줘 **다음 행동을 0개에서 2개로 만들었다** ③새 실행경로(winget→claude)를 열지 않고 설계 §B.2의 기존 기각 결정을 존중했다. 스코프 규율까지 지켰다.

### M3 심층 검증 — "이 상태로 머지 가능한가"

| 처방 | 상태 | 검증 근거 |
|---|---|---|
| ② 동어반복 제거(고정 리터럴 표) | **✅ 실효** | `platform_tests.rs:428-436`. `("'", "''''")` 등 8행. 내가 손으로 재계산해 일치 확인(`quote_token(Win,"'")` = `'` + `''` + `'` = 4따옴표). 구현식을 재실행해 만든 값이 아니다. |
| ① `-EncodedCommand` 전환 | **✅ 코드상 완결** | `cli_launcher.rs:132-134`. `git grep -- "-Command"` 전수 결과 **프로덕션 잔존 0건**(남은 것은 주석 4곳뿐). `spawn_terminal_window`가 3개 공개 진입점 전부의 유일한 스폰 지점이므로 우회로 없음. |
| ③ `encode_powershell_command` 순수함수 왕복 | **✅ 실효** | `platform_tests.rs:648-686`. 함수를 다시 호출하지 않고 손으로 base64 디코드 → UTF-16LE 재조립 → 원본 비교. 한글 케이스 포함. base64 알파벳 외 문자 부재까지 단언 — 이 처방의 핵심 값어치를 직접 검사한다. |
| ③ 실제 PowerShell 왕복(`#[cfg(windows)]` 2개) | **❌ 미실행 + 미컴파일** | N7 참조 — PM이 전달한 사실보다 **한 단계 더 나쁘다** |

**판정: 부분 해소.** 검증 표면은 3계층에서 1계층(PowerShell이 디코딩한 뒤의 스크립트 파싱)으로 실제로 줄었고, 그 1계층의 안전은 `''''` 이스케이프에 걸려 있다. 그 이스케이프가 PowerShell 파서에서 실제로 맞는지는 **여전히 아무도 관측하지 못했다.** 다만 이건 코드 결함이 아니라 검증 공백이고, 주석·테스트 코멘트가 그 공백을 정확히 명시한다(`platform_tests.rs:420-425, 549-560`). **머지를 막을 사유로는 보지 않는다** — 막는 것은 아래 N1~N3이다.

---

## 이번 diff 신규 지적

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| **N1** | 🟠 Major | 이기종머신 / 설계조항 | `plan_table.rs:366-372`(`MANUAL_XCODE_CLT`) ↔ `480-483`(`UPDATE_TABLE` SystemManaged 행) ↔ `classify.rs:390-395`(SystemManaged 판정) | 세 자리 Read + `git grep XcodeClt` 전수 + `install_resolver.rs:50-67`(`resolve_plan`)에 플랫폼 분기 부재 확인 | Windows에서 Git(`C:\Program Files\Git\cmd\git.exe`)·Node(`C:\Program Files\nodejs\node.exe`) — **둘 다 이 앱이 선언한 1번 후보이자 각 공식 설치기의 기본 경로** — 는 `SystemManaged`로 분류되고, `UPDATE_TABLE`의 `tool:None, SystemManaged` 행이 `MANUAL_XCODE_CLT`를 돌려준다: **"이 도구는 macOS 명령어 도구(Xcode CLT) 등 시스템이 제공합니다"** + `copyable_command: "softwareupdate --list"`. Windows 사용자는 6개 도구 중 2개에서 **존재하지 않는 OS의 안내와 실행 불가능한 명령**을 본다. 이 작업 전체가 없애려던 거짓 표시 그 자체이며, M1("확인할 수 없습니다" — 모호하지만 거짓은 아님)보다 나쁘다. `install_manual_plan`은 이미 mac/win으로 갈라져 있고(`plan_table.rs:520-525`) 가드 테스트(`809-841`)도 있지만, **그 가드는 미설치 경로만 덮고 업데이트 경로의 Manual 상수는 덮지 않는다.** | `install_manual_plan`과 동일하게 SystemManaged Manual을 플랫폼 분기: Win은 "Git/Node.js 공식 설치기(또는 winget)로 설치된 것으로 보입니다 — 앱이 자동 업데이트하지 않습니다" + `winget upgrade --id Git.Git` / `OpenJS.NodeJS.LTS`를 `copyable_command`로. 그리고 `install_manual_plan_windows_variant_uses_winget_not_brew`와 동형의 가드를 **업데이트 경로 Manual 상수 전체**(`MANUAL_XCODE_CLT`·`MANUAL_CASK_NOT_WRITABLE`·`MANUAL_NO_RUNNER`)에 확장 |
| **N2** | 🟠 Major | 설계조항 / 위임권한 / 주장vs관측 | `query.rs:170`(`None => (vec![label], "", true)`) ↔ `plan_table.rs:291-306`(`RUN_WINGET_UPGRADE_GH.preview_args: None`) ↔ `devToolsApi.ts:12-15`(배치 제외 규칙) | 세 자리 Read + `git grep preview_reliable` 전수(테스트 0건) | winget gh **업데이트** 프리뷰는 `preview_args`가 `None`이라 `build_run_preview`의 마지막 폴백을 타고 **`preview_reliable: true`, `affected: ["GitHub CLI"]`, `notes: ""`** 를 내보낸다. 설계 §B.3은 "winget 프리뷰는 `preview_reliable:false`"라고 단언했는데, 그 분기(`no_dry_run_install_notes`, `query.rs:196-200`)는 **설치 경로에만 있고 업데이트 경로에는 없다.** 결과 ①시뮬레이션한 적 없는 `winget upgrade`를 "영향받는 항목: GitHub CLI 하나"라고 **거짓 확언**한다 — M2의 정확한 역방향이며, 과잉 경고가 아니라 **거짓 안심**이라 더 위험하다 ②`previewReliable===true && affected.length===1`이므로 gh가 **"전체 업데이트" 배치에 자동 포함**되어 개별 확인 없이 실행된다 → 사용자가 버튼 하나를 눌렀는데 머신 스코프 winget의 **UAC 승격 창이 예고 없이 뜬다**. 직전 라운드 Q4의 "동의가 클릭으로 선행한다"는 판단이 이 경로에서만 성립하지 않는다 ③`preview_reliable` 기본값이 `true`라 앞으로 추가되는 모든 `preview_args: None` 플랜이 같은 함정에 자동으로 빠진다. 프리뷰 신뢰도를 검사하는 테스트는 **한 개도 없다** | 최소안(계약 불변, ~6줄): `build_run_preview`의 `None` 폴백을 러너별로 가른다 — `Runner::Winget`이면 `no_dry_run_install_notes`와 같은 문구 + `false`. 구조안: R-4(아래) |
| **N3** | 🟠 Major | 이기종머신 | `classify.rs:390-395`(`system_prefixes` 리터럴 2개) ↔ `mod.rs:113-124, 138-148, 176-181` ↔ `classify_tests.rs:539-560`(테스트의 `Unknown` 제외) | 14개 후보 전수 추적 + `plan_table.rs:392-397`(`MANUAL_UNKNOWN_METHOD`에 `copyable_command`·`doc_url` 모두 `None`) 확인 | **위임서 특별질의 2에 대한 답이 여기다.** `Unknown` 제외는 이번 라운드 스코프 판단으로는 합당하고 주석이 정직하게 공시했지만, **그 자리에 이미 결함 4건이 들어앉아 있다.** 14개 후보 중 4개가 `Unknown` → `MANUAL_UNKNOWN_METHOD`(다음 행동 0개)로 떨어진다: gh `C:\Program Files\GitHub CLI\gh.exe`(설계 §B.2가 **run**으로 단언한 도구의 1번 후보), git `C:\Program Files (x86)\Git\cmd\git.exe`(32비트 설치), git `%LOCALAPPDATA%\Programs\Git\cmd\git.exe`(**Git for Windows의 "only for me" 비관리자 설치 — 사내에서 가장 흔한 형태**), node `%LOCALAPPDATA%\Programs\nodejs\node.exe`. 증상은 M1과 **글자 그대로 동일**하다("설치 방식을 확인할 수 없어…", 복사할 명령도 문서 링크도 없음). 게다가 제외가 **판정식**(`matches!(method, Unknown(_)) → continue`)이라 **앞으로 추가되는 후보가 `Unknown`으로 떨어지면 아무 신호 없이 면제 집합에 합류한다** — 가드의 구멍이 시간이 갈수록 커지는 형태다 | ①`system_prefixes`를 `DEV_TOOLS`가 선언한 설치기 관리 경로 전부로 확장(`Program Files (x86)\Git\`, `%LOCALAPPDATA%\Programs\{Git,nodejs}\`, `Program Files\GitHub CLI\`) ②제외를 판정식이 아니라 **명시적 리터럴 허용목록**으로 바꾸고 `assert_eq!(실제_Unknown_집합, 허용목록)`으로 고정 — 새 후보가 조용히 합류하지 못하게 한다. ①이 어렵더라도 ②는 지금 넣어야 한다(그래야 N3이 다음 라운드에 자동으로 재부상한다) |
| **N4** | 🟡 Minor | 화면 정직성 | `devTools.ts:472-487` | 파일 Read | winget 경로는 `affected.length === 0` + `previewReliable === false`가 **동시에** 성립해 거의 같은 말의 경고 배너가 두 장 연속으로 쌓인다("⚠ 영향 범위를 확인하지 못했습니다 — 무엇이 바뀔지 알 수 없습니다." 바로 아래 "⚠ 미리보기로 영향 범위를 확인할 수 없어…"). 경고 인플레이션은 경고를 읽히지 않게 만든다 | `!previewReliable`일 때 `warnUnknownAffected` 배너를 억제(상위 개념이 이미 같은 말을 한다) |
| **N5** | 🟡 Minor | 위임권한 blast-radius | `cli_launcher.rs:121-141` ↔ `platform.rs:425-432` | 두 자리 Read + `spawn_terminal_window` 호출부 전수 | **위임서 제1질문에 대한 답.** `-EncodedCommand`는 **창 본문에 base64를 노출하지 않는다** — PowerShell은 `-Command`든 `-EncodedCommand`든 스크립트를 에코하지 않고 실행 결과만 찍기 때문이다. 즉 **설계 의도("사용자가 직접 보는 터미널 창")는 훼손되지 않는다.** 다만 이 전환으로 **사후 감사 표면이 닫힌다**: 이전에는 작업 관리자 "명령줄" 열이나 `Get-CimInstance Win32_Process`로 "앱이 뭘 실행했지?"를 사후 확인할 수 있었으나 이제 base64뿐이다. 그리고 macOS는 Terminal.app `do script`가 명령을 **창에 타이핑해 보여주는데**(`cli_launcher.rs:143-152`) Windows는 원래도 안 보여줬다 — 플랫폼 간 투명성 비대칭이 이 전환으로 고착된다. dev_tools의 "터미널에서 실행"은 앱 UI가 사전에 명령을 보여주지만(`devTools.ts:15-19`), `open_terminal_program` 계열(gh/wrangler 로그인, mcp add·login — 호출부 6곳)에는 그 사전 표시가 **없다** | 인코딩 **전** 스크립트 앞에 에코 한 줄을 붙인다: `Write-Host '> <명령>' -ForegroundColor DarkGray; <원래 스크립트>`. 인코딩의 이점을 그대로 두면서 창 안에서 실행 명령이 보이고, macOS와의 비대칭도 오히려 해소된다(에코 문자열은 `quote_token`으로 같이 인용) |
| **N6** | 🟡 Minor | 실행보안 / 이기종머신 | `cli_launcher.rs:134` | 파일 Read + B2 수정의 위협모델(`cli_launcher.rs:112-118` 주석) 대조 | `powershell.exe -NoProfile -EncodedCommand <base64>`는 EDR·AV가 가장 강하게 시그니처화하는 실행 패턴이다(Defender ASR "난독화 스크립트 실행 차단", 다수 Sysmon 규칙). 이 앱의 Windows 배포물은 **다운로드 폴더에 놓인 서명 없는 포터블 exe**이고(B2 수정이 직접 명시한 전제), 그것이 가장 의심스러운 형태로 PowerShell을 띄운다. 차단되면 gh/wrangler 로그인·mcp 설정·터미널 안내가 **전부** 동작하지 않는다. 이 머신에서도 CI에서도 확인할 수 없다 — **실기 PC에서만 드러나는 종류** | 실기 1호 PC 체크리스트의 **첫 항목**으로 올린다(사내 EDR 정책 하에서 터미널이 실제로 열리는가). 차단이 관측되면 즉시 🟠 Major로 승격하고 대안(임시 `.ps1` + `-File`, 또는 `-Command` 복귀 + N5 에코)으로 전환. 미리 고치지는 않는다 — 추측으로 M3의 이점을 되돌릴 근거가 없다 |
| **N7** | 🟡 Minor | 주장vs관측 | `.github/workflows/ci.yml`(미푸시) + `platform_tests.rs:561-637` | `git ls-remote --heads origin` + `gh run view 34945881995 --json headBranch,headSha,createdAt` + `git log --date=iso-local` | **위임서 전달 사실의 정정 2건.** ①브랜치 `feat/devtools-windows-parity`는 **origin에 존재하지 않는다**(원격 heads는 `main`과 `winparity-code` 둘뿐) ②PM이 인용한 windows-latest 성공 빌드(run 34945881995)는 `winparity-code@603c812`, 생성 시각 **08:14:33Z = 17:14 KST**에 돌았다. 그런데 M2 커밋은 17:28, **M1·M3 커밋은 17:45**다. 즉 그 빌드는 `ff5ce31`(17:11) 시점 코드를 컴파일한 것이고, **`encode_powershell_command`·`-EncodedCommand` 호출부·`ManualReason::UnsupportedMethod`·새 `#[cfg(windows)]` 테스트 2개는 Windows 타깃에서 한 번도 컴파일된 적이 없다.** 앞 셋은 `cfg(windows)`가 아니라 런타임 `match`/공용 코드라 macOS `cargo test` 349건이 컴파일을 보증하지만, **`#[cfg(windows)]` 테스트 2개는 어디서도 컴파일되지 않았다** — 문법·바인딩 오류가 있어도 지금은 아무도 모른다 | 머지 전: `winparity-code`를 현재 HEAD에서 ci.yml 커밋만 제외하고 다시 만들어 push → `tauri-portable-build` 1회 재실행. **컴파일만이라도** 확인된다(그 워크플로는 테스트를 돌리지 않는다는 한계 그대로). ci.yml 자체는 `workflow` 스코프가 있는 크리덴셜을 확보할 때까지 별건으로 분리 |

### 직전 Minor 5건(m4~m8) + n9 현황 — 위임서 특별질의 4

**이번 커밋들이 건드린 것은 0건이다.** 전부 그대로 남아 있다(확인방법: 각 위치 HEAD 기준 Read).

| ID | 위치 | 현황 | 머지 전 필수? |
|---|---|---|---|
| m4 | `plan_table.rs:415-421` | 미수정. `MANUAL_NO_RUNNER`가 여전히 "brew/npm/winget" 단일 문구 — macOS 사용자에게 `winget`을 안내한다 | **아니오.** 단 **N1과 같은 결함 클래스**이므로 N1을 고칠 때 한 번에 처리하는 것이 옳다(둘 다 "업데이트 경로 Manual 상수에 플랫폼 축이 없다") |
| m5 | `plan_table.rs:266-267` vs `299-301` | 미수정. install에는 `--accept-package-agreements`, upgrade에는 없음. 비대칭 근거 주석도 없음 | 아니오(미검증 Minor 유지) |
| m6 | `platform.rs:202-205` | 미수정. `rfind('\\')`만 봄 | 아니오(한 글자 수정, 도달 가능성 낮음) |
| m7 | `actions.rs` + `plan_table.rs:299-301` | 미수정. UAC 거부 종료코드 미매핑 | **N2가 격상시킨다** — 배치 실행 중 UAC 거부가 "설치 실패"로 표시되면 사용자가 원인을 알 수 없다. N2를 고치면(gh가 배치에서 빠지면) 영향이 개별 실행으로 한정되므로 m7은 Minor 유지 |
| m8 | `plan_table.rs:592-612` | 미수정. Git 문구에만 UAC 예고. pnpm 문구에 "새 터미널을 열어야 PATH 반영" 안내가 추가된 것은 별개 개선 | 아니오 |
| n9 | `platform.rs:150`, `classify.rs:254` | 미수정. "컴파일 호스트" 표현 그대로 | 아니오 |

---

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 실행보안 | `-EncodedCommand` 도입으로 명령행 길이 제한(32,767자)에 걸릴 수 있다 | **기각** | base64는 UTF-16LE(×2) 후 ×4/3 = 원본의 약 2.67배. 실제 최장 스크립트는 `build_chained_terminal_command_line`의 2커맨드 체인(`mcp_manager/mod.rs:222-228`)으로 수백 바이트 수준이다. MCP 서버 이름이 외부 출처라 이론상 길어질 수 있으나 `claude mcp list` 파싱 결과가 12,000자를 넘는 시나리오를 제시할 수 없다 — 재현 경로 없음 |
| 실행보안 | base64 문자열의 `+`·`/`가 Rust `Command::args`의 Windows 재인용이나 PowerShell 파라미터 파서에서 특수 처리된다 | **기각** | `platform_tests.rs:656-663`이 출력 알파벳이 `[A-Za-z0-9+/=]`뿐임을 단언한다. 공백·따옴표가 없으므로 Rust는 인용 없이 그대로 넘기고, 값 위치(파라미터명 위치가 아님)라 `/` 접두 파라미터로 해석될 자리가 아니다 |
| 주장vs관측 | `#[cfg(windows)]` 테스트 2번이 `Write-Output`에 위치 인자 3개를 넘겨 파라미터 바인딩 오류로 실패할 것이다 | **강등 → N7에 흡수** | `Write-Output -InputObject`가 `PSObject[]` 위치 파라미터라 위치 인자들이 배열로 묶이는 것이 표준 동작이다. 다만 이 머신에서 확인할 수단이 없고, **테스트 자체가 한 번도 컴파일·실행된 적 없다**는 더 근본적인 사실(N7)에 포함시킨다 |
| 이기종머신 | 워킹트리의 `autonomy/runner.rs`·`dev_tools/process.rs`·`mcp_manager/*` 변경이 이 브랜치와 충돌한다 | **스코프 밖** | 위임서 명시 제외(다른 세션의 동시 작업). `git diff main..HEAD`만 대상으로 삼았고 워킹트리 파일은 한 번도 직접 읽지 않았다 |
| 설계조항 | `session_list.rs` 1,174줄이 Rust 1,000줄 규율 위반 | **스코프 밖** | 직전 라운드와 동일 사유(main에 이미 있던 기존 위반) |

---

## 페르소나별 관점

### [실행 프로세스 보안] — 판정: 🟡 Amber
`-EncodedCommand` 전환은 **깨끗하다.** `git grep -- "-Command"` 전수에서 프로덕션 잔존 0건이고, `spawn_terminal_window`(`cli_launcher.rs:121`)가 3개 공개 진입점 전부의 유일한 스폰 지점이라 우회로가 구조적으로 없다. 인용 책임이 `quote_token` → `build_terminal_command_line` → `encode_powershell_command`로 한 줄에 꿰여 있고, 각 층이 무엇을 책임지고 무엇을 책임지지 않는지가 주석에 정확히 적혀 있다("이스케이프 규칙 자체는 무변경" — 실제로 `quote_token`은 한 글자도 안 바뀌었다). B1/B2/B3 수정(`ff5ce31`)에 대한 직전 라운드 판정은 이번 diff로 흔들리지 않았다.

남는 것은 검증 공백 하나와 배포 리스크 하나다: PowerShell 파서가 `''''`를 실제로 어떻게 읽는지는 여전히 미관측(M3 부분 해소), 그리고 `-EncodedCommand`가 EDR에 어떻게 보일지는 실기에서만 드러난다(N6). **둘 다 "지금 코드를 고쳐야 할 문제"가 아니라 "실기 1호 PC에서 무엇을 먼저 봐야 하는가"의 문제다.**

### [주장 vs 관측] — 판정: 🟠 Amber
위임서가 신뢰하라고 한 측정치 중 **검증 가능한 것은 전부 맞았고, 하나가 틀렸다.**
- ✅ `capabilities/default.json` diff 0 / `contract.rs` diff 0 — `git diff main..HEAD --stat`에 두 파일 모두 부재로 확인.
- ✅ 커밋 8개 구성 일치.
- ❌ "GH Actions windows-latest 릴리스 빌드 성공 → `#[cfg(windows)]` 코드가 사상 처음 실제 컴파일됐다" — **M1·M3 커밋에는 해당하지 않는다**(N7). 그 빌드는 31분 전 코드를 컴파일했다.
- ⏸ "커밋된 HEAD만 분리 워크트리에 체크아웃해 349 passed" — **재측정하지 않았다.** 워킹트리·git 상태 변경 금지 범위라 워크트리를 만들 수 없었고, `.claude/worktrees/`의 기존 워크트리는 다른 세션의 `f7457f8`(무관한 커밋)이었다. PM 측정치를 그대로 받되 **이 리뷰의 근거로 삼지 않았다** — 위 모든 판정은 소스 정독과 손 추적으로 도출했다.

M1 테스트의 진위 검증(이 라운드 최대 질문)은 **동어반복이 아님을 독립 추적으로 확인**했다. 그러나 같은 눈으로 보면 이번 라운드에도 "자기가 정한 규칙을 자기가 확인"하는 자리가 남아 있다 — N2(프리뷰 신뢰도를 검사하는 테스트가 0개)와 N3(가드가 판정식이라 면제 집합이 조용히 자란다).

### [이기종 머신 운영자] — 판정: 🔴→🟠 (Major 3건)
**Windows 사용자가 이 화면에서 실제로 보게 될 것을 6개 도구 × 14개 후보로 전부 따라가 봤다.** M1은 닫혔지만 같은 종류의 막다른 골목·거짓 안내가 더 많이 남아 있다.

| 도구 | Windows 설치 형태 | 화면에 나오는 것 | 다음 행동 |
|---|---|---|---|
| Git | 공식 설치기(전체 사용자) | **"macOS 명령어 도구(Xcode CLT)…"** + `softwareupdate --list` | 없음(거짓) — **N1** |
| Git | 공식 설치기("only for me") | "설치 방식을 확인할 수 없어…" | **0개** — N3 |
| Node | 공식 설치기 | **"macOS 명령어 도구(Xcode CLT)…"** | 없음(거짓) — **N1** |
| gh | winget(Links shim으로 해석될 때) | 프리뷰 "영향받는 항목: GitHub CLI" **(신뢰 가능 표시)** → 전체 업데이트에 자동 포함 → UAC | 있음(거짓 안심) — **N2** |
| gh | `C:\Program Files\GitHub CLI\gh.exe`로 해석될 때 | "설치 방식을 확인할 수 없어…" | **0개** — N3(설계는 run으로 단언) |
| Wrangler | pnpm 전역 | Run ✅ | M1 해소 확인 |
| Claude | winget | 전용 문구 + `claude update` ✅ | M1 부수 개선 |

즉 **6개 중 최소 2개(Git·Node)가 거짓 문구를, 최대 3개가 막다른 골목을 만날 수 있다.** "화면이 더 이상 전부 '설치 안 됨'으로 거짓말하지 않는다"는 1차 목표는 달성됐지만, **거짓말의 형태가 바뀌었을 뿐 사라지지는 않았다.** 이것이 머지 차단 판단의 핵심이다.

### [설계 계약 감사] — 판정: 🟠 Amber
직전 라운드에서 "조용한 누락"으로 잡은 §B.2는 M1 수정으로 닫혔다. **이번 라운드에는 §B.3에서 같은 형태가 나왔다.**

| 설계 조항 | 구현 위치 | 상태 |
|---|---|---|
| §B.2 "run 3 / manual 3 도구 단위 일치" | `classify.rs:339-357` + `classify_tests.rs:539-608` | **구현 + 강제 수단 확보** — 직전 지적이 요구한 교차 불변식 테스트가 정확히 그 형태로 들어왔다 |
| §B.3 "winget 프리뷰는 `preview_reliable:false`" | `query.rs:196-200`(설치) / **업데이트 경로 없음** | **절반만 구현(N2)** — 설치 경로만 조항을 지키고 업데이트 경로는 반대값을 낸다 |
| §B.2 "대안 A(winget→claude) 기각" | `plan_table.rs:398-413, 470-479` | **구현 + 기각 근거 주석 보존** — 새 실행경로를 열지 않고 문구만 정확히 한 것은 스코프 규율의 모범 |
| §E.1/§E.3 터미널 인용 책임 단일화 | `cli_launcher.rs:121-141`, `platform.rs:358-398` | 구현 — `-EncodedCommand`가 인용 책임 위치를 바꾸지 않았음을 확인 |
| 업데이트 경로 Manual 문구의 플랫폼 분리 | **조항 부재** | **설계서가 애초에 이 축을 다루지 않았다** — `install_manual_plan`만 플랫폼 분리했고 `UPDATE_TABLE`의 Manual 상수는 논의 대상이 아니었다. N1·m4가 그 공백에서 나왔다 |

§B.3이 §B.2와 **정확히 같은 방식**으로 깨졌다는 점이 중요하다 — 조항은 있는데 강제 수단이 없고, 구현자가 두 경로 중 한쪽만 손댔다. 처방도 같아야 한다: **조항마다 그것을 강제하는 테스트 한 개.**

### [위임 권한 blast-radius] — 판정: 🟠 Amber (이번 라운드 신규 투입)
이 페르소나는 "실행이 성공했을 때 무엇이 어디에 얼마나 일어나는가, 사용자가 사전에 알고 사후에 확인할 수 있는가"만 묻는다. 새 실행경로에 대해 셋 다 봤다.

- **권한의 크기**: `-EncodedCommand`는 권한을 **넓히지 않는다** — 같은 사용자 컨텍스트, 같은 `-NoProfile`, 같은 스크립트. 전달 형식만 바뀌었다. 권한 경계 이동 없음(따라서 등급 재판정 불필요).
- **사전 고지**: dev_tools 경로는 합격 — "터미널에서 실행"이 명령을 먼저 보여주고 확인을 받는다(`devTools.ts:15-19`). **`open_terminal_program` 계열 6개 호출부는 불합격** — gh 로그인 버튼을 누르면 사전 표시 없이 창이 뜬다(기존 동작, 이번 변경이 악화시키지도 개선하지도 않음).
- **사후 감사**: **이번 변경으로 후퇴했다.** 작업 관리자 명령줄이 base64가 된다(N5). 창 본문은 원래도 명령을 보여주지 않았으므로 창 자체는 무변화.
- **가장 큰 blast radius는 `-EncodedCommand`가 아니라 N2다.** 버튼 하나("전체 업데이트")의 동의 범위 안에 **머신 스코프 winget 업그레이드 + UAC 승격**이 사용자 모르게 포함된다. 직전 라운드 Q4가 "동의가 클릭으로 선행한다"로 UAC를 납득 가능하다고 판정했는데, 그 판정의 전제가 이 경로에서만 성립하지 않는다. N2를 고치면(gh가 배치에서 빠지면) Q4의 판정이 전 경로에서 회복된다.

---

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| **R-4** | `RunPlan.preview_args: Option<&[Arg]>` — 프리뷰 **능력**을 `None`이라는 부재로 표현하고, 부재는 `build_run_preview`의 마지막 폴백에서 **`preview_reliable: true`(신뢰 가능)로 해석**된다 | 능력을 **선언**하게 한다: `preview: PreviewCapability` 단일 필드 — `DryRun(&[Arg])` / `NoneButReliable(&'static str 사유)` / `NotOffered(&'static str 사유)`. 기본값을 없애 새 플랜 추가 시 셋 중 하나를 **반드시 고르게** 만든다 | **N2는 "판단을 적지 않았더니 낙관적 기본값이 대신 판단해 준" 결함이다.** npm/pnpm은 §7.1의 근거로 진짜 `NoneButReliable`이고 winget은 `NotOffered`인데, 현재 타입은 이 둘을 **구분할 자리 자체를 제공하지 않는다.** 선언형으로 바꾸면 ①N2가 컴파일 에러가 되고 ②`notes` 문구를 사유 문자열에서 바로 생성할 수 있어 `no_dry_run_install_notes`의 install/update 이중화가 사라지며 ③프리뷰 신뢰도 테스트(현재 0개)가 "모든 플랜의 선언이 실제 출력과 일치하는가"라는 구조적 순회 한 개로 끝난다 — `all_winget_run_plans_…pass_security_gate`가 이미 증명한 패턴 | 낮음~중간. `RunPlan` 리터럴 9개를 고쳐야 하지만 **기존 테스트는 건드리지 않는다**(프리뷰 신뢰도를 보는 테스트가 없으므로) — "기존 테스트 0개 수정" 제약과 충돌하지 않는다. 다만 이번 브랜치에 넣을지는 PM 판단: 최소안(6줄)으로도 N2는 닫힌다 |
| **R-5** | 도구 하나의 "Windows에서 무엇을 보여줄 것인가"가 여전히 흩어져 있다 — 미설치 문구는 `install_manual_plan_windows`(플랫폼 분리됨), 업데이트 문구는 `UPDATE_TABLE`의 공용 Manual 상수(**플랫폼 축 없음**), 분류 규칙은 `classify.rs`의 리터럴 접두사, 후보 경로는 `mod.rs` | 직전 R-1(도구당 1레코드)을 **플랫폼 × 상태 2차원으로** 확장: `{ platform, tool } → { detect: [경로], classify: method, when_installed: Plan, when_missing: Plan }`. 한 셀이 비면 컴파일이 깨지게 | **N1·N3·m4가 전부 "플랫폼 축이 어떤 표에는 있고 어떤 표에는 없다"에서 나왔다.** 직전 라운드의 R-1은 "5곳 분산"을 지적했는데, 이번 라운드가 보여준 것은 그 5곳이 **플랫폼 축을 일관되게 갖고 있지도 않다**는 점이다. 2차원 표로 접으면 "Windows Git의 업데이트 문구" 같은 칸이 비어 있을 수 없다 | 중간~높음. **이번 브랜치에는 넣지 말 것** — 직전 라운드 권고 그대로다. 실기 검증 후 별도 Refactor 등급 작업 |
| **R-6** | 가드 테스트가 "나쁜 것이 없는가"(`violations.is_empty()`)를 묻는다 — 제외 조건은 판정식이고, 제외된 집합의 크기는 아무도 보지 않는다 | 가드를 **집합 동일성**으로 바꾼다: `assert_eq!(현재_Unknown으로_떨어지는_후보_집합, 명시적_리터럴_허용목록)`. 줄어들면 허용목록을 줄이라고 테스트가 요구하고, 늘어나면 즉시 실패한다 | **N3의 본질은 결함이 아니라 가드의 형태다.** "없음을 단언하는 가드"는 면제 조건이 생기는 순간 조용히 무력화되고, 그 사실이 아무 신호도 만들지 않는다. 집합 동일성 가드는 미해결 항목의 개수를 **테스트 코드 안에 상시 게시**한다 — 다음 라운드의 리뷰어가 grep 한 번으로 남은 빚을 센다. 이번 라운드의 `checked == total_candidates` 단언("가드의 가드")이 이미 절반 그 정신이다 | 매우 낮음(테스트 1개 수정). **지금 넣을 수 있고 넣어야 한다** — N3의 처방 ②가 바로 이것이다 |

---

## 트레이드오프 (페르소나 간 충돌)

1. **검증 표면 축소(M3) vs 사후 감사 가시성·EDR 표면(blast-radius, 이기종머신)** — 실행보안 페르소나는 `-EncodedCommand`가 두 파싱 계층을 통째로 없앤 것을 이번 라운드 최고의 구조 개선으로 평가한다. blast-radius는 사후 감사 표면이 닫혔다고 보고, 이기종머신은 EDR 차단 가능성을 제기한다.
   → **권고: `-EncodedCommand`를 유지한다.** N5의 에코 한 줄이 두 반대 의견을 **동시에** 해소한다(창에서 보이고, 창 자체가 감사 기록이 된다). EDR(N6)은 추측으로 되돌릴 사안이 아니라 실기 1호 PC에서 **가장 먼저 확인할 항목**이다.

2. **계약 동결(`contract.rs` diff 0) vs 프리뷰 정직성** — 설계계약감사는 계약 불변을 이번 작업의 모범 사례로 유지 평가하고, 주장vs관측은 N2가 그 동결 때문에 생겼다고 볼 여지를 제기한다.
   → **권고: 계약은 그대로 둔다. N2는 계약 문제가 아니다.** `preview_reliable`이라는 필드가 부족한 게 아니라 **백엔드가 그 필드에 틀린 값을 넣고 있다.** `previewUnavailableReason` 신설은 **지금 하지 않아도 된다**(위임서 질의에 대한 답) — 필드를 늘려도 값이 `true`면 소용없다. 순서는 ①N2 최소안으로 값을 바로잡고 ②실기 피드백으로 "Windows 사용자는 전체 업데이트에서 gh가 항상 빠진다"는 마찰이 실제 문제인지 확인한 뒤 ③그때 R-4로 구조를 정리한다.

3. **머지 속도 vs 남은 거짓 표시** — 이기종머신은 직원 90%가 지금 이 화면에서 거짓을 보고 있으므로 빠른 배포를 지지해 왔다. 그러나 **이번 라운드에서 같은 페르소나가 머지 차단으로 돌아섰다** — Git·Node 2개 도구에서 macOS 문구가 나오는 상태로 배포하면 "Windows 지원을 했다"는 주장 자체가 첫 실행에서 무너진다.
   → **권고: N1·N2·N3을 고친 뒤 머지.** 세 건 모두 처방이 국소적이고(각각 문구 분기 / 6줄 / 테스트 형태 변경) 기존 테스트를 수정하지 않는다. N3의 ①(접두사 확장)이 무겁다고 판단되면 ②(허용목록 가드)만으로도 머지 가능 — 결함은 남지만 **다음 라운드에 자동으로 재부상하는 형태**가 되기 때문이다.

---

## 잘 된 점 (유지할 패턴)

1. **지적을 받은 테스트를 "통과시키는" 대신 "더 강하게" 고쳤다** — `classifies_pnpm_standalone_and_global_package_on_windows`는 입력을 1개→3개로 늘리고, 기대값을 교정하고, 라우팅 단언까지 이어 붙였다. 지적당한 부분만 최소로 손대는 것이 일반적인데 그 반대로 갔다.
2. **부수 발견을 숨기지 않고 함께 고쳤다** — Claude/WingetPackage는 위임 범위 밖이었는데 교차 테스트가 잡아내자 **스코프를 넓히지 않는 방식으로**(새 실행경로를 열지 않고 문구만 정확히) 처리하고 그 판단 근거를 주석에 남겼다(`plan_table.rs:398-405`). "발견했으나 고치지 않았다"도 "발견해서 다 고쳤다"도 아닌 세 번째 선택지다.
3. **제외한 것을 목록으로 공시했다** — `classify_tests.rs:520-538` 주석이 `Unknown` 제외 대상 4개를 **도구·경로 단위로 나열**하고 "후속 조사 대상, 미검증"이라 명시했다. N3은 이 공시가 없었다면 다음 라운드에도 발견되지 않았을 것이다. 형태만 판정식→허용목록으로 바꾸면(R-6) 공시가 기계적으로 강제된다.
4. **순수함수 왕복 테스트가 구현을 재호출하지 않는다** — `encode_powershell_command_round_trips_via_manual_utf16le_base64_decode`(`platform_tests.rs:648-686`)는 base64를 손으로 디코드하고 UTF-16LE를 손으로 재조립해 비교한다. M3 지적("구현식을 테스트가 재작성")을 받고 만든 테스트가 같은 함정을 반복하지 않았다.
5. **1,000줄 규율을 같은 패턴으로 반복 적용했다** — `classify.rs` → `classify_tests.rs` 분리가 `platform.rs`/`platform_tests.rs`와 동형이고, 정본(분류 함수)은 원 파일에 남겼다. 규율 준수가 임기응변이 아니라 확립된 패턴이 됐다.
6. **미검증을 문장 단위로 구분해 적는다** — "인코딩 함수 자체는 순수함수라 이 머신에서 100% 검증했고, PowerShell이 이 형식을 기대한다는 사실은 공식 문서 근거이며, 실제 디코딩·실행은 미검증"(`platform.rs:419-424`). 한 문단 안에서 검증 수준 3단계를 구분했다. 이 규율 덕에 N7 같은 사실관계 정정이 **코드가 아니라 CI 상태에서만** 나왔다.

---

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| M1 수정이 실제로 결함을 닫는가 | 주장vs관측 | 필수 | ✅ | 14개 후보 손 추적으로 독립 확인 |
| M1 수정이 macOS·npm 경로에 회귀를 만들지 않는가 | 이기종머신 | 필수 | ✅ | 프로덕션 diff가 Windows pnpm 분기 6줄뿐 |
| 교차 불변식 테스트가 동어반복이 아닌가 | 주장vs관측 | 필수 | ✅ | 수정 전 코드에서 2건 실패함을 독립 재현 |
| 그 테스트의 제외 조건이 결함을 숨기지 않는가 | 이기종머신 | 필수 | ❌ | **N3** — 4건이 이미 그 자리에 있고, 제외가 판정식이라 자동 증식 |
| 화면이 거짓을 말하지 않는가 | 이기종머신 | 필수 | ❌ | **N1**(macOS 문구), **N2**(거짓 안심) |
| 설계 §B.3 winget 프리뷰 신뢰도 | 설계조항 | 필수 | ❌ | **N2** — 설치 경로만 구현 |
| 설계 §B.2 run/manual 도구 단위 일치 | 설계조항 | 필수 | ⚠️ | M1은 해소. gh가 N3에 걸리면 재발 |
| `-EncodedCommand`가 실행 권한을 넓히지 않는가 | 위임권한 | 필수 | ✅ | 전달 형식만 변경, 권한 경계 이동 없음 |
| `-EncodedCommand`가 설계 의도(사용자가 보는 창)를 훼손하지 않는가 | 위임권한 | 필수 | ✅ | `-Command`도 에코하지 않았다 — 창 본문 무변화(N5는 사후 감사 축) |
| Windows 인용이 독립 검증되는가 | 실행보안 | 필수 | ⚠️ | 동어반복은 제거. 실기 왕복은 여전히 미관측(M3 부분) |
| 새 코드가 Windows 타깃에서 컴파일되는가 | 주장vs관측 | 권장 | ❌ | **N7** — `#[cfg(windows)]` 테스트 2개는 미컴파일 |
| 미검증 항목이 정직하게 표기됐는가 | 주장vs관측 | 필수 | ✅ | 주석·테스트 코멘트 전반에 일관. 이번 라운드에도 유지 |
| 실패 시 막다른 골목이 아닌가 | 이기종머신 | 권장 | ❌ | **N3** — 최대 3개 도구가 다음 행동 0개 |

---

## PM에게 권고

**머지 차단 사유: 있음 — N1·N2·N3.**

**우선순위 1 (머지 전 필수)**
- **N1** — Windows Git·Node가 "macOS 명령어 도구(Xcode CLT)" 안내를 받는다. 6개 중 2개, 100% 재현, Windows PC 없이도 확정된 사실이다. **이 상태로 배포하면 첫 실행에서 "Windows 지원"이라는 주장 자체가 무너진다.** m4를 같이 처리할 것(같은 결함 클래스).
- **N2** — winget gh 업데이트 프리뷰가 `preview_reliable: true`를 낸다. 거짓 안심 + 배치 자동 실행 + 예고 없는 UAC. 최소안 6줄.
- **N3** — 최소한 처방 ②(제외를 판정식→명시적 허용목록으로, R-6)만이라도. 테스트 1개 수정이고, 이것만 해두면 남은 4건이 다음 라운드에 자동 재부상한다.

**우선순위 2 (머지 후 / 실기 1호 PC 전)**
- **N7** — `winparity-code`를 현재 HEAD 기준으로 다시 push해 `tauri-portable-build` 1회. `#[cfg(windows)]` 테스트 2개의 **컴파일만이라도** 확인한다. ci.yml은 `workflow` 스코프 확보 시까지 별건 분리.
- **N5** — 디코딩 스크립트 앞 에코 한 줄. 사후 감사 표면을 되살리고 macOS와의 투명성 비대칭까지 해소한다.
- **N4** — 경고 배너 중복 억제.

**우선순위 3 (실기 1호 PC 체크리스트 — 순서대로)**
1. **N6: EDR이 `-EncodedCommand`를 차단하는가** — 차단되면 gh/wrangler 로그인·mcp·터미널 안내가 전부 죽는다. 다른 무엇보다 먼저.
2. M3 잔여: `''''` 이스케이프가 PowerShell에서 실제로 되돌려지는가(악성 MCP 서버 이름 1건으로 재현).
3. pnpm Windows 실제 레이아웃(`%LOCALAPPDATA%\pnpm\` 직하인가 `\bin\` 하위인가) — 현재는 양쪽 다 방어했으나 어느 쪽인지 모른다.
4. gh 실제 설치 자리(`C:\Program Files\GitHub CLI\` 직하인가 `\bin\` 하위인가) — N3의 gh 항목이 실제 문제인지 죽은 후보인지가 여기서 갈린다.
5. m5(winget upgrade 동의 플래그), m7(UAC 거부 종료코드).

**우선순위 4 (백로그)** — m6, m8, n9, R-4, R-5.

**배포 방식** — 직전 라운드 권고 유지: N1~N3 수정 후 **1~2명 선배포**. 화면이 `installMethod`와 해석된 경로를 노출하므로(`devTools.ts:385`) N1·N3의 실제 발현 여부를 스크린샷 한 장으로 회수할 수 있다.

---

## 정직 보고 — 이 리뷰가 확인하지 못한 것

- **Windows 실기 동작은 이 리뷰도 전혀 검증하지 못했다.** "Windows에서 이렇게 동작한다"고 단정한 곳은 없다. N1·N2·N3은 **코드 내부의 표 대조와 제어 흐름 추적**으로 성립하는 지적이라 실기 없이도 확정되지만, 그 경로에 실제로 도달하는 빈도(예: 사내 Git 설치가 전체 사용자인지 "only for me"인지)는 모른다.
- **`cargo test`를 재실행하지 않았다.** git 상태·워킹트리 변경 금지 범위라 분리 워크트리를 만들 수 없었다. PM의 349 passed 측정치를 받되 **이 보고서의 어떤 판정도 그 수치에 의존하지 않는다** — 전부 소스 정독과 손 추적이다.
- **`#[cfg(windows)]` 테스트 2개의 컴파일 가능성을 확인하지 못했다.** macOS에서는 `cfg`로 제외되고, Windows 타깃 `cargo check`는 공유 중인 `src-tauri/target`을 건드리게 되어 실행하지 않았다(다른 세션 동시 작업). N7로 남긴다.
- **화면 캡처 없음.** UI 지적(N1·N2·N4·N5)은 소스를 읽어 도출했고 렌더링 결과를 보지 않았다. macOS에서는 winget·Windows 분류 경로가 구조적으로 도달 불가능해 재현할 수단이 없다. 코드 기반 추정임을 명시한다.
- **워킹트리 파일을 한 번도 읽지 않았다.** 모든 인용은 `git show HEAD:<path>` / `git show <sha>` / `git diff main..HEAD` 기준이다. 따라서 `autonomy/runner.rs`·`dev_tools/process.rs`·`mcp_manager/*`의 미커밋 변경은 이 리뷰에 섞이지 않았다.
- **N6(EDR)은 업계 통념에 근거한 리스크 제기이지 관측이 아니다.** 사내 엔드포인트 정책을 모르고, 특정 제품이 이 패턴을 차단한다고 확인하지도 않았다. 그래서 Major가 아니라 Minor + 실기 체크리스트 1번으로 두었다.
- **실행 액션 없음.** 이 리뷰는 읽기 전용이었다. 파일 수정·git 상태 변경(add/commit/push/checkout/stash)·병합·배포를 **하지 않았다.** 실행한 것은 `git log/show/diff/grep/ls-remote/cat-file`(전부 읽기), `gh run view`(읽기), 그리고 `docs/reviewer/` 아래 이 보고서 1개 작성 + 페르소나 6개의 "적용 이력" append + `INDEX.md`의 "최근 재사용" 열 갱신뿐이다(페르소나 기록은 리뷰 표준 §6 산출물 게이트가 요구하는 항목이라 수행했고, 전부 `docs/reviewer/` 하위다).
