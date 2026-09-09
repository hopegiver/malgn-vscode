// 하드코딩된 샘플 데이터. "프로젝트"(workspaceApi.ts), "세션목록"(sessionsApi.ts),
// "카탈로그"(catalogApi.ts), "개발 환경"(devToolsApi.ts), OTel/GitHub/Cloudflare/
// Jira 설정(otelApi.ts/integrationsApi.ts), "사용량 통계"(usageApi.ts)는 실제
// 로컬 데이터를 쓰므로 이 파일에 없다 — 자율업무는 순수 목업이고, 아래
// MOCK_USAGE.byProject는 프로젝트별 토큰 집계 UI가 아직 없어 어디서도 렌더링하지
// 않는 죽은 데이터다(건드리지 않고 그대로 유지).

// ---------------- 사용량 통계 (byProject만 남은 죽은 데이터) ----------------

export interface UsageByProject {
  readonly project: string;
  readonly tokens: number;
}

export const MOCK_USAGE: {
  readonly byProject: readonly UsageByProject[];
} = {
  byProject: [
    { project: 'malgn-agent', tokens: 3820000 },
    { project: 'malgnai-hub', tokens: 2650000 },
    { project: 'malgn-vscode', tokens: 2190000 },
    { project: 'malgnuniv', tokens: 1540000 },
    { project: 'malgnsales', tokens: 980000 },
    { project: 'malgn-billing', tokens: 640000 },
  ],
};

// ---------------- 자율업무 ----------------
// 순수 목업 — 실제 스케줄 실행 엔진은 붙어 있지 않다. on/off 토글·새 항목 추가는
// 세션 안 메모리 상태만 바꾸고 새로고침하면 사라진다.

export interface AutonomousTaskHistoryEntry {
  readonly at: string;
  readonly result: '성공' | '실패';
}

export interface AutonomousTask {
  id: string;
  name: string;
  description: string;
  scheduleLabel: string;
  lastRunLabel: string | null;
  nextRunLabel: string;
  enabled: boolean;
  jiraSpace?: string;
  history?: readonly AutonomousTaskHistoryEntry[];
}

export const MOCK_AUTONOMOUS_TASKS: readonly AutonomousTask[] = [
  {
    id: 'task-status-check',
    name: '프로젝트 상태 점검',
    description: '등록된 프로젝트의 STATUS.md를 훑어 오래 갱신되지 않은 항목을 알려줍니다.',
    scheduleLabel: '매 1시간마다',
    lastRunLabel: '10분 전',
    nextRunLabel: '50분 후',
    enabled: true,
    jiraSpace: 'DEV',
    history: [
      { at: '오늘 14:00', result: '성공' },
      { at: '오늘 13:00', result: '성공' },
      { at: '오늘 12:00', result: '실패' },
    ],
  },
  {
    id: 'task-dep-update',
    name: '의존성 업데이트 확인',
    description: '카탈로그의 에이전트·스킬·지식 중 구버전 항목을 찾아 알려줍니다.',
    scheduleLabel: '매일 09:00',
    lastRunLabel: '어제 09:00',
    nextRunLabel: '내일 09:00',
    enabled: true,
    history: [
      { at: '어제 09:00', result: '성공' },
      { at: '2일 전 09:00', result: '성공' },
    ],
  },
  {
    id: 'task-otel-health',
    name: 'OTel 헬스체크',
    description: '설정된 OTLP 엔드포인트로 핑을 보내 수집기 상태를 확인합니다.',
    scheduleLabel: '매 30분마다',
    lastRunLabel: '5분 전',
    nextRunLabel: '25분 후',
    enabled: false,
    jiraSpace: 'OPS',
  },
  {
    id: 'task-usage-report',
    name: '주간 사용량 리포트',
    description: '프로젝트별 토큰 사용량을 집계해 요약을 만듭니다.',
    scheduleLabel: '매주 월요일 09:00',
    lastRunLabel: '지난주 월요일',
    nextRunLabel: '다음주 월요일',
    enabled: true,
  },
  {
    id: 'task-session-cleanup',
    name: '세션 메타데이터 정리',
    description: '오래된 로컬 세션 메타데이터 파일을 정리 대상으로 표시합니다.',
    scheduleLabel: '매일 03:00',
    lastRunLabel: '오늘 03:00',
    nextRunLabel: '내일 03:00',
    enabled: false,
  },
  {
    id: 'task-performance-log',
    name: '오늘 업무 성과관리 등록',
    description: '그날 수행한 작업을 사내 성과관리시스템(performance 프로젝트)에 자동으로 등록·업데이트합니다.',
    scheduleLabel: '매일 18:00',
    lastRunLabel: '어제 18:00',
    nextRunLabel: '오늘 18:00',
    enabled: true,
    history: [
      { at: '어제 18:00', result: '성공' },
      { at: '2일 전 18:00', result: '성공' },
    ],
  },
];

// ---------------- 자율업무 진행상황판 ----------------
// 진행중/대기중/성공/실패 상태별 최근 실행 이력 샘플. 실제 실행기가 없으므로
// 시각 표기도 "오늘/어제" 같은 사람이 읽는 문자열로만 하드코딩한다.

export type TaskRunStatus = '진행중' | '성공' | '실패' | '대기중';

export interface TaskRun {
  readonly id: string;
  readonly taskName: string;
  readonly status: TaskRunStatus;
  readonly startedAt: string;
  readonly endedAt: string | null;
}

export const MOCK_TASK_RUNS: readonly TaskRun[] = [
  { id: 'run-1', taskName: '프로젝트 상태 점검', status: '진행중', startedAt: '오늘 14:32', endedAt: null },
  { id: 'run-2', taskName: '의존성 업데이트 확인', status: '성공', startedAt: '오늘 09:00', endedAt: '오늘 09:02' },
  { id: 'run-3', taskName: 'OTel 헬스체크', status: '실패', startedAt: '오늘 08:30', endedAt: '오늘 08:31' },
  { id: 'run-4', taskName: '주간 사용량 리포트', status: '대기중', startedAt: '다음주 월요일 09:00 예정', endedAt: null },
  { id: 'run-5', taskName: '세션 메타데이터 정리', status: '성공', startedAt: '오늘 03:00', endedAt: '오늘 03:01' },
  { id: 'run-6', taskName: '프로젝트 상태 점검', status: '성공', startedAt: '오늘 13:00', endedAt: '오늘 13:00' },
  { id: 'run-7', taskName: '프로젝트 상태 점검', status: '실패', startedAt: '오늘 12:00', endedAt: '오늘 12:00' },
  { id: 'run-8', taskName: '의존성 업데이트 확인', status: '성공', startedAt: '어제 09:00', endedAt: '어제 09:02' },
  { id: 'run-9', taskName: '오늘 업무 성과관리 등록', status: '성공', startedAt: '어제 18:00', endedAt: '어제 18:01' },
  { id: 'run-10', taskName: '오늘 업무 성과관리 등록', status: '성공', startedAt: '2일 전 18:00', endedAt: '2일 전 18:01' },
];
