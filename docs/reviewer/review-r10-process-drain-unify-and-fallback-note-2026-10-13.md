# R10 리뷰: 파이프 수거 상한 일원화 + 설치 폴백 노트 표시

리뷰 대상: 커밋 범위 `61d8646..2abc9f6` (8fc6c98, 2abc9f6) / target_id `r10-process-drain-unify-and-fallback-note`
- 등급: Refactor, 최초 리뷰, 풀패널(노출 범위 축소 미적용 — 위임 메시지에 축소 주장 없음)
- 리스크 범주: 자율작업·세션 채팅·로그인(claude_auth) 경로의 자식 프로세스 종료/출력 대기, 공유 프로세스 유틸 구조 변경, 플러그인 설치 결과 UI
- 일자: 2026-10-13 / 리뷰어: reviewer

## 종합 판정: 🟢 Green (조건 1개)

Critical 0 / Major 0 / Minor 3 / Nit 4 / Rethink 2

조건: **이 두 커밋은 아직 push 전이다**(`origin/main` = 61d8646). Windows `#[cfg(windows)]` 변경(autonomy/runtime.rs:254-270)은 Mac에서 컴파일되지 않아 정적 검토만 했다. push 뒤 `ci` 워크플로의 windows-latest rust-test가 통과해야 Green이 확정된다(RV-003).

hub 이슈 01m4snx3fy1gmgdxatns41gxmq의 (1)(2)는 **이슈에 적힌 지점 기준으로는 해소**됐다. 다만 (1)의 목적인 "턴/로그인 스레드가 반드시 끝난다"는 stdout 읽기 루프가 상한 밖에 남아 있어 완전히 보장되지 않는다(RV-001). 이슈를 닫는다면 이 잔여분을 새 항목으로 옮겨야 한다.

## 페르소나 재사용 판정

