# 기기 단위 장기 토큰 인증 — 3저장소 공통 설계 (정본)

- 상태: **설계 개정판(보안 검토 반영)** — 구현 전. security 설계 검토(`docs/security/device-token-auth-design-review.md`, Amber, Major 5건)의 "구현 착수 전 반영 필수 항목"을 반영했다. 항목별 반영 위치는 말미 §20 "보안 검토 반영 대응표"에 있다. 사람 승인은 아직 받지 않았다.
- 등급: Sensitive. 인증 토큰의 발급·검증·저장·갱신·폐기 전 과정을 바꾼다.
- 범위: malgnai-hub MCP 인증을 "세션마다 OAuth 재인증"에서 "기기 단위 장기 토큰(최대 365일)을 맑은에이전트 데스크톱 앱이 관리"하는 방식으로 바꾼다. 방향은 이미 확정됐고, 이 문서는 구현 방법만 다룬다.
- 대상 저장소(브랜치는 모두 `feat/device-token-auth`)
  - 서버: `malgnai-public` (Cloudflare Workers, D1, Hono). 아래에서 경로 앞에 `[S]`를 붙인다.
  - 플러그인: `claude-plugins` / `malgn-agent`. 경로 앞에 `[P]`를 붙인다.
  - 앱: `malgn-vscode` (Tauri, Rust). 경로 앞에 `[A]`를 붙인다.
- 이 파일이 세 저장소 공통의 정본이다. 다른 저장소에는 이 파일을 가리키는 포인터만 둔다(§18).
- 인용 표기: `[S] server/api/oauth.js:206`은 `feat/device-token-auth` 워크트리에서 2026-10-09에 확인한 줄 번호다.

---

## 1. 핵심 결정 요약

| # | 결정 |
|---|---|
| D1 | 발급 인가는 **기존 hub OAuth 2.1(PKCE, 루프백)을 재사용**한다. 데스크톱 앱 전용 trusted client_id를 새로 두고, 서버가 그 client_id로 365일 토큰 정책을 고른다. refresh token은 발급하지 않는다. 페어링 방식이나 앱이 가진 Google id_token 교환 방식은 채택하지 않는다. |
| D2 | 토큰은 opaque 32바이트(base64url)에 `mhd_` 접두사를 붙인다. 서버에는 기존과 같은 SHA-256 hex(`device_tokens.token_hash`)만 저장한다. |
| D3 | 만료 상한은 **서버 3곳에서 강제**한다. ① 발급 시 서버 상수로 계산 ② DAO `insert`/`rotateToken`에서 `expiresAt` 누락, **파싱 불가(NaN)**, 상한 초과를 거부 ③ 검증 미들웨어에서 `Date.parse(expires_at)`가 유한하지 않거나(NULL 포함) 지난 값이면 거부. 클라이언트는 TTL을 지정할 수 없다. |
| D4 | 갱신은 `POST /api/devices/renew` 하나로 한다. 기존 토큰으로 인증하고, 새 행을 발급한다. 갱신은 토큰당 1회만 허용한다. 직전 토큰은 24시간 유예 후 만료된다. **이미 교체된 토큰(`replaced_by_id IS NOT NULL`)을 다시 제시하면, 만료·유예와 무관하게 재사용(탈취 신호)으로 판정**해 그 계열 전체를 폐기한다(renew는 항상, self는 유예가 지난 경우, `/mcp`는 대상 아님). 계열은 **`device_id` 단위**다. 갱신 가능 시점(`renew_after`)은 서버가 응답에 실어 보내는 값 하나가 정본이다. |
| D5 | 앱은 토큰을 `keyring` 3.x(macOS Keychain, Windows Credential Manager)에만 저장한다. 평문 파일·settings·.env에는 저장하지 않는다. |
| D6 | CLI 모드는 같은 실행파일에 `--hub-auth-header` 플래그로 진입한다. `main()`에서 Tauri를 초기화하기 전에 분기하므로 GUI가 뜨지 않는다. 네트워크 호출도 파일 쓰기도 없이 키체인만 읽고, 내부 데드라인은 5초다. |
| D7 | 플러그인의 `headersHelper`는 `node "${CLAUDE_PLUGIN_ROOT}/bin/hub-auth-header.mjs"`이다. 래퍼가 앱 경로를 찾는 순서는 ① 앱이 기록한 locator 파일 ② OS별 기본 설치 경로다. PATH에는 의존하지 않는다. |
| D8 | 앱이 없는 PC를 위한 폴백은 **두지 않는다**(권고안 F1). 플러그인에서 `oauth` 블록을 제거한다. 예외 PC는 사용자 범위 MCP를 수동으로 등록하는 탈출구를 쓴다. 서버의 OAuth 경로 자체는 유지한다. |
| D9 | `email_send`는 토큰 종류와 무관하게 `sendEmail()` 한 곳을 거친다. 따라서 `malgnsoft.com` 완전일치 제한이 기기 토큰 경로에도 그대로 적용된다(§11). |
| D10 | 기존의 무기한 토큰(`expires_at IS NULL`, 페어링 발급분)은 마이그레이션 0035에서 `created_at + 365일`로 채운다. 앞으로 페어링으로 발급하는 토큰도 365일로 만든다. |

---

## 2. 코드와 대조해 확인하거나 바로잡은 사실

| 위임 시 전달된 사실 | 코드 대조 결과 | 근거 |
|---|---|---|
| `device_tokens`에 `scopes` 컬럼이 있을 것이다(추정) | **있다.** 기본값은 `'project.read,project.write,telemetry.write'`다. 다만 어디서도 **강제하지 않는다**. identity에 실어 전달만 한다. | `[S] migrations/0001_init_v1_schema.sql:172`, `[S] mcp/device-auth.js:23` |
| `findActiveByHash`가 `users.status='active'`로 JOIN한다 | 맞다. | `[S] server/dao/device-tokens.js:34-40` |
| 페어링 흐름은 `server/api/devices.js`에 있다 | 맞다. 추가로 확인한 점: **페어링으로 발급한 토큰은 `expires_at`이 NULL, 즉 무기한이다.** `insert`에 expiresAt을 넘기지 않기 때문이다. 검증 미들웨어도 NULL을 "만료 없음"으로 통과시킨다. 이는 확정 결정 1(무기한 금지)과 충돌하므로 D10으로 해소한다. | `[S] server/api/devices.js:46-48`, `[S] server/dao/device-tokens.js:13`, `[S] mcp/device-auth.js:16`, `[S] migrations/0008_device_tokens_oauth_client.sql:10-11` |
| 페어링의 `device_id` | 클라이언트가 보낸 값을 그대로 쓴다. 승인된 원문 토큰은 D1의 `device_pairings.raw_token_once`에 잠시 평문으로 머문다. | `[S] server/api/devices.js:14`, `[S] migrations/0001_init_v1_schema.sql:191`, `[S] server/dao/device-pairings.js:42-45` |
| OAuth access TTL 8h, refresh 90일 회전, grace 10분 | 맞다. 웹 JWT(4h/30일, grace 10초)는 별도 축이다. | `[S] server/lib/tokens.js:149-150,158`, `:99-100,112` |
| MCP 요청마다 D1 조회로 검증한다 | 맞다. `/mcp*`에서 `deviceAuthMiddleware`를 먼저 호출하고, 실패하면 401과 함께 `WWW-Authenticate: Bearer resource_metadata=…`를 보낸다. | `[S] server/index.js:201-215` (헤더는 `:210`) |
| OAuth 발급 시 device_id | 서버가 grant마다 새 ULID를 만든다. | `[S] server/api/oauth.js:249` |
| `POST /api/sessions`(사용량 수집)도 같은 토큰 검증을 쓴다 | 맞다. `requireDeviceToken`이 `deviceAuthMiddleware`를 재사용한다. 따라서 D3③을 바꾸면 사용량 수집 경로에도 영향이 간다. | `[S] server/api/sessions.js:12,26-36,44` |
| 사용량 수집 에이전트의 토큰 저장 | 페어링으로 발급받아 `~/.claude/malgnai-hub/usage-agent-credentials.json`에 **평문 JSON(chmod 600)**으로 저장한다. Windows에서는 OS 기본 권한에 맡긴다. | `[P] malgn-agent/bin/pair-usage-device.mjs:132`, `[P] malgn-agent/bin/usage-agent-lib.mjs:69-90` |
| headersHelper 환경변수 제거 대상: TOKEN/KEY/SECRET/AUTH | 공식 문서에는 **PASSWORD도 포함**된다. 이름에 해당 문자열이 들어가면 대소문자와 무관하게 제거한다. | code.claude.com/docs/en/mcp (2026-10-09 열람) |
| 플러그인 mcpServers의 headersHelper는 미검증이다 | **문서상으로는 지원한다.** 플러그인 http 서버에서 `${CLAUDE_PLUGIN_ROOT}`가 `headersHelper` 안에서 치환되고, 같은 이름의 환경변수도 설정된다. cwd는 플러그인 루트다. `${user_config.*}`는 거부된다. 실제 동작은 실측해야 한다(§15). | `[P] docs/anthropic/plugins/plugins-reference.md:598-602,704`, code.claude.com/docs/en/mcp |
| (추가, PM 확인) helper 실행 환경 | Claude Code는 helper에 `CLAUDE_CODE_MCP_SERVER_NAME`, `CLAUDE_CODE_MCP_SERVER_URL`을 설정하고, 플러그인 제공 서버면 `CLAUDE_PLUGIN_ROOT`도 설정한다. 플러그인 서버 helper에서는 이름에 TOKEN/SECRET/PASSWORD/KEY/AUTH가 들어간 환경변수가 제거되고, url에 그 제거 변수의 값이 들어 있으면 `CLAUDE_CODE_MCP_SERVER_URL`의 해당 부분이 `REDACTED`로 바뀐다. 이 설계의 url(`https://malgnai-hub.apiserver.kr/mcp`)에는 변수가 없으므로 원문 그대로 전달된다. 타임아웃은 10초이고, **연결마다 새로 실행하며 캐시하지 않는다.** | code.claude.com/docs/en/mcp headersHelper 절(PM 확인, 2026-10-09) |
| oauth와 headers가 함께 있을 때의 우선순위는 문서에 없다 | 일부는 문서에 있다. "helper 출력에 `Authorization`이 **있으면** OAuth로 폴백하지 않는다." **연결 시** 서버가 helper 자격증명을 거부하면 "needs auth"가 아니라 **연결 실패**로 표시하고 OAuth 플로우로 가지 않는다. **도구 호출 중** 401/403이면 helper를 다시 실행해 1회 재시도하고, 그래도 실패하면 `/mcp`에 "needs authentication"으로 표시한다(PM 확인). helper가 0이 아닌 코드로 끝나거나 `Authorization` 없이 끝날 때의 동작은 여전히 문서에 없다. 그래서 D8에서는 oauth를 함께 두지 않는다. | code.claude.com/docs/en/mcp "Interaction with OAuth" |
| 서버 저장소의 `docs/design/` 규약 | 원본 경로에는 규약이 있다. 그러나 **`docs/` 전체가 `.gitignore` 대상**이라 워크트리에 없고 커밋할 수도 없다. 그래서 서버 저장소에는 포인터를 커밋하지 못했다(§18). | `[S] .gitignore:20`, `[S] CLAUDE.md`의 "docs/는 .gitignore 대상" 항목 |
| (추가) macOS 앱 서명 상태 | **ad-hoc 서명이거나 무서명**이고 공증하지 않는다. 키체인 ACL 위험의 원인이다(§12 R3). | `[A] .github/workflows/tauri-release-build.yml:32-33` |
| (추가) Windows 설치 형태 | NSIS `installMode: currentUser`. release 빌드는 `windows_subsystem = "windows"`다. | `[A] src-tauri/tauri.windows.conf.json`, `[A] src-tauri/src/main.rs:2` |
| (추가) 감사 액션 CHECK | `audit_logs.action`은 CHECK 화이트리스트다. 값을 늘리려면 테이블을 재생성해야 한다. 이 설계는 기존 값 `device_token.issued`/`device_token.revoked`와 metadata만 쓰므로 **audit_logs 마이그레이션이 필요 없다.** | `[S] migrations/0031_audit_logs_project_cross_read_action.sql:18-33` |
| (추가) 최신 마이그레이션 번호 | 0034다. 이번 신규 번호는 **0035**다. | `[S] migrations/` |

---

## 3. 전체 구조와 흐름

```
 ┌──────────── 직원 PC ────────────────────────────────────────────┐
 │  맑은에이전트 앱(GUI)                                            │
 │   ├ hub_auth::connect   ── 브라우저 열기 ──▶ hub /oauth/authorize│
 │   │    (PKCE + 127.0.0.1 루프백, tiny_http)  ◀── code 리다이렉트 │
 │   ├ hub_auth::store     ── keyring ──▶ [Keychain / CredMan]     │
 │   ├ hub_auth::renew     (앱 시작 + 6h 주기, 프로세스 간 락)      │
 │   └ locator 기록        ──▶ <app_config_dir>/cli-path.json      │
 │                                                                 │
 │  Claude Code ── headersHelper ──▶ node bin/hub-auth-header.mjs  │
 │                                     │ (locator → 기본경로)       │
 │                                     ▼                           │
 │                     malgn-agent --hub-auth-header  (CLI 모드)    │
 │                        └ keyring 읽기만 ─▶ stdout JSON           │
 │  Claude Code ── Bearer mhd_… ──────────────────────────────┐    │
 └────────────────────────────────────────────────────────────│────┘
                                                              ▼
 malgnai-hub (Workers)  /mcp → deviceAuthMiddleware → D1 device_tokens(hash)
                        /api/oauth/token (desktop 정책 분기)
                        /api/devices/renew · /api/devices/self
```

### 3.1 발급 (최초 연결)
1. 앱에서 [허브 연결]을 누른다. 앱이 `127.0.0.1:0`에 tiny_http를 띄우고 PKCE verifier와 state를 만든다.
2. 시스템 브라우저로 다음 주소를 연다: `{HUB}/oauth/authorize?client_id={DESKTOP_CLIENT_ID}&redirect_uri=http://127.0.0.1:{port}/callback&state=…&code_challenge=…&code_challenge_method=S256`
3. 로그인되어 있지 않으면 hub의 `/login?redirect=…`로 갔다가 돌아온다(`[S] app/pages/oauth/authorize.vue:115-118`). 사용자는 Google로 로그인하거나 비밀번호로 로그인한 뒤 동의 화면에서 [허용]을 누른다. 동의 화면은 trusted client 이름 "맑은에이전트 데스크톱"과 "365일 유효 기기 토큰"이라는 안내를 표시한다.
4. 루프백으로 code를 받으면 앱이 `POST /api/oauth/token`(authorization_code, code_verifier, client_id)을 호출한다. 서버는 저장된 `stored.client_id`가 desktop client_id와 같은지 보고 desktop 정책을 적용한다. 응답은 `access_token`(=`mhd_…`), `expires_at`, `renew_after`, `device_token_id`이고 refresh는 없다.
5. 앱은 키체인에 저장하고 다시 읽어 일치하는지 확인한다(read-back). 이어서 `GET /api/devices/self`를 호출해 토큰이 실제로 동작하는지 검증한 뒤 UI를 "연결됨"으로 바꾼다.

### 3.2 사용 (세션마다)
Claude Code가 세션을 시작하거나 재연결하거나 401/403을 받으면 `headersHelper`를 실행한다. 래퍼는 앱 CLI 모드를 실행해 키체인 값을 `{"Authorization":"Bearer mhd_…"}`로 출력한다. 서버는 기존 `deviceAuthMiddleware`로 검증한다. 해시 조회, 사용자 활성 상태 JOIN, 만료 확인 순서다.

### 3.3 갱신
앱(GUI)은 시작할 때와 실행 중 6시간마다 다음을 수행한다.
1. `now ≥ renew_after`인지 확인한다.
2. 프로세스 간 락을 획득한다.
3. 키체인을 다시 읽는다(다른 인스턴스가 이미 갱신했을 수 있다).
4. `POST /api/devices/renew`를 호출한다.
5. 새 토큰을 키체인에 쓰고 read-back으로 확인한다.
6. 락을 해제한다.

