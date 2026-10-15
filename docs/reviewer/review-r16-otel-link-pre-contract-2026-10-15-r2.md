# R16 2차 재검토: OTEL 수집키 연동 선행 작업 (허브 계약 전)

리뷰 대상: target_id `r16-otel-link-pre-contract` / 커밋 범위 `12ac008..04bebf4` (`0e01057` + 반영 커밋 `04bebf4`, 반영 diff `0e01057..04bebf4`)
- 종합 판정: **🟢 Green**
- 등급: Sensitive. 리스크 범주: 인증·크리덴셜(`mhd_` 키체인, `mhk_` settings.json 평문)
- 직전 리뷰: `docs/reviewer/review-r16-otel-link-pre-contract-2026-10-15.md` (Amber, Major RV-001, Minor RV-002~004)
- 일자: 2026-10-15 / 리뷰어: reviewer
- 실행 액션: 없음. 코드 수정·커밋·push·네트워크 쓰기를 하지 않았다. 실행한 것은 `cargo test ... otel`, `cargo check`, `pnpm test:ts`, `node scripts/pre-push-sensitive-scan.mjs`, 읽기 전용 git/grep, Claude Code 공식 문서 조회(WebFetch)다. 이 보고서를 쓰고, 페르소나 5개의 "적용 이력"과 INDEX.md "최근 재사용"에 한 줄씩 덧붙였다(커밋은 PM 몫)

## 모드 판정

- 재검토 3요소(target_id, 직전 리뷰 경로, 리스크 범주)가 모두 위임에 있다
- 동일 대상 인정 조건 4개를 직접 대조했다. target_id가 같다. 직전 리뷰와 같은 날이다. 리스크 범주가 같다. 대상 파일(`otel_link.rs`, `otel_settings.rs`, `onboarding.ts`, `home.ts`)이 겹친다
- 다만 풀패널 강제 승격 조건 "직전 Critical/Major 미해결 존재"에 걸린다. 직전 Major RV-001의 해소 여부가 바로 이번 검증 대상이기 때문이다. 그래서 축소·증분으로 내리지 않았다. 직전 5인을 그대로 풀패널로 투입했다(발산형 포함)

## 페르소나 재사용 판정

