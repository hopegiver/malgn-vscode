# 자율업무 상세화면 재설계 리뷰 보고서 (약식 2인 패널 — Standard 등급)

target_id: `autonomy-detail-redesign`
리뷰 페르소나 패널: `docs/reviewer/personas/persona-claimed-vs-verified.md`, `docs/reviewer/personas/persona-design-contract-auditor.md` (둘 다 **재사용**, 신규 0)
리뷰 대상: 커밋 `6ccf36f`(백엔드) + `a47b927`(프론트), 베이스 `55dcbf2`
 - `src-tauri/src/autonomy/log.rs`, `src-tauri/src/autonomy/mod.rs`, `src-tauri/src/lib.rs`
 - `src/autonomyApi.ts`, `src/views/autonomousTasks.ts`, `src/styles.css`, `scripts/capture/fixtures.mjs`
설계 정본: `docs/design/autonomy-task-detail-redesign.md`
리스크 범주: 로컬 파일시스템 읽기 전용 스캔 + 단일 화면 UI 재배치 (인증·권한·결제·개인정보 경계 미이동)
리뷰 일자: 2026-09-17
작업 등급: Standard (PM 지정) → 약식 2인, 발산형 생략(등급 규정상 허용). 발산형 미투입 사실은 아래 "구조적 제언" 절에 명시.
**종합 판정: 🟡 Amber** (Critical 0 / Major 3 / Minor 2 / Nit 4)

## 요약 (2분 규칙)

재설계 자체는 요구사항 4개(①이력 노출 ②정보위계 ③버튼 재배치 ④전체 재설계)를 모두 충족했고, 설계 정본 §4-2~§4-6·§5의 수치 조항은 **전수 일치**한다. 백엔드 파싱·정렬·limit·손상파일 격리도 PM이 지목한 방식대로 정확히 구현돼 있다.

그러나 병합 전 반드시 고칠 것이 하나 있다. **상세 화면에 들어갈 때마다 사이드바와 본문이 통째로 2벌 그려지는 프레임이 실제로 발생한다**(M1). `loadTaskHistory`가 렌더 함수 실행 도중 `notifyChange()`를 동기 호출해 `renderApp()`에 재진입하기 때문이며, 이는 `src/main.ts:145-147`이 이 저장소 스스로 금지해 둔 패턴이다. 코드 리딩이 아니라 playwright 실측으로 재현했다(`#app` 자식 2→4개, sidebar 2개, main 2개, 기본 픽스처 기준 98ms 지속 — 증거 스크린샷·재현 스크립트 첨부). 캡처 하네스가 이를 놓친 이유는 IPC가 해소된 **뒤에** 스크린샷을 찍기 때문이다.

나머지 Major 2건은 "마지막 실행 요약이 화면에서 완전히 사라진 것"(M2, PM 승인 이탈 (b)의 범위 밖)과 "손상 로그 1건이 '더 보기'를 영구히 숨기는 백엔드-프론트 계약 불일치"(M3)다.

---

## 페르소나 재사용 판정 (산출물 게이트)