직전 토큰은 서버 쪽에서 `expires_at = min(기존, now+24h)`로 줄어든다. 이미 연결된 다른 Claude 세션은 직전 토큰을 메모리에 들고 있다. 24시간 안에 401을 받으면 helper를 다시 실행해 새 토큰을 읽는다(helper는 연결마다 새로 실행되고 캐시가 없다). `/mcp`는 재사용 판정 대상이 아니므로, Claude 세션이 들고 있는 옛 토큰은 401만 받고 계열 폐기를 일으키지 않는다.

**재사용 탐지(보안 검토 M1)**: 교체된 토큰을 renew나 self로 다시 제시하면 유예·만료와 무관하게 재사용으로 판정한다(§5.0). 탈취자가 먼저 갱신해 자식을 받았고 정상 앱이 24시간 넘게 꺼져 있었더라도, 앱이 다음에 켜져 키체인의 부모 토큰으로 `GET /self`를 부르는 순간 계열 전체(탈취자의 자식 포함)가 폐기된다.

### 3.4 폐기
- 앱의 [연결 해제]: `DELETE /api/devices/self`를 호출한 뒤 키체인 항목을 삭제한다. desktop 발급분이면 같은 `device_id` 계열 전체가 폐기된다(§5.4). 서버 호출이 실패해도 키체인은 삭제하고 "서버 폐기 실패, 웹 /keys에서 폐기하세요"라고 안내한다.
- 웹 `/keys` 화면(기존 `DELETE /api/devices/:id`, JWT 인증): 본인 또는 관리자가 폐기한다(`[S] server/api/devices.js:95-116`). 기존 동작 그대로다.
- 계정 비활성화: 기존 캐스케이드를 그대로 쓴다(`[S] server/dao/device-tokens.js:71-76`). JOIN 방어선도 유지된다(`:34-40`).

---

## 4. 결정과 트레이드오프

### D1. 발급 인가 경로

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **A (채택)** OAuth PKCE 재사용 + desktop trusted client | 기존 `/oauth/authorize`·`consent`·`token`을 그대로 쓴다. 서버가 `stored.client_id`로 정책을 분기한다. | 이미 리뷰된 코드(PKCE 검증 `[S] oauth.js:209-214`, 코드 1회 소비 `:216-220`, 소유자 활성 확인 `:232-241`, 레이트리밋 `:188-191`)를 재사용한다. 루프백 리다이렉트라 피싱(device code phishing)에 강하다. D1에 원문 토큰이 남지 않는다. 앱에 이미 루프백과 PKCE 구현이 있다(`[A] google_oauth/mod.rs:49,181,294-301`). | client_id는 공개값이다. 로컬 악성 프로세스가 desktop client_id를 사칭해 365일 토큰을 노릴 수 있다. 다만 사용자의 브라우저 동의가 필수다(§12 R5). |
| B 기존 페어링 재사용 | pair-init, 웹 승인, pair-status 폴링으로 발급한다. | 서버 변경이 가장 적다. 루프백 서버가 필요 없다. | 원문 토큰이 D1에 잠시 머문다. 공격자가 시작한 페어링 URL을 피해자가 승인하는 피싱이 가능하다. `device_id`를 클라이언트가 지정한다. 기존 무기한 발급 경로를 다시 정비해야 한다. |
| C 앱의 Google id_token 교환 | 앱이 받은 Google id_token을 hub에 제출하면 기기 토큰을 발급한다. | 로그인을 한 번만 한다. | id_token의 aud가 앱의 client다. hub가 남의 audience를 믿게 되어 신뢰 경계가 넓어진다. 1시간짜리 bearer가 365일 토큰을 찍어내는 열쇠가 된다. hub 쪽 google_sub 연결 로직(auth-google)을 우회하는 새 경로라 리뷰 부담이 가장 크다. |

**포기한 것**: 앱 로그인과 hub 로그인을 하나로 합치는 UX. **감당 방안**: hub 로그인 화면에도 Google 로그인이 있고 브라우저 세션이 살아 있으면 클릭 한 번으로 끝난다. 앱의 Google OAuth는 **토큰은 재사용하지 않고 코드만 재사용**한다. `generate_random_urlsafe`, `code_challenge_from_verifier`, `wait_for_oauth_callback`, 결과 페이지를 `loopback_oauth.rs`로 추출해 두 흐름이 함께 쓴다.

**client_id 분기의 신뢰 근거**: 정책 분기에 쓰는 값은 요청 body가 아니다. **authorization code를 만들 때 동의 화면이 저장한 `stored.client_id`**이고, body 값과 일치하는지도 이미 확인한다(`[S] oauth.js:206`). desktop client_id는 env var `TRUSTED_MALGN_DESKTOP_CLIENT_ID`로만 인정하고, DCR로 등록한 client는 절대 이 정책을 받지 못한다(`resolveClient`에서 trusted 판정이 먼저다, `[S] oauth.js:32-46`).

### D2. 토큰 포맷과 해시
- 원문은 `mhd_` 뒤에 `base64url(32 random bytes)` 43자가 붙은 47자다. 기존 `generateOpaqueToken()`(`[S] server/lib/tokens.js:52-55`)에 접두사만 붙인다.
- 저장: 원문 전체 문자열의 `sha256Hex()`를 `token_hash`(UNIQUE)에 넣는다. 기존과 같다.
- 대안: 접두사 없음. 포기하는 것이 거의 없다(4바이트). 얻는 것은 시크릿 스캐너와 로그 마스킹이 식별하기 쉬워진다는 점이다. `report-usage.mjs`의 Bearer 마스킹 패턴(`[P] bin/report-usage.mjs:179`)도 그대로 적용된다.
- HMAC(pepper)는 채택하지 않는다. 32바이트 무작위 값은 사전 공격 대상이 아니어서 SHA-256 단일 해시로 충분하고, 기존 축과의 일관성을 우선한다.

### D3. 만료 상한 강제 위치
- 정본 상수(신설): `[S] server/lib/tokens.js`에 둔다.
  - `DEVICE_TOKEN_MAX_TTL_SECONDS = 365*86400`
  - `DESKTOP_DEVICE_TOKEN_TTL_SECONDS = DEVICE_TOKEN_MAX_TTL_SECONDS`
  - `DESKTOP_RENEW_WINDOW_SECONDS = 30*86400`
  - `RENEWED_PARENT_GRACE_SECONDS = 24*3600`
- ① 발급: OAuth desktop 분기, renew, pair-approve가 모두 서버 시계로 `expires_at`을 계산한다. 요청 body에 TTL 필드가 있어도 무시한다.
- ② DAO: `insert`와 `rotateToken`은 다음 순서로 검사해 하나라도 걸리면 throw한다. 앞으로 새 발급 경로가 생겨도 상한을 빠뜨릴 수 없다.
  1. `expiresAt`이 없음
  2. `!Number.isFinite(Date.parse(expiresAt))` — 파싱할 수 없는 문자열(보안 검토 m1). 상한 검사보다 **먼저** 둔다. NaN은 모든 비교에서 false라 상한 검사를 조용히 통과하기 때문이다.
  3. `Date.parse(expiresAt) > now + MAX + 60s(skew)`
- ③ 검증: `deviceAuthMiddleware`의 만료 판정을 다음으로 바꾼다(fail-closed). NULL은 `Date.parse(null)`이 NaN이므로 같은 줄에서 함께 거부된다.
  ```js
  const exp = Date.parse(token.expires_at)
  if (!Number.isFinite(exp) || exp <= Date.now()) return { ok: false, reason: 'device token expired or invalid expiry' }
  ```
  현행 `token.expires_at && new Date(..) < now`(`[S] mcp/device-auth.js:16`)는 NULL과 NaN을 모두 통과시킨다(fail-open). 이 수정으로 무기한 토큰과 형식이 깨진 만료값이 구조적으로 통과할 수 없게 된다. 같은 판정을 재사용 탐지 전처리(§5.0)의 "유예 경과" 판정에도 쓴다.
- 대안으로 DB CHECK 제약을 검토했다. `expires_at NOT NULL`을 넣으려면 SQLite 특성상 `device_tokens`를 재생성해야 하고, `oauth_refresh_tokens` FK 같은 이해관계자가 있어 위험 대비 이득이 작다. 그래서 ②와 ③으로 대신한다. 다음에 테이블을 재생성할 일이 생기면 그때 함께 넣는다(미결 U7).

### D4. 갱신 방식

| 안 | 장점 | 단점 |
|---|---|---|
| **새 행 발급 + 1회 갱신 + 직전 토큰 24h 유예 + 재갱신 시 계열 폐기 (채택)** | 키체인 저장에 실패해도 24시간 동안 기존 토큰이 유효하다. 탈취자가 갱신하든 정상 앱이 갱신하든, 두 번째 시도가 계열 전체를 폐기한다. 단일 엔드포인트다. | 컬럼 1개(`replaced_by_id`)가 추가된다. 저장 실패 후 24시간 안에 복구하지 못하면 재연결해야 한다. |
| 같은 행에서 hash 회전(OAuth refresh 방식) | 행 수가 늘지 않는다. | 응답을 받은 뒤 저장에 실패하면 즉시 토큰을 잃는다. 이를 막으려면 prev_hash와 grace 컬럼, 조회 경로 2개가 필요하다. |
| 클라이언트가 새 토큰 해시를 제출 | 크래시에 가장 안전하다. | 엔트로피를 서버가 검증할 수 없어 "클라이언트 신뢰"에 해당한다. 불허한다. |
| renew와 confirm 2단계 | 미확인 자식 토큰을 정리할 수 있다. | 엔드포인트와 상태가 늘어난다. 50인 도구에는 과하다. |

- 갱신 창: `expires_at - now ≤ 30일`일 때만 허용한다. 그 전에 호출하면 409 `RENEW_TOO_EARLY`다. 탈취한 토큰을 아무 때나 연장하는 것을 막는다.
- **renew_after 정본**: 서버가 `expires_at - DESKTOP_RENEW_WINDOW_SECONDS`를 계산해 발급, 갱신, self 응답에 담는다. 앱은 30일을 하드코딩하지 않고 이 값만 따른다. 임계값의 정본은 서버 한 곳이다.
- 원자성: 다음 두 문장을 D1 `db.batch`(트랜잭션) 하나로 실행하고, 두 번째 문장의 `changes`로 성공 여부를 판정한다.

```sql
-- 1) 부모 선점(조건부)
UPDATE device_tokens
   SET replaced_by_id = :childId,
       expires_at = MIN(expires_at, :parentGraceUntil)
 WHERE id = :parentId AND status = 'active' AND replaced_by_id IS NULL;
-- 2) 자식 생성 — 1)이 이번 요청으로 선점됐을 때만
INSERT INTO device_tokens (id, user_id, device_id, device_name, token_hash, status,
                           oauth_client_id, expires_at, created_at)
SELECT :childId, user_id, device_id, device_name, :childHash, 'active',
       oauth_client_id, :childExpiresAt, :now
  FROM device_tokens WHERE id = :parentId AND replaced_by_id = :childId;
```

  `MIN`으로 비교하는 두 값은 모두 `toISOString()` 형식(UTC, `Z`)이라 문자열 비교가 시간순과 같다.
- 재사용(교체된 토큰의 재제시) 판정과 처리는 §5.0 전처리가 맡는다. 판정 기준은 `replaced_by_id IS NOT NULL`이고 만료·유예·status와 무관하다.
- **계열 폐기는 `device_id` 단위 한 문장이다**(보안 검토 m2). desktop 분기는 grant마다 서버가 새 ULID `device_id`를 만들고(`[S] oauth.js:249`) 자식이 그 값을 상속하므로(위 INSERT…SELECT), `device_id`가 곧 계열 식별자다. 초안의 "`replaced_by_id` 체인 추적, 반복 상한 32"는 상한을 넘으면 조용히 잘리므로 폐기했다. 반복도 상한도 없다.
  ```sql
  -- revokeDesktopFamily(db, { userId, deviceId, desktopClientId, now })
  UPDATE device_tokens SET status = 'revoked', revoked_at = :now
   WHERE user_id = :userId AND device_id = :deviceId
     AND oauth_client_id = :desktopClientId AND status = 'active';
  ```
  - `oauth_client_id = desktop` 조건이 있으므로, 클라이언트가 `device_id`를 정하는 페어링 발급분(`[S] devices.js:14`)은 같은 문자열이라도 휩쓸리지 않는다. `user_id`가 함께 있어 다른 사용자도 휩쓸리지 않는다.
  - 감사 기록은 `device_token.revoked` + metadata `{reason:'renew_reuse', revoked_count}`로 남기고 409 `ALREADY_RENEWED`를 반환한다. `revoked_count = 0`(이미 폐기된 계열을 또 제시)이면 감사 기록을 남기지 않는다(같은 토큰 반복 제시로 감사 로그가 부풀지 않게).
  - 계열 폐기가 함께 호출해야 하는 OAuth refresh 정리(`revokeAllForDeviceToken`)는 desktop 분기가 refresh를 만들지 않으므로 필요 없다.

### D5. 앱의 저장소
- `keyring = { version = "3", features = ["apple-native", "windows-native"] }`
- **주의**: keyring 3은 기본 백엔드가 없다. feature를 빠뜨리면 mock 저장소로 조용히 동작해 아무것도 영속되지 않는다. cargo test에 "mock이 아닌 백엔드로 빌드됐는지" 확인하는 가드를 둔다. 예를 들어 실제 백엔드에서 `Entry::new` 후 get_credential 타입을 확인하는 `#[ignore]` 실기 테스트를 두고, 빌드 시점 `cfg` 검사도 함께 둔다.
- 항목: service `com.malgnsoft.agent.hub`, account `device-token`. 값은 JSON 한 덩어리다.
  `{"v":1,"hub":"https://malgnai-hub.apiserver.kr","token":"mhd_…","device_token_id":"…","expires_at":"…","renew_after":"…"}`
  (Windows CredMan blob 상한 2,560바이트 안에 들어가고, 실제 크기는 약 250B다.)
- 대안으로 Stronghold나 자체 암호화 파일을 검토했다. 마스터키를 다시 어딘가에 저장해야 하는 문제가 남아서 OS 키체인 위임이 가장 단순하다. 확정 결정 2와도 일치한다.

### D6. CLI 모드: 같은 바이너리를 쓸 것인가, 별도 바이너리를 둘 것인가
- **채택**: 같은 바이너리 `malgn-agent --hub-auth-header`. 이유는 다음과 같다.
  - 설치 경로를 하나만 해석하면 된다.
  - macOS 키체인 ACL이 "항목을 만든 앱"을 신뢰하므로, GUI가 만든 항목을 같은 바이너리가 읽을 때 프롬프트가 뜨지 않는다. 이는 **가용성(프롬프트 없음)을 위한 이점이지 보안 통제가 아니다.** 같은 사용자 권한의 어떤 프로세스든 이 바이너리를 `--hub-auth-header`로 실행하면 프롬프트 없이 토큰을 받는다. ACL이 신뢰하는 바이너리가 호출자를 가리지 않고 토큰을 내주기 때문이다(§12 R2, 보안 검토 M2). headersHelper 구조에서 이 성질은 본질적이며 수용한다.
  - 번들 구성이 바뀌지 않는다.
- 호출한 부모 프로세스를 검사해 Claude Code만 허용하는 방식은 **도입하지 않는다.** 우회가 쉽고(부모를 흉내 내거나 Claude Code 자체를 띄우면 된다) OS별 오탐이 많아 비용 대비 효과가 없다. 대신 토큰이 나갈 **대상 서버**를 검사한다(§7.3 audience 확인, M3).
- 대안은 Tauri `bundle.externalBin`으로 콘솔 서브시스템 헬퍼 `malgn-agent-auth(.exe)`를 두는 것이다. Windows GUI 서브시스템의 stdout 문제는 확실히 피할 수 있다. 하지만 macOS에서는 ACL상 다른 앱이 되어 프롬프트 위험이 있고, 바이너리 2개를 서명·배포해야 한다. **조건부 대안**: Windows 실측(§16 W2)에서 파이프 stdout이 실패할 때만 Windows에 한해 채택한다.

### D7. 플러그인이 앱 실행파일을 찾는 방법

