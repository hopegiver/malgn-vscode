# R19 수집키 발급·회전 201 `env` 서버 권장값 적용 — 리뷰 보고서

리뷰 대상: 워크트리 `/Users/hopegiver/workspace/malgn-vscode/.claude/worktrees/r19-server-env` · 커밋 범위 `88fa610..b59dd7e` (b59dd7e, 브랜치 `feat/r19-server-env`) · target_id `r19-otel-server-env`
리뷰 페르소나 패널(6): `persona-remote-config-injection-boundary.md`(신규, 보안), `persona-credential-at-rest-and-transit.md`(보안), `persona-design-contract-auditor.md`, `persona-claimed-vs-verified.md`, `persona-screen-state-honesty.md`, `persona-zero-base-redesigner.md`(발산형)
정본: malgnai-public `docs/design/device-ingest-key-provisioning.md` 16~243행(201 `env` 예시 85~92행, 2번 규칙 172행 "env 적용은 앱 설계가 정한다(§14 Q3)"), 위임 메시지의 요구사항 1~5
리스크 범주: 크리덴셜 경로 · 사용자 설정 파일(`~/.claude/settings.json`) 자동 변경 · 서버 응답 값이 사용자 파일로 들어가는 새 신뢰 경계
등급/모드: Sensitive, 최초 리뷰(풀패널). 노출 범위 축소 미적용(위임 메시지에 축소 근거 3값 없음, "축소 없음" 명시).
리뷰 일자: 2026-10-15
종합 판정: 🟡 **Amber** (Critical 0 · Major 1 · Minor 4 · Nit 3 · Rethink 1)

## 요약 (2분 규칙)
보안 경계는 단단하다. 서버가 쓸 수 있는 키는 닫힌 허용 목록 6개뿐이고, 값도 열거형·범위로 닫혀 있다. 서버(또는 변조된 응답)가 이 경로로 할 수 있는 일은 다음 셋뿐이다. 내보내기 주기를 5초~1시간 안에서 바꾸기, temporality를 세 값 중 하나로 바꾸기, 이미 `otlp`인 값을 다시 `otlp`로 쓰기. 목적지 변경, 헤더 주입, 프라이버시 키 켜기, 수집 끄기, 5초 미만 주기는 모두 막혀 있다. 거부된 키의 값은 상태·Debug·UI 어디에도 남지 않는다. 키와 env는 원자적 쓰기 한 번으로 함께 써진다. 테스트도 반환값이 아니라 파일 내용을 단언한다.

