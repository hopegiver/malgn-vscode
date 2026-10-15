# R16 리뷰: OTEL 수집키 연동 선행 작업 (허브 계약 전)

리뷰 대상: target_id `r16-otel-link-pre-contract` / 커밋 범위 `12ac008..0e01057` (커밋 `0e01057` 한 건)
- 등급: Sensitive. 풀패널(발산형 포함)로 진행했고 축소하지 않았다. 위임 메시지에 축소 근거 3값이 없고 "축소 없음"이 명시돼 있다
- 리스크 범주: 인증/크리덴셜. 허브 기기 토큰 `mhd_`는 키체인에, OTEL 수집키 `mhk_`는 `settings.json`에 평문으로 있다
- 최초 리뷰다(이 target_id의 직전 라운드 없음). 참고한 선행 리뷰는 `docs/reviewer/review-r12-otlp-ingest-key-3rd-2026-10-14.md`(R12 수집키, Green)
- 일자: 2026-10-15 / 리뷰어: reviewer
- 실행 액션: 없음. 코드 수정·커밋·push·네트워크 쓰기를 하지 않았다. 실행한 것은 `cargo test ... otel`, `pnpm test:ts`, `node scripts/pre-push-sensitive-scan.mjs`, 읽기 전용 git/grep이다. 이 보고서를 쓰고, 페르소나 5개의 "적용 이력"과 INDEX.md "최근 재사용"에 한 줄씩 덧붙였다(커밋은 PM 몫)

## 종합 판정: 🟡 Amber

Critical 0 / Major 1 / Minor 3 / Nit 4 / Rethink 2

보안 불변식은 지켜졌다. `mhd_`는 새 모듈의 인자·반환·상태·에러·Debug 어디에도 실리지 않는다. 키가 `settings.json`에 들어가는 길은 수동 입력과 같은 엔진 하나뿐이다. 그 엔진이 `mhd_` 형식을 거부한다. 허브 HTTP 호출, URL, 응답 필드명은 추가되지 않았다. capabilities도 바뀌지 않았다. 테스트로 직접 확인했다(아래 "직접 확인한 것").

Amber인 이유는 보안이 아니라 기획의도 쪽이다(RV-001). 체크리스트의 OTEL 항목은 필수 3단계 중 하나가 남아 있을 때만 보인다. 그런데 실제 사용자 1명은 이미 3단계를 끝냈으니 이 항목을 볼 수 없다. 신규 사용자도 로그인을 마치는 순간 박스와 함께 이 항목이 사라진다. 고치는 비용은 작다. 이미 있는 `.onboard-slim` 한 줄 패턴(`src/views/home.ts:169-177`)을 쓰면 된다.

## 페르소나 재사용 판정

