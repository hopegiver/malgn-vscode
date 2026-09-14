# 앱링크(App Links) 설계

사내 개발자들이 쓰는 여러 웹 앱(사내 시스템, SaaS)을 링크로 모아, 사이드바에서 **OS 기본 브라우저로** 바로 여는 기능.

- 전체 후보 목록은 **설정 > 앱링크설정** 탭에서 관리한다(추가/수정/삭제/활성 토글).
- 사용자가 **활성(enabled)** 으로 체크한 것만 사이드바 메뉴에 노출된다.
- 링크 클릭 = 외부 브라우저 열기. 임베디드 웹뷰로 열지 않는다.

## 0. 범위와 비범위

**범위 밖(설계하지 않음, 구현하지 말 것)**

- 커스텀 SSO 토큰 주입 / 임베디드 인증 / 쿠키·세션 공유 / 구글 OAuth 토큰을 링크에 싣는 일체의 동작. 맑은에이전트의 구글 로그인은 타 앱 자동로그인을 만들어주지 않는다 — 브라우저에 남아 있던 구글 세션 덕분에 "자동로그인처럼 보이는" 효과가 우연히 날 뿐이다. **URL에 어떤 자격증명도 붙이지 않는다.**
- 사용 통계 / 즐겨찾기 순위 / 동기화 서버 / 팀 공유.
- 드래그 정렬, 폴더·카테고리, 아이콘(§1-2 제외 근거 참조).

**설계 근거의 출처** — 이 저장소에는 `docs/prd.md`가 없다(`docs/`에는 `design/`·`reviewer/`만 있음). 요구사항 출처는 사용자 직접 요청이다. 기능 자체가 "로컬 JSON CRUD + 외부 열기"라 경쟁사·현행방식 대비 기술적 차별점으로 끌어올 셀이 없으므로, PM 반려 에스컬레이션 없이 **표준 설계**로 진행한다. 대신 설계 밀도는 "이 코드베이스의 기존 규약과 어긋나지 않는가"에 집중했다(각 절의 선례 인용).

---

## 1. 데이터 모델

### 1-1. 링크 1건 (`AppLink`)

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `id` | string | O | 프론트가 `crypto.randomUUID()`로 발급. 수정 시 유지, 추가 시 새로 발급. |
| `name` | string | O | 사이드바·설정목록에 표시할 이름. trim 후 1~40자. |
| `url` | string | O | `https://` 또는 `http://`로 시작. trim 후 1~2048자. |
| `enabled` | bool | O | true면 사이드바에 노출. 기본값 true(추가 시). |

**끝. 4개뿐이다.** 아래는 검토 후 **뺀** 필드와 그 이유다.

| 뺀 필드 | 뺀 이유 |
|---|---|
| `icon` / `favicon` | 파비콘을 가져오려면 앱이 사내/외부 호스트로 네트워크 요청을 하게 된다 — 지금 이 앱이 하지 않는 동작(외부 네트워크 egress)을 링크 개수만큼 새로 만든다. 게다가 캐시·실패·만료 처리가 따라붙는다. 사이드바의 기존 `subList()`(`src/sidebar.ts:182`)는 **텍스트 라벨만** 그리므로 아이콘 없이도 기존 렌더러를 그대로 쓸 수 있다. 이모지를 직접 입력받는 방안도 필드가 하나 늘고 입력 검증이 애매해져 뺐다. |
| `category` / `group` | 50인 사내에서 링크는 많아야 10~20개다. 한 줄 목록으로 충분히 읽히고, 그룹을 도입하면 데이터 필드 + 설정화면 그룹 편집 UI + 사이드바 2단 중첩이 한꺼번에 붙는다. 개수가 실제로 불편해지면 그때 추가한다(§1-3의 위치 기반 순서 덕에 그때도 스키마 파괴 없이 추가 가능). |
| `sortOrder` (숫자) | 배열 순서가 곧 표시 순서다(§1-3). 별도 숫자 필드는 중복 정본이 되고, 값이 꼬였을 때(동점·구멍) 복구 규칙을 또 정해야 한다. |
| `description` / `memo` | 사이드바에도 목록에도 표시할 자리가 없다. 표시하지 않을 값을 저장하지 않는다. |
| `lastUsedAt` / `openCount` | 사용 통계는 명시적 범위 밖. 저장하는 순간 "왜 기록하나"를 설명해야 하는 개인정보성 데이터가 된다. |
| `openInNewWindow` 등 열기 옵션 | 열기 동작은 "OS 기본 브라우저" 하나로 고정이라 분기가 없다. |

### 1-2. id 발급

**프론트가 `crypto.randomUUID()`로 발급한다.** 선례: `src/views/autonomousTasks.ts:739`가 자율업무 id를 정확히 이 방식으로 만들고 Rust는 `upsert_task()`로 받기만 한다(`src-tauri/src/autonomy/config.rs:130`). 동일 규약을 따르면 "저장 커맨드가 생성된 엔티티를 되돌려줘야 하는" 비동기 왕복이 필요 없다.

백엔드는 id를 **생성하지 않고 검증만** 한다(§4). 손으로 편집한 파일의 id가 빈 문자열이거나 중복이면 거부한다.

### 1-3. 정렬 규칙

**파일 내 배열 순서 = 설정 목록 순서 = 사이드바 순서.** 새 링크는 배열 끝에 append.

- 이름순 자동 정렬을 쓰지 않는다 — 이름을 고쳤을 뿐인데 사이드바에서 항목이 튀어 오르는 것은 놀라운 동작이다.
- `enabled` 여부로 재정렬하지 않는다 — 설정 화면에서는 비활성 항목도 같은 자리에 남아야 체크를 켜고 끄기 쉽다. 사이드바는 이 순서를 유지한 채 `enabled === false`만 **필터링**한다(정렬이 아니라 걸러내기).
- 순서 변경 UI(위/아래 버튼, 드래그)는 MVP 범위 밖. 나중에 붙여도 **스키마 변경이 필요 없다**(배열 순서를 바꿔 전체 저장하면 끝 — §3의 전체목록 저장 커맨드가 이미 그것을 지원한다).

---

## 2. 저장 방식

> 저장 **위치**는 사람 승인이 필요한 분기다(§8-B). 아래 본문은 **권고안(별도 파일)** 기준으로 쓰여 있다. 분기가 다른 쪽으로 결정되어도 **§3의 커맨드 시그니처와 JSON 스키마는 바뀌지 않는다** — 바뀌는 것은 `store.rs`의 파일 경로 상수와 직렬화 대상 구조체뿐이다. 그래서 프론트는 이 분기의 결론을 기다리지 않고 착수할 수 있다.

### 2-1. 파일

- 경로: `~/.claude/malgn-agent-apps.json`
- 포맷:

```json
{
  "version": 1,
  "links": [
    { "id": "0b1f8a2c-...", "name": "사내 위키", "url": "https://wiki.example.internal", "enabled": true },
    { "id": "7d3e5c91-...", "name": "Jira",     "url": "https://example.atlassian.net", "enabled": false }
  ]
}
```

- `version`은 `#[serde(default = "default_version")]`로 1. 지금은 읽고 쓰기만 하고 분기에 쓰지 않는다(미래 마이그레이션의 자리만 확보).
- **이 파일은 개발자 개인 PC의 홈 아래에만 존재한다.** 사내 호스트명이 저장소에 커밋되지 않는다(§8-A가 이 성질을 깨뜨릴 수 있는 유일한 분기다).

### 2-2. 읽기 규칙 (`user_config.rs`의 3분기를 그대로 따른다)

`src-tauri/src/config/user_config.rs:84` `load_from_path()`의 규칙과 동일하게:

| 상황 | 동작 |
|---|---|
| 파일 없음 / 읽기 실패 | **정상**. 빈 목록(`links: []`)으로 시작. `fileExists:false`, `ok:true`. |
| 정상 JSON | 사용. 모르는 키는 조용히 무시(`#[serde(deny_unknown_fields)]`를 **절대 붙이지 않는다** — 붙이면 미래 필드 하나에 파일 전체가 파싱 실패한다. `autonomy/config.rs:1-8`의 명시 원칙). |
| JSON 손상 | `Err` → 커맨드는 `ok:false` + `error`로 프론트에 그대로 올린다. 조용한 기본값 폴백 금지. **링크 목록은 빈 배열로 내려간다(fail-closed)** — 사이드바에 아무것도 뜨지 않고 설정 화면에 원본 에러 메시지가 뜬다. |

**항목 단위 검증은 읽기 시점에도 돈다.** 손으로 편집해 `file:///`이나 빈 이름이 들어간 항목은 `links`에서 제외하고 `warnings`에 사유를 담아 내려보낸다 — `malgn_agent_config_get()`이 `validate_workspace_entries()`로 하는 것과 같은 구조(`config/mod.rs:101`).

