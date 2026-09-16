# 자율업무 고정시간 스케줄 모드 + "지금 실행" 리뷰 보고서

리뷰 페르소나 패널: `docs/reviewer/personas/persona-design-contract-auditor.md`, `docs/reviewer/personas/persona-claimed-vs-verified.md` (2명, 둘 다 **재사용**)
리뷰 대상: 워크트리 `/Users/hopegiver/workspace/malgn-vscode-schedule` (브랜치 `feat/autonomy-fixed-schedule`, 기점 `origin/main` 07ef15d)
 - `git diff` 11파일 + 신규 `src-tauri/src/autonomy/schedule.rs`(untracked, 438줄)
설계 정본: `/private/tmp/claude-501/-Users-hopegiver-workspace-malgn-vscode/3c66d427-3435-4d68-8c84-5c87cfee4d89/scratchpad/design-autonomy-schedule-modes.md`
리스크 범주: 로컬 자동실행 스케줄러(사용자 승인 없이 `claude -p` 자식 프로세스를 반복 spawn) — 토큰 비용/중복 부작용
리뷰 등급·깊이: **Standard / 약식**(페르소나 2, 발산형 미투입). PM이 제시한 축소 근거(신규 커맨드 1개이나 `capabilities/default.json` 무변경, 경로 검증은 기존 `resolve_validated_project_root` 재사용, 단일 사용자 로컬 앱, 취급 데이터는 본인 스케줄 설정)를 실물 대조로 확인했고 그대로 수용했다.
리뷰 일자: 2026-09-16
종합 판정: 🔴 **Red**

## 요약 (2분 규칙)

PM이 가장 먼저 보라고 지목한 **① `next_occurrence` 알고리즘(설계 §4.5 16행)은 16행 중 코드로 도달 가능한 전 행이 사양과 일치하고, 무한 재실행을 막는 엄격 `>`도 정확히 구현·테스트돼 있다(`schedule.rs:127`, 테스트 `next_occurrence_excludes_exact_same_instant`). ②하위호환도 성립한다**(레거시 JSON → `Interval` default, `intervalMinutes` alias·clamp 무손상, 저장 시 `scheduleMode` 명시 기록). 이 두 축은 통과다.

