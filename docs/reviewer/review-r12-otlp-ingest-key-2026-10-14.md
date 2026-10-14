# R12 리뷰: 허브 OTLP 수집키 입력·저장·삭제

리뷰 대상: 커밋 범위 `5ccc00b..889919f` (889919f) / target_id `r12-otlp-ingest-key`
- 등급: Sensitive, 최초 리뷰, 풀패널(노출 범위 축소 미적용 — 위임 메시지에 "축소 없음" 명시)
- 리스크 범주: 크리덴셜 저장·전송(인증)
- 일자: 2026-10-14 / 리뷰어: reviewer
- 실행 액션: 없음. 코드 수정·커밋·push·허브 쓰기를 하지 않았다. 페르소나 파일과 이 보고서만 작성했다(커밋하지 않음).

## 종합 판정: 🟡 Amber

Critical 0 / Major 1 / Minor 4 / Nit 6 / Rethink 2

앱 쪽 크리덴셜 처리는 견고하다. 원문이 IPC 응답·Err·Debug·DOM으로 나가는 경로를 찾지 못했고, 마스크를 다시 저장해 키를 덮어쓰는 경로도 구조적으로 없다. 다른 헤더는 보존되고, settings.json과 백업은 0600으로 쓰인다. 모두 테스트로 실측했다.
판정을 Amber로 만든 것은 **허브 쪽 현실과 맞지 않는 안내**(RV-001) 하나다. 앱이 "허브에서 수집키 발급 ↗"로 보내는 `/keys` 화면에는 수집키 UI가 없다. 거기 있는 "새 인증키 발급"은 MCP 전권을 가진 기기 토큰(`mhd_`)을 만든다. 그래서 지금 출시하면 모든 사용자가 막다른 골목에 서고, 그 과정에서 불필요한 고권한 토큰까지 하나씩 생긴다.

**Green 필수 조치**: RV-001 하나다. 아래 둘 중 하나를 고른다.
- (a) 허브 수집키 발급 UI가 나오기 전까지 앱 문구와 버튼을 현실에 맞춘다. 발급과 폐기는 "관리자에게 요청"으로 안내하고, `/keys`를 "수집키 발급"이라 부르지 않는다.
- (b) 허브 `keys.vue`에 수집키 발급·폐기 UI가 배포된 뒤에 이 기능을 출시한다(교차 저장소 선행조건으로 WBS에 등록).

## 페르소나 재사용 판정

