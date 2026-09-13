# 캡처 하네스 (scripts/capture/)

앱의 모든 라우트 × 시나리오(정상/빈 상태/에러/로딩) 조합을 PNG로 남기는 재사용 캡처
하네스. UI/UX 리뷰(라운드 1~3)의 근거 수집과 before/after 대조에 공용으로 쓴다.

## 왜 `bin/capture.mjs`(공용 캡처 스킬)가 아니라 별도 스크립트인가

이 앱은 `window.__TAURI_INTERNALS__`가 없으면 모든 화면이 빈 채로 남는다(Tauri
데스크톱 앱). `bin/capture.mjs`는 이 객체를 주입·스텁하는 기능이 없어 이 앱에는
그대로 쓸 수 없다 — 그래서 이 디렉터리에 Tauri invoke 스텁 + 로그인 실행 + 시나리오별
픽스처 주입을 포함한 전용 하네스를 둔다. 뷰포트(1440×900)·풀페이지 캡처 등 캡처
관례는 조직 표준(Skill `common-screen-verification-and-capture`)을 따른다.

## 사전 조건

- `pnpm install` 1회 (playwright는 이미 devDependency로 있음)
- `pnpm exec playwright install chromium` 1회
- Vite dev 서버가 `http://localhost:5173`에서 떠 있으면 그대로 재사용한다. 없으면
  하네스가 `pnpm exec vite --port 5173`을 자동으로 새로 띄운다(기존 포트를 죽이지
  않는다).

## 실행

```bash
node scripts/capture/harness.mjs --round r1
```

옵션:
- `--round <이름>`: 출력 서브디렉터리 이름이자 파일명 접두사 (기본 `r1`). 라운드별로
  구분해 before/after를 나란히 보관하려면 `--round r2-after`처럼 바꿔 실행한다.
- `--label <문자열>`: 매니페스트에만 기록되는 자유 메모(선택).
- `--base-url <url>`: 기본 `http://localhost:5173`.

## 산출물

- `scripts/capture/output/<round>/<round>__<routeId>__<scenario>.png` — 결정적 파일명
  (같은 `--round`로 재실행하면 같은 이름을 덮어써 diff 도구로 바로 비교 가능).
- `scripts/capture/output/<round>/manifest.json` — 캡처 목록 + 각 캡처의 검증 결과
  (`ok`/`note`/`quirk`) + pageerror 로그 + 총 캡처/성공/실패 수.
- 이 출력 디렉터리는 `.gitignore`에 등록되어 있다 — 커밋되지 않는다.

## 커버리지

`src/route.ts`를 정본으로 라우트 18개(사이드바 8메뉴 + 프로젝트 상세·세션
상세/draft·자율업무 board/상세·설정 6탭) + 로그인 화면을 다룬다
(`scripts/capture/routes.mjs`). 라우트가 추가/변경되면 이 파일에 항목을 추가한다.

시나리오는 4가지:
- **normal**: 타입에 맞는 현실적인 한국어 픽스처(`scripts/capture/fixtures.mjs`)로
  채운 정상 데이터 상태.
- **empty**: 목록/조회 결과가 0건인 상태.
- **error**: 해당 화면이 의존하는 Tauri 커맨드가 reject하는 상태(로그인 자체는
  항상 성공 — 그래야 로그인 이후 화면들의 에러 처리를 볼 수 있다).
- **loading**: 로그인만 즉시 통과시키고 이후 모든 데이터 커맨드 응답을 캡처 구간
  내내 보류해 "불러오는 중" 상태를 붙잡아 둔다.

로그인 화면 자체는 별도로 normal/loading/error 3종만 캡처한다(빈 상태 개념이 없음).

## 검증 방식

`routes.mjs`의 각 라우트는 스크린샷 직전 실제 DOM에서 기대 콘텐츠(카드 개수, 특정
문구, `.alert`/로딩 마커 존재 등)를 확인하는 `verify()`를 갖는다 — "찍기만 하고
끝"내지 않는다. `ok:false`면 실행 로그와 매니페스트에 실패로 남는다.

일부 화면은 시나리오를 구분하지 않고 항상 같은 결과를 보여주는 것이 **앱의 실제
동작**이다(예: 세션 draft 화면은 진입 시 invoke가 없어 시나리오와 무관하게 항상
같은 초기 화면을 보여준다, MCP 관리는 등록 서버가 0개여도 별도 소스인 카탈로그
행이 항상 함께 렌더된다). 이런 경우 `verify()`가 `quirk:true`로 표시해 캡처
자체는 성공 처리하되, 매니페스트에 근거를 남긴다. `quirk`는 실패(`ok:false`)와
다르다 — src/ 수정 없이 관찰한 사실이다.

## 시나리오/픽스처 데이터 갱신

- 새 Tauri 커맨드가 추가되면 `fixtures.mjs`의 `READ_FIXTURES`(조회) 또는
  `ACTION_DEFAULTS`(쓰기/실행)에 타입에 맞는 값을 추가한다.
- `stub.mjs`는 순수 배관(딜레이·resolve/reject)만 담당한다 — 데이터는 건드리지 않는다.
- 새 라우트가 추가되면 `routes.mjs`에 `{ id, hash, label, verify }`를 추가한다.
