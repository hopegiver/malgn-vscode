# R14 연쇄 설치·자율 작업 사전조건·사용량 빈 상태 리뷰 보고서

리뷰 페르소나 패널: persona-delegated-authority-blast-radius.md, persona-claimed-vs-verified.md, persona-heterogeneous-machine-operator.md, persona-screen-state-honesty.md, persona-zero-base-redesigner.md(발산형)
리뷰 대상: `/Users/hopegiver/workspace/malgn-vscode` main `d6ec70f..16efbdd` (e72650d P2-9, 76e1729 P2-4, 16efbdd P2-6) — target_id `r14-install-chain-autonomy-prereq-usage-empty`
리스크 범주: 사용자 머신 설치 실행(동의 경계). P2-4·P2-6은 표시 전용
리뷰 일자: 2026-10-15
등급/모드: Sensitive, 최초 리뷰, 풀패널. 노출 범위 축소는 적용하지 않음(위임 지시)
종합 판정: 🟢 Green (Critical 0, Major 0, Minor 5, Nit 4, Rethink 3)

## 요약
P2-9의 핵심 불변식 네 가지가 코드로 성립한다. 이름이 적힌 동의 목록과 실행 목록이 같은 배열이고, 거절하거나 바깥을 클릭하면 아무것도 실행되지 않는다. 라운드 상한과 재진입 차단이 있고, 각 도구는 배치와 같은 `previewAndRun` 게이트를 거친다. 남은 지적은 모두 Minor 이하다. 가장 의미 있는 것은 RV-001이다. macOS Homebrew Node가 `--version`에 응답하지 않는 예외 경로는 `blocked` 필터를 비켜 간다. 그래서 배치 동의창에 실행되지 않을 의존 도구가 오르고, Node 업데이트가 "설치"로 표기된다. 다음은 RV-003이다. P2-4 플러그인 배너는 백엔드가 읽기 실패를 빈 배열로 뭉갠 결과를 "설치되어 있지 않아"로 단정할 수 있다.

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`의 역할개념 열을 대조했다. 이번 위임의 리스크 표면은 모두 기존 역할개념에 들어가므로 신규 페르소나는 없다.

| 페르소나 | 재사용/신규 | 사유 |
|---|---|---|
| persona-delegated-authority-blast-radius.md | 재사용 | 이어서 설치가 넘기는 실행 권한을 사용자가 사전에(이름 적힌 동의) 아는가 — 역할개념 그대로 |
| persona-claimed-vs-verified.md | 재사용 | 목표 6(픽스처가 백엔드 실제 모양인가)과 주석·커밋 주장 대조 |
| persona-heterogeneous-machine-operator.md | 재사용 | mac/Windows 실제 탐지·러너 경로에서 연쇄가 이어지는가, 막다른 골목 여부 |
| persona-screen-state-honesty.md | 재사용 | P2-4/P2-6의 로딩·판정 불가·확정 미충족 구분 |
| persona-zero-base-redesigner.md | 재사용(발산형) | 발산형 필수. 역할개념 유효 |

## 위임 목표 대비 판정
| 항목 | 목표 | 판정 | 근거 |
|---|---|---|---|
| P2-9 (1) | Node 설치 확인 → 재조회 → 이어서 설치 | ✅ | `devTools.ts:243-259`(개별 실행·다시 확인), `:439`(배치 뒤), `devToolsOrder.ts` planInstallChain. Windows는 `runners.rs:20-24`의 고정 후보 `C:\Program Files\nodejs\npm.cmd` 덕에 재조회 뒤 의존 도구가 `run`으로 바뀐다 |
| P2-9 (2) | 이름 없는 동의로 실행 없음, 기존 동의 모델 유지, 문구==실행 | ✅ / 예외 1 | 동의 문구가 `plan.candidates`를 그대로 쓰고 실행 루프도 같은 배열을 돈다(`devTools.ts:510-535`). `previewAndRun`이 신뢰성·다중영향 검사를 공유한다. 예외: RV-001 |
| P2-9 (3) | 유한·중복 없음·선행 실패 시 미실행+사유 | ✅ | `MAX_CHAIN_ROUNDS=3`(`:492`), `chainRunning` 재진입 차단(`:495-496`). 거절해도 `done`에 넣는다(`:515`). 실패·unknownAfter·timedOut은 재조회가 없어 비교할 변화가 없으므로 제안하지 않는다(`runPlan :228-231`) |
| P2-9 (4) | mac에서 "한 번에 설치" 주장 없음 | ✅ / RV-005 | `renderChainHint :822`의 "이 앱이 Node.js 설치를 대신하지는 않습니다". `r14-p29-06` 열람함 |
| P2-9 (5) | Node 경로 있음 + 버전 타임아웃 예외에서 오판 없음 | ⚠️ 부분 | Windows MSI(manual)는 정확하다(`r14-p29-08`). mac Homebrew(run)는 RV-001 |
| P2-9 (6) | 픽스처가 실제 모양 | ✅ / 부분 | `devToolsOrder.mjs`의 beforeNode/afterNode는 `query.rs:34-101`과 일치한다. nodeUnverified는 Windows/manual 한 모양만 있다(RV-001) |
| P2-4 | 3상태+다음 행동, 실제 소스, 판정 불가 미단정, 백엔드 불변, 재진입 회귀 없음 | ✅ / RV-003 | 순수 함수가 기존 state만 읽는다. 세 소스 모두 로그인 직후 로드된다(`main.ts:256-265`). 커밋 76e1729에는 백엔드 변경이 없다. 렌더 안에서 조회·notifyChange를 하지 않는다 |
| P2-6 | 3갈래 구분, 신호 한계 정직, 정상 경로 불변, capabilities 불확장 | ✅ / RV-004 | `capabilities/default.json` 무변경(core/opener/updater/process:allow-restart). 새 커맨드는 인자 없음, bool 3개만 반환. 경로 문자열을 노출하지 않는다. 일별 데이터가 있으면 추가 IPC가 없다(`usage.ts:249-256`, 하네스 단언) |

## 지적 사항 (통합)
| ID | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-001 | 🟡 Minor | 위임권한·이기종·주장≠검증 | `src/views/devTools.ts:374`(blocked 필터), `src-tauri/src/dev_tools/query.rs:82-101`(경로 있음 분기: `installed=version.is_some()`, actionKind=업데이트 표), `plan_table.rs:661-665`(HomebrewFormula→Run), `actions.rs:237-239`(경로 있으면 install→update 위임), `scripts/ui-harness/flows/devToolsOrder.mjs:182-184`(nodeUnverified=manual 고정) | 백엔드 판정 경로와 하네스 Read | `blocked`는 "선행 도구가 배치 실행 대상이 아닌" 경우만 거른다. 그런데 mac Homebrew Node가 `--version`에 응답하지 않으면 `installed=false, path=있음, actionKind=run`이 된다. 이때 Node가 `runnableIds`에 들어가 blocked가 비고, 요약에 "다음 4개 도구를 설치합니다: Node.js, Claude Code, pnpm, Wrangler"가 뜬다. 실제로는 Node에 `brew upgrade`가 돌고(perform_install→perform_update), 버전이 계속 미확인이면 의존 도구 3개는 건너뛴다. 실행 쪽은 안전하다(이름이 적힌 동의 범위 안). 그러나 목표 (2)의 "문구==실행 대상"과 (5)가 이 모양에서는 깨진다. 코드 주석(`:368-372`)도 manual 경우만 가정한다. 재현: 응답이 버전 조회 타임아웃(R7 보고서 기준 10초, 이번에 재확인하지 않음)을 넘기는 Homebrew node로 "전체 업데이트" | ① `installed=false && path` 인 선행 도구는 실행 대상이어도 "설치"가 아니라 "업데이트(버전 미확인)"로 요약에 따로 적는다. 그 의존 도구는 blocked와 같은 사유로 처음부터 제외한다(어차피 이번 배치에서 확인될 보장이 없다). ② 하네스 nodeUnverified에 `platform:'mac', installMethod:'Homebrew', actionKind:'run'` 모양을 추가한다 |
| RV-002 | 🟡 Minor | 이기종 | `src/views/devTools.ts:505-508`(stuck 토스트), `install_resolver.rs` resolve_install_plan의 `install_prefix_writable`→`MANUAL_NOT_WRITABLE` | Read | 의존 도구가 `run`이 되지 못한 이유를 일괄로 "앱이 아직 설치 경로를 찾지 못했습니다"라고 말한다. mac에서 Node를 공식 .pkg로 설치하면 npm 전역 prefix(/usr/local/lib/node_modules)가 루트 소유라 NOT_WRITABLE로 떨어지는데(소유권은 일반적인 pkg 설치 기준이며 이 머신에서 실측하지 않음), 이 경우 이유가 틀린다. 또 candidates와 stuck가 함께 있으면 stuck 도구는 대화상자에 아예 언급되지 않는다 | 문구를 "앱이 자동으로 설치할 수 없는 상태입니다 — 각 항목의 안내에 이유가 있습니다"처럼 원인 중립으로 바꾼다. candidates가 있는 대화상자에도 "(자동 설치 불가: X — 항목 안내 참고)"를 한 줄 붙인다 |
| RV-003 | 🟡 Minor | 화면상태 정직·주장≠검증 | `src/autonomyPrereq.ts:66`, `src-tauri/src/plugins/installed.rs:151-167,174-181`, `plugins/mod.rs:59-63`(`unwrap_or_default`), `scripts/ui-harness/flows/autonomousTasks.mjs:533,535`(`__throw`) | 백엔드 Read | `list_installed_plugins`는 실패해도 오류를 내지 않는다. 홈 디렉터리 없음, JSON 파싱 실패, `plugins` 키 스키마 변경, user 외 scope 설치가 모두 `[]`/누락으로 돌아온다. 그래서 `catalog.error`가 null이 되고, 배너는 "malgn-agent 플러그인이 설치되어 있지 않아"라고 단정한다. 목표 "판정 불가를 단정하지 않음"에 대해 확정 신호가 원래보다 약하다. 하네스의 check-failed 장면은 `check_dev_tools`/`list_installed_plugins`를 throw시키지만, 두 커맨드 모두 Vec를 반환하므로 IPC 수준 실패에서만 이 모양이 나온다. 이 관례 자체는 기존 `catalog.mjs:164`, `home.mjs:645`에서도 쓰인다. 영향: 이미 설치된 사람에게 잘못된 배너가 뜨고, 눌러도 재설치(무해)뿐이다 | 단기: 문구를 "malgn-agent 플러그인을 찾지 못했습니다"로 약화한다. 근본: 백엔드가 "파일 없음"과 "읽기/파싱 실패"를 구분해 반환한다(차기). 하네스 check-failed 주석에 "IPC 수준 실패만 이 모양"이라고 명시한다 |
| RV-004 | 🟡 Minor | 주장≠검증 | `scripts/ui-harness/flows/usage.mjs:17-33,256-260`, 캡처 `docs/reviewer/r14-p26-01-daily-not-installed.png` | 하네스 Read, 캡처 열람 | not-installed 장면이 `check_dev_tools`를 base(claude 설치됨) 그대로 둔다. 캡처 한 장 안에서 상태바는 "claude 2.1.0", 본문은 "Claude Code가 설치돼 있지 않은 것으로 보입니다"라고 서로 모순된다. 두 커맨드는 같은 `resolve_tool_path`를 쓰므로(`source.rs` collect, `query.rs:34`) 실제 백엔드는 같은 시점에 이 조합을 만들 수 없다. 근거 캡처가 불가능한 상태를 보여 준다 | not-installed/unknown-traces 장면에 claude `installed:false, path:null` 목록을 함께 주입한다. 상태바와 본문이 일치하는지 단언을 추가한다 |
| RV-005 | 🟡 Minor | 이기종·첫실행 | `src/views/devTools.ts:822`, `plan_table.rs:785-787`(mac Node 안내 "nvm/fnm/volta 등으로 관리 중이라면 그쪽에서 설치"), `dev_tools/mod.rs:135`(mac Node 후보 = homebrew·/usr/local뿐) | Read, 캡처 `r14-p29-06` 열람 | mac 안내 패널은 바로 위 줄에서 nvm 등으로 설치하라고 권한 뒤 "직접 설치한 뒤 ↻ 다시 확인을 누르면 이어서 … 묻습니다"라고 약속한다. 그러나 버전 매니저로 설치한 Node는 후보 경로 밖이고 GUI PATH에도 보통 없어 탐지되지 않는다. 그 경로로는 약속이 지켜지지 않는다(막다른 골목) | 힌트를 "Homebrew 또는 공식 설치 프로그램으로 설치한 경우"로 조건화한다. nvm 등은 "앱이 감지하지 못하므로 의존 도구도 터미널에서 직접 설치"라고 사실대로 쓴다 |
| RV-006 | ⚪ Nit | 위임권한 | `src/views/devTools.ts:526-531` | Read | 연쇄 실행 루프의 건너뜀 분기는 `lastResult`를 지우지 않고 건너뜀 개수 토스트도 없어 배치와 비대칭이다. 실제로는 candidates가 이미 "선행 전부 설치"를 만족하므로 현재 도구 정의(모두 Node에만 의존)로는 도달하지 않는다 | 배치와 같은 처리로 맞추거나, 도달 불가 방어 코드임을 주석으로 밝힌다 |
| RV-007 | ⚪ Nit | 화면상태 정직 | 캡처 `r14-p29-08`(Node 행 "설치 안 됨", 사이드바 "미설치" vs 건너뜀 패널 "파일이 발견됐지만") | 캡처 열람 | 버전 미확인 Node를 행 배지와 사이드바는 여전히 "설치 안 됨/미설치"로 표시한다. 새 사유 문구와 한 화면에서 모순된다. 기존 표시 규칙이라 이번 범위는 아니다 | 후속 항목: `installed=false && path`일 때 배지를 "버전 미확인"으로 표시 |
| RV-008 | ⚪ Nit | 첫실행 | 캡처 `r14-p29-09`, `query.rs:115`(winget 미리보기 불신뢰) | 캡처 열람, Read | Windows 배치 요약은 "다음 1개 도구를 설치합니다: Node.js"라고 묻는다. 그런데 winget 미리보기는 항상 불신뢰라 배치에서는 실행되지 않고, 개별 확인으로 다시 한 번 동의해야 한다. R7부터 있던 동작이다. 안전 방향이다 | 요약에서 "미리보기 불신뢰로 개별 확인이 필요한 도구"를 따로 적거나 제외한다 |
| RV-009 | ⚪ Nit | 화면상태 정직 | `src/usageSource.ts:33-40`, `src/onboarding.ts:66-73` | Read | Claude 설치를 판정하는 정의가 화면마다 다르다. 사용량 빈 상태는 경로만 있어도 "설치돼 있지만"으로 보고, 온보딩·자율 배너는 경로가 있어도 버전이 없으면 'unknown'으로 본다 | RT-2로 일원화하거나, usage 쪽 문구를 "실행 파일은 찾았지만"으로 바꾼다 |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 위임권한 | 이어서 설치 확인창이 떠 있는 동안 "전체 업데이트"나 개별 "설치"를 눌러 동시에 실행될 수 있다 | 기각 | `dom.ts:238-277` confirmDialog는 `document.body`에 모달 오버레이를 붙이고, 바깥 클릭·ESC는 취소로 끝난다. 실행 중에는 `updatingAll`로 배치 버튼이 잠긴다. 개별 실행이 겹치면 백엔드 `EXECUTION_LOCK`(`actions.rs:241`)이 거부한다 |
| 위임권한 | 앱 다른 경로의 재조회가 "새로 설치됨" 변화를 소비해 연쇄 제안이 사라진다 | 강등→Nit 미만, 기록만 | `loadDevTools` 호출부를 `src` 전체에서 grep했다. 최초 로드(`main.ts:256`), 오류 재시도(`devTools.ts:585`, `home.ts:115`), runPlan 성공 뒤, handleRecheck뿐이다. 오류 상태에서 재시도해 성공한 경우에만 제안을 놓치며, 그때도 실행은 일어나지 않는 안전 방향이다 |
| 이기종 | Windows에서 Node를 winget으로 새로 설치한 직후 앱 프로세스의 PATH가 낡아서 npm 설치가 실패한다 | 기각(미확인 위험으로 이관) | 재현 근거가 없다. npm 러너는 고정 절대경로(`runners.rs:23`)로 해석된다. postinstall이 PATH의 node를 찾는지는 실기 없이 판단할 수 없어 아래 "미확인"에 남긴다 |
| 주장≠검증 | 기존 이월 하네스 실패 3건(fontPolicyKoreanMonoScan, projectAgentProgress/running 2건)이 이번 diff 때문이다 | 기각 | diff의 파일 목록(`git diff --stat`)에 해당 화면·폰트 정책 파일이 없다. d6ec70f 기준에서 재실행하지는 않았다(아래 생략 참조) |

## 페르소나별 관점
### 위임 권한·영향 범위 — 판정: 🟢
- 동의 목록과 실행 목록이 같은 객체다(`plan.candidates`). 문구가 도구 이름과 개수를 그대로 적고, 확인할 수 없는 도구는 실행하지 않는다는 조건까지 미리 고지한다(`r14-p29-02`).
- 거절하면 `done`에 넣고 토스트만 띄운다. 이후 "다시 확인"을 눌러도 prev가 이미 Node 설치 상태라 다시 묻지 않는다. 유령 재제안이 없다.
- R7의 RT-1(첫 요약에서 조건부로 미리 동의)보다 보수적인 "확인 뒤 두 번째 동의"를 택했다. 동의 경계 관점에서 더 낫다.
- 남은 것은 RV-001(예외 모양에서 요약 문구가 실행과 어긋남)이다.

### 주장 vs 검증 — 판정: 🟢 (Minor 2)
- 픽스처 대부분이 `query.rs` 두 분기를 주석으로 인용하고 같은 모양을 만든다. R7 RV-002가 지적한 "백엔드가 만들 수 없는 상태" 재발은 없다.
- 예외 모양 커버리지 공백(RV-001 ②)과 usage 픽스처의 화면 내 모순(RV-004)은 남았다.
- 커밋 메시지의 "라운드 상한 3·재진입 차단·처리한 도구 재제안 금지"는 코드(`:492-496, :515`)와 일치한다.

### 이기종 머신 운영자 — 판정: 🟢 (Minor 2)
- Windows: Node(winget 불신뢰) → 개별 확인 → 성공 시 재조회 → npm.cmd 고정 후보로 의존 도구가 run이 되고 → 연쇄가 이어진다. 경로가 성립한다.
- mac: Homebrew 설치 뒤 "다시 확인"으로 연쇄가 시작된다(`r14-p29-07`). 공식 pkg 설치(RV-002)와 버전 매니저 설치(RV-005)에서는 문구가 실제와 다르다.

### 화면 상태 정직성 — 판정: 🟢 (Minor 1)
- P2-4: 로딩 중에는 침묵하고(`r14-p24-08`), 조회 실패는 따로 알리며(`r14-p24-06`), 여러 상태가 겹치면 나란히 보여 준다(`r14-p24-05`). 경로가 있는데 버전이 없는 Claude는 'unknown'으로 침묵한다. 단정하지 않는다.
- P2-6: 미설치는 신호 세 개가 모두 "없음"일 때만 낸다. 모순되면 "원인은 단정할 수 없습니다"라고 쓴다(`r14-p26-04`). "다른 위치에 설치했다면 틀릴 수 있다"는 단서도 붙인다.
- 약점은 상류 백엔드가 실패를 빈 값으로 뭉개는 플러그인 목록이다(RV-003).

### 제로베이스 재설계자(발산형) — 판정: 🔵 아래 Rethink

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵
| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-1 | "실행 환경이 준비됐는가"를 화면마다 따로 해석한다. 홈 온보딩(`onboarding.ts`), 자율 배너(`autonomyPrereq.ts`), 사용량 빈 상태(`usageSource.ts`)가 같은 사실(Claude 설치·로그인·플러그인)을 각자 판정한다 | 단일 `environmentReadiness` 순수 모델(항목별 `ok/missing/unknown/loading` + 다음 행동)을 하나 만든다. 세 화면은 그 결과를 보여 주기만 한다. 자율 작업 실행 시점에도 같은 모델로 사전 검사한다 | RV-009 같은 정의 불일치가 구조적으로 사라진다. 새 화면을 추가할 때 판정을 다시 짜지 않는다 | 중간(미확인 추정 — 세 모듈 모두 순수 함수라 합치기 쉬워 보이지만 문구 정본 이전이 필요) |
| RT-2 | P2-6이 Claude 실행 파일 탐지를 새 커맨드에서 한 번 더 한다. state.devTools에 이미 같은 함수의 결과가 있다 | 새 커맨드는 `~/.claude`·projects 신호 두 개만 준다. Claude 존재는 `state.devTools`(claudeInstallState)를 쓴다 | 한 화면 안에서 상태바와 본문이 갈리는 RV-004 모양이 실제로도 생길 수 없다. 정의 하나 | 낮음(`source.rs` 필드 1개 제거, `classifyUsageEmpty` 입력 변경). 근거: 두 경로가 같은 `resolve_tool_path`를 호출함을 Read로 확인 |
| RT-3 | 신규 머신 셋업이 모달 체인으로 진행된다(요약 → 개별 확인 → 이어서 설치 확인, Windows는 Node 동의가 2회) | 개발 도구 화면 상단에 "설정 계획" 패널을 둔다. 단계 목록(Node → 확인 → Claude·pnpm·Wrangler)과 단계별 상태·이유를 계속 보여 주고, 동의는 단계별 버튼으로 받는다 | 모달은 닫히면 맥락이 사라진다. 계획 패널은 "지금 어디까지 왔고 왜 멈췄나"(RV-002, RV-005, RV-008)를 한 곳에서 말한다. 동의 단위는 지금과 같게 유지할 수 있다 | 중간~큼(미확인 추정). 동의 모델이 바뀌므로 Sensitive 재리뷰가 필요하다 |

## 트레이드오프 (페르소나 간 충돌)
- **동의 횟수 vs 동의 경계**: 첫실행 관점은 Windows에서 Node를 두 번 동의하고 연쇄에서 한 번 더 동의하는 것이 번거롭다고 본다(RV-008). 위임권한 관점은 "확인된 뒤 이름을 적은 두 번째 동의"가 R7 RT-1의 조건부 사전 동의보다 안전하다고 본다. → 권고: 현행 유지. 횟수는 RT-3(계획 패널)로 체감만 줄이고 동의 단위는 줄이지 않는다.
- **RV-001 처리 방향**: 예외 모양에서 의존 도구를 처음부터 제외하면(안전·정직), Node 업데이트가 성공해 버전이 확인되는 드문 경우에는 연쇄로 한 번 더 물어야 한다. → 권고: 제외한다. 배치 뒤 `offerInstallChain`이 그 경우를 이미 받아 준다(Node가 false→true로 바뀌면 제안). 추가 비용이 없다.

## 잘 된 점
- 판정 로직이 전부 DOM 없는 순수 함수다(`devToolsOrder.ts` planInstallChain/describeUnmetDependencies, `autonomyPrereq.ts`, `usageSource.ts`). 하네스가 함수 단위로 단언한다.
- `previewAndRun`으로 배치와 연쇄가 같은 안전 게이트를 공유한다. 게이트가 두 벌로 갈라지는 위험을 없앴다.
- "경로 있음 + 버전 없음"을 "없다"와 구분하는 문구가 R7 RV-002가 남긴 오판 지점을 정확히 해소했다(Windows 모양 기준).
- P2-6 새 커맨드는 표면이 최소다. 인자 없음, bool 3개, 정상 경로에서는 호출하지 않음, capabilities 무변경.
- 빈 상태 문구가 자기 한계("다른 위치에 설치했다면 틀릴 수 있습니다")를 스스로 밝힌다.

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| 이름 없는 동의로 실행 없음 | 위임권한 | 필수 | ✅ | |
| 동의 문구 == 실행 대상 | 위임권한 | 필수 | ⚠️ | RV-001 예외 모양 |
| 연쇄 유한·중복 없음 | 위임권한 | 필수 | ✅ | |
| 픽스처 실제 모양 | 주장≠검증 | 필수 | ⚠️ | RV-001②, RV-004 |
| mac/Win 막다른 골목 없음 | 이기종 | 권장 | ⚠️ | RV-002, RV-005 |
| 판정 불가 미단정 | 화면상태 | 필수 | ⚠️ | RV-003(상류 백엔드 붕괴) |
| capabilities 불확장 | 주장≠검증 | 필수 | ✅ | |
| 발산형 Rethink 대안 제시 | 발산 | 필수 | ✅ | RT-1~3 |

## 생략·미확인
- **실기 미검증**: 실제 설치 실행, Tauri 실기동, Windows 실기, mac 공식 pkg의 npm prefix 소유권, winget 설치 직후 npm postinstall의 PATH 의존. 하네스와 PM 확인(`pnpm build`, `cargo test` 790 passed)은 TS/DOM/CSS 층과 Rust 단위 테스트만 덮는다.
- **이월 하네스 3건**: d6ec70f에서 재실행해 동일하게 나오는지 직접 확인하지 않았다. 파일 범위 대조로만 "이번 diff 무관"이라고 판단했다.
- **캡처 열람 범위**: 25장 중 8장을 열었다(p29-02/06/08/09, p24-05/06, p26-01/04). p24-05와 p24-06이 서로 다른 화면임을 확인했다. 나머지 17장은 하네스 단언을 근거로 삼았다.
- **화면 재사용 일관성 diff**: 새 페이지가 없어 해당 없음. 배너는 기존 `.alert` 클래스, 빈 상태는 기존 `.state-block` 클래스를 재사용한다.

## 실행 액션
- 소스 수정·커밋·push는 하지 않았다. 작성·수정한 파일은 이 보고서, 페르소나 5개 파일의 "적용 이력" append, `personas/INDEX.md`의 최근 재사용 열 갱신뿐이다. 커밋은 PM 몫이다.

## PM에게 권고
1. **Green이라 출시와 병합을 막지 않는다.** Critical·Major가 없다.
2. **이번 회차에 같이 고치면 좋은 것(저비용)**: RV-001(blocked 조건에 `installed=false && path` 선행 도구 포함 + mac 픽스처), RV-004(픽스처 1곳), RV-005(문구 1줄). 셋 다 한 파일 안의 국소 수정이다.
3. **백로그**: RV-002·RV-003(백엔드 실패 구분은 차기), Nit 4건, RT-1~3(방향 결정). RT-2는 비용이 낮고 RV-004·RV-009를 함께 없앤다.
4. **보고 문구**: P2-9 완료 보고에 "macOS 신규 머신 한 번에 설치"를 쓰지 않는다(P2-3 선행, R7 RT-4). "Windows는 Node 개별 확인 1회 뒤 이어서 설치, macOS는 Homebrew로 직접 설치하고 다시 확인한 뒤 이어서 설치"로 적는다.
