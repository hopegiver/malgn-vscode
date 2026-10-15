# R20 웹 폐기 대응 정합 — 재검토(r2) 보고서

리뷰 대상: `/Users/hopegiver/workspace/malgn-vscode` main 커밋 범위 `bbc77d5..cd99ce8` · target_id `r20-otel-revoke-align`
신규분: `3bbfb45..cd99ce8`(커밋 cd99ce8 1건, 21파일 +328/−40, 캡처 7장 포함). 1차 범위(`bbc77d5..3bbfb45`)는 1차 보고서 판정을 유지하고 새 커밋과 맞물리는 곳만 다시 봤다.
직전 리뷰: `docs/reviewer/review-r20-otel-revoke-align-2026-10-15.md`(Green, Minor 3 · Nit 4 · RT-5)
리스크 범주: 인증/크리덴셜(mhd_로 mhk_ 자동 발급·폐기 대응)
등급/모드: Sensitive, 풀패널(발산형 포함). 판정 근거는 다음과 같다.
- 재검토 3요소가 모두 있다.
- 신규분은 새 커맨드·새 엔진 상태 전이·새 요청 경로를 더하지 않는다. Rust 변경은 주석 1곳과 테스트 1건뿐이다.
- 다만 위임이 풀패널을 명시했고, 프론트에 새 자동 동작이 하나 생겼다. [허브 연결 열기]가 `hub_auth_renew_now`를 부른다. 그래서 축소하지 않고 1차와 같은 5인으로 봤다.
- 노출 범위 축소는 적용하지 않았다. 축소 근거 3값이 없다.

리뷰 일자: 2026-10-15
종합 판정: 🟢 **Green** (Critical 0 · Major 0 · Minor 0 · Nit 3(신규 RV-008~010) · Rethink 1(RT-5 이월))

## 요약 (2분 규칙)
1차 지적 7건(RV-001~007)은 모두 사실대로 해소됐다. 파일:라인과 캡처로 대조했고, 해당 분기를 깨는 변이를 넣으면 테스트가 실패하는 것도 확인했다.
- RV-004로 더한 엔진 테스트는 1차에서 살아남았던 변이 M5("쓰기 실패인데 Minted")를 이제 잡는다(992 pass / 1 fail).
- RV-003의 남은 한계("[허브 연결 열기] 클릭 경로만 renew 1회")는 **수용 가능**하다.
  - `hub_auth`의 renew 사이클은 같은 기기 토큰으로 `GET /self`를 먼저 부른다. 그리고 401·409를 엔진과 같은 규칙으로 REVOKED로 확정한다(`hub_auth/renew.rs:6-8, 122-129`).
  - 따라서 서버가 연결을 실제로 폐기한 주 경로에서는 클릭 1회로 반드시 "연결 해제됨 · 다시 연결"이 된다.
  - 불일치가 남는 경우는 둘이다. renew 자체의 네트워크 실패(토스트로 드러난다)와 토큰 교체 경합이다. 둘 다 드물고 막다른 골목이 아니다. [지금 확인·갱신]이 남아 있다.

새 회귀는 없다. 남은 것은 Nit 3건이다.
- RV-008: 설정 화면에서 같은 문장이 두 번 나오고, "허브 연결 중" 안내가 그대로 남는다.
- RV-009: 실패 상태의 웹 폐기 분기를 테스트가 잡지 못한다(변이 T10 생존).
- RV-010: `openHubConnection`의 로딩 경합과, 갱신이 성공했을 때의 후속 처리.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 "역할개념" 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| design-contract-auditor | 재사용 | RV-002(계약 `:570` 이중방어, B4)와 RV-006 주석이 허브 /keys 사실과 맞는지 조항 단위로 대조한다 |
| credential-at-rest-and-transit | 재사용 | 폐기됐을 수 있는 키가 파일에 남는 자리의 표기(RV-001), 새 renew 호출이 회전을 소모하는지 본다 |
| claimed-vs-verified | 재사용 | 커밋 주장 7건을 테스트 실행과 변이(Rust 1 · TS 10)로 확인한다 |
| screen-state-honesty | 재사용 | 캡처 01~07에서 같은 화면에 서로 반대 사실이 남지 않았는지 본다 |
| zero-base-redesigner (발산) | 재사용 | 구조 의심 역할개념 그대로다. RT-5의 진척을 재평가했다 |

