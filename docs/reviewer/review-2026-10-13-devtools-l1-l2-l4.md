# 리뷰 보고서 — dev_tools L1·L2·L4 (PATH 프로브 상한 · 락 poison 회복/미리보기 동시성 · 신뢰 brew prefix 고정)

리뷰 대상: 커밋 범위 `a0e9d38..08d809a` (afb079e, 08d809a) / target_id `devtools-l1-l2-l4` / 1차(최초)
- 등급: Standard (트리거: 4개 연동 — dev_tools·github_integration·cloudflare_integration·claude_auth/plugins — 이 공유하는 `cli_launcher::resolve_binary` 프로세스 실행 코드 + 신뢰 prefix 판정)
- 원 지적: hub 이슈 `01m2mykxgry7zgjxntwyn10jrb` (L1, L2, L4)
- 리뷰 일자: 2026-10-13 / 리뷰어: reviewer
- 실행 액션: 없음. 코드 수정·커밋·push 하지 않았다(읽기와 `cargo test` 실행만 했다). 페르소나 파일 4개의 "적용 이력"과 INDEX.md의 "최근 재사용" 열에 이번 라운드 줄을 덧붙였다.

## 종합 판정: 🟢 Green

| 심각도 | 건수 |
|---|---|
| 🔴 Critical | 0 |
| 🟠 Major | 0 |
| 🟡 Minor | 2 |
| ⚪ Nit | 3 |
| 🔵 Rethink | 1 |

원 지적 3건(L1, L2, L4)은 코드와 테스트 양쪽에서 모두 닫혔다. 회귀 우려로 받은 항목(비0 종료를 "발견"으로 보는 의미, pid 0/1에 대한 음수 kill, process_group(0)의 부작용, 정상 도구 발견)은 하나도 결함으로 확인되지 않았다(아래 "기각된 지적" 참고). 남은 것은 UX·진단성 Minor 2건이다. **단, Windows 컴파일은 로컬에서 확인하지 못했다** — 아래 "미확인 사항" 참고.

## 페르소나 재사용/신규 판정

INDEX.md(`docs/reviewer/personas/INDEX.md`)를 착수 전에 읽어 "역할개념" 열만 대조했다. 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 유형 | 사유 |
|---|---|---|---|
| `persona-process-execution-security.md` | 재사용 | 수렴 | 역할개념("명령의 argv·셸·env·프로세스 수명이 안전한가")이 L1 프로브의 spawn/kill/수거, L4 신뢰 prefix에 그대로 들어맞는다 |
| `persona-tauri-runtime-thread-and-lock-discipline.md` | 재사용 | 수렴 | 역할개념("블로킹을 락 밖으로 뺐는가, 사라진 직렬화를 누가 대신하는가")이 EXECUTION_LOCK 보유 시간 상한, poison 회복, PREVIEW_LOCK 신설에 그대로 들어맞는다 |
| `persona-heterogeneous-machine-operator.md` | 재사용 | 수렴 | 역할개념("이기종 머신에서 실제로 뜨는가, 안 될 때 막다른 골목인가")이 3초 프로브 오판과 비표준 brew prefix 사용자가 잃는 것에 그대로 들어맞는다 |
| `persona-zero-base-redesigner.md` | 재사용 | 발산 | 역할개념 그대로 유효하다. "PATH 폴백에서 프로세스를 띄울 필요가 애초에 있는가"를 묻는다 |

