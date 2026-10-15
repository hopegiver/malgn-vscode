# R17 허브 수집키 자동 발급·회전·조회·첫 수신 확인 — 리뷰 보고서

리뷰 대상: `887ddaa..0450973` (e4dc35a, 48be9dd, 1f00126, 0450973) · target_id `r17-otel-autolink` · 워크트리 `.claude/worktrees/r17-otel-autolink`, 브랜치 `feat/r17-otel-autolink`
리뷰 페르소나 패널(6): `persona-credential-at-rest-and-transit.md`, `persona-design-contract-auditor.md`, `persona-automated-retry-budget-burn.md`(신규), `persona-claimed-vs-verified.md`, `persona-screen-state-honesty.md`, `persona-zero-base-redesigner.md`(발산형)
정본: malgnai-public `docs/design/device-ingest-key-provisioning.md` 16~243행 "앱 측 복사용 계약 요약"(개정 3.1) A~H
리스크 범주: 크리덴셜 발급(mhd_로 mhk_ 발급·회전) · 사용자 설정 파일(`~/.claude/settings.json`) 쓰기 · 인증
등급/모드: Sensitive, 최초 리뷰(풀패널). 노출 범위 축소 미적용(위임 메시지에 축소 근거 3값 없음 → 풀패널). 직전 관련 리뷰 `review-r16-otel-link-pre-contract-2026-10-15-r2.md`(Green, RV-005 이월)는 다른 target_id라 이번 건은 최초 리뷰다.
리뷰 일자: 2026-10-15
종합 판정: 🟡 **Amber** (Critical 0 · Major 2 · Minor 6 · Nit 6 · Rethink 2)

## 요약 (2분 규칙)
계약 정합과 보안 경계는 단단하다. 요청 측(경로 정확일치, Bearer 하나, 쿼리 없음, 타임아웃 35초, 리다이렉트 차단)과 D-1~D-15 분기는 실제 `reqwest` 클라이언트가 127.0.0.1 목 서버와 왕복하는 테스트로 고정돼 있다. mhd_가 설정 파일·백업·상태·Debug로 새는 경로는 찾지 못했다. RV-005·N-5·N-6은 해소됐다.
출시를 막는 것은 화면 쪽 2건이다. **RV-01**: 웹에서 폐기된 키가 settings.json에 남아 있는 일반적인 confirm_required 상황에서, 홈과 설정 화면이 "적용됨" 또는 "수신 확인됨 ✓"을 보여 준다. 거절한 뒤에는 다시 시도할 버튼도 없다. **RV-02**: 설정 화면은 "직접 받은 수집키를 붙여넣을 수도 있다"고 안내한다. 그러나 허브에 연결돼 있으면 엔진이 1시간 안에 그 키를 자동 키로 덮어쓰고, 그때마다 회전 예산을 쓴다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| credential-at-rest-and-transit | 재사용 | 키가 디스크에 남는 자리와 키가 실려 나가는 목적지(C2·C4)가 이번 표면 그대로다 |
| design-contract-auditor | 재사용 | 설계 정본(계약 A~H)을 조항 단위로 대조하는 역할개념 그대로다 |
| claimed-vs-verified | 재사용 | 테스트가 빈 깡통인지, "성공" 문구의 근거가 관측인지를 본다 |
| screen-state-honesty | 재사용 | 체크리스트·패널이 상태를 정직하게 말하는지를 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 자체를 의심하는 역할개념 그대로다 |
| automated-retry-budget-burn | **신규** | INDEX에 "클라이언트 자동 루프가 서버 유한 자원(회전 예산·레이트리밋·요청 수)을 소비하는 양"을 묻는 행이 없다. 가장 가까운 `tauri-runtime-thread-and-lock-discipline`은 락·블로킹, `async-response-ordering`은 늦은 응답, `background-event-vs-foreground-work`는 사용자 작업 파괴를 본다. 이번 위임의 "비용·남용" 항목이 새 리스크 표면이다 |

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `cd src-tauri && cargo test` | 916 passed / 0 failed / 23 ignored |
| `pnpm exec tsc --noEmit` | 통과 |
| `pnpm run test:ts` (onboardingOtelLink 등) | 33 pass / 0 fail |
| `pnpm build` | 통과 |
| 캡처 `docs/reviewer/r17-otel-autolink-01~16-*.png` | 16장 전부 Read로 열람 |
| 외부 사실 대조 | Claude Code 공식 문서 `code.claude.com/docs/en/monitoring-usage`(WebFetch, 2026-10-15). traces(beta)·신호별 PROTOCOL 변수·`otelHeadersHelper`가 실재하고, 저장소 `.claude/settings.json`의 OTEL exporter 변수는 무시된다 |
| 라이브 호출·실제 `~/.claude/settings.json` 변경 | **하지 않았다.** `scripts/live-get-ingest-credential.sh`와 `#[ignore]` 라이브 GET 테스트도 실행하지 않았다 |
| 실제 Tauri 런타임(스케줄러 스레드·`otel-link-changed` 이벤트·키체인 재읽기·`live()` 조립) | **미검증.** 앱을 기동하지 않았다. 아래 판정은 코드와 단위 테스트에 근거한다 |

