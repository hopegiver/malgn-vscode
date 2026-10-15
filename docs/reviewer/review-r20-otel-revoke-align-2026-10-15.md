# R20 웹 폐기 대응 정합 — 리뷰 보고서

리뷰 대상: `/Users/hopegiver/workspace/malgn-vscode` main 커밋 범위 `bbc77d5..3bbfb45` · target_id `r20-otel-revoke-align`
포함 커밋: r20-revoke-align(9afac01, 468dc9e, bb96971, a2ed625, a837054) + r20-devtools-nits(fc71968) + 병합 3bbfb45 (28파일, +1487/−192)
직전 리뷰: `review-r18-rv09-2026-10-15.md`(N-11·N-13·N-14·RT-4), `review-r19-otel-server-env-2026-10-15-r2.md`, `review-r14-install-chain-prereq-usage-empty-2026-10-15-r2.md`(RV-002/007/010/011)
리스크 범주: 인증/크리덴셜(mhd_로 mhk_ 자동 발급·폐기 대응, 사용자 설정 파일 연동)
등급/모드: Sensitive, 풀패널(발산형 포함). 위임이 축소 없는 풀패널을 명시했다. 노출 범위 축소는 적용하지 않았다(축소 근거 3값이 없다). 재검토 3요소가 모두 있지만 이번 범위는 새 커맨드 반환형(`IngestReissueAnswer`)과 새 엔진 상태 전이(Reconnect/Blocked에서 `revoked_prefix` 해제)를 더한다. 새 리스크 표면이 있으므로 축소 모드가 아니라 최초 리뷰 깊이로 다뤘다.
리뷰 일자: 2026-10-15
종합 판정: 🟢 **Green** (Critical 0 · Major 0 · Minor 3(신규 RV-001~003) · Nit 4(신규 RV-004~007) · Rethink 1(신규 RT-5))

## 요약 (2분 규칙)
앱의 폐기 대응 문구는 허브 513f20a(`utils.js:960-962`)와 계약 §D 1·7·10·13에 맞는다. 세 분기를 확인했다.
- 일반 폐기: "다시 발급"을 묻고 "다시 로그인할 필요는 없습니다"라고 말한다.
- 유출 의심(401 → self 401/ALREADY_RENEWED): "다시 로그인해 연결"을 말하고 "다시 발급"을 내놓지 않는다.
- self 200: 연결은 정상이고 아무것도 요구하지 않는다.

앱 안에 "웹에서 회전하라"는 반대 취지 문구는 없다. 사용자 문구 기준이다. 코드 주석 1곳이 낡았다(RV-006).

RT-4 재설계는 맞다. 엔진이 `ReissueResult`를 직접 돌려주고, 토스트는 그 값과 1:1로 대응한다. 응답에는 마스킹된 앞 8자와 고정 문구만 실린다. 변이 시험 13종 가운데 11종을 테스트가 잡았다.
- 살아남은 Rust 변이 1종: "쓰기 실패인데 Minted로 보고"(RV-004). 지금 코드는 맞지만, RT-4가 막으려던 거짓 성공 토스트가 이 자리에서 재발해도 알아챌 테스트가 없다.
- 살아남은 TS 변이 1종: 관리 안내 렌더 제거. 하네스 캡처 10이 덮는다.

남은 것은 화면 표시의 사실 정합 3건(Minor)이다.
1. 재연결 필요·차단 상태에서 이미 폐기됐을 가능성이 큰 파일 키를 "적용된 수집키", 초록 "설정됨"으로 보여 준다(RV-001). PM이 관찰한 05 화면이 이것이다.
2. `blocked/suspectedLeak`의 한 줄 요약이 "연결이 해제되었습니다"라고 말한다. 이 상태는 정의상 GET이 200으로 응답한 경우라 연결이 살아 있다(RV-002).
3. OTel이 "연결이 해제되었습니다"라고 말하는 동안 허브 연결 행은 "연결됨"을 계속 보여 준다(RV-003, R17부터 있던 간극).

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| design-contract-auditor | 재사용 | 검증 1(계약 §D 1·7·10·13, 허브 /keys 문구와 조항 단위 대조)이 이 역할개념 그대로다 |
| credential-at-rest-and-transit | 재사용 | RT-4 새 응답형에 키 원문이 실리지 않는지, 폐기된 키가 파일에 남는 자리를 본다 |
| claimed-vs-verified | 재사용 | 커밋 주장("1:1 대응", "분기 고정")을 테스트·변이로 확인한다 |
| screen-state-honesty | 재사용 | 캡처 01~10의 상태와 문구가 맞는지, 05의 "적용된" 표기를 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 의심 역할개념 그대로다. 🔵 RT-5를 냈다 |

