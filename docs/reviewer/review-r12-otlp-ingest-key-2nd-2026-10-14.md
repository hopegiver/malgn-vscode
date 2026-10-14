# R12 2차 재검토: 허브 OTLP 수집키 입력·저장·삭제

리뷰 대상: target_id `r12-otlp-ingest-key` / 누적 범위 `5ccc00b..640352b` (반영분 `889919f..640352b`; `d6ec05c`는 1차 보고서 커밋이라 코드 판정 대상 아님)
- 등급: Sensitive, 2차 재검토. 패널은 1차와 같은 6인(보안·발산형 포함)이다. 위임 메시지가 "축소 금지"를 명시해 축소/증분 모드 판정을 하지 않고 풀패널로 처리했다
- 리스크 범주: 크리덴셜 저장·전송(인증)
- 직전 리뷰: `docs/reviewer/review-r12-otlp-ingest-key-2026-10-14.md` (Amber, Major RV-001 + Minor RV-002~005)
- 일자: 2026-10-14 / 리뷰어: reviewer
- 실행 액션: 없음. 코드·허브 수정, 커밋, push, 네트워크 쓰기, stash/checkout/reset을 하지 않았다. 이 보고서를 쓰고, 페르소나 6개의 "적용 이력"과 INDEX.md "최근 재사용"에 한 줄씩 덧붙인 것이 전부다(커밋은 PM 몫)

## 종합 판정: 🟢 Green (조건 없음)

Critical 0 / Major 0 / Minor 1(신규) / Nit 2(신규) + 1차 이월 Nit 6 / Rethink 1(신규) + 1차 이월 2

1차 지적 5건(RV-001~005)은 모두 해소됐다. 파일·라인과 테스트·하네스·스크린샷으로 하나씩 확인했다. 반영하면서 새로 생긴 유출 경로는 찾지 못했다. `assess_endpoints`는 불리언 3개만 내보내고, Clear 백업 실패 경로는 원문 없이 멈추며, 토스트와 확인 문구는 모두 원문이 없는 상수다.
새 지적 중 가장 무거운 것은 RV-006(Minor)이다. 문구가 "허브 관리자에게 수집키 **발급**을 요청"하라고 하지만, 허브 관리자에게는 **다른 직원 명의(user 주체)로** 수집키를 발급하는 경로가 없다. 다만 기기 토큰으로 유도하던 1차의 막다른 골목과 고권한 토큰 생성 위험은 사라졌다. 파일럿 규모(PC 2~3대, 관리자 = 개발자 본인)에서는 관리자가 직접 안내하면 완주할 수 있다. 그래서 Green을 막지는 않는다. 이 문구는 1차 보고서의 권고 (a)를 그대로 따른 결과라, 1차 리뷰가 허브 발급 주체를 끝까지 확인하지 않은 탓이 크다(아래 RV-006 참조).

## 페르소나 재사용 판정