## 직전 이월 이슈 해소 확인 (hub issue `01m4ytpk0e0aqqm1ytg7e0gjp4`)
| ID | 해소 | 근거 |
|---|---|---|
| RV-005 (앱 기본값 혼입·범용 엔드포인트 미확인) | **해소** | `otel_settings.rs:575-610` `collection_facts`가 파일 값만 쓴다. 신호별 값 → 범용 `OTEL_EXPORTER_OTLP_ENDPOINT` 순으로 유효값을 정하고, 범용 값도 `all_present`(`:586`)로 목적지 검사에 넣는다. `read_generic_endpoint`(`:615`)는 읽기 전용이다. 테스트 `otel_link_engine/tests.rs:1170-1174`가 "범용이 허브 밖"과 "엔드포인트 전부 없음"이 create·rotate 양쪽에서 키 생성 0회, 파일 불변임을 단언한다. 남은 변형 하나는 RV-04에 적었다 |
| N-6 (텔레메트리 꺼짐 단정) | **해소** | `onboarding.ts` `BLOCKER_TEXT`의 "설정 파일에 텔레메트리 켜기 설정이 없음" |
| N-5 (발급 직후 수집 불가) | **해소(결정대로)** | 201 `env`는 쓰지 않는다. 대신 `mint_gate`(`otel_link.rs:439-451`)가 준비 안 된 파일에서 `NotReady`를 내고 발급을 멈춘다. 첫 실행 자동 세팅 뒤 `otel_link_recheck`가 이어 간다(`settings.ts` `ensureOtelAutoConfigured`). 캡처 14 확인 |
| R-4 키체인 사본 | 미채택(결정대로) | 머리말 근거(`otel_link.rs:36-40`)를 확인했다. 대안 구조는 RT-1에 적었다 |

## 계약 대조표 (A·D·E 핵심)
| 조항 | 코드 | 테스트 | 판정 |
|---|---|---|---|
| A 경로·Bearer 1개·쿼리 없음·본문 JSON | `otel_hub_client.rs:27-28, 276-350` | `:484-537`, 엔진 HTTP 시나리오 `otel_link_engine/tests.rs:1309-1340`(매 시나리오 헤더·URL 단언) | 충족 |
| 타임아웃 ≥30초 | `REQUEST_TIMEOUT=35s`(`:31`). 테스트용 단축은 `#[cfg(test)]`(`:252`)에만 있다 | `:540` | 충족 |
| D-1 auto/confirm/blocked/has_current | `otel_link_engine.rs:376-385` | `tests.rs:38-124` | 충족(화면은 RV-01) |
| D-2 Authorization만 교체, env 미적용 | `accept_key` `:657-680` → `apply_ingest_key` | `:139-190` | 충족 |
| D-3 POST 유실 → 409 → 4번 | `:547-554, 592-597` | `:212-253` | 충족 |
| D-4 prefix 비교 → rotate | `:429-447` | `:255-289` | 충족(동작 변화는 RV-02) |
| D-5 유실 재시도 1회·연속 유실 2회 정지·점검당 rotate ≤2·쓰기 실패 메모리 보관 | `:503-545, 406-427` | `:291-505` | 충족 |
| D-6 TOKEN_SUPERSEDED 재읽기 1회 | `:586-590, 694-700` | `:509-553` | 충족 |
| D-7a/b 401 → /self 확인, ALREADY_RENEWED | `:721-745` | `:555-666` | 충족 |
| D-8~D-11 | `:592-621` | `:668-827` | 충족 |
| D-12 10초 이상 폴링·15분 정지·**다음 기동 때 GET 1회** | `:798-868`, 재개 `:432-443` | `:840-955` | **부분 이탈(RV-03)** |
| D-13 DELETE 뒤 Authorization 삭제 | `hub_auth/mod.rs:160-175`, `otel_link.rs:638-642` | `otel_link/tests.rs:846` | 순서 충족, 경합 있음(RV-05) |
| D-14 RATE_LIMITED 대기·ROTATE_BUDGET 보류(사용자 재시도로도 해제 안 됨) | `:613-619, 724-729, 322-328` | `:974-1034` | 충족 |
| D-15 1분→×2→1시간 | `:749-759` | `:1036-1083` | 충족 |
| E label | `otel_link.rs:475-499` | `otel_link/tests.rs` | 충족 |

