// agent provider plan() — architecture.md §3.2 표 plan 행: "정책 `agent.channel`/
// `compat.malgnAgent` vs 관측 버전 → semver 평가. 순수 함수." §3.5 known[] "이 버전은
// known 결함으로 apply가 차단됩니다"·§3.2.2 ③ entryJson 바인딩(diffHash 재동의 스킵의
// 근거)을 모두 여기서 구현한다.
//
// [순수성] 네트워크·프로세스·fs 호출이 없다 — `loadCodeConstants()`는 빌드 시 번들된
// JSON import를 1회 캐시해 반환할 뿐 I/O가 없다(`codeConstants.ts` 원문 주석 참고,
// `resolveInstallTarget`가 이미 같은 근거로 plan 단계에서 쓰인다).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { compareVersions, parseRange, parseVersion } from '../../core/policy/semver.js';
import type { EffectiveAgent, EffectiveCompat } from '../../core/policy/types.js';
import type { Change, DesiredSlice, Observed, Plan } from '../../providers/types.js';
import { computeDiffHash } from '../../core/reconciler/diffHash.js';
import { buildAgentEntryDescriptor } from './entryDescriptor.js';
import { pluginNameOnly } from './cli.js';
import type { AgentObservedDetail } from './detect.js';
import { isKnownBlockedVersion } from './knownVersionBlock.js';

export interface AgentDesiredSlice extends DesiredSlice {
  readonly providerId: 'agent';
  readonly agent: EffectiveAgent;
  readonly compat: EffectiveCompat;
}

function emptyPlan(): Plan {
  return { providerId: 'agent', changes: [], diffHash: computeDiffHash('agent', []) };
}

/** 관측 버전이 `compat.malgnAgent`(이미 PR-9로 좁혀진 유효 범위) 하한 미만이면
 * "구버전 — 업데이트 필요"로 판정한다. 범위 파싱 불가는 판단 보류(false — 호출자가
 * 이미 다른 경로에서 유효성을 보장한 값을 넘긴다고 가정, `isBelowCompatMinimum`과
 * 동일한 관용구). */
function isOutdated(version: string, effectiveRange: string): boolean {
  const parsed = parseVersion(version);
  const range = parseRange(effectiveRange);
  if (!parsed || !range) return false;
  return compareVersions(parsed, range.lower.version) < 0;
}

export function planAgent(observed: Observed, desired: AgentDesiredSlice): Plan {
  if (desired.agent.blocked) {
    // 정책이 agent 대상을 지정하지 않았거나 화이트리스트 밖 — 아무것도 계획하지
    // 않는다(PR-6, §5.3 "플러그인 미설치" 행과 같은 정신: 판단 근거가 없으면 계획하지
    // 않는다). detect가 이미 관측한 사실은 Observed 자체에 남아 있어 UI가 계속 보여줄
    // 수 있다 — plan이 침묵한다고 상태 전체가 사라지지 않는다.
    return emptyPlan();
  }

  const detail = observed.detail as AgentObservedDetail | undefined;
  if (!detail || !detail.claudeAvailable) {
    return emptyPlan();
  }

  // 마켓플레이스 저장소 불일치 — 자동 정정하지 않는다(§3.2 O-1 "저장소가 아니라
  // 플러그인 하나"와 같은 신중함: 이름이 같은 다른 저장소를 자동으로 덮어쓰면 그
  // 자체가 새 공급망 위험이다).
  if (detail.marketplaceRegistered && detail.marketplaceRepoMatches === false) {
    return emptyPlan();
  }

  const constants = loadCodeConstants();
  if (detail.version && isKnownBlockedVersion(detail.version, constants.known)) {
    // known 결함 — apply를 계획하지 않는다(§3.5 "이 버전은 known 결함으로 apply가
    // 차단됩니다"). Observed.code는 detect가 이미 채웠으므로 여기서는 그저 계획을
    // 만들지 않는 것으로 그 판정을 실행한다.
    return emptyPlan();
  }

  const pluginName = pluginNameOnly(desired.agent.plugin);
  const needsInstall = !detail.installed;
  const needsEnable = detail.installed && detail.enabled === false;
  const needsRegisterMarketplace = !detail.marketplaceRegistered;
  const needsUpdate = detail.installed && detail.version !== null && isOutdated(detail.version, desired.compat.malgnAgent);

  if (!needsInstall && !needsEnable && !needsRegisterMarketplace && !needsUpdate) {
    return emptyPlan();
  }

  const after = buildAgentEntryDescriptor(desired.agent.marketplace, detail.entry, {
    plugin: pluginName,
    scope: desired.agent.scope,
    channel: desired.agent.channel,
  });

  const kind: Change['kind'] = needsInstall || needsRegisterMarketplace ? 'install' : needsUpdate ? 'update' : 'exec';
  const rationaleParts: string[] = [];
  if (needsRegisterMarketplace) rationaleParts.push('마켓플레이스 등록');
  if (needsInstall) rationaleParts.push('플러그인 설치');
  if (needsUpdate) rationaleParts.push('업데이트(compat 하한 이상으로)');
  if (needsEnable) rationaleParts.push('플러그인 활성화');

  const change: Change = {
    id: 'agent-plugin',
    target: desired.agent.plugin,
    kind,
    level: 'L2',
    // `before`에 관측된 정확한 버전 문자열을 싣지 않는다(의도적) — §3.2.2 ③이 요구하는
    // "entryJson 동일 + compat 범위 내 버전 업데이트는 최초 1회 동의가 커버한다"는
    // `computeDiffHash(providerId, changes)`가 `Change` 전체를 해싱하기 때문에 성립하려면
    // `Change`의 어떤 필드도 순수 버전 숫자로 매 재탐지마다 달라지면 안 된다. 정확한
    // 현재 버전은 콘솔/동의 화면이 항상 함께 갖고 있는 `Observed.detail.version`에서
    // 보여주면 된다 — Change 내용에 중복해 실을 필요가 없다(반환문에 명시할 설계 갭
    // 채움: 원문에는 `before` 필드의 정확한 값 규칙이 없다).
    after,
    // claude plugin uninstall/disable로 되돌릴 수 있다 — I-A(전역 매니저 설치)와 달리
    // 정본 CLI 자신이 되돌리기 명령을 제공한다(architecture.md §4.8.2 I-A와의 대비).
    reversible: true,
    rationale: rationaleParts.join(' + '),
  };

  return { providerId: 'agent', changes: [change], diffHash: computeDiffHash('agent', [change]) };
}
