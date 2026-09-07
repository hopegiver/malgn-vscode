// 실제 `child_process.execFile` 어댑터 — `detectClaudeCodeVersion.ts`의 `ExecFileFn` DI
// 시그니처를 Node 내장 API로 채운다. 이 파일 자체는 로직이 없다(단순 위임) — 그래서
// extension.ts·tray.ts와 같은 "얇은 어댑터" 취급이며 vitest 단위 테스트 대상이 아니다
// (실제 프로세스 실행은 통합 검증 대상).

import { execFile } from 'node:child_process';
import type { ExecFileFn } from '../claudeCli/detectClaudeCodeVersion.js';

export const nodeExecFile: ExecFileFn = (file, args, options, callback) => {
  execFile(file, args as string[], { timeout: options.timeout, env: options.env as NodeJS.ProcessEnv }, (error, stdout, stderr) => {
    callback(error, stdout.toString(), stderr.toString());
  });
};
