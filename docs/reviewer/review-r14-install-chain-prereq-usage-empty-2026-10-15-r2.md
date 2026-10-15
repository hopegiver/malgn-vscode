# R14 연쇄 설치·자율 작업 사전조건·사용량 빈 상태 재검토 보고서 (2차, 모드: 축소)

리뷰 페르소나 패널: persona-delegated-authority-blast-radius.md, persona-claimed-vs-verified.md, persona-heterogeneous-machine-operator.md, persona-screen-state-honesty.md, persona-zero-base-redesigner.md(발산형)
리뷰 대상: target_id `r14-install-chain-autonomy-prereq-usage-empty` — `/Users/hopegiver/workspace/malgn-vscode` main `d6ec70f..8cbce95`(반영분 `16efbdd..8cbce95`, 커밋 8cbce95)
직전 리뷰: `docs/reviewer/review-r14-install-chain-prereq-usage-empty-2026-10-15.md` (판정: 🟢 Green, Minor 5·Nit 4·Rethink 3)
리스크 범주: 사용자 머신 설치 명령 실행 동의 경계(P2-9) + 상태 표시 정직성(P2-4·P2-6). 직전 값("사용자 머신 설치 실행(동의 경계). P2-4·P2-6은 표시 전용")과 같은 범주다
리뷰 일자: 2026-10-15
등급: Sensitive. 노출 범위 축소는 적용하지 않음
종합 판정: 🟢 Green — `d6ec70f..8cbce95` 전체 기준 (Critical 0, Major 0, Minor 1(이월 RV-002), Nit 7(이월 4 + 신규 3), Rethink 3(이월))

모드 판정 근거:
- 동일 대상 인정 4조건: 같은 target_id / 직전 리뷰와 같은 날(7일 이내) / 리스크 범주 불변 / 대상 파일 실질 중첩(`src/views/devTools.ts`, `src/devToolsOrder.ts`, `src/autonomyPrereq.ts`, 하네스 3개 흐름이 1차 대상과 같다) — 충족.
- 풀패널 강제 승격 조건: 직전 Critical/Major 없음 / 새 실행경로 없음(`planBatchInstall`은 기존 `handleUpdateAll`의 동의 계산을 순수 함수로 옮기고 조건 하나를 더한 것이고, `handled` 변경은 기존 `offerInstallChain`의 입력만 바꾼다) / 다른 Sensitive 하위도메인 진입 없음 / 2차 / 풀패널 요청 없음 — 미해당.
- 새 리스크 표면 없음 → 축소 모드. 다만 위임이 짚은 두 지점(버전 미확인 일반화, handled 집합 변경)은 동의 경계 페르소나로 깊게 봤다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 1차와 같은 리스크 표면이라 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-delegated-authority-blast-radius.md | 재사용 | 반영이 동의 경계를 약화시키지 않았는가(일반화·handled 변경) — 역할개념 그대로 |
| persona-claimed-vs-verified.md | 재사용 | "4건 해소" 주장과 새 단언이 수정 전 코드에서 실제로 실패하는지 대조 |
| persona-heterogeneous-machine-operator.md | 재사용 | mac Homebrew run 모양, nvm 안내 한정이 실제 탐지 경로와 맞는가 |
| persona-screen-state-honesty.md | 재사용 | 배너·빈 상태·동의 문구가 실제 일어날 일을 단정 없이 말하는가 |
| persona-zero-base-redesigner.md | 재사용(발산형) | 발산형 필수. RT-1~3 유효성 재확인 |