막는 것은 화면 쪽 1건이다. **RV-01**: 자동 연동이 백그라운드에서, 또는 사용자의 [다시 시도]·재발급 확인으로 서버 값을 쓴다. 그런데 설정 > OTel 폼은 다시 읽지 않는다. 그래서 같은 화면이 위에서는 "허브가 보낸 값으로 바뀐 설정: OTEL_LOGS_EXPORT_INTERVAL"이라고 말하면서, 입력칸에는 5000과 "이전 앱 버전의 기본값" 안내를 계속 띄운다. 이 상태에서 아무 필드나 고쳐 저장하면 폼이 옛 값 6개를 그대로 다시 써서 서버 값이 조용히 되돌려진다. 그 뒤로는 키가 이미 적용된 상태라 회전이 일어나지 않으므로, 되돌려진 값이 그대로 남는다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| remote-config-injection-boundary (보안) | **신규** | INDEX에 "서버 응답 값이 사용자 설정 파일로 들어가 다른 프로그램(Claude Code)의 동작을 바꾸는 신뢰 경계"를 묻는 행이 없다. 가까운 행은 셋이다. `credential-at-rest-and-transit`은 자격증명이 남는 자리와 목적지, `delegated-authority-blast-radius`는 자식 프로세스에 넘긴 권한, `external-standard-expectation-gap`은 표준 문법 단정을 본다. 서버가 무엇을 쓸 수 있는지(허용 목록·값 규칙·UI 주입)를 묻는 관점은 없다 |
| credential-at-rest-and-transit (보안) | 재사용 | 같은 쓰기에 실리는 수집키의 원자적 쓰기·0600·백업 키 제외·pending 보관이 이 역할개념 그대로다 |
| design-contract-auditor | 재사용 | 계약 201 `env`·Q3와 요구사항 1~5를 조항 단위로 대조한다 |
| claimed-vs-verified | 재사용 | 테스트가 빈 깡통인지, 주석의 주장이 코드와 맞는지 본다 |
| screen-state-honesty | 재사용 | 안내문·폼이 실제 파일 상태와 동작을 정직하게 말하는지 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 자체를 의심하는 역할개념 그대로다 |

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `cd src-tauri && cargo test` (워크트리) | 969 passed / 0 failed / 23 ignored |
| `pnpm exec tsc --noEmit` | 통과 |
| `pnpm run test:ts` | 52 pass / 0 fail |
| 화면 캡처: 워크트리 Vite(포트 1531) + 하네스 `bridge.mjs`·`fixtures.mjs`를 재사용한 스크래치 러너(저장소 밖) | `docs/reviewer/r19-otel-server-env-01~03-*.png` 3장. 3장 모두 Read로 열람했다. 러너 출력으로 DOM 덤프도 확인했다(아래 각 지적에 인용) |
| `pnpm build` | 실행하지 않았다 |
| 라이브 허브 호출, 실제 `~/.claude/settings.json` 접근, 허브 저장소 수정 | **하지 않았다.** 하네스 픽스처는 `~/.claude/plugins`·`agents`·`skills`·`projects`만 읽고 settings.json은 읽지 않는다(`scripts/ui-harness/lib/real-data.mjs` grep으로 확인) |
| 실제 Tauri 런타임(이벤트 `otel-link-changed`, 스케줄러 스레드) | **미검증.** 앱을 기동하지 않았다. RV-01은 코드 경로와 하네스 상태 재현에 근거한다 |
| 변이 테스트 | 하지 않았다(코드 수정 금지). 테스트 실효성은 단언 대상(파일 내용)을 읽어서 판정했다 |

## 요구사항 대조표
| # | 요구 | 코드 | 테스트(파일 결과 단언 여부) | 판정 |
|---|---|---|---|---|
| 1 | 6키만 값 검증 후 그대로 쓰고, 충돌하면 서버 값이 우선 | `otel_server_env.rs:26-33, 72-91`, `otel_settings.rs:546-558`(`values: server_env.accepted`) | `otel_link_engine/tests.rs:1772`(발급, 6키 파일 값), `:1813`(회전, 사용자 30000·120000 → 60000) | 충족 |
| 1 | 키 교체와 같은 원자적 쓰기, 0600·백업(키 제외)·마스킹 | `save_reporting_changes` 한 번(`otel_settings.rs:403-526`), `apply_guarded`(`otel_link_engine.rs:380-392`) | `:1772` 백업=원본·0600 단언. 실패 시 키와 env 모두 미기록, 재시도 때 함께 기록(`:1996`). 키 실패면 env도 미기록(`otel_settings.rs` 테스트 `a_failing_key_leaves_env_unwritten_too`) | 충족(테스트 공백은 RV-05) |
| 2 | 목록 밖 키·형식 위반은 절대 쓰지 않음 | `sanitize_server_env`(`otel_server_env.rs:107-137`). 2차 방어로 `save_reporting_changes`가 HEADERS·RESOURCE_ATTRIBUTES·비관리 키·readOnly 키를 거부한다(`otel_settings.rs:420-437`) | `:1858`(엔드포인트 3·HEADERS·RESOURCE_ATTRIBUTES·프라이버시 4·ENABLE_TELEMETRY·ANTHROPIC_BASE_URL → 파일에 `evil.example`·`LEAKLEAK` 없음), `:1922`(형식 위반 6종), 모듈 테스트 17종 | 충족 |
| 2 | env 거부가 수집키 적용을 실패시키지 않음 | 모듈이 `Result`를 돌려주지 않는다. 통과 값 ⊂ `validate_value` | `:1858`·`:1922`가 `Phase::Idle`과 키 기록을 단언. `every_value_the_server_filter_accepts_also_passes_the_settings_validation`이 부분집합 관계를 고정 | 충족 |
| 2 | 거부 키는 이름만 관측, 값은 로그·Debug·에러·UI에 없음 | `ServerEnv.rejected: Vec<String>`(값 필드 없음), `name_for_report`(`:93-104`) | `rejected_report_and_debug_never_contain_values`, 엔진 `:1858`이 `view`와 `mem` Debug 덤프 단언, HTTP 왕복 `otel_hub_client.rs:609-628` | 충족(Nit N-1·N-2) |
| 3 | 앱 기본값 60000, 기존 5000은 조용히 바꾸지 않음 + 정확히 5000일 때 안내 | `otel_settings.rs:76-79`, `otelLinkText.ts:85-89`, `settings.ts:473-475` | `existing_5000_in_the_file_is_reported_as_is_and_never_rewritten_by_reading`, 엔진 `:2036`, TS `otelLinkText.test.mjs` 2건 | 충족(전파 범위는 RV-03·RT-1) |
| 4 | UI 안내가 실제 동작과 일치하고 바뀌는 키를 명시 | `onboarding.ts:183`(`AUTO_LINK_CHANGES_NOTE`), `:192-199`(`serverEnvTail`) | TS 4건 | **부분 이탈(RV-01, RV-02)** |
| 5 | 허용/거부/충돌/마이그레이션/mhd_ 부재 테스트 | 위 표 | mhd_ 부재 `:2019`(호출 0회, 파일·백업 불변) | 충족(공백은 RV-05) |