착수 전 `docs/reviewer/personas/INDEX.md`를 Read해 역할개념 열을 스크리닝했다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-claimed-vs-verified.md` | **재사용** | INDEX 역할개념 "'성공'이라 말하는 근거가 주장인가 관측인가, 코드가 자기 주석과 일치하는가"가 PM 중점질문 3(재조회 루프)·1(쓰는 쪽/읽는 쪽 대칭)과 정확히 겹친다. 6대 요소 무수정, 적용 이력만 append. |
| `persona-design-contract-auditor.md` | **재사용** | INDEX 역할개념 "구현이 설계 정본의 조항·불변식을 조항 단위로 지켰는가, 사라진 조항은 없는가"가 PM 중점질문 4(설계-구현 정합)와 동일 과녁. 6대 요소 무수정, 적용 이력만 append. |
| (신규) | **0건** | 이번 리스크 표면(로그 파일 읽기 전용 스캔 + 단일 화면 재배치)에 대해 INDEX 11행 중 위 2개로 충분히 덮인다. `persona-screen-state-honesty.md`(로딩/빈/에러)도 후보였으나 설계 §4-6 4상태 검증이 design-contract-auditor의 조항 대조에 그대로 포함돼 별도 투입이 중복이라 생략했다(Standard 약식 1~2인 규정과도 일치). |

---

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| **M1** | 🟠 Major | claimed≠verified | `src/views/autonomousTasks.ts:1106` (호출경로 `:1301`→`:1134`→`:1102`) / 위반 규칙 `src/main.ts:145-147` | **실측** — 프로젝트 자체 캡처 스텁(`scripts/capture/stub.mjs`) + playwright로 `#app` 자식 수를 MutationObserver로 추적. 스크립트·스크린샷 첨부 | `loadTaskHistory`가 `historyCache.set` 직후 `notifyChange()`를 **동기 호출**하는데, 이 함수는 `renderRunHistoryCard`(`:1301`)의 렌더 경로에서 호출된다. `renderApp()`(main.ts:81) 실행 도중 `renderApp()`에 재진입해, 안쪽 렌더가 `root.replaceChildren()` 후 sidebar+main을 붙이고 → 바깥 렌더가 그 위에 sidebar+main을 **또** 붙인다. 결과: `#app` 자식 4개(sidebar×2, main×2)가 화면에 그대로 페인트된다. 이력 IPC가 해소되는 순간 다음 top-level 렌더가 정리하므로 자가 치유되지만, **상세 화면 진입 시마다 한 번씩 발생**한다 | `:1106`의 `notifyChange()`를 `queueMicrotask(notifyChange)`로 감싸면 바깥 `renderApp()`이 완전히 반환한 뒤에 실행돼 재진입이 사라진다(1줄). 더 근본적으로는 main.ts:145-147이 말하는 원칙대로 최초 조회 트리거를 렌더 밖(`handleNavigation` 또는 `loadAutonomousTasks` 완료 시점)으로 빼는 것이 낫다. 재발 방지로는 캡처 하네스가 아니라 `#app`의 `childElementCount`를 IPC 해소 **전에** 검사하는 회귀 체크가 필요하다 |
| **M2** | 🟠 Major | 설계정본 대조 / claimed≠verified | 제거된 `overviewRow('마지막 실행 요약', task.summary)` (`a47b927` diff 구 1099행) / 현재 `src/views/autonomousTasks.ts:228`에 매핑만 존재 | `grep -n "task\.summary\|\.summary" src/views/autonomousTasks.ts` → **228행 1건(매핑)뿐, 렌더 사용처 0**. 구버전 표시 코드는 `git show a47b927`의 삭제 라인에서 확인 | 재설계 전 상세 화면은 `task.summary`(직전 실행 stdout/stderr 꼬리)와 `task.logPath`를 조건부로 표시했다. 재설계 후 **어느 화면에서도 렌더되지 않는다**. PM 승인 이탈 (b)는 "이력 **각 건**의 summary 생략(`RunHistoryEntry`에 없는 필드)"까지만 덮는다 — `AutonomyRuntimeStatus.summary`는 실재하고 이전엔 표시됐으므로 그 승인 범위 밖의 조용한 누락이다. 설계 §1 위계표 10번도 이 항목을 "3차로 **남긴다**"고 판정했지 없앤다고 하지 않았다. 영향: 설계 §1이 든 화면 방문 2대 목적 중 하나("왜 실패했는지 진단")에 대해, 앱 안에서 실패 출력을 볼 유일한 수단이 사라지고 로그 경로 복사→터미널이 강제된다 | 기록 카드 최상단(또는 최신 행 펼침 시)에 "가장 최근 실행 요약"으로 `task.summary`를 되살린다. 최신 이력 행과 `task.lastStartedAt` 비교로 동일 실행인지 판정해 붙이면 "이력 각 건에 summary가 없다"는 승인 이탈과 충돌하지 않는다. **이것이 기획 의도와 다르다면(= 의도적 제거였다면) 코드 수정이 아니라 설계 §1 위계표 10번을 "제거"로 갱신하는 것이 맞는 조치다 — PM 확인 필요** |
| **M3** | 🟠 Major | claimed≠verified (계약 불일치) | `src-tauri/src/autonomy/log.rs:246-251`(주석)·`:310-315`(truncate 후 filter_map) ↔ `src/views/autonomousTasks.ts:1113` | 양쪽 원문 대조 | 백엔드는 스스로 "손상 파일이 상위 `limit`개 안에 여러 개 섞이면 결과가 `limit`보다 **적게** 나올 수 있다"고 주석에 명시한다(`candidates.truncate(limit)` **후에** `filter_map`으로 손상분을 떨어뜨리므로 구조적으로 그렇다). 프론트는 바로 그 "반환 건수 < 요청 limit"을 "더 이상 없음" 신호로 쓴다(`hasMore: items.length >= effectiveLimit`). 결과: **손상 로그 1건만 상위 20개 안에 있어도 "더 보기" 버튼이 사라져 그 뒤 이력 전체에 접근할 수 없다.** 설계 §8이 "별도 총 개수 API 없이도 정확하다"고 단언한 판정이 백엔드의 실제 계약에서는 정확하지 않다 | 셋 중 하나. (a) 백엔드가 `truncate(limit)` 대신 유효 항목이 `limit`개 채워질 때까지 후보를 소비하고, 성능 보장을 위해 열어보는 파일 수에 `limit*2` 상한을 둔다. (b) 반환 타입을 `{ entries, scannedCandidates }`로 바꿔 프론트가 `scannedCandidates >= limit`으로 판정한다. (c) 최소 처방 — 프론트 `hasMore` 판정을 `items.length >= effectiveLimit`에서 "직전 조회보다 건수가 늘었는가"로 바꾼다. (a)가 사용자 체감상 가장 정확하다 |
| **m4** | 🟡 Minor | claimed≠verified | `src-tauri/src/autonomy/mod.rs:289-297` (`#[tauri::command] pub fn autonomy_task_history`) | 선언부 원문 + 같은 파일 `:120-121`의 `pub async fn autonomy_list` 대조, `grep`으로 저장소 전체 커맨드 동기/비동기 분포 확인 | Tauri 2에서 동기 커맨드는 메인 스레드에서 실행된다. 이 커맨드는 날짜 디렉터리 전수 `read_dir` + 후보 정렬 + 최대 `limit`개 파일 `open`+`BufRead`를 수행한다. 보존 30일치가 쌓인 프로젝트에서는 메인 스레드(=UI)가 그만큼 멈춘다. 같은 모듈에서 비슷한 규모의 스캔을 하는 `autonomy_list`는 이미 `pub async fn`이다 — 신규 커맨드만 관례에서 이탈했다. (저장소에 동기 IO 커맨드가 다수 있어 Critical/Major는 아니다) | `pub fn` → `pub async fn`. 본문 변경 불필요 |
| **m5** | 🟡 Minor | 설계정본 대조 | `src/views/autonomousTasks.ts:1307-1310` | 분기 순서 원문 확인 | `else if (cache.error)` 분기가 `cache.items.length > 0`보다 앞서므로, **"더 보기"가 실패하면 이미 보고 있던 20건이 통째로 사라지고 에러 배너만 남는다**. `loadTaskHistory`의 catch는 `items: prev?.items ?? []`로 기존 목록을 성실히 보존해 두는데(`:1118`) 렌더가 그것을 쓰지 않는다 — 보존 코드와 렌더 분기가 어긋나 있다. 설계 §4-6은 에러 표시가 "이력 카드 안에만 국한된다"고만 정했을 뿐 기존 목록 파괴는 요구하지 않았다 | 에러 분기를 `cache.error && cache.items.length === 0`으로 좁히고, items가 있는 에러는 목록을 유지한 채 하단에 배너를 병기한다 |
| n1 | ⚪ Nit | claimed≠verified | `src-tauri/src/autonomy/log.rs:124-130`(쓰기) ↔ `:326-352`(읽기) | 두 포맷 문자열 손 대조 + `src-tauri/src/autonomy/config.rs:121-144`(`normalize_task`)에 name 정규화 없음 확인 + `src/views/autonomousTasks.ts:727`이 `<input>`(개행 입력 불가)임 확인 | **PM 질문 1에 대한 회답: `" / "` 구분자에 관한 한 두 쪽은 대칭이다.** `rsplitn(4, " / ")`로 오른쪽 3필드만 떼는 방식은 task 이름·프로젝트 경로에 `/`나 `" / "`가 들어와도 깨지지 않으며(오른쪽 3필드는 RFC3339 2개 + 정수 1개로 `/`를 구조적으로 포함할 수 없다), `log.rs:475-476`에 이름에 `" / "`를 심은 회귀 테스트도 있다. 유일한 비대칭은 **개행**이다 — writer는 `task_name`·`project_path`를 이스케이프 없이 2행에 끼워넣는데 reader는 2행이 정확히 한 줄이라고 가정한다. 개행이 섞이면 그 실행 건이 **아무 신호 없이** 이력에서 사라진다. UI는 `<input>`이라 개행 입력이 불가능하고 손으로 고친 `autonomy.json`에서만 가능하므로 Nit | `write_run_log`에서 name/project의 `\r`·`\n`을 공백으로 치환하거나, 기계 판독 필드만 별도 줄(`meta: started=… finished=… durationMs=…`)로 분리해 사람이 쓴 문자열과 섞지 않는다 |
| n2 | ⚪ Nit | claimed≠verified | `src-tauri/src/autonomy/log.rs:326` | 원문 확인 | `reader.lines().take(4).filter_map(\|l\| l.ok())`는 UTF-8 오류 줄을 **버려서 인덱스를 당긴다** — `lines[1]`이 실제 2행이 아닐 수 있다. 현재는 뒤따르는 RFC3339 검증(`:351-352`)이 우연히 막아주지만, 방어가 의도가 아니라 부수효과에 기대고 있다 | `.collect::<Result<Vec<_>, _>>().ok()?`로 바꿔 "한 줄이라도 읽기 실패면 이 파일은 손상"이라는 의도를 코드로 명시한다 |
| n3 | ⚪ Nit | claimed≠verified | `src/views/autonomousTasks.ts:521`·`:522`·`:1106`·`:1123` | 호출 그래프 손 추적 | 런타임 갱신 이벤트 1건당 전체 앱 재렌더가 **3회** 발생한다(`refreshHistoryIfTracked`→`loadTaskHistory`의 `notifyChange` / `applyRuntimeUpdate` 말미의 `notifyChange` / fetch `finally`의 `notifyChange`). "지금 실행" 한 번에 시작·종료 이벤트가 각각 오므로 6회 | `applyRuntimeUpdate`에서 `refreshHistoryIfTracked`를 `notifyChange()` **뒤로** 옮기고 M1 수정(`queueMicrotask`)을 적용하면 자연히 2회로 준다 |
| n4 | ⚪ Nit | 설계정본 대조(시각 일관성) | `src/styles.css:219` | `grep -n "^\.badge" src/styles.css`로 `#f3f4f6` 리터럴 출현 3회(`:166` `:167` `:219`) 확인 | `.badge-run-aborted`가 `#f3f4f6`을 세 번째로 하드코딩한다. 설계 §4-2가 "`.badge-archived`와 동일한 중립 회색 재사용, 신규 토큰 0개"를 명시했고 그 조항 자체는 지켜졌지만, 리터럴 복제 대신 클래스 재사용이나 토큰화가 가능했다 | `badgeClass: 'badge-archived'`로 기존 클래스를 그대로 쓰거나, 3회 반복 시점에 `--color-neutral-soft` 토큰을 도입한다(후자는 "신규 토큰 0개" 조항과 충돌하므로 PM 판단 필요) |