## 지적 사항 (통합)
| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-01 | 🟠 Major | 화면 정직성 / 계약 D-1 | `src-tauri/src/otel_link.rs:391-405`(`compose_status`가 ConfirmRequired·ConfirmDeclined에서 플래그만 세움) · `src/onboarding.ts:248-277`(`appliedStep`이 `confirmRequired`/`confirmDeclined`를 보지 않음) · 테스트 `scripts/tests/onboardingOtelLink.test.mjs:89,159,183,220` · 하네스 `scripts/ui-harness/flows/otelAutolink.mjs:293,322,339` | `compose_status`와 `appliedStep`의 분기를 나란히 읽었다. 테스트와 하네스에서 `confirmRequired`를 grep했고, 전부 `unlinked` 상태와의 조합뿐이었다 | confirm_required는 "이 기기 키가 웹에서 수동 폐기됨"이다. 앱은 폐기된 키를 settings.json에서 지우지 않으므로, 현실에서 흔한 경우는 **파일에 폐기된 mhk_가 남은 상태**다. 그때 파일 도출 상태는 `applied`이고 엔진은 `ConfirmRequired`인데, `appliedStep`은 이 플래그를 무시한다. 그래서 ① "수집키가 적용되었습니다 (mhk_…). 수신은 아직 확인하지 않았습니다"를 보여 준다. ② 같은 실행에서 직전에 첫 데이터가 확인됐다면 `confirmed_prefix`가 그대로 남아 **"수신 확인됨 ✓"(done)**로 표시된다. ③ 자동 팝업(RV-08)을 거절하면 applied 행에는 버튼이 없어 다시 발급할 길이 없다. 앱을 재시작하거나 키를 직접 삭제해야 한다. 캡처 05는 파일에 키가 없는 덜 흔한 경우만 보여 준다 | `appliedStep` 앞에서 `confirmRequired`/`confirmDeclined`를 먼저 처리한다. 문구는 "적용된 키(mhk_…)가 웹에서 폐기되어 수집되지 않습니다"로, 버튼은 confirm-reissue/retry로 한다. 엔진은 ConfirmRequired에 들어갈 때 `confirmed_prefix`와 `watch`를 비운다. 테스트에 `applied + confirmRequired`, `confirmed + confirmRequired`, `applied + confirmDeclined`를 추가한다 |
| RV-02 | 🟠 Major | 화면 정직성 / 예산 | `src/views/settings.ts:113`(INGEST_KEY_COPY.issue) · `src/onboarding.ts:211` · 엔진 `otel_link_engine.rs:377`(auto → POST), `:429-447`(prefix 불일치 → rotate) | 문구와 엔진 분기를 대조했다. 테스트 `d4_different_prefix_rotates_without_judging_who_rotated`(`tests.rs:266`)가 이 동작을 고정한다 | 허브에 연결된 사용자가 웹에서 받은 키를 설정 > OTel에 붙여넣고 저장하면, 다음 점검(기동·1시간 간격)에서 엔진이 키를 교체한다. issue_policy가 auto면 POST로, has_current면 prefix 불일치로 rotate해서 덮어쓴다. 화면은 "붙여넣고 저장할 수도 있습니다"라고만 말한다. 붙여넣을 때마다 회전 예산을 1개 쓴다. 반복하면 429 `ROTATE_BUDGET_EXCEEDED`의 "다른 곳에서 쓰이고 있을 수 있으니…" 경고로 이어질 수 있다. 덮어쓰인 웹 키는 살아 있으므로 키 상한 5개(D-8)도 함께 소모한다. 개발자가 짚은 리뷰 포인트와 같은 지점이다 | 최소(저비용): 허브 연결 중이면 수동 입력칸을 비활성화하거나, "허브에 연결된 동안은 앱이 자동 수집키로 교체합니다. 직접 받은 키를 쓰려면 허브 연결을 해제하세요"로 바꾼다. `onboarding.ts:211`의 미연결 문구는 유지해도 된다. 근본 대안은 RT-2 |
| RV-03 | 🟡 Minor | 계약 D-12 / 자원 | `otel_link_engine.rs:432-443`(has_current·prefix 일치·신호 없음 → `first_data_timed_out=false` + 새 watch), `:276`(1시간 Periodic) · 문구 `src/onboarding.ts:273` | `due` → `run_check` → `has_current` 경로를 추적했다. `d12_*` 테스트는 기동 재감시만 단언하고 Periodic 재감시는 단언하지 않는다 | 계약은 15분 타임아웃 뒤 "다음 앱 기동 때 GET 1회로 다시 확인"이다. 코드는 **매시간** 정기 점검에서 15분 폴링을 다시 시작한다. 신호가 한 번도 없는 키(설치만 하고 Claude Code를 쓰지 않는 PC) 1대당 시간당 1+60회, **하루 약 1,464회 GET**이다. 레이트리밋 안이고 회전은 없어서 Minor로 둔다. 화면은 매시간 "15분 안에 수신이 확인되지 않았습니다 → 첫 데이터 수신을 확인하는 중"을 오가는데, 문구는 "다음 앱 시작 때 다시 확인합니다"라고 말한다 | `first_data_timed_out`이 켜진 같은 prefix에는 Periodic에서 watch를 다시 시작하지 않는다(GET 1회 확인만 한다). 재감시는 Startup·Connected·새 키 적용 때만 한다. Periodic 회귀 테스트를 1건 추가한다 |
| RV-04 | 🟡 Minor | 보안(RV-005 변형) | `src-tauri/src/otel_settings.rs:575-610`(`collection_facts`: 엔드포인트 3키·프로토콜 1키만 봄) · `otel_link.rs:444` | 함수와 Claude Code 공식 문서를 대조했다. 문서는 traces beta(`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1` + `OTEL_TRACES_EXPORTER=otlp`, `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`), 신호별 `OTEL_EXPORTER_OTLP_{METRICS,LOGS}_PROTOCOL`을 명시한다. src에서 `TRACES`를 grep했고 0건이었다 | ① 파일에 traces beta가 켜져 있고 traces 엔드포인트가 허브 밖이면 `mint_gate`가 통과한다. 키는 범용 `OTEL_EXPORTER_OTLP_HEADERS`에 실리므로 traces exporter를 따라 그 주소로 나간다. 사용자가 손으로 만든 설정에서만 나타나고 mhk_는 쓰기 전용이라 Minor다. ② 범용 PROTOCOL 없이 신호별 PROTOCOL만 `http/protobuf`로 둔 파일은 `NotReady`로 잘못 판정해 발급을 막는다. 반대로 범용은 http/protobuf인데 신호별이 grpc인 파일은 준비됨으로 본다 | `all_present`에 `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`를 넣는다(읽기 전용). 프로토콜은 신호별 값 → 범용 값 순으로 metrics·logs 각각의 유효값을 평가한다. 테스트 2행을 추가한다 |
| RV-05 | 🟡 Minor | 동시성 / 계약 D-13 | `src-tauri/src/hub_auth/mod.rs:160-175` · `otel_link.rs:638-642` · `otel_link_engine.rs:291-299`(`on_disconnected`가 `run` 락을 잡지 않음), `:312-334, 657-680` | 해제 경로와 점검 경로의 락을 대조했다. 해제는 hub_auth 파일 락을 잡고, 엔진은 `run` 뮤텍스를 잡는다. 둘은 서로를 막지 않는다 | 스케줄러가 rotate·POST를 보내 응답을 기다리는 동안(최대 35초) 사용자가 연결을 해제하면, ① DELETE와 키 삭제가 먼저 끝나고 ② 뒤늦게 201이 도착해 `accept_key`가 키를 settings.json에 다시 쓴다. ③ `finish`가 `NoDeviceToken`을 Idle로 덮는다. 서버가 해제 때 기기 키를 함께 폐기하므로 살아 있는 키가 남지는 않는다. 그래도 "해제 후 키를 지웠다"는 상태와 파일이 어긋나고, 화면은 허브 미연결인데 "적용됨"으로 남는다 | `after_hub_disconnect`가 엔진 `run` 락을 잡은 뒤(또는 세대 번호를 올린 뒤) 키를 지운다. `accept_key`·`finish`는 세대가 바뀌었으면 쓰지 않는다 |
| RV-06 | 🟡 Minor | claimed vs verified | `src/views/settings.ts:216`, `:1182` | 토스트 조건을 읽었다 | ① 재발급 동의 뒤 결과 상태(NotReady·EndpointNotHub·CredentialLimit·try_lock 실패로 아무것도 안 함)와 무관하게 "수집키를 다시 발급해 적용했습니다"를 띄운다. ② 해제 토스트 "이 PC의 수집키도 지웠습니다"는 `after_hub_disconnect` 결과를 버린 뒤(`hub_auth/mod.rs:168-170` `let _ =`) 나간다. 키가 원래 없었거나 쓰기에 실패해도 같은 문구다 | ① 응답 `status.state`가 applied/confirmed일 때만 성공 문구를 띄우고, 그 밖에는 `failure.message`를 보여 준다. ② 해제 커맨드가 제거 여부를 상태에 실어 문구를 나눈다 |
| RV-07 | 🟡 Minor | 화면 정직성 | `otel_link.rs:400-416`(Transient·RateLimited·FeatureUnavailable 등이 applied/confirmed를 `failed`로 덮음) · `src/onboarding.ts:330-332` | 합성 분기를 읽었다. 캡처 15는 키가 없는 경우만 보여 준다 | 키가 적용돼 정상 수집 중인 PC도 1시간 점검의 GET이 한 번 끊기면 설정 배지가 "실패"가 되고 체크리스트가 `!`로 바뀐다. 다음 성공 점검까지(1분 이상) 그대로다. 또 `featureUnavailable`은 키가 적용돼 있어도 "사용량 수집키가 적용되어 있지 않습니다"라고 말한다 | 일시 원인(unreachable·rateLimited·featureUnavailable)은 파일 상태가 applied/confirmed면 상태를 유지하고 보조 문구만 붙인다 |
| RV-08 | 🟡 Minor | 화면 정직성 / 백그라운드 | `src/views/settings.ts:168,188,199,225-227`(상태를 다시 읽을 때마다 자동 팝업), `:213` · 캡처 05(초점이 "다시 발급"에 있음) | 호출 지점과 캡처를 확인했다 | 1시간 점검이 confirm_required를 만나면 `otel-link-changed` 이벤트 → `loadIngestLink` → 모달이 **어느 화면에서든** 뜬다. 기본 초점이 "다시 발급"이라, 세션 입력 중 Enter를 치면 사람 확인(D-1)이 우발적으로 통과된다. 재발급 키는 같은 기기로 가므로 피해는 작다 | 자동 팝업 대신 체크리스트·설정 행의 [확인하기]로만 열거나, 기본 초점을 "발급하지 않음"이나 닫기에 둔다 |
| N-1 | ⚪ Nit | 파일 쓰기 | `otel_settings.rs` `save_to_settings_file`(프로세스 내 뮤텍스) | 코드 열람 | 이제 사용자 동작 없이도 백그라운드에서 settings.json을 다시 쓴다(키 정렬 정규화 포함). 뮤텍스는 앱 프로세스 안만 직렬화하므로, Claude Code나 편집기의 동시 쓰기와는 lost update가 날 수 있다. 쓰기는 새 키가 생길 때만이라 빈도가 낮다 | 설정 화면 머리말에 "앱이 수집키를 자동으로 쓸 때도 정규화된다"를 한 줄 덧붙인다 |
| N-2 | ⚪ Nit | 동시성 | `otel_link.rs:656-663`, `otel_link_engine.rs:314` | 코드 열람 | 폴링 GET이 `run` 락을 잡고 있을 때 [다시 시도]를 누르면 조용히 아무것도 하지 않는다. 연속 유실 수도 비우지 않는다 | 락을 못 잡으면 `forced=User`를 예약하고 돌려준다 |
| N-3 | ⚪ Nit | 문구 | `src/views/settings.ts:118,120` | 계약 H와 대조 | "유출이 의심되면 허브 관리자에게 폐기 요청"이라고 안내하지만, 계약 H는 웹 `/keys` 셀프 폐기를 전제한다. 체크리스트의 CredentialLimit 문구는 이미 "/keys"라고 말해 두 문구가 갈린다 | "허브 웹 /keys에서 폐기(또는 관리자에게 요청)"로 맞춘다 |
| N-4 | ⚪ Nit | UX | `src/onboarding.ts:285` | 코드 열람 | `featureUnavailable`의 버튼이 "OTel 설정 열기"다. 설정에서 할 일이 없다 | 버튼을 없앤다 |
| N-5 | ⚪ Nit | 계약 D-14 | `otel_link_engine.rs:614` | 코드 열람 | `Retry-After`가 없으면 `retry_at`이 있어도 1시간 고정 대기를 쓴다(계약상 서버는 두 값을 함께 보낸다) | `retry_at`으로 남은 시간을 계산하는 폴백을 둔다 |
| N-6 | ⚪ Nit | 계약 D-11 | `otel_link_engine.rs:276`(Periodic) · `:723,731` | 코드 열람 | NotDesktopToken·HubRejected 뒤에도 1시간마다 GET을 다시 보낸다. 발급·회전은 하지 않으므로 무해하다 | 이번 실행 동안 정지(사용자 재시도만 허용)를 고려한다 |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 보안 | 저장소의 `.claude/settings.json`이 범용 엔드포인트를 덮어 user 설정의 mhk_가 악성 저장소 주소로 갈 수 있다 | 기각 | Claude Code 공식 문서가 "저장소 `.claude/settings.json`·`settings.local.json`의 OpenTelemetry exporter 변수는 무시한다"고 명시한다(WebFetch 대조) |
| 보안 | `libc::gethostname` unsafe 오버런 | 기각 | `otel_link.rs:503-512`가 `len-1`을 넘기므로 마지막 바이트는 항상 0이고, NUL을 찾지 못하면 `len-1`까지만 읽는다. 실패 시 빈 label(계약 E-2)로 간다 |
| 보안 | 테스트가 라이브 허브로 POST를 보낼 수 있다 | 기각 | 테스트 쪽의 `ReqwestIngestApi::new`는 전부 127.0.0.1 목 서버 origin이다(grep: `otel_hub_client.rs`, `otel_link_engine/tests.rs:1313,1444,1491`). 실제 의존성 `live()`는 `hub_auth::current_ctx()`가 필요한데, 테스트에서 `hub_auth::init`·`with_env`·`otel_link_retry` 호출은 0건이다(grep). 라이브 코드는 `#[ignore]` GET 1건뿐이다(`otel_link/tests.rs:964`) |
| 보안 | `scripts/live-get-ingest-credential.sh`가 토큰을 노출한다 | 기각 | `set +x`, 토큰은 변수에만 있고 curl 설정은 stdin(`-K -`)으로 넘긴다. GET만 쓴다(`-X GET`). 응답 값은 열거형 외에는 길이만 출력하고 본문은 `mktemp -d`에 두었다가 trap으로 지운다. 실행은 하지 않고 코드만 대조했다 |
| 보안 | capabilities 표면 확대 | 기각 | `capabilities/default.json`·`Cargo.toml` diff 0줄이다. 새 커맨드 3개는 기존 커스텀 Rust 커맨드 패턴이고 fs/shell 플러그인은 없다 |
| 보안 | 오류·이벤트·Debug로 mhd_/mhk_가 샌다 | 기각 | 이벤트 payload는 `()`다(`otel_link.rs:618`). `HttpError`는 서버 `message`를 버리고 code는 모양 검사를 거친다(`otel_hub_client.rs:205-237`). `Created`·`pending_write`는 `SecretString`이다. 새 Rust 파일에서 `println|eprintln|log::|tracing::|dbg!`를 grep했고 0건이었다. 테스트 `server_echoed_tokens_*`(`:679`)·`device_token_never_reaches_*`(`tests.rs:1238`)가 이를 고정한다 |
| 예산 | [다시 시도] 연타로 회전 예산을 태운다 | 기각 | 프론트에 `retrying` 가드(`settings.ts` `retryIngestLinkNow`), 백엔드에 `run.try_lock`이 있다. 한 번 클릭할 때 rotate는 최대 2회다(`MAX_ROTATES_PER_CHECK`, 계약 D-5가 허용하는 해제). 예산이 소진된 상태(RotateBudget)에는 버튼이 없다(`FAILURE_ACTION.rotateBudgetExceeded=null`). User 트리거도 `budget_hold`를 비우지 않는다(`otel_link_engine.rs:322-328`). 서버 예산(24시간 6개)이 최종 상한이다 |
| 예산 | 10초 tick이 실패 상태에서 매번 요청한다 | 기각 | `due()`(`:262-278`)는 `last_check`/`next_retry_at` 기준이라, 실패 뒤에는 백오프나 1시간 간격까지 None이다. tick마다 요청하는 것은 watch 폴링(15초 간격)뿐이다 |
| 예산 | 201에 형식이 틀린 키가 오면 매시간 rotate가 반복된다 | 기각 | 서버가 계약을 어겨야만 성립하는 추정이다. 재현 경로가 없다. 서버 예산 429로도 막힌다 |

