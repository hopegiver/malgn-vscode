# 프로세스 drain 상한·Windows 트리 kill 재검토 보고서 (2차)

리뷰 대상: target_id `r3-process-drain-and-tree-kill` / 커밋 범위 `ed5f435..c0f325a` (6ad4e94 본 수정 + c0f325a 1차 리뷰 반영. 파일: `src-tauri/src/dev_tools/process.rs`, `src-tauri/src/process_util.rs`, `src-tauri/src/dev_tools/actions.rs`(주석), `src-tauri/src/plugins/mod.rs`(주석))
직전 리뷰: `docs/reviewer/review-r3-process-drain-and-tree-kill-2026-10-13.md`(1차, Amber, 작업 트리에 미커밋 상태이며 이번 라운드에서 손대지 않았다)
리스크 범주: 사용자 PC 자식 프로세스 실행(프로세스 수명·동시성·Windows 트리 kill). 직전과 같다.
등급·모드: Sensitive, 2차 재검토, **풀패널**. 위임 지시대로 증분·축소 모드를 쓰지 않고 커밋 범위 전체(6ad4e94 포함)를 다시 봤다. 재검토 3요소(target_id·직전 리뷰 경로·리스크 범주)가 모두 명시돼 있었다. 그래도 축소·증분을 적용하지 않았으므로 동일 대상 인정 조건 대조는 생략했다(풀패널이 안전측 최대치다).
리뷰 페르소나 패널(4명, 모두 `docs/reviewer/personas/`에 있음): `persona-process-execution-security.md`, `persona-tauri-runtime-thread-and-lock-discipline.md`, `persona-claimed-vs-verified.md`, `persona-zero-base-redesigner.md`(발산형)
리뷰 일자: 2026-10-13
종합 판정(커밋 범위 `ed5f435..c0f325a`, macOS 실행 검증 + Windows 정적 읽기): 🟢 Green

## 요약 (2분 규칙)
직전 Major #1(테스트 간헐 실패)은 해소됐다. 작성자가 원인으로 든 메커니즘은 이번 라운드에서 독립적으로 재현했다. macOS std는 파이프를 `pipe()`로 만든 뒤 CLOEXEC를 따로 설정한다(rust-src `sys/pipe/unix.rs:34-39`). 그래서 그 사이에 다른 스레드가 fork하면 그 자식이 파이프를 물려받는다. std만 쓰는 스크래치 프로그램으로 cat 1500회와 `sh -c 'sleep 1.2 &'` 동시 spawn을 돌렸다. 락이 없으면 EOF가 1초를 넘긴 경우가 2·1·2회, spawn 구간에 뮤텍스를 두면 0·0·0회였다. 이 경합은 macOS에만 있다. Linux는 `pipe2(O_CLOEXEC)`를 쓰고(`:28`), Windows std는 파이프 생성부터 CreateProcess까지를 프로세스 전역 `CREATE_PROCESS_LOCK`으로 묶는다(`sys/process/windows.rs:343-345`). 수정 이후 실행에서는 debug 모듈 100회, debug 전체 80회(그중 30회는 `--test-threads=32`), release 30회 모두 실패가 0회였다.

Critical·Major는 없다. Minor는 3건이다. 그중 하나는 새로 찾은 역방향 경로다(아래 N2). 락은 "락 안 spawn끼리"만 직렬화한다. 그래서 claude_auth 테스트처럼 락 밖에서 파이프를 만드는 쪽이 장수 자손 테스트에 파이프를 물릴 수 있고, 그 테스트의 2.5초 단언이 깨질 수 있다. 확률은 낮다고 판단한다. 전체 80회 실행에서 실패는 0회였고, 경합 창은 µs 단위인 데다 해당 spawn은 실행당 몇 번뿐이다. 그래서 CI 게이트를 위협하는 수준은 아니다. Windows 측(`wait_or_kill`, 트리 kill 테스트 수정)은 정적으로 읽었을 때 결함이 없었다. 실행 확정은 windows-latest CI 몫으로 남는다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 없다. 직전 라운드와 같은 4명을 재사용했고, 본문 6대 요소는 고치지 않았다. 각 파일의 "적용 이력"에 이번 라운드 항목을 붙였고, INDEX.md "최근 재사용"에도 한 줄씩 덧붙였다(미커밋).