Standard 등급이라 약식(1~2명)도 가능했지만, 중점 확인 항목이 프로세스 수명·락·이기종 머신 세 축에 걸쳐 있어 수렴형 3명과 발산형 1명으로 구성했다.

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 확인방법 | 개선안 | 페르소나 |
|---|---|---|---|---|---|---|
| RV-001 | 🟡 Minor | `src-tauri/src/process_util.rs:198-216`, `src-tauri/src/cli_launcher.rs:81` | 프로브가 3초 상한을 넘기면 로그 없이 `false`가 돌아가고, 호출부는 이를 "미설치"(None)로 바꾼다. 느린 첫 실행(Gatekeeper 첫 평가, asdf/mise 같은 셸 shim의 콜드 스타트)으로 상한을 넘기면 실제로 있는 도구가 화면에 "미설치"로 보이고 설치 제안이 뜬다. 이 경우를 사후에 진단할 단서가 남지 않는다. 기존 `.output()`은 무기한 기다렸기 때문에 이 오판이 생길 수 없었다 — 이번 변경으로 새로 생긴 실패 모드다. | diff와 본문을 열어 읽었다. 타임아웃 분기(`_ => break` → kill → `false`)에 `eprintln!`이 없다. 같은 파일의 다른 kill 경로나 `locks.rs:41`은 eprintln을 남긴다 | 타임아웃 분기에 `eprintln!("[probe] {name} --version이 {limit:?} 안에 끝나지 않아 미발견으로 처리")` 한 줄을 추가한다. 비용은 1줄이다(근거: 같은 모듈이 이미 eprintln 진단 패턴을 쓴다) | 이기종 머신 운영자, 프로세스 실행 보안 |
| RV-002 | 🟡 Minor | `src-tauri/src/dev_tools/query.rs:369-388`, `src-tauri/src/dev_tools/locks.rs:20`, `src/views/devTools.ts:338-345, 473-474` | PREVIEW_LOCK이 **도구를 가리지 않는 전역 1건**이다. (a) "전체 업데이트" 루프는 순차 `await`라 루프 자체끼리는 부딪히지 않는다(devTools.ts:334-362). 그러나 루프가 도는 동안에도 다른 도구 카드의 미리보기 버튼은 활성 상태다(비활성화 조건은 해당 도구의 `previewLoading`뿐, :473). 사용자가 다른 도구 미리보기를 누른 순간 루프가 그 다음 도구 미리보기에 도달하면 busy로 거부되고, 루프는 `continue`로 그 도구를 **조용히 건너뛴다**(토스트 1개). 예전에는 둘 다 실행됐다. (b) brew dry-run 하나가 최대 120초 걸리는 동안, 자식 프로세스를 띄우지 않는 미리보기(설치 미리보기 등)까지 모두 거부된다. 잃는 것은 없고(재시도하면 됨) 파괴적이지도 않아 Minor로 둔다 | 프론트 호출부 2곳과 버튼 비활성화 조건을 grep으로 찾아 열어 확인했다. 백엔드 거부 경로는 `preview_rejects_concurrent_request_while_one_is_running_then_allows_after` 테스트로 확인했다 | 가장 싼 방법: `updatingAll` 동안 개별 미리보기/실행 버튼을 비활성화한다(devTools.ts의 `renderActionArea`에 조건 1개 추가). 백엔드에서는 락 대상을 "자식 프로세스를 띄우는 미리보기"로 좁히거나, 짧은 대기(예: 수백 ms) 뒤 거부하는 방안이 있다 — 후자는 미확인 추정치 | 락 규율, 발산형 |
| RV-003 | ⚪ Nit | `src-tauri/Cargo.toml:96`, `src-tauri/src/dev_tools/locks.rs:36-46`, 커밋 08d809a 메시지 | release 프로파일이 `panic = "abort"`라서 배포 빌드에서는 poison이 일어날 수 없다(패닉하면 앱 자체가 종료된다). "패닉 한 번이 업데이트를 영구 거부" 문제는 dev 빌드에서만 재현된다. 회복 코드 자체는 맞고 해도 없다 — 커밋 메시지와 주석이 실제 영향 범위를 과장할 뿐이다 | Cargo.toml `[profile.release]`를 열어 확인했다. 테스트는 cargo 규칙상 panic 설정을 무시하므로 `--release` CI에서도 poison 테스트는 유효하다 | 주석에 "release는 panic=abort라 dev/test 빌드 방어용"이라고 한 줄 적는다 | 락 규율 |
| RV-004 | ⚪ Nit | `src-tauri/src/process_util.rs:192-205` | `try_wait()`가 `Err`를 내도 `_ => break`로 그룹 SIGKILL까지 간다. `Err`가 나는 현실적인 경우는 자식이 다른 곳에서 이미 수거됐을 때(ECHILD)뿐이고, 그때는 pid가 재사용됐을 수 있다 → 엉뚱한 프로세스 그룹을 kill할 이론적 가능성이 있다. 저장소에는 `waitpid`·`SIGCHLD` 처리가 없어(grep 결과 0건) 현재 도달 경로는 없다 | `grep -rn "waitpid\|SIGCHLD"` 0건. 루프 분기는 본문을 읽어 확인했다 | `Err(_) => return false`로 분기를 분리해 kill을 건너뛴다 | 프로세스 실행 보안 |
| RV-005 | ⚪ Nit | `src-tauri/src/process_util.rs` 테스트(~:330-387), `src-tauri/src/cli_launcher.rs` 테스트(~:273-319) | 새 spawn 테스트 두 개가 `dev_tools::process::TEST_SPAWN_LOCK`(process.rs:215, R3에서 도입)을 거치지 않는다. macOS는 비원자 CLOEXEC라서, 이 테스트가 fork하는 순간 다른 테스트의 파이프 쓰기 끝을 손자 `sleep`이 물 수 있다. 다만 손자가 400~500 ms 만에 그룹 kill되므로 상대 테스트의 지연 상한은 DRAIN_GRACE(2초)보다 작다 | 전체 스위트 7회 + 대상 테스트 5회를 실행해 전부 통과했다(아래 "검증 실행" 참고). 플레이크는 관측되지 않았다 | 지금은 조치가 필요 없다. 플레이크가 관측되면 그때 테스트 spawn 락을 `process_util`로 옮겨 공유한다 | 락 규율 |
| RV-006 | 🔵 Rethink | `src-tauri/src/cli_launcher.rs:56-85` | **macOS PATH 폴백에서 프로세스를 띄울 이유가 남아 있는가.** Windows 분기는 이미 `platform::resolve_binary_with(plat, …, &path_var, …)`로 PATH를 파일시스템에서 스캔하고 **프로세스를 하나도 띄우지 않는다**(:57-62, 66-72). macOS만 `path_var = String::new()`(:63)로 스캔을 끄고 `--version` 실행에 의존한다. 그런데 기존 의미는 "spawn이 성공하면 발견"(종료 코드 무시)이었으므로, 실제로 검증하던 것은 "PATH 위에 실행 가능한 파일이 있다"뿐이고 이는 정적 스캔으로 동등하게 판정된다. | 두 분기를 나란히 읽고 `platform_now()`를 확인했다(Mac/Win 2값) | 대안 구조: macOS도 `path_var = std::env::var("PATH")`로 스캔하고 실행 비트를 확인한 뒤 **절대경로**를 반환한다. 그러면 (1) L1 결함 부류(멈추는 프로브·락 보유 시간·좀비)가 통째로 사라지고, (2) 탐지를 위해 PATH상의 임의 바이너리를 실행하는 일이 없어지며, (3) 3초 오판(RV-001)도 사라지고, (4) 이후 실행 시 PATH를 다시 해석하는 TOCTOU도 없어진다. 대가: 반환값이 bare name에서 절대경로로 바뀌므로 호출부 4곳의 표시·비교를 점검해야 한다 — 비용은 미확인 추정치. 이번 수정(상한 부여)은 올바른 단기 조치이므로 되돌릴 필요는 없다 | 발산형 |

