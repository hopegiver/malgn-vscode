# 프로젝트별 에이전트 진행 상황 (v1) — 화면·데이터 설계

> 범위: 로컬 세션 레지스트리만으로 "어느 프로젝트에서 지금 Claude Code가 돌고 있나"를 프로젝트 탭과 홈에 보여준다.
> 데이터 원천은 기존 `list_claude_sessions` 하나이며, 새 커맨드·새 권한·새 감시자는 만들지 않는다.

## 1. 현재 코드에서 확인한 사실 (설계 근거)

| 사실 | 근거 |
|---|---|
| 목록 행은 jsonl(최근 30일, mtime 상위 100건) + registry 라이브 오버레이(`running`)로 만들어진다 | `session_list/jsonl_scan.rs:18-19,77-96`, `registry.rs:84-124` |
| **실행 중 세션은 100건 상한 때문에 빠지지 않는다**: jsonl 행이 상한 밖이어도 registry 항목이 폴백 행(`running:true`)으로 추가된다. 폴백 행에는 `updatedAt`이 없을 수 있다 | `registry.rs:105-121` |
| 상한이 잘라내는 건 **끝난 세션**뿐이다 — 조용한 프로젝트는 "최근 세션 0건"으로 보일 수 있다(실측 체감 창 약 10.6일) | `jsonl_scan.rs:13-17` |
| 모든 행(jsonl·registry 양쪽)은 이미 `cwd`가 워크스페이스 프로젝트 경로 자신이거나 그 하위인지 **경로 컴포넌트 단위**(`Path::starts_with`)로 통과한 것만 남는다. 워크스페이스 밖 세션은 목록에 아예 없다 | `path_filter.rs:89-92`, `mod.rs:164-170`, `jsonl_scan.rs:162` |
| 워크스페이스 루트는 `dunce::canonicalize`로 정규화되고, 프로젝트 `path`는 루트 + 1단계 자식 이름이다 | `config/user_config.rs:257`, `workspace/mod.rs:217-225` |
| 실시간 갱신: `~/.claude/sessions/` FS 이벤트(500ms 디바운스) → `claude-sessions-changed` → `loadSessions()`. 세션·프로젝트 목록은 로그인 직후 둘 다 선로딩된다 | `session_watch.rs`, `main.ts:248-249,303-305` |
| 홈은 이미 "실행 중 세션" 숫자를 인사말과 통계 타일 두 곳에 보여준다 | `views/home.ts:74-77,99,114` |

## 2. 핵심 결정

### 2-1. 세션↔프로젝트 매칭: 백엔드가 `projectPath`를 붙인다 (신규 커맨드 없음)

- **선택**: 기존 `list_claude_sessions` 응답의 각 행에 `projectPath: string` 필드를 **추가**한다(기존 필드는 그대로, 추가만 하므로 프론트 호환 유지). 프론트는 `session.projectPath === project.path` 문자열 동등 비교로 조인한다.
- **대안**: 프론트에서 `cwd`와 `project.path`를 문자열 접두사로 조인.
- **선택 이유**: 백엔드 게이트가 이미 같은 판정(`Path::starts_with`)을 하고 있다. 프론트가 다시 하면 `/`·`\` 구분자, `foo` vs `foo-bar` 접두사 오판(`path_filter.rs`가 이미 한 번 겪은 버그), 경로 끝 슬래시를 TS로 재구현해야 하고, 두 판정이 어긋나면 "목록엔 있는데 어느 프로젝트에도 안 붙는 세션"이 생긴다. 판정을 한 곳(Rust)에 두면 **"목록에 있는 행 = 반드시 projectPath 보유"** 불변식이 성립한다.
- **포기한 것**: 백엔드 소폭 변경(커맨드 시그니처·capability는 불변). 별도 `list_project_activity` 커맨드는 같은 스캔을 한 번 더 하게 만들 뿐이라 기각.

**매칭 규칙 (`path_filter.rs`에 순수 함수 `match_workspace_project(cwd, roots) -> Option<&PathBuf>`로 구현)**

1. `cwd`가 프로젝트 경로 자신 또는 그 하위(컴포넌트 단위)이면 매칭. 하위 디렉터리(`.../malgn-vscode/src-tauri`)에서 띄운 세션도 그 프로젝트로 귀속.
2. 여러 프로젝트에 걸리면(워크스페이스 루트가 겹쳐 `~/workspace/clients`와 `~/workspace/clients/acme`가 모두 프로젝트인 경우) **컴포넌트 수가 가장 많은(가장 구체적인) 경로**가 이긴다.
3. 기존 `is_cwd_within_allowed_workspace`는 `match_workspace_project(..).is_some()`으로 재정의한다. 그러면 게이트와 매칭이 같은 함수에서 나온다.
4. `projectPath` 값은 `WorkspaceProject.path` **원문 문자열 그대로**(PathBuf로 바꿨다 되돌린 값이 아님)여서 프론트 `===` 조인이 성립한다.

**엣지 케이스 (v1 동작)**

| 케이스 | 동작 |
|---|---|
| 워크스페이스 밖 cwd (스크래치패드, `/tmp`, 홈 직하) | 기존대로 목록에서 제외. 프로젝트 탭·홈 집계 어디에도 안 나온다. 홈 문구에 "워크스페이스 기준"을 명시해 과소집계로 오해하지 않게 한다 |
| 프로젝트 폴더 자체가 심볼릭 링크 (`~/workspace/foo → /elsewhere/foo`) | Claude Code가 기록하는 `cwd`는 물리 경로라 게이트에서 탈락(**기존 한계, 이번에 바꾸지 않음**). cwd를 canonicalize해 비교하면 해결되지만 세션 행마다 fs 호출이 늘고 게이트 의미가 바뀌므로 v1 범위 밖. 알려진 한계로 기록 |
| macOS 대소문자 (`/Users/X/Workspace` vs `workspace`) | 루트는 canonicalize로 실제 표기, cwd는 getcwd 물리 표기라 보통 일치한다. 불일치하면 위와 같이 게이트 탈락(기존 동작) |
| Windows 구분자·드라이브 문자 | `Path::starts_with`가 `\`·`/`를 모두 구분자로 처리한다. 드라이브 문자 대소문자(`c:` vs `C:`) 불일치는 검증되지 않았다 → 백엔드 테스트에 `#[cfg(windows)]` 케이스를 추가하고 `docs/WINDOWS-CHECKLIST.md`에 실기 확인 1줄을 추가 |
| `projectPath`가 현재 `state.dashboard.projects`에 없음 (두 목록의 로드 시점 차이로 프로젝트가 막 추가·삭제된 경우) | 프로젝트 카드에는 안 붙는다. 홈 집계는 `projectPath` 고유값으로 세므로 계속 포함된다. 다음 `loadProjects()` 이후 일치 |
| 보관(archived) 프로젝트에서 세션 실행 중 | 인디케이터를 그대로 표시한다(실제 사실이므로 숨기지 않음) |