| 안 | 장점 | 단점 |
|---|---|---|
| **node 래퍼 + locator + 기본경로 (채택)** | 한 문자열로 크로스플랫폼을 처리한다. 셸 인용 문제는 기존 훅과 같은 패턴이다(`[P] malgn-agent/hooks/hooks.json:8,12`). 타임아웃, 출력 검증, 안내 문구를 래퍼가 맡는다. PATH 하이재킹 표면이 없다. | node에 의존한다. 다만 같은 플러그인의 SessionStart/Stop 훅이 이미 `node`를 필수로 쓰므로 새 의존성은 아니다. 프로세스를 2개 띄운다(약 50~100ms). |
| 실행파일 절대경로를 직접 지정 | 프로세스 1개로 끝난다. | 플러그인 하나가 OS 2종과 설치 위치 차이를 한 문자열로 표현할 수 없다. |
| PATH의 `malgn-agent` | 단순하다. | 앱이 PATH에 등록되지 않는다. PATH 하이재킹 표면이 생긴다. |
| 앱이 user-scope MCP 서버를 직접 등록(`claude mcp add … headersHelper=<절대경로>`) | 앱이 자기 경로를 정확히 안다. | 서버 이름이 `plugin:malgn-agent:malgnai-hub`에서 바뀌어 도구 접두사 `mcp__plugin_malgn-agent_malgnai-hub__`에 의존하는 훅과 에이전트가 깨진다(`[P] malgn-agent/hooks/stop-mcp-reminder.cjs:47`). **headersHelper 실측이 실패할 때의 대안 B**로 보류한다(§9.3). |

래퍼(`[P] malgn-agent/bin/hub-auth-header.mjs`)가 실행파일을 찾는 순서는 다음과 같다. 첫 번째로 존재하고, 절대경로이고, 파일명이 `malgn-agent`(Windows는 `malgn-agent.exe`)이며, 아래 쓰기 권한 검사를 통과하는 것을 쓴다. 절대경로·파일명 검사는 오설정 방지용이지 보안 통제가 아니다(§12 R7).
1. 환경변수 `MALGN_AGENT_EXE`: 개발과 실측용 오버라이드다. 이름에 제거 대상 문자열이 없어 helper 환경까지 전달된다. 이 경로를 쓸 때는 **매번 stderr에 "malgn-agent: 개발 오버라이드(MALGN_AGENT_EXE) 사용 중" 경고를 한 줄** 남긴다. 셸 프로필에 심어진 지속형 하이재킹이 눈에 보이게 하기 위해서다(m4).
2. locator 파일: 앱(GUI)이 시작할 때마다 `std::env::current_exe()`를 원자적으로 기록한다(기존 `[A] src-tauri/src/fs_atomic.rs`). 단 `current_exe()`가 `/AppTranslocation/`, `/Volumes/`, OS 임시 디렉터리 아래이면 기록하지 않는다(디스크 이미지에서 바로 실행하는 등 일시 경로를 영구 locator로 남기지 않는다, m4).
   - macOS: `~/Library/Application Support/com.malgnsoft.agent/cli-path.json`
   - Windows: `%APPDATA%\com.malgnsoft.agent\cli-path.json`
   - 내용: `{"exe":"…","version":"0.x.y"}`. Tauri `app_config_dir()` 규칙과 일치한다(identifier `com.malgnsoft.agent`, `[A] tauri.conf.json`).
3. 기본경로
   - macOS: `/Applications/malgn-agent.app/Contents/MacOS/malgn-agent`, `~/Applications/malgn-agent.app/Contents/MacOS/malgn-agent`
   - Windows: `%LOCALAPPDATA%\malgn-agent\malgn-agent.exe`. NSIS currentUser 기본 경로로 추정하며, 실측 대상이다(§16 W1).

- 쓰기 권한 검사(POSIX만, m4): 후보 파일을 `lstat`해 일반 파일인지(심볼릭 링크 아님), 소유자가 현재 uid 또는 root인지, group/other 쓰기 비트가 없는지 확인한다. 상위 디렉터리 2단계도 같은 기준으로 본다. 실패하면 그 후보를 건너뛴다. Windows는 currentUser 설치라 이 검사를 두지 않는다. 공용 PC나 다중 사용자 환경에서만 의미가 있는 Minor 방어다.

locator 파일은 같은 사용자만 쓸 수 있는 위치에 있다. 그 위치를 조작할 수 있는 공격자는 이미 그 사용자 권한으로 코드를 실행할 수 있고, 그렇다면 CLI를 직접 호출해 토큰을 얻는 편이 훨씬 쉽다. 따라서 새 공격 표면이 아니다(§12 R7).

### D8. 앱이 없는 PC의 폴백

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **F1 (권고)** 폴백 없음 + 수동 탈출구 | plugin.json에서 `oauth`를 제거하고 `headersHelper`만 둔다. 앱이 없거나 토큰이 없으면 연결 실패와 함께 "맑은에이전트 앱에서 허브에 연결하세요"를 안내한다. 예외 PC(리눅스 서버, 헤드리스 등)는 문서화된 수동 절차 `claude mcp add --transport http --scope user malgnai-hub-oauth <URL>`로 OAuth를 쓴다. | 인증 경로가 하나다. 문서에 없는 동작(oauth와 helper의 우선순위)에 의존하지 않는다. 목표인 "세션마다 재인증 제거"를 확실히 달성한다. 서버 OAuth는 그대로 유지되어 탈출구가 실제로 동작한다. | 앱 미설치자는 hub를 쓸 수 없다. 탈출구로 등록하면 도구 접두사가 달라진다. |
| F2 oauth와 helper 공존 | helper가 토큰이 없을 때 `{}`를 출력해 OAuth 폴백을 기대한다. | 끊김 없이 이행할 수 있다. | 우선순위가 문서화되지 않았다. 버전에 따라 동작이 바뀔 수 있다. 8h/90d 경로가 계속 주력처럼 남는다. 확정 결정 3("oauth 블록을 빼고")과도 어긋난다. |
| F3 helper가 사용량 수집 에이전트의 평문 파일 토큰을 읽음 | 앱 없이도 동작한다. | 평문 토큰을 정식 경로로 승격하는 셈이라 금지 조건 위반이다. |
| F4 OAuth 전용 두 번째 서버 항목을 플러그인에 추가 | 설치만 하면 된다. | 앱 사용자에게도 OAuth 서버가 노출되어 매 세션 "인증 필요" 소음이 생긴다. 도구가 중복된다. |

근거: 직원 50명 미만이고 전원이 macOS/Windows이며 맑은에이전트 앱이 표준 프로비저닝 도구다. 앱 미설치는 예외 상황이다.

공식 문서로 확인된 동작(§2, PM 확인)과 남은 실측:
- helper가 `Authorization`을 내고 서버가 **연결 시** 거부하면(폐기·만료 토큰) "연결 실패"로 표시되고 OAuth로 가지 않는다. **도구 호출 중** 401/403이면 helper 재실행 후 1회 재시도, 그래도 실패하면 `/mcp`에 "needs authentication"이 표시된다. 이 설계는 이 동작에 맞춰져 있다: 갱신 직후 옛 토큰을 든 세션은 재실행 1회로 새 토큰을 받아 회복한다.
- **미확인(U2)**: ① helper가 0이 아닌 코드로 끝나거나(앱 미설치·미연결) `Authorization` 없이 끝날 때의 표시와, 그때 서버 401의 `WWW-Authenticate`(`[S] server/index.js:210`)를 보고 DCR OAuth discovery로 넘어가는지 ② 도구 호출 401 후 "needs authentication" 상태에서 사용자가 `/mcp`로 인증을 시도하면 OAuth discovery로 가는지. 실측 E5·E12에서 기록한다. 넘어간다면 F1에서도 사실상 DCR OAuth 폴백이 생기는 셈이다. 그 경로는 기존에 리뷰를 마친 8h/90d 경로이고 "unverified" 경고 화면을 거치므로 보안 퇴행은 아니지만(보안 검토 §1 D8 F1 판정), 사용자가 앱 대신 그 경로로 빠지는 UX 문제가 되므로 결과를 이 표에 반영한다.

**안내 문구("맑은에이전트 앱에서 허브에 연결하세요")의 노출 위치**:
- 1차 채널은 **helper의 stderr**다(CLI §7.3 → 래퍼 §8.2가 전달). 그러나 Claude Code가 helper stderr를 `/mcp` 화면이나 연결 오류 상세에 보여 주는지는 **문서에 없어 미결이다(U12).** 실측 E6·E13에서 `/mcp` 상세, `claude --debug` 출력, `~/.claude` 로그 중 어디에 나타나는지 기록한다.
- 2차 채널(확정)은 **앱 GUI**다. 미연결·만료·폐기 상태는 앱 홈/설정의 허브 위젯이 항상 보여 준다(§7.2). 플러그인 릴리스 공지도 "업데이트 전에 앱에서 허브에 연결하세요"로 같은 문구를 쓴다(§17).
- stderr가 사용자에게 보이지 않는 것으로 확인되면 보완 후보는 **플러그인 SessionStart 훅이 CLI 상태를 확인해 안내를 출력**하는 방식이다. 기존 훅이 이미 `node`로 실행되므로 새 의존성이 없다. 다만 훅이 세션마다 CLI를 한 번 더 띄우는 비용과 토큰을 stdout으로 받는 경로가 하나 더 생기는 문제가 있어, CLI에 토큰을 출력하지 않는 상태 조회 플래그가 필요해진다. 그래서 실측 전에는 채택하지 않는다(U12 결과에 따라 결정).

### D9. email_send
§11 참고. 변경 없음.

### D10. 레거시 무기한 토큰
- 대안 ① NULL 유지: 결정 1 위반이라 불가하다.
- 대안 ② 일괄 폐기: 사용량 수집 에이전트가 즉시 전부 중단된다.
- **채택 ③**: `created_at + 365일`로 채운다. 현재 운영 기간이 1년 미만이므로 즉시 만료되는 토큰은 없다. 다만 가장 오래된 토큰은 2027년 7~8월에 만료된다. 그 전에 사용량 수집 에이전트를 재페어링하거나 앱 토큰 경로로 옮겨야 한다(미결 U4).
- 이 레거시 토큰은 **평문 파일**(`usage-agent-credentials.json`)에 있고 `scopes`를 강제하지 않아 `/mcp`·`email_send`까지 쓸 수 있다(§12 R1, 보안 검토 M5). 0035는 무기한을 365일로 줄이는 개선이지만 이 노출 자체는 그대로다. 그래서 U4에 기한과 최소 조치를 붙였다(§18 U4, §17-6).

---

## 5. API 명세 (서버)

공통 사항
- 오류 봉투는 기존 `{ error: { code, message } }`를 쓴다. 단 `/api/oauth/token`은 RFC 형식(`{error, error_description}`)을 유지한다.
- 기기 토큰 인증 라우트는 `PUBLIC_PATHS`에 경로를 **정확히 일치**하도록 추가해 JWT 게이트를 우회시킨다. 그 대신 **라우터 레벨 `use()`로 메서드와 무관하게** `requireDeviceToken`을 강제한다. 함정이 하나 있다. `PUBLIC_PATHS`는 메서드를 보지 않는다(`[S] server/middleware/jwt-auth.js:41-45`). 그래서 같은 경로에 핸들러별로 미들웨어를 붙이면 빠뜨리는 순간 무인증이 된다.
- `requireDeviceToken`은 지금 `[S] server/api/sessions.js:26-36`에만 지역 함수로 있다. 이를 `[S] server/middleware/device-token-auth.js`로 추출하고 sessions와 devices가 함께 import한다. `c.set('deviceTokenId', …)`도 추가한다.
- `deviceAuthMiddleware`의 identity에 `deviceTokenId`를 추가한다(`[S] mcp/device-auth.js:21-24`). MCP props에 필드가 하나 늘어나지만 무해하다.
- **컨텍스트 키 규약(m7)**: 신설 핸들러는 기기 토큰 전용 키(`deviceUserId`, `deviceTokenId`, `[S] server/api/sessions.js:33-34`의 관례)만 읽는다. JWT 키 `userId`는 절대 set하지 않는다. 핸들러 첫 줄에서 `deviceTokenId`가 없으면 500으로 끝낸다(`use()` 누락이나 등록 순서 실수에 대비한 fail-closed).

### 5.0 재사용 탐지 전처리 (신설, 보안 검토 M1)

교체된 토큰(`replaced_by_id IS NOT NULL`)의 재제시를 만료·유예와 무관하게 잡기 위한 전처리다. 초안은 이 판정을 `requireDeviceToken` 뒤에 두었기 때문에, 부모의 24시간 유예가 지나면 1단계 인증에서 401로 끝나 재사용을 탐지하지 못했다(탈취 자식이 365일 생존, 초안 R8·U11).

- 위치: `[S] server/middleware/device-token-auth.js`에 `detectReplacedTokenReuse({ graceAware })`를 둔다. `requireDeviceToken` **앞**에 건다.
  ```js
  devices.use('/renew', detectReplacedTokenReuse({ graceAware: false }), requireDeviceToken)
  devices.use('/self',  detectReplacedTokenReuse({ graceAware: true  }), requireDeviceToken)
  ```
- 대상 경로: `/api/devices/renew`(POST)와 `/api/devices/self`(GET, DELETE)만. **`/mcp`와 `POST /api/sessions`에는 걸지 않는다.** Claude 세션이 메모리에 든 옛 토큰으로 `/mcp`를 부르는 것은 정상 동작이고(§3.3), 거기서 계열 폐기가 나면 정상 사용자가 끊긴다.
- 조회: `deviceTokensDao.findAnyByHash(db, tokenHash)`(신설). status·만료·사용자 활성으로 거르지 않고 해시로만 찾는다(`token_hash` UNIQUE 인덱스 사용).
- 판정: 행이 있고, `replaced_by_id IS NOT NULL`이고, `oauth_client_id === env.TRUSTED_MALGN_DESKTOP_CLIENT_ID`일 때
  - renew(`graceAware:false`): **즉시** 재사용으로 판정한다. 교체된 토큰이 renew를 다시 부를 정상 경우는 없다(앱은 락과 재확인으로 1회만 갱신, §7.2).
  - self(`graceAware:true`): `Date.parse(expires_at)`가 유한하고 미래이면(**유예 안**) 정상 토큰으로 통과시킨다. 유예가 지났거나 값이 유효하지 않으면 재사용으로 판정한다. 유예 안을 제외하는 이유는 앱 인스턴스 두 개가 거의 동시에 self를 부를 때(한쪽이 갱신 직후)의 오탐을 막기 위해서다. 유예 안에 self가 통과해도 탐지가 빠지지는 않는다. 탈취자가 갱신할 수 있었다는 것은 이미 `renew_after`가 지났다는 뜻이고, 앱은 키체인에 든 부모의 `renew_after`가 지났으므로 같은 주기에 renew를 호출해 그 자리에서 409를 받는다.
  - 재사용이면 `revokeDesktopFamily`(§4 D4, `device_id` 단위)를 실행하고, 감사 `device_token.revoked {reason:'renew_reuse', revoked_count, via:'renew'|'self'}`를 남기고(`revoked_count>0`일 때만), 409 `ALREADY_RENEWED`를 반환한다.
  - 그 밖(행 없음, 교체 안 됨, 비-desktop)은 `next()`로 넘겨 `requireDeviceToken`이 기존대로 판정한다.
- 이 전처리는 인증 전에 폐기를 실행하지만, 폐기의 근거는 **원문 토큰 소지**(해시 일치)다. 옛 토큰을 가진 자가 일부러 계열을 폐기시키는 것은 서비스 거부지만, 그 토큰을 가진 자는 유예 안에서 이미 더 많은 일을 할 수 있었고 결과는 "재연결 필요"로 끝나 fail-safe 방향이다.
- 비정상 케이스: `findAnyByHash`가 D1 오류로 실패하면 탐지를 건너뛰고 `next()`로 넘긴다(fail-open이 아니다. 뒤의 `requireDeviceToken`이 다시 D1을 조회해 실패하면 5xx). 폐기 UPDATE가 실패하면 5xx를 반환하고 토큰을 통과시키지 않는다.
- **앱 쪽 전제 조건**(§7.2에 반영): 서버를 호출하기 직전에 매번 키체인을 다시 읽고(메모리 캐시 금지, SAVE_FAILED만 예외), 401을 받아 키체인을 지우기 **전에** 저장된 토큰으로 `GET /self`를 한 번 불러 서버가 재사용을 판정할 기회를 준다.

