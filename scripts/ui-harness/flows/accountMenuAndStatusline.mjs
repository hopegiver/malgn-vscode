// 흐름 — 리뷰(2026-09-24) M1(로그아웃→재로그인 첫 클릭 소실)·M1-b(메뉴 열린 채
// 탭 클릭 흡수)·m6(계정 메뉴 Escape)·M2(900px/1180px에서 상태줄 업데이트
// 배지·"새 버전 확인"·시계가 항상 뷰포트 안에 있고 클릭 가능) 수정을 실제
// 포인터 클릭(Playwright locator.click())으로 검증한다. 전부 main 브랜치에는
// 없던 Terminus 셸(탭스트립 계정 칩) 전용 시나리오라 기존 흐름과 겹치지 않는다.
//
// M1/M1-b 재현 근거: src/sidebar.ts의 logoutItem이 chip(clickable) 안쪽 자식이라
// stopPropagation 없이는 클릭이 chip까지 버블링해 방금 닫은 메뉴를 다시 연다
// (재현 로그는 docs/reviewer/review-ui-terminus-overhaul.md "M1 재현 절" 참고).
// M2 재현 근거: .statusline이 overflow:hidden 단일 flex 행이라 900px에서
// 오른쪽 세그먼트(업데이트 적용/새 버전 확인/시계)가 뷰포트 밖으로 밀려났다.
export const flowId = 'accountMenuAndStatusline';
export const startHash = '#/';

// 세 요소(업데이트 배지·새 버전 확인 버튼·시계) 모두 "뷰포트 안에 있고, 그
// 좌표를 다른 요소가 가리지 않는다(=실제로 클릭 가능하다)"를 함께 확인한다.
async function assertFullyVisibleAndClickable(page, bugs, selector, label) {
  const locator = page.locator(selector).first();
  if ((await locator.count()) === 0) {
    bugs.push({ severity: 'Critical', symptom: `[M2] ${label}(${selector})이 DOM에 없음`, file: 'src/sidebar.ts:renderStatusline' });
    return;
  }
  const result = await page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const atPoint = document.elementFromPoint(cx, cy);
    return {
      rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height },
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      selfOrDescendant: !!atPoint && (atPoint === el || el.contains(atPoint)),
      atPointTag: atPoint ? atPoint.tagName + '.' + (atPoint.className || '') : null,
    };
  }, selector);
  if (!result) {
    bugs.push({ severity: 'Critical', symptom: `[M2] ${label} 좌표 측정 실패`, file: 'src/sidebar.ts:renderStatusline' });
    return;
  }
  const { rect, innerWidth, innerHeight, selfOrDescendant, atPointTag } = result;
  if (rect.width === 0 || rect.height === 0) {
    bugs.push({ severity: 'Major', symptom: `[M2] ${label}의 렌더 크기가 0(width=${rect.width}, height=${rect.height}) — 숨겨졌을 수 있음`, file: 'src/styles.css:.statusline' });
    return;
  }
  if (rect.left < 0 || rect.right > innerWidth || rect.top < 0 || rect.bottom > innerHeight) {
    bugs.push({
      severity: 'Major',
      symptom: `[M2] ${label}이 뷰포트를 벗어남 — rect=${JSON.stringify(rect)}, viewport=${innerWidth}x${innerHeight}`,
      file: 'src/styles.css:.statusline-left/.statusline-right',
    });
  }
  if (!selfOrDescendant) {
    bugs.push({
      severity: 'Major',
      symptom: `[M2] ${label} 중심 좌표(elementFromPoint)가 자기 자신이 아님(다른 요소가 가림: ${atPointTag}) — 클릭이 다른 요소로 전달될 수 있음`,
      file: 'src/styles.css:.statusline',
    });
  }
}

