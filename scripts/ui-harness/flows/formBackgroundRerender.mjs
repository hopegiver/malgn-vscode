// 흐름 (6) 배경 이벤트로 인한 전체 재렌더 중 입력값 보존 회귀 방지
// (hub 이슈 01m2zwcx7etvk9zh617tk7bayq, 리포트 docs/reviewer/review-v0.2.5-whole-app-2026-09-21.md)
//
// main.ts:renderApp()은 root.replaceChildren()로 DOM 트리 전체를 파괴하고 새로
// 만든다 — notifyChange() 호출부가 169곳이라, 폼에 입력하는 도중 사용자 행동과
// 무관한 배경 이벤트(세션 파일 감시·자율업무 폴링·devtools 경과시간 타이머 등)
// 가 오면 입력 중이던 <input>/<textarea>의 값이 통째로 사라질 수 있었다.
//
// 이 흐름은 리뷰가 실측한 재현 폼 5개(OTel URL·마켓플레이스 URL·workspaces
// 편집·MCP 등록 모달·자율업무 프롬프트) 전부와, 구조적으로 동일한 버그를 가졌던
// 앱링크 추가 폼(보너스, 원래 5개에는 없었지만 같은 패턴이라 함께 고쳤다)을
// 덮는다. 배경 이벤트는 실제 트리거 중 하나인 "claude-sessions-changed"를
// emitEvent(bridge.mjs)로 직접 발화해 흉내 낸다 — 어느 라우트에 있든 main.ts의
// onSessionsChanged 구독이 항상 살아있어(initLiveWatchers, 세션당 1회) 즉시
// notifyChange()를 트리거하므로, 지금 보고 있는 화면과 무관한 배경 이벤트가
// 전체 재렌더를 일으키는 실제 상황을 정확히 재현한다.
export const flowId = 'formBackgroundRerender';
export const startHash = '#/';

const BACKGROUND_EVENT = 'claude-sessions-changed';

async function assertPreserved(page, { bugs, locator, typed, symptomLabel, file }) {
  const after = await locator.inputValue().catch(() => null);
  if (after !== typed) {
    bugs.push({
      severity: 'Critical',
      symptom: `배경 이벤트(${BACKGROUND_EVENT}) 발화로 인한 전체 재렌더 후 ${symptomLabel} 입력값이 유실됨(입력: "${typed}", 실제: ${JSON.stringify(after)})`,
      file,
      repro: `${symptomLabel} 입력 → ${BACKGROUND_EVENT} 이벤트 발화 → 값 확인`,
    });
  }
  return after === typed;
}