생략한 관점과 사유:
- `automated-retry-budget-burn`: 새 요청 경로는 클릭 1회의 `hub_auth_renew_now`뿐이다. 자동 루프가 아니다. 회전 소모는 credential 관점에서 따로 봤다(아래).
- `async-response-ordering`: `openHubConnection`의 await 순서 하나뿐이라 screen-state-honesty로 묶었다(RV-010).
- `tauri-runtime-thread-and-lock-discipline`, `remote-config-injection-boundary`: Rust 제품 코드 변경은 주석뿐이다(`git diff 3bbfb45 cd99ce8 --stat -- src-tauri`: 2파일, 테스트 +34 · 주석 +4/−3).

## 실행한 검증
| 항목 | 결과 |
|---|---|
| `cd src-tauri && cargo test` (본 저장소) | **993 passed / 0 failed / 23 ignored**. PM 실측과 일치한다. 1차의 992에 RV-004 테스트 1건이 더해졌다 |
| `pnpm test:ts` | **88 pass / 0 fail**. PM 실측과 일치한다(1차 82 → 신규 6건) |
| `pnpm exec tsc --noEmit` | 통과(exit 0) |
| Rust 변이 M5 (스크래치 APFS 클론 `scratchpad/mut`, `.env` 제거, 저장소 무변경) | `Applied::Failed` 분기에 `st.minted = Some(prefix)` 삽입 → **992 pass / 1 fail**. 실패한 테스트는 `r20_accept_confirm_that_cannot_write_the_settings_file_never_reports_minted`(`tests_r20.rs:524`)다. 1차에서 살아남은 변이를 이제 잡는다 |
| TS 변이 10종 (스크래치 사본 `scratchpad/tsmut2`, `src`·`scripts`·`src-tauri/src` 복사, 기준 88/0) | T1 `fileKeyLiveness` reconnectRequired → noSignal: **2 fail**<br>T2 quota → maybeRevoked: **2 fail**<br>T3 `webRevoked`의 hubConnected 무시: **3 fail**<br>T4 `failedStep`의 keyNote 무시: **1 fail**<br>T5 RV-005 꼬리 복원: **1 fail**<br>T6 RV-002 slim 원복: **1 fail**<br>T9 `toolStatusKind` versionUnverified → missing: **1 fail**<br>T7 설정 배지 liveness 분기 제거: **생존**. 렌더 경로라 하네스 `keyBlockCheck`와 캡처 02·03이 덮는다<br>T8 `openHubConnection`의 renew 제거: **생존**. 렌더 경로라 하네스 `r20fix-reconnect-required-opens-hub-row-already-revoked`와 캡처 05가 덮는다<br>T10 `fileKeyLiveness` failed 기본 분기의 revoked → noSignal: **생존**(RV-009) |
| 캡처 열람 | 01~07 7장을 모두 Read로 열었다. 01과 04는 같은 홈인데 OTel 행 문구가 다르다(01 "연결이 해제되었습니다" / 04 "발급할 수 없습니다 · 연결을 해제한 뒤"). 05와 06은 같은 MCP 관리 화면인데 허브 행이 다르다(05 "연결 해제됨 · [다시 연결]" / 06 "연결됨 · [지금 확인·갱신][연결 해제]"). 02와 03은 같은 설정 화면인데 배지가 다르다(02 "파일에 남음 · 폐기됐을 수 있음" / 03 "폐기됨 · 파일에 남음"). 모두 서로 다른 상태다 |
| 실행하지 않은 것 | UI 하네스 재실행·재캡처(캡처는 커밋된 것을 열람만 했다), 실제 Tauri 런타임, 라이브 허브 호출, `pnpm build`, Windows에서의 `cargo test`(RV-004 테스트는 `#[cfg(unix)]`라 Windows CI에서는 컴파일되지 않는다. 기존 0o555 테스트들과 같은 패턴이다) |
| 실행 액션 | 커밋·push·stash·checkout 없음. 저장소에서 바뀐 것은 이 보고서, 페르소나 5개 파일의 "적용 이력" append, `INDEX.md` 5행 덧붙임뿐이다. 스크래치 사본은 세션 scratchpad 안에만 있다 |

