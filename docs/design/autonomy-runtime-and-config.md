# 자율업무 Runtime 단순화 + 전역 설정 파일 + OTel 저장 — 설계

대상: `malgn-vscode`(Tauri 데스크톱 앱). 등급: Refactor.
목적 3가지 — ① 자율업무 실행 상태를 디스크에서 걷어내고(설정=파일 / 상태=메모리 / 이력=로그), ② 전역 설정 파일 `~/.claude/malgn-agent.json`을 신설하고, ③ OTel 설정 화면을 실제 저장 가능하게 만든다.

## 0. 전 담당자 공통 금지·제약 (구현 전 반드시 읽을 것)

| 금지 | 내용 |
|---|---|
| 권한 우회 | `--dangerously-skip-permissions` 등 어떤 권한 우회 플래그도 추가 금지. 무인 실행도 대상 프로젝트의 기존 `.claude/settings.json` 권한 범위 안에서만 돈다. |
| shell 경유 | `sh -c`/`cmd /c` 금지. `std::process::Command::new(binary)` + argv 배열 직접 실행만. |
| 의존성 | 새 crate 추가 금지(이번 설계는 새 crate 0개로 성립한다). async 런타임 전면 도입 금지. queue/workflow engine/distributed scheduler 금지. |
| 구조 | 대규모 계층 리팩터링(Repository/Service/Adapter) 금지. 이번 작업과 무관한 파일 수정 금지. |
| 시크릿 | 이 저장소는 **public**이다. 사내 collector IP/호스트를 `src/`·`src-tauri/src/`에 리터럴로 넣는 것 금지(§6). |
| uncommitted 파일 | `src-tauri/src/lib.rs`, `src/state.ts`, `src/views/settings.ts`는 MCP 작업 중인 미커밋 변경이 있다 → **전체 재작성(Write) 금지, 국소 Edit만**. `src-tauri/src/mcp_manager.rs`, `src/mcpApi.ts`, `src/main.ts`는 **절대 손대지 않는다**(이 설계는 이 3개를 한 줄도 건드리지 않고 성립하도록 짰다). |
| 기타 | `STATUS.md` 수정 금지. git commit/push 금지. |

범위 밖(이번에 하지 않는다): 자율업무 "지금 실행" 버튼, `malgn-agent.json` 쓰기 UI(읽기 전용), 로그 뷰어 화면, OS 스케줄러(launchd/schtasks) 연동, 로그 인덱싱/DB.

---

## 1. 결정 1 — Runtime 실행 모델

**선택: 단일 tick 스케줄러(10초) + 메모리 상태머신.** 스레드는 "스케줄러 스레드 1개 + 실행 중 task마다 일회성 워커 스레드"만 존재한다.

판정식(매 tick, 상태 Mutex 보유 상태에서):

```
due(task) = task.enabled
          && !rt.running
          && now >= rt.next_run_at            // 미등록 task는 등록 시 next_run_at = now + STARTUP_GRACE
```
due 후보를 `next_run_at` 오름차순(가장 오래 밀린 것 우선)으로 정렬 → 앞에서부터 `running_count < concurrency`인 동안만 `running=true`로 마킹하고 워커 스레드를 spawn. 워커가 끝나면 `running=false; last_finished_at=now; next_run_at = now + interval`.

- **동일 task 직렬**: `!rt.running` 한 줄로 보장(구조적 — `preventOverlap` 옵션 자체가 불필요해져 삭제한다).
- **interval = 완료 후 대기**: `next_run_at`을 *완료 시점*에만 계산하므로 자동으로 성립.
- **concurrency 게이트**: 별도 동기화 원시타입 없이 **상태 Mutex를 쥔 채 `running==true` 개수를 세는 것**으로 끝. 세마포어/Condvar를 쓰지 않는다 — 게이트 판정 지점이 tick 안 한 곳뿐이라 경쟁이 존재하지 않는다.
- **설정 리로드 시점 = 매 tick**. tick마다 workspace를 훑어 `autonomy.json`을 다시 읽고 런타임 레지스트리를 reconcile한다. 그래서 추가/삭제/토글에 **최대 10초 안에** 반응하고, 이를 위한 코드가 따로 없다(리로드가 곧 tick 본체다).
  - 추가: 새 키 등록 → `next_run_at = now + STARTUP_GRACE`.
  - 삭제: 설정에 없어진 키는 `running==false`일 때만 레지스트리에서 제거(실행 중이면 끝난 뒤 다음 tick에 제거).
  - **실행 중 disable/삭제는 현재 실행을 중단시키지 않는다** — 다음 회차가 스케줄되지 않을 뿐이다(작업 도중 kill하면 프로젝트가 반쯤 편집된 상태로 남는다).
  - 실행 중 prompt/interval 편집: 이번 실행은 **spawn 시점 스냅숏**으로 끝까지 진행(현행 `DueTaskSnapshot` 규율 유지), 새 값은 다음 회차부터.
- **앱 시작 유예(STARTUP_GRACE = 3분)** — 상태를 영속화하지 않으므로 재시작할 때마다 모든 enabled task가 "한 번도 안 돈 상태"가 된다. 유예가 없으면 **앱을 켤 때마다 즉시 전체 task가 `claude -p`로 발사되어 토큰이 탄다**. 신규 등록 task도 같은 규칙(등록 후 3분 뒤 첫 실행)을 쓴다 — 규칙이 하나뿐이라 UI가 설명하기 쉽고, 어차피 `nextRunAt`을 백엔드가 내려주므로 사용자는 정확한 시각을 본다.
- **앱 종료 시 자식 프로세스 정리**(고아 방지):
  1. 전역 `static SHUTTING_DOWN: AtomicBool`.
  2. 워커는 `run_process_with_timeout_cancellable(..., should_abort: &|| SHUTTING_DOWN.load(Relaxed))`로 실행한다. 이 함수의 기존 100ms `try_wait()` 폴링 루프가 매 회차 `should_abort`를 확인해, true면 타임아웃과 **똑같은 경로**(`force_kill_process_group`: unix=프로세스그룹 SIGTERM→3초→SIGKILL / windows=`child.kill()`)로 죽인다. 이미 검증된 kill 코드를 재사용하고 unix·windows 양쪽이 함께 커버된다.
  3. `lib.rs`의 `run()` 말미를 `builder.build(ctx)?.run(|_h, event| ...)` 형태로 바꿔 `RunEvent::ExitRequested`에서 플래그를 세우고, 레지스트리의 `running_count`가 0이 될 때까지 **최대 2초** 100ms 폴링한 뒤 그대로 종료한다(2초 이상 앱 종료를 붙잡지 않는다).
  4. 2초 후에도 남아 있으면 unix 한정 보루로 레지스트리에 보관 중인 `child_pid`들에 `libc::kill(-pid, SIGKILL)`을 직접 쏜다. **Windows는 잔여 손자 프로세스가 남을 수 있다는 제약을 감수한다** — `dev_tools.rs`가 이미 문서화한 동일 제약(Job Object는 과설계)과 일관되게 간다.

