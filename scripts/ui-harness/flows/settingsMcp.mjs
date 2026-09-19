// 흐름 (4) malgnai-hub MCP 고정표시 UI — src/views/settings.ts (마켓플레이스/MCP관리)
import { manyMcpServers, mcpCatalogSample } from '../lib/fixtures.mjs';

export const flowId = 'settingsMcp';
export const startHash = '#/settings/mcp';

const HUB_NAME = 'plugin:malgn-agent:malgnai-hub';

export function scenarios(base) {
  return [
    {
      id: 'golden',
      fixtures: { ...base },
      async run(page, { shot, bugs, getListenerCount }) {
        await shot('01-mcp-panel');
        const hubRow = page.locator('.mcp-row', { hasText: 'malgnai-hub' }).first();
        if ((await hubRow.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: 'malgnai-hub MCP 행이 목록 최상단에 고정 표시되지 않음', file: 'src/views/settings.ts:buildRegisteredMcpRows' });
        } else {
          const deleteBtn = hubRow.getByRole('button', { name: '삭제' });
          if ((await deleteBtn.count()) > 0) {
            bugs.push({ severity: 'Critical', symptom: 'malgnai-hub 필수 MCP 서버에 "삭제" 버튼이 노출됨(잠금 행이어야 함)', file: 'src/views/settings.ts:renderMcpRow(locked)' });
          }
          const badge = await hubRow.locator('.badge').allTextContents();
          if (!badge.some((b) => b.includes('필수'))) {
            bugs.push({ severity: 'Minor', symptom: 'malgnai-hub 행에 "필수" 배지가 없음', file: 'src/views/settings.ts:renderMcpRow' });
          }
        }

        // ---- 새 MCP 서버 등록 모달: 열기 → ESC ----
        const before = await getListenerCount('keydown');
        await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
        await page.waitForSelector('.modal-overlay');
        await shot('02-add-modal');
        await page.keyboard.press('Escape');
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({ severity: 'Critical', symptom: 'ESC를 눌러도 "새 MCP 서버 등록" 모달이 닫히지 않음', file: 'src/views/settings.ts' });
        }
        const afterEsc = await getListenerCount('keydown');
        if (afterEsc > before) {
          bugs.push({ severity: 'Minor', symptom: `MCP 등록 모달 ESC 닫기 후 keydown 리스너 잔존(before=${before}, after=${afterEsc})`, file: 'src/views/settings.ts:detachMcpAddModalEscHandler' });
        }

        // ---- 모달을 연 채로 완전히 다른 라우트로 이탈 → settings.ts의 MCP 추가
        // 모달은 main.ts에 leave*View류 정리 호출이 전혀 없다(grep 확인 완료:
        // views/settings.ts·views/appLinks.ts에는 leaveAutonomousTasksListView
        // 같은 대응 함수 자체가 없음) — autonomousTasks/sessions와 달리 라우트를
        // 벗어나도 모듈 스코프 mcpAddModalOpen이 리셋되지 않을 가능성을 실측한다.
        await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
        await page.waitForSelector('.modal-overlay');
        const beforeLeave = await getListenerCount('keydown');
        await page.evaluate(() => { window.location.hash = '#/'; });
        await page.waitForTimeout(200);
        const afterLeave = await getListenerCount('keydown');
        if (afterLeave >= beforeLeave && beforeLeave > 0) {
          bugs.push({
            severity: 'Major',
            symptom: `MCP 등록 모달을 연 채로 완전히 다른 라우트(#/)로 이동해도 keydown 리스너가 정리되지 않음(누수) — before=${beforeLeave}, after=${afterLeave}. main.ts에 이 모달을 위한 leave*View 정리 호출이 없음(autonomousTasks/sessions/projects와 달리).`,
            file: 'src/views/settings.ts:mcpAddModalEscHandler / src/main.ts:handleNavigation',
            repro: '#/settings/mcp → "+ 새 MCP 서버" 열기(닫지 않음) → location.hash를 "#/"로 변경 → keydown 리스너 카운트 확인',
          });
        }
        await page.evaluate(() => { window.location.hash = '#/settings/mcp'; });
        await page.waitForTimeout(200);
        const stillOpenAfterReturn = await page.locator('.modal-overlay').count();
        if (stillOpenAfterReturn > 0) {
          await shot('06-stale-modal-after-route-return');
          bugs.push({
            severity: 'Major',
            symptom: '모달을 닫지 않고 다른 라우트로 나갔다가 #/settings/mcp로 되돌아오면 "새 MCP 서버 등록" 모달이 저절로 다시 열려 있음(모듈 스코프 mcpAddModalOpen이 라우트 이탈로 리셋되지 않음)',
            file: 'src/views/settings.ts:mcpAddModalOpen',
            repro: '#/settings/mcp → "+ 새 MCP 서버" 열기(닫지 않음) → hash를 "#/"로 변경 → 다시 "#/settings/mcp"로 변경',
          });
          await page.keyboard.press('Escape');
        }

        // ---- 등록 폼: stdio로 채워 제출 ----
        await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
        await page.waitForSelector('.modal-overlay');
        const form = page.locator('.mcp-add-form');
        await form.locator('input.settings-input').first().fill('테스트 서버 한글이름');
        await form.locator('input.settings-input').nth(1).fill('/usr/local/bin/test-mcp-server');
        await form.getByRole('button', { name: '저장' }).click();
        await page.waitForTimeout(300);
        await shot('03-after-add-submit');
        const addCalls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'mcp_add') ?? []);
        if (addCalls.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '"저장" 클릭이 mcp_add IPC를 호출하지 않음', file: 'src/views/settings.ts:renderMcpAddForm' });
        }
        if ((await page.locator('.modal-overlay').count()) > 0) {
          bugs.push({ severity: 'Minor', symptom: 'mcp_add 성공 후에도 등록 모달이 닫히지 않음', file: 'src/views/settings.ts:renderMcpAddForm submit handler' });
        }

        // ---- 삭제 확인 다이얼로그: 취소 ----
        const otherRow = page.locator('.mcp-row', { hasText: 'github' }).first();
        if ((await otherRow.count()) > 0) {
          await otherRow.getByRole('button', { name: '삭제' }).click();
          await page.waitForSelector('.modal-overlay');
          await shot('04-delete-confirm');
          await page.getByRole('button', { name: '취소' }).click();
          await page.waitForTimeout(150);
          const removeCallsAfterCancel = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'mcp_remove') ?? []);
          if (removeCallsAfterCancel.length > 0) {
            bugs.push({ severity: 'Critical', symptom: '삭제 확인 다이얼로그에서 "취소"를 눌렀는데도 mcp_remove가 호출됨', file: 'src/dom.ts:confirmDialog / src/views/settings.ts:handleRemoveMcp' });
          }
        }

        // ---- 마켓플레이스 탭 ----
        await page.evaluate(() => { window.location.hash = '#/settings/marketplace'; });
        await page.waitForTimeout(300);
        await shot('05-marketplace-tab');
        const refreshBtn = page.getByRole('button', { name: /마켓플레이스 새로고침/ });
        if ((await refreshBtn.count()) === 0) {
          bugs.push({ severity: 'Minor', symptom: '마켓플레이스 탭에 새로고침 버튼이 없음', file: 'src/views/settings.ts:renderMarketplacePanel' });
        }
      },
    },
    {
      id: 'hub-missing',
      fixtures: { ...base, mcp_list: base.mcp_list.filter((s) => s.name !== HUB_NAME), install_plugin: { success: true, message: '설치 완료(테스트)' } },
      async run(page, { shot, bugs }) {
        await shot('01-hub-missing');
        const installBtn = page.getByRole('button', { name: '플러그인 설치' });
        if ((await installBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'malgnai-hub가 mcp_list에 없을 때 "플러그인 설치" 안내 행이 뜨지 않음', file: 'src/views/settings.ts:renderMalgnaiHubMissingRow' });
          return;
        }
        await installBtn.click();
        await page.waitForTimeout(300);
        await shot('02-after-install-click');
        const calls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'install_plugin') ?? []);
        if (calls.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '"플러그인 설치" 클릭이 install_plugin IPC를 호출하지 않음', file: 'src/views/settings.ts:handleInstallMalgnAgentPluginForMcp' });
        }
      },
    },
    {
      id: 'empty-catalog',
      // buildInstallableMcpRows()는 mcp_catalog_list와 별개로 GitHub/Telegram
      // 퀵스타트 행을 "아직 등록 안 됐으면" 항상 추가한다(src/views/settings.ts
      // :buildInstallableMcpRows) — 그래서 mcp_catalog_list=[]만으로는
      // installableRows가 0이 되지 않는다. 진짜 "설치 가능한 서버 0건"을
      // 재현하려면 hub·github·telegram을 전부 이미 등록된 것으로 채워야 한다.
      fixtures: {
        ...base,
        mcp_list: [
          { name: HUB_NAME, target: 'https://malgnai-hub.malgnsoft.com/mcp', transport: 'http', connected: true, statusLabel: '✔ Connected' },
          { name: 'github', target: 'https://api.githubcopilot.com/mcp/', transport: 'http', connected: true, statusLabel: '✔ Connected' },
          { name: 'telegram', target: 'npx mcp-telegram-agent', transport: 'stdio', connected: true, statusLabel: '✔ Connected' },
        ],
        mcp_catalog_list: [],
      },
      async run(page, { shot, bugs }) {
        await shot('01-empty');
        const installableHeader = page.locator('.plugin-section-label', { hasText: '설치 가능한 서버' });
        if ((await installableHeader.count()) > 0) {
          bugs.push({ severity: 'Minor', symptom: 'hub/github/telegram이 모두 등록되어 있고 카탈로그도 0건인데 "설치 가능한 서버" 섹션 헤더가 렌더됨', file: 'src/views/settings.ts:renderMcpPanel' });
        }
      },
    },
    {
      id: 'large',
      fixtures: { ...base, mcp_list: manyMcpServers(120), mcp_catalog_list: mcpCatalogSample([]) },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.mcp-row').count();
        if (count < 120) bugs.push({ severity: 'Minor', symptom: `120개 이상 픽스처인데 mcp-row가 ${count}개만 렌더됨`, file: 'src/views/settings.ts:renderMcpPanel' });
        await shot('01-large');
      },
    },
    {
      id: 'error',
      fixtures: { ...base, mcp_list: { __throw: '테스트 강제 에러: claude mcp list 실행 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const alertVisible = await page.locator('.alert').count();
        if (alertVisible === 0) {
          bugs.push({ severity: 'Critical', symptom: 'mcp_list invoke가 throw해도 에러 배너가 뜨지 않음', file: 'src/views/settings.ts:loadMcp' });
        }
        await shot('01-error');
      },
    },
    {
      id: 'slow',
      fixtures: { ...base, mcp_list: { __delay: 2500, value: base.mcp_list } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const loadingVisible = await page.locator('.state-block-title:has-text("불러오는 중")').count();
        await shot('01-slow-loading');
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 동안 "불러오는 중…" 표시가 뜨지 않음', file: 'src/views/settings.ts:renderMcpPanel' });
        }
        await page.waitForTimeout(2500);
        await shot('02-slow-loaded');
      },
    },
    {
      id: 'marketplace-empty',
      startHash: '#/settings/marketplace',
      fixtures: { ...base, list_known_marketplaces: [], list_installed_plugins: [] },
      async run(page, { shot, bugs }) {
        await shot('01-marketplace-empty');
        const desc = await page.locator('.state-block-desc').first().textContent().catch(() => null);
        if (!desc || !desc.includes('없습니다')) {
          bugs.push({ severity: 'Minor', symptom: `마켓플레이스 0건인데 빈 상태 안내가 없음(실제: ${desc})`, file: 'src/views/settings.ts:renderMarketplacePanel' });
        }
      },
    },
    // 회사 마켓플레이스(malgnsoft-plugins) 원클릭 추가 추천 — 없을 때만 뜨고
    // 이미 있으면 뜨지 않는지를 양쪽 픽스처로 못박는다(views/settings.ts
    // :renderMarketplaceRecommendBlock). 실제 개발 머신 상태(loadKnownMarketplaces가
    // 읽는 known_marketplaces.json)에 의존하지 않도록 두 시나리오 모두 목록을
    // 명시적으로 고정한다.
    {
      id: 'marketplace-recommend-absent',
      startHash: '#/settings/marketplace',
      fixtures: {
        ...base,
        list_known_marketplaces: [{ id: 'acme-plugins', repo: 'acme/plugins', lastUpdated: new Date().toISOString() }],
      },
      async run(page, { shot, bugs }) {
        await shot('01-recommend-shown');
        const recommend = page.locator('.alert', { hasText: '맑은소프트 마켓플레이스를 추가하시겠습니까' });
        if ((await recommend.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'malgnsoft-plugins가 목록에 없는데도 회사 마켓플레이스 추천 블록이 뜨지 않음', file: 'src/views/settings.ts:renderMarketplaceRecommendBlock' });
          return;
        }
        const addBtn = recommend.getByRole('button', { name: /\+ 마켓플레이스 추가/ });
        if ((await addBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '추천 블록에 원클릭 추가 버튼이 없음', file: 'src/views/settings.ts:renderMarketplaceRecommendBlock' });
          return;
        }

        // handleAddMarketplace는 성공 시 loadMarketplaces()로 목록을 즉시
        // 다시 읽는다(views/settings.ts:handleAddMarketplace) — 정적 픽스처로는
        // 그 재조회 응답을 바꿀 수 없어(bridge.mjs 주석 참고) 클릭 전에
        // list_known_marketplaces만 다음 호출부터 malgnsoft-plugins를 포함한
        // 값을 돌려주도록 invoke를 즉석 교체한다(add_marketplace 등 나머지
        // 커맨드는 원래 픽스처로 그대로 위임). main.ts의 "이미 loaded면
        // 재조회 안 함" 라우트 가드(loadMarketplaces 호출부와 별개 경로)를
        // 타지 않도록 페이지 이동 없이 같은 화면에서 클릭만 한다.
        await page.evaluate(() => {
          const orig = window.__TAURI_INTERNALS__.invoke;
          window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
            if (cmd === 'list_known_marketplaces') {
              return [
                { id: 'malgnsoft-plugins', repo: 'malgnsoft/claude-plugins', lastUpdated: new Date().toISOString() },
                { id: 'acme-plugins', repo: 'acme/plugins', lastUpdated: new Date().toISOString() },
              ];
            }
            return orig(cmd, args);
          };
        });

        // 클릭 시 입력 폼을 거치지 않고 곧장 회사 마켓플레이스 URL로
        // add_marketplace를 호출하는지 확인한다.
        await addBtn.click();
        await page.waitForTimeout(300);
        await shot('02-after-recommend-click');
        const calls = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'add_marketplace') ?? []);
        if (calls.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '추천 블록의 원클릭 추가 버튼이 add_marketplace IPC를 호출하지 않음', file: 'src/views/settings.ts:handleAddMarketplace' });
        } else if (calls[calls.length - 1].args?.source !== 'https://github.com/malgnsoft/claude-plugins') {
          bugs.push({
            severity: 'Critical',
            symptom: `추천 버튼 클릭이 잘못된 소스로 add_marketplace를 호출함(실제: ${JSON.stringify(calls[calls.length - 1].args)})`,
            file: 'src/views/settings.ts:renderMarketplaceRecommendBlock',
          });
        }
        await shot('03-after-refresh-with-company-marketplace');
        if ((await page.locator('.alert', { hasText: '맑은소프트 마켓플레이스를 추가하시겠습니까' }).count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: 'malgnsoft-plugins가 목록에 포함된 뒤에도 추천 블록이 계속 표시됨(이미 추가된 걸 또 권함)',
            file: 'src/views/settings.ts:renderMarketplaceRecommendBlock',
          });
        }
      },
    },
    {
      id: 'marketplace-recommend-present',
      startHash: '#/settings/marketplace',
      fixtures: {
        ...base,
        list_known_marketplaces: [
          { id: 'malgnsoft-plugins', repo: 'malgnsoft/claude-plugins', lastUpdated: new Date().toISOString() },
          { id: 'acme-plugins', repo: 'acme/plugins', lastUpdated: new Date().toISOString() },
        ],
      },
      async run(page, { shot, bugs }) {
        await shot('01-recommend-hidden');
        if ((await page.locator('.alert', { hasText: '맑은소프트 마켓플레이스를 추가하시겠습니까' }).count()) > 0) {
          bugs.push({ severity: 'Major', symptom: 'malgnsoft-plugins가 이미 목록에 있는데도 추천 블록이 뜸(잡음)', file: 'src/views/settings.ts:renderMarketplaceRecommendBlock' });
        }

        // 사용자 결정: malgnsoft-plugins도 다른 마켓플레이스와 동등하게 "필수"
        // 배지 없이, 제거 버튼이 노출되어야 한다(더 이상 고정 표시하지 않음).
        const companyRow = page.locator('.marketplace-repo-row', { hasText: 'malgnsoft-plugins' });
        const badges = await companyRow.locator('.badge').allTextContents();
        if (badges.some((b) => b.includes('필수'))) {
          bugs.push({ severity: 'Major', symptom: 'malgnsoft-plugins 마켓플레이스 행에 여전히 "필수" 배지가 표시됨(제거 가능해야 함)', file: 'src/views/settings.ts:renderMarketplacePanel' });
        }
        if ((await companyRow.getByRole('button', { name: '제거' }).count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'malgnsoft-plugins 마켓플레이스 행에 제거 버튼이 없음(사용자 결정: 제거 가능해야 함)', file: 'src/views/settings.ts:renderMarketplacePanel' });
        }
      },
    },
  ];
}