## 직전 지적 해결 여부
| ID | 심각도 | 직전 지적 | 해결 여부 | 근거 |
|---|---|---|---|---|
| RV-001 | 🟡 Minor | mac Homebrew Node 버전 미확인(run) 모양에서 배치 동의 문구가 실행과 어긋남 | ✅ 해소 | 원본: `16efbdd:src/views/devTools.ts:374` blocked = `!runnableIds.has(dep)`만. 신규: `src/devToolsOrder.ts:116-131` `planBatchInstall`이 `unverifiedIds.has(dep)`도 blocked로 넣고, `:135-150` `describeBatchConsent`가 "업데이트를 시도합니다(버전 미확인)"를 따로 적는다. `src/views/devTools.ts:359,382-385`가 이 둘을 쓴다. 하네스: `devToolsOrder.mjs`의 `chain-mac-node-unverified-run` 픽스처가 `actionKind: win ? 'manual' : 'run'`(개선안 ②). 캡처 `r14-p29-10`(동의 대상 Node.js 하나, 의존 도구는 "이번에는 설치하지 않으며"), `r14-p29-11`(Node 확인 뒤 3개 이름을 적은 이어서 설치) 열람 |
| RV-002 | 🟡 Minor | stuck 토스트 원인 일괄 문구 | ⏸ 이월(의도) | `src/views/devTools.ts:511` 무변경 확인 |
| RV-003 | 🟡 Minor | 플러그인 배너가 읽기 실패를 "설치되어 있지 않아"로 단정 | ✅ 해소(단기안) | 원본 `16efbdd:src/autonomyPrereq.ts:70` "설치되어 있지 않아". 신규 `:72` "설치된 플러그인 목록에서 malgn-agent 플러그인을 찾지 못했습니다". 하네스 check-failed 주석도 "IPC 수준 실패에서만" 명시(`autonomousTasks.mjs:537-539`, 해당 문구 `:538`). 캡처 `r14-p24-04` 열람. 백엔드 구분(근본안)은 차기 |
| RV-004 | 🟡 Minor | usage not-installed 캡처에서 상태바 "claude 2.1.0"과 본문 미설치가 모순 | ✅ 해소 | `usage.mjs` `withClaudeMissing`이 claude를 `path:null, installed:false`로 맞추고 `expectStatusBarAgrees`가 양방향 단언(installed-no-history 장면은 `claudeFound:true` 경로로 regex가 실제로 버전을 잡는지 같이 확인). 캡처 `r14-p26-01`, `r14-p26-07` 열람 — 상태바에 "node 22.9.0"만 있고 claude 세그먼트 없음 |
| RV-005 | 🟡 Minor | mac Node 안내의 "다시 확인하면 이어서 묻는다"가 nvm 설치에서 거짓 | ✅ 해소 | `src/views/devTools.ts:826` "단, 앱이 감지하는 설치(Homebrew 또는 공식 설치 프로그램)일 때만 … nvm·fnm·volta … 터미널에서 직접 설치". 캡처 `r14-p29-06` 열람 |
| RV-006~009 | ⚪ Nit | 연쇄 건너뜀 비대칭, 버전 미확인 배지, Windows 요약 이중 동의, Claude 설치 판정 정의 불일치 | ⏸ 이월(의도) | 해당 라인 무변경 |
| RT-1~3 | 🔵 | environmentReadiness 단일 모델 / 탐지 정의 일원화 / 설정 계획 패널 | ⏸ 이월(의도) | — |

### 새 단언의 판별력 (구현자가 확인하지 못한 부분)
`git worktree add --detach`로 16efbdd를 scratchpad에 풀고, 하네스 3개 파일만 8cbce95 것으로 덮은 뒤 별도 vite(1439, 자기 pid만 종료, worktree 제거 완료)에 대해 돌렸다. main 워킹트리는 건드리지 않았다.

| 흐름 | 16efbdd 소스 + 새 하네스 | 8cbce95(main, 1420) | 해석 |
|---|---|---|---|
| devToolsOrder | 21개 중 bugs=7: `chain-mac-node-unverified-run` 5건(동의창 "다음 4개 도구를 설치합니다: Claude Code, Node.js, pnpm, Wrangler" 등), `chain-mac-manual-node-then-recheck` 1건(RV-005 문구), `pure-order-functions` 1건(`planBatchInstall is not a function`) | bugs=0 | RV-001·RV-005 단언은 수정 전 코드에서 실패한다 |
| autonomousTasks | 19개 중 bugs=1: `prereq-plugin-missing`(옛 "설치되어 있지 않아" 문구) | bugs=0 | RV-003 단언 판별력 있음 |
| usage | 10개 bugs=0 | bugs=0 | RV-004는 소스가 아니라 픽스처 수정이라 소스 차이로는 갈리지 않는다(정상). 새 단언의 판별력은 installed-no-history의 `claudeFound:true` 경로가 같은 regex로 버전을 잡아 통과하는 것으로 간접 확인 |

