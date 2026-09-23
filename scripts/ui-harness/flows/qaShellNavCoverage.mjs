// QA 독립검증 전용 흐름 — main 브랜치 구 사이드바가 제공하던 모든 사용자
// 진입점이 Terminus 셸(탭스트립+맥락 사이드바+상태줄)에서도 실제 포인터
// 클릭(Playwright locator.click())으로 도달·실행 가능한지 확인한다.
//
// 기존 흐름(autonomousTasks/sessions/catalog/settingsMcp/appLinks/
// formBackgroundRerender/majorReview20260921/updateCheck/home/tabstripOverflow)이
// 커버하지 않는 갭만 메운다 — 중복 검증은 피한다:
//   - 탭스트립 9탭 전체 클릭 도달성(기존 흐름은 일부만 hash 직접 이동으로 검증)
//   - 프로젝트 사이드바(워크스페이스 목록 행 클릭, "워크스페이스 경로 관리" 푸터)
//   - 세션 사이드바 목록 행 클릭(모달이 아니라 좁은 폭 목록 행 자체)
//   - 사용량 탭 재클릭 시 get_daily_usage 재조회 트리거
//   - 개발 도구 사이드바 행 클릭 → highlightDevTool (devtool-row-* 스크롤/하이라이트)
//   - 자율 작업 사이드바 뷰 전환(목록 ↔ 진행상황판)
//   - 카탈로그 사이드바 nav 행(플러그인 카탈로그 ↔ 전역 카탈로그)
//   - 앱 링크 사이드바 퀵오픈 행 클릭 → app_links_open invoke
//   - 설정 사이드바 nav 행 5종(OTel/GitHub/Cloudflare/마켓플레이스/MCP 관리) 전체 순회
export const flowId = 'qaShellNavCoverage';
export const startHash = '#/';

