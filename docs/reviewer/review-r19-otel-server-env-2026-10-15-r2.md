# R19 수집키 발급·회전 201 `env` 서버 권장값 적용 — 재검토 보고서 (2차)

리뷰 대상: 워크트리 `/Users/hopegiver/workspace/malgn-vscode/.claude/worktrees/r19-server-env` · 커밋 범위 `88fa610..63b3015` (전체, 중점 `b59dd7e..63b3015`) · target_id `r19-otel-server-env`
직전 리뷰: `docs/reviewer/review-r19-otel-server-env-2026-10-15.md` (Amber, 88fa610..b59dd7e)
리스크 범주: 크리덴셜 경로 · 사용자 설정 파일(`~/.claude/settings.json`) 자동 변경 · 서버 응답 값이 사용자 파일로 들어가는 신뢰 경계
등급/모드: Sensitive, 재검토 **풀패널**(위임 지시: 축소하지 않음, 보안·발산형 포함). 노출 범위 축소 미적용.
리뷰 일자: 2026-10-15
종합 판정: 🟢 **Green** (Critical 0 · Major 0 · Minor 1 · Nit 3 · Rethink 1 신규 + RT-1 이월)

## 요약 (2분 규칙)
직전 Major RV-01이 해소됐다. 이번에 확인한 것은 셋이다. 폼은 연동 상태를 받는 4경로와 탭 재진입 때 파일을 다시 읽고, 고치던 칸은 남긴다. 저장은 직전에 파일을 다시 읽어 고친 칸과 파일에 없는 칸만 보낸다. 그래서 무관한 필드를 고쳐 저장해도 서버 값이 되돌려지지 않는다. 이 내용은 단위 테스트에 더해 **실제 앱 번들을 스텁 IPC 위에서 돌려** 직접 재현했다(아래 S1~S3). 보안 경계는 직전 결론에서 바뀌지 않았다. 허용 목록·값 규칙·저장 엔진 2차 방어는 diff 0줄이고, capabilities·tauri.conf diff도 0줄이다. 새 쓰기 예외 N-3은 apply 경로에서만 **쓰기를 줄인다**.

남은 Minor는 1건이다(RV2-01). 이번 변경 뒤 기존 UI 하네스 `otelIngestKey` 12개 중 2개가 빨갛게 됐다. 제품 버그는 아니다. 하네스 픽스처가 "저장 응답 ≠ 이후 조회 응답"을 가정하는데, 이 가정이 이제 틀리게 됐다. b59dd7e 스냅숏에서는 0 bugs로 실측했다. 이것은 머지 전에 픽스처를 고치기를 권한다.

