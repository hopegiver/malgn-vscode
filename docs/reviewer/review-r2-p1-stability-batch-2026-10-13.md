# P1 안정화 배치(4커밋) 리뷰 보고서

리뷰 대상: target_id `r2-p1-stability-batch` / 커밋 범위 `27f0d3b..b020829` (main — dcb34f2, 81553f3, c4acf02, b020829)
리뷰 페르소나 패널: `persona-process-execution-security.md`, `persona-tauri-runtime-thread-and-lock-discipline.md`, `persona-async-response-ordering.md`, `persona-zero-base-redesigner.md`(발산형) — 4개 파일 실재 (`docs/reviewer/personas/`)
리스크 범주: 자식 프로세스 수명(로컬 claude CLI 실행) / 인증(claude 로그인 stdin)
등급: Standard(트리거: 자식 프로세스 타임아웃·kill, 인증 경로 락 동시성), 최초 리뷰 → 약식 패널 4인
리뷰 일자: 2026-10-13
종합 판정: 🟡 Amber

## 요약 (2분 규칙)
네 커밋의 완료조건은 코드와 테스트 기준으로 모두 참이다(`cargo test --lib` 685 통과, `tsc --noEmit` 통과). 다만 plugins 커맨드를 sync에서 async로 옮기면서, 그동안 메인 스레드가 암묵적으로 해 주던 직렬화가 사라졌다. 그래서 이제 서로 다른 `claude plugin …` 작업이 동시에 돌 수 있고, 대신 직렬화해 주는 장치는 없다(🟠 1건). 수정 비용은 낮다. 백엔드 정적 Mutex 하나면 된다.

## 페르소나 재사용 판정 (산출물 게이트)
INDEX.md(`docs/reviewer/personas/INDEX.md`)의 역할개념 열을 착수 전에 대조했다. 이 target_id는 최초 리뷰라 직전 라운드가 없다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| persona-process-execution-security.md | 재사용 | 역할개념 "argv·셸·env·**프로세스 수명**이 안전한가"가 요청된 "동시성/프로세스 수명 전문가" 관점과 일치(S5 기준) |
| persona-zero-base-redesigner.md | 재사용 | 발산형 정본. 역할개념 동일 |
| persona-tauri-runtime-thread-and-lock-discipline.md | 신규 | INDEX 15행 중 메인 스레드 블로킹, 전역 락 구간 I/O, async 전환 뒤 사라진 직렬화를 묻는 행이 없다. process-execution-security는 자식 프로세스 자체를 보고, 스레드·락 규율은 보지 않는다 |
| persona-async-response-ordering.md | 신규 | 가장 가까운 행은 background-event-vs-foreground-work(앱이 일으킨 갱신이 사용자 작업을 파괴하는가)와 screen-state-honesty(로딩/빈/에러 구분)다. 둘 다 "늦게 도착한 응답이 최신 상태를 덮어쓰는가"라는 순서 문제를 다루지 않는다 |

## 완료조건 판정

