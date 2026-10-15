# R17 허브 수집키 자동 연동 — 재검토 보고서 (2차)

리뷰 대상: `887ddaa..a801b09` 전체 범위(반영 커밋 40d3f42·a801b09에 집중, 최종 판정은 전체 범위 기준) · target_id `r17-otel-autolink` · 워크트리 `.claude/worktrees/r17-otel-autolink`, 브랜치 `feat/r17-otel-autolink`
직전 리뷰: `docs/reviewer/review-r17-otel-autolink-2026-10-15.md`(Amber, Major RV-01·RV-02, Minor RV-03~08)
리스크 범주: 크리덴셜 발급 · 사용자 설정 파일(`~/.claude/settings.json`) 쓰기 · 인증
정본: malgnai-public `docs/design/device-ingest-key-provisioning.md` 16~243행(읽기만 했다)
등급/모드: Sensitive, 2차 재검토 풀패널(증분). 재검토 3요소가 모두 있어 동일 대상으로 인정했다. 반영 커밋이 새 리스크 표면 세 가지(연결 세대·gate 직렬화, `otel_settings_save` 동기→비동기 전환과 백엔드 거절, 해제 응답 스키마 변경)를 들여와서 축소 모드가 아니라 증분 풀패널로 처리했다. 노출 범위 축소는 적용하지 않았다(축소 근거 3값 없음).
리뷰 일자: 2026-10-15
종합 판정: 🟢 **Green** (Critical 0 · Major 0 · Minor 1(신규 RV-09) · Nit 4(신규) · Rethink 1(신규) + 이월)

## 요약 (2분 규칙)
Major 2건(RV-01·RV-02)과 Minor 6건(RV-03~08)은 정상 경로에서 해소됐다. 각 항목에 회귀 테스트가 붙어 있고, 테스트를 직접 돌려 통과를 확인했다. 직렬화(RV-05)의 락 순서는 일관된다(`run → gate → SETTINGS_WRITE_LOCK`, `gate → mem`, 역방향 없음). 교착 경로는 찾지 못했다. 해제 응답의 `ingest_key`는 3값 열거형이라 비밀이 실리지 않는다. capabilities·Cargo·tauri.conf는 전체 범위에서 0줄 변경이다.
새 결함은 하나다. **RV-09**: RV-01 수정과 RV-07 수정이 맞물려 생긴 빈틈이다. 웹에서 폐기된 키가 파일에 남은 상태에서 다음 점검이 일시 오류(네트워크·5xx·429)로 끝나면, 화면이 그 폐기된 키를 다시 "적용됨"으로 보여 준다. 사용자가 재발급에 동의했는데 POST가 일시 오류로 끝난 경우에는 토스트가 "수집키를 다시 발급해 적용했습니다"라고 거짓으로 말한다. 다음 성공 점검(백오프 1분 이상)에서 저절로 바로잡히고 보안 영향은 없어서 Minor로 둔다. 비용이 작으니 출시 전에 고치기를 권한다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| credential-at-rest-and-transit | 재사용 | 해제 시 키 삭제, 해제 응답 스키마, traces 목적지(RV-04)가 이 역할개념(디스크 잔존·목적지·폐기 수단) 그대로다 |
| design-contract-auditor | 재사용 | D-1·D-4·D-12·D-13 반영을 조항 단위로 다시 대조한다 |
| automated-retry-budget-burn | 재사용 | RV-03의 GET 수 감소와 RV-02의 예산 소비 경로 차단을 다시 센다 |
| tauri-runtime-thread-and-lock-discipline | 재사용 | RV-05의 epoch/gate는 "사라진 암묵적 직렬화를 누가 대신하는가"와 락 순서·교착 문제다. `otel_settings_save`의 `run_blocking` 전환도 같은 관점이다. 늦은 응답 덮어쓰기를 보는 `async-response-ordering`도 후보였지만 위임이 락 순서·교착을 명시해 이 페르소나로 묶었다(두 관점을 모두 이 패널에서 봤다) |
| claimed-vs-verified | 재사용 | RV-06 토스트가 관측을 근거로 하는지, 회귀 테스트가 빈 깡통인지 본다 |
| screen-state-honesty | 재사용 | RV-01·RV-07·RV-08 화면 상태와 캡처 17~26을 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 의심 역할개념 그대로다. 🔵 RT-3을 냈다 |

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `git rev-parse HEAD` | `a801b09` (워크트리 HEAD = 리뷰 범위 끝) |
| `cd src-tauri && cargo test` | **937 passed / 0 failed / 23 ignored** (직전 916 → +21. rv01~rv07 계열 실재: `otel_link_engine/tests.rs:1505-1754`, `otel_link/tests.rs:1000-1154`, `otel_settings.rs:1408-1535`) |
| `pnpm exec tsc --noEmit` | 통과 |
| `pnpm run test:ts` | 44 pass / 0 fail (직전 33 → +11, `scripts/tests/otelLinkText.test.mjs` 신규 포함) |
| `pnpm build` | 통과 |
| 탐침 테스트(스크래치 경로, 저장소 밖) | `reissueOutcome`·`computeOnboarding`에 "폐기 키가 파일에 남음 + phase=Transient" 상태를 넣어 RV-09를 재현했다(아래 RV-09 확인방법) |
| 캡처 | 신규 17~26(10장)과 갱신 04·05·09·10(4장)을 전부 Read로 열람. 05와 17은 모달 아래 문구가 다른 별개 상태임을 확인 |
| capabilities 표면 | `git diff 887ddaa..a801b09 -- src-tauri/capabilities src-tauri/Cargo.toml src-tauri/tauri.conf.json package.json` → 0줄. 커맨드 등록은 1차와 같은 3개 추가(`lib.rs`)뿐이다 |
| UI 하네스(otelAutolink 35·home 32·otelIngestKey 12, bugs=0) | **재실행하지 않았다**(개발자 주장). 캡처 산출물로만 확인했다 |
| 라이브 POST/rotate/DELETE·실제 `~/.claude/settings.json` 변경·앱 실기동 | **하지 않았다** |