## 1차 지적 해소 대조
| ID | 판정 | 근거(파일:라인 · 확인방법) |
|---|---|---|
| RV-001 🟡 | ✅ 해소 | `src/onboarding.ts:264-307`: `FileKeyLiveness`, `webRevoked`, `fileKeyLiveness`, `fileKeyNote`를 한 곳에 뒀다. `:476-478`: `failedStep` 꼬리를 바꿨다. 재연결 필요·notDesktopToken·유출 의심 차단은 "폐기됐을 수 있어", quota는 "폐기되어"라고 말한다. 1차 개선안 그대로다. `src/views/settings.ts:633-641`: 배지가 `badge-archived`이고 라벨은 "폐기됨 · 파일에 남음"/"파일에 남음 · 폐기됐을 수 있음"이다. `:710`에 안내 줄이 있다. 웹 폐기 대기(02 지적분)도 `webRevoked`로 "폐기됨"이 된다. 정상 상태 대조군은 초록 "설정됨"을 유지한다(하네스 `r20fix-settings-key-badge-normal-applied-stays-set`). 파일 키를 지우지 않는다. 확인: Read, 변이 T1~T4, 캡처 01·02·03·04 |
| RV-002 🟡 | ✅ 해소 | `onboarding.ts:262`: `LEAK_REVOKE_FACT`는 "…이 연결로는 발급이 막혔습니다"다. `:497-498` slim은 "유출 의심 폐기 이력 때문에… 허브 연결을 해제한 뒤 다시 로그인해 연결하세요"다. 계약 `:570`(계열 폐기 없이 기록되는 경로)과 맞는다. "해제되었습니다"가 없다는 부재 단언 테스트가 있다. reconnectRequired slim과 서로 다르다는 단언(`assert.notEqual`)도 있다. 캡처 04는 OTel 행에, 06은 허브 행 "연결됨 + [연결 해제]"에 해당한다. 문구가 시키는 동작과 화면에 있는 버튼이 맞는다. 확인: Read, 변이 T6, 캡처 04·06 |
| RV-003 🟡 | ✅ 해소(한계 수용) | `settings.ts:567-578`의 `openHubConnection`은 cause가 `reconnectRequired`일 때만 renew를 1회 부른다. 이는 기존 [지금 확인·갱신]과 같은 `handleRenewHub`이고, `:590`에 배선됐다. 하네스는 스텁 REVOKED로 결과를 고정한다(캡처 05). **한계 판정**: 위 요약 참조. 클릭 1회의 `force` renew는 self가 통과할 때만 `POST /renew`(회전)까지 간다(`renew.rs:272-273`). 서버가 연결을 폐기했다면 self 401에서 끝나 회전이 일어나지 않는다. self가 통과하는 잔여 경우는 회전 1회를 쓰지만, 사용자가 [지금 확인·갱신]을 누른 것과 같은 비용이다. 확인: Read(`renew.rs`, `hub_auth/mod.rs:134-137, 209-214`), 캡처 05·06 |
| RV-004 ⚪ | ✅ 해소 | `tests_r20.rs:503-536`가 다음을 단언한다. `Ended(WriteFailed)`, `reissue_outcome`이 `Failed{ApplyFailed, transient:false}`, 파일 무변경, `pending_write` 보관. 확인: 변이 M5에서 실패 |
| RV-005 ⚪ | ✅ 해소 | `otelLinkText.ts:46-47`. featureUnavailable이면 꼬리가 없고, rateLimited는 꼬리를 유지한다(테스트). 확인: 변이 T5 |
| RV-006 ⚪ | ✅ 해소 | `otel_ingest_key.rs:87-90`의 주석이 `/keys` 수집키 패널, "맑은에이전트 관리" 배지, 웹 회전 불가, "버튼은 두지 않기로 했다"를 적는다. settings.ts 주석과 같은 사실이다. 확인: Read |
| RV-007 ⚪ | ✅ 해소 | (a) `devTools.ts:627-630`: `toolStatusKind`가 versionUnverified이면 `var(--status-warn)`을 쓴다. 사이드바 `sidebar.ts:482-483`도 같은 함수를 쓴다. 캡처 07에서 Node.js 배지가 노란색이다. 사이드바 점도 노란색이다. (b) `devToolsText.ts:13-16`이 `devToolsOrder.isVersionUnverified`를 재export한다. 함수 동일성 단언(`assert.equal(isVersionUnverified, orderIsVersionUnverified)`)이 있다. 확인: Read, 변이 T9, 캡처 07 |