착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 다시 대조했다. 1차 6인의 역할개념이 이번 반영 범위(문구·엔드포인트 평가·백업 처리·스캔 패턴)를 그대로 덮는다. 새 리스크 표면은 `assess_endpoints`의 응답 표면 하나인데, `credential-at-rest-and-transit`의 "키가 실려 나가는 목적지"와 `displayed-text-redaction`의 "응답·화면에 원문 없음"이 이미 다룬다. 그래서 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-credential-at-rest-and-transit.md` (보안) | 재사용 | 1차에서 신규로 만든 행이다. 백업 키 제거(RV-004)와 엔드포인트 목적지 경고(RV-002)가 이 역할개념의 C4·C5 그대로다 |
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | 새 응답 필드(`ingestEndpoint`)와 새 토스트·확인 문구에 원문이 없는지 보는 일이 역할개념 그대로다 |
| `persona-design-contract-auditor.md` | 재사용 | 앱 문구를 허브 정본(`keys.vue`·`ingest-credentials.js`·설계 §5.6)과 조항 단위로 대조하는 일이 역할개념 그대로다 |
| `persona-first-run-newcomer.md` | 재사용 | 바뀐 안내를 따라 실제로 키를 얻어 완주하는지 다시 시뮬레이션하는 일이 역할개념 그대로다 |
| `persona-claimed-vs-verified.md` | 재사용 | 새 주석·하네스 단언·테스트가 주장하는 범위와 실제 범위를 대조하는 일이 역할개념 그대로다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수이고 역할개념이 여전히 유효하다 |

## 직접 확인한 것

- `git diff 889919f..640352b` 전체(코드 9개 파일과 스크린샷 10장)를 읽었다. `src/views/settings.ts`는 200-470행을 문맥까지 읽었다. `otel_settings.rs`는 `MANAGED_OTEL_KEYS`(18-37)·`build_defaults`(83-96)·저장 경로를, `hub_auth/consts.rs:58-118`(`hub_origin`·정규화 테스트)을, `scripts/pre-push-sensitive-scan.mjs` 전문을 읽었다
- `cargo test --manifest-path src-tauri/Cargo.toml`: **777 passed / 0 failed / 21 ignored**. 1차는 768건이었다. 차이 +9는 assess 테스트 6건 + otel_settings 테스트 4건 − 삭제된 issue_url 테스트 1건과 맞는다
- `pnpm build`: 성공
- `node scripts/pre-push-sensitive-scan.mjs`: exit 0. `git grep -nE 'mh[kd]_[A-Za-z0-9_-]{43}' 640352b`로는 1건(`hub_auth/store.rs:283` TEST_TOKEN)이 나온다. 이 리터럴은 직전 줄 pragma가 표시하는 바로 그 값이다
- 스크린샷 4장을 직접 열었다(`docs/screenshots/r12-ingest-key/otelIngestKey/`). `unset-telemetry-on-01-01-unset-warning.png`(허브 거부 경고 + 관리자 발급 요청 문구, 발급 버튼 없음), `set-insecure-http-endpoint-01-01-insecure-warning.png`(평문 경고와 허브 밖 경고 2개, 허브 거부 경고 없음), `unset-internal-collector-no-hub-warning-01-01-internal-collector-no-warning.png`(경고 0개), `clear-flow-01-01-clear-confirm.png`(백업 포함·허브 미폐기·새 세션 안내). 네 장은 서로 다른 상태를 보여준다
- 허브(읽기 전용, `malgnai-public` HEAD `2768bf2`)에서 본 것: `app/pages/keys.vue`에서 `ingest|mhk|수집`이 여전히 0건이고 기기 인증키 발급·폐기만 있다(5·15·66·90행). `app/` 전체에서 `ingest-credentials` 호출 0건. `server/api/ingest-credentials.js`의 본인 발급 `POST /`(90-104, `subjectType:'user'`, `userId: c.get('userId')`), 관리자 라우터(178-205: 목록 GET + **공용 워크스테이션 주체 발급만**), 즉시 폐기 `DELETE /:id`(156-174, 소유자 **또는 administrator**). `server/index.js:163-164` 마운트. 설계 `docs/design/otlp-ingest.md` §5.6(452·458·461행)

## 1차 지적 해소 여부

| ID | 1차 심각도 | 상태 | 근거(파일:라인) | 확인방법 |
|---|---|---|---|---|
| RV-001 | 🟠 Major | **해소** (잔여 문구 정확성은 RV-006으로 분리) | `src/views/settings.ts:100-125` `INGEST_KEY_COPY` 상수로 문구를 모았다. 발급 버튼과 `openUrl` import·`handleOpenIngestKeyIssuePage`는 삭제했고, `:446` `actions`는 이제 삭제 버튼만 담는다. `:107`(issue)은 "직접 발급할 수 없음 + 관리자 요청 + mhd_는 수집키 아님"을, `:112`(revoke)는 "관리자에게 즉시 폐기 요청"을 말한다. Rust `otel_ingest_key.rs:84-93`은 `issue_url` 필드를 지우고 사유를 주석으로 남겼고, `otel_settings.rs`의 `ingest_key_issue_url`도 지웠다. 테스트 `status_serialization_has_no_raw_key`는 이제 `!json.contains("issueUrl")`를 단언한다. 하네스 `otelIngestKey.mjs:103-115`는 "발급" 버튼 0개, 블록 안 `a[href]` 0개, `/keys` 문자열 없음을 단언한다 | diff 정독, 스크린샷 2장, 허브 `keys.vue` grep 재실행 |
| RV-002 | 🟡 Minor | **해소** | `otel_ingest_key.rs:120-150` `assess_endpoints`는 origin을 `hub_origin()`과 정확히 비교한다(`consts.rs:114-118` 테스트가 정규화된 형태를 보장한다). 같은 함수가 http를 판정하고(루프백은 제외), 해석할 수 없는 값은 foreign으로 친다. `otel_settings.rs:127-141`은 METRICS/LOGS 엔드포인트를 파일 값 → 기본값 순으로 본다. `settings.ts:416-427`에서 "허브가 거부" 경고는 `targetsHub`일 때만, 평문·허브 밖 경고는 `state==='set'`일 때만 뜬다. 저장은 막지 않는다(1차 권고대로). 테스트 6건과 `status_reports_endpoint_assessment`가 있고, 하네스 시나리오 3개(사내 collector 무경고, http 경고, 허브 https 무경고)가 있다 | 코드·테스트 정독, 스크린샷 2장 |
| RV-003 | 🟡 Minor | **해소** | `settings.ts:111`(scope, 힌트 `#otel-ingest-key-scope-hint` `:463`), `:116`(savedToast, `:153`), `:117`(clearedToast, `:175`), `:114`(clearConfirm)에 모두 "새로 시작하는 Claude Code 세션부터 적용"이 있다. "이전 설정을 계속 쓸 **수 있습니다**"는 유보형이다. 기존 세션의 동작은 여전히 **실측하지 않은 추정**이므로 이 유보형이 맞다. 하네스 `:117`·`:242`가 단언한다 | 코드·하네스 정독 |
| RV-004 | 🟡 Minor | **해소** | `otel_settings.rs:479-489`: Clear일 때만 `backup_without_ingest_key`(`:360-367`)로 백업을 만든다. 이 함수는 본 저장과 같은 `HeadersEdit`를 `env`에만 적용하므로, Authorization 외 헤더와 나머지 설정은 보존된다. `apply_headers_edit`(`:344-354`)로 본 파일과 백업이 한 규칙을 쓴다. 확인 문구 `settings.ts:114`는 "(백업 포함)…허브에 등록된 키는 폐기되지 않으므로…관리자에게 폐기를 요청"이라고 말한다. 테스트: `clear_key_leaves_no_key_in_backup`(`:1051`, 직전 키와 그 이전 백업의 키 모두 사라짐, `x-team` 보존, 0600), `clear_key_backup_drops_headers_entry_when_only_key`, 대조군 `keep_save_backup_is_verbatim_original` | 코드·테스트 정독, 스크린샷 |
| RV-005 | 🟡 Minor | **해소** | `scripts/pre-push-sensitive-scan.mjs:25-30`: `mhk_`와 `mhd_` 47자 패턴을 둘 다 넣었다. 허브 `tokens.js`·`otlp-limits.js`와 형식이 같다는 주석도 달렸다. 기존 리터럴 하나(`hub_auth/store.rs:283`)에는 `:282`에 pragma를 붙였다. 스캔 실행 exit 0 | 스캔 실행, git grep |