### 2-2. 실시간 갱신

- 기존 `claude-sessions-changed` → `loadSessions()`를 그대로 쓴다. 프로젝트 탭·홈은 `state.sessions.items`에서 파생 계산만 하므로 별도 구독은 없다.
- **비정상 케이스**: 프로세스가 비정상 종료돼 registry 파일이 지워지지 않으면 FS 이벤트가 안 와서 "실행 중"이 남는다. 다음 `loadSessions()`에서 `pid_alive` 필터(`registry.rs:33-39`)가 걸러낸다. 보완책으로 **프로젝트 목록·상세·홈 라우트에 진입할 때 `loadSessions()`를 1회 재호출**한다(폴링 아님, `main.ts`의 사용량 탭 진입 패턴과 같음. `state.sessions.loading`이면 건너뜀).
- 로드가 실패하면 기존 `state.sessions.items`를 유지한 채 `error`만 세팅된다. 인디케이터는 마지막 성공값을 보여주고, 섹션에는 에러 블록을 보여준다(거짓 0 표시 금지).

### 2-3. 스트레치(서브에이전트 실시간 구성) — **v1 제외**

- 관측 가능한 것: `~/.claude/projects/<dir>/<sessionId>/subagents/agent-<id>.meta.json`의 `agentType`·`description`·`spawnDepth`·`parentAgentId`·`requestShape`(foreground/background), 그리고 같은 이름 `.jsonl`의 mtime(`usage_stats/detail.rs:354-378`이 이미 읽는 경로).
- 관측 불가능한 것: **서브에이전트가 지금 실행 중인지**. meta에 종료 표시가 없다. 부모 트랜스크립트의 `toolUseId` ↔ tool_result 짝맞추기는 `requestShape:"background"` 에이전트에서는 결과가 즉시 반환되므로 실행 종료를 뜻하지 않는다. 결국 "jsonl mtime이 N초 이내면 실행 중" 같은 추정 임계값만 남는데, 이는 `usage_stats`의 "데이터로 뒷받침되지 않는 지표는 만들지 않는다" 원칙에 어긋난다.
- 향후 정직한 대안(v2 후보): "이 세션에서 **생성된** 서브에이전트 목록(agentType·description·마지막 기록 시각)"을 사실 그대로 보여주기. "실행 중"이라고 표기하지 않는다.

## 3. 화면별 표시 요소

데스크톱 네이티브 창 기준이며 반응형 분기는 없다. Terminus 컴포넌트만 재사용하고 신규 토큰은 없다.

공통 파생 함수(신규 `src/projectActivity.ts`, 순수 함수):
`groupSessionsByProject(items) → Map<projectPath, { running: Session[]; recent: Session[] }>` (recent는 `updatedAt ?? startedAt` 내림차순),
`portfolioSummary(items) → { activeProjects: number; runningSessions: number }` (`activeProjects`는 running 세션의 고유 `projectPath` 수).

