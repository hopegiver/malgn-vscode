# 기기 단위 장기 토큰 인증 — 3저장소 공통 설계 (정본)

- 상태: **설계 초안** — 구현 전. 아직 리뷰하지도 사람이 승인하지도 않았다.
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
| D3 | 만료 상한은 **서버 3곳에서 강제**한다. ① 발급 시 서버 상수로 계산 ② DAO `insert`/`rotateToken`에서 `expiresAt` 누락이나 상한 초과를 거부 ③ 검증 미들웨어에서 `expires_at IS NULL`을 거부. 클라이언트는 TTL을 지정할 수 없다. |
| D4 | 갱신은 `POST /api/devices/renew` 하나로 한다. 기존 토큰으로 인증하고, 새 행을 발급한다. 갱신은 토큰당 1회만 허용한다. 직전 토큰은 24시간 유예 후 만료된다. 이미 갱신된 토큰이 다시 갱신을 시도하면 탈취 신호로 보고 그 계열(family) 전체를 폐기한다. 갱신 가능 시점(`renew_after`)은 서버가 응답에 실어 보내는 값 하나가 정본이다. |
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
| oauth와 headers가 함께 있을 때의 우선순위는 문서에 없다 | 일부는 문서에 있다. "helper 출력에 `Authorization`이 **있으면** OAuth로 폴백하지 않고, 연결 시 거부되면 '인증 필요'가 아니라 '연결 실패'로 표시한다." helper가 실패하거나 `Authorization` 없이 끝날 때의 동작은 문서에 없다. 그래서 D8에서는 함께 두지 않는다. | code.claude.com/docs/en/mcp "Interaction with OAuth" |
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

직전 토큰은 서버 쪽에서 `expires_at = min(기존, now+24h)`로 줄어든다. 이미 연결된 다른 Claude 세션은 직전 토큰을 메모리에 들고 있다. 24시간 안에 401을 받으면 helper를 다시 실행해 새 토큰을 읽는다.

### 3.4 폐기
- 앱의 [연결 해제]: `DELETE /api/devices/self`를 호출한 뒤 키체인 항목을 삭제한다. 서버 호출이 실패해도 키체인은 삭제하고 "서버 폐기 실패, 웹 /keys에서 폐기하세요"라고 안내한다.
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
- ② DAO: `insert`와 `rotateToken`은 `expiresAt`이 없거나 `> now + MAX + 60s(skew)`이면 throw한다. 앞으로 새 발급 경로가 생겨도 상한을 빠뜨릴 수 없다.
- ③ 검증: `deviceAuthMiddleware`에서 `!token.expires_at`을 거부한다(fail-closed). 무기한 토큰은 구조적으로 존재할 수 없게 된다.
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
- 재갱신(`replaced_by_id IS NOT NULL`인 토큰으로 renew를 시도한 경우): 부모와, `replaced_by_id`를 따라가며 찾은 모든 후손을 `revoked`로 바꾼다. 체인 길이는 많아야 수십이고, 반복 상한은 32다. 감사 기록은 `device_token.revoked` + metadata `{reason:'renew_reuse'}`로 남기고 409 `ALREADY_RENEWED`를 반환한다.

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
  - macOS 키체인 ACL이 "항목을 만든 앱"을 신뢰하므로, GUI가 만든 항목을 같은 바이너리가 읽을 때 프롬프트가 뜨지 않는다.
  - 번들 구성이 바뀌지 않는다.
- 대안은 Tauri `bundle.externalBin`으로 콘솔 서브시스템 헬퍼 `malgn-agent-auth(.exe)`를 두는 것이다. Windows GUI 서브시스템의 stdout 문제는 확실히 피할 수 있다. 하지만 macOS에서는 ACL상 다른 앱이 되어 프롬프트 위험이 있고, 바이너리 2개를 서명·배포해야 한다. **조건부 대안**: Windows 실측(§16 W2)에서 파이프 stdout이 실패할 때만 Windows에 한해 채택한다.

### D7. 플러그인이 앱 실행파일을 찾는 방법

| 안 | 장점 | 단점 |
|---|---|---|
| **node 래퍼 + locator + 기본경로 (채택)** | 한 문자열로 크로스플랫폼을 처리한다. 셸 인용 문제는 기존 훅과 같은 패턴이다(`[P] malgn-agent/hooks/hooks.json:8,12`). 타임아웃, 출력 검증, 안내 문구를 래퍼가 맡는다. PATH 하이재킹 표면이 없다. | node에 의존한다. 다만 같은 플러그인의 SessionStart/Stop 훅이 이미 `node`를 필수로 쓰므로 새 의존성은 아니다. 프로세스를 2개 띄운다(약 50~100ms). |
| 실행파일 절대경로를 직접 지정 | 프로세스 1개로 끝난다. | 플러그인 하나가 OS 2종과 설치 위치 차이를 한 문자열로 표현할 수 없다. |
| PATH의 `malgn-agent` | 단순하다. | 앱이 PATH에 등록되지 않는다. PATH 하이재킹 표면이 생긴다. |
| 앱이 user-scope MCP 서버를 직접 등록(`claude mcp add … headersHelper=<절대경로>`) | 앱이 자기 경로를 정확히 안다. | 서버 이름이 `plugin:malgn-agent:malgnai-hub`에서 바뀌어 도구 접두사 `mcp__plugin_malgn-agent_malgnai-hub__`에 의존하는 훅과 에이전트가 깨진다(`[P] malgn-agent/hooks/stop-mcp-reminder.cjs:47`). **headersHelper 실측이 실패할 때의 대안 B**로 보류한다(§9.3). |