export function scenarios(base) {
  return [
    {
      id: 'golden',
      // list_project_tree — 어떤 기존 흐름도 프로젝트 상세(#/project/*)를 실제
      // 클릭으로 방문하지 않아 기존 base 픽스처에 이 커맨드가 아예 없었다
      // (실측: 이 흐름 최초 실행 시 UNSTUBBED_COMMAND로 확인). 프로젝트 상세는
      // 워크스페이스 사이드바 행 클릭의 도착지라 실제 클릭 경로를 검증하려면
      // 필요하다.
      fixtures: {
        ...base,
        app_links_open: null,
        list_project_tree: [{ name: 'src', relativePath: 'src', isDirectory: true, children: null, truncated: false }],
      },
      async run(page, { shot, bugs }) {
        // ---- 0) 탭스트립 9탭 라벨·클릭 도달성 ----
        const expectedTabs = ['홈', '프로젝트', '세션', '사용량', '개발 도구', '자율 작업', '카탈로그', '앱 링크', '설정'];
        const tabLocator = page.locator('.tabstrip-tabs .tab');
        const tabCount = await tabLocator.count();
        if (tabCount !== expectedTabs.length) {
          bugs.push({ severity: 'Critical', symptom: `탭스트립 탭 개수가 ${expectedTabs.length}개가 아님(실제 ${tabCount}) — main 사이드바 진입점 대비 누락/추가 의심`, file: 'src/sidebar.ts:renderTabstrip' });
        }
        for (const label of expectedTabs) {
          const tab = page.locator('.tabstrip-tabs .tab', { hasText: label });
          if ((await tab.count()) === 0) {
            bugs.push({ severity: 'Critical', symptom: `탭스트립에 "${label}" 탭이 없음`, file: 'src/sidebar.ts:renderTabstrip' });
          }
        }
        await shot('00-tabstrip');

        // ---- 1) 프로젝트 탭: 워크스페이스 사이드바 목록 행 클릭 ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '프로젝트' }).click();
        await page.waitForTimeout(250);
        const wsRows = page.locator('.sidebar .ws-row');
        const wsCount = await wsRows.count();
        if (wsCount === 0) {
          bugs.push({ severity: 'Major', symptom: '프로젝트 탭 사이드바에 워크스페이스 행이 0개(실 로컬 데이터 이슈일 수 있음)', file: 'scripts/ui-harness/flows/qaShellNavCoverage.mjs' });
        } else {
          await wsRows.first().click();
          await page.waitForTimeout(250);
          if (!/^#\/project\//.test(await page.evaluate(() => window.location.hash))) {
            bugs.push({ severity: 'Critical', symptom: '프로젝트 사이드바 워크스페이스 행을 클릭해도 프로젝트 상세로 이동하지 않음', file: 'src/sidebar.ts:renderWorkspaceSidebar' });
          }
          await shot('01-project-detail-via-sidebar-row');
        }

        // ---- 2) "워크스페이스 경로 관리" 푸터 → 편집 폼 열림 ----
        await page.locator('.sidebar .sidebar-foot', { hasText: '워크스페이스 경로 관리' }).click();
        await page.waitForTimeout(250);
        if ((await page.locator('#malgn-config-workspaces').count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '"워크스페이스 경로 관리" 클릭해도 workspaces 편집 textarea가 열리지 않음', file: 'src/sidebar.ts:openWorkspacesManager' });
        }
        await shot('02-workspaces-editor');
        // 모달을 닫아야 다음 탭 클릭이 가로채이지 않는다(ESC로 닫힘 — projects.ts
        // workspacesModalEscHandler).
        await page.keyboard.press('Escape');
        await page.waitForTimeout(150);
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({ severity: 'Major', symptom: 'ESC를 눌러도 워크스페이스 편집 모달이 닫히지 않음', file: 'src/views/projects.ts:closeWorkspacesModal' });
        }

        // ---- 3) 세션 탭: 사이드바 좁은 폭 목록 행 클릭 ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '세션' }).click();
        await page.waitForTimeout(250);
        const sessionRows = page.locator('.sidebar .ws-row');
        const sessionRowCount = await sessionRows.count();
        if (sessionRowCount === 0) {
          bugs.push({ severity: 'Major', symptom: '세션 탭 사이드바에 세션 행이 0개(실 로컬 데이터 이슈일 수 있음)', file: 'scripts/ui-harness/flows/qaShellNavCoverage.mjs' });
        } else {
          await sessionRows.first().click();
          await page.waitForTimeout(250);
          if (!/^#\/sessions\//.test(await page.evaluate(() => window.location.hash))) {
            bugs.push({ severity: 'Critical', symptom: '세션 사이드바 목록 행을 클릭해도 세션 상세로 이동하지 않음', file: 'src/sidebar.ts:renderSessionsSidebar' });
          }
          await shot('03-session-detail-via-sidebar-row');
        }

        // ---- 4) 사용량 탭: 재클릭 시 get_daily_usage 재조회 ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '사용량' }).click();
        await page.waitForTimeout(300);
        const countAfterFirst = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'get_daily_usage').length);
        // 이미 #/usage에 있으면 해시가 안 바뀌므로 탭을 다시 눌러 재조회 트리거를 직접 검증.
        await page.locator('.tabstrip-tabs .tab', { hasText: '사용량' }).click();
        await page.waitForTimeout(300);
        const countAfterSecond = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'get_daily_usage').length);
        if (countAfterSecond <= countAfterFirst) {
          bugs.push({ severity: 'Major', symptom: `사용량 탭을 이미 그 탭에 있는 상태에서 다시 눌러도 get_daily_usage가 재호출되지 않음(before=${countAfterFirst}, after=${countAfterSecond})`, file: 'src/sidebar.ts:renderTabstrip 사용량' });
        }
        await shot('04-usage-refetch');

        // ---- 5) 개발 도구 탭: 사이드바 행 클릭 → highlightDevTool ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '개발 도구' }).click();
        await page.waitForTimeout(250);
        const devToolRows = page.locator('.sidebar .ws-row');
        const devToolCount = await devToolRows.count();
        if (devToolCount === 0) {
          bugs.push({ severity: 'Major', symptom: '개발 도구 탭 사이드바에 행이 0개', file: 'scripts/ui-harness/flows/qaShellNavCoverage.mjs' });
        } else {
          await devToolRows.first().click();
          await page.waitForTimeout(150);
          const highlighted = await page.evaluate(() => document.querySelectorAll('[id^="devtool-row-"].flash-highlight').length);
          if (highlighted === 0) {
            bugs.push({ severity: 'Minor', symptom: '개발 도구 사이드바 행 클릭 후 본문 목록에 .flash-highlight 강조가 적용되지 않음', file: 'src/views/devTools.ts:highlightDevTool' });
          }
          await shot('05-devtool-highlight');
        }

        // ---- 6) 자율 작업 탭: 목록 ↔ 진행상황판 사이드바 뷰 전환 ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '자율 작업' }).click();
        await page.waitForTimeout(250);
        await page.locator('.sidebar .sidebar-nav-row', { hasText: '진행상황판' }).click();
        await page.waitForTimeout(200);
        if ((await page.evaluate(() => window.location.hash)) !== '#/tasks/board') {
          bugs.push({ severity: 'Major', symptom: '자율 작업 사이드바 "진행상황판" 클릭이 #/tasks/board로 이동하지 않음', file: 'src/sidebar.ts:renderTasksSidebar' });
        }
        await shot('06-tasks-board-via-sidebar');
        await page.locator('.sidebar .sidebar-nav-row', { hasText: '목록' }).click();
        await page.waitForTimeout(200);
        if ((await page.evaluate(() => window.location.hash)) !== '#/tasks') {
          bugs.push({ severity: 'Major', symptom: '자율 작업 사이드바 "목록" 클릭이 #/tasks로 되돌아가지 않음', file: 'src/sidebar.ts:renderTasksSidebar' });
        }

        // ---- 7) 카탈로그 탭: 사이드바 nav 행(플러그인 ↔ 전역) ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '카탈로그' }).click();
        await page.waitForTimeout(250);
        await page.locator('.sidebar .sidebar-nav-row', { hasText: '전역 카탈로그' }).click();
        await page.waitForTimeout(200);
        if ((await page.evaluate(() => window.location.hash)) !== '#/catalog/global') {
          bugs.push({ severity: 'Major', symptom: '카탈로그 사이드바 "전역 카탈로그" 클릭이 #/catalog/global로 이동하지 않음', file: 'src/sidebar.ts:renderCatalogSidebar' });
        }
        await shot('07-catalog-global-via-sidebar');
        await page.locator('.sidebar .sidebar-nav-row', { hasText: '플러그인 카탈로그' }).click();
        await page.waitForTimeout(200);
        if ((await page.evaluate(() => window.location.hash)) !== '#/catalog/plugins') {
          bugs.push({ severity: 'Major', symptom: '카탈로그 사이드바 "플러그인 카탈로그" 클릭이 #/catalog/plugins로 되돌아가지 않음', file: 'src/sidebar.ts:renderCatalogSidebar' });
        }

        // ---- 8) 앱 링크 탭: 사이드바 퀵오픈 행 클릭 → app_links_open invoke ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '앱 링크' }).click();
        await page.waitForTimeout(250);
        const appLinkRows = page.locator('.sidebar .ws-row');
        const appLinkRowCount = await appLinkRows.count();
        if (appLinkRowCount === 0) {
          bugs.push({ severity: 'Minor', symptom: '앱 링크 탭 사이드바에 퀵오픈 행이 0개(fixture에 활성 링크가 없을 수 있음)', file: 'scripts/ui-harness/flows/qaShellNavCoverage.mjs' });
        } else {
          await appLinkRows.first().click();
          await page.waitForTimeout(150);
          const openCalls = await page.evaluate(() => (window.__invokeLog ?? []).filter((e) => e.cmd === 'app_links_open').length);
          if (openCalls === 0) {
            bugs.push({ severity: 'Major', symptom: '앱 링크 사이드바 퀵오픈 행 클릭이 app_links_open IPC를 호출하지 않음', file: 'src/sidebar.ts:renderAppLinksSidebar / src/views/appLinks.ts:openLink' });
          }
        }
        await shot('08-applink-quickopen');

        // ---- 9) 설정 탭: 사이드바 nav 행 5종 전체 순회 ----
        await page.locator('.tabstrip-tabs .tab', { hasText: '설정' }).click();
        await page.waitForTimeout(250);
        const settingsSubTabs = ['OTel 설정', 'GitHub 설정', 'Cloudflare 설정', '마켓플레이스 설정', 'MCP 관리'];
        for (const label of settingsSubTabs) {
          const row = page.locator('.sidebar .sidebar-nav-row', { hasText: label });
          if ((await row.count()) === 0) {
            bugs.push({ severity: 'Critical', symptom: `설정 사이드바에 "${label}" 항목이 없음`, file: 'src/sidebar.ts:renderSettingsSidebar' });
            continue;
          }
          await row.click();
          await page.waitForTimeout(200);
          const subtitle = await page.locator('.page-subtitle').first().textContent().catch(() => null);
          if (!subtitle || !subtitle.includes(label)) {
            bugs.push({ severity: 'Major', symptom: `설정 사이드바 "${label}" 클릭 후 page-subtitle이 일치하지 않음(실제: ${subtitle})`, file: 'src/views/settings.ts:renderSettingsView' });
          }
        }
        await shot('09-settings-subtabs-done');
      },
    },
    // ---- 경계 상황 A: 최소 창 900×600에서 9탭 전부 가로 오버플로 없는지 ----
    {
      id: 'boundary-900x600-no-horizontal-overflow',
      viewport: { width: 900, height: 600 },
      fixtures: { ...base, list_project_tree: [{ name: 'src', relativePath: 'src', isDirectory: true, children: null, truncated: false }] },
      async run(page, { shot, bugs }) {
        const tabLabels = ['홈', '프로젝트', '세션', '사용량', '개발 도구', '자율 작업', '카탈로그', '앱 링크', '설정'];
        for (const label of tabLabels) {
          await page.locator('.tabstrip-tabs .tab', { hasText: label }).click();
          await page.waitForTimeout(200);
          const overflowInfo = await page.evaluate(() => ({
            docScrollWidth: document.documentElement.scrollWidth,
            innerWidth: window.innerWidth,
          }));
          if (overflowInfo.docScrollWidth > overflowInfo.innerWidth + 1) {
            bugs.push({
              severity: 'Minor',
              symptom: `900×600에서 "${label}" 탭 문서 가로 오버플로(scrollWidth=${overflowInfo.docScrollWidth} > innerWidth=${overflowInfo.innerWidth})`,
              file: 'src/styles.css',
            });
          }
        }
        await shot('01-900x600-last-tab');
      },
    },
    // ---- 경계 상황 B: 키보드 포커스 가시성 — Tab 이동 시 :focus-visible 포커스 링 ----
    {
      id: 'boundary-keyboard-focus-visibility',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        // body에서 시작해 Tab을 여러 번 눌러 실제 인터랙티브 요소로 포커스가
        // 이동하는지, 그 요소가 focus-visible 스타일(box-shadow)을 실제로
        // 받는지 확인한다.
        await page.evaluate(() => document.body.focus());
        let foundVisibleRing = false;
        for (let i = 0; i < 8; i++) {
          await page.keyboard.press('Tab');
          const info = await page.evaluate(() => {
            const ae = document.activeElement;
            if (!ae || ae === document.body) return null;
            const cs = getComputedStyle(ae);
            return { tag: ae.tagName, cls: ae.className, boxShadow: cs.boxShadow, outline: cs.outline };
          });
          if (info && info.boxShadow && info.boxShadow !== 'none') {
            foundVisibleRing = true;
            break;
          }
        }
        await shot('01-after-tabbing');
        if (!foundVisibleRing) {
          bugs.push({
            severity: 'Minor',
            symptom: 'Tab 키로 8회 이동해도 :focus-visible box-shadow 포커스 링이 관측되지 않음(포커스 가시성 회귀 의심 — 요소 순서/CSS 특이도 확인 필요)',
            file: 'src/styles.css:focus-visible',
          });
        }
      },
    },
    // ---- 경계 상황 C: prefers-reduced-motion — 전역 애니메이션/트랜지션 무력화 ----
    {
      id: 'boundary-prefers-reduced-motion',
      reducedMotion: 'reduce',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const result = await page.evaluate(() => {
          const matches = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
          const offenders = [];
          for (const elx of Array.from(document.querySelectorAll('*'))) {
            const cs = getComputedStyle(elx);
            const dur = cs.transitionDuration;
            // "0.01ms" 등 styles.css 전역 규칙이 적용된 값은 통과, 그 외 유의미한(>0) 값만 수집.
            if (dur && dur !== '0s' && !/^0(\.\d+)?ms$/.test(dur) && parseFloat(dur) > 0.02) {
              offenders.push({ tag: elx.tagName, cls: String(elx.className).slice(0, 60), dur });
            }
          }
          return { matches, offenders: offenders.slice(0, 5), offenderCount: offenders.length };
        });
        await shot('01-reduced-motion');
        if (!result.matches) {
          bugs.push({ severity: 'Minor', symptom: 'Playwright reducedMotion 컨텍스트인데 matchMedia(prefers-reduced-motion:reduce)가 false — 하네스/환경 이슈일 수 있음', file: 'scripts/ui-harness/flows/qaShellNavCoverage.mjs' });
        }
        if (result.offenderCount > 0) {
          bugs.push({
            severity: 'Minor',
            symptom: `prefers-reduced-motion:reduce인데도 transition-duration이 0에 가깝지 않은 요소 ${result.offenderCount}개 발견 — 예: ${JSON.stringify(result.offenders[0])}`,
            file: 'src/styles.css:@media (prefers-reduced-motion: reduce)',
          });
        }
      },
    },
  ];
}