착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 이번 반영으로 새로 생긴 표면은 수집 준비도 플래그, 발급 전 엔드포인트 선검사, 필수 완료 후 한 줄 노출이다. 셋 다 기존 역할개념(크리덴셜 목적지, 주장 대 관측, 첫 실행 노출)으로 덮인다. 그래서 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-credential-at-rest-and-transit.md` | 재사용 | RV-004 선검사는 "키가 실려 나가는 목적지" 역할개념 그대로다 |
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | 새 플래그와 한 줄 노출에 URL이나 키 원문이 섞이는지 본다 |
| `persona-claimed-vs-verified.md` | 재사용 | RV-002·RV-003 문구, 그리고 `collectionReady`가 관측한 만큼만 주장하는지 본다 |
| `persona-first-run-newcomer.md` | 재사용 | RV-001(필수 완료 후 노출)의 완주 경로를 본다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | RT-1·RT-2 이월 상태를 확인하고, 새 구조 쟁점을 찾는다 |

생략한 관점: `persona-design-contract-auditor`는 투입하지 않았다. 허브 API 계약이 아직 없어 대조할 정본 조항이 없다(1차와 같은 사유).

## 직접 확인한 것

| 확인 | 방법 | 결과 |
|---|---|---|
| Rust 테스트 | `cargo test --manifest-path src-tauri/Cargo.toml otel` | 87 passed / 0 failed (1차 79에서 8개 늘었다) |
| TS 테스트 | `pnpm test:ts` | 28 passed / 0 failed (1차 23에서 늘었다) |
| N-2 경고 | `cargo check --message-format short`에서 `otel_` 경고 grep | 0건. 모듈 단위 `#![allow(dead_code)]`가 없어졌고, 항목 단위 allow로도 경고가 남지 않는다 |
| 허브 호출 미추가 | `git diff 0e01057..04bebf4`의 추가 라인에서 `reqwest\|ureq\|https?://\|/api/\|StatusCode\|fetch(\|invoke(` grep | 9건이 걸렸다. 모두 `otel_link.rs` 테스트 픽스처다(`collector.corp.example/v1/…`, `localhost:4318/v1/metrics`). 허브 경로는 `hub_endpoint("/v1/…")`로 OTLP 엔드포인트 설정값만 조립한다. 제품 코드에 네트워크 호출은 없다 |
| capabilities·의존성·커맨드 등록 | `git diff --stat 0e01057..04bebf4 -- src-tauri/capabilities src-tauri/Cargo.toml src-tauri/src/lib.rs` | 변경 없음. 새 커맨드·crate·권한이 없다 |
| 시크릿 리터럴 | 추가 라인에서 `(mhd_\|mhk_)[A-Za-z0-9_-]{20,}` grep, `pre-push-sensitive-scan.mjs` | 0건, exit 0 |
| 로그 출력 | `otel_link.rs`에서 `eprintln\|println\|log::\|tracing::` grep | 0건 |
| 상태 원문 비노출 | `applied_status_reports_whether_collection_can_actually_happen`의 `assert_status_clean`(직렬화와 Debug에 URL·키·"not a url"이 없음), `foreign_or_plaintext_endpoint_stops_before_issuing`(collector·localhost·KKKK 없음) | 통과. 새 필드는 불리언과 `EndpointAssessment`(불리언 3개)뿐이다 |
| 화면 PNG 7장 | Read로 01·03·05·06·07을 열었다. md5로 7장이 모두 서로 다른 파일임을 확인했다 | 01 미연동: 문구가 2줄로 줄었다. 03 헤더 문구: 새 문구이고 "자동"이 없다. 05 applied + 텔레메트리 꺼짐: `!`, "수집되지 않을 수 있습니다: 텔레메트리가 꺼져 있음", "OTel 설정 열기". 06 필수 완료: 박스는 없고 `○ 선택 · 사용량 수집키가 적용되어 있지 않습니다.` 한 줄과 "수집키 입력하기"만 있다. 07 필수 완료 + 준비 완료: 박스도 한 줄도 없다 |
| 필수 완료 + failed/warn 한 줄 | 하네스 시나리오 `otel-link-core-done-slim-failed`, `…-slim-applied-not-ready`의 단언을 읽음 | **PNG는 없다.** 단언만 있다(박스 0개, slim 정확히 1개, data-state, 버튼, ✓ 금지, 금지어, DOM 원문) |
| 하네스 재실행 | — | **하지 않았다.** bugs=3이 기준선과 같다는 것은 PM 보고를 그대로 인용한 것이다(fontPolicyKoreanMonoScan 1, projectAgentProgress 2. PM이 12ac008 worktree에서 확인) |
| 외부 사실 대조 | Claude Code 공식 문서 `code.claude.com/docs/en/monitoring-usage` | 필수 변수는 `CLAUDE_CODE_ENABLE_TELEMETRY`다. OTLP로 내보내려면 `OTEL_METRICS_EXPORTER`/`OTEL_LOGS_EXPORTER=otlp`가 필요하다. 프로토콜과 엔드포인트에는 기본값이 없다. 신호별 엔드포인트가 없으면 범용 `OTEL_EXPORTER_OTLP_ENDPOINT`를 쓴다 → RV-005 근거 |
| CI 실제 통과 | — | 확인하지 않았다(로컬에서만 실행) |

## 직전 지적 해소 상태

