// 수집키 연동 화면의 "결과 문구"와 판정 — DOM·IPC 없는 순수 함수. settings.ts가 토스트·입력칸 상태를 정할 때 쓰고,
// 단위 테스트(scripts/tests/otelLinkText.test.mjs)가 실제 결과와 문구가 어긋나지 않는지 고정한다.
// (타입 import만 있어 번들러 없이 불러온다.)
import type { IngestLinkStatus } from './otelApi';
import type { IngestKeyRemoval } from './hubAuthApi';

/**
 * 허브에 연결되어 앱이 수집키를 자동 관리하는 중인가. 이 동안 직접 붙여넣은 수집키는 다음 점검에서 앱이 발급한
 * 키로 교체되고(계약 D-4) 회전 예산을 쓰므로 수동 입력을 받지 않는다(백엔드 `otel_settings_save`도 같은 조건으로 거절한다).
 * 상태를 아직 못 읽었으면 false — 입력은 막지 않고, 백엔드가 최종 판단한다.
 */
export function keyManagedByHub(status: IngestLinkStatus | null): boolean {
  return status !== null && status.hubConnected && status.issueSupported;
}

export interface ReissueOutcome {
  /** applied: 새 키가 적용됨 / failed: 원인이 있는 실패 / pending: 아직 처리되지 않았거나 진행 중 / declined: 사용자가 거절. */
  readonly kind: 'applied' | 'failed' | 'pending' | 'declined';
  /** 토스트 문구. 거절은 토스트가 없다. */
  readonly text: string | null;
}

/** 허브가 파일의 키를 웹에서 폐기된 것으로 보고 있는가(확인 대기·거절·재확인 대기 모두). 이 동안 그 키는 수집되지 않는다. */
export function revokedByHub(status: IngestLinkStatus): boolean {
  return status.confirmRequired || status.confirmDeclined || status.revokedByHub === true;
}

/**
 * 재발급 확인창에 답한 뒤 백엔드가 돌려준 상태로 토스트를 정한다. "다시 발급해 적용했습니다"는 **다른 키가 적용됐다는 관측**이
 * 있을 때만 말한다 — 응답이 적용됨이고 폐기 확인 대기가 풀렸으며, 적용된 키(`masked`)가 확인창에 답하기 전의 키
 * (`beforeMasked`)와 다를 때다. 동의했다는 사실이나 "적용됨" 상태만으로 성공을 주장하지 않는다 — 폐기된 키가 파일에 그대로 남은 채
 * 발급 요청이 일시 오류로 끝난 경우가 같은 모양이기 때문이다(RV-06·RV-09). 답하기 전 상태를 모르면(`beforeMasked === undefined`)
 * 관측할 수 없으므로 성공이라 말하지 않는다. 키가 없던 상태에서 시작했으면 `beforeMasked`는 null이다.
 */
export function reissueOutcome(status: IngestLinkStatus | null, accepted: boolean, beforeMasked: string | null | undefined): ReissueOutcome {
  if (!accepted) return { kind: 'declined', text: null };
  if (!status) return { kind: 'pending', text: '재발급을 요청했습니다. 결과는 홈과 설정 > OTel에서 확인하세요.' };
  if (status.failure) return { kind: 'failed', text: `수집키를 다시 발급하지 못했습니다: ${status.failure.message}` };
  const keyApplied = status.state === 'applied' || status.state === 'confirmed';
  const revoked = revokedByHub(status);
  if (keyApplied && !revoked && status.masked !== null && beforeMasked !== undefined && status.masked !== beforeMasked) {
    return { kind: 'applied', text: '수집키를 다시 발급해 적용했습니다. 새로 시작하는 Claude Code 세션부터 적용됩니다.' };
  }
  if (status.state === 'issuing') {
    return { kind: 'pending', text: '수집키를 발급하는 중입니다. 결과는 홈과 설정 > OTel에서 확인하세요.' };
  }
  // 발급 요청이 일시 오류로 끝났다 — 새 키는 없고, 이전 키가 파일에 그대로 남아 있다.
  if (status.notice) {
    const left = revoked ? ' 웹에서 폐기된 수집키가 아직 이 PC에 남아 있습니다. [다시 시도]로 다시 발급할 수 있습니다.' : '';
    return { kind: 'failed', text: `수집키를 다시 발급하지 못했습니다: ${status.notice.message}${left}` };
  }
  if (status.confirmRequired || status.confirmDeclined) {
    return { kind: 'pending', text: '다른 점검이 진행 중이라 재발급을 시작하지 못했습니다. 잠시 후 [확인하기]나 [다시 시도]를 눌러 주세요.' };
  }
  if (keyApplied && !revoked) {
    return { kind: 'pending', text: '새 수집키가 적용됐는지 확인하지 못했습니다. 결과는 홈과 설정 > OTel에서 확인하세요.' };
  }
  return { kind: 'failed', text: '수집키를 발급하지 못했습니다. 허브 연결 상태를 확인하세요.' };
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
