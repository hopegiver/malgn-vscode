// 흐름 (8) 업데이트 수동 확인 — src/sidebar.ts(버전 옆 "새 버전 확인" 버튼) +
// src/updateApi.ts(checkForUpdateFromButton/tryCheckAndOfferUpdate manual 분기).
//
// 이 흐름의 핵심 가설: 자동 배경 체크(부팅 1회, initUpdateCheck)는 계속
// 조용해야 하고, 사용자가 버튼을 직접 눌렀을 때만 결과(새 버전/최신/실패)를
// showToast로 알려야 한다. 세 시나리오 모두 "클릭 전(자동 경로)에는 화면에
// 아무 변화가 없다"를 먼저 못박은 뒤에만 클릭해 수동 경로를 검증한다 — 순서를
// 바꾸면 자동 경로 침묵 회귀를 놓친다.
export const flowId = 'updateCheck';
export const startHash = '#/';

const CHECK_BTN_NAME = '새 버전 확인';

function assertAutoPathSilentSoFar(page, bugs, label) {
  return (async () => {
    const toastCount = await page.locator('.toast').count();
    if (toastCount > 0) {
      bugs.push({
        severity: 'Critical',
        symptom: `[${label}] 사용자가 버튼을 누르기 전(자동 배경 체크 단계)인데 토스트가 이미 떠 있음 — 자동 경로 침묵이 깨짐`,
        file: 'src/updateApi.ts:tryCheckAndOfferUpdate',
      });
    }
  })();
}