## 지적 사항 (신규)
| ID | 심각도 | 페르소나 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-008 | ⚪ Nit | screen-state-honesty | `src/views/settings.ts:633-635, 710`(liveness 안내), `:120`(`placeholderManaged`), `:713`(`issueManaged`), 캡처 02 | 캡처 02 열람, Read | 재연결 필요 상태의 설정 > OTel에 두 가지 문제가 있다. (a) 위 OTel 카드가 "설정 파일에 남아 있는 수집키(mhk_AbCd…)는 허브에서 함께 폐기됐을 수 있어…"라고 말한다. 바로 아래 수집키 영역이 같은 문장을 마스킹만 빼고 다시 말한다. 화면 하나에 같은 문장이 두 번 나온다. (b) 카드는 "이 PC의 맑은에이전트 연결이 해제되었습니다"라고 하는데, 입력칸 placeholder "허브 연결 중에는 앱이 수집키를 자동 관리합니다"와 `issueManaged` 안내 "허브에 연결되어 있는 동안은…"이 그대로 남는다. hub_auth 기준으로는 아직 CONNECTED라 거짓은 아니다. 1차 RV-001 본문이 언급한 부분인데 개선안 범위 밖이라 남았다 | (a) 수집키 영역 안내는 카드가 같은 사실을 말하지 않는 경우에만 렌더한다. 예: `failure.cause`가 없는 web-revoked는 카드 문구가 다르므로 유지한다. 또는 수집키 영역 쪽만 남기고 카드 꼬리를 줄인다. (b) liveness가 maybeRevoked이면 managed 안내 대신 "허브 연결 상태를 확인한 뒤 앱이 새 수집키로 교체합니다" 한 줄을 쓴다. 문구 2곳 수정이다. 비용 근거: 분기 재료(`liveness`)가 이미 같은 함수 안에 있다 |
| RV-009 | ⚪ Nit | claimed-vs-verified | `src/onboarding.ts:289-290`(`fileKeyLiveness`의 `state==='failed'` default 분기) | 변이 T10(→ 항상 noSignal) → 88 전부 통과 | 실패 원인이 reconnect/blocked/notDesktopToken이 아니고(예: unreachable, hubRejected), 동시에 웹 폐기 판정(`confirmRequired`/`revokedByHub`)이 남아 있는 상태가 있다. 이 상태에서 "폐기되어 수집되지 않습니다"를 고르는 분기를 아무 테스트도 고정하지 않는다. 기존 테스트 "failed 상태가 revokedByHub보다 먼저 보인다"로 보아 이 조합은 도달 가능하다. 다만 그 테스트는 reconnectRequired 원인만 쓴다. 이 분기가 깨지면 홈이 다시 "적용된 수집키"를 말한다(RV-001 재발) | `fileKeyLiveness(failed('unreachable', { revokedByHub: true }))`가 `'revoked'`임을 단언하는 1줄을 더한다. `step(...).detail`에 "적용된 수집키"가 없음도 함께 단언한다 |
| RV-010 | ⚪ Nit | screen-state-honesty | `src/views/settings.ts:574-577`(`openHubConnection`), `:1286-1290`(renew 뒤 ingest 재조회 없음) | Read(코드 대조, 미재현) | (a) 클릭 순간 `state.hubAuth.loading`이 true이면 renew를 건너뛴다. 이때 `loaded`는 false이고 `status`는 null이다. 시작 직후 첫 `loadHubAuth`가 끝나기 전에 누른 경우가 그렇다(`main.ts:267`). 그러면 1차 RV-003 상태로 돌아간다. (b) 잔여 경우에는 반대 방향 불일치가 남는다. renew가 성공해 허브 행이 "연결됨"인데, `runHubAuthAction`은 renew 뒤 `loadIngestLink`를 부르지 않는다(connect/disconnect만 부른다). 그래서 OTel 카드는 다음 점검까지 "연결이 해제되었습니다"를 계속 말한다. 둘 다 드물고 막다른 골목이 아니다 | (a) loading 중이면 그 로드를 기다린 뒤 판단한다. 예: 진행 중인 promise를 보관해 await한다. (b) `openHubConnection`에서 renew 뒤에도 `connected`이면 `loadIngestLink()` 또는 기존 재점검 커맨드(`otel_link_recheck`)를 1회 부른다. 비용은 2~4줄로 본다. 근거: 두 함수 모두 이미 있다. 재점검 커맨드가 요청을 몇 건 쓰는지는 세지 않았다(미확인 추정) |