## 원 지적별 닫힘 확인

| 원 지적 | 판정 | 근거(확인방법) |
|---|---|---|
| L1 PATH 폴백 `--version` 무기한 대기 | **닫힘** | `cli_launcher.rs:81`이 `command_completes_within(…, PATH_PROBE_TIMEOUT=3s)`(:47)를 쓴다. 상한 초과 시 그룹 SIGKILL(`process_util.rs:204`) + 수거 상한 1초(:209, :220)를 둔다. EXECUTION_LOCK 추가 보유는 프로브 1회당 최대 약 4초로 유한하다. perform_update는 `resolve_tool_path` 1회와 `ResolvedRunners::resolve`의 brew/npm/pnpm 몇 회를 부르므로 최악 합계도 유한하다(무기한 → 유한). 테스트 `resolve_binary_path_fallback_returns_none_within_limit_when_version_hangs`는 예전 `.output()`이었다면 30초 대기로 elapsed<4s 단언에서 실패한다 — 결함을 실제로 고정한다(코드 읽기로 판정했고, 변이 실행은 하지 않았다: 코드 수정 금지) |
| L2a EXECUTION_LOCK poison 영구 Err | **닫힘** (영향 범위는 dev 빌드 — RV-003) | `locks.rs:36-46`이 Poisoned를 회복하고 WouldBlock만 None으로 돌려준다. `clear_poison`은 가드를 쥔 상태에서 호출하므로 순서가 안전하다. 보호 대상이 `()`라 깨진 불변식이 없다. actions.rs:83, :241 두 곳 모두 교체됐다. 테스트 `poisoned_lock_is_recovered_instead_of_rejecting_forever`가 회복 뒤 WouldBlock 구분까지 고정한다 |
| L2b preview 무제한 동시 실행 | **닫힘** (UX 부작용 — RV-002) | `mod.rs:276`이 `perform_preview_exclusive`로 교체됐다. 알 수 없는 id는 락 전에 거부한다(query.rs:383). 동시 거부·해제 후 재허용·poison 회복은 테스트 2건이 주입한 락과 본문으로 결정적으로 검증한다(타이밍 의존 없음, 채널 동기화) |
| L4 HOMEBREW_PREFIX가 신뢰 prefix 확대 | **닫힘** | `classify.rs:233` 고정 목록을 쓰고, 저장소에서 `HOMEBREW_PREFIX`를 읽는 코드는 0건이다(grep에서는 주석 1줄만 나옴). 테스트 `homebrew_prefix_env_var_does_not_promote_arbitrary_cellar_to_formula`는 예전 코드라면 HomebrewFormula가 나와 실패한다 — 결함을 고정한다. 표준 두 prefix는 `standard_prefixes_still_classify_as_formula_with_defaults`로 회귀를 막는다 |