RV-05(b) 반론은 **수용**한다. 직전 리뷰가 요구한 "회전 백업에 옛 키 없음"은 기존 설계(백업 키 제거는 Clear만)와 충돌하는 오탐이었다. 아래 "RV-05(b) 판정"에 정정 근거를 적었다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| remote-config-injection-boundary (보안) | 재사용 | 직전 라운드 T6("서버 값이 사용자 저장으로 되돌려지는가")이 이번 반영의 과녁이다 |
| credential-at-rest-and-transit (보안) | 재사용 | RV-05(b) 반론의 쟁점(회전 백업의 옛 키·0600·Clear 경로)이 역할개념 그대로다 |
| design-contract-auditor | 재사용 | 설계 문서 갱신(RV-04)과 계약 G(회전 유예)를 대조한다 |
| claimed-vs-verified | 재사용 | 개발자 보고 요지를 실측으로 재현하고 테스트 수치를 직접 돌린다 |
| screen-state-honesty | 재사용 | 화면 위 연동 상태와 입력칸이 같은 사실을 말하는지(RV-01 표시), 6키 안내 반복(RV-02)을 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 대안(RT-2)을 내고 RT-1을 이월한다 |
| async-response-ordering | 재사용(이번 라운드 추가) | 위임이 짚은 "저장 직전 재조회와 refresh의 비동기 경합"이 역할개념("늦게 도착한 비동기 응답이 지금 화면 상태를 덮어쓰지 않는가") 그대로다 |
| background-event-vs-foreground-work | 재사용(이번 라운드 추가) | 위임이 짚은 "사용자 편집 칸 손실 가능성"이 역할개념("앱이 스스로 일으킨 갱신이 사용자가 하던 일을 파괴하는가") 그대로다 |

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `cd src-tauri && cargo test` (워크트리, HEAD 63b3015) | **972 passed / 0 failed / 23 ignored** (직전 969 + 신규 3) |
| `pnpm exec tsc --noEmit` | 통과(exit 0) |
| `pnpm test:ts` | **67 pass / 0 fail** (직전 52) |
| 스크래치 러너(저장소 밖 `scratchpad/r19r2/run.mjs`): 워크트리 Vite(포트 1531) + 하네스 `bridge.mjs`·`fixtures.mjs` 재사용, `otel_settings_get`·연동 상태·`otel_settings_save`를 상태 있는 스텁으로 교체 | S1~S6 실행. 아래 표에 결과 |
| 기존 하네스 `formBackgroundRerender`·`otelAutolink`·`otelIngestKey` (워크트리) | 6/6·35/35 bugs=0. **`otelIngestKey` 12개 중 2개 bugs**(RV2-01) |
| 같은 `otelIngestKey`를 b59dd7e 스냅숏(`git archive` → scratchpad, 포트 1532)에서 실행 | 12개 **bugs=0**. 회귀 시점이 63b3015임을 확인 |
| 캡처 | `docs/reviewer/r19-otel-server-env-r2-01~03-*.png` 3장. 3장 모두 Read로 열람했다. 01(적용됨 + 바뀐 설정 3키)과 02(미연동)는 상태가 다른 화면임을 확인했다 |
| 라이브 허브 호출, 실제 `~/.claude/settings.json` 접근, 허브 저장소 수정 | **하지 않았다.** 허브 계약 문서(`~/workspace/malgnai-public/docs/design/device-ingest-key-provisioning.md`)는 읽기만 했다 |
| 실제 Tauri 런타임 기동 | **미검증.** 스텁 IPC 하네스 수준이고, Rust 커맨드의 실제 파일 I/O는 cargo 테스트가 담당한다 |
| 변이 테스트, `pnpm build` | 하지 않았다 |

### 스크래치 하네스 재현 결과
| # | 시나리오 | 관측 | 판정 |
|---|---|---|---|
| S1 | 파일 5000으로 폼을 연다 → TEMPORALITY 칸에 `delta`를 입력 중 → 파일이 60000·서버 3키로 바뀐 뒤 `otel-link-changed` 발화 → 저장 | 이벤트 뒤 입력칸 `OTEL_LOGS_EXPORT_INTERVAL=60000`, `OTEL_METRIC_EXPORT_INTERVAL=60000`, 고치던 `TEMPORALITY=delta` 보존, 5000 안내(`#otel-legacy-hint-…`) 사라짐. 연동 상세: "허브가 보낸 값으로 바뀐 설정: OTEL_LOGS_EXPORT_INTERVAL, …". 저장 payload `values` = `{"OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE":"delta"}`만 | RV-01 표시·저장 해소 |
| S2 | 폼을 연 뒤 **이벤트 없이** 파일만 바뀜(폼은 5000 표시) → METRICS_EXPORTER만 고쳐 저장 | 호출 순서 `otel_settings_get → otel_settings_save`(저장 직전 재조회). payload `{"OTEL_METRICS_EXPORTER":"otlp,console"}`만 — 5000이 실리지 않고, 파일에 새로 생긴 METRIC_INTERVAL·TEMPORALITY 기본값도 실리지 않음 | 이벤트를 놓친 경로도 되돌리지 않음 |
| S3 | 폼 로드 → 파일 변경 → `#/settings/mcp` → `#/settings/otel` 재진입 | 입력칸 60000, 5000 안내 없음 | 탭 재진입 재조회 확인(`main.ts:285-288`) |
| S4·S5 | 6키 안내 문장 계수 | 설정 미연동 1회, 설정 적용됨 1회, 홈(미연동) 1회 | RV-02 해소 |
| S6 | 파일 변경 없이 이벤트만 발화, 입력 중 | 값 보존. 포커스는 잃는다 — 이번 변경과 무관한 기존 동작이다(아래 기각 표) | — |

