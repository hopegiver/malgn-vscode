// AT-U4 (docs/architecture.md §3.6.1) — "업데이트 트리거의 입력이 전수 열거된다:
// 트리거 함수가 받는 것은 ⓐ 타이머 ⓑ 사용자 명령 ⓒ 코드 상수 authority에서 받은
// 서명 검증 통과 매니페스트 셋뿐이다." `src/architecture-tests/updateChannelBoundary.ts`의
// `ALLOWED_UPDATE_TRIGGER_KINDS`가 그 계약이 정하는 리터럴 태그 이름 3종을 그대로
// 못박아 뒀다 — 여기서 새 이름을 짓지 않고 그 이름을 그대로 쓴다.
//
// [파일이 최소한으로 유지되는 이유] AT-U4는 파일 경로에 "trigger"가 들어가고 본문에
// "Trigger" 식별자가 등장하는 파일 전체에서 소문자 단일 인용 문자열 리터럴을 모두
// 뽑아 허용 3종 밖의 것이 있으면 위반으로 본다. 그래서 이 파일에는 그 세 리터럴
// 이외의 소문자 단일 인용 문자열을 두지 않는다(다른 문자열이 필요한 로직은 전부
// 이 파일 밖— `updateFlow.ts` 등—에 둔다).

export type UpdateTriggerKind = 'timer' | 'user-command' | 'verified-manifest';

export interface TimerUpdateTrigger {
  readonly kind: 'timer';
}

export interface UserCommandUpdateTrigger {
  readonly kind: 'user-command';
}

export interface VerifiedManifestUpdateTrigger {
  readonly kind: 'verified-manifest';
}

/** ⓐⓑⓒ 셋의 합집합 — 이 유니온 밖의 4번째 트리거 종류를 추가하려는 시도는 AT-U4가
 * 잡는다(src/architecture-tests/updateChannelBoundary.test.ts의 fixture 테스트가
 * 그 검출을 실측한다). */
export type UpdateTrigger = TimerUpdateTrigger | UserCommandUpdateTrigger | VerifiedManifestUpdateTrigger;

export class UnknownUpdateTriggerKindError extends Error {}

/** 외부(unknown)에서 들어온 문자열을 트리거 종류로 취급하기 전 마지막 런타임
 * 재확인 — 유니온 타입은 컴파일 타임 계약이고, 이 함수는 그 계약을 실행 경로에서도
 * 다시 강제한다. */
export function assertKnownUpdateTriggerKind(kind: string): asserts kind is UpdateTriggerKind {
  if (kind !== 'timer' && kind !== 'user-command' && kind !== 'verified-manifest') {
    throw new UnknownUpdateTriggerKindError(`알 수 없는 업데이트 트리거 종류: ${kind}`);
  }
}