> **알려진 귀결(의도한 것):** warnings로 걸러진 항목이 있는 상태에서 사용자가 설정 화면에서 저장을 누르면, 화면에 보이던 유효 항목만 저장되어 **걸러진 항목은 파일에서 사라진다.** 완화책은 두 가지가 이미 걸려 있다 — ① 저장 직전 `.malgn-bak` 롤링 1세대 백업 ② 설정 화면 상단에 warnings를 경고 배너로 항상 표시(사용자가 저장 전에 본다). workspaces 편집이 이미 같은 성질을 갖고 운영 중이므로 새로운 위험이 아니다.

### 2-3. 쓰기 규칙

`user_config.rs:185` `save_to_path()`와 동일한 3단 절차:

1. 상위 디렉터리 생성(`~/.claude/`가 없으면 만든다)
2. 기존 파일이 있으면 `~/.claude/malgn-agent-apps.json.malgn-bak`로 롤링 1세대 백업
3. 임시 파일 + `fs::rename` 원자적 교체

여기에 **otel 원칙 1개를 추가로 적용한다**: 기존 파일이 존재하는데 **파싱에 실패하면 즉시 `Err`, 아무것도 쓰지 않는다**(`otel_settings.rs:321` 저장 알고리즘 1단계). 손상된 파일을 덮어써서 사용자가 손으로 고칠 기회를 없애지 않는다. 에러 메시지는 파일 경로를 포함해 "직접 고치거나 파일을 지운 뒤 다시 시도하라"고 안내한다.

- 캐시 없음. `user_config.rs:106`의 mtime 캐시는 스케줄러가 10초마다 읽기 때문에 존재한다. 앱링크는 로그인 직후 1회 + 저장 직후 + 열기 시점에만 읽으므로 캐시 기계장치를 복사하지 않는다.
- `static APP_LINKS_FILE_LOCK: Mutex<()>`로 save/open의 파일 접근 구간을 직렬화한다(`autonomy/config.rs:65` `AUTONOMY_FILE_LOCK` 선례). 백그라운드 스레드는 이 파일을 건드리지 않지만, 토글 연타로 save가 겹칠 수 있다.

### 2-4. 원자적 쓰기 유틸 — 3번째 소비자이므로 공용 모듈로 추출

`write_atomically()` / `backup_path_for()`는 현재 `config/user_config.rs`와 `otel_settings.rs`에 **동일한 코드로 2벌** 있고, `user_config.rs:160-162`에 그 이유가 명시돼 있다: *"소비자가 2곳뿐이라 아직 공용 모듈로 뽑지 않는다."* 앱링크가 3번째 소비자가 되면서 그 조건이 해제된다.

**지시(backend-dev):**

1. `src-tauri/src/fs_atomic.rs`를 새로 만들고 두 함수를 **한 글자도 바꾸지 않고** 옮긴다. 단, 하드코딩된 파일명 폴백(`"malgn-agent.json"` / `"settings.json"`)만 인자 없는 일반 폴백(`"config.json"`)으로 통일한다 — 이 폴백은 `path.file_name()`이 `None`일 때만 쓰이고, 두 모듈의 실제 경로에서는 결코 발생하지 않으므로 동작 변화가 없다.
2. 기존 두 모듈은 **자체 정의를 지우고 `use crate::fs_atomic::{write_atomically, backup_path_for};`로 재사용**한다(기존 호출부·테스트는 손대지 않는다).
3. `cargo test`가 기존 250개 그대로 통과해야 한다 — 이 추출에서 테스트를 수정해야 한다면 추출이 잘못된 것이다.

**대안(복사 3벌)의 트레이드오프:** 추출을 안 하면 이번 작업의 diff가 앱링크 파일 안에만 갇혀 회귀 위험이 0에 가깝다. 그러나 백업·원자적 교체는 "고쳐야 할 때 한 곳만 고쳐야 하는" 안전 코드이고, 코드베이스가 이미 추출 조건을 문서로 예고해 뒀다. 추출을 택하되 위 3번(테스트 무수정 통과)을 안전장치로 건다.

---

## 3. Tauri 커맨드 (병행 착수의 계약)

### 3-1. 모듈 배치

```
src-tauri/src/app_links/
  mod.rs     — 커맨드 3개 + 상태 구조체 (~150줄 예상)
  store.rs   — 스키마·파일 IO·순수 검증 함수 + #[cfg(test)] (~300줄 예상)
src-tauri/src/fs_atomic.rs  — §2-4에서 추출 (~40줄)
```

`config/{mod.rs, user_config.rs}` 분할과 같은 구조(커맨드·상태 / 스키마·IO·순수함수). Rust 전 파일 1,000줄 미만 규칙을 여유 있게 만족한다.

`lib.rs` 변경: `mod app_links;` `mod fs_atomic;` 2줄 + `invoke_handler!`에 3줄 추가. 그 외 `lib.rs` 수정 없음.

### 3-2. 타입

```rust
// app_links/store.rs

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppLink {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppLinksFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub links: Vec<AppLink>,
}

// ---- 검증 임계값 정본. 이 4개 상수는 이 파일에만 존재한다. ----
pub(crate) const MAX_LINKS: usize = 50;
pub(crate) const MAX_NAME_LENGTH: usize = 40;
pub(crate) const MAX_URL_LENGTH: usize = 2048;
pub(crate) const ALLOWED_SCHEMES: [&str; 2] = ["https://", "http://"];
```

```rust
// app_links/mod.rs

#[derive(Serialize, Clone, Debug)]
pub struct AppLinksLimits {
    #[serde(rename = "maxLinks")]       pub max_links: usize,
    #[serde(rename = "maxNameLength")]  pub max_name_length: usize,
    #[serde(rename = "maxUrlLength")]   pub max_url_length: usize,
    #[serde(rename = "allowedSchemes")] pub allowed_schemes: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct AppLinksStatus {
    pub ok: bool,
    pub error: Option<String>,
    #[serde(rename = "filePath")]   pub file_path: String,
    #[serde(rename = "fileExists")] pub file_exists: bool,
    pub links: Vec<AppLink>,
    pub warnings: Vec<String>,
    pub limits: AppLinksLimits,
}
```

`limits`를 내려보내는 이유는 `configApi.ts:5-10`에 이미 적힌 규약과 같다 — **프론트는 이 숫자를 하드코딩하지 않고 이 필드에서만 읽어** 입력 힌트·`maxlength` 속성에 쓴다. 임계값을 바꿀 때 고칠 파일이 1개가 되도록.

### 3-3. 커맨드 시그니처 (Rust)

```rust
/// 항상 성공한다(손상 파일도 ok:false를 담은 정상 응답으로 내려간다).
/// malgn_agent_config_get()과 동일한 "상태 조회는 Err를 던지지 않는다" 규약.
#[tauri::command]
pub fn app_links_get() -> AppLinksStatus;

/// 전체 목록 치환 저장. 추가·수정·삭제·토글·순서변경이 전부 이 하나를 쓴다.
/// 검증 실패는 전량 거부(부분 저장 없음). 성공 시 저장 직후 상태를 그대로 반환해
/// 프론트가 재조회 없이 최신 값을 받는다(malgn_agent_config_save와 동일 패턴).
#[tauri::command]
pub fn app_links_save(links: Vec<AppLink>) -> Result<AppLinksStatus, String>;

/// 저장된 링크를 OS 기본 브라우저로 연다. 프론트는 URL이 아니라 id만 넘긴다.
#[tauri::command]
pub fn app_links_open(app: tauri::AppHandle, id: String) -> Result<(), String>;
```

`app_links_open`의 내부 순서(고정):

1. 파일 로드 → 실패면 `Err`(손상된 설정으로는 아무것도 열지 않는다, fail-closed)
2. `id`로 항목 검색 → 없으면 `Err("해당 앱링크를 찾을 수 없습니다. 목록을 새로고침해 주세요.")`
3. 찾은 항목의 `url`을 **저장 때와 동일한 `validate_url()`로 다시 검증** → 실패면 `Err`(손으로 편집된 파일 방어)
4. `app.opener().open_url(url, None::<&str>)` (`google_oauth/mod.rs:231-234`와 동일 호출)
5. 실패 시 `Err("브라우저를 열지 못했습니다: {e}")`

> `enabled == false`인 항목도 id로 열 수 있다(에러 아님). 설정 화면에서 "테스트로 열어보기"를 나중에 붙일 여지를 막지 않기 위함이고, 비활성 항목을 여는 것 자체는 사용자 자신의 링크이므로 위험이 없다.

### 3-4. 커맨드 시그니처 (TypeScript — `src/appLinksApi.ts` 신규)