## 직전 지적 해소 확인
| ID | 직전 심각도 | 해소 | 근거(직접 열람) |
|---|---|---|---|
| RV-01 | 🟠 Major | **해소(정상 경로) / 잔여 RV-09** | 백엔드: `otel_link.rs:405-406` `revoked_by_hub`가 ConfirmRequired·ConfirmDeclined일 때 confirmed 승격을 막는다. 엔진: `otel_link_engine.rs:456-469` `need_confirm`이 `confirmed_prefix`·`watch`·`first_data_timed_out`을 비운다. 화면: `onboarding.ts:385-389`가 `revokedStep`을 먼저 처리하고, 거절 뒤에도 `action:'retry'`가 남는다. 테스트: `otel_link/tests.rs:1000`(applied·confirmed × required·declined)과 `:1027`(확인됨 → 웹 폐기 → 거절 → [다시 시도] 전체 경로). 캡처 17·18·19. 단, 폐기 판정 다음 점검이 일시 오류로 끝나면 표시가 되돌아간다 → RV-09 |
| RV-02 | 🟠 Major | **해소** | UI: `settings.ts:593-594, 626-631`에서 `keyManagedByHub`면 입력칸 `disabled`·드래프트 비움·전용 문구. `:495`는 저장 시 keep을 강제한다. 백엔드: `otel_settings.rs:714-741` `save_checked`가 Set을 거절한다(Keep·Clear 허용, Set일 때만 키체인 조회). UI 판정 `hubConnected && issueSupported`(`otelLinkText.ts:12`)와 백엔드 `hub_manages_ingest_key`(`otel_link.rs:669`)는 같은 식이다. `issue_supported = live().is_some()`, `hub_connected = tokens.latest().is_some()`(`otel_link.rs:615-616`). 사용자가 닿는 Set 경로는 `otel_settings_save` 하나뿐이다(grep). `values`로 헤더 키를 넣는 우회는 `otel_settings.rs:415`가 막는다. 테스트 `rv02_*` 3건(파일·백업 불변, 문구에 입력값 없음). 캡처 26 |
| RV-03 | 🟡 Minor | **해소** | `otel_link_engine.rs:515-526`: 타임아웃이 난 같은 prefix는 정기 점검에서 폴링을 다시 열지 않는다. 다시 여는 경우는 `restart_watch`(User)뿐이고, 기동·연결 직후는 메모리가 비어서 자연히 다시 연다. 테스트 `rv03_*` 2건. 신호 없는 키의 GET은 하루 약 1,464회 → 정기 24회 + 기동당 최대 90회로 준다. 문구 `onboarding.ts:298`, 캡처 10. 계약 문구와의 차이는 N-9 |
| RV-04 | 🟡 Minor | **해소** | `otel_settings.rs:610-614`: `all_present`에 traces를 넣었다. `:628-631`: metrics·logs 각각 신호별 → 범용 순으로 평가한다. 테스트 `rv04_*`(`otel_settings.rs:1490, 1514`, 엔진 gate 통과 `tests.rs:1505`). 빈 값·해석 불가 URL 경계도 단언한다 |
| RV-05 | 🟡 Minor | **해소 / 잔여 N-7(Nit)** | `otel_link_engine.rs:309-358`: 해제는 gate 안에서 epoch 증가 → 메모리 초기화 → 키 삭제를 한 임계구역으로 처리한다. 쓰기는 `apply_guarded`가 gate 안에서 stale 확인 후에 한다. 요청 전(`:595`), 폴링 응답 후(`:933-938`), `finish`(`:400`)에도 stale 확인이 있다. 락 순서 대조는 아래 "동시성 대조". 테스트 `rv05_*` 6건(`tests.rs:1661-1754`): 실제 해제 경로와 같은 클로저로 POST·rotate·Lost·재연결·폴링 중간 전환을 만든다 |
| RV-06 | 🟡 Minor | **부분 해소 → RV-09** | 해제 토스트: `hub_auth/mod.rs:158-205`, `otelLinkText.ts:45-58`, 테스트 `otel_link/tests.rs:1099, 1134`, 캡처 24·25로 **해소**. 재발급 토스트: `reissueOutcome`(`otelLinkText.ts:28-43`)이 실패·대기를 구분하게 됐다(캡처 21). 다만 "폐기 키가 파일에 남은 상태 + POST 일시 오류"에서는 거짓 성공을 낸다 → RV-09 |
| RV-07 | 🟡 Minor | **해소 / RV-09와 상호작용** | `otel_link.rs:120-124` `is_transient`, `:423-434` notice 분기. FeatureUnavailable 문구가 "허브 연결은 정상이며…"로 바뀌었다(`:141-143`). N-4 버튼 제거 `onboarding.ts:310`. 테스트 `rv07_*` 2건. 캡처 22·23 |
| RV-08 | 🟡 Minor | **해소** | `dom.ts:247-284` `defaultFocus:'cancel'`. `settings.ts:244-251` 자동 팝업은 홈·설정>OTel에서만 뜬다. `main.ts:305` 화면 진입 때 다시 묻는다. 캡처 05·17(초점이 "발급하지 않음"), 20(사용량 화면에서는 모달 없음) |
| N-1 | ⚪ Nit | 해소 | `settings.ts:480` 머리말 한 줄 |
| N-4 | ⚪ Nit | 해소 | 위 RV-07 |
| N-2·N-3·N-5·N-6 | ⚪ Nit | **이월**(개발자가 범위 밖으로 명시) | 코드 변화 없음을 확인했다 |
| RT-1 | 🔵 | **이월 확정**(위임 명시) | — |
| RT-2 | 🔵 | 최소형으로 채택 | "키 소유 모드"를 별도 상태로 두지 않고 `keyManagedByHub` 판정으로 수동 입력을 잠갔다. 엔진 D-4는 그대로다 |

