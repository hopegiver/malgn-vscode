// 흐름 (3) 카탈로그 설치 플로우 — src/views/catalog.ts
import { manyPlugins } from '../lib/fixtures.mjs';

export const flowId = 'catalog';
export const startHash = '#/catalog';

export function scenarios(base) {
  return [
    {
      id: 'golden',
      fixtures: { ...base, update_plugin: { success: true, message: '업데이트 완료(테스트)' }, install_plugin: { success: true, message: '설치 완료(테스트)' }, refresh_marketplaces: { success: true, message: '새로고침 완료(테스트)' } },
      async run(page, { shot, bugs }) {
        await shot('01-plugins-tab');
        const cards = page.locator('.plugin-card');
        const cardCount = await cards.count();
        if (cardCount === 0) {
          bugs.push({ severity: 'Major', symptom: 'golden 시나리오인데 설치된 플러그인이 0개(실제 installed_plugins.json 확인 필요)', file: 'scripts/ui-harness/flows/catalog.mjs' });
        } else {
          const updateBtn = page.getByRole('button', { name: '업데이트' }).first();
          await updateBtn.click();
          await page.waitForTimeout(300);
          await shot('02-after-update-click');
          const note = await page.locator('.plugin-update-note').first().textContent().catch(() => null);
          if (!note || !note.includes('완료')) {
            bugs.push({ severity: 'Major', symptom: `"업데이트" 클릭 후 결과 노트에 완료 문구가 없음(실제: ${note})`, file: 'src/views/catalog.ts:renderUpdateResultNote' });
          }
        }

        await page.evaluate(() => { window.location.hash = '#/catalog/global'; });
        await page.waitForTimeout(300);
        await shot('03-global-tab');

        await page.evaluate(() => { window.location.hash = '#/catalog'; });
        await page.waitForTimeout(200);
        const refreshBtn = page.getByRole('button', { name: /새로고침/ }).first();
        await refreshBtn.click();
        await page.waitForTimeout(200);
        await shot('04-after-refresh-click');
      },
    },
    {
      id: 'empty-install-flow',
      fixtures: { ...base, list_installed_plugins: [], install_plugin: { success: true, message: '설치 완료(테스트)' } },
      async run(page, { shot, bugs }) {
        await shot('01-empty');
        const emptyTitle = await page.locator('.state-block-title').first().textContent().catch(() => null);
        if (!emptyTitle || !emptyTitle.includes('없습니다')) {
          bugs.push({ severity: 'Major', symptom: `설치된 플러그인 0개인데 안내 문구가 없음(실제: ${emptyTitle})`, file: 'src/views/catalog.ts:renderPluginsTabBody' });
        }
        const installBtn = page.getByRole('button', { name: 'malgn-agent 설치' });
        if ((await installBtn.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '플러그인이 0개인데 "malgn-agent 설치" 기본 설치 버튼이 없음', file: 'src/views/catalog.ts:renderPluginsTabBody' });
          return;
        }
        await installBtn.click();
        await page.waitForTimeout(300);
        await shot('02-after-install-click');
        const invokeLog = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'install_plugin') ?? []);
        if (invokeLog.length === 0) {
          bugs.push({ severity: 'Critical', symptom: '"malgn-agent 설치" 클릭이 install_plugin IPC를 호출하지 않음', file: 'src/views/catalog.ts:handleInstallDefaultPlugin' });
        } else if (invokeLog[0].args?.pluginId !== 'malgn-agent@malgnsoft-plugins') {
          bugs.push({ severity: 'Major', symptom: `install_plugin에 잘못된 pluginId 전달: ${JSON.stringify(invokeLog[0].args)}`, file: 'src/views/catalog.ts:DEFAULT_PLUGIN_ID' });
        }
      },
    },
    {
      id: 'install-failure',
      fixtures: { ...base, list_installed_plugins: [], install_plugin: { success: false, message: '테스트 강제 실패: claude plugin install 종료코드 1' } },
      async run(page, { shot, bugs }) {
        await page.getByRole('button', { name: 'malgn-agent 설치' }).click();
        await page.waitForTimeout(300);
        await shot('01-install-failure-note');
        const note = await page.locator('.plugin-update-note').first().textContent().catch(() => null);
        if (!note || !note.includes('실패')) {
          bugs.push({ severity: 'Major', symptom: `설치 실패인데 실패 노트가 보이지 않음(실제: ${note})`, file: 'src/views/catalog.ts:renderInstallDefaultResultNote' });
        }
      },
    },
    {
      id: 'large',
      fixtures: { ...base, list_installed_plugins: manyPlugins(120) },
      async run(page, { shot, bugs }) {
        const count = await page.locator('.plugin-card').count();
        if (count !== 120) bugs.push({ severity: 'Minor', symptom: `120개 픽스처인데 ${count}개만 렌더됨`, file: 'src/views/catalog.ts:renderPluginsTabBody' });
        const updateAllBtn = page.getByRole('button', { name: /모두 업데이트/ });
        if ((await updateAllBtn.count()) === 0) {
          bugs.push({ severity: 'Minor', symptom: '플러그인 2개 이상인데 "모두 업데이트" 버튼이 없음', file: 'src/views/catalog.ts:renderCatalogView' });
        }
        await shot('01-large-list');
        const box = await page.locator('.plugin-card').first().boundingBox();
        const vw = page.viewportSize()?.width ?? 1180;
        if (box && box.width > vw + 5) {
          bugs.push({ severity: 'Minor', symptom: `긴 displayName 플러그인 카드 폭(${Math.round(box.width)}px)이 뷰포트(${vw}px) 초과`, file: 'src/views/catalog.ts:renderPluginCard' });
        }
      },
    },
    {
      id: 'error',
      fixtures: { ...base, list_installed_plugins: { __throw: '테스트 강제 에러: installed_plugins.json 파싱 실패' } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        const alertVisible = await page.locator('.alert').count();
        if (alertVisible === 0) {
          bugs.push({ severity: 'Critical', symptom: 'list_installed_plugins invoke가 throw해도 에러 배너가 뜨지 않음', file: 'src/views/catalog.ts:loadCatalog' });
        }
        await shot('01-error');
      },
    },
    {
      id: 'slow',
      fixtures: { ...base, list_installed_plugins: { __delay: 2500, value: base.list_installed_plugins } },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(400);
        const loadingVisible = await page.locator('.state-block-title:has-text("불러오는 중")').count();
        await shot('01-slow-loading');
        if (loadingVisible === 0) {
          bugs.push({ severity: 'Major', symptom: '2.5초 지연 동안 "불러오는 중…" 표시가 뜨지 않음', file: 'src/views/catalog.ts:renderPluginsTabBody' });
        }
        await page.waitForTimeout(2500);
        await shot('02-slow-loaded');
      },
    },
  ];
}
