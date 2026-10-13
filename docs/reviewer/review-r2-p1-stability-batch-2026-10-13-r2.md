# P1 안정화 배치 재검토(2차) 리뷰 보고서

리뷰 대상: target_id `r2-p1-stability-batch` / 커밋 범위 `27f0d3b..388e151` (main — dcb34f2, 81553f3, c4acf02, b020829, 388e151). 이번 라운드의 증분은 388e151(`src-tauri/src/plugins/mod.rs` 1개 파일)이다.
직전 리뷰: `docs/reviewer/review-r2-p1-stability-batch-2026-10-13.md` (1차, 종합 Amber, 🟠 #1 = plugins 커맨드 async화 뒤 동시 실행 직렬화가 사라짐)
리스크 범주: 사용자 PC 자식 프로세스 실행(프로세스 수명·동시성). 직전과 같은 범주다.
리뷰 페르소나 패널: `persona-tauri-runtime-thread-and-lock-discipline.md`, `persona-process-execution-security.md`, `persona-zero-base-redesigner.md`(발산형). 3개 파일 모두 `docs/reviewer/personas/`에 있다.
리뷰 일자: 2026-10-13
종합 판정(커밋 범위 `27f0d3b..388e151` 전체): 🟢 Green

## 요약 (2분 규칙)
직전 🟠 #1은 해소됐다. 자식 프로세스를 띄우는 plugins 커맨드 5종이 모두 `run_blocking` 하나를 거치고, `run_blocking`은 blocking 풀 클로저 안에서 `PLUGIN_OP_LOCK.try_lock()`을 잡는다. 가드는 job 전체가 끝날 때까지 살아 있다(update_plugin의 2단계, 타임아웃 kill/wait 포함). 겹친 요청은 대기 없이 바로 Busy 실패를 받는다. poison은 무시한다. 테스트 2개를 추가했고, `cargo test --lib plugins`는 39개 통과다(직전 37개). 수정이 만든 결함은 없다. 다만 직전 Minor #3(리더 스레드 join에 상한 없음)이 실제로 발생하면, 이제는 워커 하나가 묶이는 데서 그치지 않고 플러그인 작업 전체가 앱을 재시작할 때까지 거부된다. 이 영향 확대를 🟡 N1로 기록한다. 남은 Critical·Major는 없다.

## 모드 판정 (재검토 3요소 대조)
- 재검토 3요소(target_id·직전 리뷰 경로·리스크 범주)가 위임에 모두 실려 있다.
- 풀패널 강제 승격 조건을 확인했다.
  - 직전 Major 미해결: 해당 없음. 그 Major의 수정 여부를 확인하는 것이 이번 재검토의 목적이고, 아래에서 해소를 확인했다.
  - 새 실행 경로·리스크 표면 다수: 해당 없음. 새 표면은 락 1개뿐이고 새 프로세스 실행 경로는 없다.
  - 다른 Sensitive 하위도메인 진입: 해당 없음. `git diff --name-only b020829..388e151` 결과는 `plugins/mod.rs` 하나다.
  - 4차 이상: 해당 없음. 이번이 2차다.
  - 명시적 풀패널 요청: 없음.
- 동일 대상 인정 4조건: 같은 target_id, 같은 날(7일 이내), 리스크 범주 불변(동시성/프로세스 수명), 파일 중첩(`plugins/mod.rs`는 직전 범위에도 있었다). 모두 충족한다.
- 새 리스크 표면(전역 락)이 있으므로 **증분(B) 모드**로 진행했다. 락을 직접 보는 런타임 감사자와 프로세스 수명 감사자, 그리고 필수인 발산형으로 패널을 구성했다. 직전 패널의 async-response-ordering 페르소나는 이번 diff에 프론트 응답 순서 변경이 없어서 생략했다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| persona-tauri-runtime-thread-and-lock-discipline.md | 재사용 | 역할개념 "사라진 암묵적 직렬화를 누가 대신하는가"가 이번 수정 대상(#1)과 그대로 맞는다 |
| persona-process-execution-security.md | 재사용 | 역할개념에 "프로세스 수명"이 있다. 가드 수명이 kill/wait를 덮는지 확인하는 역할이다 |
| persona-zero-base-redesigner.md | 재사용 | 발산형 정본이며 필수다 |
| persona-async-response-ordering.md | 생략 | 이번 증분에 프론트 응답 순서 변경이 없다. 직전 판정(Green)을 유지한다 |

## 직전 지적 상태 추적

| 직전 # | 심각도 | 이번 상태 | 근거 |
|---|---|---|---|
| 1 | 🟠 Major | **해소** | 아래 "Major 해소 확인" 참조 |
| 2 | 🟡 Minor (앱 종료 시 plugins 자식 고아) | 유지(재평가 안 함) | 388e151에서 다루지 않았다 |
| 3 | 🟡 Minor (리더 join 상한 없음) | 유지(재평가 안 함) | 다루지 않았다. 락과 맞물려 영향이 커지는 부분은 N1로 따로 기록했다 |
| 4 | 🟡 Minor (get_daily_detail 패닉이 빈 결과로 보임) | 유지(재평가 안 함) | 다루지 않았다 |
| 5 | 🟡 Minor (날짜 스캔 중복) | 유지(재평가 안 함) | 다루지 않았다 |
| 6 | ⚪ Nit (프론트 테스트 없음) | 유지 | 다루지 않았다 |
| R1/R2 | 🔵 Rethink | 유지(차기 결정 항목) | R1의 "백엔드 직렬화" 절반은 이번 커밋으로 충족됐다. 나머지(대기열·취소)는 남아 있다. 아래 R3 참조 |

## Major 해소 확인

| 확인 항목 | 판정 | 근거(확인방법) |
|---|---|---|
| 5개 커맨드가 공통 경로를 탄다 | 참 | `plugins/mod.rs:436`(update_plugin), `:454`(refresh_marketplaces), `:464`(add_marketplace), `:471`(remove_marketplace), `:480`(install_plugin)이 모두 `run_blocking(...)`을 거친다. `src-tauri/src` 전체에서 `run_claude_command\|run_add_marketplace\|run_remove_marketplace`를 grep했다. 실제 호출은 mod.rs의 5개 커맨드 안과 marketplace.rs의 `run_add/remove_marketplace` 안(`:140,:157`)뿐이다. 이 두 함수는 `pub(crate)`이지만 호출부가 mod.rs `:464,:471` 외에는 없다. 나머지 히트는 다른 모듈의 주석이다 |
| 락을 blocking 풀 안에서 잡는다(메인 스레드·async 워커를 막지 않음) | 참 | `:417` `spawn_blocking(move \|\| with_op_lock(&PLUGIN_OP_LOCK, job))`. async 함수 본문에서는 락을 잡지 않는다 |
| 가드가 kill/wait까지 유지된다 | 참 | `with_op_lock`(`:392-407`)에서 `_guard`가 함수 끝까지 바인딩된 채로 `job()`을 동기 실행한다. `run_claude_command`가 타임아웃 kill→wait를 동기로 끝낸 뒤에 반환하므로(직전 보고서 근거 `process.rs:296`) 가드가 그 구간을 덮는다. `let _ = ...`처럼 즉시 drop되는 바인딩이 아니라는 것도 확인했다 |
| update_plugin 2단계 사이에 락이 유지된다 | 참 | `:436-449`에서 `marketplace update`와 `plugin update`가 같은 클로저(job 1개) 안에 있다. 두 단계 사이에 해제되지 않는다 |
| 겹친 요청은 즉시 실패한다 | 참 | `:399` `WouldBlock` → `success:false` + `PLUGIN_OP_BUSY_MESSAGE`. job을 실행하지 않는다. 테스트 `second_job_fails_fast_with_busy_while_first_holds_lock_then_succeeds_after_release`(`:838`)가 Busy일 때 job이 실행되지 않는 것(`ran == false`)과 해제 뒤 재획득되는 것을 확인한다 |
| poison 처리 | 참 | `:398` `Poisoned(p) => p.into_inner()`. 테스트 `lock_is_released_and_poison_ignored_after_job_panics`(`:870`)가 패닉 뒤 `is_poisoned()`인 상태에서도 다음 작업이 성공하는 것을 확인한다. 보호 데이터가 `()`라 poison을 무시해도 깨질 불변식이 없다 |
| 테스트 실행 | 통과 | `cd src-tauri && cargo test --lib plugins`: 39 passed, 0 failed, 2 ignored(직전 37 → 신규 2) |

## 새 결함 여부

| 확인 지점 | 판정 | 근거 |
|---|---|---|
| `catalog.ts` handleUpdateAll 순차 await | 결함 없음 | `src/views/catalog.ts:154-168`의 for 루프는 `await updatePlugin(p.id)`를 하나씩 기다린다. 앞 작업의 가드는 `spawn_blocking` 클로저가 반환할 때 해제되고, 그 뒤에야 프론트 Promise가 resolve된다. 그래서 다음 반복이 자기 자신 때문에 Busy를 받는 경우는 없다. 반복 사이의 빈틈에 설정 화면의 작업이 끼어들면 해당 플러그인만 Busy 실패로 `lastResult`에 기록되고(`:157`), 요약 토스트는 `k/N`으로 나온다(N2, Nit) |
| `catalog.ts` 단건 업데이트·기본 설치 | 결함 없음 | `:97`, `:125`는 `result.success=false`일 때 `result.message`를 토스트로 그대로 보여준다. Busy 문구가 사용자에게 그대로 전달된다 |
| `settings.ts` 호출부 4곳 | 결함 없음 | `:574` refresh는 실패해도 `loadMarketplaces/loadCatalog`(읽기 전용)만 다시 부른다. `:612` add는 실패 시 입력 초안을 그대로 둔다. `:645` remove는 실패 시 목록을 건드리지 않는다. `:1318` install은 실패 토스트만 띄운다. Busy를 받아서 파괴적인 후속 처리가 일어나는 분기는 없다 |
| 읽기 커맨드가 락에 막히는가 | 아니오(의도대로) | `list_installed_plugins`(`:57`)·`list_known_marketplaces`(`:64`)·`list_global_catalog`(`:72`)는 `run_blocking`을 거치지 않는다. 작업 중에도 읽기는 가능하다. 쓰기 도중 읽으면 중간 상태가 보일 수 있지만, 이는 수정 전과 같은 성질이고 결과가 다음 load에서 덮인다 |
| 교착 가능성 | 없음 | 이 락은 다른 락과 중첩되지 않는다(job 내부에서 EXECUTION_LOCK·LOGIN_STATE를 잡는 경로가 없다. `run_claude_command`가 EXECUTION_LOCK을 잡지 않는다는 점은 `process.rs:211` 주석과 grep으로 확인했다). 재진입도 없다 |

## EXECUTION_LOCK 비공유 결정 평가
**타당하다.** `dev_tools/process.rs:76` EXECUTION_LOCK은 `dev_tools/actions.rs:81,241`(도구 설치·업데이트)에서만 잡는다. 이 두 작업군은 서로 다른 외부 자원을 쓴다. plugins는 `~/.claude/plugins/*.json`과 마켓플레이스 git 클론을 쓰고, dev_tools는 brew/winget/npm 패키지 매니저 상태를 쓴다. 락을 공유하면 최대 8분짜리 플러그인 작업 때문에 관련 없는 도구 업데이트까지 거부된다. 주석(`plugins/mod.rs:373-380`)이 인정한 잔여 겹침은 하나다. dev_tools의 `claude update`(`plan_table.rs:222`)나 npm 재설치가 claude 바이너리를 교체하는 도중에 플러그인 작업이 claude를 띄우는 경우다. 이 경우 실패가 사용자에게 그대로 보이고, 재실행해도 안전하며, 터미널에서 같은 조합을 실행해도 똑같이 생긴다. 50인 미만 사내 도구의 비례 원칙으로 보면 받아들일 만한 잔여 위험이다. mcp_manager·autonomy가 띄우는 claude는 플러그인 상태 파일을 쓰지 않으므로 이 락의 대상이 아니다(모듈 주석을 Read해 확인했다. CLI 내부 동작은 미확인).

## 신규 지적 사항

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| N1 | 🟡 Minor | 프로세스 수명 × 락 | `plugins/mod.rs:396-406`(가드 수명 = job 수명), `dev_tools/process.rs:300-305`(상한 없는 `join()`) | 두 구간을 Read해 가드 해제 시점과 join 종료 조건을 대조했다 | 직전 #3이 실제로 발생하면(그룹을 벗어난 자손이 파이프를 물고 있음) job이 반환하지 않는다. 그러면 `PLUGIN_OP_LOCK`이 영구히 잡혀 있고, 이후 모든 플러그인 쓰기 작업이 앱을 재시작할 때까지 Busy가 된다. 직전에는 blocking 워커 하나가 묶이는 데서 끝났지만, 이제는 기능 전체가 멈춘다. 재현 경로는 확인하지 못했다(claude CLI가 그런 자손을 남기는지는 직전과 마찬가지로 미확인). 그래서 Major로 올리지 않는다 | 직전 #3의 처방(join에 `recv_timeout` 상한)이 이 영향도 함께 없앤다. #3을 백로그에서 처리할 때 이 결합을 근거로 우선순위를 한 단계 올리는 것을 권고한다 |
| N2 | ⚪ Nit | UX 문구 | `src/views/catalog.ts:172` | Read | "모두 업데이트" 도중 설정 화면 작업이 끼어들어 일부가 Busy로 실패해도, 요약 토스트는 `k/N개 … 완료`만 보여주고 실패 사유를 알려주지 않는다. 플러그인 카드별 `lastResult`에는 사유가 남는다 | 실패가 있으면 "일부는 다른 작업과 겹쳐 실패 — 다시 시도" 한 줄을 덧붙인다. 우선순위는 낮다 |
| N3 | ⚪ Nit | 검증 | `plugins/mod.rs:417`, 테스트 `:829-` | 테스트 본문 Read | 테스트는 로컬 락을 `with_op_lock`에 주입해 로직을 검증한다. `run_blocking`이 실제로 `PLUGIN_OP_LOCK`을 넘기는지는 테스트하지 않고, 한 줄짜리 배선이라 Read로만 확인된다. 실제 claude를 띄우지 않겠다는 테스트 방침상 합리적인 절충이다 | 필요하면 `run_blocking`의 락 인자를 주입 가능한 형태로 분리한다. 지금은 조치가 필요 없다 |

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 런타임/락 | handleUpdateAll이 앞 작업 가드가 해제되기 전에 다음 invoke를 보내 자기 자신 때문에 Busy를 받는다 | 기각 | 가드는 `spawn_blocking` 클로저 안에서 drop되고, 그 뒤에 JoinHandle이 완료되고 프론트 Promise가 resolve된다(`mod.rs:417-418`). 순차 await 구조(`catalog.ts:156`)에서는 순서가 뒤집힐 수 없다 |
| 런타임/락 | Busy 요청도 blocking 스레드를 점유하므로 연타하면 풀이 고갈된다 | 기각 | Busy 경로는 `try_lock` 한 번 뒤 즉시 반환한다(`:399-404`). 점유 시간이 마이크로초 단위이고, 프론트 버튼은 진행 중 플래그로 비활성화된다 |
| 프로세스 수명 | poison을 무시하면 반쯤 진행된 상태 파일 위에서 다음 작업이 돈다 | 기각 | 락이 보호하는 데이터는 `()`다. 상태 파일 일관성은 CLI가 책임지는 영역이고, 패닉은 Rust 쪽 조립 로직에서만 날 수 있다(자식은 이미 wait 완료이거나 아직 미기동). poison 때문에 영구 거부되는 쪽이 더 나쁘다 |
| 런타임/락 | 설정 화면 MCP 패널의 설치와 카탈로그 기본 설치가 `installingDefault`를 공유해서, 한쪽 finally가 다른 쪽 진행 표시를 끈다 | 범위 밖(기각) | `settings.ts:1314`, `catalog.ts:121`의 공유는 388e151 이전부터 있던 구조다. 이제 백엔드가 두 번째 요청을 Busy로 거절하므로, 실제 동시 설치는 일어나지 않는다 |

## 페르소나별 관점

### Tauri 런타임 스레드·락 규율 감사자(재사용): 🟢 Green
- 직전에 미충족이던 T3(사라진 직렬화의 대체)가 충족됐다. 대체 장치가 프론트 플래그가 아니라 백엔드에 있어서, 화면이 늘어나도 다시 뚫리지 않는다.
- 락 구간에 I/O가 있지만(자식 프로세스 대기) 이것이 락의 목적이고, blocking 풀 안에서만 일어난다. T2(메인 스레드·전역 상태 락 구간 I/O 0)의 취지에 어긋나지 않는다.
- T5(poison 처리) 충족. 다른 락과 중첩되지 않아 교착 경로도 없다.

### 프로세스 실행 보안 감사자(재사용): 🟢 Green
- 가드 수명이 자식 프로세스 수명과 같다. 타임아웃 kill→wait가 끝나기 전에 다음 작업이 같은 git 클론에 들어갈 수 없다.
- argv·셸·env 경로는 이번 diff에서 바뀌지 않았다.
- N1(직전 #3과 결합된 영구 Busy)은 Minor로 기록한다.

### 제로베이스 재설계자(발산형, 재사용): 🔵 (RAG 비반영)
- 아래 Rethink 1건.

## 구조적 제언 (Rethink): 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R3 | 백엔드는 직렬화하지만 프론트는 Busy를 받을 때까지 그 사실을 모른다. 카탈로그와 설정, 두 화면의 버튼은 각자의 플래그만 보고 활성화된다 | 백엔드가 `plugin_op_busy()` 조회(또는 시작·종료 이벤트 emit)를 노출하고, 두 화면이 하나의 `state.pluginOpRunning`으로 모든 플러그인 쓰기 버튼을 비활성화한다. 직전 R1(작업 큐 + 취소)의 가벼운 첫 단계다 | 사용자가 "누르면 실패하는 버튼"을 보지 않게 된다. Busy 응답은 프론트 상태가 어긋났을 때를 위한 최후 방어선으로 남는다. N2도 자연히 사라진다 | 낮음~중간. 백엔드는 락 상태 조회 1개, 프론트는 catalog·settings 플래그 4종을 묶는다. 근거: 두 화면의 플래그 위치(`catalog.ts:92,149`, `settings.ts:571,609,642,1314`)는 확인했고, 공수는 미확인 추정이다 |

## 트레이드오프
- 즉시 거절과 대기열: 직전 권고대로 즉시 거절을 택했다. 대기 중에 상한 시간이 늘어나지 않고, dev_tools와 같은 관용구다. 대신 "모두 업데이트" 도중 다른 화면 작업이 거절된다. 사내 도구 규모에서는 합당하고, R3로 UX를 보완할 수 있다.

## 잘 된 점
- 락 로직을 `with_op_lock(lock, job)`으로 분리하고 락을 주입받게 해서, 실제 claude 없이 Busy·재획득·poison 복구를 결정론적으로 실증했다. 채널 두 개로 "첫 작업이 락을 쥔 상태"를 정확히 만들고, `ran` 플래그로 "Busy면 job을 실행하지 않는다"까지 확인했다.
- 락 획득 위치(blocking 풀 안), 가드 수명(= 자식 프로세스 수명), poison 정책, EXECUTION_LOCK 비공유 근거와 잔여 위험을 모두 주석에 현재형으로 남겼다.
- 수정 범위가 파일 1개, 실질 코드 약 20줄로 최소다. 직전 권고(축소 확인으로 충분한 수 줄 수정)와 일치한다.

## 평가기준 충족 현황(이번 증분)

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| T3 사라진 직렬화 대체 | 런타임 | 필수 | 충족 | 직전 미충족 → 해소 |
| T2 메인 스레드 비차단 | 런타임 | 필수 | 충족 | try_lock이 blocking 풀 안에 있다 |
| T5 poison 처리 | 런타임 | 필수 | 충족 | 테스트 있음 |
| S5 가드가 kill/wait를 덮음 | 프로세스 수명 | 필수 | 충족 | |
| 회귀 테스트 동반 | 검증 | 필수 | 충족 | 2개, 배선은 Read 확인(N3) |

## 검증 수행 내역 / 생략
- 실행함: `cd src-tauri && cargo test --lib plugins`(39 passed, 2 ignored), `git show 388e151`, `git diff --name-only 27f0d3b..388e151`(6개 파일: claude_auth.rs, plugins/marketplace.rs, plugins/mod.rs, usage_stats/mod.rs, latestGate.ts, views/usage.ts), 백엔드 호출부·EXECUTION_LOCK grep, 프론트 호출부 7곳 Read.
- 실행하지 않음: 실제 claude 플러그인 작업(금지). 화면 실기동·캡처(UI 변경 없음). `cargo test --lib` 전체와 `tsc`는 다시 돌리지 않았다. 388e151은 plugins/mod.rs만 바꾸고 프론트는 건드리지 않았으므로, 직전 라운드 결과(685 통과, tsc 통과)를 그대로 쓴다. Windows 빌드(알려진 한계).
- 생략한 관점: async-response-ordering(프론트 응답 순서 변경 없음).
- 실행 액션: 없음. 코드 수정·커밋·push를 하지 않았다. 이 보고서, 페르소나 3개의 적용 이력 append, INDEX.md 갱신은 작업 트리에 미커밋 상태로 남겨 두었다.

## PM에게 권고
1. **통과(Green)**: 커밋 범위 `27f0d3b..388e151`에 남은 Critical·Major는 없다. 직전 #1은 해소됐다.
2. 직전 Minor #2~#5는 백로그로 둔다. 그중 #3(join 상한)은 N1 때문에 영향이 "기능 전체 영구 Busy"로 커졌으므로, 백로그 안에서 우선순위를 한 단계 올리는 것을 권고한다.
3. R1·R3(프론트 공유 busy 상태 → 작업 큐·취소)는 차기 방향 결정 항목이다.
