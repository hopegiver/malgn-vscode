# Terminus 셸 IA — 라우트/데이터 매핑

목적: 확정 목업(`docs/design/terminus-mockup.html`, 홈 화면 1장)의 새 셸 구조(상단 탭스트립 + 좌측
워크스페이스 사이드바 + 하단 상태줄)에 `src/route.ts`의 실제 라우트와 `src/sidebar.ts`가 제공하던
모든 진입점, 그리고 각 화면이 표시하는 실 데이터를 빠짐없이 매핑한다.

범위 밖: 색·폰트·정확한 px 치수(→ `docs/design/terminus-design-system.md`, visual-designer 담당),
기능 추가/삭제. 이 문서는 "지금 있는 것을 어디로 옮기는가"만 다룬다.

`visual-designer 필요:` **필요** — 셸 전체를 새 시각 언어(다크 터미널 테마)로 교체하는 신규
레이아웃 리팩터이고, 기존 `docs/design/terminus-design-system.md`가 이 셸 전용으로 별도 작성
중이다(진행 중 작업과 직결).

---

## 0. 전제

- 가짜 타이틀바(`titlebar`, `traffic` 신호등)는 구현하지 않는다. OS 네이티브 타이틀바(`tauri.conf.json`
  `title: "맑은에이전트"`, `decorations` 미지정 = 기본 네이티브)가 이미 있고, **탭스트립이 웹뷰
  최상단**이다.
- 로그인 화면(`src/views/login.ts`)은 셸(탭스트립/사이드바/상태줄) 없이 지금처럼 전체 화면 카드만
  보여준다 — 근거는 §7-1.
- 목업의 사이드바 "conn" 항목(SSH 커넥션 리스트풍)은 실제로는 `~/workspace` 스캔 결과인
  `WorkspaceProject` 목록이다(워크스페이스=프로젝트, 별도 개념 아님).

---

## 1. 탭스트립 매핑 (9탭 + 계정 슬롯)

`Route.kind`는 `src/route.ts:19-30` 12종. 탭 활성 판정은 `route.kind` 단독이 아니라 `settings`처럼
`route.tab`까지 함께 봐야 하는 탭이 있다(아래 "활성 조건" 열).

| # | 탭 라벨 | 대응 Route kind / tab | 활성 조건 | 현재 진입점(비교) |
|---|---|---|---|---|
| 1 | 홈 | `home` | `route.kind === 'home'` | sidebar.ts:103 `navItem('대시보드', …)` |
| 2 | 프로젝트 | `projects-list`, `projects-detail` | `kind==='projects-list' \|\| kind==='projects-detail'` | sidebar.ts:154-187 `renderProjectsGroup` |
| 3 | 세션 | `sessions-list`, `sessions-detail`, `sessions-draft` | 위 3종 kind 포함 | sidebar.ts:189-222 `renderSessionsGroup` |
| 4 | 사용량 | `usage` | `kind==='usage'` | sidebar.ts:107-113 `navItem('사용량 통계', …)` |
| 5 | 개발 도구 | `settings` (tab=`devtools`) | `kind==='settings' && tab==='devtools'` | sidebar.ts:52 SETTINGS_TABS 중 `devtools` 서브항목(설정 하위) — **탭스트립에서 최상위로 승격** |
| 6 | 자율 작업 | `tasks-list`, `tasks-board`, `tasks-detail` | 위 3종 kind 포함 | sidebar.ts:106 `navItem('자율업무', …)` |
| 7 | 카탈로그 | `catalog` (tab=`plugins`\|`global`) | `kind==='catalog'` | sidebar.ts:86-100 `catalogGroup` |
| 8 | 앱 링크 | `settings` (tab=`applinks`) | `kind==='settings' && tab==='applinks'` | sidebar.ts:230-259 `renderAppLinksGroup` — **탭스트립에서 최상위로 승격**, 단 "링크 클릭=외부 브라우저 열기" 목록 자체는 탭 안 콘텐츠로 이동(§4 참고) |
| 9 | 설정 | `settings` (tab=`otel`\|`github`\|`cloudflare`\|`marketplace`\|`mcp`) | `kind==='settings' && !['devtools','applinks'].includes(tab)` | sidebar.ts:70-84 `settingsGroup` |
| — | 계정 칩(우측) | 해당 라우트 없음 | 클릭 시 로그아웃 드롭다운(§4-2) | sidebar.ts:138-141 `userRow` |