| ID | 직전 | 이번 | 근거(원본 라인) |
|---|---|---|---|
| RV-001 | 🟠 Major | **해소** | `src/views/home.ts:166-180`: `allDone`이면 박스를 다시 열지 않는다. OTEL 단계에 `slim`이 있을 때만 `.onboard-slim[data-step="otel-link"]` 한 줄을 그린다. `src/onboarding.ts`의 `slim`은 unlinked·warn·failed에만 있다. applied+준비 완료, loading, 조회 실패에는 없다. PNG 06·07과 하네스 5개 시나리오(`otel-link-core-done-*`)가 이를 고정한다. 기존 사용자 회귀 방지(박스 비노출)도 `checkCoreDoneSlim`이 Critical로 단언한다 |
| RV-002 | 🟡 Minor | **해소** | `src-tauri/src/otel_link.rs:96-98` 문구가 "…Authorization 헤더가 있습니다. 허브로 보내려면 설정 > OTel에서 수집키로 교체하세요."로 바뀌었다. 시도하지 않은 자동 동작을 말하지 않는다. 금지어에 "자동으로 바꾸/교체/변경/삭제/갱신", "바꾸지 않았습니다"가 추가됐다(TS 테스트와 하네스 양쪽). Rust 테스트 `file_derived_header_message_states_the_current_fact_without_implying_automation`도 고정한다. `failed`를 `todo`로 낮추는 선택 권고는 반영하지 않았다. 문구가 중립이 됐으므로 수용한다 |
| RV-003 | 🟡 Minor | **해소(잔여 RV-005로 분리)** | `IngestLinkStatus`에 `telemetry_enabled`·`endpoint`·`collection_ready`가 추가됐다(`otel_link.rs:152-161`, `:186-194`). 준비가 안 됐으면 `warn`으로 표시하고 "새로 시작하는 … 적용됩니다" 문구를 빼는 분기가 있다(`onboarding.ts:207-225`). URL은 싣지 않는다. 다만 "준비됨" 판정의 입력이 관측보다 넓다 → RV-005 |
| RV-004 | 🟡 Minor | **해소(잔여 RV-005로 분리)** | `otel_link.rs:320-322`가 `foreign_host \|\| insecure`이면 `issuer.issue()` 전에 `EndpointNotHub`로 멈춘다. 테스트는 5개 변형에서 발급 호출 0, 파일 불변, 백업 미생성, 원문 비노출을 확인한다(회전 경로 포함). 허브 https 엔드포인트는 막지 않는다는 반대 방향 테스트도 있다. 다만 엔드포인트가 파일에 없을 때의 판정에 빈틈이 있다 → RV-005 |
| N-1 | ⚪ Nit | 해소 | `otel_link.rs:44-47`, 머리말 `:11-13`, `src/otelApi.ts:79-80` 모두 "Issuing은 백엔드 응답으로 오지 않는다"로 바뀌었다 |
| N-2 | ⚪ Nit | 해소 | 모듈 allow를 지우고 항목 단위 allow로 좁혔다. 항목마다 사유 주석이 있다. `cargo check` 경고 0 |
| N-3 | ⚪ Nit | 이월(의도) | `loadOtelEnv` 후 연동 상태를 다시 읽지 않는 것은 그대로다 |
| N-4 | ⚪ Nit | 해소 | 미연동 문구가 3문장에서 1문장 + 부연으로 줄었다(PNG 01에서 2줄) |
| RT-1 | 🔵 Rethink | 이월(의도) | `confirm_receipt`는 이제 준비도를 보존한 채 `Confirmed`를 돌려준다(`..base`). 하지만 저장 위치가 없어 다음 조회에서 applied로 돌아간다는 구조 문제는 그대로다 |
| RT-2 | 🔵 Rethink | 이월(의도) | RV-001의 저비용 임시책(한 줄 노출)이 들어갔다. 위젯 이동은 하지 않았다 |

