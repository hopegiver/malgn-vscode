# 리뷰 보고서 — R4 에러 원문 보존·플러그인 설치 폴백·배지 통일·미설치 안내

- **리뷰 대상:** target_id `r4-error-text-plugin-fallback-badge` — HEAD 4aae413 대비 워킹트리 diff(4건)
  - P1-2 에러 원문 보존: 신규 `src/errorMessage.ts`, `src/dom.ts`, `src/main.ts`, `src/styles.css`, `src/views/{appLinks,autonomousTasks,catalog,devTools,home,login,projects,sessions,settings,usage}.ts`
  - P1-5 설치 플래그 폴백: `src-tauri/src/plugins/mod.rs`
  - P2-9 배지 판정 통일: 신규 `src/projectBadge.ts`, `src/sidebar.ts`, `src/views/home.ts`, `src/views/projects.ts`
  - P2-10 미설치 안내: `src-tauri/src/github_integration.rs`, `src-tauri/src/cloudflare_integration.rs`
- **등급/모드:** Standard, 최초 리뷰(1차). 리스크 범주: 로컬 UI 정보 노출(토큰·키 마스킹), 외부 CLI 호출 인자 변경
- **일자:** 2026-10-13
- **종합 판정: 🟡 Amber** — Critical 없음, Major 2건(둘 다 "가림/절단이 사용자가 써야 할 값을 깨뜨림" 계열이고 고치는 비용이 낮음)
- **실행 액션:** 수행하지 않음(코드 수정·커밋·설치·인증 없음). 읽기 전용 `claude plugin install --help`, `claude plugin list --bogus-flag`/`-y`(거부 문구 확인용)만 실행. scratchpad에 `errorMessage.ts` 사본을 두고 node로 실측함.
- **PM 주장 재검증 여부:** tsc/build/cargo test 705 통과는 **다시 돌리지 않았다**(주장 그대로 인용). 마스킹·절단·성능은 node로 직접 실측했다.

## 페르소나 재사용 판정

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | **신규** | INDEX 역할개념 17행 중 "화면 원문의 비밀값 가림 + 그 오탐·절단으로 잃는 진단 가치"를 다루는 행이 없음. 가장 가까운 `screen-state-honesty`(로딩·빈·에러 구분)와 `process-execution-security`(argv·env·수명)는 리스크 표면이 다르다 |
| `persona-process-execution-security.md` | 재사용 | install argv 변경·재시도 루프가 역할개념 직접 대상 |
| `persona-async-response-ordering.md` | 재사용 | 기준 A4 "같은 지표 계산 함수는 1개"가 배지 통일의 과녁 |
| `persona-claimed-vs-verified.md` | 재사용 | 안내 문구가 가리키는 화면 이름·이슈 해소 주장 대조 |
| `persona-zero-base-redesigner.md` (발산) | 재사용 | Standard라 생략 가능하지만 M1의 구조적 대안을 내기 위해 포함 |

## 지적 사항