## 직전 지적 상태 추적
| ID | 직전 | 이번 상태 | 근거(파일:라인) | 확인방법 |
|---|---|---|---|---|
| RV-01 | 🟠 Major | **해소** | 연동 상태 대입이 한 곳(`applyIngestStatus`, `src/views/settings.ts:161-165`)으로 모였고, 4경로(`loadIngestLink`·`retryIngestLinkNow`·`recheckIngestLinkAfterSave`·`askReissueConfirm`)가 모두 이곳을 거친다. 재조회는 `refreshOtelFormFromFile`(`:410-423`)이 맡고, seq·saving·loading 가드와 내용 동일 시 무동작 처리가 있다. 탭 재진입은 `src/main.ts:285-288`. 병합 `mergeDraftOnRefresh`(`src/otelForm.ts:32-49`), 저장 `handleSaveOtelSettings`가 저장 직전 재조회(`settings.ts:271-277`) 후 `valuesToSave`(`otelForm.ts:57-74`)를 부른다. 수집키 삭제 응답도 `adoptOtelSettings`를 탄다(`settings.ts:307-308`) | 코드 열람, `otelForm.test.mjs` 13건, 배선 가드(`ingestLink.status =` 대입 1곳 grep), 스크래치 S1·S2·S3 |
| RV-02 | 🟡 Minor | **해소** | `INGEST_KEY_COPY.issue/issueManaged`에서 안내문 제거(`settings.ts:115-118`). 연동 상세는 `withoutAutoLinkNote`(`src/onboarding.ts:189-191`, 사용처 `settings.ts:607`). 남은 곳은 `otel-autolink-changes-note`(`:528`) 1곳이다 | 스크래치 S4·S5 DOM 계수, `onboardingOtelLink.test.mjs` RV-02 테스트 |
| RV-03 | 🟡 Minor | **대부분 해소**(잔존 N-4) | 머리말 `src-tauri/src/otel_link.rs:33-44`에 "반영 범위(주의)"가 생겨 발급·회전 201 때만 반영되고 기존 기기 5000은 남는다고 명시했다. 설계 문서 `:236`도 같다. 그런데 `otel_settings.rs:565-566` doc 주석에는 "(서버가 값을 바꾸면 앱 수정 없이 따라간다)"가 그대로 남았다 | `grep -rn "앱 수정 없이"` |
| RV-04 | 🟡 Minor | **해소** | `docs/design/autonomy-runtime-and-config.md:233`(60000/60000, 옛 5000 주석), `:236`(6키·값 규칙·otlp 목록 보존·이행 안 함·전파 범위·정본 링크) | 파일 열람 |
| RV-05(a) | 🟡 Minor | **해소** | `otel_link_engine/tests.rs:1699-1720`. env 실린 201이 해제 뒤 도착해도 키·env 모두 미기록, `env` 객체 전체가 해제 전과 같고, `env_changed` 비어 있고, `pending_write` 없음 | 테스트 본문 열람, cargo 실행 |
| RV-05(b) | 🟡 Minor | **반론 수용 — 직전 지적 기각(오탐 정정)** | 아래 "RV-05(b) 판정" | 계약 G·저장 엔진·기존 테스트 대조 |
| N-1 | ⚪ Nit | 이월(미반영, 수용 가능) | 거부 이름 문구 위조 가능성은 그대로다. 형식 필터상 HTML은 불가능하다 | — |
| N-2 | ⚪ Nit | **해소** | `ENV_NAME_SHAPE` `\d{1,10}`(`onboarding.ts:195`), 형식에 걸려 버린 이름은 "(표시하지 않은 N개)"로 셈(`:197-202`) | `onboardingOtelLink.test.mjs` N-2 테스트(999·1200·u32 끝값·가짜 11자리) |
| N-3 | ⚪ Nit | **해소** | `save_reporting_changes_with(…, keep_otlp_exporter_lists)`(`otel_settings.rs:410-415, 458-469`). `apply_ingest_key`만 `true`를 넘긴다(`:571-585`). `pending_write` 재시도도 `apply_guarded` → `apply_ingest_key`(`otel_link_engine.rs:385`)라 같은 예외를 탄다. 제외된 키는 "바뀐 설정"에 보고하지 않는다 | `server_otlp_does_not_strip_an_existing_otlp_list_but_replaces_a_non_otlp_value`(`:1674`) — `console, OTLP` 보존, `none`→`otlp` 교체, 수동 저장은 예외 미적용 |
| RT-1 | 🔵 Rethink | 이월 | 설계 문서 `:236`이 "허브 상태 응답에 env를 싣는 안은 별도 결정"으로 정직하게 남겼다 | — |

