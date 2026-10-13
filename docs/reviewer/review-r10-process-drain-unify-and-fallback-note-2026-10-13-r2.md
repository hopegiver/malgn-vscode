# R10 재검토(2차): 파이프 수거 상한 일원화 + 설치 폴백 노트 표시

리뷰 대상: 커밋 범위 `61d8646..473bb5a` (원 범위 8fc6c98·2abc9f6 + 후속 473bb5a) / target_id `r10-process-drain-unify-and-fallback-note`
- 등급: Refactor, 2차 재검토, **축소 모드(C)**. 노출 범위 축소는 적용하지 않았다(위임에 축소 주장 없음).
- 리스크 범주: 자율작업·세션 채팅·로그인 경로의 자식 프로세스 종료/출력 대기, 공유 프로세스 유틸 구조 변경, 플러그인 설치 결과 UI. 직전 라운드와 같다.
- 직전 리뷰: `docs/reviewer/review-r10-process-drain-unify-and-fallback-note-2026-10-13.md` (Green, 조건 1개)
- 일자: 2026-10-13 / 리뷰어: reviewer

## 종합 판정: 🟢 Green (조건 없음)

Critical 0 / Major 0 / Minor 1(RV-002, 이월) / Nit 2(N-3·N-4, 이월·기록) / Rethink 2(RT-1·RT-2, 이월)

- 직전 라운드의 유일한 조건 RV-003(Windows 컴파일·테스트 미검증)은 **해소**됐다. CI run 38361489303의 결과를 직접 대조했다(아래 "직접 확인한 것").
- RV-001은 주석 정정으로 **"과장된 주장" 결함이 해소**됐다. 남은 것은 stdout 루프에 상한이 없다는 기능 공백이고, 지금은 코드 주석이 그 공백을 스스로 밝히고 있다. 원 이슈가 지목한 지점 밖이므로 이월은 정당하다(아래 "PM 처리 방침 평가").
- N-1·N-2는 반영됐다.

## 모드 판정 근거

1. 풀패널 강제 승격 조건: 직전 라운드에 Critical/Major가 없다. 후속 커밋은 주석 3곳과 에러 문구 1곳만 바꿨다(새 실행경로·리스크 표면 없음). 다른 Sensitive 하위도메인으로 넘어가지 않았다. 2차 재검토다. 명시적 풀패널 요청도 없다. → **해당 없음**
2. 동일 대상 인정 4조건(직접 대조): target_id 동일 / 직전 리뷰와 같은 날(7일 이내) / 리스크 범주 문구가 직전 보고서 5행과 같다 / 후속 diff 4파일(autonomy/mod.rs, claude_auth.rs, mcp_manager/process.rs, session_chat/turn.rs)이 모두 직전 리뷰 대상 12파일 안에 있다. → **충족**
3. 새 리스크 표면이 없으므로 증분(B)이 아니라 **축소(C)**를 택했다. 직전 지적의 반영 여부 확인과 CI 대조에 집중했다.

## 페르소나 재사용 판정