생략한 관점과 사유:
- `tauri-runtime-thread-and-lock-discipline`: 새 락이 없다. `accept_confirm`은 기존 `run.try_lock`과 `mem`만 쓴다. `st.minted`는 지역 `RunState`다.
- `automated-retry-budget-burn`: 새 요청 경로가 없다. diff에서 api 호출 추가 0건이다.
- `remote-config-injection-boundary`: env 적용 경로는 0줄 변경이다.
- `heterogeneous-machine-operator`: devTools 문구 변경은 claimed-vs-verified와 screen-state-honesty로 묶었다.

**게이트 예외(정직 보고):** 위임이 "보고서 1개 작성만 허용"이라 재사용 페르소나 5개 파일의 "적용 이력" append와 INDEX.md "최근 재사용" 갱신을 하지 않았다. PM이 커밋할 때 이 5행에 `r20-otel-revoke-align / 1차(최초 깊이, 풀패널) / review-r20-otel-revoke-align-2026-10-15.md`를 덧붙여야 한다. 페르소나 파일 5개가 실재하는 것은 `ls docs/reviewer/personas/`로 확인했다.

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `cd src-tauri && cargo test` | **992 passed / 0 failed / 23 ignored**. PM 실측과 일치한다 |
| `pnpm test:ts` | **82 pass / 0 fail**. PM 실측과 일치한다 |
| `pnpm exec tsc --noEmit` | 통과(exit 0) |
| Rust 변이 7종 (스크래치 APFS 클론 `scratchpad/mut/src-tauri`, `.env` 제거, 저장소 무변경) | M1 `finish`의 Reconnect/Blocked 해제 비활성 → **2건 실패**(`tests_r20` pending_revoke…, confirmed_reissue_meets_dead…). M2 accept_key 진입 즉시 minted 설정 → **1건 실패**(stale 테스트). M3 `accept_confirm`이 minted 무시 → **4건 실패**. M4 `transient:false` 고정 → **1건 실패**. M5 **WriteFailed 분기에서만 minted 설정 → 전부 통과(생존, RV-004)**. M6 NoDeviceToken→NoConnection 매핑 제거 → **1건 실패**. M7 `after_applied` 해제 주석 처리(R18 N-14) → **2건 실패**(N-14 해소 확인) |
| TS 변이 6종 (스크래치 사본 `scratchpad/tsmut`) | T1 reissueToast reconnect 분기 제거 → **1건 실패**. T2 유출 의심 slim 분기 제거 → **1건 실패**. T3 N-11 `onFile` 무시 → **3건 실패**. T4 `classifyRun`의 hadPathBefore 무시(RV-010) → **1건 실패**. T5 설정 화면 관리 안내 렌더 제거 → **생존**(렌더 경로라 단위 테스트 범위 밖. 캡처 10이 근거). T6 stuck 안내 제거(RV-002) → **1건 실패** |
| 허브 저장소 대조(읽기만) | `malgnai-public` 513f20a: `app/assets/js/utils.js:960-962, 976-993`. 계약 `docs/design/device-ingest-key-provisioning.md:16-243`, 보강 `:531, :570, :1079-1080` |
| 캡처 열람 | 01, 02, 03, 04, 05, 06, 07, 08, 09를 Read로 봤다. 01/05/06/09는 같은 홈인데 OTel 행의 문구·아이콘·버튼이 각각 달라 서로 다른 상태임을 확인했다. 10은 08과 같은 관리 안내 블록이라 08로 대신 확인했고 따로 열지 않았다 |
| 실행하지 않은 것 | 라이브 허브 호출, 실제 Tauri 런타임, UI 하네스 재실행·재캡처, `pnpm build`, devTools 화면 캡처(이번 범위에 devTools 캡처가 없다. RV-007은 코드 대조만 했다) |
| 실행 액션 | 커밋·push·stash·checkout 없음. 저장소에서 바뀐 것은 이 보고서 1개뿐이다 |

