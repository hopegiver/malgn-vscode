# 세션 상세 = 실제 대화 + 이어쓰기 (설계 / IPC 계약)

작성: architect · 2026-09-10 · 등급 Sensitive(외부 프로세스를 사용자 입력과 함께 spawn하는 신규 기능 클래스)
대상 독자: backend-dev(Rust), frontend-dev(TS). 이 문서 하나로 **병렬 착수** 가능하도록 IPC를 확정한다.

목표(완료판정): 세션 상세 화면이 실제 jsonl 대화를 보여주고, 하단 입력창으로 보낸 메시지가 그 세션에 이어져 응답이 스트리밍된다.

---

## 0. 요약 (읽는 데 1분)

- 실행 명령은 **확정**됐고 이 머신에서 **실제 왕복까지 검증**했다:
  `claude -p --resume <sid> --output-format stream-json --verbose --include-partial-messages --permission-prompts none`
  **프롬프트는 argv가 아니라 stdin으로 넣는다**(§1-B7 — argv로 넣으면 사용자가 친 `--...`가 CLI 플래그로 해석되는 것을 실측 확인).
- 조회는 `~/.claude/projects/<mangled-cwd>/<sessionId>.jsonl`을 Rust가 직접 줄 단위로 읽어 **6종 규칙으로 접어서** 반환한다(§3).
- 신규 Rust 모듈 `src-tauri/src/session_chat.rs` 하나 + 커맨드 4개(`read_session_transcript`/`send_session_message`/`start_new_session_message`/`cancel_session_turn`) + 이벤트 2개. `capabilities/default.json` **변경 불필요**.
- **사람 승인 항목 3건**(§6): ① 라이브 세션에 이어쓰기 허용 여부 ② 도구 실행 권한 모드 ③ 취소 시 프로세스 그룹 kill 범위 — **모두 결정 완료**.
- 세션목록 행은 `read_claude_sessions()`가 **base(jsonl) + overlay(registry)** 두 층으로 만든다: 행 자체는 `~/.claude/projects/<프로젝트>/<sid>.jsonl`에서 나오고(**depth-1만** — 재귀하면 서브에이전트 트랜스크립트가 유령 세션으로 올라온다), registry는 그 행에 `running: bool` 하나만 얹는다(§1-F).

---

## 1. 실측 검증 결과 (claimed vs verified)

검증 환경: `claude 2.1.266` (`/opt/homebrew/bin/claude` → `~/.local/bin/claude` 심링크), macOS Darwin 25.5.0.
왕복 테스트 cwd: `/private/tmp/.../scratchpad/clitest` (이 저장소 아님). 테스트 세션 id `11111111-2222-4333-8444-5555555555 01/02`.

### A. 실제 1회 왕복 — **성공(verified)**

`claude -p "1+1?" --output-format stream-json --verbose --session-id <uuid> --permission-prompts none` 실행 결과(발췌):

```
{"type":"system","subtype":"init","cwd":"...","session_id":"11111111-...-501","model":"claude-sonnet-5","permissionMode":"auto",...}
{"type":"assistant","message":{...,"content":[{"type":"text","text":"2"}],...},"session_id":"11111111-...-501",...}
{"type":"rate_limit_event",...}
{"type":"result","subtype":"success","is_error":false,"result":"2","num_turns":1,"total_cost_usd":0.1107996,...}
```

이어서 같은 id로 `--resume` + `--include-partial-messages` 실행 → 문맥 유지 확인("그럼 3을 더하면?" → `"5"`), 그리고 델타 스트림 실물:

```
{"type":"stream_event","event":{"type":"message_start",...}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"5"}}}
{"type":"assistant","message":{...content:[{"type":"text","text":"5"}]...}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn",...}}}
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"result","subtype":"success",...}
```

도구 호출이 섞인 턴도 실측했다(프롬프트: "Bash 도구로 `ls -1`을 실행"):

```
stream_event content_block_start: {"type":"tool_use","id":"toolu_01Rgns...","name":"Bash","input":{},"caller":{"type":"direct"}}
stream_event content_block_delta: {"type":"input_json_delta","partial_json":"{\"command\": \"ls -1"}   ← 인자는 조각으로 온다
assistant: [{"type":"tool_use","id":"toolu_01Rgns...","name":"Bash","input":{"command":"ls -1","description":"List files in current directory"}}]  ← 여기서 완결
user:      [{"tool_use_id":"toolu_01Rgns...","type":"tool_result","content":"out1.txt\nout2.txt","is_error":false}]
stream_event content_block_delta: {"type":"text_delta","text":"out"} ...
result: success
```

**설계 반영**: 도구 한 줄 요약은 `input_json_delta`(조각 JSON)를 조립하지 말고 **완결된 `type:"assistant"` 이벤트의 tool_use 블록**에서 만든다. 파싱 난이도 0.

최종 설계 형태(프롬프트 stdin + 전체 플래그)로 한 번 더 왕복: rc=0, text 델타 28개, 첫 델타 5.55초, stderr 없음. **verified**.

### B. 플래그·동작 사실 (전부 이 머신 실행 출력 근거)

| # | 사실 | 근거 | 판정 |
|---|---|---|---|
| B1 | `-p/--print` 비대화형, `--output-format`은 `text\|json\|stream-json` | `claude --help` | verified |
| B2 | `--output-format stream-json`은 **`--verbose` 필수** | 생략 시 `Error: When using --print, --output-format=stream-json requires --verbose` (rc=1, stdout 비어있음) | verified |
| B3 | `--include-partial-messages`로 text 델타 수신(위 A) | 실행 출력 | verified |
| B4 | `--resume <uuid>`는 **cwd와 무관하게** 세션을 찾는다. 다른 디렉터리에서 resume해도 기록은 **원래 프로젝트 디렉터리의 jsonl에 append**된다 | 다른 temp dir에서 resume → 답 "5"(문맥 유지), 원본 jsonl 306984→312493B 증가, 새 cwd 디렉터리에는 빈 `memory/`만 생김 | verified |
| B5 | 같은 세션에 **동시 resume 2개가 모두 성공**한다(락 없음) | 병렬 실행 A→"7", B→"9" 둘 다 rc=0 | verified |
| B6 | **background 세션**(`claude --bg`로 뜬 것)은 resume 거부 | rc=1, `Error: Session c46e6aa0-... is running as a background session (c46e6aa0). Run 'claude attach ...' or 'claude stop ...' first to resume it here. Add --fork-session to branch off a copy instead.` | verified |
| B7 | **argv 위치인자로 들어간 사용자 입력은 CLI 플래그로 해석된다** | `claude -p --help --resume <sid> ...` → 도움말 출력 후 rc=0(대화가 아니라 CLI 도움말) | verified |
| B8 | **stdin으로 프롬프트를 주면 플래그 해석이 일어나지 않는다** | argv에 프롬프트 없이 stdin에 `--help 라는 글자를 그대로 출력해` → `result: "--help"` (rc=0) | verified |
| B9 | print 모드는 stdin을 기다린다 — stdin을 열어두고 데이터를 안 주면 `Warning: no stdin data received in 3s` 후 진행 | 2>&1 실행 시 stderr 첫 줄 | verified |
| B10 | 존재하지 않는 세션 resume → rc=1 + stdout `{"type":"result","subtype":"error_during_execution","is_error":true,...}` + stderr `No conversation found with session ID: ...` | 실행 출력 | verified |
| B11 | 잘못된 플래그 → rc=1, **stdout 비어있음**, stderr `error: unknown option '--nope'` | 실행 출력 | verified |
| B12 | 턴 도중 SIGKILL → **사용자 프롬프트는 jsonl에 남고 assistant 응답은 안 남는다** | 6초 후 kill(rc=-9, 델타 51개 수신), jsonl +1920B, 마지막 엔트리가 `user`(프롬프트) + attachment + file-history-snapshot | verified |
| B13 | 중단 후 다음 resume은 정상 동작하며, **응답 못 받은 직전 프롬프트를 다음 턴에서 다시 처리한다** | 중단 뒤 `-p "ok?"` → 중단됐던 "1부터 300까지 세어줘"의 결과가 출력됨 | verified |
| B14 | `--permission-prompts none`으로도 자동 모드가 안전하다고 판단한 도구는 **실행된다** | 위 Bash `ls -1`이 프롬프트 없이 실행됨(`permissionMode:"auto"`, `permission_denials:[]`) | verified |

### C. 미검증(솔직히 밝힘)