```ts
import { invoke } from '@tauri-apps/api/core';

export interface AppLink {
  readonly id: string;
  readonly name: string;
  readonly url: string;
  readonly enabled: boolean;
}

export interface AppLinksLimits {
  readonly maxLinks: number;
  readonly maxNameLength: number;
  readonly maxUrlLength: number;
  readonly allowedSchemes: readonly string[];
}

export interface AppLinksStatus {
  readonly ok: boolean;
  readonly error: string | null;
  readonly filePath: string;
  readonly fileExists: boolean;
  readonly links: readonly AppLink[];
  readonly warnings: readonly string[];
  readonly limits: AppLinksLimits;
}

export async function fetchAppLinks(): Promise<AppLinksStatus> {
  return invoke<AppLinksStatus>('app_links_get');
}

/** 검증 실패 시 reject된다 — 에러 메시지는 백엔드 원문을 그대로 사용자에게 보여준다. */
export async function saveAppLinks(links: readonly AppLink[]): Promise<AppLinksStatus> {
  return invoke<AppLinksStatus>('app_links_save', { links });
}

/** URL이 아니라 id만 넘긴다(§6 보안). */
export async function openAppLink(id: string): Promise<void> {
  return invoke<void>('app_links_open', { id });
}
```

### 3-5. 트레이드오프 — 전체목록 치환 저장 vs 항목별 CRUD

- **선택:** `app_links_save(links)` 하나로 add/edit/delete/toggle/reorder 전부 처리.
- **대안:** 자율업무처럼 `save_task` / `delete_task` / `set_enabled` 3개로 분리(`autonomy/mod.rs:90-132`).
- **선택 이유:** 앱링크는 최대 50건짜리 단일 화면 편집 대상이고, 편집 지점이 설정 탭 한 곳뿐이다. 커맨드 1개면 등록할 핸들러·TS 래퍼·테스트가 1/3이고, 순서 변경 기능을 나중에 붙일 때 **새 커맨드가 필요 없다**. 자율업무가 항목별 커맨드를 쓰는 이유는 백그라운드 스케줄러 스레드가 같은 파일을 동시에 읽고 쓰기 때문인데(`AUTONOMY_FILE_LOCK` 주석), 앱링크에는 그런 동시 기록자가 없다.
- **포기한 것:** 두 화면이 동시에 편집할 때의 lost update. 설정 탭 하나에서만 편집하므로 현실적 경로가 없다.
- **감당 방안:** 프론트는 항상 마지막 `AppLinksStatus.links`에서 파생한 배열을 보내고, 저장 응답으로 상태를 통째로 교체한다(낙관적 갱신 금지 — `state.ts:133-135`의 자율업무 규약과 동일). 토글 1개를 바꿔도 전체를 보낸다.

---

## 4. 입력 검증

### 4-1. 정본은 Rust 한 곳 — `app_links/store.rs`

프론트는 **UX용 사전 체크만** 한다(빈 이름/빈 URL이면 토스트 후 제출 중단, `maxlength` 속성). 프론트 체크는 권위가 없고, 백엔드가 거부하면 **에러 원문을 그대로** 사용자에게 보여준다(`dom.ts:78-81`: "원인 경로가 담긴 원본 에러 메시지를 그대로 보여주는 것이 이 앱의 강점"). 프론트에 스킴 목록·길이 상수를 **복제하지 않는다**(§3-2 `limits`로 받아 쓴다).

### 4-2. 정규화(조용히 수행, 에러 아님)

- `name`, `url` 양끝 공백 trim.

### 4-3. 거부 규칙(에러 — 전량 거부, 조용한 드롭 없음)

| # | 규칙 | 에러 메시지(예) |
|---|---|---|
| V1 | `links.len() > MAX_LINKS(50)` | `앱링크는 최대 50개까지 등록할 수 있습니다.` |
| V2 | `id`가 빈 문자열이거나 64자 초과, 또는 `[A-Za-z0-9-]` 외 문자 포함 | `앱링크 식별자 형식이 올바르지 않습니다.` |
| V3 | `id` 중복 | `'{name}': 식별자가 중복되었습니다.` |
| V4 | trim 후 `name`이 빈 문자열 | `이름은 비워 둘 수 없습니다.` |
| V5 | `name` 길이 > 40 (문자 수 기준, `chars().count()`) | `'{name}': 이름은 40자 이내여야 합니다.` |
| V6 | trim 후 `url`이 빈 문자열 | `'{name}': 주소는 비워 둘 수 없습니다.` |
| V7 | `url` 길이 > 2048 (바이트) | `'{name}': 주소가 너무 깁니다(최대 2048자).` |
| V8 | `url`이 `https://` / `http://` 중 하나로 시작하지 않음(소문자 비교) | `'{name}': 주소는 https:// 또는 http://로 시작해야 합니다.` |
| V9 | 스킴 뒤 호스트부가 비어 있음(`https://`만 입력 등) | `'{name}': 주소에 호스트가 없습니다.` |
| V10 | `url`에 공백·제어문자(`c.is_whitespace() \|\| c.is_control()`) 포함 | `'{name}': 주소에 공백이나 제어문자가 들어 있습니다.` |
| V11 | trim 후 `url` 완전 일치 중복 | `'{name}': 이미 등록된 주소입니다.` |

**허용 스킴을 `https`/`http` 둘로 정한 근거와 대안:** `opener:default`의 `allow-default-urls`는 `mailto:`·`tel:`도 허용하지만, 이 기능은 "웹 앱 열기"이므로 둘을 제외해 표면을 더 좁힌다. `file:`·`javascript:`·커스텀 스킴은 명시적으로 거부된다(V8이 화이트리스트이므로 자동). `http://`를 남긴 이유는 TLS가 없는 사내 레거시 시스템이 실재하기 때문이다 — 막으면 정당한 사용을 못 한다. 대신 §5 UI에서 `http://` 링크 옆에 "암호화되지 않음" 힌트를 조용히 표시한다(차단 아님).

**이름 중복은 허용한다.** 같은 이름 둘이 사이드바에 뜨는 것은 사용자가 곧바로 알아보고 스스로 고칠 수 있는 혼란이고, 규칙을 하나 줄이는 편이 낫다. URL 중복(V11)만 거부하는 이유는 그것이 **실수로 두 번 추가한** 경우의 신호이기 때문이다.

**URL 파서를 쓰지 않는 이유:** `url` crate를 새 의존성으로 들이면 빌드에 트랜지티브 의존성(idna 등)이 붙는다. V8~V10의 접두 검사 + 호스트부 비어있지 않음 + 공백/제어문자 금지로 이 기능이 필요로 하는 안전성(스킴 화이트리스트, 셸/헤더 인젝션 문자 배제)은 충족된다. `otel_settings.rs:257`도 엔드포인트 URL을 정확히 같은 방식(접두 검사)으로 검증하고 있다 — 코드베이스 선례와 일치.

### 4-4. 순수 함수 분해(테스트 대상, §7과 1:1)

```rust
pub(crate) fn normalize_link(link: AppLink) -> AppLink;                       // trim만
pub(crate) fn validate_url(url: &str) -> Result<(), String>;                  // V7~V10
pub(crate) fn validate_link(link: &AppLink) -> Result<(), String>;            // V2,V4,V5,V6 + validate_url
pub(crate) fn validate_links(links: &[AppLink]) -> Result<Vec<AppLink>, String>; // V1,V3,V11 + 위 전부 (저장 경로)
pub(crate) fn sanitize_loaded_links(links: Vec<AppLink>) -> (Vec<AppLink>, Vec<String>); // 읽기 경로: 유효/warnings 분리
```

저장 경로는 `validate_links`(전량 거부), 읽기 경로는 `sanitize_loaded_links`(걸러내고 warnings) — **검증 규칙 자체는 `validate_link` 한 곳**을 양쪽이 공유한다.

---

## 5. 설정 탭 UI 구조안

### 5-1. 새 뷰 파일로 분리한다 — `src/views/appLinks.ts`

`src/views/settings.ts`는 이미 **1,138줄**이고 6개 패널이 한 파일에 쌓여 있다. 여기에 CRUD 패널 하나를 더 넣으면 1,400줄을 넘긴다.

**선례가 이미 있다:** `settings.ts:6`은 마켓플레이스 탭의 로직을 자기 안에 두지 않고 `import { loadCatalog, loadMarketplaces } from './catalog'`로 다른 뷰 파일에 위임한다. 같은 패턴을 쓴다.

`settings.ts`의 변경량은 **정확히 3줄**이다:

```ts
import { renderAppLinksPanel } from './appLinks';           // (1) import
const TAB_META = [ ..., { key: 'applinks', label: '앱링크설정' } ];  // (2) 라벨
else if (tab === 'applinks') body = renderAppLinksPanel();  // (3) 디스패치
```

`src/views/appLinks.ts`가 export하는 것:

```ts
export async function loadAppLinks(): Promise<void>;   // state.appLinks 채우기 (main.ts·sidebar가 함께 사용)
export function renderAppLinksPanel(): HTMLElement;    // 설정 탭 본문
```

### 5-2. 화면 구성

```
설정  /  앱링크설정

[!] 경고 배너 — status.warnings가 있을 때만 (예: "'구버전링크': 주소는 https://...")
[!] 오류 배너 — status.ok === false일 때 errorBlock(status.error, 다시 시도)
                 (이 상태에서는 아래 목록·추가 버튼을 모두 비활성화한다)

사이드바에 노출할 링크를 체크하세요.  (12 / 50)              [ + 링크 추가 ]

┌──────────────────────────────────────────────────────────────────────┐
│ [토글]  사내 위키                                     [수정]  [삭제]  │
│         https://wiki.example.internal                                │
├──────────────────────────────────────────────────────────────────────┤
│ [토글]  그룹웨어            암호화되지 않음            [수정]  [삭제]  │
│         http://gw.example.internal                                   │
└──────────────────────────────────────────────────────────────────────┘

저장 위치: /Users/…/.claude/malgn-agent-apps.json     ← status.filePath 그대로
```