---

## 기각된 지적

패널이 냈으나 진행자가 실물 대조 후 기각/강등한 것. PM이 같은 의심을 하지 않도록 근거를 남긴다.

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| claimed≠verified | `sanitize_task_id`가 40자로 자르고 비허용 문자를 `_`로 바꾸므로, 서로 다른 두 task의 이력이 한 목록에 섞일 수 있다 | **기각** | task id는 `src/views/autonomousTasks.ts:970`에서 `crypto.randomUUID()`로 생성된다 — 36자, 전부 `[0-9a-f-]`(허용 문자)라 치환도 절단도 일어나지 않는다. 손으로 고친 `autonomy.json`에서만 성립하는 가상 시나리오라 지적으로 올리지 않는다 |
| 설계정본 대조 | 긴 절대 로그 경로가 펼침 블록에서 레이아웃을 밀어낸다(`flex:1` + `nowrap`인데 `min-width:0`이 없다) | **기각** | `src/views/autonomousTasks.ts:1254-1258`에서 `pathEl`에 `overflow: hidden`이 설정돼 있다. CSS Flexbox 명세상 flex item의 `min-width: auto`는 `overflow`가 `visible`이 아닐 때 0으로 해석되므로 ellipsis가 정상 동작한다. 코드를 열지 않고 `flex:1`만 보고 유추한 지적 |
| claimed≠verified | 폴백 폴링(`:534-546`)이 `historyCache`의 모든 키를 30초마다 재조회해 방문한 task 수에 비례하는 상시 부하를 만든다 | **강등 후 제외** | 폴링은 `onAutonomyRuntimeChanged`의 **구독 자체가 실패했을 때만** 켜진다(`:529-533` catch 경로 = IPC 브리지 없는 플레인 브라우저). 그 환경에서는 `fetchAutonomyTaskHistory`도 즉시 reject되므로 실제 파일시스템 부하가 발생하지 않는다. 재현 경로를 댈 수 없어 Nit 이하로 강등 |
| claimed≠verified | 모듈 스코프 `historyCache`의 stale 데이터가 다른 작업 상세 화면에 새어나간다 (PM 중점질문 3) | **기각(정상 동작 확인)** | 키가 `${projectPath}\0${taskId}`(`:143-145`)이고, 렌더가 표시 중인 task 객체에서 직접 키를 만든다(`:1300`). 다른 작업으로 이동 후 복귀 시에도 키가 일치해 자기 데이터만 읽는다. playwright 실측에서도 교차 오염 없음 |
| 설계정본 대조 | 설계 §4-1이 요구한 요약(summary) 표시가 펼침 블록에 없다 / 설계 §4-6 에러 트리거가 백엔드 실동작과 다르다 | **스코프 밖** | PM이 사전 승인한 의도적 이탈 2건. 단, 승인 범위의 **경계**는 M2에서 따로 다뤘다 |
| 설계정본 대조 | `durationEl.style.textAlign = 'right'`(`:1281`)는 내용폭으로 줄어드는 flex item에는 무효과다 | **기각(영향 없음)** | 우측 고정 버튼 바로 왼쪽에 놓이므로 실제 렌더 결과는 설계 §4-1이 의도한 우측 정렬과 동일하다(`after-detail__tasks-detail__normal.png`에서 확인). 무해한 잉여 스타일 |