- **실제로 살아있는 IDE 세션(`entrypoint:"claude-vscode"`)에 resume을 걸었을 때의 동작**: 시도하지 않았다. 목록의 세션은 전부 사용자/동료가 실제 작업 중인 세션이라 건드릴 수 없다. 대신 ①background 세션은 거부됨(B6) ②registry 항목을 흉내 낸 가짜 엔트리(kind `interactive`, peer 필드 포함)로는 resume이 정상 통과(rc=0)함을 확인했다 → **라이브 interactive 세션은 거부되지 않고 그냥 같이 append될 가능성이 높다**(B5의 동시성 위험이 그대로 적용). 이것이 §6-①의 확정(A안)이 안고 가는 유일한 잔여 불확실성이다.
- 중간에 관측된 1건의 120초 미완료는 프롬프트 중의성(`"숫자 6만 출력"` = 60,000 출력)으로 인한 장문 생성으로 추정하며 **재현되지 않았다**(같은 조건 재시도 2회 모두 rc=0/3초). CLI의 락·게이트 증거로 쓰지 않는다.
- Windows 동작 일체(이 검증은 macOS만).

### D. jsonl 실측 구조 (최근 60개 세션 / 20,148줄 스캔)

최상위 `type` 분포:

```
assistant 5983 · attachment 4223 · user 3281 · queue-operation 1653 · bridge-session 1081
atis-latch 1020 · last-prompt 1017 · ai-title 602 · system 592 · file-history-snapshot 406
mode 171 · file-history-delta 118 · cost-state 1        (파싱 실패 0줄)
```

블록/플래그 분포:

```
user  블록: tool_result 2576 · text 414 · <content가 문자열> 333 · image 4
asst  블록: tool_use 2578 · thinking 2056 · text 1350
user  origin.kind: (없음) 2623 · human 398 · task-notification 212 · peer 48
플래그: isMeta 69 · isApiErrorMessage 6 · isSidechain 0 · isCompactSummary 0
```

핵심: **표시할 가치가 있는 줄은 전체의 1/4 미만이고, 그중 절반 이상이 도구 호출**이다. 그래서 §3의 접기 규칙이 화면 단순함의 전부다.

### E. `~/.claude/sessions/*.json`의 정체 — 실행 중 프로세스 registry

세션목록(`list_claude_sessions`)이 **`running` 오버레이 용도로** 읽는 디렉터리다. 실측:

- 파일명은 세션 id가 아니라 **pid**다(`10492.json`). 내용에 `pid`/`sessionId`/`cwd`/`kind`/`entrypoint`/`startedAt`/`messagingSocketPath`/`bridgeSessionId`.
- IDE에서 뜬 항목은 `kind:"interactive"`, `entrypoint:"claude-vscode"`다. 관측 시점의 17개는 전부 **살아있는 프로세스**였다(`os.kill(pid,0)` 17/17 ALIVE, 죽은 항목 0).
- **`claude -p`로 띄운 프로세스도 똑같이 등록된다**(verified). `kind:"interactive"`이고 필드 모양이 IDE 항목과 사실상 동일하다 — 다른 점은 `bridgeSessionId`가 없다는 것뿐이다.
- `entrypoint`는 환경변수 `CLAUDE_CODE_ENTRYPOINT` 값을 그대로 받아쓴다(verified). 앱이 자기 env를 상속시켜 자식을 spawn하면 그 자식이 `claude-vscode`를 물려받으므로, **registry 내용만으로는 우리 자식과 IDE 세션을 구분할 수 없다.** live 집합에서 우리 자식을 걷어내는 기준은 `entrypoint`가 아니라 앱이 직접 들고 있는 pid 집합이어야 한다(§1-F-3 ①).
- `entrypoint:"cli"`(pty로 띄운 순수 CLI 대화형 세션)의 등록 여부는 **미검증**이다.
- `claude agents --json`이 돌려주는 목록과 동일한 집합이다.

**결론(설계 전제)**: registry는 **세션목록의 정본이 아니다.** 목록 행의 정본은 대화 이력(`~/.claude/projects/<프로젝트>/<sid>.jsonl`)이고, registry는 그 행에 "지금 이 머신에서 실행 중"이라는 사실 한 가지(`running: bool`)만 얹는 오버레이다(§1-F).

따라서 목록에는 지금 살아있는 세션과 **이미 끝난 과거 세션이 함께** 보이고, 실행 중 여부는 행의 `running` 플래그로만 표현된다. 이어쓰기 대상도 "다른 창에서 살아있는 세션"으로 한정되지 않는다 — 최근 30일 안에 활동한 세션이면 이미 끝난 것이든 살아있는 것이든 똑같이 `--resume`으로 이어쓴다(그 결정의 전제는 §6-①).

이 구조의 부수 효과: registry 파일이 제때 청소되는지 여부는 **행의 존재가 아니라 배지 한 개의 정확도에만** 영향을 준다(§1-F "알려진 갭").

### F. 세션 목록 구성 로직 (`read_claude_sessions`) — base(jsonl) + overlay(registry)

`read_claude_sessions()`(`src-tauri/src/session_list.rs`)는 두 층으로 목록을 만든다. **행을 만드는 것은 jsonl이고(base), registry는 그 행에 `running` 한 필드만 얹는다(overlay).**

#### F-1. base — jsonl에서 행을 만든다

| 단계 | 내용 |
|---|---|
| ① 후보 수집 | `~/.claude/projects/<프로젝트>/*.jsonl` 중 **depth-1 자식 파일만**. 2단계 `read_dir` + `!path.is_file()` 스킵이라 `<sid>/subagents/agent-*.jsonl`은 애초에 대상이 되지 않는다(재귀 없음 — S6과 같은 순회 규약) |
| ② 안전장치 | (a) `/private/tmp` 유래 프로젝트 디렉터리(`-private-tmp-` 접두사) 제외 (b) `scan_workspace_projects()` 결과 프로젝트(및 그 하위 경로)로 한정. 허용 집합의 정의는 스캐너 하나뿐이다 — 판정 로직을 복제하지 않는다 |
| ③ 상한 | 후보 전량 `stat` → **mtime 30일 컷** → mtime 내림차순 → **상위 100건**. 캐시 없음 |
| ④ 행 생성 | 상한을 통과한 파일만 head 파싱. 파일 하나 = 행 하나 |

**depth-1 제약이 이 층의 정확성 전부다**: 이 머신의 jsonl 1,473개 중 depth-1은 434개이고 **나머지 1,039개가 서브에이전트 트랜스크립트**다(실측). 재귀 수집이었다면 목록에 유령 세션 1,039개가 올라온다.

`cwd`를 끝까지 못 구한 파일(깨졌거나 필드 없음)은 그 파일만 건너뛰고 전체 스캔은 계속한다.

**상한 도달 상태(실측)**: 이 머신의 실제 행 수는 **100개 — 상한에 걸려 있다.** 즉 30일 안에 활동한 세션이 100개를 넘으면 오래된 쪽은 목록에 나오지 않는다. mtime 내림차순으로 자르므로 잘리는 쪽은 항상 "가장 오래 손대지 않은" 세션이다.

#### F-2. 행 필드 계약

| 구분 | 필드 |
|---|---|
| 필수(모든 행) | `sessionId` / `cwd` / `title` / `startedAt` / `updatedAt` / `running` |
| 선택 | `version`(jsonl head에서) / `pid`·`name`·`kind`·`entrypoint`·`bridgeSessionId`(registry 폴백 행에서만) |

- `startedAt` = jsonl 첫 파싱 가능한 `timestamp`(RFC3339 → epoch ms).
- `updatedAt` = **jsonl 파일 mtime**. jsonl은 append-only라 mtime이 곧 그 세션의 마지막 활동 시각이다.
- `title` = 기존 `find_session_title()` 재사용.

#### F-3. overlay — registry가 얹는 `running`

registry(`~/.claude/sessions/*.json`)는 행을 만들지 않는다. `filter_and_dedup_sessions()`의 3단계 결과는 **"지금 실행 중인 `sessionId` 집합"(live 집합)을 계산하는 데만** 쓰인다. 판정부는 fs 접근이 없는 순수 함수로 분리돼 있다(유닛 테스트 대상).

| 단계 | 내용 |
|---|---|
| ① | `ACTIVE_TURNS`(앱이 직접 띄운 자식 `claude -p`의 pid) 제외 |
| ② | 죽은 pid 필터(`process_util::pid_alive`) |
| ③ | 같은 `sessionId`는 `startedAt` **최신 1건**만 남긴다 |