## 반영 과정의 회귀·새 유출 경로 점검

- **`assess_endpoints`의 응답 표면**: `EndpointAssessment`는 `bool` 3개만 갖는다(`otel_ingest_key.rs:106-115`). `assessment_serializes_flags_only`(`:681`)가 호스트 문자열이 없음을 단언한다. 엔드포인트 URL은 원래 `values`/`defaults`로 이미 프론트에 나가고 있어서, 이 필드가 새로 노출하는 것은 없다. 헤더 원문은 이 함수에 들어가지도 않는다(입력은 METRICS/LOGS 엔드포인트 문자열 두 개뿐이다, `otel_settings.rs:133-141`)
- **Clear 백업의 실패 경로**: `backup_without_ingest_key`가 실패하면 `?`로 저장 전체가 멈춘다. 본 파일과 백업 둘 다 쓰지 않는다(백업 쓰기가 본 파일 쓰기보다 먼저다). 원문 백업으로 조용히 넘어가는 폴백은 없다. Err 문구 `{e}`는 serde_json 에러라 위치만 담고 원문을 담지 않는다(1차 기각 근거와 같다). 실패할 때 이전 세대 백업(옛 키를 담고 있을 수 있음)은 그대로 남는다. 그러나 본 파일도 바뀌지 않았으므로 "삭제했다"는 토스트는 나오지 않고 실패 토스트가 뜬다(`settings.ts:177`). 상태와 화면이 서로 맞는다. 입력은 바로 앞에서 파싱에 성공한 같은 `existing_content`라서, 이 경로는 사실상 도달하지 않는다
- **Clear 이후 첫 Keep 저장**: 이때 백업은 "직전 원본"인데, 그 원본이 이미 키 없는 파일이다. 따라서 키가 백업에 다시 나타나지 않는다(`keep_save_backup_is_verbatim_original`의 구조와 같다)
- **회전(Set) 저장**: 이때 백업은 옛 키를 담는다(`otel_settings.rs:1043`, "교체 저장의 백업은 직전 원본"). 롤백 가치를 지키려는 의도된 선택이다. 유출 때문에 회전하는 경우에는 허브 쪽 폐기가 별도로 필요하고, revoke 문구가 이를 안내한다. 문제로 보지 않는다
- **토스트·확인 문구**: 모두 `INGEST_KEY_COPY` 상수다(`settings.ts:106-125`). 값을 끼워 넣지 않는다. 실패 토스트는 1차와 같은 Rust 고정 문구 경로를 쓴다
- **원문 로그 grep 재실행**: 반영 diff에 `console.`·`eprintln`·`println`·`{:?}`가 0건이다(diff 정독)