## 검증 1 — 허브 /keys·계약과의 정합 (분기별)
| 분기 | 허브/계약 | 앱 (근거) | 판정 |
|---|---|---|---|
| 일반(manual) 폐기 → 재발급 | `utils.js:961` "앱에서 다시 발급해야", 계약 D-1 confirm_required | 확인창 `onboarding.ts:186` "앱에서 바로 발급하며 다시 로그인할 필요는 없습니다". 단계 문구 `:307-310, :320-323, :332-336`. 엔진 `tests_r20.rs:173-277`(GET confirm_required / POST 409 / rotate 404→GET 3경로 + 거절 뒤 [다시 시도]). 모두 `probe.count()==0`이라 재로그인 경로로 가지 않는다. 캡처 01·02·03·04 | ✅ |
| 유출 의심 → 401 → self 401 / ALREADY_RENEWED → 재로그인 | `utils.js:962` "연결도 함께 해제… 다시 로그인해 연결", 계약 D-7(a)(b), §3.5 D14 | `otel_link.rs:154` 문구, `reissueToast` reconnect 분기(`otelLinkText.ts:45`), FAILURE_SLIM(`onboarding.ts:403`). 엔진 3경로 + ALREADY_RENEWED 3경로(`tests_r20.rs:297-344`)이고, `assert_reconnect_needed`가 "다시 발급" 부재와 `accept_confirm` NotPending을 단언한다. 캡처 05·07·08 | ✅ (표시 잔여 RV-001, RV-003) |
| blocked/suspected_leak | 계약 `:570` "이중방어 — 계열 폐기 없이 기록되는 경로", `:1080` B4(계열은 살아 있고 GET은 blocked) | 상세 문구는 맞다("연결을 해제한 뒤 다시 로그인"). 한 줄 요약은 사실과 다르다 | ⚠️ RV-002 |
| blocked/quota | 계약 G 쿼터, D-1 blocked | `tests_r20.rs:360-373`이 유출 문구가 섞이지 않음을 단언한다 | ✅ |
| self 200 → 기능 없음 | 계약 D-7(a) 200 | `otel_link.rs:151` "허브 연결은 정상이며 1시간 뒤 다시 확인", 버튼 없음(`onboarding.ts:387`). 캡처 09 | ✅ (토스트 꼬리 RV-005) |
| 웹 회전 불가·앱이 관리 | `utils.js:960` "회전은 앱에서 하세요", 409 DEVICE_MANAGED | `DEVICE_KEY_MANAGED_NOTE`(`onboarding.ts:193-194`)를 managed일 때만 렌더한다(`settings.ts:695-696`). 캡처 08·10 | ✅ |
| 연결 해제(D-13) | 서버가 키를 함께 폐기 | 해제 확인 문구(`settings.ts:1289`)는 변경 없음, 정합 유지 | ✅ |
| 반대 취지 잔존 | — | 사용자 문구 grep(`src/*.ts src/views/*.ts src-tauri/src/*.rs`, 패턴 `회전|셀프서비스 화면이 아직|관리자에게…폐기를 요청|허브에 다시 연결하세요`) 결과 0건. 테스트 `r20 … "웹에서 회전하라"…없다`도 같은 범위를 고정한다. 주석 1곳만 낡았다 | ✅ (RV-006) |