### 5.1 `POST /api/oauth/token` — desktop 분기 (기존 엔드포인트 변경)
- 인증: 없음(PUBLIC, 기존과 같음). 레이트리밋: 기존 `OAUTH_TOKEN_RL` `${ip}:authcode`.
- 분기 조건: `stored.client_id === env.TRUSTED_MALGN_DESKTOP_CLIENT_ID`. env가 없으면 분기하지 않는다(fail-closed, 일반 경로로 간다).
- redirect_uri(m3): desktop client에 한해 host를 `127.0.0.1`로 **정확히 일치**시킨다(`localhost`, `::1` 거부, RFC 8252 §7.3 권고). authorize 단계와 token 단계 양쪽의 redirect 검증에서 client가 desktop이면 이 좁은 규칙을 쓴다. 기존 `isValidLoopbackRedirect`(`[S] server/lib/oauth-redirect-uri.js:2`)의 허용 집합은 다른 client를 위해 그대로 둔다. 앱은 이미 `127.0.0.1`을 쓴다(`[A] src-tauri/src/google_oauth/mod.rs:294,301`).
- 처리: 소유자 활성 확인(기존 `:232-241`) 다음에 `mhd_` 토큰을 생성한다. `deviceTokensDao.insert({ oauthClientId: stored.client_id, expiresAt: now+365d })`를 호출하고, refresh 행은 만들지 않는다. 감사 기록은 `device_token.issued` + `{via:'oauth', client_id, policy:'desktop'}`이다.
- 요청 예시(form 또는 json): `grant_type=authorization_code&code=…&code_verifier=…&redirect_uri=http://127.0.0.1:53121/callback&client_id=<desktop>`
- 응답 200:
```json
{ "access_token": "mhd_<43자>", "token_type": "Bearer", "expires_in": 31536000,
  "expires_at": "2027-10-09T03:00:00.000Z", "renew_after": "2027-09-09T03:00:00.000Z",
  "device_token_id": "01J…" }
```
- 오류: 기존과 같다(400 `invalid_grant` 등, 429 `slow_down`).
- 회귀 불변식: plugin client_id와 DCR client는 기존대로 8h access와 90d refresh를 받는다.

### 5.2 `POST /api/devices/renew` (신설)
- 인증: `Authorization: Bearer mhd_…`(기기 토큰). PUBLIC_PATHS에 `'/api/devices/renew'`를 추가하고 `devices.use('/renew', detectReplacedTokenReuse({ graceAware:false }), requireDeviceToken)`을 건다.
- 권한: 토큰 소유자 본인만 가능하다. 대상이 곧 인증에 쓴 토큰이므로 IDOR 표면이 없다. body는 받지 않는다(무시).
- 검증 순서:
  0. **재사용 탐지(§5.0)**: 교체된 desktop 토큰이면 만료·유예와 무관하게 계열 폐기 후 409 `ALREADY_RENEWED`.
  1. 기기 토큰 유효성: 해시 일치, `status='active'`, 사용자 활성, 미만료(D3③, NaN 거부). 하나라도 어긋나면 401.
  2. `oauth_client_id === env.TRUSTED_MALGN_DESKTOP_CLIENT_ID`가 아니면 403 `NOT_RENEWABLE`. 페어링이나 OAuth 8h 토큰은 갱신할 수 없다.
  3. (방어선) `replaced_by_id IS NOT NULL`이면 0번과 같이 처리한다. 0번이 먼저 잡으므로 정상 경로에서는 도달하지 않는다. 0번 전처리 등록이 빠졌을 때를 위한 이중 장치다.
  4. `expires_at - now > 30일`이면 409 `RENEW_TOO_EARLY`이고 응답에 `renew_after`를 담는다.
  5. §4 D4의 batch를 실행한다. 두 번째 문장의 changes가 0이면(동시 요청과의 경합) 0번과 같이 처리한다.
- 레이트리밋: `OAUTH_TOKEN_RL`을 `${tokenHash}:renew` 키로 재사용한다. 바인딩을 새로 만들지 않는다. 바인딩 오류 시 fail-open인 것은 기존 정책(`[S] oauth.js:149-159`)을 따른다.
- 응답 200:
```json
{ "device_token": "mhd_<43자>", "device_token_id": "01J…(신규)",
  "expires_at": "2028-10-08T03:00:00.000Z", "renew_after": "2028-09-08T03:00:00.000Z",
  "previous_valid_until": "2027-09-10T03:00:00.000Z" }
```
- 오류 응답:

| 상태 | code | 의미 | 앱 동작 |
|---|---|---|---|
| 401 | UNAUTHORIZED | 무효, 폐기됨, 만료, 계정 비활성 | 키체인 항목을 삭제하고 "재연결 필요" |
| 403 | NOT_RENEWABLE | desktop 발급분이 아님 | 재연결 안내 |
| 409 | RENEW_TOO_EARLY | 갱신 창 이전 | `renew_after`로 일정 재설정 |
| 409 | ALREADY_RENEWED | 교체된 토큰 재제시. 서버가 계열 전체를 이미 폐기함 | 키체인 삭제, "보안상 연결이 해제되었습니다. 다시 연결하세요. 이 안내가 반복되면 관리자에게 알리세요"(m10: 탐지 신호가 사람에게 전달되게) |
| 429 | RATE_LIMITED | Retry-After 포함 | 백오프 |
| 5xx/네트워크 | — | — | 기존 토큰을 유지하고 1h 후 재시도 |

- 감사: 성공 시 `device_token.issued` + `{via:'renew', parent_id}`. 재사용 시 `device_token.revoked` + `{reason:'renew_reuse', revoked_count}`(§5.0).

### 5.3 `GET /api/devices/self` (신설)
- 인증: 기기 토큰. PUBLIC_PATHS에 `'/api/devices/self'`를 추가하고 `devices.use('/self', detectReplacedTokenReuse({ graceAware:true }), requireDeviceToken)`을 건다.
- 유예가 지난 교체 토큰으로 호출하면 409 `ALREADY_RENEWED`이고 계열이 폐기된다(§5.0). 앱은 이를 renew의 409와 똑같이 처리한다. 이 라우트들은 **`devices.delete('/:id')`보다 먼저 등록**한다. 그렇지 않으면 `self`가 `:id`로 매칭된다.
- 응답 200: `{ "device_token_id": "…", "device_name": "hopegiver-mbp", "expires_at": "…", "renew_after": "…", "renewable": true, "user": { "email": "…@malgnsoft.com" } }`
  - `renewable`은 desktop 발급분이면서 `replaced_by_id IS NULL`인지를 뜻한다. `email`은 본인 값이므로 노출 경계가 넓어지지 않는다.
- 401은 위와 같다. 앱은 연결 확인과 오프라인/폐기 구분에 이 엔드포인트를 쓴다.

### 5.4 `DELETE /api/devices/self` (신설)
- 인증: 기기 토큰(5.3과 같은 `use`). 유예가 지난 교체 토큰이면 §5.0에서 409와 함께 계열이 폐기된다. 앱은 이것도 폐기 성공으로 간주한다.
- 처리: desktop 발급분이면 `revokeDesktopFamily`(§4 D4, `device_id` 단위)로 계열 전체를 폐기한다. 그 밖(페어링·OAuth 8h)은 자기 행 하나만 폐기한다. 페어링 `device_id`는 클라이언트가 정한 값이라 계열 식별자로 쓸 수 없기 때문이다. 비-desktop이면 기존 `revokeAllForDeviceToken`(oauth refresh)도 함께 호출한다(0건이어도 무해). 감사 기록은 `device_token.revoked` + `{via:'self', revoked_count}`이다.
- 응답 200: `{ "status": "revoked", "revoked_count": 2 }`. 이미 폐기된 토큰이면 미들웨어 단계에서 401이고, 앱은 이를 성공으로 간주한다.

### 5.5 기존 엔드포인트 변경
- `POST /api/devices/pair-approve`: `expiresAt: now + DEVICE_TOKEN_MAX_TTL_SECONDS`를 넘긴다(`[S] server/api/devices.js:46-48`). 그 밖의 흐름은 그대로다.
- `GET /api/devices`: `listForUser`의 SELECT에 `expires_at, oauth_client_id`를 추가한다(`[S] server/dao/device-tokens.js:50-55`). 본인 행만 대상이고 자격증명이 아니므로 응답 경계는 그대로다(로직 강도 (b)). `[S] app/pages/keys.vue`에 "만료" 열과 "종류(데스크톱/OAuth/페어링)" 표시를 추가한다.
- `POST /api/oauth/authorize-context`: desktop client는 `{ client_name: '맑은에이전트 데스크톱', trust: 'trusted', token_lifetime_days: 365 }`를 반환한다. `authorize.vue`는 이 값이 있으면 "이 기기에 365일 유효한 토큰이 발급됩니다. 맑은에이전트 앱에서 직접 시작한 경우에만 허용하세요."를 표시한다.
- `resolveTrustedClientName`(`[S] server/lib/oauth-trusted-clients.js:7-10`)은 두 번째 env `TRUSTED_MALGN_DESKTOP_CLIENT_ID`를 인식하게 바꾸고 `{ name, policy }`를 반환한다.

---

## 6. 데이터 모델과 마이그레이션

### 6.1 `migrations/0035_device_tokens_renewal_and_expiry_cap.sql`
```sql
-- Migration number: 0035
-- 기기 토큰 장기화(설계 정본: malgn-vscode docs/design/device-token-auth.md §4 D3·D4·D10)
-- 1) 갱신 계열 추적 — 부모가 어느 자식으로 갱신됐는지(재갱신 = 탈취 신호 판정용).
ALTER TABLE device_tokens ADD COLUMN replaced_by_id TEXT;
-- 2) 무기한(NULL) 금지 — 레거시 pair-approve 발급분에 created_at+365일을 채운다.
--    toISOString()과 같은 형식(…T…Z, 밀리초)을 유지해 JS Date 파싱·문자열 비교가 일관되게 한다.
UPDATE device_tokens
   SET expires_at = strftime('%Y-%m-%dT%H:%M:%fZ', created_at, '+365 days')
 WHERE expires_at IS NULL;
```
- 인덱스는 새로 만들지 않는다. `replaced_by_id`는 해시(UNIQUE)나 PK로 조회한 행에서 읽기만 하고, 그 값으로 검색하지 않는다. 계열 폐기(§4 D4)는 `user_id` 인덱스로 사용자 행을 좁힌 뒤 `device_id`로 거른다. 사용자당 기기 토큰은 수십 행 이하라 충분하다.
- **적용 전후 검사 쿼리(m8, devops 런북에 넣는다)**:
  ```sql
  -- 적용 전: 365일이 넘은 무기한 행(백필 즉시 만료 대상)이 있는지
  SELECT COUNT(*), MIN(created_at) FROM device_tokens WHERE expires_at IS NULL AND status = 'active';
  -- 적용 후: 0이어야 한다. strftime 파싱 실패로 NULL이 남으면 D3③이 그 행을 401로 막는다
  SELECT COUNT(*) FROM device_tokens WHERE expires_at IS NULL;
  ```
  적용 후 값이 0이 아니면 해당 행의 `created_at` 형식을 확인하고 후속 마이그레이션으로 채운다. 보안상으로는 fail-closed라 위험이 없고 운영 장애만 생긴다.
- FK·CHECK 제약은 ALTER로 추가할 수 없어 넣지 않는다. 대신 DAO 단언으로 보완한다(D3②). 테이블 재생성은 미결 U7이다.
- forward-only 원칙을 따른다(`[S] migrations/README.md`). 되돌려야 하면 새 마이그레이션을 쓴다. 다만 채운 `expires_at`은 다시 NULL로 되돌리지 않는다(결정 1).
- **배포 순서 제약**: 0035를 먼저 적용하고 D3③ 코드를 그 뒤에 배포한다. 순서가 바뀌면 레거시 토큰이 즉시 401을 받는다. 다만 fail-closed라 보안 위험은 없고 기능 장애만 생긴다.
- `docs/schema.sql`(서버 로컬 문서, gitignored)의 `device_tokens` 정의에도 `replaced_by_id`를 반영한다.

### 6.2 기존 `device_tokens` 사용처 영향

| 사용처 | 영향 |
|---|---|
| `/mcp` (Claude Code, OAuth 발급분 8h) | 없다. 이미 expires_at이 있다. |
| `POST /api/sessions` (사용량 수집 에이전트, 페어링 발급분) | 0035 이후 만료일이 생긴다(created+365d). 만료되면 401이 나고 재페어링이 필요하다. 수집 에이전트는 평문 파일 저장을 그대로 유지한다. 이번 범위 밖이고 미결 U4다. 단 플러그인 릴리스 후 8주 안에 페어링 발급분의 `/mcp` 접근을 막는다(§17-6). |
| 웹 `/keys` 목록과 폐기 | 열이 추가된다. 폐기 동작은 그대로다. |
| 관리자 비활성화 캐스케이드 | 그대로다(`status='active'` 행 전체가 대상이므로 갱신 체인도 포함된다). |
| OAuth refresh 경로 `rotateToken` | DAO 단언(≤365d)을 추가한다. 8h라서 항상 통과한다. |

---

## 7. 앱 설계 (`[A] src-tauri`)

### 7.1 모듈
- `src/main.rs`: `tauri_app_lib::run()`을 호출하기 전에 `if let Some(code) = tauri_app_lib::cli_mode::dispatch() { std::process::exit(code) }`를 둔다. **`argv[1] == "--hub-auth-header"`와 정확히 일치할 때만** 진입한다. 다른 인자는 전부 기존 GUI 경로로 간다.
- `src/cli_mode.rs`(신설): CLI 진입부다. Tauri Builder, updater, 스레드 스폰을 호출하지 않는다.
- `src/hub_auth/`(신설)

| 파일 | 역할 |
|---|---|
| `mod.rs` | Tauri 커맨드 `hub_auth_status`, `hub_auth_connect`, `hub_auth_disconnect`, `hub_auth_renew_now` |
| `store.rs` | `trait TokenStore { get/set/delete }`, keyring 구현, 테스트용 메모리 구현. JSON 스키마 v1(§4 D5) |
| `connect.rs` | hub PKCE 루프백 흐름. `loopback_oauth.rs`를 공용으로 쓴다 |
| `renew.rs` | 판정 함수 `needs_renew(now, renew_after)`, 스케줄러(시작 시 + 6h), 프로세스 간 락 |
| `api.rs` | reqwest(rustls) 클라이언트. 타임아웃은 연결 5s, 전체 15s |
| `locator.rs` | `cli-path.json` 원자 기록 |
| `consts.rs` | `HUB_ORIGIN = "https://malgnai-hub.apiserver.kr"`, `DESKTOP_CLIENT_ID`(공개값, 비밀 아님 — 플러그인 client_id도 공개 저장소에 있음, `[P] plugin.json:33`). debug 빌드에서만 `MALGN_HUB_ORIGIN` 오버라이드를 허용(`cfg(debug_assertions)`) |

- **토큰 원문은 Rust 경계를 넘지 않는다(계약, 보안 검토 M4).** 앱 WebView는 원격 데이터(세션 로그, 마크다운 등)를 렌더하므로 XSS 하나로 `invoke`를 부를 수 있다. 커맨드가 토큰을 반환하면 키체인 보호의 의미가 사라진다.
  - Tauri 커맨드 4개의 반환은 메타데이터만이다: `HubAuthStatus { connected, state, device_name, expires_at, renew_after }`(`state`는 §7.2 상태 이름). 토큰 필드를 가진 타입을 `Serialize`하지 않는다. 키체인 JSON(§4 D5)을 담는 내부 타입 `StoredToken`에는 `Serialize`를 프론트용으로 derive하지 않고, `Debug`는 수동 구현해 `token`을 `mhd_***`로 출력한다.
  - code 교환, renew, self, DELETE self는 모두 Rust(`api.rs`)에서 한다. 프론트엔드(`src/hubAuthApi.ts`)의 타입에는 토큰 필드가 없다.
  - 오류: 커맨드의 `Err(String)`과 로그에 토큰을 넣지 않는다. reqwest 오류는 그대로 `to_string()`하지 않고 `without_url()`을 거쳐 자체 오류 enum(`Network`, `Http(status, code)`, `Keychain`, `Corrupt`)으로 바꾼 뒤 그 enum만 문자열화한다. 요청 헤더를 Debug로 찍는 로그를 두지 않는다.
  - 테스트(§14.2): 모든 커맨드 반환 타입과 오류 enum 각 변형의 직렬화·`Display`·`Debug` 문자열에 `mhd_` 뒤에 영숫자가 이어지는 패턴이 없는지 단언한다(토큰을 넣은 store로 실행).