착수 전 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-process-execution-security.md` | 재사용 | 자식 출력 수거 상한·Windows taskkill 위임이 역할개념("프로세스 수명이 안전한가")에 그대로 들어간다. Windows 분기 정적 검토도 여기서 맡았다 |
| `persona-tauri-runtime-thread-and-lock-discipline.md` | 재사용 | wait/done/drain 순서 변경과 앱 종료 경로의 스레드 병렬화가 역할개념 그대로다 |
| `persona-claimed-vs-verified.md` | 재사용 | 커밋·주석의 "남은 무상한 대기 일원화" 주장과 실제 보장 범위의 대조, 테스트 실효성 판단이 역할개념 그대로다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수이고 역할개념이 여전히 유효하다 |

`persona-heterogeneous-machine-operator`는 따로 부르지 않았다. 이번 Windows 관점은 실기 동작이 아니라 컴파일 정합성과 종료 지연이라서 process-execution-security가 함께 맡았다.

## 직접 확인한 것

- `git diff 61d8646..2abc9f6` 전체(12파일)와 변경 주변 원문: turn.rs:317-468, claude_auth.rs:579-760, mcp_manager/process.rs 전체, process_util.rs:1-220, autonomy/runtime.rs:200-270·484-590, autonomy/mod.rs:430-445, plugins/mod.rs:490-616, errorMessage.ts:132-193, dom.ts:438-455
- `cargo test --lib -- mcp_manager::process process_util dev_tools::process claude_auth session_chat autonomy` 실행: **205 passed / 0 failed / 4 ignored** (3.68s). PM이 보고한 730건 전체 실행은 다시 하지 않았다
- `node --experimental-strip-types`로 `runFallbackNoteChecks`를 단독 실행: **26/26 PASS**
- UI 하네스 `catalog`의 `install-fallback-success`, `install-plain-success`, `fallback-note-boundary` 시나리오를 재실행: 모두 bugs=0, consoleErrors=0. 스크린샷 2장을 직접 열어 확인했다
  - `docs/screenshots/r10-fallback-note/catalog/install-fallback-success-01-01-install-fallback-success.png`
  - `docs/screenshots/r10-fallback-note/catalog/install-plain-success-01-01-install-plain-success.png`
- hub 이슈 01m4snx3fy1gmgdxatns41gxmq 원문의 범위: "(1) 무상한 join … mcp_manager/process.rs:103, session_chat/turn.rs:437, claude_auth.rs:714 (2) autonomy Windows taskkill 상한 없음"
- `origin/main`과 `gh run list`: 2abc9f6은 push되지 않았고 CI도 아직 돌지 않았다

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 |
|---|---|---|---|---|---|
| RV-001 | 🟡 Minor (Major에서 강등) | `src-tauri/src/session_chat/turn.rs:389`, `src-tauri/src/claude_auth.rs:691`. 주석은 turn.rs:432-435, claude_auth.rs:710-714 | stderr 수거에는 상한이 생겼지만, 그보다 **앞에서 도는 stdout 읽기 루프**(turn은 `reader.lines()`, auth는 `pump_stdout`)는 여전히 EOF까지 상한 없이 막힌다. 자손이 부모의 표준 입출력을 그대로 물려받는 흔한 경우 stdout과 stderr를 **둘 다** 쥐므로, 실행은 stdout 루프에서 먼저 멈추고 새 상한 코드까지 오지 못한다. 상한이 실제로 효과를 내는 것은 자손이 stderr만 쥔 드문 경우뿐이다. 그런데 주석은 고착 증상이 해소됐다고 서술한다. claude_auth 주석은 "브라우저 실행 헬퍼가 stderr를 물면"을 예로 드는데, 이 헬퍼는 stdout도 함께 물 가능성이 높다. 재현 경로: `/bin/sh -c 'sleep 60 & echo {}; exit 0'`처럼 자손을 남기는 실행 파일을 claude 경로로 주면 turn.rs:389 루프가 60초 동안 반환하지 않는다 | (a) hub 이슈 (1)을 닫을 때 "turn/auth stdout 루프 상한"을 잔여 항목으로 옮긴다. (b) 두 주석을 "자손이 stderr만 쥔 경우에 한함 — stdout을 쥐면 아래 루프에서 막힘(유닉스는 취소·LOGIN_MAX_WAIT 그룹 kill로 풀림)"으로 고친다. 근본 해결은 RT-2 | 코드 정독. turn/auth 실기 재현은 AppHandle이 필요해 하지 않았다 |
| RV-002 | 🟡 Minor | turn.rs:436-443, claude_auth.rs:715-723, `src/views/settings.ts:1376-1377` | 테스트 공백 세 곳. (1) turn·auth의 순서 변경(wait → done → drain)에 회귀 테스트가 없다. 신규 테스트 2건은 mcp_manager에만 있다. (2) 설정 MCP 화면의 지속 경고 노트에 하네스 시나리오가 없다(`settingsMcp` 흐름은 바뀌지 않았다). (3) 경계값 검사 `fallback-note-boundary`는 카탈로그 흐름 안에서만 돈다 | turn/auth의 "spawn부터 수거까지"를 AppHandle 없는 함수로 떼어 내면 mcp의 `run_bounded`처럼 테스트할 수 있다(RT-2와 같은 작업). settingsMcp에는 폴백 성공 시나리오 1건을 추가한다 | 신규·변경 테스트 목록 대조. settings 경로는 스크린샷을 찍지 않았다 |
| RV-003 | 🟡 Minor (검증 미완) | `src-tauri/src/autonomy/runtime.rs:254-270` | Windows 분기가 Mac에서 컴파일되지 않아 CI가 유일한 검증 수단인데, 2abc9f6이 아직 push되지 않아 CI가 돌지 않았다. 정적 검토에서는 문제를 찾지 못했다: `child_pid: Option<u32>`(runtime.rs:26)와 `kill_process_group_with_grace(pid: u32)`(process_util.rs:112)의 타입이 맞는다. 지운 `use SilentCommand`는 더 쓰이지 않는다. dev_tools/process.rs에서 지운 `Mutex`·`Read`도 Windows 분기에서 참조하지 않는다(grep 0건). 테스트 보조 함수 import는 `#[cfg(unix)]` 아래에 있다(dev_tools/process.rs:701-702) | push 뒤 `ci` windows-latest의 `force_kill_all_remaining_terminates_registered_running_child_on_windows`(runtime.rs:540) 통과를 확인하고 나서 이슈 (2)를 닫는다 | grep, 타입 대조, `git rev-parse origin/main`, `gh run list` |
| N-1 | ⚪ Nit | `src-tauri/src/mcp_manager/process.rs:126-129` | `run_bounded`는 `timeout`을 인자로 받는데, 타임아웃 에러 문구는 상수 `MCP_COMMAND_TIMEOUT_SECS`를 쓴다. 테스트(400ms)에서는 "15초 내에…"가 나온다. 프로덕션 값은 같아서 동작 영향은 없다 | 문구에 `timeout.as_secs()`를 쓰거나 이 대로 둔다고 주석에 남긴다 | 코드 정독 |
| N-2 | ⚪ Nit | `src-tauri/src/autonomy/mod.rs:434` | `request_shutdown_and_wait` 주석이 여전히 "unix 한정 보루"라고 적는다. Windows 보루도 실재하고, 이번 라운드에서 그 Windows 보루를 고쳤다 | "unix SIGKILL / Windows taskkill(상한 5초, pid별 병렬)"로 갱신한다 | 코드 정독 |
| N-3 | ⚪ Nit | `src/views/catalog.ts:381-386`, 225-226 | 지속 경고 노트는 닫을 수 없고, 같은 세션에서 다음 설치를 할 때까지 남는다. `-y`만 뺀 폴백(설치 범위는 user 유지)에서도 "확인이 필요합니다"로 노란 경고가 뜬다. 스크린샷에서는 토스트와 노트가 같은 긴 문단을 겹쳐 보여준다 | 범위 경고가 없을 때는 중립 톤을 쓰거나, 닫기 버튼을 둔다. 45인 사내 도구라 지금 그대로도 수용할 만하다 | 스크린샷 install-fallback-success, 코드 정독 |
| N-4 | ⚪ Nit (트레이드오프 기록) | `src-tauri/src/process_util.rs:501-507` | 상한을 넘겨 리더를 포기한 뒤 자손이 다시 쓰면 SIGPIPE를 받는다는 의도된 동작이, 이제 turn·auth 경로의 자손에게도 적용된다. 예전에는 리더가 영구히 블록돼 있었으므로 자손이 쓰기 실패를 겪지 않았다 | 조치는 필요 없다. 이 동작이 3경로 공통이 됐다는 점만 주석 첫머리에 한 줄 남긴다 | 코드 정독 |