## 검증 2 — R18 항목 처리
| ID | 처리 | 근거 |
|---|---|---|
| N-11 | ✅ 해소 | `onboarding.ts:299-300` `revokedWhat`이 `onFile`로 갈린다. 변이 T3에서 3건이 실패한다 |
| N-13 | ✅ 해소 | 하네스 `otelAutolink.mjs` r20 시나리오, 캡처 01·02("폐기됨 · 재확인 필요" 배지는 설정 패널에서만 렌더된다. 홈은 원래 tag를 그리지 않는다, `settings.ts:601`) |
| N-14 | ✅ 해소 | 변이 M7에서 2건이 실패한다(`tests_r20` n14 + 기존 rv09) |
| RT-4 | ✅ 채택, 적정 | 아래 |

**RT-4 상세**
- 대응: 엔진 `ReissueResult`(`otel_link_engine.rs:247-263`) → `reissue_outcome`(`otel_link.rs:235-253`) → 프론트 `reissueToast`(`otelLinkText.ts:28-50`) switch로 1:1 대응한다. `beforeMasked` 캡처는 삭제됐다(`settings.ts:226-246`).
- Minted 조건: Minted는 `apply_guarded`가 `Written`일 때만 나온다(`:846-850`). 세대 확인과 쓰기를 같은 gate 안에서 하므로 Stale이면 NoDeviceToken → NoConnection이 된다(테스트 `r20_accept_confirm_without_a_device_token…`, 변이 M2·M6에서 실패).
- 회귀: Busy/NotPending이 분리됐다. 재연결 필요 상태에서 accept는 NotPending이 되고 요청을 추가하지 않는다(`tests_r20.rs:292-294`).
- 키 노출: `IngestReissueOutcome::Minted{masked}`는 앞 8자 + "…"뿐이다. `Failed`는 원인 enum과 고정 `&'static str` 문구다(`otel_link.rs:135-136`). 직렬화 테스트 `otel_link/tests_r20.rs:36-73`이 `mhd_`와 원문 부재를 단언한다. diff에서 `println/eprintln/log/console` 추가는 0건이다. 프론트 catch(`settings.ts:243`)는 `run_blocking`의 join 오류 문자열만 싣는다.
- 테스트 빈틈: RV-004.

## 검증 3 — R14 이월 RV-002/007/010/011
| ID | 처리 | 근거 |
|---|---|---|
| RV-002 | ✅ | `chainStuckToast`는 원인을 중립으로 말하고, `chainConsentMessage`는 후보와 stuck이 함께일 때 한 줄을 붙인다(`devToolsText.ts:61-70`). 변이 T6에서 실패한다 |
| RV-007 | ✅ (표현 Nit RV-007) | `toolStatusLabel` "버전 미확인"(`devToolsText.ts:20-23`)을 행 배지(`devTools.ts:627-629`)와 사이드바(`sidebar.ts:482-486`)가 함께 쓴다. 필수 배지의 위험색도 제외된다(`devTools.ts:640`) |
| RV-010 | ✅ | `runOrigin`을 실행 직전에 기록하고(`devTools.ts:232`), `classifyRun`이 "이전 버전 미확인"으로 표기한다. 토스트와 패널이 같은 함수를 쓴다(`:173`, `:861-880`). 변이 T4에서 실패한다. 실행 경로가 모두 `runPlan`을 거치는 것을 `lastResult` 쓰기 지점 5곳(`:143, 226, 236, 386, 428`)으로 확인했다 |
| RV-011 | ✅ | 주석(`devTools.ts:503-505`)이 Homebrew Node 예외를 적는다 |