## 서버(또는 변조 응답)가 이 경로로 할 수 있는 일 — 최악 통과 값 표
`persona-remote-config-injection-boundary` 방법론 2. 확인 방법은 `value_ok`(`otel_server_env.rs:72-91`)와 준비 판정(`otel_link.rs:220`)을 읽고, 엔진 테스트의 파일 단언과 대조하는 것이었다.

| 위협 | 가능? | 근거 |
|---|---|---|
| 수집 데이터를 다른 곳으로 보내기 | 불가 | 엔드포인트 3종·HEADERS는 허용 목록 밖. 2차로 저장 엔진도 HEADERS를 거부(`otel_settings.rs:423-428`). 테스트 `:1858` |
| 프라이버시 키 켜기(`OTEL_LOG_*`) | 불가 | 목록 밖 + 저장 엔진 readOnly 거부(`:432-436`) |
| 수집 끄기(exporter `none`, ENABLE_TELEMETRY 0) | 불가 | exporter는 `otlp`만 허용(`:85`), ENABLE_TELEMETRY는 목록 밖 |
| 호출 빈도 올리기 | 5초까지만 | 하한 5000(`:37`)은 예전 앱 기본값이다. 1ms 같은 값은 거부(`interval_bounds_are_inclusive`) |
| 수집 사실상 끄기(주기 1시간) | 가능 | 상한 3600000. 데이터 소유자인 허브 자신의 선택이고, 설정 화면 상한(`otel_settings.rs:318`)과 같다. 위협으로 보지 않는다 |
| 사용자가 끈 logs exporter를 다시 켜기 | 불가(간접) | `none`이 있는 파일은 준비 판정에서 막혀(`otel_link.rs:220`) 201 자체가 오지 않는다. 남는 것은 N-3 |
| 줄바꿈·제어문자로 다른 env 끼워 넣기 | 불가 | `is_plain_token`(`:63-65`), 테스트 `"cumulative\nOTEL_X=1"`(`tests.rs:1922`) |
| UI에 HTML 주입 | 불가 | 백엔드 이름 형식 필터 + 프론트 `ENV_NAME_SHAPE`(`onboarding.ts:186`) + `el()`이 텍스트 노드로만 넣음(`src/dom.ts:21-23`). 캡처 01 러너 DOM 덤프에 `<img src=x`·`onerror` 0건 |

