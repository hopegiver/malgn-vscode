// 원격·컨테이너 환경 감지 — architecture.md §2.6(M-47) "감지 신호(WSL 배포판 존재 ·
// .devcontainer/ · SSH 원격 설정 등)를 발견하면 경고를 띄우고 그 환경에 대한 apply
// 경로를 만들지 않는다. 감지 실패는 apply를 막지 않는다 — 감지는 안내용이고 차단은
// '원격 경로가 코드에 없다'는 구조적 사실이 집행한다."
//
// **집행은 이 파일이 하지 않는다.** 이 파일은 배너에 쓸 신호만 계산한다 — 실제 차단은
// "원격 대상에 대한 apply()를 만들지 않는다"는 구조적 사실이 집행하고(이 슬라이스에는
// apply 경로 자체가 아직 없다 — activationSequence.ts가 그것을 증명한다), 이 신호를
// 소비해 apply 여부를 가르는 조건문은 이 저장소 어디에도 없어야 한다(그런 조건문을
// 추가하는 순간 §2.6이 명시적으로 금지한 "명령 생성 → 원격 붙여넣기" 절충안 없는
// 원격 지원을 절반 만드는 셈이 된다).

export interface RemoteEnvSignals {
  readonly ssh: boolean;
  readonly wsl: boolean;
}

/**
 * 현재 프로세스 환경변수만으로 판단 가능한 두 신호. `.devcontainer/` 존재 여부는
 * "어느 폴더 기준인가"가 워크스페이스 개념이 사라진 네이티브 앱에서 정의되지 않아
 * (구 판은 워크스페이스 루트가 있었다) 이 전역 배너 신호에는 포함하지 않는다 — 이후
 * I-B 대상 폴더 지정 provider(install 등)가 생기면 그 폴더 기준으로 별도 검사할 수
 * 있다(그 자리는 이 함수가 아니라 해당 provider의 detect()).
 */
export function detectRemoteEnvSignals(env: Readonly<Record<string, string | undefined>>): RemoteEnvSignals {
  const ssh = Boolean(env.SSH_CONNECTION || env.SSH_TTY || env.SSH_CLIENT);
  const wsl = Boolean(env.WSL_DISTRO_NAME || env.WSL_INTEROP);
  return { ssh, wsl };
}

export function isAnyRemoteEnvDetected(signals: RemoteEnvSignals): boolean {
  return signals.ssh || signals.wsl;
}

/** §2.6 "헤더 상시 표시" 원문 그대로. 원격 신호 유무와 무관하게 항상 표시한다 —
 * 감지가 실패해도(오탐 방지가 아니라 신호 자체가 없어도) 안내는 항상 떠 있어야
 * "감지는 안내용, 차단은 구조적 사실"이라는 원칙이 지켜진다. */
export const REMOTE_SCOPE_BANNER_TEXT = '이 앱은 이 PC만 프로비저닝합니다. WSL·SSH·devcontainer 환경은 대상이 아닙니다.';

/** 원격 신호가 감지됐을 때 추가로 띄우는 경고 문구 — 배너와 별개로 1회성 알림에 쓴다. */
export function buildRemoteEnvWarningMessage(signals: RemoteEnvSignals): string | null {
  if (!isAnyRemoteEnvDetected(signals)) return null;
  const detected: string[] = [];
  if (signals.ssh) detected.push('SSH 원격 세션');
  if (signals.wsl) detected.push('WSL');
  return `${detected.join(', ')} 환경이 감지되었습니다 — 이 환경은 프로비저닝 대상이 아닙니다. ${REMOTE_SCOPE_BANNER_TEXT}`;
}