범위 밖 관찰(이번 변경과 무관한 기존 결함, 판정에 반영하지 않음): `autonomy/runtime.rs:441-445` 테스트는 가짜 `child_pid: Some(4242), running: true`를 전역 RUNTIME에 넣는다. 병렬로 도는 `force_kill_all_remaining_does_not_panic_when_called`(runtime.rs:492)가 그 항목을 집으면, 유닉스에서는 프로세스 그룹 4242에 SIGKILL을 보낸다. Windows에서는 pid 4242에 `taskkill /T /F`를 보낸다. 후속 백로그에 올리기를 권한다.

## 페르소나별 관점

### process-execution-security (수렴)
- 정상 경로의 동작은 그대로다. mcp는 성공 시 stdout을 쓰고, 실패면서 stderr가 비어 있지 않으면 stderr를 쓰는 선택 규칙(mcp_manager/process.rs:131-136)이 바뀌지 않았다. turn의 에러 판정(canceled → result_error → 비정상 종료 시 stderr 꼬리) 순서와 auth의 `finish_login` → 상태 재확인 흐름도 그대로다. 정상 종료에서는 EOF가 자식 종료와 거의 동시에 오므로 stderr를 wait 뒤에 모아도 내용을 잃지 않는다.
- 유한 대기 보장을 지점별로 보면 이렇다. mcp는 stdout·stderr 모두 상한 안에 들어온다. autonomy Windows는 taskkill 5초 상한 × 병렬이다. turn·auth는 stderr만 상한이 있다(RV-001).
- Windows 위임은 B2(절대경로 taskkill)와 CREATE_NO_WINDOW를 한 함수(process_util.rs:111-128)로 모아 사본 하나를 없앴다. 앱 종료의 최악 지연은 `max_wait` + 5초로 유한하다. 스레드 수는 concurrency 한도(runtime.rs:174-182)로 묶인다.