착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 이번 표면(크리덴셜 분리, 화면 문구의 비밀값 마스킹, 미구현 기능 주장 여부, 첫 실행 안내, 구조 재검토)은 기존 역할개념으로 모두 덮인다. 그래서 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-credential-at-rest-and-transit.md` (보안) | 재사용 | `mhd_`/`mhk_` 저장 위치(본 파일·백업·임시파일)와 권한 `0600`, 키가 나가는 목적지를 보는 일이 역할개념 그대로다 |
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | `errorMessage.ts`에 `mhk_` 규칙을 추가했는지, 마스킹 표시값(`mhk_AbCd…`)이 깨지지 않는지 보는 일이다 |
| `persona-claimed-vs-verified.md` | 재사용 | 화면 문구·주석·테스트 이름이 실제 도달 가능한 동작과 맞는지 대조한다(요구사항 3 "미구현 기능 오도 금지"의 본체) |
| `persona-first-run-newcomer.md` | 재사용 | 체크리스트 OTEL 항목을 언제 보는지, 보고 나서 완주할 수 있는지가 역할개념 그대로다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수다. 상태 모델과 체크리스트 배치가 애초에 옳은지 묻는다 |

생략한 관점: `persona-design-contract-auditor`는 투입하지 않았다. 허브 API 계약이 아직 없어 대조할 정본 조항이 없기 때문이다. 요구사항 1~4는 위 5인이 나눠 대조했다.

## 직접 확인한 것

| 확인 | 방법 | 결과 |
|---|---|---|
| Rust 테스트 | `cargo test --manifest-path src-tauri/Cargo.toml otel` | 79 passed / 0 failed (otel_link 신규 테스트 포함) |
| TS 테스트 | `pnpm test:ts` (Node v22.23.2) | 23 passed / 0 failed |
| 허브 호출 미추가 | `git diff 12ac008..0e01057`에서 `reqwest\|ureq\|https?://\|/api/\|StatusCode` grep | 추가 라인 중 일치 2건. 둘 다 `scripts/tests/errorMessage.test.mjs`의 `example.test` 테스트 문자열이다. `otel_link.rs`에는 네트워크 코드가 없다 |
| capabilities | `git diff --stat 12ac008..0e01057 -- src-tauri/capabilities src-tauri/Cargo.toml` | 변경 없음. 새 crate도 없다 |
| 시크릿 리터럴 | diff에서 `(mhd_\|mhk_)[A-Za-z0-9_-]{20,}` grep, `pre-push-sensitive-scan.mjs` | 0건, exit 0. 가짜 값은 모두 `repeat()`로 조립한다 |
| 로그 출력 | `src-tauri/src/otel_*.rs`, `fs_atomic.rs`에서 `eprintln\|println\|log::\|tracing::` grep | 0건 |
| 수동 입력 경로 유지 | `otel_settings.rs` diff | `save_to_settings_file`의 가시성만 `pub(crate)`로 넓혔다. 로직 변경은 없다. `apply_ingest_key`는 `values: {}`로 같은 엔진을 부르고, upsert 루프(`otel_settings.rs:448-454`)는 넘겨받은 키만 만지므로 기존 관리 키를 지우지 않는다 |
| 화면 PNG 4장 | Read로 4장 모두 열어 봄 | 서로 다른 상태다. 01 미연동 `○`+"수집키 입력하기", 02 적용됨 `◐`+`mhk_AbCd…`, 03 실패 `!`+"OTel 설정 열기", 04 조회실패 `?`+"Bearer ***"+"다시 확인". 카운트는 모두 "2/3 완료"다 |
| UI 하네스 재실행 | — | **하지 않았다.** PNG는 PM이 제공한 것이다. `otel-link-unlinked-does-not-reopen-checklist` 시나리오는 스크린샷 없이 단언만 있다 |
| CI 실제 통과 | — | 확인하지 않았다(로컬에서만 실행) |

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 | 축 |
|---|---|---|---|---|---|---|
| RV-001 | 🟠 Major | `src/onboarding.ts:165`, `:220-221` / `src/views/home.ts:165`, `:167-178` | OTEL 항목이 `advisory`라서 `allDone`·`hasTodo` 판정에서 빠진다. 그래서 필수 3단계가 끝나면 박스가 통째로 숨겨지고(`home.ts:165`) 이 항목도 같이 사라진다. 필수 단계가 로딩·에러뿐일 때 나오는 slim 줄(`:167-178`)에도 OTEL 항목은 없다. 결과: ① 실제 사용자 1명(필수 단계 완료)에게는 한 번도 보이지 않는다. ② 신규 사용자도 로그인을 마치는 순간 이 항목을 잃는다. 이 시점이 바로 OTEL을 설정할 차례인데도 그렇다. 목표("설치 후 OTEL 설정 한 번")와 맞지 않는다 | 필수 단계가 모두 done이고 OTEL 상태가 `todo`/`failed`일 때 `.onboard-slim` 한 줄을 그린다. 문구는 "선택: 사용량 수집키 미적용", 버튼은 기존과 같다. `applied`/`confirmed`/`loading`/`error`이면 아무것도 그리지 않는다. 박스 전체를 다시 열지 않으니 기존 사용자 회귀 방지 의도는 유지된다. 하네스 `otel-link-unlinked-does-not-reopen-checklist`의 `.onboard-slim` 부재 단언은 "`.onboard-box` 부재 + OTEL slim 존재"로 바꾼다. 비용: 낮음. 근거는 `.onboard-slim` 마크업·CSS가 이미 있다는 것(`home.ts:169-177`) | `home.ts` renderOnboarding 전체와 `onboarding.ts` computeOnboarding을 읽음. `onboardingOtelLink.test.mjs` 마지막 테스트가 "필수 완료+미적용 → allDone=true"를 의도로 고정하고 있음을 확인 | 기획의도 |
| RV-002 | 🟡 Minor | `src-tauri/src/otel_link.rs:81-82`, `:166-167` | `HeaderUnrecognized` 문구는 "…헤더가 있어 **자동으로 바꾸지 않았습니다**"이다. 그런데 이 원인은 지금 파일을 읽어 도출하는 경로(`derive_link_status`)에서만 나온다. 자동 발급이 시도된 적이 없는데도 화면(PNG 03)은 앱이 자동으로 바꾸려다 멈춘 것처럼 읽힌다. 요구사항 3("미구현 기능으로 오도 금지")과 어긋나는 문장이다. 하네스 BANNED 목록에 이 표현이 없어 검사를 통과했다. 또 사내 collector용 자기 Authorization을 일부러 둔 사용자도 `!` 실패로 표시된다 | 문구를 맥락별로 나눈다. 파일 도출용은 "설정 파일에 허브 수집키 형식이 아닌 Authorization 헤더가 있습니다. 허브로 보내려면 설정 > OTel에서 수집키로 교체하세요."로 쓴다. "자동으로 바꾸지 않았습니다"는 실제 발급 시도가 멈췄을 때만 쓴다(원인 변형을 하나 더 두거나 `issue_and_apply` 쪽에서 원인을 갈아끼운다). 표시는 `failed` 대신 `todo`로 낮추는 것도 고려한다 | 문구 정의와 호출부 2곳(`derive_link_status`, `issue_and_apply:252-257`)을 읽음. 현재 `issue_and_apply`는 테스트에서만 호출됨(`#![allow(dead_code)]`, 커맨드 미등록)을 `lib.rs` diff로 확인. PNG 03 열람 | 기획의도 |
| RV-003 | 🟡 Minor | `src/onboarding.ts:184-188` / `src-tauri/src/otel_link.rs:156-170` | `applied` 상태는 헤더에 `mhk_` 키가 있는지만 본다. 그런데 문구는 "새로 시작하는 Claude Code 세션부터 적용됩니다"이다. `CLAUDE_CODE_ENABLE_TELEMETRY`가 꺼져 있거나 엔드포인트가 비었거나 허브가 아니면 아무것도 수집되지 않는데, 사용자는 수집될 거라고 믿게 된다. 엔드포인트 평가(`assess_effective_endpoints`, `otel_settings.rs:181`)는 이미 있지만 연동 상태가 쓰지 않는다 | `IngestLinkStatus`에 `telemetryEnabled`·`targetsHub` 두 불리언을 싣는다(URL은 싣지 않는다). 둘 중 하나라도 거짓이면 문구를 "수집키는 적용됐지만 텔레메트리가 꺼져 있거나 허브 엔드포인트가 아니라 수집되지 않습니다"로 바꾼다. 더 싸게 가려면 문구에서 "적용됩니다"를 빼고 "수집 여부는 설정 > OTel에서 확인하세요"로 바꾼다 | `derive_link_status`, `read_ingest_inputs`(`otel_settings.rs:519-522`)와 `build_status`(`:180-193`)를 나란히 읽음 | 기획의도 |
| RV-004 | 🟡 Minor | `src-tauri/src/otel_link.rs:250-270`, 머리말 `:23-24` | `issue_and_apply`는 허브 형식이 아닌 Authorization 헤더가 있으면 발급 전에 멈춘다. 하지만 엔드포인트가 허브가 아닌 곳이나 평문 http로 향하는 경우는 검사하지 않는다. `OTEL_EXPORTER_OTLP_HEADERS`는 설정된 모든 엔드포인트로 나간다(`otel_ingest_key.rs:104-106`). 수동 입력은 화면 경고(R12)로 이 위험을 알리지만, 자동 경로에는 경고 화면이 없다. 그래서 허브가 발급한 `mhk_`가 사용자 모르게 외부 collector로 나갈 수 있다. 지금은 dead code라 실제 영향은 없다. 다만 머리말의 "계약이 정해지면 바뀌는 곳은 구현체뿐"이라는 문장이 이 빈틈을 가린다 | 발급 전 선검사에 `assess_endpoints`를 넣는다. `foreign_host \|\| insecure`이면 새 원인(예: `EndpointNotHub`)으로 멈춘다. 지금 고치지 않는다면 머리말에 "실 발급기 연결 전 엔드포인트 목적지 검사 추가 필요"를 명시해 계약 착수 시 유실되지 않게 한다 | `issue_and_apply` 전체, `assess_endpoints`(`otel_ingest_key.rs:122-152`), R12 3차 보고서의 목적지 경고 맥락 대조 | 보안(미래 경로) |
| N-1 | ⚪ Nit | `src-tauri/src/otel_link.rs:41-42` | `issue_and_apply`가 동기 함수라 `Issuing`은 계약이 붙어도 백엔드가 돌려줄 수 없는 상태다(호출 중에는 응답이 없다). 지금 enum 주석 "허브 호출이 붙기 전에는 만들어지지 않는다"는 붙은 뒤에는 만들어진다는 뜻으로 읽힌다 | RT-1 참조. 주석을 "프론트 진행 중 표시용, 백엔드 응답으로는 오지 않는다"로 고치거나 프론트 전용 상태로 옮긴다 | 함수 시그니처 확인 | 정확성 |
| N-2 | ⚪ Nit | `src-tauri/src/otel_link.rs:27` | 모듈 전체 `#![allow(dead_code)]`라 앞으로 진짜 죽은 코드가 생겨도 경고가 나지 않는다 | 항목 단위 `#[allow(dead_code)]`로 좁힌다(`otel_settings.rs:506`처럼) | 파일 머리 확인 | 유지보수 |
| N-3 | ⚪ Nit | `src/main.ts:266`, `src/views/settings.ts:130` | 연동 상태는 앱 세션당 한 번 읽는다. 저장·삭제 후에는 다시 읽지만, `settings.json`을 바깥에서 고치거나 설정 화면에서 `loadOtelEnv`로 새로 읽을 때는 갱신하지 않는다 | `loadOtelEnv` 성공 시 `loadIngestLink()`도 부른다 | 호출부 grep | 정확성 |
| N-4 | ⚪ Nit | `src/onboarding.ts:174-181` (unlinked 문구), PNG 01 | 미연동 안내가 세 문장, 두 줄 이상이다. 체크리스트 다른 행은 한 줄이다 | 행에는 "설정 > OTel에서 수집키를 붙여넣으세요"만 남기고, 나머지는 설정 화면 안내(이미 R12에 있음)로 넘긴다 | PNG 01 열람 | UX |
| RT-1 | 🔵 Rethink | `src-tauri/src/otel_link.rs:155-170`, `:274-286`, `:288-299` | **`confirmed`가 저장될 곳이 없다.** 상태는 매번 `settings.json`에서만 도출된다. 그래서 `confirm_receipt`가 `Confirmed`를 돌려줘도 다음 `otel_ingest_link_status` 조회에서 `applied`로 돌아간다. 이를 피하려고 조회 때마다 허브를 부르면 커맨드 주석("허브를 부르지 않는다")과 충돌한다. 즉 지금 모델은 "계약이 붙으면 구현체만 바뀐다"고 주장하지만, 실제로는 상태 저장 위치라는 설계 결정이 하나 더 남아 있다 | 대안 구조: (a) 확인 결과를 앱 데이터 디렉터리에 `{maskedPrefix, confirmedAt}`로 캐시한다. 조회 시 현재 키의 마스킹과 같을 때만 `confirmed`로 승격하고, 키를 바꾸거나 지우면 자동으로 무효화된다(값은 마스킹뿐이라 시크릿이 늘지 않는다). (b) `Issuing`은 백엔드 enum에서 빼고 프론트 진행 중 플래그로 둔다. 그러면 백엔드 상태는 4개이고 모두 실물(파일+캐시)에서 도출된다 | `derive_link_status`가 파일 입력만 받음, `otel_ingest_link_status`가 `confirm_receipt`를 부르지 않음을 확인 | 구조 |
| RT-2 | 🔵 Rethink | `src/views/home.ts:58`, `:163-187` | **OTEL 연동이 "시작 체크리스트"의 행이어야 하나?** 체크리스트는 다 끝나면 사라지는 일회성 장치다. 반면 연동 상태(적용됨→수신확인됨, 실패, 키 회전)는 계속 살아 있는 상태라 성격이 다르다. RV-001은 이 불일치의 증상이다 | 대안 구조: 홈 하단의 이미 있는 `mcpHubWidget()`(`home.ts:58`, malgnai-hub 위젯)에 "수집키: 미적용 / 적용됨(`mhk_AbCd…`, 수신 미확인) / 실패" 한 줄과 버튼을 붙인다. 허브 기기 연결과 수집키를 한 위젯에서 보면 "`mhd_` 연결 → `mhk_` 발급" 흐름도 같이 드러난다. 체크리스트에는 필수 3단계만 남긴다. RV-001의 slim 줄은 이 구조로 가기 전의 저비용 임시책이다 | `home.ts` 레이아웃(`:51-59`)과 위젯 목록 확인 | 구조 |