| 페르소나 | 판정 | 사유 |
|---|---|---|
| persona-process-execution-security.md | 재사용 | 새 코드 `wait_or_kill`과 taskkill spawn 경로가 S1(셸 없음·절대경로)과 S5(프로세스 수명)의 직접 대상이다 |
| persona-tauri-runtime-thread-and-lock-discipline.md | 재사용 | Major #1의 해법이 "락으로 직렬화"다. 그 락이 무엇을 직렬화하고 무엇을 놓치는지가 T 관점의 과녁이다. taskkill 상한이 `PLUGIN_OP_LOCK` 가드 수명에 주는 영향도 같다 |
| persona-claimed-vs-verified.md | 재사용 | 작성자는 "원인 확정"이라고 주장하면서 "반복 결과는 판별력이 없다"고 스스로 밝혔다. 이 주장을 관측으로 바꾸는 것이 이번 라운드의 핵심이다 |
| persona-zero-base-redesigner.md | 재사용(발산형, 필수) | 테스트 전용 락이라는 국소 해법 대신 "테스트 spawn 규율" 자체를 다시 그리는 역할이다 |
| persona-heterogeneous-machine-operator.md | 생략 | 이번 변경의 Windows 쟁점은 taskkill 상한과 테스트 전제다. 기계 다양성 문제가 아니라서 process-execution-security가 맡았다(직전 라운드와 같은 판단) |

## 직전 지적 상태 추적

| 직전 | 심각도 | 이번 상태 | 근거(확인방법) |
|---|---|---|---|
| #1 테스트 간헐 실패 | 🟠 Major | **해소** | `process.rs:207-217`(`#[cfg(test)] TEST_SPAWN_LOCK`), `:392-397`(spawn 구간만 가드). 메커니즘은 스크래치 재현으로 독립 확인했다(락 없음 5/4500, 락 있음 0/4500). 수정 후 debug 모듈 100회, debug 전체 80회, release 30회에서 실패 0회. 작성자 말대로 반복 통과만으로는 수정 전과 구별되지 않는다. 직전 75회 2실패율(약 2.7%)이 유지된다고 가정하면 100회 0실패가 나올 확률은 약 7%라 보조 근거에 그친다. 정근거는 재현 실험이다. 남은 역방향 경로는 N2 |
| #2 상한 초과 무신호 | 🟡 | **해소** | `process.rs:437-458` `eprintln!`. 인자 대신 프로그램 파일명만 남긴다. `--nocapture` 전체 실행 80회에서 로그가 실행마다 정확히 3줄씩 찍혔고, 모두 의도된 자손 테스트 3개(`program=sh`, 상한 2000ms 정상종료/타임아웃, 600ms 정상종료)에서 나왔다. 배포물에서는 이 로그가 보이지 않는다(N3) |
| #3 taskkill 무상한 | 🟡 | **해소(공유 헬퍼 한정)** | `process_util.rs:112-126` spawn + `wait_or_kill`(`:139`) + `TASKKILL_TIMEOUT` 5초(`:133`). 단위 테스트 `:229`가 통과했다. 직전 보고서의 "이 함수 하나만 고치면 4개 호출부가 모두 덮인다"는 **리뷰어 쪽 오류**였다. autonomy의 Windows `force_kill_all_remaining`(`autonomy/runtime.rs:258-280`)은 공유 헬퍼가 아닌 별도 사본이고, 상한 없는 `.output()`이 그대로 남아 있다(N1) |
| #4 SIGPIPE 동작 미기재 | 🟡 | **해소** | `process.rs:197-204`에 현재형으로 적혔다 |
| #5 낡은 주석 3곳 | 🟡 | **해소** | `actions.rs:28-32`, `plugins/mod.rs:28-32`, `:267-271`. 전체 grep(`직속 자식만\|고아 없음\|TerminateProcess`)에 남은 4건은 설명 문맥이라 사실과 맞는다(`process.rs:131`, `:162`, `:972`, `plugins/mod.rs:748`. 마지막은 그룹 안 손자를 다루는 테스트라 정확하다) |
| #6 형제 사본 무상한 join | 🟡 | **범위 밖(PM 결정, 백로그)** | 지적으로 삼지 않는다. 커밋 메시지가 "범위 밖(미처리)"로 명시해 직전 권고 4항("process.rs 한정"으로 기록)을 충족했다 |
| #7 테스트 전제 검증 | 🟡 | **해소** | ① `:931-968`: 하한 550ms(`:955`)와 `mid` 보존 단언을 추가했고 이름도 맞췄다. ② `:980-1018`: 타임아웃 3초 + `ready` 단언(`:1004`). Windows 실행은 미검증 |
| #8 테스트 여유 | ⚪ | **해소** | 상한 1500ms |
| #9 pid 파일 경로 미인용 | ⚪ | **미처리** | `:888` 그대로다. 커밋도 처리를 주장하지 않았다. 실패해도 크게 드러나므로 유지한다 |
| #10 clippy 신규 경고 | ⚪ | **해소** | `:348` 근거를 단 `#[allow]`. 남은 `:320`(인자 8개)은 직전에 "기존"으로 분류한 경고다 |
| R1 임시 파일 캡처 | 🔵 | **범위 밖(PM 결정, 백로그)** | 지적으로 삼지 않는다 |