## 페르소나별 관점
- **design-contract-auditor 🟢**: RV-002 새 문구가 계약 `:570`·`:1080` B4의 "계열 생존, GET blocked" 의미와 맞는다. 문구가 지시하는 첫 동작(연결 해제)이 캡처 06의 [연결 해제] 버튼과 맞물린다. RV-006 주석은 허브 /keys 현재 사실과 settings.ts 주석 양쪽에 맞는다. 401 → self 401 경로 문구(`otel_link.rs:154`)는 바뀌지 않았다(diff 0줄).
- **credential-at-rest-and-transit 🟢**: 폐기됐거나 폐기됐을 수 있는 키를 더 이상 "적용된/설정됨"으로 부르지 않는다. 키를 자동 삭제하지 않는 결정도 유지됐다. 1차 기각 사유와 같다. 새 renew 호출은 서버가 연결을 폐기한 경우 회전을 쓰지 않는다. 화면 문구와 테스트 픽스처에는 마스킹된 앞 8자만 쓰인다(`mhk_AbCd…`).
- **claimed-vs-verified 🟢**: 커밋 메시지 7개 항목을 모두 코드와 테스트로 확인했다. 변이 11종 가운데 8종이 실패했다. 살아남은 3종 중 2종(T7·T8)은 렌더 경로로, 커밋에 포함된 하네스 시나리오와 캡처가 덮는다. 단, 하네스는 재실행하지 않았다. 나머지 1종이 RV-009다.
- **screen-state-honesty 🟢**: 1차의 두 간극이 닫혔다. 하나는 "폐기"와 "적용된/설정됨"이 함께 나오던 것(RV-001)이고, 다른 하나는 OTel "해제" 대 허브 행 "연결됨"(RV-003 주 경로)이다. 남은 것은 같은 문장 중복과 managed 안내 잔존(RV-008), 드문 경합(RV-010)이다.
- **zero-base-redesigner (발산)**: 아래 RT-5.