## 동시성 대조 (RV-05 · `otel_settings_save` 비동기화)
| 락 | 잡는 곳 | 그 안에서 더 잡는 것 |
|---|---|---|
| `run`(try_lock) | `run_check`·`accept_confirm`·`poll_first_data`(`otel_link_engine.rs:373, 869, 920`) | `mem`(짧은 클로저), `gate`(`apply_guarded`) |
| `gate` | `on_connected`·`disconnect_with`·`apply_guarded`(`:311, 333, 350`) | `mem`, `SETTINGS_WRITE_LOCK`(`apply_ingest_key`/`remove_ingest_key` → `save_to_settings_file`, `otel_settings.rs:397`) |
| `mem` | `with()`(`:274`) | 없음(클로저가 다른 락을 잡지 않는다) |
| `SETTINGS_WRITE_LOCK` | `save_to_settings_file` | 없음(엔진·gate를 부르지 않는다. grep으로 `otel_settings.rs`에 엔진 호출 0건) |

- 순서는 항상 `run → gate → SETTINGS_WRITE_LOCK`과 `gate → mem`이고 역방향이 없다. `notify()`는 gate 밖에서 부른다(`:318, 343`). 해제 경로는 `connect::disconnect`(hub_auth 락)가 끝난 **뒤** 별도 `run_blocking`에서 `disconnect_with`를 부르므로(`hub_auth/mod.rs:190-205`) hub_auth 락과 gate가 겹치지 않는다. `run`은 try_lock이라 기다리지 않는다. → **교착 경로 없음.**
- gate는 서버 응답을 기다리는 동안 잡지 않는다(파일 입출력 동안만). 그래서 해제가 최대 35초 막히는 일이 없다.
- `otel_settings_save`의 `async` + `run_blocking` 전환(`otel_settings.rs:733-742`): 키체인 읽기(`hub_manages_ingest_key`)를 IPC 스레드 밖으로 뺐다. `HubAuthError`는 문자열로 매핑하고 반환형 `Result<OtelSettings, String>`은 그대로라 프론트 호출부는 바뀌지 않았다(tsc 통과). Set이 거절되면 같은 저장의 다른 값도 저장되지 않는다. 다만 UI가 잠긴 상태에서는 Set이 나가지 않으므로(`settings.ts:495`) 상태를 못 읽은 경우에만 나타나고, 거절 문구가 이유를 밝힌다.