## 페르소나별 관점
### 자격증명 보관·전송 경로 감사자 — 🟢 Green(Minor 1)
- mhd_는 `DeviceTokens::latest()`의 `SecretString`이고 `IngestApi` 호출에만 쓰인다. 헤더에 싣기 전에 `is_valid_token`으로 CRLF 주입을 막는다(`otel_hub_client.rs:283`, 테스트 `:741`). 리다이렉트는 따라가지 않는다(`:261`, 테스트 `:697`).
- 목적지 검사는 파일 실물 기준으로 바뀌어 RV-005를 해소했다. 남은 변형은 traces 엔드포인트다(RV-04).
- 해제 시 백업에서도 키를 뺀다(Clear 경로). 원자적 쓰기와 0600은 R12 엔진을 그대로 쓴다.

### 설계 계약 감사자 — 🟡 Amber
- 위 대조표처럼 D-1~D-15는 코드와 테스트가 1:1로 대응한다. 엔진 HTTP 시나리오가 C표 상태코드를 실제 reqwest로 돌리며 요청 측까지 단언한다. 테스트는 빈 깡통이 아니다.
- 이탈: D-12 재확인 주기(RV-03). D-13 경합(RV-05). D-1이 파일에 키가 남은 경우 화면에서 깨진다(RV-01).