대신 문제는 전부 **"언제 `next_run_at`을 다시 계산하는가 / TaskKey를 누가 만드는가"** 쪽에서 나왔다. 가장 심각한 것은 신규 `autonomy_run_now`가 RUNTIME 레지스트리 키를 `resolve_validated_project_root`(**std** `canonicalize`)에서 만드는데, 스케줄러·`autonomy_list`는 같은 키를 `dunce::canonicalize` 기반 경로에서 만든다는 점이다 — 이 저장소가 `classify.rs:459-462`에 스스로 적어 둔 대로 **Windows에서 두 값은 `\\?\` 접두 유무로 항상 다르다.** 그 결과 Windows에서는 "지금 실행"이 유령 키를 만들어 (a)스케줄러의 "이미 실행 중" 방어를 우회해 같은 task를 동시에 두 번 돌릴 수 있고 (b)실행이 화면에 전혀 보이지 않는다. Windows는 이 제품의 1급 지원 대상이다(CLAUDE.md, 직전 브랜치명이 `devtools-windows-parity`).

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| C1 | 🔴 | 계약감사 | `src-tauri/src/autonomy/mod.rs:191` + `src-tauri/src/workspace/tree.rs:39` vs `src-tauri/src/config/user_config.rs:221` | 세 파일 실물 Read + `grep -rn canonicalize src-tauri/src` 전수 | `autonomy_run_now`가 TaskKey를 `resolve_validated_project_root`의 **std `canonicalize`** 결과로 만든다. 스케줄러(`scheduler.rs:41` → `workspace_roots_checked` → `user_config.rs:221` **`dunce::canonicalize`**)와 `autonomy_list`(`mod.rs:70`)는 dunce 계열 경로로 키를 만든다. 이 저장소가 `dev_tools/classify.rs:459-462`에 "Windows `std::fs::canonicalize()`는 `\\?\` 접두를 붙여 문자열 비교가 깨진다"고 이미 문서화해 둔 바로 그 함정이다. **Windows에서 두 키는 항상 다르다.** 결과: ①`try_start_now`(`runtime.rs:139`)가 유령 키에 `running=true`를 찍으므로 스케줄러의 "이미 실행 중" 방어가 무력화 → 같은 task가 수동+자동으로 **동시 2회 `claude -p`** ②`emit_status`/`mark_finished`가 유령 키에 기록 → FE의 `runtimeKey(task.projectPath, ...)` 조인 실패 → "지금 실행"이 눌러도 아무 반응 없는 것처럼 보이고 결과·로그 링크도 안 뜸 ③`prune_missing`이 종료 후 유령 키를 지워 흔적이 사라짐 | `mod.rs:118`·`mod.rs:191`의 키 생성부에서 `dunce::canonicalize(&root).unwrap_or(root)`를 거쳐 스캐너와 같은 표현으로 맞춘다(`dunce`는 이미 `Cargo.toml:45` 의존). 또는 `resolve_validated_project_root`(`tree.rs:39`) 자체를 `dunce::canonicalize`로 바꾼다 — 단 `session_chat`이 같은 함수를 공유하므로 영향 범위를 함께 본다. **회귀 테스트**: 스캐너 경로 문자열과 커맨드 경로 문자열이 같은 프로젝트에 대해 일치하는지 단언하는 테스트 1건. 설계 §9-4가 "구현 단계에서 두 경로 문자열이 일치하는지 실측으로 확인할 것"이라 지시했는데 코드·테스트·주석 어디에도 확인 흔적이 없다 |
| M1 | 🟠 | 주장-검증 | `src-tauri/src/autonomy/runtime.rs:124-130`(`mark_started`) vs `132-138`(주석) + `scheduler.rs:164-196` | 두 함수 본문·락 구간 Read | `try_start_now`의 doc 주석은 "`select_due`처럼 조회 락과 마킹 락을 분리하면 TOCTOU가 생길 수 있어 분리하지 않는다"고 단언하지만, **보호되는 건 자기 쪽 한 방향뿐이다.** 스케줄러는 `scheduler.rs:164-167`에서 락을 잡고 `select_due`로 due 키를 뽑은 뒤 **락을 놓고**, `scheduler.rs:189`에서 `mark_started`를 다시 잡는다. `mark_started`에는 `running` 재확인 가드가 없다(`runtime.rs:124-130`은 무조건 `running=true` 덮어쓰기). 그 사이 창에서 사용자가 "지금 실행"을 누르면 `try_start_now`가 먼저 성공하고, 뒤이어 `mark_started`가 그 위를 덮어쓰며 **같은 task로 두 번째 `run_task` 스레드를 spawn**한다. 부작용: `claude -p` 2회 과금, `set_child_pid`가 뒤 것으로 덮여 앞 자식의 pid 유실(타임아웃·종료 시 SIGKILL 보루가 한쪽만 도달), 먼저 끝난 쪽이 `running=false`를 찍어 아직 도는 프로세스가 "미실행"으로 보임. 추가로 `try_start_now`의 concurrency 검사는 `select_due`가 이미 점유 예약한 슬롯을 모르므로 동시 실행 수가 한도를 초과할 수 있다 | `mark_started`를 `-> bool`로 바꿔 "이미 `running`이면 false 반환, 상태 무변경"으로 하고, `scheduler.rs:189`에서 false면 `continue`(spawn 생략). 12줄 안쪽 변경이고 `select_due`는 한 글자도 안 건드린다. 회귀 테스트: "running=true인 키에 `mark_started`를 호출하면 false를 반환하고 `last_started_at`을 덮지 않는다" |
| M2 | 🟠 | 계약감사 | `src-tauri/src/autonomy/mod.rs:139-155`(`autonomy_set_enabled`, 이번 diff에서 **무변경**) + `scheduler.rs:73-88` | 커맨드 본문 Read + `select_due` 필터 조건 대조 | **FixedTime task를 껐다가 나중에 다시 켜면 즉시 예정에 없는 실행이 일어난다.** `autonomy_set_enabled`는 `next_run_at`을 손대지 않고, `select_due`는 `enabled`가 false인 동안 후보에서 빼기만 할 뿐 `next_run_at`을 밀어주지 않는다. 그래서 화요일 09:00에 다음 회차가 확정된 상태로 중지 → 금요일 15:00에 재개하면 `next_run_at(화 09:00) <= now`가 즉시 성립해 **3일 지난 회차가 바로 실행된다.** 설계 §5의 대원칙("앱이 꺼져 있던 동안 지난 회차는 건너뛴다, 30분 창만 예외")과 정면으로 어긋나고, §4.5 #16은 `enabled=false`를 "select_due가 제외한다"로만 적어 재개 시점을 다루지 않았다. Interval 모드에선 "완료 후 N분이 이미 지났으니 지금 실행"이 자연스러워 기존에는 문제가 아니었다 — FixedTime이 들어오면서 생긴 신규 결함이다 | `autonomy_set_enabled`에서 `enabled=true`로 전환되고 저장된 task가 FixedTime이면 `runtime::reschedule_if_idle(&key, schedule::initial_next_run_at(saved, Utc::now()))`를 호출한다(이미 `autonomy_save_task`에 있는 관용구 재사용, 5줄). ※ 이때 M3의 따라잡기 앵커 문제를 같이 해결해야 한다 |
| M3 | 🟠 | 계약감사 | `src-tauri/src/autonomy/mod.rs:119` + `src-tauri/src/autonomy/schedule.rs:148-162` | 두 함수 본문 Read + 앵커 값 손추적 | `autonomy_save_task`가 편집 후 재스케줄에 `initial_next_run_at`을 그대로 재사용하는데, 이 함수는 앵커를 `now - MISSED_RUN_GRACE(30분)`로 당기는 **"앱 시작 시 놓친 회차 따라잡기"** 전용 로직이다(`schedule.rs:157-158`). 편집 경로에 이 앵커를 쓰면 "이미 오늘 돈 회차"가 다시 과거 인스턴트로 반환된다. 재현: atTime 09:00 task가 09:00~09:02에 정상 실행 → `next_run_at`=내일 09:00 → 사용자가 09:10에 프롬프트를 고쳐 저장 → 앵커 08:40 → 오늘 09:00이 `> 08:40`이라 후보로 잡힘 → `max(now+STARTUP_GRACE)` = 09:13에 **오늘 두 번째 실행.** 30분 창 안이면 저장할 때마다 반복 가능하다. 같은 이유로 09:10에 "매일 09:00" task를 **신규 등록**하면 3분 뒤 곧바로 1회 실행된다(사용자 기대와 어긋남). 설계 §4.3이 (A)를 "최초 등록 — 앱 시작 직후/tick이 파일을 처음 발견했을 때"로 한정했는데 구현이 그 한정을 편집 경로까지 넓혔다 | 따라잡기 앵커 없는 재스케줄 함수를 하나 더 둔다: `reschedule_next_run_at(task, now) = next_occurrence(spec, now)`(STARTUP_GRACE 하한은 유지해도 무방). `mod.rs:119`와 M2의 신규 호출부가 이 함수를 쓰고, `initial_next_run_at`은 `scheduler.rs:143`의 reconcile 경로에만 남긴다. 회귀 테스트: "방금 지난 회차가 있어도 편집 재스케줄은 다음 회차를 반환한다" |
| M4 | 🟠 | 주장-검증 | `src/views/autonomousTasks.ts:185-195` + `src-tauri/src/autonomy/runtime.rs:108-115` + `src/views/autonomousTasks.ts:957-958` | 저장 흐름 추적(`saveAutonomyTask` → `closeTaskFormModal` → `loadAutonomousTasks`) + `reschedule_if_idle` 주석 | **유효한 시각으로 고정시각 task를 새로 만들면, 직후 화면이 "실행 시각이 올바르지 않습니다"라고 거짓말한다.** `computeNextRunLabel`은 `nextRunAt === null`이면 fixedTime일 때 무조건 이 문구를 낸다(`:191`). 그런데 신규 등록 직후에는 RUNTIME에 키가 없고, `reschedule_if_idle`은 자기 주석대로 **미등록 키에 no-op**이므로(`runtime.rs:108-115`) `next_run_at`은 다음 tick(최대 `TICK_SECONDS`=10초)까지 존재하지 않는다. 저장 성공 직후 `loadAutonomousTasks()`가 바로 돌아 목록을 다시 그리므로(`:958`) 이 거짓 오류가 신규 작성 **정상 경로에서 매번** 보인다. 신기능의 대표 화면에서 "네 입력이 잘못됐다"고 말하는 문구라 사용자가 멀쩡한 시각을 고치러 되돌아간다 | ①`AutonomousTask`가 "아직 등록 전"과 "영구 null"을 구분하도록 런타임 엔트리 존재 여부를 따로 들고 오거나, ②등록 전 상태에는 `'첫 실행 시각 계산 중'` 같은 중립 문구를 쓰고 파싱 실패는 백엔드가 명시 신호(예: `atTime`이 있는데 `fixed_time_spec()`이 None)로 내려주게 한다. 최소 조치로는 ③`reschedule_if_idle` 대신 미등록 키도 삽입하는 upsert형 재스케줄을 `autonomy_save_task`에서 쓰면 창 자체가 사라진다 |
| m1 | 🟡 | 계약감사 | `src-tauri/src/autonomy/mod.rs:116-120` | 조건문 실물 Read | 재스케줄이 `saved.schedule_mode == FixedTime`일 때만 걸려, **FixedTime → Interval 전환은 `next_run_at`이 옛 고정시각 그대로 남는다.** "매일 09:00"을 "완료 후 30분"으로 바꿔도 내일 09:00이 되어야 비로소 30분 주기가 시작된다(최대 ~24시간 무반응). 같은 화면에서 `scheduleLabel`은 "이전 실행 완료 후 30분 뒤 재실행", `nextRunLabel`은 "23시간 후"로 **서로 모순된 두 문구가 동시에** 뜬다. 설계 §7이 "Interval 모드에는 호출하지 않는다 — 현행은 편집해도 다음 회차부터 반영"이라 적었지만, 그 "현행"은 두 모드가 없던 시절의 규칙이다 | 모드가 **바뀐** 경우(이전 값과 비교)에는 모드와 무관하게 재스케줄한다. 이전 모드를 알아야 하므로 `upsert_task` 전에 기존 항목의 `schedule_mode`를 읽어두면 된다(3줄) |
| m2 | 🟡 | 주장-검증 | `src-tauri/src/autonomy/schedule.rs:166-179` + `schedule.rs:398-419`(테스트) | schedule.rs 테스트 21건 전수 대조 + 설계 §8 표 대조 | PM이 "유일한 치명 경로"로 지목한 **`next_run_after_finish`의 FixedTime 분기에 직접 테스트가 0건**이다. 설계 §8도 #19(Interval만)를 적고 FixedTime 판을 빠뜨렸다. 엄격 `>` 자체는 `next_occurrence_excludes_exact_same_instant`가 덮지만, 그 테스트는 `next_occurrence_in`에 `FixedOffset`을 주입한 것이라 실제 프로덕션 호출 사슬(`next_run_after_finish` → `next_occurrence` → `Local`)을 지나지 않는다. 지금 구현은 맞지만, 누가 `next_run_after_finish`에 `>=`나 "오늘부터" 같은 변형을 넣어도 초록이 유지된다 | `ScheduleSnapshot{ mode: FixedTime, fixed: Some(spec) }`에 `finished_at`을 넣어 "반환값 > finished_at"과 "24시간 이내"를 단언하는 테스트 1건 추가. tz 비의존으로 쓰려면 `next_run_after_finish`에 `_in` 제네릭 변형을 하나 두고 얇은 래퍼만 `Local`을 쓰게 하면 된다(`next_occurrence`와 같은 패턴) |
| m3 | 🟡 | 주장-검증 | `src/configApi.ts:20-24` + `src-tauri/src/config/mod.rs:54-55`, `:69` | 두 파일 diff 나란히 대조 | TS 주석이 "`missedRunGraceMinutes`는 backend-dev가 **병렬로 추가 중인** 필드라 옵셔널로 선언한다"고 적었는데, **그 필드는 같은 diff의 `config/mod.rs:54-55`에 비-옵셔널 `u32`로 이미 추가돼 있다.** 주석이 서술하는 과도기는 이 브랜치 안에서 이미 끝났다. 결과로 `?` 선언과 `?? null` 폴백, 그리고 `views/autonomousTasks.ts:862-863`의 "숫자 없는 일반 문구" 분기가 전부 도달 불가 죽은 코드가 됐다 | 주석을 지우고 `readonly missedRunGraceMinutes: number;`로 바꾼다. 폴백 분기는 남겨도 무해하나, 남긴다면 이유를 "설정 로드 실패 시 `status`가 null일 수 있음"으로 정확히 다시 쓴다 |
| n1 | ⚪ | 계약감사 | `src-tauri/src/autonomy/scheduler.rs:143` | 호출부와 `ensure_registered`(`runtime.rs:95-101`) 대조 | `ensure_registered`는 `or_insert_with`라 이미 등록된 키엔 값을 안 쓰는데, 인자인 `schedule::initial_next_run_at(...)`은 **매 tick(10초) × 매 task마다 무조건 평가**된다. FixedTime task 1개당 최대 9일 × (primary+fallback) = 18회 `Local` 로컬시간 해석이 10초마다 돈다. 동작엔 영향 없고 비용도 미미하다 | `ensure_registered`를 `FnOnce() -> Option<DateTime<Utc>>` 클로저를 받게 하면 `or_insert_with` 안에서만 평가된다(2줄) |
| n2 | ⚪ | 주장-검증 | `src-tauri/src/autonomy/runtime.rs:206-343`(테스트 모듈) | 테스트 5건 전수 Read | 신규 `runtime.rs` 테스트가 프로세스 전역 `RUNTIME`을 공유하고 `cleanup`도 각 테스트 시작·끝에서 자기 키만 지운다. 현재는 동시에 `running=true`인 항목이 최대 3개라 `try_start_now_marks_unregistered_task_running_on_success`(concurrency 8)가 깨지지 않지만, **이 파일에 running 항목을 만드는 테스트가 5개 더 늘어나면 조용히 flaky가 된다.** 지금 깨지지 않는다는 뜻이지 안전하다는 뜻은 아니다 | `try_start_now`를 `select_due`처럼 맵을 인자로 받는 순수 함수 + 얇은 전역 래퍼로 쪼개거나, 테스트 모듈에 전용 직렬화 뮤텍스를 둔다 |

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 계약감사 | `fixed_time_spec()`(`config.rs:79-91`)이 `days`를 범위 필터 없이 `clone`하므로 손편집 `[9]`가 그대로 넘어가 `next_occurrence`가 `None`을 반환, 설계 §4.5 #15(fail-open=매일)와 어긋난다 | **기각** | `read_autonomy_file`(`config.rs:168-177`)이 읽기 직후 모든 task에 `normalize_task`를 적용한다. `fixed_time_spec()`에 도달하는 4개 호출부(`scheduler.rs:143`·`:185`, `mod.rs:119`·`:201`)가 전부 그 함수를 경유하므로 정규화되지 않은 `days`는 실행 경로에 존재하지 않는다. 설계 #15대로 fail-open이 성립 |
| 주장-검증 | `initial_next_run_at_catches_up_run_missed_within_grace`가 `Local::now()`를 쓰므로 CI 타임존/자정 경계에서 flaky하다 | **기각** | 테스트 본문을 손추적했다. `now`를 한 번만 캡처하고 앵커(`now-30분`)·기대값(`now+STARTUP_GRACE`)을 같은 값에서 파생하며, 자정을 넘어도 `next_occurrence_in`이 앵커의 로컬 날짜부터 훑기 때문에 전날 회차를 정상적으로 잡는다. 남은 비결정성은 실행 중 DST 전환뿐이라 실질 위험 없음 |
| 계약감사 | `autonomy_run_now`가 `task.enabled`를 검사하지 않아 중지된 task도 실행된다 | **기각(스코프/의도)** | `views/autonomousTasks.ts:1078-1084` 주석이 "중지는 다음 예약 실행을 하지 않는다는 뜻일 뿐 수동 1회 실행까지 막을 이유가 없다"고 의도를 명시했고, 지시서의 거부 사유도 "실행 중 / concurrency 한도" 둘뿐이다. 의도된 동작 |
| 계약감사 | FixedTime 모드에서도 `interval` 필드를 계속 전송·clamp하는 것은 죽은 필드다 | **기각(설계 채택 대안)** | 설계 §6 기각표가 `interval: Option<u32>`를 명시적으로 기각했고(alias 계약·clamp 테스트 2건 동시 파손), 구현이 그 결론을 그대로 따랐다. 설계와 다른 취향을 리뷰 지적으로 올리지 않는다 |

## 페르소나별 관점

### [설계 계약 감사관 `persona-design-contract-auditor.md`] — 판정: 🔴 Red

설계서 §4.5 경계 케이스 **16행을 한 행씩 코드에 대조**했다. 결과: #1·#2·#3·#4·#5·#6·#7·#8은 `next_occurrence_in`(`schedule.rs:117-135`)의 루프·`day_allowed`·엄격 `>`로 정확히 성립하고 대응 테스트도 존재한다. #9·#10·#11은 `initial_next_run_at`(`schedule.rs:148-162`)의 앵커 당기기 + `max(startup_floor)`로 성립. #12·#13·#14는 `pick`(`schedule.rs:85-97`)과 `normalize_task`로 성립. #15는 위 기각표대로 성립. #16은 `select_due` 무변경으로 성립. **16행 전부 통과**이며, 설계 §8이 지정한 schedule.rs 테스트 21건·config.rs 테스트 8건도 전수 실재를 확인했다(이름까지 일치).

문제는 설계서가 **다루지 않은 조항의 빈칸**에서 나왔다. 설계 §4.3은 진입점을 (A)최초 등록 / (B)완료 후 **둘뿐**으로 정의했는데, 구현은 여기에 (C)편집 후 재스케줄이라는 제3의 진입점을 추가하면서 (A)를 그대로 재활용했다 → **M3**. 그리고 (D)중지→재개라는 제4의 진입점은 아무도 처리하지 않아 옛 `next_run_at`이 그대로 살아남는다 → **M2**. 두 결함 모두 "설계 조항 위반"이 아니라 "설계가 열거하지 않은 상태 전이"라, 조항 대조만으로는 안 잡히고 상태 전이표를 그려야 나온다.

가장 무거운 **C1**은 설계가 §7·§9-4에서 **이미 경고한 자리**다. 다만 설계는 이 리스크를 `reschedule_if_idle`(fail-safe로 퇴화)에만 걸어 평가했고, 그 뒤에 추가된 `autonomy_run_now`가 같은 경로 문자열로 **런타임 키를 새로 만든다**는 사실은 재평가되지 않았다. 같은 불일치가 한쪽에선 fail-safe이고 다른 쪽에선 중복 실행이다. §9-4의 "실측으로 확인할 것"은 코드·테스트·주석 어디에서도 수행 흔적이 없다.

### [주장 vs 검증 `persona-claimed-vs-verified.md`] — 판정: 🔴 Red

코드가 **자기 주석과 일치하는지**를 축으로 봤다.

- `try_start_now` 주석(`runtime.rs:132-138`)은 "조회 락과 마킹 락을 분리하지 않아 TOCTOU가 없다"고 단언한다. 자기 함수 안에서는 참이다. 그러나 경합 상대인 `scheduler::tick`은 여전히 분리된 두 락으로 움직이고 `mark_started`에 가드가 없다. **주장은 한 방향만 참인데 문장은 양방향처럼 읽힌다** → M1.
- `configApi.ts:20-24` 주석은 "backend-dev가 병렬로 추가 중"이라는 **같은 브랜치 안에서 이미 끝난 과도기**를 현재형으로 서술한다 → m3.
- `mod.rs:110-117` 주석은 경로 불일치를 "안전하게 퇴화할 뿐 오동작하지 않는다"고 적었다. `reschedule_if_idle`에 한정하면 맞지만, 같은 함수 20줄 아래 `autonomy_run_now`에는 그 안전성이 없다 → C1.
- `views/autonomousTasks.ts:188-190` 주석은 `nextRunAt === null`을 "atTime 파싱 실패로 영구히 None인 경우가 실제로 있다"로 설명한다. 참이지만 **그것만은 아니다** — 등록 직후 미등록 창이 같은 값을 만든다 → M4.

반대로 검증이 **충실한** 자리도 분명하다. `pick()`을 순수 함수로 분리해 `LocalResult` 3분기를 직접 주입한 DST 테스트 3건은, `chrono-tz` 없이 DST 규칙을 결정적으로 검증하는 드문 설계다. `next_occurrence_in`을 `Tz: TimeZone` 제네릭으로 두고 테스트가 `FixedOffset`을 주입하는 것도 "CI 러너 타임존에 의존하지 않는다"는 주장을 실제로 성립시킨다 — 주석이 약속한 것을 코드가 그대로 지켰다.

## 구조적 제언 (Rethink) 🔵

> **발산형 페르소나 미투입** — Standard/약식 등급이라 패널을 2명(둘 다 수렴형)으로 줄였다. 아래는 발산형 패널의 산출물이 아니라, 수렴형 두 관점이 공통 근원으로 지목한 구조 관찰 1건이다.

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R1 | `next_run_at`을 다시 계산할 "계기"가 4가지(앱시작 reconcile / 신규등록 / 편집 / 중지→재개)인데 함수는 `initial_next_run_at`·`next_run_after_finish` 2개뿐이라, 한 함수가 서로 다른 의미의 계기를 겸임한다 | 계기를 enum(`RescheduleCause::{AppStart, Created, Edited, Reenabled, Finished}`)으로 명시하고, "따라잡기 창을 적용하는가 / STARTUP_GRACE 하한을 적용하는가"를 계기별 표 하나로 결정하게 한다 | M2·M3·m1이 전부 "이 계기엔 어떤 앵커를 써야 하나"를 함수 이름이 답해주지 못해 생긴 같은 뿌리의 결함이다. 표가 생기면 새 계기가 추가될 때 빈칸이 컴파일 타임에 드러난다 | `schedule.rs`에 20~30줄, 호출부 4곳 수정. 기존 테스트 21건은 그대로 유지 가능(낮음~중간) |

## 트레이드오프 (페르소나 간 충돌)

- **계약감사 vs 주장-검증 — M2(재개 시 즉시 실행)의 성격**: 계약감사는 "설계 §5가 못 박은 '놓친 회차 건너뛰기' 원칙 위반"으로 Major를 주장했고, 주장-검증은 "설계에 조항 자체가 없으니 기획의도 재확인 대상(요구사항 문제)이지 결함이 아니다"라고 강등을 주장했다. → **권고: Major 유지.** 사용자 승인 없이 `claude -p`가 예정 밖으로 도는 쪽이 비용·부작용이 비대칭적으로 크고, 설계 §5가 세운 원칙을 그대로 적용하면 답이 하나로 정해진다(재개 시 재계산). 다만 PM이 "재개하면 바로 한 번 돌아주는 게 사용자 기대"라고 판단한다면 이는 **기획의도 확정 사항**이므로, 그 경우 코드 수정 대신 설계 §4.5에 #17행을 추가해 의도임을 못 박고 FE 문구로 알리는 것으로 닫는다.
- **C1의 수정 범위**: `resolve_validated_project_root`(`tree.rs:39`) 자체를 `dunce`로 바꾸는 쪽이 근본적이지만 `session_chat`까지 영향이 번진다. 이번 브랜치 스코프를 지키려면 `autonomy/mod.rs`의 키 생성부 2곳만 고치는 국소 수정이 맞다. → **권고: 국소 수정 + 별도 이슈로 근본 수정 기록.**

## 잘 된 점 (유지할 패턴)

1. **`select_due()` 무변경 유지가 실제로 지켜졌다.** `scheduler.rs:61-92`에 모드 개념이 한 글자도 새어 들어가지 않았고, 모드 차이가 전부 "`next_run_at`을 누가 계산하는가"로만 표현된다. 설계가 회귀 기준으로 삼은 바로 그 절약이 코드에 그대로 남았다.
2. **`pick()`을 순수 함수로 분리한 DST 테스트 설계.** `chrono-tz`(≈1MB tz 데이터) 의존을 늘리지 않고 `LocalResult::{Single, Ambiguous, None}`을 직접 주입해 DST 3분기를 결정적으로 검증한다. 다음에 tz 관련 로직을 다룰 때 그대로 가져다 쓸 패턴.
3. **`Tz: TimeZone` 제네릭 + `FixedOffset` 주입.** "테스트가 CI 러너 타임존에 의존하지 않는다"는 흔히 말만 하고 못 지키는 약속을 코드 구조로 강제했다.
4. **하위호환 3종 세트가 전부 실물 테스트로 고정됐다** — 레거시 JSON default(`legacy_json_without_schedule_fields_defaults_to_interval_mode`), `intervalMinutes` alias 보존, FixedTime에서도 `interval` clamp 유지(`normalize_still_clamps_interval_in_fixed_time_mode`). 특히 "`atTime` 파싱 실패를 Interval로 다운그레이드하지 않는다"는 fail-closed 결정을 **주석에 이유(최대 288배 과잉 실행)까지 남기고 테스트로 못 박은 것**이 모범적이다.
5. **상수 불변식을 테스트로 고정**(`missed_run_grace_is_shorter_than_minimum_occurrence_gap`). 문서의 ⚠️ 경고를 코드가 강제하게 만든 드문 사례.
6. **`missedRunGraceMinutes`를 `autonomy::config` 정본 → `LimitsSection` → FE로 흘려보내 숫자 하드코딩을 피했다.** 기존 `startupGraceMinutes` 관용구를 그대로 따랐다.

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| `next_occurrence`가 설계 §4.5 16행과 일치 | 계약감사 | 필수 | ✅ | 16/16 행 대조 완료 |
| 엄격 `>`로 동일 회차 재선택 차단(무한 재실행 방지) | 계약감사 | 필수 | ✅ | `schedule.rs:127` + 전용 테스트 |
| 레거시 `autonomy.json` 무손상 읽기 | 계약감사 | 필수 | ✅ | serde default + alias + clamp 테스트 실재 |
| 저장 후 의도 보존(다음 저장 시 `scheduleMode` 명시) | 계약감사 | 필수 | ✅ | `skip_serializing_if` 미부착 확인 |
| `try_start_now` 락 구간이 중복 spawn을 실제로 차단 | 주장-검증 | 필수 | ❌ | M1 — 자기 방향만 차단, `mark_started` 무가드 |
| TaskKey가 스케줄러/커맨드 양쪽에서 동일 | 주장-검증 | 필수 | ❌ | C1 — Windows에서 항상 불일치 |
| 모드 전환 시 `next_run_at`/`interval` 오염 없음 | 계약감사 | 필수 | ⚠️ | interval→fixedTime ✅ / fixedTime→interval ❌(m1), 중지→재개 ❌(M2) |
| `computeScheduleLabel` 호출부 전부 갱신 | 계약감사 | 필수 | ✅ | grep 전수 3곳(`:217`, `:252`) + `computeNextRunLabel` 3곳(`:219`, `:526`) 모두 새 시그니처 |
| 요일 인덱스 축 0=일 FE/BE 일치 | 계약감사 | 필수 | ✅ | `WEEKDAY_LABELS`(`:51`) ↔ `num_days_from_sunday()`(`schedule.rs:79`), 프리셋 평일[1-5]·주말[0,6]도 정합 |
| 신규 커맨드의 권한 표면 이동 없음 | 계약감사 | 필수 | ✅ | `capabilities/default.json` 무변경, `resolve_validated_project_root` 경유 확인 |
| 설계 §8 지정 테스트 29건 실재 | 계약감사 | 권장 | ✅ | schedule.rs 21 + config.rs 8, 이름까지 일치. runtime.rs는 지정 2건 + 자발적 3건 |
| 치명 경로(`next_run_after_finish` FixedTime) 직접 커버리지 | 주장-검증 | 권장 | ❌ | m2 |

## PM에게 권고

1. **병합 차단(🔴).** C1이 해소되기 전에는 Windows 빌드에 "지금 실행"을 내보내면 안 된다. 수정은 `mod.rs:118`·`mod.rs:191` 두 줄을 dunce 계열 경로로 맞추는 국소 변경이고, 스캐너 경로 문자열과 커맨드 경로 문자열의 일치를 단언하는 회귀 테스트 1건을 반드시 함께 받는다. 설계 §9-4가 요구한 "실측 확인"의 이행이기도 하다.
2. **출시 전 필수(🟠 4건).** M1(`mark_started` 가드) → M3(편집용 재스케줄 함수 분리) → M2(재개 시 재계산, M3 완료 후 그 함수를 씀) → M4(등록 직후 거짓 오류 문구) 순서로 처리하면 의존 관계가 맞는다. M1은 12줄, M2·M3은 함께 30줄 안쪽이다.
3. **M2는 착수 전 기획의도 확인 1문항.** "중지했던 고정시각 자율업무를 재개하면 ①다음 예정 시각까지 기다린다 ②밀린 회차를 지금 한 번 돈다" — 둘 중 어느 쪽인가. ①이면 코드 수정, ②면 설계 §4.5에 행 추가 + FE 안내 문구로 닫는다(트레이드오프 절 참조).
4. **백로그(🟡 3건, ⚪ 2건).** m2(FixedTime 완료-후 재계산 테스트)는 PM이 "유일한 치명 경로"로 꼽은 곳의 커버리지라 백로그 중 최우선 권장. m1·m3·n1·n2는 차기 반영으로 충분.
5. **R1(계기 enum)은 채택/보류 판단이 필요하다.** M2·M3·m1을 개별 패치로 막으면 다음 계기가 생길 때 같은 형태가 재발한다. 셋을 한 번에 고칠 계획이라면 R1 구조로 가는 편이 총비용이 낮을 수 있다(근거: 세 결함 모두 "어느 앵커를 쓸지"가 호출부에 흩어져 있다는 동일 원인 — 관련 유틸이 이미 `schedule.rs` 한 파일에 모여 있어 이동 비용이 낮음을 실물로 확인했다).

## 생략한 것 (정직 보고)

- **화면 캡처 없음.** 이 리뷰는 Tauri 앱 실기동 없이 코드·설계서 대조만으로 수행했다. `docs/screenshots/`에 이번 라운드 이미지가 없다. M4(거짓 오류 문구)·m1(모순 라벨)은 **코드 추적 기반 예측**이며 실제 렌더링으로 확인하지 않았다 — PM이 실기동 시 우선 확인 항목으로 삼기를 권한다.
- **Windows 실기 미검증.** C1은 `dunce`/`std::fs::canonicalize` 동작 차이(저장소 자신이 `dev_tools/classify.rs:459-462`에 문서화)와 소스 경로 추적에 근거한 판정이며, 실제 Windows 머신에서 재현하지 않았다.
- **발산형 페르소나 미투입.** Standard/약식 등급에 따른 의도적 생략(위 Rethink 절 명시).
- **PM 선검증 항목 재실행 없음.** `cargo test` 379 passed, `tsc --noEmit`, `select_due`/clamp 무변경, `capabilities/default.json` 무변경은 지시대로 재실행하지 않고 그대로 신뢰했다. 단 `select_due` 무변경은 `scheduler.rs:61-92`를 직접 읽어 독립 확인했다.
- **스코프 아웃 항목 미검토**: 보고서 발송·알림 채널, task 설명 필드 분리, 무작위 지연 문구, OS 스케줄러/launchd 연동.