래퍼(`[P] malgn-agent/bin/hub-auth-header.mjs`)가 실행파일을 찾는 순서는 다음과 같다. 첫 번째로 존재하고, 절대경로이고, 파일명이 `malgn-agent`(Windows는 `malgn-agent.exe`)인 것을 쓴다.
1. 환경변수 `MALGN_AGENT_EXE`: 개발과 실측용 오버라이드다. 이름에 제거 대상 문자열이 없어 helper 환경까지 전달된다.
2. locator 파일: 앱(GUI)이 시작할 때마다 `std::env::current_exe()`를 원자적으로 기록한다(기존 `[A] src-tauri/src/fs_atomic.rs`).
   - macOS: `~/Library/Application Support/com.malgnsoft.agent/cli-path.json`
   - Windows: `%APPDATA%\com.malgnsoft.agent\cli-path.json`
   - 내용: `{"exe":"…","version":"0.x.y"}`. Tauri `app_config_dir()` 규칙과 일치한다(identifier `com.malgnsoft.agent`, `[A] tauri.conf.json`).
3. 기본경로
   - macOS: `/Applications/malgn-agent.app/Contents/MacOS/malgn-agent`, `~/Applications/malgn-agent.app/Contents/MacOS/malgn-agent`
   - Windows: `%LOCALAPPDATA%\malgn-agent\malgn-agent.exe`. NSIS currentUser 기본 경로로 추정하며, 실측 대상이다(§16 W1).

locator 파일은 같은 사용자만 쓸 수 있는 위치에 있다. 그 위치를 조작할 수 있는 공격자는 이미 그 사용자 계정 권한을 가진 것이라, 새 공격 표면이 아니다(§12 R7).

### D8. 앱이 없는 PC의 폴백

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **F1 (권고)** 폴백 없음 + 수동 탈출구 | plugin.json에서 `oauth`를 제거하고 `headersHelper`만 둔다. 앱이 없거나 토큰이 없으면 연결 실패와 함께 "맑은에이전트 앱에서 허브에 연결하세요"를 안내한다. 예외 PC(리눅스 서버, 헤드리스 등)는 문서화된 수동 절차 `claude mcp add --transport http --scope user malgnai-hub-oauth <URL>`로 OAuth를 쓴다. | 인증 경로가 하나다. 문서에 없는 동작(oauth와 helper의 우선순위)에 의존하지 않는다. 목표인 "세션마다 재인증 제거"를 확실히 달성한다. 서버 OAuth는 그대로 유지되어 탈출구가 실제로 동작한다. | 앱 미설치자는 hub를 쓸 수 없다. 탈출구로 등록하면 도구 접두사가 달라진다. |
| F2 oauth와 helper 공존 | helper가 토큰이 없을 때 `{}`를 출력해 OAuth 폴백을 기대한다. | 끊김 없이 이행할 수 있다. | 우선순위가 문서화되지 않았다. 버전에 따라 동작이 바뀔 수 있다. 8h/90d 경로가 계속 주력처럼 남는다. 확정 결정 3("oauth 블록을 빼고")과도 어긋난다. |
| F3 helper가 사용량 수집 에이전트의 평문 파일 토큰을 읽음 | 앱 없이도 동작한다. | 평문 토큰을 정식 경로로 승격하는 셈이라 금지 조건 위반이다. |
| F4 OAuth 전용 두 번째 서버 항목을 플러그인에 추가 | 설치만 하면 된다. | 앱 사용자에게도 OAuth 서버가 노출되어 매 세션 "인증 필요" 소음이 생긴다. 도구가 중복된다. |

근거: 직원 50명 미만이고 전원이 macOS/Windows이며 맑은에이전트 앱이 표준 프로비저닝 도구다. 앱 미설치는 예외 상황이다. 실측 E12(§15)에서 "helper 실패 시 Claude Code가 WWW-Authenticate를 보고 OAuth discovery로 넘어가는지"를 기록한다. 넘어간다면 F1에서도 사실상 DCR OAuth 폴백이 생기는 셈이니, 그 결과를 이 표에 반영한다(미결 U2).

### D9. email_send
§11 참고. 변경 없음.

### D10. 레거시 무기한 토큰
- 대안 ① NULL 유지: 결정 1 위반이라 불가하다.
- 대안 ② 일괄 폐기: 사용량 수집 에이전트가 즉시 전부 중단된다.
- **채택 ③**: `created_at + 365일`로 채운다. 현재 운영 기간이 1년 미만이므로 즉시 만료되는 토큰은 없다. 다만 가장 오래된 토큰은 2027년 7~8월에 만료된다. 그 전에 사용량 수집 에이전트를 재페어링하거나 앱 토큰 경로로 옮겨야 한다(미결 U4).

---

## 5. API 명세 (서버)