## 지적 사항 (신규)
| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-09 | 🟡 Minor | 화면 정직성 / claimed vs verified (RV-01×RV-07 상호작용) | `src-tauri/src/otel_link.rs:405-406`(`revoked_by_hub`가 **직전 점검의 phase**에만 의존), `:423-427`(applied + 일시 원인 → notice만) · `otel_link_engine.rs:459-463`(`need_confirm`이 `confirmed_prefix`를 비움), `:842-852`(`transient`) · `src/otelLinkText.ts:32-35`(`keyApplied && !confirm*` → 성공 토스트) | 코드 경로 추적: `accept_confirm` → `mint(Create{confirm:true})` → 5xx/NotSent → `common_error` → `transient` → `Phase::Transient`. compose에서 파일 state=Applied, `revoked_by_hub=false`, `confirmed_prefix=None`이 되고 Transient는 `is_transient`라서 `notice`가 된다(같은 모양을 `rv07_*` 테스트 `otel_link/tests.rs:1070-1075`가 단언). 그 상태 객체를 스크래치 탐침으로 `reissueOutcome(…, true)`·`computeOnboarding`에 넣었다. 출력은 `{"kind":"applied","text":"수집키를 다시 발급해 적용했습니다…"}`, 체크리스트 `"수집키가 적용되었습니다 (mhk_OLDK…). 수신은 아직 확인하지 않았습니다… 마지막 점검: 허브에 연결하지 못했습니다…"`였다 | 웹에서 폐기된 키가 settings.json에 남은 상태(RV-01이 다룬 흔한 경우)에서 다음 결과가 **일시 오류**(Unreachable·RateLimited·FeatureUnavailable)이면 폐기 판정 플래그가 사라진다. 그 결과 ① 체크리스트·설정 패널이 폐기된 키를 다시 "수집키가 적용되었습니다"로 보여 준다(RV-01 증상 ①이 일시적으로 재발). ② 재발급에 동의했는데 POST가 일시 오류로 끝나면 토스트가 **발급하지 않은 키를 "다시 발급해 적용했습니다"라고 말한다**(RV-06 목표 위반). 같은 일이 거절 뒤 [다시 시도]의 GET이 끊길 때와 1시간 점검의 GET이 끊길 때도 생긴다. 다음 성공 점검(백오프 1분 이상)이 confirm_required를 다시 받아 바로잡고, 잘못된 키를 쓰거나 내보내는 일은 없다 → Minor | 저비용 두 가지. ① 엔진 메모리에 `revoked_prefix: Option<String>`을 둔다. `need_confirm`이 그때의 파일 prefix를 기록하고, `after_applied`·`on_connected`·`disconnect_with`가 비운다. compose는 `file_prefix == revoked_prefix`이면 phase와 무관하게 폐기 표시를 유지한다(일시 원인은 notice로 덧붙인다). ② `reissueOutcome`은 동의 전 `masked`와 응답 `masked`가 **달라졌을 때만** 성공이라고 말한다(프론트만으로 막는 최소안). 회귀 테스트: "applied(폐기 키) + ConfirmRequired → accept → POST 5xx" 엔진 경로 1건, `reissueOutcome` 표 1행 |
| N-7 | ⚪ Nit | 동시성 (RV-05 잔여) | `otel_link_engine.rs:399-411`(`finish`: stale 확인과 `with()`가 분리됨), `:842-852`·`:820`·`:705`(낡은 점검의 `transient`·RATE_LIMITED·ROTATE_BUDGET 분기가 stale 확인 없이 메모리에 씀), `:548-558`(`after_applied`가 gate 밖) | 오류 분기마다 메모리 쓰기 앞에 stale 확인이 있는지 대조했다 | 응답을 기다리는 사이 **재연결**이 일어나면, 낡은 점검의 5xx·429가 새 연결 메모리에 `backoff_ms`·`budget_hold`를 남긴다. 새 연결의 첫 점검은 `forced=Connected`라 그대로 돈다(`due` `:294`). 영향은 백오프가 한 단계 커지고, 같은 기기 예산 보류가 이어지는 정도다. `finish`의 확인→쓰기 사이에 해제가 끼면 phase가 Idle로 남지만, 토큰이 없어 다음 점검은 네트워크 없이 `NoDeviceToken`으로 끝난다. 파일·비밀에는 영향이 없다 | 점검 중 메모리 쓰기를 `with_live(st, \|m\| …)` 하나로 모아 **mem 락 안에서** epoch를 비교한다. epoch 증가가 메모리 초기화보다 먼저 일어나므로 mem 락 안의 비교만으로 충분하다 |
| N-8 | ⚪ Nit | 화면 정직성 | 캡처 19(`otel-ingest-key-status`가 초록 "설정됨 mhk_AbCd…"인데, 바로 위 패널은 "폐기됨 · 재발급 거절"), 캡처 26(잠긴 입력칸의 외형이 활성 칸과 같고, 아래 "비워 두고 저장하면 기존 키를 그대로 둡니다" 안내가 남아 있음) · `settings.ts:571-645` | 캡처 열람과 렌더 분기 대조 | 파일 사실("설정됨")은 맞지만, 초록 톤이 패널의 폐기 판정과 한 화면에서 엇갈린다. 잠금은 placeholder 문구로만 알 수 있다 | 폐기 판정 중이면 키 상태 배지를 중립 톤으로 바꾸고 "(웹에서 폐기됨)"을 덧붙인다. 잠김 상태에서는 입력칸에 비활성 스타일을 주고 storage 안내를 숨긴다 |
| N-9 | ⚪ Nit | 계약 D-12 문구 | 정본 198행(15분 뒤 "다음 앱 기동 때 GET 1회로 다시 확인") vs `otel_link_engine.rs:515-526`(기동 때는 메모리가 비어 15분 폴링 재개, 정기 점검은 GET 1회) | 정본과 코드 대조 | 직전 리뷰 RV-03 권고("재감시는 Startup·Connected·새 키 적용 때만")를 따른 결과라 계약 문구와 다르다. 영속 상태를 금지하므로 기동 시점에 "이미 타임아웃됐다"는 것을 알 수 없어서 불가피하다. 기동당 최대 90회 GET이다 | 계약 D-12에 "앱은 영속 상태를 두지 않으므로 기동 때 폴링을 다시 시작할 수 있다"를 덧붙이도록 허브 쪽에 개정을 요청하거나, 엔진 주석에 이탈 근거를 남긴다 |
| N-10 | ⚪ Nit | RV-02 한계 | `otel_link.rs:669-671`(`hub_manages_ingest_key` = 키체인에 토큰 있음) | 판정 식과 FeatureUnavailable·Reconnect 경로 대조 | 개발자가 밝힌 한계(featureUnavailable 동안 잠김)에 더해, **Reconnect**(토큰은 남았지만 서버가 거부)와 NotDesktopToken 상태에서도 잠긴다. 이 상태들에서는 엔진이 발급·회전을 하지 못한다(`common_error` → 401/403 분기, `dispatch`에 도달하지 않음). 그래서 붙여넣은 키를 덮어쓸 주체가 없는데도 입력이 막힌다 | 아래 "한계 수용 판단" 참고. 필요하면 Set 거절 조건을 `hub 연결 && phase ∉ {FeatureUnavailable, Reconnect, NotDesktopToken}`으로 좁힌다 |

