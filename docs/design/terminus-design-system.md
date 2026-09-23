# Terminus 디자인 시스템 스펙

정본 목업: `docs/design/terminus-mockup.html` (홈 화면 1장, 다크 단일 테마 · Terminus/Tabby 터미널 에뮬레이터 미학)
이 문서는 그 목업의 색·타이포 토큰을 **그대로 승격**하고, 목업에 없는 나머지 화면(전체 10개)에 필요한 컴포넌트(버튼·입력·모달·토스트 등)를 목업과 같은 어휘로 **외삽**한 것이다. frontend-dev는 `src/styles.css`(884줄, 구 인디고 라이트 테마)를 전면 교체할 때 이 문서 하나만 보면 된다.

## 0. 범위와 제약 재확인

- 다크 단일 테마. 라이트 모드 없음.
- 실제 앱은 OS 네이티브 타이틀바를 쓴다 → 목업의 `.titlebar`/`.traffic`(신호등)/`--tl-red/yellow/green`은 **이 스펙에서 제외**한다. 탭스트립이 웹뷰 최상단이다.
- 인사말 커서(`.cursor`)는 정적이다(깜빡임 애니메이션 없음) — 다른 로딩/펄스 애니메이션까지 정적으로 만들라는 뜻은 아니다(§7 참고).
- 셸 정보구조(탭/사이드바/상태줄에 무엇을 보여줄지)는 `docs/design/terminus-shell-ia.md`(ux-designer, 별도 진행)가 결정한다. 이 문서는 그 컨테이너들의 **외형**(색·보더·타이포·간격)만 정의한다.
- 데스크톱 전용 — 모바일 반응형 없음. 창 최소 900×600, 기본 1180×760. 목업의 컨테이너 쿼리 브레이크포인트(1040px, 760px)는 창 축소 대응이며 그대로 승계한다.

---

## 1. 디자인 토큰

### 1.1 색상 — 목업 `:root` 정본 그대로 승격

아래 표의 HEX 값은 전부 `terminus-mockup.html`의 `:root`에서 그대로 가져왔다(재계산·변경 없음). "신규" 표시된 두 토큰(`--text-muted-aa`, `--border-interactive`)만 이 문서에서 새로 파생했고, 근거는 §1.2에 있다.

| 토큰 | HEX | 용도 | 출처 |
|---|---|---|---|
| `--win-bg` | `#0d0e12` | 앱 최상위 배경(구 `--color-canvas`에 대응) | 목업 정본 |
| `--panel-bg` | `#121319` | 박스/카드/입력/버튼 표면(구 `--color-surface`) | 목업 정본 |
| `--panel-bg-alt` | `#15161d` | 한 단계 밝은 표면(hover, 리스트 교대 배경, 채팅 사용자 버블) | 목업 정본 |
| `--sidebar-bg` | `#0f1014` | 사이드바 컨테이너 배경 | 목업 정본 |
| `--tabstrip-bg` | `#111217` | 탭스트립 컨테이너 배경 | 목업 정본 |
| `--statusline-bg` | `#0a0b0e` | 상태줄 배경, 코드/로그 블록 배경(가장 어두운 층) | 목업 정본 |
| `--border` | `#25272f` | 박스/카드 등 그룹 경계(장식적, 저대비 의도) | 목업 정본 |
| `--border-soft` | `#1b1c22` | 리스트 행 구분선 등 더 옅은 경계 | 목업 정본 |
| `--border-interactive` | `#5c6478` | **신규** — 버튼/입력/토글 등 조작 가능한 요소의 기본 경계 | §1.2 파생 |
| `--text-primary` | `#dadde3` | 본문·제목 텍스트 | 목업 정본 |
| `--text-secondary` | `#8d919c` | 보조 텍스트(설명, 라벨 값) | 목업 정본 |
| `--text-muted` | `#585c68` | **텍스트 색으로 쓰지 않는다** — 장식 전용(§1.2) | 목업 정본(용도 제한) |
| `--text-muted-aa` | `#7d818c` | **신규** — caption/timestamp/상태 라벨 등 실제로 읽어야 하는 저채도 텍스트 | §1.2 파생 |
| `--accent` | `#5fd0dc` | 브랜드 포인트(ANSI bright cyan) — 링크, 활성 표시, 주요 버튼 배경 | 목업 정본 |
| `--accent-dim` | `#356469` | accent의 저채도 배경 전용 변형(눌림 상태, 선택 배경) — 텍스트로 쓰지 않는다 | 목업 정본(용도 제한) |
| `--ansi-green` | `#52d97f` | Semantic: success | 목업 정본 |
| `--ansi-yellow` | `#e2b34f` | Semantic: warning | 목업 정본 |
| `--ansi-red` | `#f25f6c` | Semantic: danger | 목업 정본 |
| `--ansi-magenta` | `#c98ae6` | 보조 강조(사용처 자유, 과용 금지) | 목업 정본 |
| `--ansi-blue` | `#6fa2f2` | Semantic: info (목업엔 미배정, 이 문서에서 info로 확정) | 목업 정본 재활용 |
| `--status-good` | `var(--ansi-green)` | 상태 dot/텍스트 | 목업 정본 |
| `--status-warn` | `var(--ansi-yellow)` | 상태 dot/텍스트 | 목업 정본 |
| `--status-bad` | `var(--ansi-red)` | 상태 dot/텍스트 | 목업 정본 |
| `--status-idle` | `var(--text-muted-aa)` | **변경** — 원래 `var(--text-muted)`였으나 상태 정보를 전달하는 dot이라 §1.2 사유로 AA 토큰으로 교체 | 목업에서 재라우팅 |

**색을 새로 들이지 않는다**: 표 22개 토큰 중 20개는 목업 리터럴 그대로이고, 2개(`--text-muted-aa`, `--border-interactive`)만 신규 파생이며 둘 다 §1.2에 계산 근거가 있다. `--info`라는 새 이름 대신 이미 목업에 있던 `--ansi-blue`를 그대로 의미역에 배정했을 뿐 새 HEX는 없다.

### 1.2 WCAG 대비 검증 (실측, sRGB 상대휘도 기준)