| 커밋 | 완료조건 | 판정 | 근거 |
|---|---|---|---|
| c4acf02 P1-1 | 메인 스레드 비차단(async+spawn_blocking) | 참 | `plugins/mod.rs:388,406,416,423,432` 5개 커맨드가 모두 `pub async fn`이고 `run_blocking`(`mod.rs:366`)을 거친다 |
| | 모든 실행에 상한 | 참(조건부) | `run_claude_command(args, timeout)`의 timeout이 필수 인자(`mod.rs:236`)이고, plugins 안의 호출 5곳이 모두 `TIMEOUT_*`를 넘긴다(grep `run_claude_command(`). wait에는 상한이 있지만, 리더 스레드 join에는 상한이 없다(아래 #3) |
| | 초과 시 자식·그룹 종료·회수 | 참(unix) | `dev_tools/process.rs:251` `process_group(0)`, `:296` 그룹 kill→wait. 테스트 `hanging_child_is_killed_and_reaped_with_timeout_result`가 손자까지 회수되는 것을 확인하고 통과(실행함). Windows는 알려진 한계 |
| | 시간초과 의미 노출 | 참 | `timeout_message`가 "시간 초과…{n}초…종료했습니다" 문장과 직전 출력 tail을 붙이고, `settings.ts:575`·`catalog.ts:102` 토스트가 message를 그대로 보여준다 |
| | stdout/stderr 동시 소비 | 참 | `process.rs:278-291` 리더 스레드 2개. `large_stdout_and_stderr_do_not_deadlock`(2MB×2)을 실행해 통과 |
| | 테스트 존재 | 참 | `run_claude_binary_tests` 10개(unix 9 실행, windows 1 미컴파일) |
| b020829 P1-7 | write_all이 락 밖 | 참 | `claude_auth.rs:255-275` `take_login_stdin`은 락 안에서 take만 하고, write는 `InFlightStdin::write_line`이 락 없이 한다 |
| | 4경로 EOF·고아 없음 | 참 | 취소 `:776`의 stdin_closed 처리와 Drop(`:213-231`)이 재보관을 거부한다. 타임아웃 `:681`, 정상완료 `finish_login :280`, 앱종료 `request_shutdown :288`은 그룹 kill로 write가 EPIPE를 받고 해제된다. 테스트 8종 실행해 통과 |
| | 제출 중 취소·로그인 교체·동시 제출·패닉 | 참 | 각각 테스트가 있다: `cancel_during_submit_wins…`, `returned_handle_is_not_stored_into_a_newer_login`(generation), `Busy` 검사(`:265`), `in_flight_flag_is_released_even_if_submit_panics`(poison 무시 `:220-223`) |
| dcb34f2 P1-3 | get_daily_detail async+spawn_blocking | 참 | `usage_stats/mod.rs:33-37` |
| | 마지막 선택 날짜만 반영 | 참 | `usage.ts:251-266`에서 성공·실패 두 분기 모두 `isLatest`를 검사하고, 오래된 응답은 loading을 내리지 않는다. 접기 `:274`는 invalidate한다. 재시도 버튼 `:339`은 `loadDailyDetail`이 begin을 다시 발급한다 |
| 81553f3 P1-6 | 단일 정의, 분모 0 → null | 참 | `usage.ts:91-94` `computeCacheHitRate`. 홈(`home.ts:113,292`)과 사용량(`usage.ts:416`)이 모두 `computeUsageTotals`(`usage.ts:128`)를 거친다. `src/` 전체에서 `cacheHitRate\|cacheReadTokens`를 grep한 결과 다른 계산 지점은 없다 |
| 범위 밖 변경 | capabilities·tauri.conf.json·STATUS.md 미포함 | 참 | `git diff --name-only 27f0d3b..b020829`는 6개 파일(claude_auth.rs, plugins/marketplace.rs, plugins/mod.rs, usage_stats/mod.rs, latestGate.ts, views/usage.ts)뿐이다 |

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| 1 | 🟠 Major | 런타임/락 | `src-tauri/src/plugins/mod.rs:388-440` (5개 커맨드 async 전환), 프론트 `src/views/catalog.ts:92,327`(플러그인별 플래그), `catalog.ts:125` vs `settings.ts:1318`(같은 `installPlugin(DEFAULT_PLUGIN_ID)`), `settings.ts:574/612/645` | 커맨드 diff Read. 프론트 `await updatePlugin\|installPlugin\|refreshMarketplaces\|addMarketplace\|removeMarketplace` 전수 grep. 백엔드 `static .*Mutex` grep 결과 plugins에는 락이 없다(`EXECUTION_LOCK`은 dev_tools 전용) | Tauri v2의 sync 커맨드는 메인 스레드에서 돌아서, 그동안 서로 직렬화돼 있었다(UI가 얼었던 원인이자 부수효과). async로 바꾸면서 직렬화가 사라졌고, 대체 장치는 버튼별 프론트 플래그뿐이다. **재현 경로**: 같은 마켓플레이스(`malgnsoft-plugins`)의 플러그인 A와 B에서 "업데이트"를 연달아 누른다. 이 버튼은 `updating[plugin.id]` 플래그만 보므로 둘 다 활성 상태다. 그러면 `claude plugin marketplace update malgnsoft-plugins`가 2개 동시에 실행되고, 같은 git 클론 디렉터리에 동시에 fetch/pull한다. git `index.lock`/ref lock 충돌로 한쪽이 실패할 수 있다. 화면 간 조합(카탈로그 "모두 업데이트" 중 설정에서 "새로고침"·"추가"·"제거", 또는 카탈로그와 설정 MCP 패널의 malgn-agent 설치 동시 클릭)도 같다. CLI의 `installed_plugins.json`·`known_marketplaces.json` read-modify-write가 겹치면 항목이 유실될 수 있다(CLI 내부 잠금 여부는 미확인). 이번 변경으로 UI가 최대 8분(SYNC 180 + INSTALL 300초) 동안 반응하게 되면서, 겹치는 조작이 실제로 가능해졌다. **실행 미검증**(실제 claude 실행은 금지됨) | `plugins/mod.rs`에 `static PLUGIN_OP_LOCK: Mutex<()>`를 두고 `run_blocking` 클로저 안에서 잡는다. 대기 시 실패 반환이 좋은지 대기열이 좋은지는 트레이드오프 참조. `dev_tools`의 `EXECUTION_LOCK.try_lock()` 관용구(`dev_tools/actions.rs:81`)를 그대로 쓰면 수 줄로 끝난다 |
| 2 | 🟡 Minor | 프로세스 수명 | `src-tauri/src/lib.rs:213-217` (ExitRequested), `plugins/mod.rs:18-30` 주석 "고아 없음" | lib.rs 종료 핸들러 Read. plugins에 pid 레지스트리·shutdown 함수가 있는지 grep → 없음 | 앱을 종료하는 순간 진행 중이던 `claude plugin install`은 새 프로세스 그룹(`process.rs:251`)에 있어 그대로 남는다. session_chat·claude_auth·autonomy와 달리 plugins는 종료 정리 대상이 아니다. 이번 변경 전에도 GUI 부모가 죽으면 자식이 살아남았으므로 회귀는 아니다. 다만 상단 주석의 "고아 없음"은 타임아웃 경로에만 해당하는 말이다 | `on_spawn`으로 pid를 기록하고 `plugins::request_shutdown()`을 lib.rs:215 옆에 추가한다. 또는 주석 범위를 "타임아웃 경로에 한해"로 좁힌다 |
| 3 | 🟡 Minor | 프로세스 수명 | `src-tauri/src/dev_tools/process.rs:300-305` | 해당 함수 Read | wait에는 상한이 있지만, 그 뒤 리더 스레드 `join()`에는 상한이 없다. setsid 등으로 프로세스 그룹을 벗어나 stdout/stderr를 물고 있는 자손이 있으면, 정상 종료든 kill 후든 `read_to_end`가 EOF를 받지 못해 `spawn_blocking` 워커가 영구히 묶인다. 재현 경로는 확인하지 못했다(claude CLI가 그런 자손을 남기는지 미확인). 기존 헬퍼의 성질이며, P1-1 "모든 실행에 상한"이 이 헬퍼에 기대고 있어서 기록한다 | join 대기에 상한을 둔다. 채널 + `recv_timeout`(예: 2초)을 쓰고 넘으면 부분 출력으로 반환한다 |
| 4 | 🟡 Minor | 응답 순서/정직성 | `src-tauri/src/usage_stats/mod.rs:36` | Read | `spawn_blocking`이 패닉하면 `unwrap_or_default()`로 빈 보고서를 돌려준다. 프론트는 이것을 "그날 사용 기록 없음"으로 보여주므로 실패와 구분되지 않는다. 형제 커맨드 `get_daily_usage`와 같은 관용구이긴 하다 | `Result<_, String>`으로 바꿔 프론트 error 분기(`usage.ts:262`)로 보낸다 |
| 5 | 🟡 Minor | 응답 순서 | `src/views/usage.ts:251-266`, `usage_stats/mod.rs:34` | 코드 추적 | 게이트는 응답을 버릴 뿐 백엔드 스캔을 취소하지 않는다. 날짜 N개를 빠르게 누르면 jsonl 전체 재귀 스캔 N개가 blocking 풀에서 동시에 돈다(CPU·IO 낭비, 결과는 정확함) | 날짜별 결과 캐시(같은 날짜 재클릭 시 재스캔 생략)나 in-flight 1개 + 최신 날짜만 큐잉. 우선순위는 낮다 |
| 6 | ⚪ Nit | 검증 | `src/latestGate.ts`, `src/views/usage.ts:91` | `package.json`의 test 스크립트와 `*.test.ts` 탐색 → 프론트 테스트 러너 없음 | 두 순수 함수에 단위 테스트가 없다. 프로젝트에 TS 테스트 인프라가 없으므로 결함이 아니라 참고 사항이다 | 프론트 러너를 도입할 때 이 둘을 첫 대상으로 삼는다 |

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 런타임/락 | `cancel_claude_auth_login`이 sync라 메인 스레드에서 3초 유예 sleep을 한다 | 기각 | `process_util.rs:85-97`을 Read해 보니 유예 sleep은 별도 스레드에서 한다. 메인 스레드는 SIGTERM만 보내고 즉시 반환한다 |
| 프로세스 수명 | 앱 종료 시 제출 중인 핸들이 Drop에서 슬롯으로 되돌아가 stdin이 열린 채 남는다 | 기각 | `request_shutdown`(`claude_auth.rs:288`)이 그룹 kill을 하므로 자식이 죽어 EOF가 의미 없다. 이후 `run_login`의 `finish_login`(`:720`)이 슬롯째 drop한다. 고아는 생기지 않는다 |
| 프로세스 수명 | EPIPE 대신 SIGPIPE로 앱이 죽는다 | 기각 | Rust 런타임은 바이너리 시작 시 SIGPIPE를 SIG_IGN으로 설정한다. 테스트 `blocked_stdin_write…`가 write 실패(Err)를 실제로 관측하고 통과했다 |
| 런타임/락 | `run_claude_binary`가 cwd=None일 때 Windows `%SystemRoot%`로 바뀌어 기존 동작과 달라진다 | 강등→기각 | `process.rs:236-246`은 Windows 전용 보정이고, `.cmd` shim 해석 하이재킹 방지라는 의도된 개선이다. 회귀로 볼 근거가 없다 |
| 응답 순서 | 날짜 전환 시 이전 날짜 report가 잠깐 남아 보인다 | 기각 | `toggleDailyDetail`(`usage.ts:281-283`)이 전환 즉시 `report=null`로 비우고, 렌더 `:333`이 로딩 블록을 보여준다 |

## 페르소나별 관점

### 프로세스 실행 보안 감사자(재사용) — 판정: 🟢 Green
- S5 4종(stdin=null, 리더 스레드, try_wait 타임아웃, `kill(-pgid)`)이 plugins 전 경로에 닿는다. 이전에는 `.output()`(상한 없음, 그룹 없음)이었다. 손자 회수까지 테스트로 증명했다.
- argv 조립은 바뀌지 않았다(문자열 슬라이스를 그대로 넘긴다). 셸 경유는 새로 생기지 않았다.
- 로그인 stdin 4경로 EOF 불변식은 `stdin_closed` 표식과 generation으로 보강됐다. 핸들이 슬롯 밖에 있는 동안 취소가 "닫기"를 잃는 창이 닫혔다.
- #2·#3(앱 종료 시 고아, join 상한)은 Minor다.

### Tauri 런타임 스레드·락 규율 감사자(신규) — 판정: 🟠 Amber
- T1(블로킹 커맨드의 async화)과 T2(락 구간 I/O 0)는 충족한다. claude_auth의 락 구간은 take/반납/플래그 조작뿐이다(`:255-275`, `:213-231`).
- T3 미충족 → #1. 메인 스레드가 해 주던 직렬화를 백엔드 락으로 대체하지 않았다.
- T4: plugins `run_blocking`은 JoinError를 "작업 스레드가 비정상 종료" 실패로 매핑해 좋다. 반면 `get_daily_detail`은 빈 값으로 매핑한다(#4).
- T5: Drop이 poison을 `into_inner`로 처리해 이중 패닉을 피한다.

### 비동기 응답 순서 감시자(신규) — 판정: 🟢 Green
- A1~A3 충족: 성공·실패 두 분기에 모두 게이트가 있고, 오래된 응답은 loading을 건드리지 않으며, 접기는 무효화한다. B→A 순서(나중 요청이 먼저 도착)도 A의 응답이 버려지는 것을 손으로 추적해 확인했다.
- A4 충족: 캐시 히트율 계산 지점은 `computeCacheHitRate` 1곳이다.
- A5 미충족(권장) → #5.

### 제로베이스 재설계자(발산형, 재사용) — 판정: 🔵 (RAG 비반영)
- 아래 Rethink 2건.

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R1 | 플러그인 작업 5종을 각각 독립 커맨드로 두고, 화면마다 버튼별 bool 플래그로 중복을 막는다. 사용자는 최대 8분을 기다리며 취소할 수 없다 | 백엔드에 플러그인 작업 큐(동시성 1)를 하나 둔다. 프론트는 `state.pluginJob`(현재 작업·대기열) 하나를 모든 화면이 공유하고, "취소" 버튼으로 `should_abort` 플래그를 세운다 | #1을 구조적으로 없애고(화면이 늘어도 다시 뚫리지 않는다), 긴 상한의 비용("하염없이 기다림")을 사용자 취소로 상쇄한다. `run_process_with_timeout_cancellable`이 이미 `should_abort` 인자를 받으므로(`process.rs:221`) 백엔드 쪽 기반은 있다 | 중간. 백엔드는 정적 상태 + 취소 커맨드 1개, 프론트는 catalog·settings의 플래그 4종을 통합. 근거: should_abort 배관 존재를 확인함. 프론트 공수는 미확인 추정 |
| R2 | 날짜를 클릭할 때마다 해당 날짜의 jsonl 전체를 재스캔하고, 프론트 게이트로 늦은 응답을 버린다 | 날짜별 상세를 백엔드 LRU(최근 N일)나 프론트 `Map<date, report>`에 캐시하고, 오늘 날짜만 재조회한다 | 과거 날짜는 불변 데이터라 재스캔할 이유가 없다. 게이트가 필요해진 근본 원인(느린 응답)이 사라지고 #5도 해소된다 | 낮음~중간. 무효화 규칙은 "오늘만 재조회" 하나다. 미확인 추정치 |

## 트레이드오프 (페르소나 간 충돌)
- #1의 처리 방식: 즉시 거절(`try_lock`, "다른 플러그인 작업이 진행 중입니다")과 대기열(`lock`, 순서대로 실행)이 갈린다. 런타임 감사자는 즉시 거절을 선호한다. 결정적이고 dev_tools와 같은 관용구이며, 대기 중 상한 시간이 늘어나는 문제가 없다. 발산형은 "모두 업데이트" 같은 연속 작업에 대기열이 자연스럽다고 본다. **권고**: 이번에는 `try_lock` 즉시 거절로 막고(수 줄), 대기열·취소는 R1로 차기에 판단한다. 50인 미만 사내 도구의 비례 원칙과도 맞다.

## 잘 된 점
- 상한 상수 3개를 단일 정본으로 두고, 메시지의 초 값을 인자에서 생성해 숫자가 어긋날 수 없게 했다. 값이 판단값이라는 사실과 그 근거를 주석에 정직하게 남겼다.
- `run_claude_binary`를 분리해 실제 claude 없이 `/bin/sh`로 타임아웃·손자 회수·2MB 양방향 교착을 실증했다. "주장"이 아니라 "관측"으로 검증한 사례다.
- `InFlightStdin` Drop 반납 + generation + `stdin_closed`로 락 밖 I/O의 대가(반납 경합)를 빠짐없이 다뤘다. 경합마다 결정론적 테스트가 1:1로 붙어 있다. 테스트 전역 슬롯 직렬화 락(`LOGIN_STATE_TEST_LOCK`)도 기존 테스트 간 간섭을 없앴다.
- `latestGate`는 21줄짜리 범용 유틸로, 다른 화면에 재사용할 수 있다. 실패 분기와 접기 무효화까지 챙겼다.
- 커밋 범위가 6개 파일로 깔끔하다. 권한 표면(capabilities)은 건드리지 않았다.

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| S5 수명 4종 | 프로세스 수명 | 필수 | 충족 | unix 실행 검증, Windows 미컴파일(알려진 한계) |
| T1 블로킹 커맨드 async | 런타임 | 필수 | 충족 | 6개 커맨드 |
| T2 락 구간 I/O 0 | 런타임 | 필수 | 충족 | claude_auth |
| T3 사라진 직렬화 대체 | 런타임 | 필수 | 미충족 | #1 |
| T4 JoinError 구분 | 런타임 | 권장 | 부분 | #4 |
| A1~A3 최신성 | 응답 순서 | 필수 | 충족 | |
| A4 단일 정의 | 응답 순서 | 필수 | 충족 | |
| A5 중복 작업 제한 | 응답 순서 | 권장 | 미충족 | #5 |

## 검증 수행 내역 / 생략
- 실행함: `cargo test --lib plugins`(37 통과, 2 ignored), `cargo test --lib claude_auth`(17 통과), `cargo test --lib` 전체(685 통과, 21 ignored), `pnpm exec tsc --noEmit`(통과).
- 실행하지 않음: 실제 claude 로그인·플러그인 설치(금지), 화면 실기동(UI 변경은 상태 로직뿐이라 코드 추적으로 판정. 화면 캡처 없음), Windows 빌드(알려진 한계).
- 실행 액션: 없음. 코드 수정·커밋·push를 하지 않았다. 이 보고서와 페르소나 파일 4개(신규 2, 이력 append 2), INDEX.md 갱신은 작업 트리에 미커밋 상태로 남겨 두었다.

## PM에게 권고
1. **#1(🟠) 수정 후 통과 권장**: `plugins/mod.rs`에 `PLUGIN_OP_LOCK` + `try_lock` 즉시 거절을 넣고, 회귀 테스트(락을 쥔 상태에서 두 번째 호출이 즉시 실패)를 추가한다. 이 건은 별도 재리뷰 없이 diff 확인(축소 모드)으로 충분하다.
2. #2~#5 Minor 4건은 백로그로 둔다. #3(join 상한)은 dev_tools와 autonomy도 공유하는 헬퍼라 고치면 효과가 넓다.
3. R1(작업 큐 + 취소)은 차기 방향 결정 항목이다. 채택하면 #1의 근본 해결을 겸한다.