- **행 렌더**: 기존 설정 화면의 목록 행 클래스를 재사용한다. 토글은 `dom.ts:56` `toggleSwitch(checked, onChange)`.
- **빈 상태**: 목록 대신 안내문 1줄("아직 등록된 앱링크가 없습니다. 자주 쓰는 사내 시스템·SaaS 주소를 추가하면 사이드바에서 바로 열 수 있습니다.") + 추가 버튼.
- **저장 타이밍**: 토글 변경·추가·수정·삭제 **각각이 곧바로 `saveAppLinks(전체목록)`** 을 호출한다. 별도 "저장" 버튼 없음(체크박스 화면에 저장 버튼을 따로 두면 안 누르고 나가는 사고가 난다). 호출 중에는 해당 행을 비활성화하고, 성공 시 `showToast`, 실패 시 `showToast(에러 원문)` + 상태 롤백(응답 상태로 통째 교체).
- **추가/수정 모달**: `dom.ts:118` `createModalOverlay` + `modal-box/modal-header/modal-close-btn/modal-body` 구조. `views/autonomousTasks.ts:770` `renderTaskFormModal`을 형태 그대로 따른다(배경 클릭·ESC·닫기·취소 4경로 닫힘). 폼 필드는 **이름 / 주소** 2개뿐 + 힌트 1줄("https:// 또는 http:// 로 시작하는 주소를 입력하세요").
- **삭제**: `dom.ts:143` `confirmDialog(\`'${name}' 앱링크를 삭제할까요?\`, { danger: true })`. `window.confirm()`은 이 앱에서 동작하지 않는다(해당 주석 참조).
- **`http://` 힌트**: 목록 행에서 URL이 `http://`로 시작하면 회색 작은 글씨로 "암호화되지 않음"을 붙인다. 경고 배너가 아니라 조용한 라벨이다.

### 5-3. 상태 slice (`src/state.ts`)

```ts
appLinks: {
  status: AppLinksStatus | null;
  loading: boolean;
  error: string | null;   // 커맨드 호출 자체가 실패한 경우(Tauri IPC 부재 등)
  loaded: boolean;
  saving: boolean;
};
```

초기값: `{ status: null, loading: false, error: null, loaded: false, saving: false }`. `otel`·`malgnAgentConfig` slice와 같은 모양이다. 파생값(활성 링크 목록)은 저장하지 않고 렌더 시점에 `status.links.filter(l => l.enabled)`로 계산한다 — 정본 하나.

---

## 6. 사이드바 노출 방식

### 6-1. 형태 — 기존 `navGroup` 재사용, 라우트 없음

`src/sidebar.ts:201` `navGroup(spec)`은 `subItems: { label, active, onClick }[]`을 받는다. 앱링크는 여기에 **정확히 맞는다**: `label = link.name`, `active = false`(항상), `onClick = () => void handleOpenAppLink(link)`. 새 렌더 함수를 만들지 않는다.

```ts
const appLinksGroup = navGroup({
  label: '앱링크',
  active: false,                      // 대응하는 라우트가 없다
  expanded: state.sidebar.appLinksExpanded,
  onToggle: () => { state.sidebar.appLinksExpanded = !state.sidebar.appLinksExpanded; notifyChange(); },
  subItems: enabledLinks.length > 0
    ? enabledLinks.map((l) => ({ label: l.name, active: false, onClick: () => void openLink(l) }))
    : [{ label: '앱링크설정에서 추가 →', active: false, onClick: () => navigate('#/settings/applinks') }],
});
```

**라우트를 만들지 않는 이유:** 라우트는 "전환될 화면"이 있을 때만 의미가 있다. 앱링크 클릭은 외부 브라우저를 열 뿐 이 앱의 화면을 바꾸지 않는다. 해시를 바꾸면 ① 뒤로가기 히스토리가 오염되고 ② `parseRoute()`가 처리할 수 없는 종류의 상태가 생기며 ③ 새로고침 시 같은 링크가 다시 열리는 부작용까지 생긴다. `Route` 타입·`parseRoute()`는 **손대지 않는다**(단, `SettingsTab` 확장은 필요 — §9).

**배치 위치:** `mainItems` 배열에서 `개발 환경` 다음, `설정` 그룹 앞. 사이드바 주석(`sidebar.ts:27`)의 "사용 빈도순 배치" 방침에 따르되, 검증되지 않은 신규 기능이므로 상단이 아닌 하단부에 둔다. **1줄 순서 변경이므로 사용 후 얼마든지 조정 가능** — 승인 분기로 올리지 않는다.

**기본 펼침 상태:** `state.sidebar.appLinksExpanded = true`(초기값). `navGroup`은 `expanded || active`로 펼침을 계산하는데 이 그룹은 `active`가 영원히 false라, 기본값을 false로 두면 아무도 이 기능을 발견하지 못한다. 활성 링크를 한눈에 보여주는 것이 이 기능의 전부다.

### 6-2. 활성 링크 0건일 때

그룹을 **숨기지 않는다.** 서브 항목 자리에 클릭 가능한 안내 1건("앱링크설정에서 추가 →")을 넣어 `#/settings/applinks`로 보낸다. 그룹을 통째로 숨기면 한 번도 설정하지 않은 사용자는 기능의 존재 자체를 모른다. 기존 `sidebar-subnav-empty` 클래스(로딩 표시용)와 달리 이 항목은 **클릭 가능**해야 하므로 `subList`의 일반 항목으로 넣는다.

목록을 아직 못 불러온 동안(`!state.appLinks.loaded`)은 같은 자리에 `불러오는 중…`을 비클릭 항목으로 표시한다 — 프로젝트/세션 그룹이 이미 하는 방식(`sidebar.ts:118`).

### 6-3. 로딩 시점

`src/main.ts`의 `handleNavigation()` 프리로드 목록에 한 줄 추가:

```ts
if (!state.appLinks.loaded && !state.appLinks.loading) void loadAppLinks();
```

`loadProjects()`·`loadSessions()` 등 10개가 이미 같은 자리에 같은 가드로 있다(`main.ts:160-169`). 사이드바가 전 화면에 상시 렌더되므로 탭 진입을 기다릴 수 없다.

### 6-4. 열기 실패 처리

```ts
async function openLink(link: AppLink): Promise<void> {
  try {
    await openAppLink(link.id);
  } catch (err) {
    showToast(err instanceof Error ? err.message : `'${link.name}'을(를) 열지 못했습니다`);
  }
}
```

토스트로 그친다 — 사이드바 클릭에 모달·배너를 띄우지 않는다. 실패 원인 중 "목록이 오래됨"(다른 창에서 삭제)은 백엔드 메시지가 새로고침을 안내한다(§3-3).

---

## 7. 테스트 가능 표면

전부 `src-tauri/src/app_links/store.rs`의 `#[cfg(test)] mod tests`에 둔다. 순수 함수 + `std::env::temp_dir()` 임시 경로 방식(`user_config.rs:279` `temp_subdir` 헬퍼 그대로)으로 실제 홈 디렉터리를 건드리지 않는다.

**검증 순수 함수 (11)**

| # | 테스트 | 기대 |
|---|---|---|
| T1 | `validate_url("https://a.example.com")` | Ok |
| T2 | `validate_url("http://a.example.com")` | Ok |
| T3 | `validate_url("mailto:x@y.com")` / `"tel:123"` | Err (V8) |
| T4 | `validate_url("file:///etc/passwd")` / `"javascript:alert(1)"` | Err (V8) |
| T5 | `validate_url("wiki.example.com")` (스킴 없음) | Err (V8) |
| T6 | `validate_url("https://")` | Err (V9 호스트 없음) |
| T7 | `validate_url("https://a.com/ b")` / `"https://a.com/\nX"` | Err (V10) |
| T8 | `validate_url(&format!("https://a.com/{}", "x".repeat(3000)))` | Err (V7) |
| T9 | `validate_link` — 빈 이름 / 41자 이름 / 빈 id / `id="a b"` | 각각 Err |
| T10 | `validate_links` — 동일 URL 2건 / 동일 id 2건 | 각각 Err (V11, V3) |
| T11 | `validate_links` — 51건 | Err (V1) |

**정규화 (2)**

| # | 테스트 | 기대 |
|---|---|---|
| T12 | `normalize_link` — `name="  위키  "`, `url=" https://a.com "` | 양쪽 trim됨 |
| T13 | `validate_links` — trim 후에야 유효해지는 입력이 통과 | Ok |

**읽기 경로 (4)**

