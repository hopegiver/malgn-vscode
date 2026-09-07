// 동의 화면 1개 — architecture.md §4.1(공통 동의 모델) 실제 구현. `trustDialogAdapter.ts`와
// 같은 이유로 `dialog.showMessageBox`를 쓴다(메인 프로세스 네이티브 다이얼로그라 §2.8의
// 렌더러 CSP 요건이 적용되지 않는다 — HTML을 렌더링하지 않는다). 문구 조립(무엇을
// 보여줄지)은 `core/consent/formatPlanForDisplay.ts`가 전담하고, 이 파일은 "어떻게
// 띄우는가"만 안다 — `core/reconciler/applyOrchestrator.ts`가 요구하는
// `requestApproval(plan) => Promise<boolean>` 시그니처를 채우는 어댑터일 뿐이다.

import { dialog } from 'electron';
import type { Plan } from '../../providers/types.js';
import { formatPlanForDisplay } from '../../core/consent/formatPlanForDisplay.js';

const PROVIDER_LABEL: Readonly<Record<string, string>> = {
  agent: 'malgn-agent 플러그인',
  mcp: 'malgnai-hub MCP',
};

export async function showApplyConsentDialogElectron(plan: Plan): Promise<boolean> {
  const label = PROVIDER_LABEL[plan.providerId] ?? plan.providerId;
  const result = await dialog.showMessageBox({
    type: 'question',
    message: `${label}에 다음 변경을 적용하시겠습니까?`,
    // 문구 조립은 formatPlanForDisplay가 고정 틀로 만든다 — provider가 자유
    // 텍스트(rationale)에 임의 문자열을 넣을 수 있지만 HTML을 렌더링하지 않는
    // 네이티브 다이얼로그라 §2.8의 "외부 통제 문자열은 텍스트로만" 요건이 자동으로
    // 충족된다(주입 실행 표면 자체가 없다).
    detail: formatPlanForDisplay(plan),
    buttons: ['적용', '취소'],
    defaultId: 1,
    cancelId: 1,
  });
  return result.response === 0;
}