### RV-05(b) 판정 — 반론 수용
직전 리뷰는 "env가 실린 회전 테스트에 백업에 옛 키가 없다는 단언을 추가하라"고 했다. 그런데 이 요구는 기존 설계와 맞지 않는 오탐이었다.
- 저장 엔진 정본은 이렇다. "쓰기 전 원본을 백업(롤링 1세대). **수집키 삭제(Clear)일 때만** 백업에서도 `Authorization`을 뺀다"(`otel_settings.rs:389-390`, 구현 `:534-541`). R19 이전부터 있던 테스트 `settings_and_backup_are_owner_only_on_unix`가 "교체 저장의 백업은 직전 원본"이라고 고정한다(`:1336`). 직전 리뷰가 요구사항 표에 적은 "백업(키 제외)"은 Clear 경로의 불변식을 회전 경로까지 넓혀 읽은 것이었다.
- 연결 해제는 `remove_ingest_key` → `IngestKeyChange::Clear`를 거친다(`otel_settings.rs:742-756`). 그래서 기기를 끊으면 백업에서도 키가 빠진다.
- 계약에서 옛 키가 언제 무효가 되는지 확인했다(`malgnai-public/docs/design/device-ingest-key-provisioning.md` §G 233-234행). **회전된 옛 키는 즉시 무효가 아니다. 48시간 유예 동안 유효하고**, 그 안에 다시 회전하면 그때 즉시 무효가 된다. 따라서 위임문의 "회전된 옛 키는 서버가 무효화함"은 "최대 48시간 뒤, 또는 다음 회전 때 무효화"로 정확히 읽어야 한다.
- 그래도 수용하는 이유: 백업은 본 파일과 같은 디렉터리, 같은 0600이다(테스트 `:1867` 끝의 `mode_of` 단언). 본 파일에는 **지금 유효한** 새 키가 있다. 그래서 백업을 읽을 수 있는 주체는 본 파일의 더 강한 키도 이미 읽을 수 있다. 백업이 새 공격자 집단을 만들지 않고, 옛 키의 잔여 수명은 48시간 이하다.
- 개발자가 대신 넣은 단언(`tests.rs:1867-1892`)은 의미가 있다. 백업이 원본과 바이트 동일하고, 새 키 원문과 서버 값(60000)이 백업에 없으며, 본 파일에 옛 키가 없고, 백업이 0600이다.
- 남는 것은 테스트 주석의 "회전으로 **폐기되는** 옛 키"(`tests.rs:1868`)라는 표현이다. 계약상 48시간 유예라 부정확하다(N-5).

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 확인방법 | 개선안 | 관점 |
|---|---|---|---|---|---|---|
| RV2-01 | 🟡 Minor | `scripts/ui-harness/flows/otelIngestKey.mjs:204-209`(`set-masked`)·`clear-flow`, 원인 `src/views/settings.ts:161-165, 410-423` | 63b3015 뒤 기존 하네스 `otelIngestKey` 2개 시나리오가 실패한다: "저장 후 새 마스킹이 표시되지 않음: 설정됨 mhk_AAAA…", "삭제 후 미설정 배지가 아님". 원인은 이렇다. 저장 뒤 `loadIngestLink` → `applyIngestStatus` → `refreshOtelFormFromFile`이 `otel_settings_get`을 다시 읽는다. 그런데 픽스처의 조회 값이 고정(SET_A)이라 저장 응답(SET_B)을 옛 값으로 덮는다. **제품 버그는 아니다.** 실제 `otel_settings_save`는 쓰기 뒤 `otel_settings_get()`을 그대로 돌려주므로(`otel_settings.rs:785-793`) 이후 조회와 같다. 다만 회귀망 둘이 빨간 채로 남으면 이 화면의 수집키 마스킹·삭제 회귀를 더는 잡지 못한다. 또 이 화면에는 "이벤트 → 재조회 → 입력칸"을 DOM 수준에서 고정하는 하네스 시나리오가 없다(직전 권고). `otelForm.test.mjs`의 배선 가드는 소스 정규식이다 | 워크트리와 b59dd7e 스냅숏에서 같은 흐름 실행: 63b3015 bugs=2 / b59dd7e bugs=0. `last-run-report.json`(gitignore 대상)으로 증상 확인 | 하네스 스텁을 상태 있게 바꾼다(`otel_settings_save` 응답을 이후 `otel_settings_get` 응답으로 쓴다). 이 스크래치 러너의 `dyn()` 방식을 그대로 옮기면 된다. 같은 장치로 S1·S2를 `otelAutolink` 시나리오로 추가한다 | claimed-vs-verified, background-event-vs-foreground-work |
| N-4 | ⚪ Nit | `src-tauri/src/otel_settings.rs:565-566` | `apply_ingest_key` doc 주석에 "(서버가 값을 바꾸면 앱 수정 없이 따라간다)"가 남아 있다. 머리말·설계 문서가 좁힌 범위("새 키를 받을 때만")와 어긋난다(RV-03 잔존). `otel_server_env.rs:12`의 같은 구절은 "201 한 번 안에서 한 키 거부가 다른 키를 막지 않는다"는 맥락이라 그대로 둬도 된다 | `grep -rn "앱 수정 없이"` | "(발급·회전 201을 받을 때 서버 값이 이긴다)"로 좁힌다 | claimed-vs-verified |
| N-5 | ⚪ Nit | `src-tauri/src/otel_link_engine/tests.rs:1868` | 주석 "회전으로 폐기되는 옛 키"는 계약 §G(48시간 유예)와 다르다 | 계약 233-234행 대조 | "회전 뒤 48시간 유예(또는 다음 회전)까지 유효한 옛 키가 백업에 한 세대 남는 것은 설계다 — 본 파일과 같은 0600, 해제(Clear)는 백업에서도 뺀다"로 고친다 | credential-at-rest-and-transit |
| N-6 | ⚪ Nit | `src/otelForm.ts:40-46`, `src/views/settings.ts:401-405` | 사용자가 고치던 칸을 서버가 같은 키로 바꾸면 드래프트가 이긴다(명시적 선택, 테스트 `otelForm.test.mjs` "typing" 단언). 이때 입력칸 옆에는 "파일 값이 그 사이 X로 바뀌었다"는 표시가 없다. 위쪽 "바뀐 설정"에 키 이름만 뜨므로, 저장하면 서버 값을 덮는다는 사실이 필드 단위로는 보이지 않는다 | 코드 추적 + S1 변형 추론(실행하지 않음) | 병합 때 `edited && next 파일 값 ≠ prev 파일 값`인 칸에 "파일 값이 바뀌었습니다: 60000" 힌트를 단다. 빈도가 낮아 후속으로 미뤄도 된다 | screen-state-honesty |
| RT-2 | 🔵 Rethink | `src/otelForm.ts:57-74`, `src/views/settings.ts:271-277`, `src-tauri/src/otel_settings.rs:410-` | "무엇이 고친 칸인가"를 프론트가 저장 직전 재조회로 판정한다. 그래서 재조회와 Rust 쓰기 사이의 ms 창, 재조회 실패 시 화면이 아는 옛 값으로 판단하는 폴백 경로(`:271-276`)가 구조적으로 남는다. 둘 다 결과는 "파일에 없던 칸의 기본값 기록" 정도로 작다(기각 표) | 코드 추적 | **대안**: 저장 payload에 칸마다 `base`(폼이 본 파일 값, 없으면 absent)를 싣는다. 저장 엔진은 **락 안에서** "파일 현재 값 == base"이거나 사용자가 고친 칸만 쓴다(compare-and-set). 프론트 재조회와 폴백이 사라지고, Rust 테스트 하나로 "되돌리지 않음"을 파일 결과로 고정할 수 있다. 현 구조로도 실사용 위험은 낮으므로 다음 라운드 후보다 | zero-base-redesigner |
| RT-1 | 🔵 Rethink(이월) | 직전 보고서 RT-1 | 전파를 키 발급 사건에 묶은 구조. 설계 문서가 범위를 정직하게 적었다 | — | 허브 상태 응답 `env`(대안 B)는 허브 저장소 결정 사항이다 | zero-base-redesigner |

