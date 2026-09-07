// claude CLI 버전 실측 — §2.2 ⑤ "각 provider detect()"의 install/agent 이전에, 활성화
// 시퀀스 ②(호환 게이트, §3.5)가 필요로 하는 신호 생산 지점. `core/reconciler/stopGate.ts`의
// `isBelowCompatMinimum()`은 이미 계산된 실측 버전 문자열을 받기만 하고 "실제
// `claude --version` 실행은 이 함수 밖(provider detect() 몫)"이라 명시한다 — provider가
// 아직 없는 지금(W7 install/agent 이전) 그 실행 지점은 호스트 어댑터(W-N1)가 대신 채운다.
//
// [실행 제약 — tech-stack.md §4] "실행 파일의 출처 제한": PATH에서 해석된 사용자 전역
// 도구만, 최소 env(PATH·HOME)만 전달. 셸을 경유하지 않는다(`execFile`, 문자열 결합 없음).
// [N-3] "detect()가 응답 없음(예: gh auth status가 keyring 프롬프트로 멈춤)" — 같은
// 실패 형태를 claude CLI도 겪을 수 있어 5초 타임아웃(§2.2 ⑤ DETECT_TIMEOUT_MS와 동일
// 정본 값, `core/reconciler/engine.ts`)을 건다.

export const CLAUDE_VERSION_DETECT_TIMEOUT_MS = 5000;

/** `execFile`을 그대로 감싼 DI 시그니처 — 테스트가 실제 프로세스를 띄우지 않고 가짜
 * 구현을 주입할 수 있게 한다. Node의 `execFile`과 동일한 콜백 계약이다. */
export type ExecFileFn = (
  file: string,
  args: readonly string[],
  options: { readonly timeout: number; readonly env: Readonly<Record<string, string | undefined>> },
  callback: (error: Error | null, stdout: string, stderr: string) => void
) => void;

/** `claude --version` 출력에서 semver 형태만 뽑는다. 실측 출력 형태가 문서화돼 있지
 * 않아(devops 확인 대상) 관대하게 첫 semver 패턴을 찾는다 — 형식이 어긋나면 파싱
 * 불가로 처리해 `null`을 반환한다(추측하지 않는다, PR-6). */
const SEMVER_IN_OUTPUT_RE = /(\d+\.\d+\.\d+)/;

export function parseClaudeVersionOutput(stdout: string): string | null {
  const match = SEMVER_IN_OUTPUT_RE.exec(stdout);
  return match ? match[1]! : null;
}

export interface DetectClaudeCodeVersionOptions {
  readonly execFileFn: ExecFileFn;
  readonly homeDir: string;
  readonly pathEnv: string;
  readonly timeoutMs?: number;
}

/**
 * `claude --version`을 최소 env(PATH·HOME만)로 실행해 semver를 얻는다. 실행 실패·
 * 타임아웃·파싱 불가는 모두 `null`(=`unknown`, PR-8 "던지지 않고 unknown 반환")로
 * 접는다 — 호출자가 이 결과 없이도(=호환 게이트를 건너뛰고) 계속 진행할 수 있어야
 * 한다(claude 미설치 PC에서 앱 자체가 멈추면 안 된다).
 */
export async function detectClaudeCodeVersion(options: DetectClaudeCodeVersionOptions): Promise<string | null> {
  const timeoutMs = options.timeoutMs ?? CLAUDE_VERSION_DETECT_TIMEOUT_MS;
  return new Promise((resolve) => {
    options.execFileFn(
      'claude',
      ['--version'],
      { timeout: timeoutMs, env: { PATH: options.pathEnv, HOME: options.homeDir } },
      (error, stdout) => {
        if (error) {
          resolve(null);
          return;
        }
        resolve(parseClaudeVersionOutput(stdout));
      }
    );
  });
}