**3단계는 본체·시그니처·적용 순서를 그대로 둔다**(유닛 테스트 포함). 순서의 근거는 그대로다 — 우리 자식 항목의 `startedAt`은 턴을 시작한 시각이라 원본 IDE 항목보다 항상 더 최신이므로, ③을 ①보다 먼저 돌리면 "최신 1건"이 원본 대신 우리 자식을 고르는 역전이 생긴다. 다만 출력이 목록 행이 아니라 sessionId 집합으로 접히면서 그 역전이 좌우하는 범위는 "행 메타데이터가 통째로 바뀐다"에서 **`running` 판정 하나**로 줄었다. 검증된 판정부를 건드릴 이유가 없어 그대로 재사용한다.

`pid`나 `sessionId` 필드가 없는 항목은 판단 근거가 없으므로 **보수적으로 통과**시킨다(dedup 키가 없으니 고유 항목 취급).

오버레이 적용 규칙:

| 상황 | 처리 |
|---|---|
| jsonl 행의 `sessionId`가 live 집합에 있음 | `running: true` |
| jsonl 행의 `sessionId`가 live 집합에 없음 | `running: false` |
| live 집합에는 있는데 대응 jsonl 행이 없음(세션 생성 직후 수 초) | registry 항목을 **폴백 행**으로 추가(`running: true`, `title` 없으면 빈 문자열 — `title` 키는 출력 계약상 항상 있어야 한다) |

#### F-4. 프론트 표시 (`src/views/sessions.ts`)

- 정렬 키는 **`updatedAt` 내림차순**, 없으면 `startedAt` 폴백. 목록이 대화 이력이므로 "언제 시작했나"보다 "마지막으로 언제 말했나"가 사용자 기대에 맞는다.
- 행에는 `kind`/`entrypoint` 배지를 두지 않는다. 실행 중 표시는 **`running === true`인 행에만 붙는 조용한 점 하나**다(경고 문구가 아니다 — 근거는 §6-①).
- `updatedAt`은 이 층이 생기면서 처음으로 값이 채워지는 필드다. 목록·상세 모두 `갱신 …`으로 표시한다.

**알려진 갭(Windows — 배지 오탐)**: `pid_alive()`는 **Windows에서 무조건 `true`를 반환한다**(추가 crate 없이 쓸 수 있는 표준 생존 확인 API가 없어 OS 레벨 구현을 두지 않았다). 따라서 **②단계가 Windows에서는 no-op**이고, 취소·크래시로 자식이 죽은 뒤 `finish_turn()`이 그 pid를 `ACTIVE_TURNS`에서 빼면 ①도 더는 걸러주지 않아, registry 파일만 남은 항목이 live 집합에 그대로 들어간다. **영향 범위는 `running` 점 하나다** — 행은 jsonl이 만들므로 사라지지도, 메타데이터가 바뀌지도 않는다. 즉 Windows의 증상은 **이미 종료된 세션에 `running` 점이 잘못 켜지는 것**이고, 다음 `claude` 실행으로 registry가 갱신될 때까지 그 상태로 남는다. unix는 `kill(pid,0)`로 실제 확인해 ②에서 걸러낸다.

---

## 2. 아키텍처 & 데이터 흐름

신규 파일 **1개**(`src-tauri/src/session_chat.rs`) + 신규 파일 1개(`src/sessionChat.ts` 또는 기존 `src/sessionsApi.ts` 확장). 기존 파일 수정은 `lib.rs`에 `mod`/`invoke_handler` 2줄, `src/views/sessions.ts` 상세 뷰 교체뿐이다 — 워킹트리에서 진행 중인 MCP 작업(`lib.rs`/`mcp_manager.rs`/프론트 4개)과 충돌면을 최소화한 배치다.

```
[views/sessions.ts]  세션 상세
      │ invoke read_session_transcript(sessionId)
      ▼
[session_chat.rs] resolve_transcript_path → BufReader 줄 단위 → 접기 규칙 → SessionTranscript
      │
      │ invoke send_session_message(sessionId, text)   ← 즉시 {turnId} 반환
      ▼
  스폰 스레드
      ├─ resolve_binary_expand_home(["/opt/homebrew/bin/claude", ...], "claude")
      ├─ cwd = 트랜스크립트에서 읽은 cwd (프론트가 준 경로 아님)
      ├─ Command::new(claude).args([-p,--resume,sid,--output-format,stream-json,
      │                             --verbose,--include-partial-messages,--permission-prompts,none])
      │        .stdin(piped) ← 여기에 사용자 텍스트를 쓰고 close
      │        .stdout(piped).stderr(piped).env("PATH", build_child_path_env(..)).process_group(0)
      ├─ stdout 줄마다 JSON 파싱 → emit "session-chat-delta"
      ├─ stderr 리더 스레드(파이프 교착 방지, 마지막 4KB만 보관)
      └─ 종료 → emit "session-chat-done"
      ▼
[프론트] 델타를 임시 말풍선에 이어붙임 → done 수신 시 read_session_transcript 재호출로 화면 갱신
```

신규 세션 경로(`start_new_session_message`)는 위 스폰 스레드(`run_turn`)를 그대로 공유하고 **입력 두 개만 다르다**: cwd는 트랜스크립트가 아니라 검증을 통과한 `project_path`에서 오고(§5-1), 세션 id는 앱이 미리 만든 UUID를 `--session-id`로 넘긴다(`--resume` 대신). 스트림 처리·이벤트·취소·정리는 완전히 동일하다.

**"done 후 전체 재조회"를 쓰는 이유**: 화면에 남는 최종 상태를 항상 파일(jsonl) 하나에서만 만든다. 스트리밍 버퍼와 파일이 어긋날 여지가 사라지고, 중단(B12)·다른 창의 동시 기록(B5)도 자동으로 반영된다. 비용은 파일 1회 재읽기(수 MB, 밀리초 단위).

---

## 3. 메시지 모델과 jsonl 접기 규칙 (확정)

화면 메시지는 **3종뿐**이다: `user` / `assistant` / `tool`(회색 한 줄). thinking·attachment·훅·내부 상태는 전부 버린다.

### 3-1. 줄 단위 판정 규칙

| # | 조건 | 처리 |
|---|---|---|
| R1 | `type=="user"` && content가 문자열이거나 text 블록 포함 | **user 말풍선**. 단 R2에 걸리면 버림 |
| R2 | (버림) `isSidechain==true` \| `isMeta==true` \| `origin`이 있고 `origin.kind!="human"` \| 텍스트가 `<task-notification>`/`<system-reminder>`/`<local-command-`로 시작 | 버림 |
| R3 | `type=="user"` && content에 `tool_result` 블록 | 말풍선 만들지 않음. 직전 tool 줄의 상태만 갱신(`is_error==true`면 `실패` 표시) |
| R4 | `type=="assistant"` && text 블록 | **assistant 말풍선**. 같은 `message.id`의 여러 줄은 **하나로 이어붙인다**(스트리밍이 여러 줄로 쪼개져 기록됨 — `lib.rs:1296` 주석의 message.id 중복 이슈와 같은 성질) |
| R5 | `type=="assistant"` && tool_use 블록 | **tool 한 줄**: `⚙ {name} · {대표인자}` (대표인자 60자 컷) |
| R6 | `type=="assistant"` && thinking 블록만 | 버림 |
| R7 | 그 외 모든 `type` (attachment/queue-operation/bridge-session/atis-latch/last-prompt/ai-title/file-history-*/mode/system/cost-state) | 버림 |

`origin` 필드는 신버전에만 있다(실측: 없는 줄 2623개). 그래서 R2는 **origin이 없으면 통과**시키고 텍스트 접두사로 2차 방어한다 — 우리가 `-p`로 보낸 메시지에도 `origin`이 없기 때문에(실측) origin 필수 규칙을 쓰면 우리 메시지가 화면에서 사라진다.

### 3-2. 대표 인자 추출 (R5)

```
Bash → input.command | Read/Edit/Write/NotebookEdit → input.file_path
Grep/Glob → input.pattern | Task → input.description | WebFetch → input.url
그 외 → input의 첫 문자열 값 | 없으면 빈 문자열
```

### 3-3. 연속 도구 접기 (화면 단순함의 핵심)

tool 줄이 **연속 3개 이상**이면 하나로 합쳐 `⚙ 도구 12회 실행`으로 표시한다(`toolCount`에 개수). 2개 이하는 그대로 둔다. 실측 분포상 도구 줄이 표시 대상의 과반이라, 이 규칙 하나로 화면이 "사람 대화 + 가끔 회색 한 줄"이 된다.

### 3-4. 크기 가드