공통 사항
- 오류 봉투는 기존 `{ error: { code, message } }`를 쓴다. 단 `/api/oauth/token`은 RFC 형식(`{error, error_description}`)을 유지한다.
- 기기 토큰 인증 라우트는 `PUBLIC_PATHS`에 경로를 **정확히 일치**하도록 추가해 JWT 게이트를 우회시킨다. 그 대신 **라우터 레벨 `use()`로 메서드와 무관하게** `requireDeviceToken`을 강제한다. 함정이 하나 있다. `PUBLIC_PATHS`는 메서드를 보지 않는다(`[S] server/middleware/jwt-auth.js:41-45`). 그래서 같은 경로에 핸들러별로 미들웨어를 붙이면 빠뜨리는 순간 무인증이 된다.
- `requireDeviceToken`은 지금 `[S] server/api/sessions.js:26-36`에만 지역 함수로 있다. 이를 `[S] server/middleware/device-token-auth.js`로 추출하고 sessions와 devices가 함께 import한다. `c.set('deviceTokenId', …)`도 추가한다.
- `deviceAuthMiddleware`의 identity에 `deviceTokenId`를 추가한다(`[S] mcp/device-auth.js:21-24`). MCP props에 필드가 하나 늘어나지만 무해하다.

### 5.1 `POST /api/oauth/token` — desktop 분기 (기존 엔드포인트 변경)
- 인증: 없음(PUBLIC, 기존과 같음). 레이트리밋: 기존 `OAUTH_TOKEN_RL` `${ip}:authcode`.
- 분기 조건: `stored.client_id === env.TRUSTED_MALGN_DESKTOP_CLIENT_ID`. env가 없으면 분기하지 않는다(fail-closed, 일반 경로로 간다).
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
- 인증: `Authorization: Bearer mhd_…`(기기 토큰). PUBLIC_PATHS에 `'/api/devices/renew'`를 추가하고 `devices.use('/renew', requireDeviceToken)`을 건다.
- 권한: 토큰 소유자 본인만 가능하다. 대상이 곧 인증에 쓴 토큰이므로 IDOR 표면이 없다. body는 받지 않는다(무시).
- 검증 순서:
  1. 기기 토큰 유효성: 해시 일치, `status='active'`, 사용자 활성, 미만료. 하나라도 어긋나면 401.
  2. `oauth_client_id === env.TRUSTED_MALGN_DESKTOP_CLIENT_ID`가 아니면 403 `NOT_RENEWABLE`. 페어링이나 OAuth 8h 토큰은 갱신할 수 없다.
  3. `replaced_by_id IS NOT NULL`이면 계열 폐기 후 409 `ALREADY_RENEWED`.
  4. `expires_at - now > 30일`이면 409 `RENEW_TOO_EARLY`이고 응답에 `renew_after`를 담는다.
  5. §4 D4의 batch를 실행한다. 두 번째 문장의 changes가 0이면(동시 요청과의 경합) 3번과 같이 처리한다.
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
| 409 | ALREADY_RENEWED | 재갱신. 서버가 계열 전체를 이미 폐기함 | 키체인 삭제, "보안상 연결이 해제되었습니다. 다시 연결하세요" |
| 429 | RATE_LIMITED | Retry-After 포함 | 백오프 |
| 5xx/네트워크 | — | — | 기존 토큰을 유지하고 1h 후 재시도 |

- 감사: 성공 시 `device_token.issued` + `{via:'renew', parent_id}`. 재갱신 시 `device_token.revoked` + `{reason:'renew_reuse', revoked_count}`.

### 5.3 `GET /api/devices/self` (신설)
- 인증: 기기 토큰. PUBLIC_PATHS에 `'/api/devices/self'`를 추가하고 `devices.use('/self', requireDeviceToken)`을 건다. 이 라우트들은 **`devices.delete('/:id')`보다 먼저 등록**한다. 그렇지 않으면 `self`가 `:id`로 매칭된다.
- 응답 200: `{ "device_token_id": "…", "device_name": "hopegiver-mbp", "expires_at": "…", "renew_after": "…", "renewable": true, "user": { "email": "…@malgnsoft.com" } }`
  - `renewable`은 desktop 발급분이면서 `replaced_by_id IS NULL`인지를 뜻한다. `email`은 본인 값이므로 노출 경계가 넓어지지 않는다.
- 401은 위와 같다. 앱은 연결 확인과 오프라인/폐기 구분에 이 엔드포인트를 쓴다.