착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-credential-at-rest-and-transit.md` (보안) | **신규** | INDEX에 "자격증명이 디스크(본 파일·백업·임시 파일)에 남는 자리의 권한·수명과 전송 목적지"를 다루는 행이 없다. 가까운 행은 셋인데 과녁이 다르다. `process-execution-security`는 실행 명령의 argv·env·프로세스 수명, `displayed-text-redaction-vs-diagnostic-value`는 화면 텍스트 가림, `delegated-authority-blast-radius`는 자식 프로세스 권한을 사용자가 아는지를 본다. 이번 위임은 보안 관점 페르소나가 필수다 |
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | DOM·토스트·입력칸 원문 비노출과 마스킹의 식별 가치를 보는 일이 역할개념 그대로다 |
| `persona-design-contract-auditor.md` | 재사용 | 허브 정규식·§5.3 전달 방식·안내문 상수화 조항 대조가 역할개념 그대로다 |
| `persona-first-run-newcomer.md` | 재사용 | 경고를 본 사용자가 키를 얻는 데까지 완주하는지가 역할개념 그대로다 |
| `persona-claimed-vs-verified.md` | 재사용 | 불변식 주석·하네스 주장과 실제 검증 범위의 대조가 역할개념 그대로다 |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수이고 역할개념이 여전히 유효하다 |

## 직접 확인한 것

- `git diff 5ccc00b..889919f` 전체를 읽었다. `otel_ingest_key.rs` 574줄 전문, `otel_settings.rs` 변경과 주변(30-130, 230-475), `fs_atomic.rs` 전문, `otelApi.ts`, `settings.ts` 변경분, 하네스 `otelIngestKey.mjs`(50-75, 150-245)
- `cargo test --manifest-path src-tauri/Cargo.toml`: **768 passed / 0 failed / 21 ignored**. `otel` 필터로는 54 passed
- `pnpm build`: 성공
- 원문 유출 grep: `otel_settings.rs`·`otel_ingest_key.rs`·`fs_atomic.rs`에 `eprintln`/`println`/`log::`가 0건이다. `src/*.ts`·`settings.ts`의 `console.*` 3건은 모두 수집키와 무관하다(settings.ts:225는 자동 설정 실패 로그이고, 그 payload에는 ingestKey가 없다). `localStorage`는 updateApi.ts에만 있다. `otelDraft`는 모듈 변수라 영속화되지 않는다
- settings.json 쓰기 주체: `save_to_settings_file` 하나뿐이다(grep `settings.json`, `write_atomically`). `user_config`·`app_links`·`hub_auth/locator`는 다른 파일을 쓴다. 그래서 키가 든 파일이 다른 모듈의 0644 쓰기로 다시 넓어지는 경로는 없다
- 스크린샷 3장을 직접 열었다: `unset-telemetry-on-01-01-unset-warning.png`, `set-masked-03-03-after-save-masked-b.png`, `save-error-no-echo-01-01-save-error.png`(모두 `docs/screenshots/r12-ingest-key/otelIngestKey/`). 세 장은 서로 다른 상태를 보여준다. 미설정 경고, 설정됨 `mhk_BBBB…`, 형식 오류 토스트(원문 미인용)다
- 허브(읽기 전용): `server/lib/otlp-limits.js:72-74`(정규식 동치 확인), `server/api/ingest-credentials.js`(POST/GET/rotate/DELETE, JWT 축), `app/pages/keys.vue`(수집키 UI 없음, grep `ingest|mhk|수집` 0건, `app/` 전체 grep `ingest-credentials|mhk_` 0건), `server/lib/tokens.js:59`(기기 토큰 접두사 `mhd_`), `docs/design/otlp-ingest.md` §5.3(360-373행)
- pre-push 패턴 오탐 점검: `git grep -nE 'mhk_[A-Za-z0-9_-]{43}' 889919f` 0건, 범위 diff 0건

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 |
|---|---|---|---|---|---|
| RV-001 | 🟠 Major | `src/views/settings.ts:427`(버튼), `:445`(안내문), `:448`("즉시 폐기"), `:163`(실패 토스트), `src-tauri/src/otel_settings.rs:123`(`/keys` URL) | **안내가 허브 현실과 다르다.** 허브 `/keys`("인증키 관리")에는 MCP 기기 토큰(`mhd_`) 발급·폐기만 있고 수집키 UI가 없다. 허브 `app/` 전체에도 `ingest-credentials` 호출이 0건이다. 따라서 (1) 버튼을 따라간 사용자는 수집키를 얻을 수 없다. (2) 그 화면의 "새 인증키 발급"을 누르면 **MCP 전권 토큰이 하나 더 생긴다**. 앱이 형식 검증으로 거부하므로 저장되지는 않는다. 하지만 거부당한 사용자가 그 토큰을 채팅·이슈에 붙여 넣고 도움을 청할 가능성이 생긴다. (3) "유출 시 허브에서 즉시 폐기"도 사용자에게는 수단이 없다. 자가 폐기는 JWT로 `DELETE /api/ingest-credentials/:id`를 직접 호출해야만 된다. 이 기능의 목적("기본 설정 PC가 수집됨")이 출시 시점에 셀프서비스로 달성되지 않는다 | 위 "Green 필수 조치" (a) 또는 (b). (a)를 고르면 버튼은 "수집키 발급 요청 방법" 같은 문구로 바꾸고 `/keys`로 보내지 않는다. 안내문은 "관리자에게 발급 요청", 폐기는 "관리자에게 즉시 폐기 요청"으로 바꾼다. 허브 UI가 나오면 원래 문구로 되돌린다(허브 WBS와 연결) | 허브 `keys.vue` 원문 열람(5·15·36·90행), `app/` grep, `tokens.js:59` |
| RV-002 | 🟡 Minor | `src-tauri/src/otel_settings.rs:296`, `:432`, `src/views/settings.ts:377`·`:395-407` | 수집키는 `OTEL_EXPORTER_OTLP_HEADERS`이므로 **설정된 모든 엔드포인트로** 나간다. 그런데 엔드포인트 검증은 `http://`도 허용하고 호스트도 가리지 않는다. 그래서 키를 넣은 PC에서 누군가 엔드포인트를 `http://…`나 허브가 아닌 collector로 바꾸면 Bearer 키가 평문으로, 또는 제3 호스트로 나간다. 반대로 경고 조건은 엔드포인트를 보지 않는다. 사내 collector(①경로)를 쓰는 PC에서도 "허브가 인증 실패로 거부합니다"라는 틀린 경고가 뜬다(설계 결정 8 "3경로 공존") | 키가 `set`일 때 엔드포인트가 `https://` + 허브 origin이 아니면 경고를 띄운다(저장 거부까지는 과하다, 45인 사내 도구). "허브가 거부" 경고는 엔드포인트가 허브 origin일 때만 띄운다 | 코드 정독. 실기 전송은 하지 않았다 |
| RV-003 | 🟡 Minor | `src/views/settings.ts:127`, `:445`, 삭제 다이얼로그 `:140` | 저장·삭제 직후 "저장했습니다/삭제했습니다"만 나온다. 이미 떠 있는 Claude Code 세션은 시작 시 읽은 env로 OTel exporter를 만들었다. 그래서 새 키는 **새 세션부터** 적용되고, 삭제해도 기존 세션은 옛 키로 계속 보낼 것으로 추정한다. 허브 설계 §14-11③도 같은 성격(endpoint 변경 후 기존 프로세스 동작)을 미확인 항목으로 둔다 | 안내문과 토스트에 "새로 시작하는 Claude Code 세션부터 적용됩니다"를 한 줄 넣는다 | 코드 정독 + OTel SDK 초기화 시점 추론(**실측 아님**). 확인 못 한 추정이라 Minor로 둔다 |
| RV-004 | 🟡 Minor | `src-tauri/src/otel_settings.rs:440-443`(백업), 테스트 `:997`, `src/views/settings.ts:140` | "수집키 삭제" 뒤에도 직전 키가 `settings.json.malgn-bak`(0600)에 평문으로 남는다. 테스트(`:997` "백업은 직전 원본")가 이 동작을 의도로 고정한다. 다음 저장 때까지 유지된다. 그리고 로컬 삭제는 허브 쪽 폐기가 아니다. 다이얼로그는 "수집되지 않습니다"만 말하므로, 유출 의심으로 삭제를 누른 사용자는 안전해졌다고 오해할 수 있다 | 다이얼로그에 "허브의 키는 폐기되지 않습니다(유출이 의심되면 폐기 요청). 직전 백업에 1세대 남습니다"를 적는다. 또는 Clear일 때 백업 내용에서도 Authorization 항목을 지운다(롤백 가치와 맞바꾸는 선택이다) | 코드·테스트 정독 |
| RV-005 | 🟡 Minor | `scripts/pre-push-sensitive-scan.mjs:13-33` | 구현자 제안(`mhk_[A-Za-z0-9_-]{43}` 추가)은 **이번 범위에 넣기를 권한다**. 이 저장소는 public이다. 이번 변경으로 개발자가 실제 `mhk_` 키를 로컬에서 다루기 시작한다(하네스·fixture 디버깅 중 붙여넣기 위험). 현재 트리와 이번 diff에서 오탐 0건을 확인했다. 테스트 가짜 키는 모두 `repeat()`로 만들어 리터럴이 아니다. 비용은 1줄이다. 같은 이유로 기기 토큰 `mhd_[A-Za-z0-9_-]{43}`도 함께 넣을 만하다 | PATTERNS에 `{ name: "malgnai-hub ingest key", re: /mhk_[A-Za-z0-9_-]{43}/g }`을 추가한다(선택: `mhd_` 동형) | `git grep`·diff grep 0건 실측 |
| N-1 | ⚪ Nit | `scripts/ui-harness/flows/otelIngestKey.mjs:50-63` | `assertNoRawKey` 주석은 "HTML + 텍스트 + input.value"를 점검한다고 하지만 `values`는 수집만 하고 비교하지 않는다. 저장 후 입력칸 비움은 `:166-169`가 따로 확인하므로 결과에는 영향이 없다 | 주석을 실제 범위로 고치거나, 입력 중이 아닌 시점에서는 values도 비교한다 | 코드 정독 |
| N-2 | ⚪ Nit | `src-tauri/src/otel_settings.rs:195-198` | "`Debug`는 derive해도 안전하다 — values에는 수집키 키가 들어올 수 없고 저장 시 거부된다"는 과대 주장이다. 거부는 역직렬화 **뒤**에 일어난다. 프론트가 values로 헤더를 보내면 그 payload의 Debug에는 원문이 찍힌다. 지금은 payload를 Debug로 찍는 곳이 없어 실제 피해는 없다 | 주석을 "values 경유 원문은 저장 단계에서 거부되지만 payload Debug에는 찍힐 수 있다 — payload를 로그로 남기지 말 것"으로 정정한다 | 코드 정독, grep `{:?}` 0건 |
| N-3 | ⚪ Nit | `src-tauri/src/fs_atomic.rs:52-54` | `write_private_file`이 open 뒤 `write_all`에서 실패하면(디스크 가득 등) 키 일부가 든 임시 파일이 남는다. 정리는 rename 실패 때만 한다. 파일 모드가 0600이라 위험은 낮다 | private 분기에서는 write 실패 때도 `remove_file`한다 | 코드 정독 |
| N-4 | ⚪ Nit | `src-tauri/src/otel_ingest_key.rs:206-219` | `classify`는 **첫** Authorization을 본다. OTel JS가 중복 키에서 무엇을 쓰는지는 미확인이다(대개 마지막). 또 `Bearer%20mhk_…`를 Unrecognized로 분류하지만, OTel JS가 값을 퍼센트 디코딩한다면 실제로는 동작한다. 손으로 편집한 파일에서만 생기는 일이고, 경고 문구도 "거부할 수 있습니다"로 유보형이라 피해가 작다 | 조치는 필요 없다. 필요하면 주석에 한계를 한 줄 남긴다 | 코드 정독. OTel JS 동작은 **확인 필요** |
| N-5 | ⚪ Nit | `src/views/settings.ts:448`, 허브 `server/api/ingest-credentials.js:15`, 설계 §5.3(370행) | 안내문이 허브 `NOTICE` 상수와 글자까지 같은 하드코딩 사본이다. §5.3은 안내문을 서버 상수 하나에서 생성하라고 한다. 허브 문구가 바뀌면 앱만 옛 문구로 남는다. 범위 밖이지만 관련된 관찰도 하나 있다. 앱 기본 `OTEL_LOGS_EXPORT_INTERVAL=5000`이 허브 `OTLP_CLIENT_ENV`의 60000과 다르다. 이번 변경으로 ③경로가 실제로 열리므로 키당 레이트리밋(`OTLP_INGEST_RL_LIMIT=120/60s`)과의 관계를 확인할 가치가 있다(레이트리밋이 키 단위인지는 미확인) | 사본임을 주석으로 표시한다. 기본 주기 정합은 별도 백로그로 올린다 | 허브 원문 대조 |
| N-6 | ⚪ Nit | `src-tauri/src/fs_atomic.rs:64-67`·`:84-87` | Windows에서는 ACL을 조이지 않는다. `%USERPROFILE%\.claude`의 기본 상속 ACL(사용자·SYSTEM·Administrators)에 기댄다. 주석이 이 사실을 정직하게 적어 두어 수용할 만하다 | 조치는 필요 없다 | 코드 정독. Windows 실기 ACL은 **미확인** |

## 기각된 지적

| 후보 | 기각 사유 |
|---|---|
| "settings.json 파싱 에러(`설정 파일을 해석하지 못했습니다: {e}`)에 키 원문이 실린다" | serde_json 구문 에러 메시지는 위치(line/column)만 담고 입력 내용은 인용하지 않는다. `Value`로 파싱하므로 invalid type 에러도 생기지 않는다. 테스트 `errors_never_contain_raw_key`(3)이 파싱 불가 파일의 parseError에 키가 없음을 실측한다 |
| "Tauri 인자 역직렬화 에러가 key 원문을 에코한다" | `IngestKeyChange`의 에러는 `unknown variant`·`missing field` 형태로 변형 이름만 담는다. 문자열 key 값은 에코하지 않는다. 비문자열 key는 정상 프론트 경로에서 생기지 않는다 |
| "다른 모듈이 settings.json을 0644로 다시 써서 권한이 넓어진다" | grep 결과 settings.json 쓰기 주체는 `save_to_settings_file` 하나다. Claude Code 자체의 재쓰기 동작은 이 저장소 범위 밖이라 판정하지 않았다 |
| "마스크(`mhk_AAAA…`)가 다시 저장돼 키를 덮어쓴다" | 마스크는 입력칸 값으로 쓰이지 않는다. values 경로는 헤더 키를 거부하고(`otel_settings.rs` 저장 검증), Set 경로는 47자 형식 검증으로 거부한다. 테스트 `save_without_new_key_keeps_existing_key`와 하네스 keep payload 검사(`otelIngestKey.mjs:147-152`)가 둘 다 실측한다 |

## PM 결정 평가 — settings.json env 평문(0600) vs 키체인+otelHeadersHelper

**동의한다. 단 전제 하나가 현재 깨져 있다.**

동의 근거:
1. **런타임 노출 표면이 같다.** 키체인에 넣더라도 helper가 Claude Code 프로세스에 헤더를 공급한다. env 경로라면 Bash 도구·hook·stdio MCP 자식에게 상속되고(허브 §5.3 M4, PoC 확인), helper 경로라도 Claude Code 프로세스 메모리에 있고 helper 명령 자체를 에이전트가 실행할 수 있다. 에이전트·프롬프트 인젝션이라는 주 위협 앞에서 키체인은 표면을 거의 줄이지 못한다.
2. **실패 양상이 더 나쁘다.** helper는 실패하면 export가 조용히 멈춘다(§5.3). 또 settings.json에 앱 바이너리 경로가 박혀 앱 이동·업데이트·Windows 경로 차이에 취약해진다. 이 앱이 키체인을 쓰는 선례(`hub_auth`, 기기 토큰)는 **MCP 전권**이라는 권한 크기 차이로 정당화된다. 쓰기 전용 수집키에 같은 비용을 치를 이유가 약하다.
3. **권한이 작다.** `/v1/*` 쓰기 전용이고, 탈취됐을 때의 최대 피해는 그 직원 사용량 데이터의 오염(무결성)이다. 쿼터 자동 폐기(`QUOTA_REVOKE_ROW_THRESHOLD`)와 90일 휴면 폐기가 피해를 묶는다.

반례로 남겨 둘 것:
- 키체인이 실제로 막는 채널은 **정적 파일 채널**이다. settings.json은 dotfiles 저장소 커밋, 지원 요청 때의 "settings.json 보여주세요", 백업·동기화 도구, 에이전트의 `Read ~/.claude/settings.json`으로 자주 새는 파일이다. 0600은 다른 로컬 사용자만 막고 이 채널들은 막지 못한다. 근거 2·3이 이 반례를 이기는 이유는 "새도 폐기하면 된다"이다.
- 그런데 **폐기 수단이 지금 사용자에게 없다**(RV-001). 이 결정의 근거 3("폐기 가능")은 허브 수집키 UI가 나올 때 비로소 성립한다. 따라서 이 결정 자체는 승인하되, RV-001 해소(또는 관리자 폐기 경로 안내)를 이 결정의 성립 조건으로 기록하기를 권한다.

## 페르소나별 관점

### credential-at-rest-and-transit (보안, 수렴, 신규)
- C1 충족. 임시 파일은 생성 시점부터 `mode(0o600)`이고, 같은 이름의 잔여 파일이 있어도 `set_permissions`로 다시 조인다(fs_atomic.rs:73-80). 백업도 같은 경로로 쓴다(otel_settings.rs:442). 기존 0644 본파일·백업이 0600으로 좁혀지는 것과 임시 파일 잔여 0건을 테스트가 실측한다(`settings_and_backup_are_owner_only_on_unix`, `no_temp_file_left_behind`). 남은 틈은 N-3 하나다.
- C2 충족. `SecretString`에 `Serialize`가 없어 응답 구조체에 넣으면 컴파일되지 않는 장치가 좋다. `HeadersEdit::Replace`의 Debug도 가린다. 고정 문구 Err와 Unrecognized 원문 비노출(타 서비스 토큰 보호)까지 테스트가 있다.
- C3 충족. `ingestKey` 생략은 `Keep`이 기본값이라, 이전 버전 프론트나 자동 설정 경로가 키를 지울 수 없다.
- C4 미충족(RV-002). C5 부분 충족(RV-004). C6은 PM 결정 평가로 대신했다.

### displayed-text-redaction-vs-diagnostic-value (수렴, 재사용)
- 입력칸은 password 타입에 `autocomplete=new-password`이고, 저장 성공 시 드래프트를 즉시 비운다(settings.ts:128). 실패 시에는 재시도를 위해 남긴다. password 필드라 화면에도 보이지 않으니 타당하다.
- 마스킹 `mhk_`+4자는 허브 `token_prefix`(8자, `ingest-credentials.js:133`)와 같아서, 사용자가 허브 목록과 대조해 "어느 키인지" 식별할 수 있다. 가림과 식별 가치의 균형이 맞다.
- 실패 토스트는 Rust 고정 문구만 보여준다(스크린샷 save-error 확인).

### design-contract-auditor (수렴, 재사용)
- 정규식 동치를 확인했다. `is_valid_key`(len 47, `mhk_`, `[A-Za-z0-9_-]`) = `CREDENTIAL_TOKEN_BODY_RE`이고, 바이트 단위라 비ASCII도 거부된다. `bearer_key`의 대소문자 구분 `"Bearer "`도 `CREDENTIAL_TOKEN_RE`와 같다.
- §5.3 기본안(사용자 settings.json env, managed settings 미사용)과 일치한다.
- 이탈은 N-5(안내문 상수화 조항) 하나다. 기본 export 주기 불일치는 범위 밖 관찰로 기록했다.

### first-run-newcomer (수렴, 재사용)
- 완주를 시뮬레이션했다. 경고("아래에 수집키를 등록하세요") → "허브에서 수집키 발급 ↗" → 허브 "인증키 관리" → "새 인증키 발급"(기기 이름 입력) → `mhd_…` 획득 → 앱에 붙여넣기 → "형식이 올바르지 않습니다… mhk_로 시작하는 키". 여기서 **막힌다**(RV-001). 에러 문구는 정확하지만, 다음 행동(어디서 mhk_를 받나)을 알려주지 못한다.
- 저장 버튼이 15개 필드 아래에 있어, 키를 붙여 넣은 자리에서 스크롤해야 한다(RT-2와 연결).

### claimed-vs-verified (수렴, 재사용)
- 모듈 머리말 불변식 4개를 테스트로 하나씩 대조했다(응답 직렬화·Err·Debug·Unrecognized). 모두 실제 단언이 있다. 768건 전체 통과를 재실행으로 확인했다.
- 과대 주장 두 곳은 N-1(하네스 주석)과 N-2(Debug 주석)다.

### zero-base-redesigner (발산, 재사용)

| ID | 대안 구조 | 얻는 것 | 잃는 것/비용 |
|---|---|---|---|
| RT-1 🔵 | **붙여넣기 UI를 없애고 기기 토큰으로 자동 발급한다.** 앱은 이미 허브와 기기 토큰(`mhd_`, 키체인)으로 연결돼 있다. 허브에 `POST /api/devices/self/ingest-credential`(requireDeviceToken, 기기당 1개, 라벨=기기 이름)을 두면, 앱이 "허브 수집 켜기" 한 번으로 받은 `mhk_`를 바로 settings.json에 쓸 수 있다. 기기 연결을 해제하면 수집키도 함께 폐기된다 | RV-001의 막다른 골목이 구조적으로 사라진다. 키가 클립보드·브라우저·채팅을 거치지 않는다. 회전·폐기를 앱이 대행할 수 있다 | 허브 변경이 필요하다. 기기 토큰이 수집키 발급 권한을 얻는다(다만 MCP 전권보다 작은 권한이라 권한 상승은 아니다) |
| RT-2 🔵 | 수집키를 15키 env 폼과 분리해 **"허브 수집" 독립 카드**로 둔다(상태 + 자체 저장 버튼) | 다른 필드의 검증 실패가 키 저장을 막지 않는다. 저장 버튼이 키 바로 옆에 있다 | 저장 경로가 두 개가 된다. 지금도 Clear는 독립 호출이라(settings.ts:147) 비용은 작다 |

## 트레이드오프

- RV-004의 "Clear 시 백업에서도 키 제거"는 롤백 가치(실수로 지웠을 때 백업에서 복구)와 맞바꾸는 선택이다. 45인 사내 도구이고 키를 재발급할 수 있으므로 문구 보강만으로도 충분하다고 판단한다.
- RV-002는 저장 거부와 경고 중 경고를 권한다. 사내 collector(①경로) 사용자를 막지 않기 위해서다.

## 잘 된 점

- `SecretString`에 `Serialize`를 구현하지 않아 "응답에 넣으면 컴파일 실패"가 된다. 원문 비노출을 코드 리뷰가 아니라 타입으로 강제했다.
- `IngestKeyChange` 기본값이 `Keep`이라 생략·구버전 payload가 키를 건드리지 않는다. 마스크 재저장이 형식 검증에서 이중으로 막힌다.
- 0600을 rename 뒤가 아니라 **임시 파일 생성 시점**에 지정해 권한이 넓게 열리는 순간이 없다. 기존 0644 파일과 백업이 좁혀지는 것까지 테스트로 고정했다.
- 다른 서비스 토큰(Unrecognized)을 보지도, 보여주지도 않으면서 교체할 수 있다.
- 붙여넣기 관대 처리(`Bearer`/`Authorization=` 접두)와 엄격 검증을 분리했다. 이 덕분에 거부 테스트 케이스(따옴표·쉼표·이중 Bearer 등)가 명확하다.
- 하네스가 DOM 전체 덤프와 payload 로그로 원문 비노출을 실측하고, 스크린샷이 상태별로 실제로 다르다.

## PM 권고

1. RV-001은 (a)/(b) 중 하나를 고른 뒤 담당 에이전트에게 위임한다. (a)는 문구 4곳과 버튼 1개만 고치면 된다. (b)를 고르면 허브 WBS에 수집키 UI를 선행 항목으로 등록한다. 장기적으로는 RT-1을 허브 설계 후보로 올리기를 권한다.
2. RV-005(pre-push 패턴)는 비용이 1줄이니 이번 범위에 넣는다.
3. RV-002·003·004는 문구·경고 위주라 RV-001과 함께 처리하면 효율적이다. 미처리 시 백로그로 넘긴다.
4. 재검토는 증분 모드로 충분하다. RV-001 변경이 문구에 그치면 축소 모드도 된다. 재검토 위임에는 target_id `r12-otlp-ingest-key`와 이 보고서 경로, 리스크 범주를 적는다.

## 생략한 것

- 실제 허브로의 OTLP 전송(네트워크 쓰기 금지)과 Claude Code 실기동 재시작 동작(RV-003)은 확인하지 않았다.
- Windows 실기 ACL(N-6)과 OTel JS 헤더 파싱 세부(N-4)는 확인하지 않았다.
- 스크린샷 8장 중 3장만 직접 열었다(clear-flow·unrecognized·typed-dots는 하네스 단언 코드로만 확인).
- UI 하네스를 다시 실행하지 않았다. 구현자가 산출한 스크린샷과 하네스 코드를 근거로 삼았다.