`chain-mac-node-unverified-still-unknown`·`-declined`는 옛 코드에서도 통과한다. 판별 단언이 아니라 회귀 방지 단언이다(옛 코드도 실행 단계에서 의존 도구를 건너뛰었으므로). 결함은 아니다.

## 동의 경계 약화 여부 (위임 지정 확인점)
| 확인점 | 판정 | 근거 |
|---|---|---|
| "버전 미확인"을 선행이 아닌 도구까지 일반화(`unverifiedTargets`) | 약화 없음 | 실행 집합은 그대로다. 16efbdd에서도 `installed=false && path && run`인 도구는 `installTargets`→askable에 들어갔고, 지금은 같은 askable(`devTools.ts:382`)에 들어가며 이름만 "업데이트(버전 미확인)" 문장으로 옮겼다. 백엔드는 도구 종류와 무관하게 경로가 있으면 install→update로 위임하므로(`actions.rs:237-239`) 모든 도구에서 문구가 사실이다 |
| blocked 확대 | 좁아짐(안전 방향) | 새 조건은 실행 대상을 빼기만 한다. 제외된 도구는 `batchSkipReason`에 사유가 남는다(`devTools.ts:374-378`) |
| `offerInstallChain`에 넘기는 handled에서 blocked 제거 | 약화 없음 | 제거된 도구가 다시 제안되는 길은 `planInstallChain`(`devToolsOrder.ts:175-187`)뿐이다. 이 함수는 prev→next에서 선행 도구가 false→true로 바뀐 경우만 제안한다. 제안은 이름을 적은 `confirmDialog` 동의를 거치고(`devTools.ts:515-518`), 실행은 `previewAndRun` 게이트를 공유한다. 거절하면 `done`에 넣는다(`:519`). 선행이 manual인 (a)형 blocked는 배치가 선행을 실행하지 않으므로, 다른 재조회로 선행이 우연히 확인될 때만 제안된다. 그때도 이름이 적힌 동의가 필요하다 |
| 동의 거절·버전 끝내 미확인 | 실행 0 | 하네스 `-declined`(실행 호출 0), `-still-unknown`(이어서 설치 창 없음, 의존 도구 호출 0, 건너뜀 사유 "파일이 발견됐지만") — main에서 통과 |

