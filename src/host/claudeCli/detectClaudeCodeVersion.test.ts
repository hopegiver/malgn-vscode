import { describe, expect, it } from 'vitest';
import type { ExecFileFn } from './detectClaudeCodeVersion.js';
import { detectClaudeCodeVersion, parseClaudeVersionOutput } from './detectClaudeCodeVersion.js';

describe('parseClaudeVersionOutput', () => {
  it('semver를 포함한 출력에서 버전만 뽑는다', () => {
    expect(parseClaudeVersionOutput('2.1.237 (Claude Code)')).toBe('2.1.237');
  });

  it('semver가 없으면 null이다', () => {
    expect(parseClaudeVersionOutput('unexpected output')).toBeNull();
  });
});

describe('detectClaudeCodeVersion', () => {
  it('성공 시 최소 env(PATH·HOME)만 전달하고 파싱된 버전을 반환한다', async () => {
    let capturedArgs: readonly string[] | undefined;
    let capturedEnv: Readonly<Record<string, string | undefined>> | undefined;
    const fakeExecFile: ExecFileFn = (file, args, options, callback) => {
      expect(file).toBe('claude');
      capturedArgs = args;
      capturedEnv = options.env;
      callback(null, '2.1.237\n', '');
    };
    const version = await detectClaudeCodeVersion({ execFileFn: fakeExecFile, homeDir: '/home/dev', pathEnv: '/usr/bin:/bin' });
    expect(version).toBe('2.1.237');
    expect(capturedArgs).toEqual(['--version']);
    expect(capturedEnv).toEqual({ PATH: '/usr/bin:/bin', HOME: '/home/dev' });
  });

  it('실행 실패(claude 미설치 등)는 null을 반환한다 — 던지지 않는다(PR-8)', async () => {
    const fakeExecFile: ExecFileFn = (_file, _args, _options, callback) => {
      callback(new Error('command not found'), '', '');
    };
    const version = await detectClaudeCodeVersion({ execFileFn: fakeExecFile, homeDir: '/home/dev', pathEnv: '/usr/bin' });
    expect(version).toBeNull();
  });

  it('출력이 파싱 불가하면 null을 반환한다', async () => {
    const fakeExecFile: ExecFileFn = (_file, _args, _options, callback) => {
      callback(null, 'garbage', '');
    };
    const version = await detectClaudeCodeVersion({ execFileFn: fakeExecFile, homeDir: '/home/dev', pathEnv: '/usr/bin' });
    expect(version).toBeNull();
  });

  it('기본 타임아웃은 5000ms(§2.2 ⑤·engine.ts DETECT_TIMEOUT_MS와 동일 정본 값)이다', async () => {
    let capturedTimeout: number | undefined;
    const fakeExecFile: ExecFileFn = (_file, _args, options, callback) => {
      capturedTimeout = options.timeout;
      callback(null, '2.1.237', '');
    };
    await detectClaudeCodeVersion({ execFileFn: fakeExecFile, homeDir: '/h', pathEnv: '/usr/bin' });
    expect(capturedTimeout).toBe(5000);
  });
});
