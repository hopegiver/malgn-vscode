// otel provider verify() — apply 직후 재확인. agent/mcp와 같은 관용구: 별도 재확인
// 로직을 새로 만들지 않고 `detectOtel()`을 다시 돌려 같은 사실 확인 경로를 재사용한다
// (검증 로직 이중 정의 금지 — 이 provider의 "정답"은 detect() 하나뿐이다).

import type { DetectContext, VerifyResult } from '../../providers/types.js';
import { detectOtel, type OtelDetectDeps } from './detect.js';
import { MV_OTEL_OK, MV_OTEL_VERIFY_FAILED } from './errors.js';

function nowIso(): string {
  return new Date().toISOString();
}

export async function verifyOtel(deps: OtelDetectDeps, ctx: DetectContext): Promise<VerifyResult> {
  const observed = await detectOtel(deps, ctx);
  if (observed.status === 'ok') {
    return { providerId: 'otel', status: 'ok', code: MV_OTEL_OK, message: 'OTel 설정이 desired 상태로 재확인됐습니다', verifiedAt: nowIso() };
  }
  return {
    providerId: 'otel',
    status: observed.status === 'blocked' ? 'blocked' : 'drift',
    code: MV_OTEL_VERIFY_FAILED,
    message: `적용 직후 재확인이 desired 상태와 다릅니다: ${observed.message}`,
    verifiedAt: nowIso(),
  };
}