## 한계 수용 판단 — "허브 연결 중엔 수동 키 입력 잠김(featureUnavailable 동안에도)"
**수용 가능(조건부).** 근거:
- featureUnavailable에서 엔진은 GET 401 → `/self` 확인 → `FeatureUnavailable`로 끝나고 dispatch(D-4 회전)에 가지 않는다(`otel_link_engine.rs:415-428, 831-838`). 그래서 **이미 파일에 있는 수동 키는 건드리지 않는다.** 막히는 것은 "연결 중에 새로 붙여넣기"뿐이다. Clear는 허용된다.
- 잠금을 풀어도 서버 기능이 생기는 순간 D-4가 그 키를 회전해 예산 1개를 쓴다. 이 한 번의 비용은 작지만, 화면이 그 교체를 미리 말해야 한다.
- 조건: 허브 서버의 수집키 발급 기능이 **이 앱 릴리스보다 먼저 또는 함께 배포**되면, featureUnavailable 기간이 사실상 없으므로 한계가 실질적인 문제가 되지 않는다. 앱을 먼저 배포한다면, 연결된 신규 사용자는 기능이 생길 때까지 수집키를 넣을 수 없다(우회하려면 MCP 허브 연결까지 해제해야 한다). 이 경우 N-10의 조건 완화를 같이 넣기를 권한다. 배포 순서는 이번 리뷰에서 확인하지 못했다(라이브 GET 미실행).

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 동시성 | gate와 `SETTINGS_WRITE_LOCK`·`mem`·`run` 사이 교착 | 기각 | 위 "동시성 대조" 표대로 순서가 일관되고 역방향 획득이 없다. `SETTINGS_WRITE_LOCK` 안에서 엔진을 부르지 않는다(grep). `run`은 try_lock이다 |
| 자격증명 | `hub_auth_disconnect` 응답의 `ingest_key`에 비밀이 실린다 | 기각 | `IngestKeyRemoval`은 데이터 없는 3값 열거형이다(`hub_auth/mod.rs:158-169`). `HubAuthStatus` 필드는 상태·이름·시각·사유 코드뿐이다(`hub_auth/state.rs:27-36`). 직렬화 결과를 테스트가 단언한다(`otel_link/tests.rs:1134-1154`) |
| 자격증명 | 반영 커밋이 mhd_/mhk_를 로그·오류로 흘린다 | 기각 | `git diff 0450973 a801b09 -- src-tauri/src`의 추가 줄에서 `println\|eprintln\|log::\|tracing::\|dbg!`가 0건이다. `format!`+키 조합은 테스트 픽스처 1건뿐이다. 거절 문구는 고정 상수이고 입력값을 인용하지 않는다(`otel_settings.rs:707`, 테스트 `:1417`) |
| 보안 | UI 잠금을 `values`의 `OTEL_EXPORTER_OTLP_HEADERS`로 우회한다 | 기각 | `otel_settings.rs:415-419`가 values의 헤더 키를 거절한다. Set 경로는 `otel_settings_save` → `save_checked` 하나다(grep) |
| 정합 | UI 잠금 조건과 백엔드 거절 조건이 어긋난다 | 기각 | 둘 다 `live().is_some() && tokens.latest().is_some()`이다(`otel_link.rs:615-616, 669-671`, `otelLinkText.ts:12-13`). 상태를 못 읽은 경우(null)에는 UI가 막지 않고 백엔드가 최종 판단한다. 의도된 동작이고 테스트로 고정돼 있다 |
| 예산 | 낡은 점검이 해제 뒤 Lost 재시도로 rotate를 한 번 더 보낸다 | 기각 | `:595` 요청 전 stale 확인. 테스트 `rv05_a_lost_rotate_response_does_not_trigger_a_retry_after_disconnect`(rotate 1회 단언) |
| 계약 | 해제 시 서버 폐기 실패(SERVER_REVOKE_FAILED)인데도 파일 키를 지워 살아 있는 키가 서버에 남는다 | 기각(범위 밖) | 이번 범위 이전부터의 동작이다. 토스트가 웹 /keys 폐기를 안내한다(`otelLinkText.ts:46-47`). 파일에서 지우는 쪽이 노출 면에서 더 안전하다 |