## 경합·회귀 점검 (위임 확인 항목)
| 시나리오 | 결과 | 근거 |
|---|---|---|
| refresh 응답이 저장 완료 **뒤** 도착해 저장 결과를 옛 파일로 덮는다 | 발생하지 않는다 | refresh는 시작할 때(`settings.ts:411`)와 응답 뒤(`:419`) 모두 `saving`을 본다. `otel_settings_get`은 동기 커맨드(`otel_settings.rs:205-206`, async 아님)라 먼저 보낸 refresh 조회가 먼저 끝나고, 그 시점에는 `saving=true`라 버려진다. 만에 하나 옛 값을 채택해도 다음 저장은 재조회한 `fileValues` 기준으로 판정하므로 파일을 되돌리지 않는다(표시만 다음 이벤트·재진입까지 틀림) |
| refresh 중 사용자가 입력 | 보존 | `boundField`가 입력마다 `otelDraft.values`를 갱신하고(`src/dom.ts:425-426`), 병합은 응답 시점의 드래프트로 동기 처리된다(`settings.ts:401-405`). S1 실측 |
| 저장 중 입력 | 기존 동작 그대로 | 저장 성공 시 새 settings 참조로 드래프트를 다시 채우는 것은 변경 전과 같다 |
| 재조회 실패 폴백 | 영향 작음 | `otel_settings_get`은 `Result`가 아니라 IPC 자체 실패만 throw한다. 폴백도 "고친 칸 + (옛 화면 기준) 파일에 없던 칸"만 보낸다. 변경 전(전부 전송)보다 언제나 좁다 |
| 보안 경계 | 직전 결론 유지 | `otel_server_env.rs`, 저장 엔진 검증(`otel_settings.rs:420-437`), capabilities·tauri.conf diff 0줄(`git diff 88fa610..63b3015 -- src-tauri/capabilities src-tauri/tauri.conf.json`). N-3은 쓰는 키를 **빼기만** 하고 더하지 않는다. 수동 저장 경로는 이 예외를 타지 않는다(테스트로 고정) |
| 수집키 원문 노출 | 없음 | 프론트 diff는 `values`만 다룬다. `ingestKey`는 기존 경로 그대로이고, 저장 성공 시 `otelDraft.ingestKey=''`. `adoptOtelSettings`는 `ingestKey` 드래프트를 보존만 한다 |

