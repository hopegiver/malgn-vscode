# Terminus 셸 IA — 라우트/데이터 매핑

목적: Terminus 셸(상단 탭스트립 + 좌측 사이드바 + 하단 상태줄)의 라우트·데이터 매핑을 정의한다.
`src/route.ts`의 실제 라우트, `src/sidebar.ts`/`src/main.ts`가 구현해야 할 진입점, 각 화면이 표시하는
실 데이터를 빠짐없이 연결한다.

**핵심 결정(확정)**: **탭스트립 = 최상위 내비게이션(어떤 화면인가), 사이드바 = 선택된 탭의 맥락별
보조 내비게이션/필터**. 두 레벨은 연동된다 — 사이드바 내용은 `route.kind`(및 `settings`/`catalog`
탭에서는 `route.tab`)에 따라 완전히 바뀐다. VS Code의 액티비티바→사이드바 패널 전환과 같은 패턴이다
(참고 시안: 스크래치패드 `design-ide-vscode.html` — 색·타이포는 참고하지 않는다, 이 프로젝트는
`terminus-design-system.md`가 정본이다. 이 참고 시안이 보여준 것은 오직 "상단/좌측 아이콘 클릭 →
사이드바 패널 전환"이라는 상호작용 패턴 하나뿐이다).

**폐기된 이전 결정**: "사이드바는 모든 탭에서 고정된 워크스페이스 목록"이라는 이전 설계는 폐기한다.
워크스페이스 목록은 이제 9개 탭 중 2개(홈·프로젝트)에서만 쓰이는 사이드바 콘텐츠 종류의 하나일
뿐이다.

범위 밖: 색·폰트·정확한 px 치수(→ `docs/design/terminus-design-system.md`, visual-designer 담당),
새 데이터 조회·새 API·Rust 변경(아래 "신규 동작"으로 명시한 항목 제외 — 전부 클라이언트 전용이며
작다). 이 문서는 "지금 있는 것을 어디로 옮기는가"만 다룬다.

`visual-designer 필요:` **필요** — 셸 전체가 새 시각 언어(다크 터미널 테마)로 진행 중인 신규
레이아웃 리팩터이고, 사이드바가 탭마다 다른 콘텐츠를 담게 되며 아래 §7이 지적하는 새 행/그룹 헤더
컴포넌트가 `terminus-design-system.md`에 아직 없다.

---

## 0. 전제

- 가짜 타이틀바는 구현하지 않는다. OS 네이티브 타이틀바가 이미 있고, 탭스트립이 웹뷰 최상단이다.
- 로그인 화면은 셸 없이 전체 화면 카드만 보여준다(§6-1).
- `renderSidebar(route: Route)`(`src/sidebar.ts:223`)의 기존 시그니처는 그대로 유지 가능하다 —
  `route.kind`와 `settings`/`catalog` 탭에서는 `route.tab`까지 이미 인자로 들어오므로 사이드바
  콘텐츠 분기에 필요한 정보가 이미 충분하다.
- 탭스트립 계정 칩(`renderAccountChip`, `sidebar.ts:102-133`)과 상태줄의 버전/업데이트 배지
  (`sidebar.ts:254-343`)는 이번 변경의 영향을 받지 않는다 — 이미 탭스트립/상태줄에 자리 잡은 구
  진입점이고, 사이드바 콘텐츠 재설계와 독립적이다.

---

## 1. 탭스트립 매핑 (9탭 + 계정 슬롯) — 변경 없음

`Route.kind`는 `src/route.ts:19-30` 12종. 탭 활성 판정은 `route.kind` 단독이 아니라 `settings`처럼
`route.tab`까지 함께 봐야 하는 탭이 있다.

| # | 탭 라벨 | 대응 Route kind / tab | 활성 조건 |
|---|---|---|---|
| 1 | 홈 | `home` | `route.kind === 'home'` |
| 2 | 프로젝트 | `projects-list`, `projects-detail` | 두 kind 포함 |
| 3 | 세션 | `sessions-list`, `sessions-detail`, `sessions-draft` | 세 kind 포함 |
| 4 | 사용량 | `usage` | `kind==='usage'` |
| 5 | 개발 도구 | `settings` (tab=`devtools`) | `kind==='settings' && tab==='devtools'` |
| 6 | 자율 작업 | `tasks-list`, `tasks-board`, `tasks-detail` | 세 kind 포함 |
| 7 | 카탈로그 | `catalog` (tab=`plugins`\|`global`) | `kind==='catalog'` |
| 8 | 앱 링크 | `settings` (tab=`applinks`) | `kind==='settings' && tab==='applinks'` |
| 9 | 설정 | `settings` (tab=`otel`\|`github`\|`cloudflare`\|`marketplace`\|`mcp`) | `kind==='settings' && !['devtools','applinks'].includes(tab)` |
| — | 계정 칩(우측) | 해당 라우트 없음 | 클릭 시 로그아웃 드롭다운 |

5번·8번 탭은 `#/settings/devtools`, `#/settings/applinks` 해시를 그대로 쓴다(URL 구조 불변) —
라우트상으로는 "설정"의 서브탭이면서 탭스트립에서는 최상위로 승격되어 있다. 이 승격은 §4-9에서
"설정" 탭 사이드바가 이 둘을 다시 넣지 않는 근거가 된다.

---

## 2. 사이드바 총론

### 2-1. 연동 원칙
사이드바는 `route.kind`(그리고 `settings`/`catalog`에서는 `route.tab`)가 바뀔 때마다 완전히 다른
콘텐츠를 그린다. 탭스트립 클릭 한 번으로 최상위 화면과 그 화면의 보조 내비게이션이 함께 바뀐다 —
사용자가 "지금 사이드바가 뭘 보여주는 화면인지"를 탭 라벨만 보고 알 수 있다(탭이 이미 그 답이므로).

### 2-2. 하위탭 전환 UI 배치 — 사이드바로 통합
**결정**: 설정 5종(otel/github/cloudflare/marketplace/mcp)·카탈로그 2종(plugins/global)·자율
작업 2뷰(목록/진행상황판)의 전환 UI는 전부 **사이드바**로 옮긴다. 본문(main) 쪽에 같은 전환 UI를
중복해서 그리지 않는다.

**근거**: 사이드바가 이제 "선택된 탭의 맥락별 보조 내비게이션"이라는 정체성을 가지므로, 하위탭
전환은 정확히 그 정체성에 속하는 기능이다. 예전 IA는 사이드바가 전 탭 공통 워크스페이스 목록이라는
별개 정체성을 지켜야 했기 때문에 하위탭 전환을 본문으로 밀어냈지만, 그 제약이 이번 변경으로
사라졌다.

**중복 제거 조치(문서 지시 — 코드 미변경)**: `src/views/autonomousTasks.ts:847-848`의 본문
`filter-btn`(목록/진행상황판)은 사이드바로 이관되며 본문에서는 제거되어야 한다. 카탈로그·설정은
현재 코드에 본문 전환 UI가 없으므로(설정은 `TAB_META`를 부제 텍스트로만 쓰고 있음, `settings.ts:36-39`)
중복 제거 대상이 없다 — 향후 추가하지 않는다.

### 2-3. 폭 / 최소창 / 빈 패널
- 사이드바 폭은 `terminus-design-system.md` §2 그대로: 220px(760px 이하 컨테이너 쿼리에서 175px,
  배치 D 가독성 조정으로 190/150px에서 상향 — `src/styles.css:108-109`·`:400-402`).
  이번 IA 변경은 폭 자체를 바꾸지 않는다.
- 900px 최소 창에서도 사이드바는 항상 렌더된다 — 9개 탭 모두 아래 §4에서 사이드바 콘텐츠가 정의돼
  있어(홈 포함) "빈 사이드바"가 발생하는 탭이 없다.
- **원칙(향후 대비)**: 만약 어떤 탭이 정말로 보조 내비게이션이 없다면(현재는 해당 탭 없음) 빈
  `.sidebar` 박스를 그리지 않고 `<aside>` 자체를 생략한다 — 빈 패널을 보여주는 것보다 본문이
  전체 폭을 쓰는 편이 낫다.

---

## 3. 사이드바 공용 원자 컴포넌트 (탭별 표에서 재사용)

아래 행 종류는 여러 탭에서 반복된다 — 각 탭 표는 이 이름만 참조한다.

- **워크스페이스 행**(`ws-row`): 기존 `src/sidebar.ts:196-208 wsRow()` 그대로 재사용. 상태
  dot(`dotClassFor`, `sidebar.ts:176-178`) + 이름 + 상대시간(`formatDaysAgo`, `sidebar.ts:184-194`).
  홈(§4-1)·프로젝트(§4-2) 두 탭에서 동일하게 쓴다 — 새 컴포넌트 아님.
- **좁은 폭 목록 행**(신규, 시각 스펙 없음 — §7-1): 세션/자율작업 큐/개발도구/앱링크 사이드바가
  공통으로 필요로 하는 2줄 스택형 행(아이콘·dot 1개 + 제목 1줄 + 보조메타 1줄). 기존
  `.blist-row--sessions`(52/96/1fr/74/56px 5열 그리드, `terminus-design-system.md` §3.2)는 본문
  폭(600px+) 전제라 190px 사이드바에 그대로 쓸 수 없다 — 새 좁은 폭 변형이 필요하다(§7-1).
- **정적 nav 행**(신규, 시각 스펙 없음 — §7-3): 라벨 텍스트 하나 + 활성 강조만 있는 행(설정 5종,
  카탈로그 2종, 자율작업 뷰 전환 2종). `.filter-btn`(§3.11)은 가로 배치 전제라 세로 사이드바 목록에
  그대로 쓰려면 레이아웃 방향만 바꾸면 되지만(새 클래스 불요), 활성 강조가 배경 채움
  (`filter-btn.active`)이라 촘촘한 세로 목록에서 과할 수 있어 재검토 필요(§7-3).

---

## 4. 탭별 사이드바 명세

### 4-1. 홈 (`route.kind === 'home'`)

사이드바 = 워크스페이스 요약 리스트. 빈 화면 대신 실제 콘텐츠를 채운다 — 홈이 진입 화면이라
"지금 워크스페이스가 몇 개고 무엇인지"를 바로 보여주는 것이 첫 화면 가치가 크고, 프로젝트 탭과
동일한 `ws-row`를 재사용해 새 컴포넌트 비용이 0이다(§3).

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| 헤더 "workspace ~ (N)" | `state.dashboard.projects.length`(`state.ts:66-76`) | — | — |
| 워크스페이스 행(`ws-row`) × N | `sortedProjectsByRecency()`(`views/projects.ts:43-45`) | `navigate('#/project/<path>')` | 홈 탭에서는 항상 비활성(현재 상세 화면이 아니므로 `.current` 없음) |
| 하단 CTA "워크스페이스 경로 관리" | — | `openWorkspacesManager()`(`sidebar.ts:217-221`, `#/projects`로 이동 후 편집 모달 오픈) | — |

**로딩/0건/에러**: `!state.dashboard.loaded` → "불러오는 중…" 1행. `loaded && projects.length===0`
→ "워크스페이스가 없습니다" + CTA 강조. `state.dashboard.error` → 에러 메시지 1행 + 클릭 시
`loadProjects()` 재시도(모두 기존 `sidebar.ts:230-236` 로직 그대로).

우선순위: 워크스페이스 행의 상태 dot(활성/보관 여부가 최우선 시선) / 밀도: 중 / 동선: 행 클릭 →
프로젝트 상세(탭이 "프로젝트"로 전환됨, §4-2).

### 4-2. 프로젝트 (`route.kind === 'projects-list' | 'projects-detail'`)

사이드바 = 홈과 동일한 워크스페이스 리스트(§3 `ws-row` 재사용, 별도 컴포넌트 아님). 사용자가 제시한
"프로젝트 트리/리스트" 중 **리스트**를 채택한다 — **트리는 채택하지 않는다**: 프로젝트 상세 화면은
이미 본문에 실제 폴더 트리 + 파일 미리보기 패널을 갖고 있다(`renderProjectTreeSection`,
`views/projects.ts:405`, `loadProjectTree`/`ProjectTreeNode`, `state.ts:78-88`). 사이드바에 같은
트리를 또 그리면 좁은 폭(190px)에서 파일 트리 깊이가 감당 안 되고 본문과 중복된다.

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| 헤더 "workspace ~ (N)" | 4-1과 동일 | — | — |
| 워크스페이스 행 × N | 4-1과 동일 | `navigate('#/project/<path>')` | `route.kind==='projects-detail' && route.path===p.path` → `.current` 강조(기존 `sidebar.ts:197` 로직 그대로) |
| 하단 CTA | 4-1과 동일 | 동일 | — |

**로딩/0건/에러**: 4-1과 동일.

우선순위: 1순위 — 현재 프로젝트 강조(`.current`) / 2순위 — 상태 dot / 3순위 — 이름. 밀도: 중.
동선: 행 클릭 → 상세(본문에 트리/파일 미리보기가 이어짐, 사이드바는 그대로 리스트 유지).

### 4-3. 세션 (`route.kind === 'sessions-list' | 'sessions-detail' | 'sessions-draft'`)

사이드바 = 세션 목록(요청대로 상태 점 포함). `sortedSessions()`(`views/sessions.ts:83-86`)를 그대로
쓴다 — 새 조회 없음.

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| 헤더 "sessions (N)" | `state.sessions.items.length`(`state.ts:100-106`) | — | — |
| 세션 행 × N(좁은 폭 목록 행, §3) | `sortedSessions()` 각 항목: 제목=`sessionTitle(s)`(`sessions.ts:51-53`), 프로젝트=`projectNameFromCwd(asString(s.cwd))`(`sessions.ts:42-46`), 상태 dot=`asBoolean(s.running)`(`sessions.ts:38-40`), 시간=`updatedAt`을 `home.ts`의 `formatCompactElapsed`류 상대시간으로(재사용, 신규 계산 아님) | `navigate('#/sessions/<sessionId>')` | `route.kind==='sessions-detail' && route.sessionId===s.sessionId` |
| 하단 CTA "+ 새 세션" | — | `openNewSessionModal()`(현재 `views/sessions.ts:273`에 정의되어 있으나 미export — 사이드바 재사용을 위해 export 필요, **코드 변경은 frontend-dev 몫**) | — |

**로딩/0건/에러**: `!state.sessions.loaded` → "불러오는 중…". `loaded && items.length===0` →
"세션이 없습니다" + CTA 강조. `state.sessions.error` → 에러 1행 + `loadSessions()` 재시도.
`sessions-draft` 라우트에서는 아직 세션이 존재하지 않으므로 목록 내 활성 행이 없다(정상 —
본문 자체 헤더가 "새 세션 작성 중"임을 이미 표시).

우선순위: 1순위 — 실행중 dot(펄스, 시간 민감) / 2순위 — 제목 / 3순위 — 프로젝트명/시간. 밀도: 고
(4개 정보가 좁은 폭에 들어가야 함). 동선: 행 클릭 → 세션 상세(대화 이어보기).

### 4-4. 사용량 (`route.kind === 'usage'`)

**결정(2026-09-24, PM, 배치 B로 폐기)**: 사용량 사이드바는 **"일별 사용량" 단일 항목**만 둔다.
배치 A에서 시도했던 "최근 7일/최근 30일" 기간 토글과 "최근 활동일" 날짜 퀵점프 목록은 폐기했다.
이 탭의 본문(usage.ts) 자체가 곧 "일별 사용량" 전체이고, 사이드바가 보여줄 수 있는 다른 보조
내비게이션(하위탭·필터)이 실제로는 없다 — 정적 nav 행 하나로 "지금 보고 있는 화면이 무엇인지"만
확인시켜 준다(클릭 동작 없음, §2-3 "빈 사이드바보다 낫다" 원칙과 별개로 이 탭은 §4 다른 절과
형식을 맞추기 위해 빈 `<aside>` 생략 대신 단일 행을 택했다). 본문은 항상 최근 30일 전체를
보여주는 **main 브랜치 원래 동작**으로 되돌아갔다 — 기간 필터 자체가 없으므로 `computeUsageTotals`/
`renderDailyUsageSection`도 슬라이스 없이 `state.dailyUsage.items` 전체를 쓴다.

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| "일별 사용량" 단일 nav 행 | 없음(정적 라벨) | 없음 | 항상 `.current`(이 탭에 있는 동안 유일한 항목이므로) |

**로딩/0건/에러**: 사이드바 항목 자체는 정적이라 로딩/에러가 없다(본문이 독립적으로
`dailyUsage.loading`/`error`/`items.length===0`을 처리 — `usage.ts` 기존 로직 그대로,
"최근 30일 이내 사용 기록이 없습니다" 문구도 원복).

우선순위: 단일 위계(항목 1개). 밀도: 저. 동선: 없음(사이드바에서 이 화면을 제어하지 않는다 — 날짜
펼침은 본문 막대 행 클릭만으로 이뤄진다, `usage.ts` `toggleDailyDetail` 비export).

### 4-5. 개발 도구 (`route.kind === 'settings' && route.tab === 'devtools'`)

**요청("도구 카테고리 목록")과 실제 데이터의 간극**: `DevToolStatus`(`devToolsApi.ts:26-39`)에
카테고리 필드가 없다(claude/node/gh/git/pnpm/wrangler 6종, 플랫 목록). 대신 이미 있는
`required: boolean` 필드로 **"필수 도구" / "선택 도구"** 2개 그룹으로 묶는다 — 실제 데이터에 근거한
대체이지 임의 분류가 아니다.

| 항목 | 데이터 출처 | 클릭 시 동작(신규, 클라이언트 전용) | 활성 판정 |
|---|---|---|---|
| 그룹 헤더 "필수 도구" / "선택 도구" | `state.devTools.items.filter(t => t.required)` / `.filter(t => !t.required)`(`state.ts:247-260`) | — | — |
| 도구 행 × 6(좁은 폭 목록 행, §3) | 각 `DevToolStatus`: 이름 + 설치 여부(`installed`) + 버전 | 본문 `.devtool-list`(`devTools.ts:391`)의 해당 행으로 스크롤 + 일시 하이라이트(신규 클라이언트 전용 동작, 새 라우트·API 없음) | 마지막으로 클릭한 항목만 일시 강조(영속 상태 아님) |

**로딩/0건/에러**: `devTools.loading && !loaded` → "불러오는 중…". 0건은 발생하지 않는다(6개
고정 도구 목록이라 항상 존재). `devTools.error` → 에러 1행 + `loadDevTools()` 재시도.

우선순위: 1순위 — "필수 도구" 그룹(미설치 시 위험도 높음) / 2순위 — "선택 도구" 그룹. 밀도: 고
(6항목이 2그룹으로 나뉘어도 좁은 폭에 촘촘). 동선: 클릭 → 스크롤 이동(라우트 전환 없음, 같은 화면
내 앵커).

### 4-6. 자율 작업 (`route.kind === 'tasks-list' | 'tasks-board' | 'tasks-detail'`)

사이드바는 2블록: (a) 상단 뷰 전환(목록/진행상황판, §2-2가 본문에서 이관), (b) 하단 작업 큐
목록(요청대로).

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| 뷰 전환 "목록" / "진행상황판"(정적 nav 행, §3) | — | `navigate('#/tasks')` / `navigate('#/tasks/board')` | `route.kind==='tasks-list'` / `'tasks-board'`(§2-2 이관 — 본문 `autonomousTasks.ts:847-848`의 동일 버튼은 제거) |
| 작업 큐 행 × N(좁은 폭 목록 행, §3) | **전체 작업**(PM 확정, §8-③ 대체 — 최초안의 running/waiting
필터는 채택되지 않았다). `state.autonomousTasks.items`를 `taskGroupOf()`(실행중→활성→비활성)로
정렬(`sidebar.ts:taskGroupOf/renderTasksSidebar`), 비활성 작업은 `.sidebar-row-dim`으로 흐리게
표시한다. 표시: `t.projectName`·`t.name`·상태 텍스트("실행 중"/"대기"/"중지됨") | `navigate('#/tasks/item/<id>')` | `route.kind==='tasks-detail' && route.taskId===t.id` |

**로딩/0건/에러**: `autonomousTasks.loading && !loaded` → "불러오는 중…". `loaded && items.length===0`
→ "등록된 자율 작업이 없습니다"(큐가 전체 작업을 보여주므로 0건은 "대기 중인 게 없다"가 아니라
"등록된 게 없다"는 뜻이다, `sidebar.ts:renderTasksSidebar`) — CTA 버튼은 두지 않는다(추가는 본문
"+ 새 자율 작업"에서, 사이드바-본문 중복 방지). `autonomousTasks.error` → 에러 1행 + 재시도.
`tasks-detail`에서는 뷰 전환 두 항목 모두 비활성(목록도 보드도 아닌 제3의 화면), 작업 큐 행 중
해당 작업만 `.current`.

우선순위: 1순위 — 뷰 전환(지금 보고 있는 화면 형태) / 2순위 — 실행중(`running`) 작업 / 3순위 —
대기중 작업. 밀도: 고. 동선: 뷰 전환 → 본문 전체 교체 / 큐 행 클릭 → 작업 상세.

### 4-7. 카탈로그 (`route.kind === 'catalog'`)

`CATALOG_TABS`(`route.ts:33`, `'plugins' | 'global'`) 2종을 사이드바 nav 행으로 올린다(§2-2).

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| "플러그인 카탈로그"(정적 nav 행, §3) | `state.catalog.plugins.length`(`state.ts:264-274`)를 옆에 카운트로 표시 가능(선택, §8) | `navigate('#/catalog/plugins')` | `route.tab==='plugins'` |
| "전역 카탈로그" | `state.globalCatalog.data`의 `agents.length + skills.length`(`state.ts:279-284`) | `navigate('#/catalog/global')` | `route.tab==='global'` |

**로딩/0건/에러**: nav 행 자체는 정적 라벨이라 로딩/에러가 없다(각 탭 본문이 독립적으로
로딩/에러/빈 상태를 처리 — 기존 `catalog.ts:222-291` 그대로, 변경 없음).

우선순위: 단일 위계(두 항목 중 활성 표시가 전부). 밀도: 저. 동선: 클릭 → 본문 전체 교체(라우트
전환).

### 4-8. 앱 링크 (`route.kind === 'settings' && route.tab === 'applinks'`)

**요청("링크 그룹")과 실제 데이터의 간극**: `AppLink`(`appLinksApi.ts:10-15`)에 그룹 필드가 없다
(id/name/url/enabled뿐). 그룹 대신 **플랫 퀵오픈 목록**으로 대체한다 — 이전 IA에서 "본문 CRUD와
중복"이라는 이유로 사이드바에서 제외했던 결정을 이번에 뒤집는다: 사이드바가 이제 탭 전용 콘텐츠라
"고정 워크스페이스 목록과의 정체성 충돌"이라는 제외 사유 자체가 사라졌고, 사이드바 행의 동작(클릭
= 외부 브라우저로 즉시 열기)과 본문 행의 동작(클릭 = 이름/URL 수정)이 서로 다른 목적이라 중복이
아니다.

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| 링크 행 × N(좁은 폭 목록 행, §3) | `enabledAppLinks()`(`appLinks.ts:36-38`, 활성화된 링크만) | `openLink(link)`(`appLinks.ts:41-47`, 외부 브라우저로 즉시 열기 — 라우트 전환 없음) | 없음(외부 액션이라 지속 상태 없음) |

**로딩/0건/에러**: `appLinks.loading && !loaded` → "불러오는 중…". `loaded && enabledAppLinks().length===0`
→ "등록된 앱링크가 없습니다"(추가는 본문에서, CTA 중복 없음). `appLinks.error` → 에러 1행 +
`loadAppLinks()` 재시도.

우선순위: 단일 위계(이름만). 밀도: 저(목록이 보통 소수). 동선: 없음(클릭 = 즉시 외부로 나감,
화면 전환 없음 — 실패 시 토스트만, `appLinks.ts:44-45` 기존 동작 그대로).

### 4-9. 설정 (`route.kind === 'settings' && tab ∈ {otel, github, cloudflare, marketplace, mcp}`)

`TAB_META`(`settings.ts:23-31`)에서 `devtools`·`applinks`를 뺀 5개를 사이드바 nav 행으로 올린다
(§2-2). **개발 도구·앱 링크는 이 사이드바에 다시 넣지 않는다** — 둘 다 이미 탭스트립에서 독립
최상위 탭(#5·#8, §1)으로 별도 진입점을 갖고 있어, 설정 사이드바에도 넣으면 같은 화면으로 가는
경로가 3개(탭스트립 직접 진입 + 설정 사이드바에서 재진입 + 해시 직접 입력)로 늘어나 혼란만
커진다.

| 항목 | 데이터 출처 | 클릭 시 동작 | 활성 판정 |
|---|---|---|---|
| "OTel 설정"(정적 nav 행, §3) | `TAB_META[0].label` | `navigate('#/settings/otel')` | `route.tab==='otel'` |
| "GitHub 설정" | `TAB_META[1].label` | `navigate('#/settings/github')` | `route.tab==='github'` |
| "Cloudflare 설정" | `TAB_META[2].label` | `navigate('#/settings/cloudflare')` | `route.tab==='cloudflare'` |
| "마켓플레이스 설정" | `TAB_META[3].label` | `navigate('#/settings/marketplace')` | `route.tab==='marketplace'` |
| "MCP 관리" | `TAB_META[4].label` | `navigate('#/settings/mcp')` | `route.tab==='mcp'` |

**로딩/0건/에러**: nav 행은 정적 라벨(로딩/에러 없음) — 각 패널이 독립적으로 처리(변경 없음,
`settings.ts`의 기존 `loadOtelEnv`/`loadGithubStatus`/`loadCloudflareStatus`/`loadMcp`/
`loadMarketplaces` 그대로).

우선순위: 단일 위계. 밀도: 저(5항목, 라벨만). 동선: 클릭 → 본문 전체 교체.

---

## 5. 하위 라우트에서 사이드바 상태

| 하위 라우트 | 사이드바가 보여주는 탭 콘텐츠 | 활성 항목 |
|---|---|---|
| `projects-detail`(프로젝트 상세) | §4-2 워크스페이스 리스트 그대로 유지 | 해당 프로젝트 행 `.current` |
| `sessions-detail`(세션 상세) | §4-3 세션 리스트 그대로 유지 | 해당 세션 행 활성 |
| `sessions-draft`(새 세션 작성) | §4-3 세션 리스트 그대로 유지 | 없음(아직 세션이 존재하지 않음 — 정상) |
| `tasks-detail`(자율업무 상세) | §4-6 그대로 유지 | 뷰 전환 2항목 모두 비활성, 작업 큐에서 해당 작업만 활성 |

공통 원칙: 상세/초안 라우트에 들어가도 사이드바는 "탭이 바뀌었다"고 취급하지 않는다 — 같은 탭
안의 하위 화면이므로 사이드바 콘텐츠 종류는 유지되고, 그 안에서 어떤 항목이 "지금 보고 있는 것"인지
만 강조가 바뀐다. 별도의 "뒤로가기" 사이드바 상태를 만들지 않는다 — 본문 자체의 뒤로가기 링크가
그 역할을 한다(기존 각 상세 화면이 이미 갖고 있다고 가정, 이 문서가 신설하는 요구사항 아님).

---

## 6. 상태줄·탭스트립 부가 사항 (변경 없음 — 참고용 재확인)

- 상태줄 세그먼트(워크스페이스 수·세션 실행 상태·claude/node 버전·앱 버전·업데이트 배지·시계)는
  이번 사이드바 재설계와 무관하다. 정의는 `sidebar.ts:254-343`(구현 기존 그대로).
- ### 6-1. 로그인 화면: 셸(탭스트립/사이드바/상태줄)을 보이지 않는다 — `main.ts:93-97`이
  `!state.authenticated`일 때 `renderLoginView()`만 반환하는 구조 그대로.
- ### 6-2. 900px 오버플로: 탭스트립은 `overflow-x: auto` + 계정 칩만 우측 고정 — 변경 없음.

---

## 7. 시각 스펙 보강이 필요한 사이드바 요소 (목록만 — 스펙 자체는 visual-designer 작성)

1. **사이드바 전용 좁은 폭 목록 행**(2줄 스택형: 아이콘/dot + 제목 1줄 + 보조메타 1줄) — 세션·
   자율작업 큐·개발도구·앱링크 사이드바 공통 필요. 기존 `.blist-row--sessions` 등(본문 폭 전제,
   5열 그리드)은 190px 사이드바에 그대로 쓸 수 없다.
2. **사이드바 그룹 헤더**(예: 개발 도구의 "필수 도구"/"선택 도구") 라벨 타이포/간격.
3. **사이드바 정적 nav 행의 활성 강조**(설정 5종·카탈로그 2종·자율작업 뷰 전환 2종) — 기존
   `.filter-btn.active`(배경 전체 채움)를 세로 촘촘한 목록에 그대로 쓸지, 좌측 강조선 등 더 가벼운
   방식을 쓸지 결정 필요.
4. **사이드바 카운트 배지**(예: 카탈로그 nav 행 옆 설치 개수) — `.box-head .count`는 있으나 사이드바
   nav 행 전용 위치/크기 미정.
5. **개발 도구 사이드바 행의 압축 상태 표시**(설치 여부를 좁은 폭에서 아이콘 1개로 표현) —
   `.blist-devtool-flag`(본문용, 텍스트 포함)를 그대로 줄이면 잘릴 수 있음.
6. **사이드바 일시 강조**(개발 도구 항목 클릭 시 본문 스크롤 대상 하이라이트) — 애니메이션/지속
   시간 미정.

---

## 8. 사람 판단이 필요한 지점

1. §4-7 카탈로그·§4-9 설정 사이드바 nav 행에 카운트 배지(설치 플러그인 수, MCP 서버 수 등)를
   추가할지 — 정보 밀도는 올라가지만 설정 5종 중 otel/github/cloudflare는 해당 탭에 들어가기
   전까지 데이터가 로드되지 않아(`main.ts:262-270` 지연 로드) 배지가 한동안 빈 값/로딩으로 보일 수
   있다.
2. (해소, 2026-09-24) §4-4 사용량 사이드바의 기간 토글은 PM 결정으로 폐기됐다 — 영속화 여부
   판단 자체가 더 이상 필요 없다.
3. §4-6 자율 작업 사이드바 큐 목록의 정렬 기준(현재 `home.ts`와 동일하게 "실행중 우선, 그다음
   활성 대기중"만 보여주고 완료/비활성은 숨김) — 완료된 작업도 사이드바에서 바로 열람하고 싶은
   수요가 있는지.

---

## 9. visual-designer 필요 여부

**필요.** 사이드바가 탭마다 다른 콘텐츠(좁은 폭 목록 행, 그룹 헤더, 정적 nav 행, 카운트 배지 등
§7의 6개 요소)를 새로 갖게 되며, 이는 기존 스타일가이드/디자인시스템(`terminus-design-system.md`)이
아직 커버하지 못하는 신규 컴포넌트다. 관리자단 여부와 무관하게 "신규 모듈 개발" 기준으로 필요
판정한다.

---

## 화면별 3필드 (우선순위/밀도/동선) — 셸 컨테이너 단위

- **탭스트립**: 우선순위 — 활성 탭 라벨. 밀도 — 저. 동선 — 탭 클릭 → 해당 라우트 즉시 전환(사이드바
  콘텐츠도 함께 전환됨, §2-1).
- **사이드바**: 탭별 세부 우선순위/밀도/동선은 §4-1~§4-9 각 절 끝에 개별 기재했다(탭마다 성격이
  달라 단일 값으로 일반화하지 않는다). 공통 원칙만 여기 남긴다 — 동선은 항상 "행 클릭 → 본문 갱신
  또는 라우트 전환" 둘 중 하나이며, 좁은 폭 탓에 밀도는 대부분 중~고에 수렴한다.
- **상태줄**: 우선순위 — 세션 실행 상태. 밀도 — 저. 동선 — 없음(정보 전용, 업데이트 배지만 예외적
  클릭 가능).
