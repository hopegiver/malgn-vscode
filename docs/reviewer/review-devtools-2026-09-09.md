# 개발 환경 실설치/업데이트(devtools) 리뷰 보고서 — 사후(post-hoc)

리뷰 페르소나 패널: `docs/reviewer/personas/persona-process-execution-security.md`, `persona-claimed-vs-verified.md`, `persona-heterogeneous-machine-operator.md`, `persona-zero-base-redesigner.md`(발산형)
리뷰 대상: `src-tauri/src/dev_tools.rs`, `src-tauri/src/cli_launcher.rs`(`resolve_binary_expand_home`), `src-tauri/src/lib.rs`(devtools 등록부), `src/devToolsApi.ts`, `src/views/devTools.ts`, `src/mockData.ts`·`state.ts`·`main.ts`(devtools 부분)
**리뷰 기준 리비전: HEAD `261590d`** (도입 커밋 `c88f296`). 워킹트리에 미커밋 변경이 진행 중이어서 HEAD 스냅숏으로 고정해 리뷰했다(아래 "프로세스 관찰" 참조). 모든 라인번호는 HEAD 기준.
리스크 범주: **사용자 머신 비가역 변경(로컬 전역 개발도구 설치·업데이트 명령 실행)**
target_id: `malgn-vscode-devtools-real-update` / 1차(최초, 풀패널) / 등급 Sensitive
리뷰 일자: 2026-09-09
**종합 판정: 🟡 Amber** — Critical 0건. 이미 public에 나간 상태에서 되돌릴 필요는 없으나, Major 4건은 손보고 다음 릴리스에 태우는 것을 권고한다.

## 요약 (2분 규칙)
설계 정본(결정 1~6 + 부록 A/B/C)의 **안전장치 핵심은 실제로 구현돼 있다** — argv 화이트리스트·경로 파싱 formula(`node@22`)·셸 미경유·stdin null·리더 스레드·프로세스 그룹 kill·전역 뮤텍스·버전 델타 판정·plan_id 해시 게이트가 모두 코드와 테스트로 확인됐고, 프론트-백엔드 계약은 필드·enum variant 전수 일치한다. 지금 당장 고칠 값어치가 있는 것은 네 가지다: ① 자식 PATH 선두에 **빈 엔트리(=CWD)**가 들어갈 수 있는 1줄 결함, ② brew 쓰기권한 검사 대상이 설계의 `<prefix>/Cellar`가 아니라 `<prefix>`라 **Intel 맥(`/usr/local` = root:wheel)에서 기능이 통째로 죽을** 가능성, ③ `installed:false + actionKind:"run"` 조합에서 **plan_id 종류가 어긋나 영구 실패 루프**, ④ "전체 업데이트"가 preview가 없는 계획(claude/npm/pnpm)을 **사용자 확인 없이 즉시 실행**하는데 파일 상단 주석은 "실행 전 항상 사용자 확인"이라고 단언한다.