---

## 페르소나별 관점

### [정직성 감사자 — claimed ≠ verified] — 판정: 🟡 Amber

평가기준 V4("코드 주석·문서의 단언이 실제 분기와 어긋나지 않는가")에서 **미충족**이다.

- `loadTaskHistory` 상단 주석(`:1082-1087`)은 "이미 로드됐거나 로딩 중이면 재조회하지 않는다"고 단언한다. 이 단언 자체는 **참이다** — 무한 재조회 루프는 실제로 막힌다(`ensureTaskHistoryLoaded`가 캐시 set **후에** `notifyChange`를 부르므로 재진입 렌더에서는 캐시 히트가 나고 재귀가 1단계에서 끝난다). 그러나 이 주석이 방어한다고 믿게 만드는 위험(전체 재렌더 구조에서의 fetch)에는 **루프 말고 재진입도 있었고**, 그쪽은 열려 있다(M1). "루프는 막혔다"를 "안전하다"로 읽으면 안 된다는 것이 이 라운드의 교훈이다.
- 그 재진입이 금지 사항이라는 근거는 다른 곳도 아닌 **이 저장소 자신**에 있다: `src/main.ts:145-147` — "렌더 함수 자체 안에서 하면 로딩 콜백이 재귀적으로 `renderApp()`을 다시 부르는 동안 바깥 `renderApp()`이 아직 실행 중인 상태와 겹쳐서 DOM이 꼬일 수 있다." 정확히 그 일이 일어났다.
- V3(계약 전수 일치)도 미충족이다. `RunHistoryEntry` 필드 5개는 Rust `#[serde(rename)]`(`log.rs:219-231`)와 TS 인터페이스(`autonomyApi.ts`)가 1:1로 맞는다. 어긋난 것은 필드가 아니라 **의미론적 계약**이다 — "반환 건수가 limit보다 적을 수 있다"(백엔드가 스스로 문서화)와 "반환 건수가 limit보다 적으면 끝이다"(프론트 판정)가 정면 충돌한다(M3).
- V5(실패·미확인 시 진단 정보가 사용자에게 도달하는가)는 M2 때문에 후퇴했다. 앱 안에서 실패 출력을 읽을 수단이 사라졌다.
- 반대로 **잘 지켜진 단언**: `log.rs:238-251`의 doc 주석이 "파일을 열지 않고 정렬 후 limit개만 연다"고 약속했고, 코드는 정확히 그렇게 한다(`:309-310` 정렬→truncate가 `:314` 파싱보다 앞선다). PM 중점질문 2("limit을 정렬 전에 자르는가 후에 자르는가")에 대한 답은 **"정렬 후에 자른다 — 올바르다"**이다. 파일 핸들도 `parse_history_header` 스코프에서 즉시 drop된다.

