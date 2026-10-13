# 프로세스 drain 상한·Windows 트리 kill 리뷰 보고서

리뷰 대상: target_id `r3-process-drain-and-tree-kill` / 커밋 범위 `ed5f435..6ad4e94` (커밋 6ad4e94 1개, 파일 `src-tauri/src/dev_tools/process.rs` 1개, +317/-32)
관련 직전 리뷰(맥락용): `docs/reviewer/review-r2-p1-stability-batch-2026-10-13-r2.md` N1, `docs/reviewer/review-r2-p1-stability-batch-2026-10-13.md` Minor #3
리스크 범주: 사용자 PC 자식 프로세스 실행(프로세스 수명·동시성·Windows 트리 kill)
등급·모드: Sensitive, 풀패널, 최초 리뷰. 위임 지시대로 증분이나 축소 모드는 쓰지 않았고 커밋 전체를 대상으로 했다.
리뷰 페르소나 패널(4명, 모두 `docs/reviewer/personas/`에 있음): `persona-process-execution-security.md`, `persona-tauri-runtime-thread-and-lock-discipline.md`, `persona-claimed-vs-verified.md`, `persona-zero-base-redesigner.md`(발산형)
리뷰 일자: 2026-10-13
종합 판정(커밋 범위 `ed5f435..6ad4e94`): 🟡 Amber

## 요약 (2분 규칙)
목적은 직전 N1과 #3이었다. 리더 join에 상한이 없어서 `PLUGIN_OP_LOCK`이 영구히 잡힐 수 있었다. 이 목적은 코드상 달성됐다. `run_process_inner`는 자식이 끝나거나 kill된 뒤 두 파이프에 마감시각 하나(DRAIN_GRACE 2초)를 같이 걸고 기다린다. 마감 전에 읽은 출력은 공유 버퍼에 남는다(`process.rs:395-401`, `245-255`). 그래서 가드 수명에도 상한이 생겼다. Unix는 timeout + 3초 유예 + 2초다. Windows는 timeout + taskkill 소요 시간 + 2초다. 정상 종료 경로에는 지연이 없다. Windows kill은 셸 없이 System32 절대경로의 `taskkill /T /F`로 바뀌었고, S1·S3 불변식은 그대로다.