| # | 테스트 | 기대 |
|---|---|---|
| T14 | `load_from_path` — 파일 없음 | Ok, `links` 비어 있음 |
| T15 | `load_from_path` — `{ this is not valid json` | **Err** |
| T16 | `load_from_path` — 모르는 키(`"theme":"dark"`, 링크 항목 안의 `"icon":"x"`)가 있는 정상 JSON | Ok, 알려진 필드 정상 파싱 |
| T17 | `sanitize_loaded_links` — 유효 2 + `file://` 1 + 빈 이름 1 | 유효 2건만 반환, warnings 2건 |

**쓰기 경로 (3)**

| # | 테스트 | 기대 |
|---|---|---|
| T18 | `save_to_path` → `load_from_path` 라운드트립 | id/name/url/enabled/순서 동일 |
| T19 | 두 번째 저장 시 `.malgn-bak` 생성 + 그 내용이 **첫 번째** 저장 값 | 통과 |
| T20 | 기존 파일이 파싱 불가일 때 저장 시도 | **Err** + 파일 바이트가 원본과 완전 동일 (`otel_settings.rs:457` 회귀 테스트와 같은 형태) |

**열기 경로 (1)**

| # | 테스트 | 기대 |
|---|---|---|
| T21 | `find_link_for_open(&links, "없는-id")` | Err. (`open_url` 자체는 AppHandle이 필요해 단위테스트 대상이 아니다 — 조회+재검증까지를 순수 함수로 떼어내 그 부분만 덮는다.) |

**회귀 조건:** 기존 250개 테스트가 **수정 없이** 통과해야 한다. §2-4의 `fs_atomic` 추출이 이 조건을 깨면 추출을 되돌리고 3번째 복사본으로 간다.

---

## 8. 보안 검토

Skill `domain-backend-api-security`의 "언제 정하는가" 절을 항목별로 대조한 결과. 이 기능은 HTTP API가 아니라 **로컬 단일 사용자 Tauri IPC**라 대부분이 N/A지만, 대응물이 있는 항목은 값으로 확정했다.

| (a) 구조적 결정 항목 | 이 기능에서의 결론 |
|---|---|
| 인증 게이트 위치 / 공개 경로 경계 | **N/A(HTTP 없음).** 대응물은 `capabilities/default.json`의 권한 표면이다 → **변경 없음**으로 확정. `core:default` + `opener:default` 그대로, fs/shell 범용 플러그인 추가 금지. 새 능력은 전부 커스텀 Rust 커맨드로만 노출한다. |
| 인가의 축(역할) | **N/A.** 로컬 단일 사용자, 역할 개념 없음. 도입하지 않는다. |
| 소유권/테넌트 키 컬럼의 존재 | **N/A.** 다중 주체 데이터가 없다(파일 전체가 그 PC 사용자 소유). |
| 자격증명 전달 방식 / CORS | **N/A이자 명시적 비범위.** 이 기능은 어떤 자격증명도 다루지 않는다 — URL에 토큰·쿠키·OAuth 산출물을 싣지 않는다. |
| 민감 데이터의 저장 형태 | **확정:** 평문 JSON, 사용자 홈 아래(`~/.claude/`)에만. 사용자가 토큰이 박힌 URL을 붙여넣을 수 있으므로 **URL 원문을 stdout/stderr에 절대 로깅하지 않는다.** `app_links_open` 실패 메시지도 URL이 아니라 **링크 이름**으로 사람을 안내한다. |
| 응답 노출 경계 | `app_links_get`이 전체 목록을 렌더러에 내려주지만, 그 렌더러가 곧 이 목록의 편집 주체다 — 노출 경계가 넓어지지 않는다. |
| 최소 요건(fail-closed / 인젝션) | **확정:** ① 설정 파일 손상 시 링크 0건 + 열기 거부(통과 아님). ② 저장 시 스킴 **화이트리스트**(블랙리스트 아님). ③ 셸을 거치지 않는다 — `Command::new("open").arg(url)` 류를 **쓰지 않고** `app.opener().open_url()`만 쓴다(OS 명령 인젝션 경로 자체를 만들지 않는다). |

| (b) 로직 강도 항목 | 처리 |
|---|---|
| 입력검증 촘촘함(길이·범위·포맷) | §4에 초기값을 두되, 이후 필요시 `store.rs` 상수만 조정. |
| 레이트 제한 / 감사 로그 / IDOR 정교화 / 외부 호출 백오프 | 해당 없음(로컬 파일 IO + 브라우저 실행 1회). 과설계 방지를 위해 도입하지 않는다. |

### 링크 열기: 전용 Rust 커맨드 vs JS `openUrl()` (트레이드오프)

- **선택:** 전용 Rust 커맨드 `app_links_open(id)`.
- **대안:** 프론트에서 `@tauri-apps/plugin-opener`의 `openUrl(link.url)` 직접 호출(패키지·권한 모두 이미 있어 백엔드 작업 0).
- **선택 이유:**
  1. **검증 정본이 1곳으로 유지된다.** 프론트가 id만 넘기므로, 열리는 URL은 **반드시 저장·검증을 통과한 목록 안의 값**이다. JS 직접 호출이면 `open_url`에 도달하는 인자가 렌더러가 들고 있는 임의 문자열이고, 유일한 방어선이 `allow-default-urls` 스코프(= `mailto:`·`tel:`까지 허용, 우리 정책보다 넓다)가 된다.
  2. **손으로 편집된 파일도 열기 시점에 재검증**된다(§3-3 3단계). JS 경로에는 이 재검증을 걸 자리가 없다.
  3. **테스트 가능성.** 검증이 Rust 순수 함수라 §7의 T1~T8로 덮인다. TS 검증은 이 저장소에 프론트 테스트 러너가 없어 사실상 미검증으로 남는다.
  4. **프로젝트 패턴 일치.** `CLAUDE.md`의 "권한 표면을 의도적으로 좁게 유지, 커스텀 Rust 커맨드만 노출" 규약과 `google_oauth/mod.rs:231`(Rust에서 브라우저를 여는 기존 선례).
- **포기한 것:** IPC 왕복 1회(수 ms, 사용자 체감 없음)와 Rust 코드 ~30줄.
- **감당 방안:** 없어도 되는 비용이라 그대로 감수한다.

---

## 9. 구현 체크리스트 (병행 착수 분담)

### backend-dev — 프론트와 독립적으로 착수 가능

- [ ] `src-tauri/src/fs_atomic.rs` 신설 + `config/user_config.rs`·`otel_settings.rs`를 re-use로 전환(§2-4). 기존 테스트 무수정 통과 확인.
- [ ] `src-tauri/src/app_links/store.rs` — 스키마·상수·순수 검증 함수·load/save + T1~T21.
- [ ] `src-tauri/src/app_links/mod.rs` — 커맨드 3개 + `AppLinksStatus`/`AppLinksLimits` + `APP_LINKS_FILE_LOCK`.
- [ ] `src-tauri/src/lib.rs` — `mod app_links;` `mod fs_atomic;` + `invoke_handler!`에 `app_links::app_links_get/save/open` 3줄.
- [ ] `cargo test` 통과(기존 250 + 신규 21).
- [ ] `capabilities/default.json`은 **건드리지 않는다**(변경이 필요하다고 판단되면 설계 위반 — 보고할 것).

### frontend-dev — §3-4 계약만 보고 착수 가능(백엔드 완성 전에는 목록이 비어 보일 뿐)

- [ ] `src/appLinksApi.ts` 신설 — §3-4 코드 그대로.
- [ ] `src/state.ts` — `SettingsTab` 유니온에 `'applinks'` 추가 + `appLinks` slice + `sidebar.appLinksExpanded: true` 추가.
- [ ] `src/views/appLinks.ts` 신설 — `loadAppLinks()` + `renderAppLinksPanel()` + 추가/수정 모달.
- [ ] `src/views/settings.ts` — 3줄만(§5-1).
- [ ] `src/sidebar.ts` — `SETTINGS_TABS`에 `{ key: 'applinks', label: '앱링크설정' }` + `appLinksGroup`을 `mainItems`에 삽입.
- [ ] `src/route.ts` — `SETTINGS_TABS` 배열에 `'applinks'` 추가.
- [ ] `src/main.ts` — 프리로드 1줄.
- [ ] 필요 시 `src/styles.css` — 기존 설정 목록 행 클래스 재사용을 우선하고, 새 클래스는 최소로.

### ⚠ 탭 목록이 4곳에 흩어져 있다 — 하나라도 빠지면 조용히 깨진다

`'applinks'`는 아래 **4곳 전부**에 추가해야 한다. `route.ts`를 빠뜨리면 `#/settings/applinks`가 조용히 `otel` 탭으로 폴백해 "메뉴를 눌렀는데 OTel 설정이 뜨는" 증상이 난다.

1. `src/state.ts:19` — `export type SettingsTab = ... | 'applinks'`
2. `src/route.ts:34` — `const SETTINGS_TABS: readonly SettingsTab[] = [...]`
3. `src/sidebar.ts:11` — `SETTINGS_TABS` (라벨 포함)
4. `src/views/settings.ts:23` — `TAB_META` (라벨 포함)