export function scenarios(base) {
  return [
    {
      id: 'manual-new-version',
      // 베이스는 plugin:updater|check:null(자동 체크가 "최신"으로 조용히 끝남)
      // 그대로 둔다 — 부팅 시 자동 체크가 실제로 한 번 돌고 조용히 끝나는 것까지
      // 지나간 뒤, 버튼 클릭부터는 invoke를 즉석 교체해 "새 버전 발견"으로
      // 분기시킨다(정적 픽스처로는 같은 커맨드를 호출 시점별로 다르게 응답할 수
      // 없어 settingsMcp.mjs:marketplace-recommend-absent와 동일한 기법을 쓴다).
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await assertAutoPathSilentSoFar(page, bugs, 'manual-new-version:부팅직후');
        if ((await page.locator('.sidebar-update-item').count()) > 0) {
          bugs.push({
            severity: 'Major',
            symptom: '[manual-new-version] 클릭 전인데 sidebar-update-item(업데이트 적용 버튼)이 이미 떠 있음 — 베이스 픽스처가 null이 아닌 것으로 보임',
            file: 'scripts/ui-harness/flows/updateCheck.mjs',
          });
        }
        await shot('01-before-click-quiet');

        // ---- invoke 즉석 교체: 이제부터 check()는 새 버전을, download()는
        // 성공을 돌려준다. check()에 인위적 지연(400ms)을 줘 "확인 중…" 상태를
        // 실제로 관찰할 수 있게 한다.
        await page.evaluate(() => {
          const orig = window.__TAURI_INTERNALS__.invoke;
          window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
            if (cmd === 'plugin:updater|check') {
              await new Promise((r) => setTimeout(r, 400));
              return { rid: 9001, currentVersion: '0.2.8', version: '0.3.0', date: '2026-09-21', body: '테스트 릴리스 노트', rawJson: '{}' };
            }
            if (cmd === 'plugin:updater|download') {
              return 9002;
            }
            return orig(cmd, args);
          };
        });

        const checkBtn = page.getByRole('button', { name: CHECK_BTN_NAME });
        await checkBtn.click();

        // ---- "확인 중…" + 비활성화 + 연타 방지를 클릭 직후 짧은 창에서 확인 ----
        await page.waitForTimeout(80);
        const checkingLabel = await page.locator('.sidebar-version-check-btn').textContent().catch(() => null);
        if (!checkingLabel || !checkingLabel.includes('확인 중')) {
          bugs.push({ severity: 'Major', symptom: `[manual-new-version] 클릭 직후 "확인 중…" 표시가 안 보임(실제: ${checkingLabel})`, file: 'src/sidebar.ts:renderVersionRow' });
        }
        const disabledDuringCheck = await page.locator('.sidebar-version-check-btn').isDisabled();
        if (!disabledDuringCheck) {
          bugs.push({ severity: 'Major', symptom: '[manual-new-version] 확인 중에도 버튼이 비활성화되지 않아 연타가 가능함', file: 'src/sidebar.ts:renderVersionRow' });
        }
        await shot('02-checking');

        // 확인 중(비활성화) 상태에서 강제로 한 번 더 클릭 이벤트를 발생시켜도
        // check 커맨드가 추가 호출되지 않아야 한다(네이티브 disabled + JS 가드
        // 이중 방어).
        const checkCallsDuringCheck = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'plugin:updater|check').length ?? 0);
        await page.locator('.sidebar-version-check-btn').dispatchEvent('click').catch(() => {});
        await page.waitForTimeout(50);
        const checkCallsAfterForcedReclick = await page.evaluate(() => window.__invokeLog?.filter((e) => e.cmd === 'plugin:updater|check').length ?? 0);
        if (checkCallsAfterForcedReclick > checkCallsDuringCheck) {
          bugs.push({
            severity: 'Major',
            symptom: `[manual-new-version] 확인 중 상태에서 강제 재클릭했더니 check 커맨드가 추가로 호출됨(연타 방지 실패, before=${checkCallsDuringCheck}, after=${checkCallsAfterForcedReclick})`,
            file: 'src/sidebar.ts:renderVersionRow / src/updateApi.ts:checkForUpdateFromButton',
          });
        }

        // ---- 결과: 새 버전 발견 → 기존 업데이트 버튼 흐름이 그대로 뜬다(추가
        // 문구 불필요, 토스트도 없어야 한다) ----
        await page.waitForTimeout(600);
        await shot('03-after-new-version-found');
        const updateItem = page.locator('.sidebar-update-item');
        if ((await updateItem.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '[manual-new-version] 새 버전을 찾았는데도 사이드바 "업데이트 적용" 버튼이 나타나지 않음', file: 'src/updateApi.ts:tryCheckAndOfferUpdate' });
        } else {
          const updateLabel = await updateItem.textContent();
          if (!updateLabel || !updateLabel.includes('0.3.0')) {
            bugs.push({ severity: 'Major', symptom: `[manual-new-version] 업데이트 버튼에 새 버전(0.3.0)이 표시되지 않음(실제: ${updateLabel})`, file: 'src/sidebar.ts:renderUpdateItem' });
          }
        }
        const toastAfterNewVersion = await page.locator('.toast').count();
        if (toastAfterNewVersion > 0) {
          bugs.push({
            severity: 'Minor',
            symptom: `[manual-new-version] 새 버전 발견 시 추가 토스트까지 뜸(요구사항: 추가 문구 불필요, 기존 버튼 흐름만으로 충분) — 토스트 개수=${toastAfterNewVersion}`,
            file: 'src/updateApi.ts:tryCheckAndOfferUpdate',
          });
        }
        const btnLabelAfter = await page.locator('.sidebar-version-check-btn').textContent().catch(() => null);
        const btnDisabledAfter = await page.locator('.sidebar-version-check-btn').isDisabled().catch(() => true);
        if (!btnLabelAfter || btnLabelAfter.includes('확인 중') || btnDisabledAfter) {
          bugs.push({ severity: 'Minor', symptom: `[manual-new-version] 체크 종료 후 "새 버전 확인" 버튼이 원상복구되지 않음(label=${btnLabelAfter}, disabled=${btnDisabledAfter})`, file: 'src/sidebar.ts:renderVersionRow' });
        }
      },
    },
    {
      id: 'manual-latest',
      // 베이스 그대로(plugin:updater|check:null) — 자동 체크와 수동 클릭이
      // 똑같은 "최신" 응답을 받는다. 자동 체크는 화면에 아무 변화가 없어야
      // 하고, 수동 클릭만 "최신 버전입니다" 토스트를 띄워야 한다.
      fixtures: { ...base },
      async run(page, { shot, bugs }) {
        await page.waitForTimeout(300);
        await assertAutoPathSilentSoFar(page, bugs, 'manual-latest:부팅직후');
        await shot('01-before-click-quiet');

        await page.getByRole('button', { name: CHECK_BTN_NAME }).click();
        await page.waitForTimeout(400);
        await shot('02-after-latest-toast');

        const toast = page.locator('.toast', { hasText: '최신' });
        if ((await toast.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '[manual-latest] 최신 버전인데 "최신 버전입니다" 류 토스트가 뜨지 않음(버튼이 고장난 것처럼 보임)', file: 'src/updateApi.ts:tryCheckAndOfferUpdate' });
        }
        if ((await page.locator('.sidebar-update-item').count()) > 0) {
          bugs.push({ severity: 'Major', symptom: '[manual-latest] 최신 버전인데도 "업데이트 적용" 버튼이 나타남', file: 'src/updateApi.ts:tryCheckAndOfferUpdate' });
        }
        const btnDisabledAfter = await page.locator('.sidebar-version-check-btn').isDisabled().catch(() => true);
        if (btnDisabledAfter) {
          bugs.push({ severity: 'Minor', symptom: '[manual-latest] 결과 확인 후에도 "새 버전 확인" 버튼이 비활성화 상태로 남음', file: 'src/sidebar.ts:renderVersionRow' });
        }
      },
    },
    {
      id: 'manual-failure',
      // check() 자체가 매번 reject하도록 고정한다 — 자동(부팅 1회)과 수동
      // 클릭 모두 이 실패를 겪는다. 자동 경로는 여전히 조용해야 하고(에러
      // 배너/토스트 금지, updateApi.ts 상단 주석의 설계 의도), 수동 클릭만
      // 담백한 실패 안내를 띄워야 한다.
      fixtures: { ...base, 'plugin:updater|check': { __throw: '테스트 강제 네트워크 오류(업데이트 서버 미응답)' } },
      async run(page, { shot, bugs, errors }) {
        await page.waitForTimeout(300);
        await assertAutoPathSilentSoFar(page, bugs, 'manual-failure:부팅직후(자동체크 실패 겪은 뒤)');
        if ((await page.locator('.alert').count()) > 0) {
          bugs.push({
            severity: 'Critical',
            symptom: '[manual-failure] 부팅 시 자동 업데이트 체크가 실패했는데 에러 배너(.alert)가 뜸 — 자동 경로는 침묵해야 한다',
            file: 'src/updateApi.ts:tryCheckAndOfferUpdate',
          });
        }
        await shot('01-before-click-quiet-despite-auto-failure');

        await page.getByRole('button', { name: CHECK_BTN_NAME }).click();
        await page.waitForTimeout(400);
        await shot('02-after-failure-toast');

        const toast = page.locator('.toast', { hasText: '확인하지 못했습니다' });
        if ((await toast.count()) === 0) {
          bugs.push({ severity: 'Major', symptom: '[manual-failure] 수동 확인이 실패했는데 실패를 알리는 토스트가 뜨지 않음(버튼이 고장난 것처럼 보임)', file: 'src/updateApi.ts:tryCheckAndOfferUpdate' });
        }
        const btnDisabledAfter = await page.locator('.sidebar-version-check-btn').isDisabled().catch(() => true);
        if (btnDisabledAfter) {
          bugs.push({ severity: 'Minor', symptom: '[manual-failure] 실패 확인 후에도 "새 버전 확인" 버튼이 비활성화 상태로 남아 재시도가 불가능함', file: 'src/sidebar.ts:renderVersionRow' });
        }

        // pageerror(uncaught)로 새어나간 게 없는지도 함께 본다 — check()
        // reject가 어딘가에서 안 잡히면 여기 찍힌다.
        const pageErrors = errors.filter((e) => e.startsWith('PAGEERROR'));
        if (pageErrors.length > 0) {
          bugs.push({ severity: 'Major', symptom: `[manual-failure] 강제 실패 흐름에서 처리되지 않은 예외 발생: ${pageErrors.join(' | ')}`, file: 'src/updateApi.ts' });
        }
      },
    },
  ];
}