### 5.4 `DELETE /api/devices/self` (신설)
- 인증: 기기 토큰(5.3과 같은 `use`).
- 처리: 자기 자신과 `replaced_by_id`로 이어진 후손을 폐기한다. 기존 `revokeAllForDeviceToken`(oauth refresh)도 함께 호출한다(0건이어도 무해). 감사 기록은 `device_token.revoked` + `{via:'self'}`이다.
- 응답 200: `{ "status": "revoked" }`. 이미 폐기된 토큰이면 미들웨어 단계에서 401이고, 앱은 이를 성공으로 간주한다.

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
- 인덱스는 새로 만들지 않는다. `replaced_by_id`는 PK로 조회한 행에서 읽기만 하고, 그 값으로 검색하지 않는다. 체인 추적도 `id`(PK)로 따라간다.
- FK·CHECK 제약은 ALTER로 추가할 수 없어 넣지 않는다. 대신 DAO 단언으로 보완한다(D3②). 테이블 재생성은 미결 U7이다.
- forward-only 원칙을 따른다(`[S] migrations/README.md`). 되돌려야 하면 새 마이그레이션을 쓴다. 다만 채운 `expires_at`은 다시 NULL로 되돌리지 않는다(결정 1).
- **배포 순서 제약**: 0035를 먼저 적용하고 D3③ 코드를 그 뒤에 배포한다. 순서가 바뀌면 레거시 토큰이 즉시 401을 받는다. 다만 fail-closed라 보안 위험은 없고 기능 장애만 생긴다.
- `docs/schema.sql`(서버 로컬 문서, gitignored)의 `device_tokens` 정의에도 `replaced_by_id`를 반영한다.

### 6.2 기존 `device_tokens` 사용처 영향

| 사용처 | 영향 |
|---|---|
| `/mcp` (Claude Code, OAuth 발급분 8h) | 없다. 이미 expires_at이 있다. |
| `POST /api/sessions` (사용량 수집 에이전트, 페어링 발급분) | 0035 이후 만료일이 생긴다(created+365d). 만료되면 401이 나고 재페어링이 필요하다. 수집 에이전트는 평문 파일 저장을 그대로 유지한다. 이번 범위 밖이고 미결 U4다. |
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

- `src/loopback_oauth.rs`(신설): `google_oauth/mod.rs:40-235`의 범용 부분을 추출한 것이다. google_oauth는 이 파일을 호출하도록 바꾸고, 동작은 그대로 유지한다.
- `src/lib.rs`: `mod` 선언 추가, `invoke_handler`에 커맨드 4개 추가(`:91-156`). `setup`에서 `locator::write()`를 호출하고 `hub_auth::renew::spawn_scheduler()`를 띄운다(`:60-90`).
- `capabilities/default.json`: **변경 없음.** 커스텀 커맨드만 추가하고 fs/shell 플러그인 권한을 늘리지 않는다.
- `Cargo.toml`: `keyring` 3.x에 feature 2개(§4 D5).
- 프론트엔드: `src/hubAuthApi.ts`(신설). `src/views/settings.ts`의 MCP 행(`:1118` 이하)과 `src/views/home.ts`의 hub 위젯(`:333-360`)에 "허브 연결 상태(연결됨/만료일/기기명), 연결, 해제"를 추가한다. hub 서버에 대해서는 기존 `mcp_login`(`claude mcp login` 터미널, `[A] mcp_manager/mod.rs:369-385`) 버튼을 숨긴다(OAuth 경로를 쓰지 않으므로).

### 7.2 상태 머신 (GUI)

| 상태 | 진입 조건 | UI | 전이 |
|---|---|---|---|
| NOT_CONNECTED | 키체인 항목 없음 | [허브 연결] | connect 성공 → CONNECTED |
| CONNECTED | 항목 있음, `self` 200 | 연결됨, 만료일 표시 | now≥renew_after → RENEW_DUE. 401 → REVOKED |
| RENEW_DUE | 갱신 창 진입 | (표시 없음, 백그라운드 처리) | 성공 → CONNECTED. 네트워크 실패 → RENEW_RETRYING |
| RENEW_RETRYING | 오프라인이나 5xx | "갱신 대기 중(만료 D-n)" | 1h마다 재시도. now≥expires_at → EXPIRED |
| SAVE_FAILED | 갱신은 성공했지만 키체인 쓰기나 read-back이 실패 | "키체인 저장 실패. 24시간 안에 해결하지 않으면 다시 연결해야 합니다" | 새 토큰을 메모리에 들고 1분마다 저장을 재시도(앱 실행 중에만). 성공 → CONNECTED |
| EXPIRED / REVOKED | 만료, 401, ALREADY_RENEWED | "다시 연결하세요" + 사유 | connect → CONNECTED |
| KEYCHAIN_DENIED | 읽기 거부나 잠김 | "키체인 접근이 거부되었습니다" + [다시 시도] + 해결 안내 | 재시도 성공 → 직전 상태 |

- 재연결할 때 기존 항목이 있으면 먼저 `DELETE /self`를 best-effort로 호출한 뒤 덮어쓴다. 같은 기기에 유효한 토큰이 여러 개 남지 않게 하기 위해서다.
- **갱신 단일화**: 단일 인스턴스 플러그인이 없어서 앱 2개가 동시에 뜰 수 있다. 동시 갱신은 ALREADY_RENEWED로 계열 전체를 폐기시키는 자기파괴를 낳으므로 프로세스 간 락이 필수다.
  - 락 파일 `<app_config_dir>/hub-renew.lock`을 `create_new`로 만든다. 내용은 pid와 시각이다. 120초가 지나면 stale로 보고 회수한다.
  - 락을 잡은 뒤 키체인을 다시 읽고, `renew_after`가 이미 미래이면 건너뛴다.