## 🔵 Rethink
| ID | 현재 구조 | 대안 구조 | 이유 | 비용 |
|---|---|---|---|---|
| RT-5 (이월, 일부 진척) | 키 생사 판정이 프론트 `fileKeyLiveness` 한 곳으로 모였다. 1차 대안의 `keyLiveness` 3값 규칙과 거의 같다. 다만 여전히 프론트가 상태 조합(`state`·`failure.cause`·`blockedReason`·플래그 3개·`hubConnected`)으로 다시 추론한다. hub_auth 쪽은 엔진 판정을 모르고, 클릭 시 renew로 뒤늦게 맞춘다(RV-003, RV-010) | 1차와 같다. 백엔드 `compose_status`가 `keyLiveness`를 계산해 싣는다. 엔진이 `Phase::Reconnect`에 도달하면 hub_auth에 재평가를 요청한다(renew 진입점 재사용). 그러면 `openHubConnection`의 조건부 renew와 RV-009·RV-010이 구조적으로 사라진다 | 판정을 프론트 한 곳으로 모은 것은 정확히 맞는 방향이다. 그런데 다음 폐기 사유가 추가되면 Rust enum과 TS switch를 함께 고쳐야 한다. 그 사실을 컴파일러가 알리지 않는다. TS default가 noSignal로 떨어지기 때문이다 | 작음~중간(미확인 추정). 근거: 이번에 TS 규칙이 확정됐으므로 Rust로 옮기는 것은 규칙 이식이다. hub_auth 전파 비용은 세지 않았다 |

## 기각된 지적
| 후보 | 기각 사유 |
|---|---|
| "RV-007 배지가 `badge-archived` 클래스에 인라인 color만 덮어써서 테두리가 회색으로 남는다" | 캡처 07에서 Node.js "버전 미확인" 배지의 글자와 테두리가 모두 노란색이다. 실물과 다르다 |
| "[허브 연결 열기] 자동 renew가 회전 예산을 태운다" | renew 사이클은 self를 먼저 부른다(`renew.rs:6-8`). reconnectRequired의 주 원인(self 401/409)에서는 회전 전에 끝난다. 클릭 1회에 한정되고 자동 루프가 없다. 잔여 경우의 비용은 기존 [지금 확인·갱신]과 같다 |
| "RV-004 테스트가 root로 돌면 권한이 무시돼 거짓 통과한다" | root라면 쓰기가 성공해 Minted가 나오고, 테스트는 실패 쪽으로 깨진다. 조용한 통과가 아니다. CI의 rust-test는 macOS·Windows 러너다(`ci.yml:82`). `#[cfg(unix)]` 패턴은 기존 0o555 테스트 4개 파일과 같다 |

## 잘 된 점
- 1차 개선안을 문자 그대로 옮기는 데 그치지 않았다. 판정을 `fileKeyLiveness` 한 함수로 모아 홈 꼬리와 설정 배지가 같은 말을 하게 했다. 덕분에 RT-5의 절반이 이번에 이뤄졌다.
- 대조군 시나리오(정상 적용은 초록 "설정됨" 유지)를 하네스에 넣었다. 배지를 일괄로 바꾸는 과잉 수정이 아님을 스스로 증명한다.
- RV-004 테스트가 Minted 부재뿐 아니라 파일 무변경과 `pending_write` 보관까지 단언한다. RT-4가 지키려던 계약 전체를 고정한다.
- RV-002 테스트가 유출 의심 slim과 재연결 필요 slim이 서로 달라야 한다는 것까지 단언한다. 두 상태가 다시 같은 문구로 합쳐지는 회귀를 막는다.
- RV-003을 백엔드 구조 변경 없이 기존 커맨드 재사용으로 닫았다. 하네스는 라이브 호출 없이 스텁으로 고정했다.

## PM 권고
1. **출시 판단**: Green. 출시를 막는 지적이 없다. 버전 범프·STATUS.md 커밋을 진행해도 된다.
2. **다음 라운드에 얹기(Nit, 모두 소량)**: RV-009 테스트 1줄, RV-010(b) renew 뒤 재조회 1회, RV-008 문구 2곳. RV-010(a)는 RV-010(b)와 같은 함수를 손볼 때 함께 한다.
3. **백로그**: RT-5. 다음 폐기 사유나 계약 개정 때 Rust로 옮긴다.
4. **INDEX/적용 이력**: 1차 "게이트 예외"분과 이번 2차분을 이번 위임 범위에서 함께 기록했다(아래 파일 목록). PM 커밋에 포함하면 된다.
5. 하지 못한 것: UI 하네스 재실행(캡처는 열람만 했다), 실제 Tauri 런타임, 라이브 허브, Windows `cargo test`.