## 지적 사항
| ID | 심각도 | 페르소나 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-001 | 🟡 Minor | screen-state-honesty / credential | `src/onboarding.ts:430`(`적용된 수집키 (${masked}).`, 1f00126부터 있던 줄), `src/views/settings.ts:622`(초록 `설정됨` 배지), 캡처 05·06·08·02 | Read, 캡처 열람, 계약 `:851, :1005, :1079`, `compose_status`(`otel_link.rs:477-516`), 테스트 `tests_r20.rs:393` | **PM 관찰에 대한 판단: 오해를 부른다.** 표기 자체는 파일 사실이다("파일에 쓰여 있다"). 그런데 R20이 바로 앞에 "이 기기의 다른 수집키도 함께 폐기됩니다"를 붙여, 같은 줄에서 "폐기"와 "적용된"이 나란히 읽힌다. 재연결 필요의 주요 원인은 유출 의심 D14, 재사용 탐지 T5, 웹 기기 삭제다. 이 경로들에서 서버는 그 기기 키를 모두 폐기한다(계약 `:851, :1005`). quota 차단도 현행 키를 포함해 전부 폐기한다(G). 따라서 이 상태의 파일 키는 대부분 이미 죽은 키다. 설정 화면은 같은 키에 초록 "설정됨"과 "허브 연결 중에는 앱이 수집키를 자동 관리합니다"를 함께 보여 준다(08). 웹 폐기 확인 대기 상태(02)에서도 초록 "설정됨"이 그대로다. 같은 앱의 `revokedStep`은 이미 "설정 파일에 남아 있는 키(…)로는 수집되지 않습니다"라는 정확한 표현을 쓴다(`onboarding.ts:300`) | `failedStep`에서 cause가 `reconnectRequired`·`blocked`·`notDesktopToken`이면 꼬리를 "설정 파일에 남아 있는 수집키(mhk_AbCd…)는 허브에서 함께 폐기됐을 수 있어 수집되지 않을 수 있습니다"로 바꾼다. quota는 "…는 폐기되어 수집되지 않습니다"로 단정해도 된다. 설정 화면 배지는 `failure.cause ∈ {reconnectRequired, blocked}` 또는 `revokedByHub`일 때 톤을 `archived`로, 라벨을 "파일에 남음"으로 바꾼다. 파일에서 키를 자동 삭제하지는 않는다(계약 D-13은 앱이 직접 해제할 때만 삭제하고, 재연결하면 앱이 새 키로 교체한다). 테스트: `tests_r20.rs:393`의 "파일 사실은 그대로 보인다"는 유지하고, 프론트 문구 테스트에 "적용된" 부재 단언을 더한다 |
| RV-002 | 🟡 Minor | design-contract-auditor | `src/onboarding.ts:448-450`(slim), `:261, :424`(`LEAK_REVOKE_FACT` 추가), 캡처 06 | 계약 `:570`("차단 자체는 이중방어다. 계열 폐기 없이 suspected_leak가 기록되는 경로"), `:1080` B4, 엔진 `dispatch`(`otel_link_engine.rs:513`)가 GET 200에서만 Blocked를 냄 | `blocked/suspectedLeak`은 GET이 **200**으로 응답한 상태다. mhd_ 계열이 살아 있다는 뜻이다. 계열이 폐기됐다면 미들웨어가 401을 낸다. 그런데 한 줄 요약은 "유출 의심으로 폐기되어 이 PC의 맑은에이전트 연결이 해제되었습니다. 앱에서 다시 로그인해 연결하세요"라고 말한다. 사실과 다르고, 필요한 첫 동작인 "연결 해제"도 빠져 있다. 허브 연결 행은 이 상태에서 "연결됨"과 [연결 해제]를 보여 주고 로그인 버튼이 없다(`settings.ts:1314-1323`). 상세 문구는 맞지만("앱의 허브 연결을 해제한 뒤 다시 로그인해 연결하세요"), 뒤에 붙은 일반론 "유출 의심으로 폐기하면… 연결도 함께 폐기됩니다"가 관측된 사실(연결 생존)과 어긋난다. 변이 T2가 실패하므로 지금 테스트는 이 틀린 문장을 고정하고 있다 | slim: "선택 · 유출 의심 폐기 이력 때문에 이 연결로는 수집키를 발급할 수 없습니다. 허브 연결을 해제한 뒤 다시 로그인해 연결하세요." 상세의 `LEAK_REVOKE_FACT`는 "이 기기에 유출 의심으로 폐기된 수집키가 있어 이 연결로는 발급이 막혔습니다"로 바꾼다. 해당 TS 테스트 기대값도 함께 고친다 |
| RV-003 | 🟡 Minor | screen-state-honesty | 엔진 `otel_link_engine.rs:82-93`(probe Dead는 OTel phase만 바꾼다), `src/hubAuthApi.ts:93-95`(CONNECTED → "연결됨"), `src/views/settings.ts:576-579`("허브 연결 열기" → `#/settings/mcp`), `:1314-1323`, hub_auth가 폐기를 알아채는 곳은 renew뿐(`hub_auth/renew.rs:126-129`) | Read(코드 대조, 화면 미관측) | OTel 카드는 "이 PC의 맑은에이전트 연결이 해제되었습니다. 앱에서 다시 로그인해 연결하세요"라고 말한다. [허브 연결 열기]를 누르면 허브 행은 hub_auth의 다음 renew 전까지 초록 "연결됨"과 [지금 확인·갱신]/[연결 해제]를 보여 준다. 사용자가 [지금 확인·갱신]을 눌러야 REVOKED → [다시 연결]이 나타난다. 막다른 골목은 아니지만 한 앱 안의 두 화면이 반대 사실을 말한다. R17부터 있던 구조인데, R20이 "해제되었습니다"를 단정하면서 드러났다. renew 주기는 확인하지 않았다 | (작은 비용) 엔진이 `Phase::Reconnect`에 도달하면 hub_auth의 상태 재평가(`renew_now` 1회, 401이면 Revoked)를 부른다. 또는 프론트 `connect-hub` 클릭에서 ingest 실패 원인이 `reconnectRequired`이고 hubAuth가 CONNECTED이면 먼저 `handleRenewHub()`를 부른다. 둘 다 싫으면 문구를 "[허브 연결 열기] → [지금 확인·갱신] 후 [다시 연결]"로 경로까지 적는다. 근거: renew 경로와 REVOKED 렌더는 이미 있다. 수정량은 세지 않았다(미확인 추정) |
| RV-004 | ⚪ Nit | claimed-vs-verified | `otel_link_engine.rs:852-857`(WriteFailed 분기), `tests_r20.rs` | 변이 M5(WriteFailed에서 `st.minted=Some`) → 992 전부 통과 | RT-4의 핵심 주장 "Minted는 설정 파일에 실제로 적용됐을 때만"에서 쓰기 실패 쪽을 고정하는 테스트가 없다. 지금 코드는 맞다. 다만 RV-06·RV-09에서 두 번 난 거짓 성공 토스트가 이 자리에서 재발해도 아무 테스트도 알리지 않는다 | `accept_confirm` + `apply_ingest_key` 실패(읽기 전용 파일 등 기존 WriteFailed 테스트 픽스처 재사용) → `ReissueResult::Ended(Phase::WriteFailed)`이고 `reissue_outcome`이 `Failed{applyFailed, transient:false}`임을 단언하는 테스트 1건 |
| RV-005 | ⚪ Nit | screen-state-honesty | `src/otelLinkText.ts:46` | `is_transient`(`otel_link.rs:131-133`)에 FeatureUnavailable 포함 | 확인창에 동의한 뒤 POST 401 → self 200(서버 롤백)이면 토스트가 "…1시간 뒤 다시 확인합니다. 아직 새 수집키가 적용되지 않았습니다. [다시 시도]로 다시 발급할 수 있습니다"가 된다. 기능이 없는 동안 [다시 시도]는 같은 결과로 끝난다. 발생 빈도는 매우 낮다 | `failure.cause === 'featureUnavailable'`이면 꼬리를 붙이지 않는다 |
| RV-006 | ⚪ Nit | design-contract-auditor | `src-tauri/src/otel_ingest_key.rs:86-88` | grep | 주석이 "허브에 수집키 발급·폐기 셀프서비스 화면이 아직 없다… 허브 UI가 생기면 그 URL을 여기에 다시 싣는다"라고 말한다. `settings.ts:106-113` 주석은 R20에서 "/keys 패널 있음, 버튼은 두지 않는다"로 갱신됐는데 이 거울 주석은 그대로다. 사용자 문구는 아니다 | settings.ts 주석과 같은 사실로 고치고, "URL을 다시 싣는다"를 "버튼을 두지 않기로 했다"로 바꾼다 |
| RV-007 | ⚪ Nit | screen-state-honesty | `src/views/devTools.ts:627-629`, `src/sidebar.ts:482`, `src/devToolsText.ts:15-17` vs `src/devToolsOrder.ts:96-98` | Read(캡처 없음) | (a) "버전 미확인"이 사이드바에서는 경고색(`warn`) dot인데, 행 배지는 미설치와 같은 회색 `badge-archived`다. 같은 상태가 다르게 보인다. 두 삼항 분기도 같은 `toolStatusLabel`을 불러 의미가 없다. (b) `isVersionUnverifiedTool`과 `isVersionUnverified`가 같은 식을 두 곳에 정의한다. 주석이 "같은 정의"라고만 적어 두었다 | (a) 미확인일 때 행 배지를 경고 톤 클래스로 바꾼다. (b) `devToolsText`가 `devToolsOrder`의 함수를 재export하거나 반대로 한다(둘 다 순수 모듈이라 import 제약이 없다는 것은 미확인) |