## 페르소나별 관점

### 프로세스 실행 보안 (`persona-process-execution-security.md`)
- 음수 pid kill: `child.id()`는 방금 spawn한 자식이라 0이나 1일 수 없다. `process_group(0)`이므로 pgid는 pid와 같다. kill 시점까지 자식을 수거하지 않았으므로(잘해야 좀비) pid 재사용이 일어날 수 없다 → 안전하다. 예외는 `Err` 분기뿐이다(RV-004, Nit).
- `u32 → i32` 캐스트: macOS pid 상한(99998)이 i32 범위 안이다. 기존 `process_util.rs:87,93`과 같은 관용구다.
- stdin/stdout/stderr null: 예전 `.output()`도 stdin을 null로 두었으므로 의미 변화가 없다. 출력을 버리므로 파이프를 물고 있는 손자 때문에 대기할 일도 없다.
- 탐지를 위해 PATH상의 바이너리를 실행하는 구조 자체는 이번 범위 밖의 기존 동작이다 → RV-006에서 대안을 낸다.

### 락 규율 (`persona-tauri-runtime-thread-and-lock-discipline.md`)
- EXECUTION_LOCK과 PREVIEW_LOCK을 독립시킨 근거(plan_id를 락 안에서 다시 계산)는 locks.rs:12-19 주석과 일치한다. 미리보기가 실행을 막지 않고, 실행도 미리보기를 기다리지 않는다 → 데드락이나 상호 대기 경로가 없다.
- 프론트 "전체 업데이트"는 순차 `await`라 PREVIEW_LOCK과 자기충돌하지 않는다. 사용자 동시 조작과의 상호작용은 RV-002.
- 가드 수명: `_guard`가 함수 끝까지 살아 있다(`_`로 즉시 drop되는 실수가 없다). 두 락 모두 마찬가지다.