### [설계정본 대조 감사자] — 판정: 🟡 Amber

D1·D2·D4는 충족, **D3(조용한 누락 0건) 미충족**(M2 1건).

설계 정본의 수치 조항을 코드에 한 줄씩 짝지은 결과는 아래 "평가기준 충족 현황" 표에 전수로 남겼다. §4-2 결과 4종 배지, §4-3 오늘/어제/그외 + 연도 접두 + RFC3339 `title`, §4-4 `분 초` 0패딩, §4-6 4상태, §5 여백 px 9개 항목이 **전부 일치**한다. 조항을 대조하는 입장에서 이 정도로 빠짐없이 옮겨진 구현은 드물다.

미충족은 한 자리다. 설계 §1 위계표 10번("마지막 실행 요약 — 3차, 이력 항목을 펼쳤을 때만 보이면 충분")은 이 항목을 **어딘가에 남기라는 조항**이다. 구현은 펼침 블록에서 뺐고(승인된 이탈 (b)), 동시에 기존에 있던 메타 카드 표시도 함께 제거해 결과적으로 항목 자체가 화면에서 사라졌다. 뒤집었다는 기록이 설계서에도 코드 주석에도 없다 — `renderHistoryDetail` 주석(`:1248-1252`)은 "이력 각 건에는 summary가 없다"까지만 설명하고 런타임 summary의 행방은 말하지 않는다. **기록 없이 사라진 조항**이 이 페르소나의 정의상 지적이다.