- 메시지 상한 **400개**(초과 시 오래된 것부터 버리고 `truncated=true`) — 뒤에서부터 담는 고정 크기 버퍼로 O(n) 시간·O(1) 메모리.
- 메시지 1개 텍스트 상한 **8000자**(초과 시 말줄임). 실측 파일 하나가 33줄에 300KB인 경우가 있다(한 줄이 수십 KB).
- 파일 전체를 `String`으로 올리지 않는다 — 기존 코드와 같은 `BufReader::lines()` 스트리밍(`lib.rs:86-108`, `lib.rs:995-` 참조).

### 3-5. 기존 lib.rs 파싱 코드 재사용 판단 (실제로 읽고 내린 결론)

| 기존 코드 | 재사용? | 이유 |
|---|---|---|
| `extract_text_from_content` (`lib.rs:46-66`) | **아니오** | 배열에서 **첫 text 블록 하나만** 반환한다. 우리는 모든 text 블록을 이어붙이고 tool_use/tool_result도 봐야 해서 계약이 다르다. 억지로 확장하면 제목 추출(현재 유일한 호출자)에 회귀 위험만 생긴다 |
| `find_session_title` (`lib.rs:74-109`) | **아니오(그대로 둠)** | 첫 user 줄에서 조기 종료하는 것이 존재 이유다. 전체 파싱과 목적이 다르다 |
| `sanitize_cwd_for_project_dir` (`lib.rs:26-28`) | **아니오** | cwd→디렉터리명 방향인데, 우리는 프론트에서 cwd를 받지 않기로 해서(§5 보안) 반대로 **projects 하위를 스캔해 파일을 찾는다** |
| `find_recent_jsonl_files` (`lib.rs:961-989`) | **아니오** | mtime cutoff 재귀 수집 = 사용량 집계 전용. 우리는 세션 id 하나로 파일 1개를 찾는다(디렉터리 32개 stat, 무시할 비용) |
| 서브에이전트 파싱 (`lib.rs:1211~` 주석, `<sessionId>/subagents/agent-*.jsonl` + `.meta.json`) | **아니오** | 서브에이전트 대화는 화면에 표시하지 않는다(R5의 `Task` 한 줄로 충분). 필요해지면 그때 붙인다 |
| `cli_launcher::resolve_binary_expand_home` | **예** | GUI PATH 문제(launchctl PATH에 homebrew 없음)의 검증된 해법. 그대로 쓴다 |
| `dev_tools::build_child_path_env` | **예** | 자식이 다시 node/git을 부를 수 있어 PATH를 넓혀야 한다. `pub(crate)`라 그대로 호출 가능 |
| `dev_tools`의 프로세스 운영 패턴(`stdin(null)`·리더 스레드·`process_group(0)`·SIGTERM→3초→SIGKILL, `dev_tools.rs:991-1051`) | **패턴만 복사** | 시그니처가 "완료까지 모아서 반환"이라 스트리밍에 안 맞는다. 코드를 공유하지 말고 같은 규율을 따른다 |

`CLAUDE_PATH_CANDIDATES`는 `autonomy.rs:343`이 이미 자체 복사본을 두고 있다(그 파일의 "결정 5.2 — 상수를 공유하지 않고 각자 별도로 둔다, 회귀 위험 0" 관례). `session_chat.rs`도 같은 관례로 자체 상수를 둔다.

---

## 4. IPC 계약 (확정 — 이대로 병렬 구현)

### 4-1. Rust 커맨드

```rust
// src-tauri/src/session_chat.rs

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub kind: String,             // "user" | "assistant" | "tool"
    pub text: String,
    pub tool_count: u32,          // kind=="tool"일 때 접힌 개수(1 이상), 그 외 0
    pub at: Option<String>,       // jsonl 최상위 timestamp(ISO8601) 그대로. 없으면 null
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionTranscript {
    pub session_id: String,
    pub cwd: String,              // 트랜스크립트에서 읽은 실제 작업 디렉터리
    pub transcript_path: String,  // 화면 하단 출처 표기용
    pub messages: Vec<ChatMessage>,
    pub truncated: bool,          // 400개 상한으로 앞부분을 버렸는가
    pub active_turn_id: Option<String>,  // M3: 이 세션에 지금 진행 중인 턴이 있으면 그 turn_id(재부착용)
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SendStarted { pub turn_id: String }   // UUID v4 문자열

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionStarted { pub session_id: String, pub turn_id: String }

/// (a) jsonl 전문 → 구조화 메시지 배열
#[tauri::command]
pub fn read_session_transcript(session_id: String) -> Result<SessionTranscript, String>;

/// (b) 메시지 전송 시작 — 즉시 반환하고 이후는 이벤트로 흐른다
#[tauri::command]
pub fn send_session_message(
    app: tauri::AppHandle,
    session_id: String,
    text: String,
) -> Result<SendStarted, String>;

/// (b') 신규 세션 시작 — 프로젝트 경로를 받아 첫 턴을 spawn한다(S4 참조).
///      session_id는 앱이 미리 만들어 `--session-id`로 넘긴다(사후 캡처 없음).
#[tauri::command]
pub fn start_new_session_message(
    app: tauri::AppHandle,
    project_path: String,
    text: String,
) -> Result<NewSessionStarted, String>;

/// (c) 중단 — turn_id로 찾아 프로세스 그룹 종료. 이미 끝났으면 Ok(())
#[tauri::command]
pub fn cancel_session_turn(turn_id: String) -> Result<(), String>;
```

`SessionTranscript`에 `live` 필드는 두지 않는다 — 그 근거는 §6-①에 있다.

`lib.rs`에 추가할 것: `mod session_chat;` 1줄 + `invoke_handler![... , session_chat::read_session_transcript, session_chat::send_session_message, session_chat::start_new_session_message, session_chat::cancel_session_turn]`.

에러 문자열은 기존 커맨드처럼 **사용자에게 그대로 보여줄 한국어 문장**으로 만든다(`mcp_manager.rs:428-`의 관례). 확정 문구:

| 상황 | 문자열 |
|---|---|
| session_id 형식 위반 | `세션 ID 형식이 올바르지 않습니다.` |
| 트랜스크립트 없음 | `이 세션의 대화 기록 파일을 찾을 수 없습니다.` |
| cwd 없음/삭제됨 | `세션의 작업 폴더를 찾을 수 없습니다: {cwd}` |
| 프로젝트 경로가 재스캔 결과에 없음(S4) | `워크스페이스에서 확인되지 않은 프로젝트 경로입니다.` |
| 프로젝트 폴더 없음/삭제됨(S4) | `프로젝트 폴더를 찾을 수 없습니다: {project_path}` |
| claude 미설치 | `claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).` |
| 같은 세션 진행 중 | `이 세션에 이미 진행 중인 요청이 있습니다.` |
| 전역 동시 실행 초과 | `동시에 실행할 수 있는 요청은 최대 3개입니다.` |
| 빈 입력 / 초과 | `보낼 내용을 입력하세요.` / `한 번에 보낼 수 있는 길이를 초과했습니다(최대 32,000자).` |

### 4-2. Tauri 이벤트 (기존 `claude-sessions-changed`와 같은 케밥케이스 네이밍)

```
"session-chat-delta"   { sessionId, turnId, kind: "text" | "tool", text }
"session-chat-done"    { sessionId, turnId, ok: boolean, canceled: boolean, error: string | null }
```

- `delta.kind="text"` : `stream_event / content_block_delta / text_delta`의 `text` 조각. 프론트는 **그대로 이어붙인다**.
- `delta.kind="tool"` : 완결된 `type:"assistant"` 이벤트의 tool_use 블록에서 만든 **한 줄 요약**(§3-2 규칙 동일).
- 그 외 스트림 이벤트(`system/hook_*`, `rate_limit_event`, thinking, `user`(tool_result), 완결 `assistant`의 text 블록)는 **보내지 않는다**. 특히 완결 assistant의 text 블록을 보내면 델타와 중복되므로 반드시 버린다.
- `session-chat-done`은 **성공·실패·취소 모두에서 정확히 한 번** 발생한다.

> **에러 전용 이벤트를 두지 않은 이유(트레이드오프)**: 종료 이벤트가 둘이면 프론트에서 "생성 중" 상태를 푸는 경로가 둘이 되고, 한쪽을 빠뜨리면 입력창이 영영 잠긴다. 종료 이벤트 하나에 `ok/canceled/error`를 실으면 프론트의 정리 코드가 한 곳으로 강제된다. 대가는 payload가 조금 두꺼워지는 것뿐이다.