## 페르소나별 관점

### remote-config-injection-boundary (보안)
직전 T6 미충족이 충족으로 바뀌었다. 서버가 쓴 값은 이제 사용자 저장으로 되돌려지지 않는다(S1·S2). T1~T4는 diff 0줄로 유지된다. N-3 예외는 서버가 쓸 수 있는 범위를 넓히지 않고, `otlp`를 이미 포함한 목록일 때 쓰기를 생략할 뿐이다. 준비 판정은 같은 `exporter_includes_otlp`(`otel_settings.rs:642-646`)를 쓰므로 생략해도 수집 상태가 같다. 합격이다.

### credential-at-rest-and-transit (보안)
RV-05(b)는 위 판정대로 수용한다. 회전 백업의 옛 키는 기존 설계이고, 같은 0600·같은 위치·잔여 수명 48시간 이하라 노출 면을 넓히지 않는다. 해제는 Clear 경로로 백업에서도 키를 뺀다. RV-05(a) 테스트로 "해제 뒤 201의 env"까지 세대 가드가 막는다는 사실이 파일 결과로 고정됐다. 남은 것은 주석 표현 N-5뿐이다.

### design-contract-auditor
설계 문서 233·236행이 계약 §14 Q3의 앱 쪽 결론을 조항 수준으로 남긴다. 담긴 내용은 6키, 값 규칙, otlp 목록 보존, 이행하지 않음, 전파 범위, 정본 링크다. 계약 §G(회전 유예 48시간)와 백업 정책 사이의 관계는 문서에 없다. 다만 앱 설계의 백업 규칙(Clear만 제거)은 R12에서 정한 것이라 이번 범위 밖이다.

### claimed-vs-verified
개발자 보고 요지 4개를 모두 실측으로 재현했다. 4경로·탭 재진입 재조회, 고친 칸 보존, 저장 직전 재조회 후 고친 칸과 파일에 없는 칸만 전송, 6키 1회다. 수치는 cargo 972와 test:ts 67을 직접 확인했다. 다만 기존 UI 하네스 2건 회귀(RV2-01)는 보고에 없었다. 새 `otelForm.test.mjs`는 순수 함수 단언 10건과 소스 정규식 배선 가드 3건으로 이뤄져 있다. 배선 가드는 "대입이 한 곳"을 잘 고정하지만, DOM 결과를 단언하지는 않는다.

