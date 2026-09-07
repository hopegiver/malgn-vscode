// 실제 `child_process.execFile` 어댑터 — `platform/exec.ts`의 `ExecFileFn` DI 시그니처를
// Node 내장 API로 채운다(`claudeCli/detectClaudeCodeVersion.ts`가 쓰는 `nodeExecFileAdapter.ts`와
// 같은 패턴이되, `signal` 옵션을 추가로 전달해 engine의 5초 detect 타임아웃 abort가
// 실제 하위 프로세스 kill로 이어지게 한다). 이 파일 자체는 로직이 없다(단순 위임) —
// 얇은 어댑터라 vitest 단위 테스트 대상이 아니다.

import { execFile } from 'node:child_process';
import type { ExecFileFn } from '../../platform/exec.js';

export const electronExecFile: ExecFileFn = (file, args, options, callback) => {
  execFile(
    file,
    args as string[],
    { timeout: options.timeout, env: options.env as NodeJS.ProcessEnv, signal: options.signal },
    (error, stdout, stderr) => {
      callback(error, stdout.toString(), stderr.toString());
    }
  );
};