하한 5000·상한 3600000, exporter·프로토콜 단일값 제한은 타당하다고 판정한다. 프로토콜은 준비 판정이 `http/protobuf`를 요구하고(`otel_settings.rs:655-657`), exporter도 준비 판정이 `otlp` 포함을 요구한다. 그래서 서버가 이 둘을 다른 값으로 바꿀 정당한 사유가 없다.

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 확인방법 | 개선안 | 관점 |
|---|---|---|---|---|---|---|
| RV-01 | 🟠 Major | `src/main.ts:319-320`, `src/main.ts:284`, `src/views/settings.ts:184-200, 203-212, 220-240, 405-418, 494-501` | 자동 연동이 서버 env를 파일에 쓴 뒤에도 설정 > OTel 폼(`state.otel.settings`)을 다시 읽지 않는다. `otel-link-changed` 이벤트는 `loadIngestLink`만 부른다(main.ts:319-320). 탭에 다시 들어와도 `!state.otel.loaded` 조건 때문에 재조회가 없다(main.ts:284). [다시 시도]·재발급 확인·저장 후 recheck 응답도 마찬가지다. 결과 1: 같은 화면 위쪽은 "허브가 보낸 값으로 바뀐 설정: OTEL_LOGS_EXPORT_INTERVAL"이라고 말하는데, 입력칸은 5000을 보여 주고 "이전 앱 버전의 기본값" 안내까지 붙는다(요구 4 이탈). 결과 2: 폼 제출은 readOnly가 아닌 입력칸 전부를 보낸다(settings.ts:494-501). 그래서 사용자가 다른 값 하나만 고쳐 저장해도 6키의 옛 값이 다시 써져 서버 값이 조용히 되돌려진다. 이후 점검은 키 앞 8자가 같아 회전하지 않으므로(`otel_link_engine.rs` has_current, 계약 D-4) 되돌려진 값이 남는다(요구 1 "서버 값 우선"이 실사용 경로에서 깨짐). 재현: 파일 5000, 허브 연결됨 → 설정 > OTel 탭을 연 채로 웹 폐기 → 재발급 확인창 [다시 발급] → 201 적용(파일 60000) → 폼은 5000 그대로 → 아무 필드나 고쳐 저장 → 파일 5000 | 코드 경로 추적(위 라인). 캡처 `docs/reviewer/r19-otel-server-env-03-settings-applied-changed-but-form-5000.png`는 이 도달 상태를 픽스처로 재현했다. 러너 출력: "허브가 보낸 값으로 바뀐 설정: OTEL_LOGS_EXPORT_INTERVAL…", `legacy> …이전 앱 버전의 기본값`, `intervalInputs=["60000","5000"]`. 실제 이벤트 순서는 런타임 미기동이라 관측하지 못했다 | (a) 상태 응답·이벤트에서 `serverEnvChanged`가 비어 있지 않게 바뀌면, `state.otel.loaded`일 때 `loadOtelEnv()`를 다시 부른다. 그런데 기존 드래프트 보존 규칙(settings.ts:366-370) 때문에 사용자가 고치던 값은 남는다. 그래서 (b) 저장 시에는 드래프트와 `forSettings` 원본을 비교해 **바뀐 필드만** 보내는 쪽이 근본 해법이다. 회귀 테스트: 하네스에서 `otel_settings_get` 응답을 바꾼 뒤 `otel-link-changed`를 emit하고 입력칸 값과 저장 payload를 단언한다 | screen-state-honesty, remote-config-injection-boundary(T6) |
| RV-02 | 🟡 Minor | `src/views/settings.ts:113-116, 487`, `src/onboarding.ts:245, 259` | 같은 6키 문장(`AUTO_LINK_CHANGES_NOTE`, 약 400자)이 설정 > OTel 한 화면에 미연동이면 3번, 적용됨이면 2번 반복된다. 반복되는 자리는 셋이다. 연동 상세(`otel-link-detail`), 패널 공통 안내(`otel-autolink-changes-note`), 수집키 블록 안내(`INGEST_KEY_COPY.issue/issueManaged`). 사용자가 핵심("이 6개는 허브 값이 우선")을 오히려 놓치게 만든다 | 캡처 02(러너 `noteCount=3`)·03(`noteCount=2`)을 열어 확인 | 설정 화면에서는 `otel-autolink-changes-note` 한 곳만 남긴다. `INGEST_KEY_COPY`와 연동 상세에는 "(아래/위 안내 참고)" 정도만 둔다. 홈 체크리스트는 한 번이면 된다 | screen-state-honesty |
| RV-03 | 🟡 Minor | `src-tauri/src/otel_link.rs:32-34`, `src/otelLinkText.ts:81-83` | 결정 기록은 "서버가 값을 바꿔도 앱 수정 없이 따라가야 안전하다"를 적용 근거로 든다. 실제 전파 시점은 발급·회전 201뿐이다. 이미 키가 적용된 기기(R17 이후 연동한 기존 사용자)는 키 앞 8자가 같아 회전하지 않으므로, 허브가 값을 바꿔도 받지 못한다. R19가 줄이려던 5000 사용자도 그대로 남는다. 안내는 설정 화면의 5000 입력칸 아래에만 있다. 주장(따라간다)과 동작(새 키를 받을 때만 따라간다)의 범위가 다르다 | `has_current`의 "이미 적용됨 → 끝낸다" 분기 읽기, 엔진 테스트 `tests.rs:2036`(201 없는 점검은 env 불변)이 이를 고정함을 확인 | 결정 기록과 `AUTO_LINK_CHANGES_NOTE`에 "새 키를 받을 때만 반영"을 명시한다(UI 문구는 이미 "수집키를 새로 발급·교체할 때"라고 써서 정확하다. 머리말 주장만 좁히면 된다). 구조 대안은 RT-1 | claimed-vs-verified |
| RV-04 | 🟡 Minor | `docs/design/autonomy-runtime-and-config.md:233` | 앱 설계 문서가 여전히 `OTEL_LOGS_EXPORT_INTERVAL` 기본값을 `5000`으로 적는다. 허브 계약 §14 Q3(정본 1265행)이 바로 이 줄을 가리켜 "앱이 따를지는 앱 설계 쪽 결정"이라고 남겼는데, 결정(60000 + 201 env 적용)이 설계 문서에 반영되지 않았다 | `git grep OTEL_LOGS_EXPORT_INTERVAL` | 233행을 `60000 / 60000`(옛 5000은 자동 이행 없음, 권장 안내)으로 고치고, 201 env 허용 목록 6키·값 규칙을 한 절로 남긴다. 계약 Q3의 결론으로 링크한다 | design-contract-auditor |
| RV-05 | 🟡 Minor | `src-tauri/src/otel_link_engine/tests.rs:1663-1715, 1813` | (a) "요청 중 연결 해제" 테스트(RV-05 계열 4건)는 모두 env 없는 `created()`를 쓴다. 해제 뒤 도착한 201의 **env**가 파일에 쓰이지 않는다는 단언이 없다. (b) env를 싣는 회전 테스트(`:1813`)는 옛 키 `O`가 있는 파일에서 시작하지만, 백업에 옛 키가 없다는 단언이 없다(env가 실린 쓰기에서 "백업 키 제외" 불변식이 고정되지 않음). 구조상으로는 안전하다. `apply_guarded`가 세대 확인 뒤에만 쓰고(`otel_link_engine.rs:380-384`), 백업 함수는 `values`와 무관하다 | 테스트 목록 grep, 해당 테스트 본문 열람 | (a) `rv05_*` 하나를 `created_with_env('K', contract_env())`로 복제해 해제 뒤 `OTEL_LOGS_EXPORT_INTERVAL`이 5000 그대로인지 단언한다. (b) `:1813`에 `backup_path_for` 내용에 `fake_key('O')`가 없음을 추가한다 | credential-at-rest-and-transit, claimed-vs-verified |
| N-1 | ⚪ Nit | `src-tauri/src/otel_server_env.rs:93-104`, `src/onboarding.ts:192-199` | 거부 키 "이름"은 서버가 정하는 문자열이다. 최대 16개×64자이고 `[A-Za-z0-9_.-]`만 허용된다. HTML은 불가능하지만 `CALL-support.example-NOW` 같은 문구 위조는 UI에 그대로 실린다 | 형식 필터 읽기 | 거부 이름이 `OTEL_`/`CLAUDE_`/`ANTHROPIC_` 접두사가 아니면 "(그 밖 N개)"로 접는다. 또는 화면에는 개수만 쓰고 이름은 툴팁·진단으로 돌린다 | remote-config-injection-boundary |
| N-2 | ⚪ Nit | `src/onboarding.ts:186`, `src-tauri/src/otel_server_env.rs:130-134` | 백엔드는 "(그 밖 N개)"를 N 제한 없이 만들지만 프론트 정규식은 `\d{1,3}`만 받는다. 그래서 1,000개 이상은 꼬리 표시가 통째로 사라진다. 프론트 필터에 걸린 이름도 개수 없이 버려져 "거부됨"이 실제보다 적게 보인다 | 러너에 `(그 밖 1200개)` 주입 → 캡처 01 출력에 없음(`over999=false`) | 정규식을 `\d{1,6}`로 넓히거나, 프론트가 버린 수를 "(표시 안 한 N개)"로 더한다 | screen-state-honesty |
| N-3 | ⚪ Nit | `src-tauri/src/otel_server_env.rs:85`, `otel_settings.rs:616-619` | 준비 판정은 `otlp,console` 같은 쉼표 목록을 받아 준다. 그런데 서버의 `otlp`가 이 값을 덮으면 사용자의 `console` exporter가 빠진다. "바뀐 설정"에 이름은 뜨지만, 값이 무엇에서 무엇으로 바뀌었는지는 알 수 없다 | 코드 대조 | 파일 값이 이미 `otlp`를 포함하면 exporter 키는 쓰지 않는다(준비 상태가 같으므로 서버 의도를 해치지 않는다) | remote-config-injection-boundary |
| RT-1 | 🔵 Rethink | `otel_link_engine.rs` `accept_key`(:794)·`has_current`, `otel_settings.rs:546-558` | 서버 권장값 전파를 **키 발급 사건**에 묶었다. 그래서 (1) 기존 키 보유 기기에는 전파되지 않고(RV-03), (2) "앱이 쓴 5000"과 "사용자가 정한 5000"을 가를 수 없어 이행을 포기했고, (3) 사용자가 직접 정한 값도 회전 때마다 예고 없이 덮인다. 세 문제는 근원이 같다. 파일이 "누가 이 값을 정했는가"를 모른다는 것이다 | 위 지적 3건의 공통 원인 분석 | **대안 구조 A(서버 변경 없음)**: 앱이 쓴 값의 출처를 사이드카(`~/.claude/malgn-otel-owned.json`: 키 → 앱이 마지막으로 쓴 값)에 기록한다. 서버 값은 "파일 값 == 앱이 마지막으로 쓴 값"(=사용자가 손대지 않음)일 때만 덮는다. 옛 5000은 첫 실행에서 "사이드카 없음 + 정확히 5000"이면 앱 소유로 간주해 이행한다. **대안 구조 B(허브 변경)**: `GET` 상태 응답에 `env`(또는 env 버전 해시)를 싣고, 앱은 매 점검에서 같은 `sanitize_server_env` + 원자적 쓰기로 적용한다. 회전 예산을 쓰지 않고 기존 기기에도 전파된다. 단기 절충: 5000 안내에 [권장값으로 바꾸기] 버튼을 두고 홈 체크리스트에 1회 노출한다 | zero-base-redesigner |