---

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 (HEAD 기준) | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| 1 | 🟠 | 실행보안 | `src-tauri/src/dev_tools.rs:963-968` | 함수 정독 + Rust std 문서(`Path::new("foo").parent() == Some("")`) | `build_child_path_env`가 runner 경로의 `parent()`를 무조건 push한다. runner가 bare name(`"node"`, `"brew"` — `resolve_binary`의 PATH 폴백 결과)이면 parent가 **빈 문자열**이 되어 자식 PATH가 `":/opt/homebrew/bin:…"`이 된다. POSIX/Rust에서 빈 PATH 엔트리 = **CWD**이며, Rust `Command`는 자식 env의 PATH로 프로그램을 탐색하므로 CWD의 동명 파일이 먼저 실행될 수 있다. 도달 조건: 절대경로 후보가 전부 없고 PATH 폴백이 성공(주로 `pnpm tauri dev`) | `dirs.push(...)` 전에 `!bin_dir.as_os_str().is_empty()` 필터 추가(1줄). 겸사겸사 bare-name resolve 결과는 아예 실행 계획에서 제외하는 것도 검토 |
| 2 | 🟠 | 이기종머신 | `src-tauri/src/dev_tools.rs:667` (설계 부록 B.2 대비) | 코드 대조 + 이 머신 `ls -ld /usr/local` → `drwxr-xr-x root wheel` | 설계는 "대상 prefix(`<brew_prefix>/Cellar`, `npm root -g`, claude 설치 디렉터리)의 쓰기 가능 여부"를 검사하라고 했는데, 구현은 **prefix 루트**(`/opt/homebrew`, `/usr/local`)를 검사한다. Catalina 이후 Intel 맥에서 Homebrew는 `/usr/local` 자체를 root:wheel로 두고 `/usr/local/Cellar`·`bin` 등 하위만 사용자 소유로 만든다 → 사내 Intel 맥에서는 **모든 brew formula가 `Manual(NotWritable)`로 강등**돼 기능이 통째로 비활성화될 수 있다. (arm 전용 머신에서만 실측 가능해 Intel 실기 검증은 못 했다 — **미검증 추정**이나, 설계와의 문언 불일치 자체는 확정) | 검사 대상을 `Path::new(prefix).join("Cellar")`로 변경. npm/claude 경로에도 같은 사전검사 확대(지적 #7) |
| 3 | 🟠 | 정직성/이기종 | `src/views/devTools.ts:117` ↔ `src-tauri/src/dev_tools.rs:1315`, `1599-1607` | 상태 교차표 작성 후 코드 대조 | `check_dev_tools_blocking`은 `installed = version.is_some()`인데 `action_kind`는 `resolve_plan` 결과라 **`installed:false` + `actionKind:"run"`** 조합이 나올 수 있다(바이너리는 있으나 `--version`이 실패/타임아웃). 이때 UI는 "설치" 버튼 → `handleRequestPreview`는 `build_run_preview`의 `compute_plan_id`를 받고, `runPlan`은 `installDevTool`을 호출 → `perform_install`은 `compute_plan_id_for_manual`로 대조 → **항상 불일치 → "다시 미리보기를 요청해주세요"**. 다시 눌러도 같은 결과. 설계 결정 3의 "막다른 골목이 아닌 형태" 원칙 위반 | ① UI 분기를 `tool.installed`가 아니라 preview의 `willRun`/백엔드가 주는 명시적 `intent`로 바꾸거나, ② `perform_install`이 `resolve_tool_path`가 Some이면 update 경로로 위임. 최소 조치로는 `installed`를 `resolve_tool_path().is_some()`로 두고 버전 미확인을 별도 필드로 분리 |
| 4 | 🟠 | 정직성 | `src/views/devTools.ts:184-191` (주석 `devTools.ts:4-6`, `src/devToolsApi.ts:8-10`) | 함수 정독 + `dev_tools.rs:1402`(`preview_args: None` 시 `affected = [label]`) 대조 | "전체 업데이트"는 `preview.affected.length > 1`일 때만 확인 대기로 남기고 그 외에는 **즉시 실행**한다. `RUN_CLAUDE_UPDATE`·`RUN_NPM_GLOBAL`·`RUN_PNPM_SELF_UPDATE`는 `preview_args: None`이라 항상 `affected.length == 1` → **사용자 확인 화면을 한 번도 거치지 않고** `claude update` / `npm install -g <pkg>@latest` / `pnpm self-update`가 실행된다. 그런데 두 파일 상단 주석은 "실행 전 **항상** … 무엇이 바뀔지 보여주고 사용자 확인을 받은 뒤"라고 단언한다 → 코드가 자기 문서와 어긋남. (의존성 연쇄라는 최악 케이스는 `>1` 가드가 막고 있어 Critical은 아니다) | ① 최소: "전체 업데이트" 클릭 시 실행될 명령 목록을 한 번에 보여주고 1회 확인받기, ② 또는 주석을 실제 동작에 맞게 정정("의존성 연쇄가 감지된 항목만 개별 확인") |
| 5 | 🟡 | 실행보안/이기종 | `src-tauri/src/dev_tools.rs:519` + `1624-1643` | 상수 정독 + `open_manual_instruction` 흐름 추적 | `MANUAL_CASK_NOT_WRITABLE.copyable_command`가 `"brew upgrade --cask <name>"` — **플레이스홀더가 그대로** 남아 있다. "터미널에서 열기"를 누르면 Terminal.app에서 이 문자열이 실행되고 `<name>`은 zsh 입력 리다이렉트로 해석돼 `no such file` 에러만 난다. 안내가 안내 역할을 못 한다 | cask 이름은 `InstallMethod::HomebrewCask{cask}`에 이미 파싱돼 있다 → `copyable_command`를 정적 상수가 아니라 판별 결과로 채우거나(검증 통과 필수), 최소한 터미널 실행 버튼을 숨기고 복사만 제공 |
| 6 | 🟡 | 정직성 | `src-tauri/src/dev_tools.rs:914` vs `1549-1557` | 두 분기 대조 | `spawn_error`(실행 자체 실패 사유)가 `perform_update`에서 **어디에도 실리지 않는다**. `exit_code:None` + `timed_out:false`로 else 분기에 떨어져 사용자는 "실행이 실패했습니다(종료 코드 알 수 없음)"만 보고, `log_tail`도 `"\n"`뿐이라 원인을 알 수 없다 | `spawn_error`가 Some이면 별도 메시지로 승격하고 `log_tail`에 원문 포함 |
| 7 | 🟡 | 실행보안 | `src-tauri/src/dev_tools.rs:667` (설계 부록 B.2) | 코드 정독 | 쓰기권한 사전검사가 **Brew runner에만** 걸려 있다. `NpmGlobal` 계획(`npm install -g`)은 prefix가 root 소유여도 그대로 실행되어 EACCES로 실패한다. `stdin(null)` 덕에 sudo 정지는 없지만, 설계가 "sudo가 필요한 계획은 애초에 만들지 않는다"고 한 원칙에는 못 미친다 | `NpmGlobal`도 `<npm prefix>/lib/node_modules` 쓰기 검사 후 `Manual(NotWritable)`로 강등 |
| 8 | 🟡 | 정직성 | `src-tauri/src/dev_tools.rs:1384-1390` | `brew upgrade --dry-run --formula gnupg` 읽기전용 실측 후 파서 대조 | 실측 dry-run은 의존성/요청 패키지/의존 대상 3개 섹션을 모두 `A x -> y` 형태로 뱉고, 파서는 **자기 자신(gnupg)까지 포함해** 8개를 담는다. 그런데 notes 문구는 "요청한 도구 **외에** {N}개"라 off-by-one이며, 나열 목록에도 자기 자신이 섞인다(`devTools.ts:324`의 `affected.length - 1`은 이걸 보정하지만 백엔드 문구는 안 함) | notes 생성 시 대상 formula를 목록에서 제외하고 개수를 세기 |
| 9 | 🟡 | 이기종머신 | `src-tauri/src/dev_tools.rs:1130-1139` | 함수 정독 + 이 머신 rc 파일 grep(`~/.zshrc:8`만 매치) | `rc_files_contain`이 **확장된 절대경로 문자열의 리터럴 포함**만 본다. 실제 rc 파일은 보통 `export PATH="$HOME/.local/bin:$PATH"`, `$PNPM_HOME`, `~/…` 형태로 쓴다 → 멀쩡히 PATH에 있는데도 `pathVisible:false`가 되어 "설치는 됐지만 터미널에서 바로 쓸 수 없습니다" **오경고**가 뜬다. (이 머신에서는 `/etc/paths.d/homebrew` = `/opt/homebrew/bin` 덕에 우연히 통과) | `$HOME`/`~`/`$PNPM_HOME`를 확장한 변형 문자열도 함께 매칭하거나, 오경고 비용이 크면 이 진단을 "참고" 톤으로 낮추기 |
| 10 | 🟡 | 정직성 | `src-tauri/src/dev_tools.rs:1492-1496`, `1528-1536` | 분기 조건표 재작성 | `normalized_before`가 `unwrap_or_default()`로 **빈 문자열**이 될 수 있고, 그 상태에서 after가 잡히면 `"" != after` → `Updated` + `verified:true` + `"( → 2.3.4)"` 메시지. 실제로는 "이전 버전을 못 읽었을 뿐"인데 업데이트됐다고 단언한다 | before가 None이면 `UnknownAfter`에 준하는 별도 처리(또는 `verified:false`) |
| 11 | 🟡 | 정직성 | `src-tauri/src/dev_tools.rs:1549-1557` (설계 결정 4 표) | 설계 표와 코드 대조 | 설계는 `exit!=0`이어도 "version 변화가 있으면 메시지에 병기"라고 했으나 구현은 버전 델타를 무시하고 종료 코드만 말한다. 부분 성공(brew가 일부만 올리고 실패)에서 사용자가 상태를 오판할 수 있다 | Failed 메시지에 `before → after`가 다르면 병기 |
| 12 | 🟡 | 실행보안 | `src-tauri/src/cli_launcher.rs:27` | 함수 정독 + 신규 호출부 수 계산 | PATH 폴백이 `Command::new(bare).arg("--version").output()` — **타임아웃 없음, stdin 미차단, 결과 무시**. `dev_tools.rs`가 이 함수를 도구 6종 + brew + npm으로 확대 호출하므로 화면 1회 로드에서 최대 8회 무제한 대기 스폰이 생긴다(신규 코드가 기존 결함의 노출을 넓힌 형태) | `output()` 대신 `run_process_with_timeout`(이미 있음)을 재사용하거나 stdin null + 짧은 타임아웃 적용 |
| 13 | 🟡 | 실행보안 | `src-tauri/src/dev_tools.rs:1417-1462` vs `1471` | 락 획득 지점 확인 | `EXECUTION_LOCK`은 `perform_update`에만 걸려 있고 `perform_preview`(brew `--dry-run` 실제 실행)는 락 밖이다. 실행 중 다른 도구의 미리보기를 요청하면 brew 전역 락 충돌로 무의미한 실패가 날 수 있다(현재 UI는 직렬화하지만 계약상 막혀 있지 않다) | preview도 같은 뮤텍스로 보호하거나 별도 "brew 사용 중" 가드 |
| 14 | 🟡 | 정직성 | `src-tauri/src/dev_tools.rs:1051-1058`(plan_id) + 부록 A | 해시 입력 정독 + `BREW_ENV`(`HOMEBREW_NO_AUTO_UPDATE` 미설정) 확인 | plan_id는 `runner_path ‖ argv ‖ normalized_before`만 해시한다 — 사용자가 실제로 승인한 것은 **dry-run이 보여준 영향 목록**인데 그 목록은 해시에 없다. 실행 시 auto-update로 인덱스가 갱신되면 같은 plan_id로 **다른 집합**이 업그레이드될 수 있다. (설계 그대로의 구현이므로 구현 결함이 아니라 설계 한계 — 그래서 🟠에서 강등) | 최소: 프리뷰 패널에 "실제 실행 시 목록이 달라질 수 있습니다" 명시. 근본: 제언 R3 참조 |
| 15 | ⚪ | 정직성 | `src/styles.css:420` | 클래스 정의 grep(스코프 밖 파일이지만 이 클래스만 확인) | `verified/unverified` 표기가 11px faint 회색으로 동일 — 가장 중요한 정직성 신호가 가장 낮은 시각 위계다. 다만 `unknownAfter/timedOut`은 패널 톤이 amber라 실질 구분은 된다 | unverified일 때만 강조 색·아이콘 부여 |
| 16 | ⚪ | 전체 | `src-tauri/src/lib.rs:1131, 1631, 1637, 1660, 2099` | `cargo clippy --all-targets` 직접 실행(워킹트리 기준 5건) | clippy 경고는 전부 **lib.rs의 사용량 집계·파일워처 코드**(`sort_by_key`, `let...else`→`?`, `if let Ok`)로 이번 devtools 스코프 밖이며 전부 스타일 성격이다. `dev_tools.rs`는 clippy 클린 | 지금 고칠 가치 없음. 해당 코드 손댈 때 함께 정리 |
| 17 | ⚪ | 전체 | `src/views/devTools.ts:42-45` | 코드 정독 | 1초 경과 타이머가 매초 `notifyChange()`로 전체 뷰를 재렌더한다(최대 600초 = 600회) | 경과 시간만 DOM 직접 갱신 |

---

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 실행보안 | `brew upgrade -y`는 존재하지 않는 플래그라 실제 실행이 항상 exit≠0으로 실패한다 | **기각** | `brew upgrade --help` 실측 결과 `-y, --no-ask, --yes`가 실재하고 "Ask mode is the default"까지 명시돼 있다. 코드 주석(`dev_tools.rs:464`)의 주장이 정확했다 |
| 정직성 | `parse_brew_dry_run_affected`가 실제 brew 출력 형식을 못 읽는다(`==>` 헤더·컬럼 정렬 때문) | **기각** | `brew upgrade --dry-run --formula gnupg` 실측 출력(3개 섹션 8행)을 파서 로직에 대입해 8개 전부 정확히 추출됨을 확인. `==>` 헤더는 `->`를 포함하지 않아 자연히 건너뛴다 |
| 실행보안 | 경로 컴포넌트를 통한 argv 인젝션이 가능하다 | **기각** | `resolve_args`(`dev_tools.rs:722-747`)가 `Arg::Lit`이 아닌 모든 슬롯에 대해 `validate_argv_token`을 **강제**하고 실패 시 `Err`로 중단한다. 우회 분기 없음. `resolve_args_rejects_malicious_formula_before_reaching_argv`(2031행대) 테스트도 존재 |
| 실행보안 | `open_terminal_command`가 osascript로 셸 문자열을 실행하므로 셸 미경유 원칙 위반 | **강등(→ #5 🟡)** | 넘어가는 값이 전부 `&'static str` 상수(`softwareupdate --list` 등)이고 사용자 입력 유입 경로가 0이라 인젝션 위험은 없다. 남는 문제는 상수 자체가 실행 불가능한 플레이스홀더라는 UX 결함뿐이라 그쪽으로만 남겼다 |
| 전체 | 워킹트리의 Wrangler 실설치(`pnpm add -g wrangler`) 추가가 "미설치 도구는 항상 Manual" 원칙을 뒤집는다 | **스코프 밖(보류)** | 이번 위임 대상은 `git diff 8be0a18..HEAD`이고 해당 변경은 미커밋 워킹트리 상태다. 다만 원칙을 뒤집는 변경이므로 **별도 리뷰 필요**로 PM 권고에 올린다 |

---

## 페르소나별 관점

### [프로세스 실행 보안 감사자] — 판정: 🟡 Amber
설계가 약속한 "임의 문자열이 argv로 유입되는 경로 = 0"은 **실제로 성립한다**. 프론트에서 오는 값은 `toolId`/`planId`뿐이고 `ToolId::from_key`(`dev_tools.rs:105`)가 6개 키와 정확 일치 실패 시 즉시 `Err`, 이후 코드는 enum만 들고 다닌다. 동적 슬롯은 `Formula`/`Package`뿐이며 둘 다 canonical 경로 컴포넌트에서만 나오고 `validate_argv_token`을 통과해야 argv에 들어간다(722-747). 셸 경유는 `osascript` 한 곳뿐이고 인자가 정적 상수라 안전하다. 프로세스 수명 4종(stdin null / 리더 스레드 / try_wait 폴링 / `kill(-pgid)` SIGTERM→3초→SIGKILL)이 전부 있고, 행잉 프로세스 kill·stdin null 즉시 실패가 **테스트로 검증**돼 있다(2109·2127행대). 유일한 실질 구멍은 자식 PATH의 빈 엔트리(#1)다. `capabilities/default.json` 변경 없음도 확인.

### [정직성 감사자 (claimed ≠ verified)] — 판정: 🟡 Amber
성공 판정은 exit code가 아니라 **실행 후 재조회 델타**로 이뤄지고(1528-1536), `UnknownAfter`는 `verified:false`이며 UI에서 amber 패널 + "확인되지 않음"으로 그려진다(`devTools.ts:367·372·380`). 성공 토스트는 `outcome==='updated'`일 때만 뜬다(`devTools.ts:69-70`). 경로 캐시 없이 실행 후 처음부터 재해석하는 것(1515-1519)도 설계대로다. **프론트-백엔드 계약은 필드·enum variant 전수 일치**했다 — `installMethod/actionKind/manualHint/planId/willRun/commandDisplay/normalizedBefore/normalizedAfter/ranCommand/exitCode/durationMs/logTail/pathVisible/pathHint/pathHintTarget` 및 `updated|alreadyLatest|unknownAfter|failed|timedOut|notSupported` 6개 모두. 누락·오타 없음. 문제는 **코드가 자기 주석을 배반하는 지점**(#4)과 진단 정보 유실(#6), 경계 케이스의 과장된 단언(#10, #11)이다.

### [이기종 머신 운영 현실주의자] — 판정: 🟠
`node@22` 문제는 설계의 주장대로 해결됐다 — 하드코딩 맵이 아니라 Cellar 경로 파싱이라 `brew upgrade --formula node@22`가 나오고, 실측 고정 테스트(1673행대)까지 있다. 이게 이 기능의 진짜 가치다. 반면 우리 조직 관점에서 걸리는 건 셋이다: **Intel 맥에서 기능이 통째로 안 뜰 가능성**(#2), **막다른 골목 루프**(#3), **오경고**(#9, #5). 특히 #3은 "다시 미리보기를 요청해주세요"만 반복되는 형태라 사용자가 IT에 문의하게 된다. 한편 `Manual`을 1급 개념으로 둔 것과 UI가 이를 "실행 불가/안내 보기"로 그리는 것(`devTools.ts:285-291`)은 무한 재시도를 실제로 막아준다 — 설계 결정 3의 의도가 화면까지 살아 있다.

### [제로베이스 재설계자] — 판정: 🔵 (아래 별도 섹션)

---

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R1 | "도구 6개" 카드 리스트 + 도구별 업데이트 버튼. 실제로 앱이 실행할 수 있는 건 brew formula와 claude native뿐이고 git은 **구조적으로 영구 불가**, node는 버전매니저면 불가 | 1급 개념을 "도구"가 아니라 **"이 머신 개발환경 진단 리포트"**로 재정의. 상단에 "문제 2개 / 그중 앱이 고칠 수 있는 것 1개" 요약, 그 아래 조치 가능 항목만 액션 노출, 나머지는 접힌 목록 | 사용자 목표는 "6개 도구를 관리"가 아니라 "내 환경이 정상인지 확인하고 이상하면 고친다"다. 현재 구조는 매번 6개를 훑고 각각의 불가 사유를 학습하게 만든다 | 중간. 백엔드 계약(`DevToolStatus`)은 그대로 두고 렌더만 재구성 가능 — 근거: `check_dev_tools`가 이미 `actionKind`로 3분류를 주고 있어 집계에 필요한 데이터는 다 있다 |
| R2 | 앱이 자식 프로세스로 직접 실행. 그 때문에 파이프 교착·타임아웃·프로세스 그룹 kill·전역 뮤텍스·plan_id 해시 게이트 등 **실행 엔진에만 1,000줄 가까이** 든다 | 이 설계의 고유 가치(§0)인 "**경로 파싱으로 정확한 명령을 만들어낸다**"만 남기고 실행은 전부 터미널 위임(`open_terminal_command`)으로 이관. 앱은 "당신 머신에 맞는 정확한 명령은 이것입니다" + 터미널 열기 | 정확한 명령 생성이 이 기능의 차별점이고, 실행 대행은 그 부산물이다. 위임하면 실행 엔진 전체가 사라지고 사고 시 blast radius가 0이 되며, TTY가 있어 sudo·확인 프롬프트 문제도 함께 소멸한다. 이 앱엔 이미 `gh auth login` 터미널 위임 선례가 있어 철학도 일관된다 | 포기하는 것: "한 방에 끝나는" 경험. 감당: 명령이 정확하므로 붙여넣기 1회로 끝난다. v2에서 "앱이 대신 실행"을 opt-in 토글로 내리는 단계적 전환 권고 |
| R3 | `plan_id = sha256(runner_path ‖ argv ‖ normalized_before)` — **명령**을 승인 대상으로 붙잡는다 | `plan_id`에 **dry-run이 산출한 영향 목록**을 포함하고, 실행 직전 dry-run을 1회 더 돌려 목록 해시를 대조. 불일치면 재프리뷰 요구 | 사용자가 실제로 승인한 것은 "명령 문자열"이 아니라 "무엇이 바뀌는가"다. 현재 게이트는 auto-update로 대상 집합이 바뀌어도 통과한다(#14) | 실행마다 dry-run 1회 추가(수~수십초). brew 계획에만 적용하면 비용이 제한된다. 리스크: 총 소요시간 증가로 타임아웃 예산 재조정 필요 |

---

## 트레이드오프 (페르소나 간 충돌)

- **#4(전체 업데이트 무확인 실행)**: 정직성 감사자는 "확인 없이 비가역 명령을 실행하고 주석은 반대로 말한다 → 즉시 수정"을, 이기종 운영 현실주의자는 "의존성 연쇄라는 진짜 위험 케이스는 `>1` 가드가 이미 막고 있고, 사용자가 '전체 업데이트'를 누른 것 자체가 의도 표명이다 → 확인 단계를 더 넣으면 버튼 존재 이유가 사라진다"를 주장.
  → **권고**: 확인 단계를 도구별로 추가하지 말고 **"전체 업데이트" 클릭 시 실행될 명령 목록을 한 번에 보여주는 1회 확인**으로 절충. 그리고 어느 쪽을 택하든 **주석은 반드시 실제 동작에 맞춰 정정**한다(주석이 거짓인 상태가 가장 나쁘다).
- **R2(실행 대행 폐기) vs 제품 방향**: 제로베이스 재설계자는 실행 엔진 제거를 제안하지만, 사용자 메모리의 "MVP 속도 우선"·"이미 만들어 public에 나갔다"를 감안하면 지금 되돌리는 비용이 이득을 넘는다.
  → **권고**: 지금은 유지. 실행 엔진 유지보수 부담이 실제로 발생하는 시점(2번째 OS/구성에서 버그가 날 때) R2를 재검토 트리거로 삼는다.

---

## 잘 된 점 (유지할 패턴)

1. **설계 → 구현 추적성이 실제로 살아 있다.** 코드 주석이 "결정 N", "부록 B.1" 같은 근거를 달고 있어 리뷰어가 설계와 1:1 대조할 수 있었다. 이번 리뷰의 Major 2건이 발견된 것도 이 추적성 덕분이다.
2. **테스트가 실측값으로 고정돼 있다.** 26개 테스트가 형식적 통과가 아니라 실제 관측값(`/opt/homebrew/Cellar/node@22/22.23.1/bin/node`, `gh version 2.95.0 (2026-06-17)` 2줄 출력, `git version 2.50.1 (Apple Git-155)`, brew dry-run 실제 출력)으로 짜였고, **행잉 프로세스 강제 종료**·**stdin null 즉시 실패**·**plan_id 입력 민감도**처럼 짜기 귀찮은 것까지 덮었다. 이 수준을 다음 산출물의 기준으로 삼을 만하다.
3. **순수 함수 분리.** `classify_install_method`가 env/fs에 닿지 않고 `home/pnpm_home/brew_prefixes`를 인자로 받게 설계돼 합성 경로로 4종 머신 구성을 전부 테스트할 수 있다.
4. **"실행 불가"의 1급 개념화가 화면까지 관통했다.** `Action::Manual` → `actionKind:"manual"` → "안내 보기" 버튼(재시도 버튼 아님)으로 이어져, 설계가 우려한 무한 재시도를 구조적으로 막았다.
5. **목업 잔재 제거 완료.** `MOCK_DEV_TOOL_META`, `docker` 행, `state.devTools.mockUpdatedVersion`이 모두 사라졌고 "N개 업데이트 가능" 배지도 없다 — 근거 없는 `latestVersion` 주장을 하지 않는다.
6. **`dev_tools.rs`는 clippy 클린**(경고 5건 전부 스코프 밖 lib.rs).

---

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| S1 화이트리스트 후 enum 전환, 원본 문자열 폐기 | 실행보안 | 필수 | ✅ | `from_key`(105) 실패 시 즉시 Err |
| S2 동적 슬롯 검증 강제 | 실행보안 | 필수 | ✅ | `resolve_args`(722-747), 우회 분기 0 |
| S3 셸 미경유 | 실행보안 | 필수 | ✅ | osascript는 정적 상수만 |
| S4 자식 PATH 위생 | 실행보안 | 필수 | ❌ | 지적 #1 (빈 엔트리 = CWD) |
| S5 프로세스 수명 4종 | 실행보안 | 필수 | ✅ | 테스트로 검증됨 |
| S6 쓰기권한 사전검사 | 실행보안 | 권장 | ⚠ | brew만, 대상도 Cellar 아님(#2·#7) |
| V1 Updated에 재조회 개입 | 정직성 | 필수 | ✅ | 단 before 빈값 경계(#10) |
| V2 미확인이 성공으로 안 뭉개짐 | 정직성 | 필수 | ✅ | UI 톤·토스트 모두 확인 |
| V3 계약 전수 일치 | 정직성 | 필수 | ✅ | 필드·enum 6종 전부 |
| V4 주석·문서와 코드 일치 | 정직성 | 필수 | ❌ | 지적 #4 |
| V5 실패 진단 도달 | 정직성 | 권장 | ⚠ | 지적 #6 |
| V6 숫자·목록 의미 정확 | 정직성 | 권장 | ⚠ | 지적 #8 |
| O1 4종 구성 판별 추적 | 이기종 | 필수 | ⚠ | Intel 경로에서 #2 |
| O2 안내가 실행 가능 | 이기종 | 필수 | ⚠ | 지적 #5 |
| O3 실패 루프 없음 | 이기종 | 필수 | ❌ | 지적 #3 |
| O4 오탐 없음 | 이기종 | 권장 | ❌ | 지적 #9 |
| O5 로드 비용 예측 가능 | 이기종 | 권장 | ⚠ | 지적 #12 |
| R1~R5 대안 제시 | 발산 | — | ✅ | 3건 모두 대안·포기 명시 |

---

## 생략한 관점·확인 (정직 보고)

- **화면 캡처 없음 — UI 리뷰는 코드 기반이다.** 사유: 대상이 Tauri 네이티브 데스크톱 앱이라 `bin/capture.mjs`(Playwright/웹) 대상이 아니고, 앱을 띄우려면 cargo 빌드가 필요한데 **다른 세션이 같은 파일을 동시 편집 중**이라 빌드 충돌·워킹트리 오염 위험이 있었다. `docs/screenshots/`에 이미지 없음.
- **Intel 맥 실기 검증 못 함**(#2). 이 머신은 arm이고 `/usr/local`에 Homebrew가 없다. 설계 문언과의 불일치는 확정, Intel에서의 증상은 **미검증 추정**이다.
- **`claude update` / `npm i -g` / `pnpm self-update` 실제 실행 안 함**(금지 범위 준수). 읽기 전용(`--help`, `--dry-run`, `--version`, `ls -ld`)만 사용했다.
- **`cargo test`·`tsc` 재실행 안 함**(PM 측정치 재사용). `cargo clippy --all-targets`만 직접 돌렸다(워킹트리 기준 5건, PM 보고 6건과 차이는 워킹트리 변경 때문으로 보인다).
- **범위 밖 파일 미검토**: `src/brand.ts`, `src/assets/logo-mark.png`, `src/views/login.ts`, `src/sidebar.ts`, `src/styles.css`(단 `devtool-panel-*` 클래스 4줄만 지적 #15 근거로 grep), `github_integration.rs`, `cloudflare_integration.rs`, `jira_integration.rs`.

## 프로세스 관찰 (PM 확인 요망)

리뷰 도중 대상 파일이 **계속 바뀌었다**. 착수 시점 `dev_tools.rs` 2,255줄 → 몇 분 뒤 2,286줄. `git status` 확인 결과 `src-tauri/src/dev_tools.rs`, `src/views/devTools.ts`, `src/views/catalog.ts`에 미커밋 변경이 진행 중이고, 내용은 **Wrangler 미설치 상태에서 `pnpm add -g wrangler` / `npm install -g wrangler`를 실제로 실행하는 신규 경로**다. 나는 HEAD `261590d`로 스냅숏을 고정해 리뷰했다.
이 변경은 설계의 **"미설치 도구는 formula/패키지명을 추측할 근거가 없으므로 항상 Manual"**(`dev_tools.rs:592-598` 주석) 원칙을 정면으로 뒤집는다. 원칙을 바꾸는 결정이므로 **별도 Sensitive 리뷰가 필요**하다.

**보고서 작성 중 추가 관찰**: 리뷰를 마무리하는 사이 이 변경이 `5251f97 feat(dev-tools): Wrangler CLI 전역 설치 실행 지원`으로 커밋되어 HEAD가 `261590d` → `d9c69ac` → `5251f97`로 이동했다. 즉 **본 보고서가 검토한 리비전은 더 이상 HEAD가 아니다**(리뷰 기준은 `261590d` 그대로). 위 지적 #1~#17이 `5251f97`에서도 그대로 남아 있는지는 확인하지 않았다 — 수정 위임 시 최신 리비전에서 라인번호를 재확인할 것.

---

## PM에게 권고

1. **재작업 없음(되돌리기 불필요).** Critical 0건이고 설계의 핵심 안전장치는 실재한다. 이미 public에 나간 상태를 롤백할 근거는 없다.
2. **다음 릴리스 전 수정 (우선순위 순)** — 전부 국소 수정이다:
   - #1 자식 PATH 빈 엔트리 필터 (**1줄**, 즉시)
   - #3 `installed:false + actionKind:"run"` 막다른 골목 (UI 분기 또는 `perform_install` 위임)
   - #2 brew 쓰기권한 검사 대상을 `<prefix>/Cellar`로 (**1줄**, Intel 맥 대응)
   - #4 "전체 업데이트" 1회 확인 추가 **또는** 주석 정정 (둘 중 하나는 필수 — 주석이 거짓인 상태를 남기지 말 것)
3. **백로그**: #5~#14. 이 중 #5(플레이스홀더 명령)·#6(spawn_error 유실)·#8(off-by-one)은 각각 수 줄 수정으로 보이나 **미확인 추정치**다(관련 유틸 존재 여부까지 훑지는 않았다).
4. **지금 고치지 말 것**: clippy 경고 5건(#16) — 전부 이번 스코프 밖 lib.rs 스타일 경고다. 해당 코드를 손댈 때 함께 정리.
5. **별도 위임 필요**: 워킹트리에서 진행 중인 **Wrangler 실설치 경로**는 설계 원칙을 뒤집는 변경이므로 커밋 전 Sensitive 리뷰를 걸 것. 겸사겸사 같은 커밋에 무관한 변경이 섞이는 문제(이번 사후 리뷰의 발단)를 막기 위해, 다음 커밋부터는 `git add -A` 대신 경로 지정 스테이징을 권고한다(Skill `domain-git-safety-and-concurrency`).
6. **본 리뷰의 실행 액션**: 없음. 코드를 수정하지 않았고, 커밋·push·배포도 하지 않았다. 생성한 파일은 `docs/reviewer/` 아래 페르소나 4개 + INDEX.md + 이 보고서 사본뿐이며 `docs/`는 `.gitignore` 대상이라 커밋에 잡히지 않는다.