(이 4중 중복은 이번 작업이 만든 것이 아니라 기존 구조다. 한 곳으로 합치는 리팩터는 이번 범위 밖 — 대신 이 체크리스트로 막는다.)

---

## 10. 사람 승인이 필요한 분기

아래는 되돌리기 비용이 큰 결정이라 **설계자가 단독 확정하지 않았다.** 결정 전에는 §9의 착수가 막히지 않는다 — 세 분기 모두 §3의 커맨드 시그니처·JSON 스키마를 바꾸지 않기 때문이다.

### (A) 링크 전체 목록의 출처

| 옵션 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **A1. 사용자 직접 등록만** | 기본 목록 없음. 각자 추가. | 구현 추가 0. public 저장소 노출 위험 0. 사람마다 실제 쓰는 것만 남는다. | 신입이 사내 시스템 주소를 직접 알아내야 한다. 전원이 같은 주소를 각자 입력(오타·불일치). |
| **A2. 기본 카탈로그 시드 + 체크만** | 사내 앱 목록을 앱에 내장하고 사용자는 켜고 끄기만. 호스트명은 `build.rs` + `option_env!("MALGN_APP_LINKS_SEED")`로 주입(선례: `otel_settings.rs:44` `MALGN_OTEL_COLLECTOR_BASE`). | 설치 즉시 사내 시스템이 사이드바에 뜬다(온보딩 가치가 큼). public 저장소에 호스트명이 남지 않는다. | 목록을 바꾸려면 **재빌드·재배포**가 필요하다. CI에 secret 1개 추가. seed가 없는 빌드(포크·secret 없는 CI)의 동작을 별도로 정의해야 한다(빈 목록). 시드 항목과 사용자 항목의 id 충돌·"시드가 갱신되면 사용자가 지운 항목이 되살아나는가" 규칙이 새로 필요하다. |
| **A3. 둘 다** | A2 + 사용자 추가 허용. | 실사용에 가장 가깝다. | A2의 복잡도 전부 + "시드 항목을 사용자가 수정/삭제할 수 있는가"라는 3번째 규칙군이 붙는다. |

**추천: A1 (사용자 직접 등록만).**
근거 — ① 이번 MVP의 목표는 "링크를 모아 사이드바에서 연다"이고 A1로 그 목표가 100% 달성된다. ② A2/A3는 재빌드 없이는 목록을 못 고치는 구조라, 사내 시스템 주소가 바뀔 때마다 릴리스가 필요해진다(50인 조직에서 배포 비용이 목록 관리 비용보다 크다). ③ 시드↔사용자 항목 병합 규칙은 이 기능 전체보다 복잡한 문제다. ④ 나중에 A2를 얹을 때 **데이터 모델 변경이 필요 없다** — `option_env!` 시드를 "파일에 항목이 0건일 때의 초기값"으로만 쓰면 `AppLink` 4필드 그대로다. 즉 A1로 시작해도 A2로 가는 문이 닫히지 않는다.
*A2/A3를 택할 경우 추가로 결정해야 할 것: 시드 항목의 id 고정 방식, 사용자가 삭제한 시드 항목의 부활 여부, seed 미주입 빌드의 동작.*

### (B) 저장 위치

**확인한 사실(근거):**

- `grep -rn "malgn-agent.json" ~/.claude/plugins/cache/malgnsoft-plugins/malgn-agent/` → **0건.** malgn-agent CLI 플러그인은 이 파일을 읽지도 쓰지도 않는다. 현재 이 파일의 소유자는 이 Tauri 앱 단독이다(스키마 공유 제약이 **오늘은** 없다).
- 실제 파일(`~/.claude/malgn-agent.json`)은 `version/workspaces/autonomy/logs` 4키뿐이다.
- **핵심 위험:** `malgn_agent_config_save`(`config/mod.rs:222-232`)는 기존 파일을 읽어 병합하지 않고 `UserConfig`를 **새로 만들어 통째로 직렬화**한다. 즉 그 커맨드가 모르는 최상위 키는 저장 시 **소리 없이 사라진다.** 그리고 이 풀-오버라이트가 이미 통증을 만들고 있다 — `state.ts:236-239`: *"저장 API가 4개 필드를 항상 통째로 덮어쓰는 풀 오버라이트라, 두 화면이 각자 편집하지 않는 필드는 이 status의 현재값을 그대로 실어 보내야 한다."*

| 옵션 | 장점 | 단점 |
|---|---|---|
| **B1. `~/.claude/malgn-agent.json`에 `appLinks` 섹션 추가** | 설정 파일이 1개로 유지된다. 백업·이전이 파일 하나. | `UserConfig`에 필드를 추가해야 하고, `app_links_save`와 `malgn_agent_config_save` **양쪽이 서로의 필드를 보존**하도록 짜야 한다(위 풀-오버라이트 때문). 지금 2개 화면이 겪는 통증이 3개 화면으로 늘어난다. 한쪽의 파싱 실패가 다른 쪽 기능까지 죽인다(손상 시 workspaces·자율업무·앱링크가 동시에 마비). |
| **B2. 별도 파일 `~/.claude/malgn-agent-apps.json`** | 스키마·실패·락이 완전히 격리된다(앱링크 파일이 깨져도 자율업무는 정상). 전체목록 치환 저장(§3-5)을 다른 기능 걱정 없이 쓸 수 있다. 기존 파일·기존 커맨드·기존 테스트를 **한 줄도 건드리지 않는다**. | `~/.claude/`에 파일이 하나 는다. "설정이 여러 파일에 흩어진다"는 인상. |

**추천: B2 (별도 파일).**
근거 — ① B1이 초래하는 "두 커맨드가 서로의 필드를 보존해야 하는" 구조는 이 코드베이스가 **이미 겪고 있는 문제**이고(위 `state.ts` 인용), 새 기능을 그 문제에 합류시킬 이유가 없다. ② 실패 격리: 링크 JSON 하나 깨졌다고 자율업무 스케줄러가 멈추면 안 된다. ③ 기존 파일을 안 건드리므로 회귀 위험이 구조적으로 0이다. ④ `~/.claude/` 아래에는 이미 `settings.json`·`malgn-agent.json`·`installed_plugins.json` 등 용도별 파일이 공존한다 — 파일 분리가 이 디렉터리의 이례가 아니다.
*B1을 택할 경우 추가로 결정해야 할 것: `malgn_agent_config_save`가 기존 `appLinks`를 보존하도록 고칠 것인가(그리고 그 회귀 테스트), 아니면 프론트가 항상 두 섹션을 함께 실어 보낼 것인가.*

### (C) 이 화면을 "설정 탭"에 둘 것인가 — 설계자가 추가로 올리는 분기

요구사항에 "설정 화면의 앱링크설정 탭"이 명시돼 있어 본문은 그대로 따랐다. 다만 되돌리기 비용이 있는 지점이라 확인만 받는다: 앱링크는 **OTel·GitHub·Jira 같은 "연동 설정"과 성격이 다르다**(사용자가 자주 여닫는 개인 북마크에 가깝다). 설정 탭이 7개가 되면 사이드바 설정 하위메뉴가 길어진다.

| 옵션 | 트레이드오프 |
|---|---|
| **C1. 설정 탭(요구사항 원안)** | 구현이 가장 싸고(§5-1의 3줄), 관리 화면이 한 군데 모인다. 설정 하위메뉴가 7개가 된다. |
| **C2. 사이드바 "앱링크" 그룹 안의 "＋ 관리…" 항목 → 모달** | 관리 진입이 링크 바로 옆이라 동선이 짧다. 설정 탭 개수가 안 는다. 모달 안에 목록+추가/수정을 2단 중첩해야 해서 UI가 복잡해지고, `#/settings/applinks` 같은 고정 진입 경로가 없어진다(§6-2의 0건 안내가 갈 곳을 잃는다). |

**추천: C1(원안 유지).** C2의 이점은 동선 몇 픽셀이고, 비용은 모달 중첩과 0건 상태의 진입 경로 상실이다. 원안이 더 단순하다. **별도 답변이 없으면 C1로 진행한다.**

---

## 11. SSO 가능 범위 조사

사용자 요구 — "최대한 SSO 처리가 가능하면 좋겠다". 이 절은 **조사 결과만** 담는다. §0~§10의 설계는 이 조사로 바뀌지 않는다(결론이 "현 설계 유지"이기 때문이다 — §11-3).

### 11-0. 조사 방법과 그 한계 (먼저 밝힌다)