## 페르소나별 관점

### remote-config-injection-boundary (신규, 보안)
T1~T4 필수 4개를 충족했고, 합격이다. 허용 목록은 상수 배열(`otel_server_env.rs:26-33`)이다. 매칭은 정확일치이고(`contains(&key.as_str())`), 대소문자·끝 공백 변형도 거부된다(테스트 `:198-199`). 값 규칙은 키마다 닫혀 있다. 숫자 JSON 타입, 선행 0, `+`, 20자리 숫자, 제어문자, 비ASCII, 제로폭 문자까지 테스트가 있다(`:225-261`). 2차 방어도 겹친다. 같은 저장 엔진이 HEADERS·RESOURCE_ATTRIBUTES·비관리 키·readOnly 키를 별도로 거부하므로, 허용 목록 상수를 실수로 넓혀도 가장 위험한 키는 막힌다(`otel_settings.rs:420-437`). 다만 엔드포인트 키는 관리 대상이라 2차 방어가 없고, 허용 목록 테스트(`server_env_keys_are_a_managed_subset…`의 `!k.ends_with("_ENDPOINT")`)가 유일한 고정점이다. 이 테스트가 있으니 충분하다. T6은 RV-01로 미충족이다.

### credential-at-rest-and-transit (보안)
env가 수집키와 같은 `OtelSavePayload`로 묶여 읽기-수정-쓰기 1회로 끝난다. `pending_write`가 키와 env를 함께 보관해서, 재시도 때 "키만 쓰이고 env는 빠진" 중간 상태가 없다(`otel_link_engine.rs:198-204, 813-826`, 테스트 `:1996`). 해제는 `Memory::default()`로 env 보고까지 비운다(`:362-366`, 테스트 `:2066`). `PendingWrite`·`Created`의 Debug는 `SecretString`이 가린다. 테스트 `:1858`이 `mem` Debug 덤프까지 단언한다. 백업은 0600이고 원본 1세대다(`:1772`). 남은 것은 RV-05 테스트 공백뿐이다.