D5(설계 내부 모순의 임의 해소가 기록됐는가)는 충족한다. `autonomyApi.ts`의 `fetchAutonomyTaskHistory` 주석이 "설계서 §4-6 표와 달리 이쪽이 실제 동작"이라고 이탈을 **명시적으로 기록**했다 — 승인된 이탈 (a)를 다루는 모범적인 방식이다.

---

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

**발산형 페르소나 미투입.** 작업 등급 Standard의 약식 리뷰 규정(페르소나 1~2인, 발산형 생략 가능)에 따라 의도적으로 생략했다. 이 화면의 구조 자체(2컬럼이 옳은가, 이력이 이 화면에 있는 게 맞는가)는 설계 §2에서 대안 A/B를 비교해 이미 한 차례 발산적으로 검토됐고, 그 판단을 재개봉할 새 근거가 이번 diff에 없다고 보았다. 구조 재검토가 필요하다고 판단되면 `persona-zero-base-redesigner.md`(INDEX 등재, 발산형)를 투입해 별도 라운드로 진행할 것을 권고한다.

---

## 트레이드오프 (페르소나 간 충돌)

- **M3의 처방 (a)"손상 파일 우회 backfill" vs 백엔드의 성능 보장** — 설계정본 대조 관점은 "사용자가 이력에 도달하지 못하는 것은 조항 위반"이라 보고, 백엔드 주석(`log.rs:248-251`)은 backfill을 하면 "상위 limit개만 연다"는 성능 보장이 깨진다는 이유로 의도적으로 하지 않았다고 기록한다. → **권고: 상한을 건 backfill(`limit*2`개까지만 열어본다)**. 성능 보장을 "상위 2N개만 연다"로 약화하되 정확성을 얻는 쪽이 낫다. 손상 파일은 드물어 평상시에는 N개만 열고 끝난다.
- **M2의 처방 vs 승인된 이탈 (b)의 취지** — "없는 필드를 지어내지 않는다"는 이탈 (b)의 원칙은 옳고 유지해야 한다. 런타임 summary를 되살리는 것은 그 원칙과 충돌하지 않는다(실재하는 필드다). 다만 "가장 최근 1건에만 요약이 붙고 나머지에는 없는" 비대칭이 화면에 드러나므로, 라벨을 "가장 최근 실행 요약"으로 명확히 해 그 비대칭이 버그가 아니라 데이터 성격임을 말해줘야 한다. → **권고: 되살리되 라벨로 범위를 명시.**

---

## 잘 된 점 (다음 산출물의 기준)

1. **`rsplitn` 파싱 선택과 그 근거 주석(`log.rs:333-341`)** — 왜 앞에서 split하면 안 되는지, 왜 오른쪽 3필드는 안전한지를 데이터 성질(RFC3339·정수는 `/`를 포함할 수 없다)로 논증했다. 게다가 `history_entry` 테스트 픽스처(`:475-476`)가 task 이름에 일부러 `" / "`를 심어 회귀를 고정한다. **주장과 증거가 같은 커밋 안에 있다.**
2. **정렬 후 truncate라는 순서(`:309-310`)** — "30일치가 쌓여도 상위 N개만 연다"는 성능 목표를 알고리즘 순서로 달성했고, 그 트레이드오프(손상 파일이 섞이면 결과가 적어짐)까지 주석에 **불리한 쪽으로 정직하게** 적어두었다. M3를 발견할 수 있었던 것도 이 정직한 주석 덕분이다.
3. **승인 이탈의 기록 방식** — `autonomyApi.ts`가 "설계서 §4-6 표와 달리 이쪽이 실제 동작"이라고 코드 옆에 남겼다. 설계서와 코드가 갈릴 때 어느 쪽이 정본인지 다음 사람이 즉시 안다.
4. **실패 격리 설계** — 이력을 3번째 IPC로 갈라 조회 실패가 좌측 메타·지침 렌더를 막지 않는다. 설계 §4-6의 의도가 구조로 강제됐다(PM 별도 재현으로 확인됨).
5. **설계 조항의 수치 이행률** — §4-2~§5의 색상 토큰·시각 분기·소요시간 공식·여백 px가 전수 일치한다. 특히 §4-4의 `60분 00초` 0패딩처럼 놓치기 쉬운 디테일까지 맞췄다.
6. **파괴적 액션 분리** — 삭제를 헤더 프라이머리 동선에서 떼어 좌측 카드 하단 구분선 아래 단독 배치한 것(§3)이 그대로 구현됐다. before 캡처에서 `실행 중…|중지|수정|삭제`가 나란히 붙어 있던 것과 비교하면 오조작 위험이 실질적으로 줄었다.