## 페르소나별 관점
### 자격증명 보관·전송 경로 감사자 — 🟢 Green
- 해제는 이제 "세대 증가 → 메모리(보관 중 원문 포함) 초기화 → 파일 키 삭제"를 한 임계구역에서 처리한다. 해제 후 뒤늦게 온 201이 키를 다시 쓰는 경로가 닫혔다(`rv05_*` 3건이 파일에 `mhk_` 없음을 단언).
- traces 목적지가 선검사에 들어가 RV-005 계열의 마지막 변형이 닫혔다. 해제 응답·거절 문구에 비밀이 없다.

### 설계 계약 감사자 — 🟢 Green(Nit 1)
- D-1(폐기 판정 우선), D-4(수동 키 교체 경로 자체 차단), D-13(순서와 경합)이 반영됐다. D-12는 계약 문구보다 덜 호출하는 쪽으로 이탈한다(N-9, 근거 있음).

### 자동 재시도 예산 소진 감시자 — 🟢 Green
- 신호 없는 키의 GET이 하루 약 1,464회에서 정기 24회 + 기동당 ≤90회로 줄었다. 사람 행동 하나로 회전 예산을 쓰던 유일한 경로(RV-02)가 UI와 백엔드 양쪽에서 막혔다. 남은 소비 경로는 계약이 허용한 Clear → 재발급뿐이고, 삭제 확인창이 "24시간 6회"를 밝힌다(`settings.ts:126`).
- 낡은 점검은 해제·재연결 뒤 추가 요청을 보내지 않는다(`:595`).

### Tauri 런타임 스레드·락 규율 — 🟢 Green(Nit 1)
- 락 순서가 일관되고 교착 경로가 없다. gate는 네트워크 대기 동안 잡지 않는다. 키체인 읽기를 IPC 스레드에서 `run_blocking`으로 뺐다. 남은 것은 오류 분기 메모리 쓰기의 stale 확인 누락(N-7)이다.

