import { describe, expect, it } from 'vitest';
import { runExec, toLoggableExecResult, type ExecFileFn, type ExecError } from './exec.js';

function fakeExecFileFn(
  behavior: (file: string, args: readonly string[]) => { error: ExecError | null; stdout: string; stderr: string }
): ExecFileFn {
  return (file, args, _options, callback) => {
    const { error, stdout, stderr } = behavior(file, args);
    callback(error, stdout, stderr);
  };
}

describe('runExec', () => {
  it('정상 종료(exit 0)면 kind=ok', async () => {
    const execFileFn = fakeExecFileFn(() => ({ error: null, stdout: 'hello', stderr: '' }));
    const outcome = await runExec({ execFileFn, file: 'claude', args: ['--version'], env: {}, timeoutMs: 1000 });
    expect(outcome).toEqual({ kind: 'ok', stdout: 'hello', stderr: '' });
  });

  it('0이 아닌 종료 코드면 kind=nonzero-exit + exitCode', async () => {
    const error = Object.assign(new Error('failed'), { code: 1 }) as ExecError;
    const execFileFn = fakeExecFileFn(() => ({ error, stdout: '', stderr: 'boom' }));
    const outcome = await runExec({ execFileFn, file: 'claude', args: ['plugin', 'list'], env: {}, timeoutMs: 1000 });
    expect(outcome).toEqual({ kind: 'nonzero-exit', exitCode: 1, stdout: '', stderr: 'boom' });
  });

  it('killed(타임아웃에 의한 강제 종료)면 kind=timeout', async () => {
    const error = Object.assign(new Error('timeout'), { killed: true, signal: 'SIGTERM' }) as ExecError;
    const execFileFn = fakeExecFileFn(() => ({ error, stdout: 'partial', stderr: '' }));
    const outcome = await runExec({ execFileFn, file: 'claude', args: [], env: {}, timeoutMs: 1000 });
    expect(outcome.kind).toBe('timeout');
  });

  it('spawn 실패(ENOENT 등 code가 숫자가 아님)면 kind=spawn-error', async () => {
    const error = Object.assign(new Error('spawn claude ENOENT'), { code: 'ENOENT' }) as ExecError;
    const execFileFn = fakeExecFileFn(() => ({ error, stdout: '', stderr: '' }));
    const outcome = await runExec({ execFileFn, file: 'claude', args: [], env: {}, timeoutMs: 1000 });
    expect(outcome).toEqual({ kind: 'spawn-error', message: 'spawn claude ENOENT' });
  });

  it('던지지 않는다 — 실행 자체가 콜백을 부르지 않아도 이 함수는 Promise를 반환할 뿐이다(계약 확인)', () => {
    const execFileFn: ExecFileFn = () => {
      /* 콜백을 부르지 않음 — 이 테스트는 runExec 호출 자체가 동기적으로 던지지 않음만 확인 */
    };
    expect(() => runExec({ execFileFn, file: 'claude', args: [], env: {}, timeoutMs: 1000 })).not.toThrow();
  });
});

describe('toLoggableExecResult', () => {
  it('고엔트로피 문자열(비밀 형태)이 stdout에 있으면 마스킹한다', () => {
    const secretLike = 'a'.repeat(40);
    const outcome = toLoggableExecResult({ kind: 'ok', stdout: secretLike, stderr: '' });
    expect(outcome).not.toEqual({ kind: 'ok', stdout: secretLike, stderr: '' });
    expect(JSON.stringify(outcome)).not.toContain(secretLike);
  });

  it('짧은 일반 텍스트는 그대로 둔다', () => {
    const outcome = toLoggableExecResult({ kind: 'ok', stdout: '1.8.33', stderr: '' });
    expect(outcome).toEqual({ kind: 'ok', stdout: '1.8.33', stderr: '' });
  });
});
