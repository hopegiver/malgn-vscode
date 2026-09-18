// route.ts를 정본으로 삼아 로그인 이후 화면 18개(사이드바 8메뉴 + 하위/설정탭
// 포함)를 전부 나열한다. 각 라우트는 hash(고정 픽스처 ID로 조립)와, 시나리오별로
// "화면에 실제로 기대한 콘텐츠가 렌더됐는지"를 검증하는 verify()를 가진다.
//
// quirk: true인 검증은 "이 조합에서는 앱이 원래 이런 화면을 보여준다"는 뜻이다
// (버그일 수 있으나 src/ 수정 범위 밖이라 캡처 자체는 성공으로 판정하고 보고서에
// 별도로 남긴다) — 실패(ok:false)와는 다르다.
import { IDS } from './fixtures.mjs';
import { bodyText, count, hasAlert, hasLoadingText, hasSkeleton, verdict } from './checks.mjs';

const enc = encodeURIComponent;

export const AUTH_ROUTES = [
  {
    id: 'home',
    hash: '#/',
    label: '홈 대시보드',
    verify: async (page, scenario) => {
      const widgets = await count(page, '.home-widget');
      if (widgets !== 7) return verdict(false, `home-widget 개수가 7이 아님 (${widgets})`);
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커(불러오는 중) 미검출');
      if (scenario === 'error') return verdict(text.includes('상태를 불러오지 못했습니다'), 'malgnai-hub 위젯 에러 문구 미검출');
      if (scenario === 'empty') return verdict(text.includes('등록되어 있지 않습니다'), 'malgnai-hub 위젯 미설정 문구 미검출');
      return verdict(!text.includes('상태를 불러오지 못했습니다') && !text.includes('불러오는 중'), '정상 상태인데 에러/로딩 문구가 남아있음');
    },
  },
  {
    id: 'projects-list',
    hash: '#/projects',
    label: '프로젝트 목록',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasSkeleton(page), '스켈레톤 카드 미검출');
      if (scenario === 'empty') return verdict((await bodyText(page)).includes('아직 인식된 프로젝트가 없습니다'), '빈 상태 문구 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.project-card')) >= 3, 'project-card가 3개 미만');
    },
  },
  {
    id: 'projects-detail',
    hash: `#/project/${enc(IDS.PROJECT_PATH)}`,
    label: '프로젝트 상세',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'normal') return verdict(text.includes('malgn-agent') && text.includes('CLAUDE.md'), '프로젝트명/트리 항목 미검출');
      // U-02(round1)로 loading/error/찾을 수 없음 3분기를 구분하게 됐다
      // (src/views/projects.ts:184-197). empty는 목록 자체가 0건이라 이 경로도
      // 여전히 "찾을 수 없습니다"로 귀결되는 것이 정상 동작이다.
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict(text.includes('프로젝트를 찾을 수 없습니다'), '빈 상태 문구 미검출');
    },
  },
  {
    id: 'sessions-list',
    hash: '#/sessions',
    label: '세션목록',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'empty') return verdict((await bodyText(page)).includes('세션이 없습니다'), '빈 상태 문구 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.session-row')) >= 2, 'session-row가 2개 미만');
    },
  },
  {
    id: 'sessions-detail',
    hash: `#/sessions/${enc(IDS.SESSION_ID)}`,
    label: '세션 상세(채팅)',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'empty') return verdict(text.includes('표시할 대화가 없습니다'), '빈 대화 문구 미검출');
      if (scenario === 'error') return verdict(text.includes('대화 기록을 불러오지 못했습니다'), '조회 실패 문구 미검출');
      return verdict(text.includes('세션목록 정본을') && (await count(page, '.chat-thread')) > 0, '대화 내용 미검출');
    },
  },
  {
    id: 'sessions-draft',
    hash: `#/sessions/new/${enc(IDS.PROJECT_PATH)}`,
    label: '세션 draft(새 세션)',
    verify: async (page, _scenario) => {
      // 이 화면은 어떤 invoke도 호출하지 않고 항상 동일한 "새 대화를 시작하세요"
      // 안내만 보여준다(메시지를 보내기 전까지) — 시나리오와 무관하게 동일 화면.
      const text = await bodyText(page);
      return { ok: text.includes('새 대화를 시작하세요'), note: '이 화면은 시나리오와 무관하게 항상 동일(진입 시 invoke 없음)', quirk: true };
    },
  },
  {
    id: 'catalog',
    hash: '#/catalog',
    label: '카탈로그',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'empty') return verdict((await bodyText(page)).includes('설치된 플러그인이 없습니다'), '빈 상태 문구 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.plugin-card')) >= 1, 'plugin-card 미검출');
    },
  },
  {
    id: 'dev-tools',
    hash: '#/settings/devtools',
    label: '개발 환경',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커(확인 중) 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') {
        // U-04(round1)로 전용 빈 상태 문구가 생겼다(src/views/devTools.ts) —
        // 백엔드가 고정 6종을 항상 돌려주므로 빈 목록은 조회 이상 신호다.
        return verdict((await bodyText(page)).includes('도구 상태를 확인하지 못했습니다'), '빈 상태 문구 미검출');
      }
      return verdict((await count(page, '.devtool-item')) >= 4, 'devtool-item이 4개 미만');
    },
  },
  {
    id: 'usage',
    hash: '#/usage',
    label: '사용량 통계',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'empty') return verdict((await bodyText(page)).includes('최근 30일 이내 사용 기록이 없습니다'), '빈 상태 문구 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.bar-row')) > 0 && (await count(page, '.stat-card')) > 0, '일별 막대/통계 카드 미검출');
    },
  },
  {
    id: 'tasks-list',
    hash: '#/tasks',
    label: '자율업무 목록',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'empty') return verdict((await bodyText(page)).includes('등록된 자율업무가 없습니다'), '빈 상태 문구 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.task-row')) >= 2, 'task-row가 2개 미만');
    },
  },
  {
    id: 'tasks-board',
    hash: '#/tasks/board',
    label: '자율업무 진행상황판',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      // empty/normal 모두 4개 컬럼 자체는 항상 렌더된다(각 컬럼 안에 "없음"이 찍힐 뿐).
      return verdict((await count(page, '.task-board-column')) === 4, 'task-board-column이 4개가 아님');
    },
  },
  {
    id: 'tasks-detail',
    hash: `#/tasks/item/${enc(IDS.TASK_ID)}`,
    label: '자율업무 상세',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'normal') return verdict(text.includes('일일 사용량 리포트 발송'), '작업명 미검출');
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      // U-05(round1)로 loading/error/찾을 수 없음 3분기를 구분하게 됐다
      // (src/views/autonomousTasks.ts, projects-detail과 동일한 패턴). empty는
      // 목록 자체가 0건이라 이 경로도 여전히 "찾을 수 없습니다"로 귀결된다.
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict(text.includes('자율업무를 찾을 수 없습니다'), '빈 상태 문구 미검출');
    },
  },
  {
    id: 'settings-otel',
    hash: '#/settings/otel',
    label: '설정 · OTel',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') return verdict(text.includes('사내 collector 주소는 배포 빌드에 주입됩니다'), '엔드포인트 미주입 안내 문구 미검출');
      return verdict((await count(page, '.settings-input')) > 0 && !text.includes('배포 빌드에 주입됩니다'), 'OTel 입력 필드 미검출');
    },
  },
  {
    id: 'settings-github',
    hash: '#/settings/github',
    label: '설정 · GitHub',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') return verdict(text.includes('GitHub 계정 연결하기'), '미연결 안내 버튼 미검출');
      return verdict((await count(page, '.integration-account-row')) > 0 && text.includes('hopegiver'), '연결된 계정 정보 미검출');
    },
  },
  {
    id: 'settings-cloudflare',
    hash: '#/settings/cloudflare',
    label: '설정 · Cloudflare',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') return verdict(text.includes('설치되어 있지 않습니다'), 'wrangler 미설치 안내 미검출');
      return verdict((await count(page, '.integration-account-row')) > 0, '연결된 계정 정보 미검출');
    },
  },
  {
    id: 'settings-jira',
    hash: '#/settings/jira',
    label: '설정 · Jira',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') return verdict(text.includes('계정을 연결합니다'), '미연결 안내 문구 미검출');
      return verdict(text.includes('연결되어 있습니다'), '연결됨 안내 문구 미검출');
    },
  },
  {
    id: 'settings-marketplace',
    hash: '#/settings/marketplace',
    label: '설정 · 마켓플레이스',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'empty') return verdict(text.includes('등록된 마켓플레이스가 없습니다'), '빈 상태 문구 미검출');
      // U-03(round1)로 loading/error를 실제로 구분해 그리게 됐다
      // (src/views/settings.ts renderMarketplacePanel).
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      return verdict((await count(page, '.marketplace-repo-row')) >= 1, 'marketplace-repo-row 미검출');
    },
  },
  {
    id: 'settings-mcp',
    hash: '#/settings/mcp',
    label: '설정 · MCP 관리',
    verify: async (page, scenario) => {
      if (scenario === 'loading') return verdict(await hasLoadingText(page), '로딩 마커 미검출');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') {
        // mcp_list(등록된 서버)는 비어도 mcp_catalog_list(원클릭 설치 카탈로그)는
        // 별도 소스라 항상 함께 표시된다(buildMcpRows가 둘을 합친다) — 그래서
        // "등록된 MCP 서버가 없습니다" 문구 대신 카탈로그 행이 보인다. 정상 동작.
        return { ok: (await count(page, '.mcp-row')) >= 1, note: '등록 서버는 0개지만 카탈로그 행이 함께 렌더됨(정상)', quirk: true };
      }
      return verdict((await count(page, '.mcp-row')) >= 2, 'mcp-row가 2개 미만');
    },
  },
  {
    id: 'settings-applinks',
    hash: '#/settings/applinks',
    label: '설정 · 앱링크',
    verify: async (page, scenario) => {
      const text = await bodyText(page);
      if (scenario === 'loading') return verdict(text.includes('불러오는 중'), '로딩 마커 미검출');
      if (text.includes('불러오는 중')) return verdict(false, '"불러오는 중" 마커가 남아있음(로딩 상태가 해소되지 않음)');
      if (scenario === 'error') return verdict(await hasAlert(page), '.alert 미검출');
      if (scenario === 'empty') return verdict(text.includes('아직 등록된 앱링크가 없습니다'), '빈 상태 문구 미검출');
      return verdict((await count(page, '.mcp-row')) === 5, 'mcp-row(앱링크 행) 개수가 픽스처(5)와 다름');
    },
  },
];