## 지적 사항 (2차 신규)

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 |
|---|---|---|---|---|---|
| RV-006 | 🟡 Minor | `src/views/settings.ts:107-108`(issue), `:118-119`(warnUnsetHub) / 허브 `server/api/ingest-credentials.js:90-104`, `:178-205`, 설계 §5.6 458행 | **"허브 관리자에게 수집키 발급을 요청"이 허브 권한 모델과 어긋난다.** 개인(user 주체) 수집키는 본인 JWT로만 발급된다(`POST /api/ingest-credentials`, `userId: c.get('userId')`, 설계 표 "로그인 직원(본인 주체만)"). 관리자 라우터에는 **공용 워크스테이션 주체 발급**만 있다. 따라서 요청을 받은 관리자가 할 수 있는 일은 두 가지다. (1) 사용자 본인이 허브에 로그인한 상태에서 API를 직접 호출하도록 돕는다. (2) 공용 워크스테이션 키를 대신 발급한다. (2)를 택하면 사용량이 그 직원이 아니라 워크스테이션에 귀속된다(설계 L7 "공용 키 개인 PC 복사로 귀속 회피"와 같은 모양). 폐기 쪽("관리자에게 즉시 폐기 요청")은 `DELETE /:id`가 administrator를 허용하므로(160행) **사실이다**. 1차 권고 (a)의 예시 문구를 그대로 반영한 결과이므로, 원인은 1차 리뷰가 발급 주체를 확인하지 않은 데 있다 | 문구를 "허브 관리자에게 수집키 발급 방법을 문의하세요"로 바꾼다. 발급 행위의 주체를 관리자로 단정하지 않는 표현이다. 가능하면 관리자용 절차(본인 로그인 상태에서 `POST /api/ingest-credentials` 호출, 공용 키를 개인 PC에 쓰지 말 것)를 허브 쪽 runbook에 한 줄 둔다. 하네스 `otelIngestKey.mjs:110`의 정규식(`관리자에게 수집키 발급을 요청`)도 함께 고친다 | 허브 라우트·마운트(`server/index.js:163-164`)·설계 §5.6 원문 대조 |
| N-7 | ⚪ Nit | `src-tauri/src/otel_settings.rs:127-128`(주석), `:133-141` | 주석의 "Claude Code가 실제로 쓰는 값과 같은 우선순위"는 과대 주장이다. 파일에 METRICS/LOGS 엔드포인트가 없으면 Claude Code는 앱 기본값이 아니라 파일의 범용 `OTEL_EXPORTER_OTLP_ENDPOINT`(관리 대상 밖)나 SDK 기본값을 쓴다. 그런데 평가는 앱 기본값(허브)을 기준으로 해서 평문·허브 밖 경고를 놓칠 수 있다. 저장 1회 또는 자동 설정이 두 키를 파일에 기록하므로(`settings.ts:223`, `:296`), 이런 상태는 손으로 편집한 파일에서만 생긴다. 또 traces 엔드포인트(베타 기능을 켤 때)로도 헤더가 나가는지는 **확인 필요**다. 한편 `state==='unrecognized'`(타 서비스 Bearer)가 http로 나가는 경우는 경고하지 않는다 | 주석을 "앱이 저장하는 값 기준(파일에 신호별 엔드포인트가 없으면 실제와 다를 수 있음)"으로 정정한다. 필요하면 파일의 범용 엔드포인트도 평가 입력에 넣는다 | 코드 정독, OTel 신호별 우선순위 규칙(신호별 > 범용) |
| N-8 | ⚪ Nit | `src-tauri/src/hub_auth/store.rs:282`, `scripts/pre-push-sensitive-scan.mjs` allowlist 판정부(`nearby` = 추가된 줄 기준 직전 2줄) | 스캐너는 **추가된 줄끼리만** 2줄 창을 본다. 따라서 이 pragma는 TEST_TOKEN 줄과 같이 추가될 때만 효과가 있다. 나중에 TEST_TOKEN 줄만 수정하면 pragma 줄은 diff에 없어서 push가 막힌다(안전 쪽 실패라 피해는 없다). 같은 창이 파일 경계를 넘어, 다른 파일의 추가 줄 pragma가 다음 파일 첫 줄의 매치를 덮을 수도 있다(기존 동작이고 이번 범위 밖) | pragma를 리터럴과 같은 줄의 트레일링 주석으로 옮기면 어느 편집에서도 함께 diff에 실린다. 또는 TEST_TOKEN을 `repeat()`로 조립한다 | 스캐너 코드 정독 |

