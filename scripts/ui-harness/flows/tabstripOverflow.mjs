// 흐름 (10) 탭스트립 계정 드롭다운 잘림 회귀 방지 — feat/ui-terminus 브랜치
// 커밋 b8507b2(Terminus 토큰 도입)가 `.tabstrip`에 `overflow-x: auto`를
// 추가하면서, overflow-x/overflow-y 중 하나만 auto/hidden 등 visible이 아닌
// 값을 주면 나머지 축도 CSS 스펙상 auto로 계산되는 부작용으로 탭스트립
// 높이(32px) 밖으로 나가는 절대위치 계정 드롭다운(`.tabstrip-account-dropdown`,
// 로그아웃 항목이 있다)이 통째로 잘렸다. 기존 G1 시나리오(majorReview20260921.mjs)는
// 모달 오버레이 위에서 DOM `.click()`으로 우회해 이 문제를 발견하지 못했다 —
// 여기서는 모달 없이 실제 포인터 클릭(Playwright locator.click())으로만
// 검증해 히트테스트 실패(요소가 잘려 클릭 불가능한 상태)를 그대로 드러낸다.
export const flowId = 'tabstripOverflow';
export const startHash = '#/';

const VIEWPORTS = [
  { id: '1440x900', viewport: { width: 1440, height: 900 } },
  { id: '900x600', viewport: { width: 900, height: 600 } },
];

export function scenarios(base) {
  return VIEWPORTS.map(({ id, viewport }) => ({
    id: `account-dropdown-visible-and-clickable-${id}`,
    viewport,
    fixtures: { ...base },
    async run(page, { shot, bugs }) {
      await page.waitForSelector('.tabstrip-account');
      await shot('01-before-open');

      // ---- 회귀 방지 대상이 아닌 기존 결정(IA §7-2) 유지 확인: 탭이
      // 넘칠 때 .tabstrip-tabs가 여전히 가로 스크롤되고, 계정 칩은 스크롤
      // 영역 밖 형제라 뷰포트 우측에 그대로 고정돼 있어야 한다. ----
      const scrollInfo = await page.evaluate(() => {
        const tabsEl = document.querySelector('.tabstrip-tabs');
        const accountEl = document.querySelector('.tabstrip-account');
        if (!tabsEl || !accountEl) return null;
        const accountRect = accountEl.getBoundingClientRect();
        return {
          overflowX: getComputedStyle(tabsEl).overflowX,
          scrollWidth: tabsEl.scrollWidth,
          clientWidth: tabsEl.clientWidth,
          accountRight: accountRect.right,
          viewportWidth: window.innerWidth,
        };
      });
      if (!scrollInfo) {
        bugs.push({ severity: 'Critical', symptom: '.tabstrip-tabs 또는 .tabstrip-account 요소를 찾을 수 없음', file: 'src/sidebar.ts:renderTabstrip' });
        return;
      }
      if (scrollInfo.overflowX !== 'auto') {
        bugs.push({ severity: 'Major', symptom: `.tabstrip-tabs의 overflow-x가 auto가 아님(실제: ${scrollInfo.overflowX}) — IA §7-2 오버플로 스크롤 결정 회귀`, file: 'src/styles.css:.tabstrip-tabs' });
      }
      if (scrollInfo.accountRight > scrollInfo.viewportWidth + 0.5) {
        bugs.push({ severity: 'Major', symptom: `계정 칩이 뷰포트(${id}) 우측 밖으로 나감(right=${scrollInfo.accountRight}, viewportWidth=${scrollInfo.viewportWidth}) — 우측 고정 결정 회귀`, file: 'src/styles.css:.tabstrip' });
      }

      // ---- 실제 포인터 클릭으로 계정 칩을 연다(DOM .click() 아님) ----
      const chip = page.locator('.tabstrip-account');
      await chip.click();
      await page.waitForSelector('.tabstrip-account-dropdown');
      await shot('02-dropdown-open');

      const logoutItem = page.locator('.tabstrip-account-dropdown-item.sidebar-logout');
      if ((await logoutItem.count()) === 0) {
        bugs.push({ severity: 'Critical', symptom: '계정 드롭다운을 열어도 로그아웃 항목이 DOM에 없음', file: 'src/sidebar.ts:renderAccountChip' });
        return;
      }

      // ---- 1) 뷰포트 안에서 실제로 보이는가(clip되지 않았는가) ----
      const visible = await logoutItem.isVisible();
      const box = await logoutItem.boundingBox();
      const vp = page.viewportSize();
      const withinViewport =
        !!box && box.y >= 0 && box.x >= 0 && box.y + box.height <= (vp?.height ?? 0) && box.x + box.width <= (vp?.width ?? 0);
      if (!visible || !box || !withinViewport) {
        bugs.push({
          severity: 'Critical',
          symptom: `로그아웃 항목이 뷰포트(${id}) 안에 보이지 않음 — visible=${visible}, box=${JSON.stringify(box)} (탭스트립 overflow-x:auto가 overflow-y까지 auto로 계산해 잘림)`,
          file: 'src/styles.css:.tabstrip',
        });
        return;
      }

      // ---- 2) 그 좌표의 최상단 요소가 로그아웃 항목 자신이거나 그 자식인가
      // (겹친 다른 요소가 클릭을 가로채지 않는가 — elementFromPoint 히트테스트) ----
      const cx = box.x + box.width / 2;
      const cy = box.y + box.height / 2;
      const hit = await page.evaluate(({ x, y }) => {
        const top = document.elementFromPoint(x, y);
        if (!top) return { tag: null, isTargetOrChild: false, cls: null };
        const target = document.querySelector('.tabstrip-account-dropdown-item.sidebar-logout');
        return {
          tag: top.tagName,
          cls: top.className,
          isTargetOrChild: !!target && (top === target || target.contains(top)),
        };
      }, { x: cx, y: cy });
      if (!hit.isTargetOrChild) {
        bugs.push({
          severity: 'Critical',
          symptom: `로그아웃 항목 좌표(${cx},${cy})의 최상단 요소가 항목 자신/자식이 아님 — elementFromPoint=${hit.tag}.${hit.cls} (다른 요소가 클릭을 가로챔, 실제 클릭 불가능)`,
          file: 'src/styles.css:.tabstrip',
        });
        return;
      }

      // ---- 3) 실제 포인터 클릭으로 로그아웃까지 도달하는가 ----
      await logoutItem.click();
      await page.waitForTimeout(300);
      await shot('03-after-logout-click');
      const onLoginScreen = (await page.locator('.login-screen').count()) > 0;
      if (!onLoginScreen) {
        bugs.push({
          severity: 'Critical',
          symptom: '로그아웃 항목을 실제 포인터로 클릭해도 로그인 화면으로 전환되지 않음',
          file: 'src/sidebar.ts:renderAccountChip',
        });
      }
    },
  }));
}