**#6·R1 백로그 결정의 적절성(한 줄)**: 적절하다. #6은 이번 커밋이 만든 회귀가 아니라 형제 사본에 원래 있던 결함이고, R1은 직전 라운드도 "지금 바꿀 필요는 없다"고 한 재설계다. 다만 N1(autonomy taskkill 사본)을 #6과 같은 백로그 항목에 묶어야 "사본 정리"가 한 번에 끝난다.

## 지적 사항

| # | 심각도 | 영역 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| N1 | 🟡 Minor | Windows 수명(기존 결함, 직전 리뷰 정정) | `src-tauri/src/autonomy/runtime.rs:258-280` | Read와 grep(`kill_process_group_with_grace` 호출부. autonomy는 호출하지 않음) | autonomy의 Windows 종료 보루가 taskkill을 직접 `.output()`으로 실행해서 상한이 없다. 이번 #3 수정의 혜택을 받지 못한다. 경로는 앱 종료(`lib.rs`의 2초 유예 뒤)이고, `RUNTIME` 락은 pid를 모은 뒤 놓으므로(`:261-267`) 락 결합은 없다. 최악의 경우 taskkill이 멈추면 앱 종료가 함께 멈춘다. 이번 커밋의 회귀는 아니다 | `for pid in pids { crate::process_util::kill_process_group_with_grace(pid); }`로 바꾸면 S1 절대경로와 5초 상한을 함께 물려받는다. #6 백로그와 함께 처리한다. 비용 라벨은 낮음이다(근거: Windows 분기의 공유 헬퍼가 이미 같은 `taskkill /T /F` 의미이고, 호출부 시그니처가 `u32` pid로 같다) |
| N2 | 🟡 Minor | 테스트 결정성(잔여 역방향 경로) | 락: `src-tauri/src/dev_tools/process.rs:392-397`. 락 밖 파이프 생성: `src-tauri/src/claude_auth.rs:1062-1066`(`spawn_login_child`, stdin piped). 깨질 단언: `claude_auth.rs:1198`(2.5초). 장수 자손: `process.rs:849`(`sleep 60 &`), `:888`(setsid `sleep 60`), `:935`(`sleep 5`) | 코드 추적. 메커니즘은 #1의 재현으로 확인한 것과 같고 방향만 반대다. 이 방향의 실측 재현은 하지 않았다 | `TEST_SPAWN_LOCK`은 `run_process_inner`를 거치는 spawn끼리만 직렬화한다. 락 밖 spawn(claude_auth 테스트 헬퍼, `process_util`·`session_list` 테스트 등)이 파이프를 만드는 µs 창에 락 안의 장수 자손 테스트가 fork하면, 그 `sleep 60`/`sleep 5`가 claude_auth stdin 파이프의 **쓰기 끝**을 문다. 그러면 `cancel_closes_stdin_even_if_child_ignores_sigterm`(TERM 무시 `cat`)은 취소 뒤에도 EOF를 받지 못하고, 2.5초 대기(`:1198`)에서 실패한다. setsid 자손(약 3초 이상)과 `sleep 5`(N4)는 이 2.5초보다 오래 산다. 반대 방향(락 밖 자식이 dev_tools 파이프를 무는 경우)은 무해하다. 락 밖 자식은 수명이 수백 ms 이하이고, 빡빡한 단언은 cat의 2초뿐이다. 판단: 해당 spawn은 실행당 수 회이고 창은 µs 단위라 확률이 낮다. 전체 80회(32스레드 30회 포함)에서 실패 0회였다. **CI 게이트를 위협하는 수준은 아니다** | 락을 `process_util`로 옮겨 `#[cfg(test)] pub(crate) fn test_spawn_guard()`로 노출한다. 테스트에서 자식을 띄우는 헬퍼(`spawn_login_child`, `claude_auth.rs:1008`, `process_util`·`session_list` 테스트)도 같은 가드를 쥐게 한다. 비용 라벨은 낮음이다(근거: 락 밖 직접 spawn 테스트 지점은 grep 기준 claude_auth 2곳, process_util 3곳, session_list 1곳, platform_tests 3곳이다. platform_tests는 `.output()` 단명 프로세스라 우선순위가 낮다). 이 지점이 실제로 깨지면 먼저 `claude_auth.rs:1198`인지 확인한다 |
| N3 | 🟡 Minor | 주장과 실제(관측 가능성) | `process.rs:438-440` 주석 "이 한 줄이 현장 진단의 유일한 신호다", `src-tauri/src/main.rs:2`(`windows_subsystem = "windows"`) | Read. 로깅 기반 grep(`tauri-plugin-log`·`log`·`tracing` 없음, `eprintln!` 11곳이 관례) | release Windows 빌드에는 콘솔이 없고, macOS도 Finder로 실행하면 stderr가 사라진다. 그래서 사내 PC(현장)에서는 이 로그를 볼 수 없다. 개발 실행이나 터미널 실행에서만 보인다. 이 로그는 직전 #2 권고를 그대로 구현한 것이므로 권고 자체가 이 한계를 놓친 셈이다. 기능 결함은 아니다 | 주석을 "개발 실행·터미널 실행에서만 보인다"로 좁힌다. 현장 진단이 필요해지면 앱 로그 파일 도입을 백로그로 둔다(이 앱 전체의 `eprintln!` 11곳에 공통인 문제다) |
| N4 | ⚪ Nit | 테스트 위생 | `process.rs:935`(`(sleep 0.1; echo mid; sleep 5; echo late) &`), `KillOnDrop`(`:815-824`) | 32스레드 실행 직후 `pgrep`에 `sleep 5` 1건이 남아 있었고, 몇 초 뒤 사라졌다 | `$!`는 서브셸 pid라서 `KillOnDrop`이 서브셸만 죽인다. 손자 `sleep 5`는 최대 5초 고아로 남는다. 직전 버전(`sleep 1`)보다 오래 남고, 이 수명이 N2의 노출 시간을 늘린다 | `kill(-pgid)`처럼 그룹째 죽인다(sh는 `process_group(0)`으로 떠서 pgid가 sh pid다. `echo $$`로 기록한다). 또는 `sleep 5`를 `exec`하지 않는 짧은 값으로 바꾼다 |
| R2 | 🔵 Rethink | 구조(테스트 spawn 규율) | `process.rs:207-217`, N2의 지점들 | 설계 대안 검토 | 이번 해법은 "어느 테스트가 어느 테스트의 파이프를 무는가"를 사례별로 막는 국소 락이다. 같은 바이너리 안에서 자식을 띄우는 테스트가 늘 때마다 같은 질문이 반복된다(N2가 첫 사례다). 근본 원인은 테스트 프로세스 하나 안에서 macOS의 비원자 CLOEXEC와 장수 자손이 공존한다는 점이다 | **대안 구조: 장수 자손 테스트를 별도 프로세스로 격리한다.** `sleep 60`·setsid 자손을 남기는 3개 테스트를 `src-tauri/tests/`의 통합 테스트 바이너리로 옮기거나 `#[ignore]` + 단독 CI 스텝(`cargo test -- --ignored --test-threads=1 dev_tools::process::tests::descendant`)으로 분리한다. 그러면 lib 테스트 바이너리에는 장수 자손이 하나도 남지 않아 어떤 락도 필요 없다. 비용은 `run_process_inner`가 `pub(crate)` 밖으로 나가야 한다는 점(통합 테스트 방식)이나 CI 스텝 1개(ignore 방식)다. 지금의 락은 비용 대비 합리적이다. 다음에 자식을 띄우는 테스트를 추가할 때 이 분리를 검토한다. R1(임시 파일 캡처)이 도입되면 프로덕션 쪽 같은 문제도 사라진다 |

