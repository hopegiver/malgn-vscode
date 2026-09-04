// 로그 싱크 — architecture.md §1.1(`ui/log`) · §6.2: "로그는 `LogOutputChannel` 하나로
// 통일하고... 마스킹 필터는 로그 싱크에 단일 지점으로 건다."
//
// 이 파일은 `vscode.LogOutputChannel`을 직접 import하지 않는다 — `src/core/**`와 같은
// 이유로(코어는 vscode 모듈 없이 vitest에서 테스트된다), 이 모듈도 `appendLine(value:
// string): void`만 요구하는 최소 구조 타입(`LineSink`)을 받는다. `vscode.OutputChannel`
// `vscode.LogOutputChannel` 둘 다 이 구조를 만족하므로 확장 쪽(`extension.ts`, 이번
// 슬라이스가 조립하지 않는 활성화 시퀀스의 일부)이 `vscode.window.createOutputChannel(...)`
// 결과를 그대로 넘기면 된다.
//
// [단일 지점] 이 파일 안에서 `sink.appendLine`을 호출하는 곳은 `write()` 하나뿐이고,
// `write()`는 항상 `maskSensitive()`를 거친 문자열만 넘긴다. `createMaskedLogger`가
// 반환하는 `MaskedLogger`만 밖으로 노출되며 `write`나 원본 `sink`는 클로저 밖으로
// 나가지 않는다 — 호출자가 마스킹을 우회해 `sink.appendLine`을 직접 부를 방법이
// 이 모듈의 공개 표면에 없다. `maskSinglePoint.test.ts`가 이 성질을 grep으로 고정한다.

import { maskSensitive } from '../core/diagnostics/mask.js';

/** `vscode.OutputChannel`/`vscode.LogOutputChannel` 양쪽이 만족하는 최소 구조 타입. */
export interface LineSink {
  appendLine(value: string): void;
}

export type LogLevel = 'info' | 'warn' | 'error';

export interface MaskedLogger {
  info(message: string): void;
  warn(message: string): void;
  error(message: string): void;
}

/** 로그 싱크를 감싸 마스킹을 강제하는 유일한 생성자. */
export function createMaskedLogger(sink: LineSink): MaskedLogger {
  function write(level: LogLevel, message: string): void {
    sink.appendLine(`[${level}] ${maskSensitive(message)}`);
  }
  return {
    info: (message) => write('info', message),
    warn: (message) => write('warn', message),
    error: (message) => write('error', message),
  };
}