| 수단 | 결과 |
|---|---|
| `WebFetch`로 3개 사이트 루트 요청 | **실패.** `malgnai-hub.apiserver.kr`·`malgnsoft.hrai.kr`은 **HTTP 403**(봇 UA 차단으로 추정), `office.malgnsoft.com`은 본문 없음으로 보고됨. 이 실패를 "확인 불가"로 끝내지 않고 아래 수단으로 다시 시도했다. |
| `curl` + 데스크톱 브라우저 User-Agent | **성공(200).** 3개 모두 HTML 수신. |
| SPA 대응 | `malgnai-hub`·`malgnsoft.hrai.kr`은 둘 다 SPA라 루트 HTML에 로그인 UI가 없다. 두 사이트 모두 **존재하지 않는 경로도 index.html을 200으로 돌려주는** SPA 폴백이라, 단순 경로 추측은 전부 "성공처럼 보이는 실패"였다. 응답 바이트 크기가 index.html(2,816B)과 다른 경로만 실소스로 인정했다. |
| 판별 불가 영역 | 두 사이트 모두 **서버 소스는 비공개**다. 아래 판정은 전부 "공개된 클라이언트 코드 + 공개 API 엔드포인트 응답"에 근거한다. 서버에만 있고 UI에 노출되지 않은 인증 경로가 존재할 가능성은 이 방법으로 배제할 수 없다 — 그래서 §11-4에 사람 확인 항목을 남긴다. |

조사일: 2026-09-14.

### 11-1. 앱 5개 판정표

| # | 앱 | 구글 SSO 판정 | 근거 |
|---|---|---|---|
| 1 | **Gmail** (`mail.google.com`) | **지원 확정** | 구글 자체 서비스. 사용자 확인 완료(재조사 불필요 — 위임 시 확정 사실로 전달됨). |
| 2 | **지라** (`malgn.atlassian.net`) | **지원 확정** | 사용자가 "현재 구글 계정으로 로그인 중"이라고 직접 확인(재조사 불필요). |
| 3 | **맑은AI-Hub** (`malgnai-hub.apiserver.kr`) | **지원 확정 — 5개 중 가장 강함(무음 SSO)** | 아래 §11-1-a |
| 4 | **성과관리시스템** (`malgnsoft.hrai.kr`) | **미지원 확정**(이메일+비밀번호 전용) | 아래 §11-1-b |
| 5 | **맑은오피스** (`office.malgnsoft.com`) | **미지원 확정**(아이디+비밀번호 전용) | 아래 §11-1-c |

요약: **5개 중 3개가 브라우저의 구글 세션만으로 자동 로그인된다. 나머지 2개는 서버가 구글 로그인을 제공하지 않으므로 클라이언트(맑은에이전트) 쪽에서 할 수 있는 일이 없다.**

#### 11-1-a. 맑은AI-Hub — 지원 확정, 그것도 무음(zero-click)

근거는 두 겹이다.

**① 공개 클라이언트 소스** — `https://malgnai-hub.apiserver.kr/pages/login.vue` (24,451B, index.html 폴백이 아닌 실소스):

- `:117-120` — `@click="goGoogleInteractive"` / `<i class="bi bi-google">` / 버튼 문구 **`Google로 로그인`**
- `:129` — 주석 *"Google 로그인이 **기본 수단**, 자체인증(이메일/비밀번호)은 폴백 + 킬스위치 대비용으로 존치."*
- `:167` — 기본 화면 안내 문구 *"회사 Google 계정으로 접속하세요."*
- `:294` — `startSilentLogin()`이 `window.location.replace('/api/auth/google/start?silent=1&…')`
- `:170-200` — `mounted()` 판정 순서: 유효 토큰 없음 → `shouldAttemptSilent()` → **버튼 클릭 없이 무음 로그인 자동 시도**

**② 서버 엔드포인트 실응답** — `GET https://malgnai-hub.apiserver.kr/api/auth/google/start?redirect=%2F` → **HTTP 302**, `Location: https://accounts.google.com/o/oauth2/v2/auth?…&response_type=code&scope=openid+email+profile&code_challenge_method=S256&prompt=select_account&hd=malgnsoft.com`

즉 Authorization Code + PKCE 구글 OAuth가 **실제로 동작 중**이고, `hd=malgnsoft.com`으로 회사 워크스페이스 도메인에 한정돼 있다.

> **이 항목이 (a)안의 가치를 가장 잘 보여준다.** 기본 브라우저에 구글 세션이 살아 있으면, 사용자는 "Google로 로그인" 버튼조차 누르지 않는다 — `prompt=none` 무음 왕복이 자동으로 돌아 대시보드로 떨어진다. 맑은에이전트가 추가로 할 일은 **0**이다.
>
> 단, 무음 경로는 `sessionStorage`/`localStorage` 마커(`mh_g_silent_tried` 등)에 좌우되므로 **브라우저의 첫 진입에서는 버튼 클릭 1회 + 계정 선택(`prompt=select_account`) 1회가 필요할 수 있다.** "언제나 무조건 0클릭"이라고 적지 않는다.

#### 11-1-b. 성과관리시스템 — 미지원 확정

- `https://malgnsoft.hrai.kr/app.js?v=1.0.2` — ViewLogic Router 설정. `defaultRoute: 'login'`, `publicRoutes: ['login','site-not-found','platform-login','forgot-password','reset-password']`. **구글/OAuth 관련 라우트가 하나도 없다.** API 베이스는 `https://malgn-gpm-api.malgnsoft.workers.dev`.
- `https://malgnsoft.hrai.kr/src/logic/login.js` (4,572B 실소스) — 로그인 수단은 `this.$api.post('/auth/login', { email, password, domain })` 단 하나. 파일 전체에 `google`/`oauth`/`sso`/`구글` 문자열 **0건**.
- `https://malgnsoft.hrai.kr/src/views/login.html` — `type="email"`/`type="password"` 입력 2개 + 제출 버튼 + 데모 계정 버튼 4개. **구글 버튼 없음.**
- **서버 측 교차 검증**(클라이언트에만 없을 가능성 배제):
  - `POST https://malgn-gpm-api.malgnsoft.workers.dev/auth/login` → **401** `{"error":"Unauthorized","message":"이메일 또는 비밀번호가 올바르지 않습니다"}` → 라우팅이 살아 있음을 먼저 증명
  - `GET`·`POST /auth/google/start` → **404** `{"error":"Not Found","path":"/auth/google/start"}`
  - `GET /auth/google` → **404**
  - 즉 "404는 그냥 다 404" 가 아니라, 존재하는 라우트는 401을 주고 구글 경로만 404다.

#### 11-1-c. 맑은오피스 — 미지원 확정

- `https://office.malgnsoft.com/` → 본문이 `<script>top.location.replace('/main/login.jsp');</script>` 한 줄.
- `https://office.malgnsoft.com/main/login.jsp` (서버 렌더 JSP, JS 렌더링 아님 — 전체 마크업이 응답에 들어 있다):
  - `<form name="form1" method="post" target="sysfrm">` + `<input type="text" name="id" placeholder="아이디">` + `<input type="password" name="passwd" placeholder="비밀번호">` + `<button type="submit">로그인</button>`
  - 페이지 전체에 `google`/`oauth`/`sso`/`구글` 대소문자 무시 검색 → **0건**, `accounts.google.*` → **0건**
- 이 페이지가 로그인의 **유일한 진입점**이다(루트가 여기로 강제 리다이렉트).

### 11-2. "SSO 최대화"의 두 갈래 — (a) 시스템 브라우저 vs (b) 임베디드 웹뷰

#### (b)가 SSO에 **불리하다**는 근거 (추측 아님, 1차 출처)

| 플랫폼 | 출처 | 확인된 내용 |
|---|---|---|
| **macOS (WKWebView)** | Apple Developer Forums thread 774380 — **Apple Systems Engineer(DTS) 답변** | *"ASWebAuthenticationSession is a full web browser instance, with access to web browser features and the web browser's cookies. WKWebView on the other hand is a way to embed web content inside another app. … **There's no way to share cookies between the two.**"* |
| **macOS (WKWebView)** | Microsoft Learn, *Customize browsers & WebViews (MSAL iOS/macOS)* | *"**WKWebView** is an in-app browser… **It doesn't share cookies or web site data with other WKWebView instances, or with the Safari browser.**"* 같은 문서의 SSO 표에서 WKWebView의 "Shares cookies and other data" = **No**, "SSO" = **No**. |
| **macOS — 이 PC 실측** | `ls ~/Library/WebKit/` | `com.malgn.vscode-mockup`, `tauri-app` 디렉터리가 **이미 존재**한다. 앞의 것은 `src-tauri/tauri.conf.json:6`의 `identifier`와 정확히 일치 — 즉 **이 앱 전용 WebKit 데이터 저장소가 번들 ID 단위로 이미 만들어져 있다.** 브라우저와 공유되는 저장소가 아니다. |
| **Windows (WebView2)** | Microsoft Learn, *Manage user data folders* | *"WebView2 apps use user data folders to store browser data, such as **cookies**, permissions, and cached resources."* 기본 UDF 위치는 **앱 실행 파일 경로 + `.WebView2`**. *"A WebView2 control shares its WebView2 session with any other WebView2 control that uses the same UDF."* → 세션 공유 단위는 **UDF(앱별 폴더)**이지 설치된 Edge 브라우저 프로필이 아니다. Chrome과는 애초에 무관하다. |
| **Tauri 자체** | tauri-apps/tauri Discussion #8637 — 메인테이너(FabianLars) | macOS에서는 데이터 디렉터리를 Tauri가 **제어할 수 없고**, WKWebView가 `~/Library/WebKit/` 아래에 저장한다고 답변. 즉 (b)를 택해도 "브라우저 쿠키를 쓰게 만드는" 설정 스위치가 **없다.** |