### design-contract-auditor
계약 2번(172행)은 "Authorization 항목만 바꾼다"고 하면서 env 적용을 앱 설계에 맡겼다(Q3). 이번 구현은 그 재량 안에 있고, 이전 결정 N-5("적용하지 않는다")를 뒤집은 사실을 머리말 `otel_link.rs:27-39`에 정직하게 남겼다. 계약 201 예시의 6키와 허용 목록이 정확히 같다(`contract_example_passes_in_full`). `env` 필드가 아예 없는 구버전 응답도 키는 적용된다(`:1982`). 앱 설계 문서 미갱신(RV-04)만 남는다.

### claimed-vs-verified
테스트는 빈 깡통이 아니다. R19 엔진 테스트 14건이 전부 `file_json`·`env_str`·`file()` 원문으로 **파일 결과**를 단언한다. 거부 테스트는 파일 원문에 `evil.example`·`LEAKLEAK` 부재까지 본다. "서버 필터 ⊂ 설정 검증"이라는 주석의 주장은 전수 후보 테스트로 고정돼 있다. 주장이 동작보다 넓은 곳은 RV-03 한 곳이다.

### screen-state-honesty
캡처 01(홈): 바뀐 키와 거부 키가 이름만으로 정확히 나오고, 주입 문자열은 걸러졌다. 캡처 02(설정, 미연동, 파일 5000): 5000 안내가 "직접 정한 값이면 그대로 두어도 됩니다. 단, … 허브가 보낸 값이 이 값을 덮어씁니다"로, 덮어쓰기 사실을 숨기지 않는다. 다만 6키 문장이 3번 반복된다(RV-02). 캡처 03: RV-01 상태다.