| ID | 심각도 | 위치 | 문제 | 확인방법 | 개선안 |
|---|---|---|---|---|---|
| M1 | 🟠 Major | `src/errorMessage.ts:23-25,80-83` (`clampLength`, 머리 320 + 꼬리 160자) ← 호출부 `src/views/catalog.ts:367-377`, `settings.ts`·`catalog.ts`의 `sanitizeBackendText(result.message)` | 플러그인 커맨드 실패 메시지에는 `plugins/mod.rs:165-197,204-232`의 **진단 블록**(주입 PATH 전체, ProgramFiles 등)이 붙는다. 520자 절단이 그 한가운데를 잘라 **PATH 전체와 `ProgramFiles:` 줄이 사라진다.** 이 두 값은 미해결 blocker hub `01m30vamyw8z58b9pk8h5epmgh`(STATUS "PATH 전문·ProgramFiles 필요")가 사용자에게 받아야 하는 값 그 자체다. 변경 전에는 결과 노트에 원문이 다 보였으므로 **회귀**다 | 실제 진단 블록 형식대로 724자 Windows 메시지를 만들어 `sanitizeBackendText`에 넣음 → 489자, `- ProgramFiles: ` 없음, PATH는 `…C:\Windows;C` 뒤로 중략. 실제 Windows PATH는 보통 1천 자가 넘어 더 많이 잘린다 | 진단 블록(`\n\n[진단 정보` 이후)은 절단 대상에서 빼고, 접을 수 있는 `<pre>`로 마스킹만 해서 보여준다(devTools 로그 패널의 `maskSecrets(logTail)` 방식). 토스트에만 절단을 적용한다 |
| M2 | 🟠 Major | `src/views/sessions.ts:866` `maskSecrets(login.output)` ← `src/errorMessage.ts:66`(접두 없는 40자 혼합 토큰 규칙) | PM이 물은 시나리오가 실제로 일어난다. Claude 로그인 URL의 `code_challenge=`(43자 base64url)와 `state=`(43자) 값이 `***`로 바뀌어 **원문 로그에서 URL을 복사하는 폴백이 깨진다.** URL 추출(`claude_auth.rs:701-710`)은 첫 `https://`부터 공백까지를 잡는 단순 로직이라, 끝에 공백이 없거나 OSC8 하이퍼링크·안내 URL이 먼저 오면 실패할 수 있다. 이때 원문 로그가 유일한 수동 경로다. 두 값은 비밀이 아니다(PKCE challenge는 공개값이고 state는 이번 흐름에 묶인 nonce). 인앱 로그인 end-to-end는 STATUS의 최우선 확인 대기 항목이다 | 실제 형식 URL을 node로 `maskSecrets`에 넣음 → `code_challenge=***`, `state=***` 출력. 터미널 로그인 대안(`openClaudeLoginTerminal`)은 남아 있어 완전 차단은 아니라서 Critical로 올리지 않음 | 로그인 원문에는 40자 불투명 규칙을 적용하지 않는 전용 마스커를 쓰거나(접두 토큰·Bearer만), `https://` URL 토큰 안의 쿼리값은 40자 규칙에서 제외한다. 회귀 테스트에 실제 형식 OAuth URL 비변형 케이스를 추가한다 |
| m1 | 🟡 Minor | `src-tauri/src/github_integration.rs:23-24`, `cloudflare_integration.rs:47-48`, 테스트 `github_integration.rs:153`·`cloudflare_integration.rs:174` | 안내가 가리키는 화면 이름 **'개발 환경'은 화면에 없는 라벨**이다. 실제 라벨은 '개발 도구'다: `src/sidebar.ts:258`(탭), `src/views/settings.ts:42`(TAB_META), `src/views/devTools.ts:381`(페이지 제목), 프론트의 같은 상황 안내 `settings.ts:391,507`("개발 도구 화면에서…"). '개발 환경'은 옛 명칭으로 주석(`main.ts:7`, `devToolsApi.ts:1`)에만 남아 있다. 테스트가 틀린 라벨을 고정해 버린다. 이슈 01m36sw8w4kkge1ef7f8ppwwtn의 완료조건에 미달한다 | grep `'개발 환경'\|개발 도구` (src/ 전체) | 두 상수와 테스트를 '개발 도구'로 바꾼다. 가능하면 프론트 문구(settings.ts:391)와 같은 문장으로 맞춘다 |
| m2 | 🟡 Minor | `src/errorMessage.ts:34-35,49` (SECRET_KEY) | 오탐 실측: `"max_token": 1234567` → `***`(단수형 키), `PWD=/Users/...` → `***`(경로가 가려짐, 주석의 "경로는 가리지 않는다"와 어긋남), `Authorization: required header missing` → `Authorization: *** header`. **PM이 걱정한 `input_tokens=1234567`·`"cache_read_input_tokens":123456`은 가려지지 않는다**(복수형 `tokens` 뒤에 `s`가 와서 `[:=]` 조건에 안 맞음). `tokenizer=`·`session_secret_rotation_interval=`도 안전 | node 샘플 18종 | 키 끝 경계 `(?![A-Za-z])` 위에 숫자만 있는 값(`^\d+$`)을 제외하고, `pwd`는 `\bpwd\b`가 아니라 `passwd`/`password`만 남기는 쪽을 고려한다. 급하지 않음 |
| m3 | 🟡 Minor | `src-tauri/src/plugins/mod.rs:566-583` | 폴백 뒤 최종 실패가 다른 원인이면 **마지막 시도의 메시지만** 보인다. 사용자도 지원 담당도 "`-y`가 거부돼 빼고 다시 시도했다"는 사실을 모른다. 구버전 CLI에서 `-s user`까지 빠지면 기본 스코프가 user가 아닐 수 있고, 그러면 "설치 완료"인데 앱의 설치 목록(scope="user"만 취급, `plugins/installed.rs`)에 안 보일 수 있다. 구버전 기본값은 **미확인**이다(현 2.1.280 도움말은 `default: "user"`) | 코드 추적, `claude plugin install --help` | 폴백이 일어났으면 결과 메시지 끝에 "(구버전 claude CLI 호환: `-y`/`-s` 없이 재시도함 — `claude update` 권장)"을 붙인다(성공·실패 모두) |
| m4 | 🟡 Minor | hub 이슈 `01m2wm459wfe4s6g6kzgcwqd45` | 이슈 본문의 원인 가설은 둘이다: ①구버전 CLI ②Windows `.cmd` shim 인자 전달 문제. 이번 수정은 ①만 다룬다. "-y 옵션이 없다"는 원문 stderr가 아니라 요약이라서, 실제 문구가 commander 형식(`unknown option '-y'`)이 아니면 폴백이 아예 일어나지 않는다. fail-safe라서 해는 없지만 **이 diff만으로 이슈를 resolve하면 claimed 해소**다 | hub `project_search_history`로 이슈 본문 확인 | 이슈는 열어 둔 채 "구버전 폴백 적용, Windows 실기 원문 stderr 확인 대기"로 갱신한다 |
| n1 | ⚪ Nit | `src/errorMessage.ts:49,51` | 정규식 비용 실측(16KB 병적 입력): `a.` 반복 121ms, `a-` 반복 123ms, `--a` 반복 116ms. 그 밖의 입력은 0.1~57ms. `maskSecrets(login.output)`·`maskSecrets(logTail)`은 사전 절단(PRE_CUT) 없이 **렌더마다** 호출된다. 실제 입력은 짧아 문제 수준은 아니다 | node `performance.now()` | 로그 블록도 PRE_CUT을 거치게 하거나 마스킹 결과를 상태에 저장 시점에 한 번만 계산한다 |
| n2 | ⚪ Nit | `src/errorMessage.ts:66` | npm `sha512-<base64>` integrity가 `***+/w==`로 가려진다(비밀이 아님). 진단에 거의 필요 없으므로 기록만 남긴다 | node 샘플 | 필요하면 `sha\d+-` 접두를 제외한다 |
| n3 | ⚪ Nit | `cloudflare_integration.rs:48` | 문구는 "Wrangler CLI"인데 개발 도구 화면의 라벨은 "Wrangler"다(`dev_tools/mod.rs:208`). GitHub는 "GitHub CLI"로 일치한다 | grep | m1 수정 때 함께 맞춘다 |
| R1 | 🔵 Rethink | `src/errorMessage.ts` 전체 + 표시 지점 66곳 | 정리를 **표시 시점에 66곳에서** 하기 때문에 "짧게 보여주기"와 "진단 값 보존"이 한 함수 안에서 충돌한다(M1의 근원). 새 표시 지점이 생기면 또 누락될 수 있다 | 코드 구조 추적 | **대안 구조:** Rust `CommandResult`/Err를 `{ summary, diagnostic? }`로 나눠 경계에서 한 번만 마스킹한다. 프론트는 `summary`만 토스트에 쓰고, `diagnostic`은 "진단 정보 보기/복사" 접힘 영역 하나에서만 보여준다. 표시 지점은 컴포넌트 하나로 수렴한다. MVP 속도 원칙상 이번 라운드는 M1 국소 수정으로 충분하다. 다음 진단 회신 요청 전에 검토할 것을 권고한다 |