### 자동 재시도 예산 소진 감시자 (신규) — 🟢 Green(Minor 1)
- 최악의 하루 계산: ① 응답 유실만으로 한 실행에서 자동 rotate ≤3(2+1, `tests.rs:316`). ② rotate가 계속 500이면 백오프 1·2·4·8·16·32분 다음 60분 → 하루 약 29회 시도다. 500에서 키가 생성되지 않으면 예산 소비는 0이고, 생성되더라도 서버 예산 6에서 429로 멈춘다. ③ RATE_LIMITED는 `Retry-After`를 따른다. ④ 신호 없는 키의 GET은 약 1,464회/일이다(RV-03).
- 타임아웃 35초는 하한 30초 이상이다. 연결 실패(NotSent)는 재시도하지 않고 백오프로 간다(`tests.rs:304`). 유실과 미전송의 구분이 정확하다.
- RV-02(수동 키 → 교체 회전)가 사람 행동 하나로 예산을 쓰는 유일한 경로다.

### claimed vs verified — 🟡 Amber
- 테스트 신뢰도가 높다. 가짜 시계, 실제 HTTP 왕복, 파일·백업 불변 단언, mhd_ 미노출 단언이 있다.
- 문구가 관측보다 앞서는 곳이 두 군데다(RV-06). 실제 Tauri 런타임(스케줄러 스레드, 이벤트 송신, 키체인 재읽기, `spawn_blocking` 안 blocking 클라이언트 생성)은 단위 테스트 밖이고 이번 리뷰에서도 검증하지 못했다. 출시 전 개발 빌드로 "연결 → 8~18초 안에 발급 → 이벤트로 홈 갱신"을 1회 관측하기를 권한다.