### claimed vs verified — 🟡 Amber(Minor 1)
- 회귀 테스트가 단단하다. `ConnectionChangesMidCall`은 실제 해제 클로저로 요청과 응답 사이의 전환을 재현한다. 해제 토스트는 실제 결과를 말한다.
- 재발급 토스트의 성공 판정이 "파일에 키가 있음"에 기대고 있어, 키가 **바뀌었는지**는 보지 않는다(RV-09). 커밋 메시지의 "토스트 정직성"은 이 경로에서 아직 주장 단계다.
- UI 하네스 bugs=0과 실제 Tauri 런타임은 이번에도 재실행·관측하지 않았다.

### 화면 상태 정직성 — 🟡 Amber(Minor 1, Nit 1)
- 캡처 17~19는 폐기 키가 남은 경우의 문구·배지("폐기됨 · 재발급 필요/거절")·[다시 시도]를 정확히 보여 준다. 22·23은 일시 원인이 확인 사실을 지우지 않는 모습이고, 24·25는 해제 결과별 토스트다.
- 다만 폐기 판정이 phase 한 칸에만 실려 있어, 일시 오류 한 번에 판정이 사라진다(RV-09). 키 상태 배지 톤도 패널과 엇갈린다(N-8).

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵
| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-3 | `RuntimeView.phase` 한 칸이 "직전 점검이 어떻게 끝났나"(Transient·RateLimited…)와 "허브가 이 키에 대해 내린 판정"(ConfirmRequired·Declined)을 함께 싣는다. compose는 그 한 칸으로 키 상태와 실패 상태를 동시에 추론한다 | 엔진 메모리를 두 축으로 나눈다. **키 판정**(`key_verdict: {prefix, Revoked\|Confirmed\|Waiting\|TimedOut}`)은 새 키 적용·연결 변경 때만 바뀐다. **마지막 점검 결과**(`last_outcome`)는 점검마다 바뀐다. compose는 파일 사실 × 키 판정으로 상태를 정하고, 마지막 점검 결과는 notice나 failure로만 덧붙인다 | RV-01·RV-07·RV-09가 모두 "한 칸에 두 의미"에서 나왔다. 축을 나누면 일시 오류가 판정을 지우는 종류의 회귀가 구조적으로 사라진다. `confirmed_prefix`·`first_data_timed_out`·`watch`가 이미 키 판정 축의 조각이라 흩어진 필드를 하나로 모으는 일이기도 하다 | 중간. 비용 근거: `RuntimeView`(`otel_link_engine.rs:279-286`)와 `compose_status`(`otel_link.rs:388-440`)의 필드 재배치이고, 테스트는 대부분 상태 표로 돼 있어 기대값만 옮기면 된다(미확인 추정치. 테스트 수정량은 세지 않았다). 단기에는 RV-09 개선안 ①(`revoked_prefix` 한 필드)이 이 구조의 첫 조각이 된다 |
| RT-1 | (이월) mhk_ 원문을 settings.json에 평문으로 쓴다 | `otelHeadersHelper` + 키체인 1부 | 직전 보고서 RT-1 그대로다. 위임에서 범위 밖 이월이 확정됐다 | — |

## 트레이드오프
- **화면 정직성 vs 설계 계약(N-10·한계)**: 계약은 "연결 중엔 앱이 키를 소유한다"는 단순 모델에 맞는다(D-4가 누가 넣었는지 묻지 않는다). 화면 관점에서는 앱이 키를 관리할 수 없는 상태(FeatureUnavailable·Reconnect)에서도 사용자를 막는 것이 과잉이다. → 권고: 서버를 먼저 배포한다면 현행대로 둔다. 앱을 먼저 배포한다면 N-10의 조건 완화를 넣는다.
- **예산 감시자 vs RV-03 재감시**: 기동 때 폴링을 다시 여는 것은 호출 수 관점에서 계약보다 많지만(N-9), 영속 상태 금지와 "세션을 열면 곧 수집이 확인된다"는 UX를 고려하면 합리적이다. → 현행 유지, 계약 문구 정합만 한다.