### 이기종 머신 운영자 (`persona-heterogeneous-machine-operator.md`)
- L4로 잃는 것: Finder나 Dock으로 띄운 GUI 앱은 셸 프로파일의 `HOMEBREW_PREFIX`를 상속하지 않는다(macOS launchd 환경의 일반 동작이며, 이 머신에서 실측하지는 않았다). 따라서 배포 사용자는 원래 이 확장의 혜택을 받지 못했다. 잃는 사람은 "터미널에서 `pnpm tauri dev`로 띄운 개발자이면서 비표준 prefix에 brew를 설치한 사람"뿐이고, 그것도 Unknown(수동 안내)으로 안전하게 강등된다. 게다가 `runners.rs:13` BREW_CANDIDATES가 원래 표준 두 경로뿐이었으므로, 이번 고정은 러너 해석과 분류 기준을 **일치**시키는 효과도 있다.
- 3초 프로브 오판은 RV-001. 막다른 골목은 아니다(재로드하면 다시 시도한다). 다만 진단 단서가 없다.
- Windows: 프로브 분기는 `plat == Mac`에서만 실행된다. Windows에서는 `command_completes_within`이 컴파일만 되고 호출되지 않는다. 유닉스 전용 항목(`CommandExt`, `process_group`, `libc::kill`)은 전부 `#[cfg(unix)]`로 막혀 있고, 새 테스트 중 spawn과 env를 쓰는 것도 `#[cfg(unix)]`다. `libc`는 무조건 의존성이라 Windows에서도 크레이트가 존재한다. 코드 검사로는 컴파일 문제가 보이지 않는다 — 로컬 확인은 실패했다(아래 "미확인 사항").

### 제로베이스 재설계자 (`persona-zero-base-redesigner.md`, 발산)
- RV-006: "멈출 수 있는 프로브에 상한을 붙이는 것"보다 "프로브를 없애는 것"이 근본 해법이다. Windows 분기가 이미 그 구조로 동작한다는 점이 근거다.
- RV-002의 대안 관점: 미리보기 동시성을 백엔드 락으로 거부하기보다, 화면이 "한 번에 한 미리보기"라는 사실을 버튼 상태로 보여주는 편이 사용자 입장에서 정직하다.

## 트레이드오프

| 쟁점 | A안 | B안 | 권고 |
|---|---|---|---|
| 프로브 상한 3초 | 짧게(락 보유↓, 느린 도구 오판↑) | 길게(오판↓, 멈춤 시 락 보유↑) | 현행 3초를 유지하고 RV-001 로그를 추가한다. 근본 해법은 RV-006(상한 자체가 필요 없어짐) |
| PREVIEW_LOCK 거부 vs 대기 | 즉시 거부(현행, 자식 누적 차단 확실) | 짧은 대기 후 거부(배치 건너뜀↓) | 현행을 유지하고 프론트에서 배치 중 버튼을 비활성화(RV-002 최저비용안) |

## 잘 된 점

- **결함을 고정하는 테스트**: L1·L4 테스트 모두 수정 전 코드라면 실패한다. 특히 L1은 "손자까지 정리"를 pid 파일로 검증해, 직속 자식만 kill하는 회귀까지 잡는다.
- **동시성 테스트가 타이밍에 의존하지 않는다**: PREVIEW_LOCK과 poison 테스트는 락과 본문을 주입하고 채널로 동기화해 결정적이다. 전역 락이나 실제 brew를 건드리지 않는다.
- **의미 보존을 명시적으로 단언**: `exit 7`도 `true`라는 단언(process_util 테스트)이 기존 `.output().is_ok()` 의미를 고정한다.
- **재사용하지 않은 이유를 문서화**: `command_completes_within` 주석이 `run_process_with_timeout`을 쓰지 않은 이유 네 가지(파이프 버퍼, PATH 덮어쓰기, SIGTERM 유예로 락 보유가 늘어남, 모듈 의존 방향)를 적어 다음 사람의 "왜 중복이냐" 질문에 답한다.
- **수거에도 상한**: D 상태 자식을 무기한 `wait()`하지 않는다 — 락 보유 상한이 진짜로 유한하다.