착수 전 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-process-execution-security.md` | 재사용 | RV-003 Windows 분기 CI 확인이 역할개념(프로세스 수명 안전성) 그대로다 |
| `persona-claimed-vs-verified.md` | 재사용 | RV-001의 핵심은 "주석 주장 vs 실제 보장"이었다. 정정된 주석을 코드와 다시 대조하는 일이 역할개념 그대로다 |
| `persona-tauri-runtime-thread-and-lock-discipline.md` | 재사용 | 정정 주석이 워치독·취소 해제 경로를 정확히 서술하는지 확인했다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수다. RT-1·RT-2 이월의 타당성을 다시 봤다 |

## 직접 확인한 것

- `git diff 48884b5..473bb5a` 전체(4파일, +17/-11)를 읽었다.
- `git rev-parse origin/main` = `473bb5a3b7102219d15fdcb4060416e05b462136`. push가 확인됐다.
- `gh run view 38361489303`: headSha `473bb5a3…`, status completed, conclusion success. 잡 3개 frontend-check / rust-test (macos-latest) / rust-test (windows-latest)가 모두 success다.
- windows-latest 잡 로그를 grep했다: `force_kill_all_remaining_terminates_registered_running_child_on_windows ... ok`, `force_kill_all_remaining_does_not_panic_when_called ... ok`, `force_kill_all_remaining_skips_non_running_entries ... ok`, `test result: ok. 704 passed; 0 failed; 21 ignored`(두 번 실행된 단계 모두). → 직전 PM 권고 1의 확인 대상 테스트가 Windows에서 실제로 실행돼 통과했다.
- 로컬 `cargo test --lib mcp_manager::process`: 2 passed / 0 failed / 1 ignored. 에러 문구 변경이 컴파일되고 기존 테스트가 깨지지 않는다.
- `grep MCP_COMMAND_TIMEOUT_SECS`: 상수는 process.rs:16 정의와 :67 호출부(`Duration::from_secs`)에서 여전히 쓰인다. 문구에서 뺀 뒤에 unused 경고가 생기지 않는다.
- 정정 주석과 코드를 대조했다: `autonomy/runtime.rs:223-234`(unix는 `libc::kill(-(pid), SIGKILL)`, 프로세스 그룹)와 `:255-267`(Windows는 `kill_process_group_with_grace` 위임 + pid별 스레드). `claude_auth.rs:590 pump_stdout`, `:95 LOGIN_MAX_WAIT`.
- PM이 보고한 로컬 730건 전체 실행은 다시 하지 않았다. CI의 704 passed(Windows)로 대신 확인했다. 플랫폼별 cfg 때문에 건수가 다르다.

## 직전 지적 추적

| ID | 직전 상태 | 이번 상태 | 근거 |
|---|---|---|---|
| RV-001 | 🟡 Minor: 주석이 고착 해소를 과장 | **주석 결함 해소** / 기능 공백은 이월 | turn.rs:432-437 "보장 범위는 여기까지다 — 위의 stdout 줄 읽기 루프는 상한이 없어서 … `finish_turn`/`emit_done`에 도달하지 못한다(stdout 루프의 상한화는 남은 과제)". claude_auth.rs:710-715도 `pump_stdout` 기준으로 같은 서술이다. 예전의 "고착 증상 해소" 서술은 삭제됐다 |
| RV-002 | 🟡 Minor: turn/auth 순서 변경·settingsMcp 하네스 테스트 공백 | **미해결, 이월** | 후속 커밋에 테스트 변경이 없다 |
| RV-003 | 🟡 Minor: Windows 미검증(조건) | **해소** | CI run 38361489303, windows-latest success, 해당 테스트 ok |
| N-1 | ⚪ 타임아웃 문구가 상수 사용 | **해소** | mcp_manager/process.rs:127-129 `timeout.as_secs()` |
| N-2 | ⚪ "unix 한정 보루" 주석 | **해소** | autonomy/mod.rs:434-436. runtime.rs 두 cfg 구현과 일치한다 |
| N-3 | ⚪ 노트 닫기 불가·중립 톤 부재 | 미해결, 이월(RT-1과 함께) | 변경 없음 |
| N-4 | ⚪ 포기 후 SIGPIPE 동작 확대(기록용) | 미반영, 조치 불필요 | 주석 한 줄 권고는 선택 사항이었다 |
| RT-1 / RT-2 | 🔵 Rethink | 이월 | 변경 없음 |

## 이번 라운드 신규 지적

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 |
|---|---|---|---|---|---|
| N-5 | ⚪ Nit | `src-tauri/src/session_chat/turn.rs:435-437`, `src-tauri/src/claude_auth.rs:712-715` | 정정 주석은 "EOF가 올 때까지 계속 기다린다"고 정확히 적었다. 하지만 그 대기를 끝내는 수단을 적지 않았다. 유닉스에서 auth는 `LOGIN_MAX_WAIT`(5분) 워치독의 그룹 kill로 EOF가 오고, turn은 사용자 취소(`cancel_turn`)의 그룹 kill로만 EOF가 온다. Windows에서는 루트가 이미 죽었으면 두 수단 모두 손자에게 닿지 않을 수 있다. 직전 RV-001 개선안 (b)는 이 괄호를 권했는데 반영되지 않았다. 지금 주석이 틀린 것은 아니다 | 이월 이슈(stdout 상한화) 본문에 해제 수단의 플랫폼별 차이를 적어 두면 충분하다. 주석은 그대로 둬도 된다 | 직전 보고서 RV-001 (b)와 diff 대조, claude_auth.rs:95·656-666 |

## 페르소나별 관점

### claimed-vs-verified (수렴)
- 커밋 473bb5a의 제목 "주석을 실제 보장 범위에 맞추고"는 diff와 일치한다. 두 주석 모두 "보장 범위는 여기까지다"라고 한계를 현재형으로 밝힌다. 남은 과제라는 표시도 코드 안에 남아서, 다음 작업자가 "상한 일원화 완료"로 오독할 여지가 없어졌다.
- N-1 수정으로 테스트 400ms 경로의 문구는 "0초 내에…"가 된다(`as_secs()`가 절삭되므로). 프로덕션 15초에는 영향이 없다. 테스트가 문구를 단언하지 않으므로 기각 대상도 아니다. 기록만 남긴다.

### process-execution-security (수렴)
- 직전 라운드에서 정적 검토만 했던 Windows `force_kill_all_remaining`이 실제 Windows 러너에서 자식 종료까지 검증됐다. 직전 지적 중 유일한 "검증 미완" 항목이 닫혔다.
- 범위 밖 관찰이었던 가짜 pid 4242 테스트(runtime.rs:441-445)는 이번 Windows CI에서도 문제를 일으키지 않았다. 다만 이것은 운이 좋았다는 뜻이지 안전하다는 뜻이 아니다. 이월에 동의한다.

### tauri-runtime-thread-and-lock-discipline (수렴)
- 후속 커밋은 실행 순서를 바꾸지 않았다(주석과 format! 인자만 바뀜). 직전 라운드의 순서·경합 판단(기각 1)이 그대로 유효하다.
- N-5의 해제 수단 비대칭(auth에는 워치독이 있고 turn에는 취소뿐)은 RT-2 착수 때 설계 입력이 된다.

### zero-base-redesigner (발산형)
- 🔵 RT-1·RT-2는 그대로 유효하다. 이번 정정으로 RT-2의 필요성이 코드 안에 "남은 과제"로 명문화됐다. 이월 이슈의 목적을 "turn/auth stdout 상한"이 아니라 **"4경로 공용 `run_streaming_bounded`로 수렴 + AppHandle 분리 테스트"**로 잡기를 권한다. 그러면 RV-001 잔여, RV-002 (1), N-5가 한 작업으로 닫힌다. stdout 루프에 상한 사본을 두 개 더 만들면, 이번 라운드가 없앤 "호출부별 사본" 문제를 다시 만든다.

## PM 처리 방침 평가

**결론: 이월은 두 이슈의 해소 판정을 막지 않는다. 동의한다.** 근거:

1. **01m4snx3fy1gmgdxatns41gxmq 항목(1)**: 이슈 원문의 범위는 "무상한 join — mcp_manager/process.rs:103, session_chat/turn.rs:437, claude_auth.rs:714"다(직전 보고서 37행에서 원문 확인). 세 join은 모두 `PipeDrain` 상한 수거로 바뀌었다. stdout 루프는 이슈가 지목한 지점이 아니다. 그리고 이제 주석이 그 공백을 거짓 없이 적고 있다. 지목 지점 기준으로 해소됐다.
2. **같은 이슈 항목(2)**: "autonomy Windows taskkill 상한 없음"이 직전 정적 검토와 이번 Windows CI 실행으로 모두 확인됐다. 해소됐다.
3. **01m4sqb1ba3jggmg179bhrywmg(폴백 노트 미표시)**: 카탈로그 경로에서 표시를 직전 라운드가 하네스와 스크린샷으로 확인했다. 설정 MCP 경로는 코드상 노트를 붙이지만(settings.ts:1376-1377) 하네스 시나리오가 없다(RV-002 (2)). 이슈는 "보여주지 않는다"는 결함이었다. 테스트 공백은 결함이 아니라 회귀 방지 수단의 부족이다. 해소 판정을 막지 않는다.
4. RT-1·N-3(UX 개선)과 가짜 pid 4242(기존 테스트 결함)는 원 이슈들과 결함 축이 다르다. 별도 이슈로 다루는 것이 맞다.

**단, 한 가지 순서 요구**: 이슈를 종결할 때 이월 hub 이슈를 **같은 턴에 먼저 등록**하고, 종결 기록에 그 이슈 id를 적는다. 등록 없이 종결만 하면 RV-001 잔여분이 사라진다. 이것은 판정 조건이 아니라 기록 절차 권고다. 코드 산출물의 Green에는 영향이 없다.

## 기각된 지적

1. **"N-1 수정 뒤 `MCP_COMMAND_TIMEOUT_SECS`가 unused가 되어 경고/CI 실패"** — 기각. process.rs:67에서 여전히 쓰이고, CI 3잡이 모두 success다.
2. **"정정 주석이 워치독 해제를 빠뜨려 틀린 서술"** — Nit(N-5)로 강등. "EOF가 올 때까지"라는 서술은 사실이다. 누락된 것은 부가 정보일 뿐 오류가 아니다.

## 생략한 것

- 화면 캡처: 후속 커밋에 UI 변경이 없다. 직전 라운드의 스크린샷 2장을 판정 근거로 그대로 쓴다.
- cargo test 전체 재실행: CI의 macOS·Windows 전체 실행으로 대신했다. 로컬에서는 mcp_manager::process만 돌렸다.
- turn/auth 실기 재현: 직전과 같은 사유(AppHandle 의존)로 하지 않았다. 이번 변경은 주석만 바뀌어 실기 재현이 필요하지 않다.
- `persona-heterogeneous-machine-operator`: 부르지 않았다. Windows 관점은 CI 결과 대조라 process-execution-security가 맡았다.

## 잘 된 점

- 직전 지적 3건(RV-001 주석, N-1, N-2)을 범위를 넘기지 않고 정확히 반영했다. 주석에 "보장 범위는 여기까지다"와 "남은 과제"를 현재형으로 적어, 한계가 코드 안에 남게 했다.
- push 뒤 Windows CI를 실제로 돌리고 run id까지 위임 메시지에 실었다. claimed→verified 전환이 재현 가능한 형태로 남았다.
- N-2 주석이 두 플랫폼 보루를 함수 이름(`force_kill_all_remaining`, `kill_process_group_with_grace`)으로 가리켜, 다음에 고칠 때 바로 찾아갈 수 있다.

## PM 권고

1. 이월 hub 이슈를 먼저 등록한다. 권장 범위는 RV-001 잔여(turn/auth stdout 상한), RV-002, N-5, 그리고 RT-2 형태의 공용 함수 수렴이다. RT-1·N-3과 pid 4242 테스트는 별도 항목으로 등록한다. 등록한 뒤 01m4snx3fy1gmgdxatns41gxmq (1)(2)와 01m4sqb1ba3jggmg179bhrywmg를 `issue_resolve`로 종결하고, 종결 기록에 이월 이슈 id를 적는다.
2. RT-1·RT-2 채택 여부는 이월 이슈 안에서 결정으로 기록한다.
3. 실행 액션: reviewer는 이 보고서와 페르소나 적용 이력(4파일)·INDEX.md를 경로 지정으로 커밋했다. push, 코드 수정, hub 이슈 상태 변경은 하지 않았다.