## 페르소나별 관점

### 보안 — `persona-credential-at-rest-and-transit`
- `mhd_` 분리는 구조로 막혀 있다. 발급 trait의 반환은 `SecretString`뿐이고, 원인 enum `LinkFailureCause`는 데이터를 품지 않는다(`otel_link.rs:55-72`). 상태 구조체에는 원문 필드가 없다. 테스트는 settings.json, 백업, 회전 후 백업, 상태 JSON·Debug, 발급기 Debug, 진입점 에러 문자열을 모두 검사한다(`:420-466`, `:640-652`). 발급기가 `mhd_`를 돌려주는 11가지 변형도 거부한다(`:490-531`)
- `0600`(회전 후 넓어진 백업 포함)과 원자적 rename을 검증한다. 하드링크 inode 테스트와 rename 실패 시 임시파일 잔존 0 테스트가 있다(`:658-728`). 둘 다 unix 전용이다. Windows ACL은 기존과 같은 수준이고 이번 범위 밖이다
- 기존 백업을 그대로 쓴다. Set 후 백업에는 직전 `mhk_`가 한 세대 남는다. 이는 R12에서 수용된 동작과 같다(`:456-464`가 고정)
- 빈틈은 하나다. 키가 나가는 **목적지**를 검사하지 않는다 → RV-004