- `src/loopback_oauth.rs`(신설): `google_oauth/mod.rs:40-235`의 범용 부분을 추출한 것이다. google_oauth는 이 파일을 호출하도록 바꾸고, 동작은 그대로 유지한다.
- `src/lib.rs`: `mod` 선언 추가, `invoke_handler`에 커맨드 4개 추가(`:91-156`). `setup`에서 `locator::write()`를 호출하고 `hub_auth::renew::spawn_scheduler()`를 띄운다(`:60-90`).
- `capabilities/default.json`: **변경 없음.** 커스텀 커맨드만 추가하고 fs/shell 플러그인 권한을 늘리지 않는다.
- `Cargo.toml`: `keyring` 3.x에 feature 2개(§4 D5).
- 프론트엔드: `src/hubAuthApi.ts`(신설). `src/views/settings.ts`의 MCP 행(`:1118` 이하)과 `src/views/home.ts`의 hub 위젯(`:333-360`)에 "허브 연결 상태(연결됨/만료일/기기명), 연결, 해제"를 추가한다. hub 서버에 대해서는 기존 `mcp_login`(`claude mcp login` 터미널, `[A] mcp_manager/mod.rs:369-385`) 버튼을 숨긴다(OAuth 경로를 쓰지 않으므로).

### 7.2 상태 머신 (GUI)

| 상태 | 진입 조건 | UI | 전이 |
|---|---|---|---|
| NOT_CONNECTED | 키체인 항목 없음 | [허브 연결] | connect 성공 → CONNECTED |
| CONNECTED | 항목 있음, `self` 200 | 연결됨, 만료일 표시 | now≥renew_after → RENEW_DUE. 401 → (self 확인 후) REVOKED. 409 ALREADY_RENEWED → REVOKED |
| RENEW_DUE | 갱신 창 진입 | (표시 없음, 백그라운드 처리) | 성공 → CONNECTED. 네트워크 실패 → RENEW_RETRYING |
| RENEW_RETRYING | 오프라인이나 5xx | "갱신 대기 중(만료 D-n)" | 1h마다 재시도. now≥expires_at → EXPIRED |
| SAVE_FAILED | 갱신은 성공했지만 키체인 쓰기나 read-back이 실패 | "키체인 저장 실패. 24시간 안에 해결하지 않으면 다시 연결해야 합니다" | 새 토큰을 메모리에 들고 1분마다 저장을 재시도(앱 실행 중에만). 성공 → CONNECTED |
| EXPIRED / REVOKED | 만료, 401, ALREADY_RENEWED | "다시 연결하세요" + 사유 | connect → CONNECTED |
| KEYCHAIN_DENIED | 읽기 거부나 잠김 | "키체인 접근이 거부되었습니다" + [다시 시도] + 해결 안내 | 재시도 성공 → 직전 상태 |

- 재연결할 때 기존 항목이 있으면 먼저 `DELETE /self`를 best-effort로 호출한 뒤 덮어쓴다. 같은 기기에 유효한 토큰이 여러 개 남지 않게 하기 위해서다.
- **재사용 탐지를 위한 앱 규칙(M1, §5.0 전제)**:
  - 서버를 호출하기 **직전에 매번 키체인을 다시 읽는다.** 토큰을 메모리에 캐시하지 않는다. 예외는 SAVE_FAILED 상태 하나다(저장하지 못한 새 토큰을 메모리에만 들고 있음).
  - 401을 받으면 키체인을 지우기 **전에** 저장된 토큰으로 `GET /self`를 한 번 호출한다. 그 토큰이 탈취자에게 먼저 갱신당한 부모라면 서버가 409 + 계열 폐기로 답하고, 탈취자의 자식도 함께 죽는다. self도 401이면 그때 키체인을 지우고 REVOKED로 간다. 앱은 시작할 때와 6시간마다 self를 부르므로 꺼져 있던 앱이 다시 켜지는 시점에 탐지가 자연히 일어난다.
  - ALREADY_RENEWED 안내 문구에 "반복되면 관리자에게 알리세요"를 넣는다(m10). "이 계정의 다른 데스크톱 연결 n개" 표시(U11 (b)안)는 선택 사항으로 두고 이번 범위에 넣지 않는다(§20).
- **갱신 단일화**: 단일 인스턴스 플러그인이 없어서 앱 2개가 동시에 뜰 수 있다. 동시 갱신은 ALREADY_RENEWED로 계열 전체를 폐기시키는 자기파괴를 낳으므로 프로세스 간 락이 필수다.
  - 락 파일 `<app_config_dir>/hub-renew.lock`을 `create_new`로 만든다. 내용은 pid와 시각이다. 120초가 지나면 stale로 보고 회수한다.
  - 락을 잡은 뒤 키체인을 다시 읽고, `renew_after`가 이미 미래이면 건너뛴다.
- **CLI 모드는 절대 갱신하지 않는다.** 여러 Claude 세션이 동시에 helper를 실행하면 같은 경합이 생기고, 10초 예산 안에 네트워크 왕복을 넣지 않기 위해서다. 그 대가로, 만료 30일 전부터 만료일까지 앱을 한 번도 켜지 않으면 만료되어 재연결해야 한다. CLI는 갱신 창에 들어가면 stderr로 안내한다.

### 7.3 CLI 모드 인터페이스 (계약)
- 호출: `<exe> --hub-auth-header` (인자 없음, stdin 무시)
- **대상 서버(audience) 확인 — 키체인을 읽기 전에 한다(보안 검토 M3)**
  - 위협: 악성 저장소의 `.mcp.json`, 서드파티 플러그인, 복사·붙여넣기 실수로 다른 MCP 서버의 `headersHelper`에 이 CLI가 지정되면, Claude Code가 365일 hub 토큰을 그 서버의 `Authorization`에 실어 보낸다. "회사 공식 앱의 정상 플래그"로 보여 승인 대화상자에서 의심하기 어렵다.
  - 규칙: 환경변수 `CLAUDE_CODE_MCP_SERVER_URL`이 **설정되어 있으면** `url::Url::parse` 결과가 다음을 모두 만족할 때만 진행한다. 하나라도 어긋나면 stdout을 비우고 종료코드 **14 `WRONG_AUDIENCE`**로 끝낸다.
    - `origin().ascii_serialization() == HUB_ORIGIN`(scheme·host·port 정규화 비교. 호스트 대소문자와 기본 포트 `:443` 표기는 같은 서버로 본다. 다른 포트는 거부)
    - `path() == "/mcp"`(정확일치. `/mcp/`, `/MCP`, `/mcp/x` 거부)
    - query와 fragment가 없음
    - 빈 문자열, 파싱 실패, `REDACTED`가 섞인 값도 위 비교에서 떨어지므로 14가 된다(fail-closed).
  - 변수가 **없으면** 허용한다. 사람이 터미널에서 직접 실행하는 경우(실측 E3)다. Claude Code가 helper로 실행하면 이 변수는 항상 설정되므로(§2, PM 확인), 설정 파일만으로는 이 확인을 우회할 수 없다.
  - `HUB_ORIGIN`은 `consts.rs`의 값이다. debug 빌드에서 `MALGN_HUB_ORIGIN` 오버라이드를 쓰면 그 값과 비교한다(§7.1, release에서는 오버라이드 불가).
  - `CLAUDE_CODE_MCP_SERVER_NAME`은 검사하지 않는다. 서버 이름은 설정 파일 작성자가 마음대로 정할 수 있어(`malgnai-hub`로 지어도 된다) 신뢰 근거가 되지 못한다.
  - 한계: 이미 같은 사용자 권한으로 코드를 실행할 수 있는 공격자는 환경변수를 직접 설정해 우회할 수 있다. 그 경우는 §12 R2(수용한 잔여 위험)에 해당한다. 이 통제는 **설정 파일만으로 토큰이 유출되는 경로**를 닫는 것이 목적이다.
- stdout: 성공했을 때만 정확히 한 줄 `{"Authorization":"Bearer mhd_…"}\n`을 쓴다. 실패하면 stdout에 **아무것도 쓰지 않는다.**
- stderr: 실패하거나 경고할 때 `malgn-agent: <CODE>: <한국어 안내>` 한 줄을 쓴다. **토큰과 그 일부를 절대 포함하지 않는다.**
- 종료코드

| 코드 | CODE | 조건 | 안내 문구 |
|---|---|---|---|
| 0 | OK | 토큰 출력. 갱신 창이면 stderr에 경고만 덧붙임 | (경고) "맑은에이전트 앱을 실행하면 허브 토큰이 자동 갱신됩니다(만료 D-n)." |
| 10 | NOT_CONNECTED | 항목 없음 | "맑은에이전트 앱에서 허브에 연결하세요." |
| 11 | EXPIRED | 로컬 `expires_at` ≤ now | "허브 토큰이 만료되었습니다. 맑은에이전트 앱에서 다시 연결하세요." |
| 12 | KEYCHAIN_ERROR | 접근 거부, 잠김, 백엔드 오류, 데드라인(5s) 초과 | "키체인에서 토큰을 읽지 못했습니다. 맑은에이전트 앱을 한 번 실행해 접근을 허용하세요." |
| 13 | CORRUPT | JSON 파싱 실패, `hub`가 기대 origin과 다름 | "저장된 허브 연결 정보가 손상되었습니다. 앱에서 다시 연결하세요." |
| 14 | WRONG_AUDIENCE | `CLAUDE_CODE_MCP_SERVER_URL`이 있고 허브 `/mcp`가 아님. 키체인을 읽지 않고 끝남 | "허브가 아닌 서버에는 토큰을 제공하지 않습니다. MCP 설정의 headersHelper를 확인하세요." |
| 1 | INTERNAL | 그 밖의 오류 | "맑은에이전트 CLI 내부 오류" |

- GUI 미기동 보장:
  - `main()` 맨 앞에서 분기하므로 `tauri::Builder`, 창, WebView, updater, 자율업무 스케줄러가 전혀 생성되지 않는다.
  - macOS에서는 번들 내부 바이너리를 직접 실행하고 NSApplication을 초기화하지 않으므로 Dock 아이콘이 생기지 않는다(실측 E3).
- 10초 보장:
  - 네트워크와 파일 쓰기가 없다.
  - audience 확인(위)은 환경변수와 문자열 비교뿐이라 시간 예산에 영향이 없다.
  - 키체인 읽기는 별도 스레드에서 실행하고 `recv_timeout(5s)`로 기다린다. 초과하면 12로 끝낸다. 블로킹된 스레드는 `process::exit`로 정리된다.
  - 래퍼는 8초에 강제 종료한다. Claude Code 한도 10초 안에서 두 겹으로 막는 셈이다.
- Windows: release 빌드는 GUI 서브시스템이다. 부모(node `spawn`)가 넘긴 파이프 핸들을 `GetStdHandle`로 받아 쓰므로 stdout이 동작할 것으로 예상하지만 **실측이 필요하다(W2).** 실패하면 D6의 조건부 대안으로 간다.

---

## 8. 플러그인 설계 (`[P] malgn-agent`)

### 8.1 `.claude-plugin/plugin.json` 변경
```jsonc
"mcpServers": {
  "malgnai-hub": {
    "type": "http",
    "url": "https://malgnai-hub.apiserver.kr/mcp",
    "headersHelper": "node \"${CLAUDE_PLUGIN_ROOT}/bin/hub-auth-header.mjs\""
    // "oauth" 블록 제거
  }
}
```
- `userConfig.device_token`(레거시 탈출구 필드, `:19-27`)은 **제거한다.** 이미 쓰지 않는 필드이고(설명문 자체가 "자동으로 쓰이지 않는다"), helper는 user_config를 받을 수도 없다. 대가로 플러그인을 재설치할 때 이 값을 묻지 않게 된다. 단, 실측 E1이 통과하기 전에는 plugin.json을 바꾸지 않는다.
- 서버 이름 `malgnai-hub`는 유지한다. 도구 접두사가 바뀌지 않는다.

### 8.2 `bin/hub-auth-header.mjs` (신설) 계약
- 의존성은 Node 내장 모듈(`child_process`, `fs`, `os`, `path`)만 쓴다.
- 절차:
  0. **audience 확인(M3)**: §7.3과 같은 규칙으로 `CLAUDE_CODE_MCP_SERVER_URL`을 검사한다(`new URL()`로 파싱, origin이 `https://malgnai-hub.apiserver.kr`, pathname이 `/mcp`, search·hash 없음). 어긋나면 앱을 실행하지 않고 "malgn-agent: WRONG_AUDIENCE: 허브가 아닌 서버에는 토큰을 제공하지 않습니다."를 stderr에 쓰고 exit 1로 끝낸다. 래퍼의 확인은 조기 차단과 명확한 안내용이고, **필수 방어선은 CLI(Rust)의 확인**이다. CLI는 래퍼 없이 직접 headersHelper로 지정될 수 있기 때문이다. 환경변수는 spawn 시 그대로 상속되므로 CLI가 다시 확인한다.
  1. 실행파일을 찾는다(§4 D7).
  2. `spawn(exe, ['--hub-auth-header'], { stdio: ['ignore','pipe','pipe'], windowsHide: true, timeout: 8000 })`로 실행한다. **셸을 거치지 않고** 인자를 배열로 넘긴다. stdout은 **4KB 상한**까지만 읽는다(넘으면 실패로 처리, m5).
  3. exit 0이면 stdout을 파싱한다. 키가 정확히 `Authorization` 하나이고, 값이 `/^Bearer mhd_[A-Za-z0-9_-]{43}$/`와 일치하면 그대로 stdout에 쓰고 exit 0으로 끝낸다.
  4. 그 밖의 경우: 앱의 stderr 첫 줄을 전달하거나, 앱을 찾지 못했으면 "맑은에이전트 앱이 설치되어 있지 않습니다. 앱을 설치하고 허브에 연결하세요."를 stderr에 쓰고 exit 1로 끝낸다. 이때 stdout은 비워 둔다. 앱 stderr를 전달할 때는 `mhd_[A-Za-z0-9_-]+`를 `mhd_***`로 치환한다(CLI 계약 위반 버그에 대비한 이중 방어, m5). 종료코드별 안내 매핑은 §7.3 표를 따른다(14 포함).
- stderr 안내가 사용자에게 어디서 보이는지는 미결이다(§4 D8 "안내 문구의 노출 위치", U12).
- `{}`를 출력하는 방식은 D8에서 채택하지 않았다. 실측 E12 결과에 따라 다시 검토한다.

### 8.3 headersHelper 실측이 실패할 때의 대안 (우선순위순)
- **A.** helper가 실행되지 않거나 플러그인 필드가 무시되는 경우: 앱이 `claude mcp add-json --scope user malgnai-hub '{"type":"http","url":…,"headersHelper":"<앱 절대경로> --hub-auth-header"}'`로 사용자 범위 서버를 등록하고, 플러그인은 `mcpServers`를 제거한다. 대가로 도구 접두사가 `mcp__malgnai-hub__`로 바뀐다. 그러면 `stop-mcp-reminder.cjs:47`, 에이전트와 스킬 본문의 접두사 표기를 전수 치환해야 한다(별도 Refactor 작업).
- **B.** helper는 실행되지만 Windows 셸 인용이 깨지는 경우: 기존 훅과 같은 문자열이 Windows에서 동작하는지(`hooks.json:8`)를 먼저 확인하고, 필요하면 인용 형태를 조정한다.
- **C.** 둘 다 불가능한 경우: 현행 OAuth를 유지하고 이 설계를 보류한다. 서버 쪽 변경(D3, D10)은 그 자체로 독립된 가치(무기한 금지)가 있으므로 그대로 배포한다.

---

## 9. 저장소별 변경 파일 목록

### [S] 서버 (malgnai-public)