### tauri-runtime-thread-and-lock-discipline (수렴)
- claude_auth의 순서 변경은 경합을 새로 만들지 않는다(아래 기각 1).
- stdout 루프가 도는 동안에는 done이 아직 false이므로 LOGIN_MAX_WAIT 워치독이 살아 있다. 유닉스에서는 같은 그룹의 자손이 stdout을 쥐어도 최대 5분 뒤 그룹 kill로 풀린다. turn에는 이런 워치독이 없고, 사용자 취소(`cancel_turn` → 그룹 kill)만이 해제 수단이다. Windows에서는 루트가 이미 죽었으면 `taskkill /T`가 트리를 따라가지 못해 그 해제 수단도 통하지 않는다. RV-001의 잔여 위험이 Windows 쪽에서 더 큰 이유다.
- `SPAWN_LOCK`을 `process_util::test_support`로 옮겨 mcp 테스트도 함께 직렬화한 것은 맞는 방향이다. 락 밖에서 spawn하는 claude_auth 테스트의 경합(이슈 (3))은 이번 범위가 아니며 그대로 남아 있다. 구현자도 이를 밝혔다.

### claimed-vs-verified (수렴)
- 커밋 2abc9f6의 제목 "남은 무상한 파이프 join… 일원화"는 **join** 기준으로 참이다. 하지만 같은 증상의 상위 원인인 stdout 루프는 포함하지 않는다. 주석이 효과를 넓게 서술한다(RV-001).
- 신규 mcp 테스트는 실효가 있다. 시간 하한(`elapsed >= 450ms`, mcp_manager/process.rs:197-200)으로 "자손이 실제로 파이프를 물었다"는 전제를 검증하므로, 전제가 깨져 테스트가 빈 깡통이 되는 경우를 막는다. 출력(`before-exit`)과 성공 판정이 보존되는지도 단언한다.
- 프론트는 백엔드 원문 `mod.rs:609-614`의 format! 문자열을 하네스가 소스에서 직접 읽어 대조한다(fallbackNoteBoundary.mjs). 문구가 갈라지면 실패하도록 묶여 있다. `FALLBACK_NOTE_MARK`는 `'\n\n(구버전 claude CLI 호환:'`로 Rust 원문과 일치하고, 노트는 성공·실패 모두 메시지 맨 끝에 붙는다(mod.rs:575-583). 일반 성공 토스트가 기존 문구와 완전히 같다는 것은 하네스와 스크린샷 양쪽에서 확인했다.
- 노트를 sanitize하기 **전에** 원문에서 먼저 뽑아낸다(catalog.ts:131, installFallbackNote.ts:30-38). 그래서 길이 제한(errorMessage.ts:23-25, 520자)에 노트 꼬리가 잘리지 않는다. 경계값 "긴 본문 5000자"도 PASS다.

### zero-base-redesigner (발산형)
- 🔵 **RT-1 사람용 문장을 파싱하지 말고 구조화 필드를 쓴다.** 지금은 한국어 안내 문장의 앞머리를 프론트가 표식으로 잘라 낸다. 하네스 대조가 결합을 감시하지만, 결합 자체는 남아 있다. 대안: `CommandResult`에 `fallback: Option<{ dropped: Vec<String>, scope_uncertain: bool }>`를 더하고, `with_fallback_note`는 message 대신 이 필드를 채운다. 프론트는 필드로 문구를 조립한다. 그러면 `-y`만 뺀 경우는 중립 톤으로 처리되고(N-3도 해소), extractFallbackNote·경계값 검사·소스 대조는 모두 필요 없어진다. 비용은 Rust 구조체 1개, 프론트 2호출부, 기존 실패 토스트 문구 조정이다(미확인 추정치).
- 🔵 **RT-2 호출부별 수거 대신 "자식 실행+수거" 한 함수로 모은다.** 이번 라운드는 리더(PipeDrain)를 공유하게 했지만, "stdout을 메인 스레드에서 끝까지 읽는다"는 패턴은 turn·auth에 각각 사본으로 남아 RV-001을 만든다. 대안: stdout도 리더 스레드에서 읽어 줄·청크를 채널로 넘기고, 메인 루프는 `recv_timeout`으로 받으면서 `try_wait()`로 자식 종료를 감지한다. 종료를 감지하면 `DRAIN_GRACE`까지만 남은 메시지를 받는다. 이 루프를 `process_util::run_streaming_bounded(command, on_stdout_chunk)` 하나로 두면 turn·auth·mcp·dev_tools 4경로가 같은 종료 의미를 갖는다. 실행 로직이 AppHandle과 분리되므로 테스트 공백(RV-002 (1))도 같이 닫힌다.