### zero-base-redesigner (발산형)
RT-1. 지금 구조는 "허브가 정한 값"과 "사용자가 정한 값"을 같은 칸에 두고 마지막 쓰기가 이기게 했다. 그래서 이행 포기, 전파 범위 축소, 사용자 값 덮어쓰기라는 세 비용이 한꺼번에 생겼다. 값의 출처를 기록하는 한 칸(대안 A)이나 전파 채널 분리(대안 B)가 있으면 세 비용이 함께 사라진다.

## 트레이드오프
- **서버 값 우선 vs 사용자 주권**: 요구사항 1은 서버 우선을 정했고, 이번 리뷰는 그 결정을 뒤집지 않는다. 사내 50명 미만 도구이고, 허브가 쿼터 초과 시 키를 폐기하는 운영 제약이 있어서 서버 우선이 합리적이다. 대가는 사용자가 정한 주기가 회전 때 예고 없이 바뀐다는 것이다. 안내문은 이를 사전 고지("이 6개는 허브 값이 기존 값보다 우선합니다")하고, 사후에는 이름으로 알린다. RT-1 대안 A는 두 가치를 함께 살리지만 사이드카 파일이라는 상태가 하나 더 생긴다.
- **부분 적용 vs 전부-아니면-전무**: 키별 독립 적용을 택한 근거(`otel_server_env.rs:10-15`)는 타당하다. 한 키의 거부가 주기 조정까지 막지 않고, 거부된 키는 파일 값이 그대로라 앱이 모르는 값이 생기지 않는다.