### 화면 상태 정직성 — 🟠 Amber
- 캡처 16장을 열람했다. 상태별 문구·버튼·예산 소진 시각 표시(07)·해제 확인창(08)·slim 배너(16)가 일관되다. 해제 확인창은 서버 폐기와 settings.json 삭제를 모두 밝힌다.
- RV-01(폐기된 키가 남은 채 "적용됨/수신 확인됨"), RV-07(일시 오류가 정상 상태를 "실패"로 덮음), RV-08(어느 화면에서든 뜨는 모달)이 남는다.

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵
| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-1 | mhk_ 원문을 `settings.json`의 `env.OTEL_EXPORTER_OTLP_HEADERS`에 평문으로 쓰고, prefix가 다르면 회전한다(R-4 미채택) | `settings.json`에는 `"otelHeadersHelper": "<앱 exe> --otel-ingest-header"`만 쓴다. 원문은 키체인에 1부만 둔다(R-4). 헬퍼는 이미 있는 `--hub-auth-header` CLI 모드와 같은 방식으로 키체인에서 읽어 `{"Authorization":"Bearer mhk_…"}`를 출력한다. Claude Code 공식 문서상 헬퍼는 기동 때와 29분마다 다시 실행되고, http/protobuf에만 적용된다(이 앱의 준비 조건과 같다) | ① settings.json·백업에 비밀이 없어진다. ② settings.json 동기화로 생기는 PC 간 회전 핑퐁(D-14 양성 원인)이 사라진다. ③ 쓰기 실패 → 메모리 보관 → 재기동 시 회전(D-5) 경로가 사라진다. ④ 회전한 키가 실행 중인 세션에 최대 29분 안에 반영되어 "세션 재시작" 안내가 필요 없다. ⑤ env 변수로 노출되지 않아 "Claude Code가 실행하는 명령에서 보일 수 있습니다" 경고 표면이 준다. ⑥ RV-02·RV-05가 구조적으로 없어진다(파일에 키가 없음) | 중간. CLI 모드 1개와 헬퍼 경로 쓰기, 키체인 항목 1개를 더한다. Windows 경로 인용과 헬퍼 실패 시 수집이 0이 되는 점(문서 명시)을 검증해야 한다. 헬퍼 존재는 문서로 확인했고, 앱 바이너리를 헬퍼로 쓸 때의 동작은 미검증이다 |
| RT-2 | 수동 입력과 자동 발급이 같은 Authorization 칸을 두고 경쟁한다(RV-02) | "키 소유 모드"를 명시적 상태로 둔다. 허브 연결 = 자동 관리(수동 입력 숨김, "자동 관리 중 mhk_AbCd…"만 표시), 미연결 = 수동. 엔진은 자동 모드에서만 D-4 회전을 한다 | 화면 약속과 엔진 동작이 한 출처에서 나온다. 사용자가 붙여넣은 키를 몰래 교체하는 일이 없다 | 낮음. `hubConnected` 플래그가 이미 상태에 있다(`otel_link.rs:259`) |