**(A) 프로젝트 목록 카드** (`views/projects.ts renderProjectCard`)
- `project-card-top`의 이름 오른쪽(배지 왼쪽)에 실행 중일 때만 `runningDot()` + `실행 중 N`(caption, `--status-good`)을 표시한다. 0건이면 아무것도 표시하지 않는다(`dom.ts:108-112` 규약: 끝난 상태가 기본값).
- 세션 미로딩·에러: 인디케이터를 생략한다(카드 레이아웃은 유지). 카드에서는 로딩을 따로 표시하지 않는다.
- 정렬은 바꾸지 않는다. 실행 중 우선 정렬은 라이브 갱신 때마다 카드가 자리를 옮겨 클릭 실수를 유발하므로 기각.

**(B) 프로젝트 상세 "세션" 섹션** (`renderProjectsDetailView`, header 다음·폴더 트리 앞에 `.box` 하나)
- box head: `세션` + 실행 중일 때 `liveIndicator`가 아닌 `실행 중 N · 최근 M` 카운트.
- body: `.blist` 행. 실행 중 세션 전부를 먼저, 그다음 끝난 세션을 최근순으로 해서 **최대 10행**. 행 구성은 홈 `sessionBlistRow`(`home.ts:148-159`)와 같은 열(id·제목·상태·경과시간)이되, 프로젝트명 열은 뺀다. 클릭하면 `#/sessions/<id>`로 간다. → `sessionBlistRow`에 `showProject` 옵션을 추가해 재사용하고 사본은 만들지 않는다.
- 상태: 미로딩 → `loadingBlock()`, 에러 → `errorBlock(err, loadSessions)`, 0건 → "최근 세션이 없습니다. (전체 최근 100건 범위 기준)" + `+ 새 세션` 버튼(기존 카드의 새 세션 경로 재사용).

**(C) 홈 포트폴리오 요약** (`views/home.ts`, 확장이며 중복 추가 아님)
- 통계 타일 `실행 중 세션`: 값은 그대로 M이고, `stat-trend flat`으로 `N개 프로젝트`를 추가한다. N=0이면 trend를 생략한다. 미로딩이면 `—`(기존 그대로).
- 인사말 부속문 `실행 중 세션 M개` → `활동 중 N개 프로젝트 · 실행 중 세션 M개`. 툴팁(`title`)에는 "워크스페이스 내 세션 기준"을 넣는다.
- 새 박스는 추가하지 않는다. 최근 세션 박스가 이미 프로젝트명 열을 갖고 있다.

**visual-designer: 불필요.** 신규 요소가 전부 기존 컴포넌트(`runningDot`·`.blist`·`.box`·`stat-trend`)의 조합이고 새 토큰·레이아웃이 없다.

## 4. 작업 분할과 완료 판정

**BE (backend-dev) — `session_list/` 한정**
1. `path_filter.rs`: `match_workspace_project`를 추가하고 `is_cwd_within_allowed_workspace`가 이를 쓰게 재정의한다.
2. `jsonl_scan.rs build_jsonl_row`·`mod.rs read_claude_sessions`(registry 폴백 행): 게이트 통과 시 `projectPath`를 삽입한다. `registry.rs`의 dedup 순서·시그니처는 변경 금지.
- 완료 판정: `cargo test` 통과 + 신규 단위 테스트 ① 루트 자신/하위 매칭 ② `foo` vs `foo-bar` 거부 ③ 겹치는 루트에서 최장 경로 선택 ④ 반환값이 원문 문자열과 동일 ⑤ `#[cfg(windows)]` `\` 구분자 cwd 매칭. `--ignored` 실데이터 테스트에 "모든 행이 비어 있지 않은 projectPath 보유" 단언 추가.

**FE (frontend-dev)**
1. `src/projectActivity.ts`(위 두 함수) → (A)(B)(C) 반영, `sessionBlistRow` 공용화.
2. `main.ts` 라우트 진입 시 `loadSessions()` 재호출(§2-2).
- 완료 판정: `pnpm build`(tsc) 통과 + `pnpm tauri dev` 실기 확인 캡처 3장(카드 인디케이터 / 상세 세션 섹션 / 홈 타일). 시나리오: 터미널에서 `claude`를 실행하면 해당 카드에 `실행 중 1`이 수 초 내 표시되고, 종료하면 사라진다. 하위 디렉터리에서 실행해도 같은 카드에 붙는다. 세션 0건 프로젝트 상세에서 빈 상태 문구가 나온다.
- 프론트 테스트 러너가 없으므로 순수 함수는 tsc 타입 + 실기 시나리오로 갈음한다. 러너 도입은 이 작업 범위가 아니다.

`capabilities/default.json`·`tauri.conf.json`은 변경 없음(신규 커맨드 없음).

## 5. 향후 메모 (설계하지 않음)

조직 전체 진행률까지 보여주려면 malgnai-hub 연동이 필요하다. 현재 앱은 MCP 연결 상태만 확인한다(`settings/mcp`, `mcp_manager/parsing.rs`). hub가 MCP 밖에 HTTP API를 노출하는지부터 조사해야 한다.
