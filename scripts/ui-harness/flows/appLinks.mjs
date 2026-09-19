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
      id: 'tab-switch',
      // MCP 관리와 앱링크 설정은 둘 다 'settings' 라우트의 서로 다른 탭이다
      // (#/settings/mcp, #/settings/applinks). route.kind 단위로만 모달을
      // 정리하면 탭 간 이동(mcp ↔ applinks)에서는 모달이 안 닫힐 수 있다 —
      // main.ts가 탭까지 확인하는지 양방향으로 실측한다.
      fixtures: { ...base },
      async run(page, { shot, bugs, getListenerCount }) {
        // 두 방향(applinks→mcp→applinks, mcp→applinks→mcp) 모두 "떠날 때"가
        // 아니라 "원래 탭으로 되돌아왔을 때" 모달이 저절로 다시 열려 있는지를
        // 확인한다 — 탭을 바꾼 직후에는 다른 패널이 렌더되므로 이전 탭의 모달
        // DOM이 어차피 안 보인다(모듈 스코프 상태만 stale하게 남는다). 이
        // stale 상태는 그 탭으로 "돌아왔을 때" 다시 렌더되어 드러난다 —
        // settingsMcp.mjs golden 시나리오의 "라우트 이탈 후 복귀" 패턴과 동일.

        // ---- ① applinks 모달 열기 → mcp 탭 → applinks 탭으로 복귀 ----
        const before1 = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        await page.evaluate(() => { window.location.hash = '#/settings/mcp'; });
        await page.waitForTimeout(300);
        await page.evaluate(() => { window.location.hash = '#/settings/applinks'; });
        await page.waitForTimeout(300);
        await shot('01-applinks-modal-after-mcp-roundtrip');
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: '앱링크 추가 모달을 연 채 MCP 관리 탭으로 이동했다가 앱링크 탭으로 되돌아오면 모달이 저절로 다시 열려 있음(route.kind만 확인하고 탭은 확인하지 않는 것으로 보임)',
            file: 'src/main.ts:handleNavigation / src/views/appLinks.ts:linkFormModal',
            repro: '#/settings/applinks → "+ 링크 추가" 열기(닫지 않음) → hash를 "#/settings/mcp"로 변경 → 다시 "#/settings/applinks"로 변경',
          });
          await page.keyboard.press('Escape');
        }
        // getListenerCount는 baseline(예: updateApi.ts의 상시 keydown 리스너)을
        // 포함하므로, "정상적으로 정리됐다"는 곧 모달을 열기 직전 수준(before)
        // 으로 정확히 되돌아온다는 뜻이다 — before보다 커지면(>) 누수, 같으면
        // (===) 정상.
        const after1 = await getListenerCount('keydown');
        if (after1 > before1) {
          bugs.push({
            severity: 'Minor',
            symptom: `앱링크↔MCP 탭 왕복 후에도 앱링크 모달의 keydown 리스너가 잔존함(before=${before1}, after=${after1})`,
            file: 'src/views/appLinks.ts:linkFormModalEscHandler',
          });
        }

        // ---- ② mcp 모달 열기 → applinks 탭 → mcp 탭으로 복귀 ----
        // (①이 끝난 시점엔 applinks 탭에 있으므로 먼저 mcp 탭으로 이동해야
        // "+ 새 MCP 서버" 버튼이 존재한다 — 그 버튼은 MCP 탭 패널에만 있다.)
        await page.evaluate(() => { window.location.hash = '#/settings/mcp'; });
        // renderMcpPanel()은 loading && !loaded인 동안 loadingBlock()만 그리므로
        // (mcp 목록이 아직 로드 전이면 버튼 자체가 DOM에 없다), 탭 전환 후
        // 다른 곳과 동일한 300ms 고정 대기로 렌더 안정화를 기다린다.
        await page.waitForTimeout(300);
        const before2 = await getListenerCount('keydown');
        const addMcpServerBtn = page.getByRole('button', { name: '+ 새 MCP 서버' });
        // 기본 30초 액셔너빌리티 타임아웃까지 조용히 걸려 시나리오 전체가
        // Critical 예외로 죽는 것을 막기 위해, 버튼을 짧은 타임아웃으로 먼저
        // 명시적으로 기다리고 실패 시 원인을 밝히는 Critical 버그로 남긴 뒤
        // 이 검사만 건너뛴다(뒤 시나리오/스크린샷에 영향 주지 않기 위함).
        try {
          await addMcpServerBtn.waitFor({ state: 'visible', timeout: 5000 });
        } catch (err) {
          bugs.push({
            severity: 'Critical',
            symptom: `mcp 탭으로 전환한 뒤 5초 내에 "+ 새 MCP 서버" 버튼이 렌더되지 않아 역방향 검사를 진행할 수 없음(${err.message})`,
            file: 'src/views/settings.ts:renderMcpPanel',
          });
          return;
        }
        await addMcpServerBtn.click();
        await page.waitForSelector('.modal-overlay');
        await page.evaluate(() => { window.location.hash = '#/settings/applinks'; });
        await page.waitForTimeout(300);
        await page.evaluate(() => { window.location.hash = '#/settings/mcp'; });
        await page.waitForTimeout(300);
        await shot('02-mcp-modal-after-applinks-roundtrip');
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: 'MCP 등록 모달을 연 채 앱링크 설정 탭으로 이동했다가 mcp 탭으로 되돌아오면 모달이 저절로 다시 열려 있음(route.kind만 확인하고 탭은 확인하지 않는 것으로 보임)',
            file: 'src/main.ts:handleNavigation / src/views/settings.ts:mcpAddModalOpen',
            repro: '#/settings/mcp → "+ 새 MCP 서버" 열기(닫지 않음) → hash를 "#/settings/applinks"로 변경 → 다시 "#/settings/mcp"로 변경',
          });
          await page.keyboard.press('Escape');
        }
        // (역방향도 ①과 동일하게 net 카운터다 — before2와 같아지면 정리
        // 성공이므로 정상, before2보다 커질 때만(>) 누수다. >=로 바꾸면
        // "같다(=정리 성공)"까지 누수로 오탐하니 되돌리지 말 것.)
        const after2 = await getListenerCount('keydown');
        if (after2 > before2) {
          bugs.push({
            severity: 'Minor',
            symptom: `MCP↔앱링크 탭 왕복 후에도 MCP 등록 모달의 keydown 리스너가 잔존함(before=${before2}, after=${after2})`,
            file: 'src/views/settings.ts:mcpAddModalEscHandler',
          });
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