## 페르소나별 관점
- **design-contract-auditor 🟢**: §D 1·7(a)(b)·10·13과 허브 513f20a 세 문구가 앱 문구·엔진 분기와 조항 단위로 맞는다. 어긋난 곳은 blocked/suspected_leak 한 줄 요약(RV-002)이 계약 `:570`의 "이중방어(연결 생존)" 의미를 놓친 것과 낡은 주석(RV-006)이다.
- **credential-at-rest-and-transit 🟢**: 새 IPC 응답형에 원문·mhd_가 없다(직렬화 테스트 + 코드 대조). 폐기된 키가 파일에 남는 것은 계약상 정상이다. 다만 그 키를 "적용된/설정됨"으로 부르는 것은 키의 생사를 오해하게 한다(RV-001).
- **claimed-vs-verified 🟢**: 커밋 메시지의 "1:1 대응"과 "분기 고정" 주장은 변이 13종 중 11종으로 뒷받침된다. 살아남은 2종은 RV-004(Rust)와 렌더 경로 T5다. T5는 캡처가 덮는다.
- **screen-state-honesty 🟡(조건부)**: 캡처 10장의 상태와 문구가 각각 맞는다. 다만 재연결·차단 상태에서 "폐기"와 "적용된/설정됨/연결됨"이 한 화면이나 이웃 화면에 함께 나온다(RV-001, RV-003).
- **zero-base-redesigner (발산)**: 아래 RT-5.

