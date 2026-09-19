// 흐름 (5) 앱링크 — src/views/appLinks.ts (설정 > 앱링크설정 탭 패널)
import { manyAppLinks } from '../lib/fixtures.mjs';

export const flowId = 'appLinks';
export const startHash = '#/settings/applinks';

function statusOf(links, overrides = {}) {
  return {
    ok: true,
    error: null,
    filePath: '/Users/hopegiver/.claude/malgn-agent-apps.json',
    fileExists: true,
    links,
    warnings: [],
    limits: { maxLinks: 20, maxNameLength: 40, maxUrlLength: 2048, allowedSchemes: ['https', 'http'] },
    ...overrides,
  };
}

export function scenarios(base) {
  const goldenLinks = base.app_links_get.links;

  return [
    {
      id: 'golden',
      fixtures: { ...base, app_links_save: statusOf([...goldenLinks, { id: 'new-link', name: '새로 추가된 링크', url: 'https://new.example.com', enabled: true }]) },
      async run(page, { shot, bugs, getListenerCount }) {
        await shot('01-list');
        const insecureLabel = page.locator('.mcp-row-status-label', { hasText: '암호화되지 않음' });
        if ((await insecureLabel.count()) === 0) {
          bugs.push({ severity: 'Minor', symptom: 'http:// 링크에 "암호화되지 않음" 경고 라벨이 안 보임(픽스처에 http 링크 포함했는데 미표시)', file: 'src/views/appLinks.ts:renderAppLinkRow' });
        }

        // ---- 추가 모달: ESC ----
        const before = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-add-modal');
        await page.keyboard.press('Escape');
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: 'ESC를 눌러도 앱링크 추가 모달이 닫히지 않음', file: 'src/views/appLinks.ts' });
        }
        const afterEsc = await getListenerCount('keydown');
        if (afterEsc > before) {
          bugs.push({ severity: 'Minor', symptom: `앱링크 추가 모달 ESC 닫기 후 keydown 리스너 잔존(before=${before}, after=${afterEsc})`, file: 'src/views/appLinks.ts:detachLinkFormModalEscHandler' });
        }

        // ---- 모달을 연 채로 완전히 다른 라우트로 이탈 → appLinks.ts에도
        // leave*View류 정리 호출이 main.ts에 없다(grep 확인 완료) — settingsMcp
        // 흐름과 동일한 가설을 이 모달에서도 실측한다.
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        const beforeLeave = await getListenerCount('keydown');
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForTimeout(200);
        const afterLeave = await getListenerCount('keydown');
        if (afterLeave >= beforeLeave && beforeLeave > 0) {
          bugs.push({
            severity: 'Major',
            symptom: `앱링크 추가 모달을 연 채로 완전히 다른 라우트(#/)로 이동해도 keydown 리스너가 정리되지 않음(누수) — before=${beforeLeave}, after=${afterLeave}. main.ts에 이 모달을 위한 leave*View 정리 호출이 없음.`,
            file: 'src/views/appLinks.ts:linkFormModalEscHandler / src/main.ts:handleNavigation',
            repro: '#/settings/applinks → "+ 링크 추가" 열기(닫지 않음) → location.hash를 "#/"로 변경 → keydown 리스너 카운트 확인',
          });
        }
        await page.evaluate(() => { window.location.hash = '#/settings/applinks'; });
        await page.waitForTimeout(200);
        const stillOpenAfterReturn = await page.locator('.modal-overlay').count();
        if (stillOpenAfterReturn > 0) {
          await shot('05-stale-modal-after-route-return');
          bugs.push({
            severity: 'Major',
            symptom: '모달을 닫지 않고 다른 라우트로 나갔다가 #/settings/applinks로 되돌아오면 "새 앱링크 추가" 모달이 저절로 다시 열려 있음(모듈 스코프 linkFormModal이 라우트 이탈로 리셋되지 않음)',
            file: 'src/views/appLinks.ts:linkFormModal',
            repro: '#/settings/applinks → "+ 링크 추가" 열기(닫지 않음) → hash를 "#/"로 변경 → 다시 "#/settings/applinks"로 변경',
          });
          await page.keyboard.press('Escape');
        }

        // ---- 추가: 이름/URL 채워 저장 ----
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        const form = page.locator('.settings-form');
        await form.locator('input.settings-input').nth(0).fill('한글 이름 테스트 링크 <b>bold</b>');
        await form.locator('input.settings-input').nth(1).fill('https://example.internal/한글경로');
        await form.getByRole('button', { name: '추가', exact: true }).click();
        await page.waitForTimeout(300);
        await shot('03-after-add');
        const saveCalls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'app_links_save') ?? []);
        if (saveCalls.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '"추가" 클릭이 app_links_save IPC를 호출하지 않음', file: 'src/views/appLinks.ts:renderLinkForm submit handler' });
        }

        // ---- 빈 값 제출 방어 ----
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        const saveCallsBefore = (await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'app_links_save') ?? [])).length;
        await page.locator('.modal-body').getByRole('button', { name: '추가', exact: true }).click();
        await page.waitForTimeout(200);
        const saveCallsAfter = (await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'app_links_save') ?? [])).length;
        if (saveCallsAfter > saveCallsBefore) {
          bugs.push({ severity: 'Major', symptom: '이름/URL을 비운 채 "추가"를 눌러도 저장 IPC가 호출됨(프론트 사전 검증 누락)', file: 'src/views/appLinks.ts:renderLinkForm' });
        }
        await page.keyboard.press('Escape');

        // ---- 삭제 확인 다이얼로그 취소 ----
        const firstRow = page.locator('.mcp-row').first();
        await firstRow.getByRole('button', { name: '삭제' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('04-delete-confirm');
        await page.getByRole('button', { name: '취소' }).click();
        await page.waitForTimeout(150);

        // ---- 토글 스위치 ----
        const toggle = page.locator('.toggle-track').first();
        await toggle.click();
        await page.waitForTimeout(200);
        const toggleCalls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'app_links_save') ?? []);
        if (toggleCalls.length === 0) {
          bugs.push({ severity: 'Major', symptom: '토글 스위치 클릭이 app_links_save를 호출하지 않음', file: 'src/views/appLinks.ts:handleToggleLink' });
        }
      },
    },
    {
      id: 'empty',
      fixtures: { ...base, app_links_get: statusOf([]) },
      async run(page, { shot, bugs }) {
        await shot('01-empty');
        const title = await page.locator('.state-block-title').first().textContent().catch(() => null);
        if (!title || !title.includes('없습니다')) {
          bugs.push({ severity: 'Major', symptom: `앱링크 0건인데 빈 상태 안내가 없음(실제: ${title})`, file: 'src/views/appLinks.ts:renderAppLinksPanel' });
        }
      },
    },
    {
      id: 'large-over-limit',
      fixtures: { ...base, app_links_get: statusOf(manyAppLinks(120)) },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.mcp-row').count();
        if (count !== 120) bugs.push({ severity: 'Minor', symptom: `120건 픽스처인데 ${count}건만 렌더됨`, file: 'src/views/appLinks.ts:renderAppLinksPanel' });
        const countLabel = await page.locator('.applink-count-label').textContent().catch(() => null);
        if (!countLabel || !countLabel.includes('120')) {
          bugs.push({ severity: 'Minor', symptom: `한도 초과(120/20) 상황에서 카운트 라벨이 실제 건수를 보여주지 않음(실제: ${countLabel})`, file: 'src/views/appLinks.ts:renderAppLinksPanel' });
        }
        await shot('01-over-limit');
        const addBtnDisabled = await page.getByRole('button', { name: '+ 링크 추가' }).isDisabled();
        if (!addBtnDisabled) {
          bugs.push({ severity: 'Minor', symptom: `이미 한도(20)를 초과한 상태에서도 "+ 링크 추가" 버튼이 비활성화되지 않음(프론트가 limits.maxLinks를 클라이언트 측에서 강제하지 않는 것으로 보임 — 저장 시 백엔드 거부에만 의존)`, file: 'src/views/appLinks.ts:renderAppLinksPanel' });
        }
      },
    },
    {
      id: 'error',
      fixtures: { ...base, app_links_get: { __throw: '테스트 강제 에러: Tauri IPC 브리지 없음' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await shot('01-error');
        const alertOrBlocked = await page.locator('.alert').count();
        if (alertOrBlocked === 0) {
          bugs.push({ severity: 'Major', symptom: 'app_links_get invoke가 throw하는 방어 경로에서 에러 배너가 뜨지 않음', file: 'src/views/appLinks.ts:loadAppLinks' });
        }
      },
    },
    {
      id: 'slow',
      fixtures: { ...base, app_links_get: { __delay: 2500, value: base.app_links_get } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const loadingVisible = await page.locator('.state-block-title:has-text("불러오는 중")').count();
        await shot('01-slow-loading');
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 동안 "불러오는 중…" 표시가 뜨지 않음', file: 'src/views/appLinks.ts / src/dom.ts:loadingBlock' });
        }
        await page.waitForTimeout(2500);
        await shot('02-slow-loaded');
      },
    },
  ];
}