- **CLI 모드는 절대 갱신하지 않는다.** 여러 Claude 세션이 동시에 helper를 실행하면 같은 경합이 생기고, 10초 예산 안에 네트워크 왕복을 넣지 않기 위해서다. 그 대가로, 만료 30일 전부터 만료일까지 앱을 한 번도 켜지 않으면 만료되어 재연결해야 한다. CLI는 갱신 창에 들어가면 stderr로 안내한다.

### 7.3 CLI 모드 인터페이스 (계약)
- 호출: `<exe> --hub-auth-header` (인자 없음, stdin 무시)
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
| 1 | INTERNAL | 그 밖의 오류 | "맑은에이전트 CLI 내부 오류" |

- GUI 미기동 보장:
  - `main()` 맨 앞에서 분기하므로 `tauri::Builder`, 창, WebView, updater, 자율업무 스케줄러가 전혀 생성되지 않는다.
  - macOS에서는 번들 내부 바이너리를 직접 실행하고 NSApplication을 초기화하지 않으므로 Dock 아이콘이 생기지 않는다(실측 E3).
- 10초 보장:
  - 네트워크와 파일 쓰기가 없다.
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
  1. 실행파일을 찾는다(§4 D7).
  2. `spawn(exe, ['--hub-auth-header'], { stdio: ['ignore','pipe','pipe'], windowsHide: true, timeout: 8000 })`로 실행한다. **셸을 거치지 않고** 인자를 배열로 넘긴다.
  3. exit 0이면 stdout을 파싱한다. 키가 정확히 `Authorization` 하나이고, 값이 `/^Bearer mhd_[A-Za-z0-9_-]{43}$/`와 일치하면 그대로 stdout에 쓰고 exit 0으로 끝낸다.
  4. 그 밖의 경우: 앱의 stderr 첫 줄을 그대로 전달하거나, 앱을 찾지 못했으면 "맑은에이전트 앱이 설치되어 있지 않습니다. 앱을 설치하고 허브에 연결하세요."를 stderr에 쓰고 exit 1로 끝낸다. 이때 stdout은 비워 둔다.
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
| `server/dao/device-tokens.js` | `insert`/`rotateToken`에 만료 상한 단언. `buildRenewStatements()`(batch 2문장), `revokeChain()`, `listForUser` 컬럼 추가, `findById` 그대로 |
| `mcp/device-auth.js` | NULL 만료 거부, identity에 `deviceTokenId` 추가 |
| `server/middleware/device-token-auth.js` | 신설: `requireDeviceToken`(sessions.js에서 추출) |
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
| 갱신 중 네트워크 단절 | 기존 토큰을 유지하고 1h 후 재시도. 서버가 커밋했는데 응답만 유실된 경우, 다음 시도는 ALREADY_RENEWED가 되어 계열이 폐기되고 재연결해야 한다(드문 경우로 수용) | `renew.rs`, §12 R8 |
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
| R1 | 디스크, 백업, 동기화 폴더에서 토큰 유출 | OS 키체인(macOS Keychain 암호화, Windows DPAPI)에만 저장한다. 평문 파일은 없다. 서버는 해시만 저장한다 | 낮음 |
| R2 | 같은 사용자 권한의 악성 프로세스 | macOS는 키체인 ACL 덕분에 다른 앱이 읽으려 하면 프롬프트가 뜬다 | **Windows CredMan은 앱별 ACL이 없어 같은 사용자의 프로세스라면 읽을 수 있다.** 수용한다. 같은 권한의 악성코드는 Claude 세션 자체를 조종할 수도 있다. 웹 `/keys`의 last_used_at과 폐기로 대응한다 |
| R3 | macOS ad-hoc 서명이라 업데이트하면 cdhash가 바뀌고 키체인 ACL이 불일치 | 업데이트 뒤 GUI가 처음 시작할 때 "키체인 접근 확인"을 미리 실행한다. 사용자에게 사유를 먼저 보여 주고, 프롬프트에서 "항상 허용"을 누르도록 안내한다. CLI는 5s 데드라인을 넘기면 12로 끝내고 안내한다 | 업데이트 직후 앱을 켜지 않은 채 Claude만 쓰면 hub 연결이 실패할 수 있다. 실측 E9로 확정한다. 근본 해결책은 Developer ID 서명이다(미결 U3) |
| R4 | 로그나 출력에 토큰 노출 | CLI는 stdout 한 줄만 쓰고 stderr에는 토큰을 쓰지 않는다. 래퍼는 stdout을 그대로 전달하기만 하고 로그를 남기지 않는다. `mhd_` 접두사로 마스킹 대상을 쉽게 식별한다 | Claude Code 내부 디버그 로그에서의 헤더 취급은 우리가 통제할 수 없다 |
| R5 | desktop client_id 사칭(로컬 악성 앱이 동의를 유도) | PKCE와 루프백, 사용자의 명시적 동의, 동의 화면 경고 문구(§5.5), 감사 기록 `device_token.issued` | 사용자가 무심코 허용할 수 있다. 수용한다(현행 OAuth 90d refresh와 같은 수준) |
| R6 | 탈취한 토큰으로 MCP나 email_send 악용 | 수신자를 사내로 제한(§11), 레이트리밋, 즉시 폐기(웹/앱), 계정 비활성화 캐스케이드 | 폐기하기 전까지 최대 365일 동안 유효하다. 탐지는 사람이 last_used_at을 보는 수준이다 |
| R7 | locator 파일이나 `MALGN_AGENT_EXE` 조작으로 다른 실행파일 실행 | 절대경로와 파일명 검사. 위치는 사용자 쓰기 영역이다 | 조작할 수 있으면 이미 계정이 장악된 상태라 새 표면이 아니다 |
| R8 | 갱신으로 탈취 상태를 지속 | 갱신 창 30일, 토큰당 1회, 재갱신 시 계열 폐기 | 탈취자가 정상 앱보다 먼저 갱신해 자식 토큰 T를 받은 경우를 보자. 정상 앱이 부모 P의 24h 유예 안에 갱신을 시도하면 ALREADY_RENEWED가 되어 P와 T가 모두 폐기된다. **그러나 P가 이미 만료된 뒤라면** 앱은 401(REVOKED)만 받는다. 재연결 시의 `DELETE /self`도 만료된 P로는 인증되지 않으므로 **T가 최대 365일 살아남는다.** 웹 `/keys`의 "종류=데스크톱" 목록에 낯선 행이 보이는지는 사람이 확인해야 한다. 50인 규모에서는 수용하되 미결 U11로 남긴다 |
| R9 | 무기한 토큰 | D3의 세 겹과 0035 백필 | 없음 |
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
- 응답 노출 경계: 원문 토큰은 발급과 갱신 응답에만 1회 싣는다. self는 본인 email뿐이다. `/api/devices` 목록에 추가하는 열은 자격증명이 아니다
- 로직 강도 항목(레이트 임계, 감사 metadata 세부, 입력검증 세부)은 기존 값을 재사용하는 수준으로 두었다. 강화가 필요하면 서버의 `docs/security-plan.md`에 적재한다