## 트레이드오프 (페르소나 간 충돌)
- **설계 계약 감사자 vs 화면 정직성 (RV-08)**: 계약 D-1은 "사람에게 묻는다"만 요구하므로 자동 팝업은 계약을 빠르게 충족한다. 화면 관점에서는 다른 작업 중에 모달이 끼어드는 문제다. → 권고: 팝업은 홈·설정 화면에 있을 때만 띄우고, 다른 화면에서는 체크리스트 slim 배너로 알린다. 초점은 거절 쪽에 둔다.
- **자격증명 감사자 vs 예산 감시자 (RT-1/R-4)**: 키체인 사본은 비밀 보관처를 하나 늘린다(감사자). 대신 회전의 양성 원인을 없앤다(예산). → 권고: RT-1처럼 settings.json에서 원문을 빼는 방식이라면 보관처 수가 그대로(1곳)이므로 두 관점이 모두 만족한다. 차기 라운드 설계 결정으로 올린다.

## 잘 된 점
- 엔진이 시계를 읽지 않고 `now`를 주입받아(`otel_link_engine.rs:9-11`) 백오프·폴링·예산 보류를 가짜 시계로 검증한다. 다음 자동화 기능의 기준으로 삼을 만하다.
- 계약 C표를 테스트 표로 그대로 옮겼다(`otel_hub_client.rs:617-646`, `tests.rs` `status_code_table_*`). 엔진 HTTP 시나리오는 매 케이스마다 요청 측(메서드·경로·Bearer·쿼리 없음·본문에 mhd_ 없음)을 함께 단언한다.
- `ApiError::{NotSent, Lost}`로 "서버에 닿았는가"를 타입으로 나눴다. D-5 재시도의 정확성이 이 구분에 달려 있고, 타임아웃·연결 끊김·연결 거부 각각을 테스트로 고정했다(`otel_hub_client.rs:711-738`).
- 실패 원인을 데이터 없는 enum으로 두고 고정 문구만 내보내 응답·파일 값이 화면에 섞이지 않는다(`otel_link.rs:79-167`).
- 선검사(`mint_gate`)가 서버 호출 전에 멈추고, 파일·백업 불변까지 단언한다.
- 해제 확인창이 서버 폐기와 settings.json 삭제라는 결과를 모두 밝힌다(캡처 08).

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| C1 0600·원자적 쓰기 | 자격증명 | 필수 | ✓ | R12 엔진 재사용, 뮤텍스 추가 |
| C2 원문이 경계를 넘지 않음 | 자격증명 | 필수 | ✓ | grep + 테스트 |
| C3 키 교체는 명시적 동작으로만 | 자격증명 | 필수 | △ | 자동 회전은 계약상 의도된 경로다. 수동 키 교체 고지는 미흡(RV-02) |
| C4 목적지 통제 | 자격증명 | 권장 | △ | traces 변형(RV-04) |
| B1 소비 호출 상한 | 예산 | 필수 | ✓ | 점검당 2회, 실행당 3개 |
| B2 타임아웃 하한·유실 판정 | 예산 | 필수 | ✓ | 35초, NotSent/Lost |
| B3 서버 대기 존중 | 예산 | 필수 | ✓ | Retry-After, budget_hold |
| B5 실패 상태 반복 억제 | 예산 | 권장 | △ | GET 재감시(RV-03) |
| 계약 D-1~D-15 조항 실재 | 계약 | 필수 | ✓(D-12 부분) | 대조표 |
| 상태 표시 정직성 | 화면 | 필수 | ✗ | RV-01 |