### 마스킹 — `persona-displayed-text-redaction-vs-diagnostic-value`
- `mhk_` 규칙(`src/errorMessage.ts:68-71`)이 소문자만 있는 키까지 가린다. 소문자 키는 일반 불투명 토큰 규칙으로는 걸리지 않는다. 테스트(`errorMessage.test.mjs`)는 4가지 본문 모양 × 2개 접두어, JSON·URL·따옴표·여러 줄, Authorization 헤더 4가지 형태, `maskLogText`의 `keepUrls` 양쪽을 검사한다
- 마스킹 표시값 `mhk_AbCd…`(본문 4자)는 `{6,}` 하한 덕분에 보존된다. 프론트는 `MASKED_KEY_SHAPE`(`onboarding.ts:152-156`)로 한 번 더 걸러, 원문 모양이 오면 아예 표시하지 않는다. 진단 가치와 가림 사이의 균형이 맞다
- PNG 04의 "Bearer ***"는 `mhd_` 규칙이 아니라 Bearer 규칙이 먼저 잡은 결과다. `mhd_` 단독 경우는 단위 테스트가 덮으므로 문제 없다(기각 K-3)

### 주장 vs 관측 — `persona-claimed-vs-verified`
- 도달 가능한 상태(unlinked/applied/failed)의 문구에 "자동 연동"·"연동 완료"·"수신 확인됨"이 없다. 이를 TS 테스트와 하네스 양쪽이 고정한다. `applied`를 `◐`로 표시해 완료(✓)와 구분한 것도 정직하다
- 다만 금지어 목록 밖에서 두 문장이 실제보다 많이 말한다. "자동으로 바꾸지 않았습니다"(RV-002), "새로 시작하는 … 세션부터 적용됩니다"(RV-003)
- "계약이 정해지면 바뀌는 곳은 구현체뿐"(`otel_link.rs:23-24`)은 RV-004·RT-1 때문에 지금은 과장된 주장이다