## 잘 된 점
- RV-05 수정이 "락을 넓히기"가 아니라 **세대 번호 + 파일 입출력 동안만 잡는 gate**로 이뤄졌다. 해제가 네트워크 대기에 막히지 않으면서 해제 후 쓰기가 닫혔다. 테스트 쪽의 `ConnectionChangesMidCall`(응답과 반환 사이에 실제 해제 클로저를 끼움)은 경합 테스트를 결정적으로 만드는 좋은 기준이다.
- RV-02를 UI 잠금만으로 끝내지 않고 백엔드 `save_checked`로 우회까지 막았다. Set일 때만 키체인을 읽는다는 사실도 테스트로 고정했다(`rv02_keep_and_clear_…`, 호출 횟수 0 단언).
- 토스트 문구를 순수 함수(`otelLinkText.ts`)로 빼서 단위 테스트를 붙였다. 이번 RV-09도 같은 방식으로 표 한 행만 추가하면 고정된다.
- 해제 응답을 `#[serde(flatten)]`으로 확장해 기존 `HubAuthStatus` 소비처를 깨지 않았고, 그 사실을 테스트가 단언한다.
- RV-01 테스트가 "폐기 판정이 없으면 같은 입력이 confirmed"라는 대조군을 함께 단언한다(`otel_link/tests.rs:1022`). 결과가 입력 때문이 아니라 판정 때문임을 고정한다.

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| C2 원문이 경계를 넘지 않음 | 자격증명 | 필수 | ✓ | 해제 응답·거절 문구 포함 |
| C3 키 교체는 명시적 동작으로만 | 자격증명 | 필수 | ✓ | 수동 키 덮어쓰기 경로 차단(RV-02) |
| C4 목적지 통제 | 자격증명 | 권장 | ✓ | traces 포함(RV-04) |
| B1/B5 소비 호출 상한·실패 반복 억제 | 예산 | 필수/권장 | ✓ | RV-03 |
| 락 순서·교착 없음 | 런타임 | 필수 | ✓ | 동시성 대조표 |
| 낡은 응답이 현재 상태를 덮지 않음 | 런타임 | 필수 | △ | 파일·phase는 ✓, 오류 분기 메모리는 N-7 |
| 계약 D-1~D-15 | 계약 | 필수 | ✓(D-12 문구 N-9) | |
| 상태 표시 정직성 | 화면 | 필수 | △ | RV-09(일시 오류 때만) |
| 성공 문구의 근거가 관측 | claimed | 필수 | △ | RV-09 |
| capabilities 표면 불변 | 자격증명 | 필수 | ✓ | diff 0줄 |

## 생략·미검증 명시
- 실제 Tauri 런타임(스케줄러 스레드, 이벤트, 키체인 재읽기, `run_blocking` 안 저장)은 앱을 기동하지 않아 관측하지 않았다. 근거는 코드와 단위 테스트다.
- UI 하네스(otelAutolink·home·otelIngestKey)는 재실행하지 않았다. 캡처 산출물만 열람했다.
- 허브 서버 배포 상태(featureUnavailable 기간 유무)는 확인하지 않았다(라이브 GET 미실행). 한계 수용 판단은 이 사실에 조건이 걸려 있다.
- Windows 경로는 이번 반영에서 바뀐 곳이 없어 다시 보지 않았다.

## PM에게 권고
1. **Green — 통과.** 미해결 Critical·Major는 없다.
2. **RV-09(Minor)는 출시 전 수정을 권한다.** 재발급이라는 자격증명 동작에서 거짓 성공을 말하는 경로라서다. 최소안은 `reissueOutcome`이 동의 전후 `masked` 변화를 보게 하는 것으로, 프론트 한 함수와 테스트 1행이다(근거: `otelLinkText.ts:28-43`가 이미 순수 함수이고 표 테스트가 있다). 화면 회귀까지 막으려면 엔진 `revoked_prefix` 한 필드를 더한다(RV-09 개선안 ①, RT-3의 첫 조각).
3. N-7~N-10은 백로그로 둬도 된다. 단 **앱을 허브 서버보다 먼저 배포한다면 N-10을 같은 릴리스에 넣는다.**
4. 직전 권고 3(개발 빌드로 런타임 1회 관측: 연결 → 자동 발급 → 이벤트 갱신, 해제 → 키 삭제)은 아직 이뤄지지 않았다. 라이브 호출이 포함되므로 사람 승인 범위에서 한다.
5. RV-09만 고친 재검토는 같은 target_id로 위임하면 축소 모드 대상이다(변경이 `otelLinkText`/compose 분기와 테스트에 국한되는 경우).

실행 액션: 커밋·push·라이브 호출·실제 settings.json 변경·코드 수정은 하지 않았다. 검증 명령(cargo test·tsc·test:ts·build)은 실행했다(build는 워크트리 `dist/`를 다시 생성했다. `dist/`는 gitignore 대상임을 `git check-ignore`로 확인했다). 스크래치 디렉터리(저장소 밖)에 탐침 테스트 1개를 만들었다. 저장소 안에서 작성·수정한 파일은 이 보고서, 재사용 페르소나 7개의 "적용 이력" 추가, `personas/INDEX.md` "최근 재사용" 갱신뿐이다(모두 미커밋). 참고: 1차 리뷰 산출물(보고서·신규 페르소나·적용 이력)도 아직 미커밋 상태다.
