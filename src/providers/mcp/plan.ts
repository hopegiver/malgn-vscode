// mcp provider plan() — architecture.md §5.3 "설치됐으나 enabledPlugins에 없음/false →
// `claude plugin enable ...` 제안, 등급 L2". 이 표의 다른 모든 행("OAuth 미인증"·"URL
// 불일치"·"복수 scope 존재"·"health check 실패")은 **보고만**이라 Change를 만들지
// 않는다 — plan()이 Change를 만드는 경로는 이 하나뿐이다(§5.3 표 전체가 "자동
// 정정·삭제하지 않는다"를 반복한다).
//
// mcp는 정책으로 조정되는 필드가 없다(`EffectivePolicy`에 `mcp` 없음, §0.1.1 A-1 범위
// 고정) — `desired`는 이 provider에서 항상 무시된다(다른 provider와 시그니처를
// 맞추기 위해서만 받는다).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { computeDiffHash } from '../../core/reconciler/diffHash.js';
import type { Change, Plan } from '../../providers/types.js';
import type { Observed } from '../../providers/types.js';
import type { McpObservedDetail } from './detect.js';
import { MV_MCP_PLUGIN_DISABLED } from './errors.js';

function emptyPlan(): Plan {
  return { providerId: 'mcp', changes: [], diffHash: computeDiffHash('mcp', []) };
}

export function planMcp(observed: Observed): Plan {
  if (observed.code !== MV_MCP_PLUGIN_DISABLED) {
    return emptyPlan();
  }
  const detail = observed.detail as McpObservedDetail | undefined;
  if (!detail || !detail.pluginInstalled || detail.pluginEnabled !== false) {
    return emptyPlan();
  }
  const pluginId = loadCodeConstants().allowedPlugins[0];
  if (!pluginId) return emptyPlan();

  const change: Change = {
    id: 'mcp-plugin-enable',
    target: pluginId,
    kind: 'exec',
    level: 'L2',
    after: 'enabled',
    reversible: true,
    rationale: 'malgnai-hub MCP 활성화를 위해 플러그인을 활성화합니다(§5.1 ② — 주입이 아니라 enable)',
  };
  return { providerId: 'mcp', changes: [change], diffHash: computeDiffHash('mcp', [change]) };
}