### 1차 이월(미반영, PM 판단으로 범위 밖)

N-1(하네스 `assertNoRawKey` 주석), N-2(Debug derive 주석), N-3(private 쓰기 실패 시 임시 파일 잔여), N-4(첫 Authorization·퍼센트 인코딩), N-5(허브 NOTICE 사본 + export 주기 불일치), N-6(Windows ACL), RT-1, RT-2. 상태는 1차와 같다. N-5에는 한 가지가 덧붙는다. 앱 revoke 문구(`settings.ts:112-113`)가 이제 허브 `NOTICE`("즉시 폐기를 누르세요")와 **의도적으로 다르다**. 그래서 허브 수집키 UI가 생길 때 되돌려야 할 지점이 `INGEST_KEY_COPY` 한 곳으로 모였고, 주석(`:100-105`)이 그 복귀 절차를 적어 두었다. 이건 개선이다.

## 기각된 지적

| 후보 | 기각 사유 |
|---|---|
| "`ingestEndpoint` 응답이 엔드포인트 호스트를 새로 노출한다" | 필드는 불리언 3개뿐이다(`otel_ingest_key.rs:106-115`, 직렬화 테스트가 호스트 부재를 단언). 엔드포인트 URL은 원래 `values`/`defaults`로 이미 프론트에 있었고, 자격증명도 아니다 |
| "Clear 백업 생성이 실패하면 원문 백업으로 폴백해 키가 남는다" | `?`로 저장 전체를 중단하고, 원문으로 폴백하는 분기가 없다(`otel_settings.rs:356-367`, 주석이 의도를 명시). 입력이 직전에 파싱에 성공한 같은 문자열이라 사실상 도달하지도 않는다 |
| "origin 비교가 대소문자·기본 포트 차이로 허브를 foreign으로 오판한다" | `url::Url`의 `origin().ascii_serialization()`은 호스트를 소문자로 바꾸고 기본 포트를 생략한다. `hub_origin()` 상수도 같은 정규화 형태임을 `consts.rs:114-118` 테스트가 고정한다. 디버그 빌드의 `MALGN_HUB_ORIGIN` 오버라이드가 비정규 값이면 오판할 수 있지만, 그건 개발 환경뿐이다 |
| "발급 버튼을 지워 미설정 사용자에게 다음 행동이 없다(막다른 골목)" | 다음 행동은 문구로 제시된다(관리자 요청). 정확성 문제는 RV-006으로 따로 다뤘다. 존재하지 않는 셀프서비스로 보내는 것보다 버튼이 없는 편이 맞다 |
| "평문 경고가 저장 전(입력 중)에 뜨지 않는다" | 경고는 저장된 상태를 기준으로 하고, 저장 후 재조회에서 뜬다. 1차 권고도 "경고(저장 거부 아님)"였고, 45인 사내 도구 기준으로는 저장 후 경고면 충분하다. 결함이 아니라 설계 선택이다 |