---

## 평가기준 충족 현황

### 설계 정본 조항 대조 (persona-design-contract-auditor)

| 조항 | 요구 | 구현 위치 | 충족 |
|---|---|---|---|
| §4-2 | success ✓/failed ✕/timeout ⏱/aborted ⊘ 4종, 아이콘+라벨 | `autonomousTasks.ts:1179-1184` | ✅ |
| §4-2 | 색상: success-soft/danger-soft/warn-bg(+border)/중립회색, **신규 토큰 0개** | `styles.css:216-219` | ✅ (n4는 리터럴 중복에 대한 Nit일 뿐 조항은 충족) |
| §4-3 | 오늘/어제/그 외, 로컬 12시간제 | `:1192-1211` | ✅ |
| §4-3 | 올해가 아니면 연도 접두 | `:1207` | ✅ |
| §4-3 | "24시간 이내"가 아닌 **달력 날짜** 비교 | `:1186-1188` (`isSameCalendarDay`) + `:1197-1198`(`setDate(-1)`) | ✅ |
| §4-3 | `title`에 RFC3339 원본 | `:1279` (`timeEl.title = timeTitle`) | ✅ |
| §4-4 | `<60초` → `N초` | `:1215` | ✅ |
| §4-4 | `>=60초` → `N분 NN초`(초 0패딩) | `:1216-1218` | ✅ |
| §4-5 | 경로 ellipsis + `title` 전체 경로 + 복사 버튼 | `:1253-1259` | ✅ |
| §4-6 | 로딩 = `.state-block` "불러오는 중…" | `:1305-1306` (`loadingBlock()`) | ✅ |
| §4-6 | 빈 상태 = 안내 문구, 별도 CTA 버튼 중복 배치 안 함 | `:1311-1317` | ✅ |
| §4-6 | 에러 = `.alert` + "다시 시도", 다른 블록에 전파 안 됨 | `:1307-1310` (`errorBlock`) | ⚠️ 전파 격리는 충족, **기존 목록 파괴는 m5** |
| §4-6 | 더 보기 = 20씩 증가, 전체 재조회 교체 렌더 | `:1138-1144` | ✅ |
| §4-6 | 반환 건수 < 요청 limit이면 버튼 숨김 | `:1113` | ⚠️ 규칙대로 구현했으나 **백엔드 계약과 어긋남(M3)** |
| §5 | `.detail-header` margin-bottom 20px | `styles.css:189` | ✅ |
| §5 | 2컬럼 gap 24px / 헤더 아래 margin-top 20px | `styles.css:202` | ✅ |
| §5 | 840px 붕괴(기존 breakpoint 재사용) | `styles.css:203-205` | ✅ |
| §5 | 기록 카드 패딩 16px | `styles.css:211` | ✅ |
| §5 | 행 `padding: 10px 0` + hairline, 마지막 행 border 제거 | `styles.css:213-214` + `:1330-1331`(펼침 시 JS 보정) | ✅ |
| §5 | 더 보기 위 margin-top 12px, 가운데 정렬 | `:1339-1340` | ✅ |
| §5 | 펼침 블록 margin-top 8px, padding-left 4px | `:1266-1267` | ✅ |
| §5 | 좌측 하단 액션: 구분선 + `margin/padding-top 16px` + `gap 8px` | `styles.css:208` | ✅ |
| §3 | 헤더 우측 프라이머리 1개 + 세컨더리 왼쪽 | `:1378-1380` | ✅ |
| §3 | 수정/삭제를 좌측 카드 하단으로 이동, 삭제는 구분선 아래 | `:1236-1239` | ✅ |
| §1 위계표 10 | 마지막 실행 요약을 3차로 **남긴다** | — | ❌ **M2 (화면에서 소실)** |

### 정직성 기준 (persona-claimed-vs-verified)

| # | 기준 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| V3 | 백엔드/프론트 계약 전수 일치 | 필수 | ❌ | 필드는 1:1이나 `limit` 의미론이 충돌(M3) |
| V4 | 주석·문서의 단언이 실제 분기와 일치 | 필수 | ❌ | 캐시 주석은 참이나 main.ts:145-147 규칙 위반(M1) |
| V5 | 실패 시 진단 정보가 사용자에게 도달 | 권장 | ❌ | summary 소실(M2) |
| V6 | 숫자·목록 의미 정확성 | 권장 | ✅ | 정렬 후 truncate, 최신순, 0패딩 전부 정확 |

---

## PM에게 권고

### 지금 고쳐야 할 것 (병합 전)