## 페르소나별 관점

### 프로세스 실행 보안 감사자 (수렴형)
- S1·S3: 유지된다. `process_util.rs:112-126`은 여전히 `windows_system_tool(&roots, r"System32\taskkill.exe")` 절대경로를 쓰고, argv는 정적 상수와 정수 pid뿐이며 셸 경유가 없다. stdio는 `.output()`과 같은 의미로 null이다(`.output()`도 stdin을 닫는다). 파이프를 만들지 않으므로 읽기 쪽이 필요 없다.
- `wait_or_kill`(`:139-154`): 20ms 폴링이고, 상한을 넘거나 `try_wait`가 실패하면 `kill`과 `wait`로 넘어간다. kill 대상은 우리가 띄운 taskkill.exe라서 TerminateProcess가 실패할 이유가 사실상 없다. 그래서 뒤따르는 무한 `wait`는 기각했다(아래). `cfg(any(windows, test))` 게이팅에도 이상이 없다. Windows 비테스트 빌드에서는 사용되고, Unix 비테스트 빌드에서는 컴파일되지 않아 dead-code 경고가 없다.
- 호출부 보존: Windows에서 이 헬퍼를 쓰는 곳은 claude_auth 4곳(`:294`, `:640`, `:682`, `:781`), session_chat 3곳(`turn.rs:149`, `:363`, `:480`), dev_tools 1곳(`process.rs:181`)이다. 모두 락 가드를 놓은 뒤에 호출하고, 반환값을 쓰지 않는다. 정상 경로의 의미("끝날 때까지 기다린 뒤 결과 무시")는 그대로이고, 비정상 경로에만 5초 상한이 생겼다.
- 판정: 필수 기준 미충족은 없다. N1(형제 사본)은 기존 결함이다.