### 첫 실행 신입 — `persona-first-run-newcomer`
- 완주를 시뮬레이션했다. 필수 단계가 남은 동안 OTEL 행이 보이고, "수집키 입력하기"를 누르면 `#/settings/otel`로 가서 붙여넣는 경로까지 이어진다(하네스 단언). 로그인을 마치면 박스가 사라지고, OTEL 행을 다시 볼 길이 없다 → RV-001
- 안내는 정확하다. 허브 웹에 발급 화면이 없고 기기 토큰은 수집키가 아니라는 사실이 R12 결론과 맞는다. 다만 길다 → N-4

### 발산형 — `persona-zero-base-redesigner`
- RT-1(`confirmed`의 저장 위치가 없고 `Issuing`은 구조상 백엔드에서 나올 수 없음), RT-2(연동 상태는 일회성 체크리스트가 아니라 상시 위젯 쪽에 맞음). 두 대안 모두 현재 코드 자산(마스킹 값, `mcpHubWidget`)을 재사용하는 구조다

## PM 쟁점에 대한 판단

**1. 기존 비허브 Authorization이 있으면 `issue_and_apply`가 발급 전에 멈춤(수동 입력은 덮어씀) — 타당하다.**
- 자동 경로가 사용자가 둔 다른 서비스의 자격증명을 말없이 지우면 안 된다. 수동 입력은 사용자가 명시적으로 한 행동이라 교체해도 된다. 이 비대칭이 맞다
- 적용하지 못할 키를 먼저 발급받지 않으니, 허브에 폐기되지 않은 고아 키가 쌓이지도 않는다(테스트 `issuer.calls() == 0`로 고정, `otel_link.rs:568`)
- 보완 두 가지. ① 같은 논리라면 **목적지가 허브가 아닌 경우**에도 멈춰야 한다(RV-004). 남의 헤더를 지키는 것보다 우리 키가 남에게 가지 않게 하는 것이 더 중요한 경계다. ② 실 발급기를 붙일 때는 멈춘 뒤 막다른 골목이 되지 않도록 "확인 후 교체" 동의 버튼을 두는 것이 좋다. 지금은 "OTel 설정 열기"로 수동 경로가 열려 있어 막다른 골목은 아니다

