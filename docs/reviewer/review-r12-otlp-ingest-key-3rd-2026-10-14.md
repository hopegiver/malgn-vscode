# R12 3차 재검토(확인): 허브 OTLP 수집키 입력·저장·삭제

리뷰 대상: target_id `r12-otlp-ingest-key` / 누적 범위 `5ccc00b..ff7e41e` (반영분 `f0a03b9..ff7e41e`; `d6ec05c`·`f0a03b9`는 1·2차 보고서 커밋이라 코드 판정 대상 아님)
- 등급: Sensitive, 3차 재검토(확인). 패널은 1·2차와 같은 6인(보안·발산형 포함)이다. 위임 메시지가 "축소 금지"를 명시해 축소/증분 모드 판정 없이 풀패널로 처리했다
- 리스크 범주: 크리덴셜 저장·전송(인증)
- 직전 리뷰: `docs/reviewer/review-r12-otlp-ingest-key-2nd-2026-10-14.md` (Green, Minor RV-006 신규, Nit N-7·N-8, Rethink RT-3)
- 일자: 2026-10-14 / 리뷰어: reviewer
- 실행 액션: 없음. 코드·허브 수정, 커밋, push, 네트워크 쓰기, stash/checkout/reset을 하지 않았다. 실행한 것은 `pnpm build`, `node scripts/pre-push-sensitive-scan.mjs`, 읽기 전용 git/grep뿐이다. 이 보고서를 쓰고, 페르소나 6개의 "적용 이력"과 INDEX.md "최근 재사용"에 한 줄씩 덧붙였다(커밋은 PM 몫)

## 종합 판정: 🟢 Green (조건 없음) — 누적 범위 `5ccc00b..ff7e41e` 최종

Critical 0 / Major 0 / Minor 0 / Nit 1(신규 N-9) + 이월 Nit 8(N-1~N-8) / Rethink 이월 3(RT-1~RT-3)

RV-006은 해소됐다. 새 문구는 허브 `ingest-credentials.js`의 권한 구조와 세 가지 모두 맞는다. 개인 키는 본인 세션으로만 발급된다. 관리자는 공용 워크스테이션 키만 발급한다. 폐기는 소유자나 관리자가 한다. 반영 커밋 `ff7e41e`는 `src/views/settings.ts`(주석과 문구 2개), 하네스 단언 1블록, 스크린샷 10장만 바꿨다. 로직, Rust, 다른 파일은 건드리지 않았다. 누적 범위 전체로 보면 1~3차의 Major·Minor 6건(RV-001~006)이 모두 닫혔다. 남은 것은 Nit와 Rethink뿐이라 병합 조건이 아니다.

## 페르소나 재사용 판정