## 🔵 Rethink
| ID | 현재 구조 | 대안 구조 | 이유 | 비용 |
|---|---|---|---|---|
| RT-5 | 키의 "살아 있음"을 세 출처가 따로 말한다. 파일 사실(`derive_link_status`: 설정됨/적용된), 엔진 phase(`compose_status`: 폐기·재연결·차단), hub_auth 상태(연결됨/연결 해제됨)다. 화면마다 다른 출처를 읽어 RV-001과 RV-003이 생겼다 | `IngestLinkStatus`에 `keyLiveness: 'live' \| 'revoked' \| 'unknown'` 하나를 백엔드가 계산해 싣는다. 규칙은 confirmRequired/revokedKept → revoked, Reconnect/Blocked(quota) → revoked, Blocked(leak)/NotDesktopToken → unknown, 그 밖 → live다. 홈 꼬리·설정 배지·OTel 카드는 이 값만 보고 고른다. 엔진이 Reconnect를 판정하면 hub_auth에도 같은 사실을 전파해 허브 행이 즉시 "연결 해제됨"이 되게 한다 | "적용됨"을 프론트가 상태 조합으로 다시 추론하는 구조는 RT-4가 토스트에서 없앤 것과 같은 문제다. 생사 판정을 엔진 한 곳에 두면 다음 계약 변경(예: 새 폐기 사유) 때 화면 3곳을 따로 고치지 않아도 된다 | 작음~중간(미확인 추정). 근거: 판정 재료(phase, revoked_prefix, file prefix)가 모두 `compose_status` 안에 이미 있다. hub_auth 전파는 renew 진입점을 재사용한다. 커맨드 래퍼와 프론트 수정량은 세지 않았다 |