export function scenarios(base) {
  return [
    // 재현 폼 1/5 — OTel 설정
    {
      id: 'otel-url-survives-background-rerender',
      startHash: '#/settings/otel',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        const input = page.locator('#otel-CLAUDE_CODE_ENABLE_TELEMETRY');
        if ((await input.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: 'OTel 설정 폼에 CLAUDE_CODE_ENABLE_TELEMETRY 입력 필드가 없음(픽스처 managedKeys 확인 필요)', file: 'src/views/settings.ts:renderOtelPanel' });
          return;
        }
        const typed = 'harness-background-rerender-otel-1234';
        await input.fill(typed);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        await assertPreserved(page, {
          bugs,
          locator: page.locator('#otel-CLAUDE_CODE_ENABLE_TELEMETRY'),
          typed,
          symptomLabel: 'OTel 설정',
          file: 'src/views/settings.ts:renderOtelPanel',
        });
      },
    },
    // 재현 폼 2/5 — 마켓플레이스 URL 인라인 추가 폼
    {
      id: 'marketplace-url-survives-background-rerender',
      startHash: '#/settings/marketplace',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        const input = page.locator('.marketplace-add-form input.settings-input');
        if ((await input.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '마켓플레이스 탭에 인라인 추가 입력 필드가 없음', file: 'src/views/settings.ts:renderMarketplaceAddForm' });
          return;
        }
        const typed = 'https://github.com/harness-test/marketplace-preserve';
        await input.fill(typed);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        await assertPreserved(page, {
          bugs,
          locator: page.locator('.marketplace-add-form input.settings-input'),
          typed,
          symptomLabel: '마켓플레이스 소스 URL',
          file: 'src/views/settings.ts:renderMarketplaceAddForm',
        });
      },
    },
    // 재현 폼 3/5 — 프로젝트 화면의 workspaces 편집 모달
    {
      id: 'workspaces-textarea-survives-background-rerender',
      startHash: '#/projects',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        const toggleBtn = page.getByRole('button', { name: 'workspace 설정' });
        if ((await toggleBtn.count()) === 0) {
          bugs.push({ severity: 'Critical', symptom: '"workspace 설정" 버튼이 보이지 않음(malgn_agent_config_get 픽스처 확인 필요)', file: 'src/views/projects.ts:renderWorkspacesToggleBtn' });
          return;
        }
        await toggleBtn.click();
        await page.waitForSelector('.modal-overlay');
        const textarea = page.locator('#malgn-config-workspaces');
        const typed = '/Users/hopegiver/workspace/harness-background-rerender-test-project';
        await textarea.fill(typed);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        await assertPreserved(page, {
          bugs,
          locator: page.locator('#malgn-config-workspaces'),
          typed,
          symptomLabel: 'workspaces 편집',
          file: 'src/views/projects.ts:renderWorkspacesEditForm',
        });
      },
    },
    // 재현 폼 4/5 — MCP 등록 모달
    {
      id: 'mcp-add-modal-survives-background-rerender',
      startHash: '#/settings/mcp',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        await page.getByRole('button', { name: '+ 새 MCP 서버' }).click();
        await page.waitForSelector('.modal-overlay');
        const nameInput = page.locator('.mcp-add-form input.settings-input').first();
        const targetInput = page.locator('.mcp-add-form input.settings-input').nth(1);
        const typedName = '하네스-배경이벤트-테스트-서버';
        const typedTarget = '/usr/local/bin/harness-background-rerender-test-mcp';
        await nameInput.fill(typedName);
        await targetInput.fill(typedTarget);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        const okName = await assertPreserved(page, {
          bugs,
          locator: page.locator('.mcp-add-form input.settings-input').first(),
          typed: typedName,
          symptomLabel: 'MCP 등록 모달 이름',
          file: 'src/views/settings.ts:renderMcpAddForm',
        });
        const okTarget = await assertPreserved(page, {
          bugs,
          locator: page.locator('.mcp-add-form input.settings-input').nth(1),
          typed: typedTarget,
          symptomLabel: 'MCP 등록 모달 target',
          file: 'src/views/settings.ts:renderMcpAddForm',
        });
        void okName;
        void okTarget;
      },
    },
    // 재현 폼 5/5 — 자율업무 추가/수정 폼의 이름·프롬프트
    {
      id: 'autonomous-task-prompt-survives-background-rerender',
      startHash: '#/tasks',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        await page.getByRole('button', { name: '+ 새 자율업무' }).click();
        await page.waitForSelector('.modal-overlay');
        const nameInput = page.locator('.task-add-form input.settings-input').first();
        const promptInput = page.locator('.task-form-textarea');
        const typedName = '하네스 배경이벤트 테스트';
        const typedPrompt = '이것은 배경 이벤트 발화 중 입력값 보존을 검증하는 테스트 프롬프트입니다.';
        await nameInput.fill(typedName);
        await promptInput.fill(typedPrompt);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        await assertPreserved(page, {
          bugs,
          locator: page.locator('.task-add-form input.settings-input').first(),
          typed: typedName,
          symptomLabel: '자율업무 이름',
          file: 'src/views/autonomousTasks.ts:renderTaskForm',
        });
        await assertPreserved(page, {
          bugs,
          locator: page.locator('.task-form-textarea'),
          typed: typedPrompt,
          symptomLabel: '자율업무 프롬프트',
          file: 'src/views/autonomousTasks.ts:renderTaskForm',
        });
      },
    },
    // 보너스 — 원래 5개에는 없었지만 같은 모듈-로컬 모달 패턴(taskFormModal과
    // 동일 구조)이라 전수 조사 중 함께 발견·수정한 앱링크 추가 폼.
    {
      id: 'app-link-form-survives-background-rerender',
      startHash: '#/settings/applinks',
      fixtures: { ...base },
      async run(page, { shot, bugs, emitEvent }) {
        await page.waitForTimeout(300);
        await page.getByRole('button', { name: '+ 링크 추가' }).click();
        await page.waitForSelector('.modal-overlay');
        const nameInput = page.locator('.modal-body input.settings-input').nth(0);
        const urlInput = page.locator('.modal-body input.settings-input').nth(1);
        const typedName = '하네스 테스트 링크';
        const typedUrl = 'https://harness-background-rerender-test.malgnsoft.internal';
        await nameInput.fill(typedName);
        await urlInput.fill(typedUrl);
        await shot('01-typed');
        await emitEvent(BACKGROUND_EVENT, null);
        await page.waitForTimeout(300);
        await shot('02-after-background-event');
        await assertPreserved(page, {
          bugs,
          locator: page.locator('.modal-body input.settings-input').nth(0),
          typed: typedName,
          symptomLabel: '앱링크 이름',
          file: 'src/views/appLinks.ts:renderLinkForm',
        });
        await assertPreserved(page, {
          bugs,
          locator: page.locator('.modal-body input.settings-input').nth(1),
          typed: typedUrl,
          symptomLabel: '앱링크 주소',
          file: 'src/views/appLinks.ts:renderLinkForm',
        });
      },
    },
  ];
}