---

## 14. 테스트 계획

### 14.1 서버 (vitest, 기존 스타일)
- `server/dao/device-tokens.test.js` (node:sqlite 실DDL, 기존 방식)
  - `insert`에 expiresAt이 없거나 365d+61s이면 throw하고, 365d면 통과한다
  - 0035를 적용하면 NULL 행만 `created_at+365d`(형식 `…T…Z`)로 바뀌고 기존 값은 그대로다. 적용 후 `new Date(expires_at)`가 유효한 날짜다
  - renew batch: 같은 부모로 순차 2회 → 두 번째는 changes 0. 부모 `expires_at = MIN(기존, grace)`. 자식이 user_id, device_id, oauth_client_id를 상속
  - `revokeChain`: 부모→자식→손자 3단이 모두 revoked, 상한 32
- `mcp/device-auth.test.js`(vi.mock, 기존 방식): `expires_at:null`이면 거부(신규), 과거면 거부, 미래면 통과, identity에 `deviceTokenId`
- `server/api/oauth.test.js`
  - desktop client → `expires_in=31536000`, `refresh_token` 없음, `mhd_` 접두사, 행 `oauth_client_id=desktop`, `renew_after = expires_at-30d`
  - plugin client와 DCR client → 기존 8h+refresh(회귀)
  - env 미설정이면 desktop 분기가 일어나지 않음(fail-closed)
  - body에 `expires_in`/`ttl`을 넣어도 무시됨
- `server/api/devices.test.js`(신설)
  - renew: 창 이전 409 TOO_EARLY, 비-desktop 403, 성공 200, 부모 재사용 시 409 + 자식 폐기 + 감사 1건, 만료/폐기/비활성 계정 401, 응답에 원문 토큰 1회
  - self GET은 본인 정보, DELETE는 체인 폐기
  - **경계 회귀**: JWT로 `/api/devices/self`를 호출하면 401(기기 토큰만 허용). 무인증 `GET /api/devices`는 401(JWT 게이트 유지). 무인증 `PUT /api/devices/renew`(다른 메서드)도 `use()`로 401. `DELETE /api/devices/self`가 `/:id` 핸들러로 가지 않음
- `server/index.test.js`: PUBLIC_PATHS에 정확한 문자열 2개만 추가됐는지 스냅숏으로 확인
- email: 코드를 바꾸지 않으므로 기존 `server/lib/email.test.js`를 회귀로 돌린다. `deviceAuthMiddleware`가 발급 경로와 무관하다는 점은 device-auth 테스트(oauth_client_id가 desktop인 행도 같은 identity shape)로 보증한다
- **실행 주의**: `vitest.config.js`의 `exclude: ['**/.claude/worktrees/**']`가 워크트리 안에서 실행할 때 테스트 전체를 걸러낼 수 있다. 착수할 때 `pnpm vitest list`로 수집 건수를 먼저 확인하고, 0건이면 `--exclude` 오버라이드나 임시 config로 실행한다(U8)