## 페르소나별 관점

### credential-at-rest-and-transit (보안, 수렴, 재사용)
- C4(전송 목적지)가 미충족에서 충족으로 바뀌었다. 평문(http, 루프백 제외)과 허브 밖 목적지를 키가 설정된 상태에서만 경고한다. 사내 collector 사용자는 막지 않는다. 남은 틈은 손으로 편집한 범용 엔드포인트(N-7) 하나다.
- C5(수명·폐기)가 부분 충족에서 충족으로 바뀌었다. Clear는 백업에서도 키를 지우고, 이전 세대 백업의 키까지 덮는 것을 테스트가 실측한다. 로컬 삭제가 허브 폐기가 아님을 확인 문구와 토스트가 둘 다 말한다. 관리자 폐기 경로는 허브 `DELETE /:id`가 administrator를 허용하므로 실재한다.
- 1차 "PM 결정 평가"의 성립 조건(폐기 수단의 실재)도 충족됐다. 셀프서비스는 아니지만 관리자 경유 폐기가 코드로 존재한다.

### displayed-text-redaction-vs-diagnostic-value (수렴, 재사용)
- 새 응답 필드, 새 토스트 2개, 새 확인 문구, 새 경고 4종 어디에도 원문이나 URL을 끼워 넣지 않는다. 마스킹 표시(`mhk_AAAA…`)는 1차와 같다(스크린샷 insecure 확인).

### design-contract-auditor (수렴, 재사용)
- 앱 문구를 허브 현실과 다시 대조했다. "웹에서 직접 발급할 수 없음"은 사실이다(`keys.vue`·`app/` 0건). "mhd_는 수집키 아님"도 사실이다(`tokens.js`·`otlp-limits.js` 접두사가 다르다). "관리자에게 폐기 요청"도 사실이다(administrator 허용). "관리자에게 발급 요청"은 **어긋난다**(RV-006). 관리자 발급은 공용 워크스테이션 주체로만 가능하다(설계 §5.6 452행).

### first-run-newcomer (수렴, 재사용)
- 다시 완주를 시뮬레이션했다. 미설정 경고 → "허브 관리자에게 수집키 발급을 요청" → 관리자에게 연락 → (관리자가 본인 로그인 상태에서 API 호출을 안내하거나 대신 처리) → `mhk_` 붙여넣기 → 저장 → 마스킹 표시 + "새 세션부터" 토스트. 1차의 막다른 골목(기기 토큰 화면, 형식 오류)은 사라졌다. 관리자 쪽 절차가 문서로 없다는 점만 남는다(RV-006 개선안).

### claimed-vs-verified (수렴, 재사용)
- 커밋 메시지와 주석이 주장하는 것(URL 미포함, 백업 키 제거, 엔드포인트별 경고)을 테스트와 하네스 단언으로 하나씩 대조했고 모두 실제 단언이 있다. 777건 통과를 재실행으로 확인했다.
- 과대 주장 두 곳을 찾았다. N-7(otel_settings.rs의 "Claude Code가 실제로 쓰는 값과 같은 우선순위")과 N-8(pragma 효과 범위)이다.

### zero-base-redesigner (발산, 재사용)

| ID | 대안 구조 | 얻는 것 | 잃는 것/비용 |
|---|---|---|---|
| RT-3 🔵 | **"관리자에게 요청"을 사람 손에 두지 말고, 허브 설계에 이미 있는 숨김 플래그 `/keys` 탭(설계 P0 행 "발급 API·/keys 탭(숨김 플래그)")을 파일럿 사용자에게만 켠다.** 개인 키 발급은 본인 JWT가 전제라서, 관리자가 대신 발급하는 경로를 새로 만드는 것보다 이미 설계된 본인 셀프서비스 UI를 여는 편이 권한 모델과 맞는다. 앱은 플래그가 켜진 사용자에게만 `issueUrl`을 다시 실으면 된다. 복귀 지점은 `INGEST_KEY_COPY` 주석에 이미 적혀 있다 | RV-006이 구조적으로 사라진다. 사용량이 그 직원 명의로 귀속된다. 관리자 수작업이 없어진다 | 허브 UI 작업이 필요하다(교차 저장소). 앱은 플래그 상태를 알아야 한다(허브 응답 필드 하나). 1차 RT-1(기기 토큰 자동 발급)과는 경쟁 관계다. 이 안은 기존 설계에 맞고, RT-1은 UX가 더 좋다 |