### screen-state-honesty
캡처 01에서 위쪽 연동 상세가 "허브가 보낸 값으로 바뀐 설정: 3키"라고 말하고, 아래 입력칸도 60000을 보여 준다(DOM 덤프). 이제 같은 화면이 같은 사실을 말한다. 5000 안내는 파일이 바뀌면 사라진다. 캡처 02에서 6키 안내는 폼 위쪽 한 곳에만 있다. N-6은 드물게 일어나는 필드 단위 고지 공백이다.

### async-response-ordering
A1(완료 분기의 최신성 검사): refresh에 seq·saving·loading·settings 존재 검사가 있다. A3(오래된 응답이 플래그를 건드리지 않음): refresh는 플래그를 쓰지 않는다. 저장 직전 재조회는 같은 함수 안에서 `await`하므로 순서 문제가 없다. 도착 순서 A→B와 B→A를 손으로 추적한 결과는 위 경합 표와 같다. 합격이다.

### background-event-vs-foreground-work
배경 트리거(`otel-link-changed`)가 사용자가 입력 중이던 값을 지우지 않는다(S1). 내용이 같으면 재렌더 자체를 하지 않는다(`sameFormContent`, `settings.ts:420`). 포커스를 잃는 것은 이번 변경 전부터 있던 이벤트 재렌더 동작이다(S6, 아래 기각 표). 검증망 기준(배경 이벤트를 전경 작업 도중에 발화하는 시나리오)은 이번 스크래치 러너가 처음 충족했고, 저장소 하네스에는 아직 없다(RV2-01).

### zero-base-redesigner (발산형)
RT-2를 낸다. "고친 칸"을 프론트가 재조회로 추정하는 방식을, 저장 엔진의 락 안 compare-and-set으로 옮기자는 것이다. 그러면 프론트 재조회 왕복, 폴백 경로, ms 창이 모두 구조에서 사라지고, 불변식을 Rust 파일 결과 테스트 하나로 고정할 수 있다. RT-1은 이월한다.

## 트레이드오프
- **사용자 입력 우선 vs 서버 값 우선(편집 중 충돌)**: 개발자는 "고치던 칸은 사용자가 이긴다"를 택했다. 요구사항 1(서버 값 우선)은 **자동 적용** 경로의 규칙이다. 사용자가 그 칸을 직접 고쳐 저장하는 것은 명시적 선택이므로 이 해석이 타당하다. 대가는 N-6(필드 단위 고지 부재)이다.
- **프론트 diff(현재) vs 백엔드 CAS(RT-2)**: 현재 방식은 Rust 변경 없이 고쳐서 범위가 작다. 남는 창은 실사용에서 사실상 닿지 않는다. RT-2는 근본적이지만 저장 payload 계약이 바뀐다.

## 기각된 지적
| 후보 | 기각 사유 | 확인방법 |
|---|---|---|
| (직전 RV-05(b)) 회전 백업에 옛 키가 없음을 단언해야 한다 | 기존 설계(Clear만 백업 키 제거)와 충돌하는 오탐이었다. 노출 면도 넓히지 않는다. 위 판정 참조 | `otel_settings.rs:389-390, 534-541, 742-756, 1336`, 계약 §G 233-234행 |
| 저장 직전 재조회와 Rust 쓰기 사이에 자동 연동이 끼면, 파일에 없던 칸의 기본값이 서버 값을 덮는다 | 사용자가 저장을 누른 그 ms 창에 1시간 주기 점검의 201 쓰기가 겹쳐야 한다. 재현 경로를 대지 못한다. 덮는 값도 현재 앱 기본값(60000 등)이라 서버 권장값과 대개 같다. 구조 대안은 RT-2로 남겼다 | 코드 추적 |
| 재조회 실패 폴백이 옛 화면 값을 기준으로 기본값을 쓴다 | `otel_settings_get`은 실패를 값으로 돌려주는 동기 커맨드라 IPC 자체가 죽지 않는 한 throw하지 않는다. 폴백도 변경 전(전부 전송)보다 좁다. 파일이 파싱 불가로 바뀌면 저장 엔진이 거부한다 | `otel_settings.rs:205-212`, 폼 `parseError` 분기 |
| refresh가 옛 파일 값을 채택해 저장 결과 표시를 덮는다 | 위 경합 표 1행. 동기 커맨드 순서와 `saving` 이중 검사로 닿지 않는다. 닿더라도 다음 저장이 재조회로 판정해 파일은 안전하다 | 코드 추적 |
| 배경 이벤트 재렌더 때 OTel 입력칸 포커스를 잃는다 | 파일 변경 없이 이벤트만 발화해도 같다(S6). 이번 변경 전부터 `loadIngestLink`가 `notifyChange`로 재렌더하던 동작이고, OTel 입력은 `boundField`에 `fieldId`를 넘기지 않는다(`settings.ts` renderOtelPanel). 이번 diff 범위 밖이다. 값 보존은 기존 하네스 `formBackgroundRerender`(6/6)가 고정한다 | 스크래치 S6, 하네스 실행 |
| N-3 대소문자: 파일 `OTLP`(대문자 단일)도 보존돼 서버 `otlp`로 정규화되지 않는다 | 준비 판정이 같은 대소문자 무시 함수를 쓰므로 수집 상태가 같다. 서버 의도(otlp로 내보냄)를 해치지 않는다 | `otel_settings.rs:642-646` |

