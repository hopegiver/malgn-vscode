// 하드코딩된 샘플 데이터. "프로젝트"(workspaceApi.ts), "세션목록"(sessionsApi.ts),
// "카탈로그"(catalogApi.ts), "개발 환경"(devToolsApi.ts), OTel/GitHub/Cloudflare/
// Jira 설정(otelApi.ts/integrationsApi.ts), "사용량 통계"(usageApi.ts), "자율업무"
// (autonomyApi.ts)는 실제 로컬 데이터를 쓰므로 이 파일에 없다 — 아래
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