## 트레이드오프

- RV-001 대응 수준: 주석 정정과 이슈 이월만 할지, RT-2까지 갈지. 실제 claude CLI의 하위 프로세스(MCP 서버·Bash 도구)는 자체 파이프를 써서 부모의 표준 입출력을 물려받지 않는다. 그래서 관측 빈도는 낮다고 본다. 이것은 실측이 아니라 추정이다. 권고: 지금은 (a)(b)만 하고, RT-2는 Windows에서 실제 고착 보고가 나오거나 turn/auth에 테스트를 넣을 때 함께 한다.

## 기각된 지적

1. **"claude_auth에서 done.store를 drain보다 앞에 두어 워치독 kill 창이 2초 줄어든다 = 경합"** — 기각. 워치독이 하는 일은 자식 kill인데, done을 세우는 시점(claude_auth.rs:716)에는 자식이 이미 `wait()`으로 수거된 뒤다. 수거 뒤에는 죽일 대상이 없다. 수거 직후 done을 세우는 순서는 예전과 같다. 따라서 pid 재사용 창도 늘지 않았다. 오히려 예전에는 stderr join이 wait보다 앞에 있었다.
2. **"Windows 분기에서 지운 import(SilentCommand·Mutex·Read)가 컴파일 오류를 낸다"** — 기각. grep으로 남은 참조가 0건이다. `.silent()`는 trait가 정의된 process_util 모듈 안에서 호출된다(RV-003 확인방법).
3. **"pid별 스레드 생성 수에 상한이 없다"** — 기각. running 항목 수는 `try_start_now`의 concurrency 검사(runtime.rs:174-182)로 제한된다.

## 생략한 것

- turn·auth의 실기 재현(자손이 파이프를 쥔 상태): AppHandle 의존 때문에 하지 않았다. 코드 정독과 mcp 테스트 동형 구조로 판단했다.
- Windows 실기와 크로스 컴파일: ring C 헤더 문제로 하지 않았다. RV-003 참조.
- 설정 MCP 화면 캡처: 해당 하네스 시나리오가 없어 생략했다(RV-002).
- cargo test 전체(730건) 재실행: 관련 모듈 필터로 205건만 실행했다.

## 잘 된 점

- 공유 리더를 옮길 때 문서 주석도 함께 옮겼다. "포기 후 SIGPIPE"와 "파이프당 1개 잔존"이라는 정확한 서술도 유지했다. 그리고 "복붙하지 말 것"을 모듈 첫머리에 못박았다.
- `collect_logged`의 context 인자에 "비밀이 들어갈 수 있는 값은 넣지 않는다"를 명시했다(process_util.rs:574-576).
- mcp는 두 파이프가 마감시각 하나를 공유한다(mcp_manager/process.rs:111-112). 최악 지연이 2배로 늘지 않는다.
- `run_bounded`로 timeout과 drain_grace를 주입받게 해서, 실제 프로세스로 테스트할 수 있게 만들었다.
- Windows 앱 종료 경로에서 복붙 사본을 없애고, 절대경로·창 억제·상한을 한 함수에 모았다.
- 프론트 판별 함수를 import 없는 순수 모듈로 두어 Node 경계값 검사가 그대로 로드한다. 백엔드 소스와의 대조도 자동이다. 일반 성공 표시는 바이트 단위로 같다.

## PM 권고

1. push 후 windows-latest CI 통과를 확인하고 나서 이슈 (2)를 종결한다(RV-003).
2. 이슈 (1)은 "join 3지점 해소"로 종결하되, turn·auth stdout 루프의 잔여 위험(RV-001)을 새 항목으로 이월한다. 두 주석 정정은 Micro 수정으로 처리할 수 있다.
3. 이슈 01m4sqb1ba3jggmg179bhrywmg(폴백 노트 미표시)는 카탈로그 경로 기준으로 해소를 확인했다. 설정 MCP 경로의 하네스 시나리오 추가(RV-002 (2))를 권장한다.
4. RT-1·RT-2는 채택 여부를 결정해 기록한다(미채택도 결정으로 남긴다).
5. 이 보고서·페르소나 이력 외의 실행 액션(push·코드 수정·이슈 상태 변경)은 하지 않았다.
