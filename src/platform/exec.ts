// 공용 안전 실행 래퍼 — architecture.md §3.2.2 원문 "실행은 항상 `platform/exec.ts`로
// 셸 미경유·argv 배열, 타임아웃 120초, 출력은 마스킹 후 로그" · tech-stack.md §4/§7
// "child_process 직접 사용 금지(`platform/exec.ts` 경유만)".
//
// [단일 지점] `child_process.execFile`을 직접 부르는 곳은 이 모듈의 어댑터 경계
// (`ExecFileFn` DI, 실제 구현은 `src/host/electron/nodeExecFileAdapter.ts`) 하나뿐이다 —
// provider 구현체(agent/mcp)는 이 파일의 `runExec()`만 호출하고 `node:child_process`를
// 직접 import하지 않는다(`agentMcpNoDirectChildProcess.test.ts`가 이 성질을 grep으로
// 고정한다). 셸을 경유하지 않는다(`execFile`, 문자열 결합·`shell:true` 없음).
//
// [출력 마스킹] `runExec()` 자신은 stdout/stderr 원문을 그대로 반환한다(호출자가 파싱해야
// 하므로 원문이 필요하다) — 로그에 남기기 직전에만 `toLoggableExecResult()`를 거쳐
// `core/diagnostics/mask.ts`의 단일 마스킹 지점을 통과시킨다. "원문을 쓰되 로그는 가린다"의
// 경계가 이 두 함수의 분리다.

import { maskDeepValues } from '../core/diagnostics/mask.js';

/** Node `child_process.execFile`과 호환되는 콜백 계약. `code`는 프로세스 종료 코드
 * (숫자) 또는 spawn 실패 시 `'ENOENT'` 등 문자열이다. `killed`/`signal`은 `timeout`·
 * `signal` 옵션에 의해 강제 종료됐을 때 채워진다(둘 다 execFile의 기본 동작). */
export interface ExecError extends Error {
  readonly code?: number | string | null;
  readonly killed?: boolean;
  readonly signal?: NodeJS.Signals | null;
}

export type ExecFileFn = (
  file: string,
  args: readonly string[],
  options: {
    readonly timeout: number;
    readonly env: Readonly<Record<string, string | undefined>>;
    readonly signal?: AbortSignal;
  },
  callback: (error: ExecError | null, stdout: string, stderr: string) => void
) => void;

export interface RunExecOptions {
  readonly execFileFn: ExecFileFn;
  readonly file: string;
  readonly args: readonly string[];
  /** 상속이 아니라 명시 구성 — 호출자가 필요한 키만 채운다(§4.8.4 F-4와 같은 정신을
   * agent/mcp 실행에도 적용: 최소 env). */
  readonly env: Readonly<Record<string, string | undefined>>;
  readonly timeoutMs: number;
  /** engine의 detect 타임아웃(§2.2 ⑤)이나 사용자 취소가 이 신호를 통해 하위 프로세스
   * kill로 이어진다 — DetectContext.signal을 그대로 전달하면 된다. */
  readonly signal?: AbortSignal;
}

export type RunExecOutcome =
  | { readonly kind: 'ok'; readonly stdout: string; readonly stderr: string }
  | { readonly kind: 'nonzero-exit'; readonly exitCode: number; readonly stdout: string; readonly stderr: string }
  | { readonly kind: 'timeout'; readonly stdout: string; readonly stderr: string }
  | { readonly kind: 'spawn-error'; readonly message: string };

/**
 * argv 배열 하나를 셸 없이 실행한다. 결과를 예외로 던지지 않고 판별 유니온으로
 * 돌려준다(PR-8과 같은 정신 — 호출자가 매번 try/catch를 반복하지 않아도 되고, 실패
 * 형태를 타입으로 구분할 수 있다). `timeout`이 지나면 execFile이 자동으로 kill하고
 * `error.killed === true`가 되므로 그 신호로 `'timeout'`을 구분한다.
 */
export async function runExec(options: RunExecOptions): Promise<RunExecOutcome> {
  return new Promise((resolve) => {
    options.execFileFn(
      options.file,
      options.args,
      { timeout: options.timeoutMs, env: options.env, signal: options.signal },
      (error, stdout, stderr) => {
        if (!error) {
          resolve({ kind: 'ok', stdout, stderr });
          return;
        }
        if (error.killed) {
          resolve({ kind: 'timeout', stdout, stderr });
          return;
        }
        if (typeof error.code === 'number') {
          resolve({ kind: 'nonzero-exit', exitCode: error.code, stdout, stderr });
          return;
        }
        resolve({ kind: 'spawn-error', message: error.message });
      }
    );
  });
}

/** 로그 싱크로 내보내기 직전에만 통과시키는 마스킹 경계 — `runExec()`의 원문 결과 자체는
 * 마스킹하지 않는다(파싱 로직이 원문을 필요로 한다). */
export function toLoggableExecResult(outcome: RunExecOutcome): RunExecOutcome {
  return maskDeepValues(outcome);
}