**버린 대안 A — task당 스레드 루프(`{permit → run → sleep(interval)}`)**: "직렬 + 완료기준 interval"은 스레드 하나로 공짜지만, ① 설정 변경 반영을 위해 스레드 생명주기(spawn/cancel/join)와 중단 가능한 sleep(1초 슬라이스 폴링)을 직접 관리해야 하고 ② concurrency에 진짜 세마포어(카운터+Condvar 자작)가 필요해진다. 포기한 것: 이론적으로 더 정확한 타이밍(tick 모델은 최대 10초 드리프트). 감당: interval 최소 단위가 5분이라 10초 드리프트는 무의미하고, `nextRunAt`을 UI가 그대로 표시하므로 사용자에게 거짓말하지 않는다.

**버린 대안 B — 현행 60초 tick 유지**: 변경량은 가장 적지만 UI 반영 지연이 최대 60초라 토글이 "먹통처럼" 보인다. 10초 tick의 비용은 프로젝트 수십 개의 `read_dir`+작은 JSON 읽기(수 ms)로 무시 가능하다. 포기한 것: 극소량의 유휴 CPU/디스크 I/O.

**안전 임계값 정본** — 아래 상수는 `src-tauri/src/autonomy/config.rs` 한 곳에만 존재하고, UI는 절대 복제하지 않고 `malgn_agent_config_get()`의 `limits`로 받아 표시한다(값을 바꿀 때 고칠 파일이 1개가 되도록).

```rust
pub(crate) const TICK_SECONDS: u64 = 10;
pub(crate) const STARTUP_GRACE_MINUTES: u32 = 3;
pub(crate) const MIN_INTERVAL_MINUTES: u32 = 5;
pub(crate) const MAX_INTERVAL_MINUTES: u32 = 10_080;   // 7일
pub(crate) const DEFAULT_TIMEOUT_MINUTES: u32 = 60;
pub(crate) const MIN_TIMEOUT_MINUTES: u32 = 1;
pub(crate) const MAX_TIMEOUT_MINUTES: u32 = 480;       // 8시간
pub(crate) const DEFAULT_CONCURRENCY: u32 = 3;
pub(crate) const MAX_CONCURRENCY: u32 = 8;
pub(crate) const DEFAULT_LOG_RETENTION_DAYS: u32 = 30;
pub(crate) const MAX_LOG_RETENTION_DAYS: u32 = 365;
```

타임아웃 결정: `effective_timeout_minutes(task) = clamp(task.timeout ?? cfg.autonomy.defaultTimeout ?? DEFAULT_TIMEOUT_MINUTES, MIN, MAX)` — 함수 하나(`autonomy/config.rs`)에만 존재.

---

## 2. 결정 2 — 메모리 Runtime 상태와 프론트 노출

**선택: `static RUNTIME: Mutex<BTreeMap<TaskKey, TaskRuntime>>`** (`TaskKey = (String /*projectPath*/, String /*taskId*/)`).

`BTreeMap`을 고른 실무적 이유: `HashMap::new()`는 const fn이 아니라 `static` 초기화에 `OnceLock`/`LazyLock` 래퍼가 추가로 필요하지만 `BTreeMap::new()`는 const라 `static ... = Mutex::new(BTreeMap::new());` 한 줄로 끝나고, 덤으로 상태 목록 정렬 순서가 결정적이다. task 수는 수십 개라 성능 차이는 없다.

```rust
pub(crate) struct TaskRuntime {
    pub running: bool,
    pub child_pid: Option<u32>,          // 종료 시 SIGKILL 보루용. 프론트에 노출하지 않는다.
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub status: Option<RunStatus>,       // Success | Failed | Timeout
    pub summary: Option<String>,         // stdout/stderr 꼬리 500자
    pub duration_ms: Option<u64>,
    pub log_path: Option<String>,
}
```

**프론트 노출 커맨드(이 시그니처/필드명을 그대로 쓴다):**

```rust
#[tauri::command]
pub fn autonomy_runtime_status() -> Vec<AutonomyRuntimeStatus>
```

```jsonc
// AutonomyRuntimeStatus — 전부 camelCase. 시각은 RFC3339(UTC, 예 "2026-09-10T05:12:33.412Z")
{
  "projectPath": "/Users/x/workspace/foo",
  "taskId": "t-1",
  "running": false,
  "lastStartedAt": "2026-09-10T05:00:00Z",   // null 가능
  "lastFinishedAt": "2026-09-10T05:03:21Z",  // null 가능
  "nextRunAt": "2026-09-10T05:33:21Z",       // null 가능(enabled=false면 null)
  "status": "success",                        // "success" | "failed" | "timeout" | null
  "summary": "…최대 500자…",                  // null 가능
  "durationMs": 201000,                       // null 가능
  "logPath": "/Users/x/workspace/foo/.claude/logs/autonomy/2026-09-10/t-1-140000.log" // null 가능
}
```

**이벤트: 기존 이름 `autonomy-task-updated`를 유지하되 payload를 위 `AutonomyRuntimeStatus` 1건으로 바꾼다.** 실행 시작 시 1회, 종료 시 1회 emit. 현재 프론트에 이 이벤트 리스너가 **한 곳도 없으므로**(`grep` 확인) 이름을 바꿔 얻을 것이 없고, 이름을 유지하면 Rust 쪽 emit 지점 이외에 바꿀 것이 없다.
프론트는 이벤트로 해당 행만 패치하고, `listen()` 구독이 실패하는 브라우저 폴백 환경에서는 30초 폴링으로 자연 열화한다(`sessionsApi.ts`가 이미 쓰는 패턴).

**버린 대안 — 상태를 `autonomy_list()` 응답에 합쳐 한 번에 내려주기**: 프론트가 호출을 1회로 줄일 수 있지만, "설정 파일의 값"과 "메모리 상태"가 한 객체에 섞이면 다시 상태를 파일에 저장하고 싶은 유혹(=지금 걷어내는 바로 그 구조)이 생긴다. 커맨드를 갈라 두면 경계가 타입으로 강제된다. 포기한 것: IPC 왕복 1회. 감당: 둘 다 밀리초 단위 로컬 호출이고, 프론트는 화면 진입 시 `Promise.all`로 병렬 호출한다.

---

## 3. 결정 3 — `autonomy.json` 스키마와 마이그레이션