**2. 체크리스트 OTEL 항목이 '선택' 참고 항목이라 필수 3단계 완료 사용자에게는 보이지 않음 — 목표 대비 부족하다(RV-001, Major).**
- 기존 사용자에게 박스를 다시 띄우지 않겠다는 의도는 옳다. 하지만 지금은 "숨김"과 "박스 전체 재노출" 둘 중 하나밖에 없어서, 실제 사용자 1명에게는 이 기능이 보이지 않은 채 배포된다
- 권고: 박스는 다시 열지 말고, 미적용·실패일 때만 slim 한 줄을 띄운다. 구조적 해법은 RT-2(허브 위젯으로 이동)다
- 트레이드오프: 사용자가 "체크리스트는 설치 중에만 보이면 된다"고 명시적으로 결정한다면 이 지적은 Minor로 내리고 그 결정을 기록하면 된다. 그 경우에도 설정 > OTel 화면이 유일한 진입점이 된다는 점은 남는다

## 기각된 지적

| ID | 원래 주장 | 기각 사유 |
|---|---|---|
| K-1 | `apply_ingest_key`가 `values: Default`로 저장해 기존 OTEL 관리 키를 지운다 | `save_to_settings_file`의 반영 루프(`otel_settings.rs:448-454`)는 넘겨받은 키만 upsert·삭제한다. 빈 맵이면 아무것도 바꾸지 않는다. 테스트 `issued_key_flow_*`가 기존 env 보존을 확인한다(`otel_link.rs:448-449`) |
| K-2 | `SecretString::new`의 `#[cfg(test)]` 제거가 노출 표면을 넓힌다 | `pub(crate)`이고, `Serialize`는 여전히 미구현이며 Debug는 가려진다(`otel_ingest_key.rs:36-59`). 생성자를 열어도 원문이 밖으로 나가는 경로는 생기지 않는다 |
| K-3 | 하네스 조회 실패 시나리오가 `mhd_` 마스킹 규칙을 증명하지 못한다(Bearer 규칙이 먼저 잡음) | 맞는 관찰이지만 결함은 아니다. `mhd_` 단독·JSON·URL 맥락은 `errorMessage.test.mjs`가 직접 덮는다 |
| K-4 | `issuer.issue()`가 동기 네트워크 호출이 되면 Tauri 메인 스레드를 막는다 | 현재 이 함수를 부르는 커맨드가 없다(실물 확인). 아직 존재하지 않는 구현을 두고 유추한 지적이라, 계약 착수 시 리뷰 항목으로 넘긴다 |
| K-5 | 테스트 임시 디렉터리가 병렬 실행에서 충돌한다 | `temp_settings_path`의 이름이 테스트마다 다르다. 같은 이름을 반복 쓰는 곳은 한 테스트 안의 순차 루프뿐이다 |

