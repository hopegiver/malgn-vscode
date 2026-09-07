// otel provider apply() — architecture.md §4.2 "파일 쓰기는... 최소 범위만 수정 →
// 임시 파일 → rename(원자적)이며 쓰기 전 백업". `ConsentToken`은 타입 수준에서
// 요구되지만(providers/types.ts) 이 함수 자신은 `gate.assertValid`를 호출하지 않는다
// (agent/mcp와 동일 관용구 — 재검증은 오케스트레이션의 단일 호출 지점 몫).
//
// [쓰기 범위] Keychain 항목·헤더 헬퍼 스크립트 생성/교체는 이 함수가 하지 않는다 —
// detect()가 이미 "헬퍼 없음/실행 불가"를 `blocked`로 판정해 plan()이 빈 계획을
// 만들므로(§4.2 표 1행 "헬퍼 실행 가능?"), 이 함수가 실행될 때는 헬퍼가 이미 정상
// (PR-1 "이미 정본이 있으면 손대지 않는다")이라는 전제가 성립한다. 헬퍼가 없는 PC에
// Keychain 자격증명을 새로 발급하는 흐름(비밀번호 출처 UI)은 이 슬라이스 범위 밖이다
// (반환문에 명시).

import type { ApplyContext, ApplyResult, Plan } from '../../providers/types.js';
import { claudeSettingsPath, writeClaudeSettingsEnvAtomic, type ReadTextFile } from './settingsFile.js';
import { MV_OTEL_APPLY_FAILED, MV_OTEL_OK } from './errors.js';

export interface OtelApplyDeps {
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
  readonly backupsDir: string;
  /** 테스트 전용 — 운영 코드는 항상 현재 시각을 쓴다(백업 타임스탬프 디렉터리 이름). */
  readonly now?: () => Date;
}

function failure(code: string, message: string): ApplyResult {
  return { providerId: 'otel', status: 'blocked', code, message, appliedChangeIds: [] };
}

/** `Change.target`(`env.<KEY>`) → 키 이름만 뽑는다. 이 provider의 `plan.ts`가 만드는
 * target 형태와 1:1 대응하는 유일한 파서다(다른 target 형태를 만들면 여기도 같이
 * 고쳐야 한다). */
function envKeyFromTarget(target: string): string | null {
  return target.startsWith('env.') ? target.slice('env.'.length) : null;
}

export async function applyOtel(deps: OtelApplyDeps, plan: Plan, _ctx: ApplyContext): Promise<ApplyResult> {
  if (plan.changes.length === 0) {
    return { providerId: 'otel', status: 'ok', code: MV_OTEL_OK, message: '적용할 변경이 없습니다', appliedChangeIds: [] };
  }

  const updates: Record<string, string> = {};
  for (const change of plan.changes) {
    const key = envKeyFromTarget(change.target);
    if (!key) {
      return failure(MV_OTEL_APPLY_FAILED, `알 수 없는 변경 대상입니다: ${change.target}`);
    }
    updates[key] = change.after;
  }

  const path = claudeSettingsPath(deps.claudeHomeDir);
  try {
    await writeClaudeSettingsEnvAtomic({
      path,
      readTextFile: deps.readTextFile,
      updates,
      backupsDir: deps.backupsDir,
      now: deps.now ?? (() => new Date()),
    });
  } catch (error) {
    return failure(MV_OTEL_APPLY_FAILED, `settings.json 쓰기 실패: ${error instanceof Error ? error.message : String(error)}`);
  }

  return {
    providerId: 'otel',
    status: 'ok',
    code: MV_OTEL_OK,
    message: `${plan.changes.length}개 env 키를 적용했습니다`,
    appliedChangeIds: plan.changes.map((c) => c.id),
  };
}