**선택: 자연 마이그레이션(구조체에서 필드를 지우고 `serde`의 기본 동작인 "모르는 필드 무시"에 맡긴다) + `interval`은 `alias`로 양쪽 수용, 쓰기는 신규 이름으로만.**

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AutonomyTaskConfig {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<String>,
    #[serde(alias = "intervalMinutes")]
    pub interval: u32,                      // 분. 이전 실행 "완료" 후 대기 시간.
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,               // 분. 미지정이면 전역 defaultTimeout.
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AutonomyFile { pub version: u32, pub tasks: Vec<AutonomyTaskConfig> }   // version: 1 유지
```

- 제거 필드 `lastRunAt / lastStatus / lastSummary / history / preventOverlap`: 구조체에 없으므로 읽을 때 조용히 버려지고, **다음 저장(추가·수정·삭제·토글 중 아무거나) 시점에 파일에서 사라진다.** 별도 마이그레이션 코드·플래그 없음. `#[serde(deny_unknown_fields)]`를 절대 붙이지 말 것(붙이면 레거시 파일이 통째로 파싱 실패한다).
- 쓰기 이름은 `interval` 하나뿐(`skip_serializing`으로 alias는 나가지 않는다).
- **`version`은 1을 유지한다.** 이번 변화는 "읽기 하위호환"이므로 번호를 올려도 사는 사람이 없다. 감수하는 리스크: 신규 파일(`interval`만 있음)을 **구버전 앱**이 읽으면 `intervalMinutes` 부재로 파싱 실패 → `unwrap_or_default()`로 빈 목록이 되어 자율업무가 조용히 사라져 보인다. 사내 배포 앱을 통째로 교체하는 운영 방식이라 다운그레이드 시나리오를 실질 위험으로 보지 않는다(다만 파일 읽기 실패 시 빈 목록 대신 오류를 남기도록 §5 방식으로 로그를 남긴다).
- **`MIN_INTERVAL_MINUTES = 5` clamp는 유지하고 오히려 더 중요해졌다** — interval이 완료 기준으로 바뀌어 5분은 "직전 실행이 끝난 뒤 5분"이라 사실상 연속 실행에 가깝다. `MAX_INTERVAL_MINUTES`(7일) 상한도 새로 둔다(오타 방어). 정규화는 `normalize_task()` 한 함수에 모으고 **읽기 직후와 upsert 양쪽에서 호출**한다(사람이 손으로 편집한 파일도 방어).
- `id` 정규화: `[A-Za-z0-9._-]` 이외 문자를 `_`로 치환한 값을 **로그 파일명에만** 쓴다(§9 — 경로 탈출 방어). 설정 안의 `id` 자체는 그대로 둔다.

**버린 대안 — 명시적 마이그레이션 함수(`version: 1 → 2` 변환기)**: 이력을 로그로 옮겨 담는 등의 "성실한" 마이그레이션이 가능하지만, 옮길 대상(history 최대 10건의 `{at, result}`)이 사실상 가치가 없고 변환기·버전 분기·테스트가 통째로 늘어난다. 포기한 것: 과거 실행 이력 10건. 감당: 사용자 요구가 "과거 실행은 로그"이므로 새 로그가 쌓이는 순간 대체된다.

---

## 4. 결정 4 — `run_process_with_timeout` 재사용 방식

**선택: `dev_tools.rs`에 그대로 두고 가시성만 `pub(crate)`로 넓힌 뒤, 취소 가능 변형을 additive로 추가한다.** 공용 모듈(`process.rs`) 신설은 하지 않는다.

```rust
// src-tauri/src/dev_tools.rs — 기존 4개 호출부는 한 글자도 바뀌지 않는다.
pub(crate) struct ProcessRunOutput { /* 필드 전부 pub(crate)로 */ }

pub(crate) fn run_process_with_timeout(
    binary_path: &str, args: &[String], path_env: &str,
    extra_env: &[(&str, &str)], timeout: Duration,
) -> ProcessRunOutput {
    run_process_with_timeout_cancellable(binary_path, args, path_env, extra_env, timeout, None, None)
}

pub(crate) fn run_process_with_timeout_cancellable(
    binary_path: &str, args: &[String], path_env: &str,
    extra_env: &[(&str, &str)], timeout: Duration,
    on_spawn: Option<&dyn Fn(u32)>,          // spawn 직후 자식 pid 공표(레지스트리 등록)
    should_abort: Option<&dyn Fn() -> bool>, // wait 루프 100ms마다 확인 → true면 타임아웃과 동일 경로로 kill
) -> ProcessRunOutput
```

`wait_up_to()`의 폴링 루프에 `should_abort` 확인 한 줄, spawn 직후 `on_spawn` 호출 한 줄만 추가된다. 반환 시 `timed_out`과 별개로 `aborted: bool` 필드를 추가해 "타임아웃"과 "앱 종료로 중단"을 로그에서 구분한다.

- **근거**: 기존 호출부 4곳·테스트 3개가 전혀 바뀌지 않아 회귀 위험이 0에 수렴한다. 2,859줄 파일에서 함수 3개(+`ProcessRunOutput`, `wait_up_to`, `force_kill_process_group`)와 `use` 헤더를 들어내는 이동은 순수한 churn이며, 이번 기능과 무관한 파일을 크게 흔든다(§0 금지).
- **주의(backend-dev)**: `dev_tools.rs`의 `EXECUTION_LOCK`은 도구 설치를 직렬화하는 락이다. **자율업무는 이 락을 절대 취하지 않는다** — 취하면 전역 동시성이 1이 되어 concurrency 설계가 무너진다.
- **감수하는 것**: "dev_tools"라는 이름의 모듈이 범용 프로세스 유틸을 소유하는 어색함. 세 번째 소비자가 생기는 시점에 `process.rs`로 추출하고 `dev_tools`는 re-export만 남기는 방식으로 회귀 없이 정리한다(지금은 그 시점이 아니다).

---

## 5. 결정 5 — 전역 설정 파일과 `workspace_roots()` 일원화

**파일**: `~/.claude/malgn-agent.json`. Claude Code의 `~/.claude/settings.json`과 **합치지 않는다** — 서로 소유자가 다르다(전자는 이 앱, 후자는 Claude Code). `malgn-agent.json`에 OTel을 넣지 않고 `settings.json`에 workspaces/autonomy를 넣지 않는다.

```jsonc
{ "version": 1,
  "workspaces": ["~/workspace"],
  "autonomy": { "concurrency": 3, "defaultTimeout": 60 },
  "logs": { "retentionDays": 30 } }
```

로드 규칙: **파일 없음 → 코드 기본값(정상)** / **정상 JSON → 사용**(섹션 누락은 `#[serde(default)]`로 개별 기본값, 모르는 키는 무시) / **JSON 손상 → 오류**(조용한 기본값 폴백 금지).

**캐시**: `static CACHE: Mutex<Option<(Option<(i64 /*mtime_ms*/, u64 /*len*/)>, Result<UserConfig, String>)>>`. `load()`는 매번 `fs::metadata`만 확인(수 µs)하고 mtime+길이가 같으면 캐시를 그대로 쓴다. 스캔 루프 안에서 tick마다 불려도 안전하고, 사용자가 앱 실행 중 파일을 고치면 워처 없이도 다음 호출에 반영된다. (mtime 초 단위 해상도를 길이 비교로 보강한다.)

**손상 오류를 `Vec<PathBuf>` 시그니처에서 전파하는 방법 — 3층으로 나눈다:**

1. 정본 API는 오류를 그대로 가진다: `config::load() -> Result<UserConfig, String>`, `config::workspace_roots_checked() -> Result<Vec<PathBuf>, String>`.
2. `lib.rs`의 `pub(crate) fn workspace_roots() -> Vec<PathBuf>`는 **시그니처를 유지**하되 본문을 `config::workspace_roots_checked().unwrap_or_default()`로 바꾼다. 즉 손상 시 **빈 목록**을 반환한다 — `~/workspace` 하드코딩으로 되돌아가지 않는다. 빈 목록이면 스캔 결과가 0건이고 스케줄러는 아무것도 실행하지 않으며 `resolve_validated_project_root()`는 모든 경로를 거부한다. "조용히 기본값으로 넘어가면 안 된다"는 요구를 **안전 실패(fail-closed)** 로 만족시키고, 시그니처 변경 파급(호출부 4곳)을 0으로 유지한다.
3. **사용자에게 보이는 오류 표면**을 별도로 만든다:
   - `autonomy_list()`의 반환을 `Result<Vec<AutonomyProjectTasks>, String>`으로 바꿔 설정 오류를 그대로 던진다(자율업무 화면에 이미 error 상태가 있어 바로 배너가 뜬다).
   - `malgn_agent_config_get()`(신규)이 `ok/error/configPath/…`를 내려주고 설정 화면이 상시 표시한다.
   - 스케줄러 tick은 설정이 손상되면 **아무 일도 하지 않고** 오류 문자열이 직전과 달라졌을 때만 1회 `eprintln!`한다(로그 폭주 방지).

**`workspaces` 항목 검증(보안상 핵심)** — 이 목록은 곧 이 앱이 `.claude/autonomy.json`을 쓰고 파일 트리를 읽는 범위다. `~`로 시작하는 항목만 홈 기준으로 확장한다(`~user` 형태는 거부, 범용 환경변수 확장 없음). 이어서 다음을 **거부하고 warning 목록에 담는다**: ① 존재하지 않거나 디렉터리가 아닌 경로 ② 파일시스템 루트(`/`, `C:\`) ③ 홈 디렉터리 그 자체 ④ 정규화 후 중복 ⑤ 8개 초과분. ②③을 막지 않으면 홈 전체가 "프로젝트"가 되어 쓰기·읽기 표면이 통째로 넓어진다. 유효 항목이 0개면 `Ok(vec![])`(fail-closed)이며 warning이 UI에 뜬다.

**`resolve_validated_project_root()` 불변식**: 현행 구현은 각 루트를 `canonicalize()`한 뒤 후보가 **직속 1단계 자식**인지 검사하며, 루트 목록을 순회해 하나라도 맞으면 통과시킨다. 루트가 N개로 늘어도 검사는 루트별로 독립이므로 불변식이 그대로 유지된다(수정 불필요). 새로 생기는 위험은 "루트 목록 자체가 넓어지는 것"뿐이며 위 검증이 그것을 막는다. 심볼릭 링크는 `canonicalize()`가 이미 해석한다.

**버린 대안 — `OnceLock`으로 앱 시작 시 1회만 읽기**: 가장 단순하지만 사용자가 설정을 고쳤을 때 앱을 재시작해야 하고, 손상 상태를 복구해도 오류 배너가 남는다. 포기한 것: 호출당 `metadata()` 한 번. 감당: µs 단위로 무시 가능.
**버린 대안 — `workspace_roots()`를 `Result`로 바꾸기**: 가장 정직하지만 호출부 4곳(그중 2곳은 `lib.rs`의 미커밋 파일)과 그 상위 시그니처가 연쇄로 바뀐다. 포기한 것: 타입 수준의 오류 강제. 감당: 오류 표면을 커맨드 3곳에서 명시적으로 노출해 사용자가 반드시 보게 만든다.

---

## 6. 결정 6 — OTel 기본값의 출처 (public 저장소)

관리 대상 14개 키를 **(a) 소스 하드코딩 / (b) 빌드타임 주입 / (c) 사용자 입력** 으로 3분한다.

**(a) 소스에 하드코딩하는 범용 기본값** — 사내 정보가 전혀 없는 프로토콜·프라이버시 값:

| 키 | 기본값 | 비고 |
|---|---|---|
| `CLAUDE_CODE_ENABLE_TELEMETRY` | `1` | `OTEL_` 접두사가 아니다 — 현행 읽기 필터가 놓치던 키 |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` | |
| `OTEL_METRICS_EXPORTER` / `OTEL_LOGS_EXPORTER` | `otlp` / `otlp` | |
| `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE` | `cumulative` | |
| `OTEL_METRIC_EXPORT_INTERVAL` / `OTEL_LOGS_EXPORT_INTERVAL` | `60000` / `5000` | ms |
| `OTEL_LOG_USER_PROMPTS`, `OTEL_LOG_TOOL_CONTENT`, `OTEL_LOG_TOOL_DETAILS`, `OTEL_LOG_RAW_API_BODIES` | 전부 `0` | **읽기 전용 고정값**(아래) |

프라이버시 4개는 UI에서 값만 보여주고 **편집 불가**로 렌더한다(`readOnlyKeys`). 이 값을 1로 올리면 프롬프트 원문·툴 입출력이 collector로 나가므로, 사내 대시보드용 앱이 원클릭으로 켜줄 성격이 아니다. 필요하면 사용자가 `settings.json`을 직접 편집한다(이 앱은 그 값을 **덮어쓰지 않고 그대로 보존**한다).

**(b) `option_env!()`로 주입하는 값** — 사내 collector 주소. `build.rs`에 `MALGN_OTEL_COLLECTOR_BASE` 한 개를 통과시키고(`src-tauri/.env` 로컬 / GitHub Actions repo secret), Rust에서 `const OTEL_COLLECTOR_BASE: Option<&str> = option_env!("MALGN_OTEL_COLLECTOR_BASE");`로 받아 `{base}/v1/metrics`, `{base}/v1/logs`를 파생시킨다. 엔드포인트 두 개를 각각 주입하지 않고 base 하나만 주입하는 이유: OTLP 경로 규약이 표준이라 파생이 안전하고, 관리할 secret이 1개로 준다. 분리가 필요한 예외 상황은 이제 **UI에서 직접 입력해 저장**할 수 있다.
**사내 IP/호스트를 `src/`·`src-tauri/src/`·`build.rs`에 리터럴로 넣는 안은 채택 불가다.** 이 설계 문서에도 실제 주소를 적지 않는다(`docs/`는 gitignore되지만 습관을 만들지 않는다). 값의 출처는 PM 지시서/사내 공유 채널이다.

**(c) 주입값이 없을 때(포크·secret 없는 CI 빌드) UI 동작**: 엔드포인트 필드를 **빈 값 + placeholder `https://collector.example.com:4318`** 로 보여주고, 필드 아래에 "사내 collector 주소는 배포 빌드에 주입됩니다. 비어 있으면 직접 입력하세요."를 표시한다. 백엔드 응답의 `endpointDefaultsInjected: false`가 그 신호다. 저장 검증: `CLAUDE_CODE_ENABLE_TELEMETRY=1`인데 두 엔드포인트 중 하나라도 비었거나 `http(s)://`로 시작하지 않으면 **저장을 거부**한다(반쯤 켜진 상태로 저장되면 원인 모를 미수집이 된다).

**값 우선순위(폼 초기값)**: `settings.json`의 기존 값 → 주입/하드코딩 기본값 → 빈 값. 즉 이미 설정된 사용자의 값이 기본값에 절대 덮이지 않는다.

**버린 대안 — 기본값을 `malgn-agent.json`에 두고 앱이 배포 시 심어주기**: 시크릿을 소스에서 빼는 목적은 달성하지만 신규 설치 시 "어디서 그 파일을 받나"라는 배포 문제가 새로 생기고, 이 프로젝트가 이미 `google_oauth_login`에서 확립한 `option_env!` 관례를 깨서 시크릿 주입 경로가 두 갈래가 된다. 포기한 것: 재빌드 없이 엔드포인트를 바꾸는 유연성. 감당: UI에서 직접 입력·저장이 가능해졌으므로 재빌드 없이도 바꿀 수 있다.

---

## 7. 결정 7 — OTel 읽기/저장 커맨드 계약

**선택: 읽기를 `otel_settings_get`으로 교체(기존 `read_otel_env` 삭제)하고 `otel_settings_save`를 신설한다. allowlist 상수 하나가 읽기·쓰기·검증을 모두 지배한다.** 코드 위치는 **신규 파일 `src-tauri/src/otel_settings.rs`** (미커밋 상태인 `lib.rs`를 최소로 건드리기 위해).

```rust
pub(crate) const MANAGED_OTEL_KEYS: [&str; 14] = [
  "CLAUDE_CODE_ENABLE_TELEMETRY",
  "OTEL_EXPORTER_OTLP_PROTOCOL", "OTEL_METRICS_EXPORTER", "OTEL_LOGS_EXPORTER",
  "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT", "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
  "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
  "OTEL_METRIC_EXPORT_INTERVAL", "OTEL_LOGS_EXPORT_INTERVAL",
  "OTEL_LOG_USER_PROMPTS", "OTEL_LOG_TOOL_CONTENT", "OTEL_LOG_TOOL_DETAILS",
  "OTEL_LOG_RAW_API_BODIES", "OTEL_RESOURCE_ATTRIBUTES",
];

#[tauri::command] pub fn otel_settings_get() -> OtelSettings;
#[tauri::command] pub fn otel_settings_save(payload: OtelSavePayload) -> Result<OtelSettings, String>;
```

```jsonc
// otel_settings_get() 반환
{ "settingsPath": "/Users/x/.claude/settings.json",
  "fileExists": true,
  "parseError": null,                    // 문자열이면 UI는 폼을 잠그고 오류만 보여준다
  "managedKeys": ["CLAUDE_CODE_ENABLE_TELEMETRY", "…"],
  "readOnlyKeys": ["OTEL_LOG_USER_PROMPTS","OTEL_LOG_TOOL_CONTENT","OTEL_LOG_TOOL_DETAILS","OTEL_LOG_RAW_API_BODIES"],
  "values":   { "OTEL_METRICS_EXPORTER": "otlp" },   // 파일에 실제로 있는 관리대상 키만
  "defaults": { "OTEL_METRICS_EXPORTER": "otlp" },   // (a)+(b) 병합 기본값
  "endpointDefaultsInjected": true }

// otel_settings_save({ payload }) 인자 — invoke('otel_settings_save', { payload })
{ "values": { "CLAUDE_CODE_ENABLE_TELEMETRY": "1", "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": "https://…" },
  "identity": { "email": "dev@malgnsoft.com", "name": "하근호" } }   // identity는 null 가능
```

저장 알고리즘(순서 고정):
1. `settings.json`을 읽어 `serde_json::Value`로 파싱한다. **파싱 실패 시 즉시 `Err`** — 파싱 못 한 파일을 절대 덮어쓰지 않는다(사용자의 hooks/permissions를 날리는 최악의 사고 경로).
2. `values`의 키가 `MANAGED_OTEL_KEYS`에 없으면 `Err`(조용히 무시하지 않는다 — 프론트 오타를 즉시 드러낸다). `readOnlyKeys`가 들어와도 `Err`.
3. 키별 검증: `CLAUDE_CODE_ENABLE_TELEMETRY`/`OTEL_LOG_*`는 `0|1`, `*_INTERVAL`은 1..=3,600,000의 정수, `*_ENDPOINT`는 `http://`|`https://`로 시작하는 URL, `*_EXPORTER`는 `otlp|console|none`, `TEMPORALITY_PREFERENCE`는 `cumulative|delta|lowmemory`. 위반 시 어떤 키가 왜 틀렸는지 담은 `Err`.
4. `identity`가 있으면 §8 규칙으로 `OTEL_RESOURCE_ATTRIBUTES`를 조립해 `values`에 주입한다(프론트가 이 키를 직접 보내면 `Err`).
5. `env` 객체에 upsert. **빈 문자열은 삭제 신호**(해당 키를 `env`에서 제거). allowlist 밖의 `env` 키와 최상위 키(`$schema, permissions, hooks, enabledPlugins, extraKnownMarketplaces, otelHeadersHelper, tui, skipDangerousModePermissionPrompt, theme, remoteControlAtStartup, agentPushNotifEnabled, model`)는 전부 보존한다.
6. 쓰기 전에 원본을 `settings.json.malgn-bak`으로 복사(매 저장 시 덮어쓰는 롤링 1세대). Claude Code 전역 설정을 건드리는 첫 기능이라 값싼 보험을 둔다.
7. 원자적 쓰기: 같은 디렉터리에 `.settings.json.malgn-tmp-<pid>`로 2칸 들여쓰기 pretty JSON을 쓰고 `fs::rename`. 저장 후 `otel_settings_get()`과 동일한 구조를 반환해 프론트가 재조회 없이 상태를 갱신한다.

**감수하는 것**: `serde_json`을 `preserve_order` 없이 쓰므로 저장 후 JSON 키가 **알파벳순으로 재정렬**된다. `preserve_order` 활성화는 `indexmap`을 끌어들여 §0의 의존성 원칙에 걸린다. 키 순서는 의미가 없고 6번 백업이 있으므로 감수하고, UI 저장 버튼 옆에 "저장 시 파일 형식이 정규화됩니다"를 한 줄 표시한다.

**하위호환**: `read_otel_env` 커맨드와 `fetchOtelEnv()`는 **삭제한다**. 외부 소비자가 없는 번들 프론트엔드이고, 읽기 필터가 두 벌 남으면 "`OTEL_` 접두사만 보다가 `CLAUDE_CODE_ENABLE_TELEMETRY`를 놓치는" 지금 이 버그가 그대로 재생산된다. `lib.rs`의 `collect_otel_env`와 이를 쓰는 테스트(L2482 부근)도 함께 제거하고 동등한 테스트를 `otel_settings.rs`로 옮긴다.

---

## 8. 결정 8 — `employee.*` 조립 규칙

**선택: 프론트가 `{email, name}`만 넘기고, 조립·인코딩은 전적으로 Rust가 한다. 값은 percent-encoding 한다.**

- `employee.id` = email의 `@` 앞부분. **기존 운영 데이터 호환을 위해 반드시 유지**(집계 대시보드가 이 값으로 사람을 식별한다).
- `employee.name` = Google 프로필 이름. `employee.email` = 전체 email.
- **percent-encoding을 정본으로 채택한다.** 근거 둘: ① 이 머신의 실제 운영 데이터가 이미 percent-encoded 한글이고 수집 측이 그것을 디코딩하고 있다는 증거가 있다(호환) ② `OTEL_RESOURCE_ATTRIBUTES`는 W3C Baggage 문법이라 비ASCII 원문을 값에 그대로 넣는 것은 규격 위반이며 collector 구현에 따라 깨진다. 지시서 예시의 raw 한글은 설명용 축약으로 보고 따르지 않는다. 부수 효과로 `,`·`=`·공백 문제가 자동 해소된다(`,`→`%2C`, `=`→`%3D`).
- 인코딩 구현: `A-Za-z0-9-_.~`를 제외한 모든 바이트를 `%XX`(대문자 hex)로 바꾸는 15줄짜리 헬퍼를 직접 둔다. 새 crate 없음(`url` crate는 baggage에 맞는 인코더를 공개 API로 노출하지 않으며 `form_urlencoded`는 공백을 `+`로 바꿔 부적합하다).
- **기존 값 보존 병합**: 파일의 `OTEL_RESOURCE_ATTRIBUTES`를 `,`로 분해해 순서 있는 `(k, v)` 목록으로 만들고 `employee.id/name/email` 3개만 upsert한 뒤 나머지(예: `team=…`)는 원래 순서대로 남긴다. 통째로 덮어쓰지 않는다.
- **`identity`가 `null`(비로그인)이면 `OTEL_RESOURCE_ATTRIBUTES`를 손대지 않는다** — 기존 귀속 정보를 지우는 것이 가장 나쁜 결과다.
- 엣지: `email`에 `@`가 없으면 `Err`. `name`이 공백뿐이면 `employee.name`을 생략(빈 값을 쓰지 않는다).
- **Rust는 email/name을 디스크에 저장하지 않는다.** 저장 커맨드 호출 동안만 메모리에 두고 결과는 `settings.json`의 그 키 하나로만 남는다. 프론트가 이미 메모리로만 보유하는 현행 인증 구조(`state.auth`)와 일관된다.

**버린 대안 — 프론트가 `OTEL_RESOURCE_ATTRIBUTES` 문자열을 통째로 조립해서 전달**: Rust 변경이 적지만, 인코딩 규칙과 병합 규칙이 TS로 새어 나가 Rust의 검증(§7-3)과 두 벌이 된다. 포기한 것: 백엔드 코드 ~40줄. 감당할 이유: 형식을 아는 곳이 한 곳뿐이어야 이 값이 깨졌을 때 볼 파일이 한 개다.

---

## 9. 로그 (과거 실행의 유일한 정본)

- 경로: `<project>/.claude/logs/autonomy/YYYY-MM-DD/<safeTaskId>-HHMMSS.log` — 날짜/시각은 **로컬 시간**(사용자가 자기 날짜로 찾는다), 파일 내부 타임스탬프는 오프셋 포함 RFC3339.
- `safeTaskId`: `[A-Za-z0-9._-]` 외 문자를 `_`로 치환하고 40자로 자른다. **필수 방어** — task id는 손으로 편집 가능한 JSON에서 오고 그대로 경로에 들어가므로, `../../..` 같은 값이 들어오면 프로젝트 밖에 파일을 쓰게 된다.
- 형식(고정 텍스트, 파서 없음):
  ```
  task.id / task.name / project / started / finished / durationMs
  result: success|failed|timeout|aborted     exitCode: <n|none>
  timedOut: true|false    timeoutMinutes: <n>    aborted(앱 종료): true|false
  --- prompt ---            (footer 포함 최종 프롬프트 전문)
  --- stdout ---            (각각 512KB 초과분은 꼬리를 남기고 절단, 절단 표시 삽입)
  --- stderr ---
  ```
- **로그 디렉터리 자체를 git에서 격리한다** — 로그에는 프롬프트 전문과 `claude` 출력(소스 조각·내부 정보 포함 가능)이 평문으로 남는데, 지정된 위치가 많은 프로젝트에서 **커밋 대상인 `.claude/` 안**이다. 그대로 두면 실행 로그가 저장소(경우에 따라 public)로 흘러간다. 대책: 로그를 처음 쓸 때 `<project>/.claude/logs/.gitignore`에 `*`(자기 자신을 포함해 전부 무시)를 생성한다(이미 있으면 건드리지 않는다). 사용자 프로젝트의 루트 `.gitignore`는 절대 수정하지 않는다 — 남의 저장소 설정을 침범하지 않으면서 격리를 얻는 최소 조치다.
- 쓰기 실패(권한·디스크)는 실행 자체를 실패로 만들지 않는다 — 런타임 `summary`에 "로그 기록 실패: …"를 덧붙이고 진행한다.
- 보존: `logs.retentionDays`(기본 30). 스윕 시점 = **앱 시작 후 백그라운드 스레드 1회 + 이후 스케줄러가 로컬 날짜가 바뀌는 첫 tick에 1회**. 대상은 설정된 workspace 아래 각 프로젝트의 `.claude/logs/autonomy/` 안에서 **이름이 `YYYY-MM-DD`로 파싱되는 디렉터리만** — 파싱되지 않는 이름은 절대 지우지 않는다. DB/인덱싱 없음.

## 9-1. 보안 구조적 결정 확정표

이 앱은 HTTP API가 아니지만 Tauri IPC 커맨드가 같은 자리(외부에서 오는 입력을 받아 로컬 자원을 만지는 경계)에 있다. 설계 단계에서 값으로 확정해야 하는 항목만 여기서 못박는다(강도 조절 항목은 구현으로 넘긴다).

| 항목 | 확정값 |
|---|---|
| 게이트를 어디에 거는가 | 커맨드 노출은 `capabilities/default.json`에 fs/shell 플러그인 없이 커스텀 커맨드만(현행 유지). **프론트에서 경로를 받는 커맨드는 예외 없이 `resolve_validated_project_root()`를 통과시킨다** — 대상: `autonomy_save_task`, `autonomy_delete_task`, `autonomy_set_enabled`. 신규 커맨드 `autonomy_runtime_status`·`malgn_agent_config_get`·`otel_settings_get`·`otel_settings_save`는 **경로 인자를 받지 않는다**(각각 인자 없음 / 고정 경로 `~/.claude/…`) — 이것이 이 4개의 게이트다. |
| 인가의 축 | **역할 개념을 두지 않는다.** 단일 사용자 로컬 데스크톱 앱이고 경계는 OS 사용자 계정이다. 대신 Google 로그인(`hd=malgnsoft.com` 강제)은 앱 진입 게이트로만 쓰고 커맨드별 권한 판단에는 쓰지 않는다(현행 유지). |
| 소유권/테넌트 키 | DB가 없어 컬럼 개념이 없다. 등가물은 **파일 경로**이며, 격리 불변식은 "쓰기는 `<검증된 project root>/.claude/` 아래 또는 `~/.claude/` 고정 파일 2개(`malgn-agent.json`은 읽기 전용, `settings.json`은 allowlist 키만)로만" 이다. |
| 신뢰 못 하는 입력 → 경로 조립 | `taskId`(손편집 가능한 JSON에서 옴)가 로그 파일명에 들어간다 → `sanitize_task_id`로 `[A-Za-z0-9._-]`만 남긴다(§9). `workspaces` 항목 → `~` 확장 후 루트/홈/비디렉터리/중복 거부(§5). 이 둘이 이번 변경에서 새로 생긴 유일한 경로 조립 지점이다. |
| 민감 데이터 저장 형태 | ① collector 주소: 소스 아님, `option_env!` 주입(§6). ② `employee.*`(이메일·이름): `settings.json`에 평문(수집 파이프라인이 요구하는 형식) — Rust는 별도 저장하지 않는다(§8). ③ 실행 로그: 로컬 평문 + git 격리(§9). |
| 프론트로 내보내는 경계 | `otel_settings_get`은 `settings.json` 전체가 아니라 **allowlist 14키만** 반환(현행 규율 유지). 런타임 상태는 `child_pid`를 제외해 노출한다. |
| 깨뜨리지 않는 최소 요건 | shell 미경유 argv 배열 실행 / 권한 우회 플래그 없음 / **fail-closed**(전역 설정 손상 → 워크스페이스 0개, `settings.json` 파싱 실패 → 쓰기 거부) / 시크릿을 로그·응답에 싣지 않음. |

## 10. 파일별 책임과 시그니처

기존 `src-tauri/src/autonomy.rs`(654줄)는 **삭제**하고 `src-tauri/src/autonomy/` 디렉터리로 대체한다. `lib.rs`의 `mod autonomy;` 선언은 그대로 작동하므로 그 줄은 수정하지 않는다.

| 파일 | 책임 | 주요 항목 |
|---|---|---|
| `src-tauri/src/config/mod.rs` | 전역 설정 진입점 | `pub(crate) fn load() -> Result<UserConfig,String>`, `pub(crate) fn workspace_roots_checked() -> Result<Vec<PathBuf>,String>`, `#[tauri::command] pub fn malgn_agent_config_get() -> MalgnAgentConfigStatus` |
| `src-tauri/src/config/user_config.rs` | 파일 파싱·`~` 확장·검증·mtime 캐시 | `UserConfig{version, workspaces, autonomy{concurrency, default_timeout}, logs{retention_days}}`, `expand_tilde`, `validate_workspace_entry`, `CACHE` |
| `src-tauri/src/autonomy/mod.rs` | 커맨드 노출 + 재export | `autonomy_list`(→`Result<…>`), `autonomy_save_task`, `autonomy_delete_task`, `autonomy_set_enabled`, `autonomy_runtime_status`, `spawn_scheduler`, `request_shutdown_and_wait(Duration)` |
| `src-tauri/src/autonomy/config.rs` | `autonomy.json` 스키마·읽기/쓰기·정규화·상수 정본 | `AutonomyTaskConfig`, `AutonomyFile`, `AUTONOMY_FILE_LOCK`, `normalize_task`, `effective_timeout_minutes`, §1 상수 블록 |
| `src-tauri/src/autonomy/runtime.rs` | 메모리 상태 + 종료 플래그 | `RUNTIME: Mutex<BTreeMap<TaskKey,TaskRuntime>>`, `SHUTTING_DOWN: AtomicBool`, `snapshot_all() -> Vec<AutonomyRuntimeStatus>`, `mark_started/mark_finished/prune` |
| `src-tauri/src/autonomy/scheduler.rs` | tick 루프·due 선정·concurrency 게이트·reconcile·로그 스윕 트리거 | `spawn_scheduler`, `tick`, `select_due` |
| `src-tauri/src/autonomy/runner.rs` | 프롬프트 조립·claude 경로 해석·프로세스 실행 | `HUB_RECORD_FOOTER`(현행 문구 그대로 유지), `build_final_prompt`, `CLAUDE_PATH_CANDIDATES`, `run_task`, `tail_chars` |
| `src-tauri/src/autonomy/log.rs` | 로그 경로·기록·보존 스윕·로그 디렉터리 git 격리 | `log_path_for`, `sanitize_task_id`, `ensure_logs_gitignore`, `write_run_log`, `sweep_old_logs` |
| `src-tauri/src/otel_settings.rs` (신규) | OTel 읽기/저장·allowlist·검증·`employee.*` | `MANAGED_OTEL_KEYS`, `otel_settings_get`, `otel_settings_save`, `percent_encode_value`, `merge_resource_attributes` |
| `src-tauri/src/dev_tools.rs` | 프로세스 러너 가시성 확대 | `pub(crate)` 승격 + `run_process_with_timeout_cancellable` 추가(§4). **그 외 로직 변경 금지** |
| `src-tauri/src/lib.rs` (**Edit만**) | 배선 | ① `mod config; mod otel_settings;` 추가 ② `workspace_roots()` 본문 1줄 위임 ③ `collect_otel_env`/`read_otel_env` + 해당 테스트 제거 ④ `invoke_handler` 목록 갱신 ⑤ `run()` 말미를 `build(..)?.run(|_h,e| …ExitRequested→autonomy::request_shutdown_and_wait(2s)…)`로 변경 |
| `src-tauri/build.rs` | `MALGN_OTEL_COLLECTOR_BASE` 통과(기존 `GOOGLE_OAUTH_CLIENT_SECRET` 블록 바로 아래 3줄) | |

프론트엔드:

| 파일 | 책임 |
|---|---|
| `src/autonomyApi.ts` (재작성 가능) | 새 `AutonomyTaskConfig`(interval/timeout), `AutonomyRuntimeStatus`, `fetchAutonomyTasks/saveAutonomyTask/deleteAutonomyTask/setAutonomyTaskEnabled/fetchAutonomyRuntimeStatus/onAutonomyRuntimeChanged` |
| `src/otelApi.ts` (재작성 가능) | `OtelSettings`/`OtelSavePayload` 타입, `fetchOtelSettings()`, `saveOtelSettings(payload)` |
| `src/configApi.ts` (신규) | `MalgnAgentConfigStatus` 타입, `fetchMalgnAgentConfig()` |
| `src/views/autonomousTasks.ts` (재작성 가능) | 설정+런타임 병합 렌더, `preventOverlap` UI 제거, `timeout` 입력 추가, `nextRunAt`/`logPath` 표시, 이벤트 구독(모듈 레벨 1회 가드) |
| `src/views/settings.ts` (**Edit만**) | `loadOtelEnv`/`renderOtelPanel` 본문만 교체(실저장). 다른 패널·MCP 탭 코드는 손대지 않는다 |
| `src/state.ts` (**Edit만**) | `AutonomousTask` 인터페이스 필드 교체, `otel` 상태 shape 교체, `malgnAgentConfig` 상태 추가 |
| `src/main.ts` | **수정하지 않는다**(이벤트 구독을 뷰 안에서 지연 초기화하기로 결정) |

## 11. 백엔드↔프론트 계약 (이 표가 정본 — 양쪽이 이대로 구현하면 충돌하지 않는다)

| 커맨드 | invoke 인자(camelCase) | 반환 | 실패 |
|---|---|---|---|
| `autonomy_list` | 없음 | `ProjectAutonomyGroup[]` = `{projectPath, projectName, tasks: AutonomyTaskConfig[]}[]` | 전역 설정 손상 시 `Err(string)` |
| `autonomy_save_task` | `{ projectPath, task }` | `void` | 경로 검증 실패/쓰기 실패 `Err(string)` |
| `autonomy_delete_task` | `{ projectPath, taskId }` | `void` | 〃 |
| `autonomy_set_enabled` | `{ projectPath, taskId, enabled }` | `void` | 〃 |
| `autonomy_runtime_status` | 없음 | `AutonomyRuntimeStatus[]` (§2) | 없음(항상 성공) |
| `malgn_agent_config_get` | 없음 | `{ ok, error, configPath, fileExists, workspaces: string[], warnings: string[], autonomy:{concurrency, defaultTimeout}, logs:{retentionDays}, limits:{minInterval, maxInterval, minTimeout, maxTimeout, maxConcurrency, startupGraceMinutes} }` | 없음(오류는 `ok:false`+`error`) |
| `otel_settings_get` | 없음 | `OtelSettings` (§7) | 없음(`parseError`로 표현) |
| `otel_settings_save` | `{ payload: { values, identity } }` | `OtelSettings` | 검증·파싱·쓰기 실패 `Err(string)` |

`AutonomyTaskConfig`(TS): `{ id: string; name: string; prompt: string; subagent: string | null; interval: number; enabled: boolean; timeout: number | null }`
이벤트: `autonomy-task-updated`, payload = `AutonomyRuntimeStatus` 1건.

## 12. 테스트 이관 (기존 24개)

| 처리 | 대상 |
|---|---|
| 이관(그대로) | `upsert_adds_new_task…`, `upsert_updates_existing…`, `upsert_clamps_interval…`, `tail_chars_*` 2개, `build_final_prompt_*` 2개, `hub_record_footer_is_conditional…`, `resolve_validated_project_root_rejects_path_outside_workspace`, `claude_path_candidates_resolve…` |
| 수정 후 이관 | `task_json_roundtrip…`(→ `interval` 직렬화 + 레거시 필드 미출력 확인), `task_deserializes_with_only_required_fields…`, `autonomy_file_roundtrip…` |
| 삭제 | `upsert_truncates_history…`, `is_due_*` 6개 중 `prevent_overlap` 2개 |
| 재작성 | `is_due_*` 4개 → `select_due_*`(런타임 상태 기반: 미등록·유예 중·running·완료 후 interval 경과·concurrency 초과) |
| 신규(최소) | 레거시 필드가 있는 JSON을 읽고 쓰면 사라지는지 / `intervalMinutes` alias 수용 / `effective_timeout_minutes` 우선순위 3단 / `sanitize_task_id`가 `../` 제거 / `expand_tilde`+workspace 검증(루트·홈 거부) / 손상 JSON이 `Err` / `MANAGED_OTEL_KEYS` 밖 키 저장 거부 / **파싱 불가 `settings.json`에 저장 시도 시 파일이 바뀌지 않고 `Err`** / `settings.json` 저장 시 최상위 키 전부 보존 / percent-encoding 한글·`,`·`=` / `OTEL_RESOURCE_ATTRIBUTES`의 비-employee 항목 보존 / `ensure_logs_gitignore`가 기존 파일을 덮어쓰지 않음 |

## 13. 구현 순서 및 담당 분배

**backend-dev (Rust) — 순서대로. 각 단계 끝에 `cargo test` 통과.**
1. `config/` 신설 + `lib.rs`의 `workspace_roots()` 위임 + `malgn_agent_config_get` 커맨드 등록. (§5)
2. `dev_tools.rs` 가시성 승격 + `run_process_with_timeout_cancellable` 추가. 기존 4개 호출부·3개 테스트가 **무수정**으로 통과하는지 확인. (§4)
3. `autonomy.rs` → `autonomy/` 6파일 분해. 스키마 축소·정규화·로그·런타임·스케줄러. 테스트 이관(§12). (§1·2·3·9)
4. `lib.rs` `run()` 종료 훅 + `invoke_handler` 갱신. (§1)
5. `otel_settings.rs` + `build.rs` 주입 + `read_otel_env`/`collect_otel_env` 제거. (§6·7·8)

**frontend-dev (TS) — 1단계 완료를 기다릴 필요 없이 §11 표만 보고 병렬 착수 가능.**
1. `src/configApi.ts`, `src/otelApi.ts`, `src/autonomyApi.ts` 타입·래퍼 작성(§11 표가 유일한 근거).
2. `src/state.ts` **국소 Edit**: `AutonomousTask` 필드 교체, `otel` 상태 교체, `malgnAgentConfig` 추가.
3. `src/views/autonomousTasks.ts`: 설정+런타임 병합, `preventOverlap` 제거, `timeout` 입력(분, placeholder는 `limits`에서), `nextRunAt`/`status`/`logPath` 표시, "등록 후 약 3분 뒤 첫 실행" 안내, 이벤트 구독 지연 초기화(+30초 폴백 폴링).
4. `src/views/settings.ts` **국소 Edit**: OTel 패널만 실저장으로 교체(`readOnlyKeys` 비활성, `parseError` 시 폼 잠금, 기본값 placeholder, 저장 시 `state.auth`의 email/name을 `identity`로 전달).

**통합(둘이 함께)**: `pnpm tauri dev`로 ① 자율업무 추가→3분 후 실행→로그 파일 생성 확인 ② 실행 중 앱 종료 후 `ps`로 잔여 `claude` 프로세스 없음 확인 ③ `malgn-agent.json`을 일부러 깨뜨렸을 때 자율업무 화면과 설정 화면에 오류가 뜨고 아무 task도 실행되지 않음 확인 ④ OTel 저장 후 `settings.json`의 `permissions`/`hooks`/`enabledPlugins`가 그대로인지 diff로 확인(`settings.json.malgn-bak`과 비교).