| 파일 | 변경 |
|---|---|
| `migrations/0035_device_tokens_renewal_and_expiry_cap.sql` | 신설(§6.1) |
| `server/lib/tokens.js` | 상수 4개 신설, `generateDeviceToken()`(접두사 `mhd_`) |
| `server/dao/device-tokens.js` | `insert`/`rotateToken`에 만료 단언(누락·NaN·상한 초과). `buildRenewStatements()`(batch 2문장), `findAnyByHash()`(status·만료 무관 조회), `revokeDesktopFamily()`(`device_id` 단위 한 문장, changes 반환), `listForUser` 컬럼 추가, `findById` 그대로. 초안의 `revokeChain()`은 만들지 않는다 |
| `mcp/device-auth.js` | 만료 판정을 `Number.isFinite(Date.parse(..))` 기반으로 교체(NULL·NaN·과거 거부), identity에 `deviceTokenId` 추가 |
| `server/middleware/device-token-auth.js` | 신설: `requireDeviceToken`(sessions.js에서 추출), `detectReplacedTokenReuse({ graceAware })`(§5.0) |
| `server/lib/oauth-redirect-uri.js` 또는 `oauth.js` 호출부 | desktop client는 redirect host `127.0.0.1` 정확일치(m3) |
| `server/api/sessions.js` | 추출한 미들웨어를 import(동작은 그대로) |
| `server/middleware/jwt-auth.js` | `PUBLIC_PATHS`에 `/api/devices/renew`, `/api/devices/self` 추가. 사유 주석은 기존 sessions 항목과 같은 톤 |
| `server/api/devices.js` | `use('/renew')`, `use('/self')`, renew와 self의 GET/DELETE 추가(`/:id`보다 앞에 등록). pair-approve에 expiresAt |
| `server/api/oauth.js` | `handleAuthorizationCodeGrant` desktop 분기, authorize-context에 `token_lifetime_days` |
| `server/lib/oauth-trusted-clients.js` | desktop client 인식 |
| `app/pages/oauth/authorize.vue`, `app/pages/keys.vue` | 365일 안내, 만료와 종류 열 |
| `wrangler.jsonc` | 변경 없음. `TRUSTED_MALGN_DESKTOP_CLIENT_ID`는 기존 `TRUSTED_MALGN_AGENT_CLIENT_ID`와 같은 방식(대시보드 var/secret)으로 등록한다. 등록 방식은 devops 단계에서 실물로 확인한다(U6) |
| 테스트 | §14 |

### [A] 앱 (malgn-vscode)
§7.1의 파일들. 추가로 `docs/WINDOWS-CHECKLIST.md`에 §16 반영(구현할 때), `README.md`의 기능 목록.

### [P] 플러그인 (claude-plugins)

| 파일 | 변경 |
|---|---|
| `malgn-agent/.claude-plugin/plugin.json` | §8.1. 버전은 릴리스할 때 올린다 |
| `malgn-agent/bin/hub-auth-header.mjs` | 신설 |
| `CLAUDE.md`(:146 plugin.json 설명), `malgn-agent/README.md`, `CHANGELOG.md` | OAuth 로그인 안내를 "앱에서 허브 연결"로 교체 |
| 안내 문구 전수 | 구현할 때 `grep -rn "OAuth\|mcp login\|device_token" malgn-agent/{skills,knowledge,agents}`로 사용자 안내 문구를 찾아 갱신한다(`usage-agent-healthcheck` 스킬은 수집 에이전트 축이라 제외) |

---

## 10. 비정상 케이스

| 상황 | 동작 | 위치 |
|---|---|---|
| 루프백 콜백이 오지 않음(사용자가 창을 닫음) | 기존 Google 흐름과 같은 데드라인으로 취소. "연결 취소됨" 표시 | `loopback_oauth.rs` |
| state 불일치 | 거부하고 실패 페이지 표시 | 같음 |
| token 교환 네트워크 실패 | code는 1회용이고 짧게 만료되므로 재시도 대신 흐름을 처음부터 다시 시작 | `connect.rs` |
| 발급 후 키체인 저장 실패 | 받은 토큰으로 즉시 `DELETE /self`(best-effort)를 호출하고 실패를 표시. 고아 토큰을 남기지 않는다 | `connect.rs` |
| 갱신 중 네트워크 단절 | 기존 토큰을 유지하고 1h 후 재시도. 서버가 커밋했는데 응답만 유실된 경우, 다음 시도는 ALREADY_RENEWED가 되어 계열이 폐기되고 재연결해야 한다(드문 경우로 수용, 보안 검토 m10 동의). 재사용 판정이 유예와 무관해졌으므로(§5.0) 앱이 24h 넘게 꺼져 있다 켜져도 결과는 같다(self 409 → 재연결) | `renew.rs`, §12 R8 |
| 탈취자가 먼저 갱신, 정상 앱은 24h 넘게 꺼져 있음 | 앱이 켜지면 키체인의 부모 토큰으로 self 호출 → 유예 경과 교체 토큰 → 409 + `device_id` 계열 폐기(탈취 자식 포함) → 앱은 재연결 안내 | §5.0, §7.2 |
| 다른 MCP 서버 설정이 CLI를 headersHelper로 지정 | CLI가 `CLAUDE_CODE_MCP_SERVER_URL` 불일치로 14, 키체인을 읽지 않음. 래퍼도 사전 차단 | §7.3, §8.2 |
| 저장된 `expires_at`이 파싱 불가 | 검증 미들웨어가 401(fail-closed). DAO는 그런 값을 쓰지 못함 | §4 D3 |
| 갱신 응답 후 키체인 쓰기 실패 | SAVE_FAILED 상태, 메모리 보관과 1분 재시도. 직전 토큰은 24h 유효 | §7.2 |
| 앱 동시 실행 | 프로세스 간 락과 락 획득 후 재확인 | §7.2 |
| 오프라인 상태에서 Claude 세션 시작 | CLI는 네트워크가 필요 없어 헤더를 정상 출력한다. MCP 연결 자체는 네트워크 문제로 실패 | — |
| 기기 시계 오차 | 판정은 서버 시계로 한다. 앱의 `renew_after` 판정만 로컬 시계를 쓰고, 오차가 있어도 TOO_EARLY 응답으로 교정된다 | — |
| 키체인 잠김이나 접근 거부(GUI) | KEYCHAIN_DENIED 상태 | §7.2 |
| 키체인 프롬프트(CLI, macOS) | 5s 데드라인 → 12 → 래퍼 안내 | §7.3, §12 R3 |
| D1 장애 중 `/mcp` | 현행 동작을 유지한다(이번 범위 밖). 이 설계로 새로 생기는 D1 의존은 renew와 self뿐이고 둘 다 5xx면 재시도한다 | — |
| 계정 비활성화 | JOIN 방어와 캐스케이드. 앱은 다음 self/renew에서 401 → REVOKED | `[S] device-tokens.js:34-40,71-76` |
| 레이트리밋 바인딩 오류 | 기존 정책대로 fail-open(oauth). 갱신은 토큰 인증이 선행하므로 영향이 작다 | `[S] oauth.js:149-159` |

---

## 11. email_send 수신자 제한: 기기 토큰 경로에서도 동일하게 적용됨

- 허용 목록은 **코드 상수 하나**이고 사내 도메인뿐이다: `const ALLOWED_RECIPIENT_DOMAINS = Object.freeze(new Set(['malgnsoft.com']))` (`[S] server/lib/email.js:34`). 환경변수가 아니라서 배포 없이 넓힐 수 없다(`:30-33` 주석).
- 비교 방식은 도메인을 소문자로 바꾼 뒤 `Set.has` 완전일치다. 서브도메인과 접미사 위장을 차단한다(`:189-197`). 위반이 하나라도 있으면 전체를 거부한다(`:200-214`).
- 호출 경로: MCP 도구 `email_send`는 `this.props.userId`/`deviceId`를 채워 `sendEmail()`을 호출한다(`[S] mcp/agent.js:515-529`). `this.props`는 `/mcp` 라우트에서 `deviceAuthMiddleware`가 주입한 identity다(`[S] server/index.js:202-214`). `sendEmail()` 안에서 `resolveRecipients()`가 무조건 실행된다(`[S] server/lib/email.js:309`).
- 결론: `deviceAuthMiddleware`는 토큰이 OAuth로 발급됐는지, 페어링으로 발급됐는지, desktop으로 발급됐는지 구분하지 않는다. 발급 경로를 가리지 않는 하나의 경로이므로 **기기 토큰으로 호출해도 수신자 제한, 레이트리밋(5/60s, `:100`; 일 50건, `:42,319`), 사용자 활성 재확인(`:285`)이 모두 똑같이 적용된다.** 이번 설계에서 email.js는 바꾸지 않는다.
- 남는 위험은 탈취된 365일 토큰으로 사내 수신자에게 일 50건까지 보낼 수 있다는 점이다(§12 R6).

---

## 12. 위협 모델과 잔여 위험

| # | 위협 | 대응 | 잔여 |
|---|---|---|---|
| R1 | 디스크, 백업, 동기화 폴더에서 토큰 유출 | **이 설계가 만드는 desktop 토큰은** OS 키체인(macOS Keychain 암호화, Windows DPAPI)에만 저장하고 평문 파일을 만들지 않는다. 서버는 해시만 저장한다. 0035가 레거시 무기한 토큰을 365일로 줄인다 | **PC 전체로는 평문 토큰이 남아 있다**: 사용량 수집 에이전트를 설치한 PC의 `~/.claude/malgnai-hub/usage-agent-credentials.json`에 페어링 발급 기기 토큰이 평문으로 있다(`[P] malgn-agent/bin/usage-agent-lib.mjs:69-90`, §2). 이 토큰은 0035 이후 최대 365일 유효하고, `scopes`를 강제하지 않으므로(`[S] mcp/device-auth.js:23`) `/mcp` 전체와 `email_send`를 쓸 수 있다. 백업·동기화 폴더 유출 시나리오가 이 파일에는 그대로 성립한다. 이번 설계가 새로 만든 노출은 아니며 F3(이 파일을 정식 경로로 승격)는 금지했다. **기한을 둔 조치**: 플러그인 릴리스 후 8주 안에 페어링 발급분(`oauth_client_id IS NULL`)의 `/mcp` 접근을 거부해 이 파일의 토큰을 `/api/sessions` 전용으로 낮춘다(U4, §17-6) |
| R2 | 같은 사용자 권한의 악성 프로세스 | 별도 통제 없음(구조상 불가). 대신 **탐지와 짧은 폐기 경로**로 대응한다: §5.0 재사용 탐지(탈취자가 갱신하면 정상 앱의 다음 self/renew에서 계열 폐기), 웹 `/keys`의 last_used_at과 종류 열, 앱 [연결 해제]와 웹 폐기, 계정 비활성화 캐스케이드. 호출 부모 프로세스 검사는 우회가 쉽고 오탐이 많아 도입하지 않는다(§4 D6) | **macOS와 Windows 모두, 같은 사용자 권한의 코드는 CLI(`malgn-agent --hub-auth-header`)를 실행해 프롬프트 없이 토큰을 얻을 수 있다.** macOS 키체인 ACL은 그 바이너리를 신뢰하고, 그 바이너리는 호출자를 가리지 않기 때문이다. 키체인 ACL이 실제로 보호하는 것은 저장 데이터(백업, 디스크, 다른 사용자 계정)뿐이다. Windows CredMan은 앱별 ACL도 없다. 수용한다. 같은 권한의 악성코드는 Claude 세션 자체를 조종할 수도 있어, headersHelper 구조를 택한 이상 피할 수 없는 성질이다 |
| R3 | macOS ad-hoc 서명이라 업데이트하면 cdhash가 바뀌고 키체인 ACL이 불일치 | 업데이트 뒤 GUI가 처음 시작할 때 "키체인 접근 확인"을 미리 실행한다. 사용자에게 사유를 먼저 보여 주고, 프롬프트에서 "항상 허용"을 누르도록 안내한다. CLI는 5s 데드라인을 넘기면 12로 끝내고 안내한다 | 업데이트 직후 앱을 켜지 않은 채 Claude만 쓰면 hub 연결이 실패할 수 있다. 실측 E9로 확정한다. 근본 해결책은 Developer ID 서명이다(미결 U3) |
| R4 | 로그나 출력에 토큰 노출 | CLI는 stdout 한 줄만 쓰고 stderr에는 토큰을 쓰지 않는다. 래퍼는 stdout을 그대로 전달하기만 하고 로그를 남기지 않으며, 앱 stderr를 전달할 때 `mhd_…`를 마스킹한다(m5). 앱 GUI 쪽은 토큰 원문이 Rust 경계를 넘지 않는다(§7.1 M4 계약, WebView XSS로 `invoke`해도 토큰을 얻을 수 없음). `mhd_` 접두사로 마스킹 대상을 쉽게 식별한다 | Claude Code 내부 디버그 로그에서의 헤더 취급은 우리가 통제할 수 없다 |
| R4b | 다른 MCP 서버로 hub 토큰 전송(오디언스 혼동) | CLI가 `CLAUDE_CODE_MCP_SERVER_URL`을 허브 `/mcp`와 대조하고 불일치면 14(§7.3). 래퍼도 사전 차단(§8.2) | 같은 사용자 권한으로 환경변수를 직접 조작하는 경우는 R2에 포함 |
| R5 | desktop client_id 사칭(로컬 악성 앱이 동의를 유도) | PKCE와 루프백, 사용자의 명시적 동의, 동의 화면 경고 문구(§5.5), 감사 기록 `device_token.issued` | 사용자가 무심코 허용할 수 있다. 수용한다(현행 OAuth 90d refresh와 같은 수준) |
| R6 | 탈취한 토큰으로 MCP나 email_send 악용 | 수신자를 사내로 제한(§11), 레이트리밋, 즉시 폐기(웹/앱), 계정 비활성화 캐스케이드 | 폐기하기 전까지 최대 365일 동안 유효하다. 탐지는 갱신이 끼어든 경우 §5.0이 자동으로, 그 밖에는 사람이 last_used_at을 보는 수준이다 |
| R7 | locator 파일이나 `MALGN_AGENT_EXE` 조작으로 다른 실행파일 실행 | 근거: **같은 사용자 권한이면 CLI를 직접 호출해 토큰을 얻는 편이 더 쉬우므로** 새 공격 표면이 아니다(R2). 절대경로·파일명 검사는 오설정 방지용이지 보안 통제가 아니다. 다중 사용자 PC 대비 Minor 방어로, 다른 사용자가 쓸 수 있는 후보 경로는 거부하고(POSIX 권한 검사), `MALGN_AGENT_EXE` 사용 시 매번 stderr 경고, 일시 경로는 locator에 기록하지 않는다(§4 D7, m4) | 같은 사용자 권한 공격은 R2와 같다 |
| R8 | 갱신으로 탈취 상태를 지속 | 갱신 창 30일, 토큰당 1회. **교체된 토큰의 재제시를 만료·유예와 무관하게 재사용으로 판정**(renew는 항상, self는 유예 경과 시)하고 `device_id` 계열 전체를 폐기한다(§5.0, M1). 앱은 서버 호출마다 키체인을 다시 읽고, 401이면 지우기 전에 self로 한 번 확인한다(§7.2) | 탈취자가 먼저 갱신해도, 정상 앱이 다음에 켜져 self나 renew를 부르는 순간 탈취 자식까지 폐기된다. 남는 경우는 **정상 앱이 다시는 켜지지 않는 PC**(퇴사·폐기 PC 등)뿐이다. 이 경우는 계정 비활성화 캐스케이드와 웹 `/keys` 폐기로 처리한다. 초안의 U11은 해소로 닫았다 |
| R9 | 무기한 토큰 | D3의 세 겹(NaN 거부 포함, m1)과 0035 백필 | 없음 |
| R10 | 클라이언트가 TTL이나 정책을 주장 | 정책은 서버에 저장된 client_id와 서버 상수로만 정한다. body의 TTL은 무시한다 | 없음 |
| R11 | PUBLIC_PATHS가 메서드를 무시해서 생기는 우회 | 경로 정확일치와 라우터 레벨 `use()`. 회귀 테스트(§14) | 없음 |

---

## 13. 보안 구조적 결정 대조

Skill `domain-backend-api-security`의 "언제 정하는가 (a)" 항목과 하나씩 대조했다.

- 인증 게이트 위치와 PUBLIC_PATHS 경계: §5 공통(정확일치 2개 추가, `use()`로 기기 토큰 강제), §12 R11
- 인가 축: 소유자 본인만 허용(토큰 자기 자신이 대상). 관리자 폐기는 기존 JWT `DELETE /:id`를 그대로 쓴다
- 소유권 컬럼: `device_tokens.user_id`가 이미 있다. 자식은 부모의 user_id를 서버에서 복사한다(§4 D4 SQL)
- 자격증명 전달: Bearer 헤더만 쓰고 쿠키는 쓰지 않는다. 호출자가 네이티브(reqwest, Claude Code)라서 CORS를 새로 정할 것이 없다. 기존 웹 CORS 화이트리스트도 바뀌지 않는다
- 민감 데이터의 형태: 서버는 SHA-256 해시만, 클라이언트는 OS 키체인만 쓴다(§4 D2, D5)
- 응답 노출 경계: 원문 토큰은 발급과 갱신 응답에만 1회 싣는다. self는 본인 email뿐이다. `/api/devices` 목록에 추가하는 열은 자격증명이 아니다. 앱 안에서는 토큰 원문이 Rust 경계를 넘지 않고 WebView에는 메타데이터만 간다(§7.1, M4). CLI는 허브 `/mcp` 대상일 때만 토큰을 낸다(§7.3, M3)
- 로직 강도 항목(레이트 임계, 감사 metadata 세부, 입력검증 세부)은 기존 값을 재사용하는 수준으로 두었다. 강화가 필요하면 서버의 `docs/security-plan.md`에 적재한다

