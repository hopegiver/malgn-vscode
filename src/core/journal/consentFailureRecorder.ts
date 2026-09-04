// W3 잔여 요구 이행 — architecture.md §1.2: "①②③⑤는 `MV_CONSENT_INVALID`(`high`) —
// 사고 취급이라 조용히 재요청하지 않고 로그·진단 리포트에 남긴다." `src/core/consent/
// gate.ts`(W3)는 이 요구를 구현하지 않은 채로 남겨졌다 — gate.ts 자체 주석: "apply()
// 오케스트레이션... 이번 슬라이스에는 없다"(그 오케스트레이션이 실제 호출자다). 이
// 슬라이스(W5)가 "저널·진단"을 다루는 자리이므로 여기서 "기록 방법"을 제공한다 — 실제
// apply 오케스트레이션(engine이 `gate.assertValid`를 부르고 이 함수로 실패를 기록하는
// 배선)은 W7이 이어받는다(반환문 "W7이 이어받을 지점" 참고).
//
// gate.ts는 건드리지 않는다(선행 슬라이스 계약 불변) — 이 모듈은 `ConsentGateError`를
// import해서 소비만 한다.

import { MV_CONSENT_INVALID } from '../consent/errors.js';
import type { ConsentGateError } from '../consent/gate.js';
import type { ProviderId } from '../../providers/types.js';
import type { MaskedLogger } from '../../ui/log.js';
import type { JournalStore } from './store.js';

export interface RecordConsentGateFailureDeps {
  readonly logger: MaskedLogger;
  /** `MV_CONSENT_INVALID`(high)일 때만 사용한다 — info(만료)는 로그만 남긴다. */
  readonly journal: JournalStore;
}

/**
 * `gate.assertValid`가 던진 에러를 등급대로 기록한다:
 *  - `MV_CONSENT_EXPIRED`(info, 정상 상황) → 로그만(`logger`의 info 레벨). 저널에 남기지 않는다
 *    — §1.2 "실패의 두 등급"의 취지(정상 상황을 사고 채널에 섞지 않는다)를 저널까지
 *    확장한 것(위 `types.ts` 주석과 같은 판단).
 *  - `MV_CONSENT_INVALID`(high, 사고 신호) → `logger.error` **+** `journal.appendConsentFailure`.
 *    "로그·진단 리포트에 남긴다"의 "진단"은 이 저널 엔트리를 가리킨다 — 다만
 *    `policy-contract.md §4`가 정의한 `DiagnosticReport`(always 필드 표)에는 이 정보를
 *    실을 슬롯이 없다(설계 문서가 v0.5에서 install 공백만 메웠고 이 축은 다루지 않았다)
 *    — **이 갭은 고치지 않고 반환문에 보고한다**(docs/ 수정 금지 범위).
 *
 * 이 함수는 에러를 삼키지 않는다 — 기록은 부수효과일 뿐이고, 호출자가 이미 캐치한
 * 에러를 여기 넘긴 뒤 자신의 흐름(재시도 금지·사용자 안내)을 계속 결정한다.
 */
export async function recordConsentGateFailure(
  error: ConsentGateError,
  deps: RecordConsentGateFailureDeps,
  providerId: ProviderId | null = null
): Promise<void> {
  if (error.severity === 'info') {
    deps.logger.info(`동의 만료(정상 상황): ${error.message}`);
    return;
  }

  deps.logger.error(`동의 게이트 사고 신호(${error.code}): ${error.message}`);

  if (error.code === MV_CONSENT_INVALID) {
    await deps.journal.appendConsentFailure({
      ts: new Date().toISOString(),
      code: error.code,
      severity: 'high',
      message: error.message,
      providerId,
    });
  }
}