## 기각된 지적
| 후보 | 기각 사유 | 확인방법 |
|---|---|---|
| 서버가 사용자가 끈 logs exporter(`none`)를 `otlp`로 되살린다 | `none`이 있는 파일은 `ExporterNotOtlp`로 준비 판정에서 막혀 발급·회전 자체가 일어나지 않는다. 남는 쉼표 목록 사례는 N-3으로 강등했다 | `otel_link.rs:220`, `otel_settings.rs:652-653` |
| 거부 키 이름으로 UI HTML 주입 | 백엔드·프론트 형식 필터를 이중으로 거치고, `el()`이 텍스트 노드만 만든다 | `src/dom.ts:21-23`, 캡처 01 DOM 덤프(`imgTag=false onerror=false`) |
| `accept_key`의 `is_stale` 확인(:806)과 `with()` 기록 사이에 해제+재연결이 끼면, 옛 201의 거부 이름이 새 연결 메모리에 남는다 | 마이크로초 창에서 해제·재연결 두 사용자 조작이 모두 일어나야 하고, 재현 경로를 대지 못한다. 영향도 이름 표시뿐이다. 같은 패턴이 기존 `lost_checks` 초기화(:796-800)에도 있다 | 코드 읽기 |
| 201 응답 본문 크기 상한 없음(`resp.text()`) | 이번 diff 밖의 기존 코드다. 목적지는 빌드 고정 허브 origin + TLS다 | `otel_hub_client.rs:307` |
| 하한 5000이 서버에게 사용자보다 높은 빈도를 허용한다 | 그 요청의 목적지가 허브 자신이라 서버가 스스로 부하를 고르는 것이다. 사용자 PC 비용은 무시할 수준이다 | `otel_server_env.rs:35-38` |

## 잘 된 점
- 신뢰 경계 모듈(`otel_server_env.rs`) 머리말이 위협·거부 동작·비노출 규칙을 한 화면에 설명하고, 모든 규칙에 테스트가 붙어 있다.
- 2중 방어: 서버 필터와 저장 엔진 검증이 따로 있다. "서버 필터는 설정 검증보다 좁다"는 관계를 전수 테스트로 고정했다.
- `Result`를 돌려주지 않는 설계로 "env 거부가 키 적용을 실패시킬 수 없음"을 타입 수준에서 보장했다.
- `pending_write`가 env를 함께 들고 다녀서, 쓰기 실패 재시도에서도 원자성이 유지된다.
- 옛 5000을 조용히 고치지 않으면서, 안내문이 "덮어쓰는 조건"까지 정직하게 말한다.
- 프론트가 백엔드를 믿지 않고 이름 형식을 한 번 더 거른다(심층 방어).

## PM 권고
1. **RV-01을 머지 전에 고친다.** 저장 시 바뀐 필드만 보내는 방식이 근본 해법이다. 여기에 `serverEnvChanged` 변화 시 `loadOtelEnv` 재조회를 더하고, 하네스 회귀 시나리오를 붙인다. 고치면 Green 후보다.
2. RV-02(안내 1회로 축소)·RV-04(설계 문서 233행)·RV-05(테스트 2건)는 같은 PR에서 함께 처리할 수 있다. 셋 다 작은 수정이다.
3. RV-03·RT-1: 기존 연동 기기의 5000이 남는다는 사실을 운영 판단에 넣는다. 허브 쪽에 GET 상태 응답 `env`를 넣는 것(대안 B)은 허브 저장소 결정 사항이라 별도 이슈로 남기기를 권한다.
4. 재검토 위임 시 재검토 3요소(target_id `r19-otel-server-env`, 이 보고서 경로, 리스크 범주)를 함께 보낸다.

## 생략·미확인
- 실제 Tauri 런타임 기동, `otel-link-changed` 이벤트 실발생 순서: 미확인(RV-01은 코드 경로 + 하네스 상태 재현).
- `pnpm build`, 변이 테스트: 하지 않았다.
- Claude Code가 `OTEL_LOGS_EXPORT_INTERVAL`·temporality를 실제로 어떻게 해석하는지는 외부 문서로 재확인하지 않았다(이번 지적은 그 해석에 의존하지 않는다).
- 실행 액션: 커밋·푸시·배포 없음. 보고서·페르소나·캡처 파일만 워크트리에 남겼고, 커밋은 PM이 한다.