## 신규 지적 사항

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 | 축 |
|---|---|---|---|---|---|---|
| RV-005 | 🟡 Minor | `src-tauri/src/otel_settings.rs:519-549` (`IngestInputs`, `read_ingest_inputs`), `:129-141` (`assess_effective_endpoints`) / `src-tauri/src/otel_link.rs:320` / 테스트 `otel_link.rs:941` | 수집 준비도와 발급 전 선검사가 설정 화면의 판정(`assess_effective_endpoints`: 파일 값 → **앱 기본값**)을 재사용한다. 그런데 이 앱 기본값은 폼 미리 채움용이다. Claude Code는 이 값을 모른다. 공식 문서상 엔드포인트와 프로토콜에는 기본값이 없고, 신호별 엔드포인트가 없으면 범용 `OTEL_EXPORTER_OTLP_ENDPOINT`를 쓴다. 결과는 세 가지다. ① 키 + `TELEMETRY=1`만 있고 엔드포인트가 없는 파일을 `collectionReady=true`로 판정해 홈에서 숨긴다. 테스트 "텔레메트리 1 + 엔드포인트 생략(기본값=허브)"이 이것을 의도로 고정한다. 실제로는 수집되지 않는다. ② `OTEL_METRICS_EXPORTER`/`OTEL_LOGS_EXPORTER`/`OTEL_EXPORTER_OTLP_PROTOCOL`이 없어도 준비됨으로 본다. ③ 범용 `OTEL_EXPORTER_OTLP_ENDPOINT`는 `MANAGED_OTEL_KEYS`에 없어 아예 읽지 않는다. 그래서 파일에 범용 엔드포인트가 허브 밖으로 있고 신호별 값이 없으면 선검사를 통과한다. 그 뒤 발급된 `mhk_`가 그 주소로 나간다(RV-004가 막으려던 경로). 같은 함수 안에서도 텔레메트리는 파일 값만 보고 엔드포인트는 기본값을 섞어, 판정 기준이 서로 다르다 | 연동 상태용 판정은 **파일 실물만** 쓴다. 엔드포인트가 없으면 `targetsHub=false`로 둔다(blocker 문구 "수집 엔드포인트가 설정되지 않음"). exporter 2개가 `otlp`이고 protocol이 있는지도 `collectionReady`에 넣는다. 범용 `OTEL_EXPORTER_OTLP_ENDPOINT`는 읽기 전용으로 읽어 신호별 값이 없을 때의 유효 엔드포인트로 평가한다. 설정 화면의 `ingestEndpoint`(폼 기준)는 그대로 둔다. 비용: 낮음~중간. 근거: `values` 맵이 이미 `read_ingest_inputs`에 있고 `assess_endpoints`는 문자열 목록만 받는다. 범용 키 읽기만 `read_managed_values` 밖에서 하나 추가하면 된다. 실 영향: 폼으로 저장한 파일에는 모든 관리 키가 기록되므로(`hardcoded_defaults` + 엔드포인트), 손으로 고친 파일에서만 나타난다. 선검사는 dead code라 지금 영향이 없다. 그래서 Minor다 | `read_ingest_inputs`와 `assess_effective_endpoints`, `build_defaults`·`hardcoded_defaults`(`otel_settings.rs:64-96`), `MANAGED_OTEL_KEYS`(`:18-36`)를 나란히 읽었다. `OTEL_EXPORTER_OTLP_ENDPOINT`를 src 전체에서 grep했고 0건이었다. Claude Code 공식 문서와 대조했다 | 정확성 / 보안(미래 경로) |
| N-5 | ⚪ Nit | `src-tauri/src/otel_link.rs:312-336`, 테스트 주석 `:503-505` | 자동 발급 경로는 키만 주입한다(`apply_ingest_key`, `values: {}`). 텔레메트리가 꺼진 파일에서도 발급을 멈추지 않는다. 그래서 계약이 붙으면 "발급 성공 → 홈에 `!` 수집 불가 경고"로 끝나는 흐름이 기본이 된다. 테스트가 이것을 정상으로 고정한다. 결함은 아니다(경고가 정직하다). 하지만 "설치 후 OTEL 설정 한 번"이라는 목표와는 한 단계 어긋난다 | 허브 계약 WBS에 설계 결정 항목으로 넣는다. 결정할 것: 발급 시 텔레메트리·exporter·허브 엔드포인트를 함께 쓸지, 아니면 준비 안 됨이면 발급 전에 멈출지 | 테스트 `issued_key_flow_*`의 새 단언과 `issue_and_apply` 분기를 대조 | 기획의도 |
| N-6 | ⚪ Nit | `src/onboarding.ts:171` | blocker 문구 "텔레메트리가 꺼져 있음"은 관측보다 강한 단정이다. 앱이 실제로 아는 것은 "settings.json에 `CLAUDE_CODE_ENABLE_TELEMETRY=1`이 없음"뿐이다. 셸 환경변수로 켠 사용자는 틀린 문장을 보게 된다. 주석(`otelApi.ts:119-120`)은 이 한계를 적어 두었지만 화면 문구는 그렇지 않다. 앞에 "수집되지 않을 수 있습니다"라는 완충이 있어 Nit로 둔다 | "설정 파일에서 텔레메트리가 켜져 있지 않음"으로 바꾼다. 하네스 `mustInclude`도 같이 바꾼다 | 문구와 `read_ingest_inputs`의 판정을 대조, PNG 05 열람 | 정확성 |

