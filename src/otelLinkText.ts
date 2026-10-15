// 수집키 연동 화면의 "결과 문구"와 판정 — DOM·IPC 없는 순수 함수. settings.ts가 토스트·입력칸 상태를 정할 때 쓰고,
// 단위 테스트(scripts/tests/otelLinkText.test.mjs)가 실제 결과와 문구가 어긋나지 않는지 고정한다.
// (타입 import만 있어 번들러 없이 불러온다.)
import type { IngestLinkStatus, IngestReissueOutcome } from './otelApi';
import type { IngestKeyRemoval } from './hubAuthApi';

/**
 * 허브에 연결되어 앱이 수집키를 자동 관리하는 중인가. 이 동안 직접 붙여넣은 수집키는 다음 점검에서 앱이 발급한
 * 키로 교체되고(계약 D-4) 회전 예산을 쓰므로 수동 입력을 받지 않는다(백엔드 `otel_settings_save`도 같은 조건으로 거절한다).
 * 상태를 아직 못 읽었으면 false — 입력은 막지 않고, 백엔드가 최종 판단한다.
 */
export function keyManagedByHub(status: IngestLinkStatus | null): boolean {
  return status !== null && status.hubConnected && status.issueSupported;
}

export interface ReissueToast {
  /** applied: 새 키가 적용됨 / failed: 원인이 있는 실패 / pending: 시작하지 못했거나 결과를 알 수 없음 / declined: 사용자가 거절. */
  readonly kind: 'applied' | 'failed' | 'pending' | 'declined';
  /** 토스트 문구. 거절은 토스트가 없다. */
  readonly text: string | null;
}

/**
 * 재발급 확인창에 답한 결과(백엔드가 직접 돌려준 `outcome`)를 토스트로 옮긴다. 결과 하나에 문구 하나가 대응한다 — 화면 상태
 * (masked·notice·폐기 표시)를 비교해 성공을 추론하지 않는다. "다시 발급해 적용했습니다"는 엔진이 새 키를 받아 설정 파일에 적용했다고
 * 알린 `minted`일 때만 나온다.
 */
export function reissueToast(outcome: IngestReissueOutcome): ReissueToast {
  switch (outcome.kind) {
    case 'declined':
      return { kind: 'declined', text: null };
    case 'minted':
      return { kind: 'applied', text: '수집키를 다시 발급해 적용했습니다. 새로 시작하는 Claude Code 세션부터 적용됩니다.' };
    case 'notStarted':
      return outcome.reason === 'busy'
        ? { kind: 'pending', text: '다른 점검이 진행 중이라 재발급을 시작하지 못했습니다. 잠시 후 [확인하기]나 [다시 시도]를 눌러 주세요.' }
        : { kind: 'pending', text: '지금은 재발급 확인을 기다리는 상태가 아니라서 발급하지 않았습니다. 현재 상태는 홈과 설정 > OTel에서 확인하세요.' };
    case 'noConnection':
      return { kind: 'failed', text: '허브 연결이 없거나 요청 도중 바뀌어 수집키를 발급하지 않았습니다. 허브 연결 상태를 확인하세요.' };
    case 'notApplied':
      return { kind: 'pending', text: '발급 요청은 끝났지만 새 수집키는 적용하지 않았습니다. 현재 상태는 홈과 설정 > OTel에서 확인하세요.' };
    case 'failed': {
      const f = outcome.failure;
      // 연결이 끊긴 경우는 "다시 발급"으로 풀리지 않는다 — 다시 로그인해 연결하는 일이다.
      if (f.cause === 'reconnectRequired') return { kind: 'failed', text: `수집키를 받지 못했습니다: ${f.message}` };
      // 서버에 기능이 없는 동안(self 200)은 [다시 시도]해도 같은 결과라 그 길을 권하지 않는다(RV-005).
      const left = outcome.transient && f.cause !== 'featureUnavailable' ? ' 아직 새 수집키가 적용되지 않았습니다. [다시 시도]로 다시 발급할 수 있습니다.' : '';
      return { kind: 'failed', text: `수집키를 다시 발급하지 못했습니다: ${f.message}${left}` };
    }
  }
}

/** 허브 연결 해제 토스트. 서버 폐기 결과(reason)와 이 PC 설정 파일의 수집키 처리 결과(removal)를 각각 사실대로 말한다(RV-06). */
export function disconnectToast(reason: string | null, removal: IngestKeyRemoval | undefined): string {
  const head =
    reason === 'SERVER_REVOKE_FAILED' ? '연결을 해제했지만 서버 폐기에 실패했습니다. 웹 /keys에서 폐기하세요.' : '허브 연결을 해제했습니다.';
  switch (removal) {
    case 'removed':
      return `${head} 이 PC의 설정 파일에서 수집키도 지웠습니다.`;
    case 'notPresent':
      return `${head} 앱이 적용한 수집키가 설정 파일에 없어 설정 파일은 바꾸지 않았습니다.`;
    case 'failed':
      return `${head} 다만 이 PC의 수집키를 설정 파일에서 지우지 못했습니다. 설정 > OTel에서 직접 삭제하세요.`;
    default:
      return head;
  }
}

/** 이전 앱 버전이 설정 파일에 기록하던 `OTEL_LOGS_EXPORT_INTERVAL` 기본값(ms). */
export const LEGACY_DEFAULT_LOGS_INTERVAL = '5000';

/**
 * 설정 파일에 옛 기본값(5000)이 있을 때 보여 주는 권장값 안내. 앱은 파일의 값을 조용히 바꾸지 않는다 — 사용자가 직접 정한 5000과
 * 이전 앱이 기록한 5000을 파일만으로는 가를 수 없기 때문이다. 그래서 안내만 하고, 바꾸는 것은 사용자가 저장할 때다.
 * `recommended`는 백엔드가 준 현재 앱 기본값(`OtelSettings.defaults`)이다. 해당 없으면 null.
 */
export function legacyIntervalHint(key: string, fileValue: string | undefined, recommended: string | undefined): string | null {
  if (key !== 'OTEL_LOGS_EXPORT_INTERVAL' || fileValue !== LEGACY_DEFAULT_LOGS_INTERVAL) return null;
  if (!recommended || recommended === fileValue) return null;
  return `${fileValue}은 이전 앱 버전의 기본값입니다. 5초마다 허브로 보내면 허브의 요청 제한과 일일 사용량 한도를 빨리 쓰므로 권장값은 ${recommended}(ms)입니다. 직접 정한 값이면 그대로 두어도 됩니다. 단, 허브에 연결된 상태에서 수집키가 새로 발급·교체될 때는 허브가 보낸 값이 이 값을 덮어씁니다. 권장값으로 바꾸려면 값을 고쳐 저장하세요.`;
}