## 기각된 지적
| 후보 | 기각 사유 |
|---|---|
| "Minted 직후 연결 해제가 끼면 성공 토스트가 해제 뒤에 뜬다" | `apply_guarded`가 세대 확인과 쓰기를 gate 안에서 함께 한다(`otel_link_engine.rs:403-415`). 쓰기 뒤에 끼는 해제는 `disconnect_with`가 키를 지우고 자기 토스트를 낸다. 순서상 사실이 맞고("적용했다" 다음 "해제했다"), 재현 경로를 실측하지 않아 지적으로 올리지 않는다 |
| "홈에 '폐기됨 · 재확인 필요' 배지가 안 보인다(N-13 미해소)" | 홈 체크리스트는 원래 tag를 렌더하지 않는다. tag는 설정 패널 배지에만 쓰인다(`settings.ts:601`). 캡처 02에 배지가 렌더돼 있다 |
| "재연결 필요 상태에서 폐기된 키를 파일에서 지워야 한다" | 계약 D-13은 앱이 직접 해제할 때만 삭제를 요구한다. 재연결하면 GET `auto` → 새 키로 교체된다(D-7). 자동 삭제는 오히려 일시 오판 시 수집을 끊는다. 표시 문제로만 다룬다(RV-001) |

## 잘 된 점
- RT-4를 백로그로 남기지 않고 이번에 끝냈다. `beforeMasked` 추론, 분기 7개 역산, 우연한 masked 일치 경계가 사라졌고 토스트 표는 이제 enum 6개짜리 switch다.
- `finish`가 Reconnect/Blocked에서 폐기 판정을 비운다. 그래서 "다시 발급"과 "다시 로그인" 안내가 섞이지 않는다. 이 동작은 변이 M1로 판별되는 테스트 2건이 고정한다.
- 엔진 테스트가 목 서버 요청 목록(`hub.requests()`)과 `probe.count()`까지 단언한다. 그래서 "재로그인 경로로 가지 않았다"는 주장이 문구가 아니라 동작으로 검증된다.
- R18 Nit 3건(N-11·13·14)과 R14 이월 4건이 모두 테스트와 함께 닫혔다.
- 허브 문구(`utils.js:960-962`)와 앱 문구가 같은 동사("앱에서 다시 발급" / "다시 로그인해 연결")를 쓴다.

## PM 권고
1. **출시 전 권장(Minor 3, 모두 문구·표시)**
   - RV-002는 slim 1줄과 `LEAK_REVOKE_FACT` 1줄, 테스트 기대값만 고치면 된다. 비용이 가장 작다.
   - RV-001은 `failedStep` 꼬리 분기와 설정 배지 톤을 고친다.
   - RV-003은 `connect-hub` 클릭 시 renew를 선행하는 방식이 가장 작다.
   - 셋 모두 엔진 동작은 바꾸지 않는다.
2. **같은 커밋에 얹기(Nit)**: RV-004 테스트 1건(RT-4 회귀 방어), RV-006 주석, RV-005 꼬리 1줄. RV-007은 devTools 손볼 때 하면 된다.
3. **백로그**: RT-5. RV-001과 RV-003을 구조적으로 없앤다. 다음 폐기 사유나 계약 개정 때 함께 검토한다.
4. **INDEX/적용 이력 갱신**: 이번 위임 제약으로 하지 못했다. 위 "게이트 예외"의 5행을 PM 커밋에 포함해야 한다.
5. 하지 못한 것: devTools 변경(RV-002/007/010/011)은 캡처 없이 코드와 단위 테스트로만 확인했다. 실제 Tauri 런타임과 라이브 허브는 실행하지 않았다.