## 이번 diff 신규 지적
| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-010 | ⚪ Nit | 화면상태 정직 | `src/views/devTools.ts:163-164`(토스트 `fresh = !versionBefore`→"설치되었습니다"), `:868-872`(결과 패널 "신규 설치: X"), `src-tauri/src/dev_tools/actions.rs:101`(`version_before = check_tool_version` 재조회), 캡처 `r14-p29-11`(Node 결과 패널 "신규 설치: 2.0.0") | Read, 캡처 열람 | 동의창은 이제 "설치가 아니라 업데이트를 시도합니다(버전 미확인)"라고 말한다. 그런데 실행 직전 버전 조회가 또 응답하지 않으면 `versionBefore=None`이 되고, 결과 토스트와 패널은 "설치되었습니다", "신규 설치"라고 적는다. 동의와 결과가 같은 실행을 다른 동사로 부른다. 캡처의 값은 스텁이 `versionBefore:null`을 고정으로 주기 때문이다. 실제로는 재조회도 타임아웃날 때만 이렇게 된다(이 모양이 원래 타임아웃이 잦은 경우라 드물지는 않다고 추정함, 실측하지 않음). 실행 결과 자체는 정확하다 | 실행 전 도구가 `path`를 가지고 있었으면 versionBefore가 없어도 "업데이트 완료(이전 버전 미확인): X"로 표기한다 |
| RV-011 | ⚪ Nit | 주장≠검증 | `src/views/devTools.ts:493-494` | Read, 캡처 `r14-p29-11` | 주석이 "macOS에서 Node.js는 구조적 manual이라 이 흐름은 '다시 확인'에서만 시작된다"고 말한다. 8cbce95부터는 macOS Homebrew Node(버전 미확인, run)의 배치 업데이트가 확인되면 배치 끝에서 연쇄가 시작된다. 하네스 `chain-mac-node-unverified-run`이 바로 그 경로를 검증한다 | "미설치 Node는 구조적 manual — 예외: 경로가 있는 Homebrew Node의 버전 미확인 업데이트는 배치 끝에서 시작"으로 주석을 고친다 |
| RV-012 | ⚪ Nit (현재 도달 불가) | 위임권한 | `src/devToolsOrder.ts:123`(`blocked`가 직접 선행만 검사), `src-tauri/src/dev_tools/mod.rs:128,220,245`(의존 도구는 모두 `[Node]`, 나머지는 `[]`) | Read | `blocked`는 한 단계만 본다. C→B→A(A가 버전 미확인)이면 B는 blocked지만 C는 아니다. 그러면 C가 "설치합니다" 목록에 오르고 실행 단계에서 건너뛴다. RV-001과 같은 "문구≠실행" 모양이다. 현재 도구 정의는 깊이가 1이라 도달하지 않는다 | blocked를 고정점까지 전파하거나(blocked에 의존하면 blocked), 깊이 1 전제를 주석으로 남긴다. 저비용 |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 주장≠검증 | `describeUnmetDependencies` 끝에 `return parts.join(' ')`가 한 번 더 있는 죽은 코드 | 기각 | 처음에 `sed` 두 구간 출력이 이어 붙어 그렇게 보였다. `grep -n "return parts.join"`으로 확인하니 `:90`(describeUnmetDependencies)과 `:150`(describeBatchConsent)으로, 서로 다른 함수다 |
| 위임권한 | `deferredByUnverified` 고지("확인되면 이름을 적어 다시 묻습니다")가 Node 확인 뒤 의존 도구가 `run`이 아니면 질문 대신 stuck 토스트로 끝나 약속과 어긋난다 | 기각 | 의존 도구가 이미 `run`이어야 blocked가 생긴다(`runnable` 안에서만 계산, `devToolsOrder.ts:117-123`). Node가 확인되면 actionKind가 바뀌지 않는 한 후보가 된다. 바뀌는 경우는 재현 경로를 대지 못했다 |
| 동의경계 | 요약 동의창이 업데이트 대상(GitHub CLI)을 적지 않는다(`r14-p29-10`) | 기각 | R7부터 이어진 기존 동의 범위다("설치돼 있는 도구의 업데이트는 요약 동의 없이 배치 처리", `devToolsOrder.ts:101`). 이번 diff가 바꾸지 않았다 |

## 페르소나별 관점 (요약)
- **위임 권한·영향 범위 🟢**: 동의 목록 = 실행 목록 불변식이 mac 버전 미확인 모양에서도 성립한다(`askable`이 그대로 `confirmedInstallTargets`). 반영은 실행 집합을 넓히지 않았다. 새 연쇄 진입로도 이름을 적은 두 번째 동의를 지난다. 잠재 Nit RV-012.
- **주장 vs 검증 🟢**: 커밋 메시지의 4건 주장이 코드·캡처와 일치한다. 구현자가 확인하지 못한 판별력을 옛 소스로 직접 확인했다(위 표). 주석 하나가 낡았다(RV-011).
- **이기종 머신 운영자 🟢**: mac Homebrew run 모양을 하네스가 이제 덮는다. nvm 계열은 막다른 골목이 아니라 "터미널에서 직접"이라고 사실대로 안내한다. 미리보기 신뢰 플래그(mac brew=신뢰, win winget=불신뢰)를 플랫폼별로 나눈 스텁 변경은 1차 RV-008 서술과 맞는다.
- **화면 상태 정직성 🟢**: 플러그인 배너와 usage 캡처가 단정·모순 없이 바뀌었다. 동의 문구는 정직해졌다. 다만 결과 동사가 아직 따라오지 못했다(RV-010).
- **제로베이스 재설계자(발산형) 🔵**: 새 Rethink는 없다. 이번 반영으로 RT-3(설정 계획 패널)의 근거가 하나 더 생겼다. 배치 동의창 한 장에 "업데이트(미확인)·이번엔 제외·확인되면 다시 묻기"가 한 문단으로 들어가(`r14-p29-10`) 모달 문장 하나가 계획 패널이 할 일을 대신하고 있다. RT-1~3은 유효하며 이월한다.