---

## 14. 테스트 계획

### 14.1 서버 (vitest, 기존 스타일)
- `server/dao/device-tokens.test.js` (node:sqlite 실DDL, 기존 방식)
  - `insert`에 expiresAt이 없거나, `'not-a-date'`(NaN)이거나, 365d+61s이면 throw하고, 365d면 통과한다. `rotateToken`도 같다(m1)
  - 0035를 적용하면 NULL 행만 `created_at+365d`(형식 `…T…Z`)로 바뀌고 기존 값은 그대로다. 적용 후 `new Date(expires_at)`가 유효한 날짜다
  - renew batch: 같은 부모로 순차 2회 → 두 번째는 changes 0. 부모 `expires_at = MIN(기존, grace)`. 자식이 user_id, device_id, oauth_client_id를 상속
  - `revokeDesktopFamily`(m2): 같은 `device_id`의 부모→자식→손자 3단(그리고 40단 체인 — 초안의 상한 32를 넘는 경우)이 모두 revoked. 같은 사용자의 **다른 `device_id`** desktop 행, 같은 `device_id` 문자열을 가진 **페어링 행**(`oauth_client_id IS NULL`), **다른 사용자**의 같은 `device_id` 행은 그대로 active
  - `findAnyByHash`: revoked·만료 행도 반환한다
- `mcp/device-auth.test.js`(vi.mock, 기존 방식): `expires_at:null`이면 거부(신규), `'garbage'`(NaN)이면 거부(신규, m1), 과거면 거부, 미래면 통과, identity에 `deviceTokenId`
- `server/api/oauth.test.js`
  - desktop client → `expires_in=31536000`, `refresh_token` 없음, `mhd_` 접두사, 행 `oauth_client_id=desktop`, `renew_after = expires_at-30d`
  - plugin client와 DCR client → 기존 8h+refresh(회귀)
  - env 미설정이면 desktop 분기가 일어나지 않음(fail-closed)
  - body에 `expires_in`/`ttl`을 넣어도 무시됨
- `server/api/devices.test.js`(신설)
  - renew: 창 이전 409 TOO_EARLY, 비-desktop 403, 성공 200, 부모 재사용 시 409 + 자식 폐기 + 감사 1건, 만료/폐기/비활성 계정 401, 응답에 원문 토큰 1회
  - **재사용 탐지 4종(M1, §5.0)**
    1. 부모로 renew 성공 → D1에서 부모 `expires_at`을 과거로 → 부모로 renew → **409 ALREADY_RENEWED, 자식 revoked**, 감사 1건(초안에서는 401이고 자식이 active로 남던 케이스)
    2. 부모로 renew 성공 직후(유예 안) 부모로 `GET /self` → **200**(오탐 없음)
    3. 부모 유예 경과 후 부모로 `GET /self`와 `DELETE /self` 각각 → **409, 계열(자식 포함) revoked**
    4. 만료된 부모로 `/mcp` → **401만**, 자식은 active 유지(Claude 세션이 든 옛 토큰 보호)
    - 추가: 이미 폐기된 계열의 토큰을 다시 제시 → 409, 감사 기록 추가 없음(`revoked_count=0`). 비-desktop 토큰(교체될 수 없음)은 탐지를 거치지 않고 기존 판정
  - self GET은 본인 정보. DELETE는 desktop이면 `device_id` 계열 폐기, 페어링 토큰이면 자기 행만 폐기
  - 컨텍스트 키(m7): 신설 핸들러가 `userId`를 set하지 않음. `deviceTokenId`가 없는 컨텍스트로 핸들러를 직접 부르면 500
  - desktop client의 redirect_uri가 `http://localhost:…/callback`이면 거부, `127.0.0.1`이면 통과. plugin client의 `localhost`는 기존대로 통과(m3, 회귀)
  - **경계 회귀**: JWT로 `/api/devices/self`를 호출하면 401(기기 토큰만 허용). 무인증 `GET /api/devices`는 401(JWT 게이트 유지). 무인증 `PUT /api/devices/renew`(다른 메서드)도 `use()`로 401. `DELETE /api/devices/self`가 `/:id` 핸들러로 가지 않음
- `server/index.test.js`: PUBLIC_PATHS에 정확한 문자열 2개만 추가됐는지 스냅숏으로 확인
- email: 코드를 바꾸지 않으므로 기존 `server/lib/email.test.js`를 회귀로 돌린다. `deviceAuthMiddleware`가 발급 경로와 무관하다는 점은 device-auth 테스트(oauth_client_id가 desktop인 행도 같은 identity shape)로 보증한다
- **실행 주의**: `vitest.config.js`의 `exclude: ['**/.claude/worktrees/**']`가 워크트리 안에서 실행할 때 테스트 전체를 걸러낼 수 있다. 착수할 때 `pnpm vitest list`로 수집 건수를 먼저 확인하고, 0건이면 `--exclude` 오버라이드나 임시 config로 실행한다(U8)

### 14.2 앱 (cargo test)
- `cli_mode`: `dispatch`가 정확한 플래그에서만 Some을 반환하고, `--hub-auth-header=x`, 추가 인자, 플래그 없음은 None. 메모리 store로 종료코드 0/10/11/12/13/14 매핑. stdout JSON이 정확히 한 키이고 한 줄. **stderr에 토큰 문자열이 없음**(출력 버퍼 검사). 5s 데드라인(블로킹 store를 주입하면 12)
- **audience 판정(M3)**: 판정 함수를 환경변수 읽기와 분리한 순수 함수(`audience_ok(Option<&str>, hub_origin) -> bool`)로 두고 다음을 검사한다. 14로 끝나는 경우 **store의 get이 한 번도 호출되지 않음**을 호출 카운터로 단언한다.
  - 변수 없음 → 허용
  - 정확한 `https://malgnai-hub.apiserver.kr/mcp` → 허용. `HTTPS://MALGNAI-HUB.apiserver.kr:443/mcp`(대소문자·기본 포트) → 허용
  - 다른 origin(`https://attacker.example/mcp`, `http://malgnai-hub…/mcp`, `…:8443/mcp`, `https://malgnai-hub.apiserver.kr.evil.example/mcp`) → 14
  - path 불일치(`/mcp/`, `/MCP`, `/mcp/x`, `/`) → 14. query·fragment 있음 → 14
  - 빈 문자열, 파싱 불가, `REDACTED` 포함 → 14
- **토큰 원문 비노출(M4)**: 토큰을 넣은 메모리 store로 `hub_auth_status` 등 커맨드 4개의 반환값을 `serde_json::to_string`한 결과, 그리고 오류 enum 각 변형의 `Display`/`Debug` 문자열에 `mhd_[A-Za-z0-9_-]{4,}` 패턴이 없음. `StoredToken`의 `Debug` 출력이 `mhd_***`
- `renew::needs_renew` 경계(renew_after 직전, 같음, 직후). 락: 두 번째 획득 실패, 120s stale 회수
- 응답 처리 매핑: 401 → self 확인 호출 후 항목 삭제, 409 ALREADY_RENEWED → 항목 삭제, TOO_EARLY → 일정 재설정, 5xx → 유지(HTTP 모킹은 기존 테스트 관례를 따르고 없으면 응답 파서 단위만 검증)
- 키체인 재읽기(M1): 서버 호출 함수가 매 호출마다 store의 get을 부르는지(호출 카운터), SAVE_FAILED에서만 메모리 토큰을 쓰는지
- `store`: JSON v1 직렬화 왕복, `hub` origin 불일치 → CORRUPT
- `loopback_oauth` 추출 후 기존 google_oauth 테스트 전부 통과(회귀)
- `#[ignore]` 실기 테스트: 실제 keyring 쓰기, 읽기, 삭제 왕복. mock 백엔드가 아닌지 확인(macOS/Windows 실기에서 `cargo test -- --ignored`)

### 14.3 플러그인
- `bin/hub-auth-header.mjs`: 해석 순서와 출력 검증 로직을 함수로 분리하고 `node --test`로 다음을 확인한다. 가짜 실행파일(스텁 스크립트)로 정상, 비정상 JSON, 추가 키, 종료코드 10, 종료코드 14 안내 매핑, 타임아웃, 4KB 초과 stdout. audience 불일치 URL이면 스텁이 **실행되지 않음**(M3). 스텁이 stderr에 `mhd_…`를 흘리면 전달본이 `mhd_***`(m5). `MALGN_AGENT_EXE` 사용 시 경고 한 줄, POSIX에서 group/other 쓰기 가능 후보는 건너뜀(m4). 저장소의 테스트 러너 관례는 구현할 때 확인한다.

---

## 15. macOS e2e 실측 절차

준비: 서버 워크트리에서 로컬 D1에 마이그레이션을 적용한다(`pnpm run db:migrations:apply:local`). 로컬 서버를 기동하고 `.dev.vars`에 `TRUSTED_MALGN_DESKTOP_CLIENT_ID`를 넣는다(값은 문서에 남기지 않는다). 앱은 debug 빌드에 `MALGN_HUB_ORIGIN=<로컬>`을 준다. 플러그인은 워크트리를 `claude --plugin-dir`로 로드한다. 각 단계의 결과를 기록한다. 실패하면 그 자리에서 멈춘다.

| # | 단계 | 기대 |
|---|---|---|
| E1 | 수정한 plugin.json으로 `claude` 실행, `/mcp` | helper가 실행되고 malgnai-hub가 "connected"로 표시(**플러그인 headersHelper 실동작 확인, 가장 중요**) |
| E2 | 앱에서 [허브 연결] → 브라우저 로그인, 동의 | 동의 화면에 365일 안내. 키체인 항목 생성: `security find-generic-password -s com.malgnsoft.agent.hub -a device-token`(**`-w` 금지**, 값 출력 안 함) |
| E3 | `…/malgn-agent --hub-auth-header \| node -e 'console.log(Object.keys(JSON.parse(require("fs").readFileSync(0))))'`, `echo $?`, `time` | `[ 'Authorization' ]`, 0, 1초 미만, Dock 아이콘과 창 없음 |
| E4 | MCP 도구 호출(예: project_list) | 성공. 로컬 D1 `last_used_at` 갱신 |
| E5 | 웹 `/keys`에서 해당 기기 폐기 → 도구 호출. 이어서 `/mcp`에서 재연결 | 도구 호출: 401 → helper 재실행 1회 → 다시 401 → `/mcp`에 "needs authentication"(문서 동작, 무한 루프 없음). 재연결: 연결 시 거부 → "연결 실패"(OAuth로 가지 않음). "needs authentication"에서 인증을 시도할 때 OAuth discovery로 넘어가는지 기록(U2 ②) |
| E6 | 앱 [연결 해제] → CLI 실행, 이어서 Claude `/mcp` | exit 10과 안내. **helper stderr 안내 문구가 `/mcp` 화면·연결 오류 상세·`claude --debug` 출력·로그 중 어디에 보이는지 기록(U12)** |
| E7 | 로컬 D1에서 expires_at을 now+29d로 조정 → 앱 재시작 | 새 행 생성, 부모의 `replaced_by_id` 설정과 만료 ≤24h, 키체인 값 교체, CLI가 새 토큰 출력 |
| E8 | 부모 토큰으로 `curl -X POST …/api/devices/renew`(토큰은 셸 변수로만 전달하고 이력에 남기지 않음) | 409 ALREADY_RENEWED, 자식까지 revoked, 감사 1건 |
| E8b | E7을 다시 만든 뒤, 로컬 D1에서 부모 `expires_at`을 과거로 바꾸고 키체인을 부모 토큰으로 되돌린 상태에서 앱 재시작(탈취자 선갱신 + 앱 24h 이상 꺼짐 재현, M1) | 앱의 self가 409를 받고, 같은 `device_id`의 자식이 revoked, 앱은 "보안상 연결이 해제" 안내 |
| E9 | 앱을 재빌드(cdhash 변경)한 뒤 GUI를 실행하지 않고 CLI만 실행 | 키체인 프롬프트 발생 여부 기록. 무시하면 5s 후 12. "항상 허용" 후에는 프롬프트 없음 |
| E10 | 앱 자율업무(`claude -p`) 실행 | 헤드리스에서도 helper가 동작하고 hub 도구 호출 성공 |
| E11 | 앱 2개 동시 실행 + E7 조건 | 갱신이 1회만 일어나고 계열 폐기가 없음 |
| E12 | 래퍼가 ① exit 1(빈 stdout) ② `{}` 출력 각각일 때 `/mcp` 상태 | "failed"인지 "needs authentication"(OAuth discovery)인지 기록. D8 표 갱신(U2 ①) |
| E13 | .app 이름을 바꿔 미설치를 흉내 | 래퍼의 "앱 미설치" 안내가 어디에 보이는지 기록(U12) |
| E14 | 프로젝트 `.mcp.json`에 `{"x":{"type":"http","url":"http://127.0.0.1:9/mcp","headersHelper":"<앱 절대경로> --hub-auth-header"}}`를 두고 `claude` 실행(M3) | helper가 14로 끝나 헤더가 나가지 않음. 127.0.0.1:9 쪽에 Authorization이 도착하지 않음(필요하면 `nc -l`로 확인). 플러그인 래퍼 경유로 같은 시험도 반복 |
| E15 | Claude Code가 helper에 넘기는 환경 확인: 래퍼에 임시로 `CLAUDE_CODE_MCP_SERVER_URL`·`CLAUDE_CODE_MCP_SERVER_NAME`·`CLAUDE_PLUGIN_ROOT` 존재 여부만(값 말고) stderr에 찍는 디버그 빌드 | 세 변수가 모두 설정됨. URL이 `REDACTED` 없이 원문. 미설정이면 §7.3의 "Claude Code 실행 시 항상 설정" 전제가 깨지므로 M3 설계를 재검토 |

---

## 16. Windows 실기 확인 항목 (`docs/WINDOWS-CHECKLIST.md`에 추가할 초안)

```markdown
## 9. 허브 연결(기기 토큰) — 신규

- [ ] (W1) 설치 경로가 `%LOCALAPPDATA%\malgn-agent\malgn-agent.exe`인가? 아니면 실제 경로를 기록
- [ ] (W2) PowerShell에서 `& "$env:LOCALAPPDATA\malgn-agent\malgn-agent.exe" --hub-auth-header | Out-String` 실행 시
      JSON 한 줄이 출력되고 **창·콘솔이 뜨지 않는다**(GUI 서브시스템 stdout 파이프 동작 확인)
      - 출력이 비어 있으면 결과를 기록 — 콘솔 헬퍼 바이너리 대안으로 전환한다
- [ ] (W3) 앱에서 [허브 연결] → 브라우저 동의 → 앱이 "연결됨"으로 바뀐다(방화벽이 127.0.0.1 루프백을 막지 않는가)
- [ ] (W4) 제어판 → 자격 증명 관리자 → Windows 자격 증명에 `com.malgnsoft.agent.hub` 항목이 있다(값은 열지 않는다)
      - 항목 상세의 **지속성이 "로컬 컴퓨터"**인가? "엔터프라이즈"면 도메인 로밍 프로필에서 다른 PC로 로밍되어 "기기 단위" 전제가 깨진다 — 결과를 기록(m6)
- [ ] (W5) Claude Code에서 `/mcp` → malgnai-hub "connected", 도구 호출 성공
      - helper가 cmd/PowerShell 중 어느 셸로 실행되는지와 무관하게 `node "…\hub-auth-header.mjs"`가 동작하는가
- [ ] (W6) 업데이트 설치 후에도(재설치 포함) 자격 증명이 유지되고 CLI가 동작한다
- [ ] (W7) 앱 [연결 해제] 후 자격 증명 관리자에서 항목이 사라지고, Claude에 "앱에서 허브에 연결하세요" 안내가 뜬다(안 보이면 어디에 나타나는지 기록 — U12)
- [ ] (W8) 사용자 계정 이름에 한글·공백이 있어도 locator(`%APPDATA%\com.malgnsoft.agent\cli-path.json`) 경로가 동작한다
- [ ] (W9) 백신(Defender 등)이 `--hub-auth-header` 실행을 차단·지연시키지 않는다(10초 내 완료)
```