## 페르소나별 관점

### 보안 — `persona-credential-at-rest-and-transit`
- RV-004 선검사가 올바른 위치에 있다. 헤더 선검사 다음, `issuer.issue()` 앞이다. 그래서 고아 키가 생기지 않고, 파일과 백업도 바뀌지 않는다. 테스트가 둘 다 확인한다. 루프백 http(`localhost:4318`)도 허브가 아니라서 막는다. 의도로 보면 엄격한 쪽이고, 이것이 맞다
- `failed_from`이 실패 시에도 기존 masked와 준비도를 보존한다. 원문 필드는 여전히 없다
- 남은 빈틈은 선검사의 입력이다. 파일에 없는 엔드포인트를 앱 기본값(허브)으로 간주하고, 범용 엔드포인트를 보지 않는다 → RV-005

### 마스킹 — `persona-displayed-text-redaction-vs-diagnostic-value`
- 새 필드 3개는 불리언과 열거형뿐이다. URL·호스트·키가 상태 JSON과 Debug에 없다는 것을 9개 케이스 테이블 테스트가 직접 확인한다
- 한 줄 노출에도 하네스 `checkNoSecrets`가 DOM 전체(outerHTML + innerText)를 검사한다. 한 줄에는 masked 값을 싣지 않는다(PNG 06). 불필요한 노출이 줄었다
- 이 관점의 신규 지적은 없다

### 주장 vs 관측 — `persona-claimed-vs-verified`
- RV-002·RV-003 문구가 해소됐다. "새로 시작하는 … 적용됩니다"는 이제 `collectionReady`일 때만 나온다. 하네스 `mustExclude`가 warn 상태에서 이 문구를 막는다
- 금지어가 늘어 회귀를 막는 그물이 넓어졌다
- 그러나 `collectionReady=true`라는 주장 자체가 관측보다 넓다. 근거는 파일에 없는 앱 기본값, 그리고 exporter·protocol 미확인이다 → RV-005. "텔레메트리가 꺼져 있음"도 단정이 지나치다 → N-6
- 머리말의 "바뀌는 곳은 구현체뿐"은 그대로다. RT-1(이월)이 남아 있어 여전히 다소 과장이다. 이월 범위이므로 재지적하지 않는다

### 첫 실행 신입 — `persona-first-run-newcomer`
- 완주를 시뮬레이션했다. 필수 완료 + 미적용이면 홈 상단에 한 줄이 나온다. "수집키 입력하기"를 누르면 `#/settings/otel`로 간다(하네스가 클릭 이동을 단언). 저장 후 준비 완료가 되면 한 줄이 사라진다(PNG 07). 막다른 골목이 없다
- 실패나 경고일 때의 한 줄은 일반 문구 "연동에 문제가 있습니다. 설정 > OTel에서 확인하세요"다. 구체 원인은 설정 화면에서 봐야 하지만 버튼으로 바로 이어지므로 수용한다
- 체크리스트 회귀는 없다. 필수 미완료 사용자의 박스 구성(01·03·05)은 1차와 같다. 박스 카운트도 "2/3 완료" 그대로다(advisory는 진행률에서 빠진다)

### 발산형 — `persona-zero-base-redesigner`
- 🔵 RT-1·RT-2는 의도적으로 이월됐다. RV-001 임시책 덕분에 RT-2의 시급성이 낮아졌다. 이제 상시 위젯 이동은 UX 정리 차원의 일이다
- 새로 보이는 구조 쟁점은 하나다. 자동 발급이 키 주입만 맡으면 연동 완성은 늘 수동 설정에 기대게 된다(N-5). "발급"과 "수집 설정 완성"을 한 동작으로 묶을지가 허브 계약 착수 때의 첫 설계 결정이다. 대안 구조는 이렇다: `issue_and_apply`가 `IngestKeyChange` + 허브 엔드포인트·exporter·텔레메트리 값을 한 번의 `save_to_settings_file` 호출로 쓴다(같은 엔진이고, 같은 원자적 쓰기·백업이다). 그러면 RV-005의 기본값 문제도 그 경로에서는 저절로 사라진다