에러 문자열 생성 규칙(우선순위):
1. `result` 이벤트의 `is_error==true` → `subtype` + `result` 필드 텍스트 (B10 형태)
2. rc != 0 이고 stdout에 result 없음 → **stderr 마지막 4KB**를 그대로 (B11에서 stdout이 비는 케이스가 실재)
3. spawn 자체 실패 → `claude 명령을 실행하지 못했습니다: {io error}`

### 4-3. TS 타입 (그대로 `src/sessionsApi.ts`에 붙일 수 있는 형태)

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type ChatMessageKind = 'user' | 'assistant' | 'tool';

export interface ChatMessage {
  readonly kind: ChatMessageKind;
  readonly text: string;
  /** kind === 'tool' 일 때 접힌 도구 호출 개수(1 이상). 그 외 0 */
  readonly toolCount: number;
  /** jsonl의 ISO8601 timestamp. 없으면 null */
  readonly at: string | null;
}

export interface SessionTranscript {
  readonly sessionId: string;
  readonly cwd: string;
  readonly transcriptPath: string;
  readonly messages: readonly ChatMessage[];
  /** 400개 상한으로 앞부분이 잘렸는가 */
  readonly truncated: boolean;
  /** M3: 이 세션에 지금 진행 중인 턴이 있으면 그 turnId(재부착용). 없으면 null */
  readonly activeTurnId: string | null;
}

export interface SendStarted {
  readonly turnId: string;
}

export interface NewSessionStarted {
  readonly sessionId: string;
  readonly turnId: string;
}

export interface SessionChatDelta {
  readonly sessionId: string;
  readonly turnId: string;
  readonly kind: 'text' | 'tool';
  readonly text: string;
}

export interface SessionChatDone {
  readonly sessionId: string;
  readonly turnId: string;
  readonly ok: boolean;
  readonly canceled: boolean;
  readonly error: string | null;
}

export async function fetchSessionTranscript(sessionId: string): Promise<SessionTranscript> {
  return invoke<SessionTranscript>('read_session_transcript', { sessionId });
}

export async function sendSessionMessage(sessionId: string, text: string): Promise<SendStarted> {
  return invoke<SendStarted>('send_session_message', { sessionId, text });
}

export async function startNewSessionMessage(projectPath: string, text: string): Promise<NewSessionStarted> {
  return invoke<NewSessionStarted>('start_new_session_message', { projectPath, text });
}

export async function cancelSessionTurn(turnId: string): Promise<void> {
  return invoke<void>('cancel_session_turn', { turnId });
}

export async function onSessionChatDelta(cb: (d: SessionChatDelta) => void): Promise<UnlistenFn> {
  return listen<SessionChatDelta>('session-chat-delta', (e) => cb(e.payload));
}