Critical은 없다. Major는 1건이다. 이 커밋 이후 기존 테스트 `stdin_is_null_so_a_reading_process_fails_fast_instead_of_hanging`이 간헐적으로 실패한다. 이 머신에서 75회 중 2회 실패했고, 같은 조건의 직전 커밋은 75회 중 0회였다. 실패하는 단언은 `elapsed < 2s`(`process.rs:620`)다. 이 2초가 DRAIN_GRACE와 같은 값이라는 점이 원인을 가리킨다. 이 저장소는 CI를 유일한 기술 게이트로 쓴다. 그래서 테스트 플레이크를 Major로 판정했다. 원인 메커니즘은 가설이다(아래 #1).

## 페르소나 재사용 판정 (산출물 게이트)
착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 없다. 4개 모두 재사용했고 본문 6대 요소는 고치지 않았다. 각 파일의 "적용 이력"에 항목을 붙였고, INDEX.md의 "최근 재사용"도 갱신했다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| persona-process-execution-security.md | 재사용 | 관심사 4(프로세스 수명)와 S5(리더 스레드·그룹 kill)가 이번 변경의 중심이다 |
| persona-tauri-runtime-thread-and-lock-discipline.md | 재사용 | 이번 변경은 `PLUGIN_OP_LOCK` 가드 수명을 정하는 마지막 무상한 블로킹을 없앤다. T2와 정확히 맞는다 |
| persona-claimed-vs-verified.md | 재사용 | 커밋 메시지·테스트 이름·호출부 주석이 바뀐 동작과 맞는지가 쟁점이다 |
| persona-zero-base-redesigner.md | 재사용(발산형, 필수) | "파이프 EOF를 출력 끝으로 본다"는 전제 자체를 지우는 대안을 제시하는 역할이다 |
| persona-heterogeneous-machine-operator.md | 생략 | 이 커밋의 Windows 측 쟁점은 taskkill 의미론과 S1이다. 기계 다양성 문제는 아니라서 process-execution-security가 맡았다 |

## 직전 지적 상태 추적

| 직전 | 심각도 | 이번 상태 | 근거 |
|---|---|---|---|
| r2 N1 (영구 Busy) | 🟡 | **해소** | `process.rs:395-401`에서 join이 `collect_until(deadline)`으로 바뀌었다. 가드 해제는 이제 위 상한 안에서 일어난다. 단, Windows의 taskkill `.output()` 자체에는 상한이 없다(아래 #3) |
| r1 #3 (join 상한 없음) | 🟡 | **해소(이 헬퍼에 한해)** | 같은 결함이 있는 형제 사본 3곳은 그대로다(아래 #6) |

## 지적 사항

| # | 심각도 | 영역 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| 1 | 🟠 Major | 테스트 결정성(CI 게이트) | `src-tauri/src/dev_tools/process.rs:620`(실패 단언), 장수 자손을 만드는 새 테스트 `:788-819`(`sleep 60 &`), `:827-863`(setsid `sleep 60`). 원인 후보는 rust std `library/std/src/sys/pipe/unix.rs`의 non-`pipe2` 분기(macOS) | `cargo test --lib -q dev_tools::process`를 HEAD에서 75회(10+25+40) 돌려 **2회 실패**했다. 두 번 모두 같은 테스트, 같은 줄(620)이었다. 같은 명령을 직전 커밋 `ed5f435`의 임시 worktree에서 75회 돌려 **0회 실패**였다. 전체 `cargo test --lib`은 5회 모두 통과했다. 원인 후보는 rustup의 rust-src를 Read해 확인했다 | 직전 커밋에도 있던 테스트가 이 커밋 이후 간헐적으로 실패한다. 실패 단언은 `/bin/cat`이 2초 안에 끝나야 한다는 것이고, 이 2초는 DRAIN_GRACE와 같다. 즉 cat의 stdout EOF가 오지 않아 drain 상한까지 기다렸다는 뜻이다. **가설**은 이렇다. macOS std는 파이프를 `pipe()`로 만든 뒤 `set_cloexec`를 따로 호출한다(비원자적). 그 틈에 병렬 테스트가 `/bin/sh`를 spawn하면, sh와 그 `sleep 60` 자손이 cat의 파이프 쓰기 끝을 물려받는다. 그러면 cat의 EOF는 KillOnDrop이 sleep을 죽일 때까지 오지 않는다. 직전 커밋에는 오래 사는 자손을 남기는 테스트가 없었다. 그래서 같은 누수가 있었더라도 수백 ms 안에 가려졌을 것이다. 별도 스크래치 실험(1000회 동시 spawn)으로는 재현하지 못했다. 그래서 메커니즘은 미확정이고, 실패 사실과 대조군 차이만 관측했다. CI는 macos-latest에서 debug와 release로 두 번 돈다(`.github/workflows/ci.yml:96-110`). 유일한 게이트가 간헐적으로 빨개진다 | (a) 테스트: process.rs 테스트 중 자식을 띄우는 것 전부를 테스트 전용 `static SPAWN_TEST_LOCK: Mutex<()>`으로 직렬화한다. 비용이 가장 작다. 또는 장수 자손 테스트를 `#[ignore]` + 별도 CI 스텝으로 분리한다. (b) 수정 후 `for i in $(seq 1 100)` 반복 실행으로 0회 실패를 확인하고 그 결과를 커밋 메시지에 남긴다. (c) 프로덕션 영향은 작다. 같은 누수가 앱 안의 병렬 spawn에서 일어나도 이제는 최대 2초 지연에 그치고 출력은 잃지 않는다. 오히려 이 커밋이 막아 주는 쪽이다. 아래 🔵 R1이 근본 대안이다 |
| 2 | 🟡 Minor | 관측 가능성 | `process.rs:245-255`(Timeout 분기), `:395-401` | Read로 확인. 상한 초과 시 로그·플래그 출력이 있는지 grep했다(`eprintln` 0건) | 상한 초과는 곧 "그룹·트리를 벗어난 자손이 살아서 파이프를 물고 있다"는 뜻이다. 이 사실이 호출자에게도 로그에도 남지 않는다. 반환값을 바꾸지 않은 결정 자체는 타당하다. 자식이 종료 전에 쓴 출력은 파이프 버퍼에서 µs 단위로 다 읽히므로, 잃는 것은 자손이 마감 뒤에 쓴 출력뿐이다. 그러나 고아가 남았다는 신호가 0이면, #1 같은 현상이나 실제 고아 누적을 현장에서 진단할 수 없다 | Timeout 분기에서 `eprintln!("[process] drain 상한 초과: pid={pid}, 그룹 밖 자손이 파이프를 물고 있음")` 한 줄. 필요하면 `ProcessRunOutput`에 `drain_capped: bool`을 추가한다(구조체 리터럴은 이 파일 2곳뿐) |
| 3 | 🟡 Minor | Windows 수명 | `src-tauri/src/process_util.rs:110-113`, 호출 `process.rs:181` | Read로 확인 | `taskkill ... .output()`에는 상한이 없다. 이제 이 호출이 plugins 경로의 `PLUGIN_OP_LOCK` 가드 안에서 돈다. 보통은 1초 안에 끝나지만, taskkill이 멈추면 N1이 다른 모양으로 되살아난다. 재현 경로는 확인하지 못했다(Windows 실기 없음). 같은 호출은 autonomy 종료 경로(`autonomy/runtime.rs:268-273`)와 claude_auth·session_chat 취소 경로에서 이미 쓰이므로, 새 위험이라기보다는 기존 위험이 락 안으로 들어온 것이다 | `spawn()` 후 `try_wait` 폴링에 상한(예: 5초)을 두고, 넘으면 `kill()`한다. `kill_process_group_with_grace` 하나만 고치면 4개 호출부가 모두 덮인다 |
| 4 | 🟡 Minor | 동작 변화(트레이드오프) | `process.rs:216-234` | 코드 추적 | 리더를 포기하면 그 리더는 다음 `read` 복귀 때 루프를 빠져나가고 파이프 읽기 끝을 닫는다. 그 뒤 파이프를 물고 있던 자손이 다시 쓰면 SIGPIPE·EPIPE로 죽는다. 이전에는 함수가 반환하지 않는 대신 자손은 살아 있었다. 의도적인 장수 자손(예: autonomy `claude -p`가 남긴 백그라운드 서버)이 있다면 동작이 바뀐다. 포기한 스레드 2개와 fd 2개는 자손이 쓰거나 닫을 때까지 남는다. 상한은 1회당 2개이고, 버퍼는 `take` 후 비며 최대 8KB가 추가된다 | 의도라면 `DRAIN_GRACE` 주석에 "포기 후 자손의 다음 write는 EPIPE가 된다"를 현재형으로 한 줄 적는다. 자손을 살려야 한다면 포기 후에도 읽어서 버리는 방식이 대안인데, 대가로 스레드가 영구히 남는다 |
| 5 | 🟡 Minor | 주장과 실제 불일치(주석) | `src-tauri/src/dev_tools/actions.rs:28-30`, `src-tauri/src/plugins/mod.rs:28-30`, `:265-268` | grep `force_kill_process_group\|직속 자식\|TerminateProcess`로 찾았다 | 이 커밋 이후 사실과 다른 주석이 3곳 남았다. actions.rs는 "타임아웃 kill(직속 자식만 죽인다 — process.rs의 기존 Windows 주석 참고)"라고 적었는데, 그 주석은 이제 트리 kill을 말한다. plugins/mod.rs:265-268은 "Windows는 직속 자식 TerminateProcess"라고 적었다. plugins/mod.rs:28-30의 "wait()까지 끝낸 뒤(고아 없음)"도 이제는 거짓이다. 상한 초과 경로가 그룹 밖 자손을 고아로 남긴 채 반환하는 것이 바로 이 커밋의 설계다 | 세 곳을 현재 동작으로 고친다. 특히 "고아 없음"은 "그룹·트리 밖 자손은 남을 수 있고, 호출은 DRAIN_GRACE 안에 반환한다"로 바꾼다 |
| 6 | 🟡 Minor | 범위(같은 결함 다른 자리) | `src-tauri/src/mcp_manager/process.rs:103-104`, `src-tauri/src/session_chat/turn.rs:437`, `src-tauri/src/claude_auth.rs:714` | grep `read_to_end\|join()` | 이번에 고친 "상한 없는 리더 join"이 형제 사본 3곳에는 그대로 있다. mcp_manager는 주석(`:31-37`)에서 이 헬퍼와 "동일한 관용구"를 "가볍게 재현"했다고 밝히므로 같은 결함을 물려받았다. 이번 커밋이 이 자리들을 해소했다고 오독할 위험이 있다 | `PipeDrain`/`spawn_pipe_drain`을 `process_util`로 올려 4곳이 공유하게 한다(`pid_alive`·`kill_process_group_with_grace`와 같은 "복붙 금지" 원칙). 백로그로 두더라도 이번 보고에 "process.rs 한정"이라고 명시한다 |
| 7 | 🟡 Minor | 테스트 전제 검증 | `process.rs:869-895`, `:906-928` | Read와 코드 추적 | ① `drain_grace_is_injectable_and_keeps_only_output_read_before_deadline`에는 하한 단언이 없다. 자손이 파이프를 물지 않았다면 즉시 반환해도 통과한다. 또 "late"는 반환 뒤(1초)에 쓰이므로, 리더 포기가 고장나도 결과에 끼어들 수 없다. 테스트 이름의 "keeps only output read before deadline"은 검증되지 않는다. ② Windows 테스트는 PING이 kill 전에 실제로 떴는지 확인하지 않는다. CI가 느려 700ms 안에 PING이 안 뜨면 공허하게 통과한다(실패하지는 않는다) | ① `elapsed >= 250ms` 하한을 추가한다. 이름은 "injected drain cap is honored"로 좁히거나, 마감 전에 출력이 도착하는 시나리오(`sleep 0.1; echo mid`)로 바꾼다. ② 타임아웃을 2초로 늘리거나, `cmd /C "echo ready & PING ..."`로 ready 출력을 단언한다 |
| 8 | ⚪ Nit | 테스트 여유 | `process.rs:890` | Read | 900ms 상한에서 여유는 약 450ms다(spawn과 100ms 폴링을 빼면). 느린 CI 러너에서 경계에 걸릴 수 있다. 이 테스트가 가르려는 것은 "주입한 300ms 대 기본 2초"이므로 2초 미만이면 어떤 값이든 충분하다 | `< 1500ms`로 넓힌다 |
| 9 | ⚪ Nit | 테스트 견고성 | `process.rs:831` | Read | perl 안의 sh 스크립트가 pid 파일 경로를 따옴표 없이 넣는다(`echo $$ > {pf}`). TMPDIR에 공백이 있으면 깨진다. 실패는 시끄럽게 난다(`:858` expect) | 내부 sh에서도 작은따옴표로 감싸거나, perl에서 직접 pid를 쓴다(`open` + `print $$`) |
| 10 | ⚪ Nit | lint | `process.rs:320` | `cargo clippy --lib --tests` | `run_process_inner`(인자 9개)에서 `too_many_arguments` 경고가 하나 늘었다. 기존 `:295`에도 같은 경고가 있다 | `#[allow(clippy::too_many_arguments)]` 또는 옵션 구조체. 우선순위는 낮다 |
| R1 | 🔵 Rethink | 구조 | `process.rs:198-256`, `:382-401` | 설계 대안 검토 | 결함의 뿌리는 "파이프 EOF를 출력 끝으로 본다"는 전제다. EOF는 쓰기 끝을 가진 **모든** 프로세스가 닫아야 오기 때문에, 그룹 밖 자손, fd 상속 경쟁(#1), Windows 손자가 모두 같은 증상을 낸다. 이번 커밋은 그 증상에 시간 상한을 씌웠다. 그 대가로 포기한 스레드, SIGPIPE 동작 변화, 2초 지연이 생겼다 | **대안 구조: 임시 파일 캡처.** stdout/stderr를 파이프 대신 생성 직후 unlink한 익명 임시 파일(0600, Windows는 `FILE_FLAG_DELETE_ON_CLOSE`)로 연결한다. 자식이 끝나면(또는 kill 후) 파일을 처음부터 읽는다. 리더 스레드, 64KB 교착, EOF 대기, 포기한 스레드, SIGPIPE, drain 상수가 모두 사라진다. #1의 누수 fd도 무해해진다. 호출부 9곳은 모두 끝난 뒤에 출력을 소비하므로 스트리밍이 필요 없다. 비용은 디스크 쓰기, 자식이 대량 출력할 때의 디스크 사용량(파일 크기 상한 검사 필요), 인증 출력이 디스크에 잠깐 머무는 점(unlink + 0600으로 완화)이다. Windows 트리 종료는 Job Object(`KILL_ON_JOB_CLOSE`)로 가야 고아 손자까지 덮는다(설계 §9 미해결쟁점 1이 이미 인정한 한계). 이번 커밋은 최소 수정으로 합리적이다. 다음에 이 헬퍼를 건드릴 때는 상한을 하나 더 얹지 말고 이 전환을 검토할 것을 권고한다 |

## 페르소나별 관점

### 프로세스 실행 보안 감사자 (수렴형)
- S1·S3: 유지된다. Windows kill은 `windows_system_tool(&roots, r"System32\taskkill.exe")` 절대경로이고, argv는 정적 상수와 정수 pid다(`process_util.rs:108-113`). 셸 경유는 0이다. 동적 값은 우리가 spawn한 자식의 `child.id()`뿐이다.
- PID 재사용: 해당 없다(아래 "기각된 지적" 참조). taskkill은 `child.wait()` 전에 실행되므로(`process.rs:181-183`) 직속 자식 핸들이 열려 있고, 그 PID는 OS가 재할당하지 않는다.
- S5: 리더 스레드에 상한이 생기면서 S5가 이제 "정상 종료든 kill이든 반환한다"까지 보장한다. 남은 구멍은 #3(taskkill 무상한)과 #6(형제 사본)이다.
- 판정: 필수 기준 미충족은 없다. Minor 3건(#3·#4·#6).

### Tauri 런타임 스레드·락 규율 감사자 (수렴형)
- T2: `PLUGIN_OP_LOCK` 가드는 job과 같은 수명이다(`plugins/mod.rs:396-406`). 이 커밋으로 가드 안의 블로킹에 처음으로 전부 상한이 생겼다(Unix 기준). Windows는 #3 때문에 "거의"다.
- 앱 종료: `lib.rs:216`의 `request_shutdown_and_wait(2s)`는 drain과 무관하게 2초 뒤 `force_kill_all_remaining`으로 넘어간다(`autonomy/mod.rs:435-445`). 종료 시간은 늘지 않는다.
- 병렬 spawn: #1의 가설은 이 앱의 프로덕션 병렬 spawn에도 똑같이 적용된다(autonomy 워커, plugins, dev_tools 조회). 이전에는 한 실행의 EOF가 다른 장수 프로세스(예: 30분짜리 `claude -p`)에 묶일 수 있었다. 이제는 그 비용이 2초로 줄었다. 이 커밋의 숨은 이득이다.
- 판정: T2 충족(Windows는 #3 조건부).

### 주장 대 관측 감사자 (수렴형)
- 커밋 메시지의 "정상 종료는 EOF가 즉시 와서 지연이 없다"는 관측과 맞았다. 빠른 프로세스 테스트 3종이 통과했고, 모듈 전체가 3.2초다.
- "공개 시그니처·호출부는 그대로다"는 맞았다. 호출부 9곳의 시그니처가 변하지 않았다(grep).
- 맞지 않은 것: 다른 모듈 주석 3곳(#5), 테스트 이름이 주장하는 내용(#7), "회귀 테스트 4개"가 기존 테스트 하나를 간헐적으로 깨뜨린 점(#1).
- 위임 메시지의 "cross `cargo check` 통과" 주장은 이 머신에서 재현하지 못했다. `cargo check --target x86_64-pc-windows-msvc --lib`(과 `--tests`)는 `ring`의 C 빌드(`assert.h` 없음)와 `tauri-app` 빌드 스크립트 실패로 이번 코드까지 가지 못한다. 작성자 환경의 툴체인 차이로 보인다. 그래서 cfg(windows) 코드의 컴파일 근거는 **미확인**이다. 정적으로 읽기만 했다: `kill_process_group_with_grace(u32)`는 `process_util.rs:107`에 `pub fn`으로 있고, Windows 테스트가 쓰는 `run_process_inner`·`Instant`·`Duration`은 `use super::*`로 들어온다. 이 범위에서는 문제를 찾지 못했다. 확정은 windows-latest CI 몫이다.

### 제로베이스 재설계자 (발산형)
- 🔵 R1(임시 파일 캡처 + Job Object)이 이 페르소나의 산출이다. 위 표 참조.
- 질문 하나를 남긴다. "자손이 파이프를 물고 있다"는 사실 자체가 제품에 의미 있는 신호인가? plugins나 autonomy가 고아를 남기는 것은 사용자 PC 자원 누수다. 그렇다면 R1로 증상을 지우는 것만으로는 부족하고, #2의 로그가 그 신호를 살리는 최소 장치다.

## 트레이드오프

| 쟁점 | 한쪽 | 다른쪽 | 권고 |
|---|---|---|---|
| DRAIN_GRACE 2초 | 짧게(예: 500ms)하면 고아가 있을 때 지연이 줄어든다 | 길게 하면 느린 머신에서 마지막 버퍼를 덜 잃는다. 단, 자식이 종료 전에 쓴 출력은 2초와 무관하게 이미 보존된다 | **2초 유지.** 지연은 고아가 있을 때만 생기고, 잃는 것은 자손의 사후 출력뿐이다. 상한값보다 #2(초과 사실의 기록)가 중요하다 |
| 상한 초과를 반환값에 드러내지 않음 | 호출부 9곳 무수정, 위험 표면 최소 | 고아 발생이 영영 보이지 않는다 | 반환값은 그대로 두고 **로그 한 줄**로 절충한다(#2) |
| 포기 후 리더 종료 vs 계속 읽어 버림 | 스레드가 결국 끝난다. 대신 자손이 SIGPIPE를 받는다 | 자손은 살지만 스레드가 영구히 남는다 | 현재 선택을 유지하고 주석으로 명시한다(#4). 근본 해소는 R1 |

## 기각된 지적

| 후보 | 기각·강등 사유 |
|---|---|
| Windows taskkill PID 재사용으로 무관한 프로세스 kill | 기각. `process.rs:181`의 taskkill은 `:183` `child.wait()` 전에 실행되고, `Child`가 프로세스 핸들을 쥐고 있는 동안 Windows는 그 PID를 재할당하지 않는다. `/T` 탐색이 stale ParentProcessId를 따라갈 가능성은 남지만, 같은 호출이 autonomy·claude_auth·session_chat에 이미 있어 이 커밋의 새 표면이 아니다 |
| 정상 종료 경로 지연 회귀 | 기각. 자손이 없으면 EOF가 즉시 와서 `recv_timeout`이 바로 반환한다. 빠른 프로세스 테스트(`:503`, `:601`, `:707`)가 75회 중 73회 통과했고, 실패 2회는 #1의 자손 간섭 패턴이었다 |
| 메모리 무상한 | 회귀로서는 기각. 버퍼 무상한은 이전 `read_to_end`와 같은 성질이다. 포기한 리더의 버퍼는 `take` 뒤 비고, 최대 8KB만 더 붙는다 |
| `collect_until`의 deadline~take 사이 바이트 유실 | 기각. 마감 뒤 데이터를 버리는 것은 설계 의도이고, 뮤텍스 덕분에 부분 기록(찢긴 쓰기)은 없다 |
| 앱 종료 시간 연장 | 기각. `lib.rs:216`의 2초 `max_wait`가 drain과 독립적으로 종료를 끊는다 |
| Windows 테스트의 하드코딩 `C:\Windows` 플레이크 | 기각. 기존 Windows 테스트(`:529`, `:575`, `:634`)와 같은 관용구이고, windows-latest 러너는 표준 설치다 |
| perl·setsid 의존 플레이크 | 강등 후 기각. rust-test 매트릭스는 macos-latest와 windows-latest뿐이고(`ci.yml:76`) macOS에는 `/usr/bin/perl`이 있다. perl이 사라지면 `:858` expect가 크게 실패하므로 공허한 통과가 아니다 |
| cfg(windows) 컴파일 실패 가능성 | 지적으로 올리지 않았다. 정적 읽기로는 문제를 찾지 못했다. 로컬 cross check를 재현하지 못했으므로 "미확인"으로만 남긴다(위 주장 대 관측 섹션) |

## 잘 된 점
- **마감시각 하나를 두 파이프가 공유한다**(`:395`). 순차 `recv_timeout`이지만 합계 상한은 2초다. 파이프별로 2초를 주는 흔한 실수를 피했다.
- **출력 보존 설계.** 공유 버퍼에 즉시 쌓으므로 포기해도 그때까지 읽은 내용이 살아남는다. 테스트 `:816-817`이 stdout과 stderr 양쪽을 확인한다.
- **판별력 있는 Windows 테스트 설계**(`:906-928`). 상한을 일부러 10초로 키워, 6초 안의 반환이 "상한 덕분"이 아니라 "트리 종료 덕분"임을 가른다. 트리 kill이 회귀하면 확실히 빨개진다.
- **전제 단언.** `:812-815`(EOF가 안 와서 실제로 기다렸다는 하한)와 `:859-862`(setsid 자손이 정말 살아 있었다는 확인)는 공허한 초록을 막는다. 이전 라운드 V1의 교훈을 반영했다.
- **KillOnDrop 가드.** 단언이 실패해도 자손을 정리한다. 반복 115회 실행 뒤 `pgrep "sleep (60|30)"`는 0건이었다.
- **한계를 현재형 주석으로 남겼다**(`:167-178`: 고아 손자, UAC 승격 손자).
- **공개 시그니처를 보존하고 상한만 주입 가능하게 분리했다**(`run_process_inner`).

## 실행한 검증
- `git show 6ad4e94`(stat, 제거 라인), `ed5f435` 시점 `process.rs` 대조
- `cargo test --lib dev_tools::process`: 1회 11/11 통과. 이어 반복 75회(10+25+40) 중 **2회 실패**, 둘 다 `:620`
- 대조군: `ed5f435` 임시 worktree(scratchpad, 작업 후 `git worktree remove`)에서 같은 명령 75회 중 **0회 실패**(8개 테스트)
- 부분 집합 반복(`stdin_is_null` + 자손 테스트 등 4개) 60회 중 0회 실패
- `cargo test --lib` 전체 5회: 모두 690 passed, 0 failed, 21 ignored
- 스크래치 실험: 동시 spawn fd 상속 재현 시도 2종(각 400·600회)에서 0건. 재현에 실패했다
- rust-src `library/std/src/sys/pipe/unix.rs` Read(macOS 비원자 CLOEXEC 확인), `process/unix/unix.rs` grep(`POSIX_SPAWN_CLOEXEC_DEFAULT` 사용 없음)
- `cargo check --target x86_64-pc-windows-msvc --lib`(`--tests` 포함): **이 머신에서 실패**(ring C 빌드, tauri-app 빌드 스크립트). cfg(windows) 컴파일은 미확인
- `cargo clippy --lib --tests`: process.rs 경고 2건(`:295` 기존, `:320` 신규)
- 호출부 grep: `run_process_with_timeout*` 9곳, `kill_process_group_with_grace` 호출부, 형제 리더 사본 3곳
- 실행하지 않은 것: 실제 claude 설치·업데이트·인증·플러그인 작업, Windows 실기 실행. 코드 수정·커밋·push는 하지 않았다. 페르소나 적용 이력과 INDEX.md 갱신은 작업 트리에 커밋하지 않은 채 남겼다

## 생략한 관점
- 화면 리뷰: 해당 없음(백엔드 단일 파일 변경)
- persona-heterogeneous-machine-operator: 생략(사유는 재사용 판정 표 참조)
- Linux 실행: rust-test 매트릭스에 없고 로컬 환경도 없어 생략

## PM 권고
1. **#1 Major를 다음 커밋에서 해소한다.** 테스트 전용 spawn 직렬화 락을 둔 뒤 100회 반복으로 0회 실패를 확인하고, 그 결과를 커밋 메시지에 남긴다. 해소 전까지 macos-latest CI 실패가 나면 먼저 `:620`인지 확인한다.
2. #2(로그 한 줄)와 #5(주석 3곳)는 비용이 거의 없으니 #1과 함께 처리하길 권고한다. #5의 "고아 없음"은 이 커밋 설계와 정면으로 충돌하는 문장이다.
3. #3(taskkill 상한)은 Windows 실기 검증이 가능할 때 `process_util` 한 곳에서 처리한다.
4. #6(형제 사본 3곳)은 백로그로 둔다. 다만 "join 상한 해소"를 process.rs 한정으로 기록해, mcp_manager·session_chat·claude_auth까지 해소된 것으로 오독되지 않게 한다. 비용 라벨은 낮음~중간이다(근거: `PipeDrain`이 이미 독립 구조체라 이동만 하면 된다. 각 호출부의 kill 정책 차이는 미확인 추정이다).
5. windows-latest CI 첫 실행에서 `timeout_kill_terminates_grandchild_tree_so_pipe_closes_before_drain_grace`와 기존 Windows 테스트 4종의 결과를 확인해야 cfg(windows) 경로가 확정된다. 그 전까지 Windows 측은 "미검증"이다.
6. R1(임시 파일 캡처 + Job Object)은 다음에 이 헬퍼를 손볼 때 검토할 대상으로 남긴다. 지금 바꿀 필요는 없다.