## 기각된 지적

| ID | 원래 주장 | 기각 사유 |
|---|---|---|
| K-6 | `confirmed` + `collectionReady=false`이면 "수신 확인됨"과 "수집되지 않을 수 있음"이 모순된다 | 지금은 `confirmed`에 도달할 수 없다(수신확인 구현체 없음). 셸 환경변수로 켠 경우처럼 실제로 생길 수 있는 조합이기도 하다. 계약 착수 시 문구를 정하면 되므로, 지금 결함은 아니다 |
| K-7 | RV-001 한 줄이 필수 단계가 loading/error일 때(기존 slim 줄)는 OTEL을 보여 주지 않는다 | 그 상태는 `allDone=false`인 과도 상태다. 확정되면 박스나 OTEL 한 줄 중 하나로 넘어간다. 실물(`home.ts:182` 이후 분기)을 읽어 확인했다. 노출 누락이 계속 남는 경로가 아니다 |
| K-8 | 테스트의 `"/v1/"`·`https://` 문자열이 허브 호출 추가다 | OTLP 엔드포인트 설정값 픽스처다(위임 메시지와도 일치). 제품 코드에 HTTP 클라이언트·호출이 없다는 것을 grep으로 확인했다 |

## 잘 된 점
- **상태와 준비도를 직교시켰다.** "적용됐지만 수집 안 됨"을 새 상태값이 아니라 플래그로 둔 근거(유니온 곱 폭증, `applied` 의미 보존)를 주석에 남겼다. 기존 분기를 건드리지 않고 경고만 얹었다
- **반대 방향 테스트가 있다.** "허브 밖이면 멈춘다"뿐 아니라 "허브 https면 막지 않는다"(`issuer.calls()==1`)도 고정했다. 과잉 차단 회귀를 막는다
- **실패해도 사실을 잃지 않는다.** `failed_from`이 masked와 준비도를 보존한다. 테스트 `failed_status_keeps_the_readiness_read_from_the_file`이 이를 고정한다
- **RV-001 해법이 저비용이다.** 이미 있는 `.onboard-slim` 마크업과 CSS를 재사용했다. 박스 재노출 금지를 Critical 단언으로 지켰다. "준비 완료면 아무것도 안 그림"과 "조회 실패면 아무것도 안 그림"을 각각 시나리오로 분리했다
- **금지어 그물을 넓히고 Rust에도 같은 검사를 넣었다.** 문구 정본(`message()`)에서 회귀를 막는다

## PM 권고
1. 판정은 **Green**이다. 직전 Major·Minor는 모두 해소됐다. 남은 것은 Minor 1개(RV-005)와 Nit 2개다. 이 범위로 진행해도 된다
2. RV-005는 지금 영향이 작다(손으로 고친 파일 + dead code 선검사). 그래도 RV-004가 막으려던 키 유출 경로의 변형이다. 그래서 둘 중 하나를 권한다. (a) 이번 라운드에 연동 상태용 판정을 "파일 실물만 + exporter·protocol + 범용 엔드포인트"로 고친다. (b) 최소한 `otel_link.rs` 머리말과 허브 계약 착수 WBS에 "선검사·준비도는 앱 기본값을 쓰지 말 것, 범용 OTEL_EXPORTER_OTLP_ENDPOINT 평가 필요"를 선행 항목으로 남긴다
3. N-5(발급과 수집 설정 완성을 묶을지), RT-1(confirmed 저장 위치)은 허브 계약 착수 WBS의 설계 결정 항목으로 둔다. N-6은 문구 한 줄이라 RV-005와 같이 처리하면 된다
4. 필수 완료 + failed/warn 한 줄은 PNG가 없고 단언만 있다. 다음에 하네스를 돌릴 때 캡처를 추가하면 화면 근거가 완결된다
5. RV-005를 (a)로 고치면 재검토는 같은 target_id로 위임한다. 바뀌는 곳이 판정 함수와 테스트뿐이면 축소 모드 대상이다