### 14.2 앱 (cargo test)
- `cli_mode`: `dispatch`가 정확한 플래그에서만 Some을 반환하고, `--hub-auth-header=x`, 추가 인자, 플래그 없음은 None. 메모리 store로 종료코드 0/10/11/12/13 매핑. stdout JSON이 정확히 한 키이고 한 줄. **stderr에 토큰 문자열이 없음**(출력 버퍼 검사). 5s 데드라인(블로킹 store를 주입하면 12)
- `renew::needs_renew` 경계(renew_after 직전, 같음, 직후). 락: 두 번째 획득 실패, 120s stale 회수
- 응답 처리 매핑: 401/409 ALREADY_RENEWED → 항목 삭제, TOO_EARLY → 일정 재설정, 5xx → 유지(HTTP 모킹은 기존 테스트 관례를 따르고 없으면 응답 파서 단위만 검증)
- `store`: JSON v1 직렬화 왕복, `hub` origin 불일치 → CORRUPT
- `loopback_oauth` 추출 후 기존 google_oauth 테스트 전부 통과(회귀)
- `#[ignore]` 실기 테스트: 실제 keyring 쓰기, 읽기, 삭제 왕복. mock 백엔드가 아닌지 확인(macOS/Windows 실기에서 `cargo test -- --ignored`)

### 14.3 플러그인
- `bin/hub-auth-header.mjs`: 해석 순서와 출력 검증 로직을 함수로 분리하고 `node --test`로 다음을 확인한다. 가짜 실행파일(스텁 스크립트)로 정상, 비정상 JSON, 추가 키, 종료코드 10, 타임아웃. 저장소의 테스트 러너 관례는 구현할 때 확인한다.

---

## 15. macOS e2e 실측 절차

준비: 서버 워크트리에서 로컬 D1에 마이그레이션을 적용한다(`pnpm run db:migrations:apply:local`). 로컬 서버를 기동하고 `.dev.vars`에 `TRUSTED_MALGN_DESKTOP_CLIENT_ID`를 넣는다(값은 문서에 남기지 않는다). 앱은 debug 빌드에 `MALGN_HUB_ORIGIN=<로컬>`을 준다. 플러그인은 워크트리를 `claude --plugin-dir`로 로드한다. 각 단계의 결과를 기록한다. 실패하면 그 자리에서 멈춘다.

| # | 단계 | 기대 |
|---|---|---|
| E1 | 수정한 plugin.json으로 `claude` 실행, `/mcp` | helper가 실행되고 malgnai-hub가 "connected"로 표시(**플러그인 headersHelper 실동작 확인, 가장 중요**) |
| E2 | 앱에서 [허브 연결] → 브라우저 로그인, 동의 | 동의 화면에 365일 안내. 키체인 항목 생성: `security find-generic-password -s com.malgnsoft.agent.hub -a device-token`(**`-w` 금지**, 값 출력 안 함) |
| E3 | `…/malgn-agent --hub-auth-header \| node -e 'console.log(Object.keys(JSON.parse(require("fs").readFileSync(0))))'`, `echo $?`, `time` | `[ 'Authorization' ]`, 0, 1초 미만, Dock 아이콘과 창 없음 |
| E4 | MCP 도구 호출(예: project_list) | 성공. 로컬 D1 `last_used_at` 갱신 |
| E5 | 웹 `/keys`에서 해당 기기 폐기 → 도구 호출 | 401 → helper 재실행 → 다시 401 → 연결 실패로 표시(무한 루프 없음) |
| E6 | 앱 [연결 해제] → CLI 실행 | exit 10과 안내. `/mcp`에 안내 문구가 표시되는지 기록 |
| E7 | 로컬 D1에서 expires_at을 now+29d로 조정 → 앱 재시작 | 새 행 생성, 부모의 `replaced_by_id` 설정과 만료 ≤24h, 키체인 값 교체, CLI가 새 토큰 출력 |
| E8 | 부모 토큰으로 `curl -X POST …/api/devices/renew`(토큰은 셸 변수로만 전달하고 이력에 남기지 않음) | 409 ALREADY_RENEWED, 자식까지 revoked, 감사 1건 |
| E9 | 앱을 재빌드(cdhash 변경)한 뒤 GUI를 실행하지 않고 CLI만 실행 | 키체인 프롬프트 발생 여부 기록. 무시하면 5s 후 12. "항상 허용" 후에는 프롬프트 없음 |
| E10 | 앱 자율업무(`claude -p`) 실행 | 헤드리스에서도 helper가 동작하고 hub 도구 호출 성공 |
| E11 | 앱 2개 동시 실행 + E7 조건 | 갱신이 1회만 일어나고 계열 폐기가 없음 |
| E12 | 래퍼가 ① exit 1(빈 stdout) ② `{}` 출력 각각일 때 `/mcp` 상태 | "failed"인지 "needs authentication"(OAuth discovery)인지 기록. D8 표 갱신(U2) |
| E13 | .app 이름을 바꿔 미설치를 흉내 | 래퍼의 "앱 미설치" 안내 |

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
- [ ] (W5) Claude Code에서 `/mcp` → malgnai-hub "connected", 도구 호출 성공
      - helper가 cmd/PowerShell 중 어느 셸로 실행되는지와 무관하게 `node "…\hub-auth-header.mjs"`가 동작하는가