export async function onSessionChatDone(cb: (d: SessionChatDone) => void): Promise<UnlistenFn> {
  return listen<SessionChatDone>('session-chat-done', (e) => cb(e.payload));
}
```

기존 `fetchClaudeSessions`/`onSessionsChanged`는 그대로 둔다. 기존 `onSessionsChanged`는 `UnlistenFn`을 버리는데(`sessionsApi.ts:20-22`), 새 두 구독은 **화면을 떠날 때 반드시 해제해야** 하므로(세션을 옮겨 다니면 리스너가 쌓인다) `UnlistenFn`을 반환한다.

### 4-4. 프론트 상태 (state.ts 추가분)

```ts
sessionChat: {
  sessionId: string | null;
  transcript: SessionTranscript | null;
  loading: boolean;
  error: string | null;
  /** 전송 중인 턴. null이면 입력 가능 */
  turnId: string | null;
  /** 스트리밍으로 쌓는 임시 assistant 말풍선 */
  streamingText: string;
  /** 스트리밍 중 도착한 도구 한 줄들 */
  streamingTools: string[];
  input: string;
};
```

초기값: `{ sessionId: null, transcript: null, loading: false, error: null, turnId: null, streamingText: '', streamingTools: [], input: '' }`.

---

## 5. 보안 경계 결정 (Sensitive 등급 핵심)

노출 범위 축소 3값(50인 미만 사내 / 로컬 데스크톱 앱 / 본인 로컬 세션 로그)을 반영해 **구조적 결정만 여기서 확정**하고, 강도 조절은 구현에 맡긴다.

| # | 항목 | 결정 | 근거 |
|---|---|---|---|
| S1 | 셸 경유 | **금지**. `std::process::Command` + 인자 배열만. `sh -c`·`osascript` 없음 | 기존 `autonomy.rs:9-14` 규율 그대로 |
| S2 | **사용자 입력의 전달 경로** | **stdin으로만 전달한다. argv에 절대 넣지 않는다** | B7에서 argv 입력이 CLI 플래그로 해석되는 것을 실측. 사용자가 `--dangerously-skip-permissions`를 치면 실행 조건이 바뀐다. stdin은 B8로 안전 확인. 부수 효과로 ARG_MAX·인용 문제도 사라짐 |
| S3 | 실행 바이너리 결정 | `cli_launcher::resolve_binary_expand_home(["/opt/homebrew/bin/claude","/usr/local/bin/claude","~/.local/bin/claude"], "claude")` — 절대경로 후보 우선, 실패 시 PATH bare name 1회 | GUI PATH 문제의 기존 검증된 해법. 프론트는 경로를 지정할 수 없다 |
| S4 | cwd 결정 — **재개 경로** (`send_session_message`) | **트랜스크립트 파일 안의 `cwd` 필드**에서 읽는다. 프론트가 경로를 넘기지 않는다. 디렉터리 존재를 확인하고 없으면 실행 거부 | 프론트발 경로 입력 자체를 없애 경로 조작 표면을 제거. 실측상 `user`/`assistant` 엔트리에 항상 `cwd`가 있다 |
| S4′ | cwd 결정 — **신규 세션 경로** (`start_new_session_message`) | 프론트가 `project_path`를 넘긴다(사람 승인 하의 **조건부 완화**). 아래 4중 안전장치를 모두 통과한 경로만 cwd가 된다 | 신규 세션은 이어붙일 트랜스크립트가 아직 없어 cwd를 파일에서 읽을 방법이 없다. 대신 **허용 집합이 이미 `list_workspace_projects` IPC로 프론트에 공개된 것과 완전히 동일**하므로, 프론트가 이 경로를 말할 수 있게 해도 **새로운 신뢰 경계가 생기지 않는다** |
| S5 | session_id 검증 | `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$` 정규식 통과분만 사용 | 파일명·`--resume` 인자 양쪽에 쓰이므로 `..`·`/`·`-`로 시작하는 값이 원천 차단된다 |
| S6 | 트랜스크립트 탐색 | `~/.claude/projects/*/` **1단계 자식 디렉터리에서 `<sid>.jsonl`만** 확인. 재귀 없음, glob 패턴 사용자 입력 없음 | 서브에이전트 디렉터리(`<sid>/subagents/`)를 건드리지 않는다 |
| S7 | 입력 길이 | 32,000자 상한, 공백만이면 거부 | 실수 붙여넣기 폭주 방지(비용 보호) |
| S8 | 동시 실행 | 세션당 **1개**, 전역 **3개** | B5에서 락이 없음을 확인 — 앱이 스스로 막아야 한다 |
| S9 | 프로세스 수명 | `stdout`/`stderr` 각각 리더 스레드 + `process_group(0)`(unix) + 취소 시 SIGTERM→3초→SIGKILL. `process_group(0)`은 자식을 **새 프로세스 그룹 리더로 분리**하므로 앱이 죽어도 자동으로 함께 죽지 않는다 — 그래서 `ExitRequested`에서 `session_chat::request_shutdown()`을 명시적으로 호출해 `ACTIVE_TURNS`에 남은 턴을 전부 SIGTERM→3초→SIGKILL로 정리한다(M1 수정) | `dev_tools.rs:991-1051`의 검증된 패턴. 파이프 버퍼(64KB)가 차서 자식이 멈추는 교착을 막는다 |
| S10 | 타임아웃 | **하드 타임아웃 없음**. 사용자가 취소 버튼으로만 끝낸다 | 실제 코딩 턴은 수 분~수십 분이 정상. 임의 타임아웃은 정상 작업을 죽인다. 대신 취소 UI를 필수로 둔다 |
| S11 | capabilities | **`capabilities/default.json` 변경 없음**. fs/shell 플러그인 추가하지 않음 | 커스텀 Rust 커맨드만으로 전부 가능(위 계약에 파일 경로 인자가 없다). 이 변경으로 확대된 것은 claude CLI 자식 프로세스의 권한이지 Tauri capabilities 표면이 아니며, 실제 권한 경계는 앱 밖 `~/.claude/settings.json`의 `permissions.defaultMode`가 정한다 |
| S12 | 화면 렌더링 | 메시지 텍스트는 **textContent로만** 렌더(기존 `el()` 헬퍼가 문자열 자식을 텍스트 노드로 넣는 방식 유지). `innerHTML`·마크다운 렌더러 도입 금지 | 대화 로그에는 임의의 HTML/스크립트 문자열이 그대로 들어있다. MVP에서 마크다운을 붙이면 그 자리가 XSS 표면이 된다 |
| S13 | 로그 노출 | Rust는 대화 내용을 stdout/파일로 로깅하지 않는다 | 이 앱의 기존 원칙(대화 전문을 필요 이상으로 옮기지 않는다) 유지 |

### 5-1. S4′ 안전장치 4중 (신규 세션 경로)

`start_new_session_message(project_path, text)`가 받은 경로는 다음을 **순서대로 전부** 통과해야 cwd가 된다.

| # | 장치 | 내용 |
|---|---|---|
| ① | 문자 그대로 신뢰하지 않음 | 받은 문자열을 그대로 `Command::current_dir()`에 넣지 않는다 |
| ② | **요청 시점 재스캔 후 완전 일치** | `scan_workspace_projects()`를 **그 자리에서 다시 실행**해, 결과 집합에 `project_path`와 **정확히 같은** 항목이 있을 때만 통과. 캐시된 목록·프론트가 들고 있는 상태는 신뢰하지 않는다(**TOCTOU 방지**) |
| ③ | spawn 직전 재확인 | 매치 후에도 `Path::new(&p).is_dir()`를 한 번 더 확인한다 — 재스캔과 spawn 사이에 디렉터리가 사라지는 잔여 창을 좁힌다 |
| ④ | 스캐너 공유 | `scan_workspace_projects()`는 **`pub(crate)`로 가시성만 상승**했다. 판정 로직을 `session_chat.rs`에 복붙하지 않는다 — 허용 집합의 정의가 두 곳으로 갈라지는 것 자체를 막는다 |

②가 통과시키는 집합은 `~/workspace` 등 workspace 루트 **바로 아래 1단계 + `CLAUDE.md` 존재**를 요구하는 스캐너의 결과다. 그래서 `/etc`·`~/.ssh`·`..` 트래버설·존재하지 않는 경로는 별도 문자열 검사 없이 **스캔 결과에 없다는 이유로** 전부 걸러진다.

**재개 경로의 S4는 완화되지 않는다.** `send_session_message`는 지금도 트랜스크립트의 `cwd`만 쓰고 프론트에서 경로를 받지 않는다.

---

## 6. 사람 승인 필요 항목 (architect가 임의로 확정하지 않음)

### ① 라이브 세션에 이어쓰기를 허용할 것인가 — **가장 중요** — **확정: A안(게이트도 경고도 없음)**

**결정: A안 — 라이브 여부와 무관하게 그대로 `--resume`으로 이어쓴다. 전송 차단도, 경고 배너도 두지 않는다.**

근거는 두 가지다.

1. **분기 위험은 존재하지 않는다(verified).** `claude -p --resume <id>`는 **원본 `session_id`를 유지하고 원본 jsonl에 그대로 append**한다 — 대화가 갈라지지 않는다. 사본으로 갈라지려면 `--fork-session`을 명시해야 한다(B6의 에러 메시지도 같은 구분을 말한다). 즉 B안·C안이 방어하려던 "다른 창이 쓰는 동안 트랜스크립트가 분기된다"는 위험 자체가 없다.
2. **live 여부는 "보낼 수 있는가"를 바꾸지 않는다.** `--resume`은 이미 끝난 세션에도 정상 동작한다. 그리고 실행 중이라는 사실 자체는 목록 행의 조용한 점(`running`)으로 이미 보이므로(§1-F-4), 상세 화면에 같은 사실을 경고 문구로 한 번 더 띄울 이유가 없다. 행동을 바꾸지 못하는 경고는 화면 잡음일 뿐이다.

그래서 `SessionTranscript.live` 필드와 `is_session_live()` 계산 로직, 화면 경고 배너는 **전부 두지 않는다**(§4-1/§4-3/§8). 남아 있는 실제 위험은 B5(락 없음)에서 온 동시 쓰기뿐이고, 그건 §5-S8(세션당 1개·전역 3개)이 앱 안에서 막는다. 앱 밖의 IDE 창과 동시에 쓰는 경우는 §7의 "다른 창이 동시에 같은 파일에 append" 행이 담당한다.

**전제(오래된 세션 재개)**: 목록에는 지금 살아있는 세션만이 아니라 **최근 30일 안에 활동한 과거 세션이 함께 뜬다**(§1-F). 한 달 전 세션에 이어쓰면 `--resume`이 그때의 컨텍스트를 그대로 되살린다. **사용자는 이 위험을 명시적으로 인지한 상태에서 "경고 없이 그대로 재개 허용"을 선택했다(사람 승인 완료)** — 그래서 재개 전 확인 대화상자나 "오래된 세션" 경고를 두지 않는다. 판단 근거는 두 가지다: (a) 50인 미만 사내에서 쓰는 로컬 데스크톱 도구다 (b) 재개 대상의 **제목과 마지막 활동 시각(`updatedAt`)이 목록 행에 이미 표시**되므로, 무엇을 재개하는지 판단할 근거가 이미 화면에 있다.

| 안 | 내용 | 얻는 것 | 잃는 것 |
|---|---|---|---|
| **A (확정)** | 라이브 여부를 보지 않고 `--resume`으로 이어쓴다. 상세 화면에 배너 없음 | 목표판정 그대로 충족. 구현·화면 모두 최소. 끝난 세션 재개도 같은 경로로 자연히 동작 | 실제 IDE 세션(`entrypoint:"claude-vscode"`)을 대상으로 한 resume은 여전히 **미검증**(§1-C). 오래된 세션을 되살리는 것도 막지 않는다(위 "전제") |
| B | live면 전송을 막고 읽기 전용으로 둔다 | — (막으려던 분기 위험이 실재하지 않으므로 얻는 것이 없다) | 다른 창에서 열려 있는 세션 — 곧 지금 가장 이어쓰고 싶은 세션 — 이 정확히 그 이유로 읽기 전용이 된다 |
| C | live면 `--fork-session`으로 사본을 떠서 새 세션에서 이어간다 | 원본과 물리적으로 분리 | 세션 id가 바뀌어 "그 세션에 이어졌다"가 아니게 됨. 사본이 새 jsonl을 만들어 목록에 원본과 두 줄로 늘어난다. 원본 무손상이라는 이점도 A가 이미 확보하고 있다 |

구현 위치: `src-tauri/src/session_chat.rs`의 `send_session_message`(하드 게이트 없음), `src/views/sessions.ts`의 입력 영역(항상 활성).

### ② 도구 실행 권한 모드 — **확정: A안**

**결정(사람 승인 완료): A안 — `--permission-prompts none`만 사용, `--tools ""` 제거.** 터미널에서 `claude`를 직접 칠 때와 동일한 권한이며, 사용자의 permission-mode 설정을 그대로 따른다. 구현: `src-tauri/src/session_chat.rs`의 `TOOL_PERMISSION_ARGS`.



B14에서 `--permission-prompts none`을 줘도 **자동 모드가 안전하다고 판단한 Bash는 프롬프트 없이 실행**됐다(`ls -1`). 즉 GUI 채팅에서 보낸 한 줄이 그 프로젝트 디렉터리에서 실제 명령을 실행할 수 있다.

| 안 | 플래그 | 성격 |
|---|---|---|
| **A (기본 권고)** | `--permission-prompts none` 만 (permission-mode는 사용자 설정 그대로) | 터미널에서 치는 것과 동일한 권한. 프롬프트가 필요한 행동은 자동 거부되어 GUI가 멈추지 않는다 |
| B | `--permission-prompts none --permission-mode plan` | 파일 변경·명령 실행 없이 계획만. 매우 안전하지만 "대화를 이어간다"는 느낌이 크게 달라짐 |
| C | `--permission-prompts none --tools ""` (도구 전부 비활성) | 순수 대화만. 가장 안전하지만 코딩 세션 이어쓰기로는 쓸모가 줄어듦 |

**권고: A.** 단 이건 "GUI 입력창 한 줄 = 로컬 파일 변경 가능"을 뜻하므로 사람 승인 대상이다.

### ③ 취소 시 프로세스 그룹 kill — **확정: A안**

`process_group(0)` + `kill(-pid)`는 claude가 낳은 손자 프로세스(사용자 도구가 띄운 빌드·테스트 등)까지 함께 죽인다. `dev_tools.rs`는 이미 이 방식을 쓴다.

- **A (기본 권고)**: 그룹 kill — 좀비/고아 프로세스가 남지 않는다. 대가: 진행 중이던 사용자 빌드도 함께 죽는다.
- B: 직속 자식만 kill — 손자가 계속 돌 수 있다(파이프가 끊겨 결과는 버려지는데 CPU만 쓰는 최악).

**결정: A** (기존 코드와 일관). Windows는 `Child::kill()`만(기존 `dev_tools.rs:1021-1026`의 제약과 판단을 그대로 승계).

---

## 7. 비정상 케이스 (③ 의무)

| 케이스 | 동작 |
|---|---|
| 트랜스크립트 파일 없음 | `read_session_transcript`가 에러 문자열 반환 → 화면은 "대화 기록 없음" 상태 블록. 입력창은 숨긴다 |
| 파일 중간에 깨진 JSON 줄 | 그 줄만 건너뛴다(`lib.rs:94-96`과 동일 규율). 실측 20,148줄 중 파싱 실패 0이지만 방어 |
| 매우 큰 트랜스크립트 | 400개 상한 + 8000자 컷 + 스트리밍 읽기(§3-4). `truncated=true`면 상단에 `이전 대화 일부는 표시하지 않습니다` |
| claude 미설치 | spawn 전에 감지해 즉시 Err (이벤트 없이 invoke 실패) |
| 중복 전송(연타) | 세션당 1개 제한으로 두 번째 invoke가 즉시 Err. 프론트도 `turnId != null`이면 버튼 비활성 |
| 턴 도중 앱이 화면을 떠남 | 프로세스는 계속 돈다. 돌아오면 `read_session_transcript`가 완료분을 보여준다. 리스너는 화면 이탈 시 해제하고 재진입 시 다시 건다 |
| 취소 | 그룹 kill → `done{ok:false, canceled:true}`. B12대로 **보낸 프롬프트는 파일에 남고 응답은 없다** → 재조회 시 마지막이 user 말풍선. 화면에 `중단됨` 표시 |
| 중단 후 다음 전송 | B13대로 모델이 중단된 프롬프트까지 함께 처리할 수 있다. 입력창 위에 한 줄 안내: `직전에 중단한 요청이 다음 응답에 함께 반영될 수 있습니다` |
| rate limit / 과금 한도 | `result.is_error` 또는 rc!=0로 흘러 `done.error`에 원문 노출(사용자가 그대로 읽고 판단) |
| 다른 창이 동시에 같은 파일에 append | done 후 전체 재조회라 **파일 순서 그대로** 화면에 반영된다. parentUuid 트리 재구성은 하지 않는다(MVP — 파일 순서 렌더가 가장 단순하고, 갈라진 것도 다 보여준다) |
| 앱 종료 중 진행 턴 | `ExitRequested` 훅이 `session_chat::request_shutdown()`을 호출해 `ACTIVE_TURNS`에 남은 프로세스 그룹을 명시적으로 정리한다(그냥 두면 `process_group(0)`으로 분리된 자식은 앱과 함께 죽지 않는다). 파일은 append-only라 손상 없음 |
| stdout 폭주(대량 델타) | 리더 스레드가 계속 소비하므로 교착 없음. 프론트는 델타를 문자열에 이어붙이기만 하고 렌더는 브라우저 프레임에 맡긴다 |

---

## 8. 화면 (frontend-dev용) — 기존 상세 뷰에서 바뀌는 부분만

기준 파일: `src/views/sessions.ts:150-198` (`renderSessionDetailView`). 아래 와이어프레임은 **그 함수의 실제 마크업을 옮겨 적은 것**이다.

```
┌───────────────────────────────────────────────────────────────┐
│ ← 세션목록                                    (back-link 그대로)│
│ {sessionTitle(session)}                       (chat-title 그대로)│
│ malgn-vscode · v2.1.263 · interactive · 시작 … · 갱신 …        │  ← kind는 registry 유래라 jsonl 행에서는 `-`
│                                               (chat-meta-row 그대로)│
├───────────────────────────────────────────────────────────────┤
│ (chat-thread — 기존 클래스 그대로, 내용만 실제 데이터로 교체)   │
│  [나] 세션목록에서 세션을 선택하면 …                            │
│  [AI] 네, 확인했습니다. …                                      │
│       ⚙ 도구 12회 실행                                         │  ← 신규 tool 줄
│  [AI] 정리하면 …                                               │
├───────────────────────────────────────────────────────────────┤
│ ┌───────────────────────────────────────────┐  ┌──────────┐   │  ← 신규 입력 영역
│ │ 메시지를 입력하세요 (Enter 전송 / Shift+Enter 줄바꿈)│  │  전송 ▶  │   │
│ └───────────────────────────────────────────┘  └──────────┘   │
│ 직전에 중단한 요청이 다음 응답에 함께 반영될 수 있습니다 (해당 시)│
├───────────────────────────────────────────────────────────────┤
│ 세션 메타데이터 (실제 데이터)          (chat-meta-panel 그대로)  │
│  sessionId  …                                                 │
└───────────────────────────────────────────────────────────────┘
```

**삭제 대상**: `SAMPLE_FOLLOWUP_TURNS` 상수(`sessions.ts:144-148`)와 `chat-sample-note` 줄(`sessions.ts:183`) — 목업 표기가 더 이상 사실이 아니게 된다. 파일 상단 주석(`sessions.ts:1-5`, "대화 전문(jsonl)은 읽지 않는다")도 함께 고쳐야 한다.
**두지 않음**: 라이브 세션 경고 배너 — 근거는 §6-①. 상세 화면에 live를 나타내는 요소가 없다.
**유지**: `chatMessage()` 헬퍼(`sessions.ts:193-198`)와 CSS 클래스. `tool` 종류만 새 클래스(`chat-tool-line`)로 추가.
**전송 중**: 마지막에 스트리밍용 assistant 말풍선 1개(`streamingText`)를 붙이고, 버튼은 `취소 ■`로 바뀐다.

---

## 9. 트레이드오프 기록 (① 의무)

| 결정 | 대안 | 고른 이유 | 포기한 것 | 감당 방안 |
|---|---|---|---|---|
| `-p --resume` 1회 실행 = 1턴 | `--input-format stream-json`으로 프로세스를 계속 살려 여러 턴 주고받기 | 상태가 없다(프로세스가 곧 턴). 크래시·좀비·재연결 로직이 통째로 사라진다. 실측으로 동작 확인 완료 | 턴마다 프로세스 기동 비용(첫 델타까지 실측 5.5초) | 사용자에게 "생성 중" 표시. 대화형 세션도 첫 응답은 비슷하게 걸린다 |
| 프롬프트 stdin 전달 | argv 위치인자 + `--` 구분자 | B7에서 argv 경로의 플래그 해석을 실증. `--` 지원 여부에 기대는 것보다 stdin이 확실하고(B8 실증) 길이 제한도 없다 | 없음(코드량 동일) | — |
| done 후 전체 재조회 | 스트리밍 버퍼를 그대로 최종 메시지로 승격 | 화면의 진실을 파일 하나로 통일. 중단·동시기록·도구 접기가 자동 반영 | 재조회 1회 비용 | 400개 상한이라 밀리초 수준 |
| 파일 순서 렌더 | parentUuid 트리 재구성 후 최신 분기만 렌더 | MVP. 우리 `--resume`은 원본 jsonl에 그대로 append하므로 분기를 만들지 않는다(§6-①). 분기 가능성은 다른 창이 동시에 쓸 때뿐이고(미검증 — §1-C), 그때도 전부 보여주는 편이 덜 놀랍다 | 분기 시 시간순으로 섞여 보임 | 실제로 분기가 문제되면 그때 트리 재구성 |
| 도구 3개 이상 접기 | 도구별 상세(인자·결과 전문) 표시 | 실측상 도구 줄이 표시 대상의 과반 — 안 접으면 화면이 로그가 된다 | 무엇을 했는지 세부를 못 봄 | 세부가 필요하면 터미널/IDE에서 본다(이 앱의 역할이 아님) |
| 하드 타임아웃 없음 | N분 타임아웃 | 정상 코딩 턴이 수십 분 걸린다 | 사용자가 취소를 안 누르면 계속 돈다 | 취소 버튼 + 세션당 1개 제한 |
| 이벤트 2종(에러를 done에 포함) | 델타/완료/에러 3종 | 종료 경로를 하나로 강제해 "입력창이 영영 잠기는" 버그를 구조적으로 막는다 | payload가 조금 두꺼움 | — |
| 목록 행을 **jsonl(base)**에서 만들고 registry는 `running` **오버레이**로 강등 | registry 단독으로 목록을 구성 | registry는 "지금 실행 중"만 알고 과거 대화를 모른다. 세션목록이 실행 중 프로세스 목록과 같아지는 제약이 사라지고, `updatedAt`(마지막 활동 시각)이 처음으로 채워진다 | 매 조회마다 jsonl 디렉터리 스캔 비용. 30일·100건 상한 밖의 세션은 안 보인다 | 전량 `stat` 후 mtime 내림차순으로 자르고, head 파싱은 상한을 통과한 파일에만 한다(§1-F-1). 상한은 상수 두 개라 조정이 한 자리다 |
| jsonl 후보를 **depth-1만** 수집 | `projects/**` 재귀 수집 | 재귀하면 서브에이전트 트랜스크립트가 전부 세션 행이 된다 — 실측 1,473개 중 1,039개가 그것이다 | `<sid>/subagents/` 안의 대화는 목록에서 볼 수 없음 | 서브에이전트는 애초에 화면 표시 대상이 아니다(§3 R5의 `Task` 한 줄로 충분) |
| live 집합에서 우리 자식을 **앱이 들고 있는 pid 집합**(`ACTIVE_TURNS`)으로 걸러냄 | registry의 `entrypoint` 값으로 구분 | `entrypoint`는 env를 그대로 물려받아 자식도 `claude-vscode`가 된다(§1-E) — registry 내용만으로는 구분 자체가 불가능하다 | 앱이 `ACTIVE_TURNS`를 정확히 유지해야 하는 책임 | 등록/해제를 spawn과 턴 종료 한 쌍에서만 하고, 종료 경로를 `done` 하나로 모아둔다(§4-2) |
| 상세 화면에 라이브 경고를 두지 않음 | live 배너 유지 | 막으려던 분기 위험이 실재하지 않고(§6-①), 실행 중이라는 사실은 목록 행의 조용한 점으로 이미 보인다 | 상세 화면만 보고 있으면 "다른 창이 쓰는 중"을 알 수 없음 | 동시 쓰기는 S8(앱 내부)과 §7의 동시 append 행으로 다룬다 |
| 오래된 세션 재개에 경고·확인을 두지 않음 | 재개 전 확인 대화상자 | 사용자가 위험을 인지하고 명시적으로 선택했다(사람 승인 완료 — §6-①). 제목과 마지막 활동 시각이 목록에 이미 있어 판단 근거가 화면에 있다 | 한 달 전 컨텍스트를 의도치 않게 되살릴 수 있음 | 50인 미만 사내 로컬 도구라는 노출 범위. 목록이 `updatedAt` 내림차순이라 오래된 세션은 아래로 밀린다 |
| 신규 세션 cwd를 프론트에서 받고 **재스캔 매치**로 검증 | 프론트발 경로를 아예 받지 않는 S4 원형 유지 | 신규 세션은 읽을 트랜스크립트가 없어 cwd 출처가 없다. 허용 집합이 이미 `list_workspace_projects`로 공개된 것과 같아 신뢰 경계가 늘지 않는다 | 프론트발 경로 입력이라는 표면이 생김 | 재스캔 완전 일치(TOCTOU 방지) + spawn 직전 `is_dir()` 재확인(§5-1) |

**프로젝트 고유성(② 의무)**: 이 설계에서 범용 채팅 UI가 아닌 부분은 세 곳이다 — (1) 세션목록을 **대화 이력(jsonl) base + 실행 중 registry overlay** 두 층으로 합치는 §1-F의 구성. 그 base에서 서브에이전트 트랜스크립트(이 머신 기준 전체 jsonl의 약 71%)를 depth-1 제약으로 걷어내야 한다는 것, 그리고 overlay 쪽 registry가 우리 자식을 내용으로 구분해 주지 않아 pid 집합으로 걸러야 한다는 것 모두 이 제품 고유의 데이터 성격에서 역산한 결과다, (2) 실측 분포(도구 줄 과반)에서 역산한 §3-3 접기 규칙, (3) GUI가 launchctl PATH만 상속한다는 이 앱이 이미 겪은 제약(`cli_launcher.rs:1-11`)을 재사용한 §5-S3. PRD가 없어 비교표 인용은 불가하며(이 저장소는 `docs/` 유실 상태 — STATUS.md 참조), 그 사실을 근거로 억지 차별화를 넣지 않았다.

---

## 10. 병렬 구현 분담과 수용 기준

### backend-dev (`src-tauri/src/session_chat.rs` 신규 + `lib.rs` 2줄)
1. `read_session_transcript` — §3 규칙 전부. **단위 테스트 필수**: (a) tool_result가 말풍선을 만들지 않는다 (b) 같은 message.id의 assistant 여러 줄이 하나로 합쳐진다 (c) `origin.kind="task-notification"` user 줄이 걸러진다 (d) origin 없는 user 줄은 살아남는다 (e) 연속 3개 이상 tool이 하나로 접힌다 (f) 400개 상한에서 최신이 남는다.
2. `send_session_message` — §5 전부(특히 **S2: 프롬프트는 stdin**), §4-2 이벤트, S8 동시 제한.
3. `cancel_session_turn` — S9.
4. 회귀: 기존 `cargo test` 전부 통과.

### frontend-dev (`src/sessionsApi.ts` 확장 + `src/state.ts` + `src/views/sessions.ts`)
1. §4-3 타입/래퍼를 그대로 추가(백엔드 완성 전에도 타입만으로 화면을 짤 수 있다).
2. 상세 뷰 교체(§8) — 목업 상수·문구 제거, 입력창, 취소, 스트리밍 말풍선(상세 화면에 live 배지·경고 배너는 만들지 않는다 — §6-①. 목록 행의 `running` 점은 별개다 — §1-F-4).
3. 화면 이탈 시 `UnlistenFn` 해제.
4. `tsc --noEmit` 클린.

### 통합 수용 기준 (claimed ≠ verified — 실제로 눌러서 확인)
- [ ] 세션 상세에 **샘플이 아닌 실제 대화**가 보인다(기존 목업 문구가 화면 어디에도 없다).
- [ ] 도구가 많은 세션에서 화면이 로그로 도배되지 않는다(접힘 확인).
- [ ] 입력창에 한 줄 보내면 토큰 단위로 응답이 흐르고, 끝나면 그 메시지가 jsonl 재조회 결과에도 들어 있다.
- [ ] 입력창에 `--help`만 쳐서 보내도 **CLI 도움말이 아니라 대화 응답**이 온다(S2 회귀 테스트).
- [ ] 턴이 진행 중일 때 **세션목록에 같은 세션이 두 줄로 보이지 않는다**(행은 jsonl 파일 하나당 하나다). 턴을 취소한 뒤에도 그 행은 그대로 남고 **`running` 점만 꺼진다**(unix 기준. Windows는 §1-F의 배지 오탐 갭 적용).
- [ ] 세션목록이 **`updatedAt` 내림차순**으로 정렬되고, 각 행의 `갱신` 값이 `-`가 아니라 실제 시각으로 보인다.
- [ ] 세션목록에 서브에이전트 트랜스크립트에서 만들어진 유령 행이 없다(§1-F-1 depth-1 제약).
- [ ] 취소 버튼을 누르면 몇 초 안에 멈추고 `ps`에 claude 자식이 남지 않는다.
- [ ] `capabilities/default.json`이 변경되지 않았다.

---

## 부록 — 검증에 쓴 명령 (재현용)

```bash
claude --version                     # 2.1.266 (Claude Code)
claude --help                        # 플래그 목록(B1,B2 근거)
# 실제 왕복(임시 디렉터리에서만)
cd /tmp/<scratch> && claude -p "1+1?" --output-format stream-json --verbose \
  --session-id <uuid> --permission-prompts none
# resume + 델타
claude -p "그럼 3을 더하면?" --resume <uuid> --output-format stream-json --verbose \
  --include-partial-messages --permission-prompts none < /dev/null
# 최종 설계 형태(프롬프트는 stdin)
printf '%s' "한국어로 짧게 세 문장" | claude -p --resume <uuid> \
  --output-format stream-json --verbose --include-partial-messages --permission-prompts none
# 라이브 세션 registry 성격 확인
claude agents --json ; ls ~/.claude/sessions/*.json
```

테스트로 만든 임시 세션·background 세션·가짜 registry 항목은 모두 정리했다(`claude stop`/`claude rm` 실행, 생성한 json 삭제 확인, 남은 background agent 0개).