## 페르소나별 관점

### 화면 텍스트 가림막 감시자 (신규)
- D1 원문 노출 경로: diff 이후 `instanceof Error`가 남은 곳은 0건이다(`errorMessage.ts` 제외). 원시 `result.message`/`.error ??`도 정리 함수를 거친 값만 남았다(`sessions.ts:928,1182`는 저장 시점에 이미 정리됨). **충족.**
- D2 비밀 형태: `ANTHROPIC_API_KEY=sk-ant-…`, `x-github-token: ghs_…`, `auth_token=…`이 실측으로 가려짐을 확인했다. PM의 스모크와 일치한다. **충족.**
- D3 복사 값 보존: **미충족**(M1 진단 블록, M2 OAuth URL).
- D4 정상값: 커밋 SHA·ULID·Windows 경로·pnpm 경로·`*_tokens=`는 비마스킹이다. 다만 단수 `max_token`과 `PWD`는 가려진다(m2). **대체로 충족.**
- D5 비용: 최악 약 120ms(n1). **충족.**

### 프로세스 실행 보안 (재사용) — PM 질문 3에 대한 답
- **신버전 argv 동일성: 충족.** `install_args(ID, ALL)` = `["plugin","install",ID,"-y","-s","user"]`로 이전과 같은 순서다(테스트 `install_args_all_flags_matches_legacy_argv`, `new_cli_succeeds_first_try_with_legacy_argv_and_no_retry`). 첫 시도가 성공하면 재시도가 없다.
- **비멱등 부작용: 없음으로 판단.** commander는 옵션 파싱 단계에서 거부하고 action을 실행하지 않는다. 현 CLI에서 `claude plugin list -y`가 즉시 `error: unknown option '-y'`로 끝나는 것을 확인했다. 재시도는 "unknown/unrecognized option + 지금 붙어 있는 `-y`/`-s`" 조건에서만 일어나므로, 부분 설치 뒤 실패(다른 메시지)는 재시도되지 않는다(`mod.rs:533-541`, 테스트 `unrelated_failures_do_not_trigger_fallback`). 같은 id를 재설치하는 것은 기존 주석(`mod.rs:38-40`)상 안전하다.
- **락:** 재시도 루프 전체가 `run_blocking` 클로저 안(`mod.rs:490-496`)에 있어 `PLUGIN_OP_LOCK`이 시도 사이에 풀리지 않는다. 최대 3회, 플래그가 줄기만 하므로 유한하다.
- S1(셸 없음·절대경로)은 변화가 없다. 인자는 정적 문자열과 `plugin_id`뿐이다.