1. **M1 — 재진입 렌더**. 1줄 수정(`:1106`을 `queueMicrotask(notifyChange)`)으로 해소된다. 상세 화면 진입마다 사용자에게 보이는 결함이고, 이 화면이 "허접하다"는 평가를 고치려는 작업이라는 점에서 그대로 내보내면 작업 목적 자체를 훼손한다. 수정 후 첨부한 재현 스크립트(`docs/reviewer/evidence-autonomy-detail-reentrant-render-2026-09-17.mjs`)를 그대로 다시 돌려 최대 자식 수가 2로 떨어지는지 확인하면 검증이 끝난다.
2. **M3 — 손상 로그 1건이 "더 보기"를 막는 문제**. 백엔드 backfill(상한 `limit*2`)이 권고안. 백엔드 1곳 수정으로 끝나고 프론트는 손대지 않아도 된다.

### PM 판단이 먼저 필요한 것

3. **M2 — 마지막 실행 요약 소실**. 이것은 "기술적으로 깨진 문제"가 아니라 **"기획 의도와 어긋났는지"**를 먼저 확인할 문제다. 의도적 제거였다면 조치는 코드 수정이 아니라 설계 §1 위계표 10번을 "제거"로 갱신하는 것이다. 의도치 않은 누락이었다면 기록 카드 상단에 "가장 최근 실행 요약"으로 되살린다. **둘 중 어느 쪽인지 결정한 뒤에 담당 에이전트에 넘길 것을 권고한다.**

### 후속으로 미뤄도 되는 것 (백로그)

4. **m4**(`pub fn`→`pub async fn`) — 1단어 수정이라 M1·M3와 함께 처리하면 비용이 사실상 0이다. 함께 묶는 것을 권고하나, 미루더라도 현재 사용 규모(사내 50명 미만, 로컬 로그)에서 체감 위험은 낮다.
5. **m5**(에러 시 기존 목록 유지) — 조건 1개 수정. 다음 UI 라운드에 묶어도 된다.
6. **n1~n4** — 참고. n1(개행 이스케이프)만은 로그 포맷을 다음에 손댈 때 함께 정리해 두면 좋다. 지금 단독으로 포맷을 바꾸면 기존 로그와의 호환을 따져야 해 비용 대비 이득이 없다.

### 리뷰 범위에서 **생략한 것** (정직 보고)

- **`renderTaskForm`(등록·수정 폼)** — PM 지시로 제외(다른 브랜치 동시 작업 중). 단 n1 판정에 필요한 범위에서 `nameInput`이 `<input>`인지만 `:727`에서 확인했다.
- **PM이 이미 검증한 항목** — `cargo test`·`tsc`·`vite build`·캡처 79장 생성 여부는 재실행하지 않았다. 다만 M1 검증을 위해 vite dev 서버(포트 5199)와 playwright는 **직접 기동했다**(소스·git 무변경). 기존 캡처 산출물 2장(`before-detail`/`after-detail`의 `tasks-detail__normal`)은 Read로 직접 열어 눈으로 대조했다.
- **모바일/좁은 폭(840px 이하) 실제 캡처** — 하지 않았다. CSS 규칙(`styles.css:203-205`)이 기존 실측 breakpoint를 그대로 재사용한다는 점만 코드로 확인했고, 붕괴 후 실제 렌더는 **미확인**이다. 창 축소 검증이 필요하면 `persona-window-resize-and-post-interaction.md`(INDEX 등재)를 투입한 별도 라운드를 권고한다.
- **30일치 로그가 쌓인 상태의 실제 스캔 소요시간** — 측정하지 않았다(m4는 알고리즘 구조와 저장소 관례 대조에 근거한 지적이며, 실측 수치는 없다).
- **발산형 페르소나** — 위 "구조적 제언" 절에 사유와 함께 미투입 명시.

### 첨부 증거

- `docs/reviewer/evidence-autonomy-detail-reentrant-render-2026-09-17.png` — M1의 중복 렌더 프레임(사이드바 2개·본문 2개가 나란히 그려진 상태). 이력 IPC 응답을 2.5초로 늘려 포착했다.
- `docs/reviewer/evidence-autonomy-detail-reentrant-render-2026-09-17.mjs` — M1 재현 스크립트. 프로젝트 파일을 수정하지 않고 `scripts/capture/stub.mjs`를 그대로 재사용한다. 실행: `node <스크립트> `(vite dev 서버가 `http://localhost:5199`에 떠 있어야 한다).
  기본 픽스처(IPC 지연 80ms) 실측 결과: `t=831ms children=4 sidebar=2 main=2` → `t=929ms children=2 sidebar=1 main=1`.