착수 전에 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 다시 대조했다. 이번 반영은 문구와 하네스 단언뿐이다. 새로 생긴 리스크 표면은 없다. 그래서 신규 페르소나는 만들지 않았다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-credential-at-rest-and-transit.md` (보안) | 재사용 | 저장·전송 로직이 바뀌지 않았는지 확인하는 일이다. 폐기 수단(관리자 폐기)의 실재를 다시 확인하는 일도 C5 그대로다 |
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | 바뀐 문구 2개에 원문이나 값이 끼워 넣어지지 않았는지 보는 일이 역할개념 그대로다 |
| `persona-design-contract-auditor.md` | 재사용 | 앱 문구를 허브 정본 라우트의 권한 분기와 조항 단위로 대조하는 일이 RV-006의 본체다 |
| `persona-first-run-newcomer.md` | 재사용 | 바뀐 안내로 완주를 다시 시뮬레이션하는 일이 역할개념 그대로다 |
| `persona-claimed-vs-verified.md` | 재사용 | 커밋 메시지·주석·하네스 단언이 주장하는 범위와 실제 범위를 대조하는 일이다(N-9가 여기서 나왔다) |
| `persona-zero-base-redesigner.md` (발산형) | 재사용 | 발산형은 필수이고 역할개념이 여전히 유효하다 |

## 직접 확인한 것

- `git diff f0a03b9..ff7e41e` 전체를 읽었다. `git diff --name-only`로 보면 12개 파일이다. `src/views/settings.ts`, `scripts/ui-harness/flows/otelIngestKey.mjs`, `docs/screenshots/r12-ingest-key/otelIngestKey/*.png` 10장이 전부다. 커밋은 `ff7e41e` 하나다(`git log 5ccc00b..ff7e41e`: 889919f, 640352b, d6ec05c, f0a03b9, ff7e41e)
- `git diff --stat 5ccc00b..ff7e41e -- . ':!docs'`: 코드 12개 파일, +1846/-26. 2차 판정 범위(`640352b`) 이후 docs 밖에서 바뀐 것은 `ff7e41e`의 2개 파일뿐이다
- `src/views/settings.ts:100-125`(주석 + `INGEST_KEY_COPY`), `:405-475`(`renderIngestKeyBlock`)를 읽었다. 하네스 `otelIngestKey.mjs:60-125`(`unset-telemetry-on` 시나리오)도 읽었다
- `pnpm build`: 성공(`✓ built in 65ms`). 빌드 후 `git status --short`: 변경 없음
- `node scripts/pre-push-sensitive-scan.mjs`: exit 0
- `grep -rn "발급을 요청|발급 요청" src src-tauri/src scripts`: 옛 문구는 남아 있지 않다. 남은 것은 주석 2곳(`settings.ts:105` "'발급 요청'이라 쓰지 않는다", 하네스 `:110`)과 하네스의 부정 단언(`:115`)뿐이다
- 스크린샷 2장을 직접 열었다. `unset-telemetry-on-01-01-unset-warning.png`(경고 "허브 웹에 수집키 발급 화면이 아직 없으니 발급 방법은 허브 관리자에게 문의한 뒤…", issue 힌트 새 문구, 발급 버튼 없음)와 `set-masked-01-01-set-masked.png`(설정됨 `mhk_AAAA…`, 경고 없음, 새 issue 힌트, 수집키 삭제 버튼)다. 두 장은 서로 다른 상태이고, 모두 새 문구로 다시 캡처됐다(캡처 시각 17:10)
- 허브(읽기 전용, `malgnai-public` HEAD `2768bf2`, 2차와 같은 커밋) `server/api/ingest-credentials.js`를 grep과 부분 열람으로 확인했다. `POST /`(90-97)는 `subjectType:'user', userId: c.get('userId')`이고, `POST /:id/rotate`(112-115)는 본인 user 키만 허용한다. `DELETE /:id`(156-160)는 `isOwner || isAdmin(administrator)`이다. `adminIngestCredentials`(180-202)는 `requireAdmin` 아래에서 목록 GET과 `subjectType:'shared_workstation'` 발급만 둔다. `app/` 전체에서 `ingest-credentials` 호출은 0건이다

## 직전 지적 해소 여부

| ID | 직전 심각도 | 상태 | 근거(파일:라인) | 확인방법 |
|---|---|---|---|---|
| RV-006 | 🟡 Minor | **해소** | `settings.ts:111`(issue): "이 화면에서 발급할 수 없고 허브 웹에 수집키 발급 화면도 아직 없습니다. 발급 방법은 허브 관리자에게 문의하세요." 발급 주체를 관리자로 단정하지 않는다. `:122`(warnUnsetHub)도 같은 취지로 바뀌었다. 허브와 대조하면 (1) "허브 웹에 발급 화면 없음"은 사실이다(`app/` 0건). (2) "관리자가 대신 발급 못 함"(주석 `:103-104`)도 사실이다. admin 라우터는 `shared_workstation` 주체만 발급한다(202행). 개인 키 발급은 `userId: c.get('userId')`다(97행). (3) "폐기는 관리자도 가능"(주석 `:105`, revoke 문구 유지)도 사실이다(`isAdmin` 160행). 공용 키를 쓰면 사용량이 직원이 아니라 워크스테이션 앞으로 집계된다는 주석과 커밋 메시지의 서술은 주체 모델과 맞는다. 관리자 발급 키는 `userId: null, workstationEmployeeId`(202행)이고, 감사 메타의 주체 키도 `w:…`다(68행). 집계 쿼리 쪽 귀속은 열지 않았고, 2차의 설계 §5.6 대조에 근거한다. 하네스 `:112-118`은 새 문구 두 조각을 긍정 단언하고, 옛 "관리자에게 수집키 발급을 요청"을 부정 단언하며, 폐기 요청 문구를 유지 단언한다 | diff 정독, 허브 라우트 grep·열람, 스크린샷 2장 |
| N-1~N-8 | ⚪ Nit | 이월(미반영) | 반영 diff가 해당 위치를 건드리지 않았다. N-7(`otel_settings.rs` 주석)과 N-8(`store.rs:282` pragma)은 `ff7e41e`에 포함되지 않았다 | `git diff --name-only f0a03b9..ff7e41e` |
| RT-1~RT-3 | 🔵 Rethink | 이월 | 허브 설계 후보로, 이번 범위 밖이다 | — |

## 반영 범위 확인 (문구·하네스·캡처 외 변경 없음)

- `src/views/settings.ts`: +6/-3. 주석 4줄 교체(`:103-106`), `issue` 문자열 1개, `warnUnsetHub` 문자열 1개만 바뀌었다. 키 이름, 렌더 조건(`:418-430`), 핸들러, import는 그대로다
- `scripts/ui-harness/flows/otelIngestKey.mjs`: +9/-2. `unset-telemetry-on` 시나리오의 if 블록 하나와 증상 문구만 바뀌었다. 다른 시나리오와 `assertNoRawKey`는 그대로다
- 스크린샷 10장: 바이트 크기만 달라졌다(같은 파일명 10개, 추가·삭제 없음). 2장을 열어 새 문구가 반영된 재캡처임을 확인했다. 나머지 8장은 열지 않았다(아래 "생략한 것")
- Rust(`src-tauri/`), `otelApi.ts`, 스캐너, fixtures는 바뀌지 않았다

## 지적 사항 (3차 신규)

| ID | 심각도 | 위치 | 문제 | 개선안 | 확인방법 |
|---|---|---|---|---|---|
| N-9 | ⚪ Nit | `scripts/ui-harness/flows/otelIngestKey.mjs:113`, `src/views/settings.ts:111`·`:122` | 하네스 첫 정규식 `/허브 웹에 수집키 발급 화면이 아직 없/`은 issue 힌트(`…발급 화면**도** 아직 없습니다`)와 맞지 않고 경고 `warnUnsetHub`(`…발급 화면**이** 아직 없으니`)와만 맞는다. 두 번째 정규식 `발급 방법은 허브 관리자에게 문의`도 두 문구 모두에 들어 있다. 그래서 이 단언은 **경고가 떠 있는 이 시나리오에서는** issue 힌트의 "허브 웹 발급 화면 없음" 부분이 빠져도 통과한다. 경고가 뜨지 않는 상태(설정됨·사내 collector)에서는 issue 힌트 문구를 단언하지 않는다. 현재 화면은 정상이다(스크린샷 2장 확인). 회귀 탐지력만 조금 약하다 | 긍정 단언 대상을 `#otel-ingest-key-issue-hint`의 innerText로 좁히고 정규식을 `발급 화면[이도] 아직 없`으로 바꾼다. 또는 `set-masked` 시나리오에도 같은 issue 힌트 단언을 하나 둔다 | 하네스 코드와 문구 대조, 렌더 구조(`settings.ts:458-468`, 경고와 힌트가 같은 `#otel-ingest-key-block` 안에 있음) |

## 기각된 지적

| 후보 | 기각 사유 |
|---|---|
| "'발급 방법은 관리자에게 문의'가 경고와 힌트에 두 번 나와 중복이다" | 경고는 미설정+허브 엔드포인트일 때만 뜬다(`settings.ts:418`). 힌트는 상태와 무관한 상시 안내다. 미설정 화면에서만 겹치고, 그 화면에서는 다음 행동을 강조하는 효과가 있다. 결함이 아니다 |
| "허브 `MyUsageBoard.vue:24`가 인증키를 `/keys`에서 확인하라고 해, 앱 문구('/keys의 mhd_는 수집키 아님')와 충돌한다" | 허브 문구는 "PC 연동과 인증키"를 가리키고, 수집키 발급을 `/keys`로 안내하지 않는다. 앱 문구와 모순되지 않는다. 설사 문제라 해도 허브 저장소 소관이라 이번 범위 밖이다 |
| "커밋이 문구 외 로직을 바꿨을 수 있다" | `git diff f0a03b9..ff7e41e`에서 settings.ts 변경은 주석과 문자열 리터럴 2개뿐이다. 하네스 변경은 단언 1블록뿐이다. 빌드도 통과한다 |

## 페르소나별 관점

### credential-at-rest-and-transit (보안, 수렴, 재사용)
- 저장·전송·백업 로직은 바뀌지 않았다(Rust 무변경). 2차의 C4·C5 충족 판정이 그대로 유지된다. 폐기 수단은 관리자 `DELETE /:id`(administrator 허용, 160행)로 실재하고, revoke 문구는 이 사실과 맞게 유지됐다.
- 새 문구가 공용 워크스테이션 키 우회를 유도하지 않게 된 점은 보안 측면에서도 개선이다. 개인 PC에 공용 키를 복사하면 귀속을 회피하게 된다(설계 L7과 같은 모양). 2차 문구 "관리자에게 발급 요청"은 그 유도 여지를 남겼었다.

### displayed-text-redaction-vs-diagnostic-value (수렴, 재사용)
- 바뀐 두 문구는 상수 리터럴이고 값을 보간하지 않는다. 마스킹 표시 `mhk_AAAA…`는 그대로다(스크린샷 set-masked).

### design-contract-auditor (수렴, 재사용)
- 앱 문구·주석의 세 주장(웹 발급 화면 없음 / 관리자 대리 발급 불가 / 관리자 폐기 가능)을 허브 라우트 분기와 하나씩 대조했고 모두 일치한다. 2차에서 어긋났던 유일한 조항이 닫혔다.

### first-run-newcomer (수렴, 재사용)
- 완주를 다시 시뮬레이션했다. 미설정 경고 → "허브 웹에 발급 화면이 없으니 관리자에게 방법 문의" → 관리자가 본인 로그인 상태의 발급 절차를 안내 → `mhk_` 붙여넣기 → 저장 → 마스킹 + 새 세션 안내. 사용자는 더 이상 관리자가 키를 "대신 만들어 줄 것"이라 기대하지 않는다. 관리자 쪽 절차 문서(허브 runbook)가 없다는 점은 2차와 같이 교차 저장소 항목으로 남는다.

### claimed-vs-verified (수렴, 재사용)
- 커밋 메시지의 주장 세 가지(문구 교체, 폐기·mhd_ 안내 유지, 하네스·캡처 정합)를 diff와 허브 코드로 확인했다. 과대 주장은 없다. 하네스 단언의 탐지 범위가 주장보다 조금 좁다는 점만 N-9로 남겼다.

### zero-base-redesigner (발산, 재사용)
- 새 Rethink는 없다. 이번 문구는 RT-3(숨김 플래그 `/keys` 탭 파일럿 개방)이 이뤄질 때까지의 정확한 임시 안내다. 복귀 지점은 `INGEST_KEY_COPY` 주석(`settings.ts:107-108`)에 그대로 남아 있다. RT-1·RT-3 중 하나를 고르면 issue 문구와 "문의" 경로가 함께 사라진다는 2차 판단을 유지한다.

## 트레이드오프

- "발급 방법을 문의"는 정확하지만 구체적이지 않다. 파일럿 규모(관리자 = 개발자 본인)에서는 충분하다는 2차 판단을 유지한다. 구체성은 허브 runbook이나 RT-3으로 회복한다.

## 잘 된 점

- 권한 모델 근거(본인 세션 전용 발급, 공용 키의 귀속 문제, 관리자 폐기 가능)를 주석에 현재형으로 적었다(`settings.ts:103-106`). 다음에 문구를 고칠 사람이 허브 코드를 다시 뒤지지 않아도 된다.
- 하네스가 새 문구의 존재뿐 아니라 옛 문구의 **부재**까지 단언한다(`:115`). 그래서 1차 권고 문구로 되돌아가는 회귀가 바로 잡힌다.
- 폐기 문구를 같이 바꾸지 않고 사실인 부분은 유지했다. 지적 범위만 정확히 고쳤다.
- 스크린샷 10장을 모두 다시 캡처해 증거와 코드가 같은 커밋 기준으로 맞춰졌다.

## PM 권고

1. Green(조건 없음)이다. 누적 범위 `5ccc00b..ff7e41e`는 병합 가능하다. 이것은 사람 승인이 필요한 Sensitive 산출물에 대한 reviewer 풀패널 판정이다. 병합 승인 자체는 별도로 받아야 한다.
2. N-9는 하네스 한 줄 수정이라 비용이 낮다. 코드 확인 기준으로, 범위가 하네스 파일 하나로 한정된다. 다음에 이 하네스를 건드릴 때 함께 처리하면 된다.
3. N-1~N-8과 RT-1~RT-3는 2차 권고 그대로 백로그나 허브 설계 후보로 유지한다. 허브 관리자 발급 절차(runbook)는 교차 저장소 항목이다.

## 생략한 것

- UI 하네스를 다시 실행하지 않았다(`pnpm build`만 실행). 재캡처된 10장 중 2장만 열었다. 나머지 8장(clear-flow 2장, save-error, insecure, set-masked 02·03, unrecognized, internal-collector)은 열지 않았다. 바뀐 문구가 모든 상태에서 같은 상수로 렌더되므로(`settings.ts:464`), 하네스 코드와 렌더 코드로 판단했다.
- `cargo test`는 다시 돌리지 않았다. Rust가 바뀌지 않았기 때문이다(2차 777 passed 유지로 간주).
- 실제 허브로의 OTLP 전송, Claude Code 실기동, Windows 실기는 2차와 같은 사유로 확인하지 않았다.