## 생략·미검증 명시
- 실제 Tauri 런타임 동작(스케줄러·이벤트·키체인 재읽기)은 앱을 기동하지 않아 확인하지 못했다. 근거는 코드와 단위 테스트다.
- Windows 경로(`COMPUTERNAME` label, 설정 파일 권한)는 코드 열람만 했다.
- `scripts/ui-harness/flows/otelAutolink.mjs` 427줄은 confirm 관련 픽스처만 grep했다. 정독하지 않았다.
- 서버 측 동작(예산·쿼터 cron)은 계약 문서 기준이다. 허브 코드는 열지 않았다.

## PM에게 권고
1. **Amber — 조건부 통과.** 출시 전에 RV-01·RV-02를 고친다. 둘 다 프론트 분기·문구와 테스트 몇 행 수준이다(근거: `appliedStep`·`compose_status`에 플래그가 이미 있고, `hubConnected`도 상태에 있다). 백엔드 계약 로직은 바꿀 필요가 없다.
2. Minor 중 RV-04(traces 목적지)·RV-05(해제 경합)는 보안 경계라 같은 라운드에 같이 처리하기를 권한다. 둘 다 함수 하나와 테스트 몇 행이다. RV-03·RV-06~08은 백로그로 둬도 된다.
3. 출시 전 개발 빌드로 런타임 1회 관측을 남긴다. 대상은 연결 → 자동 발급 → 이벤트 갱신 → 15분 폴링 시작, 그리고 해제 → 키 삭제다(라이브 허브 호출이 포함되므로 사람 승인 범위에서 한다).
4. RT-1(otelHeadersHelper + 키체인 1부)은 차기 설계 결정 후보로 올린다. 채택하면 RV-02·RV-05와 D-5 쓰기 실패 경로가 구조적으로 사라진다.
5. 재검토는 같은 target_id `r17-otel-autolink`로 위임한다. 바뀌는 곳이 onboarding/settings 분기와 테스트뿐이면 축소 모드 대상이다.

실행 액션: 커밋·push·라이브 호출·실제 settings.json 변경은 하지 않았다. 작성·수정한 파일은 이 보고서, 신규 페르소나 1개, 기존 페르소나 5개의 "적용 이력" 추가, `personas/INDEX.md` 갱신뿐이다(모두 미커밋).