## 잘 된 점
- 연동 상태 대입을 `applyIngestStatus` 한 곳으로 모았고, 그 사실을 배선 가드 테스트(대입 1곳)로 고정했다. 다음에 다섯 번째 경로가 생겨도 재조회를 건너뛸 수 없다.
- 순수 함수 모듈 `otelForm.ts`로 저장 규칙을 DOM과 분리했다. "고친 칸 / 파일에 없는 칸 / 읽기 전용 제외 / 빈 값 = 삭제 신호"가 각각 테스트로 고정돼 있다.
- 재조회 실패, 저장 중, 로딩 중의 가드가 모두 "파일을 되돌리지 않는 쪽"으로 기운다. 변경 전보다 언제나 덜 쓴다.
- N-3을 저장 엔진 플래그로 넣으면서 수동 저장 경로에는 적용되지 않음을 같은 테스트에서 단언했다(예외가 새어 나가지 않음).
- RV-05(b)를 무조건 받아들이지 않고, 설계 근거를 들어 반론하면서 의미 있는 대체 단언을 넣었다.
- 머리말과 설계 문서가 "자동으로 따라간다"를 스스로 좁혀 반영 범위를 정직하게 적었다.

## PM 권고
1. **게시 승인 가능(Green).** Critical·Major가 없다.
2. 머지 전 권장: **RV2-01** 하네스 `otelIngestKey` 스텁을 상태 있게 바꿔 2건을 녹색으로 되돌리고, S1·S2를 `otelAutolink` 시나리오로 추가한다. 작은 수정이고 제품 코드는 바뀌지 않는다.
3. N-4·N-5는 주석 두 줄이라 같은 커밋에서 처리할 수 있다. N-6·RT-2는 다음 라운드 후보다.
4. RT-1(기존 기기 5000 잔존)은 허브 저장소 결정 사항으로 별도 이슈 유지를 권한다.

## 생략·미확인
- 실제 Tauri 런타임 기동과 `otel-link-changed` 실발생 순서: 미확인. 스텁 IPC 하네스 + cargo 파일 결과 테스트로 대신했다.
- 동기 커맨드가 메인 스레드에서 FIFO로 처리된다는 순서 근거: Tauri v2 동작에 대한 코드 수준 판단이고, 런타임에서 관측하지 않았다. 순서가 뒤집혀도 파일은 안전하다(경합 표).
- N-6은 추론이며 실행하지 않았다.
- `pnpm build`, 변이 테스트: 하지 않았다.
- 실행 액션: 커밋·푸시·배포 없음. 이번에 워크트리에 남긴 것은 이 보고서, 캡처 3장(`r19-otel-server-env-r2-01~03-*.png`), 페르소나 8개 파일의 적용 이력 append, `INDEX.md` 8행 갱신이다(모두 미커밋). 스크래치 러너·b59dd7e 스냅숏·하네스 원본 캡처는 저장소 밖 scratchpad에 있다. 기존 하네스 실행이 만든 `scripts/ui-harness/last-run-report.json`은 gitignore 대상이다.