## 기각된 지적

| 후보 지적 | 기각 사유(확인방법) |
|---|---|
| "비0 종료를 미발견으로 바꿔 기존 의미가 깨진다" | 아니다. `Ok(Some(_)) => return true`(:194)가 종료 코드와 시그널 종료를 구분하지 않는다. 테스트가 `exit 7 → true`를 단언한다 |
| "`libc::kill(-pid)`가 pid 0/1에서 위험하다" | 방금 spawn한 자식의 pid는 0이나 1일 수 없고, 수거 전이라 재사용도 없다. Err 분기만 RV-004(Nit)로 남겼다 |
| "process_group(0) 때문에 tty/시그널 부작용이 생긴다" | 표준 스트림 3개가 모두 null이고 GUI 앱에는 제어 터미널이 없다. dev 모드에서 Ctrl-C가 프로브에 전파되지 않는 차이는 3초 상한으로 덮인다. 재현 경로를 대지 못해 기각한다 |
| "HOMEBREW_PREFIX를 set_var하는 테스트가 병렬 테스트를 오염시킨다" | 이 변수를 읽는 코드가 0건이다(grep). std의 env 접근은 내부 락으로 직렬화된다. 7회 전체 실행에서 실패가 없었다 |
| "process.rs에서 EXECUTION_LOCK을 제거해 `Mutex` import가 남는다(경고)" | 비테스트 `cargo check`에서 경고 0건이다. `Mutex`는 :220 등에서 계속 쓰인다 |
| "check_dev_tools 화면 로드가 프로브 때문에 느려진다" | 정상 도구는 수백 ms 안에 끝나고, 멈추는 도구는 예전에는 **무기한**이었다 → 오히려 개선이다 |

## 검증 실행 (verified)

- `cd src-tauri && cargo test`(macOS aarch64, debug): **721 passed / 0 failed / 21 ignored** — 7회 반복 실행해 모두 통과했다(각 약 3.8초).
- 대상 필터(`command_completes_within resolve_binary_path_fallback preview_ poisoned homebrew_prefix standard_prefixes`): 13 passed를 5회 반복해 모두 통과했다.
- 비테스트 `cargo check`: 경고와 에러 0건.

## 미확인 사항 (정직 보고)

- **Windows 컴파일 미확인**: `cargo check --tests --target x86_64-pc-windows-msvc`를 시도했지만 의존성 `ring`의 C 빌드가 로컬 크로스 툴체인 부재(`assert.h` not found)로 실패했다 — 이 저장소 코드와는 무관한 환경 문제다. 08d809a와 afb079e는 아직 push되지 않아 CI(windows-latest) 결과도 없다(`gh run list`의 최신 실행은 a0e9d38). 코드 검사로는 cfg 누락을 찾지 못했지만, **push 뒤 ci.yml의 rust-test(windows-latest)가 green인지 PM이 확인해야 한다.**
- release 프로파일 테스트(`cargo test --release`)는 로컬에서 돌리지 않았다(CI가 수행한다).
- 변이 테스트(수정 전 코드로 되돌려 테스트가 실패하는지 확인)는 코드 수정 금지 제약 때문에 하지 않았다. 판정은 코드 읽기로 했다.
- UI 화면 캡처는 생략했다. 이번 변경은 백엔드만 바꾸고 화면 문구는 기존 토스트 경로를 재사용하기 때문이다(RV-002는 코드 읽기로 판정했다).

## PM 권고

1. **병합 가능 (Green)**. push 뒤 CI windows-latest를 반드시 확인한다.
2. RV-001(로그 1줄)과 RV-002의 최저비용안(배치 중 개별 버튼 비활성화)은 다음 소규모 배치에 묶는다.
3. RV-006(macOS PATH 정적 스캔 전환)은 백로그에 올린다 — L1 결함 부류를 근본적으로 제거한다. 호출부 4곳 점검이 필요하며 비용은 미확인 추정치다.
4. RV-003·004·005는 선택 사항이다.