- [ ] (W6) 업데이트 설치 후에도(재설치 포함) 자격 증명이 유지되고 CLI가 동작한다
- [ ] (W7) 앱 [연결 해제] 후 자격 증명 관리자에서 항목이 사라지고, Claude에 "앱에서 허브에 연결하세요" 안내가 뜬다
- [ ] (W8) 사용자 계정 이름에 한글·공백이 있어도 locator(`%APPDATA%\com.malgnsoft.agent\cli-path.json`) 경로가 동작한다
- [ ] (W9) 백신(Defender 등)이 `--hub-auth-header` 실행을 차단·지연시키지 않는다(10초 내 완료)
```

---

## 17. 롤아웃 순서

1. **서버** (호환 유지. 플러그인이 아직 OAuth여도 영향 없음): 0035 로컬 적용과 테스트 → devops 단계에서 원격 적용 → 코드 배포 → env `TRUSTED_MALGN_DESKTOP_CLIENT_ID` 등록. 반드시 0035를 먼저 적용한다(§6.1).
2. **앱 릴리스**: 연결 UI, CLI 모드, locator, 갱신. 직원들이 [허브 연결]을 한다. 이 시점에는 플러그인이 아직 OAuth를 쓰므로 두 경로가 함께 존재한다.
3. **실측**: E1~E13, W1~W9를 통과해야 한다.
4. **플러그인 릴리스**: oauth를 제거하고 headersHelper를 넣는다. 공지 문구는 "업데이트 전에 앱에서 허브에 연결하세요"다.
5. 이후 OAuth로 발급한 기존 토큰은 자연히 만료된다(access 8h, refresh 90d). 남은 plugin client OAuth grant를 일괄 폐기할지는 미결 U5다.

---

## 18. 미결 사항

| # | 항목 | 결정 시점 / 주체 |
|---|---|---|
| U1 | 플러그인 `headersHelper` 실동작(E1). 실패하면 §8.3 대안 A/B/C 중 선택 | 실측 후, PM |
| U2 | helper 실패나 `{}` 출력 시 Claude Code의 OAuth discovery 진입 여부(E12) → D8 표 갱신 | 실측 후 |
| U3 | macOS Developer ID 서명 도입(R3 근본 해결). 지금은 ad-hoc이라 업데이트 후 키체인 프롬프트 위험이 있다 | 사람 결정(비용, 계정) |
| U4 | 사용량 수집 에이전트: 0035 백필로 가장 오래된 토큰이 2027년 7~8월에 만료된다. 재페어링 공지로 갈지, 수집 에이전트가 앱 CLI(`--hub-auth-header`)를 쓰도록 바꿀지(평문 파일 제거 효과도 있음) | 2027-06 이전, 별도 작업 |
| U5 | 플러그인 전환 후 기존 plugin-client OAuth grant(90d refresh)를 일괄 폐기할지 자연 만료로 둘지 | 플러그인 릴리스 후 |
| U6 | `TRUSTED_MALGN_DESKTOP_CLIENT_ID`의 실제 값 생성과 등록 방식(기존 `TRUSTED_MALGN_AGENT_CLIENT_ID`와 같은 경로인지 대시보드에서 확인) | devops 단계 |
| U7 | `device_tokens.expires_at NOT NULL` 같은 DB 제약은 다음 테이블 재생성 때 함께 넣는다 | 후속 |
| U8 | 워크트리 안 vitest 수집이 exclude 규칙에 걸리는지 | 구현 착수 시 실측 |
| U9 | 서버 저장소 포인터: `docs/`가 gitignored라 커밋할 수 없다. 원본 로컬 `docs/design/`에 포인터 파일을 둘지, 서버 `CLAUDE.md` 정본 문서 표에 한 줄을 추가할지 | PM(병합 시) |
| U10 | Windows GUI 서브시스템 stdout 파이프(W2). 실패하면 Windows 한정 콘솔 헬퍼 바이너리 | 실측 후 |
| U11 | R8 잔여: 부모가 만료된 뒤 남은 탈취 자식 토큰. 후보는 (a) 재연결 성공 시 서버가 같은 user의 다른 desktop 토큰을 앱 확인 후 일괄 폐기하는 옵션 (b) 앱 UI에 "이 계정의 다른 데스크톱 연결 n개" 표시. 과설계 여부를 리뷰에서 판단 | 리뷰 시 |

---

## 19. 같이 바꿔야 하는 값 (정본과 사본)

| 값 | 정본 | 사본 / 영향 |
|---|---|---|
| 365일, 30일 창, 24h 유예 | `[S] server/lib/tokens.js` 상수 | 앱은 `renew_after`와 `expires_at` 응답값만 쓴다(하드코딩 없음). 동의 화면 문구의 "365일"은 authorize-context의 `token_lifetime_days`로 받는다 |
| desktop client_id | 서버 env `TRUSTED_MALGN_DESKTOP_CLIENT_ID` | `[A] src-tauri/src/hub_auth/consts.rs`. 두 값이 다르면 desktop 분기가 일어나지 않는다(fail-closed, 8h 일반 토큰도 아님. 미등록 client라 동의 단계에서 거부) |
| hub origin | `[P] plugin.json` url | `[A] consts.rs HUB_ORIGIN`, 키체인 JSON의 `hub` 검증 |
| 토큰 접두사 `mhd_`와 길이 | `[S] tokens.js generateDeviceToken` | `[P] bin/hub-auth-header.mjs` 출력 검증 정규식 |
| CLI 종료코드 표 | 이 문서 §7.3 | `[A] cli_mode.rs`, `[P] hub-auth-header.mjs` 안내 매핑 |
