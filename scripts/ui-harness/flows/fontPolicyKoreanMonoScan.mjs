// 흐름 — 리뷰 2차(2026-09-24, docs/reviewer/review-ui-terminus-overhaul-r2.md
// M3-r) 회귀 방지. "한글이 섞일 가능성이 있는 모든 UI 텍스트는 JetBrains
// Mono(font-numeric)로 렌더되면 안 된다"는 확정 방침(terminus-design-system.md
// §1, 예외 없음)을 CSS 셀렉터 grep이 아니라 실제 렌더 결과로 검증한다 —
// 리뷰의 실측: 공용 클래스(.stat-value 등)는 셀렉터 자체는 "숫자용"이 맞아도
// 한글 값이 흘러드는 경우를 grep이 놓친다.
//
// 부록 스캔 코드(리뷰 보고서 "부록 — 한글-모노 DOM 스캔")를 그대로 하네스
// 시나리오로 옮긴다: 10개 라우트 + "개발 도구 실행 전 확인 패널을 연 상태"
// 에서 한글 텍스트 노드의 부모 computed font-family에 JetBrains가 섞이면 0건을
// 단언한다. 수정 전 트리(6cc6487)에서 이 시나리오를 돌리면 #/usage에서
// "stat-value :: 3일" 1건이 실패로 잡힌다(리뷰 실측과 동일) — 수정 후에는 0건.
export const flowId = 'fontPolicyKoreanMonoScan';
export const startHash = '#/';

// 리뷰 보고서 "부록"의 10개 라우트 그대로.
const ROUTES = [
  '#/',
  '#/projects',
  '#/sessions',
  '#/usage',
  '#/settings/devtools',
  '#/tasks',
  '#/tasks/board',
  '#/catalog',
  '#/settings/applinks',
  '#/settings',
];

// 리뷰 보고서 부록의 TreeWalker 스캔을 그대로 옮긴 것 — document.body 전체를
// 순회하며 한글이 섞인 텍스트 노드의 부모 요소 computed font-family에
// "JetBrains"가 들어있으면 수집한다. 정적 grep과 달리 실제 렌더 결과(상속 포함)를
// 본다는 것이 핵심이라, 이 함수 자체를 page.evaluate로 브라우저에서 그대로
// 실행한다(외부 스코프 참조 없음 — Playwright 직렬화 제약과 무관하게 독립 실행).
async function scanKoreanMonoHits(page) {
  return page.evaluate(() => {
    const out = new Set();
    const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let n;
    while ((n = w.nextNode())) {
      if (/[가-힣]/.test(n.textContent)) {
        const p = n.parentElement;
        if (p && /JetBrains/.test(getComputedStyle(p).fontFamily)) {
          out.add(`${p.className} :: ${n.textContent.trim().slice(0, 40)}`);
        }
      }
    }
    return [...out];
  });
}

export function scenarios(base) {
  return [
    {
      id: 'korean-text-never-renders-mono',
      // open_manual_instruction — 개발 도구 "터미널에서 실행" 확인 절차 전용
      // 커맨드(devToolsApi.ts). base에는 없다(자동 호출되지 않고 이 흐름처럼
      // 사용자가 명시 트리거할 때만 필요) — M3-r ②(devTools.ts:580, Rust가 만든
      // "다음 명령을 실행합니다: {cmd}" 문구)를 실제로 렌더해 스캔하려면 필요.
      fixtures: {
        ...base,
        open_manual_instruction: { opened: false, message: '다음 명령을 실행합니다: brew install --cask claude' },
      },
      async run(page, { shot, bugs }) {
        // ---- 1) 정적 10개 라우트 ----
        for (const route of ROUTES) {
          await page.evaluate((h) => {
            window.location.hash = h;
          }, route);
          await page.waitForTimeout(300);
          const hits = await scanKoreanMonoHits(page);
          if (hits.length > 0) {
            bugs.push({
              severity: 'Major',
              symptom: `[M3-r 회귀] ${route}에서 한글 텍스트가 JetBrains Mono로 렌더됨: ${hits.join(' | ')}`,
              file: 'src/styles.css',
              repro: `${route} 진입 → document.body TreeWalker로 한글 텍스트 노드의 부모 getComputedStyle().fontFamily 확인`,
            });
          }
        }
        await shot('09-static-routes-scan-done');

        // ---- 2) 개발 도구 실행 전 확인 패널을 연 상태(M3-r ②) ----
        // 캡처만으로는 안 잡힌다(직전 라운드가 조작 뒤에만 나타나는 패널이라
        // 정적 스캔에서 빠졌던 지점) — 실제로 패널을 열어서 스캔한다.
        await page.evaluate(() => {
          window.location.hash = '#/settings/devtools';
        });
        await page.waitForTimeout(300);
        const claudeItem = page.locator('.devtool-item', { has: page.locator('#devtool-row-claude') });
        await claudeItem.locator('.btn', { hasText: '안내 보기' }).click();
        await page.waitForTimeout(150);
        await claudeItem.locator('.btn', { hasText: '터미널에서 실행' }).click();
        await page.waitForTimeout(200);
        const panelCount = await claudeItem.locator('.devtool-panel-command').count();
        if (panelCount === 0) {
          bugs.push({
            severity: 'Major',
            symptom: '개발 도구 "터미널에서 실행" 확인 패널이 열리지 않음(.devtool-panel-command 없음) — 스캔 대상 자체를 확인하지 못함',
            file: 'src/views/devTools.ts:renderManualPanel',
          });
        }
        await shot('10-devtool-manual-confirm-panel');
        const panelHits = await scanKoreanMonoHits(page);
        if (panelHits.length > 0) {
          bugs.push({
            severity: 'Major',
            symptom: `[M3-r 회귀] 개발 도구 실행 전 확인 패널에서 한글 텍스트가 JetBrains Mono로 렌더됨: ${panelHits.join(' | ')}`,
            file: 'src/views/devTools.ts:renderManualPanel',
            repro: '#/settings/devtools → "Claude Code" 행 "안내 보기" → "터미널에서 실행" → 확인 패널 렌더 확인',
          });
        }
      },
    },
  ];
}
