// 동의 화면 표시 문구 조립 — architecture.md §4.1 "무엇을 바꿀지 보여주고 승인받기"
// (L2 동의 요건: 무엇이·왜·되돌릴 수 있는지). 순수 함수라 Electron 없이 테스트한다 —
// 실제 다이얼로그(`host/electron/consentDialogAdapter.ts`)는 이 문자열을 그대로
// `detail`에 싣기만 한다.

import type { Change, Plan } from '../../providers/types.js';

function formatChange(change: Change): string {
  const reversibility = change.reversible ? '되돌릴 수 있음' : '되돌릴 수 없음';
  const beforePart = change.before !== undefined ? `${change.before} → ` : '';
  return `• [${change.level}] ${change.target} (${change.kind}, ${reversibility})\n  ${beforePart}${change.after}\n  이유: ${change.rationale}`;
}

/** L2 동의 화면 본문 — 변경 목록 전체를 사람이 읽을 수 있는 형태로 조립한다. `plan.changes`가
 * 비어 있으면 호출자가 이 함수를 부르지 않는다는 것이 계약이다(승인할 것이 없다). */
export function formatPlanForDisplay(plan: Plan): string {
  const lines = plan.changes.map(formatChange);
  return lines.join('\n\n');
}