**핵심 발견 — `--text-muted`(#585c68)는 실사용 배경 전부에서 AA 미달이다.** 목업에 쓰인 6개 배경(`--win-bg` ~ `--tabstrip-bg`) 전부에서 2.70~2.95:1로, 본문/캡션 텍스트에 필요한 4.5:1은 물론 비텍스트 3:1도 못 넘는다. 아래는 실측치다.

| 텍스트 토큰 | win-bg | panel-bg | panel-bg-alt | sidebar-bg | statusline-bg | tabstrip-bg | 판정 |
|---|---|---|---|---|---|---|---|
| `--text-primary` #dadde3 | 14.18 | 13.62 | 13.25 | 13.97 | 14.46 | 13.75 | AA 통과 |
| `--text-secondary` #8d919c | 6.12 | 5.88 | 5.72 | 6.03 | 6.24 | 5.93 | AA 통과 |
| `--text-muted` #585c68 | 2.89 | 2.78 | 2.70 | 2.85 | 2.95 | 2.80 | **AA 미달** |
| `--text-muted-aa` #7d818c(신규) | 4.95 | 4.76 | 4.63 | 4.88 | 5.05 | 4.80 | AA 통과(전부 ≥4.5) |
| `--accent` #5fd0dc | 10.60 | 10.19 | 9.91 | 10.45 | 10.82 | 10.28 | AA 통과 |
| `--accent-dim` #356469 | 2.92 | 2.81 | 2.73 | 2.88 | 2.98 | 2.83 | AA 미달(텍스트 금지 이유) |
| `--ansi-green` #52d97f | 10.65 | 10.23 | 9.95 | 10.49 | 10.86 | 10.32 | AA 통과 |
| `--ansi-yellow` #e2b34f | 9.92 | 9.53 | 9.27 | 9.77 | 10.12 | 9.61 | AA 통과 |
| `--ansi-red` #f25f6c | 6.09 | 5.85 | 5.69 | 6.00 | 6.21 | 5.90 | AA 통과 |
| `--ansi-blue` #6fa2f2 | 7.46 | 7.17 | 6.98 | 7.36 | 7.62 | 7.24 | AA 통과 |

**처리 방침**: `--text-muted`의 HEX 값 자체는 목업 그대로 바꾸지 않는다(정본 보존). 대신 **용도를 장식 전용으로 제한**하고, 실제 정보를 전달하는 텍스트(캡션 라벨·타임스탬프·상태 단어·"완료"/"대기" 같은 배지 텍스트)는 전부 새로 파생한 `--text-muted-aa`(#7d818c)로 라우팅한다. 이 두 토큰 분리 패턴은 이 저장소의 기존 `src/styles.css`가 동일 문제(`--color-ink-faint-inverse`, `--color-ink-muted-inverse` 등)를 고친 방식을 그대로 따른 것이라 코드베이스 관례와 일치한다.

목업에서 `--text-muted`가 실제로 쓰인 곳을 전수 분류하면:
- **장식 유지(값 그대로)**: `.panel-head::before`(┌─ 박스드로잉 글리프, 순수 장식), `.tab .dot`/`.proc-status .dot.done`의 배경(옆에 항상 같은 의미의 텍스트가 병기되는 중복 인코딩이라 단독 식별 수단이 아님)
- **`--text-muted-aa`로 교체**: `.titlebar-title`(제외 대상이라 무관), `.stat-label`, `.conn-meta`, `.sidebar-foot`, `.sidebar-head`, `.greet-sub`, `.panel-head .count`, `.stat-value small`, `.stat-trend.flat`, `.proc-id`, `.proc-status.done`, `.proc-time`, `.queue-id`, `.badge.wait`, `.tool-sym`, `.usage-figure .l`, `.spark-days`, `--status-idle`(정보성 dot)

**버튼/입력 경계용 `--border-interactive`(#5c6478) 파생 근거**: 목업의 `--border`(#25272f)는 win-bg 대비 1.30:1로, 박스/카드처럼 배경 차이·헤딩으로도 그룹을 알 수 있는 컨테이너에는 문제없지만(WCAG 1.4.11은 이런 순수 장식 구분선까지 요구하지 않음), 버튼·입력·토글처럼 사용자가 조작 가능 영역의 경계 자체를 인식해야 하는 컴포넌트는 비텍스트 3:1(SC 1.4.11)이 필요하다. `#5c6478`은 목업 6개 배경 전부에서 3.05~3.33:1로 기준을 만족하며, 기존 `text-secondary`/`text-muted-aa`와 같은 청회색 계열이라 새 색상군을 만들지 않는다.

**버튼 배경 위 텍스트 색 — 중요한 함정**: `--accent`(#5fd0dc)와 `--ansi-red`(#f25f6c)는 둘 다 "다크 배경 위 밝은 전경색"으로 캘리브레이션된 값이라, 이 색을 **버튼처럼 배경으로 채워 쓸 때** 흰색/`--text-primary` 텍스트를 올리면 대비가 무너진다(accent 배경+흰 텍스트 1.82:1, ansi-red 배경+흰 텍스트 3.17:1 — 둘 다 AA 미달). 실측 결과 이 두 색을 배경으로 쓸 때는 반드시 **`--win-bg`(다크 잉크) 텍스트**를 올려야 한다(accent 배경 10.60:1, ansi-red 배경 6.09:1). 아래 §3 버튼 정의에 반영했다.

### 1.3 타이포그래피

**폰트 패밀리 — 목업 정본 그대로, 변경 없음**:
```css
--font-display: -apple-system, BlinkMacSystemFont, 'Noto Sans KR', 'Segoe UI', 'Apple SD Gothic Neo', 'Malgun Gothic', sans-serif;
--font-body: -apple-system, BlinkMacSystemFont, 'Noto Sans KR', 'Segoe UI', 'Apple SD Gothic Neo', 'Malgun Gothic', sans-serif;
--font-numeric: 'JetBrains Mono', ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace;
```

**어느 텍스트에 어느 폰트를 쓰는가(재확인, 예외 없음)**:

| 요소 유형 | 폰트 |
|---|---|
| 한글이 섞일 가능성이 있는 모든 UI 텍스트(탭/사이드바/버튼/라벨/본문/인사말 등) | `--font-display` / `--font-body` |
| 순수 숫자·버전·시계·세션ID·경로 등 한글이 섞이지 않는 leaf 요소 | `--font-numeric` |
| 박스드로잉 글리프(`┌─` 등, 생성 콘텐츠) | `--font-numeric`(정렬 목적, 국소 예외) |

**크기 계층 — `:root`에 토큰화**(목업은 `:root`에 크기 토큰이 없고 클래스마다 리터럴로 흩어져 있었다 — 여기서 새로 정리). 목업 자체에 있던 `10.5px`/`11.5px`/`9.5px` 같은 반픽셀 값은 기존 `src/styles.css`가 이미 확립한 규칙(내림 반올림 — 말줄임/줄바꿈 회귀 위험이 낮은 쪽)을 그대로 따라 정수로 내렸다.

```css
:root {
  /* 표시 전용 — h1보다 큰 특수 1회성 스타일(홈 인사말), 일반 계층에 포함하지 않음 */
  --font-size-hero: 16px;      /* .greet-line, weight 700 */

  --font-size-h1: 18px;        /* 페이지 제목(다른 9개 화면 헤더) — 목업엔 없어 절제된 값으로 신규 정의,
                                   §7 참고. 구 시스템의 22px보다 작다 — "일반 SaaS 헤더"로 보이지 않게
                                   터미널 특유의 절제된 스케일을 의도적으로 유지한다. */
  --font-size-h2: 15px;        /* 모달/상세 섹션 제목 */
  --font-size-body: 13px;      /* 기본 본문 */
  --font-size-body-lg: 14px;   /* 채팅 본문 전용 예외 — 장문 가독성 우선(줄간격도 1.7로 별도) */
  --font-size-label: 11px;     /* .panel-title류 — uppercase, tracked */
  --font-size-small: 11px;     /* 보조 텍스트(.proc-task 등), 구 11.5px 내림 */
  --font-size-caption: 10px;   /* 메타/타임스탬프/카운트, 구 10.5px 내림 */
  --font-size-micro: 9px;      /* 최소 단위(spark-days 등), 구 9.5px 내림 */

  --line-height-tight: 1.3;    /* 제목 */
  --line-height-base: 1.6;     /* 본문 */
  --line-height-loose: 1.7;    /* 채팅 본문 전용 */

  --letter-spacing-label: 0.08em;  /* .panel-title 계열 uppercase 라벨 */
  --letter-spacing-eyebrow: 0.06em;
}
```

### 1.4 간격 스케일

목업은 고정 8px 그리드를 쓰지 않고 2~24px 구간을 촘촘하게 실사용한다(터미널 정보 밀도 반영). 그 관찰값에 근거해 2px 단위의 세밀한 스케일로 토큰화한다 — 8px 단위의 일반 SaaS 그리드보다 촘촘한 것이 의도적 차별점이다.

```css
:root {
  --space-05: 2px;
  --space-1: 4px;
  --space-1-5: 6px;
  --space-2: 8px;
  --space-2-5: 10px;
  --space-3: 12px;
  --space-3-5: 14px;
  --space-4: 16px;
  --space-5: 18px;
  --space-6: 20px;
  --space-7: 22px;
  --space-8: 24px;
  --space-10: 32px;
}
```

### 1.5 보더 / 라운드 / 그림자

**라운드 — 목업 관찰에 근거한 일관 규칙**: 목업 1장 전체(패널·탭·통계 타일·배지·창 외곽 제외)에서 `border-radius`가 쓰인 곳은 상태 dot(`50%`, 원)뿐이다. 패널·통계·배지 어디에도 각진 모서리 외 값이 없다 — 이것이 "터미널" 정체성의 핵심 신호라고 판단해, 이 문서 전역에 **사각형=0, 원=상태/아이덴티티 표시 전용**이라는 규칙으로 명시적으로 확장한다(§7에 애매했던 점으로 별도 표기).

```css
:root {
  --radius-0: 0;              /* 박스/카드/버튼/입력/모달/토스트/배지/진행률바 전부 기본값 */
  --radius-circle: 50%;       /* 상태 dot, 아바타 전용 */

  --border-width: 1px;
  --border-width-accent: 2px; /* 활성 탭 밑줄, 콜아웃 좌측 강조선 두께의 절반 단위 */
  --border-width-callout: 3px;

  /* 그림자 — 목업은 창 외곽(제외 대상) 외에 그림자를 쓰지 않는다(깊이는 배경 명도 단계로 표현).
     스크림 없이 뜨는 요소(토스트/배너/드롭다운)만 예외로 최소한의 그림자를 허용한다.
     구 시스템의 rgba(0,0,0,0.2)는 밝은 배경 기준값이라 거의 검은 배경 위에서는 안 보여
     불투명도를 올렸다. */
  --shadow-float: 0 4px 16px rgba(0, 0, 0, 0.45);
}
```

### 1.6 모션 토큰

```css
:root {
  --transition-fast: 120ms ease;   /* hover/press 피드백 */
  --transition-base: 180ms ease;   /* 토글 슬라이드, 배너 등장 */
  --transition-slow: 320ms ease;   /* 모달 오버레이 페이드 */
  --pulse-duration: 1.6s ease-in-out infinite; /* live-dot, 상태 펄스 (목업 값 유지) */
  --skeleton-duration: 1.2s ease-in-out infinite; /* 스켈레톤 (구 시스템 값 유지) */
}
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```
인사말 커서(`.cursor`)는 이 규칙과 무관하게 **항상** 정적이다(애니메이션 자체를 정의하지 않음, 확정 사항). 위 reduced-motion 가드는 스피너/펄스/토스트 등 다른 모든 동작에 적용되는 전역 안전장치로, 목업에는 없던 것을 접근성 요구사항 충족을 위해 추가했다.

### 1.7 포커스 링

```css
:root {
  --focus-ring: 0 0 0 2px var(--win-bg), 0 0 0 4px var(--accent);
  /* 배경색과 같은 2px 갭을 두어 어떤 표면 위에서도 accent 링이 끊겨 보이지 않게 한다 */
}
:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
```
`--accent`는 모든 목업 배경 위에서 9.9~10.8:1로 비텍스트 3:1 기준을 크게 상회해 포커스 링 색으로 안전하다.

---

## 2. 셸 컨테이너 시각 스펙 (외형만 — 내용/IA는 ux-designer 소관)

아래는 각 컨테이너의 **색·경계·치수**만 정의한다. 무엇을 담을지는 `terminus-shell-ia.md`를 따른다.

| 컨테이너 | 배경 | 경계 | 비고 |
|---|---|---|---|
| 앱 최상위 | `--win-bg` | — | 네이티브 타이틀바 아래 전체 |
| 탭스트립 | `--tabstrip-bg` | 하단 `1px solid var(--border)` | 높이 32px(목업 값), 활성 탭은 `--win-bg` 배경 + 하단 `2px solid var(--accent)` |
| 사이드바 | `--sidebar-bg` | 우측 `1px solid var(--border)` | 너비 190px(목업 값), 760px 이하 컨테이너 쿼리에서 150px로 축소(목업 값 유지) |
| 메인 콘텐츠 | `--win-bg` | — | 패딩 `var(--space-6) var(--space-7) var(--space-5)`(20/22/18px, 목업 값) |
| 상태줄 | `--statusline-bg` | 상단 `1px solid var(--border-soft)` | 높이 24px(목업 값), 텍스트는 `--text-muted-aa`/`--text-secondary` |

---

## 3. 컴포넌트 스타일 정의

고정 클래스 어휘(`.box`/`.blist`/`.stat`/`.callout`/`.card`/`.badge`)를 새 캐노니컬 이름으로 삼는다. 나머지(버튼/입력/모달/토스트 등)는 짧은 기존 이름을 유지한다.

### 3.1 `.box` (구 `.panel`)
```css
.box { position: relative; background: var(--panel-bg); border: var(--border-width) solid var(--border); border-radius: var(--radius-0); }
.box-head {
  display: flex; align-items: center; gap: var(--space-1-5);
  padding: var(--space-1-5) var(--space-3); border-bottom: var(--border-width) solid var(--border);
  font-family: var(--font-display);
}
.box-head::before { content: "┌─"; color: var(--text-muted); font-size: 12px; font-family: var(--font-numeric); }
.box-title { font-size: var(--font-size-label); letter-spacing: var(--letter-spacing-label); text-transform: uppercase; color: var(--text-secondary); }
.box-head .count { margin-left: auto; font-size: var(--font-size-caption); color: var(--text-muted-aa); font-family: var(--font-body); }
.box-body { padding: var(--space-2-5) var(--space-3) var(--space-3); }
```
(구 `.panel`/`.panel-head`/`.panel-title`/`.panel-body` 1:1 대응, 목업 그대로 승계 + 라운드/폰트 토큰만 치환)

### 3.2 `.blist` (구 `.proc-row`/`.queue-row`/`.tool-row`/`.session-row`/`.mcp-row`/`.task-row`/`.devtool-row`/`.plugin-entry-row`/`.marketplace-*-row`/`.run-history-row`/`.thief-table-row` 계열 통합)
```css
.blist { display: flex; flex-direction: column; }
.blist-row {
  display: grid; gap: var(--space-2-5); align-items: center;
  padding: var(--space-1-5) 0; border-bottom: var(--border-width) solid var(--border-soft);
  font-size: var(--font-size-small); color: var(--text-primary);
}
.blist-row:last-child { border-bottom: none; }
.blist-row.blist-head {
  font-weight: 700; color: var(--text-secondary); font-size: var(--font-size-caption);
  border-bottom: 2px solid var(--border); text-transform: uppercase; letter-spacing: var(--letter-spacing-eyebrow);
}
/* grid-template-columns는 화면별 데이터 형태에 맞춰 스코프 클래스로 지정한다(구 시스템도 화면마다
   컬럼 폭이 달랐다 — 하나로 통일하면 정보 손실). 예시 3종: */
.blist-row--sessions { grid-template-columns: 52px 96px 1fr 74px 56px; }  /* 최근 세션: id/프로젝트/작업/상태/시간 */
.blist-row--tasks     { grid-template-columns: 62px 1fr 60px; }           /* 자율 작업 큐: id/설명/배지 */
.blist-row--devtools   { grid-template-columns: 20px 60px 1fr auto; }     /* 개발 도구: 기호/이름/버전/플래그 */
```
셀 색상 토큰: id류(`--text-muted-aa` + `--font-numeric`), 프로젝트/강조 셀(`--accent`), 본문 셀(`--text-secondary`), 상태 텍스트(semantic 토큰), 시간/우측 정렬 셀(`--text-muted-aa` + `--font-numeric` + `tabular-nums`).

### 3.3 `.stat` / `.stat-row` (목업 그대로 승계)
```css
.stat-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(155px, 1fr)); gap: var(--space-2-5); }
.stat { background: var(--panel-bg); border: var(--border-width) solid var(--border); padding: var(--space-2-5) var(--space-3); display: flex; flex-direction: column; gap: var(--space-1); }
.stat-label { font-size: var(--font-size-caption); letter-spacing: var(--letter-spacing-eyebrow); text-transform: uppercase; color: var(--text-muted-aa); }
.stat-value { font-family: var(--font-numeric); font-size: 22px; font-weight: 600; font-variant-numeric: tabular-nums; color: var(--text-primary); display: flex; align-items: baseline; gap: var(--space-1-5); }
.stat-value small { font-size: 11px; font-weight: 500; color: var(--text-muted-aa); }
.stat-trend.up { color: var(--status-good); }
.stat-trend.warn { color: var(--status-warn); }
.stat-trend.flat { color: var(--text-muted-aa); }
```

### 3.4 `.card` (구 `.project-card`/`.home-widget`/`.plugin-card`)
```css
.card { background: var(--panel-bg); border: var(--border-width) solid var(--border); border-radius: var(--radius-0); padding: var(--space-4); display: flex; flex-direction: column; }
.card[data-clickable] { cursor: pointer; }
.card:hover { border-color: var(--accent); }
.card:active { opacity: 0.85; } /* accent가 이미 밝은 톤이라 더 밝힐 여지가 없어 구 시스템의
                                     .back-link:active 선례(opacity 감쇠)를 재사용 */
.card-title { font-size: var(--font-size-body); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.card-desc { color: var(--text-secondary); font-size: var(--font-size-caption); margin-bottom: var(--space-2); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.card-footer { margin-top: auto; padding-top: var(--space-3); border-top: var(--border-width) solid var(--border-soft); display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
```

### 3.5 `.badge` (목업의 currentColor 테두리 방식을 의미색 전체로 확장)
```css
.badge { display: inline-flex; align-items: center; padding: 2px var(--space-1-5); font-size: var(--font-size-caption); letter-spacing: .02em; border: var(--border-width) solid currentColor; border-radius: var(--radius-0); line-height: 1.4; }
.badge-accent { color: var(--accent); }
.badge-good   { color: var(--status-good); }
.badge-warn   { color: var(--status-warn); }
.badge-bad    { color: var(--status-bad); }
.badge-info   { color: var(--ansi-blue); }
.badge-muted  { color: var(--text-muted-aa); }
```
`currentColor` 테두리라 색만 바꾸면 테두리·텍스트가 함께 바뀐다(목업 `.badge.wait`/`.badge.run`과 동일 어법, 확장만 함). 색만으로 의미를 전달하지 않도록 배지 라벨 텍스트(예: "대기"/"완료")는 항상 병기한다.

### 3.6 `.callout` (구 `.alert`, 각종 경고/성공 배너 통합)
```css
.callout {
  display: flex; align-items: center; gap: var(--space-3);
  padding: var(--space-3) var(--space-4); background: var(--panel-bg);
  border: var(--border-width) solid var(--border); border-left-width: var(--border-width-callout);
  font-size: var(--font-size-small); color: var(--text-primary);
}
.callout::before { font-family: var(--font-numeric); flex: none; }
.callout-warn    { border-left-color: var(--status-warn); }    .callout-warn::before    { content: "▲"; color: var(--status-warn); }
.callout-danger  { border-left-color: var(--status-bad); }     .callout-danger::before  { content: "✕"; color: var(--status-bad); }
.callout-success { border-left-color: var(--status-good); }    .callout-success::before { content: "✓"; color: var(--status-good); }
.callout-info    { border-left-color: var(--ansi-blue); }      .callout-info::before    { content: "›"; color: var(--ansi-blue); }
```
글리프(▲/✕/✓/›)는 새로 만든 아이콘이 아니라 목업이 이미 쓰던 `.stat-trend`(▲)·`.tool-flag`(✓/↑) 기호를 그대로 재사용한 것이다.

### 3.7 버튼 `.btn`
```css
.btn {
  display: inline-flex; align-items: center; gap: var(--space-1-5);
  padding: var(--space-2) var(--space-4); border-radius: var(--radius-0);
  font-family: var(--font-display); font-size: var(--font-size-small); font-weight: 500;
  background: var(--panel-bg); border: var(--border-width) solid var(--border-interactive);
  color: var(--text-primary); cursor: pointer; transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}
.btn:hover  { border-color: var(--accent); }
.btn:active { background: var(--accent-dim); }  /* accent-dim은 텍스트 대비는 낮아도(§1.2) 배경 채움에는
                                                     문제 없다 — 위에 얹는 --text-primary는 4.86:1로 AA 통과 */
.btn:disabled { opacity: 0.45; cursor: default; }

.btn-primary { background: var(--accent); border-color: var(--accent); color: var(--win-bg); font-weight: 600; }
.btn-primary:hover  { box-shadow: 0 0 0 3px rgba(95, 208, 220, .22); }  /* 목업의 dot glow 어법 재사용 */
.btn-primary:active { opacity: 0.82; }

.btn-danger { background: var(--status-bad); border-color: var(--status-bad); color: var(--win-bg); font-weight: 600; }
.btn-danger:hover  { box-shadow: 0 0 0 3px rgba(242, 95, 108, .22); }
.btn-danger:active { opacity: 0.82; }

.btn-ghost { background: transparent; border-color: transparent; color: var(--accent); padding: var(--space-1) var(--space-2); }
.btn-ghost:hover { text-decoration: underline; }

.btn-sm { padding: var(--space-1) var(--space-2-5); font-size: var(--font-size-caption); }
```
**주의**: `.btn-primary`/`.btn-danger`는 반드시 `color: var(--win-bg)`(다크 잉크)다. `--text-primary`나 흰색을 올리면 §1.2에서 확인한 대비 실패(1.3~3.2:1)가 재발한다.

### 3.8 입력 / select / textarea `.input`
```css
.input, select.input, textarea.input {
  width: 100%; padding: var(--space-2) var(--space-3); border-radius: var(--radius-0);
  border: var(--border-width) solid var(--border-interactive); background: var(--panel-bg);
  color: var(--text-primary); font-family: var(--font-display); font-size: var(--font-size-small);
}
.input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 2px rgba(95, 208, 220, .15); }
.input.input-error, .input.input-error:focus { border-color: var(--status-bad); }
textarea.input { resize: vertical; min-height: 64px; }
```
- `select`도 동일 클래스를 쓴다(코드 확인: `src/views/settings.ts`/`autonomousTasks.ts`가 이미 select에 `.settings-input`을 그대로 재사용 중 — 새 이름 `.input`으로 이어받으면 된다).
- select의 네이티브 드롭다운 팝업(옵션 목록)은 OS 렌더링 그대로 둔다 — 내부 도구 규모에서 커스텀 화살표/팝업을 새로 만들 이득이 없다는 판단(§7).
- `color-scheme: dark`가 `:root`에 이미 선언돼 있어(목업 정본), 네이티브 select/checkbox/스크롤바가 자동으로 다크 렌더링된다 — 별도 다크모드 오버라이드 불필요.

### 3.9 체크박스 / 토글
```css
input[type="checkbox"] { accent-color: var(--accent); width: 14px; height: 14px; cursor: pointer; }
/* 네이티브 렌더링을 그대로 쓴다(§7) — accent-color만 지정해 브랜드 색과 다크모드를 맞춘다 */

.toggle-track {
  display: inline-flex; align-items: center; width: 36px; height: 20px; padding: 2px;
  border-radius: var(--radius-0); background: var(--panel-bg-alt); border: var(--border-width) solid var(--border-interactive);
  cursor: pointer; transition: background var(--transition-base), border-color var(--transition-base);
}
.toggle-track.on { background: var(--accent); border-color: var(--accent); justify-content: flex-end; }
.toggle-thumb { width: 14px; height: 14px; border-radius: var(--radius-0); background: var(--text-secondary); transition: background var(--transition-base); }
.toggle-track.on .toggle-thumb { background: var(--win-bg); } /* 밝은 트랙 위 다크 thumb — §1.2 버튼과 같은 대비 논리 */
```

### 3.10 모달
```css
.modal-overlay { position: fixed; inset: 0; background: rgba(10, 11, 14, .72); display: flex; align-items: center; justify-content: center; z-index: 1000; padding: var(--space-6); }
/* 스크림 불투명도 .72 — 구 라이트 테마의 .45는 이미 어두운 UI 위에서는 거의 안 보여 실무적으로 올렸다(§7) */
.modal-box { background: var(--panel-bg); border: var(--border-width) solid var(--border); border-radius: var(--radius-0); max-width: 560px; width: 100%; max-height: 80vh; display: flex; flex-direction: column; box-shadow: var(--shadow-float); }
.modal-header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-4) var(--space-5); border-bottom: var(--border-width) solid var(--border); }
.modal-title { font-size: var(--font-size-h2); font-weight: 700; color: var(--text-primary); margin: 0; }
.modal-close-btn { border: none; background: transparent; color: var(--text-muted-aa); font-size: 16px; padding: var(--space-1) var(--space-2); cursor: pointer; }
.modal-close-btn:hover { background: var(--panel-bg-alt); color: var(--text-primary); }
.modal-body { padding: var(--space-4) var(--space-5); overflow-y: auto; }
```

### 3.11 필터 / 세그먼트 (구 `.filter-btn`)
```css
.filter-btn { padding: var(--space-2) var(--space-4); border-radius: var(--radius-0); font-size: var(--font-size-small); border: var(--border-width) solid var(--border-interactive); background: transparent; color: var(--text-secondary); cursor: pointer; }
.filter-btn:hover { border-color: var(--accent); color: var(--text-primary); }
.filter-btn.active { background: var(--accent); border-color: var(--accent); color: var(--win-bg); font-weight: 600; } /* .btn-primary와 동일 대비 논리 재사용 */
```
셸 레벨의 탭(`.tab`/`.tabstrip`, "어떤 탭이 있는가")은 IA 소관이라 이 문서 범위 밖이다. 이 절은 화면 내부 필터·세그먼트 컨트롤만 다룬다.

### 3.12 빈 상태 / 로딩 / 에러
```css
.state-block { text-align: center; padding: var(--space-10) var(--space-4); color: var(--text-secondary); }
.state-block-title { font-weight: 600; color: var(--text-primary); margin-bottom: var(--space-1); }
.state-block-desc { font-size: var(--font-size-caption); color: var(--text-muted-aa); }

.is-refreshing { opacity: 0.55; pointer-events: none; transition: opacity var(--transition-fast); }

.skeleton-line { background: var(--panel-bg-alt); border-radius: var(--radius-0); animation: pulse var(--skeleton-duration); }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }

.spinner { width: 14px; height: 14px; border-radius: 50%; /* 원형 — 상태/모션 표시 예외(§7) */
  border: 2px solid rgba(95, 208, 220, .25); border-top-color: var(--accent); animation: spin .7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

.field-error { font-size: var(--font-size-caption); color: var(--status-bad); }
```

### 3.13 코드 / 로그 블록
```css
.code-block {
  font-family: var(--font-numeric); font-size: var(--font-size-caption); line-height: var(--line-height-base);
  background: var(--statusline-bg); color: var(--text-primary); border: var(--border-width) solid var(--border-soft);
  border-radius: var(--radius-0); padding: var(--space-3) var(--space-4); white-space: pre-wrap; word-break: break-word;
  max-height: 60vh; overflow: auto;
}
```
(구 `.raw-fallback-pre`/`.devtool-panel-command`/`.devtool-log`/`.chat-auth-login-output`/`.file-preview-pre` 전부 이 하나로 통합. 가장 어두운 표면(`--statusline-bg`)을 써서 "원본 출력"이라는 층위를 시각적으로 분리한다.)

### 3.14 채팅 말풍선
```css
.chat-message { display: flex; padding: var(--space-1-5); }
.chat-message.user { justify-content: flex-end; }
.chat-message.assistant { justify-content: flex-start; }
.chat-message.user .chat-bubble { max-width: 70%; background: var(--panel-bg-alt); border: var(--border-width) solid var(--border); border-radius: var(--radius-0); padding: var(--space-2-5) var(--space-4); color: var(--text-primary); }
.chat-message.assistant .chat-message-text { width: 100%; color: var(--text-primary); }
.chat-message-text { font-size: var(--font-size-body-lg); line-height: var(--line-height-loose); white-space: pre-wrap; word-break: break-word; }
.chat-tool-line { padding: var(--space-1) var(--space-1-5) var(--space-1) var(--space-4); font-size: var(--font-size-caption); color: var(--text-muted-aa); }
```

### 3.15 기타

| 구 클래스 | 새 스펙 요지 |
|---|---|
| `.toast`, `.update-countdown-banner`, `.app-login-reopen-banner` | bg `--panel-bg`, border `--border`, `box-shadow: var(--shadow-float)`, radius 0, 텍스트 `--text-primary` |
| `.plugin-update-note` | `.callout-success`/`.callout-danger` 재사용 |
| `.integration-account-avatar` | bg `--accent`, color `--win-bg`(다크 잉크, 버튼과 동일 대비 논리), radius `50%`(아이덴티티 예외) |
| `.bar-track`/`.bar-fill` | track bg `--panel-bg-alt` border `--border-soft`, fill bg `--accent`, height 6px(구 8px에서 축소 — 터미널 정밀도 인상, §7), radius 0(구 pill에서 변경) |
| `.tree-row` | hover bg `--panel-bg-alt`, 선택 bg `--accent-dim` + color `--text-primary` |
| `.tool-usage-chip` | bg `--panel-bg-alt`, border `--border-soft`, color `--text-secondary`, radius 0(구 pill에서 변경) |
| `.page-header`/`.page-title`/`.page-subtitle` | title = `--font-size-h1`/700/`--text-primary`, subtitle = `--font-size-caption`/`--text-secondary` |
| `.task-board`/`.task-board-column`/`.task-run-card` | 컬럼 bg `--win-bg`(리스트보다 한 단계 낮은 층), 카드 bg `--panel-bg` + `border-left: 3px solid` 상태색(running=`--accent`, ok=`--status-good`, fail=`--status-bad`, pending=`--text-muted-aa`) |
| `.live-dot`/`.session-running-dot` | bg `--status-good`, `animation: var(--pulse-duration)`(목업 값 유지) |
| `.login-*`(로그인 화면) | card bg `--panel-bg` border `--border` radius 0, 진입 화면 전용 미세 그라디언트(`--win-bg`→`#101219` 45deg, 절제된 수준)만 허용 — §7 |

---

## 4. `styles.css` 기존 클래스 범주 → 새 스펙 대응표

frontend-dev가 이 표 하나로 883줄 전체를 새 톤으로 옮길 수 있도록, 존재하는 모든 클래스 패밀리를 범주별로 묶었다. "신규 스펙" 열은 위 §2~3의 해당 절을 가리킨다.

| # | 구 `styles.css` 클래스 범주(대표 셀렉터) | 신규 스펙 |
|---|---|---|
| 1 | `.btn`, `.btn-primary`, `.btn-sm`, `.btn-large`, `.filter-btn`(비-active) | §3.7 `.btn`/`.btn-primary` |
| 2 | `.back-link`, `.project-card-link`, `.home-widget-link`, `.sidebar-logout-btn`, `.sidebar-version-check-btn`, `.modal-close-btn` | §3.7 `.btn-ghost` |
| 3 | `.settings-input`(input/select 겸용), `.settings-field`, `.settings-field-label`, `.chat-input-textarea`, `task-form-textarea` | §3.8 `.input` |
| 4 | `.settings-field-error`, `.settings-input-error` | §3.8 `.input-error` + §3.12 `.field-error` |
| 5 | (신규 — 기존 코드에 커스텀 클래스 없이 네이티브 사용) `input[type=checkbox]` | §3.9 체크박스 |
| 6 | `.toggle-track`, `.toggle-thumb` | §3.9 토글 |
| 7 | `.modal-overlay`, `.modal-box`, `.modal-header`, `.modal-title`, `.modal-body` | §3.10 모달 |
| 8 | `.filter-btn.active`, `.filter-group`, `.filter-row` | §3.11 필터/세그먼트 |
| 9 | `.project-grid`/`.project-card*`, `.home-widget-grid`/`.home-widget*`, `.plugin-card*`(카드 부분) | §3.4 `.card` |
| 10 | `.stat-grid`/`.stat-card*` | §3.3 `.stat`(동일 개념, 목업 명칭으로 통합) |
| 11 | `.badge-active`/`-archived`/`-unknown`/`-run-*` | §3.5 `.badge-*` |
| 12 | `.alert`, `.plugin-update-note`, `.global-entry-status.invalid` | §3.6 `.callout-*` |
| 13 | `.state-block*` | §3.12 `.state-block` |
| 14 | `.skeleton-grid`/`.skeleton-card`/`.skeleton-line`, `.is-refreshing`, `.devtool-spinner` | §3.12 로딩(스켈레톤/스피너) |
| 15 | `.session-list`/`.session-row*`, `.task-list`/`.task-row*`, `.mcp-list`/`.mcp-row*`, `.devtool-list`/`.devtool-row*`, `.plugin-entry-list`/`-row`, `.marketplace-*-row`, `.run-history-list`/`-row`, `.thief-table*`, `.session-detail-fields`/`.session-field-row`, `.applink-*`(목록 부분) | §3.2 `.blist`/`.blist-row` |
| 16 | `.overview-card*`, `.overview-section`, `.overview-label*`, `.overview-body`, `.detail-header`/`.detail-title`/`.detail-path`, `.task-detail-grid` | §2 메인 콘텐츠 배경 + §3.1 `.box`(카드형 섹션은 `.box`, 헤더 타이포는 §1.3 `--font-size-h1/h2`) |
| 17 | `.raw-fallback-note`/`-pre`, `.devtool-panel-command`, `.devtool-log`, `.chat-auth-login-output`, `.file-preview-pre` | §3.13 `.code-block` |
| 18 | `.chat-page`, `.chat-header*`, `.chat-title`, `.chat-meta-row`, `.chat-thread`, `.chat-message*`, `.chat-bubble`, `.chat-sample-note`, `.chat-bottom-fixed`, `.chat-input-row`, `.chat-tool-line` | §3.14 채팅 말풍선 + §2 레이아웃 토큰 |
| 19 | `.toast-root`/`.toast`, `.update-countdown-banner*`, `.app-login-reopen-banner`, `.claude-auth-login-modal-body`, `.chat-auth-code-row` | §3.15 토스트/배너 + §3.10 모달(로그인 모달은 모달 스펙 상속) |
| 20 | `.bar-list`/`.bar-row*`/`.bar-track`/`.bar-fill`, `.daily-detail-panel`, `.tool-usage-list`/`-chip` | §3.15 진행률바/칩 |
| 21 | `.project-tree-layout`/`.project-tree-panel`, `.tree-root`/`.tree-row`/`.tree-icon`/`.tree-name`/`.tree-truncated-note` | §3.1 `.box`(패널 컨테이너) + §3.15 `.tree-row` |
| 22 | `.plugin-list`/`.plugin-card*`(헤더/섹션 부분), `.plugin-section-label`, `.global-catalog-*`, `.marketplace-panel`/`.marketplace-repo-*` | §3.4 `.card` + §3.1 `.box-title` 라벨 스타일 |
| 23 | `.integration-panel`/`.integration-account-*` | §3.15 아바타 + §1.2 상태 텍스트(`--status-good`) |
| 24 | `.devtool-panel*`(warn/success/danger 변형) | §3.6 `.callout-warn`/`-success`/`-danger` |
| 25 | `.task-board*`, `.task-run-card*` | §3.15 칸반 |
| 26 | `.live-indicator`, `.live-dot`, `.session-running-dot` | §3.15 상태 dot(목업 dot 패턴 그대로) |
| 27 | `.sidebar*`(다크 사이드바 전체: `-brand`, `-nav-item`, `-subnav*`, `-footer`, `-user-row`, `-version-row` 등) | §2 사이드바 컨테이너(외형만) — 항목 구성은 `terminus-shell-ia.md` |
| 28 | `.page-header`/`.page-title`/`.page-subtitle*` | §3.15 페이지 헤더 |
| 29 | `.login-screen`/`.login-card`/`.login-brand`/`.login-title`/`.login-desc`/`.login-btn`/`.login-note`/`.login-error` | §3.15 로그인 화면 + §3.7 버튼 + §3.6 콜아웃(에러) |
| 30 | `:focus-visible`(전역) | §1.7 포커스 링 |

**대응표 커버리지**: 30개 범주 전부가 §2~3의 신규 스펙 절 중 하나 이상에 매핑되어 있다. 코드에 클래스 없이 네이티브로만 쓰이던 체크박스(범주 5)도 실사용 요소이므로 포함했다.

---

## 5. 폰트 공급 결정 (CSP `default-src 'self'` 대응)

목업의 Google Fonts `<link>`는 실제 앱에서 그대로 쓸 수 없다(외부 리소스 차단). 두 글꼴 트랙을 다르게 처리한다.

**한글 UI 폰트(`--font-display`/`--font-body`) — 번들하지 않는다.** 이 앱의 배포 대상은 macOS/Windows뿐이고(Tech Stack 문서 확인), 두 OS 모두 고품질 한글 시스템 폰트를 기본 내장한다(macOS: Apple SD Gothic Neo, Windows: Malgun Gothic — 폰트 스택에 이미 순서대로 들어 있음). 웹폰트를 굳이 번들하지 않아도 각 OS에서 네이티브 수준으로 렌더링되고, CSP 문제도 원천적으로 사라지며, "데스크톱 앱다움"(OS 네이티브 느낌, 프로젝트 메모리의 "Desktop, not web UI" 방향과도 일치)도 강화된다. 목업의 Noto Sans KR 링크는 제거하고, 스택 안의 `'Noto Sans KR'`은 (혹시 있을 폰트 부재 상황의) 무해한 폴백 이름으로만 남긴다 — 실제로 이 이름의 파일을 로드하지 않으므로 매칭 실패 시 다음 스택 항목(OS 폰트)으로 자연히 넘어간다.

**숫자/버전 전용 모노스페이스(`--font-numeric`, JetBrains Mono) — 로컬 번들한다.** 이 글꼴은 두 번의 반려 끝에 확정된 방침(leaf 숫자 요소 전용)이라 OS 기본 모노(SF Mono/Menlo vs Consolas)로 대체하면 mac/Windows 간 자간·격자폭이 달라져 `tabular-nums`로 맞춘 숫자 정렬 의도가 흔들린다. 대신 번들 부담을 최소화한다:
- **Latin+숫자 서브셋만**(한글이 절대 섞이지 않는 leaf 전용이므로 한글 글리프 불필요) — 서브셋 시 웨이트당 20~30KB대(WOFF2)로 가벼움.
- **실사용 웨이트 2개만**: Regular 400(기본), SemiBold 600(`.stat-value`, `.usage-figure .v`). 500/700은 목업에 실사용처가 없어 번들하지 않는다.
- 파일 위치: `src/assets/fonts/JetBrainsMono-Regular-latin.woff2`, `JetBrainsMono-SemiBold-latin.woff2` — Vite가 빌드 시 해시를 붙여 앱 번들(same-origin) 안에 포함시키므로 `default-src 'self'`를 그대로 만족한다(CSP 변경 불필요).
- 라이선스: OFL-1.1(재배포·번들링 명시적으로 허용) — 저장소가 public이므로 `OFL.txt`를 폰트 파일과 함께 커밋한다.
```css
@font-face {
  font-family: 'JetBrains Mono';
  src: url('./assets/fonts/JetBrainsMono-Regular-latin.woff2') format('woff2');
  font-weight: 400; font-style: normal; font-display: swap;
  unicode-range: U+0000-00FF, U+2018-201F, U+2022, U+2039-203A;
}
@font-face {
  font-family: 'JetBrains Mono';
  src: url('./assets/fonts/JetBrainsMono-SemiBold-latin.woff2') format('woff2');
  font-weight: 600; font-style: normal; font-display: swap;
  unicode-range: U+0000-00FF, U+2018-201F, U+2022, U+2039-203A;
}
```

---

## 6. ux-designer 산출물 → 수치 매핑 (템플릿)

`docs/design/terminus-shell-ia.md`(ux-designer, 진행 중)가 화면별 `우선순위:`/`밀도:`/`동선:` 필드를 제공하면 아래 표로 수치화한다. 이 프로젝트의 밀도 등급은 3단계(고/중/저), spacing scale은 §1.4의 13단계 중 화면 성격에 따라 3개 대역(고밀도=`--space-1`~`--space-2-5`, 중밀도=`--space-3`~`--space-4`, 저밀도=`--space-5`~`--space-8`)으로 묶어 대응시킨다.

| ux-designer 필드 | visual-designer 변환(이 프로젝트 적용값) |
|---|---|
| 우선순위 1순위 | `--font-size-h2`(15px) + `font-weight:700` |
| 우선순위 2순위 | `--font-size-body`(13px) + `font-weight:600` |
| 우선순위 3순위 이하 | `--font-size-small`/`--font-size-caption` + `font-weight:400` |
| 밀도: 고 | spacing 고밀도 대역(`--space-1`~`--space-2-5`), `.blist-row` 위주 |
| 밀도: 중 | spacing 중밀도 대역(`--space-3`~`--space-4`), `.box`/`.card` 기본 패딩 |
| 밀도: 저 | spacing 저밀도 대역(`--space-5`~`--space-8`), 진입 화면형 넓은 여백 |
| 동선(순서) | 순번이 빠른 요소를 DOM/시각 순서상 앞에 배치, 다음 단계는 `--accent` 강조 또는 §3.6 콜아웃 화살표 글리프(`›`)로 유도 |

현재 `terminus-shell-ia.md`가 아직 없어 실값은 비어 있다 — 나오는 대로 이 표를 채운다.

---

## 7. 판단이 애매했던 점 (PM/ux-designer 확인 권장)

1. **라운드=0 규칙의 외삽 범위**: 목업 1장(홈 화면)만 봐서는 카드/모달/배지/토글처럼 목업에 아예 없던 컴포넌트까지 "사각형" 규칙을 적용해도 되는지 확정된 근거는 아니다. 다만 목업에서 관찰되는 일관성(패널·통계·탭·배지 전부 라운드 0, 원은 상태 dot뿐)이 100%였기 때문에 신뢰도 높은 외삽으로 보고 전역 규칙으로 확정했다 — 다른 9개 화면 시안이 나오면 재확인 권장.
2. **모달 스크림 불투명도 0.72**: WCAG 수치 기준이 아니라 실무적 판단(어두운 배경 위에서 구 시스템의 0.45는 사실상 안 보임)이다. 정확한 값은 실제 화면 캡처 후 조정될 수 있다.
3. **`--text-muted`의 용도 제한**: HEX 값 자체는 목업 정본을 바꾸지 않았지만("그대로 옮길 것" 제약 충족), "텍스트로 쓰지 않는다"는 사용처 제약은 이 문서에서 새로 정의한 것이다 — 목업 저자의 원래 의도와 다를 수 있어 확인 권장.
4. **진행률바 두께 6px(구 8px)**: 근거가 약한 미세 조정이라 frontend-dev 재량으로 8px 유지도 무방.
5. **select 네이티브 팝업 미개조**: 내부 도구 규모에 비해 커스텀 드롭다운 구축 비용이 크다고 판단해 브라우저 기본에 맡겼다 — 추후 불만이 나오면 재검토.
6. **레퍼런스 벤치마킹 생략**: 이 작업은 새 방향을 발굴하는 게 아니라 이미 확정된 `terminus-mockup.html`(Terminus/Tabby를 명시적으로 참조해 만들어진 시안)을 10개 화면으로 확장하는 것이라, GDWEB/dbcut/Awwwards/ThemeForest 착수전/완성후 캡처 대조는 이번 라운드에서 별도로 수행하지 않았다. 신규 화면 시안이 나오는 단계에서 필요하면 그때 수행하는 편이 적절하다고 판단했다.

---

## 8. 사이드바 탭별 콘텐츠 신규 요소 (배치 A, `terminus-shell-ia.md` §7 대응)

IA §7이 "시각 스펙 없음"으로 남겨둔 6종을 여기서 확정한다. 원칙은 §7 본문이
이미 힌트를 준 대로 **목업의 `.sidebar-head`/`.conn`/`.conn.current` 어휘와
기존 토큰에서 파생**하고, 새 색상은 도입하지 않는다.

1. **좁은 폭 목록 행** — 새 클래스를 만들지 않았다. 목업의 `.conn`(=현재
   `styles.css` `.ws-row`)이 요구사항("아이콘/dot 1개 + 제목 1줄 + 보조메타
   1줄")과 구조가 정확히 일치해 세션·자율작업 큐·개발도구·앱링크·사용량(최근
   활동일) 사이드바 전부 `.ws-row`를 그대로 재사용한다(`src/sidebar.ts`
   `narrowRow()`). `.blist-row--sessions` 같은 본문용 그리드 행을 축소하는
   대신 애초에 다른 컴포넌트(카드형 2줄 행)를 재사용한 것이 이번 결정이다.
2. **그룹 헤더** — `.sidebar-head`(최상단 섹션 헤더)와 같은 타이포를 그대로
   쓰되, 목록 중간에 다시 나올 때 위 여백만 다르므로 `.sidebar-head.group`
   수정자(`margin-top: var(--space-2)`)만 추가했다. 개발 도구 "필수 도구"/
   "선택 도구", 자율작업 "작업 큐", 사용량 "최근 활동일"이 이 클래스를 쓴다.
3. **정적 nav 행의 활성 강조** — `.filter-btn.active`(배경 전체 채움, accent
   배경+다크 텍스트)는 세로로 촘촘한 목록에서 과하다고 판단해 채택하지 않았다.
   대신 `.ws-row.current`가 이미 쓰던 "좌측 강조선(`--border-width-accent`
   solid `--accent`) + 한 단 밝은 배경(`--panel-bg`)" 어법을 그대로 옮긴
   `.sidebar-nav-row.current`를 새로 정의했다 — 새 색 없이 기존 활성 강조
   문법을 재사용한 것. 설정 5종·카탈로그 2종·자율작업 뷰 전환 2종·사용량 기간
   토글(7일/30일)이 이 클래스를 쓴다.
4. **카운트 배지** — PM 결정(IA §8-①)으로 카탈로그·설정 nav 행에 카운트 배지를
   두지 않기로 확정되어, 배치 A 시점 기준 실제로 이 요소를 쓰는 사이드바가
   없다. 필요해지면 `.box-head .count`(§3.1)와 동일한 토큰(`--text-muted-aa`,
   `--font-size-caption`)으로 `.sidebar-nav-row`에 우측 정렬 배지를 추가하면
   된다 — 별도 새 클래스를 미리 정의하지 않았다(쓰이지 않는 CSS를 남기지
   않기 위함).
5. **개발 도구 행의 압축 상태 표시** — `.blist-devtool-flag`(본문용, "✓
   설치됨"/"미설치" 텍스트 포함)는 190px 사이드바에서 잘린다. 별도 아이콘
   요소를 추가하는 대신 행이 이미 갖고 있는 dot 채널의 색으로 설치 여부를
   표현한다: 설치됨=`good`(초록), 필수인데 미설치=`bad`(빨강, 신규 추가),
   선택인데 미설치=`idle`(무채색). 새 아이콘 자산 없이 기존 semantic 색
   토큰만 재배정했다.
6. **사이드바 일시 강조** — 개발 도구 사이드바 행 클릭 시 본문의 해당 행
   (`#devtool-row-<id>`)으로 스크롤한 뒤 `.flash-highlight` 클래스를 1.2초
   붙인다(`background: var(--accent-dim)` → 원래 표면으로 감쇠). 지속 시간은
   `--transition-slow`(320ms)류 UI 피드백보다 눈에 띄게 길게(1.2s) 잡아
   "스크롤 이동이 끝난 자리"를 사용자가 놓치지 않게 했다. `prefers-reduced-
   motion: reduce`는 이 문서 §1.6이 이미 전역으로 `animation-duration`을
   0.01ms로 누르므로 별도 예외 처리 없이 자동으로 존중된다.

**새로 추가된 dot 색상 변형**: 기존 `.ws-row .dot`은 good/warn/idle 3종만
있었다. 위 5·자율작업 진행중 표시를 위해 `bad`(`--status-bad`)·`accent`
(`--accent`, `home.ts`의 `.blist-badge.run`과 동일 의미)를 추가했다 — 둘 다
§1.1에 이미 있던 semantic 토큰이라 팔레트에 새 HEX는 없다.
