// otel provider 조립 — architecture.md §1.2 균일 Provider 인터페이스를 §4.2(OTel
// macOS 세팅)로 채운다. agent/mcp와 같은 `createXxxProvider(deps)` 팩토리 패턴 — 이
// 파일은 DI를 받아 `Provider` 객체 하나를 만들 뿐이고, 각 단계 로직은 detect.ts/
// plan.ts/apply.ts/verify.ts에 있다.
//
// [apply는 사용자 동의 후에만] 이 파일도 agent/mcp와 마찬가지로 `Provider.apply`를
// 활성화 시퀀스 안에서 스스로 호출하지 않는다 — `apply()`는 `ConsentToken`을 타입
// 수준에서 요구하고(providers/types.ts), 실제 호출은 `core/reconciler/
// applyOrchestrator.ts`(사람의 동의 화면을 거친 뒤)만의 몫이다(`host/app.ts`의
// "apply()를 호출하지 않는다" 불변량과 동일 구조).

import type { ApplyContext, ApplyResult, DesiredSlice, DetectContext, Observed, Plan, Provider, VerifyResult } from '../types.js';
import { computeDiffHash } from '../../core/reconciler/diffHash.js';
import { detectOtel, type OtelDetectDeps, type PathExecutable } from './detect.js';
import { planOtel, type OtelDesiredSlice, type OtelPlanDeps } from './plan.js';
import { applyOtel, type OtelApplyDeps } from './apply.js';
import { verifyOtel } from './verify.js';
import type { ReadTextFile } from './settingsFile.js';

export interface OtelProviderDeps {
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
  readonly pathExecutable: PathExecutable;
  readonly platform: NodeJS.Platform;
  /** apply() 쓰기 직전 백업 대상 디렉터리(§6.1 배치표 "백업 파일 → 앱 데이터
   * 디렉터리+backups/<ts>/") — 실제 경로 계산은 호출자(host/app.ts) 몫이다. */
  readonly backupsDir: string;
  /** §4.2 "employee.id는 런타임에 OS 로그인 사용자명으로 채운다" — 호출자가
   * `os.userInfo().username`을 1회 구해 주입한다(이 provider는 `node:os`를 직접
   * import하지 않는다 — 값이 아니라 그 값을 얻는 시점을 호출자가 통제하게 하기
   * 위해서다, 다른 provider들의 `env`/`claudeHomeDir` 주입과 같은 패턴). */
  readonly osUsername: string;
  /** 테스트 전용 — 운영 코드는 항상 현재 시각을 쓴다. */
  readonly now?: () => Date;
}

function isOtelDesiredSlice(desired: DesiredSlice): desired is OtelDesiredSlice {
  return desired.providerId === 'otel' && 'policyEnv' in desired;
}

export function createOtelProvider(deps: OtelProviderDeps): Provider {
  const detectDeps: OtelDetectDeps = {
    claudeHomeDir: deps.claudeHomeDir,
    readTextFile: deps.readTextFile,
    pathExecutable: deps.pathExecutable,
    platform: deps.platform,
  };
  const planDeps: OtelPlanDeps = { osUsername: deps.osUsername };
  const applyDeps: OtelApplyDeps = {
    claudeHomeDir: deps.claudeHomeDir,
    readTextFile: deps.readTextFile,
    backupsDir: deps.backupsDir,
    now: deps.now,
  };

  return {
    id: 'otel',
    dependsOn: [],
    detect: (ctx: DetectContext): Promise<Observed> => detectOtel(detectDeps, ctx),
    plan: (observed: Observed, desired: DesiredSlice): Plan => {
      if (!isOtelDesiredSlice(desired)) {
        // 계약 위반(호출자가 잘못된 desired를 넘김) — 추측하지 않고 빈 plan(PR-6).
        return { providerId: 'otel', changes: [], diffHash: computeDiffHash('otel', []) };
      }
      return planOtel(planDeps, observed, desired);
    },
    apply: (plan: Plan, _consent, ctx: ApplyContext): Promise<ApplyResult> => applyOtel(applyDeps, plan, ctx),
    verify: (ctx: DetectContext): Promise<VerifyResult> => verifyOtel(detectDeps, ctx),
  };
}
