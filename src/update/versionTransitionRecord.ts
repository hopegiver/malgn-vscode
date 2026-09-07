// U-4 (docs/architecture.md §3.6.2 "버전 전환의 가시성") — "상주 프로세스가 사용자
// 모르게 자기를 교체·재시작하면 동의한 버전과 도는 버전이 갈린다. ① UI 표기
// ② 저널 기록 ③ 전환 후 첫 apply에서 재동의 여부 판정."
//
// 이 파일은 ②(저널 기록)의 데이터 계약과 기록 함수만 담는다.
//
// - **①(UI 표기)**: 호스트 어댑터(W-N1)가 이 레코드를 읽어 렌더링한다 — 이 코어
//   모듈은 UI를 갖지 않는다(Electron 바인딩은 W-N1 몫이라는 이 슬라이스의 전제
//   그대로).
// - **③(전환 후 첫 apply 재동의 판정)**: 이미 `src/core/consent/gate.ts`가 담당하고
//   있다 — 그 파일은 동의 레코드의 `extensionVersion`이 "지금 실행 중인 것"의
//   `extensionVersion`(빌드 시 `compat/compatibility.json`에서 주입되는 값)과
//   다르면 동의를 무효화한다. 자동 업데이트로 바이너리가 교체되면 새 바이너리에는
//   새 버전의 `compat/compatibility.json`이 함께 번들되므로, 다음 동의 검증에서
//   그 불일치가 **결합 없이 저절로** 감지된다 — 이 파일이 그 경로를 알거나 호출할
//   필요가 없다(그리고 AT-U1이 어차피 그 방향의 import를 금지하므로, 설령 필요하더라도
//   이 파일에서 그 경로를 직접 참조할 수는 없다).

import type { UpdateTriggerKind } from './updateTrigger.js';

export interface VersionTransitionRecord {
  readonly ts: string;
  readonly fromVersion: string;
  readonly toVersion: string;
  readonly trigger: UpdateTriggerKind;
}

/** 실제 저장 방식은 주입받는다(DI 원칙 — 이 모듈은 파일시스템을 직접 만지지 않는다).
 * 호스트 어댑터가 저널·UI 상태 저장소 중 무엇을 골라 구현할지는 W-N1의 몫이다. */
export interface VersionTransitionSink {
  record(entry: VersionTransitionRecord): Promise<void>;
}

/** ②(저널 기록)의 유일한 기록 지점 — 다른 곳에서 `sink.record`를 직접 부르지 않고
 * 항상 이 함수를 거치게 하면, 나중에 레코드 형태가 바뀔 때 고칠 곳이 하나로
 * 고정된다. */
export async function recordVersionTransition(sink: VersionTransitionSink, entry: VersionTransitionRecord): Promise<void> {
  await sink.record(entry);
}