## 잘 된 점
- 동의 계산을 `planBatchInstall`/`describeBatchConsent` 순수 함수로 끌어내 하네스가 4가지 모양(run 미확인, manual 미확인, 경로 없음 run, 설치됨)을 함수 단위로 단언한다. 1차 권고(blocked 조건 + mac 픽스처)를 그대로 따랐다.
- 1차 트레이드오프 권고("의존 도구는 처음부터 제외하고 확인되면 연쇄로 다시 묻는다")를 handled 조정 한 줄로 구현했다. 연쇄 게이트를 새로 만들지 않고 기존 `offerInstallChain`을 재사용했다.
- 기존 설치 전용 문구("다음 N개 도구를 설치합니다: …")를 바이트 단위로 보존하고 단언했다(`consentInstallOnly`).
- RV-004를 소스가 아니라 픽스처에서 고쳤다. 백엔드가 같은 `resolve_tool_path`를 쓴다는 근거를 주석으로 남겼다.

## 생략·미확인
- **실기 미검증**: 실제 Homebrew `brew upgrade node`, 실제 버전 조회 타임아웃, Tauri 실기동, Windows 실기. 하네스는 TS/DOM 층만 덮는다. Rust는 무변경이라(`git diff --stat 16efbdd..8cbce95`에 `src-tauri` 없음) `cargo test`는 다시 돌리지 않았다.
- **재실행한 것**: main(1420)에서 devToolsOrder 21·autonomousTasks 19·usage 10, 모두 bugs=0 consoleErrors=0(PM 확인값 재현). `pnpm exec tsc --noEmit` 통과. `pnpm build`는 다시 돌리지 않았다(PM 확인값 인용).
- **캡처 열람**: 위임이 지정한 13장 중 6장을 열었다(p29-10, p29-11, p29-06, p26-01, p26-07, p24-04). 나머지 7장(p26-04/06/08, p24-05 등)은 하네스 단언(상태바 일치·문구)을 근거로 삼았다. p29-10과 p29-11은 서로 다른 모달임을 확인했다.
- **화면 재사용 일관성 diff**: 새 페이지가 없어 해당 없음.

## 실행 액션
- 소스 수정·커밋·push는 하지 않았다. scratchpad에 임시 worktree(16efbdd)를 만들고 하네스를 실행한 뒤 제거했다. vite 1439는 내가 띄운 pid만 종료했다. main 워킹트리는 clean 상태 그대로다(하네스가 쓰는 `scripts/ui-harness/last-run-report.json`은 gitignore 대상이다).
- 작성·수정한 파일: 이 보고서, 페르소나 5개 파일의 "적용 이력" append, `personas/INDEX.md` 5개 행의 최근 재사용 열 갱신. 커밋은 PM 몫이다.

## 종합 판정
🟢 **Green** — `d6ec70f..8cbce95` 전체. 1차 Minor 5건 중 RV-001·003·004·005가 코드·하네스·캡처로 해소됐다. 새 단언 가운데 판별 가능한 것은 수정 전 코드에서 실패함을 확인했다. 반영은 동의 경계를 좁히는 방향이고 약화한 곳이 없다. 남은 것은 이월 Minor 1(RV-002)과 Nit 7(이월 4 + 신규 3), Rethink 3이다.

## PM에게 권고
1. **병합·출시를 막지 않는다.** Critical·Major 없음.
2. **저비용 정리(선택)**: RV-011(주석 1줄), RV-012(blocked 고정점 전파 또는 전제 주석). 둘 다 `devToolsOrder.ts`/`devTools.ts` 국소 수정이다. RV-010은 결과 패널 동사 규칙에 "실행 전 path 유무"를 하나 더하면 된다. 비용은 낮다고 추정한다(`renderResultPanel`/`notifyOutcome` 두 곳이고 `tool.path`가 이미 인자에 있다).
3. **백로그 유지**: RV-002·RV-006~009, RT-1~3. RT-2(낮은 비용)는 RV-009를 함께 없앤다.
4. **보고 문구**: 1차 권고 4 그대로다. 이번 반영으로 "macOS에서 Homebrew Node가 버전 응답이 없을 때도 의존 도구를 몰래 설치하지 않고, 확인 뒤 이름을 적어 다시 묻는다"를 추가로 말할 수 있다.