## 잘 된 점
- **불변식을 타입으로 강제했다.** 원인은 데이터 없는 enum이고 문구는 `&'static str`이다. 그래서 허브 응답 문자열이 화면에 섞일 경로가 컴파일 단계에서 없다. 계약이 붙은 뒤에도 유지된다
- **진입점이 하나다.** 수동 입력과 주입 경로가 같은 입력에 바이트 단위로 같은 본 파일·백업을 만든다는 테스트(`otel_link.rs:611-638`)가 "수동 입력 동작 유지"를 관측으로 증명한다
- **진입점이 스스로 방어한다.** 오케스트레이션의 선검증을 우회해 `apply_ingest_key`에 `mhd_`를 직접 넣어도 거부된다(`:640-652`)
- **허브 추측 구현이 0이다.** 경로·상태코드·필드명을 하나도 지어내지 않았다. 미지원은 `is_supported=false`로 프론트 계약에 실어, UI가 기능 유무를 추측하지 않게 했다
- **원자적 쓰기를 행동으로 검증했다.** "rename을 쓴다"를 코드 읽기가 아니라 inode·하드링크·실패 잔존물로 확인한다
- **CI에 TS 테스트를 의존성 추가 없이 넣었다**(`node:test` + strip-types)
- 프론트가 `masked` 형식을 다시 검사한다(`safeMasked`). 백엔드가 실수해도 원문이 나가지 않는 이중 방어다

## PM 권고
1. **RV-001을 이번에 고친다**(저비용 slim 줄). 사용자가 "체크리스트는 설치 중에만"을 원한다면 그 결정을 `decision_record`로 남기고 Minor로 내린다
2. RV-002·RV-003은 문구 수정 수준이라 RV-001과 같은 커밋에 묶는 것을 권한다
3. RV-004는 지금 고치거나, 최소한 `otel_link.rs` 머리말에 "실 발급기 연결 전 목적지 검사 필수"를 남긴다. 그리고 허브 계약 착수 WBS의 선행 항목으로 등록한다. RT-1(`confirmed` 저장 위치)도 같은 WBS에 설계 결정 항목으로 둔다
4. 수정 후 재검토는 같은 target_id `r16-otel-link-pre-contract`로 위임하고, 재검토 3요소(target_id, 이 보고서 경로, 리스크 범주)를 명시한다. 문구·프론트만 바뀌면 축소 모드 대상이다