### Tauri 런타임 스레드·락 규율 감사자 (수렴형)
- 가드 수명(T2): Windows의 `PLUGIN_OP_LOCK` 최대 보유 시간이 처음으로 유한해졌다. 타임아웃 + taskkill 5초 + `Child::kill`/`wait` + drain 2초다. 직전 라운드의 "Windows는 #3 조건부"가 풀렸다.
- `TEST_SPAWN_LOCK`: 가드 범위는 `command.spawn()` 한 줄이다(`:392-397`). 그래서 포이즌·교착 위험이 없다. 비교적 느린 perl·sh spawn이어도 µs~ms 단위라 테스트 시간이 늘지 않았다. 모듈 3.2초, 전체 3.8초로 직전과 같다. plugins 테스트도 `run_process_inner`를 거치므로 같은 락 안에 들어온다.
- 놓친 직렬화: N2. 락이 덮는 범위는 "이 함수를 거치는 spawn"이지 "이 테스트 바이너리의 spawn"이 아니다. 주석(`:207-215`)은 메커니즘을 정확히 적었지만, 그 메커니즘이 락 밖 spawn과 락 안 spawn 사이에서도 양방향으로 성립한다는 점은 적지 않았다.
- 프로덕션 판단("전역 락을 도입하지 않는다"): 동의한다. 앱 안의 병렬 spawn(autonomy 워커, plugins, dev_tools 조회)이 서로 파이프를 물어도 이 헬퍼 경로에서는 지연이 DRAIN_GRACE 2초로 묶인다. 프로덕션 전역 spawn 락은 autonomy의 동시성 설계(`process.rs:317-319` 주석의 EXECUTION_LOCK 비공유 근거)와도 충돌한다. 단, 형제 사본(#6)의 무상한 join 경로에서는 같은 누수가 상한 없는 지연이 된다. #6 백로그의 근거가 하나 더 생긴 셈이다.
- 판정: T2 충족.

### 주장 대 관측 감사자 (수렴형)
- "원인을 확정했다(스크래치 재현)": **관측으로 확인했다.** 이번 라운드에서 별도로 작성한 재현이 작성자 수치(락 없음 2~4/1800, 락 0/1800)와 같은 크기의 결과(락 없음 1~2/1500, 락 0/1500)를 냈다. 직전 라운드가 재현에 실패한 이유는 배경 spawn을 단발로 돌렸기 때문으로 보인다. 이번에는 배경 스레드 3개가 연속으로 spawn했다.
- "이 머신에서는 수정 전 코드도 100회 0실패, 반복 결과는 판별력이 없다": 정직한 자기 한정이다. 이 보고서도 반복 결과를 보조 근거로만 쓴다.
- "전체 cargo test 691 passed": 관측과 같다(`cargo test` 1회, lib 691 passed / 21 ignored, bin·doctest 0).
- "cfg(windows)는 가짜 툴체인으로 cargo check만 통과": **미확인.** 이번 라운드는 재시도하지 않았다(직전 라운드에서 이 머신의 cross check가 `ring` C 빌드에서 실패하는 것을 확인했다). 정적으로 읽은 범위는 이렇다. `silent()`는 `&mut Self`를 반환하므로(`process_util.rs:186`) `.stdin()` 체이닝이 성립한다. `Stdio`는 함수 안에서 import한다(`:115`). `wait_or_kill` 반환값을 버리는 것은 `#[must_use]`가 아니므로 경고가 없다. Windows 테스트의 `cmd /C "echo ready & PING ..."`는 따옴표 안에 특수문자 `&`가 있어서 cmd가 바깥 따옴표만 벗기고 두 명령을 실행한다. 결함은 찾지 못했다.
- "Windows 테스트 8초 미만": 상한 계산은 타임아웃 3초 + taskkill(정상 1초 미만, 상한 5초)이다. 트리 kill이 회귀하면 3초 + 주입 상한 10초가 된다. 판별력은 유지된다.
- 맞지 않은 것: N3(현장 진단 주석).

### 제로베이스 재설계자 (발산형)
- 🔵 R2(장수 자손 테스트의 프로세스 격리)가 이 페르소나의 산출이다. 위 표를 참조한다.
- 질문 하나를 남긴다. 이번 라운드에서 증상(테스트 플레이크)은 테스트 전용 장치로 막았고, 프로덕션의 같은 경합은 DRAIN_GRACE에 맡겼다. 그렇다면 "자손이 파이프를 문다"는 사실은 프로덕션에서 **언제 실제로 일어나는가**? N3 때문에 지금은 그 빈도를 현장에서 셀 수 없다. R1을 검토할 근거 데이터도 쌓이지 않는다.

## 트레이드오프

| 쟁점 | 한쪽 | 다른쪽 | 권고 |
|---|---|---|---|
| 테스트 전용 락 vs 장수 자손 테스트 격리(R2) | 변경 1곳, 테스트 시간 불변, 단언 완화 없음 | 락 밖 spawn이 늘 때마다 N2 같은 구멍이 다시 생긴다 | **현재 락 유지.** 다음에 자식을 띄우는 테스트를 추가할 때 N2의 공유 가드나 R2 중 하나를 고른다 |
| 프로덕션 spawn 직렬화 | 경합 지연 0 | autonomy 동시성을 해치고, 지연이 이미 2초로 묶여 있다 | **도입하지 않는다**(작성자 판단에 동의) |
| taskkill 상한 5초 | 짧게 하면 락 보유 시간이 줄어든다 | 과부하 PC에서 정상 taskkill을 끊어 트리가 남을 수 있다 | **5초 유지.** 끊은 뒤에도 `Child::kill` 폴백이 직속 자식은 죽인다 |

## 기각된 지적

| 후보 | 기각·강등 사유 |
|---|---|
| `wait_or_kill`의 kill 뒤 무한 `wait` | 기각. 대상은 우리가 방금 띄운 taskkill.exe라서 TerminateProcess가 실패할 경로가 없다. 이미 종료됐다면 `wait`가 즉시 반환한다. 재현 경로를 대지 못한다 |
| Windows에서도 같은 fd 상속 경합이 일어나 Windows 테스트(`sort.exe` 2초 단언 등)가 플레이크 | 기각. rust-src `sys/process/windows.rs:343-345`의 `CREATE_PROCESS_LOCK`이 std를 통한 모든 spawn의 핸들 상속 구간을 직렬화한다. 이 앱의 Windows spawn은 모두 std `Command`를 쓴다(grep) |
| Linux에서 같은 경합 | 기각. `sys/pipe/unix.rs:28`의 `pipe2(O_CLOEXEC)`는 원자적이다. CI 매트릭스에도 Linux가 없다(`ci.yml:76`) |
| `eprintln!` 패닉(stderr 쓰기 실패) | 강등 후 기각. Windows GUI 서브시스템의 null 핸들은 std가 쓰기를 버리는 방식으로 처리한다. 터미널을 닫은 뒤의 EPIPE 패닉 가능성은 이 앱의 `eprintln!` 11곳에 공통인 기존 성질이고, 이 커밋의 새 표면이 아니다 |
| 로그에 인자·경로가 새어 시크릿 노출 | 기각. `Path::file_name()`만 남긴다(`process.rs:443-446`). 인자와 env는 출력하지 않는다 |
| 주입 상한 테스트의 `mid` 도착이 느린 CI에서 600ms를 넘김 | 기각. 서브셸의 `sleep 0.1` 뒤 `echo`까지 남는 여유가 약 500ms다. 넘기더라도 실패가 크게 드러나므로 공허한 통과가 아니다 |
| 락 밖 자식이 dev_tools cat 테스트 파이프를 물어 `:677`의 2초 단언이 깨짐(N2의 정방향) | 기각. 락 밖 spawn의 자식 수명은 수백 ms 이하다. `wait_or_kill` 테스트의 sleep은 300ms 뒤 kill되고, 다른 sleep 5 두 곳은 즉시 kill되며, claude_auth 로그인 자식은 약 200ms 뒤 취소된다. 2초를 넘길 수 없다 |
| claude_auth `blocked_stdin_write...`의 EPIPE 지연(N2 경로) | 강등 후 N2에 흡수했다. 대기가 `recv_timeout(10s)`(`claude_auth.rs:1130`)라서 장수 자손 수명(최대 약 5초)보다 길다. 실패 경로는 아니다 |

## 잘 된 점
- **판별력 한계를 스스로 밝힌 커밋 메시지.** "수정 전 코드도 100회 0실패였으므로 반복 결과 자체는 판별력이 없다"고 적고, 근거를 재현 실험에 두었다. 플레이크 수정 보고의 모범이다.
- **최소 범위 락.** `#[cfg(test)]`이고 spawn 한 줄만 감싸서 프로덕션 바이너리에는 한 바이트도 들어가지 않는다. 단언을 완화하거나 `#[ignore]`로 숨기지 않았다.
- **로그 설계.** 파이프별 상태, 상한, 종료사유를 남기고 인자는 남기지 않는다. 이번 리뷰에서 이 로그를 "의도된 3개 테스트 밖 누수가 없다"는 관측 장치로 바로 쓸 수 있었다.
- **`wait_or_kill`을 플랫폼 중립으로 분리해 Unix에서 테스트한 점.** Windows 실기가 없는 환경에서 로직 부분만이라도 실행 검증한다.
- **Windows 테스트 전제 강화.** `ready`와 `timed_out`을 함께 단언해, 손자가 없는 상태의 공허한 통과를 막았다. PING flush는 확인하지 못했으므로 단언하지 않는다고 명시한 것도 정직하다.
- **주석 3곳을 고치면서 한계까지 함께 적었다**(`plugins/mod.rs:29-32` "그런 자손은 고아로 남을 수 있다").

## 실행한 검증
- `git show c0f325a`(stat과 전체 diff), `git log ed5f435..c0f325a`. 6ad4e94는 직전 보고서와 현재 코드(`process.rs` 120-460, 650-1020)를 대조했다
- `cargo test --lib dev_tools::process`(debug, 테스트 바이너리 직접 실행): **100회, 실패 0**(10회마다 진행 로그, 매회 11/11)
- `cargo test --lib` 전체(debug, `--nocapture`): **50회, 실패 0**. 상한 초과 로그는 실행마다 3줄이고 전부 의도된 테스트에서 나왔다(합계 150줄)
- `cargo test --lib` 전체(debug, `--test-threads=32`): **30회, 실패 0**, 실행마다 로그 3줄
- release 프로필: 모듈 **20회**, 전체 **10회**, 실패 0(691 passed / 21 ignored)
- `cargo test`(플래그 없음, CI와 같은 명령) 1회: lib 691 passed / 21 ignored, bin·doctest 0, 실패 0
- 스크래치 재현(scratchpad의 std만 쓰는 독립 Cargo 프로젝트, 프로젝트 코드와 무관): cat 1500회와 배경 `sh -c 'sleep 1.2 &'` 3스레드. **락 없음 2·1·2 / 락 있음 0·0·0**(EOF 1초 초과 건수)
- rust-src(rustc 1.98.1) Read: `sys/pipe/unix.rs:14-39`(macOS는 비원자, Linux는 `pipe2`), `sys/process/windows.rs:343-345`(`CREATE_PROCESS_LOCK`)
- `cargo clippy --lib --tests`: 변경 파일 경고는 `process.rs:320` 1건(기존)
- grep: `kill_process_group_with_grace` 호출부, 락 밖 테스트 spawn 지점, 낡은 주석 키워드, 로깅 기반, 장수 sleep 테스트
- 반복 실행 뒤 `pgrep`: 테스트가 남긴 `sleep 60/30` 0건. `sleep 5` 1건은 몇 초 뒤 사라졌다(N4). 스크래치의 `sleep 1.2`도 0건으로 정리됐다
- 실행하지 않은 것: Windows 실기와 cross `cargo check`(재시도 안 함), N2 역방향의 실측 재현, 실제 claude 작업. 코드 수정·커밋·push·worktree 생성은 하지 않았다. 직전 보고서는 손대지 않았다. 페르소나 적용 이력과 INDEX.md 갱신은 작업 트리에 커밋하지 않은 채 남겼다

## 생략한 관점
- 화면 리뷰: 해당 없음(백엔드와 주석만 변경)
- persona-heterogeneous-machine-operator: 생략(사유는 재사용 판정 표 참조)
- Windows 실행: windows-latest CI 몫. 그 전까지 `wait_or_kill`의 Windows 경로, 트리 kill 테스트, `ready` 단언은 "정적 읽기만 함"이다

## PM 권고
1. **이 커밋 범위를 승인해도 된다**(Critical·Major 없음). 남은 확정 조건은 windows-latest CI 첫 실행에서 `timeout_kill_terminates_grandchild_tree_so_pipe_closes_before_drain_grace`와 기존 Windows 테스트가 초록인지 확인하는 것뿐이다. `ready` 단언이 실패하면 트리 kill 결함이 아니라 테스트 전제(cmd 출력 flush) 문제부터 의심한다.
2. N1(autonomy taskkill 사본)을 #6 백로그 항목에 합친다. 직전 보고서 #3의 "4개 호출부가 모두 덮인다"는 문장은 이 보고서로 정정한다.
3. N2와 N4는 함께 처리하면 비용이 가장 작다. 공유 `test_spawn_guard()`로 옮기고 그룹째 kill하면 된다. 급하지 않다. macos-latest CI가 `claude_auth.rs:1198`에서 실패하면 이 경로를 먼저 의심한다.
4. N3은 주석 한 줄만 고치면 된다. 현장 로그 파일은 별도 백로그로 둔다(앱 전체 공통 문제).
5. R2는 다음에 자식을 띄우는 테스트를 추가할 때 검토한다. R1과 함께 보면 프로덕션과 테스트의 같은 근본 원인을 한 번에 다룰 수 있다.
