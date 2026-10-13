# 리뷰 보고서 (2차 재검토) — R4 에러 원문 보존·플러그인 설치 폴백·배지 통일·미설치 안내

- **리뷰 대상:** target_id `r4-error-text-plugin-fallback-badge` — HEAD 4aae413 대비 워킹트리 diff(재검토 반영분 포함, 미커밋)
  - 재검토 반영분: `src/errorMessage.ts`(persistent 경로, `maskLogText`), `src/views/{catalog,settings,sessions,devTools,autonomousTasks}.ts`, `src-tauri/src/{github,cloudflare}_integration.rs`, `src-tauri/src/plugins/mod.rs`(`with_fallback_note` + 테스트)
- **직전 리뷰:** `docs/reviewer/review-r4-error-text-plugin-fallback-badge-2026-10-13.md` (Amber, M1·M2 Major)
- **등급/모드:** Standard, 2차 **증분(B)** 모드
  - 풀패널 강제 승격 조건: 해당 없음. 인증·배포·결제 로직은 바뀌지 않았고 표시 문자열만 바뀌었다.
  - 동일 대상 인정: 같은 target_id이고 diff 범위가 직전 파일 집합 안에 있다.
  - 증분 모드를 고른 이유: URL 면제와 숫자값 면제라는 새 리스크 표면이 생겼다.
- **일자:** 2026-10-13
- **종합 판정: 🟢 Green** — Critical·Major 0건. M1·M2·m1·m2·n1·n3은 해소됐다. 새로 Minor 1건(m3 일부 미해소)과 Nit 3건이 나왔다.
- **실행 액션:** 하지 않았다. 코드 수정, git 조작, 설치·인증은 없다. `errorMessage.ts` 사본을 scratchpad에 두고 `node --experimental-strip-types`로 실측만 했다.
- **PM 주장 재검증:** tsc, pnpm build, cargo test 709는 다시 돌리지 않았다(PM 주장 인용). 마스킹·절단·노트 표시 여부는 node 실측과 코드 추적으로 직접 확인했다.

## 페르소나 재사용 판정