### 비동기 응답 순서 감시자 (재사용) — A4 단일 정의
- `archiveStatus`를 직접 판정하던 곳이 `src/` 전체에서 0건이 됐다(`workspaceApi.ts:21` 타입 선언만 남음). 카드 `projects.ts:541`, 상세 `:564`, 필터 `:34`, 사이드바 `sidebar.ts:362`, 홈 통계 `home.ts:443`가 모두 `projectBadgeStatus`를 쓴다. **충족.**
- 판정이 바뀌면서 STATUS.md가 없거나 파싱에 실패한 프로젝트는 '진행' 필터와 홈 "진행 N"에서 빠지고 '전체'에서만 보인다(`DashboardFilter`에 unknown이 없음, `state.ts:20`). 의도에 맞으므로(근거 없는 "진행" 금지) 지적하지 않는다.

### 주장 vs 확인 (재사용)
- P2-10: 문구의 화면 이름이 실제 라벨과 다르다(m1). 테스트가 그 오류를 고정한다. "테스트 통과"가 "안내가 맞다"를 보증하지 않는다.
- P1-5: 원인 가설 중 하나만 다루므로 Windows 실기 확인 전까지 해소를 주장할 수 없다(m4).

### 제로베이스 재설계자 (재사용, 발산)
- R1 참조.

## 트레이드오프
- **간결함 vs 진단 값(M1):** 토스트가 깨지지 않게 하려는 절단이 blocker 진단에 필요한 값을 지운다. 권고: 토스트는 절단하고, 결과 노트·진단 블록은 절단하지 않고 접는다.
- **마스킹 범위 vs 사용성(M2, m2):** 40자 불투명 규칙은 Cloudflare 토큰처럼 접두가 없는 비밀을 잡는 유일한 방어선이라 지울 수는 없다. 권고: 문맥(로그인 원문, URL 쿼리)에서만 제외한다.