**주의**: 5번·8번 탭은 여전히 `#/settings/devtools`, `#/settings/applinks` 해시로 이동한다
(`route.ts`를 건드리지 않으므로 URL 구조는 그대로 — 단지 사이드바 아코디언 대신 탭스트립 버튼이
같은 `navigate()` 호출을 한다). "설정" 탭은 나머지 5개 서브탭(otel/github/cloudflare/marketplace/mcp)
만 대표한다.

## 2. 설정/카탈로그/자율업무 하위탭 처리 — IA 대안과 선택

**문제**: 기존 사이드바는 설정 7종·카탈로그 2종의 탭 전환 UI 자체를 사이드바 아코디언이 전담했다
(`catalog.ts:175-177` 주석 "탭 전환 자체는 사이드바 하위메뉴가 담당한다… 여기서는 페이지 상단에
별도 탭 칩을 다시 그리지 않는다"). 새 셸에서 사이드바는 워크스페이스 목록 전용이 되므로, 5번·8번을
탭스트립으로 승격한 뒤 남는 **설정 5종·카탈로그 2종의 탭 전환 UI가 갈 자리가 없다.**

- **대안 A — 사이드바 콘텐츠를 라우트별로 교체**(설정/카탈로그 진입 시에만 워크스페이스 목록
  대신 서브탭 목록을 보여줌): 목업의 "사이드바 = 워크스페이스 목록"이라는 고정 정체성이 화면마다
  깨진다. 사용자가 "지금 사이드바가 뭘 보여주는 화면인지" 매번 다시 파악해야 한다.
- **대안 B(채택) — 본문 상단 보조 탭 스트립**: 자율업무 화면이 이미 이 패턴을 쓰고 있다
  (`src/views/autonomousTasks.ts:848` `filter-btn` — 목록/진행상황판 전환). 카탈로그·설정도 같은
  `filter-btn` 스타일 보조 탭을 본문(`.main` 영역) 헤더 바로 아래에 추가한다. 사이드바는 모든
  라우트에서 항상 워크스페이스 목록으로 고정된다.

**선택 근거**: B는 이미 코드베이스에 있는 패턴을 재사용해 새 컴포넌트를 만들지 않고, 목업의
사이드바 정체성(워크스페이스 리스트)을 어떤 탭에서도 흔들지 않는다. 3개 화면(설정·카탈로그·
자율업무)이 동일한 보조 탭 관례를 공유하게 되어 학습 비용도 낮다.

| 화면 | 보조 탭 | 대응 |
|---|---|---|
| 설정(탭 9) | OTel 설정 / GitHub 설정 / Cloudflare 설정 / 마켓플레이스 설정 / MCP 관리 | `route.tab` 5종, sidebar.ts:46-53 SETTINGS_TABS 라벨 그대로 이동 |
| 카탈로그(탭 7) | 플러그인 카탈로그 / 전역 카탈로그 | `route.tab` 2종, sidebar.ts:56-58 CATALOG_TABS 라벨 그대로 이동 |
| 자율 작업(탭 6) | 목록 / 진행상황판 | 기존 `autonomousTasks.ts:848` 그대로 유지(이미 본문 보조 탭) |

## 3. 앱링크 목록의 이동

기존 사이드바의 "앱링크" 그룹은 펼치면 링크 목록이 바로 클릭 가능한 서브아이템으로 보였다
(sidebar.ts:230-259, 클릭 시 `openLink(l)`로 외부 브라우저를 연다 — 화면 전환 없음). 탭스트립
"앱 링크" 탭은 라우트만 있고(`#/settings/applinks`) 클릭 즉시 외부로 나가는 목록이 아니므로, 그
목록은 "앱 링크" 탭의 **본문**(카드/리스트)으로 옮긴다 — `views/appLinks.ts`가 이미 그 설정 패널을
그리고 있어(관리 CRUD 화면) 링크 목록도 같은 패널 안에 있다. 순수 바로가기 목적(전환 없이 빠르게
열기)은 잃지만, 사이드바가 워크스페이스 전용이 되는 구조 변경의 직접 결과이며 대체 경로(탭 진입 후
클릭)가 존재하므로 Dead End는 아니다.

## 4. 사이드바 매핑 (구 사이드바 전체 → 새 사이드바)

새 사이드바는 워크스페이스(=프로젝트) 목록 전용이다. `sortedProjectsByRecency()`
(`src/views/projects.ts:43-45`, `state.dashboard.projects`)를 그대로 재사용한다.

| 목업 요소 | 실 데이터/기존 기능 | 새 위치 |
|---|---|---|
| `sidebar-head` "workspace ~ (5)" | `state.dashboard.projects.length` (`workspaceApi.ts:44-46` → `views/projects.ts:47-61 loadProjects()`) | 사이드바 헤더 그대로 |
| `.conn` 행 이름(`conn-name`) | `WorkspaceProject.name` (`workspaceApi.ts:19`) | 사이드바 행 |
| `.conn` 상태 점(dot good/warn/idle) | **git 브랜치/dirty 아님 — 데이터 없음.** 대체: `WorkspaceProject.archiveStatus`(`workspaceApi.ts:21`, `'active'\|'archived'\|'unknown'`)를 `active→good, unknown→warn, archived→idle`로 매핑 | 사이드바 행 |
| `.conn-meta` "main · clean" / "fix/batch-timeout" | **데이터 없음 — 프로젝트별 git 브랜치/워킹트리 상태를 조회하는 코드가 없다**(`devTools`의 "git"은 `git --version`만 조회, 저장소별 상태 아님, `devToolsApi.ts:1-3` 주석·`dev_tools/mod.rs:161` `ToolId::Git` 정의 참고). 대체안: `WorkspaceProject.updatedAt`(`workspaceApi.ts:24`, epoch ms)을 상대시간("3일 전" 등)으로 표시 | 사이드바 행 |
| `.conn.current` 강조 | `route.kind==='projects-detail' && route.path===p.path` (sidebar.ts:167과 동일 로직) | 사이드바 행 |
| `sidebar-foot` "+ 새 워크스페이스 연결" | 정확히 대응하는 "1클릭 연결" 기능은 없음. 가장 가까운 기존 기능은 `views/projects.ts`의 "워크스페이스 경로 관리" 토글(`projects.ts:158` `editingWorkspaces=true`, `configApi.ts` `malgn_agent_config_get/save`의 `workspaces: string[]` 편집 모달) | 사이드바 하단 CTA — 라벨은 "워크스페이스 경로 관리"로 조정 권장(1클릭 연결이 아니라 경로 목록 편집이므로) |
| `sidebar-brand`(로고+"맑은에이전트") | 상수 텍스트, OS 네이티브 타이틀바가 이미 앱 이름을 보여줌(`tauri.conf.json:17`) | **자리 애매(§8-③) — 탭스트립 좌측 고정 아이콘 슬롯 신설 권장** |
| `sidebar-user-row`(이메일+로그아웃) | `state.auth.userEmail`(`state.ts:62`, `applyAuthenticatedIdentity` `state.ts:494-498`) + 로그아웃(`resetStateForLogout`, `state.ts:514-520`) | 탭스트립 계정 칩(§1 표 "계정 칩") 클릭 → 드롭다운에 로그아웃 배치 |
| `sidebar-footer` 업데이트 배지(`renderUpdateItem`, sidebar.ts:268-281) | `state.update.available/version/installing`(`state.ts:371-380`) | **자리 애매(§8-④) — 상태줄 세그먼트 권장** |
| `renderVersionRow`(앱 버전 + "새 버전 확인", sidebar.ts:297-311) | `getAppVersion()`(sidebar.ts:41-43, Tauri `getVersion()`) | **자리 애매(§8-④) — 상태줄 세그먼트 권장** |
| 프로젝트 그룹 펼침(`renderProjectsGroup`) | 위 새 사이드바 자체가 대체(펼침/접힘 없이 항상 노출) | — |
| 세션목록 그룹 펼침(`renderSessionsGroup`, 개별 세션 제목 퀵링크) | 사이드바에서는 삭제, 대신 홈의 "최근 세션" 패널(§6)과 "세션" 탭 전체 목록이 같은 접근을 제공 — 의도적 통합(누락 아님) | 홈 패널 / 세션 탭 |
| 카탈로그 그룹 펼침 | §2 대안 B로 대체(본문 보조 탭) | 카탈로그 탭 본문 |
| 앱링크 그룹 펼침 | §3으로 대체 | 앱 링크 탭 본문 |
| 설정 그룹 펼침 | §2 대안 B로 대체(본문 보조 탭) | 설정 탭 본문 |

## 5. 상태줄(statusline) 매핑

| 목업 세그먼트 | 실 데이터 소스 | 비고 |
|---|---|---|
| "5 workspaces" | `state.dashboard.projects.length` (동일 §4) | 그대로 |
| "session: 1 running, 3 idle" | `state.sessions.items.filter(s => asBoolean(s.running))`(`views/sessions.ts:38` `asBoolean`, `views/home.ts:441` 이미 동일 계산) | **부분 데이터 없음** — 실 데이터는 `running: boolean` 하나뿐이라 "실행중/완료/대기(idle)" 3단계 구분이 불가능하다. "idle"은 `items.length - runningCount`(=미실행 전체)로만 표시 가능, 목업처럼 "완료"와 "대기"를 나눠 셀 수 없다 |
| "claude 2.1.4 · node v22.9.0" | `state.devTools.items.find(t => t.id==='claude'\|'node').version`(`devToolsApi.ts:26-39` `DevToolStatus.version`, `views/devTools.ts:38 loadDevTools()`) | 그대로 |
| "main ✓ clean" | **데이터 없음**(§4의 `.conn-meta`와 동일 사유) | 표시 안 함. 대체안: 현재 라우트가 `projects-detail`일 때만 그 프로젝트명을 표시, 그 외에는 세그먼트 생략 |
| "pnpm/gh 업데이트 2건" | **데이터 없음(현재는 온디맨드만)** — `DevToolStatus`에는 "최신 버전"/"업데이트 가능" 필드가 없다. 업데이트 가능 여부는 사용자가 개발 도구 화면에서 도구별로 미리보기를 눌러야만 채워지는 `state.devTools.preview[id]`(`state.ts:255` `DevToolPreview`)뿐이라 로그인 시점에 일괄 알 수 없다 | 상태줄에서 생략. 대체안: "개발 도구 확인 →" 같은 정적 링크로 대체(신규 API 호출 없이 탭 이동만) |
| "dev@malgnsoft.com" | 탭스트립 계정 칩과 중복 — 상태줄에서는 생략 | §1 표 참고 |
| 시계 "09:14:52" | 앱 데이터 아님(순수 클라이언트 `new Date()`) — 신규 API 불필요, 프론트 전용 구현 사항 | 유지 가능 |
| (신설) 앱 버전 + 업데이트 배지 | §4 "자리 애매(§8-④)" 항목 이관 | 상태줄 우측 |

## 6. 홈 화면 통계 타일·패널 매핑

| 목업 요소 | 실 데이터 소스 |
|---|---|
| "활성 프로젝트 5/5" | `state.dashboard.projects` 전체 vs `archiveStatus==='active'` 필터(`views/home.ts:407-408` `projectsWidget` 로직 재사용) |
| "실행 중 세션 1" | `views/home.ts:441 sessionsWidget` `runningCount` |
| "오늘 토큰 사용량 812K" | `computeTodayTokens(state.dailyUsage.items)`(`views/usage.ts:94`, `views/home.ts:350` 재사용) |
| "대기 중 자율 작업 2" | `state.autonomousTasks.items.filter(t => t.enabled && !t.running)` — `AutonomousTask`(`state.ts:28-54`), 정확한 "대기" 정의(스케줄 대기 vs 단순 미실행)는 프론트 구현 시 확정 필요(신규 API 아님, 순수 필터 로직) |
| "최근 세션" 패널 행 | `sortedSessions()`(`views/sessions.ts:83-90`) 상위 N건. 열 매핑: ID→`asString(s.sessionId)`(길면 축약), 프로젝트→`projectNameFromCwd(asString(s.cwd))`(`sessions.ts:42-46`), 작업→`sessionTitle(s)`(`sessions.ts:51-53`), 상태→`asBoolean(s.running)`(2단계만, §5와 동일 제약), 시간→`asNumber(s.updatedAt)`(`sessions.ts:84`)을 상대시간으로 변환(신규 계산, 기존 `formatTimestamp`는 절대시각이라 별도 포맷 함수 필요 — API 아님) |
| "자율 작업 큐" 패널 행 | `state.autonomousTasks.items`. ID는 목업의 "WBS-114" 같은 형식이 아니라 `AutonomousTask.id`(`autonomyApi.ts:22`) 원문 문자열 — 표시 형식 그대로 노출 권장(가짜 채번 금지) |
| "개발 도구" 패널 | `state.devTools.items`(6종: claude/node/gh/git/pnpm/wrangler, `dev_tools/mod.rs:93,118,146,161,187,206`). "✓ 최신"/"↑ 새버전" 플래그는 §5와 동일한 이유로 **데이터 없음** — 버전 숫자만 표시하고 플래그는 생략하거나 "확인" 링크로 대체 |
| "이번 주 토큰 사용량" 스파크라인 | `state.dailyUsage.items`(`usageApi.ts:9-15`) 최근 7건, 합계는 `computeUsageTotals`의 `dailyUsageTotal` 방식(`views/usage.ts:51-66`) 재사용, 캐시 히트율은 `computeUsageTotals().cacheHitRate`(`views/usage.ts:63-64`) |

## 7. 비정상/경계 상황

### 7-1. 로그인 화면
셸(탭스트립/사이드바/상태줄)을 보이지 않는다. 근거: `main.ts:93-97`이 `!state.authenticated`일 때
`renderLoginView()`만 반환하고 사이드바·메인을 아예 append하지 않는 구조가 이미 그렇다 — 인증
전에는 워크스페이스 목록도, 탭 전환도 의미가 없고(빈 사이드바만 보여 혼란), 미인증 상태에서 다른
탭으로 이동할 수 있는 것처럼 보이는 UI를 만들 이유가 없다.

### 7-2. 최소폭 900px에서 9탭 오버플로
목업 CSS(`terminus-mockup.html:119` `.window { width: clamp(900px, 78vw, 1180px) }`)가 이미 900px를
하한으로 규정한다. 9개 탭 + 계정 칩이 900px에서 물리적으로 다 안 들어갈 수 있다.
- 채택안: 탭스트립을 `overflow-x: auto`로 가로 스크롤시키고, 계정 칩(`tabstrip-account`)만
  `flex:none; margin-left:auto`로 항상 우측 고정(목업 CSS에 이미 이 속성이 있음 — 스크롤 유무와
  무관하게 유지). 탭을 줄이거나(예: "더보기" 메뉴) 라벨을 숨기는 방식은 채택하지 않는다 — 9개는
  이미 확정 목업 수이고 "더보기" 메뉴는 클릭 2회를 요구해 탭 이동 비용이 늘어난다.
- 정확한 스크롤 어포던스(그림자/화살표 표시 여부 등 시각 디테일)는 visual-designer 몫이다.

### 7-3. 워크스페이스 0개 / 로딩 / 에러 사이드바
기존 사이드바의 "B1(리뷰 v0.2.5)" 교훈(sidebar.ts:171-175, "0건"과 "로딩 중"을 구분해야 함)을
그대로 승계한다.
- 로딩(`!state.dashboard.loaded`): "불러오는 중…" 1행.
- 0개(`state.dashboard.loaded && projects.length===0`): "워크스페이스가 없습니다" + 하단 CTA
  ("워크스페이스 경로 관리")를 강조.
- 에러(`state.dashboard.error`): 에러 메시지 1행 + 클릭 시 `loadProjects()` 재시도(홈 위젯의
  `widgetErrorShell` 패턴, `views/home.ts:62-67`과 동일 원칙).

### 7-4. 탭 활성 표시와 하위 라우트의 관계
- 탭 활성 판정은 §1 표의 "활성 조건" 그대로 — `route.kind`뿐 아니라 `settings`는 `route.tab`까지
  본다.
- `projects-detail`(프로젝트 상세)·`sessions-detail`/`sessions-draft`(세션 상세/초안)·`tasks-detail`
  (자율업무 상세)에서도 상위 탭(프로젝트/세션/자율 작업)은 계속 활성 상태를 유지한다 — 별도의
  "뒤로가기" 탭을 만들지 않고, 사이드바의 `.conn.current` 강조(§4)와 본문 헤더의 뒤로가기 링크로
  "지금 상세 화면에 있다"는 것을 표시한다(기존 각 상세 화면이 이미 자체 뒤로가기 링크를 가지고
  있다고 가정 — 이 문서가 신설하는 요구사항은 아님).

## 8. 사람 판단이 필요한 지점 (요약)

1. §2 — 설정/카탈로그 보조 탭을 본문 상단(대안 B)에 둘지, 다른 배치를 원하는지.
2. §4 사이드바 풋터 CTA 라벨 — "+ 새 워크스페이스 연결"(목업 원문, 1클릭 연결처럼 읽힘) vs
   "워크스페이스 경로 관리"(실제 동작, 경로 목록 편집) 중 택1.
3. §4 브랜드 마크(로고+앱이름) — 탭스트립 좌측에 고정 슬롯을 신설할지, OS 네이티브 타이틀바만으로
   충분하다고 보고 생략할지.
4. §4·§5 앱 버전 표시 + 업데이트 배지("새 버전 확인" 버튼 포함) — 상태줄 세그먼트로 이관을
   권장하지만, 클릭 인터랙션(버튼)이 있는 요소를 얇은 상태줄에 넣는 것이 목업의 "정보 전용 상태줄"
   컨셉과 맞는지 확인 필요.

## 9. visual-designer 필요 여부

**필요.** 기존 스타일가이드/디자인시스템이 커버하지 못하는 새 다크 터미널 테마 전면 교체이며,
`docs/design/terminus-design-system.md`가 이 셸 전용으로 별도 진행 중이다(관리자단 여부와 무관하게
신규 모듈 개발 기준으로 필요 판정).

---

## 화면별 3필드 (우선순위/밀도/동선)

이 문서는 개별 화면 와이어프레임이 아니라 셸 구조 IA이므로, 셸을 구성하는 3개 레이아웃 단위에만
표기한다(신규 화면 자체가 아니라 기존 라우트의 진입 경로 재배치이므로 상세 화면별 우선순위는
각 화면 기존 설계를 승계).

- **탭스트립**: 우선순위 — 활성 탭 라벨(현재 위치 인지가 최우선). 밀도 — 저(가로 1줄, 9탭+계정
  칩만). 동선 — 탭 클릭 → 해당 라우트 즉시 전환(중간 확인 없음).
- **사이드바**: 우선순위 — 1순위: 현재 프로젝트 강조(`.conn.current`) / 2순위: 상태 점(활성 여부)
  / 3순위: 워크스페이스명. 밀도 — 중(행마다 이름+메타 2줄). 동선 — 행 클릭 → 프로젝트 상세 →
  (상세 화면 내 기존 세션/파일 탐색으로 이어짐, 이 문서 범위 밖).
- **상태줄**: 우선순위 — 세션 실행 상태(가장 시간 민감). 밀도 — 저(한 줄, 세그먼트 나열). 동선 —
  없음(정보 전용, §8-4의 업데이트 배지만 예외적으로 클릭 가능).