#### 비교 결론

| | (a) 시스템 기본 브라우저 — **현 설계** | (b) Tauri 임베디드 웹뷰 |
|---|---|---|
| 구글 세션 재사용 | **그 브라우저의 세션을 그대로 쓴다.** Gmail·지라·Hub 3개가 추가 구현 0으로 로그인된 상태로 열린다. | **쿠키 저장소가 앱 전용으로 분리된다**(위 표). 앱 안에서 5개 앱 **전부 재로그인**해야 하고, 구글 로그인 3개는 앱 웹뷰 안에서 구글 계정 인증을 처음부터 다시 밟아야 한다. |
| 구현 비용 | `app.opener().open_url()` 1줄(§3-3). 이미 `src-tauri/src/google_oauth/mod.rs:232-233`이 같은 호출을 쓴다. | 창 생성·네비게이션 제어·세션 유지·다중 창 관리가 전부 새 코드. |
| 권한 표면 | 변경 없음(`opener:default`). `CLAUDE.md`의 "커스텀 Rust 커맨드만 노출" 규약 유지. | 앱 내부에 임의 외부 사이트를 렌더하는 표면이 새로 생긴다(CSP·네비게이션 허용목록 등 새 보안 결정 다발). |
| 구글의 정책 | 정상 경로. | 구글은 임베디드 웹뷰에서의 OAuth 로그인을 차단·비권장해 왔다. 로그인 자체가 막힐 수 있다. |
| 비밀번호 관리자 | 브라우저의 저장된 비밀번호·자동완성이 그대로 동작 → **미지원 2개(성과관리·오피스)의 실질적 완화책이 여기서 나온다.** | 앱 웹뷰에는 저장된 비밀번호가 없다. 매번 수동 입력. |

**확정: (a) 시스템 기본 브라우저로 연다 — §0~§10 현 설계를 그대로 유지한다.**

**그리고 이것이 추가 개발 없이 도달 가능한 SSO 최대치다.** 근거를 한 문장씩 남긴다:

1. 구글 SSO를 지원하는 3개 앱(Gmail·지라·맑은AI-Hub)은 (a)에서 **맑은에이전트 쪽 코드 0줄로** 자동 로그인된다 — 브라우저가 이미 갖고 있는 구글 세션을 그대로 타기 때문이다.
2. 나머지 2개(성과관리시스템·맑은오피스)는 **서버가 구글 로그인을 제공하지 않는다**(§11-1-b, §11-1-c의 404/0건 근거). 클라이언트가 무엇을 하든 없는 인증 경로를 만들어 낼 수 없다. 이 2개를 자동화하려면 **각 서비스 서버에 구글 OAuth를 구현하는 것**이 유일한 길이고, 그것은 맑은에이전트의 작업이 아니다.
3. (b)는 SSO를 **늘리는 게 아니라 줄인다** — 현재 자동 로그인되는 3개까지 재로그인 대상으로 만든다.

따라서 **커스텀 SSO 토큰 브리지·자체 SSO 프록시·쿠키 주입은 제안하지 않는다.** §0 "범위 밖" 목록을 그대로 유지한다(50인 미만 사내 MVP 도구에 인증 인프라를 새로 만드는 비용/위험이 얻는 것보다 크다).

> **설계 변경 없음.** §3-3 `app_links_open`의 4단계(`app.opener().open_url(url, None::<&str>)`)가 이 결론의 구현체이며, 이미 그렇게 적혀 있다. backend-dev/frontend-dev는 이 절 때문에 무엇도 바꾸지 않는다.

### 11-3. 사용자 기대치 관리 — UI에 무엇을 약속하면 안 되는가

(a)의 SSO는 **"맑은에이전트가 로그인해 줘서"가 아니라 "브라우저에 세션이 남아 있어서"** 되는 것이다(§0에 이미 명시). 따라서:

- 설정 화면·사이드바에 "SSO 지원", "자동 로그인" 같은 **문구를 넣지 않는다.** 브라우저 세션이 만료되면 그 약속이 곧바로 거짓이 된다.
- §5-2 화면 구성에 SSO 관련 배지·힌트를 추가하지 않는다(현 설계 그대로).

### 11-4. 확인 필요 — 사용자에게 물어볼 항목

조사로 **끝내지 못한** 것만 남긴다. 위 판정표의 5건은 전부 근거로 확정됐으므로 여기에 없다.

| # | 물어볼 것 | 왜 코드로 확인할 수 없었나 |
|---|---|---|
| Q1 | **성과관리시스템**에 구글 로그인을 붙일 계획이 있는가? (있다면 그쪽 서버 작업만으로 링크는 그대로 두고 SSO가 완성된다) | 로드맵은 소스에 없다. 현재 구현이 미지원인 것만 확인됨. |
| Q2 | **맑은오피스**에 `/main/login.jsp` 말고 별도의 SSO/사내인증 진입 URL이 존재하는가? | 서버(JSP) 소스가 비공개다. 확인한 것은 "루트가 강제로 보내는 유일한 로그인 페이지에 구글 흔적이 0건"이라는 사실까지다. |
| Q3 | 앱링크를 쓸 **전원이 `@malgnsoft.com` 구글 계정 보유자**인가? | 맑은AI-Hub의 구글 경로는 `hd=malgnsoft.com`으로 회사 도메인에 한정돼 있다. 외부 계정 사용자는 이 경로를 못 탄다(자체 이메일 로그인 폴백으로 떨어진다). |
| Q4 | 각자의 **OS 기본 브라우저**가 평소 구글에 로그인해 쓰는 그 브라우저인가? | OS 설정이라 앱 소스로 알 수 없다. 기본 브라우저가 다르면 (a)의 SSO 효과가 그 사람에게만 나타나지 않는다 — 이 경우 "기본 브라우저를 맞추세요"가 유일하고 올바른 해법이다(앱이 특정 브라우저를 지정해 여는 기능은 만들지 않는다). |
| Q5 | 미지원 2개에 대해 **브라우저 비밀번호 관리자 사용을 권장**하는 것으로 충분한가? | 조직 정책 문제다. 기술적으로는 (a)에서 이미 동작한다. |

### 11-5. 기존 Jira 연동(`jira_integration.rs`)과의 중복 판정

**판정: 개념 충돌 없음 — 별개 기능이다.**

| | 기존 "Jira 설정" 탭 | 새 앱링크 "지라(Jira)" |
|---|---|---|
| 하는 일 | 사이트 URL·이메일·**API 토큰**을 받아 `https://<host>/rest/api/3/myself`로 검증한 뒤 macOS 키체인에 저장(`src-tauri/src/jira_integration.rs:167` `jira_status`, `:290` `jira_disconnect`, `:348` `build_myself_url`, `:368` 키체인 서비스명 `malgn-agent:jira:<host>`) | 저장된 URL을 OS 기본 브라우저로 여는 것뿐(§3-3) |
| 목적 | **자율업무 실행기가 Jira API를 호출**하기 위한 자격증명 보관(`jira_integration.rs:306` 주석: *"자율업무 실행기가 나중에 쓸 내부 전용 읽기 함수"*) | 사람이 Jira 웹 UI로 이동하기 위한 **바로가기** |
| 자격증명 | 있음(키체인) | **없음** — §8이 "어떤 자격증명도 다루지 않는다"로 확정 |
| 화면 | `설정 > Jira 설정` (`src/views/settings.ts:27`, `src/sidebar.ts:15`) | `설정 > 앱링크설정` + 사이드바 `앱링크` 그룹 |

둘은 데이터·저장소·호출 경로가 전혀 겹치지 않는다. 앱링크에서 지라를 지워도 API 연동은 그대로 동작하고, 그 반대도 같다.

**다만 라벨 동음이의로 인한 사소한 혼동 소지는 있다** — 설정 하위메뉴에 `Jira 설정`이 있고 사이드바 `앱링크` 그룹에 `지라`가 뜨면, "앱링크의 지라를 끄면 연동이 끊기나?"라는 오해가 가능하다.

**저비용 완화안(라벨 문구 1개, 기능 추가 없음):** 기존 탭 라벨을 `Jira 설정` → **`Jira 연동(API 토큰)`** 으로 바꾼다. 수정 지점은 `src/views/settings.ts:27`과 `src/sidebar.ts:15` 두 줄뿐이고, GitHub·Cloudflare 탭과 같은 "연동" 성격을 이름에 드러내 앱링크의 단순 바로가기와 구분된다.

> 이 완화안은 **이번 앱링크 구현의 필수 항목이 아니다.** 앱링크 기능과 무관한 기존 파일을 건드리므로, 적용 여부는 PM/사용자 판단으로 남긴다. 적용하지 않아도 앱링크 설계는 성립한다.