## 잘 된 점
- `extractRaw`가 문자열·Error·객체·중첩 `{error:{message}}`·JSON 폴백까지 다뤄서, Tauri reject가 문자열이라 원문이 버려지던 근본 원인을 정확히 막았다. "원문이 빌 때만 폴백"이라는 완료조건을 충족한다.
- 마스킹 전 PRE_CUT(16KB)으로 정규식 비용 상한을 둔 설계. 커밋 SHA·ULID·경로를 일부러 보존한 판단이 node 실측과 일치한다.
- install 폴백이 "거부 신호가 안 맞으면 아무 일도 안 일어난다"는 fail-safe 구조다. 순수함수로 분리하고 가짜 실행기를 주입해, 실설치 없이 argv 동일성·종료성·비대상 실패 무재시도를 테스트로 고정했다.
- 배지 판정을 순수함수 하나로 모으고 5개 소비처를 전부 옮겼다. 잔존 0건이다.
- 범위 밖 변경이 없다. diff stat 17파일 + 신규 2파일이 모두 4건 중 하나에 대응한다(dom.ts 토스트 유지 시간, styles.css 줄바꿈은 P1-2 부속).

## 기각된 지적
| 후보 | 기각 사유 |
|---|---|
| "재시도가 부분 설치를 남긴다" | commander는 파싱 단계에서 거부하고 action을 실행하지 않음(`claude plugin list -y` 실측). 다른 실패 메시지는 재시도 대상이 아님(`mod.rs:533-541`) |
| "재시도 사이에 락이 풀려 다른 작업이 끼어든다" | 루프 전체가 `run_blocking` → `with_op_lock` 클로저 안(`mod.rs:416-420,490-496`) |
| "`input_tokens=1234567` 같은 정상 수치가 가려진다" | node 실측 결과 비마스킹. 단수형 키만 해당하므로 m2로 강등 |
| "진단 블록의 PATH가 40자 규칙으로 가려진다" | 경로 조각은 앞이 `\`·`/`라 lookbehind로 제외됨(실측: Windows 경로 비변형). 문제는 마스킹이 아니라 절단이므로 M1로 분리 |
| "범위 밖 변경" | 해당 없음(위 잘 된 점 참조) |

## 생략한 관점·미확인
- **화면 캡처 생략:** 변경이 문구·CSS 줄바꿈 수준이다. 하네스는 TS/DOM/CSS 층만 검증할 수 있고, 실제 Tauri 창 기동과 토스트 6초·`pre-line` 육안 확인은 하지 않았다. 토스트 `max-width: min(560px, …)`의 실제 렌더는 **미확인**이다.
- tsc / pnpm build / cargo test 705는 재실행하지 않았다(PM 주장 인용).
- 구버전 claude CLI의 실제 거부 문구와 기본 스코프, Windows 실기는 **미확인**이다.
- 개발 도구 화면이 macOS/Windows에서 gh·wrangler "설치"를 실제로 제공하는지(Windows wrangler는 수동 플랜)는 이번에 다시 확인하지 않았다.

## PM 권고
1. M1·M2·m1은 이번 라운드 안에서 고친 뒤 커밋할 것을 권고한다(각각 수 줄 수준). M1은 blocker `01m30vamyw8z58b9pk8h5epmgh`의 진단 회신을 막는다.
2. 수정 뒤 재검토는 축소 모드로 충분하다. 증거로 진단 블록 비절단, OAuth URL 비변형 테스트, '개발 도구' 문구를 요구한다.
3. 이슈 `01m2wm459wfe4s6g6kzgcwqd45`는 resolve하지 말고 Windows 실기 원문 stderr 확인 대기로 갱신한다. `01m489pne4yhmv5xaysxfd1bry`(배지)는 resolve해도 된다. `01m36sw8w4kkge1ef7f8ppwwtn`은 m1을 고친 뒤 resolve한다.