착수 전 `docs/reviewer/personas/INDEX.md`를 Read해 역할개념 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-displayed-text-redaction-vs-diagnostic-value.md` | 재사용 | URL 면제·숫자값 면제·persistent 비절단이 역할개념의 직접 대상이다 |
| `persona-claimed-vs-verified.md` | 재사용 | "폴백 노트를 성공·실패 모두에 붙였다"(테스트 통과)는 주장과 실제 화면 표시를 대조한다 |
| `persona-zero-base-redesigner.md` (발산) | 재사용 | Standard라 생략할 수 있지만 R1 이월 상태를 판정하려고 넣었다 |
| `persona-process-execution-security.md`, `persona-async-response-ordering.md` | 이번 라운드 생략 | install argv와 배지 판정 로직은 직전 이후 바뀌지 않았다. `with_fallback_note`는 메시지만 바꾸고 argv와 루프는 그대로다 |

## 직전 지적 해소 상태

| ID | 직전 | 상태 | 근거(확인방법) |
|---|---|---|---|
| M1 진단 블록 절단 | 🟠 | **해소** | `errorMessage.ts:188-198` persistent는 `\n\n[진단 정보` 이후를 자르지 않고 마스킹만 한다. 소비처는 `catalog.ts:111,137,163,368,374`, `settings.ts:1331`. node: 40개 항목 PATH 진단 1463자 → persistent 1424자. `- ProgramFiles:`와 PATH 끝(`Tool39\bin`)이 보존되고, 블록 안 40자 토큰은 마스킹이 유지됐다 |
| M2 OAuth URL 마스킹 | 🟠 | **해소** | `sessions.ts:866` `maskLogText(login.output, true)`. node: 실형식 authorize URL에서 `code_challenge`(43자)·`state`(43자)가 원형으로 남았다. 같은 입력을 `keepUrls=false`로 넣으면 `***`가 되어 대조군 역할을 한다. URL 안의 `api_key=`·`access_token=`·userinfo는 계속 가려진다 |
| m1 '개발 환경' 라벨 | 🟡 | **해소** | `github_integration.rs:24-25`, `cloudflare_integration.rs:48-49`가 '개발 도구'로 바뀌었다. 테스트(`:154-157`, `:175-179`)가 '개발 환경' 부재까지 단언한다 |
| m2 SECRET_KEY 오탐 | 🟡 | **해소** | `errorMessage.ts:44-48` `maskKeyValue`. node: `"max_token": 1234567`와 `PWD=/Users/me`, `PWD=C:\Users\me`가 보존된다. `password=12345678`은 계속 가려진다 |
| m3 폴백 사실 비노출 | 🟡 | **일부 해소 → m3' 로 이월** | 실패 경로는 해소됐다. 노트가 persistent 노트에 전문으로 나오고, 토스트에서도 꼬리 160자 안에 노트 전문이 남는다(node). **성공 경로는 화면에 나오지 않는다.** 아래 m3' 참조 |
| m4 이슈 resolve | 🟡 | **PM 수용(열린 채 유지)** | 코드로 해소할 수 없다. 이슈 01m2wm459wfe4s6g6kzgcwqd45는 Windows 실기 stderr 대기로 둔다. 권고와 일치한다 |
| n1 정규식 비용 | ⚪ | **해소** | `maskLogText`가 PRE_CUT + 8개 캐시를 쓴다. node: 60KB 병적 입력 85ms, 캐시 히트 0.02ms. 잔여 내용은 n6 참조 |
| n2 sha512 integrity | ⚪ | 미조치(수용) | 기록만 남긴 항목이다 |
| n3 "Wrangler CLI" | ⚪ | **해소** | `cloudflare_integration.rs:49` "Wrangler"가 `dev_tools/mod.rs:208` label과 일치한다. gh도 "GitHub CLI"로 `:148`과 일치한다 |
| R1 summary/diagnostic 분리 | 🔵 | 이월(권고 유지) | 아래 R1' 참조 |

## 이번 라운드 신규 지적

| ID | 심각도 | 위치 | 문제 | 확인방법 | 개선안 |
|---|---|---|---|---|---|
| m3' | 🟡 Minor | `src/views/catalog.ts:127-133,245-247`, `src/views/settings.ts:1320-1326` ← `src-tauri/src/plugins/mod.rs:576,593-616` | `with_fallback_note`는 성공 결과에도 노트를 붙인다. 그런데 설치 **성공** 시 토스트는 `result.message`를 쓰지 않는 고정 문구이고, 결과 노트는 `!success`일 때만 렌더된다. 그래서 성공 경로의 노트, 특히 `-s user`를 뺐을 때의 "설치 범위가 user가 아닐 수 있어…" 경고가 **어디에도 표시되지 않는다.** 그 경고가 필요한 상황은 "설치 완료" 토스트가 떴는데 목록이 여전히 비어 있는 경우다. 이 상황에서 사용자는 이유를 알 수 없다. 테스트 `dropping_scope_note_warns_about_install_scope`(`mod.rs:1183`)는 메시지 내용만 단언하므로 "노출된다"를 보증하지 않는다. 구버전 CLI가 `-s`를 거부하는지는 여전히 미확인이라 Minor로 둔다 | 코드 추적: 두 핸들러의 성공 분기와 `catalog.ts:245` 렌더 조건 | 성공이어도 `result.message`에 폴백 노트가 있으면 결과 노트를 렌더하거나, 성공 토스트에 `sanitizeBackendText(result.message)`를 덧붙인다. 노트 마커(예: `(구버전 claude CLI 호환`)로 판별해도 충분하다 |
| n4 | ⚪ Nit | `src/errorMessage.ts:106,121-125` | URL 면제 범위를 실측했다. URL 안에서는 접두 없는 40자 불투명 값이 **어떤 키 이름이든** 노출된다. 실측으로 `?code=<40자>`, `?sig=<40자>`(SAS), `?t=<Cloudflare형>`이 원형으로 남았다. 또 `URL_TOKEN`이 `,`·`)`를 URL에 포함하므로 `https://x.com/a,<토큰>`처럼 붙어 나온 토큰도 면제된다. 다만 `keepUrls=true` 호출부는 `sessions.ts:866` 하나다. 로그인 stdin이 pipe라 사용자가 붙여넣은 코드는 출력에 에코되지 않는다(`claude_auth.rs:22-51`). Claude CLI 로그인 출력에는 공개 쿼리값만 있으므로 현재 노출 경로는 없다고 판단한다. 새 호출부가 `true`를 쓰는 순간 노출 경로가 된다 | node 샘플 8종(URL 안/밖, 붙은 토큰, 괄호) | `keepUrls`의 doc 주석에 "공개 쿼리값만 있는 출력에만 사용"을 명시한다. 더 좁히려면 면제를 `code_challenge`·`state`·`client_id` 쿼리 키 허용목록으로 바꾼다 |
| n5 | ⚪ Nit | `src/errorMessage.ts:45-46` | 숫자·경로 면제 트레이드오프를 실측했다. `access_token=12345678`, `otp_token=123456`(6자리 TOTP·MFA 코드)이 보존된다. `pwd=/s3cret`처럼 `/`로 시작하는 비밀번호도 보존된다. 반대로 `password=`, `api_key=` 숫자값은 가려진다. 실제 액세스 토큰이 순수 숫자인 경우는 드물다. OTP는 수명이 짧고 이 앱 stderr에 나올 경로도 확인되지 않았다. 그래서 수용 가능한 트레이드오프로 본다 | node | 보수적으로 바꾸려면 숫자 면제를 `max_`·`_count`·`_limit`·`tokens?$` 같은 수치형 키로 한정한다. 급하지 않다 |
| n6 | ⚪ Nit | `src/errorMessage.ts:188-198`, 호출부 `catalog.ts:368,374` | persistent 경로는 캐시 없이 **렌더마다** 마스킹한다. 진단 블록 상한이 16KB라 병적 입력에서 렌더당 약 135ms다. 실제 진단 블록은 1~2KB라 무시할 수준이다 | node | 필요하면 `maskLogText`와 같은 캐시를 공유한다 |
| R1' | 🔵 Rethink | `src/errorMessage.ts` 전체 | 이번 수정은 `\n\n[진단 정보` 문자열 마커로 summary와 diagnostic을 프론트에서 다시 나눈다. 마커 문구가 백엔드(`plugins/mod.rs:182,216`)와 프론트(`errorMessage.ts:176`)에 따로 있어 한쪽만 바뀌면 조용히 절단 경로로 돌아간다. 실측상 `\r\n\r\n`이 붙으면 마커를 못 찾고 490자로 잘린다. 현재 백엔드는 `\n`만 쓰므로 실해는 없다. 전 표시 지점 66곳에서 토스트용·노트용 중 하나를 고르는 구조도 그대로다 | node, grep | **대안 구조(직전과 동일):** `CommandResult`에 `diagnostic?: string` 필드를 둬 마커 파싱을 없앤다. 이번 라운드에서는 하지 않는다. 최소 조치로 마커 문자열 일치 테스트를 양쪽에 하나씩 둔다 |

## 요청 질문별 답

1. **URL 면제가 쿼리 안의 실제 비밀을 노출하는가:** key=value 규칙(`token`·`secret`·`api_key`·`access_token`·`client_secret` 등)과 알려진 접두(sk-ant-, ghp_, ya29., AIza, JWT), userinfo는 URL 안에서도 가려진다(실측). 노출되는 것은 비밀 키 이름이 아닌 쿼리값(`code`, `sig`, `t`)에 들어간 접두 없는 40자 값뿐이다. 현재 유일한 호출부(로그인 출력)에는 그런 값이 없다. 판정은 n4다.
2. **`access_token=12345678` 숫자값 면제:** 보존되는 것을 실측했다. TOTP류 6자리도 보존된다. 수용 가능한 트레이드오프로 본다. 판정은 n5다.
3. **persistent 경로에서도 토스트 절단이 유지되는가:** 유지된다. `errorMessage`·`sanitizeBackendText`는 바뀌지 않았다. 같은 1463자 입력이 토스트 488자, persistent 1424자였다. 토스트 호출부(`catalog.ts:103,112,131,138`, `settings.ts:1324,1332`)는 모두 비persistent 함수다.
4. **진단 블록 뒤에 붙은 fallback note 표시:** 실패 시에는 표시된다. persistent 노트에는 블록 안에 전문이 나오고, 토스트에서도 꼬리 160자에 노트 전문(스코프 경고 포함)이 들어간다(실측). 성공 시에는 표시되지 않는다(m3').

## 페르소나별 관점

### 화면 텍스트 가림막 감시자 (재사용)
- D3 복사 값 보존이 직전 **미충족에서 충족**으로 바뀌었다(M1, M2 해소).
- D2 비밀 형태는 URL 면제 이후에도 접두·키 기반 규칙이 유지된다. 남은 위험은 경계 정의 하나다(n4).
- D4 정상값: `max_token`·`PWD` 오탐이 없어졌다. 대신 면제가 넓어진 쪽의 트레이드오프가 n5다.
- D5 비용: 로그 블록은 캐시로 묶였다. persistent는 캐시가 없지만 실입력이 작다(n6).

### 주장 vs 확인 (재사용)
- "폴백 노트를 성공·실패 모두에 붙였다"는 백엔드 기준으로는 참이다. 화면 기준으로는 성공 경로가 비어 있다(m3'). 테스트 통과가 사용자 노출을 보증하지 않는다는 점은 직전 m1과 같은 형태다.
- PM 실측 수치(1479→1478자, 토스트 490자)는 이번 node 실측(1463→1424 / 488, 입력이 다름)과 방향이 일치한다.

### 제로베이스 재설계자 (재사용, 발산)
- R1' 참조. 문자열 마커 분리는 MVP 국소 해법으로 유효하다. 다음에 진단 회신 기능을 넓힐 때는 구조 필드로 올리는 것이 맞다.

## 트레이드오프
- **URL 면제 범위(n4):** 키 허용목록 방식은 더 안전하지만 Claude CLI가 쿼리 키를 바꾸면 M2가 재발한다. 현재의 "URL 안에서는 불투명 규칙만 끔"은 호출부가 하나일 때는 합리적이다. 권고는 주석으로 사용 조건을 고정하는 것이다.
- **숫자 면제(n5):** 오탐을 줄이는 대가로 숫자형 OTP가 보존된다. 이 앱의 출력원에서 OTP 경로가 확인되지 않아 현행을 유지한다.

## 잘 된 점
- persistent와 토스트를 **함수 단위로 분리**하고 각 호출부에 주석을 달았다(`catalog.ts:110`). 그래서 어느 지점이 절단되는지 grep으로 바로 대조할 수 있다.
- `maskLogText`가 URL 안에서도 접두·키 규칙을 그대로 적용하고 불투명 규칙만 끈다. 최소 면제 원칙을 지켰다. 대조군(`keepUrls=false`)으로도 차이가 정확히 그 규칙 하나임을 확인했다.
- 미설치 안내 테스트가 '개발 환경'·`npm install`·`http`의 **부재**까지 단언한다. 직전 m1 같은 회귀를 막는다.
- 폴백 테스트가 "폴백 없으면 메시지 불변"(`no_fallback_failure_message_is_unchanged`)을 고정했다. 그래서 신버전 경로는 문구까지 기존과 같다.

## 기각된 지적
| 후보 | 기각 사유 |
|---|---|
| "URL 면제로 로그인 시 붙여넣은 인증 코드가 노출된다" | 로그인 자식 stdin은 pipe이고 출력에 에코되지 않는다(`claude_auth.rs:22-51`). 붙여넣는 코드는 `https://` URL 형태도 아니다 |
| "persistent 경로가 토스트에도 쓰여 토스트가 커진다" | 토스트 호출부 전부가 `errorMessage`/`sanitizeBackendText`를 쓴다(grep). 실측 488자 |
| "진단 블록이 맨 앞에 오면(head 빈 문자열) 형식이 깨진다" | `errorMessage.ts:197`이 빈 head를 처리한다. 실측 출력 `[진단 정보]\n- a` |
| "폴백 노트 때문에 재시도 판정이 오동작한다" | 노트는 루프 종료 지점(`mod.rs:576,581`)에서만 붙는다. `next_flags_after_rejection`은 노트 전 원문만 본다 |

## 생략한 관점·미확인
- 화면 캡처는 생략했다. 문자열·조건부 렌더 변경이고 Tauri 창 실기동을 하지 않았다. 노트 `pre-line` 렌더와 1.4KB 노트의 실제 높이는 **미확인**이다.
- tsc, build, cargo test 709는 재실행하지 않았다(PM 주장 인용).
- 구버전 CLI 거부 문구와 기본 스코프, Windows 실기는 계속 **미확인**이다(m4 대기).
- process-execution-security와 async-response-ordering 관점은 대상 코드가 바뀌지 않아 생략했다.

## PM 권고
1. 커밋해도 된다(Green). m3'는 성공 분기에 수 줄을 고치면 되므로 이번 커밋에 함께 넣는 것을 권고한다. 그래도 Major가 아니라서 게이트는 아니다.
2. n4는 `keepUrls` doc 주석에 사용 조건을 한 줄 추가하는 것을 권고한다.
3. 이슈 처리:
   - `01m36sw8w4kkge1ef7f8ppwwtn`(미설치 안내)은 m1이 해소됐으므로 resolve해도 된다.
   - `01m489pne4yhmv5xaysxfd1bry`(배지)는 직전 권고대로 resolve해도 된다.
   - `01m2wm459wfe4s6g6kzgcwqd45`는 열어 둔다.
   - blocker `01m30vamyw8z58b9pk8h5epmgh`는 진단 회신을 막던 요인(M1)이 사라졌을 뿐이다. 실제 PATH 회신을 받을 때까지 열어 둔다.