export function scenarios(base) {
  const updateFixtures = {
    ...base,
    'plugin:updater|check': { rid: 9101, currentVersion: '0.2.8', version: '0.3.1', date: '2026-09-24', body: '', rawJson: '{}' },
    'plugin:updater|download': 9102,
  };

  const longProjectPath = '/Users/hopegiver/workspace/이 프로젝트는 상태줄이 잘리는지 보려고 이름을 아주 길게 지은 프로젝트입니다';
  const longProjectFixtures = {
    ...base,
    list_workspace_projects: {
      projects: [
        {
          name: '이 프로젝트는 상태줄이 잘리는지 보려고 이름을 아주 길게 지은 프로젝트입니다',
          path: longProjectPath,
          hasStatus: true,
          archiveStatus: 'active',
          sections: null,
          updatedAt: Date.now(),
        },
      ],
      skipped: [],
    },
    list_project_tree: [{ name: 'src', relativePath: 'src', isDirectory: true, children: null, truncated: false }],
  };

  return [
    // ---- M1 — 로그아웃 → 재로그인 "1회" 클릭으로 셸 진입 ----
    {
      id: 'logout-then-relogin-single-click',
      fixtures: { ...base, google_oauth_login: { email: 'dev@malgnsoft.com', name: 'QA 재로그인', hd: 'malgnsoft.com' } },
      async run(page, { shot, bugs, getListenerCount }) {
        await page.waitForSelector('.tabstrip-account');
        const clickListenersBaseline = await getListenerCount('click');

        await page.locator('.tabstrip-account').click();
        await page.waitForSelector('.tabstrip-account-dropdown');
        await shot('01-menu-open');

        await page.locator('.sidebar-logout').click();
        await page.waitForSelector('.login-screen');
        await shot('02-after-logout-login-screen');

        // M1 핵심 단언 — 로그인 화면에 바깥클릭 리스너가 남아있으면 안 된다.
        const clickListenersAfterLogout = await getListenerCount('click');
        if (clickListenersAfterLogout > clickListenersBaseline) {
          bugs.push({
            severity: 'Critical',
            symptom: `[M1] 로그아웃 후 로그인 화면에 계정 메뉴 바깥클릭 리스너가 남아있음(before=${clickListenersBaseline}, after=${clickListenersAfterLogout})`,
            file: 'src/sidebar.ts:closeAccountMenu/openAccountMenu',
          });
        }

        // M1 핵심 단언 — 로그인 버튼 "1회" 클릭만으로 셸에 진입해야 한다(리스너
        // 잔존 시 chip이 재오픈→재렌더되며 이 클릭이 흡수돼 2번째 클릭에서야
        // 들어가던 회귀).
        await page.locator('.login-btn').click();
        await page.waitForTimeout(500);
        const enteredShell = (await page.locator('.tabstrip-account').count()) > 0;
        await shot('03-after-single-login-click');
        if (!enteredShell) {
          bugs.push({
            severity: 'Critical',
            symptom: '[M1] 재로그인 시 로그인 버튼 1회 클릭으로 셸에 진입하지 못함(첫 클릭 소실 회귀)',
            file: 'src/sidebar.ts:openAccountMenu / src/views/login.ts',
            repro: '로그아웃 → 로그인 화면 → "Google 계정으로 로그인" 1회 클릭 → .tabstrip-account 존재 여부 확인',
          });
        }
      },
    },
    // ---- M1-b — 계정 메뉴가 열린 상태에서 탭 클릭이 흡수되지 않는다 ----
    {
      id: 'menu-open-tab-click-not-swallowed',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.tabstrip-account');
        await page.locator('.tabstrip-account').click();
        await page.waitForSelector('.tabstrip-account-dropdown');
        await shot('01-menu-open');

        await page.locator('.tabstrip-tabs .tab', { hasText: '세션' }).click();
        await page.waitForTimeout(200);
        const hash = await page.evaluate(() => window.location.hash);
        await shot('02-after-tab-click');
        if (hash !== '#/sessions') {
          bugs.push({
            severity: 'Major',
            symptom: `[M1-b] 계정 메뉴가 열린 상태에서 "세션" 탭을 클릭해도 이동하지 않음(실제 hash=${hash})`,
            file: 'src/sidebar.ts:openAccountMenu(바깥클릭 리스너)',
            repro: '계정 메뉴 열기 → 다른 탭 1회 클릭 → location.hash 확인',
          });
        }
        if ((await page.locator('.tabstrip-account-dropdown').count()) > 0) {
          bugs.push({ severity: 'Minor', symptom: '[M1-b] 탭 클릭 후에도 계정 메뉴가 닫히지 않음', file: 'src/sidebar.ts:accountMenuOutsideClickHandler' });
        }
      },
    },
    // ---- m6 — 계정 메뉴 Escape로 닫기 ----
    {
      id: 'menu-escape-closes',
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.tabstrip-account');
        await page.locator('.tabstrip-account').click();
        await page.waitForSelector('.tabstrip-account-dropdown');
        await shot('01-menu-open');

        await page.keyboard.press('Escape');
        await page.waitForTimeout(150);
        await shot('02-after-escape');
        if ((await page.locator('.tabstrip-account-dropdown').count()) > 0) {
          bugs.push({ severity: 'Minor', symptom: '[m6] 계정 메뉴가 열린 상태에서 Escape를 눌러도 닫히지 않음', file: 'src/sidebar.ts:openAccountMenu' });
        }
      },
    },
    // ---- M2 — 900px 최소 창에서 업데이트 배지·새 버전 확인·시계가 항상
    // 뷰포트 안에 있고 클릭 가능하다 ----
    {
      id: 'statusline-update-and-clock-visible-at-900',
      viewport: { width: 900, height: 600 },
      fixtures: updateFixtures,
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.tabstrip-account');
        // "처음 보는 버전" 자동 배경 체크가 배지를 띄울 때까지 대기(부팅 1회,
        // updateApi.ts tryCheckAndOfferUpdate).
        await page.waitForSelector('.sidebar-update-item', { timeout: 5000 }).catch(() => {});
        await shot('01-900-statusline');

        await assertFullyVisibleAndClickable(page, bugs, '.sidebar-update-item', '업데이트 적용 배지');
        await assertFullyVisibleAndClickable(page, bugs, '.sidebar-version-check-btn', '새 버전 확인 버튼');
        await assertFullyVisibleAndClickable(page, bugs, '.statusline .clock', '시계');
      },
    },
    // ---- M2 — 1180px 기본 창의 프로젝트 상세(긴 프로젝트명)에서도 시계가
    // 잘리지 않는다 ----
    {
      id: 'statusline-clock-visible-at-1180-long-project-name',
      viewport: { width: 1180, height: 760 },
      fixtures: longProjectFixtures,
      async run(page, { shot, bugs }) {
        await page.waitForSelector('.tabstrip-account');
        await page.evaluate((p) => {
          window.location.hash = `#/project/${encodeURIComponent(p)}`;
        }, longProjectPath);
        await page.waitForTimeout(300);
        await shot('01-1180-project-detail');

        await assertFullyVisibleAndClickable(page, bugs, '.statusline .clock', '시계(긴 프로젝트명, 1180px)');
      },
    },
  ];
}