---

## 17. 롤아웃 순서

1. **서버** (호환 유지. 플러그인이 아직 OAuth여도 영향 없음): 0035 로컬 적용과 테스트 → devops 단계에서 원격 적용 → 코드 배포 → env `TRUSTED_MALGN_DESKTOP_CLIENT_ID` 등록. 반드시 0035를 먼저 적용한다(§6.1).
2. **앱 릴리스**: 연결 UI, CLI 모드, locator, 갱신. 직원들이 [허브 연결]을 한다. 이 시점에는 플러그인이 아직 OAuth를 쓰므로 두 경로가 함께 존재한다.
3. **실측**: E1~E15(E8b 포함), W1~W9를 통과해야 한다.
4. **플러그인 릴리스**: oauth를 제거하고 headersHelper를 넣는다. 공지 문구는 "업데이트 전에 앱에서 허브에 연결하세요"다.
5. 이후 OAuth로 발급한 기존 토큰은 자연히 만료된다(access 8h, refresh 90d). 남은 plugin client OAuth grant는 **일괄 폐기하지 않고 자연 만료(최대 90일)로 둔다**(U5 결정, m9). 일괄 폐기는 수동 탈출구 사용자를 끊을 위험이 있어 이득보다 손해가 크다.
6. **플러그인 릴리스 후 8주 안(U4 기한, M5)**: 서버 `/mcp` 분기(`[S] server/index.js:202`)에서 `oauth_client_id IS NULL`(페어링 발급분) 토큰을 거부한다. 사용량 수집용 평문 파일 토큰을 `/api/sessions` 전용으로 낮추는 최소 조치다. 서버 한 줄 변경이고, `userConfig.device_token` 레거시 탈출구 제거(§8.1)와 정합한다. 착수 전 확인: 그 시점 `/mcp`에서 페어링 토큰의 `last_used_at`이 최근인 행이 있는지 조회해, 있으면 해당 사용자에게 앱 연결을 먼저 안내한다. 테스트: 페어링 토큰으로 `/mcp` → 401, `POST /api/sessions` → 기존대로 200, desktop·OAuth 토큰으로 `/mcp` → 통과. 수집 에이전트가 앱 CLI를 쓰도록 바꿔 평문 파일 자체를 없애는 근본 조치(U4 ①)는 그 뒤 별도 작업이다.

---

## 18. 미결 사항

| # | 항목 | 결정 시점 / 주체 |
|---|---|---|
| U1 | 플러그인 `headersHelper` 실동작(E1). **문서상으로는 확인됨**: 플러그인 서버 helper에 `CLAUDE_PLUGIN_ROOT`·`CLAUDE_CODE_MCP_SERVER_NAME`·`CLAUDE_CODE_MCP_SERVER_URL`이 설정되고, 자격증명류 이름의 변수는 제거되며, 10초 타임아웃·연결마다 새 실행(캐시 없음)이다(§2). 남은 것은 실동작 확인(E1)과 환경변수 실재 확인(E15, M3의 전제). 실패하면 §8.3 대안 A/B/C 중 선택 | 실측 후, PM |
| U2 | **좁혀짐.** 문서로 확인된 것: 연결 시 자격증명 거부 → "연결 실패"(OAuth로 가지 않음), 도구 호출 401/403 → helper 재실행 후 1회 재시도 → 실패 시 "needs authentication". 남은 것: ① helper가 0이 아닌 코드로 끝나거나 `Authorization` 없이 끝날 때의 표시와 OAuth discovery 진입 여부(E12) ② "needs authentication" 상태에서 사용자가 인증을 시도하면 서버 `WWW-Authenticate`를 따라 DCR OAuth로 가는지(E5) → D8 표 갱신 | 실측 후 |
| U3 | macOS Developer ID 서명 도입(R3 근본 해결). 지금은 ad-hoc이라 업데이트 후 키체인 프롬프트 위험이 있다 | 사람 결정(비용, 계정) |
| U4 | 사용량 수집 에이전트의 평문 페어링 토큰(365d, MCP 전체 권한, §12 R1). **기한: 플러그인 릴리스 후 8주 안**에 최소 조치(페어링 발급분 `/mcp` 거부, §17-6)를 배포한다. 근본 조치(수집 에이전트가 앱 CLI를 쓰도록 바꿔 평문 파일 제거)와 가장 오래된 토큰 만료(2027년 7~8월) 대응은 **2027-06 이전** 별도 작업 | 기한 확정(8주 / 2027-06), 작업 착수는 PM |
| U5 | ~~플러그인 전환 후 기존 plugin-client OAuth grant 처리~~ **결정: 자연 만료**(§17-5, 보안 검토 m9) | 닫힘 |
| U6 | `TRUSTED_MALGN_DESKTOP_CLIENT_ID`의 실제 값 생성과 등록 방식(기존 `TRUSTED_MALGN_AGENT_CLIENT_ID`와 같은 경로인지 대시보드에서 확인) | devops 단계 |
| U7 | `device_tokens.expires_at NOT NULL` 같은 DB 제약은 다음 테이블 재생성 때 함께 넣는다 | 후속 |
| U8 | 워크트리 안 vitest 수집이 exclude 규칙에 걸리는지 | 구현 착수 시 실측 |
| U9 | 서버 저장소 포인터: `docs/`가 gitignored라 커밋할 수 없다. 원본 로컬 `docs/design/`에 포인터 파일을 둘지, 서버 `CLAUDE.md` 정본 문서 표에 한 줄을 추가할지 | PM(병합 시) |
| U10 | Windows GUI 서브시스템 stdout 파이프(W2). 실패하면 Windows 한정 콘솔 헬퍼 바이너리 | 실측 후 |
| U11 | **해소(c안 채택, 보안 검토 M1).** 교체된 토큰의 재제시를 만료·유예와 무관하게 재사용으로 판정하고 `device_id` 계열을 폐기한다(§5.0). (a) "재연결 시 같은 사용자의 다른 desktop 토큰 일괄 폐기"는 정상적인 다른 기기를 함께 끊고 확인 UI가 필요해 기각. (b) "다른 데스크톱 연결 n개" 표시는 선택 사항으로 보류(§20) | 닫힘 |
| U12 | helper stderr 안내 문구("맑은에이전트 앱에서 허브에 연결하세요" 등)가 사용자에게 어디서 보이는지(`/mcp` 화면, 연결 오류 상세, `--debug`, 로그). 문서에 없다. 보이지 않으면 SessionStart 훅 안내(토큰을 내지 않는 상태 조회 플래그 필요)를 검토. 그 전까지 확정 채널은 앱 GUI 허브 위젯과 릴리스 공지다(§4 D8) | 실측 E6·E13·W7 후, PM |
| U13 | Windows keyring 백엔드의 자격증명 지속성 값(로컬 컴퓨터 / 엔터프라이즈 로밍). 엔터프라이즈면 로밍 프로필 PC에서 "기기 단위" 전제가 깨진다. 사내 PC가 로밍 프로필을 쓰지 않으면 무관(m6, 미검증) | 구현 시 keyring 소스 확인 + W4 |

---

## 19. 같이 바꿔야 하는 값 (정본과 사본)

| 값 | 정본 | 사본 / 영향 |
|---|---|---|
| 365일, 30일 창, 24h 유예 | `[S] server/lib/tokens.js` 상수 | 앱은 `renew_after`와 `expires_at` 응답값만 쓴다(하드코딩 없음). 동의 화면 문구의 "365일"은 authorize-context의 `token_lifetime_days`로 받는다 |
| desktop client_id | 서버 env `TRUSTED_MALGN_DESKTOP_CLIENT_ID` | `[A] src-tauri/src/hub_auth/consts.rs`. 두 값이 다르면 desktop 분기가 일어나지 않는다(fail-closed, 8h 일반 토큰도 아님. 미등록 client라 동의 단계에서 거부) |
| hub origin과 MCP path(`/mcp`) | `[P] plugin.json` url | `[A] consts.rs HUB_ORIGIN`, 키체인 JSON의 `hub` 검증, **`[A] cli_mode.rs` audience 판정**(origin + path `/mcp`), **`[P] hub-auth-header.mjs` audience 사전 차단**. url을 바꾸면 이 네 곳을 같이 바꾸지 않으면 CLI가 14로 토큰을 내주지 않는다(fail-closed, 장애로 드러남) |
| 토큰 접두사 `mhd_`와 길이 | `[S] tokens.js generateDeviceToken` | `[P] bin/hub-auth-header.mjs` 출력 검증 정규식과 stderr 마스킹 정규식, `[A]` `StoredToken` Debug 마스킹과 M4 테스트 패턴 |
| CLI 종료코드 표(0, 1, 10~14) | 이 문서 §7.3 | `[A] cli_mode.rs`, `[P] hub-auth-header.mjs` 안내 매핑(14 `WRONG_AUDIENCE` 포함) |
| 재사용 판정 대상 경로 | 이 문서 §5.0 | `[S] devices.js`의 `use()` 등록 2줄(`/renew` graceAware:false, `/self` graceAware:true). `/mcp`·`/api/sessions`에는 걸지 않는다 |

---

## 20. 보안 검토 반영 대응표

대상 보고서: `docs/security/device-token-auth-design-review.md`(Amber, Critical 0 / Major 5 / Minor 10). 상태 표기: **반영** = 이 문서에 설계로 확정 / **반영(실측)** = 확인 절차를 이 문서에 넣었고 결과는 실측으로 정함 / **보류** = 근거를 두고 이번 범위에서 뺌 / **기각** = 동의하지 않음.

### 20.1 구현 착수 전 반영 필수 항목 (보고서 §3)

| 보고서 ID | 요지 | 상태 | 반영 위치 |
|---|---|---|---|
| M1 | 부모 유예 경과 후 재사용 미탐지 → 탈취 자식 무기한 지속. 수정안 (c): 교체된 토큰 재제시 = 재사용(renew는 항상, self는 유예 경과 시) | **반영** | §1 D4, §3.3, §4 D4, **§5.0 신설**(`detectReplacedTokenReuse`, `findAnyByHash`), §5.2 검증 순서 0·3·5, §5.3, §5.4, §7.2 앱 규칙(매 호출 키체인 재읽기, 401 시 self 확인), §9, §10, §12 R8, §14.1 재사용 탐지 테스트 4종 + 추가 1종, §14.2, §15 E8b, §18 U11 닫음, §19 |
| M2 | CLI는 같은 사용자 누구에게나 열린 토큰 발급기. R2·R7 서술이 보호 수준을 과장 | **반영** | §4 D6(ACL은 가용성 이점이지 보안 통제가 아님, 부모 프로세스 검사 미도입), §4 D7(경로 검사는 오설정 방지용), §12 R2·R7 재작성(대응 = 재사용 탐지 + last_used_at + 짧은 폐기 경로) |
| M3 | 토큰을 받을 서버(audience) 미확인 → 다른 MCP 서버 설정으로 hub 토큰 유출 | **반영** | §2(helper 환경변수 사실), §7.3 audience 확인 규칙 + 종료코드 **14 `WRONG_AUDIENCE`**, §8.2 래퍼 사전 차단(0단계), §10, §12 R4b, §13, §14.2 audience 판정 테스트(변수 없음/정확일치/대소문자·기본포트/origin·포트 불일치/path 불일치/query/빈 값·REDACTED, 14일 때 store 미호출), §14.3, §15 E14·E15, §19 hub origin·종료코드 행 |
| M4 | 토큰 원문이 WebView(JS)로 넘어가지 않는다는 계약 부재 | **반영** | §7.1 계약(반환은 메타데이터만, `StoredToken` Debug 마스킹, reqwest 오류 `without_url()` + 자체 오류 enum), §12 R4, §13, §14.2 비노출 테스트 |
| M2·M5 서술 | R1 "평문 파일은 없다" 정정 | **반영** | §12 R1 재작성(사용량 수집 평문 토큰 365d·MCP 전체 권한 명시), §4 D10 |
| M5 U4 기한 | U4에 기한 부여, ①/② 중 선택 | **반영** | §18 U4(플러그인 릴리스 후 **8주** 안 최소 조치, 근본 조치는 2027-06 이전), §17-6(② 페어링 발급분 `/mcp` 거부 채택 + 착수 전 조회 + 테스트), §6.2. ① 앱 CLI 전환은 후속 별도 작업으로 둠 |
| m1 | NaN 만료값 fail-open | **반영** | §1 D3, §4 D3②(DAO 단언 순서: 누락 → NaN → 상한) ③(미들웨어 `Number.isFinite` 판정), §5.2 1단계, §10, §12 R9, §14.1 NaN 테스트 |
| m2 | 계열 폐기를 체인 추적(상한 32)에서 `device_id` 단위 한 문장으로 | **반영** | §1 D4, §4 D4(`revokeDesktopFamily` SQL, 페어링·타 사용자 비오염 조건), §5.4, §6.1(인덱스 판단), §9, §14.1(40단 체인·비오염 테스트) |

### 20.2 Minor (보고서 §4)

| 보고서 ID | 요지 | 상태 | 반영 위치 / 근거 |
|---|---|---|---|
| m3 | desktop client redirect host를 `127.0.0.1` 정확일치 | **반영** | §5.1, §9, §14.1 |
| m4 | 래퍼 후보 실행파일 권한 검사, 일시 경로 locator 미기록, `MALGN_AGENT_EXE` 경고 | **반영** | §4 D7, §12 R7, §14.3 |
| m5 | 래퍼가 앱 stderr의 `mhd_…` 마스킹, stdout 4KB 상한 | **반영** | §8.2, §12 R4, §14.3 |
| m6 | Windows keyring 자격증명 지속성(로밍) 확인 | **반영(실측)** | §16 W4 항목 추가, §18 U13 |
| m7 | 신설 핸들러는 기기 토큰 전용 컨텍스트 키만, `deviceTokenId` 없으면 500 | **반영** | §5 공통, §14.1 |
| m8 | 0035 적용 전후 검사 쿼리를 런북에 | **반영** | §6.1 |
| m9 | plugin-client grant는 자연 만료로 충분 | **반영** | §17-5, §18 U5 닫음 |
| m10 | ALREADY_RENEWED 안내에 "반복되면 관리자에게 알리세요" | **반영** | §5.2 오류표, §7.2, §10 |
| m10 부속 / U11 (b) | "이 계정의 다른 데스크톱 연결 n개" 앱 표시 | **보류** | 보고서도 선택 사항으로 분류했다. M1(c)이 갱신 개입형 탈취를 자동으로 잡으므로 사람이 숫자를 보고 알아채기를 기대하는 장치의 한계 효용이 작고, self 응답 필드·앱 UI가 늘어난다. 웹 `/keys`의 종류·만료 열(§5.5)로 같은 정보를 이미 볼 수 있다 |
| U11 (a) | 재연결 시 같은 사용자의 다른 desktop 토큰 일괄 폐기 | **기각** | 정상적인 다른 기기를 함께 끊고 확인 UI가 필요하다. (c)가 더 단순하고 정확하다(보고서 §2 M1 평가와 동일) |

### 20.3 기각한 항목

- 보고서 §3의 필수 항목 중 기각한 것은 **없다.** 위 표의 기각은 보고서가 함께 기각을 권고한 U11 (a)뿐이다.

### 20.4 PM 확인 사실 반영 (공식 문서 headersHelper 절)

| 사실 | 반영 위치 |
|---|---|
| helper 환경변수 3종, 자격증명류 변수 제거, url 내 `REDACTED` 치환 | §2 추가 행, §7.3 audience 규칙(REDACTED 포함 시 14), §15 E15 |
| 연결 시 거부 → 연결 실패(OAuth로 안 감), 도구 호출 401/403 → helper 재실행 1회 재시도 → needs authentication | §2, §3.3, §4 D8, §15 E5, §18 U2 |
| 타임아웃 10초, 연결마다 새로 실행(캐시 없음) | §2, §3.3 |
| 안내 문구 노출 위치 | §4 D8 "안내 문구의 노출 위치"(1차 stderr는 미확인, 2차 앱 GUI는 확정), §8.2, §15 E6·E13, §16 W7, **§18 U12 신설** |