## 트레이드오프

- RV-006 문구는 정확성과 간결성 사이에서 고른다. "발급 방법을 문의"는 정확하지만 덜 구체적이다. 관리자 runbook이 함께 있어야 구체성이 회복된다. 파일럿 기간(관리자 = 개발자 본인)에는 문구만 고쳐도 충분하다고 판단한다.
- 회전 저장에서 백업에 옛 키를 남기는 것(롤백 가치)과 Clear에서 백업까지 지우는 것(삭제 의도 존중)은 저장 종류에 따라 달리 처리했다. 이 분리가 타당하다.

## 잘 된 점

- 문구를 `INGEST_KEY_COPY` 한 객체로 모으고, 허브 UI가 생길 때 무엇을 되돌릴지(Rust `IngestKeyStatus`에 URL 재추가, 버튼 복원)를 주석에 적었다(`settings.ts:100-105`, `otel_ingest_key.rs:85-87`). 임시 조치가 영구 조치로 굳지 않게 하는 장치다.
- `EndpointAssessment`가 URL 대신 판단 결과만 내려보낸다. 직렬화 테스트가 호스트 부재까지 단언한다. 1차가 칭찬한 `SecretString` 타입 강제와 같은 결의 설계다.
- `apply_headers_edit`로 본 파일과 백업이 같은 변경 규칙을 공유한다. 그래서 백업만 다르게 지워지는 드리프트가 생길 수 없다. 이전 세대 백업의 키까지 덮이는지를 일부러 회전 후 Clear로 재현해 테스트했다.
- 하네스가 문구 존재(긍정)뿐 아니라 `/keys`·"발급" 버튼·외부 링크의 **부재**(부정)까지 단언한다. 회귀로 버튼이 되살아나면 바로 잡힌다.
- 스캔 패턴에 `mhk_`와 함께 1차 선택안이던 `mhd_`도 넣었고, 기존 리터럴을 실제로 찾아 표시했다.

## PM 권고

1. Green이므로 병합 가능하다. 남은 지적은 병합 조건이 아니다.
2. RV-006은 비용이 문구 2곳과 하네스 정규식 1개다. 병합 전에 같이 고치기를 권하지만, 미루면 백로그로 넘긴다. 허브 쪽 관리자 발급 절차(runbook 한 줄)는 허브 저장소 몫이라 교차 저장소 항목으로 등록한다.
3. RT-3(숨김 플래그 `/keys` 탭 파일럿 개방)을 RT-1과 함께 허브 설계 후보로 올린다. 어느 쪽이든 고르면 RV-006은 저절로 닫힌다.
4. N-7·N-8은 다음에 이 파일을 건드릴 때 같이 처리하면 된다.

## 생략한 것

- 실제 허브로의 OTLP 전송, Claude Code를 실제로 띄운 상태의 재시작·기존 세션 동작(RV-003 문구의 근거), traces 엔드포인트로의 헤더 전송 여부(N-7)는 확인하지 않았다(네트워크 쓰기 금지, 실기동 범위 밖).
- UI 하네스를 다시 실행하지 않았다. 구현자가 산출한 스크린샷 10장 중 4장을 직접 열었고, 나머지(set-masked 3장·unrecognized·clear-after·save-error)는 하네스 단언 코드로만 확인했다.
- Windows 실기(ACL, 백업 처리)는 확인하지 않았다.
- 허브 관리자 화면(`app/pages/admin/usage/index.vue`)은 공용 워크스테이션 레지스트리 호출부만 grep했다. 관리자 UI에 수집키 발급 버튼이 있는지는 grep(`ingest-credentials` 0건)으로 판단했다.
