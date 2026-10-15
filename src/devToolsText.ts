// 개발 도구 화면의 문구·배지 표기(순수) — 화면에 보이는 말이 실제 상태와 어긋나지 않게 한 곳에 모으고
// scripts/tests/r20DevToolsText.test.mjs가 그대로 import해 고정한다(DOM·invoke 의존 없음).
//
// 핵심 원칙: 모르는 것을 아는 척하지 않는다. 경로는 있는데 버전만 못 읽은 도구를 "없다/설치 안 됨"이라
// 하지 않고, 이전 버전을 모르는 실행을 "신규 설치"라 하지 않으며, 자동 설치가 안 되는 이유를 하나로 단정하지 않는다.

export interface StatusTool {
  readonly installed: boolean;
  readonly version: string | null;
  readonly path?: string | null;
}

// 경로는 있는데 버전을 확인하지 못한 도구의 판정은 devToolsOrder.ts의 한 정의를 그대로 쓴다(두 곳에 같은 식을 두지 않는다).
// 상대 import에 .ts를 붙인 것은 Node 타입 스트리핑 시험(scripts/tests)이 이 모듈을 번들러 없이 불러오기 때문이다.
import { isVersionUnverified } from './devToolsOrder.ts';
export { isVersionUnverified };

// 도구 한 줄의 상태 종류 — 행 배지 톤과 사이드바 점이 같은 판정을 쓴다("버전 미확인"은 어디서나 경고 톤).
export type ToolStatusKind = 'installed' | 'versionUnverified' | 'missing';

export function toolStatusKind(tool: Pick<StatusTool, 'installed' | 'path'>): ToolStatusKind {
  if (tool.installed) return 'installed';
  return isVersionUnverified(tool) ? 'versionUnverified' : 'missing';
}

// 행 배지·사이드바 메타에 쓰는 상태 라벨.
export function toolStatusLabel(tool: StatusTool, missingLabel: string): string {
  if (tool.installed) return tool.version ?? '설치됨';
  return isVersionUnverified(tool) ? '버전 미확인' : missingLabel;
}

export type RunKind = 'install' | 'update' | 'updateUnverified';

// 실행 결과의 동사를 정한다. "신규 설치"는 실행 전에 경로도 버전도 없던 도구에만 쓴다.
// 이전 버전을 알 수 없는 업데이트(경로는 있었거나 이미 설치로 분류돼 있었음)는 사실대로 따로 말한다.
export function classifyRun(input: { versionBefore?: string | null; wasInstalled: boolean; hadPathBefore: boolean }): RunKind {
  if (input.versionBefore) return 'update';
  if (input.wasInstalled || input.hadPathBefore) return 'updateUnverified';
  return 'install';
}

export function updatedSummary(kind: RunKind, versionAfter: string): string {
  switch (kind) {
    case 'install':
      return `v${versionAfter}(으)로 설치되었습니다`;
    case 'update':
      return `v${versionAfter}(으)로 업데이트되었습니다`;
    case 'updateUnverified':
      return `v${versionAfter}(으)로 업데이트되었습니다(이전 버전 미확인)`;
  }
}

// 결과 패널의 버전 변화 줄. 표시할 버전이 하나도 없으면 null.
export function versionChangeNote(
  kind: RunKind,
  versionBefore: string | null | undefined,
  versionAfter: string | null | undefined,
  normalizedAfter: string | null | undefined
): string | null {
  if (!versionBefore && !versionAfter) return null;
  if (!versionBefore && versionAfter) {
    const after = normalizedAfter ?? versionAfter;
    return kind === 'install' ? `신규 설치: ${after}` : `이전 버전 미확인 → ${after}`;
  }
  return `${versionBefore ?? '?'} → ${versionAfter ?? '?'}`;
}

// 선행 도구는 확인됐지만 이어 설치할 후보가 하나도 없을 때의 토스트. 이유를 하나로 단정하지 않는다
// (경로를 못 찾은 경우 외에 쓰기 권한 부족 등도 있다) — 이유는 각 항목의 안내에 있다.
export function chainStuckToast(causeNames: string, stuckNames: string): string {
  return `${causeNames} 설치가 확인되었지만 ${stuckNames}은(는) 앱이 자동으로 설치할 수 없는 상태입니다 — 각 항목의 안내에 이유가 있습니다`;
}

// 이어서 설치 확인창 문구. 자동 설치가 안 되는 도구(stuck)가 함께 있으면 대화상자에서도 빠뜨리지 않는다.
export function chainConsentMessage(causeNames: string, candidateNames: string, candidateCount: number, stuckNames: string): string {
  const stuckNote = stuckNames ? ` (자동 설치 불가: ${stuckNames} — 항목 안내 참고)` : '';
  return `${causeNames} 설치가 확인되었습니다. 이어서 다음 ${candidateCount}개 도구를 설치합니다: ${candidateNames}.${stuckNote} 각 도구는 미리보기로 영향 범위를 확인한 뒤 설치하며, 확인할 수 없는 도구는 실행하지 않고 개별 확인으로 남깁니다. 계속할까요?`;
}
