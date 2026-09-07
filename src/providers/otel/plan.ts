// otel provider plan() — architecture.md §4.2 "병합 규칙 — 덮어쓰기 금지 불변량의
// 구체형": 키 단위 3-way 비교. 정책에 있고 로컬에 없으면 add(L1), 같으면 no-op,
// 다르면 conflict(L2). 로컬에만 있는 `OTEL_*` 키는 삭제하지 않고 보고만(L0 — Change를
// 만들지 않는다, "삭제"라는 Change kind 자체가 없다). 순수 함수(네트워크·fs 없음,
// PR-2) — 필요한 사실은 전부 `Observed.detail`을 통해서만 받는다.

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { computeDiffHash } from '../../core/reconciler/diffHash.js';
import type { Change, DesiredSlice, Observed, Plan } from '../../providers/types.js';
import { buildDesiredOtelEnv, buildOtelResourceAttributesValue, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';
import type { OtelObservedDetail } from './detect.js';

export interface OtelDesiredSlice extends DesiredSlice {
  readonly providerId: 'otel';
  /** `EffectivePolicy.otel.env`(이미 loader.ts 검증을 통과한 값) — 정책을 못 읽었거나
   * otel이 blocked면 `null`. `desiredEnv.ts`의 `buildDesiredOtelEnv`가 이 값이 비어
   * 있을 때 사이트면 기본값으로 떨어진다. */
  readonly policyEnv: Readonly<Record<string, string>> | null;
}

export interface OtelPlanDeps {
  /** OS 로그인 사용자명(§4.2 "employee.id는 런타임에 OS 로그인 사용자명으로 채운다") —
   * plan() 호출 시점에는 이미 구해져 있는 값을 주입받는다(plan 자신은 os 모듈을 호출하지
   * 않는다 — PR-2 순수성). */
  readonly osUsername: string;
}

function emptyPlan(): Plan {
  return { providerId: 'otel', changes: [], diffHash: computeDiffHash('otel', []) };
}

/** 한 env 키의 변경을 Change로 만든다. `before`가 없으면(로컬에 그 키가 아예 없으면)
 * L1(순수 추가), 있고 값이 다르면 L2(덮어쓰기 — 사람의 명시 동의가 필요하다, §4.2). */
function envChange(id: string, key: string, before: string | undefined, after: string): Change {
  return {
    id,
    target: `env.${key}`,
    kind: before === undefined ? 'add' : 'update',
    level: before === undefined ? 'L1' : 'L2',
    before,
    after,
    // settings.json은 apply() 직전에 백업되므로(§4.2 "쓰기 전 백업") 백업에서 복원하는
    // 형태로 되돌릴 수 있다 — CLI가 되돌리기 명령을 제공하는 agent와는 되돌리는 방식이
    // 다를 뿐, 되돌릴 수 없는 변경은 아니다.
    reversible: true,
    rationale: before === undefined ? '조직 OTel 설정 키 추가' : '조직 OTel 설정 값 갱신',
  };
}

export function planOtel(deps: OtelPlanDeps, observed: Observed, desired: OtelDesiredSlice): Plan {
  // 판단 근거가 없으면(blocked/unknown — 헬퍼 없음·OS 미지원·settings.json 파싱 실패
  // 등) 계획하지 않는다(PR-6). detect가 detail을 채우지 않은 상태가 바로 이 신호다.
  const detail = observed.detail as OtelObservedDetail | undefined;
  if (!detail || !detail.platformSupported) return emptyPlan();
  if (detail.headersHelperConfigured === false || detail.headersHelperExecutable === false) return emptyPlan();

  const constants = loadCodeConstants();
  const desiredResult = buildDesiredOtelEnv(desired.policyEnv, constants);
  if (desiredResult.blocked) return emptyPlan();

  const changes: Change[] = [];
  for (const [key, afterValue] of Object.entries(desiredResult.env)) {
    const beforeValue = detail.observedEnv[key];
    if (beforeValue === afterValue) continue; // no-op — 정책값과 로컬값이 이미 같다
    changes.push(envChange(`otel-env-${key}`, key, beforeValue, afterValue));
  }

  // OTEL_RESOURCE_ATTRIBUTES — allowedOtelEnvKeys 화이트리스트 밖(§2.4 별도 관리)이라
  // 위 루프에 섞이지 않는다. employee.id만 담고 employee.name은 절대 담지 않는다
  // (사람 승인 확정 사항). 이미 같은 값이면 no-op.
  const desiredResourceAttributes = buildOtelResourceAttributesValue(deps.osUsername);
  const beforeResourceAttributes = detail.observedEnv[OTEL_RESOURCE_ATTRIBUTES_KEY];
  if (beforeResourceAttributes !== desiredResourceAttributes) {
    changes.push(envChange('otel-env-resource-attributes', OTEL_RESOURCE_ATTRIBUTES_KEY, beforeResourceAttributes, desiredResourceAttributes));
  }

  // 로컬에만 있는 OTEL_* 키(§4.2 "로컬에만 있는 OTEL_* 키는 삭제하지 않고 보고만")는
  // 여기서 의도적으로 Change를 만들지 않는다 — `Observed.detail.observedEnv`에 이미
  // 그 값이 남아 있어 UI가 계속 보여줄 수 있고, "삭제"에 대응하는 `ChangeKind`가
  // 애초에 이 provider군 계약에 없다(반환문에 명시).
  if (changes.length === 0) return emptyPlan();

  return { providerId: 'otel', changes, diffHash: computeDiffHash('otel', changes) };
}
